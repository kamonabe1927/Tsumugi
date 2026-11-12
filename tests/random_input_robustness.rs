//! ランダムな入力に対する頑健性テスト（依存を足さない固定シードの fuzz）。
//!
//! 手で書いたケースでは届かない入力でも、処理系が次を守ることを確かめる。
//!
//! - lexer・parser・compiler・評価器・VM のどこでも panic しない（host を落とさない。
//!   threat-model TM-AT-07・TM-AT-15）
//! - fuel の上限の中で必ず止まる（無限ループや無限再帰でも、予算超過のエラーで止まる）
//! - tree-walk と VM で、パースの成否、実行の成否、エラーの kind・message・line、
//!   stdout の内容が一致する（TM-AT-17。tree が規範で、差は VM 側の問題として扱う）
//!
//! 入力は 3 種類を作る。
//!
//! 1. 文法どおりのプログラム: 文と式を文法に沿ってランダムに組み立てる。実行時の型・範囲・
//!    予算のエラーや、capability の拒否（stdout 以外は grant しない）は起きてよく、両 engine で比べる
//! 2. token の列: 予約語・演算子・リテラル・括弧・改行を固定の表からランダムに並べる
//! 3. fixture の変異: `tests/fixtures/*.tsg` の正常系を、文字の削除・複製・token の挿入で崩す
//!
//! 乱数は自前の xorshift64 で、シードは固定なので毎回同じ入力になる。失敗したときは
//! メッセージにケース番号と入力を出すので、そのまま再現できる。
//! 外部への作用を避けるため、capability は stdout（メモリへの書き込み）だけを与え、
//! `import` を含む入力は使わない。

use std::num::NonZeroU128;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::{Arc, Mutex};

use tsumugi::budget::BudgetConfig;
use tsumugi::capability::{CapabilitySet, Output};
use tsumugi::compiler::Compiler;
use tsumugi::error::TsumugiError;
use tsumugi::eval::Evaluator;
use tsumugi::lexer::Lexer;
use tsumugi::parser::Parser;
use tsumugi::vm::Vm;

/// 1 ケースあたりの fuel。無限ループもこの範囲で予算超過になる。
const FUEL: u64 = 20_000;
/// collection 要素数の上限。
const MAX_ELEMENTS: u64 = 10_000;
/// token の列のケース数。
const TOKEN_CASES: usize = 400;
/// 文法どおりのプログラムのケース数。
const PROGRAM_CASES: usize = 600;
/// fixture 1 つあたりの変異の数。
const MUTATIONS_PER_FIXTURE: usize = 8;

/// 固定シードの xorshift64。
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

/// token の列に使う表。`import` と、外部に作用する組み込み関数の名前は入れない。
const TOKENS: &[&str] = &[
    "let",
    "fn",
    "if",
    "elif",
    "else",
    "end",
    "while",
    "for",
    "in",
    "break",
    "continue",
    "return",
    "try",
    "catch",
    "and",
    "or",
    "not",
    "true",
    "false",
    "null",
    "=",
    "==",
    "!=",
    "<",
    ">",
    "<=",
    ">=",
    "+",
    "-",
    "*",
    "/",
    "%",
    "(",
    ")",
    "[",
    "]",
    "{",
    "}",
    ",",
    ":",
    "\n",
    "\n",
    "\n",
    "a",
    "b",
    "f",
    "g",
    "xs",
    "d",
    "e",
    "0",
    "1",
    "2",
    "-1",
    "9223372036854775807",
    "1.5",
    "0.0",
    "\"s\"",
    "\"\"",
    "f\"{a}\"",
    "print",
    "len",
    "push",
    "str",
    "map",
    "x",
    "#c",
];

fn token_soup(rng: &mut Rng) -> String {
    let count = 1 + rng.below(40);
    let mut out = String::new();
    for _ in 0..count {
        out.push_str(rng.pick(TOKENS));
        out.push(' ');
    }
    out
}

/// 変数名の表。プログラムの先頭で全部宣言するので、未定義の名前のエラーばかりにはならない。
const VARS: &[&str] = &["a", "b", "c", "xs", "d", "s"];

/// 外部に作用しない組み込み関数と引数の数。
const PURE_BUILTINS: &[(&str, usize)] = &[
    ("len", 1),
    ("push", 2),
    ("pop", 1),
    ("keys", 1),
    ("values", 1),
    ("type", 1),
    ("slice", 3),
    ("contains", 2),
    ("has_key", 2),
    ("split", 2),
    ("join", 2),
    ("to_int", 1),
    ("to_str", 1),
    ("to_float", 1),
    ("range", 2),
    ("trim", 1),
    ("upper", 1),
    ("replace", 3),
    ("abs", 1),
    ("min", 2),
    ("max", 2),
    ("sort", 1),
    ("reverse", 1),
    ("floor", 1),
    ("round", 1),
    ("map", 2),
    ("filter", 2),
    ("each", 2),
];

/// capability が要る組み込み関数。stdout 以外は grant しないので、拒否されることを両 engine で比べる。
const DENIED_CALLS: &[&str] = &[
    "read_file(\"x.txt\")",
    "write_file(\"x.txt\", \"y\")",
    "env(\"HOME\")",
    "now()",
    "list_dir(\".\")",
    "input()",
];

const INT_LITERALS: &[&str] = &[
    "0",
    "1",
    "2",
    "3",
    "-1",
    "10",
    "255",
    "9223372036854775807",
    "-9223372036854775807",
];

/// 文法どおりのプログラムを作る生成器。実行時の型・範囲・予算のエラーは起きてよい。
struct ProgramGen<'r> {
    rng: &'r mut Rng,
    out: String,
    /// ループの中か（break / continue をここで出す。たまにループの外にも出す）
    loop_depth: usize,
    /// 関数の中か（return は関数の中でだけ出す。外はパースエラーになるため）
    fn_depth: usize,
}

impl<'r> ProgramGen<'r> {
    fn new(rng: &'r mut Rng) -> Self {
        Self {
            rng,
            out: String::new(),
            loop_depth: 0,
            fn_depth: 0,
        }
    }

    fn program(mut self) -> String {
        self.out.push_str(
            "let a = 1\nlet b = 2\nlet c = \"t\"\nlet xs = [1, 2, 3]\nlet d = {\"k\": 1}\nlet s = \"abc\"\n",
        );
        self.out
            .push_str("fn f(p)\n  return p\nend\nfn g(p, q)\n  return p + q\nend\n");
        let count = 1 + self.rng.below(10);
        for _ in 0..count {
            // 実行時エラーで最初の文だけで止まらないよう、半分の文は try で包み、
            // catch したエラー値を出力して比べる。
            if self.rng.below(2) == 0 {
                self.out.push_str("try\n");
                self.stmt(1, 0);
                self.out.push_str("catch e\n  print(e)\nend\n");
            } else {
                self.stmt(0, 0);
            }
        }
        self.out
    }

    fn indent(&mut self, level: usize) {
        for _ in 0..level {
            self.out.push_str("  ");
        }
    }

    fn block(&mut self, level: usize, depth: usize) {
        let count = 1 + self.rng.below(3);
        for _ in 0..count {
            self.stmt(level, depth + 1);
        }
    }

    fn stmt(&mut self, level: usize, depth: usize) {
        self.indent(level);
        let choice = if depth >= 3 {
            self.rng.below(5)
        } else {
            self.rng.below(14)
        };
        let var = self.rng.pick(VARS);
        match choice {
            0 => {
                let e = self.expr(0);
                self.out.push_str(&format!("{var} = {e}\n"));
            }
            1 => {
                let e = self.expr(0);
                self.out.push_str(&format!("print({e})\n"));
            }
            2 => {
                let e = self.expr(0);
                self.out.push_str(&format!("let {var} = {e}\n"));
            }
            3 => {
                let i = self.expr(1);
                let v = self.expr(1);
                let target = self.rng.pick(&["xs", "d", "s", "a"]);
                self.out.push_str(&format!("{target}[{i}] = {v}\n"));
            }
            4 => {
                let keyword = self.control_keyword();
                self.out.push_str(&format!("{keyword}\n"));
            }
            5 | 6 => {
                let cond = self.expr(0);
                self.out.push_str(&format!("if {cond}\n"));
                self.block(level + 1, depth);
                if self.rng.below(2) == 0 {
                    let cond = self.expr(0);
                    self.indent(level);
                    self.out.push_str(&format!("elif {cond}\n"));
                    self.block(level + 1, depth);
                }
                if self.rng.below(2) == 0 {
                    self.indent(level);
                    self.out.push_str("else\n");
                    self.block(level + 1, depth);
                }
                self.indent(level);
                self.out.push_str("end\n");
            }
            7 => {
                let cond = self.expr(0);
                self.out.push_str(&format!("while {cond}\n"));
                self.loop_depth += 1;
                self.block(level + 1, depth);
                self.loop_depth -= 1;
                self.indent(level);
                self.out.push_str("end\n");
            }
            8 | 9 => {
                let iterable = self.expr(0);
                self.out.push_str(&format!("for {var} in {iterable}\n"));
                self.loop_depth += 1;
                self.block(level + 1, depth);
                self.loop_depth -= 1;
                self.indent(level);
                self.out.push_str("end\n");
            }
            10 => {
                let name = self.rng.pick(&["f", "g", "h"]);
                self.out.push_str(&format!("fn {name}(p, q)\n"));
                let saved = self.loop_depth;
                self.loop_depth = 0;
                self.fn_depth += 1;
                self.block(level + 1, depth);
                self.fn_depth -= 1;
                self.loop_depth = saved;
                self.indent(level);
                self.out.push_str("end\n");
            }
            11 | 12 => {
                self.out.push_str("try\n");
                self.block(level + 1, depth);
                self.indent(level);
                self.out.push_str("catch e\n");
                self.block(level + 1, depth);
                self.indent(level);
                self.out.push_str("end\n");
            }
            _ => {
                let e = self.expr(0);
                self.out.push_str(&format!("{e}\n"));
            }
        }
    }

    /// break / continue / return。たまにループの外の break / continue も出す（実行時の
    /// control_flow エラーになり、両 engine で同じになるべきもの）。
    fn control_keyword(&mut self) -> String {
        if self.fn_depth > 0 && self.rng.below(2) == 0 {
            let e = self.expr(1);
            return format!("return {e}");
        }
        if self.loop_depth > 0 || self.rng.below(4) == 0 {
            return self.rng.pick(&["break", "continue"]).to_owned();
        }
        let e = self.expr(1);
        format!("print({e})")
    }

    fn expr(&mut self, depth: usize) -> String {
        let leaf = depth >= 3 || self.rng.below(3) == 0;
        if leaf {
            return self.atom();
        }
        match self.rng.below(9) {
            0 | 1 => {
                let op = self.rng.pick(&[
                    "+", "-", "*", "/", "%", "==", "!=", "<", ">", "<=", ">=", "and", "or",
                ]);
                let l = self.expr(depth + 1);
                let r = self.expr(depth + 1);
                format!("({l} {op} {r})")
            }
            2 => {
                let op = self.rng.pick(&["-", "not "]);
                let e = self.expr(depth + 1);
                format!("{op}{e}")
            }
            3 | 4 => {
                let (name, arity) = PURE_BUILTINS[self.rng.below(PURE_BUILTINS.len())];
                let args: Vec<String> = (0..arity).map(|_| self.expr(depth + 1)).collect();
                format!("{name}({})", args.join(", "))
            }
            5 => {
                let (name, mut arity) = if self.rng.below(2) == 0 {
                    ("f", 1)
                } else {
                    ("g", 2)
                };
                // たまに引数の数を間違える（argument エラーを両 engine で比べる）。
                if self.rng.below(8) == 0 {
                    arity = self.rng.below(4);
                }
                let args: Vec<String> = (0..arity).map(|_| self.expr(depth + 1)).collect();
                format!("{name}({})", args.join(", "))
            }
            6 => {
                let target = self.expr(depth + 1);
                let index = self.expr(depth + 1);
                format!("{target}[{index}]")
            }
            7 => {
                let body = self.expr(depth + 1);
                // 文の先頭の `fn(` は関数定義として読まれるため、括弧で包む。
                format!("(fn(p) {body} end)")
            }
            _ => {
                let items: Vec<String> = (0..self.rng.below(4))
                    .map(|_| self.expr(depth + 1))
                    .collect();
                if self.rng.below(2) == 0 {
                    format!("[{}]", items.join(", "))
                } else {
                    let entries: Vec<String> = items
                        .iter()
                        .enumerate()
                        .map(|(i, v)| format!("\"k{i}\": {v}"))
                        .collect();
                    format!("{{{}}}", entries.join(", "))
                }
            }
        }
    }

    fn atom(&mut self) -> String {
        match self.rng.below(12) {
            0..=3 => self.rng.pick(VARS).to_owned(),
            4 | 5 => self.rng.pick(INT_LITERALS).to_owned(),
            6 => self
                .rng
                .pick(&["1.5", "0.0", "-2.5", "1000000000000.5"])
                .to_owned(),
            7 => self
                .rng
                .pick(&["\"\"", "\"x\"", "\"あい\"", "f\"{a}-{c}\""])
                .to_owned(),
            8 => self.rng.pick(&["true", "false", "null"]).to_owned(),
            9 if self.fn_depth > 0 => self.rng.pick(&["p", "q"]).to_owned(),
            10 if self.rng.below(4) == 0 => self.rng.pick(DENIED_CALLS).to_owned(),
            _ => self.rng.pick(&["f", "g"]).to_owned(),
        }
    }
}

/// 文字単位で入力を崩す。削除・複製・token の挿入を 1〜4 回行う。
fn mutate(source: &str, rng: &mut Rng) -> String {
    let mut chars: Vec<char> = source.chars().collect();
    let edits = 1 + rng.below(4);
    for _ in 0..edits {
        if chars.is_empty() {
            break;
        }
        let at = rng.below(chars.len());
        match rng.below(3) {
            0 => {
                let len = 1 + rng.below(8.min(chars.len() - at));
                chars.drain(at..at + len);
            }
            1 => {
                let len = 1 + rng.below(16.min(chars.len() - at));
                let copy: Vec<char> = chars[at..at + len].to_vec();
                let to = rng.below(chars.len() + 1);
                chars.splice(to..to, copy);
            }
            _ => {
                let token: Vec<char> = format!(" {} ", rng.pick(TOKENS)).chars().collect();
                chars.splice(at..at, token);
            }
        }
    }
    chars.into_iter().collect()
}

/// stdout をメモリへ写す [`Output`]。
struct CapturingOutput {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl Output for CapturingOutput {
    fn policy_id(&self) -> NonZeroU128 {
        NonZeroU128::new(1).expect("non-zero")
    }
    fn write_all(&self, bytes: &[u8]) -> Result<(), tsumugi::AdapterError> {
        self.buffer
            .lock()
            .expect("buffer lock")
            .extend_from_slice(bytes);
        Ok(())
    }
    fn flush(&self) -> Result<(), tsumugi::AdapterError> {
        Ok(())
    }
}

fn capabilities(buffer: &Arc<Mutex<Vec<u8>>>) -> CapabilitySet {
    CapabilitySet::builder()
        .stdout(Arc::new(CapturingOutput {
            buffer: Arc::clone(buffer),
        }))
        .expect("stdout grant")
        .build()
}

/// 1 engine の結果。パースの失敗はエラー文字列の列、実行の失敗は (kind, message, line)。
#[derive(Debug, PartialEq)]
enum Outcome {
    ParseError(Vec<String>),
    Completed { stdout: String },
    Failed { error: String, stdout: String },
}

fn describe(error: &TsumugiError) -> String {
    format!(
        "{:?} / {} / {}行目",
        error.kind(),
        error.message(),
        error.line()
    )
}

fn budget() -> BudgetConfig {
    BudgetConfig::for_legacy(FUEL, MAX_ELEMENTS)
}

fn run_tree(source: &str) -> Outcome {
    let program = match Parser::new(Lexer::new(source).tokenize()).parse() {
        Ok(program) => program,
        Err(errors) => return Outcome::ParseError(errors.iter().map(|e| e.to_string()).collect()),
    };
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let mut evaluator = Evaluator::with_budget(budget());
    evaluator.set_capabilities(capabilities(&buffer));
    let result = evaluator.run(&program, source.len() as u64);
    let stdout = String::from_utf8_lossy(&buffer.lock().expect("buffer lock")).into_owned();
    match result {
        Ok(()) => Outcome::Completed { stdout },
        Err(error) => Outcome::Failed {
            error: describe(&error),
            stdout,
        },
    }
}

fn run_vm(source: &str) -> Outcome {
    let program = match Parser::new(Lexer::new(source).tokenize()).parse() {
        Ok(program) => program,
        Err(errors) => return Outcome::ParseError(errors.iter().map(|e| e.to_string()).collect()),
    };
    // VM は一部の診断（ループ外の break など）を compile 時に返すが、canonical な
    // kind・message・line は tree の実行時エラーと同じになる（canonical_error_inventory と同じ扱い）。
    let chunk = match Compiler::new().compile(&program) {
        Ok(chunk) => chunk,
        Err(error) => {
            return Outcome::Failed {
                error: describe(&error),
                stdout: String::new(),
            };
        }
    };
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let mut vm = Vm::with_budget(chunk, budget());
    vm.set_capabilities(capabilities(&buffer));
    // link 時の課金 token は実行が終わるまで持っておく。
    let result = vm
        .charge_link(source.len() as u64, &[])
        .and_then(|_tokens| vm.run());
    let stdout = String::from_utf8_lossy(&buffer.lock().expect("buffer lock")).into_owned();
    match result {
        Ok(()) => Outcome::Completed { stdout },
        Err(error) => Outcome::Failed {
            error: describe(&error),
            stdout,
        },
    }
}

/// panic を捕まえて、ケースを特定できるメッセージにする。
fn run_guarded(label: &str, source: &str, use_vm: bool) -> Outcome {
    let mode = if use_vm { "VM" } else { "tree" };
    catch_unwind(AssertUnwindSafe(|| {
        if use_vm {
            run_vm(source)
        } else {
            run_tree(source)
        }
    }))
    .unwrap_or_else(|_| panic!("{label} [{mode}]: panic しました。入力: {source:?}"))
}

/// tree と VM でステップ上限エラーの行番号だけが違うことが分かっているケース（issue #393）。
/// これらは行番号を比べず、kind・message・stdout は比べる。直したら消す。
const KNOWN_STEP_LIMIT_LINE_DIVERGENCES: &[&str] = &["program#143"];

fn check_case(label: &str, source: &str) {
    let tree = run_guarded(label, source, false);
    let vm = run_guarded(label, source, true);
    let (tree, vm) = if KNOWN_STEP_LIMIT_LINE_DIVERGENCES.contains(&label) {
        (without_step_limit_line(tree), without_step_limit_line(vm))
    } else {
        (tree, vm)
    };
    if tree != vm {
        panic!(
            "{label}: tree と VM の結果が違います。\n入力:\n{source}\ntree: {}\nVM:   {}",
            summarize(&tree),
            summarize(&vm)
        );
    }
}

/// ステップ上限エラーなら行番号を消す。それ以外はそのまま返す。
fn without_step_limit_line(outcome: Outcome) -> Outcome {
    match outcome {
        Outcome::Failed { error, stdout } if error.starts_with("Some(StepLimit)") => {
            let error = error
                .rsplit_once(" / ")
                .map_or(error.clone(), |(head, _line)| head.to_owned());
            Outcome::Failed { error, stdout }
        }
        other => other,
    }
}

/// 失敗メッセージ用に、stdout を末尾 200 文字までに縮めて結果を書く。
fn summarize(outcome: &Outcome) -> String {
    let tail = |stdout: &str| {
        let chars: Vec<char> = stdout.chars().collect();
        let start = chars.len().saturating_sub(200);
        format!(
            "stdout {} 文字（末尾: {:?}）",
            chars.len(),
            chars[start..].iter().collect::<String>()
        )
    };
    match outcome {
        Outcome::ParseError(errors) => format!("パースエラー {errors:?}"),
        Outcome::Completed { stdout } => format!("完了、{}", tail(stdout)),
        Outcome::Failed { error, stdout } => format!("エラー {error}、{}", tail(stdout)),
    }
}

/// CLI と同じ 8MB のスタックで実行する（深いネストでも native stack を使い切らないこと自体は
/// depth の上限が守る。ここでは CLI と同じ条件にそろえる）。
fn with_cli_stack(body: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(body)
        .expect("thread spawn")
        .join()
        .unwrap_or_else(|payload| std::panic::resume_unwind(payload));
}

/// issue #393 の最小の再現。外側の try の中で、catch したエラーを挟む入れ子のループが
/// ステップ上限に達すると、tree と VM で行番号が違う。直したら `#[ignore]` を外す。
#[test]
#[ignore = "tree と VM でステップ上限の行番号が違う既知のバグ（issue #393）"]
fn known_divergence_step_limit_line_after_caught_error() {
    let source = "let xs = [1, 2, 3]\nfn f(p)\n  return p\nend\ntry\n  while f(f)\n    try\n      while xs\n        a = f(g)\n      end\n    catch e\n    end\n  end\ncatch e\nend\n";
    check_case("issue#393", source);
}

#[test]
fn random_valid_programs_never_panic_and_engines_agree() {
    with_cli_stack(|| {
        let mut rng = Rng::new(0x5052_4F47_5241_4D53);
        for case in 0..PROGRAM_CASES {
            let source = ProgramGen::new(&mut rng).program();
            check_case(&format!("program#{case}"), &source);
        }
    });
}

#[test]
fn random_token_sequences_never_panic_and_engines_agree() {
    with_cli_stack(|| {
        let mut rng = Rng::new(0x5453_554D_5547_4931);
        for case in 0..TOKEN_CASES {
            let source = token_soup(&mut rng);
            check_case(&format!("token#{case}"), &source);
        }
    });
}

#[test]
fn mutated_fixtures_never_panic_and_engines_agree() {
    with_cli_stack(|| {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let mut names: Vec<_> = std::fs::read_dir(&dir)
            .expect("fixtures ディレクトリを読めません")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "tsg"))
            .collect();
        names.sort();
        let mut rng = Rng::new(0x4649_5854_5552_4553);
        for path in names {
            let source = std::fs::read_to_string(&path).expect("fixture を読めません");
            if source.contains("import") {
                continue;
            }
            let name = path.file_stem().expect("file stem").to_string_lossy();
            for case in 0..MUTATIONS_PER_FIXTURE {
                let mutated = mutate(&source, &mut rng);
                if mutated.contains("import") {
                    continue;
                }
                check_case(&format!("{name}#{case}"), &mutated);
            }
        }
    });
}

/// 乱数生成器と変異が決定的であること（失敗の再現性の前提）。
#[test]
fn generators_are_deterministic() {
    let mut a = Rng::new(42);
    let mut b = Rng::new(42);
    assert_eq!(token_soup(&mut a), token_soup(&mut b));
    assert_eq!(mutate("let x = 1\n", &mut a), mutate("let x = 1\n", &mut b));
}

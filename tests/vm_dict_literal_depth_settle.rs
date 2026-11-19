//! fix-vm-dict-literal-depth-settle（issue #374）: VM の Dict リテラルの構築で途中の深度を読み直さず、
//! 作り終えたときに 1 回だけ要約する。受入基準 VDL-AT-01..04（docs/execution-control.md の同名スライス小節、
//! 規範は docs/semantic-decisions.md §21.3「リテラル」）。
//! VDL-AT-01（読み直しの回数）は `src/vm.rs` の unit test `vdl_at_01_*`（cfg(test) の counter）。
//! VDL-AT-04 は既存のテスト一式が期待値の変更なしで通ること（`src/compiler.rs` の bytecode の期待文字列だけ
//! 最後の `DictInsert` の operand（`DictInsert(true)`）への変更で更新した）。

use tsumugi::budget::BudgetUsage;
use tsumugi::compiler::Compiler;
use tsumugi::eval::Evaluator;
use tsumugi::lexer::Lexer;
use tsumugi::parser::Parser;
use tsumugi::vm::Vm;

/// 失敗は (type, message, line)。
type Outcome = Result<BudgetUsage, (String, String, usize)>;

fn tree(source: &str) -> Outcome {
    let tokens = Lexer::new(source).tokenize();
    let program = Parser::new(tokens).parse().expect("parse");
    let mut evaluator = Evaluator::new();
    evaluator
        .run(&program, source.len() as u64)
        .map(|()| evaluator.budget_usage())
        .map_err(|e| {
            (
                e.error_type().to_string(),
                e.message().to_string(),
                e.line(),
            )
        })
}

fn vm(source: &str) -> Outcome {
    let tokens = Lexer::new(source).tokenize();
    let program = Parser::new(tokens).parse().expect("parse");
    let chunk = Compiler::new().compile(&program).expect("compile");
    let mut machine = Vm::new(chunk);
    machine.run().map(|()| machine.budget_usage()).map_err(|e| {
        (
            e.error_type().to_string(),
            e.message().to_string(),
            e.line(),
        )
    })
}

/// 両 backend の結果（成功は true、失敗は error の (type, message, line)）を比べて返す。
fn same_outcome(source: &str) -> Result<(), (String, String, usize)> {
    let t = tree(source).map(|_| ());
    let v = vm(source).map(|_| ());
    assert_eq!(t, v, "tree と VM の結果が違う: {source}");
    t
}

/// 深度 `depth` の Dict を `name` に作る script 断片（空の Dict が深度 1）。
fn nest(name: &str, depth: usize) -> String {
    format!(
        "let {name} = {{}}\nlet {name}_i = 1\nwhile {name}_i < {depth}\n  {name} = {{\"k\": {name}}}\n  {name}_i = {name}_i + 1\nend\n"
    )
}

/// `x` をさらに `wraps` 回、Dict で包む script 断片。
fn wrap(name: &str, wraps: usize) -> String {
    format!("let w = 0\nwhile w < {wraps}\n  {name} = {{\"k\": {name}}}\n  w = w + 1\nend\n")
}

/// VDL-AT-02: 重複した key で最も深い値を浅い値に上書きしたリテラルの深度は、作り終えた内容で決まる。
/// 256 段まで作れ、257 段で `overflow`。tree と VM で同じ。入れ子のリテラルでも同じ。
#[test]
fn vdl_at_02_depth_is_exact_after_overwrite() {
    // e255（深度 255）を入れても検査は通る（入れた時点の深度 256）。上書き後のリテラルの深度は 1。
    let prelude = nest("e", 255);
    // (リテラル, リテラル自身の深度)
    let literals = [
        ("{\"a\": e, \"b\": 1, \"a\": 1}", 1usize),
        ("{\"x\": {\"a\": e, \"a\": 1}}", 2),
        ("[{\"a\": e, \"a\": 1}]", 2),
        ("{\"a\": e, \"a\": 1, \"a\": e, \"a\": 1}", 1),
    ];
    for (literal, depth) in literals {
        let wraps_ok = 256 - depth;
        let ok = format!("{prelude}let x = {literal}\n{}", wrap("x", wraps_ok));
        assert_eq!(same_outcome(&ok), Ok(()), "256 段は作れる: {literal}");
        let bad = format!("{prelude}let x = {literal}\n{}", wrap("x", wraps_ok + 1));
        let err = same_outcome(&bad).expect_err("257 段は拒否される");
        assert_eq!(err.0, "overflow", "{literal}");
    }
}

/// VDL-AT-03: 構築の途中の検査の位置は変わらない。深度 256 の値を入れると、後ろで上書きされる key でも
/// 後ろの要素の評価（副作用）の前に `overflow` になる。key の型エラーも同じ位置。tree と VM で一致する。
#[test]
fn vdl_at_03_checks_stay_in_place() {
    let prelude = nest("e", 256);
    for body in [
        "let a = {\"a\": e, \"a\": 1}",
        "let a = [e, print(\"side\")]",
        "let a = {\"x\": {\"a\": e, \"a\": 1}}",
    ] {
        let err = same_outcome(&format!("{prelude}{body}\n")).expect_err("overflow のはず");
        assert_eq!(err.0, "overflow", "{body}");
    }
    let err = same_outcome("let a = {\"a\": 1, 2: 3, \"a\": 4}\n").expect_err("key の型");
    assert_eq!(err.0, "type", "key の型");
}

/// VDL-AT-03: 重複した key を含む Dict リテラルの fuel は tree と VM で一致し、読み直しの有無
/// （深い値を浅い値で上書きするか）で変わらない（リテラルの構築は読み直しに課金しない）。
#[test]
fn vdl_at_03_fuel_matches_and_settle_is_not_charged() {
    let script = |second: &str| {
        let mut items: Vec<String> = (0..50).map(|i| format!("\"k{i}\": 0")).collect();
        for _ in 0..40 {
            items.push("\"a\": e".to_string());
            items.push(format!("\"a\": {second}"));
        }
        format!(
            "let e = [[1]]\nlet i = 0\nwhile i < 20\n  let d = {{{}}}\n  i = i + 1\nend\n",
            items.join(", ")
        )
    };
    // 深い値を浅い値で上書きする（読み直しが起きていた形）と、上書きしても深度が変わらない形。
    let overwrite_shallow = script("1");
    let overwrite_deep = script("e");
    let t = tree(&overwrite_shallow).expect("tree");
    let v = vm(&overwrite_shallow).expect("vm");
    assert_eq!(t.committed.fuel, v.committed.fuel, "tree と VM の fuel");
    let v_deep = vm(&overwrite_deep).expect("vm");
    assert_eq!(
        v.committed.fuel, v_deep.committed.fuel,
        "読み直しの有無で fuel が変わらない"
    );
    assert_eq!(
        v.committed.fuel,
        tree(&overwrite_deep).expect("tree").committed.fuel
    );
}

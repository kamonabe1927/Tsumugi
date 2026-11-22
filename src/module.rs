//! import の解決（モジュールリンク）
//!
//! `import` は実行前にすべて解決する（AUD-030）。`import` 文を対象モジュールの文へ置き換えた
//! 「リンク済みプログラム」を作る。モジュールのトップレベル文は `import` 文があった位置に
//! 展開されるため、実行順序は従来と同じである。
//!
//! 読み込み元は 1 つだけである。[`expand_from_graph`] が、`Engine::link` が resolver capability で
//! 読み、検証した source だけから展開し、filesystem を読み直さない（capability-model §10）。
//! file/stdin の run（tree の `Engine::run`・`run_audited`、VM の `execute_vm_with_path`）、REPL、
//! scheduler の handle 経路（`Engine::create_execution` / `start`）はどれも同じ展開を使う。
//!
//! ツリーウォーク版とVM版が同じ実装を共有するので、評価時点がengine間でずれない。

use std::collections::{HashMap, HashSet};

use crate::ast::{Program, Stmt};
use crate::error::{ErrorKind, TsumugiError};
use crate::limits::MAX_IMPORT_DEPTH;

/// import 失敗 error を per-template message_id 付きで生成する。
///
/// `Import` は 3 テンプレート（ネスト深度 / 読込不能 / 構文不正）を持つクラス B kind であり、
/// kind だけでは一意化できないため、呼び出し側がテンプレートの message_id を渡す
/// （StableError §8.2、`src/stable_error.rs`）。観測挙動（kind=`import` / message / line）は
/// 不変で、message_id 内部タグだけが載る。
pub(crate) fn import_error(
    line: usize,
    message_id: std::num::NonZeroU16,
    message: impl Into<String>,
) -> TsumugiError {
    TsumugiError::runtime_with_message_id(line, ErrorKind::Import, message_id, message)
}

/// import specifier を正規形の論理 ID 文字列（`@mount/comp/...`）にする。
///
/// link 層（`Engine::link` の specifier 正規化）と、link が確かめた graph から import を展開する
/// run 層（[`expand_from_graph`]）が共有する唯一の実装（capability-model §10、
/// fix-import-run-from-link-graph）。unqualified な specifier は `@default/...` へ正規化する。
/// mount が登録済みかどうかはここでは見ない（link 層が resolver に問い合わせる）。
///
/// 戻り値は（mount 名, 正規形）。
pub(crate) fn normalize_import_specifier(
    specifier: &str,
) -> Result<(String, String), crate::capability::PathError> {
    let target = crate::capability::FilesystemTarget::parse(specifier)?;
    let mut normalized = String::from("@");
    normalized.push_str(target.mount.as_str());
    for component in target.path.components() {
        normalized.push('/');
        normalized.push_str(component);
    }
    Ok((target.mount.as_str().to_string(), normalized))
}

/// link が resolver から読み、検証を通した module 1 本（run 層の展開に使う）。
///
/// 実行スレッド上で保持 source を再 parse した `Program`（embedding-api §4.1 案 A）と、
/// その中の `import` 文を正規化した `ModuleId`・行の列を持つ。
#[derive(Debug)]
pub struct GraphModule {
    /// link 時に読んだ source の hash（`ImportNode.source_hash` と同じ値）。
    pub source_hash: crate::embedding::SourceHash,
    /// 保持 source の生 UTF-8 byte 長（source/import 予算の課金量）。
    pub byte_len: u64,
    /// 保持 source を再 parse した module の top-level 文。
    pub program: Program,
    /// module の中の `import` 文（出現順）の正規化済み ID と行番号。
    pub imports: Vec<(crate::embedding::ModuleId, usize)>,
}

/// link が確かめた graph の全 module（run 層の import 展開の唯一の読み込み元）。
#[derive(Debug, Default)]
pub struct GraphModules {
    modules: HashMap<crate::embedding::ModuleId, GraphModule>,
}

impl GraphModules {
    /// 保持 source から全 module を再 parse し、各 `import` の正規化と、その `ModuleId` が
    /// graph にあることを確かめる。`root` の `import` も同じく確かめる。
    ///
    /// 失敗は内部不整合（link が確かめた graph と合わない）で、呼び出し側は
    /// `InternalFailure` にする（capability-model §10）。`Err` の文字列は host path を含まない。
    pub fn prepare<'a>(
        root: &Program,
        sources: impl IntoIterator<
            Item = (
                &'a crate::embedding::ModuleId,
                crate::embedding::SourceHash,
                &'a str,
            ),
        >,
    ) -> Result<Self, String> {
        let mut modules = HashMap::new();
        for (id, source_hash, text) in sources {
            let tokens = crate::lexer::Lexer::new(text).tokenize();
            let program = crate::parser::Parser::new(tokens).parse().map_err(|_| {
                format!(
                    "保持 module source の再 parse に失敗しました: {}",
                    id.as_str()
                )
            })?;
            let imports = import_ids(&program)?;
            modules.insert(
                id.clone(),
                GraphModule {
                    source_hash,
                    byte_len: text.len() as u64,
                    program,
                    imports,
                },
            );
        }
        let graph = Self { modules };
        for (id, _) in import_ids(root)? {
            graph.require(&id)?;
        }
        for module in graph.modules.values() {
            for (id, _) in &module.imports {
                graph.require(id)?;
            }
        }
        Ok(graph)
    }

    fn require(&self, id: &crate::embedding::ModuleId) -> Result<&GraphModule, String> {
        self.modules
            .get(id)
            .ok_or_else(|| format!("import 先が link 済み graph にありません: {}", id.as_str()))
    }
}

/// `program` の `import` 文を出現順に正規化する。
fn import_ids(program: &Program) -> Result<Vec<(crate::embedding::ModuleId, usize)>, String> {
    program
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Import { path, line } => Some((path, *line)),
            _ => None,
        })
        .map(|(path, line)| {
            let invalid = || "import specifier を正規化できません".to_string();
            let (_, normalized) = normalize_import_specifier(path).map_err(|_| invalid())?;
            let id = crate::embedding::ModuleId::new(normalized).map_err(|_| invalid())?;
            Ok((id, line))
        })
        .collect()
}

/// graph から新しく展開した module 1 本の記録（課金と巻き戻しに使う）。
#[derive(Debug, Clone)]
pub struct ExpandedModule {
    /// module の論理 ID。
    pub id: crate::embedding::ModuleId,
    /// 展開した source の hash。
    pub source_hash: crate::embedding::SourceHash,
    /// 保持 source の生 UTF-8 byte 長。
    pub byte_len: u64,
    /// 展開のきっかけになった `import` 文の行番号。
    pub line: usize,
}

/// 同じ `ModuleId` が別の内容で既に展開されているときの import error（capability-model §10）。
pub(crate) fn hash_mismatch_error(line: usize, id: &crate::embedding::ModuleId) -> TsumugiError {
    import_error(
        line,
        crate::stable_error::message_id::IMPORT_HASH_MISMATCH,
        format!(
            "import に失敗しました: 同じモジュールが別の内容で既に展開されています: {}",
            id.as_str()
        ),
    )
}

/// root の `Program` と link が確かめた graph から、`import` 文を展開した linked program を作る
/// （capability-model §10、fix-import-run-from-link-graph 実装範囲 2）。
///
/// 展開の規則は次のとおり（`import` 文の位置に module の top-level 文を置く、
/// 展開済みの `ModuleId` は skip、展開した module の中の `import` も再帰的に展開する）。
/// `expanded` はこの context で既に展開した `ModuleId` → `source_hash`。同じ hash なら skip し、
/// その module の子孫も照合のためだけにたどる（展開も課金もしない）。hash が違えば
/// `import.hash_mismatch` の import error で止める。
///
/// import が無ければプログラムは `None` を返す。読み込み元は `graph` だけで、filesystem も
/// resolver も呼ばない。`graph` は [`GraphModules::prepare`] で検証済みの前提で、ID が無いときは
/// 内部 error（防御）を返す。
pub fn expand_from_graph(
    program: &Program,
    graph: &GraphModules,
    expanded: &HashMap<crate::embedding::ModuleId, crate::embedding::SourceHash>,
) -> Result<(Option<Program>, Vec<ExpandedModule>), TsumugiError> {
    if !program
        .iter()
        .any(|stmt| matches!(stmt, Stmt::Import { .. }))
    {
        return Ok((None, Vec::new()));
    }
    let mut state = GraphExpansion {
        graph,
        expanded,
        visited: HashSet::new(),
        newly: Vec::new(),
        newly_hashes: HashMap::new(),
    };
    let mut out = Vec::with_capacity(program.len());
    state.expand_into(program, 0, &mut out)?;
    Ok((Some(out), state.newly))
}

struct GraphExpansion<'a> {
    graph: &'a GraphModules,
    expanded: &'a HashMap<crate::embedding::ModuleId, crate::embedding::SourceHash>,
    /// この run でたどった（展開または照合した）`ModuleId`。
    visited: HashSet<crate::embedding::ModuleId>,
    newly: Vec<ExpandedModule>,
    newly_hashes: HashMap<crate::embedding::ModuleId, crate::embedding::SourceHash>,
}

impl GraphExpansion<'_> {
    fn known_hash(&self, id: &crate::embedding::ModuleId) -> Option<crate::embedding::SourceHash> {
        self.expanded
            .get(id)
            .or_else(|| self.newly_hashes.get(id))
            .copied()
    }

    fn expand_into(
        &mut self,
        program: &Program,
        depth: usize,
        out: &mut Vec<Stmt>,
    ) -> Result<(), TsumugiError> {
        for stmt in program {
            let Stmt::Import { path, line } = stmt else {
                out.push(stmt.clone());
                continue;
            };
            let id = normalize_import_specifier(path)
                .ok()
                .and_then(|(_, n)| crate::embedding::ModuleId::new(n).ok())
                .ok_or_else(|| {
                    TsumugiError::internal(*line, "import specifier を正規化できません")
                })?;
            let graph = self.graph;
            let module = graph
                .require(&id)
                .map_err(|detail| TsumugiError::internal(*line, detail))?;
            if let Some(known) = self.known_hash(&id) {
                if known != module.source_hash {
                    return Err(hash_mismatch_error(*line, &id));
                }
                // 展開済み（同じ hash）は skip し、子孫を照合のためだけにたどる。
                self.verify_descendants(&id)?;
                continue;
            }
            // link が深度を確かめているので通常は届かない（防御）。
            if depth >= MAX_IMPORT_DEPTH {
                return Err(TsumugiError::internal(
                    *line,
                    "import のネストが link の検証を超えました",
                ));
            }
            self.visited.insert(id.clone());
            self.newly_hashes.insert(id.clone(), module.source_hash);
            self.newly.push(ExpandedModule {
                id,
                source_hash: module.source_hash,
                byte_len: module.byte_len,
                line: *line,
            });
            self.expand_into(&module.program, depth + 1, out)?;
        }
        Ok(())
    }

    /// 展開済みで skip する module の子孫の hash を、展開済みの集合と照合する（展開も課金もしない）。
    /// 順序は展開と同じ source 順の深さ優先で、1 回の run の中で同じ `ModuleId` は 1 回だけたどる。
    fn verify_descendants(
        &mut self,
        start: &crate::embedding::ModuleId,
    ) -> Result<(), TsumugiError> {
        if !self.visited.insert(start.clone()) {
            return Ok(());
        }
        let graph = self.graph;
        // 明示 stack（再帰しない）。各要素は (module, 次に見る import の位置)。
        let first = graph
            .require(start)
            .map_err(|detail| TsumugiError::internal(0, detail))?;
        let mut stack: Vec<(&GraphModule, usize)> = vec![(first, 0)];
        while let Some((module, index)) = stack.pop() {
            let Some((child, line)) = module.imports.get(index) else {
                continue;
            };
            stack.push((module, index + 1));
            let child_module = graph
                .require(child)
                .map_err(|detail| TsumugiError::internal(*line, detail))?;
            match self.known_hash(child) {
                Some(known) if known != child_module.source_hash => {
                    return Err(hash_mismatch_error(*line, child));
                }
                Some(_) => {}
                // 展開済みの module の子孫はすべて展開済みの集合にある（展開と巻き戻しは
                // execution 単位で揃う）。無ければ内部不整合。
                None => {
                    return Err(TsumugiError::internal(
                        *line,
                        "展開済みモジュールの import 先が展開されていません",
                    ));
                }
            }
            if self.visited.insert(child.clone()) {
                stack.push((child_module, 0));
            }
        }
        Ok(())
    }
}

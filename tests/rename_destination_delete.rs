//! fix-rename-destination-delete: builtin の `rename` で移動先の `Delete` も事前要求する。
//! 受入基準 RDD-AT-01..02（docs/capability-model.md の同名スライス小節、§8.4、D-39）。
//!
//! 呼び出しを数える `DirectoryHandle` を grant する埋め込み API の test。tree と VM の両方で、
//! 同じ script が同じ error / 同じ adapter 呼び出し回数になることを確かめる。

use std::collections::BTreeSet;
use std::num::{NonZeroU64, NonZeroU128};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tsumugi::capability::{
    DirectoryEntry, EntryKind, FileHandle, OpenFileRequest, PublicMetadata, RelativePath,
    RemoveKind,
};
use tsumugi::compiler::Compiler;
use tsumugi::error::{ErrorKind, TsumugiError};
use tsumugi::eval::Evaluator;
use tsumugi::lexer::Lexer;
use tsumugi::parser::Parser;
use tsumugi::vm::Vm;
use tsumugi::{
    AdapterError, CapabilitySet, DirectoryHandle, FilesystemCapability, FilesystemRoot,
    FsOperation, MountName, SymlinkPolicy,
};

/// `rename` の呼び出しを数え、最後の `replace` 引数を覚える adapter。他の操作は何もしない。
struct Counting {
    policy: NonZeroU128,
    renames: AtomicUsize,
    last_replace: AtomicUsize,
}

impl Counting {
    fn new(policy: u128) -> Arc<Self> {
        Arc::new(Self {
            policy: NonZeroU128::new(policy).expect("non-zero"),
            renames: AtomicUsize::new(0),
            last_replace: AtomicUsize::new(usize::MAX),
        })
    }
}

/// `Arc<Counting>` を `DirectoryHandle` として grant するための薄い包み。
struct Shared(Arc<Counting>);

impl DirectoryHandle for Shared {
    fn policy_id(&self) -> NonZeroU128 {
        self.0.policy
    }
    fn symlink_policy(&self) -> SymlinkPolicy {
        SymlinkPolicy::DenyAll
    }
    fn open_file(
        &self,
        _path: &RelativePath,
        _request: OpenFileRequest,
    ) -> Result<Box<dyn FileHandle>, AdapterError> {
        Err(AdapterError::Host("unused".into()))
    }
    fn create_dir(&self, _path: &RelativePath) -> Result<(), AdapterError> {
        Ok(())
    }
    fn metadata(
        &self,
        _path: &RelativePath,
        _follow_final: bool,
    ) -> Result<PublicMetadata, AdapterError> {
        Ok(PublicMetadata {
            kind: EntryKind::File,
            size_bytes: 0,
            readonly: false,
        })
    }
    fn list(
        &self,
        _path: &RelativePath,
        _max_entries: Option<NonZeroU64>,
    ) -> Result<Vec<DirectoryEntry>, AdapterError> {
        Ok(Vec::new())
    }
    fn remove(&self, _path: &RelativePath, _kind: RemoveKind) -> Result<(), AdapterError> {
        Ok(())
    }
    fn rename(
        &self,
        _from: &RelativePath,
        _to_directory: &dyn DirectoryHandle,
        _to: &RelativePath,
        replace: bool,
    ) -> Result<(), AdapterError> {
        self.0.renames.fetch_add(1, Ordering::SeqCst);
        self.0
            .last_replace
            .store(usize::from(replace), Ordering::SeqCst);
        Ok(())
    }
}

fn root(mount: &str, adapter: &Arc<Counting>, ops: &[FsOperation]) -> FilesystemRoot {
    let ops: BTreeSet<FsOperation> = ops.iter().copied().collect();
    FilesystemRoot::new(
        MountName::new(mount).expect("mount"),
        adapter.policy,
        ops,
        SymlinkPolicy::DenyAll,
        Arc::new(Shared(Arc::clone(adapter))),
    )
    .expect("root")
}

/// `src` mount に `src_ops`、`dst` mount に `dst_ops` を grant した capability set。
fn caps(
    src: &Arc<Counting>,
    src_ops: &[FsOperation],
    dst: &Arc<Counting>,
    dst_ops: &[FsOperation],
) -> CapabilitySet {
    let fs = FilesystemCapability::new(vec![root("src", src, src_ops), root("dst", dst, dst_ops)])
        .expect("fs");
    CapabilitySet::builder()
        .filesystem(fs)
        .expect("grant filesystem")
        .build()
}

fn run_tree(source: &str, caps: CapabilitySet) -> Result<(), TsumugiError> {
    let program = Parser::new(Lexer::new(source).tokenize())
        .parse()
        .expect("parse");
    let mut evaluator = Evaluator::new();
    evaluator.set_capabilities(caps);
    evaluator.run(&program, source.len() as u64)
}

fn run_vm(source: &str, caps: CapabilitySet) -> Result<(), TsumugiError> {
    let program = Parser::new(Lexer::new(source).tokenize())
        .parse()
        .expect("parse");
    let chunk = Compiler::new().compile(&program).expect("compile");
    let mut vm = Vm::new(chunk);
    vm.set_capabilities(caps);
    vm.charge_link(source.len() as u64, &[])
        .expect("charge_link");
    vm.run()
}

const MOVE: &str = "rename(\"@src/a.txt\", \"@dst/b.txt\")\n";
const MOVE_CAUGHT: &str =
    "try\n  rename(\"@src/a.txt\", \"@dst/b.txt\")\ncatch e\n  let caught = 1\nend\n";

type Runner = fn(&str, CapabilitySet) -> Result<(), TsumugiError>;

/// RDD-AT-01: 移動先に `Delete` が無ければ adapter 0 回で sandbox error。tree と VM。
#[test]
fn rdd_at_01_destination_without_delete_is_denied_before_adapter() {
    use FsOperation::{Create, Delete};
    for (label, run) in [("tree", run_tree as Runner), ("vm", run_vm as Runner)] {
        let src = Counting::new(1);
        let dst = Counting::new(2);
        let err = run(MOVE, caps(&src, &[Delete], &dst, &[Create]))
            .expect_err(&format!("{label}: sandbox error のはず"));
        assert_eq!(err.kind(), Some(ErrorKind::Sandbox), "{label}: {err:?}");
        assert_eq!(src.renames.load(Ordering::SeqCst), 0, "{label}");
        assert_eq!(dst.renames.load(Ordering::SeqCst), 0, "{label}");

        // 移動先の有無で結果が変わらない: adapter の metadata は常に「ある」を返すが、
        // 検査は存在を見ないので、script からは同じ error になる（存在 oracle を作らない）。
        let err2 = run(MOVE, caps(&src, &[Delete], &dst, &[Create]))
            .expect_err(&format!("{label}: 2 回目も sandbox error"));
        assert_eq!(err.kind(), err2.kind(), "{label}");
        assert_eq!(err.to_string(), err2.to_string(), "{label}");

        // catch できる。
        let src = Counting::new(1);
        let dst = Counting::new(2);
        run(MOVE_CAUGHT, caps(&src, &[Delete], &dst, &[Create]))
            .unwrap_or_else(|e| panic!("{label}: catch 可能のはず: {e:?}"));
        assert_eq!(src.renames.load(Ordering::SeqCst), 0, "{label}");
        assert_eq!(dst.renames.load(Ordering::SeqCst), 0, "{label}");
    }
}

/// RDD-AT-01 の補足: 移動先に何も無いときと、移動元の `Delete` が無いときも adapter 0 回。
#[test]
fn rdd_at_01_other_missing_grants_still_denied() {
    use FsOperation::{Create, Delete};
    for (label, run) in [("tree", run_tree as Runner), ("vm", run_vm as Runner)] {
        for (src_ops, dst_ops) in [
            (vec![Delete], vec![FsOperation::Read]),
            (vec![Create], vec![Create, Delete]),
        ] {
            let src = Counting::new(1);
            let dst = Counting::new(2);
            let err = run(MOVE, caps(&src, &src_ops, &dst, &dst_ops))
                .expect_err(&format!("{label}: sandbox error"));
            assert_eq!(err.kind(), Some(ErrorKind::Sandbox), "{label}: {err:?}");
            assert_eq!(src.renames.load(Ordering::SeqCst), 0, "{label}");
            assert_eq!(dst.renames.load(Ordering::SeqCst), 0, "{label}");
        }
    }
}

/// RDD-AT-02: 移動先に `Create` と `Delete`、移動元に `Delete` があれば adapter の `rename` が
/// `replace=true` で 1 回呼ばれる。tree と VM。
#[test]
fn rdd_at_02_all_grants_calls_adapter_once_with_replace() {
    use FsOperation::{Create, Delete};
    for (label, run) in [("tree", run_tree as Runner), ("vm", run_vm as Runner)] {
        let src = Counting::new(1);
        let dst = Counting::new(2);
        run(MOVE, caps(&src, &[Delete], &dst, &[Create, Delete]))
            .unwrap_or_else(|e| panic!("{label}: 成功のはず: {e:?}"));
        assert_eq!(src.renames.load(Ordering::SeqCst), 1, "{label}");
        assert_eq!(
            src.last_replace.load(Ordering::SeqCst),
            1,
            "{label}: replace=true"
        );
        assert_eq!(dst.renames.load(Ordering::SeqCst), 0, "{label}");
    }
}

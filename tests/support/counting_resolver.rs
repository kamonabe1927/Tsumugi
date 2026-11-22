//! 呼ばれた回数を数える in-memory の `ModuleResolver`（handle 経路の import のテストが共有する）。
//!
//! filesystem を読まない。`sources` のキーは正規化済みの specifier（`import "a.tsg"` なら
//! `@default/a.tsg`）。

#![allow(dead_code)]

use std::collections::HashMap;
use std::num::{NonZeroU128, NonZeroUsize};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tsumugi::{
    AdapterError, CapabilityCallContext, CapabilitySet, DataClassification, ModuleChunk, ModuleId,
    ModuleResolver, ModuleSource, ResolveRequest, ResolvedModule,
};

struct InMemorySource {
    bytes: Vec<u8>,
    offset: usize,
}

impl ModuleSource for InMemorySource {
    fn read_chunk(
        &mut self,
        _context: &mut CapabilityCallContext<'_>,
        max_bytes: NonZeroUsize,
    ) -> Result<ModuleChunk, AdapterError> {
        if self.offset >= self.bytes.len() {
            return Ok(ModuleChunk::Eof);
        }
        let end = (self.offset + max_bytes.get()).min(self.bytes.len());
        let chunk = self.bytes[self.offset..end].to_vec();
        self.offset = end;
        Ok(ModuleChunk::Bytes(chunk))
    }
}

/// `resolve` の呼び出し回数を数える resolver。
pub struct CountingResolver {
    sources: HashMap<String, Vec<u8>>,
    calls: Arc<AtomicUsize>,
}

impl CountingResolver {
    /// `resolve` の回数を数える共有 counter とあわせて作る。
    pub fn new(sources: &[(&str, &str)]) -> (Self, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let sources = sources
            .iter()
            .map(|(id, body)| (id.to_string(), body.as_bytes().to_vec()))
            .collect();
        (
            Self {
                sources,
                calls: Arc::clone(&calls),
            },
            calls,
        )
    }

    /// この resolver だけを grant した capability set を作る。
    pub fn capabilities(self) -> CapabilitySet {
        CapabilitySet::builder()
            .module_resolver(Arc::new(self))
            .expect("module_resolver の grant")
            .build()
    }
}

impl ModuleResolver for CountingResolver {
    fn policy_id(&self) -> NonZeroU128 {
        NonZeroU128::new(99).expect("非ゼロ")
    }

    fn resolve(
        &self,
        _context: &mut CapabilityCallContext<'_>,
        request: ResolveRequest<'_>,
    ) -> Result<ResolvedModule, AdapterError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let id = ModuleId::new(request.specifier)
            .map_err(|e| AdapterError::Host(format!("bad id: {e:?}")))?;
        let bytes = self
            .sources
            .get(request.specifier)
            .cloned()
            .ok_or_else(|| AdapterError::Host("not found".to_string()))?;
        Ok(ResolvedModule {
            id,
            source: Box::new(InMemorySource { bytes, offset: 0 }),
            classification: DataClassification::Public,
        })
    }
}

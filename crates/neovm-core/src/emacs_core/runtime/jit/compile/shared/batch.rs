//! A worker's unsealed code batch. No entry address exists in this API until
//! consuming finalization has sealed every member. Dropping an unfinished
//! batch discards its compiler bookkeeping; earlier published code stays mapped.

use std::marker::PhantomData;
use std::rc::Rc;

use cranelift_module::Module;

use super::super::shim_refs::ShimGroup;
use super::super::sink::{LeafEntry, define_with_context};
use super::split::remap_imports;
use super::{SharedJit, declare_leaf_entry};

use super::super::CompileError;
use super::super::lowering::RegallocChoice;
use super::split::JobPayload;
use super::{WorkerBackend, WorkerCode, bump_stats, module_leaf_limit};
use crate::emacs_core::jit::backend::BackendError;
use crate::emacs_core::jit::stats::{CompilePhase, enter_phase};

/// At most this many definitions share one worker finalization boundary.
pub(crate) const WORKER_BATCH_LIMIT: usize = 8;

/// A prepared function ID tied to this borrowed backend. It cannot be invoked.
struct UnsealedCode {
    allocator: RegallocChoice,
    function: cranelift_module::FuncId,
    code_bytes: usize,
}

/// Guard-owned failure injection: never process-global or visible in
/// production, and exercised after real compiler/module work has occurred.
#[cfg(test)]
#[derive(Clone, Copy)]
enum TestFault {
    PrepareError,
    PreparePanic,
    FinalizeErrorAfter(usize),
    FinalizePanicAfter(usize),
}

#[derive(Clone, Copy)]
enum BatchState {
    Empty,
    Unsealed,
    Poisoned,
    Sealed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BatchCapacity {
    Available,
    SealFirst,
    Aborted,
}

/// The result of consuming an entire successfully sealed batch.
pub(crate) struct SealedBatch {
    /// Same order as successful preparations.
    pub(crate) codes: Vec<WorkerCode>,
    pub(crate) modules_finalized: usize,
    pub(crate) arena_seals: u64,
}

/// Exclusively borrows the compiler, so a module cannot retire outside this
/// batch while one of its opaque IDs is pending. Failure drops the whole batch.
#[must_use = "an unsealed batch must be finalized or explicitly abandoned"]
pub(crate) struct WorkerBatch<'a> {
    backend: &'a mut WorkerBackend,
    definitions: Vec<UnsealedCode>,
    state: BatchState,
    _owner_thread: PhantomData<Rc<()>>,
    #[cfg(test)]
    test_fault: Option<TestFault>,
}

static_assertions::assert_not_impl_any!(WorkerBatch<'static>: Send, Sync);

impl WorkerBackend {
    /// Bounded rollover fixture: only this owned backend's already-created module.
    #[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
    pub(crate) fn approach_module_limit_for_worker_test(&mut self, allocator: RegallocChoice) {
        self.0.modules[allocator.index()]
            .as_mut()
            .expect("fixture must publish a warm-up leaf")
            .leaves = module_leaf_limit().saturating_sub(1);
    }

    pub(crate) fn batch(&mut self) -> WorkerBatch<'_> {
        WorkerBatch {
            backend: self,
            definitions: Vec::with_capacity(WORKER_BATCH_LIMIT),
            state: BatchState::Empty,
            _owner_thread: PhantomData,
            #[cfg(test)]
            test_fault: None,
        }
    }
}

impl WorkerBatch<'_> {
    pub(crate) fn capacity(&self, allocator: RegallocChoice) -> BatchCapacity {
        match self.state {
            BatchState::Empty | BatchState::Unsealed => {}
            BatchState::Poisoned | BatchState::Sealed => return BatchCapacity::Aborted,
        }
        if self.definitions.len() == WORKER_BATCH_LIMIT
            || (!self.definitions.is_empty()
                && self.backend.0.modules[allocator.index()]
                    .as_ref()
                    .is_some_and(|module| module.leaves >= module_leaf_limit()))
        {
            BatchCapacity::SealFirst
        } else {
            BatchCapacity::Available
        }
    }

    pub(crate) fn prepare(&mut self, payload: JobPayload) -> Result<(), CompileError> {
        match self.state {
            BatchState::Empty | BatchState::Unsealed => {}
            BatchState::Poisoned | BatchState::Sealed => {
                return Err(CompileError::Backend(BackendError::Define(
                    "worker batch is no longer writable".into(),
                )));
            }
        }
        if self.capacity(payload.regalloc) == BatchCapacity::SealFirst {
            self.state = BatchState::Poisoned;
            return Err(CompileError::Backend(BackendError::Define(
                "worker batch must seal before another definition".into(),
            )));
        }
        // Errors/panics can leave a declaration or context mid-update even
        // before a definition is returned. Drop resets this backend then too.
        self.state = BatchState::Poisoned;
        let definition = self.prepare_payload(payload)?;
        #[cfg(test)]
        match self.test_fault {
            Some(TestFault::PrepareError) => {
                self.test_fault = None;
                return Err(CompileError::Backend(BackendError::Define(
                    "injected error after worker definition".into(),
                )));
            }
            Some(TestFault::PreparePanic) => {
                self.test_fault = None;
                panic!("injected panic after worker definition");
            }
            _ => {}
        }
        self.definitions.push(definition);
        self.state = BatchState::Unsealed;
        Ok(())
    }

    pub(crate) fn finish(mut self) -> Result<SealedBatch, CompileError> {
        match self.state {
            BatchState::Empty | BatchState::Unsealed => {}
            BatchState::Poisoned | BatchState::Sealed => {
                return Err(CompileError::Backend(BackendError::Finalize(
                    "cannot publish an abandoned worker batch".into(),
                )));
            }
        }
        self.state = BatchState::Poisoned;
        let finalize_phase = enter_phase(CompilePhase::Finalize);
        let arena_before = self.arena_seals();
        let mut finalized = [false; RegallocChoice::COUNT];
        let mut modules_finalized = 0;
        #[cfg(test)]
        let test_fault = self.test_fault.take();
        #[cfg(test)]
        Self::test_finalize_fault(test_fault, 0)?;
        for definition in &self.definitions {
            let index = definition.allocator.index();
            if !finalized[index] {
                self.backend.0.modules[index]
                    .as_mut()
                    .expect("preparation installed its module")
                    .module
                    .finalize_definitions()
                    .map_err(|error| {
                        CompileError::Backend(BackendError::Finalize(error.to_string()))
                    })?;
                finalized[index] = true;
                modules_finalized += 1;
                #[cfg(test)]
                Self::test_finalize_fault(test_fault, modules_finalized)?;
            }
        }
        let codes = self
            .definitions
            .iter()
            .map(|definition| {
                let module = &self.backend.0.modules[definition.allocator.index()]
                    .as_ref()
                    .expect("the borrowed module cannot retire")
                    .module;
                WorkerCode {
                    entry: module.get_finalized_function(definition.function) as usize,
                    code_bytes: definition.code_bytes,
                }
            })
            .collect();
        let arena_seals = self.arena_seals().saturating_sub(arena_before);
        bump_stats(|stats| {
            stats.shared_leaves += self.definitions.len() as u64;
            stats.split_payloads += self.definitions.len() as u64;
        });
        self.state = BatchState::Sealed;
        drop(finalize_phase);
        Ok(SealedBatch {
            codes,
            modules_finalized,
            arena_seals,
        })
    }

    #[cfg(test)]
    fn test_finalize_fault(fault: Option<TestFault>, finalized: usize) -> Result<(), CompileError> {
        match fault {
            Some(TestFault::FinalizeErrorAfter(after)) if after == finalized => {
                Err(CompileError::Backend(BackendError::Finalize(
                    "injected error during worker batch finalization".into(),
                )))
            }
            Some(TestFault::FinalizePanicAfter(after)) if after == finalized => {
                panic!("injected panic during worker batch finalization");
            }
            _ => Ok(()),
        }
    }

    fn prepare_payload(&mut self, mut payload: JobPayload) -> Result<UnsealedCode, CompileError> {
        if !payload.portable {
            return Err(CompileError::Backend(BackendError::Define(
                "a worker payload must name every external import".into(),
            )));
        }
        let setup_phase = enter_phase(CompilePhase::Setup);
        let jit = &mut self.backend.0;
        let has_group = |group| {
            payload
                .imports
                .iter()
                .any(|(_, shim)| shim.group() == group)
        };
        let tier2_profile = has_group(ShimGroup::Tier2Profile);
        let collection_journal = has_group(ShimGroup::CollectionJournal);
        let collection_observation_gate = has_group(ShimGroup::CollectionObservationGate);
        let array_profile = has_group(ShimGroup::Tier2ArrayProfile);
        let sink_versions = has_group(ShimGroup::OptSink);
        if array_profile || sink_versions {
            jit.ensure_module_selected_with_collection_journal(
                payload.regalloc,
                tier2_profile,
                array_profile,
                sink_versions,
                collection_journal,
                collection_observation_gate,
            )?;
        } else {
            jit.ensure_module_with_collection_journal(
                payload.regalloc,
                tier2_profile,
                collection_journal,
                collection_observation_gate,
            )?;
        }
        drop(setup_phase);
        let allocator = payload.regalloc;
        let SharedJit { modules, ctx, .. } = jit;
        let shared = modules[allocator.index()]
            .as_mut()
            .expect("preparation installed its module");
        remap_imports(&mut payload.func, &payload.imports, &shared.shims)?;
        let signature = payload.func.signature.clone();
        let fid = declare_leaf_entry(
            &mut shared.module,
            LeafEntry {
                name: &payload.name,
                linkage: payload.linkage,
                signature: &signature,
            },
            payload.named,
            shared.leaves,
        )?;
        ctx.clear();
        ctx.func = payload.func;
        let defined = define_with_context(&mut shared.module, fid, ctx, payload.disasm);
        let code_bytes = ctx
            .compiled_code()
            .map_or(0, |code| code.code_buffer().len());
        shared.module.clear_context(ctx);
        defined?;
        // Definition succeeded, so consume this module's unique sequence
        // immediately, even though finalization/publication happens later.
        shared.leaves += 1;
        Ok(UnsealedCode {
            allocator,
            function: fid,
            code_bytes,
        })
    }

    fn arena_seals(&self) -> u64 {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        return self.backend.0.arena.stats().seals;
        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        return 0;
    }
}

impl Drop for WorkerBatch<'_> {
    fn drop(&mut self) {
        match self.state {
            BatchState::Empty | BatchState::Sealed => {}
            BatchState::Unsealed | BatchState::Poisoned => *self.backend = WorkerBackend::new(),
        }
    }
}

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
#[path = "batch/tests/batch_test.rs"]
mod tests;

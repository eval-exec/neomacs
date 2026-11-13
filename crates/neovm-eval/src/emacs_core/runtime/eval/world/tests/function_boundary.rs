//! Test-only bridge. The owned projection cannot capture this Context/header.
use crate::emacs_core::eval::{Context, NamedCallTarget};
#[cfg(feature = "jit")]
use crate::emacs_core::jit::stats::epoch::EpochCounters;
use neovm_value::intern::SymId;
#[cfg(not(feature = "jit"))]
use neovm_value::tagged::gc::function_boundary_testkit::NoEpochRecorder;
use neovm_value::tagged::gc::function_boundary_testkit::{
    FunctionBoundaryPoint, FunctionEraEdit, FunctionEraReceipt, WriterEpochRecorder,
};
use neovm_value::tagged::transport::SharedRoot;

/// The upper-layer recorder the F4b testkit calls on the WRITER thread: it
/// snapshots this thread's epoch counters before the edit and records the
/// delta after the writer's post-edit snapshot. The counter type stays here;
/// value never inspects it.
#[cfg(feature = "jit")]
#[derive(Default)]
struct C4EpochRecorder {
    before: Option<EpochCounters>,
    delta: Option<EpochCounters>,
}

#[cfg(feature = "jit")]
impl C4EpochRecorder {
    fn writer_delta(&self) -> EpochCounters {
        self.delta
            .expect("the testkit called after_writer before returning")
    }
}

#[cfg(feature = "jit")]
impl WriterEpochRecorder for C4EpochRecorder {
    fn before_writer(&mut self) {
        self.before = Some(EpochCounters::snapshot());
    }

    fn after_writer(&mut self) {
        let before = self
            .before
            .expect("the testkit called before_writer before after_writer");
        self.delta = Some(EpochCounters::snapshot().since(&before));
    }
}

#[cfg(feature = "jit")]
type EraEpochs = EpochCounters;
#[cfg(not(feature = "jit"))]
type EraEpochs = ();

impl Context {
    #[doc(hidden)]
    pub fn c4_function_era_for_test(
        &mut self,
        ids: [SymId; 2],
        edit: FunctionEraEdit,
        replacement: SharedRoot,
        observe: impl FnMut(FunctionBoundaryPoint),
    ) -> (FunctionEraReceipt, EraEpochs) {
        #[cfg(feature = "jit")]
        let mut epochs = C4EpochRecorder::default();
        #[cfg(not(feature = "jit"))]
        let mut epochs = NoEpochRecorder;
        let result = self
            .admit_owned_proof_parallel(|projection| {
                let result =
                    neovm_value::tagged::gc::function_boundary_testkit::exercise_function_era(
                        projection.era(),
                        ids,
                        edit,
                        &replacement,
                        observe,
                        &mut epochs,
                    );
                drop(replacement);
                let (collector, keeper) = projection
                    .finish()
                    .expect("function boundary clean era finish");
                (collector, keeper, result)
            })
            .expect("function boundary owned admission");
        eprintln!("P3SYM_C4_CACHE_BOUNDARY_TEARDOWN_OK");
        let receipt = result.expect("function boundary after clean serial recovery");
        #[cfg(feature = "jit")]
        return (receipt, epochs.writer_delta());
        #[cfg(not(feature = "jit"))]
        return (receipt, ());
    }

    /// Snapshot the actual cache, without filling or relabeling its target.
    #[doc(hidden)]
    pub fn c4_named_cache_for_test(&self, id: SymId) -> Option<(u64, Option<usize>)> {
        self.named_call_cache.get(&id).map(|entry| {
            let bits = match entry.target {
                NamedCallTarget::Obarray(value) | NamedCallTarget::Subr(value) => {
                    Some(value.bits())
                }
                NamedCallTarget::Void => None,
            };
            (entry.function_epoch, bits)
        })
    }

    /// Warm through the production resolver; no synthetic cache insertion.
    #[doc(hidden)]
    pub fn c4_resolve_named_for_test(&mut self, id: SymId) -> (u64, Option<usize>) {
        let _ = self.resolve_named_call_target_by_id(id);
        self.c4_named_cache_for_test(id)
            .expect("actual named cache fill")
    }
}

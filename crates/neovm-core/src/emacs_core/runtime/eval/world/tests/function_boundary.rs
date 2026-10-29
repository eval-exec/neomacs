//! Test-only bridge. The owned projection cannot capture this Context/header.
use crate::emacs_core::eval::{Context, NamedCallTarget};
use crate::emacs_core::intern::SymId;
use crate::tagged::gc::function_boundary_testkit::{
    FunctionBoundaryPoint, FunctionEraEdit, FunctionEraReceipt,
};
use crate::tagged::transport::SharedRoot;

impl Context {
    pub(crate) fn c4_function_era_for_test(
        &mut self,
        ids: [SymId; 2],
        edit: FunctionEraEdit,
        replacement: SharedRoot,
        observe: impl FnMut(FunctionBoundaryPoint),
    ) -> FunctionEraReceipt {
        let result = self
            .admit_owned_proof_parallel(|projection| {
                let result = crate::tagged::gc::function_boundary_testkit::exercise_function_era(
                    projection.era(),
                    ids,
                    edit,
                    &replacement,
                    observe,
                );
                drop(replacement);
                let (collector, keeper) = projection
                    .finish()
                    .expect("function boundary clean era finish");
                (collector, keeper, result)
            })
            .expect("function boundary owned admission");
        eprintln!("P3SYM_C4_CACHE_BOUNDARY_TEARDOWN_OK");
        result.expect("function boundary after clean serial recovery")
    }

    /// Snapshot the actual cache, without filling or relabeling its target.
    pub(crate) fn c4_named_cache_for_test(&self, id: SymId) -> Option<(u64, Option<usize>)> {
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
    pub(crate) fn c4_resolve_named_for_test(&mut self, id: SymId) -> (u64, Option<usize>) {
        let _ = self.resolve_named_call_target_by_id(id);
        self.c4_named_cache_for_test(id)
            .expect("actual named cache fill")
    }
}

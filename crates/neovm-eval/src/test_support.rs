//! Test-support surface for callers ABOVE this crate.
//!
//! `authority` is the TA2 fence (peer rulings 06:0xZ and S-1/S-2): the
//! items in it are V1 constructors that stay unwidened, and the
//! `split_shared_guards` TA2 scan restricts every reference to
//! `crates/neovm-core/core_tests/**`. Nothing here may be called from
//! product code or from another crate's `src/`.

/// TA2 authority fence. Exactly the signed rows live here.
pub mod authority {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::eval::hooks_proof::EvalHooksInstalled;

    /// S-2: the V1 minimal VM harness, reached only with the REAL boot proof.
    ///
    /// The constructor stays `pub(crate)` and unwidened; this fence is the one
    /// route a re-homed core test may use. `proof` must come from an existing
    /// Context (`Context::hooks_installed`), i.e. from core's test_utils boot
    /// through `CORE_EVAL_HOOK_SET`; the forged `EvalHooksInstalled::mint()`
    /// path is private again (S-1).
    pub fn new_minimal_vm_harness(proof: EvalHooksInstalled) -> Context {
        Context::new_minimal_vm_harness(proof)
    }
}

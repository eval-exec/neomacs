//! Test-support surface for callers ABOVE this crate.
//!
//! `authority` is the TA2 fence (peer rulings 06:0xZ, S-1/S-2, 07:3xZ
//! Variant B and 10:1xZ Option B'): the items in it are V1 constructors and
//! fixture entry points that stay unwidened at their original paths, and the
//! `split_shared_guards` TA2 scan restricts every reference to
//! `crates/neovm-core/core_tests/**` (a fully test-gated `src` file may name
//! a LOWER crate's fence, B2). Nothing here may be called from product code
//! or from another crate's ungated `src/`.

/// TA2 authority fence. Exactly the signed AUTHORITY-ITEMS rows live here.
pub mod authority {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::eval::hooks_proof::EvalHooksInstalled;
    use crate::emacs_core::eval::world::Baton;
    use neovm_value::intern::SymId;
    use neovm_value::symbol::Obarray;
    use neovm_value::tagged::gc::{PlainSymbolBacking, RegistrationError};

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

    /// Copied identity of the World retained by a Baton: the `Arc` pointer as
    /// a scalar. Not a World handle; holding the token neither retains nor
    /// frees the World, and observing it never clones the `Arc`.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct WorldIdentity(usize);

    /// Observe the retained World identity without cloning the `Arc`.
    pub fn baton_world_identity(keeper: &Baton) -> WorldIdentity {
        WorldIdentity(keeper.world_identity_ptr())
    }

    /// Whether two observations name the same retained World.
    pub fn same_world(left: WorldIdentity, right: WorldIdentity) -> bool {
        left == right
    }

    /// One-parameter fence spelling of the restricted proof era for this
    /// World's `Baton` keeper (peer 07:3xZ amendment (a)). The original
    /// `world::era` aliases stay crate-private; this is the signed R6
    /// exception, never a VIS widening of the era type.
    pub type WorldProofEra<'heap> = neovm_value::test_support::authority::ProofEra<'heap, Baton>;

    /// Finish refusal of [`WorldProofEra`]; same item as the value fence's
    /// `ProofEraFinishFailure` with this World's keeper.
    pub type WorldProofEraFinishFailure<'heap> =
        neovm_value::test_support::authority::ProofEraFinishFailure<'heap, Baton>;
}

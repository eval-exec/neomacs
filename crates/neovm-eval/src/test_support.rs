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
    use crate::emacs_core::eval::GreenStartError;
    use crate::emacs_core::eval::hooks_proof::EvalHooksInstalled;
    use crate::emacs_core::eval::world::Baton;
    use crate::emacs_core::eval::world::WorldView;
    use crate::host_effect::wake::{ControlNudgePublisher, ControlWake, HostSessionControl};
    use neovm_value::intern::SymId;
    use neovm_value::symbol::Obarray;
    use neovm_value::tagged::gc::{
        AllStopped, FacadeEpochRetention, FacadeMarkExclusionError, FrozenLegacyRoots,
        PlainSymbolBacking, RegistrationError, RendezvousError,
    };
    use std::sync::Arc;

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

    /// S-5: pair a fresh bare `ControlWake` with its publisher for the
    /// re-homed host-input tests.
    ///
    /// `ControlWake::paired` is `pub(crate)` again at its defining path (the
    /// pre-split S6-1 owner seal); a fully test-gated core file may reach it
    /// only through this fence.
    pub fn paired_control_wake() -> (Arc<ControlWake>, ControlNudgePublisher) {
        ControlWake::paired()
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

    /// B1: the twelve symbol-fixture entry points stay in eval's gated
    /// `symbol_fixture` body; these delegators are the only cross-crate route.
    pub fn with_test_symbol_era<R>(
        body: impl for<'heap> FnOnce(WorldProofEra<'heap>, SymId) -> R,
    ) -> R {
        Context::with_test_symbol_era(body)
    }

    pub fn try_with_test_symbol_era<R>(
        body: impl for<'heap> FnOnce(WorldProofEra<'heap>, SymId) -> R,
    ) -> Result<R, RegistrationError> {
        Context::try_with_test_symbol_era(body)
    }

    pub fn try_with_test_symbol_backing_init_era<R>(
        kind: PlainSymbolBacking,
        init: impl FnOnce(&mut Obarray, SymId),
        body: impl for<'heap> FnOnce(WorldProofEra<'heap>, SymId) -> R,
    ) -> Result<R, RegistrationError> {
        Context::try_with_test_symbol_backing_init_era(kind, init, body)
    }

    pub fn with_test_symbol_backing_era<R>(
        kind: PlainSymbolBacking,
        body: impl for<'heap> FnOnce(WorldProofEra<'heap>, SymId) -> R,
    ) -> R {
        Context::with_test_symbol_backing_era(kind, body)
    }

    pub fn with_test_symbol_backings_era<R>(
        kinds: &[PlainSymbolBacking],
        body: impl for<'heap> FnOnce(WorldProofEra<'heap>, Vec<SymId>) -> R,
    ) -> R {
        Context::with_test_symbol_backings_era(kinds, body)
    }

    pub fn with_test_symbol_functions_era<R>(
        kind: PlainSymbolBacking,
        body: impl for<'heap> FnOnce(WorldProofEra<'heap>, Vec<SymId>) -> R,
    ) -> R {
        Context::with_test_symbol_functions_era(kind, body)
    }

    pub fn with_test_symbol_functions_init_era<R>(
        kind: PlainSymbolBacking,
        init: impl FnOnce(&mut Obarray, &[SymId]),
        body: impl for<'heap> FnOnce(WorldProofEra<'heap>, Vec<SymId>) -> R,
    ) -> R {
        Context::with_test_symbol_functions_init_era(kind, init, body)
    }

    pub fn with_test_symbol_name_inputs_era<R>(
        kind: PlainSymbolBacking,
        spelling: &str,
        body: impl for<'heap> FnOnce(WorldProofEra<'heap>, Vec<SymId>) -> R,
    ) -> R {
        Context::with_test_symbol_name_inputs_era(kind, spelling, body)
    }

    pub fn with_test_symbol_name_inputs_init_era<R>(
        kind: PlainSymbolBacking,
        spelling: &str,
        init: impl FnOnce(&mut Obarray, &[SymId]),
        body: impl for<'heap> FnOnce(WorldProofEra<'heap>, Vec<SymId>) -> R,
    ) -> R {
        Context::with_test_symbol_name_inputs_init_era(kind, spelling, init, body)
    }

    pub fn with_test_symbol_intern_race_era<R>(
        kind: PlainSymbolBacking,
        spellings: &[String],
        body: impl for<'heap> FnOnce(WorldProofEra<'heap>, Vec<SymId>) -> R,
    ) -> R {
        Context::with_test_symbol_intern_race_era(kind, spellings, body)
    }

    pub fn with_test_symbol_backings_init_era<R>(
        kinds: &[PlainSymbolBacking],
        init: impl FnOnce(&mut Obarray, &[SymId]),
        body: impl for<'heap> FnOnce(WorldProofEra<'heap>, Vec<SymId>) -> R,
    ) -> R {
        Context::with_test_symbol_backings_init_era(kinds, init, body)
    }

    pub fn try_with_test_symbol_backings_init_era<R>(
        kinds: &[PlainSymbolBacking],
        init: impl FnOnce(&mut Obarray, &[SymId]),
        body: impl for<'heap> FnOnce(WorldProofEra<'heap>, Vec<SymId>) -> R,
    ) -> Result<R, RegistrationError> {
        Context::try_with_test_symbol_backings_init_era(kinds, init, body)
    }

    // K1-ORPHAN (b) follow-up (peer 09:5xZ): the nine authority-class test
    // consumers delegate through these identical-signature fence functions.
    // The original methods are pub(crate) at their defining paths; the
    // re-homed core tests reach them only through this fence. WorldView and
    // GreenStartError are crate-visible types, so the interface lint is
    // allowed here rather than widening either type.

    /// Delegate for `WorldView<TaggedHeap>::heap_root`.
    #[allow(private_interfaces)]
    pub fn world_view_heap_root(
        view: &WorldView<neovm_value::tagged::gc::TaggedHeap>,
    ) -> std::ptr::NonNull<neovm_value::tagged::gc::TaggedHeap> {
        view.heap_root()
    }

    /// Delegate for `WorldView<TaggedHeap>::install_tagged_heap`.
    #[allow(private_interfaces)]
    pub fn world_view_install_tagged_heap(view: &WorldView<neovm_value::tagged::gc::TaggedHeap>) {
        view.install_tagged_heap()
    }

    /// Delegate for `Context::issue_green_baton`.
    #[allow(private_interfaces)]
    pub fn context_issue_green_baton(context: &mut Context) -> Result<Baton, GreenStartError> {
        context.issue_green_baton()
    }

    /// Delegate for `Baton::parallel_epoch_retention`.
    pub fn baton_parallel_epoch_retention(
        baton: &Baton,
    ) -> Result<FacadeEpochRetention, FacadeMarkExclusionError> {
        baton.parallel_epoch_retention()
    }

    /// Delegate for `Baton::parallel_symbol_backing`.
    pub fn baton_parallel_symbol_backing(
        baton: &Baton,
    ) -> neovm_value::symbol::parallel::SymbolBacking {
        baton.parallel_symbol_backing()
    }

    /// Delegate for `Context::capture_parallel_legacy_roots`.
    pub fn context_capture_parallel_legacy_roots(
        context: &Context,
        keeper: &Baton,
    ) -> Result<FrozenLegacyRoots, RendezvousError> {
        context.capture_parallel_legacy_roots(keeper)
    }

    /// P1-CLASSES-ALL-CRATES r2 (N3/N5): typed thread-creation fence for the
    /// re-homed load oracle. The ThreadManager (heap Values, id issuer) never
    /// leaves eval; the test gets only this typed operation.
    pub fn create_thread_for_test(
        context: &mut Context,
        function: neovm_value::value::Value,
        name: crate::emacs_core::threads::ThreadName,
    ) {
        context.create_thread_for_test(function, name)
    }

    /// r2 (N3/N5): typed feature-insert fence for the re-homed load oracle.
    /// `Vec<SymId>` is plain owned data, so the delegate mutates it in place.
    pub fn feature_insert_for_test(context: &mut Context, index: usize, feature: SymId) {
        context.insert_feature_for_test(index, feature)
    }

    /// r2 (N3/N5): typed require-stack push fence for the re-homed load oracle.
    pub fn require_stack_push_for_test(context: &mut Context, sym: SymId) {
        context.push_require_stack_for_test(sym)
    }

    /// r2 (N3/N5): typed loads-in-progress push fence for the re-homed load
    /// oracle. The vector is plain owned data; no manager view escapes.
    pub fn loads_in_progress_push_for_test(
        context: &mut Context,
        value: neovm_value::heap_types::LispString,
    ) {
        context.push_loads_in_progress_for_test(value)
    }

    /// Delegate for `Baton::keeper_designator`.
    pub fn baton_keeper_designator(
        baton: &Baton,
    ) -> Result<neovm_value::symbol::parallel::KeeperWorldDesignator<'_>, RegistrationError> {
        baton.keeper_designator()
    }

    /// Delegate for `Baton::collect_parallel_world_roots`.
    pub fn baton_collect_parallel_world_roots(
        baton: &Baton,
        stopped: &mut AllStopped<'_>,
        out: &mut Vec<neovm_value::tagged::value::TaggedValue>,
    ) -> Result<Vec<*mut *mut neovm_value::tagged::header::MarkerObj>, RendezvousError> {
        baton.collect_parallel_world_roots(stopped, out)
    }

    /// Delegate for `ControlWake::attach_host_session`.
    pub fn control_wake_attach_host_session(wake: &Arc<ControlWake>) -> HostSessionControl {
        wake.attach_host_session()
    }

    // ---- P1-SUBR (peer 15:2xZ; LEAD 15:1xZ items 5-7): global subr
    // registration is authority-class, so it lives in this fence. The
    // originals stay pub(crate); SubrEntry is pub(crate) with a test-support
    // visibility::make(pub) in the crate-private subr_abi chain (the
    // WorldSlot pattern), so this `pub use` is its only public path. Every
    // field stays pub(crate).
    use neovm_value::tagged::gc::HofOperation;
    use neovm_value::tagged::header::SubrDispatchKind;
    use neovm_value::tagged::header::SubrMetadata;
    use neovm_value::tagged::value::TaggedValue;

    /// Fence route for the authoritative subr entry type (item 5).
    pub use crate::emacs_core::eval::subr_abi::SubrEntry;

    /// Identical delegate for the registry read (item 5).
    pub fn lookup_global_subr_entry(sym: SymId) -> Option<SubrEntry> {
        crate::emacs_core::eval::lookup_global_subr_entry(sym)
    }

    /// Identical delegate for the registry write (item 6).
    pub fn register_global_subr_entry(sym: SymId, entry: SubrEntry) {
        crate::emacs_core::eval::register_global_subr_entry(sym, entry)
    }

    /// Copied read of the entry's dispatch class; the field stays pub(crate)
    /// (item 6).
    pub fn subr_entry_dispatch_kind(entry: &SubrEntry) -> SubrDispatchKind {
        entry.dispatch_kind
    }

    /// One typed change for the forged-registration tests (item 6).
    /// `BodyFrom` can only splice a REAL HOF builtin body, never an
    /// arbitrary fn pointer.
    pub enum SubrEntryChange {
        BodyFrom(HofOperation),
        MinArgs(u16),
        DispatchKind(SubrDispatchKind),
    }

    /// Apply exactly one typed change to a copied entry (item 6).
    pub fn with_subr_entry_change(entry: SubrEntry, change: SubrEntryChange) -> SubrEntry {
        let mut entry = entry;
        match change {
            SubrEntryChange::BodyFrom(operation) => {
                let source =
                    crate::emacs_core::eval::subr_abi::builtin_hof_entry_for_test(operation);
                entry.function = source.function;
            }
            SubrEntryChange::MinArgs(min_args) => entry.min_args = min_args,
            SubrEntryChange::DispatchKind(kind) => entry.dispatch_kind = kind,
        }
        entry
    }

    /// Fence delegate for the HOF fixture (item 7).
    pub fn builtin_hof_entry_for_test(operation: HofOperation) -> SubrEntry {
        crate::emacs_core::eval::subr_abi::builtin_hof_entry_for_test(operation)
    }

    /// Fence delegate for the metadata fixture (item 7).
    pub fn leaked_subr_metadata_for_test() -> (TaggedValue, *mut SubrMetadata) {
        crate::emacs_core::eval::subr_abi::leaked_subr_metadata_for_test()
    }

    /// Fence route for sealing hand-assembled bytecode (peer 15:30Z item 8b;
    /// consumer: core_tests issue554_throw_roundtrip). Runs the real `seal_ops`
    /// normalizer plus `verify_stack_effects`, exactly like the crate-private
    /// `ByteCodeFunctionExt::seal_hand_assembled_ops`; sealing gates the
    /// unchecked-fetch driver, so it is SEAL-domain and fence-only, not VIS.
    pub fn seal_hand_assembled_ops_for_test(
        function: &mut crate::emacs_core::bytecode::ByteCodeFunction,
    ) {
        crate::emacs_core::bytecode::ByteCodeFunctionExt::seal_hand_assembled_ops(function)
    }
}

/// Test-support `aset` over a caller-owned Bare argument slice (K1 fence-ask
/// item 11). This never widens `builtin_aset`/`builtin_aset_args`: the Bare
/// policy is exactly the one the cfg(test) Vec form uses.
pub fn builtin_aset_args_bare(
    args: &[neovm_value::value::Value],
) -> neovm_value::error::EvalResult {
    crate::emacs_core::builtins::collections::builtin_aset_args(
        crate::emacs_core::builtins::collections::ArraySymbolSource::Bare,
        args,
    )
}

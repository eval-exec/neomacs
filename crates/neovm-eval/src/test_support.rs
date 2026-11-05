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
    /// P1-CLASSES-ALL-CRATES FENCE: reset the symbol writer thread-locals.
    /// A reset is a write (N2), so the fence is the only cross-crate route.
    pub fn reset_symbols_thread_locals_for_test() {
        crate::emacs_core::builtins::symbols::reset_symbols_thread_locals();
    }

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

    /// P1-CLASSES-ALL-CRATES FENCE (peer 17:50Z autoload pilot): replace a
    /// Context's autoload manager with an owned manager.  The manager lives in a
    /// WorldView and holds heap Values, so N3 forbids exposing `&mut`/`&`; this
    /// typed replacement is the only cross-crate write route.
    ///
    /// # Safety
    ///
    /// Every `Value` stored in `manager` must have been allocated in
    /// `context`'s heap, and no collection may have run since those values
    /// were last rooted. After the call `context` is their sole owner.
    pub unsafe fn replace_autoloads_for_test(
        context: &mut crate::emacs_core::Context,
        manager: crate::emacs_core::autoload::AutoloadManager,
    ) {
        // SAFETY: the caller upholds the value-ownership contract above; the
        // raw helper has the identical contract and stays crate-private.
        unsafe { context.replace_autoloads_raw(manager) };
    }

    /// P1-CLASSES-ALL-CRATES FENCE (peer 19:24Z wave3 condition): seed the
    /// treesit language cache for the moved builtins fixture.
    /// `TreeSitterManager` is a WorldView and N3 forbids handing out `&mut`,
    /// so this typed operation is the only cross-crate route; the manager
    /// stays inside eval.
    pub fn cache_loaded_language_for_test(
        context: &mut Context,
        key: SymId,
        language: tree_sitter::Language,
        filename: Option<String>,
    ) {
        context.treesit_mut().cache_loaded_language(
            key,
            neovm_text::emacs_core::treesit::LoadedLanguage::from_loaded_parts(
                language, filename, None,
            ),
        );
    }

    /// P1-CLASSES-ALL-CRATES FENCE (peer 19:24Z wave3 condition): install the
    /// moved editfns line/column cache through a typed operation, so
    /// `Context::treesit_mut` keeps no test-support exposure.
    pub fn set_linecol_cache_for_test(
        context: &mut Context,
        buffer: neovm_edit::buffer::BufferId,
        line: i64,
        col: i64,
        bytepos: usize,
    ) {
        context.treesit_mut().set_linecol_cache(
            buffer,
            neovm_text::emacs_core::treesit::LineColCache::from_parts(line, col, bytepos),
        );
    }

    /// Fence delegate: force the JIT profitability gate override. The
    /// original writes a thread-local test override, so it is fence-only
    /// (no static/Cell behind a VIS route).
    pub fn force_profit_gate_for_test(on: bool) {
        crate::emacs_core::jit::compile::force_profit_gate_for_test(on)
    }

    /// Fence delegates for the JIT tiering test knobs (writes to the
    /// function's runtime state; no &mut Runtime escapes).
    #[cfg(feature = "jit")]
    pub fn jit_mark_hot_for_test(function: &crate::emacs_core::bytecode::ByteCodeFunction) {
        use crate::emacs_core::jit::RuntimeStateTieringExt;
        crate::emacs_core::bytecode::ByteCodeFunctionExt::jit_runtime(function).set_hot_for_test()
    }

    #[cfg(feature = "jit")]
    pub fn jit_mark_cold_for_test(function: &crate::emacs_core::bytecode::ByteCodeFunction) {
        use crate::emacs_core::jit::RuntimeStateTieringExt;
        crate::emacs_core::bytecode::ByteCodeFunctionExt::jit_runtime(function).set_cold_for_test()
    }

    #[cfg(feature = "jit")]
    pub fn jit_compiled_id_or_assign_for_test(
        function: &crate::emacs_core::bytecode::ByteCodeFunction,
    ) -> u64 {
        use crate::emacs_core::jit::RuntimeStateTieringExt;
        crate::emacs_core::bytecode::ByteCodeFunctionExt::jit_runtime(function)
            .compiled_id_or_assign()
    }

    /// Fence delegate for the pending-timer service (runs Lisp callbacks;
    /// state-changing, so it is fence-only).
    pub fn service_pending_timers_for_test(
        ctx: &mut crate::emacs_core::eval::Context,
    ) -> Result<(), neovm_value::error::Flow> {
        crate::emacs_core::eval::hooks_proof::wait::service_timers_without_redisplay(ctx)
    }

    // ---- eval_test re-home providers (P1-CLASSES-ALL-CRATES; peer 19:17Z
    // early feedback applied: reads copy out, writes are FENCE). ----

    /// FENCE delegate: value-side Tier-0 pin write (N2/N3; the Atomic stays in
    /// value and is never handed out).
    pub fn set_force_interpret_for_test(state: &crate::emacs_core::jit::RuntimeState, on: bool) {
        neovm_value::test_support::authority::set_force_interpret_for_test(state, on);
    }

    /// Copied JIT tiering reads (C4: values copy out; no runtime reference
    /// escapes the call).
    #[cfg(feature = "jit")]
    pub fn jit_patched_prefix_for_test(
        function: &crate::emacs_core::bytecode::ByteCodeFunction,
    ) -> usize {
        use crate::emacs_core::jit::RuntimeStateTieringExt;
        crate::emacs_core::bytecode::ByteCodeFunctionExt::jit_runtime(function).patched_prefix()
    }

    #[cfg(feature = "jit")]
    pub fn jit_is_hot_for_test(function: &crate::emacs_core::bytecode::ByteCodeFunction) -> bool {
        use crate::emacs_core::jit::RuntimeStateTieringExt;
        crate::emacs_core::bytecode::ByteCodeFunctionExt::jit_runtime(function).is_hot()
    }

    #[cfg(feature = "jit")]
    pub fn jit_compiled_id_for_test(
        function: &crate::emacs_core::bytecode::ByteCodeFunction,
    ) -> Option<u64> {
        use crate::emacs_core::jit::RuntimeStateTieringExt;
        crate::emacs_core::bytecode::ByteCodeFunctionExt::jit_runtime(function).compiled_id()
    }

    /// Data-only audited legacy value write on the Context's obarray view
    /// (N3 FENCE).
    ///
    /// # Safety
    /// `write` must perform only the data-only effects audited on
    /// `ObarrayView::with_legacy_value_write`: no Lisp, no function-cell,
    /// descriptor, storage-identity or generation mutation, and no raw or
    /// interior mutable authority may escape or overlap another phase.
    pub unsafe fn with_legacy_value_write_for_test<R>(
        ctx: &mut crate::emacs_core::eval::Context,
        write: impl for<'manager> FnOnce(&'manager mut neovm_value::symbol::Obarray) -> R,
    ) -> R {
        // SAFETY: the caller upholds the audited data-only contract; the
        // exclusive Context borrow keeps the sole view phase non-overlapping.
        unsafe { ctx.obarray.with_legacy_value_write(write) }
    }

    /// FENCE delegate: enter the panic-safe unwind-cleanup scope (C3 write).
    #[allow(private_interfaces)]
    pub fn enter_unwind_cleanup_guard_for_test(
        ctx: &mut crate::emacs_core::eval::Context,
    ) -> crate::emacs_core::eval::UnwindCleanupGuard<'_> {
        crate::emacs_core::eval::UnwindCleanupGuard::enter(ctx)
    }

    /// FENCE delegate: Tier-I test policy writes (N2).
    pub fn configure_tier_i_for_test(
        ctx: &mut crate::emacs_core::eval::Context,
        mode: crate::emacs_core::eval::TierIMode,
        threshold: u32,
    ) {
        ctx.tier_i.set_mode(mode);
        ctx.tier_i.set_threshold(threshold);
    }

    /// FENCE delegate: plant a stale named-call cache entry (dispatch-cache
    /// write; the entry type and the map stay private).
    pub fn plant_stale_named_call_cache_entry_for_test(
        ctx: &mut crate::emacs_core::eval::Context,
        sym: SymId,
    ) {
        let function_epoch = ctx.obarray().function_epoch();
        ctx.named_call_cache.insert(
            sym,
            crate::emacs_core::eval::NamedCallCacheEntry {
                function_epoch,
                target: crate::emacs_core::eval::NamedCallTarget::Void,
            },
        );
    }

    /// FENCE delegates for the JIT force knobs (thread-local test overrides;
    /// N2: the writes go through the fence, no static/Cell is exposed).
    #[cfg(feature = "jit")]
    pub fn force_inline_for_test(on: Option<bool>) {
        crate::emacs_core::jit::inline::force_inline_for_test(on)
    }

    #[cfg(feature = "jit")]
    pub fn force_inline_predicates_for_test(on: Option<bool>) {
        crate::emacs_core::jit::compile::force_inline_predicates_for_test(on)
    }

    #[cfg(feature = "jit")]
    pub fn force_inline_arith_for_test(on: bool) {
        crate::emacs_core::jit::compile::force_inline_arith_for_test(on)
    }

    #[cfg(feature = "jit")]
    pub fn force_profit_defer_for_test(factor: Option<u32>) {
        crate::emacs_core::jit::force_profit_defer_for_test(factor)
    }

    /// FENCE delegate: set the eval-depth limit for the re-homed test (N2/N3
    /// write; the field stays private and the read copies out).
    pub fn set_max_depth_for_test(ctx: &mut crate::emacs_core::eval::Context, max_depth: usize) {
        ctx.max_depth = max_depth;
    }

    /// FENCE delegate: set the noninteractive flag for the re-homed test (N2
    /// write; the field stays private).
    pub fn set_noninteractive_for_test(
        ctx: &mut crate::emacs_core::eval::Context,
        noninteractive: bool,
    ) {
        ctx.noninteractive = noninteractive;
    }

    /// FENCE delegate: arm the JIT background-compile test mode (mode-arming
    /// guard constructor; N2 writes a thread-local override). `BgMode` is the
    /// VIS plain enum; the scope type stays private.
    #[cfg(feature = "jit")]
    #[allow(private_interfaces)]
    pub fn enter_test_mode_for_test(
        mode: crate::emacs_core::jit::BgMode,
    ) -> crate::emacs_core::jit::bg::TestModeScope {
        crate::emacs_core::jit::bg::TestModeScope::enter(mode)
    }

    /// FENCE delegate: publish a Rust-local fixture value as a GC root.
    ///
    /// # Safety
    /// `value` must be a live heap value owned by `context`'s World and not
    /// collected; the push publishes it for the rest of the Context's life.
    pub unsafe fn push_eval_temp_root_for_test(
        context: &mut crate::emacs_core::eval::Context,
        value: neovm_value::value::Value,
    ) {
        // SAFETY: the caller upholds the live-heap-value contract above.
        unsafe { context.push_eval_temp_root_for_test(value) }
    }

    /// FENCE delegate: install the retained pdump mapping the rows-authority
    /// fixture owns.
    ///
    /// # Safety
    /// `image` must be a genuinely mmapped, process-retained image whose rows
    /// authority the Context may adopt; it must outlive the Context and not be
    /// mutated or unmapped while installed.
    #[allow(private_interfaces)]
    pub unsafe fn install_pdump_image_for_test(
        context: &mut crate::emacs_core::eval::Context,
        image: Option<&'static crate::emacs_core::pdump::mmap_image::LoadedMmapImage>,
    ) {
        // SAFETY: the caller upholds the retained-mapping contract above.
        unsafe { context.install_pdump_image_for_test(image) }
    }

    /// Closure-scoped, panic-safe GC inhibition for the re-homed stack-boundary
    /// oracle. The guard holds the sole Context borrow and restores the depth
    /// on drop, including when the body panics.
    pub fn with_gc_inhibited_for_test<R>(
        context: &mut crate::emacs_core::eval::Context,
        body: impl FnOnce(&mut crate::emacs_core::eval::Context) -> R,
    ) -> R {
        let mut guard = crate::emacs_core::eval::GcInhibitGuard::enter(context);
        body(guard.context())
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

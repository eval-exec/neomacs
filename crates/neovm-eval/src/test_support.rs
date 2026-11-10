//! Test-support surface for callers ABOVE this crate.
//!
//! `authority` is the TA2 fence (peer rulings 06:0xZ, S-1/S-2, 07:3xZ
//! Variant B and 10:1xZ Option B'): the items in it are V1 constructors and
//! fixture entry points that stay unwidened at their original paths, and the
//! `split_shared_guards` TA2 scan restricts every reference to
//! `crates/neovm-core/core_tests/**` (a fully test-gated `src` file may name
//! a LOWER crate's fence, B2). Nothing here may be called from product code
//! or from another crate's ungated `src/`.

/// Hosted execution-fixture helpers (SHAPE (B), LEAD 10:4xZ): the
/// Context-free execution home adapter, built only from value primitives.
pub mod execution_fixtures;

/// C5 JIT attribution probe: the pub(crate) original behind the authority
/// fence. Eval's own tests call this directly; display (a higher crate) calls
/// the signed fence.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn c5_function_epoch_probe() -> Option<fn() -> u64> {
    #[cfg(feature = "jit")]
    {
        Some(c5_jit_epoch_total)
    }
    #[cfg(not(feature = "jit"))]
    {
        None
    }
}

#[cfg(all(any(test, feature = "test-support"), feature = "jit"))]
fn c5_jit_epoch_total() -> u64 {
    crate::emacs_core::jit::stats::epoch::EpochCounters::snapshot().total()
}

/// TA2 authority fence. Exactly the signed AUTHORITY-ITEMS rows live here.
pub mod authority {
    /// P1-CLASSES-ALL-CRATES FENCE: reset the symbol writer thread-locals.
    /// A reset is a write (N2), so the fence is the only cross-crate route.
    pub fn reset_symbols_thread_locals_for_test() {
        crate::emacs_core::builtins::symbols::reset_symbols_thread_locals();
    }

    /// P1 EVAL wave F1 fence (N2 counter reset): zero the continuous
    /// print-number TLS index used by the production printer.  The original
    /// stays pub(crate); this fence is the sole cross-crate route.
    pub fn reset_print_number_index() {
        crate::emacs_core::print::reset_print_number_index();
    }

    /// P1 EVAL wave F1 fence (N2 counter advance): step the continuous
    /// print-number TLS index used by the production printer.  The original
    /// stays pub(crate); this fence is the sole cross-crate route.
    pub fn next_print_number_index() -> i64 {
        crate::emacs_core::print::next_print_number_index()
    }

    /// platform-table fence (lead 14:0xZ, N2): the `DEFVAR_BOOL` rows the
    /// CURRENT reference surface declares, as `(name, expected default)`
    /// pairs. The table, its `SurfaceSet` field and `platform_surface()` stay
    /// `pub(crate)`; core's every-GNU-bool oracle iterates this instead of the
    /// unfiltered static, so one test asserts the X11 surface on a Linux build
    /// and the NS surface on a mac build.
    pub fn gnu_bool_rows_for_surface_for_test() -> Vec<(&'static str, bool)> {
        let surface = crate::platform_surface::platform_surface();
        crate::emacs_core::defvar_bool::GNU_BOOL_VARIABLES
            .iter()
            .filter(|var| var.surfaces.contains(surface))
            .map(|var| {
                (
                    var.name,
                    // A Context is past `main`, so a main-decided row shows
                    // its running value; the rest show the surface's own
                    // declaration value.
                    var.main_value.unwrap_or_else(|| var.value_on(surface)),
                )
            })
            .collect()
    }

    /// platform-table fence (lead 14:0xZ, N2): run `f` with a scoped,
    /// thread-local surface override, restored on return (nested scopes
    /// compose). Production never reads a runtime override; the override
    /// module exists only under `cfg(any(test, feature = "test-support"))`.
    pub fn with_platform_surface_for_test(surface: &str, f: &mut dyn FnMut()) {
        use crate::platform_surface::{PlatformSurface, test_override};
        let surface = match surface {
            "x11" => PlatformSurface::X11,
            "ns" => PlatformSurface::Ns,
            "w32" => PlatformSurface::W32,
            other => panic!("unknown platform surface {other:?}"),
        };
        let guard = test_override::install(surface);
        f();
        drop(guard);
    }

    /// platform-table fence (class (e), memq 11-08): the surface name the
    /// production constructor is using right now ("x11" / "ns" / "w32").
    /// `platform_surface()` stays `pub(crate)`; host-keyed expectations use
    /// this to choose their arm on whatever host runs the test.
    pub fn platform_surface_name_for_test() -> &'static str {
        use crate::platform_surface::PlatformSurface;
        match crate::platform_surface::platform_surface() {
            PlatformSurface::X11 => "x11",
            PlatformSurface::Ns => "ns",
            PlatformSurface::W32 => "w32",
        }
    }

    /// print_buffer F1 fence (lead 13:08Z): the prepare token is `Copy` so a
    /// caller can retain it after its recorded unwind. Reusing it must fail
    /// LOUDLY in every profile; the pre-fix path returned silently in release
    /// (`debug_assert!(false); return`) and `pending` answered stale retained
    /// bytes. The accumulator, its token and its operations stay `pub(crate)`;
    /// this fixture is the one cross-crate route, called only by the
    /// print-accumulator pin in neovm-core's moved print tests.
    pub fn print_accumulator_stale_token_for_test(context: &mut crate::emacs_core::eval::Context) {
        let active = context.world_print().prepare();
        context.world_print().unwind(active);
        // A stale `Copy` token after the unwind: post-fix the accumulator is
        // Idle and the operation panics instead of silently dropping bytes.
        context.world_print().append(active, b"x");
    }

    /// print_buffer F1-a fence (lead 13:58Z, ABA): a token from an EARLIER
    /// prepare used while a NEWER print is Active must panic. The state's top
    /// is an (epoch, depth) pair; "something is active" alone would let this
    /// stale token corrupt the newer print's cursor.
    pub fn print_accumulator_aba_stale_for_test(context: &mut crate::emacs_core::eval::Context) {
        let first = context.world_print().prepare();
        context.world_print().unwind(first);
        let _second = context.world_print().prepare();
        context.world_print().append(first, b"x");
    }

    /// print_buffer F1-a fence (lead 13:58Z): an OUTER token is rejected while
    /// an inner `prepare` owns the cursor (the LIFO top is the inner epoch).
    pub fn print_accumulator_outer_while_inner_active_for_test(
        context: &mut crate::emacs_core::eval::Context,
    ) {
        let outer = context.world_print().prepare();
        let _inner = context.world_print().prepare();
        context.world_print().append(outer, b"x");
    }

    /// print_buffer F1-a fence (lead 13:58Z): the outer token must become
    /// valid again once the inner print unwinds (GNU's `print_unwind` restores
    /// the outer epoch). Returns the bytes pending under the outer token: with
    /// the accumulator's GNU nested-save quirk the result is "oO".
    pub fn print_accumulator_outer_after_inner_unwind_for_test(
        context: &mut crate::emacs_core::eval::Context,
    ) -> Vec<u8> {
        let outer = context.world_print().prepare();
        context.world_print().append(outer, b"o");
        let inner = context.world_print().prepare();
        context.world_print().append(inner, b"i");
        context.world_print().unwind(inner);
        context.world_print().append(outer, b"O");
        let pending = context.world_print().pending(outer).to_vec();
        context.world_print().unwind(outer);
        pending
    }
    /// P1 F1 FILE-NOTIFY FENCE (peer 12:57Z (c)): reset the file-notify
    /// thread-local registry.  A reset is a TLS write (N2); the fence is the
    /// only cross-crate route and the original stays `pub(crate)` with no
    /// A-prime.
    pub fn reset_file_notify_thread_locals_for_test() {
        crate::emacs_core::builtins::file_notify::reset_file_notify_thread_locals();
    }

    /// CG-1 fence (peer 11-08): GNU `makunbound` on the context's obarray
    /// through the audited writer proxy.  The adapter method is `pub(crate)`;
    /// this fence is the only cross-crate test route.
    pub fn makunbound_for_test(context: &mut Context, name: &str) {
        context.obarray_mut().makunbound(name);
    }

    /// CG-1 fence (peer 11-08): raise the whole-table function clock through
    /// the audited writer proxy.  The adapter method is `pub(crate)`; this
    /// fence is the only cross-crate test route.
    pub fn invalidate_all_function_bindings_for_test(
        context: &mut Context,
        why: FunctionEpochBump,
    ) {
        context.obarray_mut().invalidate_all_function_bindings(why);
    }

    /// P1 F1 FILE-NOTIFY FENCES (peer 12:57Z (c)): the event-injection and
    /// thread-local-registry fixtures stay authority-only.  The
    /// implementations live in the owning module, where the private fields
    /// and the private thread-local are reachable; the originals stay
    /// unwidened.
    pub use crate::emacs_core::builtins::file_notify::authority::{
        drain_batch_for_test, file_notify_register_for_test, file_notify_unregister_for_test,
    };
    #[cfg(target_os = "linux")]
    pub use crate::emacs_core::builtins::file_notify::authority::{
        inotify_event_native_name_lisp_for_test, inotify_event_registered_name_lisp_for_test,
    };

    use crate::emacs_core::bytecode::vm::resumed_chain_probe::{ChainObservation, FireRequest};
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::eval::GreenStartError;
    use crate::emacs_core::eval::hooks_proof::EvalHooksInstalled;
    use crate::emacs_core::eval::world::Baton;
    use crate::emacs_core::eval::world::WorldView;
    use crate::emacs_core::eval::{CconvMemoEvent, CconvMemoMode, TrustRefusal};
    use crate::host_effect::wake::{ControlNudgePublisher, ControlWake, HostSessionControl};
    use neovm_value::intern::SymId;
    use neovm_value::symbol::{FunctionEpochBump, Obarray};
    use neovm_value::tagged::gc::{
        AllStopped, FacadeEpochRetention, FacadeMarkExclusionError, FrozenLegacyRoots,
        PlainSymbolBacking, RegistrationError, RendezvousError,
    };
    use neovm_value::value::Value;
    use std::sync::Arc;

    /// C1 FENCE (peer 22:11Z shape): the ONE cross-crate route for owned-proof
    /// parallel admission in re-homed tests. Delegates ONLY to
    /// `Context::admit_owned_proof_parallel` (ProofPlan): no generic plan
    /// parameter, no ProductionPlan/WeakProofPlan route, and `admit_owned_with`
    /// stays `pub(crate)`. Safe because the real admission transaction (keeper
    /// issue, legacy-root capture, drain_to_quiescent, recovery) runs under
    /// `&mut Context`; the projection is lifetime-bound and consumed by finish.
    /// The projection/error types gain A' `make(pub)` only in this build; no
    /// field or constructor is exposed.
    #[doc(hidden)]
    pub fn admit_owned_proof_parallel_for_test<R>(
        context: &mut Context,
        body: impl for<'heap> FnOnce(
            crate::emacs_core::eval::world::OwnedProofProjection<'heap>,
        )
            -> (neovm_value::tagged::gc::QuiescentCollector<'heap>, Baton, R),
    ) -> Result<R, crate::emacs_core::eval::world::OwnedParallelError> {
        context.admit_owned_proof_parallel(body)
    }

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

    /// C5 intern-growth JIT attribution probe (LEAD 05:3xZ).
    ///
    /// The value-layer C5 bridge records no function-epoch facts itself; the
    /// evaluator owns the counter type. Callers pass this probe to
    /// `neovm_value::test_support::authority::c5_intern_growth_for_test`; the
    /// bridge calls it on each actor thread immediately before and after the
    /// real intern operations, so a bump is attributed to the committing
    /// thread. Without the JIT feature the historic shape has no counter.
    ///
    /// This fence delegates to the pub(crate) original at the test_support
    /// module level: eval's own tests call the original (a crate may not name
    /// its own fence; TA2_OWN_FENCE_REFERENCE).
    pub fn c5_function_epoch_probe_for_test() -> Option<fn() -> u64> {
        super::c5_function_epoch_probe()
    }

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

    /// Constructor fence for `SubrEntry` (peer #816): the ONLY cross-crate
    /// test construction path. Every field is explicit; there are no hidden
    /// defaults. Consumers replace struct literals (including the
    /// functional-update `SubrEntry { field: X, ..orig }` form) with this call
    /// and spell the preserved values out. `SubrFn` lives in the crate-private
    /// subr_abi chain, so the fence interface is private by design.
    #[allow(private_interfaces)]
    pub fn subr_entry_for_test(
        function: Option<crate::emacs_core::eval::subr_abi::SubrFn>,
        min_args: u16,
        max_args: Option<u16>,
        dispatch_kind: SubrDispatchKind,
        interactive_spec: Option<
            crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec,
        >,
    ) -> SubrEntry {
        SubrEntry {
            function,
            min_args,
            max_args,
            dispatch_kind,
            interactive_spec,
        }
    }

    /// Copied read of the entry's dispatch class; the field stays pub(crate)
    /// (item 6).
    pub fn subr_entry_dispatch_kind(entry: &SubrEntry) -> SubrDispatchKind {
        entry.dispatch_kind
    }

    /// Copied bool read of the registered entry's function shape (the
    /// `ManySlice` arm), for core's commandp oracle. The `function` field
    /// stays pub(crate) and no function pointer crosses the fence (item 5
    /// family).
    /// N2 counter reset (LEAD 10:1xZ): zero the per-thread subr-lookup
    /// counter for the moved commandp oracle. The original fn stays
    /// pub(crate) with no A-prime widening; this fence is its only
    /// cross-crate route.
    pub fn reset_global_subr_lookup_count() {
        crate::emacs_core::eval::reset_global_subr_lookup_count();
    }

    /// N2 counter reset (LEAD 10:1xZ): zero the obarray materialization
    /// counter for the moved completion oracle. The original fn stays
    /// pub(crate) with no A-prime widening; this fence is its only
    /// cross-crate route.
    pub fn reset_global_obarray_symbol_value_materializations() {
        crate::emacs_core::builtins::symbols::reset_global_obarray_symbol_value_materializations();
    }

    pub fn subr_entry_function_is_many_slice(entry: &SubrEntry) -> bool {
        matches!(
            entry.function,
            Some(crate::emacs_core::eval::subr_abi::SubrFn::ManySlice(_))
        )
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

    // ---- peer #816 write-arm fences: construction for the two remaining
    // parent types and setters for the fields whose moved tests write them.

    /// Constructor fence for `InternalEventEffects`: every field explicit.
    pub fn internal_event_effects_for_test(
        redisplay_needed: bool,
    ) -> crate::emacs_core::eval::frontend_events::InternalEventEffects {
        crate::emacs_core::eval::frontend_events::InternalEventEffects { redisplay_needed }
    }

    /// Setter fence for `Context::gc_driver_active` (the moved panic
    /// containment test arms and disarms the GC driver flag).
    pub fn set_gc_driver_active_for_test(context: &mut Context, active: bool) {
        context.gc_driver_active = active;
    }

    /// Setter fence for `Context::gc_stress` (peer #982/#1005): the field is
    /// private in every arm now, so a cross-crate write is a compile error and
    /// every legitimate writer goes through this fence.
    pub fn set_gc_stress_for_test(context: &mut Context, enabled: bool) {
        context.set_gc_stress_for_test(enabled);
    }

    /// Reset fence for `Context::abbrevs`: the moved root-retention test
    /// replaces the manager, a write to the world slot.
    pub fn reset_abbrevs_for_test(context: &mut Context) {
        set_abbrevs_manager_for_test(
            context,
            crate::emacs_core::eval::data::abbrev::AbbrevManager::new(),
        );
    }

    /// Setter fence for `Context::abbrevs` (peer #1005 field remediation):
    /// the moved root-retention test installs a `from_dump` manager.
    pub fn set_abbrevs_manager_for_test(
        context: &mut Context,
        manager: crate::emacs_core::eval::data::abbrev::AbbrevManager,
    ) {
        *context.abbrevs = manager;
    }

    /// Read fence for the dynamic-module registry (peer #1005 field
    /// remediation): the registry handle is interior-mutable
    /// (`HeapRegistryHandle::borrow_mut` takes `&self`), so handing
    /// `&Handle` across the crate boundary would be an unfenced write
    /// route.  This fence borrows the registry inside the defining crate
    /// and copies only the GC root `Value`s.
    pub fn dynamic_module_registry_gc_roots_for_test(
        context: &Context,
        roots: &mut Vec<neovm_value::value::Value>,
    ) {
        crate::emacs_core::eval::data::dynamic_module_registry::collect_dynamic_module_registry_gc_roots(
            &context.dynamic_module_registry,
            roots,
        );
    }

    /// Constructor fence for `ModuleSuffixes` (peer #1005 field
    /// remediation): every field explicit; the moved lread oracle's struct
    /// literals switch to it and the fields stay `pub(crate)`.
    pub fn module_suffixes_for_test(
        primary: &'static str,
        secondary: Option<&'static str>,
    ) -> crate::emacs_core::lread::ModuleSuffixes {
        crate::emacs_core::lread::ModuleSuffixes { primary, secondary }
    }

    /// Setter fence for `Context::gc_count` (peer #1005 field remediation):
    /// the field is `pub(crate)` in every arm and this is the only
    /// cross-crate test write route.
    pub fn set_gc_count_for_test(context: &mut Context, count: u64) {
        context.set_gc_count_for_test(count);
    }

    /// Setter fence for `Context::gc_inhibit_depth` (peer #1005 field
    /// remediation).
    pub fn set_gc_inhibit_depth_for_test(context: &mut Context, depth: usize) {
        context.set_gc_inhibit_depth_for_test(depth);
    }

    /// Guard fence: enter a GC-inhibit bracket. The moved regex fixture
    /// builders raise the inhibit depth while Rc/Value locals must not be
    /// collected; `leave_gc_inhibit_depth_for_test` closes the bracket.
    pub fn enter_gc_inhibit_depth_for_test(context: &mut Context) {
        let depth = context.gc_inhibit_depth_for_test();
        context.set_gc_inhibit_depth_for_test(depth + 1);
    }

    /// Guard fence: leave a GC-inhibit bracket opened by
    /// `enter_gc_inhibit_depth_for_test`.
    pub fn leave_gc_inhibit_depth_for_test(context: &mut Context) {
        let depth = context.gc_inhibit_depth_for_test();
        context.set_gc_inhibit_depth_for_test(depth - 1);
    }

    /// Setter fence for `Context::gc_pending` (peer #1005 field
    /// remediation).
    pub fn set_gc_pending_for_test(context: &mut Context, pending: bool) {
        context.set_gc_pending_for_test(pending);
    }

    /// Borrow-scoped mutable access to `Context::bc_buf` (peer #1005 field
    /// remediation): the moved bytecode-backtrace tests build and edit the
    /// operand stack, so push/extend/truncate/index writes go through this
    /// fence while plain reads use `Context::bc_buf_for_test`.
    pub fn with_bc_buf_mut_for_test<R>(
        context: &mut Context,
        f: impl FnOnce(&mut Vec<neovm_value::value::Value>) -> R,
    ) -> R {
        f(&mut context.bc_buf)
    }

    /// Setter fence for `Context::lexenv`: the moved quit test installs a
    /// crafted lexical environment to observe the discard path.
    pub fn set_lexenv_for_test(context: &mut Context, lexenv: neovm_value::value::Value) {
        context.lexenv = lexenv;
    }

    /// Borrow-scoped mutable access to `AbbrevManager::tables` (peer #816
    /// setter fence): `get_mut` on the field's map is a write; the field
    /// itself stays a read surface.
    pub fn with_abbrev_tables_mut_for_test<R>(
        manager: &mut crate::emacs_core::eval::data::abbrev::AbbrevManager,
        f: impl FnOnce(
            &mut std::collections::HashMap<
                SymId,
                crate::emacs_core::eval::data::abbrev::AbbrevTable,
            >,
        ) -> R,
    ) -> R {
        f(&mut manager.tables)
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

    /// Typed key-echo state write for the re-homed keyboard oracle. The
    /// KBoard field stays private (N1); this fence is the only cross-crate
    /// route and exposes no manager view.
    #[allow(private_interfaces)]
    pub fn set_key_echo_state_for_test(
        context: &mut Context,
        state: crate::emacs_core::eval::data::command_loop::KeyEchoState,
    ) {
        context.command_loop_mut().keyboard.kboard.key_echo_state = state;
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
    /// (no static/Cell behind a VIS route). jit-only: the knob lives in the
    /// cfg(feature = "jit") compiler module.
    #[cfg(feature = "jit")]
    pub fn force_profit_gate_for_test(on: bool) {
        crate::emacs_core::jit::compile::force_profit_gate_for_test(on)
    }

    /// P2 test-target combo fence (N2 TLS write): force the AOT enabled seam
    /// for the moved eval AOT benches.  The seam stays pub(crate) in its owning
    /// module; this fence is its only cross-crate route.
    #[cfg(feature = "jit")]
    pub fn aot_set_forced_enabled_for_test(v: bool) {
        crate::emacs_core::jit::aot::test_support::set_forced_enabled(v);
    }

    /// P2 test-target combo fence (N2 TLS write): inject a pre-loaded AOT unit
    /// by content hash for the moved eval AOT benches.  The seam stays
    /// pub(crate); the unit type is A-prime-widened as a read surface.
    #[cfg(feature = "jit")]
    pub fn aot_inject_unit_for_test(
        content_hash: u128,
        unit: std::sync::Arc<crate::emacs_core::jit::compile::LoadedUnit>,
    ) {
        crate::emacs_core::jit::aot::test_support::inject_unit(content_hash, unit);
    }

    /// P2 test-target combo fence (N2 TLS reset): clear every AOT test
    /// override.  The raw reset stays pub(crate) with no A-prime.
    #[cfg(feature = "jit")]
    pub fn aot_reset_for_test() {
        crate::emacs_core::jit::aot::test_support::reset();
    }

    /// P2 test-target combo fence (N2 TLS reset): drop the compiled-leaf
    /// cache.  The raw clear stays pub(crate) with no A-prime.
    #[cfg(feature = "jit")]
    pub fn clear_jit_cache_for_test() {
        crate::emacs_core::jit::cache::clear();
    }

    /// P2 test-target combo fence (N2 TLS reset): zero this thread's
    /// compile-stall aggregate.  The raw reset stays pub(crate) with no
    /// A-prime.
    #[cfg(feature = "jit")]
    pub fn reset_compile_stats_for_test() {
        crate::emacs_core::jit::stats::reset_compile_stats();
    }

    /// Fence delegate for the native-leaf depth counter. The counter stays
    /// `pub(crate)` on the JIT cache; callers copy one `u32` out.
    #[cfg(feature = "jit")]
    pub fn native_depth_for_test() -> u32 {
        crate::emacs_core::jit::cache::native_depth()
    }

    /// Fence delegate: arm the next Main select park for one pending connect.
    /// `Control` and the park types stay crate-private. The channels and the
    /// wake closure are the only values that leave eval.
    pub fn arm_main_connect_select_park_for_test(
        context: &mut crate::emacs_core::eval::Context,
        process: neovm_value::heap_types::ProcessId,
    ) -> (
        std::sync::mpsc::Receiver<()>,
        std::sync::mpsc::Sender<()>,
        std::sync::mpsc::Receiver<Result<(), String>>,
        Box<dyn Fn() + Send>,
    ) {
        context.arm_main_connect_select_park_for_test(process)
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

    /// FENCE delegate: Tier-I mode write that keeps the ambient threshold
    /// (N2; the field stays private).
    pub fn set_tier_i_mode_for_test(
        ctx: &mut crate::emacs_core::eval::Context,
        mode: crate::emacs_core::eval::TierIMode,
    ) {
        ctx.tier_i.set_mode(mode);
    }

    /// FENCE delegate: zero the Tier-I counters between relocated tests (N2
    /// reset; the TierI state and its maps stay private).
    pub fn clear_tier_i_for_test(ctx: &mut crate::emacs_core::eval::Context) {
        ctx.tier_i.clear_for_test();
    }

    /// FENCE delegate: Tier-I lazy-frame policy write (N2; the field stays
    /// private).
    pub fn set_tier_i_lazy_frames_for_test(ctx: &mut crate::emacs_core::eval::Context, on: bool) {
        ctx.tier_i.set_lazy_frames(on);
    }

    /// FENCE delegate: copied Tier-I counter snapshot for the relocated
    /// assertions (C4 read; no borrow of the private TierI state escapes).
    pub fn tier_i_stats_for_test(
        ctx: &crate::emacs_core::eval::Context,
    ) -> crate::emacs_core::eval::TierIStats {
        ctx.tier_i.stats().clone()
    }

    /// FENCE delegate: copied Tier-I mode for the relocated knob test (C4
    /// read).
    pub fn tier_i_mode_for_test(
        ctx: &crate::emacs_core::eval::Context,
    ) -> crate::emacs_core::eval::TierIMode {
        ctx.tier_i.mode()
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

    /// Setter fence for `Context::depth` (peer #982/#1005): the field is
    /// private in every arm now; consumers write it through this fence and
    /// read it through `Context::depth_for_test`.
    pub fn set_depth_for_test(ctx: &mut crate::emacs_core::eval::Context, depth: usize) {
        ctx.depth = depth;
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

    /// P1-HOOKS-2 write rule: `load_symbol_table` interns the dump's names and
    /// installs the thread-local PDUMP_LOAD_*_REMAP tables. That is a
    /// logic-state write of names and remaps, not GC/heap memory, so the
    /// cross-crate route is a SAFE fence and the converter stays pub(crate).
    pub fn load_symbol_table_for_test(
        table: &crate::emacs_core::pdump::types::DumpSymbolTable,
    ) -> Result<(), crate::emacs_core::pdump::DumpError> {
        crate::emacs_core::pdump::convert::load_symbol_table(table)
    }

    /// P1-HOOKS-2 write rule: installs the dumped fontset snapshot into the
    /// fontset registry (logic-state write; safe fence, converter stays
    /// pub(crate)).
    pub fn load_fontset_registry_for_test(
        dfr: &crate::emacs_core::pdump::types::DumpFontsetRegistry,
    ) {
        crate::emacs_core::pdump::convert::load_fontset_registry(dfr)
    }

    /// P1-HOOKS-2 write rule: clears the thread-local PDUMP_LOAD_* remaps
    /// (logic-state write; safe fence, converter stays pub(crate)).
    pub fn finish_load_interner_for_test() {
        crate::emacs_core::pdump::convert::finish_load_interner();
    }

    // ---- vm_test STOP-subset exact-tier JIT fixture (P1 vm_test re-home) ----

    /// The exact JIT code producer a moved oracle wishes to exercise.
    #[cfg(feature = "jit")]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ExactTier {
        Baseline,
        Mir,
        Opt,
    }

    /// Opaque witness for an installed exact-tier leaf. The compiler knob
    /// types, the `CompiledLeaf` handle and the cache entry never leave eval;
    /// only the copied tier identity and the entry counter are readable.
    #[cfg(feature = "jit")]
    pub struct ExactTierWitness {
        selected: ExactTier,
        leaf: std::rc::Rc<crate::emacs_core::jit::compile::CompiledLeaf>,
    }

    #[cfg(feature = "jit")]
    impl ExactTierWitness {
        /// The code producer the cache actually selected for this leaf.
        pub fn selected(&self) -> ExactTier {
            self.selected
        }

        /// Native entries observed by the installed leaf's prologue counter.
        pub fn entry_count(&self) -> u64 {
            self.leaf
                .obs
                .entry_counter()
                .expect("native entry witness")
                .get()
        }
    }

    /// FENCE: compile `function` at exactly `tier`'s code producer, install
    /// the leaf in `context`'s cache for the next call, and return the opaque
    /// witness. Thread-local compiler overrides are restored on scope exit;
    /// the compiler's knob types and the leaf handle never escape eval.
    #[cfg(feature = "jit")]
    pub fn install_exact_tier_for_test(
        context: &mut crate::emacs_core::eval::Context,
        function: neovm_value::value::Value,
        tier: ExactTier,
    ) -> ExactTierWitness {
        use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
        use crate::emacs_core::jit::compile::lowering::RegallocPolicy;
        use crate::emacs_core::jit::compile::opt_census::SelectedTier;
        use crate::emacs_core::jit::compile::{
            self, CompileRequest, OptAdmit, OptEarlyMode, OptMode, OptProfitMode,
        };
        use crate::emacs_core::jit::tier2::{CompileTier, T2Upgrade};
        // This thread owns only compiler scalar overrides. The non-Send guard
        // restores them on early return and assertion failures on this thread.
        struct Settings(std::marker::PhantomData<std::rc::Rc<()>>);
        static_assertions::assert_not_impl_any!(Settings: Send, Sync);
        impl Drop for Settings {
            fn drop(&mut self) {
                compile::force_opt_for_test(None, None);
                compile::force_opt_profit_for_test(None);
                compile::force_opt_require_osr_for_test(None);
                compile::force_opt_early_for_test(None);
                crate::emacs_core::jit::stats::force_observe_for_test(Default::default());
            }
        }
        let _settings = Settings(std::marker::PhantomData);
        crate::emacs_core::jit::stats::force_observe_for_test(
            crate::emacs_core::jit::stats::ObserveOverride {
                entry_count: true,
                ..Default::default()
            },
        );
        let (mode, regalloc, request_tier, _selected) = match tier {
            ExactTier::Baseline => (
                OptMode::Off,
                RegallocPolicy::Auto,
                CompileTier::Plain,
                SelectedTier::Baseline,
            ),
            ExactTier::Mir => (
                OptMode::Legacy,
                RegallocPolicy::Full,
                CompileTier::Plain,
                SelectedTier::Mir,
            ),
            ExactTier::Opt => (
                OptMode::Opt,
                RegallocPolicy::Full,
                CompileTier::Upgrade(T2Upgrade::Feedback),
                SelectedTier::Opt,
            ),
        };
        compile::force_opt_for_test(Some(mode), Some(OptAdmit::ALL));
        compile::force_opt_profit_for_test(Some(OptProfitMode::Off));
        compile::force_opt_require_osr_for_test(Some(false));
        compile::force_opt_early_for_test(Some(OptEarlyMode::Off));
        let source = function.get_bytecode_data().expect("rooted source");
        let leaf = compile::compile_bytecode_function_requested(
            source,
            Some(context.obarray()),
            CompileRequest {
                regalloc,
                bypass_profit_gate: true,
                origin: crate::emacs_core::jit::stats::CompileOrigin::Direct,
                tier: request_tier,
            },
        )
        .expect("exact requested native tier compiles");
        let observed = match leaf.selected_tier() {
            SelectedTier::Baseline => ExactTier::Baseline,
            SelectedTier::Mir => ExactTier::Mir,
            SelectedTier::Opt => ExactTier::Opt,
            other => panic!("unexpected selected tier: {other:?}"),
        };
        let leaf = crate::emacs_core::jit::cache::install_leaf_for_test(context, source, leaf);
        ExactTierWitness {
            selected: observed,
            leaf,
        }
    }

    /// P1-CLASSES-ALL-CRATES C3 FENCE (value wave-2 V1 r2): disjoint borrows
    /// of the Context's obarray and its mutable tagged-heap view, for the
    /// re-homed obarray-claim test. Same contract as `Context::tagged_heap_mut`;
    /// the accessor itself stays pub(crate) and this fence is its only public
    /// path.
    ///
    /// # Safety
    ///
    /// The caller must be the owning thread of this Context's World. The heap
    /// view must only be used for in-place heap operations through its
    /// `DerefMut` target; it must never be replaced, swapped with another
    /// Context's view, or moved across Worlds. The two returned borrows are
    /// disjoint, so the obarray may be read while the heap view is live.
    pub unsafe fn obarray_and_tagged_heap_mut(
        context: &mut Context,
    ) -> (
        &neovm_value::symbol::Obarray,
        &mut neovm_value::tagged::gc::TaggedHeap,
    ) {
        // SAFETY: the caller upholds the split-accessor contract above.
        unsafe { context.obarray_and_tagged_heap_mut() }
    }
    /// Re-homed jit/tests BOOT-FULL batch: thread-local HOF-inline override
    /// write (`NEOVM_JIT_HOF_INLINE` is a process knob).
    #[cfg(feature = "jit")]
    pub fn force_hof_inline_for_test(on: Option<bool>) {
        crate::emacs_core::jit::compile::force_hof_inline_for_test(on);
    }
    /// Re-homed jit switch_dispatch BOOT-FULL batch: enter the LEGACY backend
    /// scope (scalar per-thread compiler override; restored on drop).
    #[cfg(feature = "jit")]
    #[must_use = "the backend override ends when the returned guard is dropped"]
    pub fn enter_legacy_opt_mode_for_test() -> impl Drop {
        crate::emacs_core::jit::compile::opt_mode_scope_for_test(
            crate::emacs_core::jit::compile::OptMode::Legacy,
        )
    }

    /// Re-homed jit switch_dispatch BOOT-FULL batch: observe compile stats
    /// (including the MIR verdict) for the duration of `f`, restoring the
    /// thread override on exit (including panic).
    #[cfg(feature = "jit")]
    pub fn with_stats_observation_for_test<R>(f: impl FnOnce() -> R) -> R {
        struct Restore;
        impl Drop for Restore {
            fn drop(&mut self) {
                crate::emacs_core::jit::stats::force_observe_for_test(Default::default());
            }
        }
        crate::emacs_core::jit::stats::force_observe_for_test(
            crate::emacs_core::jit::stats::ObserveOverride {
                stats: true,
                naming: false,
                entry_count: false,
            },
        );
        let _restore = Restore;
        f()
    }
    /// Re-homed jit fixture DRIVER: the ordered fixture protocol with early
    /// per-form compilation. It evaluates forms and installs JIT cache
    /// entries, so it is AUTHORITY rather than VIS; SAFE (`&mut Context`
    /// proves ownership; the returned Value is standard).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn eval_fixture_program(
        ev: &mut crate::emacs_core::eval::Context,
        program: &str,
    ) -> Result<neovm_value::value::Value, neovm_value::error::EvalError> {
        super::jit_fixture::eval_fixture_program(ev, program)
    }

    /// Re-homed jit fixture DRIVER with an explicit prepared-function list;
    /// same contract as [`eval_fixture_program`].
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn eval_fixture_program_named(
        ev: &mut crate::emacs_core::eval::Context,
        program: &str,
        names: &[&str],
    ) -> Result<neovm_value::value::Value, neovm_value::error::EvalError> {
        super::jit_fixture::eval_fixture_program_named(ev, program, names)
    }

    /// Re-homed jit spec-pair batch: thread-local FORCE-SLOW-SPEC override
    /// write (N2).
    #[cfg(feature = "jit")]
    pub fn force_slow_spec_for_test(on: Option<bool>) {
        crate::emacs_core::jit::compile::force_slow_spec_for_test(on);
    }

    /// P1 CCONV family N2: set the memo policy for one test context. The
    /// memo state stays private; the fence is the only cross-crate write.
    pub fn set_cconv_memo_mode_for_test(context: &mut Context, mode: CconvMemoMode) {
        context.cconv_memo.set_mode(mode);
    }

    /// P1 CCONV family N2: enable or disable the fast untrimmed path.
    pub fn set_cconv_memo_fast_for_test(context: &mut Context, fast: bool) {
        context.cconv_memo.set_fast(fast);
    }

    /// P1 CCONV family N2: set the verify-mode strictness flag.
    pub fn set_cconv_memo_strict_for_test(context: &mut Context, strict: bool) {
        context.cconv_memo.set_strict(strict);
    }

    /// P1 CCONV family N2 reset: forget the trusted set so the next use
    /// rebuilds it.
    pub fn reset_cconv_trusted_for_test(context: &mut Context) {
        context.cconv_memo.reset_trusted_for_test();
    }

    /// P1 CCONV family N2: request one complete collection at the start of
    /// the next observed run.
    pub fn collect_cconv_in_next_observed_run_for_test(context: &mut Context) {
        context.cconv_memo.collect_in_next_observed_run_for_test();
    }

    /// P1 CCONV family N2: corrupt one recorded analysis so `verify` has
    /// something to catch.
    pub fn inject_wrong_cconv_analysis_for_test(context: &mut Context) -> bool {
        context.cconv_memo.inject_wrong_analysis_for_test()
    }

    /// P1 CCONV family C4 read: one counter, copied out of the private memo.
    pub fn cconv_memo_count_for_test(context: &Context, event: CconvMemoEvent) -> u64 {
        context.cconv_memo.stats().count(event)
    }

    /// P1 CCONV family C4 read: the trusted-set refusal, copied. The caller
    /// inspects the copy immediately; it is never stored across a GC.
    pub fn cconv_trusted_refusal_for_test(context: &Context) -> Option<TrustRefusal> {
        context.cconv_memo.trusted().refusal().cloned()
    }

    /// P1 CCONV family C4 read: the trusted cells copied as plain
    /// `(symbol, value)` words. Callers must inspect the copies immediately
    /// and never keep them across a GC (peer condition b).
    pub fn cconv_trusted_cells_for_test(
        context: &Context,
    ) -> Vec<(neovm_value::intern::SymId, Value)> {
        context.cconv_memo.trusted().trusted_cells().to_vec()
    }

    /// P1 CCONV family C4 read: the `(t)` empty-environment value, copied.
    /// The caller inspects the copy immediately; it is never stored across a
    /// GC (peer condition b).
    pub fn cconv_trusted_empty_env_for_test(context: &Context) -> Value {
        context.cconv_memo.trusted().empty_env()
    }
    /// P1 RESUMED-CHAIN family N2 FENCE: arm the per-op probe on this
    /// thread before a run. The probe thread-locals stay private.
    pub fn arm_resumed_chain_probe_for_test(context: &Context, fire: Option<FireRequest>) {
        crate::emacs_core::bytecode::vm::resumed_chain_probe::arm(context, fire);
    }

    /// P1 RESUMED-CHAIN family N2 FENCE: disarm the probe, copying the
    /// context's exit cleanup state, and return the copied observation
    /// (trace, levels, fire outcome, entry/exit cleanup state).
    pub fn disarm_resumed_chain_probe_for_test(context: &Context) -> ChainObservation {
        crate::emacs_core::bytecode::vm::resumed_chain_probe::disarm(context).observation()
    }

    /// Copied observations of the producer's actual deferred native-depth reset.
    #[cfg(all(feature = "test-support", feature = "jit"))]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct DepthResetProbe {
        panicked: bool,
        retained_depth: u32,
        normal_reset_depth: u32,
    }

    #[cfg(all(feature = "test-support", feature = "jit"))]
    impl DepthResetProbe {
        /// Read the producer-captured value from this owned observation.
        pub fn panicked(&self) -> bool {
            self.panicked
        }

        /// Read the producer-captured value from this owned observation.
        pub fn retained_depth(&self) -> u32 {
            self.retained_depth
        }

        /// Read the producer-captured value from this owned observation.
        pub fn normal_reset_depth(&self) -> u32 {
            self.normal_reset_depth
        }
    }

    /// FENCE: run the producer's real leaked-native-guard/reset probe on a
    /// fresh joined OS thread. Guard construction, the writable TLS key, and
    /// cleanup after capturing the observations stay private to the producer.
    #[cfg(all(feature = "test-support", feature = "jit"))]
    pub fn native_depth_reset_leak_probe_for_test() -> DepthResetProbe {
        let observation = crate::emacs_core::jit::cache::native_depth_reset_leak_probe_for_test();
        DepthResetProbe {
            panicked: observation.panicked,
            retained_depth: observation.retained_depth,
            normal_reset_depth: observation.normal_reset_depth,
        }
    }
    /// C1 copied observations of the real variable-cache carrier boundaries.
    #[cfg(all(feature = "test-support", feature = "vm-profile"))]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct VarCacheCarrierResetObservation {
        seeded_count: u64,
        parked_count: u64,
        reset_counts: [u64; 16],
    }

    #[cfg(all(feature = "test-support", feature = "vm-profile"))]
    impl VarCacheCarrierResetObservation {
        /// Read the producer-captured value from this owned observation.
        pub fn seeded_count(&self) -> u64 {
            self.seeded_count
        }

        /// Read the producer-captured value from this owned observation.
        pub fn parked_count(&self) -> u64 {
            self.parked_count
        }

        /// Read the producer-captured value from this owned observation.
        pub fn reset_counts(&self) -> [u64; 16] {
            self.reset_counts
        }
    }

    /// C1 regression executed from core because the eval unit target is red
    /// at the base. The producer joins its fresh OS thread before returning.
    #[cfg(all(feature = "test-support", feature = "vm-profile"))]
    pub fn var_cache_carrier_reset_probe_for_test() -> VarCacheCarrierResetObservation {
        let observation = crate::emacs_core::eval::var_cache_carrier_reset_probe_for_test();
        VarCacheCarrierResetObservation {
            seeded_count: observation.seeded_count,
            parked_count: observation.parked_count,
            reset_counts: observation.reset_counts,
        }
    }
}

/// Re-homed jit switch_dispatch BOOT-FULL batch: copied inline-switch census.
#[cfg(feature = "jit")]
pub fn switch_inline_sites_for_test() -> usize {
    crate::emacs_core::jit::compile::switch_dispatch::inline_switch_sites_for_test()
}

/// Re-homed jit switch_dispatch BOOT-FULL batch: copied MIR-build verdict.
#[cfg(feature = "jit")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MirBuildVerdictCopy {
    Ok,
    BadOperand,
    StackUnderflow,
    Unsupported(&'static str),
    Other(String),
}

/// Re-homed jit switch_dispatch BOOT-FULL batch: the independent MIR rebuild
/// of one body, with and without its GNU byte-offset map.
#[cfg(feature = "jit")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MirBuildCopy {
    pub has_offset_map: bool,
    pub with_map: MirBuildVerdictCopy,
    pub without_map: MirBuildVerdictCopy,
}

/// Re-homed jit switch_dispatch BOOT-FULL batch: rebuild the body's MIR from
/// its own ops/constants/offset map/arity and report both verdicts as copies.
#[cfg(feature = "jit")]
pub fn mir_build_copy_for_test(
    function: &crate::emacs_core::bytecode::ByteCodeFunction,
) -> MirBuildCopy {
    use crate::emacs_core::bytecode::ByteCodeFunctionExt;
    use crate::emacs_core::bytecode::function_slots::FunctionParamsExt as _;
    let ops = function.executable_ops();
    let map = function.executable_gnu_byte_offset_map();
    let arity = function
        .params
        .stack_shape()
        .expect("fixture stack parameters")
        .required();
    let constants: Vec<neovm_value::value::Value> = function.constants.iter().copied().collect();
    let verdict = |offset_map| match crate::emacs_core::jit::mir::build_mir(
        ops, &constants, offset_map, arity,
    ) {
        Ok(_) => MirBuildVerdictCopy::Ok,
        Err(crate::emacs_core::jit::compile::CompileError::BadOperand) => {
            MirBuildVerdictCopy::BadOperand
        }
        Err(crate::emacs_core::jit::compile::CompileError::StackUnderflow) => {
            MirBuildVerdictCopy::StackUnderflow
        }
        Err(crate::emacs_core::jit::compile::CompileError::UnsupportedOp(what)) => {
            MirBuildVerdictCopy::Unsupported(what)
        }
        Err(other) => MirBuildVerdictCopy::Other(format!("{other:?}")),
    };
    MirBuildCopy {
        has_offset_map: map.is_some(),
        with_map: verdict(map),
        without_map: verdict(None),
    }
}

/// Re-homed jit switch_dispatch BOOT-FULL batch: copied leaf observations.
#[cfg(feature = "jit")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledLeafCopy {
    pub tier_name: &'static str,
    pub mir_verdict: Option<String>,
}

/// Re-homed jit switch_dispatch BOOT-FULL batch: copied tier + MIR-verdict
/// read; the leaf and its observation box stay private.
#[cfg(feature = "jit")]
pub fn compiled_leaf_copy_for_test(
    leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
) -> CompiledLeafCopy {
    CompiledLeafCopy {
        tier_name: leaf.tier().name(),
        mir_verdict: leaf.obs.mir_verdict.as_deref().map(str::to_owned),
    }
}

/// Re-homed jit/tests BOOT-FULL batch: copied invocation-heat read.
#[cfg(feature = "jit")]
pub fn jit_heat_for_test(function: &crate::emacs_core::bytecode::ByteCodeFunction) -> u32 {
    use crate::emacs_core::jit::RuntimeStateTieringExt as _;
    crate::emacs_core::bytecode::ByteCodeFunctionExt::jit_runtime(function).heat()
}

/// Re-homed jit spec-pair batch: copied inline-spec call census (u64).
#[cfg(feature = "jit")]
pub fn spec_call_count_for_test() -> u64 {
    crate::emacs_core::jit::compile::SPEC_CALL_COUNT.load(std::sync::atomic::Ordering::Relaxed)
}

/// Re-homed jit spec-pair batch: copied rest-shim fast-path census (u64).
#[cfg(feature = "jit")]
pub fn spec_shim_fast_count_for_test() -> u64 {
    crate::emacs_core::jit::compile::SPEC_SHIM_FAST_COUNT.load(std::sync::atomic::Ordering::Relaxed)
}

/// Re-homed jit spec-pair batch: copied redefinition-pin presence predicate
/// (`!pins.is_empty()`, the same test predicate the origin used).
#[cfg(feature = "jit")]
pub fn redefined_pins_present_for_test() -> bool {
    !crate::emacs_core::jit::cache::redefined_pins_for_test().is_empty()
}

/// Re-homed jit fixture body (V1 one-home; nine eval test files plus the
/// moved spec pair use it). Crate-private: the only cross-crate route is the
/// authority delegates above.
#[cfg(all(feature = "jit", any(test, feature = "test-support")))]
pub(crate) mod jit_fixture {
    use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
    use crate::emacs_core::eval::Context;
    use neovm_value::intern::intern;
    use neovm_value::value::Value;

    /// Root parsed fixture forms and preparation operands on their owning
    /// evaluator, including when a public error or assertion unwinds this
    /// loader.
    struct FixtureRoots<'a> {
        ev: &'a mut Context,
        scope: crate::emacs_core::eval::SpecpdlRootScopeState,
    }

    impl<'a> FixtureRoots<'a> {
        fn new(ev: &'a mut Context) -> Self {
            let scope = ev.save_specpdl_roots();
            Self { ev, scope }
        }
    }

    impl Drop for FixtureRoots<'_> {
        fn drop(&mut self) {
            self.ev.restore_specpdl_roots(self.scope);
        }
    }

    /// Prepare only source-listed fixture functions and their immutable
    /// bytecode constants. No caller/callee body executes here, and no
    /// compilation mode is overridden. Loaded runtime functions are outside
    /// this protocol.
    fn prepare_fixture_functions(ev: &mut Context, names: &[&str]) {
        let roots = FixtureRoots::new(ev);
        let mut pending: Vec<_> = names
            .iter()
            .filter_map(|name| roots.ev.obarray.symbol_function_id(intern(name)))
            .filter(|value| value.is_bytecode())
            .collect();
        for value in &pending {
            roots.ev.push_specpdl_root(*value);
        }
        let mut seen = std::collections::HashSet::new();
        while let Some(value) = pending.pop() {
            if !seen.insert(value.bits()) {
                continue;
            }
            let function = value.get_bytecode_data().expect("fixture bytecode");
            for constant in function
                .constants
                .iter()
                .copied()
                .filter(|v| v.is_bytecode())
            {
                roots.ev.push_specpdl_root(constant);
                pending.push(constant);
            }
            // SAFETY: the fixture owns `roots.ev` exclusively for this
            // preparation and the cache entry stays on this thread.
            crate::emacs_core::jit::cache::prepare_entry_for_test(
                roots.ev as *mut Context,
                function,
            );
        }
    }

    /// Test-only early compilation between existing top-level fixture forms.
    /// In particular, code is installed before the ORIGINAL warm loops
    /// execute; preparation never replays those loops or the later semantic
    /// observation. Only an outer `progn` is expanded: lexical binding
    /// wrappers stay intact.
    pub(crate) fn eval_fixture_program(
        ev: &mut Context,
        program: &str,
    ) -> Result<Value, neovm_value::error::EvalError> {
        let names: std::collections::BTreeSet<_> = program
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .filter(|name| name.starts_with("neovm--"))
            .collect();
        eval_fixture_program_named(ev, program, &names.into_iter().collect::<Vec<_>>())
    }

    /// The same ordered protocol for a fixture with explicitly named
    /// functions outside the `neovm--` namespace. Only those function cells
    /// are prepared.
    pub(crate) fn eval_fixture_program_named(
        ev: &mut Context,
        program: &str,
        names: &[&str],
    ) -> Result<Value, neovm_value::error::EvalError> {
        ev.setup_thread_locals();
        let forms = crate::emacs_core::value_reader::read_all(program, ev.variables()).map_err(
            |error| {
                neovm_value::error::EvalError::signal(
                    intern("error"),
                    vec![Value::string(format!("Read error: {}", error.message))],
                    None,
                )
            },
        )?;
        let roots = FixtureRoots::new(ev);
        for form in &forms {
            roots.ev.push_specpdl_root(*form);
        }
        let mut ordered = Vec::new();
        for form in forms {
            if let Some(parts) = neovm_value::value::list_to_vec(&form)
                && parts.first().and_then(|head| head.as_bare_symbol_name()) == Some("progn")
            {
                ordered.extend(parts.into_iter().skip(1));
            } else {
                ordered.push(form);
            }
        }
        let last_root = roots.ev.push_specpdl_root_slot(Value::NIL);
        let mut result = Value::NIL;
        for form in ordered {
            // The previous form's discarded result must not become a new GC
            // root during the next effectful form (e.g. a weak-key collection).
            roots.ev.set_specpdl_root_slot(&last_root, Value::NIL);
            prepare_fixture_functions(roots.ev, names);
            result = roots.ev.eval_form(form)?;
            roots.ev.set_specpdl_root_slot(&last_root, result);
        }
        prepare_fixture_functions(roots.ev, names);
        Ok(result)
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

/// Owned plain-data summary of what a file pdump's raw mmap sections carry.
///
/// The section-coverage oracle asserts only these flags/counts; no loader,
/// decoder or image handle escapes the reader below.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSectionInventory {
    pub symbol_table_present: bool,
    pub object_starts_present: bool,
    pub object_extra_objects: usize,
    pub obarray_symbols: usize,
    pub charset_payload_bytes: usize,
    pub coding_systems_payload_bytes: usize,
    pub face_table_payload_bytes: usize,
    pub buffers_payload_bytes: usize,
    pub roots_payload_bytes: usize,
    pub autoloads_payload_bytes: usize,
    pub runtime_state_present: bool,
}

/// The ONE copied inspection route for the re-homed section-coverage oracle:
/// run every production section loader over a loaded file image and return
/// the owned summary above. The loaders, their decoded types and the image
/// handle all stay inside this crate; failures keep the production loader's
/// own panic messages.
pub fn raw_section_inventory_for_test(
    image: &crate::emacs_core::pdump::mmap_image::LoadedMmapImage,
) -> RawSectionInventory {
    use crate::emacs_core::pdump::mmap_image::DumpSectionKind;
    let section = |kind: DumpSectionKind| image.section(kind);

    let symbol_table_present = section(DumpSectionKind::SymbolTable).is_some();
    let object_starts_present = section(DumpSectionKind::ObjectStarts).is_some();
    let object_extra_objects = {
        let payload = section(DumpSectionKind::ObjectExtra).expect("object-extra section");
        crate::emacs_core::pdump::object_extra::load_object_extra(payload)
            .expect("object extra")
            .len()
    };
    let obarray_symbols = {
        let payload = section(DumpSectionKind::Obarray).expect("obarray section");
        crate::emacs_core::pdump::obarray_image::load_obarray_section(payload)
            .expect("obarray")
            .symbols
            .len()
    };
    let charset_payload_bytes = {
        let payload = section(DumpSectionKind::CharsetRegistry).expect("charset-registry section");
        let _charset = crate::emacs_core::pdump::charset_image::load_charset_section(payload)
            .expect("charset registry");
        payload.len()
    };
    let coding_systems_payload_bytes = {
        let payload = section(DumpSectionKind::CodingSystems).expect("coding-systems section");
        let _coding_systems =
            crate::emacs_core::pdump::coding_system_image::load_coding_system_section(payload)
                .expect("coding systems");
        payload.len()
    };
    let face_table_payload_bytes = {
        let payload = section(DumpSectionKind::FaceTable).expect("face-table section");
        let _faces = crate::emacs_core::pdump::face_image::load_face_table_section(payload)
            .expect("face table");
        payload.len()
    };
    let buffers_payload_bytes = {
        let payload = section(DumpSectionKind::Buffers).expect("buffers section");
        let _buffers = crate::emacs_core::pdump::buffer_image::load_buffer_manager_section(payload)
            .expect("buffers");
        payload.len()
    };
    let roots_payload_bytes = {
        let payload = section(DumpSectionKind::Roots).expect("roots section");
        let _roots =
            crate::emacs_core::pdump::roots_image::load_roots_section(payload).expect("roots");
        payload.len()
    };
    let autoloads_payload_bytes = {
        let payload = section(DumpSectionKind::Autoloads).expect("autoloads section");
        let _autoloads = crate::emacs_core::pdump::autoloads_image::load_autoloads_section(payload)
            .expect("autoloads");
        payload.len()
    };
    {
        let payload = section(DumpSectionKind::RuntimeManagers).expect("runtime-managers section");
        let _runtime_managers =
            crate::emacs_core::pdump::runtime_managers_image::load_runtime_managers_section(
                payload,
            )
            .expect("runtime managers");
    }
    let runtime_state_present = section(DumpSectionKind::RuntimeState).is_some();

    RawSectionInventory {
        symbol_table_present,
        object_starts_present,
        object_extra_objects,
        obarray_symbols,
        charset_payload_bytes,
        coding_systems_payload_bytes,
        face_table_payload_bytes,
        buffers_payload_bytes,
        roots_payload_bytes,
        autoloads_payload_bytes,
        runtime_state_present,
    }
}

/// The JIT leaf census (`[neovm-jit-final-builtin-leaves]` format) for tests
/// that must prove a call site took a Bcall leaf and that the leaf's guard
/// bounced on a redefinition.  Behind `test-support`, like this module;
/// JIT-only: the census lives in the cfg(feature = "jit") compiler module.
#[cfg(feature = "jit")]
pub fn leaf_census_for_test() -> String {
    crate::emacs_core::jit::compile::leaf_abi::render_leaf_stats()
}

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

    /// Renamed-to w32 event forge. Linux unit builds compile the windows
    /// module; the event fields stay private.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    pub use crate::emacs_core::builtins::file_notify::authority::w32_renamed_to_event_lisp_for_test;
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
    /// FENCE: enter the real serial keeper scope. The original stays
    /// `pub(crate)`; `Baton` is !Clone and !Copy, and the keeper is borrowed
    /// only for the scope, so no keeper escapes.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_weak_sweep_keeper_for_test<R>(
        context: &mut Context,
        body: impl FnOnce(&mut Context, &Baton) -> R,
    ) -> R {
        context.with_weak_sweep_keeper_for_test(body)
    }

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

    /// FENCE (oracleb value-tables re-home): weak-table proof admission for
    /// the re-homed B8 table tests. Delegates ONLY to the closed-fixture
    /// `Context::admit_owned_parallel_with_weak_tables_for_test`
    /// (WeakProofPlan); the original stays `pub(crate)`. The real admission
    /// transaction runs under `&mut Context` and the projection is consumed
    /// by finish, exactly as in the owned-proof fence.
    #[doc(hidden)]
    pub fn admit_owned_weak_tables_parallel_for_test<R>(
        context: &mut Context,
        tables: &[neovm_value::tagged::value::TaggedValue],
        body: impl for<'heap> FnOnce(
            crate::emacs_core::eval::world::OwnedProofProjection<'heap>,
        )
            -> (neovm_value::tagged::gc::QuiescentCollector<'heap>, Baton, R),
    ) -> Result<R, crate::emacs_core::eval::world::OwnedParallelError> {
        context.admit_owned_parallel_with_weak_tables_for_test(tables, body)
    }

    /// FENCE (oracleb worker re-home): the Context-free puthash builtin used
    /// by the re-homed Tier-H cold-mutation test. The original stays
    /// `pub(crate)` and this fence is the only cross-crate route.
    #[doc(hidden)]
    pub fn builtin_puthash_with_symbols_for_test(
        args: Vec<neovm_value::value::Value>,
        symbols_with_pos_enabled: bool,
    ) -> neovm_value::error::EvalResult {
        crate::emacs_core::builtins::builtin_puthash_with_symbols(args, symbols_with_pos_enabled)
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

    /// Drop-scope restore of the per-thread inline-arith override.
    /// `jit_inline_arith_on` stays pub(crate).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[must_use = "inline arith returns to its previous value when the guard drops"]
    pub fn inline_arith_scope_for_test(on: bool) -> impl Drop {
        struct Guard(bool);
        impl Drop for Guard {
            fn drop(&mut self) {
                crate::emacs_core::jit::compile::force_inline_arith_for_test(self.0);
            }
        }
        let was_on = crate::emacs_core::jit::compile::jit_inline_arith_on();
        crate::emacs_core::jit::compile::force_inline_arith_for_test(on);
        Guard(was_on)
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

    /// FENCE: whether inline-arith lowering is on (widening hub, #1871). The
    /// knobs module stays private; this copies the boolean out.
    #[cfg(feature = "jit")]
    pub fn jit_inline_arith_on_for_test() -> bool {
        crate::emacs_core::jit::compile::jit_inline_arith_on()
    }

    /// FENCE: the current JIT background-compile mode (widening hub, #1871).
    /// The `bg` module stays private; `BgMode` is the public plain enum.
    #[cfg(feature = "jit")]
    pub fn bg_mode_for_test() -> crate::emacs_core::jit::BgMode {
        crate::emacs_core::jit::bg::mode()
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

    /// FENCE: opaque identity of the compiled leaf cached for `id`.
    /// Copy + Eq. The pointer stays on the pub(crate) cache helper.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn compiled_leaf_identity_for_test(id: u64) -> Option<LeafIdentity> {
        crate::emacs_core::jit::cache::compiled_leaf_ptr_for_test(id)
            .map(|ptr| LeafIdentity::from_address(ptr as usize))
    }

    /// FENCE: opaque identity of the installed OSR leaf at this header.
    /// Copy + Eq. The pointer stays on the pub(crate) cache helper.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn osr_leaf_identity_for_test(
        func: &crate::emacs_core::bytecode::ByteCodeFunction,
        pc: usize,
    ) -> Option<LeafIdentity> {
        crate::emacs_core::jit::cache::osr_leaf_ptr_for_test(func, pc)
            .map(|ptr| LeafIdentity::from_address(ptr as usize))
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

    /// Opaque compiled-leaf identity. Copy + Eq. The field is the address and
    /// stays private, so a raw `*const` never crosses this module.
    #[cfg(any(test, feature = "test-support"))]
    #[derive(Clone, Copy, Eq, PartialEq, Hash, Debug)]
    pub struct LeafIdentity(usize);

    #[cfg(any(test, feature = "test-support"))]
    impl LeafIdentity {
        pub(crate) fn from_address(address: usize) -> Self {
            Self(address)
        }
    }

    /// Signed knob fences (peer V5, #274/#276/#278). Each delegates to the
    /// `pub(crate)` original. `None` on an `Option` setter restores that
    /// thread's process knob. A sticky setter (no `None`) relies on nextest's
    /// process-per-test. Eval's own unit tests keep calling the originals.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[allow(private_interfaces)]
    pub fn force_direct_call_for_test(on: Option<bool>) {
        crate::emacs_core::jit::compile::force_direct_call_for_test(on);
    }

    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[allow(private_interfaces)]
    pub fn force_direct_memory_for_test(on: Option<bool>) {
        crate::emacs_core::jit::compile::force_direct_memory_for_test(on);
    }

    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[allow(private_interfaces)]
    pub fn force_direct_sites_for_test(
        mode: Option<crate::emacs_core::jit::compile::DirectSitesMode>,
    ) {
        crate::emacs_core::jit::compile::force_direct_sites_for_test(mode);
    }

    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[allow(private_interfaces)]
    pub fn force_direct_shapes_for_test(
        knob: Option<crate::emacs_core::jit::compile::DirectShapesKnob>,
    ) {
        crate::emacs_core::jit::compile::force_direct_shapes_for_test(knob);
    }

    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[allow(private_interfaces)]
    pub fn force_direct_self_kernel_for_test(on: Option<bool>) {
        crate::emacs_core::jit::compile::force_direct_self_kernel_for_test(on);
    }

    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[allow(private_interfaces)]
    pub fn force_direct_self_heat_for_test(
        mode: Option<crate::emacs_core::jit::compile::direct_call::DirectSelfHeat>,
    ) {
        crate::emacs_core::jit::compile::force_direct_self_heat_for_test(mode);
    }

    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[allow(private_interfaces)]
    pub fn force_register_abi_for_test(on: Option<bool>) {
        crate::emacs_core::jit::compile::force_register_abi_for_test(on);
    }

    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[allow(private_interfaces)]
    pub fn force_call_census_for_test(on: Option<bool>) {
        crate::emacs_core::jit::compile::force_call_census_for_test(on);
    }

    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[allow(private_interfaces)]
    pub fn force_inline2_for_test(mode: Option<crate::emacs_core::jit::compile::Inline2Mode>) {
        crate::emacs_core::jit::compile::force_inline2_for_test(mode);
    }

    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[allow(private_interfaces)]
    pub fn force_inline_switch_for_test(on: Option<bool>) {
        crate::emacs_core::jit::compile::force_inline_switch_for_test(on);
    }

    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[allow(private_interfaces)]
    pub fn force_inline_vars_for_test(
        knob: Option<crate::emacs_core::jit::compile::InlineVarsKnob>,
    ) {
        crate::emacs_core::jit::compile::force_inline_vars_for_test(knob);
    }

    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[allow(private_interfaces)]
    pub fn force_switch_hoist_for_test(on: Option<bool>) {
        crate::emacs_core::jit::compile::force_switch_hoist_for_test(on);
    }

    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[allow(private_interfaces)]
    pub fn force_mir_reach_for_test(reach: Option<crate::emacs_core::jit::compile::MirReach>) {
        crate::emacs_core::jit::compile::force_mir_reach_for_test(reach);
    }

    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[allow(private_interfaces)]
    pub fn force_tail_unrooted_for_test(on: Option<bool>) {
        crate::emacs_core::jit::compile::force_tail_unrooted_for_test(on);
    }

    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[allow(private_interfaces)]
    pub fn force_opt_passes_for_test(passes: Option<crate::emacs_core::jit::compile::OptPasses>) {
        crate::emacs_core::jit::compile::force_opt_passes_for_test(passes);
    }

    /// Sticky TLS store of the every-guard-fails harness. `None` is not a
    /// value of this setter; isolation relies on nextest's process-per-test.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn force_deopt_for_test(on: bool) {
        crate::emacs_core::jit::compile::force_deopt_for_test(on);
    }

    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[allow(private_interfaces)]
    pub fn force_mode_for_test(mode: Option<crate::emacs_core::jit::BgMode>) {
        crate::emacs_core::jit::bg::force_mode_for_test(mode);
    }

    /// Process-wide atomic stress knob. `None` restores the environment value.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn force_stress_for_test(on: Option<bool>) {
        crate::emacs_core::jit::bg::force_stress_for_test(on);
    }

    /// Thread-local inline-instruction cap. `None` restores the production knob.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn force_inline_insts_for_test(limit: Option<u64>) {
        crate::emacs_core::jit::bg::force_inline_insts_for_test(limit);
    }

    /// Effective background-compile mode name. Read only.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn bg_mode_name_for_test() -> &'static str {
        crate::emacs_core::jit::bg::bg_mode_name_for_test()
    }

    /// Effective inline-instruction threshold. Read only.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn in_line_insts_for_test() -> u64 {
        crate::emacs_core::jit::bg::in_line_insts_for_test()
    }

    /// Runtime-field write of this function's heat. Sticky. Isolation relies
    /// on nextest's process-per-test.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn set_heat_for_test(function: &crate::emacs_core::bytecode::ByteCodeFunction, heat: u32) {
        use crate::emacs_core::jit::RuntimeStateTieringExt as _;
        crate::emacs_core::bytecode::ByteCodeFunctionExt::jit_runtime(function)
            .set_heat_for_test(heat);
    }

    /// Runtime-field write that stores `HOT_THRESHOLD`. Sticky. Isolation
    /// relies on nextest's process-per-test.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn set_hot_for_test(function: &crate::emacs_core::bytecode::ByteCodeFunction) {
        use crate::emacs_core::jit::RuntimeStateTieringExt as _;
        crate::emacs_core::bytecode::ByteCodeFunctionExt::jit_runtime(function).set_hot_for_test();
    }

    /// Drop-scope restore of the per-thread opt-mode override.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[allow(private_interfaces)]
    #[must_use = "the backend override ends when the returned guard is dropped"]
    pub fn opt_mode_scope_for_test(mode: crate::emacs_core::jit::compile::OptMode) -> impl Drop {
        crate::emacs_core::jit::compile::opt_mode_scope_for_test(mode)
    }

    /// Thread lifecycle: wait until no background job is queued or running.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn quiesce_for_test(timeout: std::time::Duration) -> bool {
        crate::emacs_core::jit::bg::quiesce_for_test(timeout)
    }

    /// Drop-scope restore: workers take no jobs until the guard drops.
    /// `WorkerHold` stays private. `impl Drop` is the cross-crate route.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[must_use = "workers stay held until the guard drops"]
    pub fn hold_workers_for_test() -> impl Drop {
        crate::emacs_core::jit::bg::hold_workers_for_test()
    }

    /// Copied background-compile counters. `BgStats` and `JobClass` stay
    /// crate-private. Discriminants match `JobClass`.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    #[repr(u8)]
    pub enum BgJobClassCopy {
        Osr = 0,
        FirstSight = 1,
        Entry = 2,
        Upgrade = 3,
    }

    /// Copied per-class enqueue and install counts, plus pending probes.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct BgJobStatsCopy {
        pub(crate) enqueued: [u64; 4],
        pub(crate) installed: [u64; 4],
        pub(crate) pending_probes: u64,
    }

    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    impl BgJobStatsCopy {
        /// Field stays pub(crate). Cross-crate reads use this Copy getter.
        pub fn enqueued(self) -> [u64; 4] {
            self.enqueued
        }

        /// Field stays pub(crate). Cross-crate reads use this Copy getter.
        pub fn installed(self) -> [u64; 4] {
            self.installed
        }

        /// Field stays pub(crate). Cross-crate reads use this Copy getter.
        pub fn pending_probes(self) -> u64 {
            self.pending_probes
        }
    }

    /// This thread's background-compile counters.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn bg_stats_for_test() -> BgJobStatsCopy {
        let stats = crate::emacs_core::jit::bg::stats_snapshot();
        debug_assert_eq!(
            crate::emacs_core::jit::bg::JobClass::Osr as u8,
            BgJobClassCopy::Osr as u8
        );
        debug_assert_eq!(
            crate::emacs_core::jit::bg::JobClass::FirstSight as u8,
            BgJobClassCopy::FirstSight as u8
        );
        debug_assert_eq!(
            crate::emacs_core::jit::bg::JobClass::Entry as u8,
            BgJobClassCopy::Entry as u8
        );
        debug_assert_eq!(
            crate::emacs_core::jit::bg::JobClass::Upgrade as u8,
            BgJobClassCopy::Upgrade as u8
        );
        let mut enqueued = [0; 4];
        let mut installed = [0; 4];
        enqueued.copy_from_slice(&stats.enqueued);
        installed.copy_from_slice(&stats.installed);
        const _: () =
            assert!(<crate::emacs_core::jit::bg::JobClass as strum::EnumCount>::COUNT == 4);
        BgJobStatsCopy {
            enqueued,
            installed,
            pending_probes: stats.pending_probes,
        }
    }

    /// What this thread's cache holds for an already-assigned id.
    /// `"none"` when the id was never assigned is the caller's branch.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn cache_entry_kind_for_test(id: u64) -> &'static str {
        crate::emacs_core::jit::cache::cache_entry_kind_for_test(id)
    }

    /// Park this body's tier-up at `heat`.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn defer_tier_up_for_test(
        function: &crate::emacs_core::bytecode::ByteCodeFunction,
        heat: u32,
    ) {
        use crate::emacs_core::bytecode::ByteCodeFunctionExt as _;
        use crate::emacs_core::jit::RuntimeStateTieringExt as _;
        function.jit_runtime().defer_tier_up(heat);
    }

    /// `dispatch_sized` of this body's executable length is `Plan::Compiled`.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn dispatch_sized_is_compiled_for_test(
        function: &crate::emacs_core::bytecode::ByteCodeFunction,
    ) -> bool {
        use crate::emacs_core::bytecode::ByteCodeFunctionExt as _;
        use crate::emacs_core::jit::RuntimeStateTieringExt as _;
        matches!(
            function
                .jit_runtime()
                .dispatch_sized(function.executable_ops().len()),
            crate::emacs_core::jit::Plan::Compiled
        )
    }

    /// Install every pending compile whose backend has finished.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn drain_ready_pending_for_test(ctx: Option<&crate::emacs_core::eval::Context>) {
        crate::emacs_core::jit::cache::drain_ready_pending(ctx);
    }

    /// Whether resolving this function installs or finds a compiled leaf.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn compiled_leaf_resolved_for_test(
        ctx: &mut crate::emacs_core::eval::Context,
        function: &crate::emacs_core::bytecode::ByteCodeFunction,
    ) -> bool {
        crate::emacs_core::jit::cache::resolve_compiled_leaf_ptr(ctx, function).is_some()
    }

    /// Upper bound of the soak's fake-running probes for one finished job.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn stress_probe_skip_max_for_test() -> u8 {
        crate::emacs_core::jit::bg::stress_probe_skip_max_for_test().get()
    }

    /// Process-wide static reset of the call-census counters. The leaf is
    /// renamed so it does not collide with `aot_reset_for_test`.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn call_census_reset_for_test() {
        crate::emacs_core::jit::compile::call_census::reset_for_test();
    }

    /// Sticky TLS store of the OSR override. Isolation relies on nextest's
    /// process-per-test. The original in `jit` stays `pub(crate)`.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn force_osr_for_test(on: bool) {
        crate::emacs_core::jit::force_osr_for_test(on);
    }

    /// Runtime-field write: fix a fresh Context to Green threading.
    ///
    /// Delegates to `Context::set_green_threading_mode_at_construction_for_test`.
    /// `Context::new` is already Sim, and no named test asks for a Sim fence,
    /// so this fence does not add one.
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_threading_mode_at_construction(ctx: &mut Context) {
        ctx.set_green_threading_mode_at_construction_for_test();
    }

    /// Thread admission. Delegates to `Context::admit_threads` and returns
    /// that function's `Result`, including every refusal. No error path is
    /// skipped.
    #[cfg(any(test, feature = "test-support"))]
    #[allow(private_interfaces)]
    pub fn admit_threads(ctx: &mut Context) -> Result<(), crate::emacs_core::eval::AdmissionError> {
        ctx.admit_threads()
    }

    /// Plain copy of an admission refusal. `AdmissionError` stays crate-private.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum AdmissionRefusal {
        NotGreen,
        AlreadyAdmitted,
        SimulatedThreadsLive,
        InhibitDepthOverflow,
        Collector,
        MarkExclusion,
        Capacity,
    }

    fn admission_refusal(error: &crate::emacs_core::eval::AdmissionError) -> AdmissionRefusal {
        use crate::emacs_core::eval::AdmissionError;
        match error {
            AdmissionError::NotGreen => AdmissionRefusal::NotGreen,
            AdmissionError::AlreadyAdmitted => AdmissionRefusal::AlreadyAdmitted,
            AdmissionError::SimulatedThreadsLive => AdmissionRefusal::SimulatedThreadsLive,
            AdmissionError::InhibitDepthOverflow => AdmissionRefusal::InhibitDepthOverflow,
            AdmissionError::Collector(_) => AdmissionRefusal::Collector,
            AdmissionError::MarkExclusion(_) => AdmissionRefusal::MarkExclusion,
            AdmissionError::Capacity => AdmissionRefusal::Capacity,
        }
    }

    /// Thread admission, with the refusal as a plain copy.
    #[cfg(any(test, feature = "test-support"))]
    pub fn admit_threads_refusal_for_test(ctx: &mut Context) -> Result<(), AdmissionRefusal> {
        ctx.admit_threads()
            .map_err(|error| admission_refusal(&error))
    }

    /// Capacity refusal after a full reservation. The private reservation
    /// stays crate-private; the refusal is a plain copy.
    #[cfg(any(test, feature = "test-support"))]
    pub fn admit_threads_refusing_capacity_for_test(
        ctx: &mut Context,
    ) -> Result<(), AdmissionRefusal> {
        ctx.admit_threads_refusing_capacity_for_test()
            .map_err(|error| admission_refusal(&error))
    }

    /// Plain copy of one specpdl entry's binding kind. The entry itself stays
    /// on the stack. `KeyboardLet` is a let subkind, matching
    /// `SpecBinding::let_bound_symbol`.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ObservedSpecBinding {
        Let,
        LetLocal,
        LetDefault,
        KeyboardLet,
        OwnerBinding,
        Other,
    }

    fn observed_spec_binding(
        binding: &crate::emacs_core::eval::SpecBinding,
    ) -> ObservedSpecBinding {
        use crate::emacs_core::eval::{NativeUnwindAction, SpecBinding};
        match binding {
            SpecBinding::Let { .. } => ObservedSpecBinding::Let,
            SpecBinding::LetLocal { .. } => ObservedSpecBinding::LetLocal,
            SpecBinding::LetDefault { .. } => ObservedSpecBinding::LetDefault,
            SpecBinding::NativeUnwind {
                action: NativeUnwindAction::KeyboardBinding(_),
            } => ObservedSpecBinding::KeyboardLet,
            SpecBinding::NativeUnwind {
                action: NativeUnwindAction::OwnerBinding(_),
            } => ObservedSpecBinding::OwnerBinding,
            SpecBinding::LexicalEnv { .. }
            | SpecBinding::GcRoot { .. }
            | SpecBinding::Backtrace { .. }
            | SpecBinding::Backtrace1 { .. }
            | SpecBinding::Backtrace2 { .. }
            | SpecBinding::BacktraceNative { .. }
            | SpecBinding::UnwindProtect { .. }
            | SpecBinding::SaveExcursion { .. }
            | SpecBinding::SaveCurrentBuffer { .. }
            | SpecBinding::SaveRestriction { .. }
            | SpecBinding::LoadsInProgress { .. }
            | SpecBinding::RequireStack { .. }
            | SpecBinding::NativeUnwind { .. }
            | SpecBinding::Nop => ObservedSpecBinding::Other,
        }
    }

    /// One plain kind per specpdl entry, outermost first. Callers do not
    /// borrow the stack.
    pub fn spec_binding_kinds_for_test(context: &Context) -> Vec<ObservedSpecBinding> {
        context
            .specpdl()
            .iter()
            .map(observed_spec_binding)
            .collect()
    }

    /// Plain copy of the echo-geometry window. The ownership field stays private.
    pub fn echo_geometry_window_for_test(
        context: &Context,
    ) -> Option<neovm_edit::emacs_core::display_metrics::WindowId> {
        context.echo_geometry_window_for_test()
    }

    /// Which owner storage a binding shadows. `Channel` stays crate-private.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ObservedOwnerChannel {
        Default,
        BufferLocal(neovm_edit::buffer::BufferId),
    }

    /// What popping an owner binding restores, as copied values. `UndoTarget`
    /// and `SlotValue` stay crate-private.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ObservedOwnerUndo {
        InheritedNone,
        InheritedSuspended(neovm_value::value::Value),
        Previous(neovm_value::value::Value),
        Captured(neovm_value::value::Value),
        Retired,
    }

    /// Live owner records for `id`, outermost first. `None` when this owner
    /// has no slab. The slab and its records stay crate-private.
    pub fn owner_records_for_test(
        context: &Context,
        id: neovm_value::intern::SymId,
    ) -> Option<Vec<(ObservedOwnerChannel, ObservedOwnerUndo)>> {
        use crate::emacs_core::eval::{Channel, UndoTarget};
        let slab = context.owner().slab()?;
        Some(
            slab.records_of(id)
                .map(|record| {
                    let channel = match record.channel {
                        Channel::Default => ObservedOwnerChannel::Default,
                        Channel::BufferLocal(buffer) => ObservedOwnerChannel::BufferLocal(buffer),
                    };
                    let undo = match record.undo {
                        UndoTarget::Inherited { suspended: None } => {
                            ObservedOwnerUndo::InheritedNone
                        }
                        UndoTarget::Inherited {
                            suspended: Some(value),
                        } => ObservedOwnerUndo::InheritedSuspended(value.get()),
                        UndoTarget::Previous(value) => ObservedOwnerUndo::Previous(value.get()),
                        UndoTarget::Captured(value) => ObservedOwnerUndo::Captured(value.get()),
                        UndoTarget::Retired => ObservedOwnerUndo::Retired,
                    };
                    (channel, undo)
                })
                .collect(),
        )
    }

    /// Canonical value on the default channel. `Channel` stays crate-private.
    pub fn owner_canonical_default_for_test(
        context: &mut Context,
        id: neovm_value::intern::SymId,
    ) -> neovm_value::value::Value {
        use crate::emacs_core::eval::Channel;
        context.owner_canonical_value(id, Channel::Default)
    }

    /// Canonical value on one buffer-local channel. `Channel` stays crate-private.
    pub fn owner_canonical_buffer_local_for_test(
        context: &mut Context,
        id: neovm_value::intern::SymId,
        buffer: neovm_edit::buffer::BufferId,
    ) -> neovm_value::value::Value {
        use crate::emacs_core::eval::Channel;
        context.owner_canonical_value(id, Channel::BufferLocal(buffer))
    }

    /// Park this owner. The parked token is opaque; only
    /// [`resume_owner_for_test`] consumes it. `ParkedOwner` stays crate-private.
    #[allow(private_interfaces)]
    pub fn park_owner_for_test(context: &mut Context) -> crate::emacs_core::eval::ParkedOwner {
        context.park_owner()
    }

    /// Resume an owner parked by [`park_owner_for_test`].
    #[allow(private_interfaces)]
    pub fn resume_owner_for_test(
        context: &mut Context,
        parked: crate::emacs_core::eval::ParkedOwner,
    ) {
        context.resume_owner(parked);
    }

    /// FENCE: copied count of warm Tier-I bodies whose first node is an
    /// already-lowered `let` or `let*`. The registry stays private.
    pub fn tier_i_lowered_binding_body_count_for_test(ctx: &Context) -> usize {
        ctx.tier_i.lowered_binding_body_count_for_test()
    }

    /// FENCE: drop a Green World's thread state. `green_teardown` stays
    /// pub(crate).
    pub fn green_teardown_for_test(ctx: &mut Context) {
        ctx.green_teardown();
    }

    /// Plain copy of one main-native wait event. The scheduler log stays
    /// crate-private.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ObservedMainNativeWait {
        Released,
        PhysicalBlockEntered,
        PhysicalBlockReturned,
        Reacquired,
    }

    /// FENCE: copied owner-park count. The registry stays inside Green.
    pub fn parked_owner_len_for_test(ctx: &Context) -> usize {
        ctx.parked_owner_len_for_test()
    }

    /// FENCE: whether every parked owner and host record is gone.
    pub fn parked_owners_empty_for_test(ctx: &Context) -> bool {
        ctx.parked_owners_empty_for_test()
    }

    /// FENCE: whether main's owner batch is parked.
    pub fn parked_contains_main_for_test(ctx: &Context) -> bool {
        ctx.parked_contains_main_for_test()
    }

    /// FENCE: park this owner as runnable. `Wait` stays crate-private.
    pub fn green_park_runnable_for_test(ctx: &mut Context) -> Result<(), neovm_value::error::Flow> {
        ctx.green_park_runnable_for_test()
    }

    /// FENCE: start Green threads. The start-error enum stays crate-private.
    pub fn start_green_threads_for_test(ctx: &mut Context) -> Result<(), String> {
        ctx.start_green_threads_for_test()
    }

    /// FENCE: copied main-native wait log.
    pub fn main_native_wait_events_for_test(ctx: &Context) -> Vec<ObservedMainNativeWait> {
        ctx.main_native_wait_events_for_test()
            .into_iter()
            .map(|event| match event {
                crate::emacs_core::eval::MainNativeWaitObservation::Released => {
                    ObservedMainNativeWait::Released
                }
                crate::emacs_core::eval::MainNativeWaitObservation::PhysicalBlockEntered => {
                    ObservedMainNativeWait::PhysicalBlockEntered
                }
                crate::emacs_core::eval::MainNativeWaitObservation::PhysicalBlockReturned => {
                    ObservedMainNativeWait::PhysicalBlockReturned
                }
                crate::emacs_core::eval::MainNativeWaitObservation::Reacquired => {
                    ObservedMainNativeWait::Reacquired
                }
            })
            .collect()
    }

    /// FENCE: install the fixture GC handshake. The private completion
    /// record stays in the host service; only the worker thread id is forwarded.
    pub fn install_host_fixture_completion_for_test(
        ctx: &Context,
        sender: std::sync::mpsc::Sender<std::thread::ThreadId>,
        count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) {
        ctx.install_host_fixture_completion_for_test(sender, count);
    }

    /// FENCE: finish the fixture GC. A stage error is the same `error` signal
    /// the eval test built. `HostRootError` stays crate-private.
    pub fn finish_host_fixture_gc_for_test(ctx: &Context) -> Result<(), neovm_value::error::Flow> {
        ctx.finish_host_fixture_gc_for_test()
    }

    /// FENCE: unlock the mutex Value. The `MutexHandle` newtype stays
    /// pub(crate); the lookup and the unlock happen here.
    pub fn green_mutex_unlock_value_for_test(
        ctx: &mut Context,
        mutex: &neovm_value::value::Value,
    ) -> neovm_value::error::EvalResult {
        let handle = ctx
            .threads()
            .mutex_id_from_handle(mutex)
            .expect("same-World mutex argument");
        ctx.green_mutex_unlock(handle)
    }

    /// FENCE: the decoder's acceptance verdict. `validate_gnu_bytecode` stays
    /// pub(crate).
    pub fn validate_gnu_bytecode_for_test(
        bytecodes: &[u8],
    ) -> Result<(), crate::emacs_core::bytecode::decode::DecodeError> {
        crate::emacs_core::bytecode::decode::validate_gnu_bytecode(bytecodes)
    }

    /// FENCE: full-decode counter. The counter stays pub(crate).
    #[cfg(any(test, feature = "test-support"))]
    pub fn full_decode_count_for_test() -> usize {
        crate::emacs_core::bytecode::decode::full_decode_count_for_test()
    }

    /// FENCE: whether this build decodes GNU bytecode eagerly.
    pub fn eager_gnu_bytecode_for_test() -> bool {
        crate::emacs_core::bytecode::chunk::eager_gnu_bytecode()
    }

    /// FENCE: lazy GNU decode counter. The counter stays pub(crate).
    #[cfg(any(test, feature = "test-support"))]
    pub fn lazy_gnu_decode_count_for_test() -> usize {
        crate::emacs_core::bytecode::chunk::lazy_gnu_decode_count_for_test()
    }

    /// FENCE: the make-byte-code validate-only override. The setter stays
    /// pub(crate).
    #[cfg(any(test, feature = "test-support"))]
    pub fn force_make_byte_code_validate_only_for_test(on: bool) {
        crate::emacs_core::builtins::symbols::force_make_byte_code_validate_only_for_test(on);
    }

    // Signed knob tokens for re-homed compiler tests. Each converts to the
    // pub(crate) original inside this module. Eval's own tests keep calling
    // the originals. No `TestOptAdmit::ALL`: callers use `TestOptAdmit::parts`. Fields stay pub(crate).

    #[cfg(feature = "jit")]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum TestDirectSitesMode {
        All,
        Unbounded,
        SelfOnly,
    }

    #[cfg(feature = "jit")]
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
    pub struct TestDirectShapes {
        pub(crate) optional: bool,
        pub(crate) rest: bool,
        pub(crate) constant: bool,
        pub(crate) framed: bool,
    }

    #[cfg(feature = "jit")]
    impl TestDirectShapes {
        pub const OFF: Self = Self {
            optional: false,
            rest: false,
            constant: false,
            framed: false,
        };
        pub const ALL: Self = Self {
            optional: true,
            rest: true,
            constant: true,
            framed: true,
        };
    }

    #[cfg(feature = "jit")]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub enum TestDirectSelfHeat {
        #[default]
        Off,
        Seen,
        Hot,
    }

    #[cfg(feature = "jit")]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub enum TestInline2Mode {
        #[default]
        Off,
        Named,
        Closure,
        Hof,
        All,
    }

    #[cfg(feature = "jit")]
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
    pub struct TestInlineVars {
        pub(crate) read: bool,
        pub(crate) set: bool,
        pub(crate) bind: bool,
    }

    #[cfg(feature = "jit")]
    impl TestInlineVars {
        pub const OFF: Self = Self {
            read: false,
            set: false,
            bind: false,
        };
        pub const ALL: Self = Self {
            read: true,
            set: true,
            bind: true,
        };
        pub const DEFAULT: Self = Self {
            read: true,
            set: false,
            bind: false,
        };

        /// Fields stay pub(crate). Cross-crate construction uses this Copy constructor.
        pub fn parts(read: bool, set: bool, bind: bool) -> Self {
            Self { read, set, bind }
        }
    }

    #[cfg(feature = "jit")]
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
    pub struct TestMirReach {
        pub(crate) dead: bool,
    }

    #[cfg(feature = "jit")]
    impl TestMirReach {
        pub const OFF: Self = Self { dead: false };
        pub const ALL: Self = Self { dead: true };
    }

    #[cfg(feature = "jit")]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub enum TestOptMode {
        #[default]
        Legacy,
        Off,
        Opt,
    }

    /// Full literal only. There is no `ALL` constant.
    #[cfg(feature = "jit")]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct TestOptAdmit {
        pub(crate) args: bool,
        pub(crate) env: bool,
        pub(crate) vars: bool,
        pub(crate) binds: bool,
        pub(crate) switch: bool,
        pub(crate) handlers: bool,
    }

    #[cfg(feature = "jit")]
    impl TestOptAdmit {
        /// Fields stay pub(crate). Cross-crate construction uses this Copy constructor.
        pub fn parts(
            args: bool,
            env: bool,
            vars: bool,
            binds: bool,
            switch: bool,
            handlers: bool,
        ) -> Self {
            Self {
                args,
                env,
                vars,
                binds,
                switch,
                handlers,
            }
        }
    }

    #[cfg(feature = "jit")]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum TestOptPasses {
        All,
        FoldBoolRepsReads,
        AllWithPrimitiveOverride { sink: bool },
    }

    #[cfg(feature = "jit")]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum TestRegallocPolicy {
        Auto,
        Full,
    }

    #[cfg(feature = "jit")]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum TestLeafAbi {
        Memory,
        Register { arity: u8 },
    }

    #[cfg(feature = "jit")]
    fn to_sites(mode: TestDirectSitesMode) -> crate::emacs_core::jit::compile::DirectSitesMode {
        use crate::emacs_core::jit::compile::DirectSitesMode;
        match mode {
            TestDirectSitesMode::All => DirectSitesMode::All,
            TestDirectSitesMode::Unbounded => DirectSitesMode::Unbounded,
            TestDirectSitesMode::SelfOnly => DirectSitesMode::SelfOnly,
        }
    }

    #[cfg(feature = "jit")]
    fn to_shapes(knob: TestDirectShapes) -> crate::emacs_core::jit::compile::DirectShapesKnob {
        crate::emacs_core::jit::compile::DirectShapesKnob {
            optional: knob.optional,
            rest: knob.rest,
            constant: knob.constant,
            framed: knob.framed,
        }
    }

    #[cfg(feature = "jit")]
    fn from_shapes(knob: crate::emacs_core::jit::compile::DirectShapesKnob) -> TestDirectShapes {
        TestDirectShapes {
            optional: knob.optional,
            rest: knob.rest,
            constant: knob.constant,
            framed: knob.framed,
        }
    }

    #[cfg(feature = "jit")]
    fn to_heat(
        mode: TestDirectSelfHeat,
    ) -> crate::emacs_core::jit::compile::direct_call::DirectSelfHeat {
        use crate::emacs_core::jit::compile::direct_call::DirectSelfHeat;
        match mode {
            TestDirectSelfHeat::Off => DirectSelfHeat::Off,
            TestDirectSelfHeat::Seen => DirectSelfHeat::Seen,
            TestDirectSelfHeat::Hot => DirectSelfHeat::Hot,
        }
    }

    #[cfg(feature = "jit")]
    fn to_inline2(mode: TestInline2Mode) -> crate::emacs_core::jit::compile::Inline2Mode {
        use crate::emacs_core::jit::compile::Inline2Mode;
        match mode {
            TestInline2Mode::Off => Inline2Mode::Off,
            TestInline2Mode::Named => Inline2Mode::Named,
            TestInline2Mode::Closure => Inline2Mode::Closure,
            TestInline2Mode::Hof => Inline2Mode::Hof,
            TestInline2Mode::All => Inline2Mode::All,
        }
    }

    #[cfg(feature = "jit")]
    fn to_inline_vars(knob: TestInlineVars) -> crate::emacs_core::jit::compile::InlineVarsKnob {
        crate::emacs_core::jit::compile::InlineVarsKnob {
            read: knob.read,
            set: knob.set,
            bind: knob.bind,
        }
    }

    #[cfg(feature = "jit")]
    fn from_inline_vars(knob: crate::emacs_core::jit::compile::InlineVarsKnob) -> TestInlineVars {
        TestInlineVars {
            read: knob.read,
            set: knob.set,
            bind: knob.bind,
        }
    }

    #[cfg(feature = "jit")]
    fn to_reach(reach: TestMirReach) -> crate::emacs_core::jit::compile::MirReach {
        crate::emacs_core::jit::compile::MirReach { dead: reach.dead }
    }

    #[cfg(feature = "jit")]
    fn from_reach(reach: crate::emacs_core::jit::compile::MirReach) -> TestMirReach {
        TestMirReach { dead: reach.dead }
    }

    #[cfg(feature = "jit")]
    fn to_opt_mode(mode: TestOptMode) -> crate::emacs_core::jit::compile::OptMode {
        use crate::emacs_core::jit::compile::OptMode;
        match mode {
            TestOptMode::Legacy => OptMode::Legacy,
            TestOptMode::Off => OptMode::Off,
            TestOptMode::Opt => OptMode::Opt,
        }
    }

    #[cfg(feature = "jit")]
    fn to_admit(admit: TestOptAdmit) -> crate::emacs_core::jit::compile::OptAdmit {
        crate::emacs_core::jit::compile::OptAdmit {
            args: admit.args,
            env: admit.env,
            vars: admit.vars,
            binds: admit.binds,
            switch: admit.switch,
            handlers: admit.handlers,
        }
    }

    #[cfg(feature = "jit")]
    fn to_passes(passes: TestOptPasses) -> crate::emacs_core::jit::compile::OptPasses {
        use crate::emacs_core::jit::compile::OptPasses;
        match passes {
            TestOptPasses::All => OptPasses::ALL,
            TestOptPasses::FoldBoolRepsReads => OptPasses::parse(Some("fold,bool,reps,reads")),
            TestOptPasses::AllWithPrimitiveOverride { sink } => OptPasses {
                sink,
                ..OptPasses::ALL
            },
        }
    }

    #[cfg(feature = "jit")]
    fn to_policy(
        policy: TestRegallocPolicy,
    ) -> crate::emacs_core::jit::compile::lowering::RegallocPolicy {
        use crate::emacs_core::jit::compile::lowering::RegallocPolicy;
        match policy {
            TestRegallocPolicy::Auto => RegallocPolicy::Auto,
            TestRegallocPolicy::Full => RegallocPolicy::Full,
        }
    }

    #[cfg(feature = "jit")]
    fn from_abi(abi: crate::emacs_core::jit::compile::reg_abi::LeafAbi) -> TestLeafAbi {
        use crate::emacs_core::jit::compile::reg_abi::LeafAbi;
        match abi {
            LeafAbi::Memory => TestLeafAbi::Memory,
            LeafAbi::Register { arity } => TestLeafAbi::Register { arity },
        }
    }

    #[cfg(feature = "jit")]
    pub fn force_direct_sites_token_for_test(mode: Option<TestDirectSitesMode>) {
        force_direct_sites_for_test(mode.map(to_sites));
    }

    #[cfg(feature = "jit")]
    pub fn force_direct_shapes_token_for_test(knob: Option<TestDirectShapes>) {
        force_direct_shapes_for_test(knob.map(to_shapes));
    }

    #[cfg(feature = "jit")]
    pub fn direct_shapes_parse_for_test(value: Option<&str>) -> TestDirectShapes {
        from_shapes(crate::emacs_core::jit::compile::DirectShapesKnob::parse(
            value,
        ))
    }

    #[cfg(feature = "jit")]
    pub fn force_direct_self_heat_token_for_test(mode: Option<TestDirectSelfHeat>) {
        force_direct_self_heat_for_test(mode.map(to_heat));
    }

    #[cfg(feature = "jit")]
    pub fn force_inline2_token_for_test(mode: Option<TestInline2Mode>) {
        force_inline2_for_test(mode.map(to_inline2));
    }

    #[cfg(feature = "jit")]
    pub fn force_inline_vars_token_for_test(knob: Option<TestInlineVars>) {
        force_inline_vars_for_test(knob.map(to_inline_vars));
    }

    #[cfg(feature = "jit")]
    pub fn inline_vars_parse_for_test(value: Option<&str>) -> TestInlineVars {
        from_inline_vars(crate::emacs_core::jit::compile::InlineVarsKnob::parse(
            value,
        ))
    }

    #[cfg(feature = "jit")]
    pub fn force_mir_reach_token_for_test(reach: Option<TestMirReach>) {
        force_mir_reach_for_test(reach.map(to_reach));
    }

    #[cfg(feature = "jit")]
    pub fn mir_reach_parse_for_test(value: Option<&str>) -> TestMirReach {
        from_reach(crate::emacs_core::jit::compile::MirReach::parse(value))
    }

    #[cfg(feature = "jit")]
    pub fn force_opt_token_for_test(mode: Option<TestOptMode>, admit: Option<TestOptAdmit>) {
        crate::emacs_core::jit::compile::force_opt_for_test(
            mode.map(to_opt_mode),
            admit.map(to_admit),
        );
    }

    #[cfg(feature = "jit")]
    #[must_use = "the backend override ends when the returned guard is dropped"]
    pub fn opt_mode_token_scope_for_test(mode: TestOptMode) -> impl Drop {
        opt_mode_scope_for_test(to_opt_mode(mode))
    }

    #[cfg(feature = "jit")]
    pub fn force_opt_passes_token_for_test(passes: Option<TestOptPasses>) {
        force_opt_passes_for_test(passes.map(to_passes));
    }

    #[cfg(feature = "jit")]
    pub fn compile_with_regalloc_for_test(
        function: &crate::emacs_core::bytecode::ByteCodeFunction,
        obarray: Option<&neovm_value::symbol::Obarray>,
        policy: TestRegallocPolicy,
    ) -> Result<
        crate::emacs_core::jit::compile::CompiledLeaf,
        crate::emacs_core::jit::compile::CompileError,
    > {
        crate::emacs_core::jit::compile::compile_bytecode_function_tiered(
            function,
            obarray,
            to_policy(policy),
        )
    }

    #[cfg(feature = "jit")]
    pub fn leaf_abi_for_test(leaf: &crate::emacs_core::jit::compile::CompiledLeaf) -> TestLeafAbi {
        from_abi(leaf.abi)
    }

    /// The cache retains the leaf. Only the ABI word is copied out; the
    /// pointer stays inside eval.
    #[cfg(feature = "jit")]
    pub fn cached_leaf_abi_for_test(id: u64) -> Option<TestLeafAbi> {
        let ptr = crate::emacs_core::jit::cache::compiled_leaf_ptr_for_test(id)?;
        // SAFETY: the calling test retains this cache entry and does not clear
        // it across the read. The copy is a two-word ABI discriminant.
        Some(from_abi(unsafe { (*ptr).abi }))
    }

    #[cfg(feature = "jit")]
    pub fn leaf_chains_empty_for_test(
        leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
    ) -> bool {
        leaf.chains.is_empty()
    }

    #[cfg(feature = "jit")]
    pub fn leaf_abi_for_build_for_test(
        aot: bool,
        osr: bool,
        arity: usize,
        frameless: bool,
        dynamic_prefix: usize,
        self_site: bool,
    ) -> TestLeafAbi {
        from_abi(
            crate::emacs_core::jit::compile::reg_abi::LeafAbi::for_build(
                aot,
                osr,
                arity,
                frameless,
                dynamic_prefix,
                self_site,
            ),
        )
    }

    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn direct_sites_emitted_for_test() -> usize {
        crate::emacs_core::jit::compile::direct_call::direct_sites_emitted_for_test()
    }

    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn rootwin_counters_for_test() -> (u32, u32) {
        crate::emacs_core::jit::compile::lowering::rootwin_counters()
    }

    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn tail_calls_unrooted_for_test() -> usize {
        crate::emacs_core::jit::compile::lowering::TAIL_CALLS_UNROOTED.with(std::cell::Cell::get)
    }

    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn stack_guards_emitted_for_test() -> usize {
        crate::emacs_core::jit::compile::stack_guard::stack_guards_emitted_for_test()
    }

    /// FENCE: bytecode-frame count. `bc_frames` stays pub(crate).
    pub fn bc_frames_len_for_test(ctx: &Context) -> usize {
        ctx.bc_frames.len()
    }

    /// FENCE: JIT bind-stack depth. `jit_bind_stack` stays pub(crate).
    pub fn jit_bind_stack_len_for_test(ctx: &Context) -> usize {
        ctx.jit_bind_stack.len()
    }

    /// FENCE: the three observability booleans. `ObserveOverride` stays
    /// pub(crate); this is not a new mode type.
    #[cfg(feature = "jit")]
    pub fn force_observe_knobs_for_test(stats: bool, naming: bool, entry_count: bool) {
        crate::emacs_core::jit::stats::force_observe_for_test(
            crate::emacs_core::jit::stats::ObserveOverride {
                stats,
                naming,
                entry_count,
            },
        );
    }

    /// FENCE: install named byte-compiled functions without running them.
    /// `prepare_entry_for_test` stays pub(crate).
    #[cfg(feature = "jit")]
    pub fn prepare_named_for_test(ctx: &mut Context, names: &[&str]) {
        use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
        for name in names {
            let value = ctx
                .obarray()
                .symbol_function_id(neovm_value::intern::intern(name))
                .expect("fixture function defined");
            let function = value
                .get_bytecode_data()
                .expect("fixture function bytecompiled");
            crate::emacs_core::jit::cache::prepare_entry_for_test(ctx, function);
        }
    }

    /// FENCE: run `body` on a fresh stack segment and restore the caller's
    /// limit. `grow_tracking_jit_limit` stays pub(crate).
    pub fn grow_tracking_jit_limit_for_test<R>(
        ctx: &mut Context,
        segment: usize,
        body: impl FnOnce(&mut Context) -> R,
    ) -> R {
        crate::emacs_core::eval::native_stack::grow_tracking_jit_limit(
            ctx,
            Context::jit_stack_limit_mut,
            segment,
            body,
        )
    }

    /// FENCE: the red-zone constant, as a number. The const stays pub(crate).
    pub fn jit_stack_red_zone_for_test() -> usize {
        crate::emacs_core::eval::native_stack::JIT_STACK_RED_ZONE
    }

    /// FENCE: this thread's stack-limit address. The helper stays pub(crate).
    pub fn jit_stack_limit_here_for_test() -> usize {
        crate::emacs_core::eval::native_stack::jit_stack_limit_here()
    }

    /// FENCE: address identity of a compiled leaf. The pointer stays here.
    #[cfg(feature = "jit")]
    pub fn leaf_identity_for_test(
        leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
    ) -> LeafIdentity {
        LeafIdentity::from_address(
            leaf as *const crate::emacs_core::jit::compile::CompiledLeaf as usize,
        )
    }

    /// FENCE: address identity of a leaf's raw entry word.
    #[cfg(feature = "jit")]
    pub fn leaf_entry_identity_for_test(
        leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
    ) -> LeafIdentity {
        LeafIdentity::from_address(leaf.entry as usize)
    }

    /// FENCE: the identity of a null entry or leaf pointer.
    #[cfg(feature = "jit")]
    pub fn null_identity_for_test() -> LeafIdentity {
        LeafIdentity::from_address(0)
    }

    /// FENCE: index in the bytecode spec-slot list of the slot whose leaf
    /// pointer is `callee`. `SpecSlot` stays pub(crate).
    #[cfg(feature = "jit")]
    pub fn bytecode_slot_index_for_callee_for_test(
        caller: &crate::emacs_core::jit::compile::CompiledLeaf,
        callee: &crate::emacs_core::jit::compile::CompiledLeaf,
    ) -> Option<usize> {
        caller
            .bytecode_spec_slots()
            .position(|slot| std::ptr::eq(slot.leaf_ptr(), callee))
    }

    /// FENCE: direct-entry identity of bytecode spec slot `index`.
    #[cfg(feature = "jit")]
    pub fn bytecode_slot_direct_entry_identity_for_test(
        caller: &crate::emacs_core::jit::compile::CompiledLeaf,
        index: usize,
    ) -> LeafIdentity {
        let slot = caller
            .bytecode_spec_slots()
            .nth(index)
            .expect("bytecode spec slot");
        LeafIdentity::from_address(slot.direct_entry_ptr_for_test() as usize)
    }

    /// FENCE: leaf-pointer identity of bytecode spec slot `index`.
    #[cfg(feature = "jit")]
    pub fn bytecode_slot_leaf_identity_for_test(
        caller: &crate::emacs_core::jit::compile::CompiledLeaf,
        index: usize,
    ) -> LeafIdentity {
        let slot = caller
            .bytecode_spec_slots()
            .nth(index)
            .expect("bytecode spec slot");
        LeafIdentity::from_address(slot.leaf_ptr() as usize)
    }

    /// FENCE: entry identity of the leaf a bytecode spec slot points at.
    /// A null leaf pointer is the null identity. The pointer is not returned.
    #[cfg(feature = "jit")]
    pub fn bytecode_slot_pointed_entry_identity_for_test(
        caller: &crate::emacs_core::jit::compile::CompiledLeaf,
        index: usize,
    ) -> LeafIdentity {
        let slot = caller
            .bytecode_spec_slots()
            .nth(index)
            .expect("bytecode spec slot");
        let ptr = slot.leaf_ptr();
        if ptr.is_null() {
            return LeafIdentity::from_address(0);
        }
        // SAFETY: a non-null spec-slot leaf word addresses a cache-retained
        // CompiledLeaf. The calling test keeps that cache entry alive across
        // this read. Only the entry address is copied out.
        LeafIdentity::from_address(unsafe { (*ptr).entry } as usize)
    }

    /// FENCE: every armed bytecode slot's direct entry equals the entry of
    /// the leaf it points at, and that direct entry is non-null.
    #[cfg(feature = "jit")]
    pub fn bytecode_slots_direct_entries_match_pointed_leaves_for_test(
        caller: &crate::emacs_core::jit::compile::CompiledLeaf,
    ) -> bool {
        caller.bytecode_spec_slots().all(|slot| {
            let direct = slot.direct_entry_ptr_for_test();
            // SAFETY: the predicate only runs when the direct entry is
            // non-null, and an armed slot's leaf word then addresses a
            // cache-retained CompiledLeaf. The calling test retains it.
            !direct.is_null() && unsafe { (*slot.leaf_ptr()).entry } == direct
        })
    }

    /// The cache retains the leaf. The closure sees it by reference; the
    /// pointer is not returned.
    #[cfg(feature = "jit")]
    pub fn with_cached_leaf_for_test<R>(
        id: u64,
        body: impl FnOnce(&crate::emacs_core::jit::compile::CompiledLeaf) -> R,
    ) -> Option<R> {
        let ptr = crate::emacs_core::jit::cache::compiled_leaf_ptr_for_test(id)?;
        // SAFETY: the calling test retains this cache entry and does not clear
        // it across `body`. The reference does not outlive the closure.
        Some(body(unsafe { &*ptr }))
    }

    /// FENCE: resume a precise deopt on the Tier-0 interpreter.
    /// `resume_deopt` stays pub(crate).
    #[cfg(feature = "jit")]
    pub fn resume_deopt_for_test(
        ctx: &mut Context,
        func: &crate::emacs_core::bytecode::ByteCodeFunction,
        func_value: neovm_value::value::Value,
        leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
        resume: crate::emacs_core::jit::compile::DeoptResume,
    ) -> Result<neovm_value::value::Value, neovm_value::error::Flow> {
        crate::emacs_core::jit::compile::resumed_chain::resume_deopt(
            ctx, func, func_value, leaf, resume,
        )
    }

    /// FENCE: whether this mutex Value is owned by the current thread.
    /// `MutexHandle` stays pub(crate).
    pub fn mutex_owned_by_current_for_test(
        ctx: &Context,
        mutex: &neovm_value::value::Value,
    ) -> bool {
        let handle = ctx
            .threads()
            .mutex_id_from_handle(mutex)
            .expect("same-World mutex argument");
        ctx.threads().mutex_owned_by_current_thread(handle)
    }

    /// FENCE: var-shim census. The counters stay pub(crate). Order is
    /// varref, varset, varbind, unbind.
    #[cfg(feature = "jit")]
    pub fn var_shim_calls_for_test() -> (usize, usize, usize, usize) {
        crate::emacs_core::jit::compile::var_shim_calls()
    }

    /// FENCE: forget the inline-var site census. The counter stays pub(crate).
    #[cfg(feature = "jit")]
    pub fn reset_inline_var_sites_for_test() {
        crate::emacs_core::jit::compile::inline_vars::reset_inline_var_sites();
    }

    /// FENCE: inline varref sites since the last reset. No new op type.
    #[cfg(feature = "jit")]
    pub fn inline_var_read_sites_for_test() -> u32 {
        crate::emacs_core::jit::compile::inline_vars::inline_var_sites(
            crate::emacs_core::jit::compile::inline_vars::InlineVarOp::Read,
        )
    }

    /// FENCE: inline varset sites since the last reset.
    #[cfg(feature = "jit")]
    pub fn inline_var_set_sites_for_test() -> u32 {
        crate::emacs_core::jit::compile::inline_vars::inline_var_sites(
            crate::emacs_core::jit::compile::inline_vars::InlineVarOp::Set,
        )
    }

    /// FENCE: inline varbind sites since the last reset.
    #[cfg(feature = "jit")]
    pub fn inline_var_bind_sites_for_test() -> u32 {
        crate::emacs_core::jit::compile::inline_vars::inline_var_sites(
            crate::emacs_core::jit::compile::inline_vars::InlineVarOp::Bind,
        )
    }

    /// FENCE: inline unbind sites since the last reset.
    #[cfg(feature = "jit")]
    pub fn inline_var_unbind_sites_for_test() -> u32 {
        crate::emacs_core::jit::compile::inline_vars::inline_var_sites(
            crate::emacs_core::jit::compile::inline_vars::InlineVarOp::Unbind,
        )
    }

    /// FENCE: enter the compile environment for a direct `lower_leaf`.
    /// `with_compile_env_for_test` stays pub(crate).
    #[cfg(feature = "jit")]
    pub fn with_compile_env_for_test<R>(ctx: &Context, body: impl FnOnce() -> R) -> R {
        crate::emacs_core::jit::compile::inline_vars::with_compile_env_for_test(ctx, body)
    }
    /// FENCE: optional count of a stack parameter shape.
    /// `StackParamShapeExt` stays pub(crate).
    #[cfg(any(test, feature = "test-support"))]
    pub fn stack_param_optional_for_test(
        shape: crate::emacs_core::bytecode::StackParamShape,
    ) -> Option<usize> {
        use crate::emacs_core::bytecode::function_slots::StackParamShapeExt as _;
        shape.optional()
    }

    /// FENCE: the formal bindings of a dynamic arglist, in GNU walk order.
    /// `FormalCursor` stays pub(crate). The walk finishes each binding and
    /// stops at the cursor's end, so the returned list is the whole arglist.
    #[cfg(any(test, feature = "test-support"))]
    pub fn dynamic_formal_bindings_for_test(
        arglist: crate::emacs_core::bytecode::DynamicArglist,
    ) -> Vec<neovm_value::tagged::bytecode::function_slots::FormalBinding> {
        use crate::emacs_core::bytecode::function_slots::DynamicArglistExt as _;
        let mut cursor = arglist.invocation();
        let mut out = Vec::new();
        loop {
            match cursor.next_binding().expect("formal walk") {
                Some(binding) => {
                    cursor.finish_binding().expect("finish binding");
                    out.push(binding);
                }
                None => return out,
            }
        }
    }

    /// FENCE: JIT bind-stack reserve. The vec stays pub(crate).
    pub fn jit_bind_stack_reserve_for_test(ctx: &mut Context, additional: usize) {
        ctx.jit_bind_stack.reserve(additional);
    }

    /// FENCE: clear the JIT bind stack. The vec stays pub(crate).
    pub fn clear_jit_bind_stack_for_test(ctx: &mut Context) {
        ctx.jit_bind_stack.clear();
    }

    /// FENCE: whether the JIT bind stack is empty.
    pub fn jit_bind_stack_is_empty_for_test(ctx: &Context) -> bool {
        ctx.jit_bind_stack.is_empty()
    }

    /// FENCE: JIT root-stack top. The field stays pub(crate).
    pub fn jit_root_stack_top_for_test(ctx: &Context) -> usize {
        ctx.jit_root_stack_top
    }

    /// FENCE: whether a body may re-enter Lisp. The predicate stays pub(crate).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn body_may_reenter_lisp_for_test(ops: &[crate::emacs_core::bytecode::Op]) -> bool {
        crate::emacs_core::jit::compile::stack_guard::body_may_reenter_lisp(ops)
    }

    /// FENCE: current opt-mode token. `OptMode` stays pub(crate).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn jit_opt_mode_token_for_test() -> TestOptMode {
        match crate::emacs_core::jit::compile::jit_opt_mode() {
            crate::emacs_core::jit::compile::OptMode::Legacy => TestOptMode::Legacy,
            crate::emacs_core::jit::compile::OptMode::Off => TestOptMode::Off,
            crate::emacs_core::jit::compile::OptMode::Opt => TestOptMode::Opt,
        }
    }

    /// Widening-hub token (#1871/#1877(1)): the flonum compile mode.
    #[cfg(feature = "jit")]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub enum TestFlonumMode {
        #[default]
        Off,
        OpLocal,
        Resident,
    }

    #[cfg(feature = "jit")]
    fn to_flonum(mode: TestFlonumMode) -> crate::emacs_core::jit::compile::FlonumMode {
        use crate::emacs_core::jit::compile::FlonumMode;
        match mode {
            TestFlonumMode::Off => FlonumMode::Off,
            TestFlonumMode::OpLocal => FlonumMode::OpLocal,
            TestFlonumMode::Resident => FlonumMode::Resident,
        }
    }

    /// FENCE: force the flonum compile mode for this thread; the private
    /// `FlonumMode` stays crate-private.
    #[cfg(feature = "jit")]
    pub fn force_flonum_mode_token_for_test(mode: Option<TestFlonumMode>) {
        crate::emacs_core::jit::compile::force_flonum_mode_for_test(mode.map(to_flonum));
    }

    /// Widening-hub token (#1871/#1877(1)): the profit-gate test policy.
    #[cfg(feature = "jit")]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub enum TestProfitPolicy {
        #[default]
        Runtime,
        Enforce,
        Bypass,
    }

    #[cfg(feature = "jit")]
    fn to_profit_policy(
        policy: TestProfitPolicy,
    ) -> crate::emacs_core::jit::compile::ProfitGateTestPolicy {
        use crate::emacs_core::jit::compile::ProfitGateTestPolicy;
        match policy {
            TestProfitPolicy::Runtime => ProfitGateTestPolicy::Runtime,
            TestProfitPolicy::Enforce => ProfitGateTestPolicy::Enforce,
            TestProfitPolicy::Bypass => ProfitGateTestPolicy::Bypass,
        }
    }

    #[cfg(feature = "jit")]
    fn from_profit_policy(
        policy: crate::emacs_core::jit::compile::ProfitGateTestPolicy,
    ) -> TestProfitPolicy {
        use crate::emacs_core::jit::compile::ProfitGateTestPolicy;
        match policy {
            ProfitGateTestPolicy::Runtime => TestProfitPolicy::Runtime,
            ProfitGateTestPolicy::Enforce => TestProfitPolicy::Enforce,
            ProfitGateTestPolicy::Bypass => TestProfitPolicy::Bypass,
        }
    }

    /// FENCE: replace the profit-gate test policy and return the previous
    /// one, the caller's restore token. The private enum stays crate-private.
    #[cfg(feature = "jit")]
    #[must_use = "the previous policy is the caller's restore token"]
    pub fn replace_profit_gate_token_for_test(policy: TestProfitPolicy) -> TestProfitPolicy {
        from_profit_policy(
            crate::emacs_core::jit::compile::replace_profit_gate_for_test(to_profit_policy(policy)),
        )
    }

    #[cfg(feature = "jit")]
    fn from_inline2(mode: crate::emacs_core::jit::compile::Inline2Mode) -> TestInline2Mode {
        use crate::emacs_core::jit::compile::Inline2Mode;
        match mode {
            Inline2Mode::Off => TestInline2Mode::Off,
            Inline2Mode::Named => TestInline2Mode::Named,
            Inline2Mode::Closure => TestInline2Mode::Closure,
            Inline2Mode::Hof => TestInline2Mode::Hof,
            Inline2Mode::All => TestInline2Mode::All,
        }
    }

    /// FENCE: the current inline-v2 mode as the signed token. `Inline2Mode`
    /// stays crate-private.
    #[cfg(feature = "jit")]
    pub fn jit_inline2_mode_token_for_test() -> TestInline2Mode {
        from_inline2(crate::emacs_core::jit::compile::jit_inline2_mode())
    }

    /// FENCE: force the interpreter-only tier policy on a borrowed VM. The
    /// VM is exclusively borrowed for the call; the policy field never
    /// crosses the fence.
    #[cfg(feature = "jit")]
    pub fn force_interpreter_only_for_test(vm: &mut crate::emacs_core::bytecode::Vm) {
        vm.force_interpreter_only_for_test();
    }

    /// Opaque handle holding the numeric-feedback TLS scope alive for the
    /// test's lifetime. The scope type stays crate-private and no field is
    /// reachable.
    #[cfg(feature = "jit")]
    pub struct NumericFeedbackScopeHandle(
        #[allow(dead_code)] crate::emacs_core::jit::compile::NumericFeedbackScope,
    );

    /// FENCE: publish the numeric feedback `f` observed for the handle's
    /// lifetime; the TLS scope type never crosses the fence.
    #[cfg(feature = "jit")]
    pub fn publish_numeric_feedback_scope_for_test(
        f: &crate::emacs_core::bytecode::ByteCodeFunction,
    ) -> NumericFeedbackScopeHandle {
        NumericFeedbackScopeHandle(crate::emacs_core::jit::compile::publish_numeric_feedback(f))
    }

    /// FENCE: prepare the cache entry for `f`. The internal entry point takes
    /// a raw Context pointer and uses it only for the duration of the call;
    /// the caller's exclusive borrow keeps it valid.
    #[cfg(feature = "jit")]
    pub fn prepare_entry_for_test(
        ctx: &mut crate::emacs_core::eval::Context,
        f: &crate::emacs_core::bytecode::ByteCodeFunction,
    ) {
        crate::emacs_core::jit::cache::prepare_entry_for_test(ctx as *mut _, f);
    }

    /// FENCE: cold-exit counter. The atomic stays pub(crate).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn direct_cold_exits_for_test() -> u64 {
        crate::emacs_core::jit::compile::direct_call::DIRECT_COLD_EXITS
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// FENCE: framed-call counter. The atomic stays pub(crate).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn direct_framed_calls_for_test() -> u64 {
        crate::emacs_core::jit::compile::direct_call::DIRECT_FRAMED_CALLS
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// FENCE: whether a shim panic is still pending.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn shim_panic_pending_for_test() -> bool {
        crate::emacs_core::jit::compile::shim_panic_pending()
    }

    /// FENCE: the framed direct trampoline. The extern stays pub(crate).
    /// The bits are the test's existing call words. No pointer is returned.
    ///
    /// # Safety
    /// The same contract as `neovm_jit_direct_framed`: `ctx` is the dormant
    /// Context, `leaf_bits` names the live entered leaf, `args_ptr` addresses
    /// `nargs` live call words, and `out` is a writable slot.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub unsafe fn direct_framed_call_for_test(
        ctx: *mut u8,
        callee_bits: i64,
        leaf_bits: i64,
        const_base: i64,
        args_ptr: *const i64,
        nargs: i64,
        bt_count: i64,
        out: *mut i64,
    ) -> i64 {
        crate::emacs_core::jit::compile::direct_call::neovm_jit_direct_framed(
            ctx,
            callee_bits,
            leaf_bits,
            const_base,
            args_ptr,
            nargs,
            bt_count,
            out,
        )
    }

    /// FENCE: `direct_consts` register-ABI flag of bytecode spec slot `index`.
    /// The key constant stays pub(crate). The returned word is the masked flag.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn bytecode_slot_register_key_for_test(
        caller: &crate::emacs_core::jit::compile::CompiledLeaf,
        index: usize,
    ) -> u64 {
        use crate::emacs_core::jit::compile::spec_slot::SpecSlot;
        let slot = caller
            .bytecode_spec_slots()
            .nth(index)
            .expect("bytecode spec slot");
        slot.direct_consts_for_test() & SpecSlot::KEY_REGISTER
    }

    /// FENCE: drop the cached leaf of bytecode spec slot `index`.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn bytecode_slot_clear_leaf_for_test(
        caller: &crate::emacs_core::jit::compile::CompiledLeaf,
        index: usize,
    ) {
        caller
            .bytecode_spec_slots()
            .nth(index)
            .expect("bytecode spec slot")
            .clear_leaf();
    }

    /// FENCE: whether bytecode spec slot `index` holds the framed dispatch tag.
    /// `DirectEntryTag` stays pub(crate).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn bytecode_slot_direct_entry_is_framed_tag_for_test(
        caller: &crate::emacs_core::jit::compile::CompiledLeaf,
        index: usize,
    ) -> bool {
        use crate::emacs_core::jit::compile::spec_slot::DirectEntryTag;
        let slot = caller
            .bytecode_spec_slots()
            .nth(index)
            .expect("bytecode spec slot");
        slot.direct_entry_ptr_for_test() as usize == DirectEntryTag::Framed as usize
    }

    /// FENCE: entry-shape discriminant. `EntryShape` stays pub(crate).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn leaf_entry_shape_name_for_test(
        leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
    ) -> &'static str {
        use crate::emacs_core::jit::compile::EntryShape;
        match leaf.entry_shape {
            EntryShape::RawMemory => "RawMemory",
            EntryShape::RawRegister => "RawRegister",
            EntryShape::Framed => "Framed",
        }
    }

    /// FENCE: whether the leaf's entry shape is framed.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn leaf_entry_shape_is_framed_for_test(
        leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
    ) -> bool {
        use crate::emacs_core::jit::compile::EntryShape;
        matches!(leaf.entry_shape, EntryShape::Framed)
    }

    /// FENCE: whether the leaf has dynamic bindings. The flag stays pub(crate).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn leaf_has_binds_for_test(leaf: &crate::emacs_core::jit::compile::CompiledLeaf) -> bool {
        leaf.has_binds
    }

    /// FENCE: whether the leaf has condition handlers. The flag stays pub(crate).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn leaf_has_handlers_for_test(
        leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
    ) -> bool {
        leaf.has_handlers
    }

    /// FENCE: whether the leaf's entry guards read Context. The flag stays pub(crate).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn leaf_needs_vmctx_for_test(leaf: &crate::emacs_core::jit::compile::CompiledLeaf) -> bool {
        leaf.needs_vmctx
    }

    /// FENCE: how many bytecode spec slots this leaf has. `SpecSlot` stays pub(crate).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn bytecode_spec_slot_count_for_test(
        leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
    ) -> usize {
        leaf.bytecode_spec_slots().count()
    }

    /// FENCE: every bytecode spec slot's leaf word is null.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn bytecode_spec_slots_all_unarmed_for_test(
        leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
    ) -> bool {
        leaf.bytecode_spec_slots()
            .all(|slot| slot.leaf_ptr().is_null())
    }

    /// FENCE: every bytecode spec slot's fast-path key is armed (nonzero).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn bytecode_spec_slots_all_consts_armed_for_test(
        leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
    ) -> bool {
        leaf.bytecode_spec_slots()
            .all(|slot| slot.direct_consts_for_test() != 0)
    }

    /// FENCE: some source spec slot's direct entry is the entry of the leaf
    /// it points at, and that entry is non-null. No pointer is returned.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn source_slots_any_direct_entry_matches_pointed_leaf_for_test(
        caller: &crate::emacs_core::jit::compile::CompiledLeaf,
    ) -> bool {
        caller.source_spec_slots().any(|slot| {
            let direct = slot.direct_entry_ptr_for_test();
            // SAFETY: the predicate only runs when the direct entry is
            // non-null, and an armed source slot's leaf word then addresses
            // a cache-retained CompiledLeaf. The calling test retains it.
            !direct.is_null() && unsafe { (*slot.leaf_ptr()).entry } == direct
        })
    }

    /// FENCE: clear every bytecode spec slot that caches `leaf`. The pointer
    /// stays inside. Returns how many slots were cleared.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn unlink_spec_slots_for_test(
        leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
    ) -> usize {
        crate::emacs_core::jit::cache::unlink_spec_slots(leaf)
    }

    /// FENCE: nonzero call-census cells, `(site, shape, calls)`.
    /// `CallShape` stays pub(crate). Shape names are `CallShape::name`.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn census_counts_for_test() -> Vec<(&'static str, &'static str, u64)> {
        crate::emacs_core::jit::compile::call_census::census_counts()
    }

    /// FENCE: spec-fast census `(armed, framed)`. The atomics stay pub(crate).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn call_census_spec_fast_counts_for_test() -> (u64, u64) {
        crate::emacs_core::jit::compile::call_census::spec_fast_counts_for_test()
    }

    /// FENCE: record one accepted spec-fast entry. `framed` counts a frame.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn record_spec_fast_for_census_for_test(framed: bool) {
        crate::emacs_core::jit::compile::call_census::record_spec_fast_for_census_for_test(framed);
    }

    /// FENCE: sidecar, then bind, then handler, as census shape names.
    /// Returns `(still_direct_eligible, exact, after_binds, after_handlers)`.
    /// `LeafSidecar` and `CallShape` stay pub(crate). The names are
    /// `CallShape::name` (`exact`, `framed`).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn sidecar_census_shape_names_for_test(
        leaf: &mut crate::emacs_core::jit::compile::CompiledLeaf,
    ) -> (bool, &'static str, &'static str, &'static str) {
        use crate::emacs_core::jit::compile::LeafSidecar;
        use crate::emacs_core::jit::compile::call_census::named_shape_name_for_test;
        use crate::emacs_core::jit::compile::spec_slot::SpecSlot;
        leaf.sidecar = Some(Box::new(LeafSidecar {
            reloc_base: core::ptr::null(),
            spill_base: core::ptr::null(),
            meta_pc: core::ptr::null(),
            meta_depth: core::ptr::null(),
            meta_handlers: core::ptr::null(),
            spec_slot_base: core::ptr::null(),
            spec_expected_base: core::ptr::null(),
        }));
        let eligible = leaf.direct_call_eligible();
        let slot = SpecSlot::at_epoch(0);
        let consts = [neovm_value::value::Value::NIL];
        slot.arm_leaf(leaf, consts.as_ptr(), false, true, false);
        let exact = named_shape_name_for_test(&slot);
        leaf.has_binds = true;
        let binds = named_shape_name_for_test(&slot);
        leaf.has_binds = false;
        leaf.has_handlers = true;
        let handlers = named_shape_name_for_test(&slot);
        let _keep_consts = consts;
        (eligible, exact, binds, handlers)
    }

    /// FENCE: deopt-cause discriminant. `DeoptCause` stays pub(crate).
    /// `None` is a deopt whose cold block stored no reason.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn deopt_cause_name_for_test(
        resume: &crate::emacs_core::jit::compile::DeoptResume,
    ) -> Option<&'static str> {
        use crate::emacs_core::jit::reopt::DeoptCause;
        resume.cause.map(|cause| match cause {
            DeoptCause::ArithOperands(_) => "ArithOperands",
            DeoptCause::ArithOverflow => "ArithOverflow",
            DeoptCause::InlinedCall => "InlinedCall",
            DeoptCause::InlineEpochMoved => "InlineEpochMoved",
            DeoptCause::TypeError => "TypeError",
            DeoptCause::OsrEntry => "OsrEntry",
            DeoptCause::Rerun => "Rerun",
            DeoptCause::Unattributed => "Unattributed",
            DeoptCause::EntryGuard(_) => "EntryGuard",
            DeoptCause::Unreached => "Unreached",
            DeoptCause::InlineIdentity => "InlineIdentity",
            DeoptCause::InlineAttention => "InlineAttention",
            DeoptCause::DepthLimit => "DepthLimit",
            DeoptCause::ColdFlagged => "ColdFlagged",
        })
    }

    /// FENCE: the deopt has no inlined-frame chain.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn deopt_inlined_is_none_for_test(
        resume: &crate::emacs_core::jit::compile::DeoptResume,
    ) -> bool {
        resume.inlined.is_none()
    }

    /// FENCE: how many inlined frames the deopt read back. `None` when the
    /// chain itself is absent. `InlinedResume` stays pub(crate).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn deopt_inlined_frame_count_for_test(
        resume: &crate::emacs_core::jit::compile::DeoptResume,
    ) -> Option<usize> {
        resume.inlined.as_ref().map(|chain| chain.frames.len())
    }

    /// FENCE: pc of inlined frame `index`. The frame type stays pub(crate).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn deopt_inlined_frame_pc_for_test(
        resume: &crate::emacs_core::jit::compile::DeoptResume,
        index: usize,
    ) -> usize {
        resume.inlined.as_ref().expect("inlined chain").frames[index].pc
    }

    /// FENCE: operand stack of inlined frame `index`, copied out.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn deopt_inlined_frame_stack_for_test(
        resume: &crate::emacs_core::jit::compile::DeoptResume,
        index: usize,
    ) -> Vec<neovm_value::value::Value> {
        resume.inlined.as_ref().expect("inlined chain").frames[index]
            .stack
            .clone()
    }

    /// FENCE: OSR transfers taken. The atomic stays pub(crate).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn osr_transfer_count_for_test() -> u64 {
        crate::emacs_core::jit::cache::OSR_TRANSFER_COUNT.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// FENCE: compile and install an OSR entry without transferring.
    /// `prepare_osr_for_test` stays pub(crate).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn prepare_osr_for_test(
        ctx: &mut Context,
        func: &crate::emacs_core::bytecode::ByteCodeFunction,
        pc: usize,
    ) {
        crate::emacs_core::jit::cache::prepare_osr_for_test(ctx, func, pc);
    }

    /// FENCE: transfer into a prepared OSR entry. `try_run_osr` stays
    /// pub(crate).
    ///
    /// # Safety
    /// `ctx` is the live owner `Context` on this thread and stays valid for
    /// the call. The caller holds the dormant-Context contract of
    /// `try_run_osr`.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub unsafe fn try_run_osr_for_test(
        ctx: &mut Context,
        func: &crate::emacs_core::bytecode::ByteCodeFunction,
        osr_pc: usize,
        stack: &[neovm_value::value::Value],
        binds: &[usize],
    ) -> Option<crate::emacs_core::jit::compile::NativeRun> {
        // SAFETY: the caller upholds the dormant-Context contract above.
        unsafe { crate::emacs_core::jit::cache::try_run_osr(ctx, func, osr_pc, stack, binds) }
    }

    /// FENCE: CLIF text of every function `compile` lowers on this thread.
    /// The dump override stays pub(crate).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn captured_clif_for_test(compile: impl FnOnce()) -> Vec<String> {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("leaves.clif");
        crate::emacs_core::jit::compile::lowering::force_clif_dump_for_test(Some(
            path.to_string_lossy().into_owned(),
        ));
        compile();
        crate::emacs_core::jit::compile::lowering::force_clif_dump_for_test(None);
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        text.split("\n;; ")
            .filter(|chunk| !chunk.trim().is_empty())
            .map(str::to_string)
            .collect()
    }

    /// FENCE: hold the self-source compiler token. `SelfSourceScope` stays
    /// pub(crate). The guard restores the previous token on drop.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    #[must_use = "the self-source token ends when the guard drops"]
    pub fn self_source_scope_for_test(
        function: &crate::emacs_core::bytecode::ByteCodeFunction,
        may_call_self: bool,
        call_heavy: bool,
    ) -> impl Drop {
        // `SelfSourceScope` restores its token through the field drop glue.
        // That glue does not implement `Drop`, so the opaque guard does.
        struct Hold(crate::emacs_core::jit::compile::direct_call::SelfSourceScope);
        impl Drop for Hold {
            fn drop(&mut self) {}
        }
        Hold(
            crate::emacs_core::jit::compile::direct_call::SelfSourceScope::enter_for(
                function,
                may_call_self,
                call_heavy,
            ),
        )
    }

    /// FENCE: write the JIT stack-limit word. The field stays pub(crate).
    pub fn set_jit_stack_limit_for_test(ctx: &mut Context, limit: usize) {
        ctx.jit_stack_limit = limit;
    }

    /// FENCE: `(compiled id, entry-counted, native entries)` for every leaf
    /// the report still holds. `LeafRow` stays pub(crate).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn leaf_report_entry_rows_for_test() -> Vec<(u64, bool, u64)> {
        let (rows, _) = crate::emacs_core::jit::cache::leaf_report_rows();
        rows.into_iter()
            .map(|row| (row.id, row.obs.entry_counted, row.obs.entries))
            .collect()
    }

    /// FENCE: how many source spec slots this leaf has. `SpecSlot` stays
    /// private to the compile module. The count is the only word returned.
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn source_spec_slot_count_for_test(
        leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
    ) -> usize {
        leaf.source_spec_slots().count()
    }

    /// FENCE: MIR census `(build_failed, reach_dead, taken, dead_leaders)`.
    /// `CompileStats` stays pub(crate).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn compile_stats_mir_census_for_test() -> (u64, u64, u64, u64) {
        let stats = crate::emacs_core::jit::stats::compile_stats_snapshot();
        (
            stats.mir_build_failed,
            stats.mir_reach_dead,
            stats.mir_taken,
            stats.mir_dead_leaders,
        )
    }

    /// FENCE: owned GNU byte-offset map. The trait that reads it stays
    /// pub(crate). `GnuByteOffsetMapEntry` is the existing public entry.
    #[cfg(any(test, feature = "test-support"))]
    pub fn executable_gnu_byte_offset_map_for_test(
        function: &crate::emacs_core::bytecode::ByteCodeFunction,
    ) -> Option<Vec<crate::emacs_core::bytecode::chunk::GnuByteOffsetMapEntry>> {
        use crate::emacs_core::bytecode::ByteCodeFunctionExt as _;
        function
            .executable_gnu_byte_offset_map()
            .map(|map| map.to_vec())
    }

    /// FENCE: every bytecode spec slot's direct-entry word is null.
    /// `SpecSlot` stays pub(crate).
    #[cfg(all(feature = "jit", any(test, feature = "test-support")))]
    pub fn bytecode_spec_slots_all_direct_entries_null_for_test(
        leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
    ) -> bool {
        leaf.bytecode_spec_slots()
            .all(|slot| slot.direct_entry_ptr_for_test().is_null())
    }

    /// FENCE: shipped alias rows as `(name, source, authority is World)`.
    /// `InventoryEntry` and `AliasTarget` stay pub(crate).
    #[cfg(any(test, feature = "test-support"))]
    pub fn shipped_alias_rows_for_test(
        ctx: &crate::emacs_core::eval::Context,
    ) -> Vec<(&'static str, &'static str, bool)> {
        crate::emacs_core::subr::effect_catalog::shipped_alias_rows_for_test(ctx)
    }

    /// FENCE: the live function cell of `name` resolves to World authority.
    /// `AliasTarget` stays pub(crate).
    #[cfg(any(test, feature = "test-support"))]
    pub fn alias_authority_is_world_for_test(
        ctx: &crate::emacs_core::eval::Context,
        name: &str,
    ) -> bool {
        crate::emacs_core::subr::effect_catalog::alias_authority_is_world_for_test(ctx, name)
    }

    /// FENCE: primitive inventory names. `InventoryEntry` stays pub(crate).
    #[cfg(any(test, feature = "test-support"))]
    pub fn primitive_inventory_names_for_test() -> Vec<&'static str> {
        crate::emacs_core::subr::effect_catalog::primitive_inventory_names_for_test()
    }

    /// FENCE: parse an opt-pass spec (`sink`, `all`, `fold,bool,reps,reads`).
    /// `OptPasses` stays pub(crate). `None` clears the override.
    #[cfg(feature = "jit")]
    pub fn force_opt_passes_spec_for_test(spec: Option<&str>) {
        crate::emacs_core::jit::compile::force_opt_passes_for_test(
            spec.map(|spec| crate::emacs_core::jit::compile::OptPasses::parse(Some(spec))),
        );
    }

    /// FENCE: `(fold, bool_rep, reps, gvn, reads, range, licm, sink)`.
    #[cfg(feature = "jit")]
    pub fn opt_passes_flags_for_test() -> (bool, bool, bool, bool, bool, bool, bool, bool) {
        crate::group1_startup_drivers::opt_passes_flags_for_test()
    }

    /// FENCE: parse a profit token (`loops`, `primitive-loops`, `off`).
    /// `OptProfitMode` stays pub(crate). `None` clears the override.
    #[cfg(feature = "jit")]
    pub fn force_opt_profit_name_for_test(name: Option<&str>) {
        crate::emacs_core::jit::compile::force_opt_profit_for_test(
            name.map(|name| crate::emacs_core::jit::compile::OptProfitMode::parse(Some(name))),
        );
    }

    /// FENCE: the profit variant ident (`PrimitiveLoops`, not the parse token).
    #[cfg(feature = "jit")]
    pub fn opt_profit_name_for_test() -> &'static str {
        crate::group1_startup_drivers::opt_profit_name_for_test()
    }

    /// FENCE: parse an early-mode token. `OptEarlyMode` stays pub(crate).
    /// `None` clears the override.
    #[cfg(feature = "jit")]
    pub fn force_opt_early_name_for_test(name: Option<&str>) {
        crate::emacs_core::jit::compile::force_opt_early_for_test(
            name.map(|name| crate::emacs_core::jit::compile::OptEarlyMode::parse(Some(name))),
        );
    }

    /// FENCE: force the opt op ceiling. `None` clears the override.
    #[cfg(feature = "jit")]
    pub fn force_opt_max_ops_for_test(value: Option<usize>) {
        crate::emacs_core::jit::compile::force_opt_max_ops_for_test(value);
    }

    /// FENCE: the live opt op ceiling.
    #[cfg(feature = "jit")]
    pub fn opt_max_ops_for_test() -> usize {
        crate::emacs_core::jit::compile::jit_opt_max_ops()
    }

    /// FENCE: force the OSR admission prerequisite. `None` clears the override.
    #[cfg(feature = "jit")]
    pub fn force_opt_require_osr_for_test(value: Option<bool>) {
        crate::emacs_core::jit::compile::force_opt_require_osr_for_test(value);
    }

    /// FENCE: the live OSR admission prerequisite.
    #[cfg(feature = "jit")]
    pub fn opt_require_osr_for_test() -> bool {
        crate::emacs_core::jit::compile::jit_opt_require_osr()
    }

    /// FENCE: Plain compile, origin Direct, profit gate bypassed.
    /// `CompileRequest` fields stay pub(crate).
    #[cfg(feature = "jit")]
    pub fn compile_plain_bypassing_profit_for_test(
        function: &crate::emacs_core::bytecode::ByteCodeFunction,
        obarray: Option<&neovm_value::symbol::Obarray>,
        policy: TestRegallocPolicy,
    ) -> Result<
        crate::emacs_core::jit::compile::CompiledLeaf,
        crate::emacs_core::jit::compile::CompileError,
    > {
        use crate::emacs_core::jit::compile::CompileRequest;
        crate::emacs_core::jit::compile::compile_bytecode_function_requested(
            function,
            obarray,
            CompileRequest {
                regalloc: to_policy(policy),
                bypass_profit_gate: true,
                origin: crate::emacs_core::jit::stats::CompileOrigin::Direct,
                tier: crate::emacs_core::jit::tier2::CompileTier::Plain,
            },
        )
    }

    /// FENCE: feedback upgrade, origin Direct, profit gate bypassed, full
    /// regalloc. `CompileTier` stays pub(crate).
    #[cfg(feature = "jit")]
    pub fn compile_feedback_upgrade_for_test(
        function: &crate::emacs_core::bytecode::ByteCodeFunction,
        obarray: Option<&neovm_value::symbol::Obarray>,
        policy: TestRegallocPolicy,
    ) -> Result<
        crate::emacs_core::jit::compile::CompiledLeaf,
        crate::emacs_core::jit::compile::CompileError,
    > {
        use crate::emacs_core::jit::compile::CompileRequest;
        crate::emacs_core::jit::compile::compile_bytecode_function_requested(
            function,
            obarray,
            CompileRequest {
                regalloc: to_policy(policy),
                bypass_profit_gate: true,
                origin: crate::emacs_core::jit::stats::CompileOrigin::Direct,
                tier: crate::emacs_core::jit::tier2::CompileTier::Upgrade(
                    crate::emacs_core::jit::tier2::T2Upgrade::Feedback,
                ),
            },
        )
    }

    /// FENCE: selected-tier name (`baseline`, `mir`, `aot`, `opt`).
    /// `SelectedTier` stays pub(crate).
    #[cfg(feature = "jit")]
    pub fn compiled_leaf_selected_tier_name_for_test(
        leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
    ) -> &'static str {
        crate::group1_startup_drivers::compiled_leaf_selected_tier_name_for_test(leaf)
    }

    /// FENCE: `(cons_sources, cons_reads_elided)` when the sink pass ran.
    /// `SinkStats` stays pub(crate).
    #[cfg(feature = "jit")]
    pub fn compiled_leaf_opt_sink_counts_for_test(
        leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
    ) -> Option<(usize, usize)> {
        crate::group1_startup_drivers::compiled_leaf_opt_sink_counts_for_test(leaf)
    }

    /// FENCE: `(MaterializeCons count, every such pc is after Baset)`.
    /// The IR walk stays inside eval.
    #[cfg(feature = "jit")]
    pub fn sink_only_materialize_at_aset_for_test(
        source: &crate::emacs_core::bytecode::ByteCodeFunction,
        leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
    ) -> (u32, bool) {
        crate::group1_startup_drivers::sink_only_materialize_at_aset_for_test(source, leaf)
    }

    /// FENCE: required-parameter count. The field stays pub(crate). Arity
    /// uses [`CompiledLeaf::arity`].
    #[cfg(feature = "jit")]
    pub fn compiled_leaf_required_for_test(
        leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
    ) -> usize {
        crate::group1_startup_drivers::compiled_leaf_required_for_test(leaf)
    }

    /// FENCE: register-ABI argument ceiling. `MAX_REG_ARGS` stays pub(crate).
    #[cfg(feature = "jit")]
    pub fn max_reg_args_for_test() -> usize {
        crate::group1_startup_drivers::max_reg_args_for_test()
    }

    /// FENCE: lists48-osr profile plus the surrounding Opt stack. The profile
    /// TLS stays pub(super).
    #[cfg(feature = "jit")]
    #[must_use = "the lists48-osr profile ends when the guard is dropped"]
    pub fn enter_lists48_osr_profile_for_test() -> impl Drop {
        crate::emacs_core::jit::compile::enter_lists48_osr_profile_for_test()
    }

    /// FENCE: redefining-callback map snapshot. `MapCallee` stays pub(crate).
    pub fn map_resume_redefining_snapshot_for_test(
        eval: &mut crate::emacs_core::eval::Context,
        split: usize,
    ) -> String {
        crate::group1_startup_drivers::map_resume_redefining_snapshot_for_test(eval, split)
    }

    /// FENCE: debug-on-entry mapcar observations. Depth and the specpdl
    /// stay pub(crate); the test compares the returned words.
    #[cfg(feature = "jit")]
    pub fn mapcar_debug_entry_obs_for_test(
        ctx: &mut crate::emacs_core::eval::Context,
    ) -> (
        Option<i64>,
        String,
        usize,
        usize,
        usize,
        usize,
        bool,
        bool,
        String,
    ) {
        crate::group1_startup_drivers::mapcar_debug_entry_obs_for_test(ctx)
    }

    /// FENCE: mid-callback mapcar backtrace words. `HofResume` stays pub(crate).
    #[cfg(feature = "jit")]
    pub fn inline_hof_mapcar_backtrace_obs_for_test(
        ctx: &mut crate::emacs_core::eval::Context,
    ) -> (usize, usize, Option<i64>, usize, bool) {
        crate::group1_startup_drivers::inline_hof_mapcar_backtrace_obs_for_test(ctx)
    }

    /// FENCE: switch-hoist mutation answer. The plan and the lower stay in eval.
    #[cfg(feature = "jit")]
    pub fn opt_switch_hoist_call_mutation_for_test(
        ctx: &mut crate::emacs_core::eval::Context,
    ) -> (bool, bool, String) {
        crate::group1_startup_drivers::opt_switch_hoist_call_mutation_for_test(ctx)
    }

    /// FENCE: raw frameless entry of either ABI. `EntryShape` stays pub(crate).
    ///
    /// # Safety
    /// `args_ptr` addresses `leaf`'s arity in tagged words, `vmctx` is the
    /// dormant owner `Context` for the call, and `out` is a writable slot.
    #[cfg(feature = "jit")]
    pub unsafe fn entry_call_raw_consts_for_test(
        leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
        vmctx: *mut u8,
        consts: *const neovm_value::value::Value,
        args_ptr: *const i64,
        out: &mut i64,
    ) -> i64 {
        unsafe { leaf.entry_call_raw_consts(vmctx, consts, args_ptr, out) }
    }

    /// FENCE: lists48-osr aset observations. The OSR entry stays pub(crate).
    #[cfg(feature = "jit")]
    pub fn lists48_osr_aset_advice_obs_for_test(
        ctx: &mut crate::emacs_core::eval::Context,
    ) -> (
        usize,
        bool,
        &'static str,
        bool,
        bool,
        bool,
        &'static str,
        bool,
        bool,
        Option<i64>,
        usize,
        String,
        Option<i64>,
    ) {
        crate::group1_startup_drivers::lists48_osr_aset_advice_obs_for_test(ctx)
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

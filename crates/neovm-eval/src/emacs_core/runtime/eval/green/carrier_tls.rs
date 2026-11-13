//! Carrier TLS across jobs (peer review round 2, conditions 1 and 2).
//!
//! A carrier keeps its OS-thread TLS across jobs; a fresh OS thread would not.
//! Two disjoint passes restore the fresh-thread state, split by what an idle
//! carrier may still hold:
//!
//! - [`park_carrier_tls`] runs on the carrier thread at the end of every job,
//!   before the carrier is registered idle. It clears the heap slot and its
//!   hot-path mirrors plus every heap-pointer `OwnerCarried` record
//!   (standard-table objects, the Context-owned registry handles, the
//!   in-flight root registry, the quit request). These steps are plain
//!   stores: a registry slot forgets its handle (`None`) and none allocates
//!   or dereferences a heap object. An idle carrier
//!   therefore names no heap or World object, so an idle-time collection may
//!   free everything the retired job allocated.
//! - [`reset_carrier_tls`] runs as the first action of a reused job, before
//!   `baton.enter` and before any owner prologue, and clears the remaining
//!   `MustReset` caches. Every deferred step only clears or forgets
//!   containers of `Copy` `Value` bits and native memory; none dereferences a
//!   heap object, because an idle-time collection may have freed whatever
//!   those `Value`s named. A fresh carrier and a carrier that is never handed
//!   another job pay nothing.
//!
//! Invariants (stated here, enforced in debug builds):
//!
//! - once a carrier is registered idle, the only work left before it blocks
//!   on its job channel is `Control::finish` handing the Baton to the
//!   dispatcher; it touches no carrier TLS record and no World state, and the
//!   job channel wait and thread exit are the only things that run until the
//!   next job;
//! - the reuse reset is the first action of a reused job;
//! - parking leaves the heap slot empty (test builds: the known harness heap)
//!   and every heap-pointer record at its fresh value; `park_carrier_tls`
//!   asserts this after each step and the reuse prologue asserts it again
//!   before the deferred reset, so a carrier cannot acquire a heap pointer
//!   while idle.
//!
//! World-carried swap slots are deliberately not reset: the previous job's
//! `stow_carried_state` already moved the World's value out of the carrier,
//! leaving its placeholder. Carrier-intrinsic records (intern epoch caches,
//! allocator/std TLS) are not reset either.

/// Cold, idempotent reuse reset. Runs on the carrier thread at the start of a
/// reused job, before its owner prologue; never on the park path.
static CARRIER_TLS_RESETS: std::sync::Mutex<Vec<CarrierTlsReset>> =
    std::sync::Mutex::new(Vec::new());

/// One workspace crate's carrier-park reset, registered by the editor binary
/// (those crates cannot be named from neovm-core without a layering cycle).
pub struct CarrierTlsReset {
    name: &'static str,
    reset: fn(),
    #[cfg(debug_assertions)]
    fingerprint: fn() -> u64,
}

impl CarrierTlsReset {
    /// Release build registration (the fingerprint exists only in debug).
    pub const fn new(name: &'static str, reset: fn()) -> Self {
        Self {
            name,
            reset,
            #[cfg(debug_assertions)]
            fingerprint: || 0,
        }
    }

    /// Debug build registration with a snapshot fingerprint.
    #[cfg(debug_assertions)]
    pub const fn with_fingerprint(
        name: &'static str,
        reset: fn(),
        fingerprint: fn() -> u64,
    ) -> Self {
        Self {
            name,
            reset,
            fingerprint,
        }
    }
}

/// Register one workspace crate's carrier TLS reset. Idempotent by name;
/// called once at editor startup, before any Lisp runs.
pub fn register_carrier_tls_reset(reset: CarrierTlsReset) {
    let mut resets = CARRIER_TLS_RESETS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if resets.iter().any(|entry| entry.name == reset.name) {
        return;
    }
    resets.push(reset);
}

/// Run one park reset step and, in debug builds, assert it left the carrier's
/// tagged-heap slot empty. The next job's prologue requires an empty slot; a
/// step that allocates would otherwise install the test fallback heap and
/// look fine until the next job aborts.
macro_rules! reset_step {
    ($name:expr, $call:expr) => {{
        $call;
        #[cfg(debug_assertions)]
        debug_assert!(
            neovm_value::tagged::gc::carrier_heap_is_acceptable_for_job_start(),
            "carrier park reset step `{}` left a stale tagged heap installed",
            $name
        );
    }};
}

/// Park-time heap-pointer nulls. Runs on the carrier thread at the end of
/// every job, before the carrier is registered idle.
///
/// Plain stores only: no step allocates and no step dereferences a heap
/// object. The heap slot and its hot-path mirrors go first, so every later
/// step reads a fresh (null) installed-heap identity. Registry slots forget
/// their handle (`None`); the registries themselves stay alive through the
/// World's Contexts, so the idle carrier merely drops its clone. The final
/// clear undoes the one harness exception: a test build's lazy charset
/// initializer may install the test fallback heap, which must not stay in an
/// idle carrier's slot either.
pub(crate) fn park_carrier_tls() {
    reset_step!(
        "heap-clear",
        neovm_value::tagged::gc::clear_carrier_heap_tls()
    );

    // Heap-pointer OwnerCarried records: the standard-table object caches.
    reset_step!(
        "casetab",
        neovm_text::emacs_core::casetab::reset_casetab_thread_locals()
    );
    reset_step!(
        "category",
        neovm_text::emacs_core::category::reset_category_thread_locals()
    );
    reset_step!(
        "syntax_tables",
        neovm_text::emacs_core::syntax::reset_syntax_thread_locals()
    );

    // Context-owned registry handles; each registry holds heap `Value`s.
    reset_step!("ccl", neovm_text::emacs_core::ccl::park_ccl_registry());
    reset_step!(
        "charset",
        neovm_text::emacs_core::charset::park_carrier_tls()
    );
    reset_step!(
        "builtins_registries",
        crate::emacs_core::builtins::park_carrier_builtins_thread_locals()
    );
    reset_step!(
        "dynamic_module_registry",
        super::super::data::dynamic_module_registry::park_dynamic_module_registry()
    );
    reset_step!(
        "terminal",
        crate::emacs_core::eval::lifecycle::terminal_park_carrier_tls()
    );
    reset_step!(
        "xfaces",
        crate::emacs_core::eval::lifecycle::xfaces_park_carrier_tls()
    );

    // Residual owner-carried pointers and the owner's quit request.
    reset_step!("in_flight_roots", neovm_value::error::park_carrier_tls());
    reset_step!("quit_request", crate::emacs_core::eval::park_carrier_tls());

    // Idle carriers must name no heap at all; a test build's lazy charset
    // initializer can have installed the harness heap above.
    reset_step!(
        "heap-clear-final",
        neovm_value::tagged::gc::clear_carrier_heap_tls()
    );
}

/// Deferred `MustReset` cache clear. Runs as the first action of a reused
/// job, before `baton.enter` and the owner prologue; skipped for a fresh
/// carrier or one that is never handed another job.
///
/// No heap clear happens here: [`park_carrier_tls`] already nulled every
/// heap-pointer record, and the reuse prologue asserts that before calling
/// this. Every step below only clears or forgets containers of `Copy` `Value`
/// bits and native memory; none dereferences a heap object, because an
/// idle-time collection may have freed whatever those `Value`s named.
pub(crate) fn reset_carrier_tls() {
    // Mutator / collection state.
    reset_step!(
        "collection_reads",
        neovm_value::tagged::collection_reads::reset_carrier_tls()
    );
    reset_step!("gc", neovm_value::tagged::gc::reset_carrier_tls());
    reset_step!("value", neovm_value::tagged::value::reset_carrier_tls());

    // Residual MustReset caches. The heap-pointer records they used to clear
    // as defence in depth are park-nulled above.
    reset_step!(
        "syntax",
        neovm_text::emacs_core::syntax::reset_carrier_tls()
    );
    reset_step!(
        "parse_cache",
        neovm_text::emacs_core::syntax::parse_cache::reset_carrier_tls()
    );
    reset_step!(
        "charset_memo",
        neovm_text::emacs_core::charset::reset_carrier_tls()
    );
    reset_step!(
        "string_pos_cache",
        neovm_value::string_pos_cache::reset_carrier_tls()
    );
    reset_step!(
        "regex",
        neovm_text::emacs_core::regex::reset_regex_thread_locals()
    );
    reset_step!(
        "regex_emacs",
        neovm_text::emacs_core::regex_emacs::reset_carrier_tls()
    );
    reset_step!(
        "builtins_caches",
        crate::emacs_core::builtins::reset_carrier_builtins_thread_locals()
    );
    reset_step!(
        "dynamic_module",
        crate::emacs_core::eval::lifecycle::reset_dynamic_module_carrier_tls()
    );
    reset_step!(
        "error_format",
        crate::emacs_core::print::reset_error_format_for_carrier()
    );
    reset_step!("eval", crate::emacs_core::eval::reset_carrier_tls());
    reset_step!("intern", neovm_value::intern::reset_carrier_tls());
    reset_step!("symbol", neovm_value::symbol::reset_carrier_tls());
    reset_step!("load", crate::emacs_core::load::reset_carrier_tls());
    reset_step!(
        "xdisp",
        super::super::data::xdisp_column_spec::reset_carrier_tls()
    );
    reset_step!(
        "indent",
        neovm_edit::emacs_core::indent::reset_carrier_tls()
    );
    reset_step!("print", crate::emacs_core::print::reset_carrier_tls());
    reset_step!(
        "subr_leaf",
        crate::emacs_core::subr::leaf::reset_carrier_tls()
    );
    reset_step!(
        "perf_trace",
        crate::emacs_core::eval::perf::reset_carrier_tls()
    );
    reset_step!(
        "pdump_convert",
        crate::emacs_core::pdump::convert::reset_carrier_tls()
    );
    reset_step!(
        "pdump_runtime",
        crate::emacs_core::pdump::runtime::reset_carrier_tls()
    );

    // JIT per-thread caches, scratch and counters. The submodules exist only
    // with the `jit` feature (evaluator-only builds must still compile).
    #[cfg(feature = "jit")]
    {
        reset_step!("jit_aot", crate::emacs_core::jit::aot::reset_carrier_tls());
        reset_step!("jit_bg", crate::emacs_core::jit::bg::reset_carrier_tls());
        reset_step!(
            "jit_cache",
            crate::emacs_core::jit::cache::reset_carrier_tls()
        );
        reset_step!(
            "jit_compile",
            crate::emacs_core::jit::compile::reset_carrier_tls()
        );
        reset_step!(
            "jit_inline",
            crate::emacs_core::jit::inline::reset_carrier_tls()
        );
        reset_step!(
            "jit_stats",
            crate::emacs_core::jit::stats::reset_carrier_tls()
        );
        reset_step!(
            "jit_tier2",
            crate::emacs_core::jit::tier2::reset_carrier_tls()
        );
    }

    // Workspace crates that neovm-core can name directly, then the ones the
    // editor binary registered.
    reset_step!(
        "input_latency",
        neomacs_display_protocol::input_latency::reset_carrier_tls()
    );
    let registered: Vec<(&'static str, fn())> = CARRIER_TLS_RESETS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .iter()
        .map(|entry| (entry.name, entry.reset))
        .collect();
    for (name, reset) in registered {
        reset_step!(name, reset());
    }
}

/// Debug-only fingerprint over every MustReset record (per-module probes).
#[cfg(debug_assertions)]
#[allow(dead_code)]
pub(crate) fn snapshot_carrier_tls() -> u64 {
    // Build the charset placeholder now (allocating the test-only plists on
    // the fallback heap), then restore the empty slot before sampling.
    neovm_text::emacs_core::charset::warm_carrier_charset_registry();
    #[cfg(test)]
    neovm_value::tagged::gc::clear_carrier_heap_tls();
    let mut h = neovm_value::tls_fingerprint::SEED;
    neovm_value::tagged::gc::carrier_tls_fingerprint(&mut h);
    neovm_value::tagged::collection_reads::carrier_tls_fingerprint(&mut h);
    neovm_value::tagged::value::carrier_tls_fingerprint(&mut h);
    neovm_text::emacs_core::casetab::carrier_tls_fingerprint(&mut h);
    neovm_text::emacs_core::category::carrier_tls_fingerprint(&mut h);
    neovm_text::emacs_core::ccl::carrier_tls_fingerprint(&mut h);
    neovm_text::emacs_core::charset::carrier_tls_fingerprint(&mut h);
    neovm_text::emacs_core::syntax::carrier_tls_fingerprint(&mut h);
    neovm_text::emacs_core::syntax::parse_cache::carrier_tls_fingerprint(&mut h);
    neovm_value::string_pos_cache::carrier_tls_fingerprint(&mut h);
    neovm_text::emacs_core::regex::carrier_tls_fingerprint(&mut h);
    neovm_text::emacs_core::regex_emacs::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::eval::lifecycle::terminal_carrier_tls_fingerprint(&mut h);
    crate::emacs_core::eval::lifecycle::xfaces_carrier_tls_fingerprint(&mut h);
    crate::emacs_core::builtins::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::eval::lifecycle::dynamic_module_carrier_tls_fingerprint(&mut h);
    neovm_value::error::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::eval::carrier_tls_fingerprint(&mut h);
    neovm_value::intern::carrier_tls_fingerprint(&mut h);
    neovm_value::symbol::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::load::carrier_tls_fingerprint(&mut h);
    super::super::data::xdisp_column_spec::carrier_tls_fingerprint(&mut h);
    neovm_edit::emacs_core::indent::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::print::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::subr::leaf::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::eval::perf::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::pdump::convert::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::pdump::runtime::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::jit::carrier_tls_fingerprint(&mut h);
    neovm_value::tls_fingerprint::mix(
        &mut h,
        neomacs_display_protocol::input_latency::carrier_tls_fingerprint(),
    );
    let fingerprints: Vec<fn() -> u64> = CARRIER_TLS_RESETS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .iter()
        .map(|entry| entry.fingerprint)
        .collect();
    for fingerprint in fingerprints {
        neovm_value::tls_fingerprint::mix(&mut h, fingerprint());
    }
    h
}

/// Test hook: arm the fresh-vs-parked snapshot in the next carrier jobs.
#[cfg(all(test, debug_assertions))]
pub(crate) fn arm_carrier_tls_snapshot() {
    TLS_SNAPSHOT_ARMED.store(true, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(all(test, debug_assertions))]
static TLS_SNAPSHOT_ARMED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(all(test, debug_assertions))]
thread_local! {
    /// The fresh-carrier fingerprint, captured before the first job's prologue.
    static SNAPSHOT_REFERENCE: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

/// Debug hook at job start: capture the fresh-carrier reference once. Must run
/// before `new_green_owner` so the reference is the true fresh TLS. A no-op
/// outside test debug builds.
#[inline(always)]
pub(crate) fn carrier_tls_job_started() {
    #[cfg(all(test, debug_assertions))]
    {
        if !TLS_SNAPSHOT_ARMED.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        SNAPSHOT_REFERENCE.with(|slot| {
            if slot.get().is_none() {
                slot.set(Some(snapshot_carrier_tls()));
            }
        });
    }
}

/// Debug hook at the start of a reused job, before the deferred reset: the
/// carrier was idle, so park must have left no heap pointer behind. A carrier
/// that still names a heap here would run the deferred resets against freed
/// state. A no-op outside debug builds.
#[inline(always)]
pub(crate) fn carrier_tls_idle_state_and_check() {
    #[cfg(debug_assertions)]
    {
        debug_assert!(
            neovm_value::tagged::gc::carrier_heap_is_acceptable_for_job_start(),
            "a reused carrier still names a heap: park did not clear the heap slot"
        );
    }
}

/// Debug hook after a reuse reset: every MustReset record must again equal the
/// fresh-carrier value captured at this thread's first job start. A skipped
/// reset diverges.
/// A no-op outside test debug builds.
#[inline(always)]
pub(crate) fn carrier_tls_reset_and_check() {
    #[cfg(all(test, debug_assertions))]
    {
        if !TLS_SNAPSHOT_ARMED.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let Some(reference) = SNAPSHOT_REFERENCE.with(|slot| slot.get()) else {
            return;
        };
        let after = snapshot_carrier_tls();
        assert!(
            reference == after,
            "carrier TLS diverged after park: fresh={reference:#x} after_reset={after:#x}"
        );
    }
}

#[cfg(test)]
mod absent_terminal_tls_tests {
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::eval::data::terminal_state::{
        TerminalLispRegistry, TerminalRegistryHandle,
    };
    use crate::emacs_core::eval::lifecycle::{
        install_owner_terminal_registry_handle, terminal_park_carrier_tls,
    };
    use neovm_value::value::Value;

    fn nonempty_registry() -> TerminalRegistryHandle {
        let mut registry = TerminalLispRegistry::default();
        registry
            .ensure_terminal(701)
            .params
            .push((Value::fixnum(11), Value::fixnum(22)));
        registry.ensure_terminal(702);
        TerminalRegistryHandle::new(registry)
    }

    fn registry_contents(
        handle: &TerminalRegistryHandle,
    ) -> (usize, Vec<u64>, Vec<(u64, Value, Vec<(Value, Value)>)>) {
        let registry = handle.borrow();
        let mut terminals: Vec<_> = registry
            .terminals
            .iter()
            .map(|(id, terminal)| (*id, terminal.handle, terminal.params.clone()))
            .collect();
        terminals.sort_by_key(|(id, _, _)| *id);
        (
            handle.heap_identity(),
            registry.creation_order.clone(),
            terminals,
        )
    }

    #[test]
    fn absent_terminal_owner_install_preserves_the_nonempty_lower_registry() {
        let _context = Context::new(crate::test_utils::unit_runtime());
        let handle = nonempty_registry();
        let before = registry_contents(&handle);
        install_owner_terminal_registry_handle(&handle);
        assert_eq!(registry_contents(&handle), before);
    }

    #[test]
    fn absent_terminal_park_preserves_the_nonempty_lower_registry() {
        let _context = Context::new(crate::test_utils::unit_runtime());
        let handle = nonempty_registry();
        let before = registry_contents(&handle);
        terminal_park_carrier_tls();
        assert_eq!(registry_contents(&handle), before);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn absent_terminal_fingerprint_folds_the_zero_heap_identity_once() {
        let _context = Context::new(crate::test_utils::unit_runtime());
        let seed = neovm_value::tls_fingerprint::SEED;
        let mut expected = seed;
        neovm_value::tls_fingerprint::mix(&mut expected, 0);
        assert_ne!(expected, seed, "folding an absent identity is observable");
        let mut actual = seed;
        crate::emacs_core::eval::lifecycle::terminal_carrier_tls_fingerprint(&mut actual);
        assert_eq!(actual, expected);
    }
}

#[cfg(test)]
mod lifecycle_carrier_protocol_tests {
    use std::any::Any;
    use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

    use crate::emacs_core::eval::Context;
    use crate::emacs_core::eval::data::dynamic_module_registry::{
        dynamic_module_registry_heap_identity, park_dynamic_module_registry, with_global_refs,
    };
    use crate::emacs_core::eval::lifecycle;

    fn restore_after<T>(
        context: &mut Context,
        owner: bool,
        run: impl FnOnce(&mut Context) -> T,
    ) -> T {
        // Restore only after catch_unwind has ended the first unwind. A Drop
        // guard that itself reached an unbound hook could otherwise abort.
        let result = catch_unwind(AssertUnwindSafe(|| run(context)));
        let restored = catch_unwind(AssertUnwindSafe(|| {
            if owner {
                context.setup_owner_thread_locals();
            } else {
                context.setup_thread_locals();
            }
        }));
        if let Err(payload) = restored {
            resume_unwind(payload);
        }
        match result {
            Ok(value) => value,
            Err(payload) => resume_unwind(payload),
        }
    }

    fn panic_text(payload: Box<dyn Any + Send>) -> String {
        match payload.downcast::<String>() {
            Ok(text) => *text,
            Err(payload) => match payload.downcast::<&str>() {
                Ok(text) => (*text).to_owned(),
                Err(_) => "<non-string panic>".to_owned(),
            },
        }
    }

    #[test]
    fn absent_xfaces_park_preserves_the_lower_face_registry() {
        let context = Context::new(crate::test_utils::unit_runtime());
        context.font_registry.borrow_mut().face_set_generation = 19;
        let identity = context.font_registry.heap_identity();
        lifecycle::xfaces_park_carrier_tls();
        assert_eq!(context.font_registry.borrow().face_set_generation, 19);
        assert_eq!(context.font_registry.heap_identity(), identity);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn absent_xfaces_fingerprint_folds_zero_once() {
        let _context = Context::new(crate::test_utils::unit_runtime());
        let seed = neovm_value::tls_fingerprint::SEED;
        let mut expected = seed;
        neovm_value::tls_fingerprint::mix(&mut expected, 0);
        assert_ne!(expected, seed);
        let mut actual = seed;
        lifecycle::xfaces_carrier_tls_fingerprint(&mut actual);
        assert_eq!(actual, expected);
    }

    #[test]
    fn absent_xfaces_behavior_still_refuses_face_creation() {
        let mut context = Context::new(crate::test_utils::unit_runtime());
        let result = catch_unwind(AssertUnwindSafe(|| {
            context.eval_str("(internal-make-lisp-face 'carrier-protocol-face)")
        }));
        let text = panic_text(result.expect_err("xfaces behavior remains unavailable"));
        assert!(
            text.contains("ABSENT-LAYER display::internal-make-lisp-face"),
            "{text}"
        );
    }

    #[test]
    fn absent_dynamic_reset_does_not_park_or_replace_global_refs() {
        let mut context = Context::new(crate::test_utils::unit_runtime());
        // A real lower registry with a vacant global-ref slot needs no FFI
        // pointer or fake upper module environment.
        context.dynamic_module_registry.borrow_mut().push(None);
        let identity = context.dynamic_module_registry.heap_identity();
        assert_eq!(dynamic_module_registry_heap_identity(), Some(identity));
        restore_after(&mut context, true, |_| {
            lifecycle::reset_dynamic_module_carrier_tls();
            assert_eq!(dynamic_module_registry_heap_identity(), Some(identity));
            assert_eq!(with_global_refs(|slot| slot.borrow().len()), 1);
            park_dynamic_module_registry();
            assert_eq!(dynamic_module_registry_heap_identity(), None);
            lifecycle::reset_dynamic_module_carrier_tls();
            assert_eq!(dynamic_module_registry_heap_identity(), None);
        });
        assert_eq!(dynamic_module_registry_heap_identity(), Some(identity));
        assert_eq!(with_global_refs(|slot| slot.borrow().len()), 1);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn absent_dynamic_fingerprint_folds_the_real_lower_identity_between_zeros() {
        fn expected(identity: u64) -> u64 {
            let mut hash = neovm_value::tls_fingerprint::SEED;
            for word in [0, identity, 0] {
                neovm_value::tls_fingerprint::mix(&mut hash, word);
            }
            hash
        }
        let mut context = Context::new(crate::test_utils::unit_runtime());
        let identity = context.dynamic_module_registry.heap_identity() as u64;
        assert_ne!(identity, 0);
        assert_ne!(expected(identity), expected(0));
        restore_after(&mut context, true, |_| {
            let mut live = neovm_value::tls_fingerprint::SEED;
            lifecycle::dynamic_module_carrier_tls_fingerprint(&mut live);
            assert_eq!(live, expected(identity));
            park_dynamic_module_registry();
            assert_eq!(dynamic_module_registry_heap_identity(), None);
            let mut parked = neovm_value::tls_fingerprint::SEED;
            lifecycle::dynamic_module_carrier_tls_fingerprint(&mut parked);
            assert_eq!(parked, expected(0));
        });
        assert_eq!(
            dynamic_module_registry_heap_identity(),
            Some(identity as usize)
        );
    }

    #[cfg(debug_assertions)]
    static PROBE_RESETS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    #[cfg(debug_assertions)]
    const PROBE_NAME: &str = "5714-executing-carrier-protocol";
    #[cfg(debug_assertions)]
    const PROBE_FINGERPRINT: u64 = 0x5714_5a5a_f01d_0001;

    #[cfg(debug_assertions)]
    fn reset_probe() {
        PROBE_RESETS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    #[cfg(debug_assertions)]
    fn fingerprint_probe() -> u64 {
        PROBE_FINGERPRINT
    }

    #[cfg(debug_assertions)]
    struct ProbeRegistration;
    #[cfg(debug_assertions)]
    impl Drop for ProbeRegistration {
        fn drop(&mut self) {
            super::CARRIER_TLS_RESETS
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .retain(|entry| entry.name != PROBE_NAME);
            super::TLS_SNAPSHOT_ARMED.store(false, std::sync::atomic::Ordering::SeqCst);
            super::SNAPSHOT_REFERENCE.with(|slot| slot.set(None));
        }
    }

    #[cfg(debug_assertions)]
    #[test]
    fn every_lifecycle_carrier_tls_phase_executes_and_restores_the_owner() {
        use std::sync::atomic::Ordering;
        crate::test_utils::init_test_tracing();
        let hooks = crate::test_utils::unit_runtime();
        // Capture on this fresh carrier before constructing its first Context.
        let without_probe = super::snapshot_carrier_tls();
        let _registration = ProbeRegistration;
        PROBE_RESETS.store(0, Ordering::SeqCst);
        super::register_carrier_tls_reset(super::CarrierTlsReset::with_fingerprint(
            PROBE_NAME,
            reset_probe,
            fingerprint_probe,
        ));
        let mut expected = without_probe;
        neovm_value::tls_fingerprint::mix(&mut expected, PROBE_FINGERPRINT);
        assert_ne!(expected, without_probe);
        assert_eq!(
            super::snapshot_carrier_tls(),
            expected,
            "registered fingerprint must be folded"
        );
        super::arm_carrier_tls_snapshot();
        super::carrier_tls_job_started();
        let reference = super::SNAPSHOT_REFERENCE
            .with(|slot| slot.get())
            .expect("fresh reference");
        assert_eq!(reference, expected);
        let mut context = Context::new(hooks);
        let identity = context.dynamic_module_registry.heap_identity();
        for (cycle, owner) in [false, true].into_iter().enumerate() {
            restore_after(&mut context, owner, |_| {
                super::park_carrier_tls();
                assert_eq!(neovm_value::tagged::gc::current_tagged_heap_identity(), None);
                assert_eq!(dynamic_module_registry_heap_identity(), None);
                assert_eq!(PROBE_RESETS.load(Ordering::SeqCst), cycle);
                super::carrier_tls_idle_state_and_check();
                // The registered reset is deferred until reuse, exactly once
                // for each completed park; the actual protocol enforces it.
                super::reset_carrier_tls();
                // Check before snapshot's test-only fallback-heap cleanup can
                // hide a stale or newly installed heap from the observer.
                assert_eq!(neovm_value::tagged::gc::current_tagged_heap_identity(), None);
                assert_eq!(PROBE_RESETS.load(Ordering::SeqCst), cycle + 1);
                super::carrier_tls_reset_and_check();
                assert_eq!(super::snapshot_carrier_tls(), reference);
            });
            assert_eq!(dynamic_module_registry_heap_identity(), Some(identity));
            assert_eq!(
                neovm_value::tagged::gc::current_tagged_heap_identity(),
                Some(identity)
            );
        }
        assert_eq!(PROBE_RESETS.load(Ordering::SeqCst), 2);
    }
}

#[cfg(test)]
mod perf_carrier_tls_tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use crate::emacs_core::eval::{Context, perf};

    #[test]
    fn absent_perf_reset_has_no_system_tracing_state_to_drop() {
        let _context = Context::new(crate::test_utils::unit_runtime());
        assert!(!perf::enabled());
        perf::reset_carrier_tls();
    }

    #[cfg(debug_assertions)]
    #[test]
    fn absent_perf_fingerprint_folds_zero_once() {
        let _context = Context::new(crate::test_utils::unit_runtime());
        let seed = neovm_value::tls_fingerprint::SEED;
        let mut expected = seed;
        neovm_value::tls_fingerprint::mix(&mut expected, 0);
        assert_ne!(expected, seed, "the zero-state contribution must be folded");
        let mut actual = seed;
        perf::carrier_tls_fingerprint(&mut actual);
        assert_eq!(actual, expected);
    }

    #[test]
    fn absent_perf_record_still_refuses_and_timing_stays_disabled() {
        let _context = Context::new(crate::test_utils::unit_runtime());
        assert!(!perf::enabled());
        assert_eq!(perf::time_op(perf::PerfOp::StringMatch, || 17), 17);
        let result = catch_unwind(AssertUnwindSafe(|| {
            perf::record_op(perf::PerfOp::StringMatch, std::time::Duration::ZERO);
        }));
        let payload = result.expect_err("the absent system counter writer must still refuse");
        let text = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("<non-string panic>");
        assert!(text.contains("ABSENT-LAYER system::perf.record"), "{text}");
    }
}

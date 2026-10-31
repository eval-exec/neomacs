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
            crate::tagged::gc::carrier_heap_is_acceptable_for_job_start(),
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
    reset_step!("heap-clear", crate::tagged::gc::clear_carrier_heap_tls());

    // Heap-pointer OwnerCarried records: the standard-table object caches.
    reset_step!(
        "casetab",
        crate::emacs_core::casetab::reset_casetab_thread_locals()
    );
    reset_step!(
        "category",
        crate::emacs_core::category::reset_category_thread_locals()
    );
    reset_step!(
        "syntax_tables",
        crate::emacs_core::syntax::reset_syntax_thread_locals()
    );

    // Context-owned registry handles; each registry holds heap `Value`s.
    reset_step!("ccl", crate::emacs_core::ccl::park_ccl_registry());
    reset_step!("charset", crate::emacs_core::charset::park_carrier_tls());
    reset_step!(
        "builtins_registries",
        crate::emacs_core::builtins::park_carrier_builtins_thread_locals()
    );
    reset_step!(
        "dynamic_module_registry",
        crate::emacs_core::dynamic_module::park_dynamic_module_registry()
    );
    reset_step!(
        "terminal",
        crate::emacs_core::terminal::pure::park_carrier_tls()
    );
    reset_step!("xfaces", crate::emacs_core::xfaces::park_carrier_tls());

    // Residual owner-carried pointers and the owner's quit request.
    reset_step!(
        "in_flight_roots",
        crate::emacs_core::error::park_carrier_tls()
    );
    reset_step!("quit_request", crate::emacs_core::eval::park_carrier_tls());

    // Idle carriers must name no heap at all; a test build's lazy charset
    // initializer can have installed the harness heap above.
    reset_step!(
        "heap-clear-final",
        crate::tagged::gc::clear_carrier_heap_tls()
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
        crate::tagged::collection_reads::reset_carrier_tls()
    );
    reset_step!("gc", crate::tagged::gc::reset_carrier_tls());
    reset_step!("value", crate::tagged::value::reset_carrier_tls());

    // Residual MustReset caches. The heap-pointer records they used to clear
    // as defence in depth are park-nulled above.
    reset_step!("syntax", crate::emacs_core::syntax::reset_carrier_tls());
    reset_step!(
        "parse_cache",
        crate::emacs_core::syntax::parse_cache::reset_carrier_tls()
    );
    reset_step!(
        "charset_memo",
        crate::emacs_core::charset::reset_carrier_tls()
    );
    reset_step!(
        "string_pos_cache",
        crate::emacs_core::string_pos_cache::reset_carrier_tls()
    );
    reset_step!(
        "regex",
        crate::emacs_core::regex::reset_regex_thread_locals()
    );
    reset_step!(
        "regex_emacs",
        crate::emacs_core::regex_emacs::reset_carrier_tls()
    );
    reset_step!(
        "builtins_caches",
        crate::emacs_core::builtins::reset_carrier_builtins_thread_locals()
    );
    reset_step!(
        "dynamic_module",
        crate::emacs_core::dynamic_module::reset_carrier_tls()
    );
    reset_step!(
        "error_format",
        crate::emacs_core::print::reset_error_format_for_carrier()
    );
    reset_step!("eval", crate::emacs_core::eval::reset_carrier_tls());
    reset_step!("intern", crate::emacs_core::intern::reset_carrier_tls());
    reset_step!("symbol", crate::emacs_core::symbol::reset_carrier_tls());
    reset_step!("load", crate::emacs_core::load::reset_carrier_tls());
    reset_step!("xdisp", crate::emacs_core::xdisp::reset_carrier_tls());
    reset_step!("indent", crate::emacs_core::indent::reset_carrier_tls());
    reset_step!("print", crate::emacs_core::print::reset_carrier_tls());
    reset_step!(
        "subr_leaf",
        crate::emacs_core::subr::leaf::reset_carrier_tls()
    );
    reset_step!(
        "perf_trace",
        crate::emacs_core::perf_trace::reset_carrier_tls()
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
    crate::emacs_core::charset::warm_carrier_charset_registry();
    #[cfg(test)]
    crate::tagged::gc::clear_carrier_heap_tls();
    let mut h = crate::tls_fingerprint::SEED;
    crate::tagged::gc::carrier_tls_fingerprint(&mut h);
    crate::tagged::collection_reads::carrier_tls_fingerprint(&mut h);
    crate::tagged::value::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::casetab::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::category::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::ccl::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::charset::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::syntax::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::syntax::parse_cache::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::string_pos_cache::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::regex::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::regex_emacs::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::terminal::pure::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::xfaces::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::builtins::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::dynamic_module::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::error::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::eval::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::intern::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::symbol::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::load::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::xdisp::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::indent::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::print::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::subr::leaf::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::perf_trace::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::pdump::convert::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::pdump::runtime::carrier_tls_fingerprint(&mut h);
    crate::emacs_core::jit::carrier_tls_fingerprint(&mut h);
    crate::tls_fingerprint::mix(
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
        crate::tls_fingerprint::mix(&mut h, fingerprint());
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
            crate::tagged::gc::carrier_heap_is_acceptable_for_job_start(),
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

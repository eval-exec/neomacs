//! T-A3 (design `p1-3-per-symbol-versions` §4.4 C1, §11): a speculated
//! call site whose slot went stale for an UNRELATED redefinition re-arms
//! through its symbol's stamp (`resync_spec_slot`), keeping its cached leaf
//! and re-entering the fast gate once; a related redefinition, a retired
//! leaf, a disarmed slot and the force harness keep today's path.

use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::stats::epoch::{EpochCounters, SpecRevalidation};
use crate::emacs_core::print::print_value;
use crate::emacs_core::symbol::force_fn_stamps_for_test;

/// The compiled leaf the cache holds for the function SYM names.
fn cached_leaf(ev: &Context, sym: &str) -> Option<&'static CompiledLeaf> {
    let f = ev.obarray.symbol_function_id(intern(sym))?;
    let id = f.get_bytecode_data()?.jit_runtime().compiled_id_or_assign();
    let ptr = crate::emacs_core::jit::cache::compiled_leaf_ptr_for_test(id)?;
    // SAFETY: a cached leaf stays allocated while the cache holds it or has
    // retired it (the tests never clear the cache while they look).
    Some(unsafe { &*ptr })
}

/// The caller's Bytecode spec slot that caches CALLEE's leaf.
fn slot_calling<'a>(caller: &'a CompiledLeaf, callee: &CompiledLeaf) -> Option<&'a SpecSlot> {
    caller
        .bytecode_spec_slots()
        .find(|slot| std::ptr::eq(slot.leaf_ptr(), callee))
}

/// A callee with a branch (a call, not an inline), a caller of it, and a
/// caller that raises a quit right before its call; all compiled and warm.
const PROGRAM: &str = r#"(progn
  (defun neovm--ps-callee (a b) (if (> a b) (- a b) (+ a b)))
  (defun neovm--ps-caller (x) (neovm--ps-callee x 1))
  (defun neovm--ps-quitter (x q) (when q (setq quit-flag t)) (neovm--ps-callee x 1))
  (dolist (f '(neovm--ps-callee neovm--ps-caller neovm--ps-quitter))
    (byte-compile f))
  (dotimes (i 1500) (neovm--ps-caller i) (neovm--ps-quitter i nil)))"#;

/// The knobs every test runs under: calls stay calls (no fuser), every
/// body compiles at once, and `NEOVM_FN_STAMPS` as given.
fn warm(stamps: bool) -> Context {
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
    crate::emacs_core::jit::force_profit_defer_for_test(Some(1));
    force_fn_stamps_for_test(Some(stamps));
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(PROGRAM).expect("warmed");
    ev
}

/// The caller's slot for the callee, armed with the callee's leaf.
fn armed_slot(ev: &Context, caller: &str) -> (&'static SpecSlot, *const CompiledLeaf) {
    let caller = cached_leaf(ev, caller).expect("the caller is compiled");
    let callee = cached_leaf(ev, "neovm--ps-callee").expect("the callee is compiled");
    let slot = slot_calling(caller, callee).expect("the caller's slot caches the callee");
    (slot, callee as *const CompiledLeaf)
}

fn run(ev: &mut Context, src: &str) -> String {
    print_value(&ev.eval_str(src).expect(src))
}

fn since(base: &EpochCounters, kind: SpecRevalidation) -> u64 {
    EpochCounters::snapshot().since(base).spec_for(kind)
}

/// An unrelated `fset` leaves the cached leaf in place: the call re-arms
/// through the stamp, re-enters the gate once and takes the fast path.
#[test]
fn an_unrelated_redefinition_keeps_the_leaf_and_reenters_the_fast_path() {
    let mut ev = warm(true);
    let (slot, callee) = armed_slot(&ev, "neovm--ps-caller");
    run(&mut ev, "(fset 'neovm--ps-unrelated #'car)");
    let clock = ev.obarray.function_epoch();
    assert_ne!(
        slot.epoch.load(Ordering::Relaxed),
        clock,
        "the slot is stale"
    );
    let base = EpochCounters::snapshot();
    let fast = SPEC_SHIM_FAST_COUNT.load(Ordering::Relaxed);
    assert_eq!(run(&mut ev, "(neovm--ps-caller 5)"), "4");
    assert_eq!(since(&base, SpecRevalidation::StampResynced), 1);
    assert_eq!(since(&base, SpecRevalidation::StampReentered), 1);
    assert_eq!(since(&base, SpecRevalidation::Rearmed), 0, "no bits re-arm");
    assert!(SPEC_SHIM_FAST_COUNT.load(Ordering::Relaxed) > fast);
    assert_eq!(slot.epoch.load(Ordering::Relaxed), clock);
    assert_eq!(slot.leaf_ptr(), callee, "the leaf was kept");
    // Current again: the next call needs nothing.
    let base = EpochCounters::snapshot();
    assert_eq!(run(&mut ev, "(neovm--ps-caller 7)"), "6");
    assert_eq!(since(&base, SpecRevalidation::StampResynced), 0);
    assert_eq!(since(&base, SpecRevalidation::StampReentered), 0);
    force_fn_stamps_for_test(None);
}

/// With the knob off the same call re-validates by bits, as before.
#[test]
fn the_knob_off_rearms_by_bits() {
    let mut ev = warm(false);
    let (slot, callee) = armed_slot(&ev, "neovm--ps-caller");
    run(&mut ev, "(fset 'neovm--ps-unrelated #'car)");
    let base = EpochCounters::snapshot();
    assert_eq!(run(&mut ev, "(neovm--ps-caller 5)"), "4");
    assert_eq!(since(&base, SpecRevalidation::StampResynced), 0);
    assert_eq!(since(&base, SpecRevalidation::Rearmed), 1);
    assert_eq!(
        slot.epoch.load(Ordering::Relaxed),
        ev.obarray.function_epoch()
    );
    assert_eq!(slot.leaf_ptr(), callee, "re-resolved to the same leaf");
    force_fn_stamps_for_test(None);
}

/// Redefining the called symbol takes effect on the very next call: the
/// stamp refuses, the bits differ, the leaf goes.
#[test]
fn a_related_redefinition_clears_the_leaf_and_calls_the_new_definition() {
    let mut ev = warm(true);
    let (slot, _) = armed_slot(&ev, "neovm--ps-caller");
    run(
        &mut ev,
        "(fset 'neovm--ps-callee (lambda (a b) (* a b 100)))",
    );
    let base = EpochCounters::snapshot();
    assert_eq!(run(&mut ev, "(neovm--ps-caller 5)"), "500");
    assert_eq!(since(&base, SpecRevalidation::StampResynced), 0);
    assert_eq!(since(&base, SpecRevalidation::BindingChanged), 1);
    assert!(slot.leaf_ptr().is_null());
    force_fn_stamps_for_test(None);
}

/// The ABA shape: the binding moves away and back to an object with the
/// same bits. The stamp never vouches across a change, so the site takes
/// the bits re-arm, which drops the leaf and resolves it again from the
/// object the cell holds (design R2: equal bits are not identity).
#[test]
fn a_binding_restored_to_equal_bits_still_takes_the_bits_path() {
    let mut ev = warm(true);
    let (slot, _) = armed_slot(&ev, "neovm--ps-caller");
    run(
        &mut ev,
        "(progn (defvar neovm--ps-saved (symbol-function 'neovm--ps-callee))
                (fset 'neovm--ps-callee #'+)
                (fset 'neovm--ps-callee neovm--ps-saved))",
    );
    let base = EpochCounters::snapshot();
    assert_eq!(run(&mut ev, "(neovm--ps-caller 5)"), "4");
    assert_eq!(since(&base, SpecRevalidation::StampResynced), 0);
    assert_eq!(since(&base, SpecRevalidation::Rearmed), 1);
    assert!(
        !slot.leaf_ptr().is_null(),
        "re-armed from the cell's object"
    );
    force_fn_stamps_for_test(None);
}

/// A retired callee leaf (re-tier, eviction, deopt invalidation) is not
/// kept by a resync (design I5): the next call resolves the current one.
#[test]
fn a_resync_drops_a_retired_leaf() {
    let mut ev = warm(true);
    let (slot, callee) = armed_slot(&ev, "neovm--ps-caller");
    let id = ev
        .obarray
        .symbol_function_id(intern("neovm--ps-callee"))
        .and_then(|f| {
            f.get_bytecode_data()
                .map(|bc| bc.jit_runtime().compiled_id_or_assign())
        })
        .expect("compiled id");
    crate::emacs_core::jit::cache::evict_compiled(id);
    // SAFETY: a retired leaf stays allocated.
    assert!(unsafe { (*callee).retired.get() });
    assert_eq!(slot.leaf_ptr(), callee, "eviction alone does not unlink");
    run(&mut ev, "(fset 'neovm--ps-unrelated #'cdr)");
    let base = EpochCounters::snapshot();
    assert_eq!(run(&mut ev, "(neovm--ps-caller 5)"), "4");
    assert_eq!(since(&base, SpecRevalidation::StampResynced), 1);
    assert_eq!(
        since(&base, SpecRevalidation::StampReentered),
        0,
        "nothing for the fast gate to enter"
    );
    assert_ne!(slot.leaf_ptr(), callee, "the retired leaf is gone");
    force_fn_stamps_for_test(None);
}

/// `NEOVM_JIT_FORCE_SLOW_SPEC` keeps exercising the bits re-validation.
#[test]
fn the_force_harness_takes_the_bits_path() {
    crate::emacs_core::jit::compile::force_slow_spec_for_test(Some(true));
    let mut ev = warm(true);
    run(&mut ev, "(fset 'neovm--ps-unrelated #'car)");
    let base = EpochCounters::snapshot();
    assert_eq!(run(&mut ev, "(neovm--ps-caller 5)"), "4");
    assert_eq!(since(&base, SpecRevalidation::StampResynced), 0);
    assert!(since(&base, SpecRevalidation::Rearmed) >= 1);
    crate::emacs_core::jit::compile::force_slow_spec_for_test(None);
    force_fn_stamps_for_test(None);
}

/// A loader-disarmed slot never re-arms, through a stamp or otherwise.
#[test]
fn a_disarmed_slot_never_resyncs() {
    let mut ev = warm(true);
    let (slot, _) = armed_slot(&ev, "neovm--ps-caller");
    slot.clear_leaf();
    slot.epoch.store(SPEC_EPOCH_DISARMED, Ordering::Relaxed);
    run(&mut ev, "(fset 'neovm--ps-unrelated #'car)");
    let base = EpochCounters::snapshot();
    assert_eq!(run(&mut ev, "(neovm--ps-caller 5)"), "4");
    assert_eq!(since(&base, SpecRevalidation::StampResynced), 0);
    assert_eq!(slot.epoch.load(Ordering::Relaxed), SPEC_EPOCH_DISARMED);
    assert!(slot.leaf_ptr().is_null());
    force_fn_stamps_for_test(None);
}

/// A quit pending at a stale site: the resync (pure) still runs, the
/// re-entered gate steps aside for the quit, and the slow half signals it
/// with no frame left behind.
#[test]
fn a_pending_quit_at_a_resynced_site_signals_quit() {
    let mut ev = warm(true);
    run(&mut ev, "(fset 'neovm--ps-unrelated #'car)");
    let depth = ev.specpdl.len();
    let base = EpochCounters::snapshot();
    assert_eq!(
        run(
            &mut ev,
            "(condition-case nil (neovm--ps-quitter 5 t) (quit 'quit))"
        ),
        "quit"
    );
    assert_eq!(ev.specpdl.len(), depth, "no frame left behind");
    assert_eq!(since(&base, SpecRevalidation::StampResynced), 1);
    assert_eq!(run(&mut ev, "(neovm--ps-quitter 5 nil)"), "4");
    force_fn_stamps_for_test(None);
}

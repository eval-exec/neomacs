//! T-F1: the calls a native caller makes while its callee's first-sight
//! compile is pending take the strict path, with the frames and results the
//! interpreter and the installed leaf give.
//!
//! `runtime_startup_context` is minted in this crate. The cache id, entry
//! kind, tier-up, dispatch, drain, and job counters stay behind authority.

use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use crate::test_utils::{init_test_tracing, runtime_startup_context};
use neovm_eval::test_support::authority::{
    BgJobClassCopy, bg_stats_for_test, cache_entry_kind_for_test, defer_tier_up_for_test,
    dispatch_sized_is_compiled_for_test, drain_ready_pending_for_test, force_deopt_for_test,
    force_mode_for_test, hold_workers_for_test, jit_compiled_id_for_test, quiesce_for_test,
};
use neovm_value::intern::intern;
use neovm_value::value::Value;
use std::time::Duration;

const CHAIN: &str = r#"(progn
  (defvar neovm--bgf-seen nil)
  (defun neovm--bgf-show ()
    (let (fs)
      (mapbacktrace
       (lambda (_evald f args _flags)
         (unless (eq f 'mapbacktrace)
           (push (list (if (symbolp f) f (type-of f)) args) fs))))
      (setq neovm--bgf-seen (seq-take (nreverse fs) 3))
      (backtrace-frame 1 'neovm--bgf-show)))
  (defun neovm--bgf-inner (x) (if (eq x 'show) (neovm--bgf-show) (+ x 1)))
  (defun neovm--bgf-outer (x y) (if y (neovm--bgf-inner x) (+ x 2)))
  (dolist (f '(neovm--bgf-show neovm--bgf-inner neovm--bgf-outer))
    (byte-compile f)))"#;

/// The frames `neovm--bgf-show` saw under `(neovm--bgf-outer 'show 7)`,
/// and `backtrace-frame` from inside it.
fn frames(ev: &mut Context) -> String {
    let frame = ev.eval_str("(neovm--bgf-outer 'show 7)").expect("runs");
    let seen = ev.eval_str("neovm--bgf-seen").expect("recorded");
    format!("{} {}", print_value(&seen), print_value(&frame))
}

fn kind_of(ev: &Context, name: &str) -> &'static str {
    let function = ev
        .obarray()
        .symbol_function_id(intern(name))
        .expect("defined");
    let bc = function.get_bytecode_data().expect("byte-compiled");
    match jit_compiled_id_for_test(bc) {
        Some(id) => cache_entry_kind_for_test(id),
        None => "none",
    }
}

/// T-F1: the calls a native caller makes while its callee's first-sight
/// compile is pending take the strict path, with the frames and results the
/// interpreter and the installed leaf give.
#[test]
fn jit_bg_first_sight_strict_calls_keep_the_frames() {
    // The callee must stay cold while its caller warms up.
    if crate::emacs_core::jit::hot_threshold() != crate::emacs_core::jit::Runtime::HOT_THRESHOLD {
        return;
    }
    init_test_tracing();
    let mut ev = runtime_startup_context();
    ev.eval_str(CHAIN).expect("chain defined");
    let interpreted = frames(&mut ev);
    assert_eq!(
        interpreted,
        "((neovm--bgf-show nil) (neovm--bgf-inner (show)) (neovm--bgf-outer (show 7))) \
         (t neovm--bgf-inner show)"
    );
    force_mode_for_test(Some(crate::emacs_core::jit::BgMode::Threaded));
    force_deopt_for_test(false);
    // Make the caller hot without ever calling the callee, and install it.
    ev.eval_str("(dotimes (i 3000) (neovm--bgf-outer i nil))")
        .expect("warm");
    assert!(quiesce_for_test(Duration::from_secs(60)));
    drain_ready_pending_for_test(Some(&ev));
    assert_eq!(kind_of(&ev, "neovm--bgf-outer"), "compiled");
    {
        let caller = ev
            .obarray()
            .symbol_function_id(intern("neovm--bgf-outer"))
            .expect("defined");
        let bc = caller.get_bytecode_data().expect("byte-compiled");
        // The idle drain installs without the caller's RuntimeState, so a
        // completed job's backoff can still hold its dispatch interpreted.
        // This fixture requires the installed caller to execute natively.
        defer_tier_up_for_test(bc, 0);
        assert!(
            dispatch_sized_is_compiled_for_test(bc),
            "the installed caller must dispatch to compiled code"
        );
    }
    assert_eq!(
        ev.eval_str("(neovm--bgf-outer 5 nil)")
            .expect("caller preflight"),
        Value::make_int(7)
    );
    assert_eq!(kind_of(&ev, "neovm--bgf-outer"), "compiled");
    assert_eq!(kind_of(&ev, "neovm--bgf-inner"), "none");
    let first_sight = bg_stats_for_test().enqueued()[BgJobClassCopy::FirstSight as usize];
    {
        let _hold = hold_workers_for_test();
        let got = ev.eval_str("(neovm--bgf-outer 5 t)").expect("runs");
        assert_eq!(got, Value::make_int(6));
        assert_eq!(
            bg_stats_for_test().enqueued()[BgJobClassCopy::FirstSight as usize],
            first_sight + 1,
            "the native caller's first call deferred the callee"
        );
        assert_eq!(kind_of(&ev, "neovm--bgf-inner"), "pending");
        assert_eq!(frames(&mut ev), interpreted, "strict path while pending");
    }
    assert!(quiesce_for_test(Duration::from_secs(60)));
    let spec_fast = crate::emacs_core::jit::compile::spec_fast_call_count_for_test();
    assert_eq!(frames(&mut ev), interpreted, "native after install");
    assert_eq!(kind_of(&ev, "neovm--bgf-inner"), "compiled");
    assert!(
        crate::emacs_core::jit::compile::spec_fast_call_count_for_test() > spec_fast,
        "the installed callee is entered from the site"
    );
    force_mode_for_test(None);
}

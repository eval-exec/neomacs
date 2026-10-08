//! First-sight compiles in the background (P2.4 B9): a speculated call
//! site's first call into an uncompiled callee defers the callee's backend;
//! the site takes the strict call path until the leaf is installed. T-F1.

use std::time::Duration;

use super::*;
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::compile_pipeline_tests::function;
use crate::emacs_core::jit::compile::force_deopt_for_test;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::Value;

/// A first-sight resolve defers the callee's backend and answers "no leaf"
/// (the strict path); the resolve after the backend finished installs it.
#[test]
fn jit_bg_first_sight_defers_then_installs() {
    force_mode_for_test(Some(BgMode::Sync));
    force_deferred_install_for_test(true);
    hold_publish_for_test(true);
    force_deopt_for_test(false);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = function(
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::make_int(1)],
        1,
    );
    let before = stats_snapshot().enqueued[JobClass::FirstSight as usize];
    assert!(cache::resolve_compiled_leaf_ptr(ctx, &f).is_none());
    let id = f.jit_runtime().compiled_id().expect("id");
    assert_eq!(cache::cache_entry_kind_for_test(id), "pending");
    assert_eq!(
        stats_snapshot().enqueued[JobClass::FirstSight as usize],
        before + 1
    );
    assert!(
        cache::resolve_compiled_leaf_ptr(ctx, &f).is_none(),
        "still running: the strict path again"
    );
    assert_eq!(publish_held_for_test(), 1);
    assert!(cache::resolve_compiled_leaf_ptr(ctx, &f).is_some());
    assert_eq!(cache::cache_entry_kind_for_test(id), "compiled");
    hold_publish_for_test(false);
    force_deferred_install_for_test(false);
    force_mode_for_test(None);
}

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
        .obarray
        .symbol_function_id(intern(name))
        .expect("defined");
    let bc = function.get_bytecode_data().expect("byte-compiled");
    match bc.jit_runtime().compiled_id() {
        Some(id) => cache::cache_entry_kind_for_test(id),
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
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(CHAIN).expect("chain defined");
    let interpreted = frames(&mut ev);
    assert_eq!(
        interpreted,
        "((neovm--bgf-show nil) (neovm--bgf-inner (show)) (neovm--bgf-outer (show 7))) \
         (t neovm--bgf-inner show)"
    );
    force_mode_for_test(Some(BgMode::Threaded));
    force_deopt_for_test(false);
    // Make the caller hot without ever calling the callee, and install it.
    ev.eval_str("(dotimes (i 3000) (neovm--bgf-outer i nil))")
        .expect("warm");
    assert!(quiesce_for_test(Duration::from_secs(60)));
    cache::drain_ready_pending(Some(&ev));
    assert_eq!(kind_of(&ev, "neovm--bgf-outer"), "compiled");
    {
        let caller = ev
            .obarray
            .symbol_function_id(intern("neovm--bgf-outer"))
            .expect("defined");
        let bc = caller.get_bytecode_data().expect("byte-compiled");
        // The idle drain installs without the caller's RuntimeState, so a
        // completed job's backoff can still hold its dispatch interpreted.
        // This fixture requires the installed caller to execute natively.
        bc.jit_runtime().defer_tier_up(0);
        assert_eq!(
            bc.jit_runtime().dispatch_sized(bc.executable_ops().len()),
            crate::emacs_core::jit::Plan::Compiled,
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
    let first_sight = stats_snapshot().enqueued[JobClass::FirstSight as usize];
    {
        let _hold = hold_workers_for_test();
        let got = ev.eval_str("(neovm--bgf-outer 5 t)").expect("runs");
        assert_eq!(got, Value::make_int(6));
        assert_eq!(
            stats_snapshot().enqueued[JobClass::FirstSight as usize],
            first_sight + 1,
            "the native caller's first call deferred the callee"
        );
        assert_eq!(kind_of(&ev, "neovm--bgf-inner"), "pending");
        assert_eq!(frames(&mut ev), interpreted, "strict path while pending");
    }
    assert!(quiesce_for_test(Duration::from_secs(60)));
    let spec_fast = crate::emacs_core::jit::compile::SPEC_FAST_CALL_COUNT
        .load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(frames(&mut ev), interpreted, "native after install");
    assert_eq!(kind_of(&ev, "neovm--bgf-inner"), "compiled");
    assert!(
        crate::emacs_core::jit::compile::SPEC_FAST_CALL_COUNT
            .load(std::sync::atomic::Ordering::Relaxed)
            > spec_fast,
        "the installed callee is entered from the site"
    );
    force_mode_for_test(None);
}

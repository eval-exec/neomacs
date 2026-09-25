//! OSR compiles in the background (P2.4 B10): a hot back-edge whose OSR
//! leaf is compiling interprets on without latching its frame, and a later
//! wrap transfers once the leaf is installed. T-O1 and T-O2.

use std::sync::atomic::Ordering;

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::vm::Vm;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::compile_pipeline_tests::{corpus, function};
use crate::emacs_core::jit::compile::force_deopt_for_test;
use crate::emacs_core::jit::force_osr_for_test;
use crate::emacs_core::value::Value;

/// `(lambda (n) (let ((i 0)) (while (< i n) (setq i (1+ i))) i))`, already
/// hot, so its first 256-iteration wrap asks for an OSR leaf.
fn counting_loop() -> ByteCodeFunction {
    let (ops, constants, arity) = corpus().swap_remove(3);
    let f = function(ops, constants, arity);
    f.jit_runtime().set_hot_for_test();
    f
}

fn count(ctx: &mut Context, f: &ByteCodeFunction, n: i64) -> Value {
    Vm::from_context(ctx)
        .execute(f, vec![Value::make_int(n)])
        .expect("runs")
}

fn transfers() -> u64 {
    cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed)
}

fn deferred_install() {
    force_mode_for_test(Some(BgMode::Sync));
    force_deferred_install_for_test(true);
    force_osr_for_test(true);
    force_deopt_for_test(false);
}

fn done() {
    hold_publish_for_test(false);
    force_deferred_install_for_test(false);
    force_osr_for_test(false);
    force_mode_for_test(None);
}

/// T-O1: the first hot wrap defers the OSR leaf's backend and interprets
/// on; the next wrap finds it finished, installs it and transfers.
#[test]
fn jit_bg_pending_osr_transfers_at_a_later_wrap() {
    deferred_install();
    let mut ctx = Context::new();
    let f = counting_loop();
    let (before, enqueued) = (
        transfers(),
        stats_snapshot().enqueued[JobClass::Osr as usize],
    );
    assert_eq!(count(&mut ctx, &f, 100_000), Value::make_int(100_000));
    assert_eq!(transfers() - before, 1, "transferred after the install");
    let stats = stats_snapshot();
    assert_eq!(stats.enqueued[JobClass::Osr as usize], enqueued + 1);
    assert!(stats.installed[JobClass::Osr as usize] >= 1);
    done();
}

/// T-O1, the latch: while the OSR leaf stays unfinished every hot wrap
/// looks again (a latched frame would stop looking) and the loop finishes
/// interpreted with the right answer; the next call's first wrap installs
/// and transfers.
#[test]
fn jit_bg_pending_osr_never_latches_the_frame() {
    deferred_install();
    hold_publish_for_test(true);
    let mut ctx = Context::new();
    let f = counting_loop();
    let (before, waits) = (transfers(), stats_snapshot().osr_waits);
    assert_eq!(count(&mut ctx, &f, 100_000), Value::make_int(100_000));
    assert_eq!(transfers(), before, "nothing to transfer into");
    assert!(
        stats_snapshot().osr_waits - waits > 100,
        "every later wrap asked again: {}",
        stats_snapshot().osr_waits - waits
    );
    assert_eq!(publish_held_for_test(), 1);
    assert_eq!(count(&mut ctx, &f, 100_000), Value::make_int(100_000));
    assert_eq!(transfers() - before, 1, "the next call transferred");
    done();
}

/// A pending OSR leaf's constants are GC roots; the handshake probe counts
/// them; evicting the source drops it.
#[test]
fn jit_bg_pending_osr_is_rooted_and_evicted_with_its_source() {
    deferred_install();
    hold_publish_for_test(true);
    let mut ctx = Context::new();
    let f = counting_loop();
    assert_eq!(count(&mut ctx, &f, 1_000), Value::make_int(1_000));
    let id = f.jit_runtime().compiled_id().expect("id");
    assert!(pending_count() >= 1);
    assert!(cache::osr_compile_in_flight(&f, 1));
    cache::evict_compiled(id);
    assert!(!cache::osr_compile_in_flight(&f, 1));
    assert_eq!(pending_count(), 0);
    done();
}

/// T-O1 with the worker: a long one-shot loop requests its OSR leaf, keeps
/// interpreting while the worker compiles it, and finishes natively.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn jit_bg_worker_osr_transfers_while_the_loop_runs() {
    force_mode_for_test(Some(BgMode::Threaded));
    force_osr_for_test(true);
    force_deopt_for_test(false);
    let mut ctx = Context::new();
    let f = counting_loop();
    let before = transfers();
    assert_eq!(count(&mut ctx, &f, 20_000_000), Value::make_int(20_000_000));
    assert_eq!(transfers() - before, 1, "the loop moved to native code");
    assert!(stats_snapshot().installed[JobClass::Osr as usize] >= 1);
    force_osr_for_test(false);
    force_mode_for_test(None);
}

/// T-O2: a hot loop that calls a frame-watching callee shows the same
/// frames before its OSR leaf is installed (interpreted) and after (native).
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn jit_bg_osr_install_keeps_the_frames() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(
        r#"(progn
  (defvar neovm--bgo-seen nil)
  (defun neovm--bgo-probe (i)
    (when (memq i '(3 1999997))
      (push (list i (backtrace-frame 1 'neovm--bgo-probe)) neovm--bgo-seen))
    1)
  (defun neovm--bgo-loop (n)
    (let ((i 0) (s 0))
      (while (< i n) (setq s (+ s (neovm--bgo-probe i))) (setq i (1+ i)))
      s))
  (byte-compile 'neovm--bgo-probe)
  (byte-compile 'neovm--bgo-loop))"#,
    )
    .expect("defined");
    force_mode_for_test(Some(BgMode::Threaded));
    force_osr_for_test(true);
    force_deopt_for_test(false);
    let before = transfers();
    let got = ev.eval_str("(neovm--bgo-loop 2000000)").expect("runs");
    assert_eq!(got, Value::make_int(2_000_000));
    assert_eq!(transfers() - before, 1, "the loop moved to native code");
    let seen = ev.eval_str("(reverse neovm--bgo-seen)").expect("seen");
    assert_eq!(
        crate::emacs_core::print::print_value(&seen),
        "((3 (t neovm--bgo-loop 2000000)) (1999997 (t neovm--bgo-loop 2000000)))",
        "the same caller frame interpreted and native"
    );
    force_osr_for_test(false);
    force_mode_for_test(None);
}

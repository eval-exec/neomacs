//! The background-compile soak (P2.4 B13): a Lisp program run first on
//! its interpreted definitions, then byte-compiled under `NEOVM_JIT_BG=on`
//! with the stress knob (every worker job 0-2 ms late, every install 1-4
//! probes late), must give the same answer at every iteration: entry
//! tier-ups, first-sight callees and OSR loops go pending and install at
//! random points, while a callee is redefined mid-run and the collector
//! runs.

use std::time::Duration;

use super::*;
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::compile::{force_deopt_for_test, force_profit_gate_for_test};
use crate::emacs_core::jit::force_osr_for_test;
use crate::emacs_core::print::print_value;

const DEFINITIONS: &str = r#"(progn
  (defvar bgs-dyn 3)
  (defun bgs-fib (n) (if (< n 2) n (+ (bgs-fib (- n 1)) (bgs-fib (- n 2)))))
  (defun bgs-floats (n)
    (let ((s 0.0) (i 0))
      (while (< i n) (setq s (+ s (* i 0.5))) (setq i (1+ i)))
      s))
  (defun bgs-squares (xs) (mapcar (lambda (x) (* x x)) xs))
  (defun bgs-adder (k) (let ((f (lambda (x) (+ x k)))) (funcall f 10)))
  (defun bgs-strings (n)
    (let ((acc nil))
      (dotimes (i n) (push (format "%d-%s" i (make-string (% i 5) ?a)) acc))
      (length (apply #'concat acc))))
  (defun bgs-table (n)
    (let ((h (make-hash-table :test 'equal)) (s 0))
      (dotimes (i n) (puthash (number-to-string i) i h))
      (maphash (lambda (_k v) (setq s (+ s v))) h)
      s))
  (defun bgs-catch (n) (catch 'done (dotimes (i n) (when (= i 7) (throw 'done i))) -1))
  (defun bgs-divide (x) (condition-case err (/ 10 x) (arith-error (list 'div0 (car err)))))
  (defun bgs-dyn-read () bgs-dyn)
  (defun bgs-dyn-bind (n) (let ((bgs-dyn n)) (bgs-dyn-read)))
  (defun bgs-conses (n)
    (let ((l nil))
      (dotimes (i n) (push (cons i (* i i)) l))
      (apply #'+ (mapcar #'cdr l))))
  (defun bgs-callee (x) (if (> x 1000000) 0 (+ x 1)))
  (defun bgs-caller (x) (* 2 (bgs-callee x)))
  (defun bgs-step (i)
    (when (= i 330) (defalias 'bgs-callee (lambda (x) (+ x 2))))
    (when (= (% i 97) 0) (garbage-collect))
    (list (bgs-fib 9) (bgs-floats 600) (bgs-squares (list i 2 3)) (bgs-adder i)
          (bgs-strings 6) (bgs-table 12) (bgs-catch 20) (bgs-divide (% i 3))
          (bgs-dyn-bind i) (bgs-dyn-read) (bgs-conses 40) (bgs-caller i)))
  (defun bgs-drive (n)
    ;; The iterations at which the answer changed, with the answer.
    (let ((log nil) (last nil) (i 0))
      (while (< i n)
        (let ((r (bgs-step i)))
          (unless (equal r last) (push (cons i r) log) (setq last r)))
        (setq i (1+ i)))
      (nreverse log))))"#;

const FUNCTIONS: [&str; 15] = [
    "bgs-fib",
    "bgs-floats",
    "bgs-squares",
    "bgs-adder",
    "bgs-strings",
    "bgs-table",
    "bgs-catch",
    "bgs-divide",
    "bgs-dyn-read",
    "bgs-dyn-bind",
    "bgs-conses",
    "bgs-callee",
    "bgs-caller",
    "bgs-step",
    "bgs-drive",
];

/// Functions whose first call reaches the native dispatch threshold.
const PRIMED: [&str; 4] = ["bgs-drive", "bgs-step", "bgs-floats", "bgs-caller"];

const ITERATIONS: u32 = 450;

#[test]
fn jit_bg_stress_soak_matches_the_interpreter() {
    // A collection per allocation makes this program hours long in a debug
    // build; the collector's side of the soak is the test below.
    if std::env::var("NEOVM_GC_STRESS").as_deref() == Ok("1") {
        return;
    }
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(DEFINITIONS).expect("defined");
    let drive = format!("(bgs-drive {ITERATIONS})");
    let reference = print_value(&ev.eval_str(&drive).expect("interpreted run"));
    // Byte-compile everything and restore the callee the run redefines.
    // Prime the driver, step and floating-point loop for background entry
    // tier-ups, and the caller for native dispatch on its first call. The
    // remaining bytecode functions start cold for first-sight compiles.
    ev.eval_str(DEFINITIONS).expect("redefined");
    for name in FUNCTIONS {
        ev.eval_str(&format!("(byte-compile '{name})"))
            .expect("byte-compiled");
        if !PRIMED.contains(&name) {
            continue;
        }
        let function = ev
            .obarray
            .symbol_function_id(intern(name))
            .expect("defined");
        let bc = function.get_bytecode_data().expect("byte-code");
        bc.jit_runtime()
            .set_heat_for_test(crate::emacs_core::jit::hot_threshold().saturating_sub(1));
    }
    // The call-dominated driver must compile to exercise first-sight jobs.
    // Production profitability deferral can correctly keep it interpreted
    // for this whole run; this fixture tests pending/install correctness.
    force_profit_gate_for_test(false);
    // A native caller must reach a cold callee before the soak warms it.
    // Depending on worker scheduling to install the caller first made the
    // FirstSight coverage assertion fail in optimized test builds. Compile
    // only this caller synchronously, without running it or inlining its
    // callee; the driver, step and loops still tier up during the soak.
    force_mode_for_test(Some(BgMode::Sync));
    force_deopt_for_test(false);
    crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
    let caller = ev
        .obarray
        .symbol_function_id(intern("bgs-caller"))
        .expect("caller defined");
    let caller_bc = caller.get_bytecode_data().expect("caller byte-compiled");
    assert!(
        crate::emacs_core::jit::cache::resolve_compiled_leaf_ptr(&mut ev, caller_bc).is_some(),
        "caller installed without executing its callee"
    );
    crate::emacs_core::jit::inline::force_inline_for_test(None);
    force_mode_for_test(Some(BgMode::Threaded));
    force_stress_for_test(Some(true));
    force_osr_for_test(true);
    let before = stats_snapshot();
    let compiled = print_value(&ev.eval_str(&drive).expect("compiled run"));
    assert!(quiesce_for_test(Duration::from_secs(60)));
    let after = stats_snapshot();
    force_osr_for_test(false);
    force_stress_for_test(None);
    force_mode_for_test(None);
    force_profit_gate_for_test(true);
    assert_eq!(
        compiled, reference,
        "the same answers at the same iterations"
    );
    // Which classes the run reaches depends on the tier-up threshold (a
    // callee left cold is what a first-sight compile needs).
    let default_threshold =
        crate::emacs_core::jit::hot_threshold() == crate::emacs_core::jit::Runtime::HOT_THRESHOLD;
    for class in [JobClass::Osr, JobClass::FirstSight, JobClass::Entry]
        .into_iter()
        .filter(|_| default_threshold)
    {
        assert!(
            after.enqueued[class as usize] > before.enqueued[class as usize],
            "{class:?} compiles went to the worker: {after:?}"
        );
    }
    let enqueued: u64 = after.enqueued.iter().sum::<u64>() - before.enqueued.iter().sum::<u64>();
    let installed: u64 = after.installed.iter().sum::<u64>() - before.installed.iter().sum::<u64>();
    assert!(
        enqueued >= 5,
        "the soak compiled in the background: {after:?}"
    );
    assert!(installed >= 3, "and installed: {after:?}");
    assert!(
        after.pending_probes > before.pending_probes,
        "calls found jobs still running"
    );
}

/// The soak's collector side, on a bare Context small enough to collect at
/// every allocation (`NEOVM_GC_STRESS=1` makes it so; `gc_stress` here
/// does it either way): functions whose only reference to a constant list
/// is their leaf tier up through the worker while the eval thread keeps
/// collecting; every call, interpreted, pending or native, returns the
/// intact list.
#[test]
fn jit_bg_worker_installs_under_a_collection_per_allocation() {
    use crate::emacs_core::bytecode::Op;
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::jit::compile::compile_pipeline_tests::function;
    use crate::emacs_core::value::Value;
    force_mode_for_test(Some(BgMode::Threaded));
    force_stress_for_test(Some(true));
    force_deopt_for_test(false);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let mut functions = Vec::new();
    for i in 0..6 {
        let list = ev
            .eval_str(&format!(
                "(list 'jit-bg-gc-{i} (make-string {i} ?z) (cons {i} {i}))"
            ))
            .expect("list");
        let symbol = format!("jit-bg-gc-fn-{i}");
        let name = intern(&symbol);
        let f = function(vec![Op::Constant(0), Op::Return], vec![list], 0);
        // Bound, so the heap object (and through it the list) stays alive.
        ev.obarray
            .set_symbol_function_id(name, Value::make_bytecode(f));
        let expected = format!("(jit-bg-gc-{i} \"{}\" ({i} . {i}))", "z".repeat(i));
        functions.push((symbol, name, expected));
    }
    ev.gc_stress = true;
    let collections = ev.tagged_heap.gc_collections();
    for round in 0..40 {
        for (symbol, name, expected) in &functions {
            let bc = ev
                .obarray
                .symbol_function_id(*name)
                .and_then(|f| f.get_bytecode_data())
                .expect("bound");
            let bits = crate::emacs_core::jit::try_run_compiled(ctx, bc, Value::NIL, &[])
                .expect("no signal");
            let value = match bits {
                Some(bits) => Value::from_bits(bits),
                // Interpreted this call (cold, or pending).
                None => ev
                    .apply(Value::symbol(symbol), vec![])
                    .expect("interpreted"),
            };
            assert_eq!(&print_value(&value), expected, "round {round}");
            ev.eval_str("(make-list 64 (cons 1 2))").expect("churn");
        }
        if round == 5 {
            assert!(quiesce_for_test(Duration::from_secs(60)));
        }
    }
    ev.gc_stress = false;
    assert!(
        ev.tagged_heap.gc_collections() >= collections + 240,
        "a collection at least per call: {} -> {}",
        collections,
        ev.tagged_heap.gc_collections()
    );
    let stats = stats_snapshot();
    assert!(stats.installed[JobClass::Entry as usize] >= 6, "{stats:?}");
    force_stress_for_test(None);
    force_mode_for_test(None);
}

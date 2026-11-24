//! Compiled bytecode execution in worker-owned Lisp contexts.

use crate::isolate::{self, IsolateBoot, IsolatePoolConfig};
use crate::lisp;
use neovm_core::emacs_core::eval::Context;
use neovm_core::emacs_core::load::apply_runtime_startup_state;
use neovm_core::emacs_core::load_boot::create_bootstrap_evaluator_cached;
use neovm_core::emacs_core::print::format_eval_result_with_eval;

fn main_isolate() -> Box<Context> {
    let _ = isolate::install(IsolatePoolConfig {
        threads: 2,
        queue_capacity: 64,
        boot: IsolateBoot::CachedBootstrap,
    });
    let mut ctx = Box::new(create_bootstrap_evaluator_cached().expect("bootstrap evaluator"));
    ctx.setup_thread_locals();
    apply_runtime_startup_state(&mut ctx).expect("runtime startup state");
    lisp::register_subrs(&mut ctx);
    ctx
}

fn eval(ctx: &mut Context, source: &str) -> String {
    let result = ctx.eval_str(source);
    format_eval_result_with_eval(ctx, &result)
}

#[test]
fn bytecode_vm_compiles_and_runs_inside_worker() {
    let mut ctx = main_isolate();
    assert_eq!(
        eval(
            &mut ctx,
            "(neomacs-worker-wait (neomacs-worker-run (lambda () (let ((fn (byte-compile (lambda (n) (let ((sum 0)) (dotimes (i n) (setq sum (+ sum i))) sum))))) (list (byte-code-function-p fn) (funcall fn 100))))))",
        ),
        "OK (t 4950)"
    );
}

#[test]
fn bytecode_function_value_crosses_into_worker() {
    let mut ctx = main_isolate();
    assert_eq!(
        eval(
            &mut ctx,
            "(progn \
               (defun s6-main-bytecode (n) (let ((sum 0)) \
                 (dotimes (i n) (setq sum (+ sum i))) sum)) \
               (let ((fn (byte-compile 's6-main-bytecode))) \
                 (list (byte-code-function-p fn) \
                       (neomacs-worker-wait (neomacs-worker-run fn 100)))))",
        ),
        "OK (t 4950)"
    );
}

#[test]
fn hot_bytecode_loop_executes_in_worker_for_jit_observation() {
    let mut ctx = main_isolate();
    assert_eq!(
        eval(
            &mut ctx,
            "(neomacs-worker-wait (neomacs-worker-run (lambda () (defun s6-worker-jit-witness (n) (let ((sum 0)) (dotimes (i n) (setq sum (+ sum i))) sum)) (let ((fn (byte-compile 's6-worker-jit-witness))) (fset 's6-worker-jit-witness fn) (list (byte-code-function-p fn) (s6-worker-jit-witness 3000000))))))",
        ),
        "OK (t 4499998500000)"
    );
}

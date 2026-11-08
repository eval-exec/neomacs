//! Public `profiler.el` memory workflow.
//!
//! Re-homed from neovm-eval: the subject is the canonical boot image, which
//! lives in core. The assertions are the eval test's assertions.

use crate::emacs_core::Value;

#[test]
fn profiler_el_public_memory_workflow_builds_and_renders_a_report() {
    let mut ctx = crate::emacs_core::load_boot::create_bootstrap_evaluator_cached().unwrap();
    let lisp_root = crate::test_utils::workspace_root()
        .as_path()
        .join("lisp")
        .canonicalize()
        .unwrap();
    let profiler_el = lisp_root.join("profiler.el");
    ctx.obarray_mut().set_symbol_value(
        "load-path",
        Value::list(vec![
            Value::string(lisp_root.to_string_lossy()),
            Value::string(lisp_root.join("emacs-lisp").to_string_lossy()),
        ]),
    );
    let load_result = ctx.eval_str(&format!(
        r#"(load {:?} nil t t)"#,
        profiler_el.to_string_lossy()
    ));
    assert_eq!(crate::emacs_core::format_eval_result(&load_result), "OK t");

    ctx.eval_str("(profiler-start 'mem)").unwrap();
    ctx.eval_str("(make-list 256 'profiled-value)").unwrap();
    ctx.eval_str("(profiler-stop)").unwrap();
    assert!(
        ctx.eval_str(
            "(and (hash-table-p profiler-memory-log) (> (hash-table-count profiler-memory-log) 0))"
        )
        .unwrap()
        .is_truthy()
    );
    assert!(
        ctx.eval_str("(profiler-calltree-p (profiler-calltree-build profiler-memory-log))")
            .unwrap()
            .is_truthy()
    );
    ctx.eval_str("(profiler-report)").unwrap();
}

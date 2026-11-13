//! Re-homed car/cdr split BOOT-FULL oracle (jitcall BOOT-FULL batch 3): the
//! guarded `car`/`cdr` split's hot cadr loop against GNU, moved whole from
//! eval's jit/tests/car_cdr_split_test.rs (the file's two CONTEXT-FREE tests
//! stay in eval).
use crate::emacs_core::print::print_value;

#[test]
fn hot_cadr_loop_keeps_gnu_results_for_cons_nil_and_non_list() {
    let mut ctx = crate::test_utils::runtime_startup_context();
    ctx.eval_str(
        "(defun jit-car-cdr-split-cadrs (l)
           (let (acc)
             (while l
               (setq acc (cons (car (cdr l)) acc))
               (setq l (cdr (cdr l))))
             (nreverse acc)))",
    )
    .expect("defun");
    ctx.eval_str("(byte-compile 'jit-car-cdr-split-cadrs)")
        .expect("byte-compile");
    ctx.eval_str("(dotimes (_ 3000) (jit-car-cdr-split-cadrs '(1 2 3 4 5 6)))")
        .expect("warm up");
    for (list, want) in [
        ("'(1 2 3 4)", "(2 4)"),
        ("'(1 2 3)", "(2 nil)"),
        ("'(1 . 5)", "(wrong-type-argument listp 5)"),
        ("'(1 2 3 . 7)", "(wrong-type-argument listp 7)"),
    ] {
        let form = format!("(condition-case e (jit-car-cdr-split-cadrs {list}) (error e))");
        let got = ctx.eval_str(&form).expect("call");
        assert_eq!(print_value(&got), want, "{list}");
    }
}

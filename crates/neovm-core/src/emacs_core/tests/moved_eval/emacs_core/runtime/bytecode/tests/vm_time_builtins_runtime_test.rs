//! VM time-builtins test re-homed from neovm-eval's VM test module.
//!
//! `float-time` decodes a Lisp time value with the system time suite
//! (`decode_lisp_time`/`frac_to_double`); a seam would force a duplicate time
//! decoder into eval, so the test runs against the runtime that owns the
//! canonical implementation. Core's `new_context` constructs the real
//! surface with the canonical hook-installation proof. The Lisp form and
//! expected string are unchanged.

fn vm_eval_str(src: &str) -> String {
    let mut eval = crate::test_utils::new_context();
    let result = eval.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
}

#[test]
fn vm_time_builtins_use_direct_timefns_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(list
                 (length (current-time))
                 (consp (current-cpu-time))
                 (null (current-idle-time))
                 (consp (get-internal-run-time))
                 (let ((f (float-time '(0 1 500000 0))))
                   (and (> f 1.4) (< f 1.6)))
                 (equal (time-add '(0 1 200000 0) '(0 2 900000 0))
                        '(0 4 100000 0))
                 (equal (time-subtract '(0 3 100000 0) '(0 1 200000 0))
                        '(0 1 900000 0))
                 (time-less-p '(0 1 0 0) '(0 2 0 0))
                 (time-equal-p '(0 1 0 0) '(0 1 0 0))
                 (time-equal-p nil nil)
                 (null (time-equal-p nil '(0 0 0 0)))
                 (time-equal-p 'not-a-time 'not-a-time)
                 (equal (current-time-string '(0 0 0 0) t)
                        "Thu Jan  1 00:00:00 1970")
                 (and (member (car (cdr (current-time-zone nil t))) '("GMT" "UTC")) t)
                 (equal (encode-time '(0 0 0 1 1 1970 nil nil 0))
                        '(0 0))
                 (equal (decode-time '(0 0 0 0) t)
                        '(0 0 0 1 1 1970 4 nil 0))
                 (equal (time-convert '(0 42 0 0) 'integer) 42))"#
        ),
        r#"OK (4 t t t t t t t t t t t t t t t t)"#
    );
}

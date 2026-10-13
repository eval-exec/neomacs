//! `make-byte-code` stores its constant vector verbatim, as GNU
//! `Fmake_byte_code` (alloc.c) does with `Fvector`: list constants shaped
//! like `(make-hash-table-from-literal ...)` stay ordinary lists.
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

#[test]
fn make_byte_code_keeps_literal_shaped_constants_verbatim() {
    // A bare Context has no macros (no dotimes/unless), so the form sticks
    // to special forms and subrs.
    let mut ctx = Context::new();
    // GNU 31.1 answers (t t t make-hash-table-from-literal) for this form.
    let result = ctx
        .eval_str(
            "(let* ((circular (list 'mbl-head 1))
                    (_ (setcdr (cdr circular) circular))
                    (literal '(make-hash-table-from-literal
                               '(hash-table test eq data (mbl-k 1))))
                    (vec (vector
                          literal
                          (list (make-symbol \"make-hash-table-from-literal\")
                                ''(hash-table data (mbl-k 2)))
                          '(mbl-other '(hash-table data (mbl-k 3)))
                          '(make-hash-table-from-literal)
                          '(make-hash-table-from-literal . improper)
                          '(1 . 2)
                          circular))
                    (constants (aref (make-byte-code 0 \"\\300\\207\" vec 1) 2)))
               (list (eq constants vec)
                     (let ((all-conses t) (i 0))
                       (while (< i (length constants))
                         (if (consp (aref constants i)) nil (setq all-conses nil))
                         (setq i (1+ i)))
                       all-conses)
                     (eq (aref constants 0) literal)
                     (car (aref constants 0))))",
        )
        .expect("make-byte-code");
    let items = crate::emacs_core::value::list_to_vec(&result).expect("result list");
    assert_eq!(items.len(), 4, "{result:?}");
    assert_eq!(items[0], Value::T, "constants vector identity");
    assert_eq!(items[1], Value::T, "every constant stays a cons");
    assert_eq!(items[2], Value::T, "literal-shaped constant identity");
    assert_eq!(items[3], Value::symbol("make-hash-table-from-literal"));
}

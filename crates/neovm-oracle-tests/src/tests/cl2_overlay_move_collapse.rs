//! GNU-refreshed deletion-collapse observations, with process-isolated A/B.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

const FORM: &str = r#"
(let (out)
  (dolist (text (list (make-string 60 ?a)
                      (make-string 60 ?ж)
                      (apply #'unibyte-string (make-list 60 255))
                      (string-as-multibyte (apply #'unibyte-string (make-list 60 255)))))
    (with-temp-buffer
      (set-buffer-multibyte (multibyte-string-p text))
      (insert text)
      (let ((a (make-overlay 10 30 nil t t))
            (b (make-overlay 20 30 nil t t))
            (moving (make-overlay 40 45 nil t t)))
        (dolist (item (list (cons a 'a) (cons b 'b) (cons moving 'moving)))
          (overlay-put (car item) 'tag (cdr item))
          (overlay-put (car item) 'payload (cdr item)))
        (overlay-put a 'priority 1)
        (overlay-put b 'priority 2)
        (overlay-put moving 'priority 3)
        (delete-region 5 25)
        (move-overlay moving 3 10)
        (move-overlay moving 5 10)
        ;; This insertion exposes GNU's pre-order-dependent reattachment.
        (goto-char 5) (insert "X")
        (push (list (multibyte-string-p text)
                    (mapcar (lambda (ov)
                              (list (overlay-get ov 'tag) (overlay-start ov) (overlay-end ov)))
                            (overlays-at 6))
                    (get-char-property 6 'payload)
                    (next-overlay-change 5) (previous-overlay-change 12)) out))))
  (nreverse out))
"#;

#[test]
fn oracle_prop_cl2_overlay_move_deletion_collapse_knob_off() {
    // SAFETY: nextest isolates tests in separate processes. Set process-only
    // policy before runtime initialization reads the once-published knob.
    unsafe { std::env::set_var("NEOVM_OVERLAY_LOCAL_MOVE", "off") };
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((nil ((a 6 11) (moving 6 11) (b 6 11)) moving 6 11) (t ((a 6 11) (moving 6 11) (b 6 11)) moving 6 11) (nil ((a 6 11) (moving 6 11) (b 6 11)) moving 6 11) (t ((a 6 11) (moving 6 11) (b 6 11)) moving 6 11))""#
    ]];
    crate::common::assert_oracle_parity_expect(FORM, expect);
}

#[test]
fn oracle_prop_cl2_overlay_move_deletion_collapse_knob_on() {
    // SAFETY: nextest isolates tests in separate processes. Each arm must
    // initialize its own policy; Lisp setenv cannot reset this OnceLock.
    unsafe { std::env::set_var("NEOVM_OVERLAY_LOCAL_MOVE", "on") };
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((nil ((a 6 11) (moving 6 11) (b 6 11)) moving 6 11) (t ((a 6 11) (moving 6 11) (b 6 11)) moving 6 11) (nil ((a 6 11) (moving 6 11) (b 6 11)) moving 6 11) (t ((a 6 11) (moving 6 11) (b 6 11)) moving 6 11))""#
    ]];
    crate::common::assert_oracle_parity_expect(FORM, expect);
}

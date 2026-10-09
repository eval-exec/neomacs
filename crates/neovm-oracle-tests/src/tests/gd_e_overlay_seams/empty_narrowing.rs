use super::*;

#[test]
fn oracle_prop_gde_review_empty_narrowing_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((default-text-properties '(face italic x fallback q default-q)) out)
  (with-temp-buffer
    (insert "abcdef")
    (put-text-property 1 7 'face 'bold)
    (put-text-property 4 7 'x 1)
    (dolist (bounds '((1 1) (3 3) (7 7) (3 4)))
      (save-restriction
        (narrow-to-region (car bounds) (cadr bounds))
        (let ((pos (point-max)))
          (push (list bounds (text-properties-at pos)
                      (get-text-property pos 'face)
                      (get-char-property pos 'face)
                      (get-char-property-and-overlay pos 'face)) out))))
    (let ((ov (make-overlay 3 4)))
      (overlay-put ov 'q 5)
      (save-restriction
        (narrow-to-region 3 3)
        (dolist (value '(nil overlay))
          (overlay-put ov 'face value)
          (let ((pair (get-char-property-and-overlay 3 'face)))
            (push (list value (text-properties-at 3)
                        (get-text-property 3 'face) (get-char-property 3 'face)
                        (car pair) (eq (cdr pair) ov)
                        (get-text-property 3 'q) (get-char-property 3 'q)) out))))))
  (nreverse out))"#;
    let expected = expect_test::expect![[
        r#""OK (((1 1) nil italic italic (italic)) ((3 3) nil italic italic (italic)) ((7 7) nil italic italic (italic)) ((3 4) (x 1 face bold) bold bold (bold)) (nil nil italic italic italic nil default-q 5) (overlay nil italic overlay overlay t default-q 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expected);
}

#[test]
fn oracle_prop_gde_review_empty_narrowing_compiled_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((f (byte-compile
          (lambda (pos object)
            (list (text-properties-at pos object)
                  (get-text-property pos 'face object)
                  (get-char-property pos 'face object)
                  (car (get-char-property-and-overlay pos 'face object)))))))
  (with-temp-buffer
    (insert "aж😀z")
    (put-text-property 1 5 'face 'bold)
    (let ((default-text-properties '(face italic front-sticky t)))
      (narrow-to-region 2 2)
      (dotimes (_ 48) (funcall f 2 (current-buffer)))
      (list (funcall f 2 nil) (funcall f (copy-marker 2) (current-buffer))
            (get-pos-property 2 'face)
            (condition-case err (funcall f 1 nil) (error (car err)))
            (condition-case err (funcall f 3 nil) (error (car err)))))))"#;
    let expected = expect_test::expect![[
        r#""OK ((nil italic italic italic) (nil italic italic italic) italic args-out-of-range args-out-of-range)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expected);
}

use super::*;

#[test]
fn oracle_prop_gde_overlay_missing_overlay_property_falls_back_to_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((default-text-properties '(probe default)) out)
  (with-temp-buffer
    (insert "abcdef")
    (put-text-property 2 5 'probe 'text)
    (let ((a (make-overlay 1 7)) (b (make-overlay 2 6)))
      (overlay-put a 'face 'bold)
      (overlay-put a 'priority 9)
      (overlay-put b 'probe nil)
      (overlay-put b 'priority '(99 . 99))
      (dolist (pos '(1 2 3 5 7))
        (let ((pair (get-char-property-and-overlay pos 'probe)))
          (push (list pos (get-char-property pos 'probe) (car pair) (overlayp (cdr pair))) out)))))
  (nreverse out))"#;
    let expected = expect_test::expect![[
        r#""OK ((1 default default nil) (2 text text nil) (3 text text nil) (5 default default nil) (7 default default nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expected);
}

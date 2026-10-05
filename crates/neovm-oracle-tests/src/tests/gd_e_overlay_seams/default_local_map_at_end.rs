use super::*;

#[test]
fn oracle_prop_gde_overlay_default_local_map_at_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((property-map (make-sparse-keymap)) (buffer-map (make-sparse-keymap)))
  (define-key property-map "x" #'ignore)
  (define-key buffer-map "x" #'forward-char)
  (with-temp-buffer
    (insert "a")
    (goto-char (point-max))
    (use-local-map buffer-map)
    (let ((default-text-properties (list 'local-map property-map 'rear-nonsticky t)))
      (list (key-binding "x")
            (not (null (memq property-map (current-active-maps))))
            (null (get-pos-property (point-max) 'local-map))))))"#;
    let expected = expect_test::expect![[r#""OK (ignore t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expected);
}

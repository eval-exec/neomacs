use super::*;

#[test]
fn oracle_prop_gde_overlay_end_default_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((default-text-properties '(probe fallback front-sticky t)))
(list (let ((s "a")) (list (get-text-property 1 'probe s)
(get-char-property 1 'probe s) (get-char-property-and-overlay 1 'probe s)))
(with-temp-buffer (insert "ab")
(list (get-text-property (point-max) 'probe) (get-char-property (point-max) 'probe)
(get-char-property-and-overlay (point-max) 'probe)
(progn (overlay-put (make-overlay 1 3) 'face 'bold)
(list (get-char-property 3 'probe) (get-char-property-and-overlay 3 'probe)))
(save-restriction (narrow-to-region 1 2) (get-char-property (point-max) 'probe))))))"#;
    let expected = expect_test::expect![[
        r#""OK ((fallback fallback (fallback)) (fallback fallback (fallback) (fallback (fallback)) fallback))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expected);
}

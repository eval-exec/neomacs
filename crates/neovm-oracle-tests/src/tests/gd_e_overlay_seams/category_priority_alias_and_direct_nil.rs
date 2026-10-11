use super::*;

#[test]
fn oracle_prop_gde_overlay_category_priority_alias_and_direct_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((char-property-alias-alist '((probe alias))) out)
  (unwind-protect
      (progn
        (put 'cl2-oracle-category 'probe 'category-value)
        (put 'cl2-oracle-category 'priority '(2 . 3))
        (with-temp-buffer
          (insert "abcdef")
          (let ((a (make-overlay 1 6)) (b (make-overlay 1 6)) (c (make-overlay 1 6)))
            (overlay-put a 'category 'cl2-oracle-category)
            (overlay-put b 'probe 'plain)
            (overlay-put b 'priority 1)
            (overlay-put c 'alias 'aliased)
            (overlay-put c 'priority 3)
            (push (get-char-property 3 'probe) out)
            (overlay-put c 'probe nil)
            (push (get-char-property 3 'probe) out)
            (put 'cl2-oracle-category 'priority 5)
            (push (get-char-property 3 'probe) out)
            (overlay-put a 'priority 0)
            (push (get-char-property 3 'probe) out)
            (overlay-put a 'priority 7)
            (overlay-put a 'probe nil)
            (push (get-char-property 3 'probe) out)))
        (nreverse out))
    (setplist 'cl2-oracle-category nil)))"#;
    let expected =
        expect_test::expect![[r#""OK (aliased category-value category-value plain plain)""#]];
    crate::common::assert_oracle_parity_expect(form, expected);
}

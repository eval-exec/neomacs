use super::*;

#[test]
fn oracle_prop_gde_overlay_priority_components_and_range_rules() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let (out)
  (dolist (spec '(((1 . 0) (0 . 99) (0 . 100))
                  ((0 . 8) (0 . 2) (0 . 4))
                  (nil (nil . 3) (bad . 2))
                  ((2 . bad) (1 . 99) 3)
                  (-2 -1 nil)))
    (with-temp-buffer
      (insert "abcdefghijkl")
      (let ((a (make-overlay 1 10)) (b (make-overlay 2 8)) (c (make-overlay 3 11)))
        (dolist (item (list (list a 'a (nth 0 spec)) (list b 'b (nth 1 spec)) (list c 'c (nth 2 spec))))
          (overlay-put (nth 0 item) 'probe (nth 1 item))
          (overlay-put (nth 0 item) 'priority (nth 2 item)))
        (push (list spec (get-char-property 5 'probe)
                    (get-char-property 2 'probe) (get-char-property 10 'probe)) out))))
  (nreverse out))"#;
    let expected = expect_test::expect![[
        r#""OK ((((1 . 0) (0 . 99) (0 . 100)) a a c) (((0 . 8) (0 . 2) (0 . 4)) c b c) ((nil (nil . 3) (bad . 2)) b b c) (((2 . bad) (1 . 99) 3) c a c) ((-2 -1 nil) c b c))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expected);
}

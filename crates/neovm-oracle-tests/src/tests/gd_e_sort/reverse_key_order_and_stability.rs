use super::*;

#[test]
fn oracle_sort_reverse_key_order_and_stability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((out nil))
      (dolist (seq '([3 1 2] (3 1 2) [1] nil))
        (let ((seen nil))
          (push (list (sort seq :reverse t :key (lambda (x) (push x seen) x))
                      (nreverse seen)) out)))
      (push (sort '((1 . a) (2 . b) (1 . c) (2 . d)) :key #'car :reverse t) out)
      (nreverse out))"#;
    assert_oracle_parity_under_envs_expect(
        form,
        ENVS,
        expect_test::expect![[
            r#""OK (([3 2 1] (2 1 3)) ((3 2 1) (2 1 3)) ([1] nil) (nil nil) ((2 . b) (2 . d) (1 . a) (1 . c)))""#
        ]],
    );
}

use super::*;

#[test]
fn oracle_sort_in_place_signal_and_callback_visibility() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((out nil))
      (dolist (compiled '(nil t))
        (dolist (in-place '(nil t))
          (dolist (reverse '(nil t))
            (dolist (stop '(1 3 4 6 9))
              (let* ((v (vector 3 2 1 5 4)) (n 0) (seen nil)
                     (pred (lambda (a b)
                       (setq n (1+ n)) (push (copy-sequence v) seen)
                       (when (= n stop) (error "stop")) (< a b))))
                (when compiled (setq pred (byte-compile pred)))
                (let ((result (condition-case err
                     (sort v :lessp pred :in-place in-place :reverse reverse)
                     (error (car err)))))
                  (push (list compiled in-place reverse stop result v (nreverse seen)) out)))))))
      (nreverse out))"#;
    assert_oracle_parity_under_envs_expect(
        form,
        ENVS,
        expect_test::expect![[
            r#""OK ((nil nil nil 1 error [3 2 1 5 4] ([3 2 1 5 4])) (nil nil nil 3 error [3 2 1 5 4] ([3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4])) (nil nil nil 4 error [3 2 1 5 4] ([3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4])) (nil nil nil 6 error [3 2 1 5 4] ([3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4])) (nil nil nil 9 [1 2 3 4 5] [3 2 1 5 4] ([3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4])) (nil nil t 1 error [3 2 1 5 4] ([3 2 1 5 4])) (nil nil t 3 error [3 2 1 5 4] ([3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4])) (nil nil t 4 error [3 2 1 5 4] ([3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4])) (nil nil t 6 error [3 2 1 5 4] ([3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4])) (nil nil t 9 [5 4 3 2 1] [3 2 1 5 4] ([3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4])) (nil t nil 1 error [3 2 1 5 4] ([3 2 1 5 4])) (nil t nil 3 error [3 2 1 5 4] ([3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4])) (nil t nil 4 error [1 2 3 5 4] ([3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4] [1 2 3 5 4])) (nil t nil 6 error [1 2 3 5 4] ([3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4] [1 2 3 5 4] [1 2 3 5 4] [1 2 3 5 4])) (nil t nil 9 [1 2 3 4 5] [1 2 3 4 5] ([3 2 1 5 4] [3 2 1 5 4] [3 2 1 5 4] [1 2 3 5 4] [1 2 3 5 4] [1 2 3 5 4] [1 2 3 5 4])) (nil t t 1 error [4 5 1 2 3] ([4 5 1 2 3])) (nil t t 3 error [4 5 1 2 3] ([4 5 1 2 3] [4 5 1 2 3] [4 5 1 2 3])) (nil t t 4 error [4 5 1 2 3] ([4 5 1 2 3] [4 5 1 2 3] [4 5 1 2 3] [4 5 1 2 3])) (nil t t 6 error [1 4 5 2 3] ([4 5 1 2 3] [4 5 1 2 3] [4 5 1 2 3] [4 5 1 2 3] [1 4 5 2 3] [1 4 5 2 3])) (nil t t 9 [5 4 3 2 1] [5 4 3 2 1] ([4 5 1 2 3] [4 5 1 2 3] [4 5 1 2 3] [4 5 1 2 3] [1 4 5 2 3] [1 4 5 2 3] [1 2 4 5 3] [1 2 4 5 3])) (t nil nil 1 error [3 2 1 5 4] nil) (t nil nil 3 error [3 2 1 5 4] nil) (t nil nil 4 error [3 2 1 5 4] nil) (t nil nil 6 error [3 2 1 5 4] nil) (t nil nil 9 [1 2 3 4 5] [3 2 1 5 4] nil) (t nil t 1 error [3 2 1 5 4] nil) (t nil t 3 error [3 2 1 5 4] nil) (t nil t 4 error [3 2 1 5 4] nil) (t nil t 6 error [3 2 1 5 4] nil) (t nil t 9 [5 4 3 2 1] [3 2 1 5 4] nil) (t t nil 1 error [3 2 1 5 4] nil) (t t nil 3 error [3 2 1 5 4] nil) (t t nil 4 error [1 2 3 5 4] nil) (t t nil 6 error [1 2 3 5 4] nil) (t t nil 9 [1 2 3 4 5] [1 2 3 4 5] nil) (t t t 1 error [4 5 1 2 3] nil) (t t t 3 error [4 5 1 2 3] nil) (t t t 4 error [4 5 1 2 3] nil) (t t t 6 error [1 4 5 2 3] nil) (t t t 9 [5 4 3 2 1] [5 4 3 2 1] nil))""#
        ]],
    );
}

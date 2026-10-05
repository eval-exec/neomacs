use super::*;

#[test]
fn oracle_sort_key_resolution_and_frames() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
      (defvar neovm--gde-sort-log nil)
      (let ((out nil))
        (dolist (overrides '(nil ((unused . identity))))
          (let ((internal--compiler-function-overrides overrides))
        (dolist (compiled '(nil t))
          (setq neovm--gde-sort-log nil)
          (defalias 'neovm--gde-sort-alias 'neovm--gde-sort-key)
          (let ((key (lambda (x)
            (when (= x 3)
              (fset 'neovm--gde-sort-key (lambda (y) (- y)))
              (garbage-collect))
            (push (list x (not (null (backtrace-frame 0 'neovm--gde-sort-alias)))) neovm--gde-sort-log)
            x)))
            (fset 'neovm--gde-sort-key (if compiled (byte-compile key) key)))
          (push (list overrides compiled (sort [3 1 2] :key #'neovm--gde-sort-alias)
                      (nreverse neovm--gde-sort-log)) out))))
        (nreverse out)))"#;
    assert_oracle_parity_under_envs_expect(
        form,
        ENVS,
        expect_test::expect![[
            r#""OK ((nil nil [1 2 3] ((3 nil) (1 nil) (2 nil))) (nil t [1 2 3] ((3 nil) (1 nil) (2 nil))) (((unused . identity)) nil [1 2 3] ((3 nil) (1 nil) (2 nil))) (((unused . identity)) t [1 2 3] ((3 nil) (1 nil) (2 nil))))""#
        ]],
    );
}

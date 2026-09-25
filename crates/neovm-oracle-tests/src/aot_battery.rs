//! The AOT battery (P4.2 A7): GNU parity for forms that run loadup Lisp,
//! with Neomacs serving the dump-time preload (`NEOVM_AOT=1`) under a knob
//! matrix, and each run required to have served AOT leaves (`aot_loads > 0`
//! in its JIT statistics). Runs only with `NEOVM_ORACLE_AOT=1` against a
//! binary built with `cargo xtask fresh-build --aot-preload`; see
//! `common::assert_oracle_parity_under_aot_expect`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// Required-only `subr.el` predicates and list/string helpers, the shape
/// the preload serves.
#[test]
fn oracle_aot_battery_subr_helpers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list (xor t nil) (xor 1 2) (ensure-list 3) (ensure-list '(3))
      (fixnump 5) (bignump (expt 2 70)) (booleanp nil) (zerop 0) (zerop 0.0)
      (remq 'a (list 'a 'b 'a 'c)) (remove 2 (list 1 2 3 2))
      (delete-dups (list 1 2 1 3)) (flatten-tree '(1 (2 (3 nil 4)) ((5))))
      (string-to-list "abc") (string-to-vector "ab") (copy-tree '(1 (2 3))))
"#;

    let expect = expect_test::expect![[
        r#""OK (t nil (3) (3) t t t t t (b c) (1 3) (1 2 3) (1 2 3 4 5) (97 98 99) [97 98] (1 (2 3)))""#
    ]];
    crate::common::assert_oracle_parity_under_aot_expect(form, expect);
}

/// The same helpers called often enough to tier up under the JIT knobs, so
/// served AOT leaves and JIT leaves meet in one loop.
#[test]
fn oracle_aot_battery_hot_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((acc nil) (i 0))
  (while (< i 200)
    (push (list (xor (zerop (% i 3)) (zerop (% i 5)))
                (length (flatten-tree (list i (list i) nil))))
          acc)
    (setq i (1+ i)))
  (list (length acc) (car acc) (nth 199 acc)
        (length (delete-dups (mapcar #'car acc)))))
"#;

    let expect = expect_test::expect![[r#""OK (200 (nil 2) (nil 2) 2)""#]];
    crate::common::assert_oracle_parity_under_aot_expect(form, expect);
}

/// Signals raised inside served functions keep GNU's data.
#[test]
fn oracle_aot_battery_signals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list (condition-case err (string-to-list 5) (error err))
      (condition-case err (flatten-tree 1) (error err))
      (condition-case err (zerop 'x) (error err)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument sequencep 5) (1) (wrong-type-argument number-or-marker-p x))""#
    ]];
    crate::common::assert_oracle_parity_under_aot_expect(form, expect);
}

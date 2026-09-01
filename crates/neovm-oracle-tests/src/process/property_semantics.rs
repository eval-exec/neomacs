//! Oracle parity tests for GNU process liveness and process plist helpers.
//!
//! GNU implements `process-live-p`, `process-get`, and `process-put` in
//! `lisp/subr.el` over primitive process status and plist accessors.  Notably,
//! `process-live-p` returns nil for non-process objects, while plist accessors
//! signal `wrong-type-argument processp`.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_process_live_p_non_processes_return_nil_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (process-live-p nil)
 (process-live-p t)
 (process-live-p 42)
 (process-live-p "not-process")
 (process-live-p (current-buffer))
 (process-live-p '(fake process)))
"#;

    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_process_plist_get_put_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((program (or (executable-find "cat") "cat")))
  (let ((p (start-process "neomacs-oracle-process-plist" nil program)))
    (unwind-protect
        (list
         (processp p)
         (process-plist p)
         (process-put p 'alpha 1)
         (process-get p 'alpha)
         (process-put p :keyword '(x y))
         (process-get p :keyword)
         (process-get p 'missing)
         (process-plist p)
         (process-live-p p)
         (progn (delete-process p)
                (process-live-p p)))
      (ignore-errors (delete-process p)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (t nil (alpha 1 :keyword (x y)) 1 (alpha 1 :keyword (x y)) (x y) nil (alpha 1 :keyword (x y)) (run open listen connect stop) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_process_plist_accessors_signal_on_non_process() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err (process-plist nil) (error err))
 (condition-case err (set-process-plist nil '(a 1)) (error err))
 (condition-case err (process-get nil 'a) (error err))
 (condition-case err (process-put nil 'a 1) (error err))
 (condition-case err (process-plist 42) (error err))
 (condition-case err (process-get "x" 'a) (error err)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument processp nil) (wrong-type-argument processp nil) (wrong-type-argument processp nil) (wrong-type-argument processp nil) (wrong-type-argument processp 42) (wrong-type-argument processp \"x\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

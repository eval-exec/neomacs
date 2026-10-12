//! Active catches keep GNU's selected EQ policy through source loading.
use crate::common::{
    assert_oracle_parity_with_shared_tempdir_expect, return_if_neovm_enable_oracle_proptest_not_set,
};

#[test]
fn issue554_oracle_positioned_throw_through_load_restores_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // eval.c:1477 selects the target before1449 unbinds; lisp.h:1316-1323
    // honors positioned tags; lread.c:2338-2340 uses the active handler chain.
    assert_oracle_parity_with_shared_tempdir_expect(
        r#"(let* ((dir (getenv "NEOVM_ORACLE_TEST_TMPDIR"))
                  (file (expand-file-name "issue554-throw-policy.el" dir)))
             (unwind-protect
                 (progn
                   (write-region
                    ";;; -*- lexical-binding: nil -*-\n(let ((symbols-with-pos-enabled t)) (throw (position-symbol 'issue554-load-tag 0) 7))\n"
                    nil file nil 'silent)
                   (let ((symbols-with-pos-enabled nil))
                     (list (catch 'issue554-load-tag
                             (load file nil t t) 'fell-through)
                           symbols-with-pos-enabled)))
               (when (file-exists-p file) (delete-file file))))"#,
        expect_test::expect![[r#""OK (7 nil)""#]],
    );
}

#[test]
fn issue554_oracle_bare_throw_through_load_matches_positioned_catch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity_with_shared_tempdir_expect(
        r#"(let* ((dir (getenv "NEOVM_ORACLE_TEST_TMPDIR"))
                  (file (expand-file-name "issue554-throw-policy.el" dir)))
             (unwind-protect
                 (progn
                   (write-region
                    ";;; -*- lexical-binding: nil -*-\n(let ((symbols-with-pos-enabled t)) (throw 'issue554-load-tag 8))\n"
                    nil file nil 'silent)
                   (let ((symbols-with-pos-enabled nil))
                     (list (catch (position-symbol 'issue554-load-tag 9)
                             (load file nil t t) 'fell-through)
                           symbols-with-pos-enabled)))
               (when (file-exists-p file) (delete-file file))))"#,
        expect_test::expect![[r#""OK (8 nil)""#]],
    );
}

#[test]
fn issue554_oracle_disabled_throw_through_load_still_signals_no_catch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity_with_shared_tempdir_expect(
        r#"(let* ((dir (getenv "NEOVM_ORACLE_TEST_TMPDIR"))
                  (file (expand-file-name "issue554-throw-policy.el" dir)))
             (unwind-protect
                 (progn
                   (write-region
                    ";;; -*- lexical-binding: nil -*-\n(let ((symbols-with-pos-enabled nil)) (throw (position-symbol 'issue554-load-tag 0) 7))\n"
                    nil file nil 'silent)
                   (let ((symbols-with-pos-enabled nil))
                     (list (condition-case err
                               (catch 'issue554-load-tag
                                 (load file nil t t) 'fell-through)
                             (no-catch (car err)))
                           symbols-with-pos-enabled)))
               (when (file-exists-p file) (delete-file file))))"#,
        expect_test::expect![[r#""OK (no-catch nil)""#]],
    );
}

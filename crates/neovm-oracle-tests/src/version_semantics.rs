//! Oracle parity tests for GNU `subr.el` version parsing and comparison.
//!
//! GNU implements `version-to-list`, `version-list-*`, and `version*`
//! comparison functions in Lisp.  These APIs are used by package dependency
//! checks and must preserve GNU's treatment of pre-release markers, snapshots,
//! letter suffixes, and trailing zeroes.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_version_to_list_valid_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (version-to-list ".5")
 (version-to-list "0.9 alpha")
 (version-to-list "0.9AlphA1")
 (version-to-list "0.9snapshot")
 (version-to-list "1.0-git")
 (version-to-list "1.0.7.5")
 (version-to-list "1.0.cvs")
 (version-to-list "1.0PRE2")
 (version-to-list "22.8 Beta3")
 (version-to-list "22.3a")
 (version-to-list "22.3Z"))
"#;

    let expect = expect_test::expect![[
        r#""OK ((0 5) (0 9 -3) (0 9 -3 1) (0 9 -4) (1 0 -4) (1 0 7 5) (1 0 -4) (1 0 -1 2) (22 8 -2 3) (22 3 1) (22 3 26))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_version_to_list_invalid_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(mapcar
 (lambda (ver)
   (condition-case err
       (version-to-list ver)
     (error (list (car err) (cadr err)))))
 '("1.0prepre2" "1.0..7.5" "22.8X3" "alpha3.2" "" "pre1"))
"#;

    let expect = expect_test::expect![[
        r#""OK ((error \"Invalid version syntax: ‘1.0prepre2’\") (error \"Invalid version syntax: ‘1.0..7.5’\") (error \"Invalid version syntax: ‘22.8X3’\") (error \"Invalid version syntax: ‘alpha3.2’ (must start with a number)\") (error \"Invalid version syntax: ‘’ (must start with a number)\") (error \"Invalid version syntax: ‘pre1’ (must start with a number)\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_version_list_comparison_trailing_zeroes_and_prereleases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (version-list-= '(1) '(1 0))
 (version-list-= '(1) '(1 0 0 0))
 (version-list-< '(1 -1) '(1))
 (version-list-< '(1 -2) '(1 -1))
 (version-list-< '(1 -3) '(1 -2))
 (version-list-< '(1 -4) '(1 -3))
 (version-list-< '(1 0 1) '(1 0))
 (version-list-<= '(1 0 0) '(1))
 (version-list-<= '(1 -1) '(1))
 (version-list-not-zero '(0 0 -2 3))
 (version-list-not-zero '(0 0 0)))
"#;

    let expect = expect_test::expect![[r#""OK (t t t t t t nil t t -2 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_version_string_comparison_wrappers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (version= "1" "1.0.0")
 (version< "1pre" "1")
 (version< "1beta" "1pre")
 (version< "1alpha" "1beta")
 (version< "1snapshot" "1alpha")
 (version< "1.0-git" "1.0alpha")
 (version<= "2.4.snapshot" "2.4")
 (version<= "22.8beta3" "22.8")
 (version= "22.3a" "22.3.1")
 (version< "22.3b" "22.3.3"))
"#;

    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_version_dynamic_separator_regexp_and_error_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (let ((version-separator "-"))
   (list
    (version-to-list "-5")
    (version-to-list "1-2-3")
    (version-to-list "1-2-beta3")
    (condition-case err
        (version-to-list "1.2")
      (error (list (car err) (cdr err))))))
 (let ((version-regexp-alist '(("^~$" . -9)
                               ("^[-._+ ]?dev$" . -8))))
   (list
    (version-to-list "1~2")
    (version-to-list "1.dev4")
    (version-list-< '(1 -9 9) '(1 -8))
    (version-list-<= '(1 -8) '(1 -8 0))
    (version-list-= '(1 -8 0 0) '(1 -8))))
 (condition-case err
     (version-to-list nil)
   (error (list (car err) (cdr err))))
 (condition-case err
     (version-list-< '(1 a) '(1 0))
   (error (list (car err) (cdr err))))
 (condition-case err
     (version-list-not-zero '(0 a))
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (((0 5) (1 2 3) (1 2 -2 3) (1 -4 2)) ((1 -9 2) (1 -8 4) t t t) (error (\"Version must be a string\")) (wrong-type-argument (number-or-marker-p a)) (wrong-type-argument (number-or-marker-p a)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

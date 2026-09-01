//! Strict combo oracle probes, batch 230: package/version handling.
//! version-to-list, package-version-join, version-list comparison (<, =, >),
//! and version< / version<= / version= string comparison.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_version_to_list_join_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'package)
(list (version-to-list "1.2.3")
      (version-to-list "1.10")
      (version-to-list "0.0.1")
      (version-to-list "26.1")
      (package-version-join '(1 2 3))
      (package-version-join '(1 0))
      (package-version-join (version-to-list "2.5.7")))
"##;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3) (1 10) (0 0 1) (26 1) \"1.2.3\" \"1.0\" \"2.5.7\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_version_list_compare_string_version_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'package)
(list (version-list-< '(1 2) '(1 3))
      (version-list-< '(1 2) '(1 2))
      (version-list-= '(1 2) '(1 2))
      (version-list-> '(2 0) '(1 9))
      (version< "1.2" "1.10")
      (version< "1.10" "1.2")
      (version< "1.0" "1.0")
      (version<= "1.0" "1.0")
      (version= "1.2.3" "1.2.3")
      (version-list-<= '(1 2) '(1 2 0)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function version-list->)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_package_desc_construct_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'package)
(let ((desc (package-desc-create :name 'probe-pkg
                                  :version '(1 2 3)
                                  :summary "A probe package"
                                  :reqs nil
                                  :kind 'single
                                  :extras nil)))
  (list (package-desc-p desc)
        (package-desc-name desc)
        (package-desc-version desc)
        (package-desc-summary desc)
        (package-desc-kind desc)
        (package-version-join (package-desc-version desc))))
"##;
    let expect = expect_test::expect![[
        r#""OK (t probe-pkg (1 2 3) \"A probe package\" single \"1.2.3\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

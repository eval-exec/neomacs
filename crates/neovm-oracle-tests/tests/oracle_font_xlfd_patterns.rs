//! Portable font-name parsing controls from the pinned GNU implementation.
use super::common;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn xlfd_abbreviations_family_dashes_and_sizes_match_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(mapcar
             (lambda (name)
               (condition-case err
                   (let ((spec (font-spec :name name)))
                     (cons name
                           (mapcar (lambda (prop) (font-get spec prop))
                                   '(:foundry :family :weight :slant :width
                                     :adstyle :registry :size :dpi :spacing :avgwidth))))
                 (error (list name :error (car err)))))
             '("*" "-*" "-*-Hack-*" "*-Hack-bold-r-*-13-*"
               "-*-Hack-130-*" "-*-Hack-reg*-*"
               "-*-Hack-*-iso10646-1"
               "-SRC-Some-Hyphen-Family-regular-normal-normal-*-13-*-*-*-*-0-iso10646-1"
               "*-Hack-regular-normal-normal-*-13-*-*-*-*-0-iso10646-1"
               "-SRC-Hack-regular-normal-normal-*-0-*-*-*-*-0-iso10646-1"
               "-SRC-Hack-regular-normal-normal-*-[1 0 0 13]-*-*-*-*-0-iso10646-1"
               "-SRC-Hack-regular-normal-normal-*-*-130-*-*-*-0-iso10646-1"
               "-f-family-regular-normal-normal--13-130-*-96-m-0-*"
               "-f-family-regular-normal-normal--13-130-*-96-m-0-*-*"
               "-f-family-regular-normal-normal-*-13-*-*-oops-m-0-iso10646-1"
               "-f-family-regular-normal-normal-*-13-*-*-*-oops-0-iso10646-1"
               "-f-family-regular-normal-normal-*-13-*-*-*-m-oops-iso10646-1"
               "-SRC-Hack-unknown-normal-normal-*-13-*-*-*-*-0-iso10646-1"
               "-SRC-Hack" "Hack*"))"#,
        expect_test::expect![[
            r#""OK ((\"*\" nil nil nil nil nil nil nil nil nil nil nil) (\"-*\" nil nil nil nil nil nil nil nil nil nil nil) (\"-*-Hack-*\" nil Hack nil nil nil nil nil nil nil nil nil) (\"*-Hack-bold-r-*-13-*\" nil Hack bold r nil nil nil 13 nil nil nil) (\"-*-Hack-130-*\" :error error) (\"-*-Hack-reg*-*\" nil nil nil nil nil Hack nil nil nil nil nil) (\"-*-Hack-*-iso10646-1\" nil Hack nil nil nil nil iso10646-1 nil nil nil nil) (\"-SRC-Some-Hyphen-Family-regular-normal-normal-*-13-*-*-*-*-0-iso10646-1\" SRC Some-Hyphen-Family regular normal normal nil iso10646-1 13 nil nil 0) (\"*-Hack-regular-normal-normal-*-13-*-*-*-*-0-iso10646-1\" nil Hack regular normal normal nil iso10646-1 13 nil nil 0) (\"-SRC-Hack-regular-normal-normal-*-0-*-*-*-*-0-iso10646-1\" SRC Hack regular normal normal nil iso10646-1 0 nil nil 0) (\"-SRC-Hack-regular-normal-normal-*-[1 0 0 13]-*-*-*-*-0-iso10646-1\" SRC Hack regular normal normal nil iso10646-1 13 nil nil 0) (\"-SRC-Hack-regular-normal-normal-*-*-130-*-*-*-0-iso10646-1\" SRC Hack regular normal normal nil iso10646-1 13.0 nil nil 0) (\"-f-family-regular-normal-normal--13-130-*-96-m-0-*\" f family regular normal normal ## nil 13 nil 100 0) (\"-f-family-regular-normal-normal--13-130-*-96-m-0-*-*\" f family regular normal normal ## nil 13 96 100 0) (\"-f-family-regular-normal-normal-*-13-*-*-oops-m-0-iso10646-1\" :error error) (\"-f-family-regular-normal-normal-*-13-*-*-*-oops-0-iso10646-1\" :error error) (\"-f-family-regular-normal-normal-*-13-*-*-*-m-oops-iso10646-1\" :error error) (\"-SRC-Hack-unknown-normal-normal-*-13-*-*-*-*-0-iso10646-1\" :error error) (\"-SRC-Hack\" :error error) (\"Hack*\" :error error))""#
        ]],
    );
}

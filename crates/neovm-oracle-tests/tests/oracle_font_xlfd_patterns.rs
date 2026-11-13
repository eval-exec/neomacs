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

#[test]
fn xlfd_replacement_clears_wildcards_and_preserves_other_properties_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(mapcar
              (lambda (name)
                (let ((spec (font-spec :family "Hack" :foundry "SRC" :adstyle "ja"
                                       :registry "iso10646-1" :weight 'bold :size 17 :dpi 96 :avgwidth 120 :name name)))
                  (cons name (mapcar (lambda (prop) (font-get spec prop))
                                     '(:foundry :family :adstyle :registry :weight :size :dpi :avgwidth)))))
              '("*" "-*-Hack-*" "-*-*-*-*-*-*-*-*-*-*-*-*-*-*"
                "-f-family-regular-normal-normal--13-130-72-*-m-0-*"))"#,
        expect_test::expect![[
            r#""OK ((\"*\" nil nil nil iso10646-1 bold 17 96 120) (\"-*-Hack-*\" nil Hack nil iso10646-1 bold 17 96 120) (\"-*-*-*-*-*-*-*-*-*-*-*-*-*-*\" nil nil nil nil bold 17 nil nil) (\"-f-family-regular-normal-normal--13-130-72-*-m-0-*\" f family ## iso10646-1 regular 13 nil 0))""#
        ]],
    );
}

#[test]
fn xlfd_representable_point_prefixes_and_forced_symbols_match_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(list
  (condition-case err
   (font-get (font-spec :name "*-9999999999999999999999999999999999999999-*") :size)
   (error (list (car err) (cadr err))))
  (condition-case err
   (font-get (font-spec :name "-SRC-Hack-regular-normal-normal-*-9999999999999999999999999999999999999999-130x-*-*-*-0-iso10646-1") :size)
   (error (list (car err) (cadr err))))
  (mapcar
   (lambda (point)
    (let ((size (font-get
                 (font-spec :name
                  (concat "-SRC-Hack-regular-normal-normal-*-*-" point "-*-*-*-0-iso10646-1"))
                 :size)))
     (list (floatp size) size)))
   '("130x" "000130x" "x130" "2147483647"))
  (let ((spec (font-spec :name "-9999999999999999999999999999999999999999-9223372036854775808-regular-normal-normal-*-13-*-*-*-*-0-iso10646-1")))
   (list (symbol-name (font-get spec :foundry)) (symbol-name (font-get spec :family)) (font-get spec :size))))"#,
        expect_test::expect![[
            r#""OK ((overflow-error \"9999999999999999999999999999999999999999\") (overflow-error \"9999999999999999999999999999999999999999\") ((t 13.0) (t 13.0) (nil nil) (t 214748364.7)) (\"9999999999999999999999999999999999999999\" \"9223372036854775808\" 13))""#
        ]],
    );
}

// Exact overflow behavior is established on 64-bit glibc, not every platform
// using the XLFD parser. Oracle identity is checked by the shared harness.
#[cfg(all(target_os = "linux", target_env = "gnu", target_pointer_width = "64"))]
#[test]
fn xlfd_numeric_overflow_and_complete_point_match_64_bit_glibc_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(list
  (mapcar
   (lambda (digits)
    (condition-case err
     (font-get (font-spec :name (concat "*-" digits "-*")) :size)
     (error (list (car err) (cadr err)))))
   '("2305843009213693952" "9223372036854775807" "0000002305843009213693952" "9999999999999999999999999999999999999999"))
  (condition-case err
   (font-get (font-spec :name "-SRC-Hack-regular-normal-normal-*-2305843009213693952-130x-*-*-*-0-iso10646-1") :size)
   (error (list (car err) (cadr err))))
  (mapcar
   (lambda (point)
    (let ((size (font-get
                 (font-spec :name
                  (concat "-SRC-Hack-regular-normal-normal-*-*-" point "-*-*-*-0-iso10646-1"))
                 :size)))
     (list (floatp size) size)))
   '("2147483648" "4294967296" "4294967426" "2305843009213693951" "2305843009213693952" "9223372036854775807" "9223372036854775808" "9999999999999999999999999999999999999999"))
  (font-get (font-spec :name "*-9223372036854775808-*") :size))"#,
        expect_test::expect![[
            r#""OK (((overflow-error \"2305843009213693952\") (overflow-error \"9223372036854775807\") (overflow-error \"0000002305843009213693952\") (overflow-error \"9999999999999999999999999999999999999999\")) (overflow-error \"2305843009213693952\") ((nil nil) (t 0.0) (t 13.0) (nil nil) (t 0.0) (nil nil) (nil nil) (nil nil)) 0)""#
        ]],
    );
}

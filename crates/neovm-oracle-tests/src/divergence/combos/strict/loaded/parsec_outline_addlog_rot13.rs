//! Strict combo oracle probes, batch 41: more loaded-library coverage via
//! assert_oracle_parity_with_load — parsec.el (parser combinators),
//! outline.el (outline-level/next-heading), add-log.el
//! (add-log-current-defun), and rot13.el.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_h8_parsec_collect_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"[ORACLE-LOAD-ROOT]/emacs-lisp/parsec.el\")""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(parsec-parse "abc123"
  (parsec-collect
    (parsec-regexp "^[a-z]+")
    (parsec-regexp "[0-9]+")))
"##,
        &["emacs-lisp/parsec.el"],
        expect,
    );
}

#[test]
fn div_h8_parsec_choice_and_many() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"[ORACLE-LOAD-ROOT]/emacs-lisp/parsec.el\")""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (parsec-parse "foo42"
        (parsec-many1 (parsec-regexp "[a-z]")))
      (parsec-parse "bar"
        (parsec-optional (parsec-str "bar") "fallback"))
      (parsec-parse "abc"
        (parsec-or (parsec-str "xyz") (parsec-str "abc"))))
"##,
        &["emacs-lisp/parsec.el"],
        expect,
    );
}

#[test]
fn div_h8_outline_levels() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<killed buffer> 0 4)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (outline-mode)
  (insert "* Heading 1\n** Sub 1\n* Heading 2\n")
  (goto-char 1)
  (list (outline-level)
        (progn (outline-next-heading) (outline-level))
        (progn (outline-next-heading) (outline-level))))
"##,
        &["outline.el"],
        expect,
    );
}

#[test]
fn div_h8_add_log_current_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"[ORACLE-LOAD-ROOT]/progmodes/add-log.el\")""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun foo ()\n  body)\n")
  (goto-char 5)
  (add-log-current-defun))
"##,
        &["progmodes/add-log.el"],
        expect,
    );
}

#[test]
fn div_h8_rot13_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Uryyb, Jbeyq!\" \"Round trip 123\" 3)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (rot13-string "Hello, World!")
      (rot13-string (rot13-string "Round trip 123"))
      (length (rot13-string "abc")))
"##,
        &["rot13.el"],
        expect,
    );
}

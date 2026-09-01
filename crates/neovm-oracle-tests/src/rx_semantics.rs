//! Oracle parity tests for GNU `emacs-lisp/rx.el` regexp translation semantics.
//!
//! `rx` is macro-expanded into ordinary regexp strings.  These tests compare
//! exact expansion strings and selected match behavior for local and global
//! definitions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_rx_to_string_core_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'rx)
  (list
   (rx-to-string '(seq bos (or "cat" "dog") (+ digit) eos))
   (rx-to-string '(seq symbol-start (+ (any word ?- ?_)) symbol-end))
   (rx-to-string '(seq (group-n 2 (+ alpha)) ":" (backref 2)))
   (rx-to-string '(seq (not (any "abc")) (*? anything)))
   (rx-to-string '(seq (syntax word) (not (syntax whitespace))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"\\\\(?:\\\\`\\\\(?:cat\\\\|dog\\\\)[[:digit:]]+\\\\'\\\\)\" \"\\\\(?:\\\\_<[_[:word:]-]+\\\\_>\\\\)\" \"\\\\(?:\\\\(?2:[[:alpha:]]+\\\\):\\\\2\\\\)\" \"\\\\(?:[^a-c][^z-a]*?\\\\)\" \"\\\\(?:\\\\w\\\\S-\\\\)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_rx_macro_expansion_and_match_behavior() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'rx)
  (let ((re (rx bos (group (+ alpha)) "-" (group (+ digit)) eos)))
    (list
     re
     (string-match re "abc-123")
     (match-string 1 "abc-123")
     (match-string 2 "abc-123")
     (string-match re "abc-"))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"\\\\`\\\\([[:alpha:]]+\\\\)-\\\\([[:digit:]]+\\\\)\\\\'\" 0 \"abc\" \"123\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_rx_let_and_rx_let_eval_definitions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'rx)
  (list
   (rx-let ((hex (any digit "abcdef"))
            (pair (x) (seq x x)))
     (list
      (rx-to-string 'hex)
      (rx-to-string '(pair "ab"))
      (rx hex ":" (pair "xy"))))
   (let ((chars "xyz"))
     (rx-let-eval ((runtime-chars (any (eval chars))))
       (rx-to-string 'runtime-chars)))))
"#;

    let expect =
        expect_test::expect![[r#""ERR (error \"Cannot redefine built-in rx name ‘hex’\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_rx_let_rest_and_binding_error_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'rx)
  (list
   (rx-to-string '(seq (literal "a.b") (regexp "[0-9]+")) t)
   (condition-case err
       (let ((suffix "END"))
         (rx-to-string '(seq "pre" (literal (eval suffix))) t))
     (error (list (car err) (cadr err))))
   (rx-let ((word+ (&rest parts) (seq bow parts eow))
            (braced (x) (seq "{" x "}")))
     (list
      (rx (word+ (+ alpha) "-" (+ digit)))
      (rx (braced (word+ "x")))))
   (condition-case err
       (rx-let ((any anything))
         (rx any))
     (error (list (car err) (cadr err))))
   (condition-case err
       (rx-let ((bad (x . y) (seq x y)))
         (rx bad))
     (error (list (car err) (cadr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"a\\\\.b\\\\(?:[0-9]+\\\\)\" (error \"rx ‘literal’ form with non-string argument\") (\"\\\\<[[:alpha:]]+-[[:digit:]]+\\\\>\" \"{\\\\<x\\\\>}\") (error \"Cannot redefine built-in rx name ‘any’\") (wrong-type-argument listp))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_rx_define_and_error_signaling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'rx)
  (rx-define oracle-rx-word (seq bow (+ word) eow))
  (rx-define oracle-rx-tag (name) (seq "<" name ">"))
  (list
   (rx-to-string 'oracle-rx-word)
   (rx-to-string '(oracle-rx-tag "h1"))
   (rx oracle-rx-word ":" (oracle-rx-tag "h1"))
   (condition-case err
       (rx-to-string '(oracle-rx-tag))
     (error (list (car err) (cadr err))))
   (condition-case err
       (rx-to-string '(unknown-rx-form "x"))
     (error (list (car err) (cadr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"\\\\(?:\\\\<[[:word:]]+\\\\>\\\\)\" \"\\\\(?:<h1>\\\\)\" \"\\\\<[[:word:]]+\\\\>:<h1>\" (error \"Expanding rx def ‘oracle-rx-tag’: too few arguments (got 0, need 1)\") (error \"Unknown rx form ‘unknown-rx-form’\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Oracle parity tests for GNU `emacs-lisp/regexp-opt.el` semantics.
//!
//! These tests cover generated regexp strings, grouping modes,
//! longest-match behavior, charset construction, empty inputs, and
//! `regexp-opt-depth`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_regexp_opt_grouping_modes_and_empty_input() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'regexp-opt)
  (list
   regexp-unmatchable
   (regexp-opt nil)
   (regexp-opt nil t)
   (regexp-opt '("if" "in" "while" "when") nil)
   (regexp-opt '("if" "in" "while" "when") t)
   (regexp-opt '("if" "in" "while" "when") 'words)
   (regexp-opt '("if" "in" "while" "when") 'symbols)
   (regexp-opt '("if" "in" "while" "when") "\\(?1:")))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"\\\\`a\\\\`\" \"\\\\(?:\\\\`a\\\\`\\\\)\" \"\\\\(\\\\`a\\\\`\\\\)\" \"\\\\(?:i[fn]\\\\|wh\\\\(?:en\\\\|ile\\\\)\\\\)\" \"\\\\(i[fn]\\\\|wh\\\\(?:en\\\\|ile\\\\)\\\\)\" \"\\\\<\\\\(i[fn]\\\\|wh\\\\(?:en\\\\|ile\\\\)\\\\)\\\\>\" \"\\\\_<\\\\(i[fn]\\\\|wh\\\\(?:en\\\\|ile\\\\)\\\\)\\\\_>\" \"\\\\(?1:i[fn]\\\\|wh\\\\(?:en\\\\|ile\\\\)\\\\)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_opt_longest_match_and_equivalence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'regexp-opt)
  (let* ((strings '("a" "aa" "aaa" "ab" "abc" "abcd"))
         (re (regexp-opt strings)))
    (list
     re
     (string-match re "abcd!")
     (match-string 0 "abcd!")
     (string-match re "aaa!")
     (match-string 0 "aaa!")
     (mapcar (lambda (s) (and (string-match-p (concat "\\`" re "\\'") s) s))
             strings)
     (string-match-p (concat "\\`" re "\\'") "ac"))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"\\\\(?:a\\\\(?:aa\\\\|bcd?\\\\|[ab]\\\\)?\\\\)\" 0 \"abcd\" 0 \"aaa\" (\"a\" \"aa\" \"aaa\" \"ab\" \"abc\" \"abcd\") nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_opt_charset_and_quoting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'regexp-opt)
  (let* ((chars '("." "*" "+" "?" "[" "]" "^" "-" "\\"))
         (mixed '("a+" "a*" "a?" "b." "b[" "b]"))
         (char-re (regexp-opt chars))
         (mixed-re (regexp-opt mixed t)))
    (list
     char-re
     (mapcar (lambda (s) (and (string-match-p (concat "\\`" char-re "\\'") s) s))
             chars)
     mixed-re
     (mapcar (lambda (s) (and (string-match-p (concat "\\`" mixed-re "\\'") s) s))
             mixed)
     (string-match-p (concat "\\`" mixed-re "\\'") "a."))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"[]*+.?[\\\\^-]\" (\".\" \"*\" \"+\" \"?\" \"[\" \"]\" \"^\" \"-\" \"\\\\\") \"\\\\(a[*+?]\\\\|b[].[]\\\\)\" (\"a+\" \"a*\" \"a?\" \"b.\" \"b[\" \"b]\") nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_opt_depth_counts_only_capturing_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'regexp-opt)
  (list
   (regexp-opt-depth "\\(a\\)")
   (regexp-opt-depth "\\(?:a\\)")
   (regexp-opt-depth "\\(?1:a\\)")
   (regexp-opt-depth "\\(a\\)\\(?:b\\)\\(?2:c\\)\\(d\\)")
   (regexp-opt-depth "[()]\\(x\\)")
   (condition-case err
       (regexp-opt-depth "\\(unterminated")
     (error (list (car err) (cadr err))))))
"#;

    let expect =
        expect_test::expect![[r#""OK (1 0 0 2 1 (invalid-regexp \"Unmatched ( or \\\\(\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

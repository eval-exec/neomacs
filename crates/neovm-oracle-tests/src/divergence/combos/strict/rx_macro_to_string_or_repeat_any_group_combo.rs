//! Strict combo oracle probes, batch 164: the rx macro. rx-to-string over
//! seq/one-or-more/any/repeat forms, rx with or/?/group/bol/eol/not-space,
//! matching captured groups built via rx, rx-define-evaluation and named
//! insertion, and rx-equivalent of word-boundary + char alternation.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_rx_to_string_seq_repeat_any_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'rx)
(list (rx-to-string '(seq "foo" (one-or-more digit) "bar"))
      (rx-to-string '(repeat 3 (any "abc")))
      (rx-to-string '(repeat 1 3 digit))
      (rx-to-string '(or "cat" "dog" "bird"))
      (rx-to-string '(seq bow (+ word) eow))
      (rx-to-string '(group (seq "key:" (1+ digit))))
      (string-match (rx (seq "key:" (group (1+ digit)))) "key:42rest")
      (match-string 1 "key:42rest")
      (rx-to-string '(seq "v" (opt "1") "=" (0+ digit))))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"\\\\(?:foo[[:digit:]]+bar\\\\)\" \"[a-c]\\\\{3\\\\}\" \"[[:digit:]]\\\\{1,3\\\\}\" \"\\\\(?:bird\\\\|cat\\\\|dog\\\\)\" \"\\\\(?:\\\\<[[:word:]]+\\\\>\\\\)\" \"\\\\(key:[[:digit:]]+\\\\)\" 0 \"42\" \"\\\\(?:v1?=[[:digit:]]*\\\\)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_rx_named_groups_interval_submatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'rx)
(list (rx-to-string '(: "id=" (group-n id (1+ digit))))
      (let ((re (rx "v" (= 3 digit) "-" (1+ alpha))))
        (list (string-match re "v123-abcd")
              (match-string 0 "v123-abcd")
              (string-match re "v12-ab")))
      (rx-to-string '(seq "p" (repeat 2 4 hex)))
      (rx-to-string '(| "yes" "no" "maybe"))
      (rx-to-string '(seq "x" (max-n 2 digit) "y"))
      (rx-to-string '(seq "a" (minimal-match (zero-or-more nonl)) "z")))
"##;
    let expect = expect_test::expect![[
        r#""ERR (error \"rx ‘group-n’ requires a positive number as first argument\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_rx_pcase_and_backreference_constructs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'rx)
(list (rx-let ((field (seq (+ (any "a-z")) ":"))) (rx-to-string '(seq field (+ digit))))
      (rx-to-string '(seq (group (any "([{")) (0+ nonl) (backref 1)))
      (let ((re (rx-to-string '(seq word-start "def" word-end))))
        (list (string-match re "def foo")
              (match-end 0 "def foo")))
      (rx-to-string '(not (any "0-9")))
      (rx-to-string '(seq "a" (| "b" "c") "d"))
      (string-match (rx (seq "id=" (group (1+ digit)) " name=" (group (1+ alpha)))) "id=99 name=abc")
      (list (match-string 1 "id=99 name=abc")
            (match-string 2 "id=99 name=abc")))
"##;
    let expect = expect_test::expect![[r#""ERR (error \"Unknown rx symbol ‘field’\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Complex combo batch 164 — `rx` macro constructions, `rx-let-eval`,
//! `rx-to-string`, `rx-or`, `rx-and`, character classes, anchors,
//! repetition, grouping, back-reference patterns.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx164_rx_basic_constructions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\\(?:\\\\`hello\\\\'\\\\)\" \"\\\\(?:prefix\\\\([[:digit:]]+\\\\)suffix\\\\)\" \"\\\\(?:\\\\<[[:word:]]+\\\\>\\\\)\" \"\\\\(?:\\\\(?:alph\\\\|bet\\\\|gamm\\\\)a\\\\)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (rx-to-string '(seq bos "hello" eos))
      (rx-to-string '(seq "prefix" (group (+ digit)) "suffix"))
      (rx-to-string '(seq bow (+ word) eow))
      (rx-to-string '(or "alpha" "beta" "gamma")))
"##,
        expect,
    );
}

#[test]
fn div_cx164_rx_repetition_constructs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\\(?:[[:digit:]]+\\\\)\" \"\\\\(?:[[:digit:]]*\\\\)\" \"[[:digit:]]\\\\{3\\\\}\" \"[[:alpha:]]\\\\{5,\\\\}\" \"[\u{2}\u{4}[:alpha:]]\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (rx-to-string '(+ digit))
      (rx-to-string '(* digit))
      (rx-to-string '(= 3 digit))
      (rx-to-string '(>= 5 alpha))
      (rx-to-string '(| 2 4 alpha)))
"##,
        expect,
    );
}

#[test]
fn div_cx164_rx_character_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Unknown rx category ‘letter’\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (rx-to-string 'any)
      (rx-to-string 'nonl)
      (rx-to-string '(any "A-Z"))
      (rx-to-string '(any "a-z" "A-Z" "0-9"))
      (rx-to-string '(not (any "X")))
      (rx-to-string '(syntax word))
      (rx-to-string '(syntax symbol))
      (rx-to-string '(category letter)))
"##,
        expect,
    );
}

#[test]
fn div_cx164_rx_anchors_and_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\\`\" \"\\\\'\" \"\\\\(?:^\\\\)\" \"\\\\(?:$\\\\)\" \"\\\\<\" \"\\\\>\" \"\\\\b\" \"\\\\B\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (rx-to-string 'bos)
      (rx-to-string 'eos)
      (rx-to-string 'bol)
      (rx-to-string 'eol)
      (rx-to-string 'bow)
      (rx-to-string 'eow)
      (rx-to-string 'word-boundary)
      (rx-to-string 'not-word-boundary))
"##,
        expect,
    );
}

#[test]
fn div_cx164_rx_grouping_and_backref() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\\(\\\\`[[:word:]]+\\\\'\\\\)\" \"\\\\(?:\\\\([[:alpha:]]+\\\\)-\\\\1\\\\)\" \"\\\\(?:\\\\(?1:[[:digit:]]+\\\\)-\\\\1\\\\)\" \"\\\\(?:\\\\(?1:[[:word:]]+\\\\):\\\\1\\\\)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (rx-to-string '(group bos (one-or-more word) eos))
      (rx-to-string '(seq (group (one-or-more alpha)) "-" (backref 1)))
      (rx-to-string '(seq (group-n 1 (+ digit)) "-" (backref 1)))
      (rx-to-string '(seq (submatch-n 1 (+ word)) ":" (backref 1))))
"##,
        expect,
    );
}

#[test]
fn div_cx164_rx_let_eval_with_custom_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored invalid-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (rx-let-eval ((identifier (seq (any "a-zA-Z_") (* (any "a-zA-Z0-9_"))))
                  (ws (* (any " \t"))))
      (rx-to-string '(seq bos identifier ws ":" ws identifier eos)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx164_rx_with_eval_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable name)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((name "neo-cx164-var"))
  (list (rx-to-string `(seq bos (eval (regexp-quote ,name)) eos))
        (rx-to-string `(seq (group (+ word)) ":" (eval name) eos))))
"##,
        expect,
    );
}

#[test]
fn div_cx164_rx_literal_string_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"literal\\\\.string\" \"\\\\(?:literal\\\\.string\\\\)\" \"with \\\\[special] chars\" \"and (parens) too\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (rx "literal.string")
      (rx-to-string '(literal "literal.string"))
      (rx "with [special] chars")
      (rx "and (parens) too"))
"##,
        expect,
    );
}

#[test]
fn div_cx164_rx_regexp_match_with_constructed_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\\(?:\\\\`\\\\([A-Z_a-z]+\\\\):[0-9]*\\\\'\\\\)\" 0 \"ABC\" nil 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((pat (rx-to-string
            '(seq bos
                  (group (+ (any "A-Za-z_")))
                  ":"
                  (* (any "0-9"))
                  eos))))
  (list pat
        (string-match pat "ABC:123")
        (match-string 1 "ABC:123")
        (string-match pat "9invalid")
        (string-match pat "ABC:")))
"##,
        expect,
    );
}

#[test]
fn div_cx164_rx_named_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((pat (rx-to-string
                '(seq bos
                      (group-n alpha (+ (any "A-Z")))
                      ":"
                      (group-n num (+ digit))
                      eos))))
      (list pat
            (string-match pat "ABC:123")
            (match-string 1 "ABC:123")
            (match-string 2 "ABC:123")))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx164_rx_case_fold_interactions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\\\\(?:\\\\`hello\\\\'\\\\)\" 0 nil 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((pat (rx-to-string '(seq bos "hello" eos))))
  (list pat
        (let ((case-fold-search nil)) (string-match pat "hello"))
        (let ((case-fold-search nil)) (string-match pat "HELLO"))
        (let ((case-fold-search t)) (string-match pat "HELLO"))))
"##,
        expect,
    );
}

#[test]
fn div_cx164_rx_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 2 18)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((pat (rx-to-string
            '(seq bos
                  (group (+ (any "A-Za-z_")))
                  ":"
                  (* (any "0-9"))
                  eos))))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "Match: %s" (string-match pat "ABC:123")))
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list pat
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
                          (text-properties-at 1))))))
"##,
        expect,
    );
}

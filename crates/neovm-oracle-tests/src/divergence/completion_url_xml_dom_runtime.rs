//! Completion (try/all-completions, test-completion, boundaries,
//! completion-all-completions), URL parse/encode, and XML/DOM parse parity.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn completing_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"a\" (\"a\" \"ab\" \"abc\") (\"ab\" . 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((coll '(("a" . 1) ("ab" . 2) ("abc" . 3) ("z" . 4))))
  (list (try-completion "a" coll) (sort (all-completions "a" coll) #'string<)
        (assoc-string "ab" coll)))"##,
        expect,
    );
}

#[test]
fn completion_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"ab\" (\"abc\" \"abd\" \"xyz\") (0 . 0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((coll '("abc" "abd" "xyz")))
  (list (try-completion "ab" coll) (all-completions "" coll)
        (completion-boundaries "ab" coll nil "")))"##,
        expect,
    );
}

#[test]
fn completion_styles_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((completion-styles '(basic))
       (coll '("display-buffer" "display-line" "delete-char")))
  (sort (completion-all-completions "dis" coll nil 3) (lambda (a b) (if (and (stringp a) (stringp b)) (string< a b) t))))"##,
        expect,
    );
}

#[test]
fn try_all_completions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"foo\" \"fooba\" (\"foobar\" \"foobaz\" \"fooqux\") t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((coll '("foobar" "foobaz" "fooqux" "other")))
  (list (try-completion "foo" coll) (try-completion "foob" coll)
        (sort (all-completions "foo" coll) #'string<)
        (test-completion "foobar" coll) (test-completion "foo" coll)))"##,
        expect,
    );
}

#[test]
fn dom_build_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (div \"x\" \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'dom)
(let ((node (dom-node 'div '((class . "x")) "hello")))
  (list (dom-tag node) (dom-attr node 'class) (dom-text node)))"##,
        expect,
    );
}

#[test]
fn url_encode_decode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"a%20b%26c%3Dd\" \"a b&c\" \"https://x.org/a%20b\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (url-hexify-string "a b&c=d")
        (url-unhex-string "a%20b%26c")
        (url-encode-url "https://x.org/a b"))"##,
        expect,
    );
}

#[test]
fn url_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"https\" \"example.com\" 8080 \"/path?q=1\" \"user\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'url-parse)
(let ((u (url-generic-parse-url "https://user@example.com:8080/path?q=1#frag")))
  (list (url-type u) (url-host u) (url-port u) (url-filename u) (url-user u)))"##,
        expect,
    );
}

#[test]
fn xml_parse_dom() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function dom-children)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "<root><child id=\"1\">text</child><child id=\"2\">more</child></root>")
  (let ((dom (libxml-parse-xml-region (point-min) (point-max))))
    (list (car dom) (length (dom-children dom))
          (dom-attr (car (dom-by-tag dom 'child)) 'id)
          (dom-text (car (dom-by-tag dom 'child))))))"##,
        expect,
    );
}

#[test]
fn xml_parse_native() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (a 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "<a><b>1</b><b>2</b></a>")
  (let ((tree (xml-parse-region (point-min) (point-max))))
    (list (caar tree) (length (xml-get-children (car tree) 'b)))))"##,
        expect,
    );
}

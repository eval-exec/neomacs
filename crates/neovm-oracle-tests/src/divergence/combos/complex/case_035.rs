//! Complex combo batch 35 — JSON/XML/CSV parsing, ansi-color, rot13,
//! url-hexify, format-encode/decode. Deterministic complex parsers that
//! neomacs reimplements — prime divergence candidates.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx35_json_encode_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'json)
      (list (json-encode '((a . 1) (b . "hello") (c . [1 2 3])))
            (json-encode '("a" "b" "c"))
            (json-encode '((nested . ((x . 1) (y . 2))))
            (json-encode-null)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx35_json_read_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((a . 1) (b . [1 2 3])) [1 2 \"hello\" nil t :json-false] \"escaped \\\"string\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'json)
      (list (json-read-from-string "{\"a\": 1, \"b\": [1, 2, 3]}")
            (json-read-from-string "[1, 2, \"hello\", null, true, false]")
            (json-read-from-string "\"escaped \\\"string\\\"\"")))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx35_json_encode_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"{\\\"name\\\":\\\"café世界\\\",\\\"emoji\\\":\\\"😀\\\"}\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'json)
      (json-encode '((name . "café世界") (emoji . "😀"))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx35_json_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"{\\\"a\\\":1,\\\"b\\\":\\\"test\\\",\\\"c\\\":[4,5,6]}\" ((a . 1) (b . \"test\") (c . [4 5 6])))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'json)
      (let* ((data '((a . 1) (b . "test") (c . [4 5 6])))
             (encoded (json-encode data))
             (decoded (json-read-from-string encoded)))
        (list encoded decoded)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx35_libxml_parse_html() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'dom)
      (let ((html "<html><body><p>hello <b>world</b></p></body></html>"))
        (with-temp-buffer
          (insert html)
          (let ((dom (if (fboundp 'libxml-parse-html-region)
                         (libxml-parse-html-region (point-min) (point-max))
                       :not-available)))
            (if (eq dom :not-available) :not-available
              (list (dom-tag (car dom))
                    (dom-text (car (dom-by-tag dom 'p))))))))
    (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx35_xml_parse_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (root (child ((attr . \"val\")) \"text\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'xml)
      (with-temp-buffer
        (insert "<root><child attr=\"val\">text</child></root>")
        (let ((dom (xml-parse-region (point-min) (point-max))))
          (if dom
              (list (caar dom) (car (xml-node-children (car dom))))
            :parse-failed))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx35_csv_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . file-missing)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'csv)
      (with-temp-buffer
        (insert "a,b,c\n1,2,3\n")
        (let ((rows (csv-parse-buffer)))
          (length rows))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx35_ansi_color_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"red text\" \"green bold\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'ansi-color)
      (list (ansi-color-filter-apply "\e[31mred\e[0m text")
            (ansi-color-filter-apply "\e[1;32mgreen bold\e[0m")))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx35_rot13() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"uryyb jbeyq\" \"pnsé\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'rot13)
      (list (rot13-string "hello world")
            (rot13-string "café")
            (equal "hello" (rot13-string (rot13-string "hello")))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx35_url_hexify_unhex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"caf%C3%A9%20world%20%26%20friends\" \"café world\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'url-util)
      (list (url-hexify-string "café world & friends")
            (url-unhex-string "caf%C3%A9%20world")
            (equal "hello world" (url-unhex-string (url-hexify-string "hello world")))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx35_format_encode_decode_rich_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'format)
      (let* ((s (propertize "hello" 'face 'bold))
             (enc (format-encode-string s 'rich-text))
             (dec (format-decode-string enc nil 'rich-text)))
        (list (stringp enc) (stringp dec)
              (text-properties-at 0 dec))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx35_dom_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (html (body) ((p nil \"text\")) \"x\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'dom)
      (let ((dom '(html nil (body nil (p nil "text") (div ((class . "x")) "div")))))
        (list (dom-tag dom)
              (mapcar #'dom-tag (dom-children dom))
              (dom-by-tag dom 'p)
              (dom-attr (car (dom-by-tag dom 'div)) 'class)
              (dom-text dom))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx35_json_encode_plist_hashtable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"{\\\"key\\\":\\\"value\\\",\\\"num\\\":42}\" \"{\\\"a\\\":1,\\\"b\\\":2}\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'json)
      (let ((ht (make-hash-table :test 'equal)))
        (puthash "key" "value" ht)
        (puthash "num" 42 ht)
        (list (json-encode ht)
              (json-encode '(:a 1 :b 2)))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx35_encode_coding_region_utf8_no_op_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 11 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "café世界"))
  (with-temp-buffer
    (insert s)
    (let ((len-before (length (buffer-string))))
      (encode-coding-region (point-min) (point-max) 'utf-8)
      (let ((len-after (length (buffer-string))))
        (list len-before len-after (> len-after len-before))))))
"##,
        expect,
    );
}

#[test]
fn div_cx35_process_exit_code_exit_zero_correct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (exit 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx35-e0" :command '("true"))))
  (accept-process-output p 2)
  (list (process-status p) (process-exit-status p)))
"##,
        expect,
    );
}

#[test]
fn div_cx35_char_syntax_consistency_multibyte_vs_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (119 119 119 32 40 41 60 34 39 92 39 95)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (char-syntax ?a) (char-syntax ?A) (char-syntax ?1) (char-syntax ?\s)
      (char-syntax ?\() (char-syntax ?\)) (char-syntax ?\;) (char-syntax ?\")
      (char-syntax ?') (char-syntax ?\\) (char-syntax ?#) (char-syntax ?_))
"##,
        expect,
    );
}

#[test]
fn div_cx35_coding_system_priority_list_exact_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 20""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(length (coding-system-priority-list))
"##,
        expect,
    );
}

#[test]
fn div_cx35_buffer_hash_after_insert_undo_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"d73ef92426f2b11dfc4aed4d4bfc41c49ee1087c\" \"632eeea13a2786db73229c5e9c07d087aa0894d8\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "original")
  (let ((h1 (buffer-hash)))
    (undo-boundary)
    (insert " more")
    (let ((h2 (buffer-hash)))
      (undo)
      (list h1 h2 (equal h1 (buffer-hash))))))
"##,
        expect,
    );
}

#[test]
fn div_cx35_cl_setf_on_nth_cdr_push_pop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 2 99 88 5) 99 88)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lst (list 1 2 3 4 5)))
  (setf (nth 2 lst) 99)
  (setf (car (cdr (cdr (cdr lst)))) 88)
  (list lst (nth 2 lst) (nth 3 lst)))
"##,
        expect,
    );
}

#[test]
fn div_cx35_overlay_display_slice_width_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 2 5 'display '(slice 0 0 3 1))
  (list (current-column)
        (string-width (buffer-substring 1 5))))
"##,
        expect,
    );
}

#[test]
fn div_cx35_map_concat_hash_table_values_sorted() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p \"2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "a" 1 ht) (puthash "b" 2 ht) (puthash "c" 3 ht)
  (let (vals)
    (maphash (lambda (k v) (push (number-to-string v) vals)) ht)
    (mapconcat #'identity (sort vals #'<) "-")))
"##,
        expect,
    );
}

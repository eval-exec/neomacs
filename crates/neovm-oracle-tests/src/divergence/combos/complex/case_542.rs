/// Batch 542: misc edge cases - %S on various objects, format on circular, etc.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx542_format_S_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello\" \"42\" \"\\\"hello\\\"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%S" 'hello) (format "%S" 42) (format "%S" "hello"))
"##,
        expect,
    );
}

#[test]
fn div_cx542_format_S_nil_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"nil\" \"t\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%S" nil) (format "%S" t))
"##,
        expect,
    );
}

#[test]
fn div_cx542_format_S_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"(a b c)\" \"(a . b)\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%S" '(a b c)) (format "%S" '(a . b)))
"##,
        expect,
    );
}

#[test]
fn div_cx542_format_S_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"[1 2 3]\" \"[:a :b]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%S" [1 2 3]) (format "%S" [:a :b]))
"##,
        expect,
    );
}

#[test]
fn div_cx542_format_S_hash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#s(hash-table data (a 1))\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ht (make-hash-table)))
  (puthash 'a 1 ht)
  (format "%S" ht))
"##,
        expect,
    );
}

#[test]
fn div_cx542_format_S_bool_vec() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#&3\\\"\u{5}\\\"\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(format "%S" (bool-vector t nil t))
"##,
        expect,
    );
}

#[test]
fn div_cx542_format_S_char_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK \"#^[119 nil syntax-table 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119]\"""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ct (make-char-table 'syntax-table ?w)))
  (format "%S" ct))
"##,
        expect,
    );
}

#[test]
fn div_cx542_format_percent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"100% complete\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(format "100%% complete")
"##,
        expect,
    );
}

#[test]
fn div_cx542_format_multiline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"line1\\nline2\\nline3\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(format "line1\nline2\nline3")
"##,
        expect,
    );
}

#[test]
fn div_cx542_format_unicode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"cafeé world\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(format "cafe\u00e9 world")
"##,
        expect,
    );
}

#[test]
fn div_cx542_propertize_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK #(\"hello\" 0 5 (face bold))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(propertize "hello" 'face 'bold)
"##,
        expect,
    );
}

#[test]
fn div_cx542_propertize_multi() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"hello\" 0 5 (face bold mouse-face highlight help-echo \"help\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(propertize "hello" 'face 'bold 'mouse-face 'highlight 'help-echo "help")
"##,
        expect,
    );
}

#[test]
fn div_cx542_propertize_read_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t (face bold))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let* ((s (propertize "text" 'face 'bold))
       (p (prin1-to-string s))
       (r (car (read-from-string p))))
  (list (equal s r) (text-properties-at 0 r)))
"##,
        expect,
    );
}

#[test]
fn div_cx542_format_S_obarray_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function obarray-default)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((obs (obarray-default)))
  (format "%S" obs))
"##,
        expect,
    );
}

#[test]
fn div_cx542_format_S_window_buffer_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#<buffer *scratch*>\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(format "%S" (current-buffer))
"##,
        expect,
    );
}

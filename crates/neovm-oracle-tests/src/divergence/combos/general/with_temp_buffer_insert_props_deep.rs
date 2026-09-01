//! Deep combo: with-temp-buffer + insert + buffer ops + text props + markers.
//! Tests temp buffer patterns with text property and marker interactions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_with_temp_buffer_basic_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"hello world\" 12 11)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (with-temp-buffer\n\
         (insert \"hello\")\n\
         (goto-char (point-max))\n\
         (insert \" world\")\n\
         (list (buffer-string) (point) (buffer-size))))",
        expect,
    );
}

#[test]
fn deficiency_with_temp_buffer_props_survive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (a b c #(\"AAABBBCCC\" 0 3 (zone a) 3 6 (zone b) 6 9 (zone c)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (with-temp-buffer\n\
         (insert \"AAABBBCCC\")\n\
         (put-text-property 1 4 'zone 'a)\n\
         (put-text-property 4 7 'zone 'b)\n\
         (put-text-property 7 10 'zone 'c)\n\
         (list (get-text-property 1 'zone)\n\
         (get-text-property 5 'zone)\n\
         (get-text-property 8 'zone)\n\
         (buffer-string))))",
        expect,
    );
}

#[test]
fn deficiency_with_temp_buffer_marker_after_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"ABCDXXXEFGHIJ\" 3 10)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (with-temp-buffer\n\
         (insert \"ABCDEFGHIJ\")\n\
         (let ((m3 (copy-marker 3))\n\
         (m7 (copy-marker 7)))\n\
         (goto-char 5)\n\
         (insert \"XXX\")\n\
         (list (buffer-string)\n\
         (marker-position m3)\n\
         (marker-position m7)))))",
        expect,
    );
}

#[test]
fn deficiency_with_temp_buffer_search_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 \"F1 bar F2 baz F3\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (with-temp-buffer\n\
         (insert \"foo bar foo baz foo\")\n\
         (goto-char 1)\n\
         (let ((count 0))\n\
         (while (re-search-forward \"foo\" nil t)\n\
         (replace-match (format \"F%d\" (cl-incf count))))\n\
         (list count (buffer-string)))))",
        expect,
    );
}

#[test]
fn deficiency_with_temp_buffer_narrow_widen() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 13 \"BBCCCCDD\" nil \"AAABBBCCCCDDDDEEEE\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (with-temp-buffer\n\
         (insert \"AAABBBCCCCDDDDEEEE\")\n\
         (narrow-to-region 5 13)\n\
         (list (point-min) (point-max)\n\
         (buffer-string)\n\
         (widen)\n\
         (buffer-string))))",
        expect,
    );
}

#[test]
fn deficiency_with_temp_buffer_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 10 5 1)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (with-temp-buffer\n\
         (insert \"IMPORTANT-TEXT\")\n\
         (let ((ov (make-overlay 1 10)))\n\
         (overlay-put ov 'priority 5)\n\
         (overlay-put ov 'face 'bold)\n\
         (list (overlay-start ov)\n\
         (overlay-end ov)\n\
         (overlay-get ov 'priority)\n\
         (length (overlays-in 1 15))))))",
        expect,
    );
}

#[test]
fn deficiency_with_temp_buffer_undo_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 6 15)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (with-temp-buffer\n\
         (insert \"BASE\")\n\
         (put-text-property 1 5 'ver 0)\n\
         (undo-boundary)\n\
         (goto-char (point-max))\n\
         (insert \"-MODIFIED\")\n\
         (put-text-property 6 15 'ver 1)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s (buffer-string)\n\
         (get-text-property 1 'ver)))))",
        expect,
    );
}

#[test]
fn deficiency_with_temp_buffer_kill_yank() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"The  brown fox quick\" 15 19 (pos adj) 19 20 (rear-nonsticky t pos adj)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (with-temp-buffer\n\
         (insert \"The quick brown fox\")\n\
         (put-text-property 5 10 'pos 'adj)\n\
         (kill-region 5 10)\n\
         (goto-char (point-max))\n\
         (insert \" \")\n\
         (yank)\n\
         (list (buffer-string)\n\
         (get-text-property 5 'pos))))",
        expect,
    );
}

#[test]
fn deficiency_nested_with_temp_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"OUTER\" \"INNER\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((outer-result\n\
         (with-temp-buffer\n\
         (insert \"OUTER\")\n\
         (let ((inner-result\n\
         (with-temp-buffer\n\
         (insert \"INNER\")\n\
         (buffer-string))))\n\
         (list (buffer-string) inner-result)))))\n\
         outer-result))",
        expect,
    );
}

#[test]
fn deficiency_with_temp_buffer_format_build() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"item-1: 1\\nitem-2: 4\\nitem-3: 9\\nitem-4: 16\\nitem-5: 25\\n\" 5 52)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (with-temp-buffer\n\
         (dolist (i '(1 2 3 4 5))\n\
         (insert (format \"item-%d: %d\\n\" i (* i i))))\n\
         (list (buffer-string)\n\
         (count-lines (point-min) (point-max))\n\
         (buffer-size))))",
        expect,
    );
}

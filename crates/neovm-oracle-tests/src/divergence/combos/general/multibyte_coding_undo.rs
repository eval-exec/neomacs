//! Deep stress: coding-system + multibyte + buffer-string + substring + text props.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_multibyte_insert_delete_undo_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 15 18)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mid\")))\n\
         (with-current-buffer buf\n\
         (insert \"\\u00e9\\u00e8\\u00ea Hello \\u4e16\\u754c \\u00fc\\u00f6\\u00e4\")\n\
         (put-text-property 1 4 'type 'accent)\n\
         (put-text-property 5 11 'type 'ascii)\n\
         (put-text-property 12 14 'type 'cjk)\n\
         (put-text-property 15 18 'type 'umlaut)\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"\\u2603\")\n\
         (put-text-property 5 6 'type 'symbol)\n\
         (undo-boundary)\n\
         (goto-char 12)\n\
         (delete-region 12 14)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (t1 (get-text-property 1 'type))\n\
         (t5 (get-text-property 5 'type))\n\
         (t6 (get-text-property 6 'type))\n\
         (t12 (get-text-property 12 'type)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s t1 t5 t6 t12\n\
         (buffer-string)\n\
         (get-text-property 1 'type)\n\
         (get-text-property 5 'type)\n\
         (get-text-property 12 'type)\n\
         (get-text-property 15 'type)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_string_as_multibyte_buffer_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"sam\")))\n\
         (with-current-buffer buf\n\
         (insert (string-to-multibyte \"Hello World\"))\n\
         (put-text-property 1 6 'lang 'en)\n\
         (put-text-property 7 12 'lang 'en)\n\
         (let ((len1 (length (buffer-string)))\n\
         (bytes1 (string-bytes (buffer-string))))\n\
         (undo-boundary)\n\
         (erase-buffer)\n\
         (insert \"\\u4f60\\u597d\\u4e16\\u754c\")\n\
         (put-text-property 1 5 'lang 'zh)\n\
         (undo-boundary)\n\
         (let ((len2 (length (buffer-string)))\n\
         (bytes2 (string-bytes (buffer-string))))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list len1 bytes1 len2 bytes2\n\
         (buffer-string)\n\
         (length (buffer-string))\n\
         (string-bytes (buffer-string))\n\
         (get-text-property 1 'lang))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_encode_decode_region_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 20)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"edr\")))\n\
         (with-current-buffer buf\n\
         (insert \"caf\\u00e9 na\\u00efve r\\u00e9sum\\u00e9\")\n\
         (put-text-property 1 20 'original t)\n\
         (let ((orig (buffer-string)))\n\
         (undo-boundary)\n\
         (let ((encoded (encode-coding-string orig 'utf-8)))\n\
         (erase-buffer)\n\
         (insert encoded)\n\
         (undo-boundary)\n\
         (decode-coding-region 1 (point-max) 'utf-8)\n\
         (let ((decoded (buffer-string)))\n\
         (list (string= orig decoded)\n\
         (length orig) (length decoded)\n\
         (get-text-property 1 'original)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_char_syntax_multibyte_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"csm\")))\n\
         (with-current-buffer buf\n\
         (insert \"abc(xyz)  \\u00e9\\u00e8[123]\\u4e16\\u754c{456}\")\n\
         (goto-char (point-min))\n\
         (let ((syntax-list\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (char-syntax (char-after i))))\n\
         (parens (cl-loop for i from 1 to (buffer-size)\n\
         when (memq (char-syntax (char-after i)) '(?\\( ?\\)))\n\
         collect (list i (char-after i)))))\n\
         (list syntax-list\n\
         parens\n\
         (length syntax-list)\n\
         (length parens)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_buffer_position_multibyte_byte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"bpm\")))\n\
         (with-current-buffer buf\n\
         (insert \"AA\\u00e9BB\\u4e16\\u754cCC\")\n\
         (put-text-property 1 3 'zone 'ascii1)\n\
         (put-text-property 3 4 'zone 'accent)\n\
         (put-text-property 4 6 'zone 'ascii2)\n\
         (put-text-property 6 8 'zone 'cjk)\n\
         (put-text-property 8 10 'zone 'ascii3)\n\
         (let ((positions (list (point) (point-min) (point-max)))\n\
         (zones (list (get-text-property 1 'zone)\n\
         (get-text-property 3 'zone)\n\
         (get-text-property 4 'zone)\n\
         (get-text-property 6 'zone)\n\
         (get-text-property 8 'zone)))\n\
         (chars (cl-loop for i from 1 to (buffer-size)\n\
         collect (char-after i))))\n\
         (undo-boundary)\n\
         (goto-char 4)\n\
         (insert \"\\u00f6\\u00fc\")\n\
         (undo-boundary)\n\
         (let ((s (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list positions zones chars s\n\
         (buffer-string))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_case_change_multibyte_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ccm\")))\n\
         (with-current-buffer buf\n\
         (insert \"Hello World \\u00e9\\u00e8\\u00ea\")\n\
         (put-text-property 1 12 'case 'mixed)\n\
         (put-text-property 13 15 'case 'accent)\n\
         (undo-boundary)\n\
         (upcase-region 1 12)\n\
         (undo-boundary)\n\
         (downcase-region 1 12)\n\
         (undo-boundary)\n\
         (capitalize-region 1 12)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (c1 (get-text-property 1 'case))\n\
         (c7 (get-text-property 7 'case))\n\
         (c13 (get-text-property 13 'case)))\n\
         (primitive-undo 3 buffer-undo-list)\n\
         (list s c1 c7 c13\n\
         (buffer-string)\n\
         (get-text-property 1 'case)\n\
         (get-text-property 7 'case)\n\
         (get-text-property 13 'case)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_whitespace_cleanup_undo_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"wcu\")))\n\
         (with-current-buffer buf\n\
         (insert \"line1   \\n  line2  \\nline3\\t\\t\\n  line4\\n\")\n\
         (put-text-property 1 9 'line 1)\n\
         (put-text-property 10 19 'line 2)\n\
         (put-text-property 20 28 'line 3)\n\
         (put-text-property 29 36 'line 4)\n\
         (undo-boundary)\n\
         (goto-char (point-min))\n\
         (while (re-search-forward \"[ \\t]+$\" nil t)\n\
         (replace-match \"\"))\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (l1 (get-text-property 1 'line))\n\
         (l10 (get-text-property 10 'line))\n\
         (l20 (get-text-property 20 'line)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s l1 l10 l20\n\
         (buffer-string)\n\
         (get-text-property 1 'line)\n\
         (get-text-property 10 'line)\n\
         (get-text-property 20 'line)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_indent_region_undo_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 33 41)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"iru\")))\n\
         (with-current-buffer buf\n\
         (insert \"(defun foo ()\\n(body1)\\n(body2)\\n(body3))\")\n\
         (put-text-property 1 14 'depth 0)\n\
         (put-text-property 15 23 'depth 1)\n\
         (put-text-property 24 32 'depth 1)\n\
         (put-text-property 33 41 'depth 1)\n\
         (undo-boundary)\n\
         (goto-char 15)\n\
         (insert \"  \")\n\
         (goto-char 26)\n\
         (insert \"  \")\n\
         (goto-char 39)\n\
         (insert \"  \")\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (d1 (get-text-property 1 'depth))\n\
         (d15 (get-text-property 15 'depth))\n\
         (d27 (get-text-property 27 'depth)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s d1 d15 d27\n\
         (buffer-string)\n\
         (get-text-property 1 'depth)\n\
         (get-text-property 15 'depth)\n\
         (get-text-property 24 'depth)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_apply_macro_undo_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"amu\")))\n\
         (with-current-buffer buf\n\
         (fset 'my-kbd-macro\n\
         (kbd \"I hello ESC\"))\n\
         (insert \"world\")\n\
         (put-text-property 1 6 'gen 'initial)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (insert \"prefix-\")\n\
         (put-text-property 1 7 'gen 'prefix)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (g1 (get-text-property 1 'gen))\n\
         (g7 (get-text-property 7 'gen)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s g1 g7\n\
         (buffer-string)\n\
         (get-text-property 1 'gen)\n\
         (get-text-property 5 'gen)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_mapconcat_over_intervals_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mci\")))\n\
         (with-current-buffer buf\n\
         (insert \"one,two,three,four,five\")\n\
         (let ((parts (split-string (buffer-string) \",\")))\n\
         (cl-loop for p in parts\n\
         for i from 0\n\
         with pos = 1\n\
         do (put-text-property pos (+ pos (length p)) 'index i)\n\
         (cl-incf pos (1+ (length p))))\n\
         (undo-boundary)\n\
         (erase-buffer)\n\
         (insert (mapconcat #'identity (reverse parts) \";\"))\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (i1 (get-text-property 1 'index)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list parts s i1\n\
         (buffer-string)\n\
         (get-text-property 1 'index)\n\
         (get-text-property 4 'index)\n\
         (get-text-property 8 'index))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

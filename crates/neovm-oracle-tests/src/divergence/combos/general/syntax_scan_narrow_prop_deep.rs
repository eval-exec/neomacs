//! Deep combo: syntax-table + scan-sexps + parse-partial-sexp + narrowing + text properties.
//! Tests syntax-aware parsing under narrowing with property interference.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_scan_sexps_through_narrowed_code_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ssp\")))\n\
         (with-current-buffer buf\n\
         (insert \"(defun foo (x y)\\n  (let ((a (+ x 1))\\n        (b (- y 2)))\\n    (list a b)))\")\n\
         (put-text-property 1 7 'face 'keyword)\n\
         (put-text-property 8 11 'face 'function-name)\n\
         (narrow-to-region 8 58)\n\
         (list\n\
         (scan-sexps (point-min) 1)\n\
         (scan-sexps (point-min) 2)\n\
         (scan-sexps (point-min) -1)\n\
         (buffer-string)\n\
         (get-text-property (point-min) 'face)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_parse_partial_sexp_in_narrowed_let_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"pps\")))\n\
         (with-current-buffer buf\n\
         (insert \"(let ((a 1) (b 2) (c (list (+ a b) (* a b)))) (format \\\"%S\\\" c))\")\n\
         (put-text-property 1 5 'depth 0)\n\
         (put-text-property 6 9 'depth 1)\n\
         (narrow-to-region 6 62)\n\
         (let ((p1 (parse-partial-sexp (point-min) 10))\n\
         (p2 (parse-partial-sexp (point-min) 30))\n\
         (p3 (parse-partial-sexp (point-min) (point-max))))\n\
         (list (nth 0 p1) (nth 0 p2) (nth 0 p3)\n\
         (nth 3 p1) (nth 3 p2) (nth 3 p3)\n\
         (buffer-string)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_modify_syntax_entry_then_scan_in_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\\"w\\\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mse\")))\n\
         (with-current-buffer buf\n\
         (insert \"foo-bar/baz*quux\")\n\
         (modify-syntax-entry ?/ \\\"w\\\")\n\
         (modify-syntax-entry ?* \\\"w\\\")\n\
         (put-text-property 1 4 'word 'first)\n\
         (put-text-property 5 8 'word 'second)\n\
         (narrow-to-region 4 14)\n\
         (let ((w1 (buffer-substring (point-min) (progn (forward-word 1) (point))))\n\
         (w2 (progn (forward-word 1) (buffer-substring (1+ (point)) (progn (forward-word 1) (point))))))\n\
         (list w1 w2 (buffer-string)\n\
         (get-text-property 1 'word))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_forward_comment_with_syntax_change_and_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"fcs\")))\n\
         (with-current-buffer buf\n\
         (insert \"/* block comment */ code /* another */ more\")\n\
         (modify-syntax-entry ?/ \\\" 14\\\")\n\
         (modify-syntax-entry ?* \\\" 23\\\")\n\
         (put-text-property 1 21 'role 'comment)\n\
         (put-text-property 22 27 'role 'code)\n\
         (goto-char 1)\n\
         (let ((r1 (forward-comment 1))\n\
         (p1 (point))\n\
         (r2 (forward-comment 1))\n\
         (p2 (point)))\n\
         (list r1 p1 r2 p2\n\
         (get-text-property 1 'role)\n\
         (get-text-property 22 'role)\n\
         (buffer-string)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_scan_lists_nested_parens_with_overlay_arrows() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"sln\")))\n\
         (with-current-buffer buf\n\
         (insert \"((a (b c)) (d (e f)) (g (h i)))\")\n\
         (dotimes (i 4)\n\
         (let ((s (+ 1 (* i 8))))\n\
         (put-text-property s (+ s 7) 'group i)))\n\
         (let ((ov (make-overlay 1 3)))\n\
         (overlay-put ov 'paren 'open)\n\
         (list\n\
         (scan-lists 1 1 0)\n\
         (scan-lists 1 2 0)\n\
         (scan-lists 1 -1 0)\n\
         (scan-lists 2 1 1)\n\
         (overlay-start ov)\n\
         (overlay-end ov)\n\
         (get-text-property 2 'group)\n\
         (get-text-property 10 'group)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_parse_partial_sexp_with_comment_syntax_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\\"<\\\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ppc\")))\n\
         (with-current-buffer buf\n\
         (insert \"(progn ;; comment\\n  (list 1 2) ;; another\\n  (+ 3 4))\")\n\
         (modify-syntax-entry ?\\; \\\"<\\\")\n\
         (modify-syntax-entry ?\\n \\\">\\\")\n\
         (put-text-property 1 6 'form 'head)\n\
         (let ((p1 (parse-partial-sexp 1 20))\n\
         (p2 (parse-partial-sexp 1 (point-max))))\n\
         (list (nth 0 p1) (nth 0 p2)\n\
         (nth 4 p1) (nth 4 p2)\n\
         (buffer-string)\n\
         (get-text-property 1 'form)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_syntax_table_copy_then_parse_in_narrowed_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\\"_\\\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"stc\")))\n\
         (with-current-buffer buf\n\
         (insert \"aaa.bbb:ccc!ddd\")\n\
         (let ((st (copy-syntax-table (syntax-table))))\n\
         (with-syntax-table st\n\
         (modify-syntax-entry ?. \\\"_\\\")\n\
         (modify-syntax-entry ?: \\\"_\\\")\n\
         (modify-syntax-entry ?! \\\"_\\\")\n\
         (put-text-property 1 4 'tok 1)\n\
         (put-text-property 5 8 'tok 2)\n\
         (narrow-to-region 3 12)\n\
         (goto-char (point-min))\n\
         (let ((w (thing-at-point 'word)))\n\
         (forward-word 1)\n\
         (let ((w2 (thing-at-point 'word)))\n\
         (list w w2\n\
         (buffer-string)\n\
         (get-text-property (point-min) 'tok))))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_forward_word_boundary_with_prop_changes_mid_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"fwb\")))\n\
         (with-current-buffer buf\n\
         (insert \"hello world foo bar baz\")\n\
         (put-text-property 1 6 'w 1)\n\
         (put-text-property 7 12 'w 2)\n\
         (put-text-property 13 16 'w 3)\n\
         (goto-char 1)\n\
         (let ((positions nil))\n\
         (dotimes (_ 5)\n\
         (push (point) positions)\n\
         (forward-word 1))\n\
         (narrow-to-region 7 16)\n\
         (goto-char (point-min))\n\
         (dotimes (_ 3)\n\
         (push (list (point) (get-text-property (point) 'w)) positions)\n\
         (forward-word 1))\n\
         (nreverse positions)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_kill_word_yank_in_narrowed_with_syntax_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"kyn\")))\n\
         (with-current-buffer buf\n\
         (insert \"alpha beta gamma delta epsilon\")\n\
         (put-text-property 1 6 'pos 1)\n\
         (put-text-property 7 12 'pos 2)\n\
         (put-text-property 13 19 'pos 3)\n\
         (put-text-property 20 26 'pos 4)\n\
         (narrow-to-region 7 19)\n\
         (goto-char (point-min))\n\
         (kill-word 1)\n\
         (goto-char (point-min))\n\
         (yank)\n\
         (list (buffer-string)\n\
         (cl-loop for i from (point-min) to (point-max)\n\
         collect (cons i (get-text-property i 'pos))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_backward_up_list_through_overlay_and_prop_zones() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (scan-error \"Containing expression ends prematurely\" 1 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"bul\")))\n\
         (with-current-buffer buf\n\
         (insert \"(outer (middle (inner) tail) end)\")\n\
         (let ((ov1 (make-overlay 1 8))\n\
         (ov2 (make-overlay 9 18)))\n\
         (overlay-put ov1 'level 'outer)\n\
         (overlay-put ov2 'level 'inner)\n\
         (put-text-property 1 8 'depth 1)\n\
         (put-text-property 9 18 'depth 2)\n\
         (put-text-property 19 30 'depth 3)\n\
         (goto-char 14)\n\
         (let ((p1 (point))\n\
         (up1 (scan-lists (point) -1 1))\n\
         (up2 (scan-lists (point) -2 1)))\n\
         (list p1 up1 up2\n\
         (get-text-property up1 'depth)\n\
         (get-text-property up2 'depth)\n\
         (overlay-get ov1 'level)\n\
         (overlay-get ov2 'level))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

//! Deep stress: rectangle ops + region + kill-ring-yank + undo + textprop combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_kill_rectangle_yank_rectangle_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"kry\")))\n\
         (with-current-buffer buf\n\
         (insert \"LINE1 AAAA\\nLINE2 BBBB\\nLINE3 CCCC\\n\")\n\
         (put-text-property 1 6 'row 1)\n\
         (put-text-property 7 11 'row 1)\n\
         (put-text-property 12 17 'row 2)\n\
         (put-text-property 18 22 'row 2)\n\
         (put-text-property 23 28 'row 3)\n\
         (put-text-property 29 33 'row 3)\n\
         (undo-boundary)\n\
         (goto-char 7)\n\
         (delete-region 7 11)\n\
         (insert \"XXXX\")\n\
         (undo-boundary)\n\
         (goto-char 20)\n\
         (delete-region 20 24)\n\
         (insert \"YYYY\")\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (r7 (get-text-property 7 'row))\n\
         (r20 (get-text-property 20 'row)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s r7 r20\n\
         (buffer-string)\n\
         (get-text-property 7 'row)\n\
         (get-text-property 20 'row)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_delete_indentation_undo_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"diu\")))\n\
         (with-current-buffer buf\n\
         (insert \"line one\\n  line two\\n    line three\")\n\
         (put-text-property 1 9 'indent 0)\n\
         (put-text-property 10 12 'indent 2)\n\
         (put-text-property 12 19 'indent 2)\n\
         (put-text-property 20 24 'indent 4)\n\
         (put-text-property 24 34 'indent 4)\n\
         (undo-boundary)\n\
         (goto-char 10)\n\
         (delete-region 10 12)\n\
         (undo-boundary)\n\
         (goto-char 18)\n\
         (delete-region 18 22)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (i1 (get-text-property 1 'indent))\n\
         (i10 (get-text-property 10 'indent)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s i1 i10\n\
         (buffer-string)\n\
         (get-text-property 1 'indent)\n\
         (get-text-property 10 'indent)\n\
         (get-text-property 20 'indent)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_kill_whole_line_undo_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"kwl\")))\n\
         (with-current-buffer buf\n\
         (insert \"line1 content\\nline2 content\\nline3 content\\nline4 content\")\n\
         (put-text-property 1 14 'lnum 1)\n\
         (put-text-property 15 28 'lnum 2)\n\
         (put-text-property 29 42 'lnum 3)\n\
         (put-text-property 43 56 'lnum 4)\n\
         (undo-boundary)\n\
         (goto-char 15)\n\
         (delete-region 15 29)\n\
         (undo-boundary)\n\
         (goto-char 15)\n\
         (delete-region 15 29)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (l1 (get-text-property 1 'lnum))\n\
         (l15 (get-text-property 15 'lnum)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s l1 l15\n\
         (buffer-string)\n\
         (get-text-property 1 'lnum)\n\
         (get-text-property 15 'lnum)\n\
         (get-text-property 29 'lnum)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_append_to_kill_ring_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"aku\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAA BBB CCC DDD EEE\")\n\
         (put-text-property 1 4 'word 1)\n\
         (put-text-property 5 8 'word 2)\n\
         (put-text-property 9 12 'word 3)\n\
         (put-text-property 13 16 'word 4)\n\
         (put-text-property 17 20 'word 5)\n\
         (undo-boundary)\n\
         (kill-region 5 8)\n\
         (undo-boundary)\n\
         (kill-region 9 12)\n\
         (undo-boundary)\n\
         (kill-region 9 12)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (kr (car kill-ring)))\n\
         (primitive-undo 3 buffer-undo-list)\n\
         (list s kr\n\
         (buffer-string)\n\
         (get-text-property 5 'word)\n\
         (get-text-property 9 'word)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_yank_pop_undo_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ypu\")))\n\
         (with-current-buffer buf\n\
         (insert \"source buffer\")\n\
         (let ((m1 (copy-marker 1))\n\
         (m2 (copy-marker 8)))\n\
         (kill-region 1 8)\n\
         (kill-region 1 7)\n\
         (kill-region 1 1)\n\
         (undo-boundary)\n\
         (insert (current-kill 0))\n\
         (undo-boundary)\n\
         (let ((s1 (buffer-string)))\n\
         (delete-region 1 (1+ (length (current-kill 0))))\n\
         (insert (current-kill 1))\n\
         (undo-boundary)\n\
         (let ((s2 (buffer-string)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s1 s2\n\
         (buffer-string)\n\
         (marker-position m1)\n\
         (marker-position m2)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_region_active_with_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 3 3)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ran\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGHIJ\\nKLMNOPQRST\\nUVWXYZ\")\n\
         (put-text-property 1 11 'row 1)\n\
         (put-text-property 12 22 'row 2)\n\
         (put-text-property 23 28 'row 3)\n\
         (undo-boundary)\n\
         (narrow-to-region 3 20)\n\
         (let ((s-narrow (buffer-string))\n\
         (r-min (get-text-property (point-min) 'row)))\n\
         (undo-boundary)\n\
         (goto-char (point-min))\n\
         (delete-region (point-min) (+ (point-min) 5))\n\
         (undo-boundary)\n\
         (widen)\n\
         (let ((s-wide (buffer-string))\n\
         (r1 (get-text-property 1 'row))\n\
         (r3 (get-text-property 3 'row)))\n\
         (primitive-undo 3 buffer-undo-list)\n\
         (list s-narrow r-min s-wide r1 r3\n\
         (buffer-string)\n\
         (get-text-property 1 'row)\n\
         (get-text-property 3 'row))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_exchange_point_mark_undo_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 17 17)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"epm\")))\n\
         (with-current-buffer buf\n\
         (insert \"The quick brown fox jumps\")\n\
         (put-text-property 1 4 'word 1)\n\
         (put-text-property 5 10 'word 2)\n\
         (put-text-property 11 16 'word 3)\n\
         (put-text-property 17 20 'word 4)\n\
         (put-text-property 21 26 'word 5)\n\
         (goto-char 5)\n\
         (set-mark 20)\n\
         (let ((before (list (point) (mark t))))\n\
         (undo-boundary)\n\
         (delete-region (region-beginning) (region-end))\n\
         (undo-boundary)\n\
         (let ((s (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list before s\n\
         (buffer-string)\n\
         (get-text-property 5 'word)\n\
         (get-text-property 11 'word)\n\
         (get-text-property 17 'word))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_kill_region_with_overlay_bounds_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 12 18)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"kro\")))\n\
         (with-current-buffer buf\n\
         (insert \"HEADERBODYFOOTER\")\n\
         (let ((ov-h (make-overlay 1 7))\n\
         (ov-b (make-overlay 7 12))\n\
         (ov-f (make-overlay 12 18)))\n\
         (overlay-put ov-h 'section 'head)\n\
         (overlay-put ov-b 'section 'body)\n\
         (overlay-put ov-f 'section 'foot)\n\
         (put-text-property 1 7 'zone 'head)\n\
         (put-text-property 7 12 'zone 'body)\n\
         (put-text-property 12 18 'zone 'foot)\n\
         (undo-boundary)\n\
         (kill-region 7 12)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (ob (overlay-start ov-b))\n\
         (z7 (get-text-property 7 'zone)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s ob z7\n\
         (buffer-string)\n\
         (overlay-start ov-b)\n\
         (overlay-get ov-b 'section)\n\
         (get-text-property 7 'zone)\n\
         (overlay-get ov-h 'section)\n\
         (overlay-get ov-f 'section))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_copy_region_as_kill_yank_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"cky\")))\n\
         (with-current-buffer buf\n\
         (insert \"COPY THIS TEXT PLEASE\")\n\
         (put-text-property 1 5 'action 'copy)\n\
         (put-text-property 6 10 'action 'target)\n\
         (put-text-property 11 15 'action 'fill)\n\
         (put-text-property 16 22 'action 'please)\n\
         (copy-region-as-kill 6 10)\n\
         (undo-boundary)\n\
         (goto-char (point-max))\n\
         (insert \" \")\n\
         (yank)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (a1 (get-text-property 1 'action))\n\
         (a23 (get-text-property 23 'action)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s a1 a23\n\
         (buffer-string)\n\
         (get-text-property 1 'action)\n\
         (get-text-property 16 'action)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_insert_buffer_substring_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf1 (generate-new-buffer \"ib1\"))\n\
         (buf2 (generate-new-buffer \"ib2\")))\n\
         (with-current-buffer buf1\n\
         (insert \"SOURCE MATERIAL HERE\")\n\
         (put-text-property 1 7 'type 'src)\n\
         (put-text-property 8 16 'type 'src)\n\
         (put-text-property 17 21 'type 'src))\n\
         (with-current-buffer buf2\n\
         (insert \"DEST: \")\n\
         (put-text-property 1 6 'type 'dest)\n\
         (undo-boundary)\n\
         (insert-buffer-substring buf1 8 16)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (t1 (get-text-property 1 'type))\n\
         (t7 (get-text-property 7 'type)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s t1 t7\n\
         (buffer-string)\n\
         (get-text-property 1 'type)\n\
         (get-text-property 7 'type))))\n\
         (kill-buffer buf1)\n\
         (kill-buffer buf2)))",
        expect,
    );
}

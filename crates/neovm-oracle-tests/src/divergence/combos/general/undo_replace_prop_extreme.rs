//! Deep stress: extreme undo + replace + text property interval corruption probes.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_undo_30_replace_operations_prop_scan() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"u30\")))\n\
         (with-current-buffer buf\n\
         (insert \"AA.BB.CC.DD.EE.FF.GG.HH.II.JJ.KK.LL.MM.NN.OO\")\n\
         (dotimes (i 15)\n\
         (let ((start (+ 1 (* i 3))))\n\
         (put-text-property start (+ start 2) 'idx (1+ i))))\n\
         (undo-boundary)\n\
         (dotimes (i 15)\n\
         (goto-char 1)\n\
         (when (re-search-forward \"\\\\([A-Z]\\\\)\\\\1\" nil t)\n\
         (replace-match (format \"%02d\" (1+ i)))))\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (props (cl-loop for i from 1 to 44\n\
         collect (get-text-property i 'idx))))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list (substring s 0 20)\n\
         (cl-subseq props 0 10)\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to 44\n\
         collect (get-text-property i 'idx)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_insert_at_boundary_prop_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uib\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAAAAAAAAAAAAAAAAAA\")\n\
         (put-text-property 1 21 'owner 'block-a)\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"BBBB\")\n\
         (undo-boundary)\n\
         (goto-char 15)\n\
         (insert \"CCCC\")\n\
         (undo-boundary)\n\
         (goto-char 25)\n\
         (insert \"DDDD\")\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (props (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'owner))))\n\
         (primitive-undo 3 buffer-undo-list)\n\
         (list s props\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'owner)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_delete_across_prop_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"udb\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAABBBCCCDDDEEEFFFGGG\")\n\
         (put-text-property 1 4 'grp 'a)\n\
         (put-text-property 4 7 'grp 'b)\n\
         (put-text-property 7 10 'grp 'c)\n\
         (put-text-property 10 13 'grp 'd)\n\
         (put-text-property 13 16 'grp 'e)\n\
         (put-text-property 16 19 'grp 'f)\n\
         (put-text-property 19 22 'grp 'g)\n\
         (undo-boundary)\n\
         (delete-region 3 10)\n\
         (undo-boundary)\n\
         (delete-region 6 13)\n\
         (undo-boundary)\n\
         (delete-region 3 8)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (props (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'grp))))\n\
         (primitive-undo 3 buffer-undo-list)\n\
         (list s props\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'grp)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_replace_within_propertized_section() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 42 49)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"urw\")))\n\
         (with-current-buffer buf\n\
         (insert \"[open]content here[/open][open]more text[/open]\")\n\
         (put-text-property 1 7 'tag 'open)\n\
         (put-text-property 7 20 'tag 'text)\n\
         (put-text-property 20 27 'tag 'close)\n\
         (put-text-property 27 33 'tag 'open)\n\
         (put-text-property 33 42 'tag 'text)\n\
         (put-text-property 42 49 'tag 'close)\n\
         (undo-boundary)\n\
         (goto-char 7)\n\
         (re-search-forward \"content\")\n\
         (replace-match \"DATA\")\n\
         (undo-boundary)\n\
         (goto-char 31)\n\
         (re-search-forward \"more\")\n\
         (replace-match \"LESS\")\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (t7 (get-text-property 7 'tag))\n\
         (t11 (get-text-property 11 'tag))\n\
         (t15 (get-text-property 15 'tag))\n\
         (t31 (get-text-property 31 'tag))\n\
         (t35 (get-text-property 35 'tag)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s t7 t11 t15 t31 t35\n\
         (buffer-string)\n\
         (get-text-property 7 'tag)\n\
         (get-text-property 11 'tag)\n\
         (get-text-property 15 'tag)\n\
         (get-text-property 31 'tag)\n\
         (get-text-property 35 'tag)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_propertize_replace_propertize_cycle_5x() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"upc\")))\n\
         (with-current-buffer buf\n\
         (insert \"XXXXXXXXXXXXXXXXXXXX\")\n\
         (put-text-property 1 21 'gen 0)\n\
         (dotimes (i 5)\n\
         (undo-boundary)\n\
         (goto-char (+ 1 (* i 4)))\n\
         (delete-region (point) (+ (point) 4))\n\
         (insert (format \"%04d\" (1+ i)))\n\
         (put-text-property (+ 1 (* i 4)) (+ 5 (* i 4)) 'gen (1+ i)))\n\
         (let ((s (buffer-string))\n\
         (gens (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'gen))))\n\
         (dotimes (_ 5)\n\
         (primitive-undo 1 buffer-undo-list))\n\
         (list s gens\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'gen))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_with_overlays_covering_different_ranges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uod\")))\n\
         (with-current-buffer buf\n\
         (insert \"0123456789ABCDEFGHIJ\")\n\
         (let ((ov-even (make-overlay 1 20))\n\
         (ov-odd (make-overlay 2 19))\n\
         (ov-inner (make-overlay 5 16)))\n\
         (overlay-put ov-even 'parity 'even)\n\
         (overlay-put ov-odd 'parity 'odd)\n\
         (overlay-put ov-inner 'parity 'core)\n\
         (undo-boundary)\n\
         (goto-char 10)\n\
         (insert \"MMMM\")\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (delete-region 5 10)\n\
         (undo-boundary)\n\
         (let ((snap\n\
         (list (buffer-string)\n\
         (list (overlay-get ov-even 'parity)\n\
         (overlay-get ov-odd 'parity)\n\
         (overlay-get ov-inner 'parity))\n\
         (list (overlay-start ov-even) (overlay-end ov-even))\n\
         (list (overlay-start ov-odd) (overlay-end ov-odd))\n\
         (list (overlay-start ov-inner) (overlay-end ov-inner)))))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list snap\n\
         (buffer-string)\n\
         (list (overlay-get ov-even 'parity)\n\
         (overlay-get ov-odd 'parity)\n\
         (overlay-get ov-inner 'parity))\n\
         (list (overlay-start ov-even) (overlay-end ov-even))\n\
         (list (overlay-start ov-odd) (overlay-end ov-odd))\n\
         (list (overlay-start ov-inner) (overlay-end ov-inner)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_multibyte_with_prop_intervals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ump\")))\n\
         (with-current-buffer buf\n\
         (insert \"AA\\u00e9\\u00e8BB\\u4e16\\u754cCC\\u00f6\\u00fcDD\")\n\
         (put-text-property 1 3 'zone 'ascii)\n\
         (put-text-property 3 5 'zone 'accent)\n\
         (put-text-property 5 7 'zone 'ascii)\n\
         (put-text-property 7 9 'zone 'cjk)\n\
         (put-text-property 9 11 'zone 'ascii)\n\
         (put-text-property 11 13 'zone 'umlaut)\n\
         (put-text-property 13 15 'zone 'ascii)\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"\\u2603\\u2603\")\n\
         (put-text-property 5 7 'zone 'snowman)\n\
         (undo-boundary)\n\
         (goto-char 9)\n\
         (delete-region 9 11)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (z5 (get-text-property 5 'zone))\n\
         (z7 (get-text-property 7 'zone))\n\
         (z9 (get-text-property 9 'zone)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s z5 z7 z9\n\
         (buffer-string)\n\
         (get-text-property 5 'zone)\n\
         (get-text-property 7 'zone)\n\
         (get-text-property 9 'zone)\n\
         (get-text-property 11 'zone)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_with_interleaved_marker_moves() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uim\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGHIJKLMNOPQRSTUVWXYZ\")\n\
         (let ((m1 (copy-marker 5))\n\
         (m2 (copy-marker 10))\n\
         (m3 (copy-marker 15))\n\
         (m4 (copy-marker 20)))\n\
         (set-marker-insertion-type m1 nil)\n\
         (set-marker-insertion-type m2 t)\n\
         (set-marker-insertion-type m3 nil)\n\
         (set-marker-insertion-type m4 t)\n\
         (put-text-property 1 10 'half 'first)\n\
         (put-text-property 11 26 'half 'second)\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"111\")\n\
         (undo-boundary)\n\
         (goto-char 10)\n\
         (delete-region 10 15)\n\
         (undo-boundary)\n\
         (goto-char 15)\n\
         (insert \"222\")\n\
         (undo-boundary)\n\
         (let ((snap\n\
         (list (buffer-string)\n\
         (marker-position m1) (marker-position m2)\n\
         (marker-position m3) (marker-position m4)\n\
         (get-text-property 5 'half)\n\
         (get-text-property 15 'half))))\n\
         (primitive-undo 3 buffer-undo-list)\n\
         (list snap\n\
         (buffer-string)\n\
         (marker-position m1) (marker-position m2)\n\
         (marker-position m3) (marker-position m4)\n\
         (get-text-property 5 'half)\n\
         (get-text-property 15 'half))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_after_set_buffer_multibuf() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 10 10)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf1 (generate-new-buffer \"ub1\"))\n\
         (buf2 (generate-new-buffer \"ub2\")))\n\
         (with-current-buffer buf1\n\
         (insert \"BUF1-AAAA-BBBB\")\n\
         (put-text-property 1 5 'src 'buf1)\n\
         (put-text-property 6 10 'src 'buf1-a)\n\
         (put-text-property 10 14 'src 'buf1-b)\n\
         (undo-boundary)\n\
         (delete-region 6 14)\n\
         (undo-boundary))\n\
         (with-current-buffer buf2\n\
         (insert \"BUF2-CCCC-DDDD\")\n\
         (put-text-property 1 5 'src 'buf2)\n\
         (put-text-property 6 10 'src 'buf2-c)\n\
         (put-text-property 10 14 'src 'buf2-d)\n\
         (undo-boundary)\n\
         (goto-char 6)\n\
         (insert \"INSERT\")\n\
         (undo-boundary))\n\
         (with-current-buffer buf1\n\
         (primitive-undo 1 buffer-undo-list))\n\
         (with-current-buffer buf2\n\
         (primitive-undo 1 buffer-undo-list))\n\
         (list (with-current-buffer buf1 (buffer-string))\n\
         (with-current-buffer buf1 (get-text-property 6 'src))\n\
         (with-current-buffer buf1 (get-text-property 10 'src))\n\
         (with-current-buffer buf2 (buffer-string))\n\
         (with-current-buffer buf2 (get-text-property 6 'src))\n\
         (with-current-buffer buf2 (get-text-property 12 'src))))\n\
         (kill-buffer buf1)\n\
         (kill-buffer buf2)))",
        expect,
    );
}

#[test]
fn deficiency_undo_with_next_property_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"unp\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAABBBCCCDDDEEEFFF\")\n\
         (put-text-property 1 4 'val 1)\n\
         (put-text-property 4 7 'val 2)\n\
         (put-text-property 7 10 'val 3)\n\
         (put-text-property 10 13 'val 4)\n\
         (put-text-property 13 16 'val 5)\n\
         (put-text-property 16 19 'val 6)\n\
         (let ((boundaries\n\
         (cl-loop for pos = 1 then next\n\
         while pos\n\
         for next = (next-single-property-change pos 'val)\n\
         collect (list pos (get-text-property pos 'val))\n\
         while next)))\n\
         (undo-boundary)\n\
         (goto-char 4)\n\
         (insert \"XXX\")\n\
         (undo-boundary)\n\
         (goto-char 10)\n\
         (delete-region 10 16)\n\
         (undo-boundary)\n\
         (let ((after-ops\n\
         (cl-loop for pos = 1 then next\n\
         while pos\n\
         for next = (next-single-property-change pos 'val)\n\
         collect (list pos (get-text-property pos 'val))\n\
         while next)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (let ((after-undo\n\
         (cl-loop for pos = 1 then next\n\
         while pos\n\
         for next = (next-single-property-change pos 'val)\n\
         collect (list pos (get-text-property pos 'val))\n\
         while next)))\n\
         (list boundaries after-ops after-undo\n\
         (buffer-string))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

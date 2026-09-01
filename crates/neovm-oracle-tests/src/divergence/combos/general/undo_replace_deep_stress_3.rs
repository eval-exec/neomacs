//! Deep stress: undo+replace spanning prop boundaries + marker tracking + overlay stacks.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_replace_across_3_prop_boundaries_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"r3p\")))\n\
         (with-current-buffer buf\n\
         (insert \"1111222233334444555566667777\")\n\
         (dotimes (i 7)\n\
         (let ((s (+ 1 (* i 4))))\n\
         (put-text-property s (+ s 4) 'block (1+ i))))\n\
         (let ((m4 (copy-marker 4))\n\
         (m12 (copy-marker 12))\n\
         (m20 (copy-marker 20)))\n\
         (undo-boundary)\n\
         (goto-char 3)\n\
         (re-search-forward \"22223333\")\n\
         (replace-match \"AAAAAAAA\")\n\
         (put-text-property 3 11 'block 'merged)\n\
         (undo-boundary)\n\
         (goto-char 11)\n\
         (re-search-forward \"55556666\")\n\
         (replace-match \"BBBBBBBB\")\n\
         (put-text-property 11 19 'block 'merged)\n\
         (undo-boundary)\n\
         (let ((scan (cl-loop for i from 1 to (buffer-size)\n\
         collect (cons i (get-text-property i 'block)))))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list scan\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (cons i (get-text-property i 'block)))\n\
         (marker-position m4)\n\
         (marker-position m12)\n\
         (marker-position m20))))))\n\
         (kill-buffer buf)))", expect);
}

#[test]
fn deficiency_10_step_undo_with_propertize_each_step() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"10s\")))\n\
         (with-current-buffer buf\n\
         (insert \"BASE\")\n\
         (put-text-property 1 5 'gen 0)\n\
         (dotimes (i 10)\n\
         (undo-boundary)\n\
         (goto-char (point-max))\n\
         (insert (format \"-G%d\" (1+ i)))\n\
         (put-text-property\n\
         (- (point) 3) (point)\n\
         'gen (1+ i)))\n\
         (let ((full-gen (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'gen))))\n\
         (dotimes (_ 5)\n\
         (primitive-undo 1 buffer-undo-list))\n\
         (let ((after-5 (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'gen))))\n\
         (dotimes (_ 5)\n\
         (primitive-undo 1 buffer-undo-list))\n\
         (list full-gen after-5\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'gen)))))))\n\
         (kill-buffer buf)))", expect);
}

#[test]
fn deficiency_undo_with_overlay_and_prop_on_same_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uor\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAAABBBBBCCCCC\")\n\
         (let ((ov (make-overlay 6 11)))\n\
         (overlay-put ov 'face 'bold)\n\
         (overlay-put ov 'data 'middle)\n\
         (put-text-property 1 6 'region 'left)\n\
         (put-text-property 6 11 'region 'center)\n\
         (put-text-property 11 16 'region 'right)\n\
         (let ((m (copy-marker 6)))\n\
         (undo-boundary)\n\
         (goto-char 6)\n\
         (delete-region 6 11)\n\
         (undo-boundary)\n\
         (goto-char 6)\n\
         (insert \"XXXXX\")\n\
         (put-text-property 6 11 'region 'replaced)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (ov-s (overlay-start ov))\n\
         (ov-e (overlay-end ov))\n\
         (ov-d (overlay-get ov 'data))\n\
         (r6 (get-text-property 6 'region))\n\
         (mp (marker-position m)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s ov-s ov-e ov-d r6 mp\n\
         (buffer-string)\n\
         (overlay-start ov)\n\
         (overlay-end ov)\n\
         (overlay-get ov 'data)\n\
         (get-text-property 6 'region)\n\
         (marker-position m)))))))\n\
         (kill-buffer buf)))", expect);
}

#[test]
fn deficiency_undo_insert_between_two_different_prop_zones() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uib\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAACCCC\")\n\
         (put-text-property 1 5 'zone 'alpha)\n\
         (put-text-property 5 9 'zone 'gamma)\n\
         (let ((m-boundary (copy-marker 5)))\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"BBBB\")\n\
         (put-text-property 5 9 'zone 'beta)\n\
         (undo-boundary)\n\
         (let ((scan (cl-loop for i from 1 to (buffer-size)\n\
         collect (cons i (get-text-property i 'zone)))))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list scan\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (cons i (get-text-property i 'zone)))\n\
         (marker-position m-boundary))))))\n\
         (kill-buffer buf)))", expect);
}

#[test]
fn deficiency_undo_with_3_overlays_different_evaporate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"3oe\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAABBBCCCDDDEEE\")\n\
         (let ((ov1 (make-overlay 1 4))\n\
         (ov2 (make-overlay 5 8))\n\
         (ov3 (make-overlay 9 12)))\n\
         (overlay-put ov1 'evaporate nil)\n\
         (overlay-put ov2 'evaporate t)\n\
         (overlay-put ov3 'evaporate t)\n\
         (overlay-put ov1 'tag 'keep)\n\
         (overlay-put ov2 'tag 'vanish)\n\
         (overlay-put ov3 'tag 'vanish)\n\
         (put-text-property 1 5 'grp 1)\n\
         (put-text-property 5 9 'grp 2)\n\
         (put-text-property 9 13 'grp 3)\n\
         (put-text-property 13 16 'grp 4)\n\
         (undo-boundary)\n\
         (delete-region 5 12)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (o1 (and (overlay-start ov1) t))\n\
         (o2 (and (overlay-start ov2) t))\n\
         (o3 (and (overlay-start ov3) t)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s o1 o2 o3\n\
         (buffer-string)\n\
         (and (overlay-start ov1) t)\n\
         (and (overlay-start ov2) t)\n\
         (and (overlay-start ov3) t)\n\
         (overlay-get ov1 'tag)\n\
         (overlay-get ov2 'tag)\n\
         (overlay-get ov3 'tag)\n\
         (get-text-property 5 'grp)\n\
         (get-text-property 9 'grp))))))\n\
         (kill-buffer buf)))", expect);
}

#[test]
fn deficiency_undo_after_wide_replace_narrow_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"urn\")))\n\
         (with-current-buffer buf\n\
         (insert \"HEADER-AAA-BODY-BBB-FOOTER-CCC-END\")\n\
         (put-text-property 1 7 'section 'header)\n\
         (put-text-property 7 11 'section 'sep)\n\
         (put-text-property 11 16 'section 'body)\n\
         (put-text-property 16 20 'section 'sep)\n\
         (put-text-property 20 27 'section 'footer)\n\
         (put-text-property 27 31 'section 'sep)\n\
         (put-text-property 31 35 'section 'end)\n\
         (let ((m-body-start (copy-marker 11))\n\
         (m-body-end (copy-marker 16)))\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (while (re-search-forward \"AAA\\\\|BBB\\\\|CCC\" nil t)\n\
         (replace-match \"XXX\"))\n\
         (undo-boundary)\n\
         (narrow-to-region 11 27)\n\
         (undo-boundary)\n\
         (goto-char (point-min))\n\
         (insert \"MMMM\")\n\
         (undo-boundary)\n\
         (widen)\n\
         (let ((s (buffer-string))\n\
         (ms (marker-position m-body-start))\n\
         (me (marker-position m-body-end))\n\
         (s11 (get-text-property 11 'section)))\n\
         (primitive-undo 3 buffer-undo-list)\n\
         (list s ms me s11\n\
         (buffer-string)\n\
         (marker-position m-body-start)\n\
         (marker-position m-body-end)\n\
         (get-text-property 11 'section)\n\
         (get-text-property 16 'section)))))))\n\
         (kill-buffer buf)))", expect);
}

#[test]
fn deficiency_undo_5_sequential_replace_ops_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"u5r\")))\n\
         (with-current-buffer buf\n\
         (insert \"v1-v2-v3-v4-v5-v6-v7-v8-v9-v10\")\n\
         (cl-loop for i from 0 to 9\n\
         for start = (+ 1 (* i 3))\n\
         do (put-text-property start (+ start 3) 'slot i))\n\
         (let ((m5 (copy-marker 5))\n\
         (m15 (copy-marker 15))\n\
         (m25 (copy-marker 25)))\n\
         (undo-boundary)\n\
         (dotimes (i 5)\n\
         (goto-char 1)\n\
         (when (re-search-forward \"v[0-9]+\" nil t)\n\
         (replace-match (format \"r%d\" i))))\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (slots (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'slot))))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s\n\
         (cl-subseq slots 0 10)\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'slot))\n\
         (marker-position m5)\n\
         (marker-position m15)\n\
         (marker-position m25))))))\n\
         (kill-buffer buf)))", expect);
}

#[test]
fn deficiency_undo_after_3_nested_narrows_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""ERR (args-out-of-range 12 4)""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"3nn\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAABBBCCCDDDEEEFFFGGGHHHIIIJJJKKKLLL\")\n\
         (dotimes (i 12)\n\
         (let ((s (+ 1 (* i 3))))\n\
         (put-text-property s (+ s 3) 'tri i)))\n\
         (let ((m6 (copy-marker 6))\n\
         (m18 (copy-marker 18)))\n\
         (undo-boundary)\n\
         (narrow-to-region 4 30)\n\
         (undo-boundary)\n\
         (narrow-to-region 8 25)\n\
         (undo-boundary)\n\
         (narrow-to-region 12 20)\n\
         (undo-boundary)\n\
         (goto-char (point-min))\n\
         (insert \"ZZZZ\")\n\
         (put-text-property (point-min) 4 'tri 'inserted)\n\
         (undo-boundary)\n\
         (widen) (widen) (widen)\n\
         (let ((s (buffer-string))\n\
         (p1 (get-text-property 1 'tri))\n\
         (p4 (get-text-property 4 'tri))\n\
         (p8 (get-text-property 8 'tri)))\n\
         (primitive-undo 4 buffer-undo-list)\n\
         (list s p1 p4 p8\n\
         (buffer-string)\n\
         (get-text-property 1 'tri)\n\
         (get-text-property 4 'tri)\n\
         (get-text-property 8 'tri)\n\
         (marker-position m6)\n\
         (marker-position m18))))))\n\
         (kill-buffer buf)))", expect);
}

#[test]
fn deficiency_undo_with_propertized_kill_yank_across_bufs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK t""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf1 (generate-new-buffer \"ky1\"))\n\
         (buf2 (generate-new-buffer \"ky2\")))\n\
         (with-current-buffer buf1\n\
         (insert \"[IMPORTANT DATA HERE]\")\n\
         (put-text-property 1 2 'bracket 'open)\n\
         (put-text-property 2 18 'bracket 'content)\n\
         (put-text-property 18 19 'bracket 'close)\n\
         (put-text-property 2 11 'highlight 'yes)\n\
         (put-text-property 11 18 'highlight 'no)\n\
         (kill-region 2 18))\n\
         (with-current-buffer buf2\n\
         (insert \"TARGET: \")\n\
         (put-text-property 1 9 'role 'label)\n\
         (undo-boundary)\n\
         (yank)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (r1 (get-text-property 1 'role))\n\
         (b9 (get-text-property 9 'bracket))\n\
         (h9 (get-text-property 9 'highlight)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s r1 b9 h9\n\
         (buffer-string)\n\
         (get-text-property 1 'role)\n\
         (get-text-property 9 'bracket)\n\
         (get-text-property 9 'highlight))))\n\
         (kill-buffer buf1)\n\
         (kill-buffer buf2)))", expect);
}

#[test]
fn deficiency_undo_complex_15_step_edit_session() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"15e\")))\n\
         (with-current-buffer buf\n\
         (insert \"INITIAL\")\n\
         (put-text-property 1 8 'ver 0)\n\
         (dotimes (i 15)\n\
         (undo-boundary)\n\
         (goto-char (1+ (random (max 1 (buffer-size)))))\n\
         (let ((op (% i 4)))\n\
         (cond\n\
         ((= op 0)\n\
         (insert (format \"I%d\" i))\n\
         (put-text-property (1- (point)) (point) 'ver (1+ i)))\n\
         ((= op 1)\n\
         (when (> (buffer-size) 2)\n\
         (let ((p (1+ (random (max 1 (- (buffer-size) 2))))))\n\
         (delete-region p (+ p 1)))))\n\
         ((= op 2)\n\
         (goto-char 1)\n\
         (when (re-search-forward \"[A-Z0-9]\" nil t)\n\
         (replace-match (format \"R%d\" i))))\n\
         (t\n\
         (put-text-property 1 (min 5 (buffer-size)) 'ver (1+ i))))))\n\
         (let ((s (buffer-string))\n\
         (v1 (get-text-property 1 'ver)))\n\
         (dotimes (_ 5)\n\
         (primitive-undo 1 buffer-undo-list))\n\
         (list s v1\n\
         (buffer-string)\n\
         (get-text-property 1 'ver)\n\
         (> (buffer-size) 0)))))\n\
         (kill-buffer buf)))", expect);
}

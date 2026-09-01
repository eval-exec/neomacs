//! Deep stress: overlay + textprop + undo + marker + narrow mega-combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_overlay_stack_undo_after_delete_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"osu\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAAAAAAAAAAAAAAAAAA\")\n\
         (let ((ovs (cl-loop for i from 1 to 18 by 2\n\
         collect (let ((ov (make-overlay i (+ i 2))))\n\
         (overlay-put ov 'idx (1+ (/ (1- i) 2)))\n\
         ov))))\n\
         (put-text-property 1 11 'half 'left)\n\
         (put-text-property 11 21 'half 'right)\n\
         (let ((m5 (copy-marker 5)) (m10 (copy-marker 10)) (m15 (copy-marker 15)))\n\
         (undo-boundary)\n\
         (delete-region 5 15)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (ov-pos (mapcar (lambda (ov)\n\
         (list (overlay-start ov) (overlay-end ov)\n\
         (overlay-get ov 'idx))) ovs))\n\
         (p5 (marker-position m5))\n\
         (p10 (marker-position m10))\n\
         (p15 (marker-position m15))\n\
         (h1 (get-text-property 1 'half)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s ov-pos p5 p10 p15 h1\n\
         (buffer-string)\n\
         (mapcar (lambda (ov)\n\
         (list (overlay-start ov) (overlay-end ov)\n\
         (overlay-get ov 'idx))) ovs)\n\
         (marker-position m5)\n\
         (marker-position m10)\n\
         (marker-position m15)\n\
         (get-text-property 1 'half)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_narrow_overlay_prop_undo_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"nop\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAABBBCCCDDDEEEFFFGGGHHH\")\n\
         (let ((ovs (cl-loop for i from 1 to 22 by 3\n\
         collect (let ((ov (make-overlay i (+ i 3))))\n\
         (overlay-put ov 'grp (1+ (/ (1- i) 3)))\n\
         ov))))\n\
         (put-text-property 1 9 'section 'first)\n\
         (put-text-property 10 17 'section 'second)\n\
         (put-text-property 18 25 'section 'third)\n\
         (let ((m (copy-marker 9)))\n\
         (undo-boundary)\n\
         (narrow-to-region 4 21)\n\
         (undo-boundary)\n\
         (goto-char (point-min))\n\
         (insert \"XX\")\n\
         (undo-boundary)\n\
         (widen)\n\
         (let ((s (buffer-string))\n\
         (ov-data (mapcar (lambda (ov)\n\
         (and (overlay-start ov)\n\
         (overlay-get ov 'grp))) ovs))\n\
         (sec (get-text-property 4 'section))\n\
         (mp (marker-position m)))\n\
         (primitive-undo 3 buffer-undo-list)\n\
         (list s ov-data sec mp\n\
         (buffer-string)\n\
         (mapcar (lambda (ov)\n\
         (and (overlay-start ov)\n\
         (overlay-get ov 'grp))) ovs)\n\
         (get-text-property 4 'section)\n\
         (marker-position m)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_kill_yank_overlay_preserve() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"kyo\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAA[BBB]CCC[DDD]EEE\")\n\
         (let ((ov1 (make-overlay 4 7))\n\
         (ov2 (make-overlay 12 15)))\n\
         (overlay-put ov1 'bracket 'open)\n\
         (overlay-put ov2 'bracket 'close)\n\
         (put-text-property 1 4 'zone 'before)\n\
         (put-text-property 4 7 'zone 'inside)\n\
         (put-text-property 7 8 'zone 'between)\n\
         (put-text-property 8 11 'zone 'normal)\n\
         (put-text-property 12 15 'zone 'inside)\n\
         (put-text-property 15 18 'zone 'after)\n\
         (undo-boundary)\n\
         (kill-region 4 7)\n\
         (undo-boundary)\n\
         (goto-char 12)\n\
         (yank)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (o1-live (and (overlay-start ov1) t))\n\
         (o2-live (and (overlay-start ov2) t))\n\
         (o1-data (overlay-get ov1 'bracket))\n\
         (o2-data (overlay-get ov2 'bracket)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s o1-live o2-live o1-data o2-data\n\
         (buffer-string)\n\
         (and (overlay-start ov1) t)\n\
         (and (overlay-start ov2) t)\n\
         (overlay-get ov1 'bracket)\n\
         (overlay-get ov2 'bracket))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_5_overlay_layers_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"5ol\")))\n\
         (with-current-buffer buf\n\
         (insert \"XXXXXXXXXXXXXXXXXXXX\")\n\
         (let ((ov1 (make-overlay 1 21))\n\
         (ov2 (make-overlay 3 19))\n\
         (ov3 (make-overlay 5 17))\n\
         (ov4 (make-overlay 7 15))\n\
         (ov5 (make-overlay 9 13)))\n\
         (overlay-put ov1 'layer 1)\n\
         (overlay-put ov2 'layer 2)\n\
         (overlay-put ov3 'layer 3)\n\
         (overlay-put ov4 'layer 4)\n\
         (overlay-put ov5 'layer 5)\n\
         (put-text-property 1 11 'half 'left)\n\
         (put-text-property 11 21 'half 'right)\n\
         (let ((m (copy-marker 11)))\n\
         (undo-boundary)\n\
         (goto-char 11)\n\
         (insert \"YYYY\")\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (delete-region 5 9)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (layers (mapcar (lambda (ov) (overlay-get ov 'layer))\n\
         (list ov1 ov2 ov3 ov4 ov5)))\n\
         (starts (mapcar #'overlay-start (list ov1 ov2 ov3 ov4 ov5)))\n\
         (mp (marker-position m)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s layers starts mp\n\
         (buffer-string)\n\
         (mapcar (lambda (ov) (overlay-get ov 'layer))\n\
         (list ov1 ov2 ov3 ov4 ov5))\n\
         (mapcar #'overlay-start (list ov1 ov2 ov3 ov4 ov5))\n\
         (marker-position m)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_evaporate_overlay_delete_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"eod\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAABBBCCC\")\n\
         (let ((ov-a (make-overlay 1 4))\n\
         (ov-b (make-overlay 4 7))\n\
         (ov-c (make-overlay 7 10)))\n\
         (overlay-put ov-a 'evaporate t)\n\
         (overlay-put ov-b 'evaporate t)\n\
         (overlay-put ov-c 'evaporate t)\n\
         (overlay-put ov-a 'grp 'a)\n\
         (overlay-put ov-b 'grp 'b)\n\
         (overlay-put ov-c 'grp 'c)\n\
         (put-text-property 1 4 'zone 'a)\n\
         (put-text-property 4 7 'zone 'b)\n\
         (put-text-property 7 10 'zone 'c)\n\
         (undo-boundary)\n\
         (delete-region 4 7)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (oa (and (overlay-start ov-a) t))\n\
         (ob (and (overlay-start ov-b) t))\n\
         (oc (and (overlay-start ov-c) t)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s oa ob oc\n\
         (buffer-string)\n\
         (and (overlay-start ov-a) t)\n\
         (and (overlay-start ov-b) t)\n\
         (and (overlay-start ov-c) t)\n\
         (overlay-get ov-a 'grp)\n\
         (overlay-get ov-b 'grp)\n\
         (overlay-get ov-c 'grp)\n\
         (get-text-property 1 'zone)\n\
         (get-text-property 4 'zone)\n\
         (get-text-property 7 'zone))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_replace_match_overlay_move_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"rmo\")))\n\
         (with-current-buffer buf\n\
         (insert \"begin AAA middle BBB end CCC done\")\n\
         (let ((ov-aaa (make-overlay 7 10))\n\
         (ov-bbb (make-overlay 18 21))\n\
         (ov-ccc (make-overlay 26 29)))\n\
         (overlay-put ov-aaa 'value 'first)\n\
         (overlay-put ov-bbb 'value 'second)\n\
         (overlay-put ov-ccc 'value 'third)\n\
         (put-text-property 1 6 'section 'before)\n\
         (put-text-property 7 10 'section 'match)\n\
         (put-text-property 11 17 'section 'between)\n\
         (put-text-property 18 21 'section 'match)\n\
         (put-text-property 22 25 'section 'between)\n\
         (put-text-property 26 29 'section 'match)\n\
         (put-text-property 30 34 'section 'after)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (re-search-forward \"AAA\")\n\
         (replace-match \"XXX\")\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (re-search-forward \"BBB\")\n\
         (replace-match \"YYY\")\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (oa (list (overlay-start ov-aaa) (overlay-end ov-aaa)))\n\
         (ob (list (overlay-start ov-bbb) (overlay-end ov-bbb))))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s oa ob\n\
         (buffer-string)\n\
         (list (overlay-start ov-aaa) (overlay-end ov-aaa))\n\
         (list (overlay-start ov-bbb) (overlay-end ov-bbb))\n\
         (overlay-get ov-aaa 'value)\n\
         (overlay-get ov-bbb 'value)\n\
         (get-text-property 7 'section))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_overlay_invisible_text_prop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"oit\")))\n\
         (with-current-buffer buf\n\
         (insert \"SHOWHIDEHIDEHIDEHIDESHOW\")\n\
         (let ((ov1 (make-overlay 5 9))\n\
         (ov2 (make-overlay 9 13))\n\
         (ov3 (make-overlay 13 17))\n\
         (ov4 (make-overlay 17 21)))\n\
         (overlay-put ov1 'invisible 'hidden)\n\
         (overlay-put ov2 'invisible 'hidden)\n\
         (overlay-put ov3 'invisible 'hidden)\n\
         (overlay-put ov4 'invisible 'hidden)\n\
         (put-text-property 1 5 'vis 'show)\n\
         (put-text-property 5 21 'vis 'hide)\n\
         (put-text-property 21 25 'vis 'show)\n\
         (undo-boundary)\n\
         (overlay-put ov1 'invisible nil)\n\
         (undo-boundary)\n\
         (put-text-property 5 9 'vis 'shown)\n\
         (undo-boundary)\n\
         (let ((v5 (get-text-property 5 'vis))\n\
         (i1 (overlay-get ov1 'invisible)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list v5 i1\n\
         (buffer-string)\n\
         (get-text-property 5 'vis)\n\
         (overlay-get ov1 'invisible))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_overlay_face_priority_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ofp\")))\n\
         (with-current-buffer buf\n\
         (insert \"SHARED REGION\")\n\
         (let ((ov1 (make-overlay 1 14))\n\
         (ov2 (make-overlay 1 14))\n\
         (ov3 (make-overlay 1 14)))\n\
         (overlay-put ov1 'priority 10)\n\
         (overlay-put ov2 'priority 20)\n\
         (overlay-put ov3 'priority 30)\n\
         (overlay-put ov1 'face 'bold)\n\
         (overlay-put ov2 'face 'italic)\n\
         (overlay-put ov3 'face 'underline)\n\
         (put-text-property 1 14 'original t)\n\
         (undo-boundary)\n\
         (overlay-put ov3 'priority 5)\n\
         (overlay-put ov1 'priority 50)\n\
         (undo-boundary)\n\
         (let ((p1 (overlay-get ov1 'priority))\n\
         (p2 (overlay-get ov2 'priority))\n\
         (p3 (overlay-get ov3 'priority))\n\
         (f1 (overlay-get ov1 'face))\n\
         (f2 (overlay-get ov2 'face))\n\
         (f3 (overlay-get ov3 'face)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list p1 p2 p3 f1 f2 f3\n\
         (overlay-get ov1 'priority)\n\
         (overlay-get ov2 'priority)\n\
         (overlay-get ov3 'priority)\n\
         (overlay-get ov1 'face)\n\
         (overlay-get ov2 'face)\n\
         (overlay-get ov3 'face)\n\
         (get-text-property 1 'original))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_overlay_modify_after_kill_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"omk\")))\n\
         (with-current-buffer buf\n\
         (insert \"CONTAINER WITH CONTENT\")\n\
         (let ((ov (make-overlay 11 22)))\n\
         (overlay-put ov 'type 'content)\n\
         (put-text-property 1 10 'role 'container)\n\
         (put-text-property 11 22 'role 'content)\n\
         (undo-boundary)\n\
         (delete-region 11 22)\n\
         (undo-boundary)\n\
         (insert \"REPLACED\")\n\
         (put-text-property 11 19 'role 'replaced)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (ov-live (and (overlay-start ov) t))\n\
         (ov-type (overlay-get ov 'type))\n\
         (r11 (get-text-property 11 'role)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s ov-live ov-type r11\n\
         (buffer-string)\n\
         (and (overlay-start ov) t)\n\
         (overlay-get ov 'type)\n\
         (get-text-property 11 'role))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_many_overlays_same_point_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mos\")))\n\
         (with-current-buffer buf\n\
         (insert \"XXXXXXXXXXXXXXXXXXXX\")\n\
         (let ((ovs (cl-loop for i from 1 to 10\n\
         collect (let ((ov (make-overlay 5 16)))\n\
         (overlay-put ov 'idx i)\n\
         ov))))\n\
         (put-text-property 1 21 'base t)\n\
         (undo-boundary)\n\
         (goto-char 10)\n\
         (insert \"YYYY\")\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (all-at-same (cl-every (lambda (ov)\n\
         (= (overlay-start ov) 5)) ovs)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s all-at-same\n\
         (buffer-string)\n\
         (cl-every (lambda (ov)\n\
         (= (overlay-start ov) 5)) ovs)\n\
         (cl-every (lambda (ov)\n\
         (= (overlay-end ov) 16)) ovs)\n\
         (mapcar (lambda (ov) (overlay-get ov 'idx)) ovs)\n\
         (get-text-property 1 'base))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

//! Combo: registers + markers + overlays + textprop + undo + narrow.
//! Tests register save/restore interactions with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_register_save_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"rsm\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAABBBBCCCCDDDD\")\n\
         (put-text-property 1 5 'seg 'a)\n\
         (put-text-property 5 9 'seg 'b)\n\
         (put-text-property 9 13 'seg 'c)\n\
         (put-text-property 13 17 'seg 'd)\n\
         (let* ((ov (make-overlay 5 13))\n\
         (_ (overlay-put ov 'face 'highlight))\n\
         (m1 (make-marker))\n\
         (m2 (make-marker))\n\
         (_ (set-marker m1 5))\n\
         (_ (set-marker m2 13))\n\
         (_ (point-to-register ?a)))\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"XXXX\")\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (insert \"YYYY\")\n\
         (undo-boundary)\n\
         (let ((r (get-register ?a))\n\
         (mp1 (marker-position m1))\n\
         (mp2 (marker-position m2))\n\
         (os (overlay-start ov))\n\
         (oe (overlay-end ov))\n\
         (s (buffer-string)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list r mp1 mp2 os oe s\n\
         (buffer-string)\n\
         (marker-position m1)\n\
         (marker-position m2)\n\
         (overlay-start ov)\n\
         (overlay-end ov)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn combo_register_point_narrow_overlay_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"rnp\")))\n\
         (with-current-buffer buf\n\
         (insert \"alpha-beta-gamma-delta\")\n\
         (put-text-property 1 6 'zone 'a)\n\
         (put-text-property 7 11 'zone 'b)\n\
         (put-text-property 12 17 'zone 'c)\n\
         (put-text-property 18 23 'zone 'd)\n\
         (let* ((ov (make-overlay 7 17))\n\
         (_ (overlay-put ov 'priority 5))\n\
         (m (make-marker))\n\
         (_ (set-marker m 12)))\n\
         (point-to-register ?x)\n\
         (narrow-to-region 7 17)\n\
         (goto-char (point-min))\n\
         (re-search-forward \"beta\")\n\
         (let ((z (get-text-property (point-min) 'zone))\n\
         (mp (marker-position m))\n\
         (os (overlay-start ov))\n\
         (oe (overlay-end ov))\n\
         (bs (buffer-substring (point-min) (point-max))))\n\
         (widen)\n\
         (list z mp os oe bs\n\
         (buffer-string)\n\
         (overlay-start ov)\n\
         (overlay-end ov)\n\
         (marker-position m)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn combo_register_insert_with_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"riu\")))\n\
         (with-current-buffer buf\n\
         (insert \"First line\\nSecond line\\nThird line\")\n\
         (put-text-property 1 11 'line 1)\n\
         (put-text-property 12 23 'line 2)\n\
         (put-text-property 24 34 'line 3)\n\
         (let* ((ov (make-overlay 12 23))\n\
         (_ (overlay-put ov 'face 'bold))\n\
         (m (make-marker))\n\
         (_ (set-marker m 15)))\n\
         (set-register ?r \"INSERTED-TEXT\")\n\
         (point-to-register ?p)\n\
         (undo-boundary)\n\
         (goto-char 12)\n\
         (insert-register ?r)\n\
         (undo-boundary)\n\
         (let ((mp (marker-position m))\n\
         (os (overlay-start ov))\n\
         (oe (overlay-end ov))\n\
         (k (get-text-property 12 'line))\n\
         (s (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list mp os oe k s\n\
         (buffer-string)\n\
         (marker-position m)\n\
         (overlay-start ov)\n\
         (overlay-end ov)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn combo_register_rect_marker_overlay_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"rrm\")))\n\
         (with-current-buffer buf\n\
         (insert \"abcde\\nfghij\\nklmno\\npqrst\")\n\
         (put-text-property 1 6 'row 0)\n\
         (put-text-property 7 12 'row 1)\n\
         (put-text-property 13 18 'row 2)\n\
         (put-text-property 19 24 'row 3)\n\
         (let* ((ov (make-overlay 1 18))\n\
         (_ (overlay-put ov 'face 'region))\n\
         (m1 (make-marker))\n\
         (m2 (make-marker))\n\
         (_ (set-marker m1 3))\n\
         (_ (set-marker m2 15)))\n\
         (copy-rectangle-to-register ?z 1 18)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (delete-rectangle 1 18)\n\
         (undo-boundary)\n\
         (let ((mp1 (marker-position m1))\n\
         (mp2 (marker-position m2))\n\
         (os (overlay-start ov))\n\
         (oe (overlay-end ov))\n\
         (r0 (get-text-property 1 'row))\n\
         (s (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list mp1 mp2 os oe r0 s\n\
         (buffer-string)\n\
         (marker-position m1)\n\
         (marker-position m2)\n\
         (overlay-start ov)\n\
         (overlay-end ov)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn combo_register_narrow_undo_overlay_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"rnu\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAA-BBBB-CCCC-DDDD\")\n\
         (put-text-property 1 5 'seg 'a)\n\
         (put-text-property 6 10 'seg 'b)\n\
         (put-text-property 11 15 'seg 'c)\n\
         (put-text-property 16 20 'seg 'd)\n\
         (let* ((ov (make-overlay 6 15))\n\
         (_ (overlay-put ov 'face 'highlight))\n\
         (m (make-marker))\n\
         (_ (set-marker m 8)))\n\
         (point-to-register ?q)\n\
         (narrow-to-region 6 15)\n\
         (undo-boundary)\n\
         (goto-char (point-min))\n\
         (insert \"XX-\")\n\
         (undo-boundary)\n\
         (let ((mp (marker-position m))\n\
         (os (overlay-start ov))\n\
         (oe (overlay-end ov))\n\
         (k (get-text-property (point-min) 'seg))\n\
         (bs (buffer-substring (point-min) (point-max))))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (widen)\n\
         (list mp os oe k bs\n\
         (buffer-string)\n\
         (marker-position m)\n\
         (overlay-start ov)\n\
         (overlay-end ov)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

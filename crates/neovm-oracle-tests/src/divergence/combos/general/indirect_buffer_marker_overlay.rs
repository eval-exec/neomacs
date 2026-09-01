//! Combo: indirect buffers + clone-buffer + markers + overlays + textprop + undo.
//! Tests buffer cloning and indirect buffer interactions with markers and overlays.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_clone_buffer_marker_overlay_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"cln\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAA-BBBB-CCCC\")\n\
         (put-text-property 1 5 'zone 'a)\n\
         (put-text-property 6 10 'zone 'b)\n\
         (put-text-property 11 15 'zone 'c)\n\
         (let* ((ov (make-overlay 1 10))\n\
         (_ (overlay-put ov 'face 'bold))\n\
         (m1 (make-marker))\n\
         (m2 (make-marker))\n\
         (_ (set-marker m1 5))\n\
         (_ (set-marker m2 10))\n\
         (clone (clone-buffer \"cln-clone\")))\n\
         (with-current-buffer clone\n\
         (let* ((za (get-text-property 1 'zone))\n\
         (zb (get-text-property 6 'zone))\n\
         (z1 (marker-position m1))\n\
         (z2 (marker-position m2))\n\
         (o1 (overlay-start ov))\n\
         (o2 (overlay-end ov))\n\
         (bs (buffer-string)))\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (insert \"XXXX-\")\n\
         (undo-boundary)\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list za zb z1 z2 o1 o2 bs\n\
         (buffer-string)\n\
         (marker-position m1)\n\
         (marker-position m2)\n\
         (overlay-start ov)\n\
         (overlay-end ov))))\n\
         (kill-buffer clone)\n\
         (kill-buffer buf))))",
        expect,
    );
}

#[test]
fn combo_indirect_buffer_overlay_shared_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 14 18)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ind\")))\n\
         (with-current-buffer buf\n\
         (insert \"shared-text-here\")\n\
         (put-text-property 1 7 'part 'first)\n\
         (put-text-property 8 13 'part 'second)\n\
         (put-text-property 14 18 'part 'third))\n\
         (let ((ind (make-indirect-buffer buf \"ind-view\" t)))\n\
         (with-current-buffer ind\n\
         (let* ((ov (make-overlay 1 13))\n\
         (_ (overlay-put ov 'priority 10))\n\
         (m (make-marker))\n\
         (_ (set-marker m 8))\n\
         (p1 (get-text-property 1 'part))\n\
         (p2 (get-text-property 8 'part))\n\
         (mp (marker-position m)))\n\
         (undo-boundary)\n\
         (goto-char 8)\n\
         (insert \"INSERTED-\")\n\
         (undo-boundary)\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list p1 p2 mp\n\
         (buffer-string)\n\
         (marker-position m)\n\
         (overlay-start ov)\n\
         (overlay-end ov))))\n\
         (kill-buffer ind)\n\
         (kill-buffer buf))))",
        expect,
    );
}

#[test]
fn combo_clone_buffer_narrow_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable clone)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"cnm\")))\n\
         (with-current-buffer buf\n\
         (insert \"LINE1-LINE2-LINE3-LINE4\")\n\
         (put-text-property 1 6 'ln 1)\n\
         (put-text-property 7 12 'ln 2)\n\
         (put-text-property 13 18 'ln 3)\n\
         (put-text-property 19 24 'ln 4)\n\
         (let* ((ov (make-overlay 7 18))\n\
         (_ (overlay-put ov 'face 'highlight))\n\
         (m (make-marker))\n\
         (_ (set-marker m 15))\n\
         (clone (clone-buffer \"cnm-clone\")))\n\
         (with-current-buffer clone\n\
         (narrow-to-region 7 18)\n\
         (goto-char (point-min))\n\
         (let* ((ln2 (get-text-property 7 'ln))\n\
         (ln3 (get-text-property 13 'ln))\n\
         (mp (marker-position m))\n\
         (os (overlay-start ov))\n\
         (oe (overlay-end ov))\n\
         (bs (buffer-substring (point-min) (point-max))))\n\
         (widen)\n\
         (list ln2 ln3 mp os oe bs\n\
         (buffer-string)))))\n\
         (kill-buffer clone)\n\
         (kill-buffer buf))))",
        expect,
    );
}

#[test]
fn combo_clone_buffer_revert_undo_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable clone)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"cru\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAA-BBBB-CCCC\")\n\
         (put-text-property 1 5 'kind 'alpha)\n\
         (put-text-property 6 10 'kind 'beta)\n\
         (put-text-property 11 15 'kind 'gamma)\n\
         (let* ((m (make-marker))\n\
         (_ (set-marker m 6))\n\
         (ov (make-overlay 6 10))\n\
         (_ (overlay-put ov 'face 'region))\n\
         (clone (clone-buffer \"cru-clone\")))\n\
         (with-current-buffer clone\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (re-search-forward \"AAAA\")\n\
         (replace-match \"1111\")\n\
         (undo-boundary)\n\
         (goto-char 6)\n\
         (re-search-forward \"BBBB\")\n\
         (replace-match \"2222\")\n\
         (undo-boundary)\n\
         (let ((r1 (buffer-string))\n\
         (k1 (get-text-property 1 'kind))\n\
         (k6 (get-text-property 6 'kind))\n\
         (mp (marker-position m)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list r1 k1 k6 mp\n\
         (buffer-string)\n\
         (marker-position m)\n\
         (overlay-start ov)\n\
         (overlay-end ov)))))\n\
         (kill-buffer clone)\n\
         (kill-buffer buf))))",
        expect,
    );
}

#[test]
fn combo_multi_buffer_marker_cross_ref() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((b1 (generate-new-buffer \"mb1\"))\n\
         (b2 (generate-new-buffer \"mb2\")))\n\
         (with-current-buffer b1\n\
         (insert \"BUFFER-ONE-TEXT\")\n\
         (put-text-property 1 7 'buf 'one)\n\
         (put-text-property 8 11 'buf 'marker-zone))\n\
         (with-current-buffer b2\n\
         (insert \"BUFFER-TWO-TEXT\")\n\
         (put-text-property 1 7 'buf 'two))\n\
         (let* ((m1 (make-marker))\n\
         (m2 (make-marker))\n\
         (_ (with-current-buffer b1 (set-marker m1 8)))\n\
         (_ (with-current-buffer b2 (set-marker m2 4)))\n\
         (ov (with-current-buffer b1 (make-overlay 1 11))))\n\
         (with-current-buffer b1\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (insert \"PRE-\")\n\
         (undo-boundary)\n\
         (let ((mp1 (marker-position m1))\n\
         (mp2 (marker-position m2))\n\
         (os (overlay-start ov))\n\
         (oe (overlay-end ov))\n\
         (k (get-text-property 1 'buf))\n\
         (bs (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list mp1 mp2 os oe k bs\n\
         (marker-position m1)\n\
         (overlay-start ov)\n\
         (overlay-end ov))))\n\
         (kill-buffer b1)\n\
         (kill-buffer b2))))",
        expect,
    );
}

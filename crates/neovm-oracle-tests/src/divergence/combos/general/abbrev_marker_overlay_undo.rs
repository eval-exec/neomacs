//! Combo: abbrev tables + markers + textprop + undo + narrow + overlays.
//! Tests abbreviation expansion interactions with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_abbrev_expand_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"aeu\"))\n\
         (at (make-abbrev-table)))\n\
         (with-current-buffer buf\n\
         (define-abbrev at \"btw\" \"by the way\")\n\
         (insert \"btw is an abbreviation btw\")\n\
         (put-text-property 1 4 'kind 'abbrev)\n\
         (put-text-property 24 27 'kind 'abbrev)\n\
         (put-text-property 5 7 'kind 'connector)\n\
         (let* ((ov (make-overlay 1 27))\n\
         (_ (overlay-put ov 'face 'bold))\n\
         (m1 (make-marker))\n\
         (m2 (make-marker))\n\
         (_ (set-marker m1 4))\n\
         (_ (set-marker m2 27))\n\
         (local-abbrev-table at)\n\
         (abbrev-mode 1))\n\
         (undo-boundary)\n\
         (goto-char 3)\n\
         (expand-abbrev)\n\
         (undo-boundary)\n\
         (let* ((s (buffer-string))\n\
         (mp1 (marker-position m1))\n\
         (mp2 (marker-position m2))\n\
         (os (overlay-start ov))\n\
         (oe (overlay-end ov))\n\
         (k1 (get-text-property 1 'kind)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s mp1 mp2 os oe k1\n\
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
fn combo_abbrev_narrow_expand_with_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ane\"))\n\
         (at (make-abbrev-table)))\n\
         (with-current-buffer buf\n\
         (define-abbrev at \"emc\" \"Emacs\")\n\
         (define-abbrev at \"nmc\" \"Neomacs\")\n\
         (insert \"emc and nmc are editors\")\n\
         (put-text-property 1 4 'kind 'abbrev1)\n\
         (put-text-property 9 12 'kind 'abbrev2)\n\
         (put-text-property 5 8 'kind 'text)\n\
         (let* ((ov (make-overlay 1 12))\n\
         (_ (overlay-put ov 'priority 10))\n\
         (m (make-marker))\n\
         (_ (set-marker m 9))\n\
         (local-abbrev-table at))\n\
         (narrow-to-region 1 12)\n\
         (goto-char 3)\n\
         (expand-abbrev)\n\
         (let ((mp (marker-position m))\n\
         (os (overlay-start ov))\n\
         (oe (overlay-end ov))\n\
         (k (get-text-property 1 'kind))\n\
         (bs (buffer-substring (point-min) (point-max))))\n\
         (widen)\n\
         (list mp os oe k bs\n\
         (buffer-string)\n\
         (marker-position m)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn combo_abbrev_overlay_priority_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"aop\"))\n\
         (at (make-abbrev-table)))\n\
         (with-current-buffer buf\n\
         (define-abbrev at \"pls\" \"please\")\n\
         (insert \"pls help me pls\")\n\
         (put-text-property 1 4 'aid 1)\n\
         (put-text-property 12 15 'aid 2)\n\
         (let* ((ov1 (make-overlay 1 4))\n\
         (ov2 (make-overlay 12 15))\n\
         (_ (overlay-put ov1 'priority 1))\n\
         (_ (overlay-put ov2 'priority 2))\n\
         (m (make-marker))\n\
         (_ (set-marker m 4))\n\
         (local-abbrev-table at))\n\
         (undo-boundary)\n\
         (goto-char 3)\n\
         (expand-abbrev)\n\
         (undo-boundary)\n\
         (goto-char 14)\n\
         (expand-abbrev)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (mp (marker-position m))\n\
         (os1 (overlay-start ov1))\n\
         (oe1 (overlay-end ov1))\n\
         (os2 (overlay-start ov2))\n\
         (oe2 (overlay-end ov2)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s mp os1 oe1 os2 oe2\n\
         (buffer-string)\n\
         (marker-position m)\n\
         (overlay-start ov1)\n\
         (overlay-end ov1)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn combo_abbrev_multi_buffer_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((b1 (generate-new-buffer \"ab1\"))\n\
         (b2 (generate-new-buffer \"ab2\"))\n\
         (at (make-abbrev-table)))\n\
         (define-abbrev at \"thx\" \"thanks\")\n\
         (with-current-buffer b1\n\
         (insert \"thx for the help\")\n\
         (put-text-property 1 4 'src 'b1)\n\
         (let* ((ov (make-overlay 1 4))\n\
         (_ (overlay-put ov 'face 'bold))\n\
         (m (make-marker))\n\
         (_ (set-marker m 4))\n\
         (local-abbrev-table at))\n\
         (undo-boundary)\n\
         (goto-char 3)\n\
         (expand-abbrev))\n\
         (with-current-buffer b2\n\
         (insert \"thx a lot\")\n\
         (put-text-property 1 4 'src 'b2)\n\
         (let* ((m2 (make-marker))\n\
         (_ (set-marker m2 4))\n\
         (local-abbrev-table at))\n\
         (undo-boundary)\n\
         (goto-char 3)\n\
         (expand-abbrev))\n\
         (let ((s2 (buffer-string))\n\
         (mp2 (marker-position m2))\n\
         (k2 (get-text-property 1 'src)))\n\
         (with-current-buffer b1\n\
         (let ((s1 (buffer-string))\n\
         (mp1 (marker-position m))\n\
         (k1 (get-text-property 1 'src))\n\
         (os (overlay-start ov))\n\
         (oe (overlay-end ov)))\n\
         (list s1 mp1 k1 os oe s2 mp2 k2))))\n\
         (kill-buffer b1)\n\
         (kill-buffer b2)))",
        expect,
    );
}

#[test]
fn combo_abbrev_textprop_replace_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"atu\"))\n\
         (at (make-abbrev-table)))\n\
         (with-current-buffer buf\n\
         (define-abbrev at \"idk\" \"I don't know\")\n\
         (insert \"idk what idk means\")\n\
         (put-text-property 1 4 'tag 'first)\n\
         (put-text-property 12 15 'tag 'second)\n\
         (put-text-property 5 10 'tag 'text)\n\
         (let* ((ov (make-overlay 1 15))\n\
         (_ (overlay-put ov 'face 'highlight))\n\
         (m (make-marker))\n\
         (_ (set-marker m 12))\n\
         (local-abbrev-table at))\n\
         (undo-boundary)\n\
         (goto-char 3)\n\
         (expand-abbrev)\n\
         (put-text-property 1 13 'expanded t)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (mp (marker-position m))\n\
         (tg (get-text-property 1 'tag))\n\
         (ex (get-text-property 1 'expanded))\n\
         (os (overlay-start ov))\n\
         (oe (overlay-end ov)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s mp tg ex os oe\n\
         (buffer-string)\n\
         (marker-position m)\n\
         (get-text-property 1 'tag)\n\
         (get-text-property 1 'expanded)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

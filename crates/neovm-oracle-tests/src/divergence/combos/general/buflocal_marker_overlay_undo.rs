//! Combo: buffer-local variables + markers + overlays + undo + narrow + clone-buffer.
//! Tests buffer-local variable interactions with buffer cloning and undo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_buflocal_clone_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable clone)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"blc\")))\n\
         (with-current-buffer buf\n\
         (setq-local my-var 'original)\n\
         (setq-local my-count 0)\n\
         (insert \"AAAA-BBBB-CCCC\")\n\
         (put-text-property 1 5 'seg 'a)\n\
         (put-text-property 6 10 'seg 'b)\n\
         (put-text-property 11 15 'seg 'c)\n\
         (let* ((ov (make-overlay 6 10))\n\
         (_ (overlay-put ov 'face 'bold))\n\
         (m (make-marker))\n\
         (_ (set-marker m 8))\n\
         (clone (clone-buffer \"blc-clone\")))\n\
         (with-current-buffer clone\n\
         (setq-local my-var 'modified)\n\
         (setq-local my-count (1+ my-count))\n\
         (undo-boundary)\n\
         (goto-char 6)\n\
         (insert \"XX-\")\n\
         (undo-boundary)\n\
         (let* ((v1 my-var)\n\
         (c1 my-count)\n\
         (s1 (get-text-property 1 'seg))\n\
         (mp (marker-position m))\n\
         (os (overlay-start ov))\n\
         (oe (overlay-end ov))\n\
         (bs (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list v1 c1 s1 mp os oe bs\n\
         (buffer-string)\n\
         (marker-position m)))))\n\
         (kill-buffer clone)\n\
         (kill-buffer buf))))",
        expect,
    );
}

#[test]
fn combo_buflocal_narrow_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"bln\")))\n\
         (with-current-buffer buf\n\
         (setq-local depth 0)\n\
         (setq-local state 'init)\n\
         (insert \"LINE1-LINE2-LINE3\")\n\
         (put-text-property 1 6 'ln 1)\n\
         (put-text-property 7 12 'ln 2)\n\
         (put-text-property 13 18 'ln 3)\n\
         (let* ((ov (make-overlay 7 12))\n\
         (_ (overlay-put ov 'priority 5))\n\
         (m (make-marker))\n\
         (_ (set-marker m 9)))\n\
         (setq-local depth (1+ depth))\n\
         (setq-local state 'narrowed)\n\
         (narrow-to-region 7 12)\n\
         (undo-boundary)\n\
         (goto-char (point-min))\n\
         (insert \"XX-\")\n\
         (undo-boundary)\n\
         (let ((d depth)\n\
         (st state)\n\
         (ln (get-text-property 7 'ln))\n\
         (mp (marker-position m))\n\
         (os (overlay-start ov))\n\
         (oe (overlay-end ov))\n\
         (bs (buffer-substring (point-min) (point-max))))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (widen)\n\
         (list d st ln mp os oe bs\n\
         (buffer-string)\n\
         (marker-position m)\n\
         (overlay-start ov)\n\
         (overlay-end ov)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn combo_buflocal_multi_clone_shared_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"blm\")))\n\
         (with-current-buffer buf\n\
         (setq-local flag 'base)\n\
         (setq-local counter 10)\n\
         (insert \"AAAA-BBBB-CCCC\")\n\
         (put-text-property 1 5 'zone 'x)\n\
         (put-text-property 6 10 'zone 'y)\n\
         (put-text-property 11 15 'zone 'z)\n\
         (let* ((ov (make-overlay 1 15))\n\
         (_ (overlay-put ov 'face 'region))\n\
         (m (make-marker))\n\
         (_ (set-marker m 6))\n\
         (c1 (clone-buffer \"blm-c1\"))\n\
         (c2 (clone-buffer \"blm-c2\")))\n\
         (with-current-buffer c1\n\
         (setq-local flag 'clone1)\n\
         (setq-local counter (+ counter 100))\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (insert \"PRE-\")\n\
         (undo-boundary)\n\
         (let ((f flag)\n\
         (c counter)\n\
         (s (buffer-string))\n\
         (mp (marker-position m)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list f c s mp\n\
         (buffer-string)\n\
         (marker-position m))))\n\
         (with-current-buffer c2\n\
         (setq-local flag 'clone2)\n\
         (setq-local counter (+ counter 200))\n\
         (let ((f flag)\n\
         (c counter))\n\
         (list f c)))\n\
         (kill-buffer c1)\n\
         (kill-buffer c2)\n\
         (kill-buffer buf))))",
        expect,
    );
}

#[test]
fn combo_buflocal_defvar_setq_marker_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar bl-test-var 'default)\n\
         (let ((buf (generate-new-buffer \"bld\")))\n\
         (with-current-buffer buf\n\
         (setq-local bl-test-var 'buffer-val)\n\
         (insert \"AAAA-BBBB-CCCC\")\n\
         (put-text-property 1 5 'kind 'a)\n\
         (put-text-property 6 10 'kind 'b)\n\
         (put-text-property 11 15 'kind 'c)\n\
         (let* ((ov (make-overlay 6 10))\n\
         (_ (overlay-put ov 'face 'highlight))\n\
         (m (make-marker))\n\
         (_ (set-marker m 8)))\n\
         (undo-boundary)\n\
         (goto-char 6)\n\
         (insert \"XX-\")\n\
         (setq-local bl-test-var 'after-insert)\n\
         (undo-boundary)\n\
         (let ((v bl-test-var)\n\
         (k (get-text-property 1 'kind))\n\
         (mp (marker-position m))\n\
         (os (overlay-start ov))\n\
         (oe (overlay-end ov))\n\
         (s (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list v k mp os oe s\n\
         bl-test-var\n\
         (buffer-string)\n\
         (marker-position m)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn combo_buflocal_hook_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (wrong-number-of-arguments (closure (t) nil (setq my-log (cons (buffer-string) my-log))) 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"blh\"))\n\
         (log nil))\n\
         (with-current-buffer buf\n\
         (setq-local my-log log)\n\
         (insert \"AAAA-BBBB-CCCC\")\n\
         (put-text-property 1 5 'seg 'a)\n\
         (put-text-property 6 10 'seg 'b)\n\
         (put-text-property 11 15 'seg 'c)\n\
         (let* ((ov (make-overlay 6 10))\n\
         (_ (overlay-put ov 'priority 3))\n\
         (m (make-marker))\n\
         (_ (set-marker m 8))\n\
         (hook-fn (lambda () (push (buffer-string) my-log))))\n\
         (add-hook 'before-change-functions hook-fn nil t)\n\
         (undo-boundary)\n\
         (goto-char 6)\n\
         (insert \"XX\")\n\
         (undo-boundary)\n\
         (let ((l my-log)\n\
         (s (buffer-string))\n\
         (mp (marker-position m))\n\
         (os (overlay-start ov))\n\
         (oe (overlay-end ov))\n\
         (k (get-text-property 1 'seg)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list l s mp os oe k\n\
         (buffer-string)\n\
         (marker-position m)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

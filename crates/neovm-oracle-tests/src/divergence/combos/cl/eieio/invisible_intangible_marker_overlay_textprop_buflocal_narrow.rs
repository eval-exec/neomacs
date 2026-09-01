//! Combo: cl-eieio overlay intangible/invisible + markers + overlays + textprop + buflocal + narrow + undo.
//! Tests intangible/invisible overlay properties with EIEIO objects affecting navigation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_invisible_overlay_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass visibility-zone ()
    ((name :initarg :name :accessor vz-name :initform "")
     (start :initarg :start :accessor vz-start :initform 1)
     (end :initarg :end :accessor vz-end :initform 1)
     (hidden :initarg :hidden :accessor vz-hidden :initform nil)))
  (let* ((buf (generate-new-buffer "iv1"))
         (vz1 (visibility-zone :name "visible"))
         (vz2 (visibility-zone :name "hidden")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'vz vz1)
      (put-text-property 6 9 'vz vz2)
      (put-text-property 10 13 'vz vz1)
      (setq-local my-vzs (list vz1 vz2))
      (let* ((ov (make-overlay 6 9))
             (_ (overlay-put ov 'invisible t))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 1))
             (results nil))
        (undo-boundary)
        (setf (vz-hidden vz2) t)
        (push (list 'at-1 (point)) results)
        (goto-char 5)
        (push (list 'at-5 (point)) results)
        (goto-char 6)
        (push (list 'at-6 (point)) results)
        (goto-char 10)
        (push (list 'at-10 (point)) results)
        (let ((visible-text (buffer-substring-no-properties 1 13)))
          (push (list 'visible visible-text) results))
        (let ((all-text (buffer-string)))
          (push (list 'all all-text) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s vz1=%s vz2=%s m=%d"
                       results (vz-hidden vz1) (vz-hidden vz2) (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'vis-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-vzs))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_invisible_multiple_layers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass hide-layer ()
    ((layer-name :initarg :layer-name :accessor hl-name :initform "")
     (invisible-val :initarg :invisible-val :accessor hl-val :initform nil)))
  (let* ((buf (generate-new-buffer "iv2"))
         (hl1 (hide-layer :layer-name "test" :invisible-val 'test))
         (hl2 (hide-layer :layer-name "fold" :invisible-val 'fold)))
    (with-current-buffer buf
      (insert "AAAAAAAAAAAA")
      (setq-local my-layers (list hl1 hl2))
      (let* ((ov1 (make-overlay 1 5))
             (ov2 (make-overlay 5 9))
             (ov3 (make-overlay 9 13))
             (_ (overlay-put ov1 'invisible (hl-val hl1)))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'invisible (hl-val hl2)))
             (_ (overlay-put ov2 'priority 2))
             (_ (overlay-put ov3 'invisible (hl-val hl1)))
             (_ (overlay-put ov3 'priority 3))
             (m (make-marker))
             (_ (set-marker m 1))
             (results nil))
        (undo-boundary)
        (let ((add-invisible (list 'test (add-to-invisibility-spec 'test))))
          (push (list 'add-spec add-invisible) results))
        (push (list 'buf-string (buffer-string)) results)
        (remove-from-invisibility-spec 'test)
        (push (list 'after-remove (buffer-string)) results)
        (add-to-invisibility-spec 'fold)
        (push (list 'after-fold (buffer-string)) results)
        (remove-from-invisibility-spec 'fold)
        (push (list 'after-unfold (buffer-string)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d" results (marker-position m)))
        (set-marker m 5)
        (put-text-property (1- (point-max)) (point-max) 'inv-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os1 (overlay-start ov1))
              (oe3 (overlay-end ov3))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os1 oe3 bs
                (marker-position m)
                (buffer-string)
                my-layers))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_invisible_narrow_visible() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass narrow-vis ()
    ((narrow-range :initarg :narrow-range :accessor nv-range :initform nil)
     (visible-text :initarg :visible-text :accessor nv-text :initform "")))
  (let* ((buf (generate-new-buffer "iv3"))
         (nv1 (narrow-vis))
         (nv2 (narrow-vis)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'vis 'show)
      (put-text-property 6 10 'vis 'hide)
      (put-text-property 11 15 'vis 'show)
      (put-text-property 16 20 'vis 'hide)
      (setq-local my-nvs (list nv1 nv2))
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'invisible t))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 6))
             (results nil))
        (undo-boundary)
        (setf (nv-range nv1) '(1 . 15)
              (nv-text nv1) (buffer-substring-no-properties 1 15))
        (push (list 'full-visible (nv-text nv1)) results)
        (save-restriction
          (narrow-to-region 6 15)
          (setf (nv-range nv2) (list (point-min) (point-max))
                (nv-text nv2) (buffer-substring-no-properties (point-min) (point-max)))
          (push (list 'narrow-visible (nv-range nv2) (nv-text nv2)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s nv1=%s nv2=%s m=%d"
                       results
                       (list (nv-range nv1) (nv-text nv1))
                       (list (nv-range nv2) (nv-text nv2))
                       (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'nv-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-nvs))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_intangible_forward_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass skip-zone ()
    ((label :initarg :label :accessor sz-label :initform "")
     (range :initarg :range :accessor sz-range :initform nil)
     (skipped :initarg :skipped :accessor sz-skipped :initform 0)))
  (let* ((buf (generate-new-buffer "iv4"))
         (sz1 (skip-zone :label "gap" :range '(5 . 9) :skipped 0)))
    (with-current-buffer buf
      (insert "AAAA-GAPPP-CCCC")
      (put-text-property 1 5 'zone 'normal)
      (put-text-property 6 10 'zone 'gap)
      (put-text-property 11 14 'zone 'normal)
      (setq-local my-sz sz1)
      (let* ((ov (make-overlay 5 10))
             (_ (overlay-put ov 'intangible 1))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 1))
             (positions nil))
        (undo-boundary)
        (goto-char 1)
        (while (< (point) (point-max))
          (let ((old-pos (point)))
            (forward-char 1)
            (when (= (point) old-pos) (push 'stuck positions))
            (push (point) positions)))
        (setq positions (reverse positions))
        (goto-char (point-max))
        (insert (format " | positions=%s sz=%s m=%d"
                       positions
                       (list (sz-label sz1) (sz-range sz1))
                       (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'skip-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-sz))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_invisible_undo_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass invisible-snap ()
    ((step :initarg :step :accessor is-step :initform "")
     (buf-string :initarg :buf-string :accessor is-bs :initform "")))
  (let* ((buf (generate-new-buffer "iv5"))
         (s1 (invisible-snap :step "before"))
         (s2 (invisible-snap :step "after")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'group 1)
      (put-text-property 6 9 'group 2)
      (put-text-property 10 13 'group 3)
      (setq-local my-snaps (list s1 s2))
      (let* ((ov (make-overlay 6 9))
             (_ (overlay-put ov 'invisible t))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 6)))
        (undo-boundary)
        (setf (is-bs s1) (buffer-string))
        (let ((inhibit-read-only t))
          (goto-char 6)
          (insert "HIDDEN")
          (setf (is-bs s2) (buffer-string)))
        (let ((snap1 (is-bs s1))
              (snap2 (is-bs s2)))
          (goto-char (point-max))
          (insert (format " | s1=%s s2=%s m=%d ov=[%d,%d]"
                         snap1 snap2
                         (marker-position m) (overlay-start ov) (overlay-end ov)))
          (set-marker m 3)
          (put-text-property (1- (point-max)) (point-max) 'isnap-log t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-snaps))))
    (kill-buffer buf)))"#,
        expect,
    );
}

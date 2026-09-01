//! Combo: cl-eieio composition property + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests composition text properties with EIEIO objects, font-lock-like property stacking.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_composition_property_scan() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass comp-region ()
    ((start :initarg :start :accessor cr-start :initform 0)
     (end :initarg :end :accessor cr-end :initform 0)
     (comp-p :initarg :comp-p :accessor cr-comp :initform nil)
     (other-props :initarg :other-props :accessor cr-other :initform nil)))
  (let* ((buf (generate-new-buffer "cp1"))
         (regions nil))
    (with-current-buffer buf
      (insert "AAAAAAAAAAAAAA")
      (put-text-property 1 6 'face 'bold)
      (put-text-property 7 10 'face 'italic)
      (put-text-property 11 14 'face 'underline)
      (setq-local my-regions regions)
      (let* ((ov (make-overlay 4 10))
             (_ (overlay-put ov 'priority 1))
             (_ (overlay-put ov 'face 'shadow))
             (m (make-marker))
             (_ (set-marker m 5))
             (results nil))
        (undo-boundary)
        (compose-region 1 6 ?X)
        (compose-region 7 10 ?Y)
        (push (comp-region :start 1 :end 6
                          :comp-p (get-text-property 1 'composition)
                          :other-props (get-text-property 1 'face)) regions)
        (push (comp-region :start 7 :end 10
                          :comp-p (get-text-property 7 'composition)
                          :other-props (get-char-property 7 'face)) regions)
        (push (comp-region :start 11 :end 14
                          :comp-p (get-text-property 11 'composition)
                          :other-props (get-text-property 11 'face)) regions)
        (setq regions (reverse regions))
        (setq results (mapcar (lambda (r) (list (cr-start r) (cr-end r) (cr-comp r) (cr-other r))) regions))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'cr-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length regions)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_decompose_region_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass decomp-snap ()
    ((step :initarg :step :accessor ds-step :initform "")
     (comp-at-5 :initarg :comp5 :accessor ds-comp5 :initform nil)
     (comp-at-8 :initarg :comp8 :accessor ds-comp8 :initform nil)
     (face-at-5 :initarg :face5 :accessor ds-face5 :initform nil)))
  (let* ((buf (generate-new-buffer "cp2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAAAA-BBBBBB-CCCCCC")
      (put-text-property 1 7 'face 'bold)
      (put-text-property 8 14 'face 'italic)
      (put-text-property 15 21 'face 'underline)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 4 14))
             (_ (overlay-put ov 'priority 1))
             (_ (overlay-put ov 'face 'shadow))
             (m (make-marker))
             (_ (set-marker m 5))
             (results nil))
        (undo-boundary)
        (compose-region 1 7 ?X)
        (compose-region 8 14 ?Y)
        (push (decomp-snap :step "after-compose"
                          :comp5 (get-text-property 5 'composition)
                          :comp8 (get-text-property 8 'composition)
                          :face5 (get-char-property 5 'face)) snaps)
        (decompose-region 1 7)
        (push (decomp-snap :step "after-decomp-1"
                          :comp5 (get-text-property 5 'composition)
                          :comp8 (get-text-property 8 'composition)
                          :face5 (get-char-property 5 'face)) snaps)
        (decompose-region 4 14)
        (push (decomp-snap :step "after-decomp-2"
                          :comp5 (get-text-property 5 'composition)
                          :comp8 (get-text-property 8 'composition)
                          :face5 (get-char-property 5 'face)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ds-step s) (ds-comp5 s) (ds-comp8 s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ds-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_composition_narrow_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass comp-narrow-snap ()
    ((narrow-bounds :initarg :narrow :accessor cn-narrow :initform nil)
     (comp-present :initarg :comp :accessor cn-comp :initform nil)
     (buf-string :initarg :buf-string :accessor cn-bs :initform "")))
  (let* ((buf (generate-new-buffer "cp3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'z1)
      (put-text-property 6 10 'zone 'z2)
      (put-text-property 11 15 'zone 'z3)
      (put-text-property 16 20 'zone 'z4)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (compose-region 1 10 ?A)
        (push (comp-narrow-snap :narrow (list (point-min) (point-max))
                               :comp (get-text-property 3 'composition)
                               :buf-string (buffer-string)) snaps)
        (save-restriction
          (narrow-to-region 6 15)
          (push (comp-narrow-snap :narrow (list (point-min) (point-max))
                                 :comp (get-text-property 8 'composition)
                                 :buf-string (buffer-string)) snaps)
          (goto-char 8)
          (insert "XX")
          (push (comp-narrow-snap :narrow (list (point-min) (point-max))
                                 :comp (get-text-property 8 'composition)
                                 :buf-string (buffer-string)) snaps))
        (push (comp-narrow-snap :narrow (list (point-min) (point-max))
                               :comp (get-text-property 3 'composition)
                               :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (cn-narrow s) (cn-comp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'cn-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_composition_multi_prop_stacking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass stacked-prop-snap ()
    ((pos :initarg :pos :accessor sps-pos :initform 0)
     (composition :initarg :comp :accessor sps-comp :initform nil)
     (face :initarg :face :accessor sps-face :initform nil)
     (zone :initarg :zone :accessor sps-zone :initform nil)))
  (let* ((buf (generate-new-buffer "cp4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAAAAAAAAAAAAAAAAAA")
      (put-text-property 1 6 'face 'bold)
      (put-text-property 7 12 'face 'italic)
      (put-text-property 13 20 'face 'underline)
      (put-text-property 1 20 'zone 'active)
      (setq-local my-snaps snaps)
      (let* ((ov1 (make-overlay 3 9))
             (ov2 (make-overlay 11 17))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov1 'face 'shadow))
             (_ (overlay-put ov2 'priority 2))
             (_ (overlay-put ov2 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 5))
             (results nil))
        (undo-boundary)
        (compose-region 1 6 ?X)
        (compose-region 7 12 ?Y)
        (compose-region 13 20 ?Z)
        (dolist (pos '(1 4 6 8 10 14 18))
          (push (stacked-prop-snap :pos pos
                                   :comp (get-text-property pos 'composition)
                                   :face (get-char-property pos 'face)
                                   :zone (get-text-property pos 'zone))
                snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (sps-pos s) (sps-comp s) (sps-face s) (sps-zone s))) snaps))
        (goto-char 5)
        (insert "QQ")
        (push (list 'after-insert
                   (get-text-property 5 'composition)
                   (get-char-property 5 'face)
                   (get-text-property 5 'zone)
                   (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'sps-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov1) (overlay-end ov1)
                (overlay-start ov2) (overlay-end ov2)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_composition_undo_decompose_recompose() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass recomp-snap ()
    ((step :initarg :step :accessor rc-step :initform "")
     (comp-beg :initarg :comp-beg :accessor rc-beg :initform 0)
     (comp-end :initarg :comp-end :accessor rc-end :initform 0)
     (has-comp :initarg :has-comp :accessor rc-has :initform nil)))
  (let* ((buf (generate-new-buffer "cp5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAAAA-BBBBBB-CCCCCC")
      (put-text-property 1 7 'face 'bold)
      (put-text-property 8 14 'face 'italic)
      (put-text-property 15 21 'face 'underline)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 4 17))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (compose-region 1 7 ?A)
        (push (recomp-snap :step "compose-A" :comp-beg 1 :comp-end 7
                          :has-comp (get-text-property 1 'composition)) snaps)
        (compose-region 8 14 ?B)
        (push (recomp-snap :step "compose-B" :comp-beg 8 :comp-end 14
                          :has-comp (get-text-property 8 'composition)) snaps)
        (undo-boundary)
        (primitive-undo 1 buffer-undo-list)
        (push (recomp-snap :step "undo-compose-B" :comp-beg 8 :comp-end 14
                          :has-comp (get-text-property 8 'composition)) snaps)
        (decompose-region 1 7)
        (push (recomp-snap :step "decomp-A" :comp-beg 1 :comp-end 7
                          :has-comp (get-text-property 1 'composition)) snaps)
        (compose-region 1 14 ?C)
        (push (recomp-snap :step "recompose-C" :comp-beg 1 :comp-end 14
                          :has-comp (get-text-property 1 'composition)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (rc-step s) (rc-has s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'rc-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

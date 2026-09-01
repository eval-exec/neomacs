//! Combo: cl-eieio comment-region + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests comment/uncomment operations with EIEIO objects tracking state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_comment_region_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass comment-snap ()
    ((step :initarg :step :accessor cs-step :initform "")
     (line-count :initarg :lines :accessor cs-lines :initform 0)
     (first-char :initarg :first-char :accessor cs-char :initform 0)
     (buf-string :initarg :buf-string :accessor cs-bs :initform "")))
  (let* ((buf (generate-new-buffer "cm1"))
         (snaps nil))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(foo 1)\n(bar 2)\n(baz 3)")
      (put-text-property 1 8 'zone 'z1)
      (put-text-property 9 16 'zone 'z2)
      (put-text-property 17 24 'zone 'z3)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 9 16))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 12))
             (results nil))
        (undo-boundary)
        (push (comment-snap :step "init"
                           :lines (count-lines (point-min) (point-max))
                           :first-char (char-after 1)
                           :buf-string (buffer-string)) snaps)
        (comment-region 9 24)
        (push (comment-snap :step "after-comment"
                           :lines (count-lines (point-min) (point-max))
                           :first-char (char-after 9)
                           :buf-string (buffer-string)) snaps)
        (uncomment-region 9 24)
        (push (comment-snap :step "after-uncomment"
                           :lines (count-lines (point-min) (point-max))
                           :first-char (char-after 9)
                           :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (cs-step s) (cs-lines s) (cs-char s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 5)
        (put-text-property (1- (point-max)) (point-max) 'cs-log t)
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
fn combo_eieio_comment_region_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass comment-prop-snap ()
    ((step :initarg :step :accessor cps-step :initform "")
     (prop-at-5 :initarg :prop5 :accessor cps-p5 :initform nil)
     (prop-at-12 :initarg :prop12 :accessor cps-p12 :initform nil)
     (buf-string :initarg :buf-string :accessor cps-bs :initform "")))
  (let* ((buf (generate-new-buffer "cm2"))
         (snaps nil))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(foo 1)\n(bar 2)\n(baz 3)\n(qux 4)")
      (put-text-property 1 8 'face 'bold)
      (put-text-property 9 16 'face 'italic)
      (put-text-property 17 24 'face 'underline)
      (put-text-property 25 32 'face 'shadow)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 9 24))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 12))
             (results nil))
        (undo-boundary)
        (push (comment-prop-snap :step "init"
                                :prop5 (get-text-property 5 'face)
                                :prop12 (get-text-property 12 'face)
                                :buf-string (buffer-string)) snaps)
        (comment-region 1 32)
        (push (comment-prop-snap :step "commented"
                                :prop5 (get-text-property 5 'face)
                                :prop12 (get-text-property 12 'face)
                                :buf-string (buffer-string)) snaps)
        (setq results (list (length snaps) (marker-position m)
                           (overlay-start ov) (overlay-end ov)))
        (goto-char (point-max))
        (insert (format " | results=%s" results))
        (put-text-property (1- (point-max)) (point-max) 'cps-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (get-text-property 5 'face)
                (get-text-property 12 'face)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_comment_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass comment-narrow-snap ()
    ((step :initarg :step :accessor cns-step :initform "")
     (narrow-bounds :initarg :narrow :accessor cns-narrow :initform nil)
     (buf-string :initarg :buf-string :accessor cns-bs :initform "")
     (m-pos :initarg :m-pos :accessor cns-mp :initform 0)))
  (let* ((buf (generate-new-buffer "cm3"))
         (snaps nil))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(foo 1)\n(bar 2)\n(baz 3)\n(qux 4)\n(quux 5)")
      (put-text-property 1 8 'zone 'a)
      (put-text-property 9 16 'zone 'b)
      (put-text-property 17 24 'zone 'c)
      (put-text-property 25 32 'zone 'd)
      (put-text-property 33 41 'zone 'e)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 9 24))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 12))
             (results nil))
        (undo-boundary)
        (push (comment-narrow-snap :step "init"
                                  :narrow (list (point-min) (point-max))
                                  :buf-string (buffer-string)
                                  :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 9 24)
          (comment-region (point-min) (point-max))
          (push (comment-narrow-snap :step "narrow-comment"
                                    :narrow (list (point-min) (point-max))
                                    :buf-string (buffer-string)
                                    :m-pos (marker-position m)) snaps))
        (push (comment-narrow-snap :step "after-widen"
                                  :narrow (list (point-min) (point-max))
                                  :buf-string (buffer-string)
                                  :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (cns-step s) (cns-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'cns-log t)
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
fn combo_eieio_comment_marker_relocation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass comment-marker-snap ()
    ((step :initarg :step :accessor cms-step :initform "")
     (m1-pos :initarg :m1 :accessor cms-m1 :initform 0)
     (m2-pos :initarg :m2 :accessor cms-m2 :initform 0)
     (buf-string :initarg :buf-string :accessor cms-bs :initform "")))
  (let* ((buf (generate-new-buffer "cm4"))
         (snaps nil))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(aaa)\n(bbb)\n(ccc)\n(ddd)")
      (put-text-property 1 6 'zone 'a)
      (put-text-property 7 12 'zone 'b)
      (put-text-property 13 18 'zone 'c)
      (put-text-property 19 24 'zone 'd)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 7 18))
             (_ (overlay-put ov 'priority 1))
             (m1 (make-marker))
             (m2 (make-marker))
             (_ (set-marker m1 7))
             (_ (set-marker m2 18))
             (results nil))
        (undo-boundary)
        (push (comment-marker-snap :step "init"
                                  :m1 (marker-position m1)
                                  :m2 (marker-position m2)
                                  :buf-string (buffer-string)) snaps)
        (comment-region 7 18)
        (push (comment-marker-snap :step "comment"
                                  :m1 (marker-position m1)
                                  :m2 (marker-position m2)
                                  :buf-string (buffer-string)) snaps)
        (goto-char 5)
        (insert "XX")
        (push (comment-marker-snap :step "insert"
                                  :m1 (marker-position m1)
                                  :m2 (marker-position m2)
                                  :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (cms-step s) (cms-m1 s) (cms-m2 s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m1=%d m2=%d"
                       results (marker-position m1) (marker-position m2)))
        (put-text-property (1- (point-max)) (point-max) 'cms-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m1) (marker-position m2)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_comment_toggle_with_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass toggle-snap ()
    ((step :initarg :step :accessor ts-step :initform "")
     (commented :initarg :commented :accessor ts-commented :initform nil)
     (buf-string :initarg :buf-string :accessor ts-bs :initform "")))
  (let* ((buf (generate-new-buffer "cm5"))
         (snaps nil))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(foo)\n(bar)\n(baz)")
      (put-text-property 1 6 'zone 'a)
      (put-text-property 7 12 'zone 'b)
      (put-text-property 13 18 'zone 'c)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 7 12))
             (_ (overlay-put ov 'priority 1))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 9))
             (results nil))
        (undo-boundary)
        (push (toggle-snap :step "init" :commented nil
                          :buf-string (buffer-string)) snaps)
        (comment-region 1 18)
        (push (toggle-snap :step "commented" :commented t
                          :buf-string (buffer-string)) snaps)
        (comment-region 1 18)
        (push (toggle-snap :step "uncommented" :commented nil
                          :buf-string (buffer-string)) snaps)
        (comment-region 7 12)
        (push (toggle-snap :step "partial-comment" :commented t
                          :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ts-step s) (ts-commented s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d] ov-face=%s"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)
                       (overlay-get ov 'face)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ts-log t)
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

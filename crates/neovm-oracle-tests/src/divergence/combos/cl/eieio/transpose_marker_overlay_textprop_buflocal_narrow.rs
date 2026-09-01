//! Combo: cl-eieio transpose operations + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests transpose-chars/words/lines/sexps with EIEIO objects tracking state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_transpose_chars_with_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass transpose-char-snap ()
    ((step :initarg :step :accessor tc-step :initform "")
     (m1-pos :initarg :m1 :accessor tc-m1 :initform 0)
     (m2-pos :initarg :m2 :accessor tc-m2 :initform 0)
     (buf-string :initarg :buf-string :accessor tc-bs :initform "")))
  (let* ((buf (generate-new-buffer "tr1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "ABCDEFGH")
      (put-text-property 1 4 'zone 'a)
      (put-text-property 5 8 'zone 'b)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 3 6))
             (_ (overlay-put ov 'priority 1))
             (m1 (make-marker))
             (m2 (make-marker))
             (_ (set-marker m1 3))
             (_ (set-marker m2 6))
             (results nil))
        (undo-boundary)
        (push (transpose-char-snap :step "init"
                                  :m1 (marker-position m1)
                                  :m2 (marker-position m2)
                                  :buf-string (buffer-string)) snaps)
        (goto-char 3)
        (transpose-chars 1)
        (push (transpose-char-snap :step "transpose-3"
                                  :m1 (marker-position m1)
                                  :m2 (marker-position m2)
                                  :buf-string (buffer-string)) snaps)
        (goto-char 5)
        (transpose-chars 1)
        (push (transpose-char-snap :step "transpose-5"
                                  :m1 (marker-position m1)
                                  :m2 (marker-position m2)
                                  :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (tc-step s) (tc-m1 s) (tc-m2 s) (tc-bs s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m1=%d m2=%d ov=[%d,%d]"
                       (mapcar (lambda (r) (list (nth 0 r) (nth 1 r) (nth 2 r))) results)
                       (marker-position m1) (marker-position m2)
                       (overlay-start ov) (overlay-end ov)))
        (put-text-property (1- (point-max)) (point-max) 'tc-log t)
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
fn combo_eieio_transpose_words_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass transpose-word-snap ()
    ((step :initarg :step :accessor tw-step :initform "")
     (buf-string :initarg :buf-string :accessor tw-bs :initform "")
     (prop-at-1 :initarg :prop1 :accessor tw-p1 :initform nil)
     (prop-at-6 :initarg :prop6 :accessor tw-p6 :initform nil)))
  (let* ((buf (generate-new-buffer "tr2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA BBBB CCCC DDDD")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'shadow)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (push (transpose-word-snap :step "init"
                                  :buf-string (buffer-string)
                                  :prop1 (get-text-property 1 'face)
                                  :prop6 (get-text-property 6 'face)) snaps)
        (goto-char 6)
        (transpose-words 1)
        (push (transpose-word-snap :step "transpose-1"
                                  :buf-string (buffer-string)
                                  :prop1 (get-text-property 1 'face)
                                  :prop6 (get-text-property 6 'face)) snaps)
        (goto-char 1)
        (transpose-words 2)
        (push (transpose-word-snap :step "transpose-2"
                                  :buf-string (buffer-string)
                                  :prop1 (get-text-property 1 'face)
                                  :prop6 (get-text-property 6 'face)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (tw-step s) (tw-p1 s) (tw-p6 s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'tw-log t)
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
fn combo_eieio_transpose_lines_with_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 28 33)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass transpose-line-snap ()
    ((step :initarg :step :accessor tl-step :initform "")
     (m-pos :initarg :m-pos :accessor tl-mp :initform 0)
     (ov-bounds :initarg :ov-bounds :accessor tl-ov :initform nil)
     (buf-string :initarg :buf-string :accessor tl-bs :initform "")))
  (let* ((buf (generate-new-buffer "tr3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "line1\nline2\nline3\nline4\nline5")
      (put-text-property 1 6 'zone 'a)
      (put-text-property 7 13 'zone 'b)
      (put-text-property 14 20 'zone 'c)
      (put-text-property 21 27 'zone 'd)
      (put-text-property 28 33 'zone 'e)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 7 20))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 10))
             (results nil))
        (undo-boundary)
        (push (transpose-line-snap :step "init"
                                  :m-pos (marker-position m)
                                  :ov-bounds (list (overlay-start ov) (overlay-end ov))
                                  :buf-string (buffer-string)) snaps)
        (goto-char 7)
        (transpose-lines 1)
        (push (transpose-line-snap :step "transpose-1"
                                  :m-pos (marker-position m)
                                  :ov-bounds (list (overlay-start ov) (overlay-end ov))
                                  :buf-string (buffer-string)) snaps)
        (goto-char 14)
        (transpose-lines 1)
        (push (transpose-line-snap :step "transpose-2"
                                  :m-pos (marker-position m)
                                  :ov-bounds (list (overlay-start ov) (overlay-end ov))
                                  :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (tl-step s) (tl-mp s) (tl-ov s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'tl-log t)
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
fn combo_eieio_transpose_sexps_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass transpose-sexp-snap ()
    ((step :initarg :step :accessor tss-step :initform "")
     (narrow-bounds :initarg :narrow :accessor tss-narrow :initform nil)
     (m-pos :initarg :m-pos :accessor tss-mp :initform 0)
     (buf-string :initarg :buf-string :accessor tss-bs :initform "")))
  (let* ((buf (generate-new-buffer "tr4"))
         (snaps nil))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(a b c d e f)")
      (put-text-property 1 3 'zone 'a)
      (put-text-property 4 6 'zone 'b)
      (put-text-property 7 9 'zone 'c)
      (put-text-property 10 12 'zone 'd)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 4 12))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 5))
             (results nil))
        (undo-boundary)
        (push (transpose-sexp-snap :step "init"
                                  :narrow (list (point-min) (point-max))
                                  :m-pos (marker-position m)
                                  :buf-string (buffer-string)) snaps)
        (goto-char 4)
        (transpose-sexps 1)
        (push (transpose-sexp-snap :step "transpose-1"
                                  :narrow (list (point-min) (point-max))
                                  :m-pos (marker-position m)
                                  :buf-string (buffer-string)) snaps)
        (save-restriction
          (narrow-to-region 4 12)
          (goto-char 4)
          (transpose-sexps 1)
          (push (transpose-sexp-snap :step "narrow-transpose"
                                    :narrow (list (point-min) (point-max))
                                    :m-pos (marker-position m)
                                    :buf-string (buffer-string)) snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (tss-step s) (tss-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'tss-log t)
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
fn combo_eieio_transpose_undo_marker_integrity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass trans-undo-snap ()
    ((step :initarg :step :accessor tus-step :initform "")
     (m1-pos :initarg :m1 :accessor tus-m1 :initform 0)
     (m2-pos :initarg :m2 :accessor tus-m2 :initform 0)
     (buf-string :initarg :buf-string :accessor tus-bs :initform "")))
  (let* ((buf (generate-new-buffer "tr5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m1 (make-marker))
             (m2 (make-marker))
             (_ (set-marker m1 5))
             (_ (set-marker m2 15))
             (_ (set-marker-insertion-type m1 t))
             (results nil))
        (undo-boundary)
        (push (trans-undo-snap :step "init"
                              :m1 (marker-position m1)
                              :m2 (marker-position m2)
                              :buf-string (buffer-string)) snaps)
        (goto-char 3)
        (transpose-chars 1)
        (push (trans-undo-snap :step "transpose-char"
                              :m1 (marker-position m1)
                              :m2 (marker-position m2)
                              :buf-string (buffer-string)) snaps)
        (goto-char 6)
        (transpose-words 1)
        (push (trans-undo-snap :step "transpose-word"
                              :m1 (marker-position m1)
                              :m2 (marker-position m2)
                              :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (tus-step s) (tus-m1 s) (tus-m2 s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m1=%d m2=%d"
                       results (marker-position m1) (marker-position m2)))
        (put-text-property (1- (point-max)) (point-max) 'tus-log t)
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

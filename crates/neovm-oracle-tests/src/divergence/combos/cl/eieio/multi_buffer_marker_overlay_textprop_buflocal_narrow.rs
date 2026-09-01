//! Combo: cl-eieio multi-buffer cross-buffer + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests cross-buffer operations with EIEIO objects, shared overlays, and marker relocation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_two_buffers_shared_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (cl-no-applicable-method cbs-label t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass cross-buf-snap ()
    ((label :initarg :label :accessor cbs-label :initform "")
     (buf-name :initarg :buf-name :accessor cbs-buf :initform "")
     (m-buffer :initarg :m-buffer :accessor cbs-mbuf :initform nil)
     (m-pos :initarg :m-pos :accessor cbs-mpos :initform 0)))
  (let* ((buf1 (generate-new-buffer "xb1a"))
         (buf2 (generate-new-buffer "xb1b"))
         (m (make-marker))
         (snaps nil))
    (with-current-buffer buf1
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 9 'zone 'b)
      (put-text-property 10 13 'zone 'c)
      (set-marker m 6 buf1)
      (push (cross-buf-snap :label "init-buf1"
                           :buf-name (buffer-name)
                           :m-buffer (marker-buffer m)
                           :m-pos (marker-position m)) snaps))
    (with-current-buffer buf2
      (insert "XXXX-YYYY-ZZZZ")
      (put-text-property 1 5 'zone 'x)
      (put-text-property 6 9 'zone 'y)
      (put-text-property 10 13 'zone 'z)
      (let* ((ov (make-overlay 6 9))
             (_ (overlay-put ov 'priority 1)))
        (set-marker m 8 buf2)
        (push (cross-buf-snap :label "moved-buf2"
                             :buf-name (buffer-name)
                             :m-buffer (marker-buffer m)
                             :m-pos (marker-position m)) snaps)
        (goto-char 3)
        (insert "PP")
        (push (cross-buf-snap :label "edit-buf2"
                             :buf-name (buffer-name)
                             :m-buffer (marker-buffer m)
                             :m-pos (marker-position m)) snaps)))
    (with-current-buffer buf1
      (push (cross-buf-snap :label "back-buf1"
                           :buf-name (buffer-name)
                           :m-buffer (marker-buffer m)
                           :m-pos (marker-position m)) snaps))
    (setq snaps (reverse snaps))
    (let ((results (mapcar (lambda (s) (list (cbs-label t) (cbs-mpos t))) snaps)))
      results)
    (let ((result-str (format "%S" (mapcar (lambda (s) (list (cbs-label s) (cbs-mpos s))) snaps))))
      (kill-buffer buf1)
      (kill-buffer buf2)
      result-str)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_two_buffers_overlay_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass dual-buf-snap ()
    ((label :initarg :label :accessor dbs-label :initform "")
     (buf1-string :initarg :buf1 :accessor dbs-b1 :initform "")
     (buf2-string :initarg :buf2 :accessor dbs-b2 :initform "")))
  (let* ((buf1 (generate-new-buffer "xb2a"))
         (buf2 (generate-new-buffer "xb2b"))
         (snaps nil)
         (ov1 nil)
         (ov2 nil))
    (with-current-buffer buf1
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 9 'zone 'b)
      (setq ov1 (make-overlay 3 7))
      (overlay-put ov1 'priority 1))
    (with-current-buffer buf2
      (insert "XXXX-YYYY-ZZZZ")
      (put-text-property 1 5 'zone 'x)
      (put-text-property 6 9 'zone 'y)
      (setq ov2 (make-overlay 4 8))
      (overlay-put ov2 'priority 2))
    (push (dual-buf-snap :label "init"
                        :buf1 (with-current-buffer buf1 (buffer-string))
                        :buf2 (with-current-buffer buf2 (buffer-string))) snaps)
    (with-current-buffer buf1
      (goto-char 4)
      (insert "MM"))
    (with-current-buffer buf2
      (goto-char 3)
      (insert "NN"))
    (push (dual-buf-snap :label "after-edit"
                        :buf1 (with-current-buffer buf1 (buffer-string))
                        :buf2 (with-current-buffer buf2 (buffer-string))) snaps)
    (with-current-buffer buf1
      (delete-region 3 5))
    (push (dual-buf-snap :label "after-delete"
                        :buf1 (with-current-buffer buf1 (buffer-string))
                        :buf2 (with-current-buffer buf2 (buffer-string))) snaps)
    (setq snaps (reverse snaps))
    (let ((results (mapcar (lambda (s) (list (dbs-label s) (length (dbs-b1 s)) (length (dbs-b2 s)))) snaps)))
      (let ((ov1-start (with-current-buffer buf1 (overlay-start ov1)))
            (ov2-start (with-current-buffer buf2 (overlay-start ov2)))
            (result-str (format "%S" (list results ov1-start ov2-start))))
        (kill-buffer buf1)
        (kill-buffer buf2)
        result-str)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_cross_buffer_copy_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass copy-props-snap ()
    ((label :initarg :label :accessor cps-label :initform "")
     (src-props :initarg :src-props :accessor cps-src :initform nil)
     (dst-props :initarg :dst-props :accessor cps-dst :initform nil)))
  (let* ((buf1 (generate-new-buffer "xb3a"))
         (buf2 (generate-new-buffer "xb3b"))
         (snaps nil))
    (with-current-buffer buf1
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 9 'face 'italic)
      (put-text-property 10 13 'face 'underline)
      (let* ((ov (make-overlay 3 7))
             (_ (overlay-put ov 'priority 1))
             (_ (overlay-put ov 'face 'shadow))
             (text-with-props (buffer-substring 3 7)))
        (push (copy-props-snap :label "src"
                              :src-props (get-text-property 4 'face)
                              :dst-props nil) snaps)
        (with-current-buffer buf2
          (insert "XXXXXXXXX")
          (goto-char 3)
          (insert text-with-props)
          (push (copy-props-snap :label "dst-after-insert"
                                :src-props (get-text-property 4 'face)
                                :dst-props (get-text-property 4 'face)) snaps)
          (delete-region 3 7)
          (push (copy-props-snap :label "dst-after-delete"
                                :src-props nil
                                :dst-props (get-text-property 3 'face)) snaps))))
    (setq snaps (reverse snaps))
    (let ((results (mapcar (lambda (s) (list (cps-label s) (cps-src s) (cps-dst s))) snaps)))
      (let ((result-str (format "%S" results)))
        (kill-buffer buf1)
        (kill-buffer buf2)
        result-str)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_cross_buffer_narrow_sequential() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass seq-narrow-snap ()
    ((label :initarg :label :accessor sns-label :initform "")
     (buf-name :initarg :buf-name :accessor sns-buf :initform "")
     (narrow-bounds :initarg :narrow :accessor sns-narrow :initform nil)
     (visible :initarg :visible :accessor sns-visible :initform "")))
  (let* ((buf1 (generate-new-buffer "xb4a"))
         (buf2 (generate-new-buffer "xb4b"))
         (snaps nil))
    (with-current-buffer buf1
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd))
    (with-current-buffer buf2
      (insert "XXXX-YYYY-ZZZZ-WWWW")
      (put-text-property 1 5 'zone 'x)
      (put-text-property 6 10 'zone 'y)
      (put-text-property 11 15 'zone 'z)
      (put-text-property 16 20 'zone 'w))
    (with-current-buffer buf1
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1)))
        (save-restriction
          (narrow-to-region 6 15)
          (push (seq-narrow-snap :label "buf1-narrow"
                                :buf-name (buffer-name)
                                :narrow (list (point-min) (point-max))
                                :visible (buffer-string)) snaps))
        (push (seq-narrow-snap :label "buf1-wide"
                              :buf-name (buffer-name)
                              :narrow (list (point-min) (point-max))
                              :visible (buffer-string)) snaps)))
    (with-current-buffer buf2
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 2)))
        (save-restriction
          (narrow-to-region 11 20)
          (push (seq-narrow-snap :label "buf2-narrow"
                                :buf-name (buffer-name)
                                :narrow (list (point-min) (point-max))
                                :visible (buffer-string)) snaps))
        (push (seq-narrow-snap :label "buf2-wide"
                              :buf-name (buffer-name)
                              :narrow (list (point-min) (point-max))
                              :visible (buffer-string)) snaps)))
    (setq snaps (reverse snaps))
    (let ((results (mapcar (lambda (s) (list (sns-label s) (sns-narrow s) (length (sns-visible s)))) snaps)))
      (let ((result-str (format "%S" results)))
        (kill-buffer buf1)
        (kill-buffer buf2)
        result-str)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_cross_buffer_undo_sequential() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass cross-undo-snap ()
    ((label :initarg :label :accessor cus-label :initform "")
     (buf1-string :initarg :buf1 :accessor cus-b1 :initform "")
     (buf2-string :initarg :buf2 :accessor cus-b2 :initform "")))
  (let* ((buf1 (generate-new-buffer "xb5a"))
         (buf2 (generate-new-buffer "xb5b"))
         (snaps nil))
    (with-current-buffer buf1
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 9 'zone 'b)
      (let* ((ov (make-overlay 3 7))
             (_ (overlay-put ov 'priority 1)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (goto-char 3)
        (insert "XX")
        (push (cross-undo-snap :label "buf1-edit"
                              :buf1 (buffer-string)
                              :buf2 "") snaps)))
    (with-current-buffer buf2
      (insert "XXXX-YYYY-ZZZZ")
      (put-text-property 1 5 'zone 'x)
      (put-text-property 6 9 'zone 'y)
      (let* ((ov (make-overlay 4 8))
             (_ (overlay-put ov 'priority 2)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (goto-char 3)
        (insert "QQ")
        (push (cross-undo-snap :label "buf2-edit"
                              :buf1 ""
                              :buf2 (buffer-string)) snaps)))
    (with-current-buffer buf1
      (primitive-undo 1 buffer-undo-list)
      (push (cross-undo-snap :label "buf1-undo"
                            :buf1 (buffer-string)
                            :buf2 "") snaps))
    (with-current-buffer buf2
      (primitive-undo 1 buffer-undo-list)
      (push (cross-undo-snap :label "buf2-undo"
                            :buf1 ""
                            :buf2 (buffer-string)) snaps))
    (setq snaps (reverse snaps))
    (let ((results (mapcar (lambda (s) (list (cus-label s) (length (cus-b1 s)) (length (cus-b2 s)))) snaps)))
      (let ((result-str (format "%S" results)))
        (kill-buffer buf1)
        (kill-buffer buf2)
        result-str)))"#,
        expect,
    );
}

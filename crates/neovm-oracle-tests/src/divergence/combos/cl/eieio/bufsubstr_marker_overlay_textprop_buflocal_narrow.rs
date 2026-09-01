//! Combo: cl-eieio buffer-substring composition + overlays + markers
//! + textprop + buflocal + narrow + undo.
//! Tests buffer-substring, filter-buffer-substring, and insert-buffer-substring
//! with complex overlay and text property interactions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_bufsubstr_with_overlay_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass bss-snap ()
    ((step :initarg :step :accessor bss-step :initform "")
     (substr :initarg :substr :accessor bss-sub :initform "")
     (no-props :initarg :no-props :accessor bss-np :initform "")
     (m-pos :initarg :m-pos :accessor bss-mp :initform 0)))
  (let* ((buf (generate-new-buffer "bs1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (setq-local my-bs-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 10))
             (m (set-marker (make-marker) 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (bss-snap :step "init"
                       :substr (buffer-substring 5 16)
                       :no-props (buffer-substring-no-properties 5 16)
                       :m-pos (marker-position m)) snaps)
        (goto-char 8)
        (insert "XX")
        (setq my-bs-log (cons "ins@8" my-bs-log))
        (push (bss-snap :step "edit"
                       :substr (buffer-substring 5 18)
                       :no-props (buffer-substring-no-properties 5 18)
                       :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 5 20)
          (push (bss-snap :step "narrow"
                         :substr (buffer-substring (point-min) (point-max))
                         :no-props (buffer-substring-no-properties
                                    (point-min) (point-max))
                         :m-pos (marker-position m)) snaps)
          (goto-char 8)
          (insert "YY")
          (setq my-bs-log (cons "ins@narrow-8" my-bs-log))
          (push (bss-snap :step "narrow-edit"
                         :substr (buffer-substring (point-min) (point-max))
                         :no-props (buffer-substring-no-properties
                                    (point-min) (point-max))
                         :m-pos (marker-position m)) snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (bss-step s) (bss-mp s)
                                                (length (bss-sub s)))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S log=%S"
                       results (reverse my-bs-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'bss-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-bs-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_bufsubstr_insert_into_other_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass cross-buf-snap ()
    ((step :initarg :step :accessor cbs-step :initform "")
     (src-substr :initarg :src :accessor cbs-src :initform "")
     (dst-string :initarg :dst :accessor cbs-dst :initform "")
     (m-pos :initarg :m-pos :accessor cbs-mp :initform 0)))
  (let* ((src (generate-new-buffer "bs2s"))
         (dst (generate-new-buffer "bs2d"))
         (snaps nil))
    (with-current-buffer src
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (with-current-buffer dst
          (insert "XXXX"))
        (push (cross-buf-snap :step "init"
                             :src (buffer-substring 5 16)
                             :dst (with-current-buffer dst (buffer-string))
                             :m-pos (marker-position m)) snaps)
        (let ((sub (buffer-substring 6 15)))
          (with-current-buffer dst
            (goto-char (point-max))
            (insert sub)
            (setq-local dst-marker (point-max))))
        (push (cross-buf-snap :step "cross-insert"
                             :src (buffer-substring 5 16)
                             :dst (with-current-buffer dst (buffer-string))
                             :m-pos (marker-position m)) snaps)
        (goto-char 8)
        (insert "ZZ")
        (let ((sub (buffer-substring 6 17)))
          (with-current-buffer dst
            (goto-char (point-max))
            (insert sub)))
        (push (cross-buf-snap :step "after-edit"
                             :src (buffer-substring 5 18)
                             :dst (with-current-buffer dst (buffer-string))
                             :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (cbs-step s) (cbs-mp s)
                                                (length (cbs-src s))
                                                (length (cbs-dst s)))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S" results))
        (put-text-property (1- (point-max)) (point-max) 'cbs-log t)
        (list (buffer-string)
              (with-current-buffer dst (buffer-string))
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov))))
    (kill-buffer src)
    (kill-buffer dst)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_bufsubstr_filter_buffer_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<buffer bs3> 1 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass filter-snap ()
    ((step :initarg :step :accessor fs-step :initform "")
     (filtered :initarg :filtered :accessor fs-filt :initform "")
     (raw :initarg :raw :accessor fs-raw :initform "")
     (m-pos :initarg :m-pos :accessor fs-mp :initform 0)))
  (let* ((buf (generate-new-buffer "bs3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (setq-local my-filt-log nil)
      (let* ((ov1 (make-overlay 6 15))
             (ov2 (make-overlay 16 25))
             (_ (overlay-put ov1 'invisible 'hide-zone))
             (_ (overlay-put ov1 'priority 5))
             (_ (overlay-put ov1 'face 'shadow))
             (_ (overlay-put ov2 'face 'error))
             (_ (overlay-put ov2 'priority 10))
             (m (set-marker (make-marker) 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (filter-snap :step "init"
                          :filtered (filter-buffer-substring 1 25 t)
                          :raw (buffer-substring 1 25)
                          :m-pos (marker-position m)) snaps)
        (add-to-invisibility-spec 'hide-zone)
        (setq my-filt-log (cons "hide-zone" my-filt-log))
        (push (filter-snap :step "hidden"
                          :filtered (filter-buffer-substring 1 25 t)
                          :raw (buffer-substring 1 25)
                          :m-pos (marker-position m)) snaps)
        (remove-from-invisibility-spec 'hide-zone)
        (setq my-filt-log (cons "show-zone" my-filt-log))
        (push (filter-snap :step "visible"
                          :filtered (filter-buffer-substring 1 25 t)
                          :raw (buffer-substring 1 25)
                          :m-pos (marker-position m)) snaps)
        (goto-char 8)
        (insert "MMM")
        (setq my-filt-log (cons "edit" my-filt-log))
        (push (filter-snap :step "edit"
                          :filtered (filter-buffer-substring 1 28 t)
                          :raw (buffer-substring 1 28)
                          :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 5 22)
          (push (filter-snap :step "narrow"
                            :filtered (filter-buffer-substring
                                       (point-min) (point-max) t)
                            :raw (buffer-substring (point-min) (point-max))
                            :m-pos (marker-position m)) snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (fs-step s) (fs-mp s)
                                                (length (fs-filt s))
                                                (length (fs-raw s)))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S filt-log=%S"
                       results (reverse my-filt-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'fs-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov1) (overlay-end ov1)
              (overlay-start ov2) (overlay-end ov2)
              buffer-invisibility-spec)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_bufsubstr_prop_transfer_cross_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass prop-transfer-snap ()
    ((step :initarg :step :accessor pts-step :initform "")
     (dst-faces :initarg :dst-faces :accessor pts-faces :initform nil)
     (src-faces :initarg :src-faces :accessor pts-sfaces :initform nil)
     (m-pos :initarg :m-pos :accessor pts-mp :initform 0)))
  (let* ((src (generate-new-buffer "bs4s"))
         (dst (generate-new-buffer "bs4d"))
         (snaps nil))
    (with-current-buffer src
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (setq-local my-pt-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 5))
             (_ (overlay-put ov 'face 'shadow))
             (m (set-marker (make-marker) 10))
             (results nil)
             (snap-faces
              (lambda ()
                (list (get-text-property 1 'face)
                      (get-text-property 5 'face)
                      (get-text-property 10 'face)
                      (get-text-property 15 'face)))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (with-current-buffer dst (insert "----"))
        (push (prop-transfer-snap :step "init"
                                 :dst-faces (with-current-buffer dst
                                             (list (get-text-property 1 'face)
                                                   (get-text-property 4 'face)))
                                 :src-faces (funcall snap-faces)
                                 :m-pos (marker-position m)) snaps)
        (let ((sub (buffer-substring 3 12)))
          (with-current-buffer dst
            (goto-char (point-max))
            (insert sub)))
        (setq my-pt-log (cons "transfer-3-12" my-pt-log))
        (push (prop-transfer-snap :step "transfer"
                                 :dst-faces (with-current-buffer dst
                                             (list (get-text-property 1 'face)
                                                   (get-text-property 5 'face)
                                                   (get-text-property 9 'face)
                                                   (get-text-property 12 'face)))
                                 :src-faces (funcall snap-faces)
                                 :m-pos (marker-position m)) snaps)
        (with-current-buffer dst
          (put-text-property 5 9 'face 'error))
        (setq my-pt-log (cons "dst-edit-face" my-pt-log))
        (push (prop-transfer-snap :step "dst-face"
                                 :dst-faces (with-current-buffer dst
                                             (list (get-text-property 5 'face)
                                                   (get-text-property 9 'face)))
                                 :src-faces (funcall snap-faces)
                                 :m-pos (marker-position m)) snaps)
        (goto-char 7)
        (insert "NNN")
        (setq my-pt-log (cons "src-edit" my-pt-log))
        (let ((sub (buffer-substring 3 15)))
          (with-current-buffer dst
            (goto-char (point-max))
            (insert sub)))
        (push (prop-transfer-snap :step "second-transfer"
                                 :dst-faces (with-current-buffer dst
                                             (list (get-text-property 1 'face)
                                                   (get-text-property 14 'face)))
                                 :src-faces (funcall snap-faces)
                                 :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (pts-step s) (pts-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S pt-log=%S"
                       results (reverse my-pt-log)))
        (put-text-property (1- (point-max)) (point-max) 'pts-log t)
        (list (buffer-string)
              (with-current-buffer dst (buffer-string))
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov))))
    (kill-buffer src)
    (kill-buffer dst)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_bufsubstr_undo_after_insert_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass undo-substr-snap ()
    ((step :initarg :step :accessor uss-step :initform "")
     (buf-string :initarg :bs :accessor uss-bs :initform "")
     (m-pos :initarg :m-pos :accessor uss-mp :initform 0)
     (ov-bounds :initarg :ov :accessor uss-ov :initform nil)))
  (let* ((buf (generate-new-buffer "bs5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (setq-local my-us-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (undo-substr-snap :step "init"
                               :bs (buffer-string)
                               :m-pos (marker-position m)
                               :ov (list (overlay-start ov) (overlay-end ov))) snaps)
        (let ((sub (buffer-substring 6 15)))
          (goto-char (point-max))
          (insert "-COPY-")
          (insert sub))
        (undo-boundary)
        (setq my-us-log (cons "append-copy" my-us-log))
        (push (undo-substr-snap :step "copy"
                               :bs (buffer-string)
                               :m-pos (marker-position m)
                               :ov (list (overlay-start ov) (overlay-end ov))) snaps)
        (goto-char 8)
        (insert "XXX")
        (undo-boundary)
        (setq my-us-log (cons "edit@8" my-us-log))
        (push (undo-substr-snap :step "edit"
                               :bs (buffer-string)
                               :m-pos (marker-position m)
                               :ov (list (overlay-start ov) (overlay-end ov))) snaps)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (push (undo-substr-snap :step "undo-edit"
                                 :bs (buffer-string)
                                 :m-pos (marker-position m)
                                 :ov (list (overlay-start ov) (overlay-end ov))) snaps
          (setq my-us-log (cons (format "after-undo:%S" bs) my-us-log))))
        (primitive-undo 1 buffer-undo-list)
        (push (undo-substr-snap :step "undo-copy"
                               :bs (buffer-string)
                               :m-pos (marker-position m)
                               :ov (list (overlay-start ov) (overlay-end ov))) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (uss-step s) (uss-mp s)
                                                (length (uss-bs s)))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S us-log=%S"
                       results (reverse my-us-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'uss-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov))))
    (kill-buffer buf)))"#,
        expect,
    );
}

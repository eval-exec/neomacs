//! Combo: cl-eieio file I/O insert-file-contents + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests file I/O operations with EIEIO objects, temp files, overlays and markers.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_insert_file_contents_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass file-io-snap ()
    ((step :initarg :step :accessor fis-step :initform "")
     (buf-string :initarg :buf-string :accessor fis-bs :initform "")
     (buf-size :initarg :buf-size :accessor fis-size :initform 0)))
  (let* ((tmpfile (make-temp-file "oracle-io1" nil ".txt"))
         (buf (generate-new-buffer "io1"))
         (snaps nil))
    (with-temp-buffer
      (insert "HELLO-WORLD-TEST")
      (write-region (point-min) (point-max) tmpfile nil 'silent))
    (with-current-buffer buf
      (insert "AAAA-BBBB-")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 5))
             (results nil))
        (undo-boundary)
        (push (file-io-snap :step "init"
                           :buf-string (buffer-string)
                           :buf-size (buffer-size)) snaps)
        (goto-char (point-max))
        (insert-file-contents tmpfile)
        (push (file-io-snap :step "after-insert-file"
                           :buf-string (buffer-string)
                           :buf-size (buffer-size)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (fis-step s) (fis-size s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'fis-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)
    (delete-file tmpfile)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_write_region_read_back() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass write-read-snap ()
    ((step :initarg :step :accessor wrs-step :initform "")
     (buf-string :initarg :buf-string :accessor wrs-bs :initform "")
     (m-pos :initarg :m-pos :accessor wrs-mp :initform 0)))
  (let* ((tmpfile (make-temp-file "oracle-io2" nil ".txt"))
         (buf (generate-new-buffer "io2"))
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
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (push (write-read-snap :step "init"
                              :buf-string (buffer-string)
                              :m-pos (marker-position m)) snaps)
        (write-region 6 15 tmpfile nil 'silent)
        (push (write-read-snap :step "after-write"
                              :buf-string (buffer-string)
                              :m-pos (marker-position m)) snaps)
        (delete-region 6 15)
        (push (write-read-snap :step "after-delete"
                              :buf-string (buffer-string)
                              :m-pos (marker-position m)) snaps)
        (goto-char 6)
        (insert-file-contents tmpfile)
        (push (write-read-snap :step "after-read-back"
                              :buf-string (buffer-string)
                              :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (wrs-step s) (length (wrs-bs s)) (wrs-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'wrs-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)
    (delete-file tmpfile)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_insert_file_with_overlays_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass file-overlay-snap ()
    ((step :initarg :step :accessor fos-step :initform "")
     (m-pos :initarg :m-pos :accessor fos-mp :initform 0)
     (ov-start :initarg :ov-start :accessor fos-ovs :initform 0)
     (ov-end :initarg :ov-end :accessor fos-ove :initform 0)
     (buf-string :initarg :buf-string :accessor fos-bs :initform "")))
  (let* ((tmpfile (make-temp-file "oracle-io3" nil ".txt"))
         (buf (generate-new-buffer "io3"))
         (snaps nil))
    (with-temp-buffer
      (insert "INSERTED-CONTENT")
      (write-region (point-min) (point-max) tmpfile nil 'silent))
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
             (_ (set-marker m1 8))
             (results nil))
        (undo-boundary)
        (push (file-overlay-snap :step "init"
                                :m-pos (marker-position m1)
                                :ov-start (overlay-start ov)
                                :ov-end (overlay-end ov)
                                :buf-string (buffer-string)) snaps)
        (goto-char 3)
        (insert-file-contents tmpfile)
        (push (file-overlay-snap :step "after-insert"
                                :m-pos (marker-position m1)
                                :ov-start (overlay-start ov)
                                :ov-end (overlay-end ov)
                                :buf-string (buffer-string)) snaps)
        (delete-region 3 10)
        (push (file-overlay-snap :step "after-delete"
                                :m-pos (marker-position m1)
                                :ov-start (overlay-start ov)
                                :ov-end (overlay-end ov)
                                :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (fos-step s) (fos-mp s) (fos-ovs s) (fos-ove s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m1=%d"
                       results (marker-position m1)))
        (put-text-property (1- (point-max)) (point-max) 'fos-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m1)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)
    (delete-file tmpfile)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_file_io_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass file-narrow-snap ()
    ((step :initarg :step :accessor fns-step :initform "")
     (narrow-bounds :initarg :narrow :accessor fns-narrow :initform nil)
     (buf-string :initarg :buf-string :accessor fns-bs :initform "")))
  (let* ((tmpfile (make-temp-file "oracle-io4" nil ".txt"))
         (buf (generate-new-buffer "io4"))
         (snaps nil))
    (with-temp-buffer
      (insert "FILE-DATA-HERE")
      (write-region (point-min) (point-max) tmpfile nil 'silent))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (push (file-narrow-snap :step "init"
                               :narrow (list (point-min) (point-max))
                               :buf-string (buffer-string)) snaps)
        (save-restriction
          (narrow-to-region 6 15)
          (goto-char (point-max))
          (insert-file-contents tmpfile)
          (push (file-narrow-snap :step "narrow-insert"
                                 :narrow (list (point-min) (point-max))
                                 :buf-string (buffer-string)) snaps))
        (push (file-narrow-snap :step "after-widen"
                               :narrow (list (point-min) (point-max))
                               :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (fns-step s) (length (fns-bs s)))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'fns-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)
    (delete-file tmpfile)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_file_io_undo_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass file-undo-snap ()
    ((step :initarg :step :accessor fus-step :initform "")
     (buf-string :initarg :buf-string :accessor fus-bs :initform "")
     (buf-size :initarg :buf-size :accessor fus-size :initform 0)))
  (let* ((tmpfile (make-temp-file "oracle-io5" nil ".txt"))
         (buf (generate-new-buffer "io5"))
         (snaps nil))
    (with-temp-buffer
      (insert "FILECONTENT")
      (write-region (point-min) (point-max) tmpfile nil 'silent))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 9 'zone 'b)
      (put-text-property 10 13 'zone 'c)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 5))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (file-undo-snap :step "init"
                             :buf-string (buffer-string)
                             :buf-size (buffer-size)) snaps)
        (goto-char (point-max))
        (insert-file-contents tmpfile)
        (undo-boundary)
        (push (file-undo-snap :step "after-insert-file"
                             :buf-string (buffer-string)
                             :buf-size (buffer-size)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (file-undo-snap :step "after-undo"
                             :buf-string (buffer-string)
                             :buf-size (buffer-size)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (fus-step s) (fus-size s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (put-text-property (1- (point-max)) (point-max) 'fus-log t)
        (list (buffer-string)
              (length snaps)
              (marker-position m)
              (overlay-start ov) (overlay-end ov))))
    (kill-buffer buf)
    (delete-file tmpfile)))"#,
        expect,
    );
}

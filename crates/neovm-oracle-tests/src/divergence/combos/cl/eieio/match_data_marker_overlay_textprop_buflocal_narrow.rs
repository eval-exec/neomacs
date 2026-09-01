//! Combo: cl-eieio match-data save/restore + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests match-data interactions with EIEIO objects, overlays, and editing operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_match_data_basic_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass match-snap ()
    ((pattern :initarg :pattern :accessor ms-pat :initform "")
     (match-data :initarg :match-data :accessor ms-md :initform nil)
     (group-0 :initarg :group-0 :accessor ms-g0 :initform "")))
  (let* ((buf (generate-new-buffer "md1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-AAAA-CCCC-AAAA")
      (put-text-property 1 5 'zone 'z1)
      (put-text-property 6 10 'zone 'z2)
      (put-text-property 11 15 'zone 'z3)
      (put-text-property 16 20 'zone 'z4)
      (put-text-property 21 25 'zone 'z5)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 1))
             (results nil))
        (undo-boundary)
        (goto-char 1)
        (while (search-forward "AAAA" nil t)
          (let ((md (match-data))
                (g0 (match-string 0)))
            (push (match-snap :pattern "AAAA" :match-data md :group-0 g0) snaps)))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ms-md s) (ms-g0 s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ms-log t)
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
fn combo_eieio_match_data_edit_after_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass edit-after-match ()
    ((match-pos :initarg :match-pos :accessor eam-pos :initform 0)
     (edit-type :initarg :edit-type :accessor eam-type :initform "")
     (saved-md :initarg :saved-md :accessor eam-saved :initform nil)
     (current-md :initarg :current-md :accessor eam-current :initform nil)))
  (let* ((buf (generate-new-buffer "md2"))
         (snaps nil)
         (saved-md nil))
    (with-current-buffer buf
      (insert "XXXX-YYYY-XXXX-ZZZZ-XXXX")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 1))
             (results nil))
        (undo-boundary)
        (goto-char 1)
        (search-forward "XXXX")
        (setq saved-md (match-data))
        (push (edit-after-match :match-pos (point) :edit-type "after-search"
                               :saved-md saved-md :current-md (match-data)) snaps)
        (goto-char 3)
        (insert "AA")
        (push (edit-after-match :match-pos 3 :edit-type "insert"
                               :saved-md saved-md :current-md (match-data)) snaps)
        (delete-region 3 5)
        (push (edit-after-match :match-pos 3 :edit-type "delete"
                               :saved-md saved-md :current-md (match-data)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (eam-type s) (eam-saved s) (eam-current s)))
                             snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 4)
        (put-text-property (1- (point-max)) (point-max) 'eam-log t)
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
fn combo_eieio_match_data_narrow_restricted() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass narrow-match ()
    ((narrow-bounds :initarg :narrow :accessor nm-bounds :initform nil)
     (match-data :initarg :match-data :accessor nm-md :initform nil)
     (found :initarg :found :accessor nm-found :initform nil)))
  (let* ((buf (generate-new-buffer "md3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "PAT1-XXXX-PAT2-YYYY-PAT3-ZZZZ-PAT4")
      (put-text-property 1 5 'section 's1)
      (put-text-property 6 10 'section 's2)
      (put-text-property 11 15 'section 's3)
      (put-text-property 16 20 'section 's4)
      (put-text-property 21 25 'section 's5)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 25))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 1))
             (results nil))
        (undo-boundary)
        (save-match-data
          (goto-char 1)
          (search-forward "PAT2")
          (push (narrow-match :narrow (list (point-min) (point-max))
                             :match-data (match-data)
                             :found (match-string 0)) snaps)
          (save-restriction
            (narrow-to-region 11 25)
            (goto-char (point-min))
            (search-forward "PAT3")
            (push (narrow-match :narrow (list (point-min) (point-max))
                               :match-data (match-data)
                               :found (match-string 0)) snaps))
          (search-forward "PAT4")
          (push (narrow-match :narrow (list (point-min) (point-max))
                             :match-data (match-data)
                             :found (match-string 0)) snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (nm-bounds s) (nm-md s) (nm-found s)))
                             snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'nm-log t)
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
fn combo_eieio_match_data_replace_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass replace-step ()
    ((step-num :initarg :step :accessor rs-step :initform 0)
     (match-pos :initarg :pos :accessor rs-pos :initform 0)
     (match-str :initarg :match :accessor rs-match :initform "")
     (replaced-with :initarg :replaced :accessor rs-replaced :initform "")
     (buf-after :initarg :buf-after :accessor rs-buf :initform "")))
  (let* ((buf (generate-new-buffer "md4"))
         (steps nil)
         (step-num 0))
    (with-current-buffer buf
      (insert "OLD-OLD-OLD-OLD-OLD")
      (put-text-property 1 4 'zone 'a)
      (put-text-property 5 8 'zone 'b)
      (put-text-property 9 12 'zone 'c)
      (put-text-property 13 16 'zone 'd)
      (put-text-property 17 20 'zone 'e)
      (setq-local my-steps steps)
      (let* ((ov (make-overlay 5 16))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 1))
             (results nil))
        (undo-boundary)
        (goto-char 1)
        (while (search-forward "OLD" nil t)
          (setq step-num (1+ step-num))
          (let ((pos (match-beginning 0))
                (match-str (match-string 0)))
            (replace-match "NEW")
            (push (replace-step :step step-num :pos pos :match match-str
                               :replaced "NEW" :buf-after (buffer-string)) steps)))
        (setq steps (reverse steps))
        (setq results (mapcar (lambda (s) (list (rs-step s) (rs-pos s))) steps))
        (goto-char (point-max))
        (insert (format " | results=%s steps=%d m=%d ov=[%d,%d]"
                       results (length steps) (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'rs-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length steps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_match_data_overlay_markers_integrity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function mis)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass md-integrity-snap ()
    ((label :initarg :label :accessor mis-label :initform "")
     (md-markers :initarg :md-markers :accessor mis-md :initform nil)
     (marker-pos :initarg :marker-pos :accessor mis-mp :initform 0)
     (ov-bounds :initarg :ov-bounds :accessor mis-ov :initform nil)))
  (let* ((buf (generate-new-buffer "md5"))
         (snaps nil)
         (saved-md nil))
    (with-current-buffer buf
      (insert "ABCDEFGH-ABCDEFGH-ABCDEFGH")
      (put-text-property 1 9 'zone 'a)
      (put-text-property 10 18 'zone 'b)
      (put-text-property 19 27 'zone 'c)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 10 18))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 10))
             (results nil))
        (undo-boundary)
        (goto-char 1)
        (search-forward "ABCDEFGH")
        (setq saved-md (match-data))
        (push (mis :label "first-match" :md-markers saved-md
                  :marker-pos (marker-position m) :ov-bounds (list (overlay-start ov) (overlay-end ov))) snaps)
        (goto-char 3)
        (insert "XX")
        (push (mis :label "after-insert" :md-markers (match-data)
                  :marker-pos (marker-position m) :ov-bounds (list (overlay-start ov) (overlay-end ov))) snaps)
        (let ((restore-integers (list (car saved-md) (cadr saved-md))))
          (set-match-data restore-integers)
          (push (mis :label "after-restore" :md-markers (match-data)
                    :marker-pos (marker-position m)
                    :ov-bounds (list (overlay-start ov) (overlay-end ov))) snaps))
        (delete-region 3 5)
        (push (mis :label "after-delete" :md-markers (match-data)
                  :marker-pos (marker-position m) :ov-bounds (list (overlay-start ov) (overlay-end ov))) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (mis-label s) (mis-mp s) (mis-ov s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'mis-log t)
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

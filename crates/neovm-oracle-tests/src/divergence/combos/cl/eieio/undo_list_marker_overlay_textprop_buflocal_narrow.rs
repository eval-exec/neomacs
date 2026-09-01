//! Combo: cl-eieio undo-boundary/buffer-undo-list inspection + overlays + markers + textprop + buflocal + narrow.
//! Tests undo list structure inspection with EIEIO objects.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_undo_list_insert_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass undo-entry ()
    ((type :initarg :type :accessor ue-type :initform "")
     (count :initarg :count :accessor ue-count :initform 0)))
  (let* ((buf (generate-new-buffer "ul1"))
         (entries nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'z1)
      (put-text-property 6 9 'zone 'z2)
      (put-text-property 10 13 'zone 'z3)
      (setq-local my-entries entries)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 5))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (goto-char 3)
        (insert "XX")
        (undo-boundary)
        (goto-char 8)
        (insert "YY")
        (undo-boundary)
        (delete-region 3 5)
        (undo-boundary)
        (let ((ul buffer-undo-list)
              (insert-count 0)
              (delete-count 0)
              (boundary-count 0))
          (while ul
            (let ((entry (car ul)))
              (cond
               ((null entry) nil)
               ((numberp entry) nil)
               ((markerp entry) nil)
               ((and (consp entry) (stringp (cdr entry)))
                (setq insert-count (1+ insert-count)))
               ((and (consp entry) (numberp (car entry)) (numberp (cdr entry)))
                (setq delete-count (1+ delete-count)))
               ((eq entry t) nil)
               (t (setq boundary-count (1+ boundary-count)))))
            (setq ul (cdr ul)))
          (push (undo-entry :type "insert" :count insert-count) entries)
          (push (undo-entry :type "delete" :count delete-count) entries)
          (push (undo-entry :type "other" :count boundary-count) entries))
        (setq entries (reverse entries))
        (setq results (mapcar (lambda (e) (list (ue-type e) (ue-count e))) entries))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ue-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 4 buffer-undo-list)
          (list bs (buffer-string)
                (length entries)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_undo_list_prop_changes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass undo-prop-entry ()
    ((type :initarg :type :accessor upe-type :initform "")
     (detail :initarg :detail :accessor upe-detail :initform nil)))
  (let* ((buf (generate-new-buffer "ul2"))
         (entries nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'z1)
      (put-text-property 6 9 'zone 'z2)
      (put-text-property 10 13 'zone 'z3)
      (setq-local my-entries entries)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 5))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (put-text-property 1 5 'zone 'modified)
        (undo-boundary)
        (put-text-property 6 9 'zone 'changed)
        (undo-boundary)
        (remove-text-properties 1 9 '(zone nil))
        (undo-boundary)
        (goto-char 3)
        (insert "XX")
        (undo-boundary)
        (let ((ul buffer-undo-list)
              (prop-changes 0)
              (text-changes 0))
          (while ul
            (let ((entry (car ul)))
              (cond
               ((null entry) nil)
               ((and (consp entry) (not (stringp (cdr entry))))
                (setq prop-changes (1+ prop-changes)))
               ((and (consp entry) (stringp (cdr entry)))
                (setq text-changes (1+ text-changes)))))
            (setq ul (cdr ul)))
          (push (undo-prop-entry :type "prop" :detail prop-changes) entries)
          (push (undo-prop-entry :type "text" :detail text-changes) entries))
        (setq entries (reverse entries))
        (setq results (mapcar (lambda (e) (list (upe-type e) (upe-detail e))) entries))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'upe-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 5 buffer-undo-list)
          (list bs (buffer-string)
                (length entries)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (get-text-property 1 'zone)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_undo_list_overlay_changes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass undo-ov-entry ()
    ((step :initarg :step :accessor uoe-step :initform "")
     (ov-start :initarg :ov-start :accessor uoe-start :initform 0)
     (ov-end :initarg :ov-end :accessor uoe-end :initform 0)
     (ov-face :initarg :ov-face :accessor uoe-face :initform nil)))
  (let* ((buf (generate-new-buffer "ul3"))
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
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (undo-ov-entry :step "init"
                            :ov-start (overlay-start ov)
                            :ov-end (overlay-end ov)
                            :ov-face (overlay-get ov 'face)) snaps)
        (goto-char 3)
        (insert "XX")
        (push (undo-ov-entry :step "after-insert"
                            :ov-start (overlay-start ov)
                            :ov-end (overlay-end ov)
                            :ov-face (overlay-get ov 'face)) snaps)
        (overlay-put ov 'face 'italic)
        (push (undo-ov-entry :step "after-face-change"
                            :ov-start (overlay-start ov)
                            :ov-end (overlay-end ov)
                            :ov-face (overlay-get ov 'face)) snaps)
        (move-overlay ov 3 18)
        (push (undo-ov-entry :step "after-move"
                            :ov-start (overlay-start ov)
                            :ov-end (overlay-end ov)
                            :ov-face (overlay-get ov 'face)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (uoe-step s) (uoe-start s) (uoe-end s) (uoe-face s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'uoe-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 3 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (overlay-get ov 'face)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_undo_narrow_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass undo-narrow-snap ()
    ((step :initarg :step :accessor uns-step :initform "")
     (buf-string :initarg :buf-string :accessor uns-bs :initform "")
     (m-pos :initarg :m-pos :accessor uns-mpos :initform 0)
     (undo-list-len :initarg :ul-len :accessor uns-ullen :initform 0)))
  (let* ((buf (generate-new-buffer "ul4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'zone 'z1)
      (put-text-property 6 10 'zone 'z2)
      (put-text-property 11 15 'zone 'z3)
      (put-text-property 16 20 'zone 'z4)
      (put-text-property 21 25 'zone 'z5)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (undo-narrow-snap :step "init"
                               :buf-string (buffer-string)
                               :m-pos (marker-position m)
                               :ul-len (length buffer-undo-list)) snaps)
        (save-restriction
          (narrow-to-region 6 15)
          (goto-char 8)
          (insert "XX")
          (push (undo-narrow-snap :step "narrow-insert"
                                 :buf-string (buffer-string)
                                 :m-pos (marker-position m)
                                 :ul-len (length buffer-undo-list)) snaps))
        (push (undo-narrow-snap :step "after-widen"
                               :buf-string (buffer-string)
                               :m-pos (marker-position m)
                               :ul-len (length buffer-undo-list)) snaps)
        (goto-char 3)
        (insert "YY")
        (push (undo-narrow-snap :step "wide-insert"
                               :buf-string (buffer-string)
                               :m-pos (marker-position m)
                               :ul-len (length buffer-undo-list)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (uns-step s) (uns-mpos s) (uns-ullen s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'uns-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 3 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_undo_list_marker_movement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass undo-marker-snap ()
    ((step :initarg :step :accessor ums-step :initform "")
     (m1-pos :initarg :m1 :accessor ums-m1 :initform 0)
     (m2-pos :initarg :m2 :accessor ums-m2 :initform 0)
     (buf-string :initarg :buf-string :accessor ums-bs :initform "")))
  (let* ((buf (generate-new-buffer "ul5"))
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
             (m1 (make-marker))
             (m2 (make-marker))
             (_ (set-marker m1 5))
             (_ (set-marker m2 15))
             (_ (set-marker-insertion-type m1 t))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (undo-marker-snap :step "init"
                               :m1 (marker-position m1)
                               :m2 (marker-position m2)
                               :buf-string (buffer-string)) snaps)
        (goto-char 3)
        (insert "XX")
        (push (undo-marker-snap :step "insert-at-3"
                               :m1 (marker-position m1)
                               :m2 (marker-position m2)
                               :buf-string (buffer-string)) snaps)
        (set-marker m1 8)
        (delete-region 5 8)
        (push (undo-marker-snap :step "delete-5-8"
                               :m1 (marker-position m1)
                               :m2 (marker-position m2)
                               :buf-string (buffer-string)) snaps)
        (set-marker m2 5)
        (put-text-property 1 5 'zone 'restored)
        (push (undo-marker-snap :step "prop-change"
                               :m1 (marker-position m1)
                               :m2 (marker-position m2)
                               :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ums-step s) (ums-m1 s) (ums-m2 s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m1=%d m2=%d"
                       results (marker-position m1) (marker-position m2)))
        (put-text-property (1- (point-max)) (point-max) 'ums-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 4 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m1) (marker-position m2)
                (overlay-start ov) (overlay-end ov)
                (get-text-property 1 'zone)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

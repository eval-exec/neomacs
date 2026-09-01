//! Combo: cl-eieio cl-labels/cl-flet closures inside EIEIO methods
//! + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests closure capture of EIEIO object state combined with overlay
//! manipulation and editing operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_closure_capture_slots_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass closure-obj ()
    ((name :initarg :name :accessor co-name :initform "")
     (edits :initarg :edits :accessor co-edits :initform 0)
     (last-pos :initarg :pos :accessor co-pos :initform 0)))
  (let* ((buf (generate-new-buffer "cl1"))
         (snaps nil)
         (obj (closure-obj :name "test" :edits 0 :pos 0)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (put-text-property 26 30 'face 'error)
      (setq-local my-cl-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil)
             (do-edit
              (cl-labels ((track-edit (pos)
                            (setf (co-edits obj) (1+ (co-edits obj)))
                            (setf (co-pos obj) pos)
                            (push (format "edit@%d:n%d" pos (co-edits obj))
                                  my-cl-log)))
                (lambda (at str)
                  (goto-char at)
                  (insert str)
                  (track-edit at)))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "init" (co-edits obj) (co-pos obj)) results)
        (funcall do-edit 8 "XXX")
        (undo-boundary)
        (push (list "edit1" (co-edits obj) (co-pos obj)
                    (marker-position m)) results)
        (funcall do-edit 15 "YYY")
        (undo-boundary)
        (push (list "edit2" (co-edits obj) (co-pos obj)
                    (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 25)
          (funcall do-edit 10 "ZZ")
          (undo-boundary)
          (push (list "edit-narrow" (co-edits obj) (co-pos obj)
                      (marker-position m)) results))
        (push (list "widen" (co-edits obj) (co-pos obj)
                    (marker-position m)) results)
        (primitive-undo 1 buffer-undo-list)
        (push (list "undo-narrow" (co-edits obj) (co-pos obj)
                    (marker-position m)) results)
        (primitive-undo 1 buffer-undo-list)
        (push (list "undo-edit2" (co-edits obj) (co-pos obj)
                    (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S cl-log=%S"
                       results (reverse my-cl-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'cl-log t)
        (list (buffer-string)
              (co-edits obj) (co-pos obj)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-cl-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_closure_overlay_mod_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable ov1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass hook-track ()
    ((fire-count :initarg :count :accessor ht-count :initform 0)
     (last-range :initarg :range :accessor ht-range :initform nil)
     (label :initarg :label :accessor ht-label :initform "")))
  (let* ((buf (generate-new-buffer "cl2"))
         (snaps nil)
         (tracker (hook-track :count 0 :range nil :label "main")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (setq-local my-ht-log nil)
      (let* ((ov1 (make-overlay 6 15))
             (ov2 (make-overlay 10 20))
             (_ (overlay-put ov1 'face 'bold))
             (_ (overlay-put ov1 'priority 5))
             (_ (overlay-put ov2 'face 'italic))
             (_ (overlay-put ov2 'priority 10))
             (m (set-marker (make-marker) 12))
             (results nil)
             (make-hook
              (lambda (label)
                (cl-labels ((on-modify (ov after-p beg end &optional _len)
                              (when after-p
                                (setf (ht-count tracker) (1+ (ht-count tracker)))
                                (setf (ht-range tracker) (list beg end))
                                (push (format "%s:%d-%d" label beg end)
                                      my-ht-log)))))
                  (list #'on-modify)))))
        (overlay-put ov1 'modification-hooks (funcall make-hook "ov1"))
        (overlay-put ov2 'modification-hooks (funcall make-hook "ov2"))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "init" (ht-count tracker) (ht-range tracker)) results)
        (goto-char 8)
        (insert "MMM")
        (push (list "edit1" (ht-count tracker) (ht-range tracker)
                    (marker-position m)) results)
        (goto-char 15)
        (insert "NNN")
        (push (list "edit2" (ht-count tracker) (ht-range tracker)
                    (marker-position m)) results)
        (delete-region 6 10)
        (push (list "delete" (ht-count tracker) (ht-range tracker)
                    (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S ht-log=%S"
                       results (reverse my-ht-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ht-log t)
        (list (buffer-string)
              (ht-count tracker) (ht-range tracker)
              (marker-position m)
              (overlay-start ov1) (overlay-end ov1)
              (overlay-start ov2) (overlay-end ov2)
              my-ht-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_closure_nested_save_excursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass pos-track ()
    ((pos-list :initarg :pos :accessor pt-positions :initform nil)
     (counter :initarg :count :accessor pt-count :initform 0)))
  (let* ((buf (generate-new-buffer "cl3"))
         (snaps nil)
         (tracker (pos-track :pos nil :count 0)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (put-text-property 31 35 'zone 'g)
      (put-text-property 36 40 'zone 'h)
      (setq-local my-pt-log nil)
      (let* ((ov (make-overlay 10 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 18))
             (results nil)
             (record-pos
              (cl-labels ((snap () (setf (pt-count tracker) (1+ (pt-count tracker)))
                            (setf (pt-positions tracker) (cons (point) (pt-positions tracker)))
                            (push (format "snap@%d" (point)) my-pt-log)))
                (lambda ()
                  (snap)
                  (list (pt-count tracker) (marker-position m))))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (goto-char 5)
        (push (funcall record-pos) results)
        (save-excursion
          (save-restriction
            (narrow-to-region 8 28)
            (goto-char 10)
            (push (funcall record-pos) results)
            (insert "XXX")
            (setq my-pt-log (cons "ins-narrow@10" my-pt-log))
            (save-excursion
              (goto-char 20)
              (push (funcall record-pos) results)
              (insert "YYY"))
            (push (funcall record-pos) results)))
        (push (funcall record-pos) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S pt-log=%S"
                       results (reverse my-pt-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'pt-log t)
        (list (buffer-string)
              (pt-count tracker)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-pt-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_closure_defmethod_with_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass edit-ctx ()
    ((buf-name :initarg :buf :accessor ec-buf :initform "")
     (edit-log :initarg :log :accessor ec-log :initform nil)
     (total-inserts :initarg :total :accessor ec-total :initform 0)))
  (defmethod ec-do-edit ((ctx edit-ctx) at str)
    (with-current-buffer (get-buffer (ec-buf ctx))
      (goto-char at)
      (insert str)
      (setf (ec-total ctx) (+ (length str) (ec-total ctx)))
      (push (format "ins@%d:%S" at str) (ec-log ctx))))
  (let* ((buf (generate-new-buffer "cl4"))
         (snaps nil)
         (ctx (edit-ctx :buf (buffer-name buf) :log nil :total 0)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (setq-local my-ec-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "init" (ec-total ctx) (ec-log ctx)) results)
        (ec-do-edit ctx 8 "XXX")
        (undo-boundary)
        (push (list "edit1" (ec-total ctx) (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 20)
          (ec-do-edit ctx 10 "YYY")
          (undo-boundary)
          (push (list "narrow-edit" (ec-total ctx) (marker-position m)) results))
        (ec-do-edit ctx 20 "ZZZ")
        (undo-boundary)
        (push (list "edit3" (ec-total ctx) (marker-position m)) results)
        (primitive-undo 1 buffer-undo-list)
        (push (list "undo3" (ec-total ctx) (marker-position m)) results)
        (primitive-undo 1 buffer-undo-list)
        (push (list "undo-narrow" (ec-total ctx) (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S ec-log=%S"
                       results (reverse (ec-log ctx))))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ec-log t)
        (list (buffer-string)
              (ec-total ctx)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (ec-log ctx))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_closure_with_overlay_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function overlay-live-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ov-collect ()
    ((collected :initarg :col :accessor oc-col :initform nil)
     (count :initarg :count :accessor oc-count :initform 0)
     (label :initarg :label :accessor oc-label :initform "")))
  (let* ((buf (generate-new-buffer "cl5"))
         (snaps nil)
         (collector (ov-collect :col nil :count 0 :label "main")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (setq-local my-oc-log nil)
      (let* ((ov1 (make-overlay 3 8))
             (ov2 (make-overlay 6 15))
             (ov3 (make-overlay 12 20))
             (ov4 (make-overlay 18 25))
             (_ (overlay-put ov1 'face 'bold))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'face 'italic))
             (_ (overlay-put ov2 'priority 2))
             (_ (overlay-put ov3 'face 'underline))
             (_ (overlay-put ov3 'priority 3))
             (_ (overlay-put ov4 'face 'shadow))
             (_ (overlay-put ov4 'priority 4))
             (m (set-marker (make-marker) 10))
             (results nil)
             (collect-at
              (lambda (pos)
                (cl-labels ((gather (ovs)
                             (dolist (o ovs)
                               (when (overlay-live-p o)
                                 (push (list (overlay-start o)
                                            (overlay-end o)
                                            (overlay-get o 'face))
                                       (oc-col collector))
                                 (setf (oc-count collector)
                                       (1+ (oc-count collector)))))))
                  (setf (oc-col collector) nil)
                  (setf (oc-count collector) 0)
                  (gather (overlays-at pos))
                  (push (format "at-%d:%d" pos (oc-count collector)) my-oc-log)
                  (oc-count collector)))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "init"
                    (funcall collect-at 10)
                    (oc-col collector)) results)
        (goto-char 5)
        (insert "PPP")
        (push (list "edit1"
                    (funcall collect-at 10)
                    (oc-col collector)
                    (marker-position m)) results)
        (delete-overlay ov2)
        (setq my-oc-log (cons "del-ov2" my-oc-log))
        (push (list "del-ov2"
                    (funcall collect-at 10)
                    (oc-col collector)
                    (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S oc-log=%S"
                       results (reverse my-oc-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'oc-log t)
        (list (buffer-string)
              (marker-position m)
              (overlay-live-p ov1) (overlay-live-p ov2)
              (overlay-live-p ov3) (overlay-live-p ov4)
              my-oc-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

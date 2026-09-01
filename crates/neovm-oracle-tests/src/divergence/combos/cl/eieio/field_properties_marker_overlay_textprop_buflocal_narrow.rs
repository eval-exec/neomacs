//! Combo: cl-eieio field properties + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests field property interactions with EIEIO objects, narrowing, and buffer editing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_field_beginning_end_of_field() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass field-boundary ()
    ((name :initarg :name :accessor fb-name :initform "")
     (beg-of-field :initarg :beg :accessor fb-beg :initform 0)
     (end-of-field :initarg :end :accessor fb-end :initform 0)))
  (let* ((buf (generate-new-buffer "fp1"))
         (boundaries nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'field 'alpha)
      (put-text-property 6 10 'field 'beta)
      (put-text-property 11 15 'field 'gamma)
      (put-text-property 16 20 'field 'delta)
      (setq-local my-boundaries boundaries)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (dotimes (i 20)
          (let* ((pos (1+ i))
                 (bof (field-beginning pos))
                 (eof (field-end pos)))
            (push (field-boundary :name (format "p%d" pos) :beg bof :end eof)
                  boundaries)))
        (setq boundaries (reverse boundaries))
        (setq results (mapcar (lambda (b) (list (fb-name b) (fb-beg b) (fb-end b)))
                             boundaries))
        (goto-char 3)
        (insert "XX")
        (push (list 'after-insert
                   (field-beginning 3) (field-end 3)
                   (marker-position m)) results)
        (delete-region 3 5)
        (push (list 'after-delete
                   (field-beginning 3) (field-end 3)
                   (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%d field-at-3=%s m=%d"
                       (length results)
                       (get-text-property 3 'field)
                       (marker-position m)))
        (set-marker m 4)
        (put-text-property (1- (point-max)) (point-max) 'fb-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length boundaries)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_field_overlay_priority_clash() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass field-clash ()
    ((pos :initarg :pos :accessor fc-pos :initform 0)
     (text-field :initarg :text-field :accessor fc-tf :initform nil)
     (ov-field :initarg :ov-field :accessor fc-of :initform nil)))
  (let* ((buf (generate-new-buffer "fp2"))
         (clashes nil))
    (with-current-buffer buf
      (insert "AAAAAAAAAAAAAAAAAAAA")
      (put-text-property 1 6 'field 'text-a)
      (put-text-property 7 12 'field 'text-b)
      (put-text-property 13 20 'field 'text-c)
      (setq-local my-clashes clashes)
      (let* ((ov1 (make-overlay 4 9))
             (ov2 (make-overlay 10 16))
             (_ (overlay-put ov1 'field 'ov-alpha))
             (_ (overlay-put ov1 'priority 10))
             (_ (overlay-put ov2 'field 'ov-beta))
             (_ (overlay-put ov2 'priority 5))
             (m (make-marker))
             (_ (set-marker m 7))
             (results nil))
        (undo-boundary)
        (let ((positions '(1 3 5 7 9 11 13 15 17 19)))
          (dolist (pos positions)
            (let ((tf (get-text-property pos 'field))
                  (of (get-char-property pos 'field)))
              (push (field-clash :pos pos :text-field tf :ov-field of) clashes))))
        (setq clashes (reverse clashes))
        (setq results (mapcar (lambda (c) (list (fc-pos c) (fc-tf c) (fc-of c)))
                             clashes))
        (goto-char 5)
        (insert "MM")
        (push (list 'after-insert
                   (get-text-property 5 'field)
                   (get-char-property 5 'field)
                   (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%d m=%d ov1=[%d,%d] ov2=[%d,%d]"
                       (length results) (marker-position m)
                       (overlay-start ov1) (overlay-end ov1)
                       (overlay-start ov2) (overlay-end ov2)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'fc-log t)
        (list (buffer-string)
              (length clashes)
              (marker-position m)
              (overlay-start ov1) (overlay-end ov1)
              (overlay-start ov2) (overlay-end ov2)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_field_narrow_constrain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass narrow-field-snap ()
    ((narrow-bounds :initarg :narrow :accessor nfs-narrow :initform nil)
     (field-at-point :initarg :field :accessor nfs-field :initform nil)
     (field-beg :initarg :fbeg :accessor nfs-fbeg :initform 0)
     (field-end :initarg :fend :accessor nfs-fend :initform 0)
     (constrained-beg :initarg :cbeg :accessor nfs-cbeg :initform 0)
     (constrained-end :initarg :cend :accessor nfs-cend :initform 0)))
  (let* ((buf (generate-new-buffer "fp3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'field 'f1)
      (put-text-property 6 10 'field 'f2)
      (put-text-property 11 15 'field 'f3)
      (put-text-property 16 20 'field 'f4)
      (put-text-property 21 25 'field 'f5)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (push (narrow-field-snap :narrow (list (point-min) (point-max))
                                :field (get-text-property 8 'field)
                                :fbeg (field-beginning 8)
                                :fend (field-end 8)
                                :cbeg (field-beginning 8 t)
                                :cend (field-end 8 t)) snaps)
        (save-restriction
          (narrow-to-region 4 18)
          (push (narrow-field-snap :narrow (list (point-min) (point-max))
                                  :field (get-text-property 8 'field)
                                  :fbeg (field-beginning 8)
                                  :fend (field-end 8)
                                  :cbeg (field-beginning 8 t)
                                  :cend (field-end 8 t)) snaps)
          (goto-char 8)
          (insert "QQ")
          (push (narrow-field-snap :narrow (list (point-min) (point-max))
                                  :field (get-text-property 8 'field)
                                  :fbeg (field-beginning 8)
                                  :fend (field-end 8)
                                  :cbeg (field-beginning 8 t)
                                  :cend (field-end 8 t)) snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s)
                               (list (nfs-narrow s) (nfs-field s)
                                     (nfs-fbeg s) (nfs-fend s)
                                     (nfs-cbeg s) (nfs-cend s)))
                             snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 5)
        (put-text-property (1- (point-max)) (point-max) 'nfs-log t)
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
fn combo_eieio_field_delete_field_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass field-edit ()
    ((op :initarg :op :accessor fe-op :initform "")
     (field-before :initarg :field-before :accessor fe-before :initform nil)
     (field-after :initarg :field-after :accessor fe-after :initform nil)
     (buf-string :initarg :buf-string :accessor fe-bs :initform "")))
  (let* ((buf (generate-new-buffer "fp4"))
         (edits nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'field 'fa)
      (put-text-property 6 10 'field 'fb)
      (put-text-property 11 15 'field 'fc)
      (put-text-property 16 20 'field 'fd)
      (setq-local my-edits edits)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 2))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (push (field-edit :op "init"
                         :field-before (get-text-property 3 'field)
                         :field-after (get-text-property 3 'field)
                         :buf-string (buffer-string)) edits)
        (delete-region 3 8)
        (push (field-edit :op "delete-3-8"
                         :field-before 'unknown
                         :field-after (get-text-property 3 'field)
                         :buf-string (buffer-string)) edits)
        (goto-char 6)
        (insert "NEW")
        (push (field-edit :op "insert-at-6"
                         :field-before 'unknown
                         :field-after (get-text-property 6 'field)
                         :buf-string (buffer-string)) edits)
        (setq edits (reverse edits))
        (setq results (mapcar (lambda (e) (list (fe-op e) (fe-after e))) edits))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'fe-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length edits)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (get-text-property 3 'field)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_field_line_beginning_field_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (wrong-number-of-arguments line-beginning-position 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass line-field-snap ()
    ((pos :initarg :pos :accessor lfs-pos :initform 0)
     (bol :initarg :bol :accessor lfs-bol :initform 0)
     (field-bol :initarg :fbol :accessor lfs-fbol :initform 0)
     (field-at-pos :initarg :field :accessor lfs-field :initform nil)))
  (let* ((buf (generate-new-buffer "fp5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA\nBBBB\nCCCC\nDDDD")
      (put-text-property 1 5 'field 'f1)
      (put-text-property 6 10 'field 'f2)
      (put-text-property 11 15 'field 'f3)
      (put-text-property 16 20 'field 'f4)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (let ((positions '(1 5 6 8 11 13 16 18)))
          (dolist (pos positions)
            (let ((bol (line-beginning-position (if (or (= pos 1) (= pos 5)) 1 1)))
                  (fbol (line-beginning-position 1 pos)))
              (push (line-field-snap :pos pos
                                    :bol bol
                                    :fbol (field-beginning pos)
                                    :field (get-text-property pos 'field))
                    snaps))))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s)
                               (list (lfs-pos s) (lfs-bol s)
                                     (lfs-fbol s) (lfs-field s)))
                             snaps))
        (goto-char 8)
        (insert "XX")
        (push (list 'after-insert
                   (line-beginning-position)
                   (field-beginning)
                   (get-text-property 8 'field)
                   (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 4)
        (put-text-property (1- (point-max)) (point-max) 'lfs-log t)
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

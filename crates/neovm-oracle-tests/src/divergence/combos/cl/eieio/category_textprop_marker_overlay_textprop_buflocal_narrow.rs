//! Combo: cl-eieio category text properties + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests category properties with EIEIO objects, syntax lookup via categories.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_category_syntax_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass cat-state ()
    ((pos :initarg :pos :accessor cat-pos :initform 0)
     (category :initarg :category :accessor cat-cat :initform nil)
     (syntax :initarg :syntax :accessor cat-syn :initform nil)))
  (let* ((buf (generate-new-buffer "ct1"))
         (my-cat (make-category-table))
         (states nil))
    (with-current-buffer buf
      (set-category-table my-cat)
      (define-category ?a "alpha" my-cat)
      (define-category ?b "beta" my-cat)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'category ?a)
      (put-text-property 6 9 'category ?b)
      (put-text-property 10 13 'category ?a)
      (setq-local my-states states)
      (let* ((ov (make-overlay 6 9))
             (_ (overlay-put ov 'priority 1))
             (_ (overlay-put ov 'category ?b))
             (m (make-marker))
             (_ (set-marker m 6))
             (results nil))
        (undo-boundary)
        (dolist (pos '(1 3 5 7 9 11))
          (let ((cat (get-text-property pos 'category)))
            (push (cat-state :pos pos :category cat :syntax nil) states)))
        (setq states (reverse states))
        (setq results (mapcar (lambda (s) (list (cat-pos s) (cat-cat s))) states))
        (goto-char 3)
        (insert "XX")
        (push (list 'after-insert (get-text-property 3 'category) (marker-position m)) results)
        (delete-region 3 5)
        (push (list 'after-delete (get-text-property 3 'category) (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'cat-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length states)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_category_multi_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass multi-cat ()
    ((range :initarg :range :accessor mc-range :initform nil)
     (cat-a :initarg :cat-a :accessor mc-a :initform nil)
     (cat-b :initarg :cat-b :accessor mc-b :initform nil)
     (merged :initarg :merged :accessor mc-merged :initform nil)))
  (let* ((buf (generate-new-buffer "ct2"))
         (my-cat (make-category-table))
         (states nil))
    (with-current-buffer buf
      (set-category-table my-cat)
      (define-category ?x "cat-x" my-cat)
      (define-category ?y "cat-y" my-cat)
      (insert "AAAAAAAAAAAAAAAA")
      (put-text-property 1 5 'category ?x)
      (put-text-property 6 10 'category ?y)
      (put-text-property 11 16 'category ?x)
      (setq-local my-states states)
      (let* ((ov1 (make-overlay 3 8))
             (ov2 (make-overlay 9 14))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov1 'category ?y))
             (_ (overlay-put ov2 'priority 2))
             (_ (overlay-put ov2 'category ?x))
             (m (make-marker))
             (_ (set-marker m 5))
             (results nil))
        (undo-boundary)
        (dolist (pos '(1 3 5 7 9 11 13 15))
          (let ((text-cat (get-text-property pos 'category))
                (char-cat (get-char-property pos 'category)))
            (push (multi-cat :range pos :cat-a text-cat :cat-b char-cat
                            :merged (or text-cat char-cat)) states)))
        (setq states (reverse states))
        (setq results (mapcar (lambda (s) (list (mc-range s) (mc-a s) (mc-b s)))
                             states))
        (goto-char 4)
        (insert "MM")
        (push (list 'after-insert (get-text-property 4 'category)
                   (get-char-property 4 'category) (marker-position m)) results)
        (delete-region 4 6)
        (push (list 'after-delete (get-text-property 4 'category)
                   (get-char-property 4 'category) (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'mc-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length states)
                (marker-position m)
                (overlay-start ov1) (overlay-end ov1)
                (overlay-start ov2) (overlay-end ov2)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_category_narrow_scan() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass cat-scan-result ()
    ((pos :initarg :pos :accessor csr-pos :initform 0)
     (category :initarg :category :accessor csr-cat :initform nil)
     (narrow-bounds :initarg :narrow :accessor csr-narrow :initform nil)))
  (let* ((buf (generate-new-buffer "ct3"))
         (my-cat (make-category-table))
         (scan-results nil))
    (with-current-buffer buf
      (set-category-table my-cat)
      (define-category ?p "para" my-cat)
      (define-category ?c "code" my-cat)
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'category ?p)
      (put-text-property 6 10 'category ?c)
      (put-text-property 11 15 'category ?p)
      (put-text-property 16 20 'category ?c)
      (put-text-property 21 25 'category ?p)
      (setq-local my-scan scan-results)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (save-restriction
          (narrow-to-region 6 20)
          (let ((pos (point-min)))
            (while (< pos (point-max))
              (let ((cat (get-text-property pos 'category)))
                (push (cat-scan-result :pos pos :category cat
                                      :narrow (list (point-min) (point-max)))
                      scan-results)
                (setq pos (or (next-single-property-change pos 'category) (point-max)))))))
        (setq scan-results (reverse scan-results))
        (setq results (mapcar (lambda (r) (list (csr-pos r) (csr-cat r))) scan-results))
        (goto-char (point-max))
        (insert (format " | results=%s scan=%d m=%d"
                       results (length scan-results) (marker-position m)))
        (set-marker m 4)
        (put-text-property (1- (point-max)) (point-max) 'csr-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length scan-results)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_category_overlay_evaporate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function overlay-live-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass cat-ov-snap ()
    ((label :initarg :label :accessor cos-label :initform "")
     (ov-alive :initarg :ov-alive :accessor cos-alive :initform nil)
     (cat-at-pos :initarg :cat :accessor cos-cat :initform nil)
     (pos :initarg :pos :accessor cos-pos :initform 0)))
  (let* ((buf (generate-new-buffer "ct4"))
         (my-cat (make-category-table))
         (snaps nil))
    (with-current-buffer buf
      (set-category-table my-cat)
      (define-category ?a "alpha" my-cat)
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'category ?a)
      (put-text-property 6 10 'category ?a)
      (put-text-property 11 15 'category ?a)
      (put-text-property 16 20 'category ?a)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'evaporate t))
             (_ (overlay-put ov 'priority 1))
             (_ (overlay-put ov 'category ?a))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (push (cat-ov-snap :label "init" :ov-alive (overlay-live-p ov)
                          :cat-at-pos (get-char-property 7 'category) :pos 7) snaps)
        (delete-region 6 10)
        (push (cat-ov-snap :label "after-delete" :ov-alive (overlay-live-p ov)
                          :cat-at-pos (get-text-property 6 'category) :pos 6) snaps)
        (push (list 'm-after (marker-position m) (marker-live-p m)) results)
        (goto-char 3)
        (insert "NEW")
        (push (cat-ov-snap :label "after-insert" :ov-alive (overlay-live-p ov)
                          :cat-at-pos (get-text-property 3 'category) :pos 3) snaps)
        (setq snaps (reverse snaps))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s snaps=%d m=%d"
                       results (length snaps) (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'cos-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-live-p ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_category_undo_restore_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass cat-undo-snap ()
    ((step :initarg :step :accessor cus-step :initform "")
     (cats :initarg :cats :accessor cus-cats :initform nil)))
  (let* ((buf (generate-new-buffer "ct5"))
         (my-cat (make-category-table))
         (snaps nil))
    (with-current-buffer buf
      (set-category-table my-cat)
      (define-category ?x "x-cat" my-cat)
      (define-category ?y "y-cat" my-cat)
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'category ?x)
      (put-text-property 6 10 'category ?y)
      (put-text-property 11 15 'category ?x)
      (put-text-property 16 20 'category ?y)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (_ (overlay-put ov 'category ?x))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (push (cat-undo-snap :step "init"
                            :cats (mapcar (lambda (p) (get-text-property p 'category))
                                         '(1 6 11 16))) snaps)
        (put-text-property 1 10 'category ?y)
        (push (cat-undo-snap :step "after-put"
                            :cats (mapcar (lambda (p) (get-text-property p 'category))
                                         '(1 6 11 16))) snaps)
        (remove-text-properties 1 20 '(category nil))
        (push (cat-undo-snap :step "after-remove"
                            :cats (mapcar (lambda (p) (get-text-property p 'category))
                                         '(1 6 11 16))) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (cus-step s) (cus-cats s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'cus-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (mapcar (lambda (p) (get-text-property p 'category)) '(1 6 11 16))))))
    (kill-buffer buf)))"#,
        expect,
    );
}

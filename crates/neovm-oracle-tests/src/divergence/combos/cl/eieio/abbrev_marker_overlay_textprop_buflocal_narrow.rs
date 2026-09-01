//! Combo: cl-eieio abbrev expansion + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests abbrev-mode interactions with EIEIO objects tracking expansions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_abbrev_expansion_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass abbrev-snap ()
    ((step :initarg :step :accessor abs-step :initform "")
     (buf-string :initarg :buf-string :accessor abs-bs :initform "")
     (point :initarg :point :accessor abs-point :initform 0)))
  (let* ((buf (generate-new-buffer "ab1"))
         (table (make-abbrev-table))
         (snaps nil))
    (with-current-buffer buf
      (setq-local abbrev-mode t)
      (setq local-abbrev-table table)
      (define-abbrev table "xp" "EXPANDED" nil)
      (insert "AAAA-xp-BBBB")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 8 'zone 'b)
      (put-text-property 9 13 'zone 'c)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 5 9))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 6))
             (results nil))
        (undo-boundary)
        (push (abbrev-snap :step "init"
                          :buf-string (buffer-string)
                          :point (point)) snaps)
        (goto-char 8)
        (expand-abbrev)
        (push (abbrev-snap :step "after-expand"
                          :buf-string (buffer-string)
                          :point (point)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (abs-step s) (length (abs-bs s)) (abs-point s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'abs-log t)
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
fn combo_eieio_abbrev_multiple_expansions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass multi-abbr-snap ()
    ((step :initarg :step :accessor mas-step :initform "")
     (buf-string :initarg :buf-string :accessor mas-bs :initform "")
     (m-pos :initarg :m-pos :accessor mas-mp :initform 0)))
  (let* ((buf (generate-new-buffer "ab2"))
         (table (make-abbrev-table))
         (snaps nil))
    (with-current-buffer buf
      (setq-local abbrev-mode t)
      (setq local-abbrev-table table)
      (define-abbrev table "fa" "FIRST")
      (define-abbrev table "sb" "SECOND")
      (define-abbrev table "tc" "THIRD")
      (insert "fa-sb-tc")
      (put-text-property 1 3 'zone 'a)
      (put-text-property 4 6 'zone 'b)
      (put-text-property 7 9 'zone 'c)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 1 9))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 3))
             (results nil))
        (undo-boundary)
        (push (multi-abbr-snap :step "init"
                              :buf-string (buffer-string)
                              :m-pos (marker-position m)) snaps)
        (goto-char 3)
        (expand-abbrev)
        (push (multi-abbr-snap :step "after-fa"
                              :buf-string (buffer-string)
                              :m-pos (marker-position m)) snaps)
        (search-forward "sb")
        (backward-char 2)
        (expand-abbrev)
        (push (multi-abbr-snap :step "after-sb"
                              :buf-string (buffer-string)
                              :m-pos (marker-position m)) snaps)
        (search-forward "tc")
        (backward-char 2)
        (expand-abbrev)
        (push (multi-abbr-snap :step "after-tc"
                              :buf-string (buffer-string)
                              :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (mas-step s) (length (mas-bs s)) (mas-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'mas-log t)
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
fn combo_eieio_abbrev_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass abbr-prop-snap ()
    ((step :initarg :step :accessor aps-step :initform "")
     (buf-string :initarg :buf-string :accessor aps-bs :initform "")
     (prop-at-point :initarg :prop :accessor aps-prop :initform nil)))
  (let* ((buf (generate-new-buffer "ab3"))
         (table (make-abbrev-table))
         (snaps nil))
    (with-current-buffer buf
      (setq-local abbrev-mode t)
      (setq local-abbrev-table table)
      (define-abbrev table "xp" "EXPANDED")
      (insert "AAAA-xp-BBBB")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 8 'face 'italic)
      (put-text-property 9 13 'face 'underline)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 5 9))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 7))
             (results nil))
        (undo-boundary)
        (push (abbr-prop-snap :step "init"
                             :buf-string (buffer-string)
                             :prop (get-text-property 7 'face)) snaps)
        (goto-char 8)
        (expand-abbrev)
        (push (abbr-prop-snap :step "after-expand"
                             :buf-string (buffer-string)
                             :prop (get-text-property 7 'face)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (aps-step s) (aps-prop s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'aps-log t)
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
fn combo_eieio_abbrev_narrow_expand() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass abbr-narrow-snap ()
    ((step :initarg :step :accessor ans-step :initform "")
     (narrow-bounds :initarg :narrow :accessor ans-narrow :initform nil)
     (buf-string :initarg :buf-string :accessor ans-bs :initform "")))
  (let* ((buf (generate-new-buffer "ab4"))
         (table (make-abbrev-table))
         (snaps nil))
    (with-current-buffer buf
      (setq-local abbrev-mode t)
      (setq local-abbrev-table table)
      (define-abbrev table "xp" "EXPANDED")
      (insert "AAAA-xp-BBBB-xp-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 8 'zone 'b)
      (put-text-property 9 13 'zone 'c)
      (put-text-property 14 16 'zone 'd)
      (put-text-property 17 21 'zone 'e)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 16))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 7))
             (results nil))
        (undo-boundary)
        (push (abbr-narrow-snap :step "init"
                               :narrow (list (point-min) (point-max))
                               :buf-string (buffer-string)) snaps)
        (save-restriction
          (narrow-to-region 6 16)
          (goto-char 3)
          (expand-abbrev)
          (push (abbr-narrow-snap :step "narrow-expand"
                                 :narrow (list (point-min) (point-max))
                                 :buf-string (buffer-string)) snaps))
        (push (abbr-narrow-snap :step "after-widen"
                               :narrow (list (point-min) (point-max))
                               :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ans-step s) (length (ans-bs s)))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ans-log t)
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
fn combo_eieio_abbrev_undo_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass abbr-undo-snap ()
    ((step :initarg :step :accessor aus-step :initform "")
     (m-pos :initarg :m-pos :accessor aus-mp :initform 0)
     (buf-string :initarg :buf-string :accessor aus-bs :initform "")))
  (let* ((buf (generate-new-buffer "ab5"))
         (table (make-abbrev-table))
         (snaps nil))
    (with-current-buffer buf
      (setq-local abbrev-mode t)
      (setq local-abbrev-table table)
      (define-abbrev table "xp" "EXPANDED")
      (insert "AAAA-xp-BBBB")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 8 'zone 'b)
      (put-text-property 9 13 'zone 'c)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 5 9))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 7))
             (results nil))
        (undo-boundary)
        (push (abbr-undo-snap :step "init"
                             :m-pos (marker-position m)
                             :buf-string (buffer-string)) snaps)
        (goto-char 8)
        (expand-abbrev)
        (push (abbr-undo-snap :step "after-expand"
                             :m-pos (marker-position m)
                             :buf-string (buffer-string)) snaps)
        (goto-char 3)
        (insert "QQ")
        (push (abbr-undo-snap :step "after-insert"
                             :m-pos (marker-position m)
                             :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (aus-step s) (aus-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'aus-log t)
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

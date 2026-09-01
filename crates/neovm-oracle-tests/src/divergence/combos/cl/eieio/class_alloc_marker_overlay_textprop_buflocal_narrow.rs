//! Combo: cl-eieio :allocation :class (shared slots across instances)
//! + overlays + markers + textprop + buflocal + narrow.
//! Tests class-allocated slots shared state through editing operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_class_alloc_shared_counter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass shared-counter ()
    ((instance-name :initarg :name :accessor sc-name :initform "")
     (shared-count :allocation :class :initarg :count :accessor sc-count :initform 0)
     (instance-log :initarg :log :accessor sc-log :initform nil)))
  (let* ((buf (generate-new-buffer "ca1"))
         (snaps nil)
         (a (shared-counter :name "A" :log nil))
         (b (shared-counter :name "B" :log nil)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (setq-local my-ca-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "init" (sc-count a) (sc-count b)) results)
        (setf (sc-count a) 1)
        (push (list "inc-a" (sc-count a) (sc-count b)) results)
        (setf (sc-count b) (1+ (sc-count b)))
        (push (list "inc-b" (sc-count a) (sc-count b)) results)
        (goto-char 8)
        (insert "XXX")
        (setf (sc-count a) (1+ (sc-count a)))
        (push (sc-count a) (sc-log a))
        (push (list "edit" (sc-count a) (sc-count b) (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 25)
          (setf (sc-count b) (1+ (sc-count b)))
          (push (sc-count b) (sc-log b))
          (push (list "narrow" (sc-count a) (sc-count b) (marker-position m)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S log-a=%S log-b=%S"
                       results (sc-log a) (sc-log b)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ca-log t)
        (list (buffer-string)
              (sc-count a) (sc-count b)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-ca-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_class_alloc_shared_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass shared-list ()
    ((tag :initarg :tag :accessor sl-tag :initform "")
     (shared-data :allocation :class :initarg :data :accessor sl-data :initform nil)
     (personal :initarg :personal :accessor sl-personal :initform nil)))
  (let* ((buf (generate-new-buffer "ca2"))
         (snaps nil)
         (x (shared-list :tag "X" :personal (list 1)))
         (y (shared-list :tag "Y" :personal (list 2))))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (setq-local my-sl-ca-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "init" (sl-data x) (sl-data y)) results)
        (setf (sl-data x) (cons 'a (sl-data x)))
        (push (list "push-x" (sl-data x) (sl-data y)) results)
        (setf (sl-data y) (cons 'b (sl-data y)))
        (push (list "push-y" (sl-data x) (sl-data y)) results)
        (goto-char 8)
        (insert "XXX")
        (setq my-sl-ca-log (cons "ins@8" my-sl-ca-log))
        (push (list "edit" (sl-data x) (sl-data y) (marker-position m)) results)
        (push (marker-position m) (sl-personal x))
        (push (marker-position m) (sl-personal y))
        (push (list "personal" (sl-personal x) (sl-personal y)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S sl-ca-log=%S"
                       results (reverse my-sl-ca-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'sl-ca-log t)
        (list (buffer-string)
              (sl-data x) (sl-data y)
              (sl-personal x) (sl-personal y)
              (marker-position m)
              (overlay-start ov) (overlay-end ov))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_class_alloc_marker_shared() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass shared-marker-obj ()
    ((label :initarg :label :accessor smo-label :initform "")
     (shared-mk :allocation :class :initarg :mk :accessor smo-mk :initform nil)
     (count :initarg :count :accessor smo-count :initform 0)))
  (let* ((buf (generate-new-buffer "ca3"))
         (snaps nil)
         (p (shared-marker-obj :label "P" :count 0))
         (q (shared-marker-obj :label "Q" :count 0)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (setq-local my-smk-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (setf (smo-mk p) m)
        (push (list "set-mk-p" (marker-position (smo-mk p))
                    (marker-position (smo-mk q))) results)
        (set-marker m 15)
        (push (list "move-mk" (marker-position (smo-mk p))
                    (marker-position (smo-mk q))) results)
        (goto-char 8)
        (insert "XXX")
        (setf (smo-count p) (1+ (smo-count p)))
        (setq my-smk-log (cons "ins@8" my-smk-log))
        (push (list "edit" (marker-position (smo-mk p))
                    (marker-position (smo-mk q))
                    (smo-count p) (smo-count q)) results)
        (set-marker (smo-mk q) 3)
        (push (list "move-via-q" (marker-position (smo-mk p))
                    (marker-position (smo-mk q))) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S smk-log=%S"
                       results (reverse my-smk-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'smk-log t)
        (list (buffer-string)
              (marker-position (smo-mk p))
              (marker-position (smo-mk q))
              (smo-count p) (smo-count q)
              (overlay-start ov) (overlay-end ov)
              my-smk-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_class_alloc_with_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass shared-state ()
    ((id :initarg :id :accessor ss-id :initform "")
     (shared-val :allocation :class :initarg :sval :accessor ss-sval :initform 0)
     (local-val :initarg :lval :accessor ss-lval :initform 0)
     (log :initarg :log :accessor ss-log :initform nil)))
  (let* ((buf (generate-new-buffer "ca4"))
         (snaps nil)
         (s1 (shared-state :id "1" :lval 10 :log nil))
         (s2 (shared-state :id "2" :lval 20 :log nil)))
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
      (setq-local my-ss-log nil)
      (let* ((ov (make-overlay 10 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 18))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "init" (ss-sval s1) (ss-sval s2) (ss-lval s1) (ss-lval s2)) results)
        (save-restriction
          (narrow-to-region 8 28)
          (setf (ss-sval s1) 5)
          (setf (ss-lval s1) (+ (ss-lval s1) (marker-position m)))
          (push (list "narrow-edit" (ss-sval s1) (ss-sval s2)
                      (ss-lval s1) (ss-lval s2)
                      (marker-position m)) results)
          (goto-char 10)
          (insert "XXX")
          (setf (ss-sval s2) (+ (ss-sval s2) (ss-sval s1)))
          (push (list "narrow-ins" (ss-sval s1) (ss-sval s2)
                      (ss-lval s1) (ss-lval s2)
                      (marker-position m)) results))
        (setf (ss-lval s2) (+ (ss-lval s2) (ss-sval s2)))
        (push (list "post-narrow" (ss-sval s1) (ss-sval s2)
                    (ss-lval s1) (ss-lval s2)
                    (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S ss-log=%S"
                       results (reverse my-ss-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ss-log t)
        (list (buffer-string)
              (ss-sval s1) (ss-sval s2)
              (ss-lval s1) (ss-lval s2)
              (marker-position m)
              (overlay-start ov) (overlay-end ov))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_class_alloc_inherited_shared() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass base-shared ()
    ((shared-counter :allocation :class :initarg :sc :accessor bs-sc :initform 0)
     (shared-list :allocation :class :initarg :sl :accessor bs-sl :initform nil)))
  (defclass child-shared (base-shared)
    ((child-name :initarg :cn :accessor cs-cn :initform "")
     (child-val :initarg :cv :accessor cs-cv :initform 0)))
  (let* ((buf (generate-new-buffer "ca5"))
         (snaps nil)
         (base-obj (base-shared))
         (child-obj (child-shared :cn "child" :cv 100)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (setq-local my-cs-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "init" (bs-sc base-obj) (bs-sc child-obj)
                    (bs-sl base-obj) (bs-sl child-obj)) results)
        (setf (bs-sc base-obj) 1)
        (push (list "inc-base" (bs-sc base-obj) (bs-sc child-obj)) results)
        (setf (bs-sc child-obj) (1+ (bs-sc child-obj)))
        (push (list "inc-child" (bs-sc base-obj) (bs-sc child-obj)) results)
        (push 'base (bs-sl base-obj))
        (push 'child (bs-sl child-obj))
        (push (list "push-both" (bs-sl base-obj) (bs-sl child-obj)) results)
        (goto-char 8)
        (insert "XXX")
        (setf (cs-cv child-obj) (+ (cs-cv child-obj) (marker-position m)))
        (setq my-cs-log (cons "ins@8" my-cs-log))
        (push (list "edit" (bs-sc base-obj) (bs-sc child-obj)
                    (cs-cv child-obj) (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S cs-log=%S"
                       results (reverse my-cs-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'cs-log t)
        (list (buffer-string)
              (bs-sc base-obj) (bs-sc child-obj)
              (bs-sl base-obj) (bs-sl child-obj)
              (cs-cv child-obj)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-cs-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

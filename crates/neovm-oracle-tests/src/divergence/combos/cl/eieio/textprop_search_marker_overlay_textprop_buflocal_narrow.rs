//! Combo: cl-eieio text property search + markers + overlays + textprop + buflocal + narrow + undo.
//! Tests next-property-change / next-single-property-change with EIEIO objects as property values.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_textprop_next_change_with_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass annotation ()
    ((tag :initarg :tag :accessor ann-tag :initform "")
     (level :initarg :level :accessor ann-level :initform 0)))
  (let* ((buf (generate-new-buffer "tp1"))
         (a1 (annotation :tag "alpha" :level 1))
         (a2 (annotation :tag "beta" :level 2))
         (a3 (annotation :tag "gamma" :level 3)))
    (with-current-buffer buf
      (insert "AA--BB--CC--DD--EE")
      (put-text-property 1 3 'ann a1)
      (put-text-property 5 7 'ann a2)
      (put-text-property 9 11 'ann a3)
      (put-text-property 13 15 'ann a1)
      (put-text-property 17 19 'ann a2)
      (setq-local annots (list a1 a2 a3))
      (let* ((ov (make-overlay 5 15))
             (_ (overlay-put ov 'priority 2))
             (m (make-marker))
             (_ (set-marker m 5))
             (positions nil)
             (pos 1))
        (undo-boundary)
        (while (< pos (point-max))
          (let ((next (next-property-change pos (current-buffer))))
            (when next
              (let ((val (get-text-property pos 'ann)))
                (push (list pos next
                           (if val (list (ann-tag val) (ann-level val)) nil))
                      positions))
              (setq pos next))
            (unless next (setq pos (point-max)))))
        (setq positions (reverse positions))
        (goto-char (point-max))
        (insert (format " | scan=%s" positions))
        (setf (marker-position m) 3)
        (put-text-property (1- (point-max)) (point-max) 'scan-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                annots))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_textprop_single_change_overlay_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass styled-chunk ()
    ((style-name :initarg :style-name :accessor sc-name :initform "")
     (priority :initarg :priority :accessor sc-priority :initform 0)
     (bg :initarg :bg :accessor sc-bg :initform nil)))
  (let* ((buf (generate-new-buffer "tp2"))
         (s1 (styled-chunk :style-name "heading" :priority 3 :bg 'red))
         (s2 (styled-chunk :style-name "body" :priority 1 :bg 'green))
         (s3 (styled-chunk :style-name "quote" :priority 2 :bg 'blue)))
    (with-current-buffer buf
      (insert "HHHH-BBBB-QQQQ-HHHH-BBBB")
      (put-text-property 1 5 'style s1)
      (put-text-property 6 10 'style s2)
      (put-text-property 11 15 'style s3)
      (put-text-property 16 20 'style s1)
      (put-text-property 21 25 'style s2)
      (setq-local styles (list s1 s2 s3))
      (let* ((ov1 (make-overlay 1 10))
             (ov2 (make-overlay 11 20))
             (_ (overlay-put ov1 'priority (sc-priority s1)))
             (_ (overlay-put ov2 'priority (sc-priority s3)))
             (m (make-marker))
             (_ (set-marker m 11))
             (boundaries nil)
             (pos 1))
        (undo-boundary)
        (while (< pos (point-max))
          (let ((next (next-single-property-change pos 'style (current-buffer))))
            (when next
              (let ((val (get-text-property pos 'style)))
                (push (list pos next
                           (if val (list (sc-name val) (sc-bg val)) nil))
                      boundaries))
              (setq pos next))
            (unless next (setq pos (point-max)))))
        (setq boundaries (reverse boundaries))
        (save-excursion
          (save-restriction
            (narrow-to-region 6 20)
            (let ((narrow-boundaries nil)
                  (pos (point-min)))
              (while (< pos (point-max))
                (let ((next (next-single-property-change pos 'style (current-buffer) (point-max))))
                  (when next
                    (let ((val (get-text-property pos 'style)))
                      (push (list pos next
                                 (if val (sc-name val) nil))
                            narrow-boundaries))
                    (setq pos next))
                  (unless next (setq pos (point-max)))))
              (setq narrow-boundaries (reverse narrow-boundaries))
              (goto-char (point-max))
              (insert (format "NARROW:%s" narrow-boundaries)))))
        (goto-char (point-max))
        (insert (format " | bounds=%s" boundaries))
        (setf (marker-position m) 8)
        (put-text-property (1- (point-max)) (point-max) 'bound-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov1))
              (oe (overlay-end ov2))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                styles))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_textprop_remove_add_overlay_clash() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass token ()
    ((kind :initarg :kind :accessor tk-kind :initform "")
     (value :initarg :value :accessor tk-value :initform 0)))
  (let* ((buf (generate-new-buffer "tp3"))
         (t1 (token :kind "keyword" :value 100))
         (t2 (token :kind "ident" :value 200))
         (t3 (token :kind "number" :value 300)))
    (with-current-buffer buf
      (insert "KW-ID-NUM-KW-ID-NUM")
      (put-text-property 1 3 'tok t1)
      (put-text-property 4 6 'tok t2)
      (put-text-property 7 10 'tok t3)
      (put-text-property 11 13 'tok t1)
      (put-text-property 14 16 'tok t2)
      (put-text-property 17 20 'tok t3)
      (setq-local tokens (list t1 t2 t3))
      (let* ((ov (make-overlay 4 16))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 7))
             (results nil))
        (undo-boundary)
        (remove-text-properties 7 10 '(tok nil))
        (push (list 'after-remove
                   (get-text-property 7 'tok)
                   (get-text-property 11 'tok))
              results)
        (put-text-property 7 10 'tok t2)
        (push (list 'after-replace
                   (get-text-property 7 'tok)
                   (if (get-text-property 7 'tok) (tk-kind (get-text-property 7 'tok)) nil))
              results)
        (save-excursion
          (save-restriction
            (narrow-to-region 4 13)
            (let ((all-toks nil)
                  (pos (point-min)))
              (while (< pos (point-max))
                (let ((val (get-text-property pos 'tok)))
                  (push (if val (tk-kind val) nil) all-toks)
                  (setq pos (or (next-single-property-change pos 'tok (current-buffer) (point-max))
                                (point-max)))))
              (push (list 'narrowed-toks (reverse all-toks)) results))))
        (let ((all-toks nil)
              (pos 1))
          (while (< pos (point-max))
            (let ((val (get-text-property pos 'tok)))
              (push (if val (list (tk-kind val) (tk-value val)) nil) all-toks)
              (setq pos (or (next-single-property-change pos 'tok (current-buffer) (point-max))
                            (point-max)))))
          (push (list 'full-toks (reverse all-toks)) results))
        (goto-char (point-max))
        (insert (format " | results=%s" (reverse results)))
        (setf (marker-position m) 12)
        (put-text-property (1- (point-max)) (point-max) 'tok-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                tokens))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_textprop_object_identity_eq_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ref ()
    ((label :initarg :label :accessor ref-label :initform "")
     (count :initarg :count :accessor ref-count :initform 0)))
  (let* ((buf (generate-new-buffer "tp4"))
         (r1 (ref :label "shared-a" :count 1))
         (r2 (ref :label "shared-b" :count 2))
         (r3 (ref :label "unique" :count 3)))
    (with-current-buffer buf
      (insert "AA-BB-CC-DD-EE-FF-GG-HH")
      (put-text-property 1 3 'ref r1)
      (put-text-property 4 6 'ref r1)
      (put-text-property 7 9 'ref r2)
      (put-text-property 10 12 'ref r2)
      (put-text-property 13 15 'ref r3)
      (put-text-property 16 18 'ref r1)
      (put-text-property 19 21 'ref r2)
      (put-text-property 22 24 'ref r3)
      (setq-local refs (list r1 r2 r3))
      (let* ((ov (make-overlay 7 18))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 10))
             (eq-checks nil)
             (seen-refs (make-hash-table :test 'eq)))
        (undo-boundary)
        (let ((pos 1))
          (while (< pos (point-max))
            (let* ((val (get-text-property pos 'ref))
                   (next (next-single-property-change pos 'ref (current-buffer) (point-max))))
              (when val
                (let ((existing (gethash val seen-refs)))
                  (push (list pos (ref-label val)
                             (if existing 'seen-before 'first-encounter))
                        eq-checks)
                  (puthash val (1+ (or existing 0)) seen-refs)))
              (setq pos (or next (point-max))))))
        (setq eq-checks (reverse eq-checks))
        (let ((counts (list (ref-count r1) (ref-count r2) (ref-count r3))))
          (goto-char (point-max))
          (insert (format " | eq=%s counts=%s" eq-checks counts))
          (setf (marker-position m) 6)
          (put-text-property (1- (point-max)) (point-max) 'eq-log t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                refs))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_textprop_add_transitive_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable chain-weight)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass link ()
    ((from :initarg :from :accessor lk-from :initform "")
     (to :initarg :to :accessor lk-to :initform "")
     (weight :initarg :weight :accessor lk-weight :initform 1.0)))
  (let* ((buf (generate-new-buffer "tp5"))
         (l1 (link :from "A" :to "B" :weight 0.5))
         (l2 (link :from "B" :to "C" :weight 1.5))
         (l3 (link :from "C" :to "A" :weight 2.5)))
    (with-current-buffer buf
      (insert "AB-BC-CA-AB-BC-CA")
      (put-text-property 1 3 'link l1)
      (put-text-property 4 6 'link l2)
      (put-text-property 7 9 'link l3)
      (put-text-property 10 12 'link l1)
      (put-text-property 13 15 'link l2)
      (put-text-property 16 18 'link l3)
      (setq-local links (list l1 l2 l3))
      (let* ((ov1 (make-overlay 1 9))
             (ov2 (make-overlay 10 18))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2))
             (m (make-marker))
             (_ (set-marker m 4))
             (chain nil))
        (undo-boundary)
        (let* ((pos 1)
               (current-link (get-text-property pos 'link))
               (chain-weight (lk-weight current-link)))
          (push (lk-from current-link) chain)
          (push (lk-to current-link) chain)
          (while (< pos (point-max))
            (let* ((val (get-text-property pos 'link))
                   (next (next-single-property-change pos 'link (current-buffer) (point-max))))
              (when (and val (not (eq val current-link)))
                (if (eq (lk-from val) (lk-to current-link))
                    (progn
                      (setq chain-weight (+ chain-weight (lk-weight val)))
                      (push (lk-to val) chain))
                  (setq chain-weight (lk-weight val))
                  (setq chain (list (lk-to val) (lk-from val))))
                (setq current-link val))
              (setq pos (or next (point-max))))))
          (goto-char (point-max))
          (insert (format " | chain=%s weight=%.1f" (reverse chain) chain-weight))
          (setf (marker-position m) 7)
          (put-text-property (1- (point-max)) (point-max) 'chain-log t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os1 (overlay-start ov1))
              (oe2 (overlay-end ov2))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os1 oe2 bs
                (marker-position m)
                (buffer-string)
                links))))
    (kill-buffer buf))"#,
        expect,
    );
}

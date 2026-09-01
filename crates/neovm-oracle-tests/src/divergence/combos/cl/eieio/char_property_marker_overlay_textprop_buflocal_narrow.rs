//! Combo: cl-eieio char-property overlay priority + markers + overlays + textprop + buflocal + narrow + undo.
//! Tests get-char-property vs get-text-property with overlay/textprop priority and EIEIO objects.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_char_prop_vs_text_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass style ()
    ((name :initarg :name :accessor st-name :initform "")
     (source :initarg :source :accessor st-source :initform "")))
  (let* ((buf (generate-new-buffer "cp1"))
         (text-style (style :name "text-blue" :source "text-property"))
         (ov-style (style :name "ov-red" :source "overlay")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 13 'color text-style)
      (setq-local my-styles (list text-style ov-style))
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'color ov-style))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 6))
             (results nil))
        (undo-boundary)
        (push (list 'pos1-text (st-name (get-text-property 1 'color))) results)
        (push (list 'pos1-char (st-name (get-char-property 1 'color))) results)
        (push (list 'pos6-text (st-name (get-text-property 6 'color))) results)
        (push (list 'pos6-char (st-name (get-char-property 6 'color))) results)
        (push (list 'pos11-text (st-name (get-text-property 11 'color))) results)
        (push (list 'pos11-char (st-name (get-char-property 11 'color))) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d" results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'cp-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-styles))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_char_prop_overlay_priority_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass priority-val ()
    ((level :initarg :level :accessor pv-level :initform 0)
     (label :initarg :label :accessor pv-label :initform "")))
  (let* ((buf (generate-new-buffer "cp2"))
         (pv1 (priority-val :level 1 :label "low"))
         (pv2 (priority-val :level 2 :label "medium"))
         (pv3 (priority-val :level 3 :label "high")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 13 'val pv1)
      (setq-local my-pvs (list pv1 pv2 pv3))
      (let* ((ov1 (make-overlay 1 13))
             (ov2 (make-overlay 1 13))
             (ov3 (make-overlay 1 13))
             (_ (overlay-put ov1 'val pv1))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'val pv2))
             (_ (overlay-put ov2 'priority 2))
             (_ (overlay-put ov3 'val pv3))
             (_ (overlay-put ov3 'priority 3))
             (m (make-marker))
             (_ (set-marker m 1))
             (results nil))
        (undo-boundary)
        (push (list 'text-prop (pv-label (get-text-property 1 'val))) results)
        (push (list 'char-prop (pv-label (get-char-property 1 'val))) results)
        (overlay-put ov3 'priority 0)
        (push (list 'after-priority-change (pv-label (get-char-property 1 'val))) results)
        (overlay-put ov2 'val nil)
        (push (list 'after-ov2-nil (pv-label (get-char-property 1 'val))) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d" results (marker-position m)))
        (set-marker m 5)
        (put-text-property (1- (point-max)) (point-max) 'pv-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os1 (overlay-start ov1))
              (oe3 (overlay-end ov3))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os1 oe3 bs
                (marker-position m)
                (buffer-string)
                my-pvs))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_char_prop_narrow_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass zone-val ()
    ((zone-name :initarg :zone-name :accessor zv-name :initform "")
     (value :initarg :value :accessor zv-value :initform 0)))
  (let* ((buf (generate-new-buffer "cp3"))
         (zv1 (zone-val :zone-name "inside" :value 10))
         (zv2 (zone-val :zone-name "outside" :value 20)))
    (with-current-buffer buf
      (insert "IIII-OOOO-IIII")
      (put-text-property 1 5 'zone zv1)
      (put-text-property 6 9 'zone zv2)
      (put-text-property 10 13 'zone zv1)
      (setq-local my-zvs (list zv1 zv2))
      (let* ((ov (make-overlay 6 9))
             (_ (overlay-put ov 'zone zv2))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 6))
             (results nil))
        (undo-boundary)
        (save-restriction
          (narrow-to-region 1 5)
          (push (list 'narrow-text (zv-name (get-text-property 1 'zone))) results)
          (push (list 'narrow-char (zv-name (get-char-property 1 'zone))) results))
        (push (list 'wide-pos1-text (zv-name (get-text-property 1 'zone))) results)
        (push (list 'wide-pos1-char (zv-name (get-char-property 1 'zone))) results)
        (push (list 'wide-pos6-text (zv-name (get-text-property 6 'zone))) results)
        (push (list 'wide-pos6-char (zv-name (get-char-property 6 'zone))) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d" results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'zone-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-zvs))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_char_prop_multiple_overlays_same_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass tag ()
    ((id :initarg :id :accessor tg-id :initform 0)
     (weight :initarg :weight :accessor tg-weight :initform 0)))
  (let* ((buf (generate-new-buffer "cp4"))
         (t1 (tag :id 1 :weight 1))
         (t2 (tag :id 2 :weight 2))
         (t3 (tag :id 3 :weight 3))
         (t4 (tag :id 4 :weight 0)))
    (with-current-buffer buf
      (insert "XXXXXXXXXXXX")
      (put-text-property 1 13 'tag t4)
      (setq-local my-tags (list t1 t2 t3 t4))
      (let* ((ov1 (make-overlay 1 7))
             (ov2 (make-overlay 4 10))
             (ov3 (make-overlay 7 13))
             (_ (overlay-put ov1 'tag t1))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'tag t2))
             (_ (overlay-put ov2 'priority 2))
             (_ (overlay-put ov3 'tag t3))
             (_ (overlay-put ov3 'priority 3))
             (m (make-marker))
             (_ (set-marker m 4))
             (scan nil))
        (undo-boundary)
        (let ((pos 1))
          (while (< pos (point-max))
            (let* ((text-val (get-text-property pos 'tag))
                   (char-val (get-char-property pos 'tag))
                   (next-text (next-single-property-change pos 'tag (current-buffer) (point-max)))
                   (next-char (next-char-property-change pos (point-max))))
              (push (list pos
                         (if text-val (tg-id text-val) nil)
                         (if char-val (tg-id char-val) nil)
                         next-text next-char)
                    scan)
              (setq pos (max (1+ pos) (or next-char (point-max)))))))
        (setq scan (reverse scan))
        (goto-char (point-max))
        (insert (format " | scan=%s m=%d" scan (marker-position m)))
        (set-marker m 5)
        (put-text-property (1- (point-max)) (point-max) 'tag-scan t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os1 (overlay-start ov1))
              (oe3 (overlay-end ov3))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os1 oe3 bs
                (marker-position m)
                (buffer-string)
                my-tags))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_char_prop_eq_identity_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass identity-tag ()
    ((label :initarg :label :accessor it-label :initform "")))
  (let* ((buf (generate-new-buffer "cp5"))
         (it1 (identity-tag :label "shared"))
         (it2 (identity-tag :label "overlay-only")))
    (with-current-buffer buf
      (insert "AAAAAAAAAAAA")
      (put-text-property 1 13 'tag it1)
      (setq-local my-tags (list it1 it2))
      (let* ((ov (make-overlay 7 13))
             (_ (overlay-put ov 'tag it2))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 1))
             (results nil))
        (undo-boundary)
        (let ((text-1 (get-text-property 1 'tag))
              (char-1 (get-char-property 1 'tag))
              (text-7 (get-text-property 7 'tag))
              (char-7 (get-char-property 7 'tag)))
          (push (list 'pos1-text-eq-it1 (eq text-1 it1)) results)
          (push (list 'pos1-char-eq-it1 (eq char-1 it1)) results)
          (push (list 'pos7-text-eq-it1 (eq text-7 it1)) results)
          (push (list 'pos7-char-eq-it2 (eq char-7 it2)) results)
          (push (list 'pos7-text-label (it-label text-7)) results)
          (push (list 'pos7-char-label (it-label char-7)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d" results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'id-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-tags))))
    (kill-buffer buf)))"#,
        expect,
    );
}

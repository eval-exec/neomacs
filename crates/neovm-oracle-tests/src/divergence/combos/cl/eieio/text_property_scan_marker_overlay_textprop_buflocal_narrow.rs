//! Combo: cl-eieio text-property scan + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests text-property-any, text-property-not-all, next-property-change scanning with EIEIO.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_text_property_any_scan() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass prop-scan-result ()
    ((prop-name :initarg :prop :accessor psr-prop :initform "")
     (found-at :initarg :found-at :accessor psr-at :initform 0)
     (value :initarg :value :accessor psr-val :initform nil)))
  (let* ((buf (generate-new-buffer "tp1"))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (setq-local my-results results)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (_ (overlay-put ov 'zone 'overlay))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (push (prop-scan-result :prop "zone-b"
                               :found-at (text-property-any 1 20 'zone 'b)
                               :value (get-text-property 7 'zone)) results)
        (push (prop-scan-result :prop "zone-overlay"
                               :found-at (text-property-any 1 20 'zone 'overlay)
                               :value (get-char-property 7 'zone)) results)
        (push (prop-scan-result :prop "zone-missing"
                               :found-at (text-property-any 1 20 'zone 'missing)
                               :value nil) results)
        (goto-char 3)
        (insert "XX")
        (push (prop-scan-result :prop "zone-b-after"
                               :found-at (text-property-any 1 22 'zone 'b)
                               :value (get-text-property 9 'zone)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       (mapcar (lambda (r) (list (psr-prop r) (psr-at r) (psr-val r))) results)
                       (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'psr-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length results)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_text_property_not_all_scan() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass notall-result ()
    ((range :initarg :range :accessor nar-range :initform nil)
     (prop-name :initarg :prop :accessor nar-prop :initform "")
     (first-diff :initarg :first-diff :accessor nar-diff :initform 0)))
  (let* ((buf (generate-new-buffer "tp2"))
         (results nil))
    (with-current-buffer buf
      (insert "AAAAAAAAAAAAAAAAAAAA")
      (put-text-property 1 10 'face 'bold)
      (put-text-property 11 20 'face 'italic)
      (setq-local my-results results)
      (let* ((ov (make-overlay 8 14))
             (_ (overlay-put ov 'priority 1))
             (_ (overlay-put ov 'face 'shadow))
             (m (make-marker))
             (_ (set-marker m 5)))
        (undo-boundary)
        (push (notall-result :range '(1 . 20)
                            :prop "bold"
                            :first-diff (text-property-not-all 1 20 'face 'bold)) results)
        (push (notall-result :range '(1 . 10)
                            :prop "bold"
                            :first-diff (text-property-not-all 1 10 'face 'bold)) results)
        (push (notall-result :range '(11 . 20)
                            :prop "italic"
                            :first-diff (text-property-not-all 11 20 'face 'italic)) results)
        (goto-char 5)
        (insert "XX")
        (push (notall-result :range '(1 . 22)
                            :prop "bold"
                            :first-diff (text-property-not-all 1 22 'face 'bold)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       (mapcar (lambda (r) (list (nar-range r) (nar-prop r) (nar-diff r))) results)
                       (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'nar-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length results)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_next_property_change_scan() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass change-point ()
    ((from :initarg :from :accessor cp-from :initform 0)
     (to :initarg :to :accessor cp-to :initform 0)
     (prop-at-from :initarg :prop :accessor cp-prop :initform nil)))
  (let* ((buf (generate-new-buffer "tp3"))
         (changes nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (setq-local my-changes changes)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 3))
             (results nil))
        (undo-boundary)
        (let ((pos 1))
          (while (< pos (point-max))
            (let ((next (or (next-single-property-change pos 'zone) (point-max))))
              (push (change-point :from pos :to next
                                 :prop (get-text-property pos 'zone)) changes)
              (setq pos next))))
        (setq changes (reverse changes))
        (setq results (mapcar (lambda (c) (list (cp-from c) (cp-to c) (cp-prop c))) changes))
        (goto-char 3)
        (insert "XX")
        (push (list 'after-insert
                   (next-single-property-change 1 'zone)
                   (get-text-property 3 'zone)
                   (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S changes=%d m=%d"
                       results (length changes) (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'cp-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length changes)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_property_scan_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass narrow-scan-result ()
    ((narrow-bounds :initarg :narrow :accessor nsr-bounds :initform nil)
     (found-b :initarg :found-b :accessor nsr-foundb :initform 0)
     (notall-b :initarg :notall-b :accessor nsr-notallb :initform 0)
     (changes-count :initarg :changes :accessor nsr-changes :initform 0)))
  (let* ((buf (generate-new-buffer "tp4"))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (setq-local my-results results)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (push (narrow-scan-result :narrow (list (point-min) (point-max))
                                 :found-b (text-property-any 1 25 'zone 'b)
                                 :notall-b (text-property-not-all 1 25 'zone 'a)
                                 :changes (let ((cnt 0) (pos 1))
                                           (while (setq pos (next-single-property-change pos 'zone))
                                             (setq cnt (1+ cnt)))
                                           cnt)) results)
        (save-restriction
          (narrow-to-region 6 15)
          (push (narrow-scan-result :narrow (list (point-min) (point-max))
                                   :found-b (text-property-any (point-min) (point-max) 'zone 'b)
                                   :notall-b (text-property-not-all (point-min) (point-max) 'zone 'b)
                                   :changes (let ((cnt 0) (pos (point-min)))
                                             (while (and (setq pos (next-single-property-change pos 'zone))
                                                        (< pos (point-max)))
                                               (setq cnt (1+ cnt)))
                                             cnt)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       (mapcar (lambda (r) (list (nsr-bounds r) (nsr-foundb r) (nsr-changes r))) results)
                       (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'nsr-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length results)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_property_scan_undo_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass edit-scan-snap ()
    ((step :initarg :step :accessor ess-step :initform "")
     (zone-a-end :initarg :a-end :accessor ess-aend :initform 0)
     (zone-b-start :initarg :b-start :accessor ess-bstart :initform 0)
     (buf-string :initarg :buf-string :accessor ess-bs :initform "")))
  (let* ((buf (generate-new-buffer "tp5"))
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
             (_ (set-marker m 5))
             (results nil))
        (undo-boundary)
        (push (edit-scan-snap :step "init"
                             :a-end (or (next-single-property-change 1 'zone) 20)
                             :b-start (text-property-any 1 20 'zone 'b)
                             :buf-string (buffer-string)) snaps)
        (goto-char 3)
        (insert "XX")
        (push (edit-scan-snap :step "after-insert"
                             :a-end (or (next-single-property-change 1 'zone) 22)
                             :b-start (text-property-any 1 22 'zone 'b)
                             :buf-string (buffer-string)) snaps)
        (put-text-property 1 7 'zone 'merged)
        (push (edit-scan-snap :step "after-merge"
                             :a-end (or (next-single-property-change 1 'zone) 22)
                             :b-start (text-property-any 1 22 'zone 'b)
                             :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ess-step s) (ess-aend s) (ess-bstart s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ess-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (get-text-property 3 'zone)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

//! Combo: cl-eieio truncate-lines/word-wrap + overlays + markers + textprop + buflocal + narrow.
//! Tests line display settings with EIEIO objects, markers, and editing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_truncate_lines_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass truncate-snap ()
    ((step :initarg :step :accessor trs-step :initform "")
     (truncate :initarg :truncate :accessor trs-trunc :initform nil)
     (line-count :initarg :lines :accessor trs-lines :initform 0)
     (buf-string :initarg :buf-string :accessor trs-bs :initform "")))
  (let* ((buf (generate-new-buffer "tw1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAAAAAAAAAAAAAAAAAA\nBBBBBBBBBBBBBBBBBBBB\nCCCCCCCCCCCCCCCCCCCC")
      (put-text-property 1 21 'zone 'a)
      (put-text-property 22 42 'zone 'b)
      (put-text-property 43 63 'zone 'c)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 10 35))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 15))
             (results nil))
        (undo-boundary)
        (push (truncate-snap :step "init"
                            :truncate truncate-lines
                            :lines (count-lines (point-min) (point-max))
                            :buf-string (buffer-string)) snaps)
        (setq-local truncate-lines t)
        (push (truncate-snap :step "truncate-on"
                            :truncate truncate-lines
                            :lines (count-lines (point-min) (point-max))
                            :buf-string (buffer-string)) snaps)
        (setq-local truncate-lines nil)
        (push (truncate-snap :step "truncate-off"
                            :truncate truncate-lines
                            :lines (count-lines (point-min) (point-max))
                            :buf-string (buffer-string)) snaps)
        (goto-char 5)
        (insert "XX")
        (push (truncate-snap :step "after-edit"
                            :truncate truncate-lines
                            :lines (count-lines (point-min) (point-max))
                            :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (trs-step s) (trs-trunc s) (trs-lines s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'trs-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                truncate-lines))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_word_wrap_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass wrap-snap ()
    ((step :initarg :step :accessor ws-step :initform "")
     (word-wrap :initarg :word-wrap :accessor ws-wrap :initform nil)
     (truncate :initarg :truncate :accessor ws-trunc :initform nil)
     (prop-at-5 :initarg :prop :accessor ws-prop :initform nil)
     (buf-string :initarg :buf-string :accessor ws-bs :initform "")))
  (let* ((buf (generate-new-buffer "tw2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
      (put-text-property 1 20 'face 'bold)
      (put-text-property 21 40 'face 'italic)
      (put-text-property 41 53 'face 'underline)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 15 40))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 25))
             (results nil))
        (undo-boundary)
        (push (wrap-snap :step "init"
                        :word-wrap word-wrap
                        :truncate truncate-lines
                        :prop (get-text-property 5 'face)
                        :buf-string (buffer-string)) snaps)
        (setq-local word-wrap t)
        (push (wrap-snap :step "wrap-on"
                        :word-wrap word-wrap
                        :truncate truncate-lines
                        :prop (get-text-property 5 'face)
                        :buf-string (buffer-string)) snaps)
        (setq-local truncate-lines t)
        (setq-local word-wrap nil)
        (push (wrap-snap :step "truncate-on"
                        :word-wrap word-wrap
                        :truncate truncate-lines
                        :prop (get-text-property 5 'face)
                        :buf-string (buffer-string)) snaps)
        (goto-char 10)
        (insert "MMMM")
        (push (wrap-snap :step "after-insert"
                        :word-wrap word-wrap
                        :truncate truncate-lines
                        :prop (get-text-property 10 'face)
                        :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ws-step s) (ws-wrap s) (ws-trunc s) (ws-prop s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ws-log t)
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
fn combo_eieio_truncate_narrow_line_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass trunc-narrow-snap ()
    ((step :initarg :step :accessor tns-step :initform "")
     (narrow-bounds :initarg :narrow :accessor tns-narrow :initform nil)
     (line-count :initarg :lines :accessor tns-lines :initform 0)
     (truncate :initarg :truncate :accessor tns-trunc :initform nil)))
  (let* ((buf (generate-new-buffer "tw3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC\nDDDD-EEEE-FFFF\nGGGG-HHHH-IIII\nJJJJ-KKKK-LLLL")
      (put-text-property 1 14 'zone 'a)
      (put-text-property 15 28 'zone 'b)
      (put-text-property 29 42 'zone 'c)
      (put-text-property 43 56 'zone 'd)
      (setq-local my-snaps snaps
                  truncate-lines t)
      (let* ((ov (make-overlay 15 42))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 20))
             (results nil))
        (undo-boundary)
        (push (trunc-narrow-snap :step "init"
                                :narrow (list (point-min) (point-max))
                                :lines (count-lines (point-min) (point-max))
                                :truncate truncate-lines) snaps)
        (save-restriction
          (narrow-to-region 15 42)
          (push (trunc-narrow-snap :step "narrow"
                                  :narrow (list (point-min) (point-max))
                                  :lines (count-lines (point-min) (point-max))
                                  :truncate truncate-lines) snaps)
          (goto-char 5)
          (insert "XX"))
        (push (trunc-narrow-snap :step "after-widen"
                                :narrow (list (point-min) (point-max))
                                :lines (count-lines (point-min) (point-max))
                                :truncate truncate-lines) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (tns-step s) (tns-lines s) (tns-trunc s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'tns-log t)
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
fn combo_eieio_wrap_overlay_invisible() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function overlay-live-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass wrap-invis-snap ()
    ((step :initarg :step :accessor wis-step :initform "")
     (buf-string :initarg :buf-string :accessor wis-bs :initform "")
     (visible-len :initarg :visible-len :accessor wis-vlen :initform 0)
     (m-pos :initarg :m-pos :accessor wis-mp :initform 0)))
  (let* ((buf (generate-new-buffer "tw4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAAAAAAAAAA-BBBBBBBBBBBB-CCCCCCCCCCCC")
      (put-text-property 1 13 'zone 'a)
      (put-text-property 14 26 'zone 'b)
      (put-text-property 27 39 'zone 'c)
      (setq-local my-snaps snaps
                  word-wrap t)
      (let* ((ov (make-overlay 14 26))
             (_ (overlay-put ov 'invisible t))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 14))
             (results nil))
        (undo-boundary)
        (push (wrap-invis-snap :step "init"
                              :buf-string (buffer-string)
                              :visible-len (length (buffer-substring-no-properties 1 39))
                              :m-pos (marker-position m)) snaps)
        (setq-local truncate-lines t)
        (setq-local word-wrap nil)
        (push (wrap-invis-snap :step "truncate"
                              :buf-string (buffer-string)
                              :visible-len (length (buffer-substring-no-properties 1 39))
                              :m-pos (marker-position m)) snaps)
        (goto-char 5)
        (insert "XX")
        (push (wrap-invis-snap :step "after-edit"
                              :buf-string (buffer-string)
                              :visible-len (length (buffer-substring-no-properties 1 41))
                              :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (wis-step s) (wis-vlen s) (wis-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d ov-alive=%s"
                       results (marker-position m) (overlay-live-p ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'wis-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                truncate-lines word-wrap))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_wrap_undo_marker_integrity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass wrap-undo-snap ()
    ((step :initarg :step :accessor wus-step :initform "")
     (m1-pos :initarg :m1 :accessor wus-m1 :initform 0)
     (m2-pos :initarg :m2 :accessor wus-m2 :initform 0)
     (buf-string :initarg :buf-string :accessor wus-bs :initform "")))
  (let* ((buf (generate-new-buffer "tw5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAAAAAAAAAAAAAAAAAAAAAAAAAA")
      (put-text-property 1 10 'face 'bold)
      (put-text-property 11 20 'face 'italic)
      (put-text-property 21 29 'face 'underline)
      (setq-local my-snaps snaps
                  truncate-lines t)
      (let* ((ov (make-overlay 10 20))
             (_ (overlay-put ov 'priority 1))
             (m1 (make-marker))
             (m2 (make-marker))
             (_ (set-marker m1 10))
             (_ (set-marker m2 20))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (wrap-undo-snap :step "init"
                             :m1 (marker-position m1)
                             :m2 (marker-position m2)
                             :buf-string (buffer-string)) snaps)
        (setq-local word-wrap t)
        (setq-local truncate-lines nil)
        (goto-char 5)
        (insert "XXXXX")
        (undo-boundary)
        (push (wrap-undo-snap :step "after-insert"
                             :m1 (marker-position m1)
                             :m2 (marker-position m2)
                             :buf-string (buffer-string)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (wrap-undo-snap :step "after-undo"
                             :m1 (marker-position m1)
                             :m2 (marker-position m2)
                             :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (wus-step s) (wus-m1 s) (wus-m2 s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m1=%d m2=%d"
                       results (marker-position m1) (marker-position m2)))
        (put-text-property (1- (point-max)) (point-max) 'wus-log t)
        (list (buffer-string)
              (length snaps)
              (marker-position m1) (marker-position m2)
              (overlay-start ov) (overlay-end ov))))
    (kill-buffer buf)))"#,
        expect,
    );
}

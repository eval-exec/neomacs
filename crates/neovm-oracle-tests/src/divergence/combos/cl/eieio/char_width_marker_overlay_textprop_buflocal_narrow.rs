//! Combo: cl-eieio char-width/string-width + overlays + markers + textprop + buflocal + narrow.
//! Tests character and string width calculations with EIEIO objects and editing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_char_width_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass width-snap ()
    ((step :initarg :step :accessor ws-step :initform "")
     (char-widths :initarg :widths :accessor ws-widths :initform nil)
     (string-width :initarg :str-width :accessor ws-sw :initform 0)))
  (let* ((buf (generate-new-buffer "cw1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA\tBBBB\tCCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 14 'zone 'c)
      (setq-local my-snaps snaps
                  tab-width 4)
      (let* ((ov (make-overlay 5 10))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 6))
             (results nil))
        (undo-boundary)
        (push (width-snap :step "init"
                         :widths (list (char-width ?A) (char-width ?\t) (char-width ?B))
                         :str-width (string-width (buffer-string))) snaps)
        (setq-local tab-width 8)
        (push (width-snap :step "tab8"
                         :widths (list (char-width ?A) (char-width ?\t) (char-width ?B))
                         :str-width (string-width (buffer-string))) snaps)
        (setq-local tab-width 2)
        (push (width-snap :step "tab2"
                         :widths (list (char-width ?A) (char-width ?\t) (char-width ?B))
                         :str-width (string-width (buffer-string))) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ws-step s) (ws-sw s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       (mapcar (lambda (s) (list (ws-step s) (ws-widths s) (ws-sw s))) snaps)
                       (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ws-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                tab-width))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_string_width_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass str-width-snap ()
    ((step :initarg :step :accessor sws-step :initform "")
     (line-wid :initarg :line-wid :accessor sws-lw :initform 0)
     (buf-len :initarg :buf-len :accessor sws-bl :initform 0)
     (m-pos :initarg :m-pos :accessor sws-mp :initform 0)))
  (let* ((buf (generate-new-buffer "cw2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAAAAAAAA\tBBBBBBBBBB\tCCCCCCCCCC")
      (put-text-property 1 10 'zone 'a)
      (put-text-property 11 20 'zone 'b)
      (put-text-property 21 30 'zone 'c)
      (setq-local my-snaps snaps
                  tab-width 4)
      (let* ((ov (make-overlay 11 20))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 15))
             (results nil))
        (undo-boundary)
        (push (str-width-snap :step "init"
                             :line-wid (string-width (buffer-substring (line-beginning-position) (line-end-position)))
                             :buf-len (buffer-size)
                             :m-pos (marker-position m)) snaps)
        (goto-char 5)
        (insert "XXXXX")
        (push (str-width-snap :step "after-insert"
                             :line-wid (string-width (buffer-substring (line-beginning-position) (line-end-position)))
                             :buf-len (buffer-size)
                             :m-pos (marker-position m)) snaps)
        (delete-region 5 10)
        (push (str-width-snap :step "after-delete"
                             :line-wid (string-width (buffer-substring (line-beginning-position) (line-end-position)))
                             :buf-len (buffer-size)
                             :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (sws-step s) (sws-lw s) (sws-bl s) (sws-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'sws-log t)
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
fn combo_eieio_char_width_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass narrow-width-snap ()
    ((step :initarg :step :accessor nws-step :initform "")
     (narrow-bounds :initarg :narrow :accessor nws-narrow :initform nil)
     (visible-width :initarg :vis-wid :accessor nws-vw :initform 0)
     (m-pos :initarg :m-pos :accessor nws-mp :initform 0)))
  (let* ((buf (generate-new-buffer "cw3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA\tBBBB\tCCCC\tDDDD\tEEEE")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (setq-local my-snaps snaps
                  tab-width 4)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (push (narrow-width-snap :step "init"
                                :narrow (list (point-min) (point-max))
                                :vis-wid (string-width (buffer-string))
                                :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 6 15)
          (push (narrow-width-snap :step "narrow"
                                  :narrow (list (point-min) (point-max))
                                  :vis-wid (string-width (buffer-string))
                                  :m-pos (marker-position m)) snaps))
        (push (narrow-width-snap :step "widen"
                                :narrow (list (point-min) (point-max))
                                :vis-wid (string-width (buffer-string))
                                :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (nws-step s) (nws-vw s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'nws-log t)
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
fn combo_eieio_char_width_overlay_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass display-width-snap ()
    ((step :initarg :step :accessor dws-step :initform "")
     (ov-start :initarg :ov-start :accessor dws-ovs :initform 0)
     (ov-end :initarg :ov-end :accessor dws-ove :initform 0)
     (buf-string :initarg :buf-string :accessor dws-bs :initform "")))
  (let* ((buf (generate-new-buffer "cw4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (setq-local my-snaps snaps
                  tab-width 4)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'priority 1))
             (_ (overlay-put ov 'display "XX"))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (push (display-width-snap :step "init"
                                 :ov-start (overlay-start ov)
                                 :ov-end (overlay-end ov)
                                 :buf-string (buffer-string)) snaps)
        (goto-char 3)
        (insert "MM")
        (push (display-width-snap :step "after-insert"
                                 :ov-start (overlay-start ov)
                                 :ov-end (overlay-end ov)
                                 :buf-string (buffer-string)) snaps)
        (overlay-put ov 'display "LONGER")
        (push (display-width-snap :step "after-display"
                                 :ov-start (overlay-start ov)
                                 :ov-end (overlay-end ov)
                                 :buf-string (buffer-string)) snaps)
        (delete-region 3 5)
        (push (display-width-snap :step "after-delete"
                                 :ov-start (overlay-start ov)
                                 :ov-end (overlay-end ov)
                                 :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (goto-char (point-max))
        (insert (format " | snaps=%S m=%d"
                       (mapcar (lambda (s) (list (dws-step s) (dws-ovs s) (dws-ove s))) snaps)
                       (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'dws-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (overlay-get ov 'display)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_char_width_undo_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass width-undo-snap ()
    ((step :initarg :step :accessor wus-step :initform "")
     (tab-width :initarg :tab :accessor wus-tab :initform 0)
     (buf-width :initarg :bw :accessor wus-bw :initform 0)
     (m-pos :initarg :m-pos :accessor wus-mp :initform 0)))
  (let* ((buf (generate-new-buffer "cw5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA\tBBBB\tCCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 14 'zone 'c)
      (setq-local my-snaps snaps
                  tab-width 8)
      (let* ((ov (make-overlay 5 10))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 6))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (width-undo-snap :step "init"
                              :tab tab-width
                              :bw (string-width (buffer-string))
                              :m-pos (marker-position m)) snaps)
        (goto-char 3)
        (insert "XX")
        (undo-boundary)
        (push (width-undo-snap :step "after-insert"
                              :tab tab-width
                              :bw (string-width (buffer-string))
                              :m-pos (marker-position m)) snaps)
        (setq-local tab-width 4)
        (push (width-undo-snap :step "tab-change"
                              :tab tab-width
                              :bw (string-width (buffer-string))
                              :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (width-undo-snap :step "after-undo"
                              :tab tab-width
                              :bw (string-width (buffer-string))
                              :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (wus-step s) (wus-tab s) (wus-bw s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (put-text-property (1- (point-max)) (point-max) 'wus-log t)
        (list (buffer-string)
              (length snaps)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              tab-width)))
    (kill-buffer buf)))"#,
        expect,
    );
}

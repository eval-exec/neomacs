//! Combo: cl-eieio buffer-substring / insert-buffer-substring + markers + overlays + textprop + buflocal + narrow + undo.
//! Tests buffer content extraction/insertion with text property preservation and EIEIO objects.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_buffer_substring_prop_transfer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass content-block ()
    ((id :initarg :id :accessor cb-id :initform 0)
     (source-range :initarg :source-range :accessor cb-range :initform nil)
     (prop-snapshot :initarg :prop-snapshot :accessor cb-props :initform nil)))
  (let* ((buf (generate-new-buffer "bs1"))
         (b1 (content-block :id 1))
         (b2 (content-block :id 2))
         (b3 (content-block :id 3)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'level 'high)
      (put-text-property 6 10 'level 'medium)
      (put-text-property 11 15 'level 'low)
      (put-text-property 16 20 'level 'medium)
      (put-text-property 21 25 'level 'high)
      (setq-local blocks (list b1 b2 b3))
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 2))
             (m (make-marker))
             (_ (set-marker m 6))
             (results nil))
        (undo-boundary)
        (setf (cb-range b1) '(1 . 6)
              (cb-props b1) (get-text-property 1 'level))
        (let ((sub1 (buffer-substring 1 6)))
          (push (list 'sub1 sub1 (get-text-property 1 'level)
                     (text-property-not-all 1 6 'level 'high)) results))
        (setf (cb-range b2) '(6 . 11)
              (cb-props b2) (get-text-property 6 'level))
        (let ((sub2 (buffer-substring 6 11)))
          (push (list 'sub2 sub2 (get-text-property 6 'level)) results))
        (setf (cb-range b3) '(11 . 16)
              (cb-props b3) (get-text-property 11 'level))
        (let ((sub3 (buffer-substring 11 16)))
          (push (list 'sub3 sub3 (get-text-property 11 'level)) results))
        (goto-char (point-max))
        (insert "--COPY--")
        (insert-buffer-substring (current-buffer) 1 11)
        (push (list 'inserted (buffer-string)
                   (get-text-property (+ 25 8 1) 'level)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s b1=%s b2=%s b3=%s m=%d"
                       results
                       (list (cb-range b1) (cb-props b1))
                       (list (cb-range b2) (cb-props b2))
                       (list (cb-range b3) (cb-props b3))
                       (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'copy-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                blocks))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_substring_narrow_propagate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass snippet ()
    ((tag :initarg :tag :accessor sn-tag :initform "")
     (text :initarg :text :accessor sn-text :initform "")
     (props :initarg :props :accessor sn-props :initform nil)))
  (let* ((buf (generate-new-buffer "bs2"))
         (s1 (snippet :tag "narrowed"))
         (s2 (snippet :tag "wide"))
         (s3 (snippet :tag "inserted")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'kind 'alpha)
      (put-text-property 6 10 'kind 'beta)
      (put-text-property 11 15 'kind 'gamma)
      (put-text-property 16 20 'kind 'delta)
      (put-text-property 21 25 'kind 'epsilon)
      (setq-local snippets (list s1 s2 s3))
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 6))
             (results nil))
        (undo-boundary)
        (save-restriction
          (narrow-to-region 6 15)
          (setf (sn-text s1) (buffer-substring (point-min) (+ (point-min) 5)))
          (setf (sn-props s1) (get-text-property (point-min) 'kind))
          (push (list 'narrowed-sub (sn-text s1) (sn-props s1)
                     (buffer-string)) results))
        (setf (sn-text s2) (buffer-substring 1 6))
        (setf (sn-props s2) (get-text-property 1 'kind))
        (push (list 'wide-sub (sn-text s2) (sn-props s2)) results)
        (goto-char (point-max))
        (insert "--")
        (save-excursion
          (save-restriction
            (narrow-to-region 6 15)
            (insert-buffer-substring (current-buffer) 1 6)))
        (setf (sn-text s3) (buffer-substring 27 32))
        (setf (sn-props s3) (get-text-property 27 'kind))
        (push (list 'inserted-narrow (sn-text s3) (sn-props s3)
                   (buffer-string)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s s1=%s s2=%s s3=%s"
                       results
                       (list (sn-text s1) (sn-props s1))
                       (list (sn-text s2) (sn-props s2))
                       (list (sn-text s3) (sn-props s3))))
        (set-marker m 4)
        (put-text-property (1- (point-max)) (point-max) 'snippet-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                snippets))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_substring_object_as_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass tagged-span ()
    ((label :initarg :label :accessor ts-label :initform "")
     (color :initarg :color :accessor ts-color :initform "")))
  (let* ((buf (generate-new-buffer "bs3"))
         (t1 (tagged-span :label "red" :color "red"))
         (t2 (tagged-span :label "green" :color "green"))
         (t3 (tagged-span :label "blue" :color "blue")))
    (with-current-buffer buf
      (insert "RRRR-GGGG-BBBB-RRRR-GGGG")
      (put-text-property 1 5 'tag t1)
      (put-text-property 6 10 'tag t2)
      (put-text-property 11 15 'tag t3)
      (put-text-property 16 20 'tag t1)
      (put-text-property 21 25 'tag t2)
      (setq-local spans (list t1 t2 t3))
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'priority 2))
             (m (make-marker))
             (_ (set-marker m 11))
             (extracted nil))
        (undo-boundary)
        (let ((sub1 (buffer-substring 1 6)))
          (push (list 'sub1 sub1
                     (let ((p (get-text-property 0 'tag sub1)))
                       (if p (ts-label p) nil))) extracted))
        (let ((sub2 (buffer-substring 6 16)))
          (push (list 'sub2 sub2
                     (let ((p (get-text-property 0 'tag sub2)))
                       (if p (ts-label p) nil))
                     (let ((p (get-text-property 5 'tag sub2)))
                       (if p (ts-label p) nil))) extracted))
        (goto-char (point-max))
        (insert "--")
        (insert-buffer-substring (current-buffer) 1 16)
        (push (list 'after-insert (buffer-string)
                   (let ((p (get-text-property 28 'tag)))
                     (if p (ts-label p) nil))) extracted)
        (setq extracted (reverse extracted))
        (goto-char (point-max))
        (insert (format " | extracted=%s m=%d ov=[%d,%d]"
                       extracted (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 7)
        (put-text-property (1- (point-max)) (point-max) 'extract-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                spans))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_substring_multibuf_transfer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass transfer-log ()
    ((source :initarg :source :accessor tl-source :initform "")
     (dest :initarg :dest :accessor tl-dest :initform "")
     (range :initarg :range :accessor tl-range :initform nil)
     (prop-ok :initarg :prop-ok :accessor tl-prop-ok :initform nil)))
  (let* ((buf1 (generate-new-buffer "bs4a"))
         (buf2 (generate-new-buffer "bs4b"))
         (tl1 (transfer-log :source "buf1" :dest "buf2" :range '(1 . 6)))
         (tl2 (transfer-log :source "buf1" :dest "buf2" :range '(6 . 11))))
    (with-current-buffer buf1
      (insert "XXXXX-YYYYY-ZZZZZ")
      (put-text-property 1 6 'origin 'first)
      (put-text-property 7 12 'origin 'second)
      (put-text-property 13 17 'origin 'third))
    (with-current-buffer buf2
      (insert "AAAAA-BBBBB-CCCCC")
      (put-text-property 1 6 'origin 'dest-first)
      (put-text-property 7 12 'origin 'dest-second)
      (put-text-property 13 17 'origin 'dest-third)
      (setq-local tlogs (list tl1 tl2))
      (let* ((ov (make-overlay 7 12))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 7))
             (results nil))
        (undo-boundary)
        (goto-char (point-max))
        (insert "--")
        (insert-buffer-substring buf1 1 7)
        (setf (tl-prop-ok tl1) (eq (get-text-property 20 'origin) 'first))
        (push (list 'transfer1 (buffer-string) (tl-prop-ok tl1)) results)
        (goto-char 1)
        (insert-buffer-substring buf1 7 13)
        (setf (tl-prop-ok tl2) (eq (get-text-property 1 'origin) 'second))
        (push (list 'transfer2 (buffer-string) (tl-prop-ok tl2)
                   (marker-position m) (overlay-start ov) (overlay-end ov)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s tl1=%s tl2=%s"
                       results
                       (list (tl-range tl1) (tl-prop-ok tl1))
                       (list (tl-range tl2) (tl-prop-ok tl2))))
        (set-marker m 5)
        (put-text-property (1- (point-max)) (point-max) 'xfer-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                tlogs))))
    (kill-buffer buf1)
    (kill-buffer buf2)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_substring_undo_prop_restoration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable m)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass prop-snapshot ()
    ((step :initarg :step :accessor ps-step :initform "")
     (props-at-1 :initarg :props-at-1 :accessor ps-p1 :initform nil)
     (props-at-6 :initarg :props-at-6 :accessor ps-p6 :initform nil)
     (buf-len :initarg :buf-len :accessor ps-len :initform 0)))
  (let* ((buf (generate-new-buffer "bs5"))
         (ps1 (prop-snapshot :step "initial"))
         (ps2 (prop-snapshot :step "after-insert"))
         (ps3 (prop-snapshot :step "after-undo")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'type 'alpha)
      (put-text-property 6 10 'type 'beta)
      (put-text-property 11 15 'type 'gamma)
      (put-text-property 16 20 'type 'delta)
      (setq-local snapshots (list ps1 ps2 ps3))
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 6)))
        (undo-boundary)
        (setf (ps-p1 ps1) (get-text-property 1 'type)
              (ps-p6 ps1) (get-text-property 6 'type)
              (ps-len ps1) (point-max))
        (goto-char (point-max))
        (insert "--")
        (insert-buffer-substring (current-buffer) 1 11)
        (setf (ps-p1 ps2) (get-text-property 1 'type)
              (ps-p6 ps2) (get-text-property 6 'type)
              (ps-len ps2) (point-max))
        (undo-boundary)
        (primitive-undo 1 buffer-undo-list)
        (setf (ps-p1 ps3) (get-text-property 1 'type)
              (ps-p6 ps3) (get-text-property 6 'type)
              (ps-len ps3) (point-max))
        (goto-char (point-max))
        (insert (format " | ps1=%s ps2=%s ps3=%s m=%d ov=[%d,%d]"
                       (list (ps-step ps1) (ps-p1 ps1) (ps-p6 ps1) (ps-len ps1))
                       (list (ps-step ps2) (ps-p1 ps2) (ps-p6 ps2) (ps-len ps2))
                       (list (ps-step ps3) (ps-p1 ps3) (ps-p6 ps3) (ps-len ps3))
                       (marker-position m) (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'snapshot-log t))
      (list (marker-position m) (overlay-start ov) (overlay-end ov) (buffer-string)
            snapshots))
    (kill-buffer buf)))"#,
        expect,
    );
}

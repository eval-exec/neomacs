//! Combo: cl-eieio marker deletion/evaporate + overlays + textprop + buflocal + narrow + undo.
//! Tests marker lifecycle and overlay evaporate behavior with EIEIO objects.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_marker_set_nil_evaporate_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function overlay-live-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass marker-state ()
    ((pos :initarg :pos :accessor ms-pos :initform 0)
     (buffer :initarg :buffer :accessor ms-buf :initform nil)
     (insertion-type :initarg :insertion-type :accessor ms-itype :initform nil)))
  (let* ((buf (generate-new-buffer "ev1"))
         (snapshots nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'marker-group 'a)
      (put-text-property 6 9 'marker-group 'b)
      (put-text-property 10 13 'marker-group 'c)
      (setq-local my-snaps snapshots)
      (let* ((ov (make-overlay 3 7))
             (_ (overlay-put ov 'evaporate t))
             (_ (overlay-put ov 'priority 1))
             (m1 (make-marker))
             (m2 (make-marker))
             (_ (set-marker m1 5))
             (_ (set-marker m2 7))
             (_ (set-marker-insertion-type m2 t)))
        (undo-boundary)
        (push (marker-state :pos (marker-position m1) :buffer (marker-buffer m1)
                           :insertion-type (marker-insertion-type m1)) snapshots)
        (push (marker-state :pos (marker-position m2) :buffer (marker-buffer m2)
                           :insertion-type (marker-insertion-type m2)) snapshots)
        (goto-char 3)
        (insert "XX")
        (push (marker-state :pos (marker-position m1) :buffer (marker-buffer m1)
                           :insertion-type (marker-insertion-type m1)) snapshots)
        (push (marker-state :pos (marker-position m2) :buffer (marker-buffer m2)
                           :insertion-type (marker-insertion-type m2)) snapshots)
        (delete-region 3 5)
        (push (marker-state :pos (marker-position m1) :buffer (marker-buffer m1)) snapshots)
        (push (marker-state :pos (marker-position m2) :buffer (marker-buffer m2)) snapshots)
        (set-marker m1 nil)
        (push (marker-state :pos (marker-position m1) :buffer (marker-buffer m1)) snapshots)
        (delete-region (overlay-start ov) (overlay-end ov))
        (push (list 'ov-alive (overlay-live-p ov) 'ov-start (overlay-start ov)) snapshots)
        (setq snapshots (reverse snapshots))
        (goto-char (point-max))
        (insert (format " | snaps=%d first=%s last=%s"
                       (length snapshots)
                       (list (ms-pos (car snapshots)) (ms-itype (car snapshots)))
                       (car (last snapshots))))
        (put-text-property (1- (point-max)) (point-max) 'evap-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs
                (buffer-string)
                my-snaps
                (overlay-live-p ov)
                (marker-live-p m2)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_evaporate_overlay_with_textprops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function overlay-live-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass evaporate-log ()
    ((label :initarg :label :accessor el-label :initform "")
     (ov-start :initarg :ov-start :accessor el-start :initform 0)
     (ov-end :initarg :ov-end :accessor el-end :initform 0)
     (ov-alive :initarg :ov-alive :accessor el-alive :initform nil)))
  (let* ((buf (generate-new-buffer "ev2"))
         (logs nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'z1)
      (put-text-property 6 10 'zone 'z2)
      (put-text-property 11 15 'zone 'z3)
      (put-text-property 16 20 'zone 'z4)
      (setq-local my-logs logs)
      (let* ((ov1 (make-overlay 1 5))
             (ov2 (make-overlay 6 10))
             (_ (overlay-put ov1 'evaporate t))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'evaporate t))
             (_ (overlay-put ov2 'priority 2))
             (m (make-marker))
             (_ (set-marker m 3))
             (results nil))
        (undo-boundary)
        (push (evaporate-log :label "init"
                            :ov-start (overlay-start ov1)
                            :ov-end (overlay-end ov1)
                            :ov-alive (overlay-live-p ov1)) logs)
        (push (evaporate-log :label "init2"
                            :ov-start (overlay-start ov2)
                            :ov-end (overlay-end ov2)
                            :ov-alive (overlay-live-p ov2)) logs)
        (delete-region 1 5)
        (push (list 'after-del1 (overlay-live-p ov1) (overlay-live-p ov2) (marker-position m)) results)
        (delete-region 2 6)
        (push (list 'after-del2 (overlay-live-p ov1) (overlay-live-p ov2) (marker-position m)) results)
        (let ((zone-at-m (get-text-property (max 1 (1- (marker-position m))) 'zone)))
          (push (list 'zone zone-at-m) results))
        (setq logs (reverse logs))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | logs=%d results=%s m=%d"
                       (length logs) results (marker-position m)))
        (set-marker m 2)
        (put-text-property (1- (point-max)) (point-max) 'el-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs
                (buffer-string)
                my-logs
                (marker-position m)
                (overlay-live-p ov1)
                (overlay-live-p ov2)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_marker_relocation_after_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass marker-track ()
    ((name :initarg :name :accessor mt-name :initform "")
     (positions :initarg :positions :accessor mt-positions :initform nil)))
  (let* ((buf (generate-new-buffer "ev3"))
         (mt1 (marker-track :name "m1"))
         (mt2 (marker-track :name "m2"))
         (mt3 (marker-track :name "m3")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'section 's1)
      (put-text-property 6 10 'section 's2)
      (put-text-property 11 15 'section 's3)
      (put-text-property 16 20 'section 's4)
      (setq-local my-mts (list mt1 mt2 mt3))
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m1 (make-marker))
             (m2 (make-marker))
             (m3 (make-marker))
             (_ (set-marker m1 1))
             (_ (set-marker m2 8))
             (_ (set-marker m3 16))
             (_ (set-marker-insertion-type m2 t)))
        (undo-boundary)
        (push (marker-position m1) (mt-positions mt1))
        (push (marker-position m2) (mt-positions mt2))
        (push (marker-position m3) (mt-positions mt3))
        (goto-char 5)
        (insert "XX")
        (push (marker-position m1) (mt-positions mt1))
        (push (marker-position m2) (mt-positions mt2))
        (push (marker-position m3) (mt-positions mt3))
        (delete-region 3 7)
        (push (marker-position m1) (mt-positions mt1))
        (push (marker-position m2) (mt-positions mt2))
        (push (marker-position m3) (mt-positions mt3))
        (save-restriction
          (narrow-to-region 4 10)
          (goto-char (point-min))
          (insert "NN")
          (push (marker-position m1) (mt-positions mt1))
          (push (marker-position m2) (mt-positions mt2))
          (push (marker-position m3) (mt-positions mt3)))
        (setf (mt-positions mt1) (reverse (mt-positions mt1)))
        (setf (mt-positions mt2) (reverse (mt-positions mt2)))
        (setf (mt-positions mt3) (reverse (mt-positions mt3)))
        (goto-char (point-max))
        (insert (format " | mt1=%s mt2=%s mt3=%s ov=[%d,%d]"
                       (mt-positions mt1) (mt-positions mt2) (mt-positions mt3)
                       (overlay-start ov) (overlay-end ov)))
        (put-text-property (1- (point-max)) (point-max) 'mt-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs
                (buffer-string)
                (mt-positions mt1)
                (mt-positions mt2)
                (mt-positions mt3)
                my-mts
                (overlay-start ov)
                (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_marker_copy_relocate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass marker-pair ()
    ((src-pos :initarg :src-pos :accessor mp-src :initform 0)
     (dst-pos :initarg :dst-pos :accessor mp-dst :initform 0)
     (same :initarg :same :accessor mp-same :initform nil)))
  (let* ((buf (generate-new-buffer "ev4"))
         (pairs nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'block 'a)
      (put-text-property 6 10 'block 'b)
      (put-text-property 11 15 'block 'c)
      (put-text-property 16 20 'block 'd)
      (put-text-property 21 25 'block 'e)
      (setq-local my-pairs pairs)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 2))
             (m-src (make-marker))
             (_ (set-marker m-src 8))
             (m-dst (make-marker))
             (_ (set-marker m-dst 20))
             (results nil))
        (undo-boundary)
        (push (marker-pair :src-pos (marker-position m-src)
                          :dst-pos (marker-position m-dst)
                          :same (eq m-src m-dst)) pairs)
        (set-marker m-dst 8)
        (push (marker-pair :src-pos (marker-position m-src)
                          :dst-pos (marker-position m-dst)
                          :same (eq m-src m-dst)) pairs)
        (goto-char 8)
        (insert "QQQQ")
        (push (marker-pair :src-pos (marker-position m-src)
                          :dst-pos (marker-position m-dst)
                          :same (eq m-src m-dst)) pairs)
        (set-marker m-dst 20)
        (push (marker-pair :src-pos (marker-position m-src)
                          :dst-pos (marker-position m-dst)
                          :same (eq m-src m-dst)) pairs)
        (delete-region 6 10)
        (push (marker-pair :src-pos (marker-position m-src)
                          :dst-pos (marker-position m-dst)
                          :same (eq m-src m-dst)) pairs)
        (setq pairs (reverse pairs))
        (setq results (mapcar (lambda (p) (list (mp-src p) (mp-dst p) (mp-same p))) pairs))
        (goto-char (point-max))
        (insert (format " | results=%s ov=[%d,%d]"
                       results (overlay-start ov) (overlay-end ov)))
        (put-text-property (1- (point-max)) (point-max) 'mp-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs
                (buffer-string)
                my-pairs
                (overlay-start ov)
                (overlay-end ov)
                (marker-position m-src)
                (marker-position m-dst)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_marker_in_deleted_overlay_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function marker-live-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass region-state ()
    ((label :initarg :label :accessor rs-label :initform "")
     (m-pos :initarg :m-pos :accessor rs-mpos :initform 0)
     (m-alive :initarg :m-alive :accessor rs-malive :initform nil)
     (ov-alive :initarg :ov-alive :accessor rs-ovalive :initform nil)))
  (let* ((buf (generate-new-buffer "ev5"))
         (states nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'region 'r1)
      (put-text-property 6 10 'region 'r2)
      (put-text-property 11 15 'region 'r3)
      (put-text-property 16 20 'region 'r4)
      (put-text-property 21 25 'region 'r5)
      (setq-local my-states states)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'evaporate t))
             (_ (overlay-put ov 'priority 1))
             (m1 (make-marker))
             (m2 (make-marker))
             (_ (set-marker m1 6))
             (_ (set-marker m2 15))
             (results nil))
        (undo-boundary)
        (push (region-state :label "init"
                           :m-pos (marker-position m1)
                           :m-alive (marker-live-p m1)
                           :ov-alive (overlay-live-p ov)) states)
        (goto-char 8)
        (insert "XXX")
        (push (region-state :label "after-insert"
                           :m-pos (marker-position m1)
                           :m-alive (marker-live-p m1)
                           :ov-alive (overlay-live-p ov)) states)
        (delete-region 6 18)
        (push (region-state :label "after-delete"
                           :m-pos (marker-position m1)
                           :m-alive (marker-live-p m1)
                           :ov-alive (overlay-live-p ov)) states)
        (push (list 'm2-pos (marker-position m2) 'm2-alive (marker-live-p m2)) results)
        (let ((prop-at-m1 (get-text-property (max 1 (1- (marker-position m1))) 'region)))
          (push (list 'prop prop-at-m1) results))
        (setq states (reverse states))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | states=%d results=%s"
                       (length states) results))
        (put-text-property (1- (point-max)) (point-max) 'rs-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs
                (buffer-string)
                my-states
                (marker-position m1)
                (marker-live-p m1)
                (overlay-live-p ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

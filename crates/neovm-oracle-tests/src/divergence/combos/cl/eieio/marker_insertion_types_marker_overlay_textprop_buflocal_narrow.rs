//! Combo: cl-eieio marker insertion-type + overlay + textprop + buflocal + undo.
//! Tests marker insertion-type (t/nil) with EIEIO objects as tracked state, overlay edge behavior.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_marker_insertion_type_before_after() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass anchor ()
    ((label :initarg :label :accessor anchor-label :initform "")
     (position :initarg :position :accessor anchor-position :initform 0)))
  (let* ((buf (generate-new-buffer "mi1"))
         (a1 (anchor :label "start" :position 1))
         (a2 (anchor :label "end" :position 10)))
    (with-current-buffer buf
      (insert "AAAAAAAAAA-BBBBBBBBBB")
      (put-text-property 1 10 'region 'left)
      (put-text-property 11 20 'region 'right)
      (setq-local anchor1 a1)
      (setq-local anchor2 a2)
      (let* ((ov (make-overlay 1 10))
             (_ (overlay-put ov 'face 'bold))
             (m-before (make-marker))
             (m-after (make-marker))
             (_ (set-marker m-before 5))
             (_ (set-marker m-after 5))
             (_ (set-marker-insertion-type m-after t)))
        (undo-boundary)
        (setf (anchor-position a1) 1
              (anchor-position a2) 10)
        (goto-char 5)
        (insert "XXXX")
        (let ((pos-before (marker-position m-before))
              (pos-after (marker-position m-after))
              (ov-start (overlay-start ov))
              (ov-end (overlay-end ov)))
          (setf (anchor-position a1) pos-before
                (anchor-position a2) pos-after)
          (goto-char (point-max))
          (insert (format "|b=%d:a=%d:ov=[%d,%d]"
                         pos-before pos-after ov-start ov-end))
          (put-text-property (1- (point-max)) (point-max) 'marker-log t))
        (undo-boundary)
        (let ((mp-b (marker-position m-before))
              (mp-a (marker-position m-after))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (a1-pos (anchor-position anchor1))
              (a2-pos (anchor-position anchor2)))
          (primitive-undo 1 buffer-undo-list)
          (list mp-b mp-a os oe bs a1-pos a2-pos
                (marker-position m-before)
                (marker-position m-after)
                (buffer-string)
                anchor1 anchor2)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_multi_marker_types_insert_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass span ()
    ((name :initarg :name :accessor span-name :initform "")
     (start-mark :initarg :start-mark :accessor span-start :initform nil)
     (end-mark :initarg :end-mark :accessor span-end :initform nil)))
  (let* ((buf (generate-new-buffer "mi2"))
         (m1 (make-marker))
         (m2 (make-marker))
         (m3 (make-marker))
         (m4 (make-marker))
         (_ (set-marker m1 1))
         (_ (set-marker m2 5))
         (_ (set-marker m3 10))
         (_ (set-marker-insertion-type m2 t))
         (_ (set-marker-insertion-type m4 t))
         (_ (set-marker m4 15))
         (s1 (span :name "first" :start-mark m1 :end-mark m2))
         (s2 (span :name "second" :start-mark m3 :end-mark m4)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'span 1)
      (put-text-property 6 10 'span 2)
      (put-text-property 11 15 'span 3)
      (put-text-property 16 20 'span 4)
      (setq-local spans (list s1 s2))
      (let* ((ov1 (make-overlay 1 5))
             (ov2 (make-overlay 11 15))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2)))
        (undo-boundary)
        (goto-char 5)
        (insert "XXXX")
        (goto-char 15)
        (insert "YYYY")
        (let ((s1-start (marker-position (span-start s1)))
              (s1-end (marker-position (span-end s1)))
              (s2-start (marker-position (span-start s2)))
              (s2-end (marker-position (span-end s2)))
              (ov1s (overlay-start ov1))
              (ov1e (overlay-end ov1))
              (ov2s (overlay-start ov2))
              (ov2e (overlay-end ov2))
              (bs (buffer-string)))
          (goto-char (point-max))
          (insert (format " | s1=[%d,%d] s2=[%d,%d] ov1=[%d,%d] ov2=[%d,%d]"
                         s1-start s1-end s2-start s2-end ov1s ov1e ov2s ov2e))
          (put-text-property (1- (point-max)) (point-max) 'span-log t))
        (undo-boundary)
        (let ((bs (buffer-string))
              (s1s (marker-position (span-start (car spans))))
              (s1e (marker-position (span-end (car spans))))
              (s2s (marker-position (span-start (cadr spans))))
              (s2e (marker-position (span-end (cadr spans)))))
          (primitive-undo 1 buffer-undo-list)
          (list bs s1s s1e s2s s2e
                (marker-position (span-start (car spans)))
                (marker-position (span-end (car spans)))
                (buffer-string)
                spans)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_overlay_evaporate_with_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable ov1-live)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass region-state ()
    ((name :initarg :name :accessor rs-name :initform "")
     (overlay :initarg :overlay :accessor rs-overlay :initform nil)
     (original-start :initarg :original-start :accessor rs-ostart :initform 0)
     (original-end :initarg :original-end :accessor rs-oend :initform 0)))
  (let* ((buf (generate-new-buffer "mi3"))
         (ov1 (make-overlay 6 10))
         (ov2 (make-overlay 11 15))
         (_ (overlay-put ov1 'evaporate t))
         (_ (overlay-put ov2 'evaporate t))
         (_ (overlay-put ov1 'face 'bold))
         (_ (overlay-put ov2 'face 'italic))
         (rs1 (region-state :name "left" :overlay ov1 :original-start 6 :original-end 10))
         (rs2 (region-state :name "right" :overlay ov2 :original-start 11 :original-end 15)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'part 'a)
      (put-text-property 6 10 'part 'b)
      (put-text-property 11 15 'part 'c)
      (put-text-property 16 20 'part 'd)
      (setq-local regions (list rs1 rs2))
      (let* ((m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (goto-char 6)
        (delete-region 6 10)
        (let ((ov1-live (overlay-buffer (rs-overlay rs1)))
              (ov2-live (overlay-buffer (rs-overlay rs2)))
              (ov1-start (and ov1-live (overlay-start (rs-overlay rs1))))
              (ov1-end (and ov1-live (overlay-end (rs-overlay rs1))))
              (ov2-start (overlay-start (rs-overlay rs2)))
              (ov2-end (overlay-end (rs-overlay rs2)))
              (mp (marker-position m))
              (bs (buffer-string)))
          (goto-char (point-max))
          (insert (format " | live=%s,%s ov1=%s ov2=[%d,%d] m=%d"
                         ov1-live ov2-live
                         (if ov1-live (format "[%d,%d]" ov1-start ov1-end) "dead")
                         ov2-start ov2-end mp)))
        (undo-boundary)
        (let ((bs (buffer-string))
              (ov1-revived (overlay-buffer (rs-overlay (car regions))))
              (ov2-still (overlay-buffer (rs-overlay (cadr regions)))))
          (primitive-undo 1 buffer-undo-list)
          (list bs ov1-revived ov2-still
                (buffer-string)
                (marker-position m)
                regions)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_marker_adjustment_set_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass cursor-track ()
    ((id :initarg :id :accessor ct-id :initform 0)
     (marker :initarg :marker :accessor ct-marker :initform nil)))
  (let* ((buf1 (generate-new-buffer "mi4a"))
         (buf2 (generate-new-buffer "mi4b"))
         (m1 (make-marker))
         (m2 (make-marker))
         (_ (set-marker m1 3 buf1))
         (_ (set-marker m2 5 buf2))
         (c1 (cursor-track :id 1 :marker m1))
         (c2 (cursor-track :id 2 :marker m2)))
    (with-current-buffer buf1
      (insert "AAA-BBB-CCC")
      (put-text-property 1 4 'buf 'b1)
      (put-text-property 5 8 'buf 'b1b)
      (put-text-property 9 11 'buf 'b1c))
    (with-current-buffer buf2
      (insert "DDD-EEE-FFF")
      (put-text-property 1 4 'buf 'b2)
      (put-text-property 5 8 'buf 'b2b)
      (put-text-property 9 11 'buf 'b2c))
    (let* ((ov1 (with-current-buffer buf1 (let ((ov (make-overlay 5 8)))
                                            (overlay-put ov 'priority 1) ov)))
           (ov2 (with-current-buffer buf2 (let ((ov (make-overlay 5 8)))
                                            (overlay-put ov 'priority 2) ov))))
      (with-current-buffer buf1
        (setq-local cursors (list c1 c2))
        (goto-char 5)
        (insert "XXX"))
      (with-current-buffer buf2
        (goto-char 5)
        (insert "YYY"))
      (let ((m1-pos (marker-position (ct-marker c1)))
            (m2-pos (marker-position (ct-marker c2)))
            (m1-buf (marker-buffer (ct-marker c1)))
            (m2-buf (marker-buffer (ct-marker c2)))
            (ov1s (overlay-start ov1))
            (ov1e (overlay-end ov1))
            (ov2s (overlay-start ov2))
            (ov2e (overlay-end ov2))
            (b1s (with-current-buffer buf1 (buffer-string)))
            (b2s (with-current-buffer buf2 (buffer-string))))
        (set-marker (ct-marker c1) 7 buf2)
        (let ((m1-new-pos (marker-position (ct-marker c1)))
              (m1-new-buf (marker-buffer (ct-marker c1))))
          (with-current-buffer buf1 (primitive-undo 1 buffer-undo-list))
          (with-current-buffer buf2 (primitive-undo 1 buffer-undo-list))
          (list m1-pos m2-pos ov1s ov1e ov2s ov2e b1s b2s
                m1-new-pos m1-new-buf
                (marker-position (ct-marker c1))
                (marker-position (ct-marker c2))
                (with-current-buffer buf1 (buffer-string))
                (with-current-buffer buf2 (buffer-string))))))
    (kill-buffer buf1)
    (kill-buffer buf2)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_marker_overlay_textprop_stress() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass edit-operation ()
    ((op-type :initarg :op-type :accessor eo-type :initform nil)
     (marker :initarg :marker :accessor eo-marker :initform nil)
     (overlay :initarg :overlay :accessor eo-overlay :initform nil)))
  (let* ((buf (generate-new-buffer "mi5"))
         (operations nil))
    (with-current-buffer buf
      (insert "ABCDEFGHIJKLMNOPQRSTUVWXYZ")
      (dotimes (i 10)
        (let* ((m (make-marker))
               (_ (set-marker m (+ 1 (* 2 i))))
               (ov (make-overlay (+ 1 (* 2 i)) (+ 3 (* 2 i))))
               (_ (overlay-put ov 'priority i))
               (op (edit-operation :op-type (if (= (% i 2) 0) 'even 'odd)
                                   :marker m :overlay ov)))
          (push op operations)))
      (setq operations (nreverse operations))
      (setq-local ops operations)
      (let* ((m (make-marker))
             (_ (set-marker m 5))
             (all-markers (mapcar (lambda (op) (marker-position (eo-marker op))) operations))
             (all-ov-starts (mapcar (lambda (op) (overlay-start (eo-overlay op))) operations))
             (all-ov-ends (mapcar (lambda (op) (overlay-end (eo-overlay op))) operations)))
        (undo-boundary)
        (goto-char 10)
        (insert "-----")
        (let ((new-markers (mapcar (lambda (op) (marker-position (eo-marker op))) operations))
              (new-ov-starts (mapcar (lambda (op) (overlay-start (eo-overlay op))) operations))
              (new-ov-ends (mapcar (lambda (op) (overlay-end (eo-overlay op))) operations))
              (mp (marker-position m))
              (bs (buffer-string)))
          (goto-char 10)
          (insert (format "[%s->%s]" all-markers new-markers))
          (put-text-property 10 (+ 10 (length (format "[%s->%s]" all-markers new-markers)))
                            'stress-result t)
          (undo-boundary)
          (let ((bs2 (buffer-string))
                (restored-markers (mapcar (lambda (op) (marker-position (eo-marker op)))
                                          ops)))
            (primitive-undo 1 buffer-undo-list)
            (list mp bs new-markers new-ov-starts new-ov-ends
                  bs2 restored-markers
                  (buffer-string)
                  (marker-position m)
                  ops)))
        (kill-buffer buf)))))"#,
        expect,
    );
}

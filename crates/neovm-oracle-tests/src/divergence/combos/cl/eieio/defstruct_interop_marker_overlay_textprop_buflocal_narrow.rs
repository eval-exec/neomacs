//! Combo: cl-eieio defstruct interop + marker + overlay + textprop + buflocal + undo.
//! Tests interaction between cl-defstruct and EIEIO defclass objects with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_defstruct_shared_accessors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct (coord (:constructor make-coord (x y))) x y)
  (defclass point-3d ()
    ((x :initarg :x :accessor pt-x :initform 0)
     (y :initarg :y :accessor pt-y :initform 0)
     (z :initarg :z :accessor pt-z :initform 0)))
  (let* ((buf (generate-new-buffer "ds1"))
         (c (make-coord 10 20))
         (p (point-3d :x 10 :y 20 :z 30)))
    (with-current-buffer buf
      (insert "COORD:(10,20)-PT3D:(10,20,30)")
      (put-text-property 1 6 'type 'coord)
      (put-text-property 7 14 'field 'cvals)
      (put-text-property 15 20 'type 'pt3d)
      (put-text-property 21 33 'field 'pvals)
      (setq-local my-coord c)
      (setq-local my-point p)
      (let* ((ov (make-overlay 7 14))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 10)))
        (undo-boundary)
        (let ((cx (coord-x c))
              (cy (coord-y c))
              (px (pt-x p))
              (py (pt-y p))
              (pz (pt-z p)))
          (setf (coord-x c) (+ cx 5))
          (setf (coord-y c) (* cy 2))
          (setf (pt-x p) (+ px 5))
          (setf (pt-y p) (* py 2))
          (setf (pt-z p) (+ pz 10))
          (let ((new-cx (coord-x c))
                (new-cy (coord-y c))
                (new-px (pt-x p))
                (new-py (pt-y p))
                (new-pz (pt-z p)))
            (goto-char 7)
            (insert (format "(%d,%d)->(%d,%d)|(%d,%d,%d)"
                           cx cy new-cx new-cy
                           new-px new-py new-pz))
            (setf (marker-position m) 15)
            (put-text-property 7 (+ 7 (length (format "(%d,%d)->(%d,%d)|(%d,%d,%d)"
                                                        cx cy new-cx new-cy
                                                        new-px new-py new-pz)))
                              'coord-change t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (final-cx (coord-x my-coord))
              (final-cy (coord-y my-coord))
              (final-px (pt-x my-point))
              (final-py (pt-y my-point))
              (final-pz (pt-z my-point)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs final-cx final-cy final-px final-py final-pz
                (marker-position m)
                (buffer-string)
                my-coord my-point)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_defstruct_type_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct (range (:constructor make-range (start end))) start end)
  (defclass bounded-region ()
    ((lo :initarg :lo :accessor br-lo :initform 0)
     (hi :initarg :hi :accessor br-hi :initform 100)))
  (let* ((buf (generate-new-buffer "ds2"))
         (r (make-range 5 15))
         (b (bounded-region :lo 0 :hi 50)))
    (with-current-buffer buf
      (insert "RANGE:5-15-REGION:0-50")
      (put-text-property 1 6 'type 'range)
      (put-text-property 7 8 'field 'rstart)
      (put-text-property 9 11 'field 'rend)
      (put-text-property 12 19 'type 'region)
      (put-text-property 20 21 'field 'blo)
      (put-text-property 22 24 'field 'bhi)
      (setq-local my-range r)
      (setq-local my-region b)
      (let* ((ov (make-overlay 7 11))
             (_ (overlay-put ov 'priority 3))
             (m (make-marker))
             (_ (set-marker m 9))
             (preds (list (range-p r)
                         (bounded-region-p b)
                         (type-of r)
                         (type-of b))))
        (undo-boundary)
        (let ((span (- (range-end r) (range-start r)))
              (br-span (- (br-hi b) (br-lo b))))
          (setf (range-start r) 3)
          (setf (range-end r) 20)
          (setf (br-lo b) -10)
          (setf (br-hi b) 60)
          (let ((new-span (- (range-end r) (range-start r)))
                (new-br-span (- (br-hi b) (br-lo b))))
            (goto-char 7)
            (insert (format "%d->%d|%d->%d" span new-span br-span new-br-span))
            (setf (marker-position m) 12)
            (put-text-property 7 (+ 7 (length (format "%d->%d|%d->%d"
                                                        span new-span br-span new-br-span)))
                              'span-change t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (rs (range-start my-range))
              (re (range-end my-range))
              (blo (br-lo my-region))
              (bhi (br-hi my-region)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs rs re blo bhi preds
                (marker-position m)
                (buffer-string)
                my-range my-region)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_defstruct_copy_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct (snapshot (:constructor make-snapshot (label data)))
    label data)
  (defclass mutable-state ()
    ((tag :initarg :tag :accessor ms-tag :initform "")
     (history :initarg :history :accessor ms-history :initform nil)))
  (let* ((buf (generate-new-buffer "ds3"))
         (s1 (make-snapshot "v1" '(a b c)))
         (s2 (copy-snapshot s1))
         (ms (mutable-state :tag "active" :history (list s1))))
    (with-current-buffer buf
      (insert "SNAP:v1:(a,b,c)-STATE:active:h=1")
      (put-text-property 1 5 'type 'snap)
      (put-text-property 6 8 'field 'label)
      (put-text-property 9 16 'field 'data)
      (put-text-property 17 22 'type 'state)
      (put-text-property 23 29 'field 'tag)
      (put-text-property 30 33 'field 'hcount)
      (setq-local snap1 s1)
      (setq-local snap2 s2)
      (setq-local state ms)
      (let* ((ov (make-overlay 6 16))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (let ((s1-data (snapshot-data s1))
              (s2-data (snapshot-data s2))
              (s2-label (snapshot-label s2)))
          (setf (snapshot-data s2) '(d e f))
          (setf (snapshot-label s2) "v2")
          (push s2 (ms-history ms))
          (let ((h-len (length (ms-history ms)))
                (s1-still (snapshot-data s1))
                (s2-new (snapshot-data s2)))
            (goto-char 6)
            (insert (format "%s->%s|h=%d" s1-data s2-new h-len))
            (setf (marker-position m) 12)
            (put-text-property 6 (+ 6 (length (format "%s->%s|h=%d"
                                                        s1-data s2-new h-len)))
                              'snap-change t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (s1-data (snapshot-data snap1))
              (s2-data (snapshot-data snap2))
              (s2-label (snapshot-label snap2))
              (h-len (length (ms-history state)))
              (tag (ms-tag state)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs s1-data s2-data s2-label h-len tag
                (marker-position m)
                (buffer-string)
                snap1 snap2 state)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_defstruct_nested_with_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct (metric-val (:constructor make-metric-val (name value unit)))
    name value unit)
  (defclass observation ()
    ((timestamp :initarg :timestamp :accessor obs-ts :initform 0)
     (metrics :initarg :metrics :accessor obs-metrics :initform nil)
     (source :initarg :source :accessor obs-source :initform "")))
  (let* ((buf (generate-new-buffer "ds4"))
         (m1 (make-metric-val "cpu" 85.5 "percent"))
         (m2 (make-metric-val "mem" 4.2 "gb"))
         (m3 (make-metric-val "disk" 120 "iops"))
         (obs (observation :timestamp 1000
                          :metrics (list m1 m2 m3)
                          :source "host-1")))
    (with-current-buffer buf
      (insert "OBS:1000:host-1:cpu=85.5:mem=4.2:disk=120")
      (put-text-property 1 4 'type 'obs)
      (put-text-property 5 9 'field 'ts)
      (put-text-property 10 16 'field 'source)
      (put-text-property 17 26 'field 'cpu)
      (put-text-property 27 35 'field 'mem)
      (put-text-property 36 45 'field 'disk)
      (setq-local my-obs obs)
      (let* ((ov (make-overlay 17 35))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 20)))
        (undo-boundary)
        (let ((metrics-before (mapcar (lambda (mv) (cons (metric-val-name mv) (metric-val-value mv)))
                                      (obs-metrics obs))))
          (setf (metric-val-value m1) 92.3)
          (setf (metric-val-value m2) 6.8)
          (setf (obs-source obs) "host-1b")
          (let ((metrics-after (mapcar (lambda (mv) (cons (metric-val-name mv) (metric-val-value mv)))
                                       (obs-metrics obs))))
            (goto-char 17)
            (insert (format "%s->%s" metrics-before metrics-after))
            (setf (marker-position m) 25)
            (put-text-property 17 (+ 17 (length (format "%s->%s" metrics-before metrics-after)))
                              'metric-change t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (cpu (metric-val-value m1))
              (mem (metric-val-value m2))
              (disk (metric-val-value m3))
              (source (obs-source my-obs))
              (ts (obs-ts my-obs)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs cpu mem disk source ts
                (marker-position m)
                (buffer-string)
                my-obs)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_defstruct_record_type_of() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct (entry (:type list) :named) key value)
  (defclass container ()
    ((entries :initarg :entries :accessor ct-entries :initform nil)
     (name :initarg :name :accessor ct-name :initform "")))
  (let* ((buf (generate-new-buffer "ds5"))
         (e1 (make-entry :key "k1" :value 10))
         (e2 (make-entry :key "k2" :value 20))
         (e3 (make-entry :key "k3" :value 30))
         (c (container :name "test" :entries (list e1 e2 e3))))
    (with-current-buffer buf
      (insert "CONTAINER:test:e1=10:e2=20:e3=30")
      (put-text-property 1 10 'field 'header)
      (put-text-property 11 15 'field 'name)
      (put-text-property 16 21 'field 'e1)
      (put-text-property 22 27 'field 'e2)
      (put-text-property 28 33 'field 'e3)
      (setq-local my-container c)
      (let* ((ov (make-overlay 11 27))
             (_ (overlay-put ov 'face 'underline))
             (m (make-marker))
             (_ (set-marker m 14))
             (types (list (type-of e1) (type-of c)
                         (entry-p e1) (container-p c)
                         (car e1))))
        (undo-boundary)
        (let ((total (cl-loop for e in (ct-entries c) sum (entry-value e)))
              (e1-v (entry-value e1))
              (e2-v (entry-value e2)))
          (setf (entry-value e1) 15)
          (setf (entry-value e2) 25)
          (setf (ct-name c) "updated")
          (let ((new-total (cl-loop for e in (ct-entries c) sum (entry-value e))))
            (goto-char 16)
            (insert (format "sum=%d->%d" total new-total))
            (setf (marker-position m) 20)
            (put-text-property 16 (+ 16 (length (format "sum=%d->%d" total new-total)))
                              'entry-change t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (e1v (entry-value e1))
              (e2v (entry-value e2))
              (e3v (entry-value e3))
              (name (ct-name my-container)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs e1v e2v e3v name types
                (marker-position m)
                (buffer-string)
                my-container)))
      (kill-buffer buf))))"#,
        expect,
    );
}

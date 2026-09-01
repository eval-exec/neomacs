//! Combo: cl-eieio change-class + marker + overlay + textprop + buflocal + narrow + undo.
//! Tests change-class with buffer state manipulation across multiple classes.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_change_class_with_buffer_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass shape ()
    ((name :initarg :name :accessor shape-name :initform "unknown")
     (color :initarg :color :accessor shape-color :initform "white")))
  (defclass circle (shape)
    ((radius :initarg :radius :accessor circle-radius :initform 1.0)))
  (defclass rectangle (shape)
    ((width :initarg :width :accessor rect-width :initform 1.0)
     (height :initarg :height :accessor rect-height :initform 1.0)))
  (let* ((buf (generate-new-buffer "cc1"))
         (c (circle :name "my-circle" :color "red" :radius 5.0)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'shape-type 'circle)
      (put-text-property 6 10 'shape-type 'rect)
      (put-text-property 11 15 'shape-type 'triangle)
      (setq-local shape-cache (list c))
      (let* ((ov (make-overlay 1 10))
             (_ (overlay-put ov 'shape-ov t))
             (m (make-marker))
             (_ (set-marker m 3)))
        (undo-boundary)
        (let ((before-type (type-of c))
              (before-slots (mapcar (lambda (s) (slot-value c s))
                                    '(name color radius))))
          (change-class c 'rectangle :width 10.0 :height 20.0)
          (let ((after-type (type-of c))
                (after-name (shape-name c))
                (after-color (shape-color c))
                (after-width (rect-width c))
                (after-height (rect-height c))
                (after-radius (slot-exists-p c 'radius)))
            (goto-char 6)
            (insert (format "[%s->%s %sx%s]"
                           before-type after-type
                           after-width after-height))
            (put-text-property 6 (+ 6 (length (format "[%s->%s %sx%s]"
                                                        before-type after-type
                                                        after-width after-height)))
                              'changed t)
            (setf (marker-position m) 12)))
        (undo-boundary)
        (let ((v shape-cache)
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (tp (get-text-property (point-min) 'shape-type))
              (bs (buffer-string))
              (class-name (eieio-class-name (eieio-object-class c))))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list v mp os oe tp bs class-name
                shape-cache
                (marker-position m)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_change_class_setf_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass vehicle-base ()
    ((make :initarg :make :accessor vehicle-make)
     (model :initarg :model :accessor vehicle-model)
     (year :initarg :year :accessor vehicle-year)))
  (defclass sedan (vehicle-base)
    ((doors :initarg :doors :accessor sedan-doors :initform 4)))
  (defclass pickup (vehicle-base)
    ((bed-size :initarg :bed-size :accessor pickup-bed-size :initform 6.5)))
  (let* ((buf (generate-new-buffer "cc2"))
         (v (sedan :make "Toyota" :model "Camry" :year 2020 :doors 4)))
    (with-current-buffer buf
      (insert "LINE1-LINE2-LINE3-LINE4")
      (put-text-property 1 6 'prop 'a)
      (put-text-property 7 12 'prop 'b)
      (put-text-property 13 18 'prop 'c)
      (put-text-property 19 24 'prop 'd)
      (setq-local my-vehicle v)
      (let* ((ov (make-overlay 7 18))
             (_ (overlay-put ov 'priority 10))
             (m (make-marker))
             (_ (set-marker m 10)))
        (let ((pre-type (type-of v))
              (pre-doors (and (slot-exists-p v 'doors) (slot-boundp v 'doors) (sedan-doors v))))
          (change-class v 'pickup :bed-size 8.0)
          (setf (vehicle-year v) 2024)
          (goto-char 7)
          (insert (format "%s->pickup[bed=%s,yr=%d]"
                         pre-type (pickup-bed-size v) (vehicle-year v)))
          (setf (marker-position m) 12)
          (put-text-property 7 (+ 7 (length (format "%s->pickup[bed=%s,yr=%d]"
                                                      pre-type (pickup-bed-size v) (vehicle-year v))))
                            'changed t))
        (list (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (buffer-string)
              (type-of my-vehicle)
              (vehicle-make my-vehicle)
              (vehicle-year my-vehicle)
              (pickup-bed-size my-vehicle)
              my-vehicle)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_change_class_multi_instance_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass animal ()
    ((name :initarg :name :accessor animal-name)
     (sound :initarg :sound :accessor animal-sound :initform "...")))
  (defclass dog (animal)
    ((breed :initarg :breed :accessor dog-breed :initform "mutt")))
  (defclass cat (animal)
    ((indoor :initarg :indoor :accessor cat-indoor :initform t)))
  (let* ((buf (generate-new-buffer "cc3"))
         (d1 (dog :name "Rex" :sound "woof" :breed "shepherd"))
         (d2 (dog :name "Buddy" :sound "bark" :breed "labrador")))
    (with-current-buffer buf
      (insert "DOG1-DOG2-CAT1-CAT2")
      (put-text-property 1 5 'pet 'dog)
      (put-text-property 6 10 'pet 'dog)
      (put-text-property 11 15 'pet 'cat)
      (put-text-property 16 20 'pet 'cat)
      (setq-local pets (list d1 d2))
      (let* ((ov1 (make-overlay 1 10))
             (ov2 (make-overlay 11 20))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2))
             (m1 (make-marker))
             (m2 (make-marker))
             (_ (set-marker m1 3))
             (_ (set-marker m2 13)))
        (undo-boundary)
        (let ((n1 (animal-name d1))
              (b1 (dog-breed d1))
              (n2 (animal-name d2))
              (b2 (dog-breed d2)))
          (change-class d1 'cat :indoor nil)
          (change-class d2 'cat :indoor t)
          (goto-char 1)
          (insert (format "[%s:%s->cat|%s:%s->cat]" n1 b1 n2 b2))
          (setf (marker-position m1) 15)
          (setf (marker-position m2) 30)
          (put-text-property 1 10 'changed t))
        (undo-boundary)
        (let ((mp1 (marker-position m1))
              (mp2 (marker-position m2))
              (os1 (overlay-start ov1))
              (oe1 (overlay-end ov1))
              (os2 (overlay-start ov2))
              (oe2 (overlay-end ov2))
              (tp (get-text-property 1 'pet))
              (bs (buffer-string))
              (c1-type (type-of d1))
              (c1-indoor (cat-indoor d1))
              (c2-type (type-of d2))
              (c2-indoor (cat-indoor d2)))
          (primitive-undo 1 buffer-undo-list)
          (list mp1 mp2 os1 oe1 os2 oe2 tp bs
                c1-type c1-indoor c2-type c2-indoor
                (marker-position m1) (marker-position m2)
                (buffer-string)
                pets)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_change_class_with_hash_and_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass node ()
    ((id :initarg :id :accessor node-id :initform 0)
     (data :initarg :data :accessor node-data :initform nil)))
  (defclass leaf-node (node)
    ((value :initarg :value :accessor leaf-value :initform 0)))
  (defclass branch-node (node)
    ((children :initarg :children :accessor branch-children :initform nil)))
  (let* ((buf (generate-new-buffer "cc4"))
         (n (leaf-node :id 1 :data "test" :value 42)))
    (with-current-buffer buf
      (insert "LEAF-BRANCH-MERGE-SPLIT")
      (put-text-property 1 5 'node-type 'leaf)
      (put-text-property 6 12 'node-type 'branch)
      (setq-local node-table (make-hash-table :test 'equal))
      (puthash "node-1" n node-table)
      (setq-local node-plist (list :id 1 :type 'leaf :value 42))
      (let* ((ov (make-overlay 6 18))
             (_ (overlay-put ov 'node-ov t))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (let ((pre-type (type-of n))
              (pre-val (leaf-value n))
              (pre-hash (gethash "node-1" node-table)))
          (change-class n 'branch-node :children (list 10 20 30))
          (let* ((post-type (type-of n))
                 (post-children (branch-children n))
                 (post-has-leaf (slot-exists-p n 'value))
                 (post-hash (gethash "node-1" node-table))
                 (post-hash-type (and post-hash (type-of post-hash)))
                 (plist-updated (plist-put node-plist :type 'branch)))
            (goto-char 6)
            (insert (format "{%s->%s children=%s}" pre-type post-type post-children))
            (setf (marker-position m) 20)
            (put-text-property 6 (+ 6 (length (format "{%s->%s children=%s}"
                                                       pre-type post-type post-children)))
                              'changed t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (tp (get-text-property 1 'node-type))
              (bs (buffer-string))
              (hash-type (type-of (gethash "node-1" node-table)))
              (plist-val (plist-get node-plist :type)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe tp bs hash-type plist-val
                (marker-position m)
                (buffer-string)
                node-table)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_change_class_setf_replace_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass metric ()
    ((label :initarg :label :accessor metric-label :initform "")
     (unit :initarg :unit :accessor metric-unit :initform "count")))
  (defclass temperature (metric)
    ((celsius :initarg :celsius :accessor temp-celsius :initform 0.0)))
  (defclass distance (metric)
    ((meters :initarg :meters :accessor dist-meters :initform 0.0)))
  (let* ((buf (generate-new-buffer "cc5"))
         (obj (temperature :label "outside" :unit "C" :celsius 22.5)))
    (with-current-buffer buf
      (insert "TEMP:22.5C-DIST:0.0M-NOTE:OK")
      (put-text-property 1 11 'metric 'temp)
      (put-text-property 12 21 'metric 'dist)
      (put-text-property 22 29 'metric 'note)
      (setq-local current-metric obj)
      (let* ((ov (make-overlay 1 21))
             (_ (overlay-put ov 'highlight t))
             (m (make-marker))
             (_ (set-marker m 6)))
        (undo-boundary)
        (let ((t-label (metric-label obj))
              (t-unit (metric-unit obj))
              (t-celsius (temp-celsius obj)))
          (change-class obj 'distance :meters 100.0)
          (setf (metric-label obj) "road"
                (metric-unit obj) "m")
          (let ((d-label (metric-label obj))
                (d-unit (metric-unit obj))
                (d-meters (dist-meters obj)))
            (goto-char 1)
            (re-search-forward "TEMP")
            (replace-match (format "DIST[%s:%s%s]" d-label d-meters d-unit))
            (setf (marker-position m) 15)
            (put-text-property 1 15 'metric 'changed)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (tp (get-text-property 1 'metric))
              (bs (buffer-string))
              (obj-type (type-of current-metric))
              (obj-meters (dist-meters current-metric)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe tp bs obj-type obj-meters
                (marker-position m)
                (buffer-string)
                current-metric)))
      (kill-buffer buf))))"#,
        expect,
    );
}

//! Combo: cl-eieio print-object + clone + MOP introspection + marker + overlay + textprop + buflocal + narrow + undo.
//! Tests object printing, cloning, class introspection with buffer manipulation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_print_object_custom_buffer_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (defclass named-entity ()
    ((name :initarg :name :accessor entity-name :initform "")
     (id :initarg :id :accessor entity-id :initform 0)))
  (defclass user-entity (named-entity)
    ((email :initarg :email :accessor user-email :initform "")))
  (defmethod cl-print-object ((e named-entity) stream)
    (princ (format "#<entity:%s[%d]>" (entity-name e) (entity-id e)) stream))
  (let* ((buf (generate-new-buffer "po1"))
         (u (user-entity :name "alice" :id 42 :email "alice@test.com")))
    (with-current-buffer buf
      (insert "USER:alice:42:alice@test.com")
      (put-text-property 1 5 'field 'type)
      (put-text-property 6 11 'field 'name)
      (put-text-property 12 14 'field 'id)
      (put-text-property 15 29 'field 'email)
      (setq-local entity u)
      (let* ((ov (make-overlay 6 14))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (let ((printed (format "%s" u))
              (e-name (entity-name u))
              (e-id (entity-id u)))
          (setf (entity-name u) "bob"
                (entity-id u) 99)
          (let ((new-printed (format "%s" u))
                (new-email (user-email u)))
            (goto-char 6)
            (insert (format "%s->%s" printed new-printed))
            (setf (marker-position m) 20)
            (put-text-property 6 (+ 6 (length (format "%s->%s" printed new-printed)))
                              'print-change t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (final-name (entity-name entity))
              (final-id (entity-id entity))
              (final-email (user-email entity)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs final-name final-id final-email
                (marker-position m)
                (buffer-string)
                entity)))
      (kill-buffer buf))))"##,
        expect,
    );
}

#[test]
fn combo_eieio_clone_independent_mutations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass score-record ()
    ((player :initarg :player :accessor score-player :initform "")
     (points :initarg :points :accessor score-points :initform 0)
     (level :initarg :level :accessor score-level :initform 1)))
  (let* ((buf (generate-new-buffer "cl1"))
         (s1 (score-record :player "alice" :points 100 :level 5)))
    (with-current-buffer buf
      (insert "SCORE:alice:100:L5")
      (put-text-property 1 6 'field 'label)
      (put-text-property 7 12 'field 'player)
      (put-text-property 13 16 'field 'points)
      (put-text-property 17 19 'field 'level)
      (setq-local original s1)
      (let* ((ov (make-overlay 7 16))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 10))
             (s2 (clone s1)))
        (setq-local cloned s2)
        (undo-boundary)
        (let ((orig-player (score-player s1))
              (orig-points (score-points s1))
              (orig-level (score-level s1))
              (clone-player (score-player s2))
              (clone-points (score-points s2))
              (clone-level (score-level s2)))
          (setf (score-points s1) 200
                (score-level s1) 10)
          (setf (score-player s2) "bob"
                (score-points s2) 50)
          (let ((s1-after (list (score-player s1) (score-points s1) (score-level s1)))
                (s2-after (list (score-player s2) (score-points s2) (score-level s2))))
            (goto-char 7)
            (insert (format "%s:%d:L%d" (score-player s1) (score-points s1) (score-level s1)))
            (setf (marker-position m) 15)
            (put-text-property 7 (+ 7 (length (format "%s:%d:L%d"
                                                        (score-player s1) (score-points s1) (score-level s1))))
                              'mutated t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (orig-p (score-player original))
              (orig-pts (score-points original))
              (orig-lvl (score-level original))
              (clone-p (score-player cloned))
              (clone-pts (score-points cloned))
              (clone-lvl (score-level cloned)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs orig-p orig-pts orig-lvl clone-p clone-pts clone-lvl
                (marker-position m)
                (buffer-string)
                original cloned)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_class_precedence_introspection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function eieio-class-precedence-list)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass base () ((x :initarg :x :accessor base-x :initform 0)))
  (defclass mid1 (base) ((y :initarg :y :accessor mid1-y :initform 0)))
  (defclass mid2 (base) ((z :initarg :z :accessor mid2-z :initform 0)))
  (defclass derived (mid1 mid2) ((w :initarg :w :accessor derived-w :initform 0)))
  (let* ((buf (generate-new-buffer "cp1"))
         (d (derived :x 1 :y 2 :z 3 :w 4)))
    (with-current-buffer buf
      (insert "DERIVED:x=1:y=2:z=3:w=4")
      (put-text-property 1 8 'layer 'derived)
      (put-text-property 9 12 'layer 'base)
      (put-text-property 13 16 'layer 'mid1)
      (put-text-property 17 20 'layer 'mid2)
      (put-text-property 21 24 'layer 'derived)
      (setq-local obj d)
      (let* ((ov (make-overlay 1 8))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 5))
             (cpl (eieio-class-precedence-list (eieio-object-class d)))
             (cpl-names (mapcar (lambda (c) (eieio-class-name c)) cpl))
             (slots (eieio-class-slots (eieio-object-class d)))
             (slot-count (length slots)))
        (undo-boundary)
        (let ((cpl-str (format "%s[%d]" cpl-names slot-count)))
          (setf (base-x d) 10
                (mid1-y d) 20)
          (goto-char 9)
          (insert (format "CPL:%s" cpl-str))
          (setf (marker-position m) 15)
          (put-text-property 9 (+ 9 (length (format "CPL:%s" cpl-str)))
                            'cpl t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (x (base-x obj))
              (y (mid1-y obj))
              (w (derived-w obj)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs x y w
                (marker-position m)
                (buffer-string)
                obj)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_object_types_and_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function class-ancestor-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass shape ()
    ((area :initarg :area :accessor shape-area :initform 0.0)))
  (defclass circle-shape (shape)
    ((radius :initarg :radius :accessor circle-radius :initform 1.0)))
  (defclass rect-shape (shape)
    ((width :initarg :width :accessor rect-width :initform 1.0)
     (height :initarg :height :accessor rect-height :initform 1.0)))
  (let* ((buf (generate-new-buffer "tp1"))
         (c (circle-shape :radius 5.0))
         (r (rect-shape :width 3.0 :height 4.0)))
    (with-current-buffer buf
      (insert "CIRCLE:r=5-RECT:w=3,h=4")
      (put-text-property 1 7 'shape 'circle)
      (put-text-property 8 11 'shape 'radius)
      (put-text-property 12 16 'shape 'rect)
      (put-text-property 17 23 'shape 'dims)
      (setq-local shapes (list c r))
      (let* ((ov (make-overlay 1 23))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 9))
             (preds (list
                     (object-of-class-p c 'shape)
                     (object-of-class-p c 'circle-shape)
                     (object-of-class-p c 'rect-shape)
                     (child-of-class-p (eieio-object-class c) 'shape)
                     (class-ancestor-p 'shape (eieio-object-class c))
                     (type-of c)
                     (type-of r))))
        (undo-boundary)
        (setf (shape-area c) (* float-pi (expt (circle-radius c) 2)))
        (setf (shape-area r) (* (rect-width r) (rect-height r)))
        (let ((c-area (shape-area c))
              (r-area (shape-area r)))
          (goto-char 1)
          (insert (format "A=%.2f,%.2f:" c-area r-area))
          (setf (marker-position m) 10)
          (put-text-property 1 (+ 1 (length (format "A=%.2f,%.2f:" c-area r-area)))
                            'areas t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (c-area (shape-area (car shapes)))
              (r-area (shape-area (cadr shapes))))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs c-area r-area preds
                (marker-position m)
                (buffer-string)
                shapes)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_print_read_roundtrip_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (defclass serializable ()
    ((data :initarg :data :accessor serial-data :initform nil)
     (version :initarg :version :accessor serial-version :initform 1)))
  (defmethod cl-print-object ((s serializable) stream)
    (princ (format "#<serial:%s:v%d>"
                   (prin1-to-string (serial-data s))
                   (serial-version s)) stream))
  (let* ((buf (generate-new-buffer "pr1"))
         (s (serializable :data '(1 "two" (three . 4)) :version 3)))
    (with-current-buffer buf
      (insert "SERIAL:(1 two (three . 4)):v3")
      (put-text-property 1 8 'field 'type)
      (put-text-property 9 26 'field 'data)
      (put-text-property 27 29 'field 'version)
      (setq-local ser-obj s)
      (let* ((ov (make-overlay 9 26))
             (_ (overlay-put ov 'face 'underline))
             (m (make-marker))
             (_ (set-marker m 12)))
        (undo-boundary)
        (let ((printed (format "%s" s))
              (data-before (serial-data s))
              (ver-before (serial-version s)))
          (setf (serial-data s) (append data-before '(5 6 7)))
          (setf (serial-version s) (+ ver-before 1))
          (let ((new-printed (format "%s" s))
                (new-data (serial-data s)))
            (goto-char 9)
            (insert (format "%s->v%d" (prin1-to-string new-data) (serial-version s)))
            (setf (marker-position m) 20)
            (put-text-property 9 (+ 9 (length (format "%s->v%d"
                                                        (prin1-to-string new-data) (serial-version s))))
                              'serial-change t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (final-data (serial-data ser-obj))
              (final-ver (serial-version ser-obj)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs final-data final-ver
                (marker-position m)
                (buffer-string)
                ser-obj)))
      (kill-buffer buf))))"##,
        expect,
    );
}

//! Strict combo oracle probes, batch 162: EIEIO. defclass with :initarg/
//! :initform/:accessor, make-instance, slot inheritance + initform override in
//! subclass, slot-value/oset/object-of-class-p/slot-boundp, and defmethod
//! primary + :before + :after method combination with a call-order log.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_eieio_defclass_slots_inheritance_accessors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'eieio)
(require 'cl-generic)
(defclass probe-animal ()
  ((pname :initarg :pname :initform "anon" :accessor probe-animal-name)
   (plegs :initarg :plegs :initform 4 :accessor probe-animal-legs)))
(defclass probe-dog (probe-animal)
  ((plegs :initform 4)
   (breed :initarg :breed :initform "mutt" :accessor probe-dog-breed)))
(let* ((d (probe-dog :pname "Rex" :breed "lab"))
       (a (probe-animal :pname "Cat" :plegs 4)))
  (list (probe-animal-name d)
        (probe-animal-legs d)
        (probe-dog-breed d)
        (probe-animal-name a)
        (object-of-class-p d 'probe-animal)
        (object-of-class-p d 'probe-dog)
        (object-of-class-p a 'probe-dog)
        (slot-value d 'pname)
        (slot-value d 'breed)
        (slot-boundp d 'breed)
        (slot-boundp d 'plegs)
        (progn (oset d breed "poodle") (probe-dog-breed d))))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"Rex\" 4 \"lab\" \"Cat\" t t nil \"Rex\" \"lab\" t t \"poodle\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_eieio_defmethod_before_after_combination() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'eieio)
(require 'cl-generic)
(defclass probe-creature () ())
(defclass probe-beast (probe-creature) ())
(defvar probe-clog nil)
(cl-defgeneric probe-cry (c))
(cl-defmethod probe-cry ((c probe-creature)) (push 'primary probe-clog) "primary")
(cl-defmethod probe-cry :before ((c probe-creature)) (push 'before probe-clog))
(cl-defmethod probe-cry :after ((c probe-beast)) (push 'after probe-clog))
(let ((probe-clog nil))
  (let ((res (probe-cry (probe-creature))))
    (list res (nreverse probe-clog))))
"##;
    let expect = expect_test::expect![[r#""OK (\"primary\" (before primary))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_eieio_slot_default_initarg_type_cl_includes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'eieio)
(defclass probe-box ()
  ((label :initarg :label :initform "?" :type string)
   (count :initarg :count :initform 0 :type integer)))
(let* ((b1 (probe-box :label "first" :count 3))
       (b2 (probe-box)))
  (list (slot-value b1 'label)
        (slot-value b1 'count)
        (slot-value b2 'label)
        (slot-value b2 'count)
        (cl-typep b1 'probe-box)
        (cl-typep b1 'probe-animal-missing)
        (with-slots (label count) b1
          (list label count))))
"##;
    let expect = expect_test::expect![[r#""ERR (error \"Unknown type probe-animal-missing\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

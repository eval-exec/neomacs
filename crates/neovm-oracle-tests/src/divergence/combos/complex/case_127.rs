//! Complex combo batch 127 — eieio `cl-defgeneric` options, method
//! combination with `:around`, multiple dispatch, EQL specializer.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx127_eieio_cl_defgeneric_with_method_combination() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx127-c1 () ())
      (cl-defgeneric neo-cx127-combine (obj)
        (:method-combination +))
      (cl-defmethod neo-cx127-combine + ((obj neo-cx127-c1)) 1)
      (cl-defmethod neo-cx127-combine + ((obj neo-cx127-c1)) 2)
      (cl-defmethod neo-cx127-combine + ((obj neo-cx127-c1)) 3)
      (let ((inst (make-instance 'neo-cx127-c1)))
        (neo-cx127-combine inst)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx127_eieio_cl_defgeneric_argument_precedence_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :ab""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx127-a () ())
      (defclass neo-cx127-b () ())
      (cl-defgeneric neo-cx127-call (obj-a obj-b)
        (:argument-precedence-order obj-b obj-a))
      (cl-defmethod neo-cx127-call ((a neo-cx127-a) (b neo-cx127-b)) :ab)
      (let ((inst-a (make-instance 'neo-cx127-a))
            (inst-b (make-instance 'neo-cx127-b)))
        (neo-cx127-call inst-a inst-b)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx127_eieio_cl_defgeneric_declares_generic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t \"\\n\\n(fn &rest ARGS)\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx127-d () ())
      (cl-defgeneric neo-cx127-declared (obj))
      (let* ((generic (cl-generic-p 'neo-cx127-declared))
             (lazy-fn (and generic (cl--generic-lazy-function generic))))
        (list (fboundp 'neo-cx127-declared)
              (eq (cl--generic-name generic) 'neo-cx127-declared)
              (functionp lazy-fn)
              (documentation-stringp (aref lazy-fn 4))
              (documentation lazy-fn t)
              (fboundp 'neo-cx127-not-declared))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx127_eieio_eql_specializer_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:eql-special :default)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx127-eq () ())
      (cl-defgeneric neo-cx127-eql-disp (obj))
      (cl-defmethod neo-cx127-eql-disp ((obj (eql :special))) :eql-special)
      (cl-defmethod neo-cx127-eql-disp (obj) :default)
      (list (neo-cx127-eql-disp :special)
            (neo-cx127-eql-disp :other)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx127_eieio_around_method_with_call_next_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:primary-result (:around-begin :primary :around-end))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx127-around () ())
      (let (calls)
        (cl-defgeneric neo-cx127-call (obj))
        (cl-defmethod neo-cx127-call :around ((obj neo-cx127-around))
          (push :around-begin calls)
          (let ((r (cl-call-next-method)))
            (push :around-end calls)
            r))
        (cl-defmethod neo-cx127-call ((obj neo-cx127-around))
          (push :primary calls)
          :primary-result)
        (let ((inst (make-instance 'neo-cx127-around)))
          (list (neo-cx127-call inst)
                (nreverse calls)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx127_eieio_method_dispatch_priority_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:root :mid :leaf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx127-root () ())
      (defclass neo-cx127-mid (neo-cx127-root) ())
      (defclass neo-cx127-leaf (neo-cx127-mid) ())
      (cl-defgeneric neo-cx127-where (obj))
      (cl-defmethod neo-cx127-where ((obj neo-cx127-root)) :root)
      (cl-defmethod neo-cx127-where ((obj neo-cx127-mid)) :mid)
      (cl-defmethod neo-cx127-where ((obj neo-cx127-leaf)) :leaf)
      (list (neo-cx127-where (make-instance 'neo-cx127-root))
            (neo-cx127-where (make-instance 'neo-cx127-mid))
            (neo-cx127-where (make-instance 'neo-cx127-leaf))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx127_eieio_call_next_method_chain_through_methods() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx127-chain () ())
      (cl-defgeneric neo-cx127-chain-call (obj))
      (cl-defmethod neo-cx127-chain-call ((obj neo-cx127-chain))
        (cons :primary
              (when (next-method-p)
                (cl-call-next-method))))
      (let ((inst (make-instance 'neo-cx127-chain)))
        (neo-cx127-chain-call inst)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx127_eieio_find_method_via_cl_method_qualifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx127-find () ())
      (cl-defgeneric neo-cx127-fm (obj))
      (cl-defmethod neo-cx127-fm :before ((obj neo-cx127-find)) :b)
      (cl-defmethod neo-cx127-fm ((obj neo-cx127-find)) :p)
      (cl-defmethod neo-cx127-fm :after ((obj neo-cx127-find)) :a)
      (let ((methods (cl-generic-methods 'neo-cx127-fm))
            (specializers (cl--generic-method-specializers
                            (car (cl-generic-methods 'neo-cx127-fm)))))
        (list (consp methods)
              (= (length methods) 3))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx127_eieio_class_definition_inherits_slots_and_accessors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"alpha\" 30 \"alpha\" 30)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx127-parent ()
        ((name :initarg :name :initform "anon"
               :reader neo-cx127-parent-name)))
      (defclass neo-cx127-child (neo-cx127-parent)
        ((age :initarg :age :initform 0
              :reader neo-cx127-child-age)))
      (let ((inst (make-instance 'neo-cx127-child :name "alpha" :age 30)))
        (list (slot-value inst 'name)
              (slot-value inst 'age)
              (neo-cx127-parent-name inst)
              (neo-cx127-child-age inst))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx127_eieio_dispatch_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx127-mega ()
        ((value :initarg :value :initform 0)))
      (cl-defgeneric neo-cx127-touch (obj multiplier))
      (cl-defmethod neo-cx127-touch :before ((obj neo-cx127-mega) mult)
        (oset obj value (+ (slot-value obj 'value) mult)))
      (cl-defmethod neo-cx127-touch :after ((obj neo-cx127-mega) mult)
        (oset obj value (* (slot-value obj 'value) mult)))
      (cl-defmethod neo-cx127-touch ((obj neo-cx127-mega) mult)
        (oset obj value (+ (slot-value obj 'value) mult))
        (slot-value obj 'value))
      (let ((inst (make-instance 'neo-cx127-mega :value 1)))
        (with-temp-buffer
          (buffer-enable-undo)
          (insert "EIEIO dispatch mega test buffer content")
          (put-text-property 1 6 'face 'bold)
          (let ((m (set-marker (make-marker) 8))
                (ov (make-overlay 4 14)))
            (overlay-put ov 'face 'italic)
            (overlay-put ov 'evaporate t)
            (narrow-to-region 2 18)
            (let ((r (neo-cx127-touch inst 5)))
              (let ((state (list r (slot-value inst 'value)
                                 (buffer-string)
                                 (marker-position m)
                                 (overlay-start ov) (overlay-end ov)
                                 (text-properties-at 1))))
                (undo)
                (widen)
                (list state (buffer-string) (marker-position m)
                      (overlay-start ov) (overlay-end ov)
                      (text-properties-at 1))))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

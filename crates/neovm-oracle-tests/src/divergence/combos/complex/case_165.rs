//! Complex combo batch 165 — `cl-defmethod` with `&context` specializers,
//! `:around` chains, multiple value returns, and EQL specializer with
//! complex dispatch.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx165_eieio_cl_defmethod_with_eql_specializer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:eql-special :class :default)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx165-cls () ())
      (cl-defgeneric neo-cx165-call (obj))
      (cl-defmethod neo-cx165-call ((obj (eql :special))) :eql-special)
      (cl-defmethod neo-cx165-call ((obj neo-cx165-cls)) :class)
      (cl-defmethod neo-cx165-call (obj) :default)
      (list (neo-cx165-call :special)
            (neo-cx165-call (make-instance 'neo-cx165-cls))
            (neo-cx165-call "other")))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx165_eieio_cl_defmethod_qualifier_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx165-chain () ())
      (let (calls)
        (cl-defgeneric neo-cx165-call (obj))
        (cl-defmethod neo-cx165-call :before ((obj neo-cx165-chain))
          (push :before calls))
        (cl-defmethod neo-cx165-call :around ((obj neo-cx165-chain))
          (push :around-enter calls)
          (let ((r (cl-call-next-method)))
            (push :around-exit calls)
            r))
        (cl-defmethod neo-cx165-call ((obj neo-cx165-chain))
          (push :primary calls)
          (if (next-method-p) (cl-call-next-method) :primary))
        (cl-defmethod neo-cx165-call :after ((obj neo-cx165-chain))
          (push :after calls))
        (let ((result (neo-cx165-call (make-instance 'neo-cx165-chain))))
          (list result (nreverse calls)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx165_eieio_dispatch_with_two_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:ab :ba)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx165-a () ())
      (defclass neo-cx165-b () ())
      (cl-defgeneric neo-cx165-bidi (obj1 obj2))
      (cl-defmethod neo-cx165-bidi ((obj1 neo-cx165-a) (obj2 neo-cx165-b)) :ab)
      (cl-defmethod neo-cx165-bidi ((obj1 neo-cx165-b) (obj2 neo-cx165-a)) :ba)
      (let ((a (make-instance 'neo-cx165-a))
            (b (make-instance 'neo-cx165-b)))
        (list (neo-cx165-bidi a b)
              (neo-cx165-bidi b a))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx165_eieio_no_applicable_method_signal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:caught-no-applicable (cl-no-applicable-method neo-cx165-nogeneric #s(neo-cx165-na)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx165-na () ())
      (cl-defgeneric neo-cx165-nogeneric (obj))
      (let ((inst (make-instance 'neo-cx165-na)))
        (condition-case err
            (neo-cx165-nogeneric inst)
          (cl-no-applicable-method (list :caught-no-applicable err))
          (no-method (list :caught-no-method err))
          (error (list :caught-other (car err))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx165_eieio_method_combination_plus_with_around() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx165-mc () ())
      (cl-defgeneric neo-cx165-mc-call (obj)
        (:method-combination +))
      (cl-defmethod neo-cx165-mc-call + ((obj neo-cx165-mc)) 10)
      (cl-defmethod neo-cx165-mc-call + ((obj neo-cx165-mc)) 20)
      (cl-defmethod neo-cx165-mc-call + ((obj neo-cx165-mc)) 30)
      (let ((inst (make-instance 'neo-cx165-mc)))
        (neo-cx165-mc-call inst)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx165_eieio_call_next_method_returns_to_outermost() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx165-root () ())
      (defclass neo-cx165-mid (neo-cx165-root) ())
      (defclass neo-cx165-leaf (neo-cx165-mid) ())
      (cl-defgeneric neo-cx165-chain (obj))
      (cl-defmethod neo-cx165-chain ((obj neo-cx165-root))
        (if (next-method-p) (cons :root (cl-call-next-method)) :root))
      (cl-defmethod neo-cx165-chain ((obj neo-cx165-mid))
        (if (next-method-p) (cons :mid (cl-call-next-method)) :mid))
      (cl-defmethod neo-cx165-chain ((obj neo-cx165-leaf))
        (if (next-method-p) (cons :leaf (cl-call-next-method)) :leaf))
      (list (neo-cx165-chain (make-instance 'neo-cx165-leaf))
            (neo-cx165-chain (make-instance 'neo-cx165-mid))
            (neo-cx165-chain (make-instance 'neo-cx165-root))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx165_eieio_around_with_call_next_method_through_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:primary (:around-2-enter :primary :around-2-exit))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx165-around () ())
      (let (calls)
        (cl-defgeneric neo-cx165-around-call (obj))
        (cl-defmethod neo-cx165-around-call :around ((obj neo-cx165-around))
          (push :around-1-enter calls)
          (let ((r (cl-call-next-method)))
            (push :around-1-exit calls)
            r))
        (cl-defmethod neo-cx165-around-call :around ((obj neo-cx165-around))
          (push :around-2-enter calls)
          (let ((r (cl-call-next-method)))
            (push :around-2-exit calls)
            r))
        (cl-defmethod neo-cx165-around-call ((obj neo-cx165-around))
          (push :primary calls)
          :primary)
        (let ((result (neo-cx165-around-call (make-instance 'neo-cx165-around))))
          (list result (nreverse calls)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx165_eieio_method_combination_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx165-listc () ())
      (cl-defgeneric neo-cx165-listc-call (obj)
        (:method-combination list))
      (cl-defmethod neo-cx165-listc-call list ((obj neo-cx165-listc)) :a)
      (cl-defmethod neo-cx165-listc-call list ((obj neo-cx165-listc)) :b)
      (cl-defmethod neo-cx165-listc-call list ((obj neo-cx165-listc)) :c)
      (neo-cx165-listc-call (make-instance 'neo-cx165-listc)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx165_eieio_cl_defmethod_with_keyword_argument() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:class :a) (:class :b))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx165-kwarg () ())
      (cl-defgeneric neo-cx165-kw-call (obj &key mode))
      (cl-defmethod neo-cx165-kw-call ((obj neo-cx165-kwarg) &key mode)
        (list :class mode))
      (let ((inst (make-instance 'neo-cx165-kwarg)))
        (list (neo-cx165-kw-call inst :mode :a)
              (neo-cx165-kw-call inst :mode :b))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx165_eieio_method_combination_max() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx165-maxc () ())
      (cl-defgeneric neo-cx165-maxc-call (obj)
        (:method-combination max))
      (cl-defmethod neo-cx165-maxc-call max ((obj neo-cx165-maxc)) 10)
      (cl-defmethod neo-cx165-maxc-call max ((obj neo-cx165-maxc)) 50)
      (cl-defmethod neo-cx165-maxc-call max ((obj neo-cx165-maxc)) 25)
      (neo-cx165-maxc-call (make-instance 'neo-cx165-maxc)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx165_eieio_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx165-mega ()
        ((value :initarg :value :initform 0)))
      (cl-defgeneric neo-cx165-mega-call (obj multiplier))
      (cl-defmethod neo-cx165-mega-call :before ((obj neo-cx165-mega) mult)
        (oset obj value (+ (slot-value obj 'value) mult)))
      (cl-defmethod neo-cx165-mega-call :after ((obj neo-cx165-mega) mult)
        (oset obj value (* (slot-value obj 'value) mult)))
      (cl-defmethod neo-cx165-mega-call ((obj neo-cx165-mega) mult)
        (oset obj value (+ (slot-value obj 'value) mult))
        (slot-value obj 'value))
      (let ((inst (make-instance 'neo-cx165-mega :value 1)))
        (with-temp-buffer
          (buffer-enable-undo)
          (insert "EIEIO mega dispatch test buffer content")
          (put-text-property 1 6 'face 'bold)
          (let ((m (set-marker (make-marker) 8))
                (ov (make-overlay 4 14)))
            (overlay-put ov 'face 'italic)
            (overlay-put ov 'evaporate t)
            (narrow-to-region 2 18)
            (let ((r (neo-cx165-mega-call inst 5)))
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

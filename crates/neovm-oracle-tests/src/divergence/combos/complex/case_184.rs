//! Complex combo batch 184 — `eieio` deep `cl-defmethod` with
//! `&context` (context specializers), `:before`/`:after`/`:around`
//! qualifier chains on generics, and `cl-call-next-method` recursion.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx184_eieio_dispatch_inheritance_chain_call_next() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx184-root () ())
      (defclass neo-cx184-mid (neo-cx184-root) ())
      (defclass neo-cx184-leaf (neo-cx184-mid) ())
      (cl-defgeneric neo-cx184-chain (obj))
      (cl-defmethod neo-cx184-chain ((obj neo-cx184-root))
        (if (next-method-p) (cons :root (cl-call-next-method)) :root))
      (cl-defmethod neo-cx184-chain ((obj neo-cx184-mid))
        (if (next-method-p) (cons :mid (cl-call-next-method)) :mid))
      (cl-defmethod neo-cx184-chain ((obj neo-cx184-leaf))
        (if (next-method-p) (cons :leaf (cl-call-next-method)) :leaf))
      (list (neo-cx184-chain (make-instance 'neo-cx184-leaf))
            (neo-cx184-chain (make-instance 'neo-cx184-mid))
            (neo-cx184-chain (make-instance 'neo-cx184-root))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx184_eieio_qualifier_chain_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:result (:around-enter :before :primary :after :around-exit))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx184-q () ())
      (let (calls)
        (cl-defgeneric neo-cx184-q-call (obj))
        (cl-defmethod neo-cx184-q-call :before ((obj neo-cx184-q)) (push :before calls))
        (cl-defmethod neo-cx184-q-call :around ((obj neo-cx184-q))
          (push :around-enter calls)
          (let ((r (cl-call-next-method)))
            (push :around-exit calls)
            r))
        (cl-defmethod neo-cx184-q-call ((obj neo-cx184-q)) (push :primary calls) :result)
        (cl-defmethod neo-cx184-q-call :after ((obj neo-cx184-q)) (push :after calls))
        (let ((r (neo-cx184-q-call (make-instance 'neo-cx184-q))))
          (list r (nreverse calls)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx184_eieio_no_applicable_method_signal_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:caught-no-applicable)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx184-na () ())
      (cl-defgeneric neo-cx184-na-call (obj))
      (let ((inst (make-instance 'neo-cx184-na)))
        (condition-case err
            (neo-cx184-na-call inst)
          (cl-no-applicable-method (list :caught-no-applicable))
          (no-method (list :caught-no-method))
          (error (list :caught-other (car err))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx184_eieio_method_combination_plus() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx184-mc () ())
      (cl-defgeneric neo-cx184-mc-call (obj) (:method-combination +))
      (cl-defmethod neo-cx184-mc-call + ((obj neo-cx184-mc)) 10)
      (cl-defmethod neo-cx184-mc-call + ((obj neo-cx184-mc)) 20)
      (cl-defmethod neo-cx184-mc-call + ((obj neo-cx184-mc)) 30)
      (neo-cx184-mc-call (make-instance 'neo-cx184-mc)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx184_eieio_method_combination_max() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx184-maxc () ())
      (cl-defgeneric neo-cx184-maxc-call (obj) (:method-combination max))
      (cl-defmethod neo-cx184-maxc-call max ((obj neo-cx184-maxc)) 10)
      (cl-defmethod neo-cx184-maxc-call max ((obj neo-cx184-maxc)) 50)
      (cl-defmethod neo-cx184-maxc-call max ((obj neo-cx184-maxc)) 25)
      (neo-cx184-maxc-call (make-instance 'neo-cx184-maxc)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx184_eieio_eql_specializer_with_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:eql :class :default)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx184-eq () ())
      (cl-defgeneric neo-cx184-eq-call (obj))
      (cl-defmethod neo-cx184-eq-call ((obj (eql :special))) :eql)
      (cl-defmethod neo-cx184-eq-call ((obj neo-cx184-eq)) :class)
      (cl-defmethod neo-cx184-eq-call (obj) :default)
      (list (neo-cx184-eq-call :special)
            (neo-cx184-eq-call (make-instance 'neo-cx184-eq))
            (neo-cx184-eq-call "other")))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx184_eieio_two_argument_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:ab :ba)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx184-a () ())
      (defclass neo-cx184-b () ())
      (cl-defgeneric neo-cx184-bidi (obj1 obj2))
      (cl-defmethod neo-cx184-bidi ((o1 neo-cx184-a) (o2 neo-cx184-b)) :ab)
      (cl-defmethod neo-cx184-bidi ((o1 neo-cx184-b) (o2 neo-cx184-a)) :ba)
      (let ((a (make-instance 'neo-cx184-a))
            (b (make-instance 'neo-cx184-b)))
        (list (neo-cx184-bidi a b)
              (neo-cx184-bidi b a))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx184_eieio_cl_find_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx184-find () ())
      (cl-defgeneric neo-cx184-fm (obj))
      (cl-defmethod neo-cx184-fm :before ((obj neo-cx184-find)) :b)
      (cl-defmethod neo-cx184-fm ((obj neo-cx184-find)) :p)
      (cl-defmethod neo-cx184-fm :after ((obj neo-cx184-find)) :a)
      (let ((methods (cl-generic-methods 'neo-cx184-fm)))
        (list (consp methods)
              (= (length methods) 3))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx184_eieio_class_slots_shared_across_instances() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 5 10 10 :a :b)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx184-static ()
        ((counter :allocation :class :initform 0)
         (tag :initarg :tag)))
      (let ((a (make-instance 'neo-cx184-static :tag :a))
            (b (make-instance 'neo-cx184-static :tag :b)))
        (oset a counter 5)
        (let ((c-a (slot-value a 'counter))
              (c-b (slot-value b 'counter)))
          (oset b counter 10)
          (list c-a c-b
                (slot-value a 'counter)
                (slot-value b 'counter)
                (slot-value a 'tag)
                (slot-value b 'tag)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx184_eieio_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx184-mega ()
        ((value :initarg :value :initform 0)))
      (cl-defgeneric neo-cx184-mega-call (obj mult))
      (cl-defmethod neo-cx184-mega-call :before ((obj neo-cx184-mega) m)
        (oset obj value (+ (slot-value obj 'value) m)))
      (cl-defmethod neo-cx184-mega-call :after ((obj neo-cx184-mega) m)
        (oset obj value (* (slot-value obj 'value) m)))
      (cl-defmethod neo-cx184-mega-call ((obj neo-cx184-mega) m)
        (oset obj value (+ (slot-value obj 'value) m))
        (slot-value obj 'value))
      (let ((inst (make-instance 'neo-cx184-mega :value 1)))
        (with-temp-buffer
          (buffer-enable-undo)
          (insert "EIEIO mega dispatch test buffer content")
          (put-text-property 1 6 'face 'bold)
          (let ((m (set-marker (make-marker) 8))
                (ov (make-overlay 4 14)))
            (overlay-put ov 'face 'italic)
            (overlay-put ov 'evaporate t)
            (narrow-to-region 2 18)
            (let ((r (neo-cx184-mega-call inst 5)))
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

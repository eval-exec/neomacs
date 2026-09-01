/// Batch 481: speedbar, dframe, cedet, semantic, srecode, pulse, inversion.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx481_speedbar_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'speedbar)
  (list (fboundp 'speedbar) (boundp 'speedbar-mode-map)))
"##,
        expect,
    );
}

#[test]
fn div_cx481_dfrane_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'dframe)
  (list (boundp 'dframe-version) (fboundp 'dframe-frame-mode)))
"##,
        expect,
    );
}

#[test]
fn div_cx481_cedet_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'cedet)
  (list (boundp 'cedet-version) (fboundp 'cedet-ede-minor-mode)))
"##,
        expect,
    );
}

#[test]
fn div_cx481_semantic_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'semantic)
  (list (boundp 'semantic-version) (fboundp 'semantic-mode)))
"##,
        expect,
    );
}

#[test]
fn div_cx481_srecode_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'srecode)
  (list (fboundp 'srecode-insert) (boundp 'srecode-version)))
"##,
        expect,
    );
}

#[test]
fn div_cx481_pulse_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'pulse)
  (list (fboundp 'pulse-momentary-highlight-one-line) (boundp 'pulse-version)))
"##,
        expect,
    );
}

#[test]
fn div_cx481_inversion_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'inversion)
  (list (fboundp 'inversion-require-version) (boundp 'inversion-version)))
"##,
        expect,
    );
}

#[test]
fn div_cx481_eieio_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'eieio)
  (list (fboundp 'defclass) (fboundp 'defmethod)))
"##,
        expect,
    );
}

#[test]
fn div_cx481_eieio_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'eieio)
  (defclass neo-cx481-class () ((x :initarg :x)))
  (let ((obj (make-instance 'neo-cx481-class :x 42)))
    (oref obj x)))
"##,
        expect,
    );
}

#[test]
fn div_cx481_eieio_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'eieio)
  (defclass neo-cx481-base () ((val :initarg :val)))
  (cl-defmethod neo-cx481-double ((obj neo-cx481-base))
    (* (oref obj val) 2))
  (let ((obj (make-instance 'neo-cx481-base :val 21)))
    (neo-cx481-double obj)))
"##,
        expect,
    );
}

#[test]
fn div_cx481_eieio_inherit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 neo-cx481-child)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'eieio)
  (defclass neo-cx481-parent () ((p :initarg :p)))
  (defclass neo-cx481-child (neo-cx481-parent) ((c :initarg :c)))
  (let ((obj (make-instance 'neo-cx481-child :p 1 :c 2)))
    (list (oref obj p) (oref obj c) (object-class obj))))
"##,
        expect,
    );
}

#[test]
fn div_cx481_eieio_slot_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 99""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'eieio)
  (defclass neo-cx481-slots () ((x :initarg :x) (y :initarg :y)))
  (let ((obj (make-instance 'neo-cx481-slots :x 5 :y 10)))
    (setf (slot-value obj 'x) 99)
    (slot-value obj 'x)))
"##,
        expect,
    );
}

#[test]
fn div_cx481_eieio_object_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (invalid-read-syntax \"Invalid byte-code object\" 6 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'eieio)
  (defclass neo-cx481-on () ())
  (let ((obj (make-instance 'neo-cx481-on)))
    (stringp (object-name obj)))

#[test]
fn div_cx481_eieio_object_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r##"(progn (require 'eieio)
  (defclass neo-cx481-oc () ())
  (let ((obj (make-instance 'neo-cx481-oc)))
    (list (object-class obj) (object-class-fast obj))))
"##,
        expect,
    );
}

#[test]
fn div_cx481_eieio_same_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument eieio--class #s(neo-cx481-sp) class)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'eieio)
  (defclass neo-cx481-sp () ())
  (let ((o1 (make-instance 'neo-cx481-sp))
        (o2 (make-instance 'neo-cx481-sp)))
    (list (same-class-p o1 o2) (object-of-class-p o1 'neo-cx481-sp))))
"##,
        expect,
    );
}

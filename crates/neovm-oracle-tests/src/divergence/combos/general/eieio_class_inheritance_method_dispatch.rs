//! Deep combo: eieio class + inheritance + method dispatch + slot access.
//! Tests object system basics with class hierarchies.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_defclass_basic_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();

        let expect = expect_test::expect![[r#""OK (10 20 10 20)""#]];
crate::common::assert_oracle_parity_expect(
        "(progn\n\n         (defclass my-point ()\n\n         ((x :initarg :x :initform 0 :accessor get-x)\n\n         (y :initarg :y :initform 0 :accessor get-y)))\n\n         (let ((p (my-point :x 10 :y 20)))\n\n         (list (get-x p) (get-y p)\n\n         (slot-value p 'x) (slot-value p 'y))))", expect);
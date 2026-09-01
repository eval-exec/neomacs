//! EIEIO lifecycle (slot-boundp/makeunbound, clone +overrides, make-instance,
//! initialize-instance :after, eieio-class-slots/name, object-class,
//! slot-exists-p), rx advanced (regexp/literal/group-n/**/backref/>=/in), ewoc
//! create/enter/nth/data, and cl-struct pcase pattern parity.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn cl_struct_pcase() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(cl-defstruct neo-ps-xyz aa bb)
(let ((s (make-neo-ps-xyz :aa 1 :bb 2)))
  (pcase s ((cl-struct neo-ps-xyz aa bb) (list aa bb))))"##,
        expect,
    );
}

#[test]
fn eieio_class_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function eieio-class-object)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'eieio)
(defclass neo-cs-xyz () ((p :initarg :p) (q :initarg :q)))
(list (mapcar #'eieio-slot-descriptor-name (eieio-class-slots 'neo-cs-xyz))
      (eieio-class-name (eieio-class-object 'neo-cs-xyz)))"##,
        expect,
    );
}

#[test]
fn eieio_clone() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 10 99 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'eieio)
(defclass neo-cl-xyz () ((x :initarg :x :accessor neo-x)))
(let* ((o (neo-cl-xyz :x 10)) (c (clone o)) (c2 (clone o :x 99)))
  (list (neo-x o) (neo-x c) (neo-x c2) (eq o c)))"##,
        expect,
    );
}

#[test]
fn eieio_initialize() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 100""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'eieio)
(defclass neo-init-xyz () ((sum :initform 0)))
(cl-defmethod initialize-instance :after ((o neo-init-xyz) &rest _)
  (setf (slot-value o 'sum) 100))
(let ((o (neo-init-xyz))) (slot-value o 'sum))"##,
        expect,
    );
}

#[test]
fn eieio_make_instance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (7 neo-mi-xyz t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'eieio)
(defclass neo-mi-xyz () ((v :initarg :v :initform 0)))
(let ((o (make-instance 'neo-mi-xyz :v 7)))
  (list (slot-value o 'v) (eieio-object-class o) (object-of-class-p o 'neo-mi-xyz)))"##,
        expect,
    );
}

#[test]
fn eieio_print_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t neo-po-xyz 1 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'eieio)
(defclass neo-po-xyz () ((n :initarg :n)))
(let ((o (neo-po-xyz :n 5)))
  (list (eieio-object-p o) (object-class o) (slot-exists-p o 'n) (slot-exists-p o 'z)))"##,
        expect,
    );
}

#[test]
fn eieio_slot_boundp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t eieio--unbound nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'eieio)
(defclass neo-sb-xyz () ((a :initarg :a) (b :initform 5)))
(let ((o (neo-sb-xyz :a 1)))
  (list (slot-boundp o 'a) (slot-boundp o 'b)
        (slot-makeunbound o 'b) (slot-boundp o 'b)))"##,
        expect,
    );
}

#[test]
fn ewoc_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ([[[[[#1 #4 b #<marker in no buffer>] #3 \"\" #<marker in no buffer>] #2 DL-LIST #<marker in no buffer>] #1 \"\" #<marker in no buffer>] [#1 [#2 [#3 [#4 #1 \"\" #<marker in no buffer>] DL-LIST #<marker in no buffer>] \"\" #<marker in no buffer>] b #<marker in no buffer>] a #<marker in no buffer>] a b)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'ewoc)
  (with-temp-buffer
    (let ((e (ewoc-create (lambda (data) (insert (format "%S" data))))))
      (ewoc-enter-last e 'a) (ewoc-enter-last e 'b)
      (list (ewoc-nth e 0) (ewoc-data (ewoc-nth e 0)) (ewoc-data (ewoc-nth e 1)))))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn rx_advanced_constructs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"\\\\(?:[0-9]+\\\\)-a\\\\.b\" \"\\\\(?3:[a-z]\\\\)\" \"x\\\\{2,4\\\\}\" \"\\\\`[[:word:]]+\\\\'\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'rx)
(list (rx (regexp "[0-9]+") "-" (literal "a.b"))
      (rx (group-n 3 (any "a-z")))
      (rx (** 2 4 "x"))
      (rx (seq bos (+ word) eos)))"##,
        expect,
    );
}

#[test]
fn rx_backref_repeat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"\\\\([a-c]\\\\)\\\\1\" \"[[:digit:]]\\\\{3\\\\}\" \"\\\\(?:ab\\\\)\\\\{2,\\\\}\" \"[abx-z]\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'rx)
(list (rx (group (any "a-c")) (backref 1))
      (rx (= 3 digit)) (rx (>= 2 "ab")) (rx (in ?a ?b (?x . ?z))))"##,
        expect,
    );
}

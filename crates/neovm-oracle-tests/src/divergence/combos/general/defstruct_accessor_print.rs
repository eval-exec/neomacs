//! Divergence tests: defstruct + cl-defstruct + accessor + print combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_cl_defstruct_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct (test-ds-xxx (:constructor test-ds-make-xxx))
    (name "unknown")
    (age 0)
    (active t))
  (let ((p1 (test-ds-make-xxx))
        (p2 (test-ds-make-xxx :name "Alice" :age 30)))
    (list (test-ds-xxx-name p1)
          (string= (test-ds-xxx-name p1) "unknown")
          (test-ds-xxx-age p1)
          (= (test-ds-xxx-age p1) 0)
          (test-ds-xxx-active p1)
          (eq (test-ds-xxx-active p1) t)
          (test-ds-xxx-name p2)
          (string= (test-ds-xxx-name p2) "Alice")
          (test-ds-xxx-age p2)
          (= (test-ds-xxx-age p2) 30)))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_defstruct_setf_accessor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct test-mut-xxx (val 0) (items nil))
  (let ((obj (make-test-mut-xxx :val 10 :items '(a b))))
    (setf (test-mut-xxx-val obj) 99)
    (setf (test-mut-xxx-items obj) '(x y z))
    (list (test-mut-xxx-val obj)
          (= (test-mut-xxx-val obj) 99)
          (test-mut-xxx-items obj)
          (equal (test-mut-xxx-items obj) '(x y z))))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_defstruct_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct test-pred-xxx x y)
  (let ((obj (make-test-pred-xxx :x 1 :y 2)))
    (list (test-pred-xxx-p obj)
          (null (test-pred-xxx-p "not a struct"))
          (null (test-pred-xxx-p nil))
          (null (test-pred-xxx-p '(1 2)))
          (test-pred-xxx-p (make-test-pred-xxx))))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_defstruct_copy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct test-copy-xxx (a 1) (b 2))
  (let ((orig (make-test-copy-xxx :a 10 :b 20))
        (other (make-test-copy-xxx :a 30 :b 40)))
    (let ((copy (copy-test-copy-xxx orig)))
      (setf (test-copy-xxx-a copy) 99)
      (list (test-copy-xxx-a orig)
            (= (test-copy-xxx-a orig) 10)
            (test-copy-xxx-a copy)
            (= (test-copy-xxx-a copy) 99)
            (test-copy-xxx-b copy)
            (= (test-copy-xxx-b copy) 20)
            (test-copy-xxx-a other)
            (= (test-copy-xxx-a other) 30))))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_defstruct_named_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct (test-np-xxx (:constructor test-np-new-xxx))
    x y z)
  (let ((obj (test-np-new-xxx :x 1 :y 2 :z 3)))
    (let ((printed (prin1-to-string obj)))
      (list (stringp printed)
            (> (length printed) 0)
            (string-match "test-np" printed)
            (test-np-xxx-x obj)
            (= (test-np-xxx-x obj) 1)
            (test-np-xxx-z obj)
            (= (test-np-xxx-z obj) 3))))) "#,
        expect,
    );
}

#[test]
fn divergence_defstruct_included() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct test-base-xxx a b)
  (cl-defstruct (test-child-xxx (:include test-base-xxx))
    c d)
  (let ((obj (make-test-child-xxx :a 1 :b 2 :c 3 :d 4)))
    (list (test-base-xxx-a obj)
          (= (test-base-xxx-a obj) 1)
          (test-base-xxx-b obj)
          (= (test-base-xxx-b obj) 2)
          (test-child-xxx-c obj)
          (= (test-child-xxx-c obj) 3)
          (test-child-xxx-d obj)
          (= (test-child-xxx-d obj) 4)
          (test-child-xxx-p obj)
          (test-base-xxx-p obj)))) "#,
        expect,
    );
}

#[test]
fn divergence_defstruct_vector_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct (test-vec-xxx (:type vector))
    (x 0) (y 0) (z 0))
  (let ((obj (make-test-vec-xxx :x 1 :y 2 :z 3)))
    (list (vectorp obj)
          (test-vec-xxx-x obj)
          (= (test-vec-xxx-x obj) 1)
          (test-vec-xxx-y obj)
          (= (test-vec-xxx-y obj) 2)
          (aref obj 1)
          (= (aref obj 1) 1)
          (length obj)
          (= (length obj) 3)))) "#,
        expect,
    );
}

#[test]
fn divergence_defstruct_list_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct (test-lst-xxx (:type list))
    (name "default") (value 0))
  (let ((obj (make-test-lst-xxx :name "test" :value 42)))
    (list (listp obj)
          (test-lst-xxx-name obj)
          (string= (test-lst-xxx-name obj) "test")
          (test-lst-xxx-value obj)
          (= (test-lst-xxx-value obj) 42)
          (car obj)
          (string= (car obj) "test")))) "#,
        expect,
    );
}

#[test]
fn divergence_defstruct_boa_constructor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct (test-boa-xxx
                 (:constructor test-boa-make-xxx (a &optional b)))
    (a 0) (b 0) (c 99))
  (let ((o1 (test-boa-make-xxx 10))
        (o2 (test-boa-make-xxx 20 30)))
    (list (test-boa-xxx-a o1)
          (= (test-boa-xxx-a o1) 10)
          (test-boa-xxx-b o1)
          (= (test-boa-xxx-b o1) 0)
          (test-boa-xxx-c o1)
          (= (test-boa-xxx-c o1) 99)
          (test-boa-xxx-a o2)
          (= (test-boa-xxx-a o2) 20)
          (test-boa-xxx-b o2)
          (= (test-boa-xxx-b o2) 30)))) "#,
        expect,
    );
}

#[test]
fn divergence_defstruct_equal_and_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct test-teq-xxx (v 0))
  (let ((o1 (make-test-teq-xxx :v 42))
        (o2 (make-test-teq-xxx :v 42))
        (o3 o1))
    (list (eq o1 o2)
          (null (eq o1 o2))
          (eq o1 o3)
          (equal o1 o2)
          (equal o1 o3)
          (test-teq-xxx-p o1)
          (null (test-teq-xxx-p 42))
          (= (test-teq-xxx-v o1) (test-teq-xxx-v o2))))) "#,
        expect,
    );
}

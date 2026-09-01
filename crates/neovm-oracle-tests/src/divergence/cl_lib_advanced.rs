//! cl-lib advanced divergence probes (calibration).
//!
//! Probes the trickier cl-lib macros/functions: cl-letf, cl-symbol-macrolet,
//! cl-typecase, cl-check-type, cl-assert, cl-rotatef, cl-shiftf, cl-coerce,
//! cl-defstruct (conc-name/constructor/predicate), cl-getf, cl-loop clauses,
//! cl-do, cl-position/find/count, cl-remove-duplicates, cl-sort/stable-sort,
//! cl-subseq, cl-merge, cl-labels, cl-reduce.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cl_letf_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-letf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((x 5))
  (list (cl-letf ((x 10)) x) x))
"##,
        expect,
    );
}

#[test]
fn div_cl_letf_place() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-letf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lst (list 1 2 3)))
  (cl-letf (((nth 1) lst) 99))
  lst)
"##,
        expect,
    );
}

#[test]
fn div_cl_symbol_macrolet() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-symbol-macrolet)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lst (list 1 2 3)))
  (cl-symbol-macrolet ((x (car lst)))
    (setq x 99))
  lst)
"##,
        expect,
    );
}

#[test]
fn div_cl_typecase() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-typecase)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-typecase 5 (string "s") (number "n") (t "o"))
      (cl-typecase "x" (string "s") (number "n") (t "o"))
      (cl-typecase '(1) (string "s") (cons "c") (t "o")))
"##,
        expect,
    );
}

#[test]
fn div_cl_check_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (void-function :passed)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case err (cl-check-type 5 string) (error (car err)))
      (condition-case err (cl-check-type "x" string) (error :passed)))
"##,
        expect,
    );
}

#[test]
fn div_cl_assert_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (void-function :passed)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case err (cl-assert (= 1 2) t "nope") (error (car err)))
      (condition-case err (cl-assert (= 1 1) t "nope") (error :passed)))
"##,
        expect,
    );
}

#[test]
fn div_cl_rotatef() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-rotatef)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a 1) (b 2) (c 3))
  (cl-rotatef a b c)
  (list a b c))
"##,
        expect,
    );
}

#[test]
fn div_cl_shiftf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-shiftf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a 1) (b 2))
  (list (cl-shiftf a b 3) a b))
"##,
        expect,
    );
}

#[test]
fn div_cl_coerce() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-coerce)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-coerce '(1 2 3) 'vector)
      (cl-coerce [1 2 3] 'list)
      (cl-coerce "ab" 'list)
      (cl-coerce '(97 98) 'string))
"##,
        expect,
    );
}

#[test]
fn div_cl_defstruct_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-pt x y)
  (let ((p (make-neo-pt :x 1 :y 2)))
    (list (neo-pt-x p) (neo-pt-y p) (neo-pt-p p))))
"##,
        expect,
    );
}

#[test]
fn div_cl_defstruct_conc_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct (neo-box (:conc-name neo-box-)) size)
  (list (neo-box-size (make-neo-box :size 5))
        (neo-box-p (make-neo-box :size 5))))
"##,
        expect,
    );
}

#[test]
fn div_cl_defstruct_named_constructor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct (neo-coord (:constructor neo-coord-create (a b))) (x a) (y b))
  (let ((c (neo-coord-create 7 8)))
    (list (neo-coord-x c) (neo-coord-y c))))
"##,
        expect,
    );
}

#[test]
fn div_cl_getf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-getf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (list :a 1 :b 2)))
  (list (cl-getf p :a) (cl-getf p :c) (cl-getf p :c :default)))
"##,
        expect,
    );
}

#[test]
fn div_cl_loop_collect_sum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-loop for x in '(1 2 3) collect (* x 2))
      (cl-loop for x in '(1 2 3) sum x)
      (cl-loop for x from 1 to 5 sum x))
"##,
        expect,
    );
}

#[test]
fn div_cl_loop_while_for_equals() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-loop for x in '(1 2 3 4) while (< x 3) collect x)
      (cl-loop for x in '(1 2 3) for y = (* x 2) collect (list x y)))
"##,
        expect,
    );
}

#[test]
fn div_cl_loop_into_maximize() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-loop for x in '(3 1 4 1 5 9 2 6) maximize x into m finally (return m))
"##,
        expect,
    );
}

#[test]
fn div_cl_do() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-do)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-do ((x 0 (1+ x)) (acc nil (push x acc))) ((>= x 3) (reverse acc)))
"##,
        expect,
    );
}

#[test]
fn div_cl_position_find_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-position)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-position 2 '(1 2 3))
      (cl-find 3 '(1 2 3))
      (cl-count nil '(1 nil 2 nil))))
"##,
        expect,
    );
}

#[test]
fn div_cl_remove_duplicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-remove-duplicates)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-remove-duplicates '(1 2 2 3 3 3 1))
"##,
        expect,
    );
}

#[test]
fn div_cl_sort_stable_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-sort)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-sort (copy-sequence '(3 1 2 1 3)) #'<)
      (cl-stable-sort (copy-sequence '((1 . :a) (1 . :b) (2 . :c)))
                      (lambda (a b) (< (car a) (car b)))))
"##,
        expect,
    );
}

#[test]
fn div_cl_subseq() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-subseq)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-subseq '(1 2 3 4 5) 1 3)
      (cl-subseq "hello" 1 4)
      (cl-subseq [1 2 3 4] 2))
"##,
        expect,
    );
}

#[test]
fn div_cl_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-merge)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-merge 'list '(1 3 5) '(2 4 6) #'<)
"##,
        expect,
    );
}

#[test]
fn div_cl_labels_recursive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-labels)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-labels ((fact (n) (if (= n 0) 1 (* n (fact (1- n)))))) (fact 5))
"##,
        expect,
    );
}

#[test]
fn div_cl_reduce() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-reduce)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-reduce #'+ '(1 2 3 4))
      (cl-reduce #'cons '(1 2 3) :from-end t :initial-value 0))
"##,
        expect,
    );
}

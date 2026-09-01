//! Complex combo batch 85 — cl-lib sequence ops & macro helpers: cl-mapcar,
//! cl-map, cl-reduce, cl-some/every/notany/notevery, cl-coerce, cl-merge,
//! cl-stable-sort with predicate, cl-positions, cl-adjoin.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx85_cl_mapcar_and_cl_map_multiple_seqs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-mapcar)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (cl-mapcar #'+ '(1 2 3) '(10 20 30))
 (cl-mapcar #'list '(1 2 3) '(a b c) '("x" "y" "z"))
 (cl-map 'list #'identity '(1 2 3))
 (cl-map 'vector #'+ '(1 2 3) '(10 20 30))
 (cl-map 'string (lambda (n) (+ n ?a -1)) '(1 2 3)))
"##,
        expect,
    );
}

#[test]
fn div_cx85_cl_reduce_with_initial_and_from_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-reduce)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (cl-reduce #'+ '(1 2 3 4 5))
 (cl-reduce #'+ '(1 2 3 4 5) :initial-value 100)
 (cl-reduce #'cons '(1 2 3 4))
 (cl-reduce (lambda (a b) (cons b a)) '(1 2 3 4) :from-end t)
 (cl-reduce #'max '(3 1 4 1 5 9 2 6))
 (cl-reduce #'min '(3 1 4 1 5 9 2 6)))
"##,
        expect,
    );
}

#[test]
fn div_cx85_cl_some_every_notany_notevery() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-some)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((nums '(1 2 3 4 5)))
  (list
   (cl-some #'cl-evenp nums)
   (cl-some (lambda (x) (> x 100)) nums)
   (cl-every #'integerp nums)
   (cl-every (lambda (x) (> x 0)) nums)
   (cl-notany #'cl-oddp '(2 4 6))
   (cl-notevery #'cl-evenp nums)))
"##,
        expect,
    );
}

#[test]
fn div_cx85_cl_coerce_between_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-coerce)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (cl-coerce '(1 2 3) 'vector)
 (cl-coerce [1 2 3] 'list)
 (cl-coerce "abc" 'list)
 (cl-coerce '(97 98 99) 'string)
 (cl-coerce 5 'float)
 (cl-coerce 5 'character)
 (cl-coerce 'foo 'list)
 (cl-coerce '(1 2 3) 'list))
"##,
        expect,
    );
}

#[test]
fn div_cx85_cl_merge_stable_with_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-merge)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (cl-merge 'list '(1 3 5) '(2 4 6) #'<)
 (cl-merge 'list '(1 3 5) '(2 4 6) #'>)
 (cl-merge 'vector '(1 5) '(2 3 4) #'<)
 (cl-merge 'list '() '(1 2 3) #'<)
 (cl-merge 'list '(1 2 3) '() #'<)
 (cl-merge 'list '((1 . "a") (3 . "c")) '((2 . "b")) (lambda (a b) (< (car a) (car b)))))
"##,
        expect,
    );
}

#[test]
fn div_cx85_cl_sort_stable_sort_with_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-sort)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data (copy-sequence '((3 . "c") (1 . "a") (4 . "d") (1 . "e") (5 . "b")))))
  (list
   (cl-sort (copy-sequence data) #'< :key #'car)
   (cl-stable-sort (copy-sequence data) #'< :key #'car)
   (cl-sort (copy-sequence '("apple" "berry" "cherry")) #'string<)
   (cl-sort (copy-sequence '(3 1 4 1 5 9 2 6)) #'<)))
"##,
        expect,
    );
}

#[test]
fn div_cx85_cl_positions_and_find_with_start_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-position)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((nums '(1 2 3 4 3 2 1)))
  (list
   (cl-position 3 nums)
   (cl-position 3 nums :from-end t)
   (cl-position 3 nums :start 4)
   (cl-position 99 nums)
   (cl-find 3 nums)
   (cl-find 99 nums)
   (cl-count 3 nums)))
"##,
        expect,
    );
}

#[test]
fn div_cx85_cl_adjoin_pushnew_with_test_and_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-adjoin)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lst '((1 . "a") (2 . "b"))))
  (list
   (cl-adjoin '(1 . "x") lst :key #'car)
   (cl-adjoin '(3 . "c") lst :key #'car)
   (cl-adjoin 1 '(1 2 3))
   (let ((v '(1 2 3)))
     (cl-pushnew 2 v)
     v)
   (let ((v '(1 2 3)))
     (cl-pushnew 0 v)
     v)
   (let ((v '("apple" "berry")))
     (cl-pushnew "APPLE" v :test #'string-equal)
     v)))
"##,
        expect,
    );
}

#[test]
fn div_cx85_cl_subseq_setf_on_list_vector_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-subseq)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v (vector 1 2 3 4 5))
      (s "hello"))
  (list
   (cl-subseq v 1 3)
   (cl-subseq s 1 3)
   (setf (cl-subseq v 1 3) [99 88])
   v
   (setf (cl-subseq s 0 2) "XX")
   s))
"##,
        expect,
    );
}

#[test]
fn div_cx85_cl_copy_list_copy_seq_copy_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function copy-list)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((l '(1 (2 3) (4 5)))
       (v [1 2 3])
       (tree-l (copy-tree l))
       (copy-l (copy-list l))
       (copy-v (copy-sequence v)))
  (setf (car (cadr l)) 99)
  (aset v 0 99)
  (list l tree-l copy-l v copy-v
        (eq l copy-l)
        (eq l tree-l)
        (eq v copy-v)))
"##,
        expect,
    );
}

#[test]
fn div_cx85_cl_set_difference_union_intersection_with_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-union)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a '((1 . "a") (2 . "b") (3 . "c")))
      (b '((2 . "x") (4 . "y"))))
  (list
   (sort (cl-union a b :key #'car) (lambda (x y) (< (car x) (car y))))
   (sort (cl-intersection a b :key #'car) (lambda (x y) (< (car x) (car y))))
   (sort (cl-set-difference a b :key #'car) (lambda (x y) (< (car x) (car y))))))
"##,
        expect,
    );
}

#[test]
fn div_cx85_cl_associated_rassoc_assq_with_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-find-if)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((alist '((a . 1) (b . 2) ("str" . 3) (nil . 4))))
  (list
   (assoc 'a alist)
   (assq 'b alist)
   (assoc "str" alist)
   (rassoc 2 alist)
   (assoc 'missing alist)
   (cl-find-if (lambda (cell) (= (cdr cell) 3)) alist)))
"##,
        expect,
    );
}

#[test]
fn div_cx85_cl_seq_ops_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-sort)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((items '("alpha" "beta" "gamma" "delta" "epsilon")))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (mapconcat #'identity items "\n"))
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 20)
      (let* ((result
              (cl-sort (copy-sequence items)
                       (lambda (a b) (< (length a) (length b)))))
             (state (list result
                          (buffer-string)
                          (marker-position m)
                          (overlay-start ov) (overlay-end ov)
                          (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}

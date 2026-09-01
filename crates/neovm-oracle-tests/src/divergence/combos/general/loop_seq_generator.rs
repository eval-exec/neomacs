//! Divergence tests: cl-loop + seq + generator + lazy sequence combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_cl_loop_for_across_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((v [10 20 30 40 50]))
    (list (cl-loop for x across v sum x)
          (= (cl-loop for x across v sum x) 150)
          (cl-loop for x across v collect (* x 2))
          (equal (cl-loop for x across v collect (* x 2))
                 '(20 40 60 80 100))
          (cl-loop for i from 0 below (length v) collect (aref v i))
          (equal (cl-loop for i from 0 below (length v) collect (aref v i))
                 '(10 20 30 40 50))))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_loop_with_hash_tables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ht (make-hash-table :test 'equal)))
    (puthash 'a 1 ht)
    (puthash 'b 2 ht)
    (puthash 'c 3 ht)
    (let ((sum (cl-loop for k being the hash-keys of ht
                        using (hash-values v)
                        sum v))
          (keys (cl-loop for k being the hash-keys of ht collect k)))
      (list sum
            (= sum 6)
            (length keys)
            (= (length keys) 3)
            (hash-table-count ht)
            (= (hash-table-count ht) 3))))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_loop_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((data '((1 2) (3 4) (5 6) (7 8))))
    (list (cl-loop for (a b) in data sum (+ a b))
          (= (cl-loop for (a b) in data sum (+ a b)) 36)
          (cl-loop for (a b) in data collect (* a b))
          (equal (cl-loop for (a b) in data collect (* a b))
                 '(2 12 30 56))
          (cl-loop for (a . rest) in data collect (list a rest))
          (equal (cl-loop for (a . rest) in data collect (list a rest))
                 '((1 (2)) (3 (4)) (5 (6)) (7 (8))))))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_loop_accumulation_into() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (cl-loop for x from 1 to 10
                 sum x into total
                 maximize x into max
                 minimize x into min
                 count (cl-oddp x) into odds
                 finally return (list total max min odds))
        (equal (cl-loop for x from 1 to 10
                        sum x into total
                        maximize x into max
                        minimize x into min
                        count (cl-oddp x) into odds
                        finally return (list total max min odds))
               '(55 10 1 5)))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_loop_while_until() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (cl-loop for x from 1
                 while (< x 100)
                 when (cl-evenp x) sum x)
        (= (cl-loop for x from 1
                    while (< x 100)
                    when (cl-evenp x) sum x) 2450)
        (cl-loop for x from 1
                 until (> x 10)
                 collect x)
        (equal (cl-loop for x from 1
                        until (> x 10)
                        collect x)
               '(1 2 3 4 5 6 7 8 9 10))
        (cl-loop for x in '(1 2 3 4 5 6 7 8 9 10)
                 thereis (when (= x 5) x))
        (= (cl-loop for x in '(1 2 3 4 5 6 7 8 9 10)
                    thereis (when (= x 5) x)) 5)
        (cl-loop for x in '(1 2 3 4 5) always (< x 10))
        (cl-loop for x in '(1 2 3 4 5) never (> x 10)))) "#,
        expect,
    );
}

#[test]
fn divergence_seq_group_by_partition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-evenp)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((data '(1 2 3 4 5 6 7 8 9 10)))
    (let ((by-even (seq-group-by (lambda (x) (cl-evenp x)) data))
          (partitioned (seq-partition data 3)))
      (list (length by-even)
            (= (length by-even) 2)
            (length (cdr (assq t by-even)))
            (= (length (cdr (assq t by-even))) 5)
            (length partitioned)
            (= (length partitioned) 4)
            (car (car partitioned))
            (= (car (car partitioned)) 1)
            (length (car (last partitioned)))
            (= (length (car (last partitioned))) 1))))) "#,
        expect,
    );
}

#[test]
fn divergence_seq_sort_unique_contains() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function seq-unique)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((data '(3 1 4 1 5 9 2 6 5 3)))
    (let ((sorted (seq-sort #'< data))
          (unique (seq-unique data)))
      (list sorted
            (equal sorted '(1 1 2 3 3 4 5 5 6 9))
            unique
            (= (length unique) 7)
            (seq-contains data 5)
            (= (seq-contains data 5) 5)
            (seq-position data 9)
            (= (seq-position data 9) 7)
            (seq-drop data 3)
            (equal (seq-drop data 3) '(1 5 9 2 6 5 3)))))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_loop_for_on_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((lst '(a b c d e)))
    (list (cl-loop for sublist on lst collect (car sublist))
          (equal (cl-loop for sublist on lst collect (car sublist))
                 '(a b c d e))
          (cl-loop for sublist on lst by 'cddr collect (car sublist))
          (equal (cl-loop for sublist on lst by 'cddr collect (car sublist))
                 '(a c e))
          (cl-loop for sublist on lst count t)
          (= (cl-loop for sublist on lst count t) 5)
          (cl-loop for x in lst for i from 1 collect (cons i x))
          (equal (cl-loop for x in lst for i from 1 collect (cons i x))
                 '((1 . a) (2 . b) (3 . c) (4 . d) (5 . e)))))) "#,
        expect,
    );
}

#[test]
fn divergence_seq_map_indexed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((0 10) (1 20) (2 30) (3 40)) t (40 30 20 10) t 10 t [20 30 40] t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((v [10 20 30 40]))
    (list (seq-map-indexed (lambda (elt idx) (list idx elt)) v)
          (equal (seq-map-indexed (lambda (elt idx) (list idx elt)) v)
                 '((0 10) (1 20) (2 30) (3 40)))
          (seq-reduce (lambda (acc x) (cons x acc)) v nil)
          (equal (seq-reduce (lambda (acc x) (cons x acc)) v nil)
                 '(40 30 20 10))
          (seq-first v)
          (= (seq-first v) 10)
          (seq-rest v)
          (equal (seq-rest v) [20 30 40])))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_loop_nested_for() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (cl-loop for x from 1 to 3
                 append (cl-loop for y from 1 to 3
                                 collect (* x y)))
        (= (length (cl-loop for x from 1 to 3
                            append (cl-loop for y from 1 to 3
                                            collect (* x y))))
           9)
        (cl-loop for x from 1 to 3
                 nconc (list x (* x x)))
        (equal (cl-loop for x from 1 to 3
                        nconc (list x (* x x)))
               '(1 1 2 4 3 9))
        (cl-loop repeat 5 for x = (random 100) collect x)
        (= (length (cl-loop repeat 5 for x = (random 100) collect x)) 5))) "#,
        expect,
    );
}

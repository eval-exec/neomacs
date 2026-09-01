//! Divergence tests: cl-loop + seq + accumulation + hash table combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_cl_loop_multi_accumulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((data '((a 1) (b 2) (c 3) (d 4) (e 5))))
    (cl-loop for (key val) in data
             sum val into total
             maximize val into max-val
             minimize val into min-val
             collect key into keys
             count (cl-oddp val) into odd-count
             finally return (list total max-val min-val
                                  (length keys) odd-count
                                  (= total 15)
                                  (= max-val 5)
                                  (= min-val 1)
                                  (= (length keys) 5)
                                  (= odd-count 3))))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_loop_with_hash_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ht (make-hash-table :test 'equal)))
    (puthash 'x 10 ht)
    (puthash 'y 20 ht)
    (puthash 'z 30 ht)
    (let ((sum (cl-loop for k being the hash-keys of ht
                        using (hash-values v)
                        sum v))
          (pairs (cl-loop for k being the hash-keys of ht
                          using (hash-values v)
                          collect (cons k v))))
      (list sum
            (= sum 60)
            (length pairs)
            (= (length pairs) 3)
            (assoc 'x pairs)
            (equal (assoc 'x pairs) '(x . 10)))))) "#,
        expect,
    );
}

#[test]
fn divergence_seq_into_different_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 13 64)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((lst '(1 2 3 4 5))
        (vec [1 2 3 4 5]))
    (list (seq-into lst 'vector)
          (equal (seq-into lst 'vector) [1 2 3 4 5])
          (seq-into vec 'list)
          (equal (seq-into vec 'list) '(1 2 3 4 5))
          (seq-into lst 'string)
          (string= (seq-into lst 'string) "\x01\x02\x03\x04\x05")
          (length (seq-into lst 'vector))
          (= (length (seq-into lst 'vector)) 5)
          (seq-into "hello" 'list)
          (equal (seq-into "hello" 'list) '(?h ?e ?l ?l ?o)))))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_loop_with_vectors_and_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((v [10 20 30 40 50])
        (s "Hello World"))
    (list (cl-loop for c across s count (char-equal c ?l))
          (= (cl-loop for c across s count (char-equal c ?l)) 3)
          (cl-loop for x across v sum x)
          (= (cl-loop for x across v sum x) 150)
          (cl-loop for i from 0 below (length v)
                   collect (aref v i))
          (equal (cl-loop for i from 0 below (length v)
                          collect (aref v i))
                 '(10 20 30 40 50))))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_loop_for_in_package() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((syms nil))
    (cl-loop for s being the symbols
             when (and (fboundp s)
                       (string-match "^car$" (symbol-name s)))
             collect s into matched
             finally (setq syms matched))
    (list (length syms)
          (>= (length syms) 1)
          (member 'car syms)
          (every (lambda (s) (fboundp s)) syms)))) #"#,
        expect,
    );
}

#[test]
fn divergence_seq_mapcat_concatenate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument characterp \"a\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((lists '((1 2) (3 4) (5 6))))
    (list (seq-mapcat #'identity lists)
          (equal (seq-mapcat #'identity lists) '(1 2 3 4 5 6))
          (seq-concatenate 'vector (seq-mapcat #'identity lists))
          (equal (seq-concatenate 'vector (seq-mapcat #'identity lists))
                 [1 2 3 4 5 6])
          (seq-concatenate 'string (seq-map (lambda (x) (string x))
                                            '(?a ?b ?c)))
          (string= (seq-concatenate 'string (seq-map (lambda (x) (string x))
                                                      '(?a ?b ?c)))
                   "abc")))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_loop_with_multiple_for_clauses() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (cl-loop for x from 1 to 5
                 for y = (* x x)
                 collect (list x y))
        (equal (cl-loop for x from 1 to 5
                        for y = (* x x)
                        collect (list x y))
               '((1 1) (2 4) (3 9) (4 16) (5 25)))
        (cl-loop for x in '(a b c d)
                 for i from 1
                 collect (cons i x))
        (equal (cl-loop for x in '(a b c d)
                        for i from 1
                        collect (cons i x))
               '((1 . a) (2 . b) (3 . c) (4 . d))))) "#,
        expect,
    );
}

#[test]
fn divergence_seq_window_slide() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK ([2 3 4] t [4 5] t [1 2 3] t [3 4 5] t [3 4 5] t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((v [1 2 3 4 5]))
    (list (seq-subseq v 1 4)
          (equal (seq-subseq v 1 4) [2 3 4])
          (seq-subseq v 3)
          (equal (seq-subseq v 3) [4 5])
          (seq-take v 3)
          (equal (seq-take v 3) [1 2 3])
          (seq-drop v 2)
          (equal (seq-drop v 2) [3 4 5])
          (seq-drop-while (lambda (x) (< x 3)) v)
          (equal (seq-drop-while (lambda (x) (< x 3)) v) [3 4 5])))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_loop_with_conditionals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 20 32)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((data (number-sequence 1 20)))
    (list (cl-loop for x in data
                   when (and (cl-evenp x) (> x 10))
                   collect x)
          (equal (cl-loop for x in data
                          when (and (cl-evenp x) (> x 10))
                          collect x)
                 '(12 14 16 18 20))
          (cl-loop for x in data
                   if (cl-oddp x) sum x into odd-sum
                   else sum x into even-sum
                   end
                   finally return (list odd-sum even-sum))
          (equal (cl-loop for x in data
                          if (cl-oddp x) sum x into odd-sum
                          else sum x into even-sum
                          end
                          finally return (list odd-sum even-sum))
                 '(100 110)))) #"#,
        expect,
    );
}

#[test]
fn divergence_seq_intersection_difference() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 14 31)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((a '(1 2 3 4 5))
        (b '(4 5 6 7 8)))
    (list (seq-intersection a b)
          (equal (seq-intersection a b) '(4 5))
          (seq-difference a b)
          (equal (seq-difference a b) '(1 2 3))
          (seq-difference b a)
          (equal (seq-difference b a) '(6 7 8))
          (seq-union a b)
          (= (length (seq-union a b)) 8)
          (seq-keep (lambda (x) (when (> x 3) (* x 10))) a)
          (equal (seq-keep (lambda (x) (when (> x 3) (* x 10))) a)
                 '(40 50))))) #"#,
        expect,
    );
}

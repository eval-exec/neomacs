//! Oracle parity tests for general sequence operations:
//! `seq-elt`, `seq-length`, `seq-map`, `seq-filter`,
//! `seq-reduce`, `seq-find`, `seq-contains-p`, `seq-remove`,
//! `seq-uniq`, `seq-count`, `seq-some`, `seq-every-p`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// seq-elt (generic element access)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_elt() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (seq-elt '(a b c d) 0)
                        (seq-elt '(a b c d) 2)
                        (seq-elt [10 20 30 40] 1)
                        (seq-elt [10 20 30 40] 3)
                        (seq-elt "hello" 0)
                        (seq-elt "hello" 4))"#;
    let expect = expect_test::expect![[r#""OK (a c 20 40 104 111)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-length
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (seq-length nil)
                        (seq-length '(a b c))
                        (seq-length [1 2 3 4])
                        (seq-length "hello"))"#;
    let expect = expect_test::expect![[r#""OK (0 3 4 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-map
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_map_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (seq-map #'1+ '(1 2 3 4 5))
                        (seq-map #'upcase '("a" "b" "c"))
                        (seq-map #'numberp '(1 "two" 3 nil)))"#;
    let expect = expect_test::expect![[r#""OK ((2 3 4 5 6) (\"A\" \"B\" \"C\") (t nil t nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_seq_map_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (seq-map #'1+ [10 20 30])
                        (seq-map (lambda (x) (* x x)) [1 2 3 4 5]))"#;
    let expect = expect_test::expect![[r#""OK ((11 21 31) (1 4 9 16 25))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-filter / seq-remove
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (seq-filter #'numberp '(1 "a" 2 nil 3 t))
                        (seq-filter (lambda (x) (> x 3)) '(1 2 3 4 5 6))
                        (seq-filter #'stringp '(1 "hello" 2 "world")))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3) (4 5 6) (\"hello\" \"world\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_seq_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (seq-remove #'numberp '(1 "a" 2 nil 3 t))
                        (seq-remove (lambda (x) (> x 3)) '(1 2 3 4 5 6)))"#;
    let expect = expect_test::expect![[r#""OK ((\"a\" nil t) (1 2 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-reduce (fold)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_reduce() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (seq-reduce #'+ '(1 2 3 4 5) 0)
                        (seq-reduce #'* '(1 2 3 4 5) 1)
                        (seq-reduce #'max '(3 1 4 1 5 9) 0)
                        (seq-reduce (lambda (acc x)
                                      (cons x acc))
                                    '(a b c d) nil))"#;
    let expect = expect_test::expect![[r#""OK (15 120 9 (d c b a))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-find
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_find() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (seq-find #'numberp '("a" "b" 42 "c"))
                        (seq-find (lambda (x) (> x 10)) '(1 5 15 3))
                        (seq-find #'null '(1 2 nil 3))
                        (seq-find #'stringp '(1 2 3))
                        (seq-find #'stringp '(1 2 3) 'not-found))"#;
    let expect = expect_test::expect![[r#""OK (42 15 nil nil not-found)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-some / seq-every-p
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_some_every() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (seq-some #'numberp '(1 2 3))
                        (seq-some #'stringp '(1 2 3))
                        (seq-some #'numberp '("a" 1 "b"))
                        (seq-every-p #'numberp '(1 2 3))
                        (seq-every-p #'numberp '(1 "a" 3))
                        (seq-every-p #'stringp '("a" "b" "c")))"#;
    let expect = expect_test::expect![[r#""OK (t nil t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-uniq
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_uniq() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (seq-uniq '(1 2 3 2 1 4 3 5))
                        (seq-uniq '(a a b b c c))
                        (seq-uniq nil)
                        (seq-uniq '(solo)))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5) (a b c) nil (solo))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-count
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (seq-count #'numberp '(1 "a" 2 nil 3))
                        (seq-count (lambda (x) (> x 3)) '(1 2 3 4 5 6))
                        (seq-count #'null '(nil nil 1 nil 2)))"#;
    let expect = expect_test::expect![[r#""OK (3 3 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: data analysis pipeline using seq-*
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_analysis_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Pipeline: filter, transform, reduce, analyze
    let form = r#"(let ((data '((alice 30 eng 90)
                                 (bob 25 qa 85)
                                 (carol 35 eng 95)
                                 (dave 28 qa 80)
                                 (eve 32 eng 88))))
                    ;; Only engineers over 30
                    (let ((senior-eng
                           (seq-filter
                            (lambda (r)
                              (and (eq (nth 2 r) 'eng)
                                   (> (nth 1 r) 30)))
                            data)))
                      ;; Extract scores
                      (let ((scores (seq-map (lambda (r) (nth 3 r))
                                             senior-eng)))
                        (list
                         (length senior-eng)
                         scores
                         (seq-reduce #'+ scores 0)
                         (/ (float (seq-reduce #'+ scores 0))
                            (length scores))
                         (seq-every-p (lambda (s) (> s 85)) scores)
                         (seq-some (lambda (s) (> s 90)) scores)))))"#;
    let expect = expect_test::expect![[r#""OK (2 (95 88) 183 91.5 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: seq-based set operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((a '(1 2 3 4 5))
                        (b '(3 4 5 6 7)))
                    ;; Union
                    (let ((union (append a
                                         (seq-remove
                                          (lambda (x) (memq x a))
                                          b))))
                      ;; Intersection
                      (let ((inter (seq-filter
                                    (lambda (x) (memq x b))
                                    a)))
                        ;; Difference a - b
                        (let ((diff (seq-remove
                                     (lambda (x) (memq x b))
                                     a)))
                          (list (sort union #'<)
                                (sort inter #'<)
                                (sort diff #'<)
                                (seq-count (lambda (x) (memq x b)) a))))))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5 6 7) (3 4 5) (1 2) 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

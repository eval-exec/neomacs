//! Oracle parity tests for `vector-or-char-table-p` predicate and
//! polymorphic operations that dispatch on vector vs char-table type.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic predicate on various types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_or_char_table_p_basic_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (vector-or-char-table-p [1 2 3])
  (vector-or-char-table-p (make-char-table 'generic))
  (vector-or-char-table-p "hello")
  (vector-or-char-table-p '(1 2 3))
  (vector-or-char-table-p nil)
  (vector-or-char-table-p 42)
  (vector-or-char-table-p 'foo)
  (vector-or-char-table-p (make-vector 0 nil))
  (vector-or-char-table-p (make-char-table 'generic 0)))"#;
    let expect = expect_test::expect![[r#""OK (t t nil nil nil nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Predicate combined with sequencep, vectorp, char-table-p
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_or_char_table_p_vs_other_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((objects (list [1 2] (make-char-table 'generic) "str" '(a b) nil 99)))
  (mapcar (lambda (obj)
            (list (vector-or-char-table-p obj)
                  (vectorp obj)
                  (char-table-p obj)
                  (sequencep obj)
                  (arrayp obj)))
          objects))"#;
    let expect = expect_test::expect![[
        r#""OK ((t t nil t t) (t nil t t t) (nil nil nil t t) (nil nil nil t nil) (nil nil nil t nil) (nil nil nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Using predicate in conditional dispatch
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_or_char_table_p_type_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-voct-classify
    (lambda (obj)
      (cond
        ((not (vector-or-char-table-p obj))
         'neither)
        ((vectorp obj)
         (cons 'vector (length obj)))
        ((char-table-p obj)
         'char-table)
        (t 'unknown))))
  (unwind-protect
      (list
        (funcall 'neovm--test-voct-classify [10 20 30])
        (funcall 'neovm--test-voct-classify (make-char-table 'generic))
        (funcall 'neovm--test-voct-classify "not-array")
        (funcall 'neovm--test-voct-classify nil)
        (funcall 'neovm--test-voct-classify (make-vector 7 'x)))
    (fmakunbound 'neovm--test-voct-classify)))"#;
    let expect =
        expect_test::expect![[r#""OK ((vector . 3) char-table neither neither (vector . 7))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Polymorphic aref/aset on both vectors and char-tables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_or_char_table_p_polymorphic_aref_aset() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // aref/aset work on both vectors and char-tables; build a function
    // that populates and reads from whichever is passed in.
    let form = r#"(progn
  (fset 'neovm--test-voct-fill-and-read
    (lambda (container keys vals)
      (let ((i 0))
        (while (< i (length keys))
          (aset container (nth i keys) (nth i vals))
          (setq i (1+ i))))
      (mapcar (lambda (k) (aref container k)) keys)))

  (unwind-protect
      (let* ((vec (make-vector 10 nil))
             (ct  (make-char-table 'generic nil))
             (vec-keys '(0 3 7 9))
             (vec-vals '(alpha beta gamma delta))
             (ct-keys  (list ?a ?m ?z ?A))
             (ct-vals  '(lower-a lower-m lower-z upper-A)))
        (list
          (funcall 'neovm--test-voct-fill-and-read vec vec-keys vec-vals)
          (funcall 'neovm--test-voct-fill-and-read ct ct-keys ct-vals)
          ;; Verify unset positions are still nil
          (list (aref vec 1) (aref vec 5) (aref ct ?b) (aref ct ?Z))))
    (fmakunbound 'neovm--test-voct-fill-and-read)))"#;
    let expect = expect_test::expect![[
        r#""OK ((alpha beta gamma delta) (lower-a lower-m lower-z upper-A) (nil nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: polymorphic frequency counter over vector or char-table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_or_char_table_p_frequency_counter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a generic "frequency table" that works on both vectors (index by int)
    // and char-tables (index by char). Stores counts, retrieves them, finds max.
    let form = r#"(progn
  (fset 'neovm--test-voct-incr
    (lambda (tbl key)
      (let ((cur (aref tbl key)))
        (aset tbl key (1+ (or cur 0))))))

  (fset 'neovm--test-voct-count-elements
    (lambda (tbl elements key-fn)
      (dolist (e elements)
        (funcall 'neovm--test-voct-incr tbl (funcall key-fn e)))
      tbl))

  (fset 'neovm--test-voct-lookup-many
    (lambda (tbl keys)
      (mapcar (lambda (k) (cons k (or (aref tbl k) 0))) keys)))

  (unwind-protect
      (let* (;; Char-table: count character frequencies in a string
             (ct (make-char-table 'generic 0))
             (text "abracadabra")
             (_ (let ((i 0))
                  (while (< i (length text))
                    (funcall 'neovm--test-voct-incr ct (aref text i))
                    (setq i (1+ i)))))
             (ct-result (funcall 'neovm--test-voct-lookup-many
                                 ct (list ?a ?b ?r ?c ?d ?z)))
             ;; Vector: count small-integer frequencies
             (vec (make-vector 20 0))
             (data '(3 7 3 3 12 7 0 0 3 12 12 19))
             (_ (funcall 'neovm--test-voct-count-elements
                         vec data (lambda (x) x)))
             (vec-result (funcall 'neovm--test-voct-lookup-many
                                  vec '(0 3 7 12 19 1)))
             ;; Combined: verify both are vector-or-char-table-p
             (pred-check (list (vector-or-char-table-p ct)
                               (vector-or-char-table-p vec)
                               (vector-or-char-table-p data))))
        (list ct-result vec-result pred-check))
    (fmakunbound 'neovm--test-voct-incr)
    (fmakunbound 'neovm--test-voct-count-elements)
    (fmakunbound 'neovm--test-voct-lookup-many)))"#;
    let expect = expect_test::expect![[
        r#""OK (((97 . 5) (98 . 2) (114 . 2) (99 . 1) (100 . 1) (122 . 0)) ((0 . 2) (3 . 4) (7 . 2) (12 . 3) (19 . 1) (1 . 0)) (t t nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested: vector containing char-tables and vice versa
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_or_char_table_p_nested_containers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((ct1 (make-char-table 'generic nil))
       (ct2 (make-char-table 'generic nil))
       (_ (set-char-table-range ct1 ?x 'found-x))
       (_ (set-char-table-range ct2 ?y 'found-y))
       (vec-of-ct (vector ct1 ct2))
       ;; Navigate: vector -> char-table -> value
       (r1 (aref (aref vec-of-ct 0) ?x))
       (r2 (aref (aref vec-of-ct 1) ?y))
       (r3 (aref (aref vec-of-ct 0) ?y))
       ;; Predicates on the nested structure
       (p1 (vector-or-char-table-p vec-of-ct))
       (p2 (vector-or-char-table-p (aref vec-of-ct 0)))
       (p3 (vectorp vec-of-ct))
       (p4 (char-table-p (aref vec-of-ct 1))))
  (list r1 r2 r3 p1 p2 p3 p4))"#;
    let expect = expect_test::expect![[r#""OK (found-x found-y nil t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

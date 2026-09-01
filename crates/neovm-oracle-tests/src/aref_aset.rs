//! Oracle parity tests for `aref`, `aset`, `fillarray` on vectors,
//! strings, and bool-vectors.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// aref on vectors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_aref_vector_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((v [10 20 30 40 50]))
                    (list (aref v 0) (aref v 2) (aref v 4)))"#;
    let expect = expect_test::expect![r#""OK (10 30 50)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_aref_vector_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Nested vectors: matrix access
    let form = r#"(let ((m (vector (vector 1 2 3)
                                  (vector 4 5 6)
                                  (vector 7 8 9))))
                    (list (aref (aref m 0) 0)
                          (aref (aref m 1) 1)
                          (aref (aref m 2) 2)
                          (aref (aref m 0) 2)
                          (aref (aref m 2) 0)))"#;
    let expect = expect_test::expect![r#""OK (1 5 9 3 7)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_aref_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // aref on string returns character code
    let form = r#"(let ((s "Hello"))
                    (list (aref s 0) (aref s 1) (aref s 4)
                          (= (aref s 0) ?H)
                          (= (aref s 4) ?o)))"#;
    let expect = expect_test::expect![r#""OK (72 101 111 t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_aref_string_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multibyte string: aref returns character (not byte)
    let form = r#"(let ((s "café"))
                    (list (length s)
                          (aref s 0)
                          (aref s 3)
                          (char-to-string (aref s 3))))"#;
    let expect = expect_test::expect![[r#""OK (4 99 233 \"é\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// aset on vectors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_aset_vector_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((v (vector 1 2 3 4 5)))
                    (aset v 0 99)
                    (aset v 2 77)
                    (aset v 4 55)
                    (list (aref v 0) (aref v 1) (aref v 2)
                          (aref v 3) (aref v 4)))"#;
    let expect = expect_test::expect![r#""OK (99 2 77 4 55)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_aset_vector_mixed_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Vectors can hold mixed types
    let form = r#"(let ((v (make-vector 5 nil)))
                    (aset v 0 42)
                    (aset v 1 "hello")
                    (aset v 2 '(a b c))
                    (aset v 3 3.14)
                    (aset v 4 t)
                    (list (aref v 0) (aref v 1) (aref v 2)
                          (aref v 3) (aref v 4)))"#;
    let expect = expect_test::expect![[r#""OK (42 \"hello\" (a b c) 3.14 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_aset_return_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // aset returns the value that was stored
    let form = r#"(let ((v (vector 0 0 0)))
                    (list (aset v 0 'alpha)
                          (aset v 1 42)
                          (aset v 2 '(x y))
                          v))"#;
    let expect = expect_test::expect![r#""OK (alpha 42 (x y) [alpha 42 (x y)])""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_aset_checks_index_before_array_type_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU data.c:Faset starts with CHECK_FIXNUM (idx), then checks ARRAY.
    // When both ARRAY and IDX are invalid, the observable signal must be the
    // index type error, not the array type error.
    let form = r#"(aset 42 'not-a-fixnum 'value)"#;
    let expect = expect_test::expect![r#""ERR (wrong-type-argument fixnump not-a-fixnum)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_aref_aset_record_and_bool_vector_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU data.c:Faref supports records and bool-vectors.  GNU data.c:Faset
    // also has an explicit RECORDP branch and stores truth values in
    // bool-vectors while still returning the original NEWELT object.
    let form = r#"(let ((record (record 'neovm--test-record 'a 'b))
      (bits (make-bool-vector 4 nil)))
  (list
   (aref record 0)
   (aref record 1)
   (aref record 2)
   (aset record 2 'changed)
   (aref record 2)
   record
   (aref bits 0)
   (aset bits 1 42)
   (aref bits 1)
   (aset bits 2 nil)
   (aref bits 2)
   (eq (fillarray bits t) bits)
   (list (aref bits 0) (aref bits 1) (aref bits 2) (aref bits 3))
   (eq (fillarray bits nil) bits)
   (list (aref bits 0) (aref bits 1) (aref bits 2) (aref bits 3))
   (condition-case err
       (aset record -1 'bad)
     (error (list (car err) (cdr err))))
   (condition-case err
       (aref bits 4)
     (error (list (car err) (cdr err))))))"#;
    let expect = expect_test::expect![[
        r#""OK (neovm--test-record a b changed changed #s(neovm--test-record a changed) nil 42 t nil nil t (t t t t) t (nil nil nil nil) (args-out-of-range (#s(neovm--test-record a changed) -1)) (args-out-of-range (#&4\"\\0\" 4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_aref_aset_byte_code_and_validation_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/data.c:Faref accepts byte-code objects through CLOSUREP, but
    // Faset first checks IDX and then CHECK_ARRAY, so byte-code objects are
    // not mutable arrays and an invalid index type wins over array type.
    let form = r#"
(let ((bc #[257 "\300\207" [42] 1]))
  (list
   (type-of bc)
   (length bc)
   (aref bc 0)
   (aref bc 2)
   (aref bc 3)
   (condition-case err
       (aref 42 'not-a-fixnum)
     (error (list (car err) (cdr err))))
   (condition-case err
       (aset bc 0 'changed)
     (error (list (car err) (cdr err))))
   (condition-case err
       (aset bc 'not-a-fixnum 'changed)
     (error (list (car err) (cdr err))))
   (condition-case err
       (aref bc 99)
     (error (list (car err) (cdr err))))))"#;

    let expect = expect_test::expect![[
        r#""OK (byte-code-function 4 257 [42] 1 (wrong-type-argument (fixnump not-a-fixnum)) (wrong-type-argument (arrayp #[257 \"��\" [42] 1])) (wrong-type-argument (fixnump not-a-fixnum)) (args-out-of-range (#[257 \"��\" [42] 1] 99)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// aset on strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_aset_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((s (copy-sequence "hello")))
                    (aset s 0 ?H)
                    (aset s 4 ?O)
                    s)"#;
    let expect = expect_test::expect![[r#""OK \"HellO\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_aset_string_build_alphabet() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build an alphabet string with aset
    let form = r#"(let ((s (make-string 26 ?_)))
                    (let ((i 0))
                      (while (< i 26)
                        (aset s i (+ ?a i))
                        (setq i (1+ i))))
                    s)"#;
    let expect = expect_test::expect![[r#""OK \"abcdefghijklmnopqrstuvwxyz\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// fillarray
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fillarray_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((v (vector 1 2 3 4 5)))
                    (fillarray v 0)
                    (list v (aref v 0) (aref v 4)))"#;
    let expect = expect_test::expect![r#""OK ([0 0 0 0 0] 0 0)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_fillarray_vector_with_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((v (make-vector 4 nil)))
                    (fillarray v 'x)
                    v)"#;
    let expect = expect_test::expect![r#""OK [x x x x]""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_fillarray_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((s (make-string 10 ?a)))
                    (fillarray s ?z)
                    s)"#;
    let expect = expect_test::expect![[r#""OK \"zzzzzzzzzz\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_fillarray_returns_array() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // fillarray returns the modified array itself
    let form = r#"(let ((v (vector 1 2 3)))
                    (eq v (fillarray v 0)))"#;
    let expect = expect_test::expect![r#""OK t""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: matrix operations using aref/aset
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_aref_aset_matrix_transpose() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Transpose a 3x3 matrix
    let form = r#"(let ((m (vector (vector 1 2 3)
                                   (vector 4 5 6)
                                   (vector 7 8 9)))
                        (result (vector (make-vector 3 0)
                                        (make-vector 3 0)
                                        (make-vector 3 0))))
                    (let ((i 0))
                      (while (< i 3)
                        (let ((j 0))
                          (while (< j 3)
                            (aset (aref result j) i
                                  (aref (aref m i) j))
                            (setq j (1+ j))))
                        (setq i (1+ i))))
                    (list (aref result 0)
                          (aref result 1)
                          (aref result 2)))"#;
    let expect = expect_test::expect![r#""OK ([1 4 7] [2 5 8] [3 6 9])""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_aref_aset_matrix_multiply() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiply two 2x2 matrices
    let form = r#"(let ((a (vector (vector 1 2)
                                   (vector 3 4)))
                        (b (vector (vector 5 6)
                                   (vector 7 8)))
                        (result (vector (make-vector 2 0)
                                        (make-vector 2 0))))
                    (let ((i 0))
                      (while (< i 2)
                        (let ((j 0))
                          (while (< j 2)
                            (let ((sum 0) (k 0))
                              (while (< k 2)
                                (setq sum (+ sum (* (aref (aref a i) k)
                                                    (aref (aref b k) j))))
                                (setq k (1+ k)))
                              (aset (aref result i) j sum))
                            (setq j (1+ j))))
                        (setq i (1+ i))))
                    (list (aref result 0) (aref result 1)))"#;
    let expect = expect_test::expect![r#""OK ([19 22] [43 50])""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: histogram using aref/aset
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_aref_aset_histogram() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Count character frequencies in a string using a vector
    let form = r#"(let ((s "hello world")
                        (freq (make-vector 256 0)))
                    (let ((i 0))
                      (while (< i (length s))
                        (let ((c (aref s i)))
                          (aset freq c (1+ (aref freq c))))
                        (setq i (1+ i))))
                    (list (aref freq ?h) (aref freq ?e)
                          (aref freq ?l) (aref freq ?o)
                          (aref freq ?\ ) (aref freq ?w)
                          (aref freq ?r) (aref freq ?d)))"#;
    let expect = expect_test::expect![r#""OK (1 1 3 2 1 1 1 1)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: sort vector in-place using aref/aset
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_aref_aset_bubble_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Bubble sort a vector using aref/aset
    let form = r#"(let ((v (vector 5 3 8 1 9 2 7 4 6)))
                    (let ((n (length v)) (swapped t))
                      (while swapped
                        (setq swapped nil)
                        (let ((i 0))
                          (while (< i (1- n))
                            (when (> (aref v i) (aref v (1+ i)))
                              (let ((tmp (aref v i)))
                                (aset v i (aref v (1+ i)))
                                (aset v (1+ i) tmp)
                                (setq swapped t)))
                            (setq i (1+ i))))
                        (setq n (1- n))))
                    v)"#;
    let expect = expect_test::expect![r#""OK [1 2 3 4 5 6 7 8 9]""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: ring buffer using aref/aset
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_aref_aset_ring_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Ring buffer with wrap-around
    let form = r#"(let ((buf (make-vector 4 nil))
                        (head 0) (tail 0) (count 0)
                        (size 4) (log nil))
                    (let ((push (lambda (val)
                                  (when (< count size)
                                    (aset buf tail val)
                                    (setq tail (% (1+ tail) size)
                                          count (1+ count)))))
                          (pop (lambda ()
                                 (when (> count 0)
                                   (let ((val (aref buf head)))
                                     (aset buf head nil)
                                     (setq head (% (1+ head) size)
                                           count (1- count))
                                     val)))))
                      ;; Push 1-5 (5 won't fit since size=4)
                      (funcall push 1)
                      (funcall push 2)
                      (funcall push 3)
                      (funcall push 4)
                      (funcall push 5) ;; overflow - should be ignored
                      (setq log (cons count log)) ;; count=4
                      ;; Pop 2
                      (setq log (cons (funcall pop) log)) ;; 1
                      (setq log (cons (funcall pop) log)) ;; 2
                      ;; Push 2 more (wraps around)
                      (funcall push 10)
                      (funcall push 11)
                      ;; Pop all
                      (setq log (cons (funcall pop) log)) ;; 3
                      (setq log (cons (funcall pop) log)) ;; 4
                      (setq log (cons (funcall pop) log)) ;; 10
                      (setq log (cons (funcall pop) log)) ;; 11
                      (setq log (cons count log)) ;; 0
                      (nreverse log)))"#;
    let expect = expect_test::expect![r#""OK (4 1 2 3 4 10 11 0)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Advanced oracle parity tests for number predicates:
//! `zerop`, `natnump`, `fixnump`, `floatp`, `integerp`, `numberp`,
//! `booleanp` on edge cases -- large numbers, negative zero, type
//! boundaries, combined predicate chains, and list filtering by predicate.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// zerop edge cases: negative zero, float zero variants, large near-zero
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_predicates_advanced_zerop_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                   ;; Integer zero
                   (zerop 0)
                   ;; Float zero variants
                   (zerop 0.0)
                   (zerop -0.0)
                   (zerop 0.0e0)
                   (zerop -0.0e0)
                   (zerop 0.00000000000)
                   ;; NOT zero
                   (zerop 1)
                   (zerop -1)
                   (zerop 0.0000001)
                   (zerop -0.0000001)
                   (zerop most-positive-fixnum)
                   (zerop most-negative-fixnum)
                   ;; Float precision edge: very small but non-zero
                   (zerop 1.0e-300)
                   (zerop -1.0e-300)
                   ;; Verify -0.0 equals 0.0
                   (= 0.0 -0.0)
                   (eql 0.0 -0.0)
                   ;; zerop on result of arithmetic producing zero
                   (zerop (- 5 5))
                   (zerop (- 5.0 5.0))
                   (zerop (* 0 999999))
                   (zerop (* 0.0 1.0e300)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t t nil nil nil nil nil nil nil nil t nil t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// natnump/wholenump: boundaries, negative, float, non-numeric types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_predicates_advanced_natnump_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                   ;; Basic naturals
                   (natnump 0)
                   (natnump 1)
                   (natnump 100)
                   (natnump most-positive-fixnum)
                   ;; Negatives: NOT natural
                   (natnump -1)
                   (natnump -100)
                   (natnump most-negative-fixnum)
                   ;; Floats: NOT natural (natnump requires integer)
                   (natnump 0.0)
                   (natnump 1.0)
                   (natnump 3.14)
                   (natnump -2.5)
                   ;; Non-numeric types
                   (natnump nil)
                   (natnump t)
                   (natnump 'hello)
                   (natnump "42")
                   (natnump '(1 2))
                   (natnump (vector 1 2))
                   ;; wholenump should be the same as natnump
                   (wholenump 0)
                   (wholenump 42)
                   (wholenump -1)
                   (wholenump 3.14)
                   ;; Verify wholenump and natnump agree
                   (eq (natnump 0) (wholenump 0))
                   (eq (natnump -1) (wholenump -1))
                   (eq (natnump 3.14) (wholenump 3.14)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t nil nil nil nil nil nil nil nil nil nil nil nil nil t t nil nil t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// fixnump/integerp/floatp: type boundaries and arithmetic results
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_predicates_advanced_type_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                   ;; fixnump on fixnum range
                   (fixnump 0)
                   (fixnump 1)
                   (fixnump -1)
                   (fixnump most-positive-fixnum)
                   (fixnump most-negative-fixnum)
                   ;; fixnump on floats: always nil
                   (fixnump 0.0)
                   (fixnump 1.0)
                   (fixnump 1.0e10)
                   ;; fixnump on non-numbers
                   (fixnump nil)
                   (fixnump "42")
                   (fixnump 'sym)
                   ;; integerp: includes fixnums (and bignums if supported)
                   (integerp 0)
                   (integerp most-positive-fixnum)
                   (integerp most-negative-fixnum)
                   (integerp 0.0)
                   (integerp nil)
                   ;; floatp
                   (floatp 3.14)
                   (floatp 0.0)
                   (floatp -0.0)
                   (floatp 1.0e10)
                   (floatp 1.0e-10)
                   (floatp 0)
                   (floatp nil)
                   ;; Arithmetic results: integer ops stay integer
                   (integerp (+ 1 2))
                   (integerp (* 3 4))
                   ;; Float contagion: any float operand makes result float
                   (floatp (+ 1 2.0))
                   (floatp (* 3 4.0))
                   (floatp (/ 1.0 3))
                   ;; Integer division stays integer
                   (integerp (/ 10 3)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t nil nil nil nil nil nil t t t nil nil t t t t t nil nil t t t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// numberp: comprehensive type discrimination
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_predicates_advanced_numberp_discrimination() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test numberp against a wide variety of Elisp types, and combine
    // with other predicates to build a full type classification.
    let form = r#"(let ((classify
                         (lambda (v)
                           (cond
                            ((and (integerp v) (zerop v)) 'int-zero)
                            ((and (integerp v) (natnump v)) 'nat)
                            ((integerp v) 'neg-int)
                            ((and (floatp v) (zerop v)) 'float-zero)
                            ((and (floatp v) (> v 0)) 'pos-float)
                            ((floatp v) 'neg-float)
                            ((booleanp v) 'boolean)
                            ((symbolp v) 'symbol)
                            ((stringp v) 'string)
                            ((consp v) 'cons)
                            ((vectorp v) 'vector)
                            (t 'other)))))
                   (let ((values (list 0 1 -1 42 -42
                                       0.0 -0.0 3.14 -2.718 1.0e100
                                       most-positive-fixnum most-negative-fixnum
                                       nil t 'foo "bar" '(1 . 2) '(a b)
                                       (vector) (vector 1))))
                     (mapcar
                      (lambda (v)
                        (list (funcall classify v)
                              (numberp v)
                              (integerp v)
                              (floatp v)))
                      values)))"#;
    let expect = expect_test::expect![[
        r#""OK ((int-zero t t nil) (nat t t nil) (neg-int t t nil) (nat t t nil) (neg-int t t nil) (float-zero t nil t) (float-zero t nil t) (pos-float t nil t) (neg-float t nil t) (pos-float t nil t) (nat t t nil) (neg-int t t nil) (boolean nil nil nil) (boolean nil nil nil) (symbol nil nil nil) (string nil nil nil) (cons nil nil nil) (cons nil nil nil) (vector nil nil nil) (vector nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// booleanp: strict boolean check (only t and nil)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_predicates_advanced_booleanp_strict() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                   ;; Only t and nil are booleans
                   (booleanp t)
                   (booleanp nil)
                   ;; Everything else is NOT boolean
                   (booleanp 0)
                   (booleanp 1)
                   (booleanp "")
                   (booleanp "t")
                   (booleanp "nil")
                   (booleanp 'true)
                   (booleanp 'false)
                   (booleanp '())
                   (booleanp 0.0)
                   (booleanp (list))
                   ;; Results of boolean expressions
                   (booleanp (and t t))
                   (booleanp (or nil nil))
                   (booleanp (not 42))
                   (booleanp (not nil))
                   (booleanp (= 1 1))
                   (booleanp (< 1 2))
                   ;; booleanp on results of predicates (predicates return t/nil)
                   (booleanp (numberp 42))
                   (booleanp (stringp "hi"))
                   (booleanp (symbolp 'foo))
                   ;; and/or can return non-boolean truthy values
                   (booleanp (and 1 2 3))
                   (booleanp (or 1 2 3)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t nil nil nil nil nil nil nil t nil t t t t t t t t t t nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combined predicate chains: filter and partition lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_predicates_advanced_filter_partition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use predicate chains to partition a heterogeneous list into
    // categories, computing statistics on each.
    let form = r#"(let ((data '(0 1 -5 3.14 -2.7 nil t 'sym "str"
                               42 0.0 -0.0 100 -100 2.718)))
                   (let ((ints nil) (floats nil) (zeros nil)
                         (nats nil) (negs nil) (non-nums nil))
                     ;; Partition
                     (dolist (x data)
                       (cond
                        ((not (numberp x))
                         (setq non-nums (cons x non-nums)))
                        ((zerop x)
                         (setq zeros (cons x zeros)))
                        ((and (integerp x) (natnump x))
                         (setq nats (cons x nats)))
                        ((and (integerp x) (< x 0))
                         (setq negs (cons x negs)))
                        ((and (floatp x) (> x 0))
                         (setq floats (cons x floats)))
                        ((floatp x)
                         (setq floats (cons x floats)))
                        (t (setq ints (cons x ints)))))
                     (list
                      (list 'zeros (nreverse zeros) (length zeros))
                      (list 'nats (nreverse nats) (length nats))
                      (list 'negs (nreverse negs) (length negs))
                      (list 'floats (nreverse floats) (length floats))
                      (list 'non-nums (nreverse non-nums) (length non-nums))
                      ;; Verify partition covers all elements
                      (= (length data)
                         (+ (length zeros) (length nats) (length negs)
                            (length floats) (length non-nums) (length ints)))
                      ;; Sum of all numeric elements
                      (let ((num-sum 0))
                        (dolist (x data)
                          (when (numberp x)
                            (setq num-sum (+ num-sum x))))
                        num-sum))))"#;
    let expect = expect_test::expect![[
        r#""OK ((zeros (0 0.0 -0.0) 1) (nats (1 42 100) 1) (negs (-5 -100) 1) (floats (3.14 -2.7 2.718) 1) (non-nums (nil t 'sym \"str\") 1) nil 41.158)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Predicate-based validation and coercion framework
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_predicates_advanced_coercion_framework() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a type coercion/validation system using predicates.
    // Each rule has (predicate . coercion-fn) pairs.
    let form = r#"(progn
  (fset 'neovm--test-coerce
    (lambda (value target-type)
      (cond
       ;; to-integer
       ((eq target-type 'integer)
        (cond
         ((integerp value) value)
         ((floatp value) (truncate value))
         ((stringp value)
          (let ((n (string-to-number value)))
            (if (and (= n 0) (not (string= value "0")))
                (cons 'error "not a number")
              (truncate n))))
         (t (cons 'error (format "cannot convert %S to integer" value)))))
       ;; to-float
       ((eq target-type 'float)
        (cond
         ((floatp value) value)
         ((integerp value) (float value))
         ((stringp value)
          (let ((n (string-to-number value)))
            (if (and (= n 0) (not (string-match-p "^0" value)))
                (cons 'error "not a number")
              (float n))))
         (t (cons 'error (format "cannot convert %S to float" value)))))
       ;; to-string
       ((eq target-type 'string)
        (cond
         ((stringp value) value)
         ((integerp value) (number-to-string value))
         ((floatp value) (number-to-string value))
         ((null value) "nil")
         ((symbolp value) (symbol-name value))
         (t (format "%S" value))))
       ;; to-boolean
       ((eq target-type 'boolean)
        (cond
         ((booleanp value) value)
         ((and (numberp value) (zerop value)) nil)
         ((numberp value) t)
         ((and (stringp value) (string= value "")) nil)
         ((stringp value) t)
         ((null value) nil)
         (t t)))
       (t (cons 'error "unknown target type")))))

  (unwind-protect
      (let ((test-values (list 0 1 -5 3.14 -2.7 0.0
                               "42" "3.14" "hello" "0"
                               nil t 'sym)))
        (list
         ;; Coerce all to integer
         (mapcar (lambda (v) (funcall 'neovm--test-coerce v 'integer))
                 test-values)
         ;; Coerce all to float
         (mapcar (lambda (v) (funcall 'neovm--test-coerce v 'float))
                 test-values)
         ;; Coerce all to string
         (mapcar (lambda (v) (funcall 'neovm--test-coerce v 'string))
                 test-values)
         ;; Coerce all to boolean
         (mapcar (lambda (v) (funcall 'neovm--test-coerce v 'boolean))
                 test-values)
         ;; Chained coercions: int -> float -> string -> int
         (let* ((start 42)
                (as-float (funcall 'neovm--test-coerce start 'float))
                (as-str (funcall 'neovm--test-coerce as-float 'string))
                (back-to-int (funcall 'neovm--test-coerce as-str 'integer)))
           (list start as-float as-str back-to-int
                 (= start back-to-int)))))
    (fmakunbound 'neovm--test-coerce)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 -5 3 -2 0 42 3 (error . \"not a number\") 0 (error . \"cannot convert nil to integer\") (error . \"cannot convert t to integer\") (error . \"cannot convert sym to integer\")) (0.0 1.0 -5.0 3.14 -2.7 0.0 42.0 3.14 (error . \"not a number\") 0.0 (error . \"cannot convert nil to float\") (error . \"cannot convert t to float\") (error . \"cannot convert sym to float\")) (\"0\" \"1\" \"-5\" \"3.14\" \"-2.7\" \"0.0\" \"42\" \"3.14\" \"hello\" \"0\" \"nil\" \"t\" \"sym\") (nil t t t t nil t t t t nil t t) (42 42.0 \"42.0\" 42 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Numeric predicate-based dispatch table and accumulator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_predicates_advanced_dispatch_accumulator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build an accumulator that tracks statistics separately for each
    // numeric type, using predicates for dispatch.
    let form = r#"(let ((int-count 0) (int-sum 0) (int-min most-positive-fixnum) (int-max most-negative-fixnum)
                        (float-count 0) (float-sum 0.0)
                        (zero-count 0)
                        (non-num-count 0)
                        (data '(1 2.5 0 -3 4.7 nil "x" 0.0 -0.0 10 -1.5
                                most-positive-fixnum 99 -99 0 3.14)))
                   ;; Process each element
                   (dolist (x data)
                     (cond
                      ((not (numberp x))
                       (setq non-num-count (1+ non-num-count)))
                      ((zerop x)
                       (setq zero-count (1+ zero-count))
                       ;; Also count in appropriate type bucket
                       (if (integerp x)
                           (setq int-count (1+ int-count)
                                 int-sum (+ int-sum x))
                         (setq float-count (1+ float-count)
                               float-sum (+ float-sum x))))
                      ((integerp x)
                       (setq int-count (1+ int-count)
                             int-sum (+ int-sum x)
                             int-min (min int-min x)
                             int-max (max int-max x)))
                      ((floatp x)
                       (setq float-count (1+ float-count)
                             float-sum (+ float-sum x)))))
                   (list
                    (list 'int-count int-count 'int-sum int-sum)
                    (list 'int-range int-min int-max)
                    (list 'float-count float-count 'float-sum float-sum)
                    (list 'zero-count zero-count)
                    (list 'non-num-count non-num-count)
                    ;; Verify all accounted for
                    (= (length data)
                       (+ int-count float-count non-num-count))))"#;
    let expect = expect_test::expect![[
        r#""OK ((int-count 7 int-sum 8) (int-range -99 99) (float-count 6 float-sum 8.84) (zero-count 4) (non-num-count 3) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

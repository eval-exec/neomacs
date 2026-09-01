//! Oracle parity tests for `type-of` with ALL value types:
//! integer, float, string, cons, vector, symbol, bool-vector, char-table,
//! hash-table, marker, subr (built-in), compiled-function (byte-code),
//! nil, t, buffer, and complex type-based dispatch patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// type-of on numeric types: integer and float edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_of_numeric_types_exhaustive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Integers: zero, positive, negative, large
  (type-of 0)
  (type-of 1)
  (type-of -1)
  (type-of 42)
  (type-of -999)
  (type-of most-positive-fixnum)
  (type-of most-negative-fixnum)
  ;; Floats: zero, positive, negative, fractional, scientific notation
  (type-of 0.0)
  (type-of 3.14)
  (type-of -2.718)
  (type-of 1.0e10)
  (type-of 1.0e-10)
  (type-of -0.0)
  ;; Special floats
  (type-of 1.0e+INF)
  (type-of -1.0e+INF)
  (type-of 0.0e+NaN)
  ;; Integer arithmetic result
  (type-of (+ 1 2))
  ;; Float arithmetic result
  (type-of (+ 1.0 2))
  ;; Division producing float
  (type-of (/ 1.0 3.0)))"#;
    let expect = expect_test::expect![[
        r#""OK (integer integer integer integer integer integer integer float float float float float float float float float integer float float)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// type-of on string, cons, vector with edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_of_compound_types_exhaustive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Strings
  (type-of "hello")
  (type-of "")
  (type-of "a")
  (type-of (make-string 5 ?x))
  (type-of (concat "a" "b"))
  (type-of (substring "hello" 1 3))
  ;; Cons cells
  (type-of '(1 . 2))
  (type-of '(a b c))
  (type-of (cons nil nil))
  (type-of (cons 'a '(b c)))
  (type-of '((1 2) (3 4)))
  ;; Vectors
  (type-of [])
  (type-of [1 2 3])
  (type-of (vector 'a 'b 'c))
  (type-of (make-vector 3 0))
  (type-of (vconcat [1] [2])))"#;
    let expect = expect_test::expect![[
        r#""OK (string string string string string string cons cons cons cons cons vector vector vector vector vector)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// type-of on symbol, nil, t
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_of_symbol_nil_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Symbols
  (type-of 'foo)
  (type-of 'bar-baz)
  (type-of '+)
  (type-of 'nil)
  (type-of 't)
  ;; nil and t directly
  (type-of nil)
  (type-of t)
  ;; Keyword symbols (keywords are symbols too)
  (type-of :keyword)
  (type-of :test)
  ;; Uninterned symbol
  (type-of (make-symbol "uninterned"))
  ;; Symbol returned by intern
  (type-of (intern "dynamically-interned"))
  ;; Equality checks: nil and t are symbols
  (eq (type-of nil) 'symbol)
  (eq (type-of t) 'symbol)
  (eq (type-of :foo) 'symbol))"#;
    let expect = expect_test::expect![[
        r#""OK (symbol symbol symbol symbol symbol symbol symbol symbol symbol symbol symbol t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// type-of on bool-vector, char-table, hash-table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_of_specialized_containers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Bool vectors
  (type-of (make-bool-vector 8 nil))
  (type-of (make-bool-vector 0 t))
  (type-of (make-bool-vector 32 t))
  ;; Char tables
  (type-of (make-char-table 'foo))
  (type-of (make-char-table 'syntax-table))
  ;; Hash tables with various :test arguments
  (type-of (make-hash-table))
  (type-of (make-hash-table :test 'eq))
  (type-of (make-hash-table :test 'equal))
  (type-of (make-hash-table :test 'eql))
  (type-of (make-hash-table :size 100))
  ;; Confirm they're all distinct types
  (let ((types (list (type-of (make-bool-vector 1 nil))
                     (type-of (make-char-table 'foo))
                     (type-of (make-hash-table)))))
    types))"#;
    let expect = expect_test::expect![[
        r#""OK (bool-vector bool-vector bool-vector char-table char-table hash-table hash-table hash-table hash-table hash-table (bool-vector char-table hash-table))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// type-of on marker and buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_of_marker_and_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "hello world")
  (let* ((m (point-marker))
         (buf (current-buffer))
         (results (list
           ;; Marker type
           (type-of m)
           ;; Buffer type
           (type-of buf)
           ;; Marker in different position
           (let ((m2 (copy-marker 1)))
             (type-of m2))
           ;; Marker at point-min
           (let ((m3 (copy-marker (point-min))))
             (type-of m3))
           ;; Marker at point-max
           (let ((m4 (copy-marker (point-max))))
             (type-of m4)))))
    results))"#;
    let expect = expect_test::expect![[r#""OK (marker buffer marker marker marker)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// type-of on subr (built-in function)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_of_subr_and_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Built-in functions (subrs)
  (type-of (symbol-function '+))
  (type-of (symbol-function 'car))
  (type-of (symbol-function 'cons))
  (type-of (symbol-function 'length))
  ;; subrp predicate should agree
  (subrp (symbol-function '+))
  (subrp (symbol-function 'car))
  ;; Lambda (interpreted function) - type varies by Emacs version
  ;; but should be consistent between oracle and neovm
  (let ((f (lambda (x) (+ x 1))))
    (type-of f))
  ;; Check that type-of on subrp-confirmed values is consistent
  (let ((fns '(+ - * car cdr cons list append length)))
    (mapcar (lambda (fn)
              (let ((sf (symbol-function fn)))
                (list fn (subrp sf))))
            fns)))"#;
    let expect = expect_test::expect![[
        r#""OK (subr subr subr subr t t interpreted-function ((+ t) (- t) (* t) (car t) (cdr t) (cons t) (list t) (append t) (length t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: type-based dispatch system
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_of_dispatch_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Build a type dispatcher: maps type-of result to handler
  (fset 'neovm--type-describe
    (lambda (val)
      (let ((typ (type-of val)))
        (cond
         ((eq typ 'integer) (format "int:%d" val))
         ((eq typ 'float) (format "float:%.2f" val))
         ((eq typ 'string) (format "str:%s" val))
         ((eq typ 'cons) (format "list:len=%d" (length val)))
         ((eq typ 'vector) (format "vec:len=%d" (length val)))
         ((eq typ 'symbol) (format "sym:%s" (symbol-name val)))
         ((eq typ 'hash-table) (format "hash:count=%d" (hash-table-count val)))
         ((eq typ 'bool-vector) (format "bvec:len=%d" (length val)))
         (t (format "other:%s" typ))))))

  ;; Build a type-safe equality checker
  (fset 'neovm--type-safe-equal
    (lambda (a b)
      (and (eq (type-of a) (type-of b))
           (equal a b))))

  (unwind-protect
      (let ((test-values (list 42 3.14 "hello" '(1 2 3) [4 5 6]
                               'foo (make-hash-table)
                               (make-bool-vector 4 t))))
        (list
         ;; Describe each value
         (mapcar (lambda (v) (funcall 'neovm--type-describe v)) test-values)
         ;; Type-safe equality checks
         (funcall 'neovm--type-safe-equal 42 42)
         (funcall 'neovm--type-safe-equal 42 42.0) ;; different types
         (funcall 'neovm--type-safe-equal "hi" "hi")
         (funcall 'neovm--type-safe-equal '(1 2) '(1 2))
         (funcall 'neovm--type-safe-equal '(1 2) [1 2]) ;; different types
         ;; Group values by type
         (let ((groups (make-hash-table :test 'eq))
               (vals (list 1 "a" 2 "b" 'x 3.0 'y 4.0)))
           (dolist (v vals)
             (let ((typ (type-of v)))
               (puthash typ (cons v (gethash typ groups nil)) groups)))
           (let ((result nil))
             (maphash (lambda (k v) (push (list k (nreverse v)) result)) groups)
             (sort result (lambda (a b)
                            (string< (symbol-name (car a))
                                     (symbol-name (car b)))))))))
    (fmakunbound 'neovm--type-describe)
    (fmakunbound 'neovm--type-safe-equal)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"int:42\" \"float:3.14\" \"str:hello\" \"list:len=3\" \"vec:len=3\" \"sym:foo\" \"hash:count=0\" \"bvec:len=4\") t nil t t nil ((float (3.0 4.0)) (integer (1 2)) (string (\"a\" \"b\")) (symbol (x y))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: serializer using type-of for encoding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_of_serializer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Serializer: encode Elisp values to tagged s-expressions
  (fset 'neovm--serialize
    (lambda (val)
      (let ((typ (type-of val)))
        (cond
         ((eq typ 'integer) (list 'INT val))
         ((eq typ 'float) (list 'FLOAT val))
         ((eq typ 'string) (list 'STR val))
         ((eq typ 'symbol) (list 'SYM (symbol-name val)))
         ((eq typ 'cons)
          (list 'LIST (mapcar (lambda (x) (funcall 'neovm--serialize x)) val)))
         ((eq typ 'vector)
          (list 'VEC (mapcar (lambda (x) (funcall 'neovm--serialize x))
                             (append val nil))))
         ((null val) (list 'NULL))
         (t (list 'UNKNOWN (format "%s" typ)))))))

  ;; Deserializer: decode tagged s-expressions back to values
  (fset 'neovm--deserialize
    (lambda (encoded)
      (let ((tag (car encoded))
            (payload (cadr encoded)))
        (cond
         ((eq tag 'INT) payload)
         ((eq tag 'FLOAT) payload)
         ((eq tag 'STR) payload)
         ((eq tag 'SYM) (intern payload))
         ((eq tag 'LIST)
          (mapcar (lambda (x) (funcall 'neovm--deserialize x)) payload))
         ((eq tag 'VEC)
          (apply 'vector
                 (mapcar (lambda (x) (funcall 'neovm--deserialize x)) payload)))
         ((eq tag 'NULL) nil)
         (t (error "Unknown tag: %s" tag))))))

  (unwind-protect
      (let* ((test-data (list 42 3.14 "hello" 'world '(1 2 3) [4 5 6]
                              '(nested (list "with" 7))))
             (encoded (mapcar (lambda (v) (funcall 'neovm--serialize v)) test-data))
             (decoded (mapcar (lambda (e) (funcall 'neovm--deserialize e)) encoded))
             ;; Check roundtrip
             (roundtrip-ok (let ((ok t) (i 0))
                             (while (< i (length test-data))
                               (unless (equal (nth i test-data) (nth i decoded))
                                 (setq ok nil))
                               (setq i (1+ i)))
                             ok)))
        (list
         ;; Show encoded forms
         encoded
         ;; Show decoded values
         decoded
         ;; Roundtrip check
         roundtrip-ok
         ;; Types preserved
         (mapcar (lambda (v) (type-of v)) decoded)))
    (fmakunbound 'neovm--serialize)
    (fmakunbound 'neovm--deserialize)))"#;
    let expect = expect_test::expect![[
        r#""OK (((INT 42) (FLOAT 3.14) (STR \"hello\") (SYM \"world\") (LIST ((INT 1) (INT 2) (INT 3))) (VEC ((INT 4) (INT 5) (INT 6))) (LIST ((SYM \"nested\") (LIST ((SYM \"list\") (STR \"with\") (INT 7)))))) (42 3.14 \"hello\" world (1 2 3) [4 5 6] (nested (list \"with\" 7))) t (integer float string symbol cons vector cons))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: type-of with mapcar across heterogeneous collections
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_of_heterogeneous_collection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((items (list 1 2.0 "three" 'four '(5 6) [7 8]
                           (make-hash-table) (make-bool-vector 3 t)
                           nil t :keyword (cons 'a 'b)))
             (types (mapcar 'type-of items))
             ;; Count each type
             (counts (let ((ht (make-hash-table :test 'eq)))
                       (dolist (typ types)
                         (puthash typ (1+ (gethash typ ht 0)) ht))
                       (let ((result nil))
                         (maphash (lambda (k v) (push (list k v) result)) ht)
                         (sort result (lambda (a b)
                                        (string< (symbol-name (car a))
                                                 (symbol-name (car b))))))))
             ;; Partition by type
             (strings (let ((acc nil))
                        (dolist (item items)
                          (when (eq (type-of item) 'string)
                            (push item acc)))
                        (nreverse acc)))
             (numbers (let ((acc nil))
                        (dolist (item items)
                          (when (memq (type-of item) '(integer float))
                            (push item acc)))
                        (nreverse acc))))
  (list types counts strings numbers))"#;
    let expect = expect_test::expect![[
        r#""OK ((integer float string symbol cons vector hash-table bool-vector symbol symbol symbol cons) ((bool-vector 1) (cons 2) (float 1) (hash-table 1) (integer 1) (string 1) (symbol 4) (vector 1)) (\"three\") (1 2.0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

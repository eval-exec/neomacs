//! Oracle parity tests for complex type system simulation:
//! comprehensive type predicates, type dispatch via `cond` + `type-of`,
//! tagged union simulation, polymorphic dispatch table,
//! coercion chain (string -> number -> string roundtrip),
//! and type-safe container with checked insert/retrieve.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Type predicates comprehensive (all type-of categories)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_predicates_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test every type predicate against every type of value
    let form = r#"(let ((values (list 42 3.14 "hello" 'foo nil t
                                '(1 2) [1 2] (make-hash-table)
                                :keyword)))
  (mapcar
   (lambda (v)
     (list (type-of v)
           (integerp v) (floatp v) (numberp v)
           (stringp v) (symbolp v) (consp v)
           (vectorp v) (listp v) (sequencep v)
           (atom v) (null v) (booleanp v)
           (keywordp v) (natnump v) (characterp v)
           (arrayp v) (hash-table-p v)))
   values))"#;
    let expect = expect_test::expect![[
        r#""OK ((integer t nil t nil nil nil nil nil nil t nil nil nil t t nil nil) (float nil t t nil nil nil nil nil nil t nil nil nil nil nil nil nil) (string nil nil nil t nil nil nil nil t t nil nil nil nil nil t nil) (symbol nil nil nil nil t nil nil nil nil t nil nil nil nil nil nil nil) (symbol nil nil nil nil t nil nil t t t t t nil nil nil nil nil) (symbol nil nil nil nil t nil nil nil nil t nil t nil nil nil nil nil) (cons nil nil nil nil nil t nil t t nil nil nil nil nil nil nil nil) (vector nil nil nil nil nil nil t nil t t nil nil nil nil nil t nil) (hash-table nil nil nil nil nil nil nil nil nil t nil nil nil nil nil nil t) (symbol nil nil nil nil t nil nil nil nil t nil nil t nil nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Type dispatch using cond + type-of
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_dispatch_cond() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a type-dispatching "describe" function using cond + type-of
    let form = r#"(progn
  (fset 'neovm--describe-type
    (lambda (val)
      (cond
        ((null val) "nil-value")
        ((eq val t) "boolean-true")
        ((integerp val)
         (concat "int:" (number-to-string val)))
        ((floatp val)
         (concat "float:" (number-to-string val)))
        ((stringp val)
         (concat "string[" (number-to-string (length val)) "]:"
                 (if (> (length val) 10)
                     (concat (substring val 0 10) "...")
                   val)))
        ((keywordp val)
         (concat "keyword:" (symbol-name val)))
        ((symbolp val)
         (concat "symbol:" (symbol-name val)))
        ((vectorp val)
         (concat "vector[" (number-to-string (length val)) "]"))
        ((consp val)
         (if (proper-list-p val)
             (concat "list[" (number-to-string (length val)) "]")
           "dotted-pair"))
        ((hash-table-p val)
         (concat "hash-table[" (number-to-string (hash-table-count val)) "]"))
        (t "unknown"))))
  (unwind-protect
      (list
        (funcall 'neovm--describe-type nil)
        (funcall 'neovm--describe-type t)
        (funcall 'neovm--describe-type 42)
        (funcall 'neovm--describe-type 3.14)
        (funcall 'neovm--describe-type "short")
        (funcall 'neovm--describe-type "a long string value here")
        (funcall 'neovm--describe-type :test)
        (funcall 'neovm--describe-type 'foo)
        (funcall 'neovm--describe-type [1 2 3])
        (funcall 'neovm--describe-type '(a b c d))
        (funcall 'neovm--describe-type '(a . b))
        (funcall 'neovm--describe-type (make-hash-table)))
    (fmakunbound 'neovm--describe-type)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"nil-value\" \"boolean-true\" \"int:42\" \"float:3.14\" \"string[5]:short\" \"string[24]:a long str...\" \"keyword::test\" \"symbol:foo\" \"vector[3]\" \"list[4]\" \"dotted-pair\" \"hash-table[0]\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tagged union simulation (cons of type-tag + data)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_tagged_union_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a tagged union type system using (tag . data) cons cells
    let form = r#"(progn
  ;; Constructors
  (fset 'neovm--make-int (lambda (n) (cons 'int n)))
  (fset 'neovm--make-str (lambda (s) (cons 'str s)))
  (fset 'neovm--make-pair (lambda (a b) (cons 'pair (cons a b))))
  (fset 'neovm--make-list-val (lambda (items) (cons 'list-val items)))

  ;; Destructor / pattern match
  (fset 'neovm--match-val
    (lambda (val)
      (let ((tag (car val))
            (data (cdr val)))
        (cond
          ((eq tag 'int) (concat "Int(" (number-to-string data) ")"))
          ((eq tag 'str) (concat "Str(\"" data "\")"))
          ((eq tag 'pair)
           (concat "Pair("
                   (funcall 'neovm--match-val (car data)) ", "
                   (funcall 'neovm--match-val (cdr data)) ")"))
          ((eq tag 'list-val)
           (concat "List["
                   (mapconcat (lambda (x) (funcall 'neovm--match-val x))
                              data ", ")
                   "]"))
          (t "Unknown")))))

  ;; Type checker
  (fset 'neovm--val-type (lambda (val) (car val)))

  (unwind-protect
      (let* ((v1 (funcall 'neovm--make-int 42))
             (v2 (funcall 'neovm--make-str "hello"))
             (v3 (funcall 'neovm--make-pair v1 v2))
             (v4 (funcall 'neovm--make-list-val (list v1 v2 v3))))
        (list
         ;; Type tags
         (funcall 'neovm--val-type v1)
         (funcall 'neovm--val-type v2)
         (funcall 'neovm--val-type v3)
         (funcall 'neovm--val-type v4)
         ;; Pattern match / toString
         (funcall 'neovm--match-val v1)
         (funcall 'neovm--match-val v2)
         (funcall 'neovm--match-val v3)
         (funcall 'neovm--match-val v4)
         ;; Nested pair of pairs
         (funcall 'neovm--match-val
           (funcall 'neovm--make-pair
             (funcall 'neovm--make-pair
               (funcall 'neovm--make-int 1)
               (funcall 'neovm--make-int 2))
             (funcall 'neovm--make-str "end")))))
    (fmakunbound 'neovm--make-int)
    (fmakunbound 'neovm--make-str)
    (fmakunbound 'neovm--make-pair)
    (fmakunbound 'neovm--make-list-val)
    (fmakunbound 'neovm--match-val)
    (fmakunbound 'neovm--val-type)))"#;
    let expect = expect_test::expect![[
        r#""OK (int str pair list-val \"Int(42)\" \"Str(\\\"hello\\\")\" \"Pair(Int(42), Str(\\\"hello\\\"))\" \"List[Int(42), Str(\\\"hello\\\"), Pair(Int(42), Str(\\\"hello\\\"))]\" \"Pair(Pair(Int(1), Int(2)), Str(\\\"end\\\"))\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Polymorphic dispatch table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_polymorphic_dispatch_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a dispatch table mapping type symbols to handler functions,
    // then dispatch operations polymorphically based on runtime type
    let form = r#"(progn
  (fset 'neovm--dispatch
    (lambda (dispatch-table val method)
      (let* ((type-tag (car val))
             (type-methods (cdr (assq type-tag dispatch-table)))
             (handler (cdr (assq method type-methods))))
        (if handler
            (funcall handler (cdr val))
          (concat "no-method:" (symbol-name method)
                  " for:" (symbol-name type-tag))))))

  (unwind-protect
      (let ((table
             (list
              ;; Circle type: data is radius
              (cons 'circle
                    (list (cons 'area (lambda (r) (* 3.14159 r r)))
                          (cons 'describe (lambda (r)
                                            (concat "circle(r="
                                                    (number-to-string r) ")")))))
              ;; Rect type: data is (w . h)
              (cons 'rect
                    (list (cons 'area (lambda (d) (* (car d) (cdr d))))
                          (cons 'describe (lambda (d)
                                            (concat "rect("
                                                    (number-to-string (car d))
                                                    "x"
                                                    (number-to-string (cdr d))
                                                    ")")))))
              ;; Triangle type: data is (base . height)
              (cons 'triangle
                    (list (cons 'area (lambda (d) (/ (* (car d) (cdr d)) 2.0)))
                          (cons 'describe (lambda (d)
                                            (concat "tri(b="
                                                    (number-to-string (car d))
                                                    ",h="
                                                    (number-to-string (cdr d))
                                                    ")"))))))))
        (let ((shapes (list (cons 'circle 5)
                            (cons 'rect (cons 4 6))
                            (cons 'triangle (cons 3 8)))))
          (list
           ;; Describe all shapes
           (mapcar (lambda (s) (funcall 'neovm--dispatch table s 'describe))
                   shapes)
           ;; Compute all areas
           (mapcar (lambda (s) (funcall 'neovm--dispatch table s 'area))
                   shapes)
           ;; Missing method
           (funcall 'neovm--dispatch table (cons 'circle 5) 'perimeter))))
    (fmakunbound 'neovm--dispatch)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"circle(r=5)\" \"rect(4x6)\" \"tri(b=3,h=8)\") (78.53975 24 12.0) \"no-method:perimeter for:circle\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Coercion chain (string -> number -> string roundtrip)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coercion_chain_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test various coercion paths and roundtrip fidelity
    let form = r#"(let ((test-values '("42" "3.14" "0" "-7" "100"
                                       "0.001" "-99.5")))
  (list
   ;; string -> number -> string roundtrip for integers
   (mapcar (lambda (s)
             (let* ((n (string-to-number s))
                    (s2 (number-to-string n)))
               (list s n s2 (equal s s2))))
           '("42" "0" "-7" "100"))
   ;; string -> number -> string roundtrip for floats
   (mapcar (lambda (s)
             (let* ((n (string-to-number s))
                    (s2 (number-to-string n))
                    (n2 (string-to-number s2)))
               (list s n s2 (= n n2))))
           '("3.14" "0.001" "-99.5"))
   ;; number -> string -> number roundtrip
   (mapcar (lambda (n)
             (let* ((s (number-to-string n))
                    (n2 (string-to-number s)))
               (list n s n2 (= n n2))))
           '(0 1 -1 42 1000 -500))
   ;; float -> string -> float roundtrip
   (mapcar (lambda (n)
             (let* ((s (number-to-string n))
                    (n2 (string-to-number s)))
               (list n s (= n n2))))
           '(1.0 0.5 -2.5 100.0))
   ;; char -> string -> char roundtrip
   (let* ((ch ?A)
          (s (char-to-string ch))
          (ch2 (string-to-char s)))
     (list ch s ch2 (= ch ch2)))
   ;; symbol -> string -> intern roundtrip
   (let* ((sym 'hello)
          (s (symbol-name sym))
          (sym2 (intern s)))
     (list sym s sym2 (eq sym sym2)))
   ;; Edge: string-to-number on non-numeric strings
   (list (string-to-number "abc")
         (string-to-number "")
         (string-to-number "42abc")
         (string-to-number "  42"))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"42\" 42 \"42\" t) (\"0\" 0 \"0\" t) (\"-7\" -7 \"-7\" t) (\"100\" 100 \"100\" t)) ((\"3.14\" 3.14 \"3.14\" t) (\"0.001\" 0.001 \"0.001\" t) (\"-99.5\" -99.5 \"-99.5\" t)) ((0 \"0\" 0 t) (1 \"1\" 1 t) (-1 \"-1\" -1 t) (42 \"42\" 42 t) (1000 \"1000\" 1000 t) (-500 \"-500\" -500 t)) ((1.0 \"1.0\" t) (0.5 \"0.5\" t) (-2.5 \"-2.5\" t) (100.0 \"100.0\" t)) (65 \"A\" 65 t) (hello \"hello\" hello t) (0 0 42 42))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Type-safe container (checked insert/retrieve)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_safe_container() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a type-safe container that only accepts values of a
    // declared type, and signals errors on type mismatch
    let form = r#"(progn
  ;; Create a typed container: (type . items-vector-as-list)
  (fset 'neovm--make-typed-container
    (lambda (type-pred)
      (list type-pred)))

  ;; Insert: check type, signal error string on mismatch
  (fset 'neovm--typed-insert
    (lambda (container val)
      (let ((pred (car container)))
        (if (funcall pred val)
            (progn (setcdr container (cons val (cdr container)))
                   'ok)
          'type-error))))

  ;; Retrieve all items
  (fset 'neovm--typed-items
    (lambda (container)
      (cdr container)))

  ;; Count
  (fset 'neovm--typed-count
    (lambda (container)
      (length (cdr container))))

  ;; Find first matching predicate
  (fset 'neovm--typed-find
    (lambda (container pred)
      (let ((items (cdr container))
            (found nil))
        (while (and items (not found))
          (when (funcall pred (car items))
            (setq found (car items)))
          (setq items (cdr items)))
        found)))

  (unwind-protect
      (let ((int-box (funcall 'neovm--make-typed-container 'integerp))
            (str-box (funcall 'neovm--make-typed-container 'stringp)))
        (list
         ;; Insert valid types
         (funcall 'neovm--typed-insert int-box 1)
         (funcall 'neovm--typed-insert int-box 2)
         (funcall 'neovm--typed-insert int-box 3)
         ;; Insert invalid type returns error
         (funcall 'neovm--typed-insert int-box "bad")
         (funcall 'neovm--typed-insert int-box 3.14)
         ;; Items (reverse order from cons)
         (funcall 'neovm--typed-items int-box)
         (funcall 'neovm--typed-count int-box)
         ;; String box
         (funcall 'neovm--typed-insert str-box "hello")
         (funcall 'neovm--typed-insert str-box "world")
         (funcall 'neovm--typed-insert str-box 42)
         (funcall 'neovm--typed-items str-box)
         (funcall 'neovm--typed-count str-box)
         ;; Find in int-box
         (funcall 'neovm--typed-find int-box
                  (lambda (x) (> x 2)))
         ;; Find in str-box
         (funcall 'neovm--typed-find str-box
                  (lambda (s) (string-equal s "hello")))))
    (fmakunbound 'neovm--make-typed-container)
    (fmakunbound 'neovm--typed-insert)
    (fmakunbound 'neovm--typed-items)
    (fmakunbound 'neovm--typed-count)
    (fmakunbound 'neovm--typed-find)))"#;
    let expect = expect_test::expect![[
        r#""OK (ok ok ok type-error type-error (3 2 1) 3 ok ok type-error (\"world\" \"hello\") 2 3 \"hello\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

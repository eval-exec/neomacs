//! Advanced oracle parity tests for `read-from-string`.
//!
//! Tests reading various types, quoted forms, index tracking, sequential
//! reads from one string, error handling, and parsing a mini-language.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Read basic types: integers, floats, strings, symbols
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_read_adv_basic_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; Integers
  (car (read-from-string "0"))
  (car (read-from-string "42"))
  (car (read-from-string "-100"))
  (car (read-from-string "+7"))
  ;; Floats
  (car (read-from-string "3.14"))
  (car (read-from-string "-0.001"))
  (car (read-from-string "2.5e3"))
  (car (read-from-string "1e-5"))
  ;; Strings
  (car (read-from-string "\"hello\""))
  (car (read-from-string "\"\""))
  (car (read-from-string "\"with \\\"quotes\\\"\""))
  (car (read-from-string "\"line1\\nline2\""))
  ;; Symbols
  (car (read-from-string "foo"))
  (car (read-from-string "my-var"))
  (car (read-from-string "nil"))
  (car (read-from-string "t"))
  ;; Keywords
  (car (read-from-string ":key"))
  (car (read-from-string ":another-key")))"####;
    let expect = expect_test::expect![[
        r#""OK (0 42 -100 7 3.14 -0.001 2500.0 1e-05 \"hello\" \"\" \"with \\\"quotes\\\"\" \"line1\\nline2\" foo my-var nil t :key :another-key)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Read compound types: lists, vectors, cons cells
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_read_adv_compound_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; Simple list
  (car (read-from-string "(1 2 3)"))
  ;; Nested list
  (car (read-from-string "((a b) (c d))"))
  ;; Mixed type list
  (car (read-from-string "(1 \"two\" three 4.0)"))
  ;; Empty list
  (car (read-from-string "()"))
  (car (read-from-string "nil"))
  ;; Dotted pair / cons cell
  (car (read-from-string "(a . b)"))
  (car (read-from-string "(1 . 2)"))
  ;; Improper list
  (car (read-from-string "(1 2 . 3)"))
  ;; Vectors
  (car (read-from-string "[]"))
  (car (read-from-string "[1 2 3]"))
  (car (read-from-string "[[1 2] [3 4]]"))
  ;; Alist
  (car (read-from-string "((a . 1) (b . 2) (c . 3))")))"####;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3) ((a b) (c d)) (1 \"two\" three 4.0) nil nil (a . b) (1 . 2) (1 2 . 3) [] [1 2 3] [[1 2] [3 4]] ((a . 1) (b . 2) (c . 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Quoted forms: 'x, #'x, `x, ,x, ,@x
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_read_adv_quoted_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; quote
  (car (read-from-string "'foo"))
  (car (read-from-string "'(1 2 3)"))
  ;; function quote
  (car (read-from-string "#'car"))
  (car (read-from-string "#'+"))
  ;; backquote
  (car (read-from-string "`(a b c)"))
  ;; Verify structure of quoted forms
  (equal (car (read-from-string "'foo")) '(quote foo))
  (equal (car (read-from-string "#'car")) '(function car))
  ;; Nested quotes
  (car (read-from-string "''x"))
  (car (read-from-string "'(a 'b 'c)"))
  ;; backquote with unquote
  (car (read-from-string "`(a ,b ,@c)")))"####;
    let expect = expect_test::expect![[
        r#""OK ('foo '(1 2 3) #'car #'+ `(a b c) t t ''x '(a 'b 'c) `(a ,b ,@c))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Second return value: index into string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_read_adv_index_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; Simple values: index should be past the value
  (cdr (read-from-string "42"))
  (cdr (read-from-string "hello"))
  (cdr (read-from-string "\"a string\""))
  (cdr (read-from-string "(1 2 3)"))
  ;; Leading whitespace: index past value
  (cdr (read-from-string "   42"))
  (cdr (read-from-string "\n\thello"))
  ;; Multiple forms: index stops at first
  (cdr (read-from-string "42 43 44"))
  (cdr (read-from-string "(a b) (c d)"))
  ;; Trailing content after value
  (let ((result (read-from-string "hello world")))
    (list (car result)
          (cdr result)
          (substring "hello world" (cdr result))))
  ;; With explicit START parameter
  (cdr (read-from-string "aaa bbb ccc" 4))
  (car (read-from-string "aaa bbb ccc" 4))
  (cdr (read-from-string "aaa bbb ccc" 8))
  (car (read-from-string "aaa bbb ccc" 8)))"####;
    let expect =
        expect_test::expect![[r#""OK (2 5 10 7 5 7 2 5 (hello 5 \" world\") 7 bbb 11 ccc)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sequential reads from one string using the returned index
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_read_adv_sequential_reads() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Read all forms from a string one by one, using the index
    let form = r####"(progn
  (fset 'neovm--test-read-all
    (lambda (str)
      (let ((pos 0)
            (len (length str))
            (forms nil))
        (condition-case nil
            (while (< pos len)
              ;; skip whitespace manually
              (while (and (< pos len)
                          (memq (aref str pos) '(?\s ?\t ?\n)))
                (setq pos (1+ pos)))
              (when (< pos len)
                (let ((result (read-from-string str pos)))
                  (setq forms (cons (car result) forms))
                  (setq pos (cdr result)))))
          (error nil))
        (nreverse forms))))

  (unwind-protect
      (list
        ;; Multiple integers
        (funcall 'neovm--test-read-all "1 2 3 4 5")
        ;; Mixed types
        (funcall 'neovm--test-read-all "42 \"hello\" (a b) [1 2] :key")
        ;; Nested forms
        (funcall 'neovm--test-read-all "(+ 1 2) (* 3 4) (list 'a 'b)")
        ;; With extra whitespace
        (funcall 'neovm--test-read-all "  10   20   30  ")
        ;; Single form
        (funcall 'neovm--test-read-all "(only-one)")
        ;; Empty string
        (funcall 'neovm--test-read-all ""))
    (fmakunbound 'neovm--test-read-all)))"####;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5) (42 \"hello\" (a b) [1 2] :key) ((+ 1 2) (* 3 4) (list 'a 'b)) (10 20 30) ((only-one)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error handling for malformed input
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_read_adv_error_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; Unclosed paren
  (condition-case err
      (read-from-string "(1 2 3")
    (error (list 'error (car err))))
  ;; Unclosed string
  (condition-case err
      (read-from-string "\"unclosed")
    (error (list 'error (car err))))
  ;; Unclosed vector
  (condition-case err
      (read-from-string "[1 2 3")
    (error (list 'error (car err))))
  ;; Extra close paren (reads fine but leftover is there)
  (let ((result (read-from-string "42)")))
    (list (car result) (cdr result)))
  ;; Empty string
  (condition-case err
      (read-from-string "")
    (error (list 'error (car err))))
  ;; Only whitespace
  (condition-case err
      (read-from-string "   ")
    (error (list 'error (car err))))
  ;; Invalid read syntax
  (condition-case err
      (read-from-string "#<invalid>")
    (error (list 'error (car err)))))"####;
    let expect = expect_test::expect![[
        r#""OK ((error end-of-file) (error end-of-file) (error end-of-file) (42 2) (error end-of-file) (error end-of-file) (error invalid-read-syntax))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: parse a mini-language from string representation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_read_adv_mini_language_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A mini-language: (define var expr), (if cond then else), (+ a b), (- a b), (print val)
    // We parse a program string into forms, then interpret them
    let form = r####"(progn
  (fset 'neovm--test-mini-eval
    (lambda (expr env)
      (cond
        ;; Number literal
        ((numberp expr) expr)
        ;; Symbol: look up in env
        ((symbolp expr)
         (let ((binding (assq expr env)))
           (if binding (cdr binding)
             (error "Unbound: %s" expr))))
        ;; List: dispatch on operator
        ((listp expr)
         (let ((op (car expr)))
           (cond
             ((eq op '+)
              (+ (funcall 'neovm--test-mini-eval (nth 1 expr) env)
                 (funcall 'neovm--test-mini-eval (nth 2 expr) env)))
             ((eq op '-)
              (- (funcall 'neovm--test-mini-eval (nth 1 expr) env)
                 (funcall 'neovm--test-mini-eval (nth 2 expr) env)))
             ((eq op '*)
              (* (funcall 'neovm--test-mini-eval (nth 1 expr) env)
                 (funcall 'neovm--test-mini-eval (nth 2 expr) env)))
             ((eq op 'if)
              (if (not (zerop (funcall 'neovm--test-mini-eval (nth 1 expr) env)))
                  (funcall 'neovm--test-mini-eval (nth 2 expr) env)
                (funcall 'neovm--test-mini-eval (nth 3 expr) env)))
             (t (error "Unknown op: %s" op)))))
        (t (error "Bad expr: %s" expr)))))

  (fset 'neovm--test-run-program
    (lambda (program-str)
      (let ((pos 0)
            (len (length program-str))
            (env nil)
            (results nil))
        (condition-case nil
            (while (< pos len)
              (while (and (< pos len)
                          (memq (aref program-str pos) '(?\s ?\t ?\n)))
                (setq pos (1+ pos)))
              (when (< pos len)
                (let* ((parsed (read-from-string program-str pos))
                       (stmt (car parsed)))
                  (setq pos (cdr parsed))
                  (cond
                    ;; (define var expr)
                    ((and (listp stmt) (eq (car stmt) 'define))
                     (let ((val (funcall 'neovm--test-mini-eval (nth 2 stmt) env)))
                       (setq env (cons (cons (nth 1 stmt) val) env))))
                    ;; Any other expression: evaluate and collect result
                    (t
                     (setq results
                           (cons (funcall 'neovm--test-mini-eval stmt env)
                                 results)))))))
          (error nil))
        (nreverse results))))

  (unwind-protect
      (list
        ;; Simple arithmetic
        (funcall 'neovm--test-run-program "(+ 1 2) (- 10 3) (* 4 5)")
        ;; Variables
        (funcall 'neovm--test-run-program
                 "(define x 10) (define y 20) (+ x y) (- y x)")
        ;; Nested expressions
        (funcall 'neovm--test-run-program
                 "(define a 3) (define b 4) (+ (* a a) (* b b))")
        ;; Conditionals
        (funcall 'neovm--test-run-program
                 "(define n 5) (if n (+ n 10) 0) (if 0 999 42)"))
    (fmakunbound 'neovm--test-mini-eval)
    (fmakunbound 'neovm--test-run-program)))"####;
    let expect = expect_test::expect![[r#""OK ((3 7 20) (30 10) (25) (15 42))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: read-from-string with hash table roundtrip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_read_adv_hash_table_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create hash table, serialize with prin1, read back, verify contents
    let form = r####"(let ((ht (make-hash-table :test 'equal)))
  (puthash "alpha" 1 ht)
  (puthash "beta" 2 ht)
  (puthash "gamma" 3 ht)
  (let* ((serialized (prin1-to-string ht))
         (restored (car (read-from-string serialized))))
    (list
      (hash-table-p restored)
      (hash-table-count restored)
      ;; All values present
      (gethash "alpha" restored)
      (gethash "beta" restored)
      (gethash "gamma" restored)
      ;; Non-existent key
      (gethash "delta" restored)
      ;; Test type preserved
      (hash-table-test restored))))"####;
    let expect = expect_test::expect![[r#""OK (t 3 1 2 3 nil equal)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

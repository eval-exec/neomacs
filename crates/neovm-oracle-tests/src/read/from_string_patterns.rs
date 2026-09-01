//! Oracle parity tests for `read-from-string` with complex patterns:
//! reading various data types, tracking positions, sequential parsing,
//! config parser implementation, tokenizer, and error handling.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Reading all basic types: integers, floats, strings, symbols, keywords
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rfs_patterns_all_basic_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; Integers: positive, negative, zero, explicit sign, large
  (car (read-from-string "0"))
  (car (read-from-string "1"))
  (car (read-from-string "-1"))
  (car (read-from-string "+42"))
  (car (read-from-string "999999"))
  (car (read-from-string "-999999"))
  ;; Floats: standard, scientific, negative exponent
  (car (read-from-string "0.0"))
  (car (read-from-string "3.14159"))
  (car (read-from-string "-2.718"))
  (car (read-from-string "1.0e10"))
  (car (read-from-string "1.5e-3"))
  (car (read-from-string "-6.022e23"))
  ;; Strings: empty, simple, with escapes
  (car (read-from-string "\"\""))
  (car (read-from-string "\"hello world\""))
  (car (read-from-string "\"line1\\nline2\""))
  (car (read-from-string "\"tab\\there\""))
  (car (read-from-string "\"quote \\\"inside\\\"\""))
  (car (read-from-string "\"backslash \\\\\""))
  ;; Symbols
  (car (read-from-string "foo"))
  (car (read-from-string "my-variable"))
  (car (read-from-string "nil"))
  (car (read-from-string "t"))
  (car (read-from-string "a/b"))
  ;; Keywords
  (car (read-from-string ":test"))
  (car (read-from-string ":equal"))
  (car (read-from-string ":my-keyword"))
  ;; Characters
  (car (read-from-string "?a"))
  (car (read-from-string "?\\n"))
  (car (read-from-string "?\\t")))"####;
    let expect = expect_test::expect![[
        r#""OK (0 1 -1 42 999999 -999999 0.0 3.14159 -2.718 10000000000.0 0.0015 -6.022e+23 \"\" \"hello world\" \"line1\\nline2\" \"tab\there\" \"quote \\\"inside\\\"\" \"backslash \\\\\" foo my-variable nil t a/b :test :equal :my-keyword 97 10 9)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Reading compound types: lists, vectors, cons cells, nested
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rfs_patterns_compound_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; Simple lists
  (car (read-from-string "(1 2 3)"))
  (car (read-from-string "(a b c d e)"))
  (car (read-from-string "()"))
  ;; Nested lists
  (car (read-from-string "((1 2) (3 4) (5 6))"))
  (car (read-from-string "(a (b (c (d))))"))
  ;; Mixed types in list
  (car (read-from-string "(1 \"two\" three 4.0 :five)"))
  ;; Cons cells / dotted pairs
  (car (read-from-string "(a . b)"))
  (car (read-from-string "(1 . 2)"))
  (car (read-from-string "(1 2 . 3)"))
  ;; Alists
  (car (read-from-string "((a . 1) (b . 2) (c . 3))"))
  ;; Vectors
  (car (read-from-string "[]"))
  (car (read-from-string "[1 2 3]"))
  (car (read-from-string "[a b c]"))
  (car (read-from-string "[[1 2] [3 4]]"))
  ;; Mixed vector
  (car (read-from-string "[1 \"two\" three]"))
  ;; Deeply nested
  (car (read-from-string "(((((deep)))))"))
  ;; List containing vector
  (car (read-from-string "(a [1 2] b)"))
  ;; Vector containing list
  (car (read-from-string "[(a b) (c d)]")))"####;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3) (a b c d e) nil ((1 2) (3 4) (5 6)) (a (b (c (d)))) (1 \"two\" three 4.0 :five) (a . b) (1 . 2) (1 2 . 3) ((a . 1) (b . 2) (c . 3)) [] [1 2 3] [a b c] [[1 2] [3 4]] [1 \"two\" three] (((((deep))))) (a [1 2] b) [(a b) (c d)])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Second return value: position tracking with START parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rfs_patterns_position_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; Position after simple values
  (cdr (read-from-string "42"))
  (cdr (read-from-string "hello"))
  (cdr (read-from-string "\"abc\""))
  (cdr (read-from-string "(1 2)"))
  (cdr (read-from-string "[a b]"))
  ;; Position with leading whitespace
  (cdr (read-from-string "  42"))
  (cdr (read-from-string "\t\nhello"))
  ;; Position with trailing content
  (cdr (read-from-string "42 rest"))
  (cdr (read-from-string "(a b) (c d)"))
  ;; Using START parameter
  (read-from-string "aaa bbb ccc" 0)
  (read-from-string "aaa bbb ccc" 4)
  (read-from-string "aaa bbb ccc" 8)
  ;; START parameter with various types
  (car (read-from-string "10 \"hello\" (a b)" 0))
  (car (read-from-string "10 \"hello\" (a b)" 3))
  (car (read-from-string "10 \"hello\" (a b)" 11))
  ;; Position after quoted forms
  (cdr (read-from-string "'foo rest"))
  (cdr (read-from-string "#'car rest"))
  ;; Verify substring extraction using returned position
  (let* ((s "alpha beta gamma")
         (r1 (read-from-string s))
         (r2 (read-from-string s (cdr r1)))
         (r3 (read-from-string s (cdr r2))))
    (list (car r1) (car r2) (car r3)
          (cdr r1) (cdr r2) (cdr r3))))"####;
    let expect = expect_test::expect![[
        r#""OK (2 5 5 5 5 4 7 2 5 (aaa . 3) (bbb . 7) (ccc . 11) 10 \"hello\" (a b) 4 5 (alpha beta gamma 5 10 16))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Reading quoted and backquoted expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rfs_patterns_quoted_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; quote
  (car (read-from-string "'x"))
  (car (read-from-string "'(1 2 3)"))
  (car (read-from-string "''x"))
  (equal (car (read-from-string "'x")) '(quote x))
  ;; function quote
  (car (read-from-string "#'car"))
  (car (read-from-string "#'(lambda (x) x)"))
  (equal (car (read-from-string "#'car")) '(function car))
  ;; backquote
  (car (read-from-string "`x"))
  (car (read-from-string "`(a b c)"))
  ;; backquote with comma
  (car (read-from-string "`(a ,b c)"))
  (car (read-from-string "`(a ,@b c)"))
  ;; Nested quoting
  (car (read-from-string "'(a 'b 'c)"))
  (car (read-from-string "`(a ,(+ 1 2) ,@(list 3 4))"))
  ;; Quoted vector
  (car (read-from-string "'[1 2 3]"))
  ;; Quoted dotted pair
  (car (read-from-string "'(a . b)"))
  ;; Hash notation
  (car (read-from-string "#t")))"####;
    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#t\")""##]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sequential reads from a string (tokenizer pattern)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rfs_patterns_sequential_tokenizer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Read successive forms from a string, collecting all objects and positions
    let form = r####"(progn
  (fset 'neovm--rfs-tokenize
    (lambda (str)
      "Read all forms from STR, returning list of (value . end-pos)."
      (let ((pos 0)
            (len (length str))
            (tokens nil))
        (condition-case nil
            (while (< pos len)
              ;; skip whitespace
              (while (and (< pos len)
                          (memq (aref str pos) '(?\s ?\t ?\n ?\r)))
                (setq pos (1+ pos)))
              (when (< pos len)
                (let ((result (read-from-string str pos)))
                  (setq tokens (cons result tokens))
                  (setq pos (cdr result)))))
          (error nil))
        (nreverse tokens))))

  (fset 'neovm--rfs-token-values
    (lambda (str)
      (mapcar #'car (funcall 'neovm--rfs-tokenize str))))

  (fset 'neovm--rfs-token-positions
    (lambda (str)
      (mapcar #'cdr (funcall 'neovm--rfs-tokenize str))))

  (unwind-protect
      (list
        ;; Simple number sequence
        (funcall 'neovm--rfs-token-values "1 2 3 4 5")
        ;; Mixed types
        (funcall 'neovm--rfs-token-values
                 "42 \"hello\" (a b) [1 2] :key 3.14 nil t")
        ;; Positions
        (funcall 'neovm--rfs-token-positions "aa bb cc")
        ;; Nested forms
        (funcall 'neovm--rfs-token-values
                 "(define x 10) (+ x 1) (* x x)")
        ;; Quoted forms in sequence
        (funcall 'neovm--rfs-token-values "'a 'b '(1 2) #'car")
        ;; Extra whitespace
        (funcall 'neovm--rfs-token-values "  10   20   30  ")
        ;; Single token
        (funcall 'neovm--rfs-token-values "(only-one)")
        ;; Empty string
        (funcall 'neovm--rfs-token-values "")
        ;; String with only whitespace
        (funcall 'neovm--rfs-token-values "   \t\n  "))
    (fmakunbound 'neovm--rfs-tokenize)
    (fmakunbound 'neovm--rfs-token-values)
    (fmakunbound 'neovm--rfs-token-positions)))"####;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5) (42 \"hello\" (a b) [1 2] :key 3.14 nil t) (2 5 8) ((define x 10) (+ x 1) (* x x)) ('a 'b '(1 2) #'car) (10 20 30) ((only-one)) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Config parser: read key-value pairs from a string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rfs_patterns_config_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse a config format: each form is a (key value) pair.
    // Support defaults, type checking, nested config sections.
    let form = r####"(progn
  (fset 'neovm--rfs-parse-config
    (lambda (config-str)
      "Parse config string into an alist of (key . value) pairs."
      (let ((pos 0)
            (len (length config-str))
            (entries nil))
        (condition-case nil
            (while (< pos len)
              (while (and (< pos len)
                          (memq (aref config-str pos) '(?\s ?\t ?\n ?\r)))
                (setq pos (1+ pos)))
              (when (< pos len)
                (let ((result (read-from-string config-str pos)))
                  (setq pos (cdr result))
                  (let ((form (car result)))
                    (when (and (listp form) (>= (length form) 2))
                      (setq entries (cons (cons (nth 0 form) (nth 1 form))
                                          entries)))))))
          (error nil))
        (nreverse entries))))

  (fset 'neovm--rfs-config-get
    (lambda (config key default)
      (let ((entry (assq key config)))
        (if entry (cdr entry) default))))

  (fset 'neovm--rfs-config-merge
    (lambda (base override)
      "Merge two configs, override taking precedence."
      (let ((result (copy-alist override)))
        (dolist (entry base)
          (unless (assq (car entry) result)
            (setq result (cons entry result))))
        result)))

  (unwind-protect
      (let* ((config-str "(name \"my-app\") (port 8080) (debug t) (workers 4) (log-level :info)")
             (config (funcall 'neovm--rfs-parse-config config-str))
             ;; Override config
             (override-str "(port 9090) (debug nil) (timeout 30)")
             (override (funcall 'neovm--rfs-parse-config override-str))
             (merged (funcall 'neovm--rfs-config-merge config override)))
        (list
          ;; Basic config parsing
          (funcall 'neovm--rfs-config-get config 'name nil)
          (funcall 'neovm--rfs-config-get config 'port 0)
          (funcall 'neovm--rfs-config-get config 'debug nil)
          (funcall 'neovm--rfs-config-get config 'workers 1)
          (funcall 'neovm--rfs-config-get config 'log-level :warn)
          ;; Default for missing key
          (funcall 'neovm--rfs-config-get config 'missing "default-val")
          ;; Override config
          (funcall 'neovm--rfs-config-get override 'port 0)
          ;; Merged config: override wins
          (funcall 'neovm--rfs-config-get merged 'port 0)
          (funcall 'neovm--rfs-config-get merged 'debug nil)
          ;; Merged config: base provides missing keys
          (funcall 'neovm--rfs-config-get merged 'name nil)
          (funcall 'neovm--rfs-config-get merged 'workers 1)
          ;; New key from override
          (funcall 'neovm--rfs-config-get merged 'timeout 0)
          ;; Number of entries
          (length config)
          (length merged)
          ;; Nested config: sections as sub-alists
          (let ((nested-str "(server (host \"localhost\") (port 3000)) (database (host \"db.local\") (port 5432))"))
            (funcall 'neovm--rfs-parse-config nested-str))))
    (fmakunbound 'neovm--rfs-parse-config)
    (fmakunbound 'neovm--rfs-config-get)
    (fmakunbound 'neovm--rfs-config-merge)))"####;
    let expect = expect_test::expect![[
        r#""OK (\"my-app\" 8080 t 4 :info \"default-val\" 9090 9090 nil \"my-app\" 4 30 5 6 ((server host \"localhost\") (database host \"db.local\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error handling with condition-case for malformed input
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rfs_patterns_error_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; Unclosed parenthesis
  (condition-case err
      (read-from-string "(1 2 3")
    (error (list 'caught (car err))))
  ;; Unclosed string
  (condition-case err
      (read-from-string "\"unclosed")
    (error (list 'caught (car err))))
  ;; Unclosed vector
  (condition-case err
      (read-from-string "[1 2")
    (error (list 'caught (car err))))
  ;; Empty string
  (condition-case err
      (read-from-string "")
    (error (list 'caught (car err))))
  ;; Only whitespace
  (condition-case err
      (read-from-string "     ")
    (error (list 'caught (car err))))
  ;; Invalid read syntax
  (condition-case err
      (read-from-string "#<buffer foo>")
    (error (list 'caught (car err))))
  ;; Extra close paren reads normally (partial)
  (let ((r (read-from-string "42) rest")))
    (list (car r) (cdr r)))
  ;; Read from position past end
  (condition-case err
      (read-from-string "abc" 10)
    (error (list 'caught (car err))))
  ;; Successful reads still work after errors
  (list
    (car (read-from-string "42"))
    (car (read-from-string "(a b c)"))
    (car (read-from-string "\"hello\""))))"####;
    let expect = expect_test::expect![[
        r#""OK ((caught end-of-file) (caught end-of-file) (caught end-of-file) (caught end-of-file) (caught end-of-file) (caught invalid-read-syntax) (42 2) (caught args-out-of-range) (42 (a b c) \"hello\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: S-expression evaluator using read-from-string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rfs_patterns_sexp_evaluator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse and evaluate arithmetic S-expressions from strings
    let form = r####"(progn
  (fset 'neovm--rfs-seval
    (lambda (expr)
      "Evaluate a parsed S-expression (arithmetic only)."
      (cond
        ((numberp expr) expr)
        ((and (listp expr) (eq (car expr) '+))
         (apply #'+ (mapcar (lambda (e) (funcall 'neovm--rfs-seval e))
                            (cdr expr))))
        ((and (listp expr) (eq (car expr) '-))
         (if (= (length (cdr expr)) 1)
             (- (funcall 'neovm--rfs-seval (cadr expr)))
           (- (funcall 'neovm--rfs-seval (cadr expr))
              (apply #'+ (mapcar (lambda (e) (funcall 'neovm--rfs-seval e))
                                 (cddr expr))))))
        ((and (listp expr) (eq (car expr) '*))
         (apply #'* (mapcar (lambda (e) (funcall 'neovm--rfs-seval e))
                            (cdr expr))))
        ((and (listp expr) (eq (car expr) 'max))
         (apply #'max (mapcar (lambda (e) (funcall 'neovm--rfs-seval e))
                              (cdr expr))))
        ((and (listp expr) (eq (car expr) 'min))
         (apply #'min (mapcar (lambda (e) (funcall 'neovm--rfs-seval e))
                              (cdr expr))))
        ((and (listp expr) (eq (car expr) 'abs))
         (abs (funcall 'neovm--rfs-seval (cadr expr))))
        (t (error "Unknown expr: %S" expr)))))

  (fset 'neovm--rfs-eval-string
    (lambda (str)
      "Parse a string into an S-expression and evaluate it."
      (funcall 'neovm--rfs-seval (car (read-from-string str)))))

  (fset 'neovm--rfs-eval-all
    (lambda (str)
      "Parse and evaluate all S-expressions in a string."
      (let ((pos 0) (len (length str)) (results nil))
        (condition-case nil
            (while (< pos len)
              (while (and (< pos len)
                          (memq (aref str pos) '(?\s ?\t ?\n)))
                (setq pos (1+ pos)))
              (when (< pos len)
                (let ((r (read-from-string str pos)))
                  (setq results (cons (funcall 'neovm--rfs-seval (car r)) results))
                  (setq pos (cdr r)))))
          (error nil))
        (nreverse results))))

  (unwind-protect
      (list
        ;; Simple arithmetic
        (funcall 'neovm--rfs-eval-string "(+ 1 2 3)")
        (funcall 'neovm--rfs-eval-string "(* 4 5)")
        (funcall 'neovm--rfs-eval-string "(- 10 3)")
        ;; Nested
        (funcall 'neovm--rfs-eval-string "(+ (* 2 3) (* 4 5))")
        (funcall 'neovm--rfs-eval-string "(- (* 10 10) (+ 1 2 3))")
        ;; Unary minus
        (funcall 'neovm--rfs-eval-string "(- 42)")
        ;; Multi-arg
        (funcall 'neovm--rfs-eval-string "(+ 1 2 3 4 5 6 7 8 9 10)")
        ;; Max/min/abs
        (funcall 'neovm--rfs-eval-string "(max 3 1 4 1 5 9)")
        (funcall 'neovm--rfs-eval-string "(min 3 1 4 1 5 9)")
        (funcall 'neovm--rfs-eval-string "(abs (- 5 10))")
        ;; Evaluate multiple expressions from one string
        (funcall 'neovm--rfs-eval-all "(+ 1 2) (* 3 4) (- 10 5)")
        ;; Complex nested
        (funcall 'neovm--rfs-eval-string
                 "(+ (max 10 20) (min 5 3) (* (abs (- 2 7)) 2))")
        ;; Just a number
        (funcall 'neovm--rfs-eval-string "42"))
    (fmakunbound 'neovm--rfs-seval)
    (fmakunbound 'neovm--rfs-eval-string)
    (fmakunbound 'neovm--rfs-eval-all)))"####;
    let expect = expect_test::expect![[r#""OK (6 20 7 26 94 -42 55 9 1 5 (3 12 5) 33 42)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

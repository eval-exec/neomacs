//! Comprehensive oracle parity tests for backquote/quasi-quote patterns:
//! deeply nested backquotes, splicing positions, backquote with let bindings,
//! macro definitions, complex data structure construction, computed symbols.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Deeply nested backquotes: backquote inside backquote (3+ levels)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_backquote_comprehensive_triple_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three levels of backquote nesting: outer resolves first-level commas,
    // leaving inner backquotes with their own comma patterns intact
    let form = r#"(let ((a 1) (b 2))
                    `(outer ,a `(middle ,a ,,b `(inner ,a ,,a ,,,b))))"#;
    let expect = expect_test::expect![r#""OK (outer 1 `(middle ,a ,2 `(inner ,a ,,a ,,2)))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_backquote_comprehensive_nested_eval_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a form via nested backquote, then eval it twice to fully resolve
    let form = r#"(let ((x 10))
                    (let ((template `(let ((y ,,x)) `(+ ,y ,,x))))
                      (list template
                            (eval template)
                            (eval (eval template)))))"#;
    let expect = expect_test::expect![[r#""ERR (void-function \\,)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Splicing into different positions and edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_backquote_comprehensive_splice_nested_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Splice into nested list structure at various depths
    let form = r#"(let ((xs '(1 2 3))
                        (ys '(a b))
                        (zs '()))
                    (list
                     ;; Splice at top level beginning, middle, end
                     `(,@xs middle ,@ys)
                     ;; Splice into nested sub-lists
                     `((head ,@xs) (,@ys tail))
                     ;; Multiple adjacent splices with empty list between
                     `(,@xs ,@zs ,@ys ,@zs end)
                     ;; Splice single-element list
                     `(before ,@'(only) after)
                     ;; Splice with dotted pair context
                     `(,@xs . final)))"#;
    let expect = expect_test::expect![
        r#""OK ((1 2 3 middle a b) ((head 1 2 3) (a b tail)) (1 2 3 a b end) (before only after) (1 2 3 . final))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_backquote_comprehensive_splice_computed_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Splice results of computations (mapcar, number-sequence, etc.)
    let form = r#"(list
                    ;; Splice mapcar result
                    `(squares ,@(mapcar (lambda (x) (* x x)) '(1 2 3 4 5)))
                    ;; Splice number-sequence
                    `(range ,@(number-sequence 5 10))
                    ;; Splice filtered list via remove-if-not (cl-remove-if-not)
                    `(evens ,@(let ((r nil))
                                (dolist (x '(1 2 3 4 5 6 7 8))
                                  (when (= 0 (% x 2))
                                    (setq r (cons x r))))
                                (nreverse r)))
                    ;; Splice reverse
                    `(reversed ,@(reverse '(a b c d))))"#;
    let expect = expect_test::expect![
        r#""OK ((squares 1 4 9 16 25) (range 5 6 7 8 9 10) (evens 2 4 6 8) (reversed d c b a))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Backquote with let bindings constructing complex forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_backquote_comprehensive_let_construction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use backquote to construct and evaluate let forms with computed bindings
    let form = r#"(let ((vars '(a b c d))
                        (vals '(10 20 30 40)))
                    ;; Build a let form with computed bindings and body
                    (let ((form `(let ,(mapcar #'list vars vals)
                                  (list ,@(mapcar (lambda (v) `(* ,v 2)) vars)))))
                      (list form (eval form))))"#;
    let expect = expect_test::expect![r#""ERR (wrong-number-of-arguments mapcar 3)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_backquote_comprehensive_nested_let_star() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Backquote constructing let* with forward references between bindings
    let form = r#"(let ((chain '((x 1) (y (+ x 2)) (z (* y 3)))))
                    (let ((form `(let* ,chain (list x y z))))
                      (eval form)))"#;
    let expect = expect_test::expect![r#""OK (1 3 9)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Backquote in macro definitions (defmacro)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_backquote_comprehensive_macro_with_gensym() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Macro using backquote with make-symbol for hygienic bindings,
    // splicing body, and nested backquote in expansion
    let form = r#"(progn
  (defmacro neovm--test-bqc-with-timing (label &rest body)
    "Execute BODY, return (LABEL result elapsed-ticks)."
    (let ((start-sym (make-symbol "start"))
          (result-sym (make-symbol "result")))
      `(let ((,start-sym 0)
             (,result-sym (progn ,@body)))
         (list ,label ,result-sym (+ ,start-sym 1)))))

  (unwind-protect
      (list
       (neovm--test-bqc-with-timing "add" (+ 1 2 3))
       (neovm--test-bqc-with-timing "mul" (* 4 5))
       ;; Nested use
       (neovm--test-bqc-with-timing "nested"
         (neovm--test-bqc-with-timing "inner" 42)))
    (fmakunbound 'neovm--test-bqc-with-timing)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"add\" 6 1) (\"mul\" 20 1) (\"nested\" (\"inner\" 42 1) 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_backquote_comprehensive_macro_generating_cond() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Macro that generates a cond form from a dispatch table
    let form = r#"(progn
  (defmacro neovm--test-bqc-dispatch (key &rest clauses)
    "Generate cond form dispatching on KEY. Each clause is (VALUE BODY...)."
    `(cond ,@(mapcar (lambda (clause)
                       `((equal ,key ',(car clause)) ,@(cdr clause)))
                     clauses)
           (t (list 'unknown ,key))))

  (unwind-protect
      (list
       (neovm--test-bqc-dispatch 'alpha (alpha 'found-alpha 1) (beta 'found-beta 2) (gamma 'found-gamma 3))
       (neovm--test-bqc-dispatch 'beta (alpha 'found-alpha 1) (beta 'found-beta 2) (gamma 'found-gamma 3))
       (neovm--test-bqc-dispatch 'gamma (alpha 'found-alpha 1) (beta 'found-beta 2) (gamma 'found-gamma 3))
       (neovm--test-bqc-dispatch 'delta (alpha 'found-alpha 1) (beta 'found-beta 2) (gamma 'found-gamma 3))
       ;; Verify macroexpansion shape
       (car (macroexpand '(neovm--test-bqc-dispatch x (a 1) (b 2)))))
    (fmakunbound 'neovm--test-bqc-dispatch)))"#;
    let expect = expect_test::expect![r#""OK (1 2 3 (unknown delta) cond)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex nested data structure construction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_backquote_comprehensive_nested_data_structures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build complex nested data: alists of alists, mixed vectors and lists
    let form = r#"(let ((fields '(name age score))
                        (records '(("Alice" 30 95) ("Bob" 25 88) ("Carol" 35 92))))
                    ;; Build alist-of-alists via backquote + mapcar
                    (mapcar (lambda (rec)
                              `((,(nth 0 fields) . ,(nth 0 rec))
                                (,(nth 1 fields) . ,(nth 1 rec))
                                (,(nth 2 fields) . ,(nth 2 rec))))
                            records))"#;
    let expect = expect_test::expect![[
        r#""OK (((name . \"Alice\") (age . 30) (score . 95)) ((name . \"Bob\") (age . 25) (score . 88)) ((name . \"Carol\") (age . 35) (score . 92)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_backquote_comprehensive_vector_in_backquote() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Backquote with vectors: `, inside vector context
    let form = r#"(let ((x 10) (y 20))
                    (list
                     `[,x ,y ,(+ x y)]
                     `(list [,x] [,y] [,(* x y)])
                     `[head ,@(list 1 2 3) tail]))"#;
    let expect =
        expect_test::expect![r#""OK ([10 20 30] (list [10] [20] [200]) [head 1 2 3 tail])""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Computed symbols via backquote and intern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_backquote_comprehensive_computed_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use backquote with intern to construct forms with computed symbol names
    let form = r#"(let ((prefix "neovm-bqc-test")
                        (suffixes '("x" "y" "z")))
                    ;; Build a list of (setq <computed-sym> <value>) forms
                    (let ((forms
                           (let ((i 0))
                             (mapcar (lambda (s)
                                       (setq i (1+ i))
                                       `(list ',(intern (concat prefix "-" s)) ,i))
                                     suffixes))))
                      ;; Eval each form
                      (mapcar #'eval forms)))"#;
    let expect = expect_test::expect![
        r#""OK ((neovm-bqc-test-x 1) (neovm-bqc-test-y 2) (neovm-bqc-test-z 3))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_backquote_comprehensive_defun_family() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generate a family of related functions via backquote + dolist + eval
    let form = r#"(progn
  (let ((ops '((add . +) (sub . -) (mul . *))))
    (dolist (op ops)
      (let ((fname (intern (concat "neovm--bqc-" (symbol-name (car op)))))
            (operator (cdr op)))
        (eval `(defun ,fname (a b) (,operator a b))))))
  (unwind-protect
      (list
       (neovm--bqc-add 3 4)
       (neovm--bqc-sub 10 3)
       (neovm--bqc-mul 6 7)
       (neovm--bqc-add (neovm--bqc-mul 2 3) (neovm--bqc-sub 10 4)))
    (fmakunbound 'neovm--bqc-add)
    (fmakunbound 'neovm--bqc-sub)
    (fmakunbound 'neovm--bqc-mul)))"#;
    let expect = expect_test::expect![r#""OK (7 7 42 12)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Backquote with quote and function quote interactions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_backquote_comprehensive_quote_interactions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Interactions between backquote, regular quote, and function quote
    let form = r#"(let ((fn-name 'car)
                        (val '(1 2 3)))
                    (list
                     ;; Backquote with quoted sub-form
                     `(quote ,(+ 1 2))
                     ;; Backquote producing a form with #'
                     `(funcall #',fn-name ',val)
                     ;; Evaluating the produced form
                     (eval `(funcall #',fn-name ',val))
                     ;; Nested quote inside backquote
                     `(a ',(+ 1 2) ',(* 3 4))
                     ;; Backquote producing backquote (meta-level)
                     (let ((inner `(list 1 2 ,(+ 1 2))))
                       `(eval ',inner))))"#;
    let expect = expect_test::expect![
        r#""OK ('3 (funcall #'car '(1 2 3)) 1 (a '3 '12) (eval '(list 1 2 3)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Backquote edge cases: nil splicing, self-referential patterns, conditionals
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_backquote_comprehensive_conditional_splicing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Conditionally splice or omit elements
    let form = r#"(let ((include-debug nil)
                        (include-logging t)
                        (extra-args '(verbose trace)))
                    (list
                     ;; Conditional splice: include items based on flags
                     `(config
                       ,@(when include-debug '(:debug t))
                       ,@(when include-logging '(:logging t))
                       ,@(when extra-args `(:extra ,@extra-args))
                       :end)
                     ;; Build function call with optional keyword args
                     `(call-fn required-arg
                       ,@(when include-debug '(:debug t))
                       ,@(when include-logging '(:log-level 3)))
                     ;; Nested conditional in backquote
                     `(progn
                       ,@(let ((forms nil))
                           (when include-logging
                             (setq forms (cons '(log "starting") forms)))
                           (setq forms (cons '(do-work) forms))
                           (when include-debug
                             (setq forms (cons '(debug-check) forms)))
                           (nreverse forms)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((config :logging t :extra verbose trace :end) (call-fn required-arg :log-level 3) (progn (log \"starting\") (do-work)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

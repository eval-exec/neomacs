//! Oracle parity tests for advanced apply/funcall patterns:
//! apply with spread args, funcall with computed function, apply with
//! variable-length arg lists, funcall in loops, apply with lambda,
//! partial application, compose functions, memoize pattern.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// apply with spread args and variable-length argument lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_funcall_spread_and_varargs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-spread-apply
    (lambda (f fixed-args var-args)
      "Apply F with FIXED-ARGS prepended to VAR-ARGS."
      (apply f (append fixed-args var-args))))

  (unwind-protect
      (list
        ;; Spread a fixed prefix + variable suffix
        (funcall 'neovm--test-spread-apply #'+ '(100 200) '(1 2 3))
        (funcall 'neovm--test-spread-apply #'list '(a b) '(c d e))
        (funcall 'neovm--test-spread-apply #'concat '("hello") '(" " "world" "!"))
        ;; Empty variable args
        (funcall 'neovm--test-spread-apply #'+ '(10 20) '())
        ;; Empty fixed args
        (funcall 'neovm--test-spread-apply #'* '() '(2 3 4))
        ;; Both empty
        (funcall 'neovm--test-spread-apply #'+ '() '())
        ;; Nested spread: build arg lists dynamically
        (let ((ops (list (list #'+ '(1 2) '(3 4))
                         (list #'* '(2) '(3 5))
                         (list #'concat '("a" "b") '("c")))))
          (mapcar (lambda (spec)
                    (funcall 'neovm--test-spread-apply
                             (nth 0 spec) (nth 1 spec) (nth 2 spec)))
                  ops))
        ;; Variable-length arg list construction
        (let ((make-args (lambda (n)
                           (let ((lst nil))
                             (dotimes (i n)
                               (setq lst (cons (1+ i) lst)))
                             (nreverse lst)))))
          (list (apply #'+ (funcall make-args 5))
                (apply #'+ (funcall make-args 10))
                (apply #'* (funcall make-args 6)))))
    (fmakunbound 'neovm--test-spread-apply)))"#;
    let expect = expect_test::expect![[
        r#""OK (306 (a b c d e) \"hello world!\" 30 24 0 (10 30 \"abc\") (15 55 720))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// funcall with computed/dynamic function selection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_funcall_computed_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-dispatch
    (lambda (op-name)
      "Return a function based on OP-NAME symbol."
      (cond
        ((eq op-name 'add) #'+)
        ((eq op-name 'mul) #'*)
        ((eq op-name 'sub) #'-)
        ((eq op-name 'cat) #'concat)
        ((eq op-name 'lst) #'list)
        (t #'identity))))

  (fset 'neovm--test-eval-rpn
    (lambda (tokens)
      "Evaluate a simple RPN (reverse Polish notation) expression.
       Numbers push to stack, symbols pop args and apply."
      (let ((stack nil))
        (dolist (tok tokens)
          (cond
            ((numberp tok)
             (setq stack (cons tok stack)))
            ((eq tok '+)
             (let ((b (pop stack)) (a (pop stack)))
               (setq stack (cons (+ a b) stack))))
            ((eq tok '-)
             (let ((b (pop stack)) (a (pop stack)))
               (setq stack (cons (- a b) stack))))
            ((eq tok '*)
             (let ((b (pop stack)) (a (pop stack)))
               (setq stack (cons (* a b) stack))))
            ((eq tok 'dup)
             (setq stack (cons (car stack) stack)))
            ((eq tok 'swap)
             (let ((a (pop stack)) (b (pop stack)))
               (setq stack (cons a (cons b stack)))))))
        (car stack))))

  (unwind-protect
      (list
        ;; Dispatch and funcall
        (funcall (funcall 'neovm--test-dispatch 'add) 10 20 30)
        (funcall (funcall 'neovm--test-dispatch 'mul) 2 3 4)
        (funcall (funcall 'neovm--test-dispatch 'sub) 100 30)
        (funcall (funcall 'neovm--test-dispatch 'cat) "foo" "bar")
        (funcall (funcall 'neovm--test-dispatch 'unknown) 42)
        ;; Dynamic dispatch in a loop
        (let ((operations '((add 1 2 3)
                            (mul 4 5)
                            (sub 100 42)
                            (cat "a" "b" "c")))
              (results nil))
          (dolist (op-spec operations)
            (let ((fn (funcall 'neovm--test-dispatch (car op-spec)))
                  (args (cdr op-spec)))
              (setq results (cons (apply fn args) results))))
          (nreverse results))
        ;; RPN evaluator
        (funcall 'neovm--test-eval-rpn '(3 4 + 2 *))
        (funcall 'neovm--test-eval-rpn '(5 1 2 + 4 * + 3 -))
        (funcall 'neovm--test-eval-rpn '(10 dup *))
        (funcall 'neovm--test-eval-rpn '(1 2 swap -)))
    (fmakunbound 'neovm--test-dispatch)
    (fmakunbound 'neovm--test-eval-rpn)))"#;
    let expect =
        expect_test::expect![[r#""OK (60 24 70 \"foobar\" 42 (6 20 58 \"abc\") 14 14 100 -1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// funcall in loops: accumulation, iteration, reduction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_funcall_in_loops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-fold-left
    (lambda (fn init lst)
      "Left fold: ((init fn x1) fn x2) fn x3 ..."
      (let ((acc init))
        (dolist (x lst)
          (setq acc (funcall fn acc x)))
        acc)))

  (fset 'neovm--test-fold-right
    (lambda (fn lst init)
      "Right fold: x1 fn (x2 fn (x3 fn init))"
      (let ((reversed (reverse lst))
            (acc init))
        (dolist (x reversed)
          (setq acc (funcall fn x acc)))
        acc)))

  (fset 'neovm--test-scan
    (lambda (fn init lst)
      "Scan (prefix sums): return list of all intermediate fold results."
      (let ((acc init)
            (results (list init)))
        (dolist (x lst)
          (setq acc (funcall fn acc x))
          (setq results (cons acc results)))
        (nreverse results))))

  (fset 'neovm--test-iterate
    (lambda (fn init n)
      "Apply FN to INIT repeatedly N times, return list of all values."
      (let ((val init)
            (results (list init)))
        (dotimes (_ n)
          (setq val (funcall fn val))
          (setq results (cons val results)))
        (nreverse results))))

  (unwind-protect
      (list
        ;; Left fold: sum, product, string building
        (funcall 'neovm--test-fold-left #'+ 0 '(1 2 3 4 5))
        (funcall 'neovm--test-fold-left #'* 1 '(1 2 3 4 5))
        (funcall 'neovm--test-fold-left
                 (lambda (acc x) (concat acc " " (number-to-string x)))
                 "nums:"
                 '(1 2 3))
        ;; Right fold: build list, difference
        (funcall 'neovm--test-fold-right #'cons '(1 2 3 4) nil)
        (funcall 'neovm--test-fold-right
                 (lambda (x acc) (- x acc))
                 '(1 2 3 4 5) 0)
        ;; Left fold vs right fold difference
        (list (funcall 'neovm--test-fold-left #'- 0 '(1 2 3))
              (funcall 'neovm--test-fold-right
                       (lambda (x acc) (- x acc))
                       '(1 2 3) 0))
        ;; Scan: running sum
        (funcall 'neovm--test-scan #'+ 0 '(1 2 3 4 5))
        ;; Scan: running max
        (funcall 'neovm--test-scan #'max 0 '(3 1 4 1 5 9 2 6))
        ;; Iterate: Collatz sequence from 6
        (funcall 'neovm--test-iterate
                 (lambda (n) (if (= (% n 2) 0) (/ n 2) (+ (* 3 n) 1)))
                 6 8)
        ;; Iterate: powers of 2
        (funcall 'neovm--test-iterate (lambda (x) (* x 2)) 1 10))
    (fmakunbound 'neovm--test-fold-left)
    (fmakunbound 'neovm--test-fold-right)
    (fmakunbound 'neovm--test-scan)
    (fmakunbound 'neovm--test-iterate)))"#;
    let expect = expect_test::expect![[
        r#""OK (15 120 \"nums: 1 2 3\" (1 2 3 4) 3 (-6 2) (0 1 3 6 10 15) (0 3 3 4 4 5 9 9 9) (6 3 10 5 16 8 4 2 1) (1 2 4 8 16 32 64 128 256 512 1024))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// apply with lambda and closure capture
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_funcall_lambda_closures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-make-adder
    (lambda (n)
      "Return a closure that adds N to its argument."
      (lambda (x) (+ x n))))

  (fset 'neovm--test-make-counter
    (lambda (&optional start step)
      "Return a counter closure with :next, :peek, :reset ops."
      (let ((val (or start 0))
            (s (or step 1))
            (initial (or start 0)))
        (lambda (op)
          (cond
            ((eq op :next)
             (let ((cur val))
               (setq val (+ val s))
               cur))
            ((eq op :peek) val)
            ((eq op :reset)
             (setq val initial)
             initial))))))

  (unwind-protect
      (list
        ;; apply lambda directly
        (apply (lambda (a b c) (+ (* a b) c)) '(3 4 5))
        ;; apply closure
        (let ((add5 (funcall 'neovm--test-make-adder 5)))
          (list (funcall add5 10)
                (funcall add5 -3)
                (apply add5 '(100))))
        ;; Multiple closures sharing nothing
        (let ((adders (mapcar (lambda (n) (funcall 'neovm--test-make-adder n))
                              '(1 5 10 100))))
          (mapcar (lambda (adder) (funcall adder 42)) adders))
        ;; Counter closure
        (let ((c (funcall 'neovm--test-make-counter 0 3)))
          (list (funcall c :next)   ;; 0
                (funcall c :next)   ;; 3
                (funcall c :next)   ;; 6
                (funcall c :peek)   ;; 9
                (funcall c :reset)  ;; 0
                (funcall c :next))) ;; 0
        ;; Funcall with lambda that captures loop variable
        (let ((fns nil))
          (dotimes (i 5)
            (let ((captured i))
              (setq fns (cons (lambda () (* captured captured)) fns))))
          (mapcar #'funcall (nreverse fns)))
        ;; apply with dynamically constructed lambda args
        (let ((make-fn (lambda (op)
                         (cond
                           ((eq op 'sum) (lambda (&rest args)
                                           (let ((s 0)) (dolist (a args) (setq s (+ s a))) s)))
                           ((eq op 'product) (lambda (&rest args)
                                               (let ((p 1)) (dolist (a args) (setq p (* p a))) p)))
                           ((eq op 'count) (lambda (&rest args) (length args)))))))
          (list (apply (funcall make-fn 'sum) '(1 2 3 4 5))
                (apply (funcall make-fn 'product) '(1 2 3 4 5))
                (apply (funcall make-fn 'count) '(a b c d e f)))))
    (fmakunbound 'neovm--test-make-adder)
    (fmakunbound 'neovm--test-make-counter)))"#;
    let expect = expect_test::expect![
        r#""OK (17 (15 2 105) (43 47 52 142) (0 3 6 9 0 0) (0 1 4 9 16) (15 120 6))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Partial application and currying via apply/funcall
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_funcall_partial_curry() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-partial
    (lambda (f &rest bound-args)
      "Partially apply F with BOUND-ARGS."
      (lambda (&rest remaining)
        (apply f (append bound-args remaining)))))

  (fset 'neovm--test-rpartial
    (lambda (f &rest bound-args)
      "Right-partial: bind trailing arguments."
      (lambda (&rest leading)
        (apply f (append leading bound-args)))))

  (fset 'neovm--test-flip
    (lambda (f)
      "Flip the first two arguments of F."
      (lambda (a b &rest rest)
        (apply f b a rest))))

  (fset 'neovm--test-complement
    (lambda (pred)
      "Return negation of predicate PRED."
      (lambda (&rest args)
        (not (apply pred args)))))

  (unwind-protect
      (list
        ;; Left partial application
        (let ((add10 (funcall 'neovm--test-partial #'+ 10))
              (mul3 (funcall 'neovm--test-partial #'* 3))
              (prepend-hello (funcall 'neovm--test-partial #'concat "hello ")))
          (list (funcall add10 5)
                (funcall add10 -3)
                (funcall mul3 7)
                (funcall prepend-hello "world")))
        ;; Right partial application
        (let ((div-by-2 (funcall 'neovm--test-rpartial #'/ 2))
              (sub-from-100 (funcall 'neovm--test-rpartial #'- 100)))
          ;; Note: (/ x 2) and (- x 100)... but rpartial appends, so
          ;; div-by-2(10) = (/ 10 2) = 5, sub-from-100(150) = (- 150 100) = 50
          (list (funcall div-by-2 10)
                (funcall sub-from-100 150)))
        ;; Flip
        (let ((flipped-cons (funcall 'neovm--test-flip #'cons))
              (flipped-sub (funcall 'neovm--test-flip #'-)))
          (list (funcall flipped-cons 1 2)     ;; (cons 2 1) = (2 . 1)
                (funcall flipped-sub 3 10)))   ;; (- 10 3) = 7
        ;; Complement
        (let ((not-null (funcall 'neovm--test-complement #'null))
              (not-zerop (funcall 'neovm--test-complement #'zerop)))
          (list (funcall not-null nil)
                (funcall not-null t)
                (funcall not-zerop 0)
                (funcall not-zerop 5)))
        ;; Compose partial applications: partial(partial(+, 10), applied to mapcar)
        (let ((add10 (funcall 'neovm--test-partial #'+ 10)))
          (mapcar add10 '(1 2 3 4 5)))
        ;; Chain: flip + partial
        (let* ((flipped-nth (funcall 'neovm--test-flip #'nth))
               (get-second (funcall 'neovm--test-partial flipped-nth 1)))
          (mapcar get-second '((a b c) (x y z) (1 2 3))))
        ;; Complement in filter
        (let ((not-numberp (funcall 'neovm--test-complement #'numberp))
              (data '(1 "a" 2 "b" nil 3 t)))
          (let ((result nil))
            (dolist (x data)
              (when (funcall not-numberp x)
                (setq result (cons x result))))
            (nreverse result))))
    (fmakunbound 'neovm--test-partial)
    (fmakunbound 'neovm--test-rpartial)
    (fmakunbound 'neovm--test-flip)
    (fmakunbound 'neovm--test-complement)))"#;
    let expect = expect_test::expect![r#""ERR (wrong-type-argument integerp (a b c))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Function composition via apply/funcall
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_funcall_compose_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-compose
    (lambda (&rest fns)
      "Compose functions right-to-left."
      (if (null fns)
          #'identity
        (let ((last-fn (car (last fns)))
              (rest-fns (nreverse (cdr (nreverse (copy-sequence fns))))))
          (lambda (&rest args)
            (let ((result (apply last-fn args)))
              (dolist (f (reverse rest-fns))
                (setq result (funcall f result)))
              result))))))

  (fset 'neovm--test-pipe
    (lambda (&rest fns)
      "Compose functions left-to-right (pipe)."
      (apply 'neovm--test-compose (reverse fns))))

  (fset 'neovm--test-on
    (lambda (cmp key-fn)
      "Create comparison function that extracts key first."
      (lambda (a b)
        (funcall cmp (funcall key-fn a) (funcall key-fn b)))))

  (unwind-protect
      (list
        ;; Basic compose
        (funcall (funcall 'neovm--test-compose
                          (lambda (x) (* x 2))
                          (lambda (x) (+ x 1)))
                 5)
        ;; Triple compose
        (funcall (funcall 'neovm--test-compose
                          #'number-to-string
                          (lambda (x) (* x x))
                          (lambda (x) (+ x 3)))
                 4)
        ;; Pipe (left-to-right)
        (funcall (funcall 'neovm--test-pipe
                          (lambda (x) (+ x 3))
                          (lambda (x) (* x x))
                          #'number-to-string)
                 4)
        ;; Empty compose = identity
        (funcall (funcall 'neovm--test-compose) 42)
        ;; Compose with variadic last function
        (funcall (funcall 'neovm--test-compose
                          (lambda (x) (* x 2))
                          #'+)
                 3 4 5)
        ;; on: sort records by field
        (let ((records '((alice . 30) (bob . 25) (carol . 35) (dave . 28)))
              (by-age (funcall 'neovm--test-on #'< #'cdr))
              (by-name (funcall 'neovm--test-on #'string< (lambda (r) (symbol-name (car r))))))
          (list (sort (copy-sequence records) by-age)
                (sort (copy-sequence records) by-name)))
        ;; Compose in a data processing pipeline
        (let* ((normalize (lambda (lst)
                            (let ((mx (apply #'max lst)))
                              (mapcar (lambda (x) (/ (float x) mx)) lst))))
               (square-all (lambda (lst) (mapcar (lambda (x) (* x x)) lst)))
               (sum-list (lambda (lst)
                           (let ((s 0)) (dolist (x lst) (setq s (+ s x))) s)))
               (pipeline (funcall 'neovm--test-pipe
                                  normalize square-all sum-list)))
          (funcall pipeline '(3 6 9 12))))
    (fmakunbound 'neovm--test-compose)
    (fmakunbound 'neovm--test-pipe)
    (fmakunbound 'neovm--test-on)))"#;
    let expect = expect_test::expect![[
        r#""OK (12 \"49\" \"49\" 42 24 (((bob . 25) (dave . 28) (alice . 30) (carol . 35)) ((alice . 30) (bob . 25) (carol . 35) (dave . 28))) 1.875)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Memoization pattern using funcall
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_funcall_memoize() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-memoize
    (lambda (f)
      "Memoize a single-arg function. Returns closure with :call/:stats/:reset."
      (let ((cache (make-hash-table :test 'equal))
            (calls 0)
            (hits 0))
        (lambda (op &rest args)
          (cond
            ((eq op :call)
             (let* ((key (car args))
                    (cached (gethash key cache :miss)))
               (if (not (eq cached :miss))
                   (progn (setq hits (1+ hits)) cached)
                 (setq calls (1+ calls))
                 (let ((result (funcall f key)))
                   (puthash key result cache)
                   result))))
            ((eq op :stats)
             (list :calls calls :hits hits :cached (hash-table-count cache)))
            ((eq op :reset)
             (clrhash cache) (setq calls 0 hits 0) nil))))))

  (fset 'neovm--test-memoize2
    (lambda (f)
      "Memoize a two-arg function."
      (let ((cache (make-hash-table :test 'equal)))
        (lambda (a b)
          (let* ((key (cons a b))
                 (cached (gethash key cache :miss)))
            (if (not (eq cached :miss))
                cached
              (let ((result (funcall f a b)))
                (puthash key result cache)
                result)))))))

  (unwind-protect
      (list
        ;; Memoize fibonacci
        (let* ((fib-calls 0)
               (memo-fib (funcall 'neovm--test-memoize
                                  (lambda (n)
                                    (setq fib-calls (1+ fib-calls))
                                    (if (< n 2) n
                                      (+ (funcall memo-fib :call (- n 1))
                                         (funcall memo-fib :call (- n 2))))))))
          (list (funcall memo-fib :call 0)
                (funcall memo-fib :call 1)
                (funcall memo-fib :call 10)
                (funcall memo-fib :call 15)
                ;; Calling 15 again should be a cache hit
                (funcall memo-fib :call 15)
                (funcall memo-fib :stats)))
        ;; Memoize string processing
        (let ((memo-upper (funcall 'neovm--test-memoize #'upcase)))
          (funcall memo-upper :call "hello")
          (funcall memo-upper :call "world")
          (funcall memo-upper :call "hello")
          (funcall memo-upper :call "hello")
          (list (funcall memo-upper :call "world")
                (funcall memo-upper :stats)))
        ;; Memoize2: two-arg memoization
        (let ((memo-pow (funcall 'neovm--test-memoize2
                                 (lambda (base exp)
                                   (let ((result 1))
                                     (dotimes (_ exp) (setq result (* result base)))
                                     result)))))
          (list (funcall memo-pow 2 10)
                (funcall memo-pow 3 5)
                (funcall memo-pow 2 10)
                (funcall memo-pow 3 5)
                ;; Different args
                (funcall memo-pow 5 3))))
    (fmakunbound 'neovm--test-memoize)
    (fmakunbound 'neovm--test-memoize2)))"#;
    let expect = expect_test::expect![r#""ERR (void-variable memo-fib)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Trampoline pattern: apply/funcall for tail-call optimization
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_funcall_trampoline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-trampoline
    (lambda (thunk)
      "Execute THUNK repeatedly while it returns a function (thunk).
       When it returns a non-function value, return that value."
      (let ((result thunk))
        (while (functionp result)
          (setq result (funcall result)))
        result)))

  (fset 'neovm--test-factorial-tramp
    (lambda (n)
      "Compute factorial using trampoline for tail-call elimination."
      (funcall 'neovm--test-trampoline
               (let ((helper nil))
                 (setq helper
                       (lambda (n acc)
                         (if (<= n 1)
                             acc
                           (let ((nn (1- n))
                                 (nacc (* acc n)))
                             (lambda () (funcall helper nn nacc))))))
                 (lambda () (funcall helper n 1))))))

  (fset 'neovm--test-even-odd-tramp
    (lambda (n)
      "Mutual recursion via trampoline: check if N is even."
      (let ((is-even nil) (is-odd nil))
        (setq is-even
              (lambda (n)
                (if (= n 0) t
                  (let ((nn (1- n)))
                    (lambda () (funcall is-odd nn))))))
        (setq is-odd
              (lambda (n)
                (if (= n 0) nil
                  (let ((nn (1- n)))
                    (lambda () (funcall is-even nn))))))
        (funcall 'neovm--test-trampoline
                 (lambda () (funcall is-even n))))))

  (unwind-protect
      (list
        ;; Factorial via trampoline
        (funcall 'neovm--test-factorial-tramp 1)
        (funcall 'neovm--test-factorial-tramp 5)
        (funcall 'neovm--test-factorial-tramp 10)
        (funcall 'neovm--test-factorial-tramp 12)
        ;; Even/odd via mutual recursion trampoline
        (funcall 'neovm--test-even-odd-tramp 0)
        (funcall 'neovm--test-even-odd-tramp 1)
        (funcall 'neovm--test-even-odd-tramp 10)
        (funcall 'neovm--test-even-odd-tramp 99)
        (funcall 'neovm--test-even-odd-tramp 100)
        ;; Large values that would overflow stack without trampoline
        (funcall 'neovm--test-even-odd-tramp 500)
        (funcall 'neovm--test-even-odd-tramp 501))
    (fmakunbound 'neovm--test-trampoline)
    (fmakunbound 'neovm--test-factorial-tramp)
    (fmakunbound 'neovm--test-even-odd-tramp)))"#;
    let expect = expect_test::expect![r#""OK (1 120 3628800 479001600 t nil t nil t t nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

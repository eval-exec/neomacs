//! Comprehensive oracle parity tests for lexical closures:
//! creation/invocation, captured variable mutation, shared environments,
//! higher-order usage, factory patterns, recursion via closures,
//! let/let* interactions, serialization, identity/equality, and
//! closure capture of loop variables.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic closure creation and invocation with all argument types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_lexical_creation_invocation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test closures with mandatory, optional, and rest parameters,
    // capturing multiple outer variables simultaneously.
    let form = r#"(let ((base 100)
                        (scale 3))
      (let ((f-mandatory (lambda (x y) (+ (* scale x) (* scale y) base)))
            (f-optional  (lambda (x &optional y z)
                           (+ base x (or y 0) (or z 0))))
            (f-rest      (lambda (x &rest others)
                           (let ((sum base))
                             (setq sum (+ sum x))
                             (while others
                               (setq sum (+ sum (car others)))
                               (setq others (cdr others)))
                             (* scale sum))))
            (f-mixed     (lambda (a b &optional c &rest ds)
                           (let ((acc (+ a b base)))
                             (when c (setq acc (+ acc c)))
                             (while ds
                               (setq acc (+ acc (car ds)))
                               (setq ds (cdr ds)))
                             acc))))
        (list
          (funcall f-mandatory 1 2)              ;; 3 + 6 + 100 = 109
          (funcall f-optional 5)                 ;; 100 + 5 = 105
          (funcall f-optional 5 10)              ;; 100 + 5 + 10 = 115
          (funcall f-optional 5 10 20)           ;; 100 + 5 + 10 + 20 = 135
          (funcall f-rest 1)                     ;; 3 * 101 = 303
          (funcall f-rest 1 2 3 4)               ;; 3 * 110 = 330
          (funcall f-mixed 1 2)                  ;; 1 + 2 + 100 = 103
          (funcall f-mixed 1 2 3)                ;; 106
          (funcall f-mixed 1 2 3 4 5 6))))"#;
    let expect = expect_test::expect![r#""OK (109 105 115 135 303 330 103 106 121)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Captured variable mutation visible across closures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_lexical_captured_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two closures (writer and reader) sharing a mutable alist.
    // The writer inserts key-value pairs; the reader retrieves them.
    let form = r#"(let ((store nil))
      (let ((put-kv (lambda (k v)
                      (let ((existing (assq k store)))
                        (if existing
                            (setcdr existing v)
                          (setq store (cons (cons k v) store))))))
            (get-kv (lambda (k)
                      (cdr (assq k store))))
            (keys   (lambda ()
                      (mapcar #'car (reverse store))))
            (size   (lambda () (length store))))
        (funcall put-kv 'x 1)
        (funcall put-kv 'y 2)
        (funcall put-kv 'z 3)
        (funcall put-kv 'x 10)  ;; overwrite
        (list
          (funcall get-kv 'x)   ;; 10
          (funcall get-kv 'y)   ;; 2
          (funcall get-kv 'z)   ;; 3
          (funcall get-kv 'w)   ;; nil (not found)
          (funcall keys)        ;; (x y z)
          (funcall size))))"#;
    let expect = expect_test::expect![r#""OK (10 2 3 nil (x y z) 3)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Shared environment between multiple closures (state machine)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_lexical_shared_env_state_machine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A simple state machine: idle -> running -> paused -> running -> done.
    // Multiple closures share the state variable and transition history.
    let form = r#"(let ((state 'idle)
                        (history nil))
      (let ((transition
              (lambda (new-state)
                (setq history (cons (cons state new-state) history))
                (setq state new-state)))
            (current (lambda () state))
            (trail   (lambda () (reverse history))))
        (funcall transition 'running)
        (funcall transition 'paused)
        (funcall transition 'running)
        (funcall transition 'done)
        (list
          (funcall current)
          (length (funcall trail))
          (funcall trail))))"#;
    let expect = expect_test::expect![[
        r#""OK (done 4 ((idle . running) (running . paused) (paused . running) (running . done)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure as function argument (higher-order patterns)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_lexical_higher_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Higher-order: compose, pipe, filter-map, and fold-right,
    // all accepting and returning closures.
    let form = r#"(let ((compose2
                     (lambda (f g) (lambda (x) (funcall f (funcall g x)))))
                    (pipe
                     (lambda (val &rest fns)
                       (let ((result val))
                         (while fns
                           (setq result (funcall (car fns) result))
                           (setq fns (cdr fns)))
                         result)))
                    (filter-map
                     (lambda (pred transform lst)
                       (let ((acc nil))
                         (dolist (x lst)
                           (when (funcall pred x)
                             (setq acc (cons (funcall transform x) acc))))
                         (nreverse acc))))
                    (fold-right
                     (lambda (fn init lst)
                       (if (null lst)
                           init
                         (funcall fn (car lst)
                                  (funcall fold-right fn init (cdr lst)))))))
      (let ((double (lambda (x) (* 2 x)))
            (inc    (lambda (x) (1+ x)))
            (evenp  (lambda (x) (= 0 (% x 2))))
            (square (lambda (x) (* x x))))
        (list
          ;; compose: inc(double(5)) = 11
          (funcall (funcall compose2 inc double) 5)
          ;; pipe: 3 -> double -> inc -> square = 49
          (funcall pipe 3 double inc square)
          ;; filter-map: evens from 1..8, squared
          (funcall filter-map evenp square '(1 2 3 4 5 6 7 8))
          ;; pipe with multiple stages
          (funcall pipe 1 inc inc inc inc inc))))"#;
    let expect = expect_test::expect![r#""OK (11 49 (4 16 36 64) 6)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure returning closure (factory/currying patterns)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_lexical_factory_currying() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multi-level factory: make-comparator returns a closure that itself
    // returns closures for various comparison operations.
    let form = r#"(let ((make-range-checker
                     (lambda (lo hi)
                       (let ((in-range (lambda (x) (and (>= x lo) (<= x hi))))
                             (clamp    (lambda (x) (max lo (min hi x))))
                             (width    (lambda () (- hi lo))))
                         (lambda (op &rest args)
                           (cond
                             ((eq op 'check)  (funcall in-range (car args)))
                             ((eq op 'clamp)  (funcall clamp (car args)))
                             ((eq op 'width)  (funcall width))
                             ((eq op 'bounds) (list lo hi))))))))
      (let ((r1 (funcall make-range-checker 0 100))
            (r2 (funcall make-range-checker -10 10)))
        (list
          (funcall r1 'check 50)      ;; t
          (funcall r1 'check 150)     ;; nil
          (funcall r1 'clamp 150)     ;; 100
          (funcall r1 'clamp -5)      ;; 0
          (funcall r1 'width)         ;; 100
          (funcall r1 'bounds)        ;; (0 100)
          (funcall r2 'check 0)       ;; t
          (funcall r2 'check 11)      ;; nil
          (funcall r2 'clamp -999)    ;; -10
          (funcall r2 'width))))"#;
    let expect = expect_test::expect![r#""OK (t nil 100 0 100 (0 100) t nil -10 20)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure in recursive context (Y-combinator style)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_lexical_recursive_context() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use a letrec-like pattern (set the variable after creating the closure)
    // to implement recursive closures for factorial and fibonacci.
    let form = r#"(let ((fact nil)
                        (fib nil)
                        (flatten nil))
      (setq fact (lambda (n)
                   (if (<= n 1) 1
                     (* n (funcall fact (1- n))))))
      (setq fib (lambda (n)
                  (cond ((<= n 0) 0)
                        ((= n 1) 1)
                        (t (+ (funcall fib (- n 1))
                              (funcall fib (- n 2)))))))
      (setq flatten (lambda (tree)
                      (cond ((null tree) nil)
                            ((not (consp tree)) (list tree))
                            (t (append (funcall flatten (car tree))
                                       (funcall flatten (cdr tree)))))))
      (list
        (funcall fact 0)
        (funcall fact 1)
        (funcall fact 5)
        (funcall fact 10)
        (funcall fib 0)
        (funcall fib 1)
        (funcall fib 7)
        (funcall fib 10)
        (funcall flatten '(1 (2 (3 4) 5) (6 (7 (8)))))))"#;
    let expect = expect_test::expect![r#""OK (1 1 120 3628800 0 1 13 55 (1 2 3 4 5 6 7 8))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure with let and let* bindings — interaction patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_lexical_let_letstar_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // let* allows sequential binding: each subsequent binding sees the prior ones.
    // Closures created at different points capture different states.
    let form = r#"(let* ((a 1)
                         (b (+ a 10))          ;; 11
                         (f1 (lambda () (list a b)))
                         (a 100)               ;; shadows outer a (but let* rebinds)
                         (c (* a 2))           ;; this a is actually the let* re-binding
                         ;; Note: in Emacs let*, each binding is sequential
                         )
      ;; f1 was created when a=1, b=11
      ;; After the let* the local a is now 100 in the same scope.
      ;; But f1 captured the binding cell, and the cell was mutated to 100 by the later let*.
      ;; Actually in Emacs, let* creates new bindings; each line is a new binding.
      ;; So f1 captured the first a=1. The second a=100 is a DIFFERENT binding.
      ;; Let's verify this precisely.
      (list
        (funcall f1)    ;; f1 sees a=1, b=11
        a               ;; current scope sees a=100
        c               ;; 200
        ;; Now demonstrate let vs let*:
        (let ((x 5)
              (y 10))
          ;; In let, x and y are bound simultaneously
          (let ((f (lambda () (+ x y))))
            (let ((x 999))
              ;; f captured x=5 from outer let
              (funcall f))))
        ;; let* sequential reference
        (let* ((p 3)
               (q (* p p))    ;; 9
               (r (+ p q)))   ;; 12
          (let ((f (lambda () (list p q r))))
            (funcall f)))))"#;
    let expect = expect_test::expect![r#""OK ((1 11) 100 200 15 (3 9 12))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure serialization (prin1-to-string structure)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_lexical_serialization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that prin1-to-string on closures produces the expected structure.
    // Closures print as (closure ENV ARGLIST BODY...).
    let form = r#"(let ((x 42))
      (let ((f (lambda (a b) (+ a b x))))
        (let ((s (prin1-to-string f)))
          (list
            ;; The string should start with "(closure"
            (string-prefix-p "(closure" s)
            ;; It should contain the argument list
            (string-match-p "(a b)" s)
            ;; It should contain the body
            (string-match-p "(\\+ a b x)" s)
            ;; A closure with no captured vars (but still lexical)
            (let ((g (lambda (z) (* z z))))
              (string-prefix-p "(closure" (prin1-to-string g)))
            ;; Verify closurep
            (functionp f)
            ;; type-of for closures
            (type-of f)))))"#;
    let expect = expect_test::expect![r#""OK (nil 2 9 nil t interpreted-function)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure identity and equality: eq, equal, eql
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_lexical_identity_equality() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two closures with identical source but different environments are not eq.
    // The same closure reference is eq to itself.
    let form = r#"(let ((make-adder (lambda (n) (lambda (x) (+ n x)))))
      (let ((a5  (funcall make-adder 5))
            (b5  (funcall make-adder 5))
            (a10 (funcall make-adder 10)))
        (let ((a5-ref a5))
          (list
            ;; Same object is eq
            (eq a5 a5-ref)
            ;; Different objects are not eq even with same captured value
            (eq a5 b5)
            ;; Different captured values are definitely not eq
            (eq a5 a10)
            ;; equal on closures — in Emacs, closures are compared structurally
            ;; Two closures with same env are equal
            (equal a5 b5)
            ;; Different env -> not equal
            (equal a5 a10)
            ;; eql behaves like eq for non-number non-char
            (eql a5 a5-ref)
            (eql a5 b5)
            ;; Functionally equivalent (same result) but structurally equal
            (= (funcall a5 10) (funcall b5 10))
            ;; Different functional results
            (/= (funcall a5 10) (funcall a10 10))))))"#;
    let expect = expect_test::expect![r#""OK (t nil nil t nil t nil t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closures capturing loop variables: proper capture patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_lexical_loop_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Classic closure-in-loop problem: capturing a loop variable.
    // Without inner let, all closures share the same binding.
    // With inner let, each closure captures its own copy.
    let form = r#"(let ((bad-closures nil)
                        (good-closures nil))
      ;; "Bad" pattern: all closures share the same i binding
      (let ((i 0))
        (while (< i 5)
          (setq bad-closures (cons (lambda () i) bad-closures))
          (setq i (1+ i))))
      ;; All bad-closures return the final value of i (5)
      (let ((bad-results (mapcar #'funcall (reverse bad-closures))))
        ;; "Good" pattern: each closure captures its own copy via inner let
        (let ((j 0))
          (while (< j 5)
            (let ((captured j))
              (setq good-closures (cons (lambda () captured) good-closures)))
            (setq j (1+ j))))
        (let ((good-results (mapcar #'funcall (reverse good-closures))))
          (list bad-results good-results))))"#;
    let expect = expect_test::expect![r#""OK ((5 5 5 5 5) (0 1 2 3 4))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure with dolist capturing each element
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_lexical_dolist_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use dolist with an inner let to capture each element.
    // Each closure formats the captured element in a unique way.
    let form = r#"(let ((formatters nil))
      (dolist (fmt '(("prefix-" . "-suffix")
                     ("<<" . ">>")
                     ("[" . "]")))
        (let ((pre (car fmt))
              (suf (cdr fmt)))
          (setq formatters
                (cons (lambda (s) (concat pre s suf))
                      formatters))))
      (let ((fns (reverse formatters)))
        (list
          (funcall (nth 0 fns) "hello")
          (funcall (nth 1 fns) "hello")
          (funcall (nth 2 fns) "hello")
          ;; Cross-apply
          (mapcar (lambda (fn) (funcall fn "X")) fns))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"prefix-hello-suffix\" \"<<hello>>\" \"[hello]\" (\"prefix-X-suffix\" \"<<X>>\" \"[X]\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure as method table (encapsulated module pattern)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_lexical_module_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create a module with private state and exported methods via a dispatch
    // closure. The internal helpers are not accessible from outside.
    let form = r#"(let ((make-deque
                     (lambda ()
                       (let ((front nil)
                             (back nil)
                             (len 0))
                         (let ((normalize
                                 (lambda ()
                                   (when (null front)
                                     (setq front (nreverse back))
                                     (setq back nil)))))
                           (lambda (op &rest args)
                             (cond
                               ((eq op 'push-front)
                                (setq front (cons (car args) front))
                                (setq len (1+ len)))
                               ((eq op 'push-back)
                                (setq back (cons (car args) back))
                                (setq len (1+ len)))
                               ((eq op 'pop-front)
                                (funcall normalize)
                                (if (null front)
                                    (signal 'error '("empty deque"))
                                  (let ((val (car front)))
                                    (setq front (cdr front))
                                    (setq len (1- len))
                                    val)))
                               ((eq op 'peek-front)
                                (funcall normalize)
                                (car front))
                               ((eq op 'size) len)
                               ((eq op 'empty-p) (= len 0))
                               ((eq op 'to-list)
                                (funcall normalize)
                                (append front nil)))))))))
      (let ((dq (funcall make-deque)))
        (funcall dq 'push-back 1)
        (funcall dq 'push-back 2)
        (funcall dq 'push-back 3)
        (funcall dq 'push-front 0)
        (funcall dq 'push-front -1)
        (let ((after-push (funcall dq 'to-list))
              (sz (funcall dq 'size)))
          (let ((v1 (funcall dq 'pop-front))
                (v2 (funcall dq 'pop-front)))
            (list
              after-push
              sz
              v1 v2
              (funcall dq 'size)
              (funcall dq 'peek-front)
              (funcall dq 'empty-p))))))"#;
    let expect = expect_test::expect![r#""OK ((-1 0) 5 -1 0 3 1 nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

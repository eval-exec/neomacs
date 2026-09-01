//! Complex oracle tests for interpreter/evaluator patterns in Elisp.
//!
//! Tests implementation of mini-languages, pattern matchers,
//! state machines, and rule engines using Elisp primitives.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Pattern matcher
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp_pattern_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simple pattern matcher: match values against patterns
    let form = "(progn
  (fset 'neovm--test-pmatch
    (lambda (pattern value)
      (cond
        ((eq pattern '_) t)
        ((and (symbolp pattern) (not (keywordp pattern)))
         t)
        ((and (numberp pattern) (numberp value))
         (= pattern value))
        ((and (stringp pattern) (stringp value))
         (string= pattern value))
        ((and (consp pattern) (consp value)
              (eq (car pattern) 'cons))
         (and (funcall 'neovm--test-pmatch (cadr pattern) (car value))
              (funcall 'neovm--test-pmatch (caddr pattern) (cdr value))))
        ((and (consp pattern) (eq (car pattern) 'or))
         (let ((alts (cdr pattern)) (found nil))
           (while (and alts (not found))
             (setq found
                   (funcall 'neovm--test-pmatch (car alts) value))
             (setq alts (cdr alts)))
           found))
        (t nil))))
  (unwind-protect
      (list
        (funcall 'neovm--test-pmatch '_ 42)
        (funcall 'neovm--test-pmatch 42 42)
        (funcall 'neovm--test-pmatch 42 43)
        (funcall 'neovm--test-pmatch '(cons x y) '(1 . 2))
        (funcall 'neovm--test-pmatch '(cons 1 _) '(1 . 2))
        (funcall 'neovm--test-pmatch '(cons 1 _) '(2 . 3))
        (funcall 'neovm--test-pmatch '(or 1 2 3) 2)
        (funcall 'neovm--test-pmatch '(or 1 2 3) 5))
    (fmakunbound 'neovm--test-pmatch)))";
    let expect = expect_test::expect![[r#""OK (t t nil t t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Rule engine
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp_rule_engine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Apply rules to facts, derive new facts
    let form = "(let ((facts (make-hash-table :test 'equal))
                      (rules
                       ;; Each rule: (condition-fn . derive-fn)
                       ;; condition-fn: facts -> bool
                       ;; derive-fn: facts -> list of new facts
                       (list
                        (cons (lambda (fs) (gethash 'raining fs))
                              (lambda (_fs) '((wet-ground . t))))
                        (cons (lambda (fs) (gethash 'wet-ground fs))
                              (lambda (_fs) '((slippery . t))))
                        (cons (lambda (fs)
                                (and (gethash 'cold fs)
                                     (gethash 'raining fs)))
                              (lambda (_fs) '((ice-risk . t)))))))
                  ;; Initial facts
                  (puthash 'raining t facts)
                  (puthash 'cold t facts)
                  ;; Forward-chain inference (max 10 iterations)
                  (let ((changed t) (iter 0))
                    (while (and changed (< iter 10))
                      (setq changed nil iter (1+ iter))
                      (dolist (rule rules)
                        (when (funcall (car rule) facts)
                          (dolist (derived (funcall (cdr rule) facts))
                            (unless (gethash (car derived) facts)
                              (puthash (car derived)
                                       (cdr derived) facts)
                              (setq changed t)))))))
                  ;; Check derived facts
                  (list (gethash 'raining facts)
                        (gethash 'wet-ground facts)
                        (gethash 'slippery facts)
                        (gethash 'ice-risk facts)
                        (gethash 'sunny facts)))";
    let expect = expect_test::expect![[r#""OK (t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Finite state machine with actions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp_fsm_with_actions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // FSM for parsing a simple number: optional sign, digits, optional dot, digits
    let form = r#"(let ((transitions (make-hash-table :test 'equal))
                        (accept (make-hash-table)))
                    ;; State transitions: (state, char-class) -> next-state
                    (puthash '(start sign) 'integer transitions)
                    (puthash '(start digit) 'integer transitions)
                    (puthash '(integer digit) 'integer transitions)
                    (puthash '(integer dot) 'fraction transitions)
                    (puthash '(fraction digit) 'fraction transitions)
                    ;; Accept states
                    (puthash 'integer t accept)
                    (puthash 'fraction t accept)
                    ;; Classify characters
                    (let ((classify
                           (lambda (ch)
                             (cond
                               ((or (= ch ?+) (= ch ?-)) 'sign)
                               ((and (>= ch ?0) (<= ch ?9)) 'digit)
                               ((= ch ?.) 'dot)
                               (t 'other)))))
                      (let ((run-fsm
                             (lambda (input)
                               (let ((state 'start)
                                     (i 0)
                                     (len (length input)))
                                 (while (and (< i len) state)
                                   (let ((cls (funcall classify
                                                       (aref input i))))
                                     (setq state
                                           (gethash
                                            (list state cls)
                                            transitions nil)))
                                   (setq i (1+ i)))
                                 (and state (gethash state accept))))))
                        (list
                          (funcall run-fsm "42")
                          (funcall run-fsm "+42")
                          (funcall run-fsm "3.14")
                          (funcall run-fsm "-0.5")
                          (funcall run-fsm "abc")
                          (funcall run-fsm "1.2.3")
                          (funcall run-fsm "")))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Simple expression evaluator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp_arithmetic_evaluator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Evaluate arithmetic expressions represented as S-expressions
    let form = "(progn
  (fset 'neovm--test-arith-eval
    (lambda (expr env)
      (cond
        ((numberp expr) expr)
        ((symbolp expr)
         (let ((binding (assq expr env)))
           (if binding (cdr binding)
             (signal 'error (list (format \"unbound: %s\" expr))))))
        ((consp expr)
         (let ((op (car expr))
               (args (cdr expr)))
           (cond
             ((eq op 'let)
              ;; (let ((var val)) body)
              (let ((bindings (car args))
                    (body (cadr args)))
                (let ((new-env env))
                  (dolist (b bindings)
                    (let ((val (funcall 'neovm--test-arith-eval
                                        (cadr b) env)))
                      (setq new-env
                            (cons (cons (car b) val) new-env))))
                  (funcall 'neovm--test-arith-eval body new-env))))
             ((memq op '(+ - * /))
              (let ((vals (mapcar
                           (lambda (a)
                             (funcall 'neovm--test-arith-eval a env))
                           args)))
                (cond
                  ((eq op '+) (apply #'+ vals))
                  ((eq op '-) (apply #'- vals))
                  ((eq op '*) (apply #'* vals))
                  ((eq op '/) (apply #'/ vals)))))
             ((eq op 'if)
              (if (not (= 0 (funcall 'neovm--test-arith-eval
                                      (car args) env)))
                  (funcall 'neovm--test-arith-eval (cadr args) env)
                (funcall 'neovm--test-arith-eval (caddr args) env)))
             (t (signal 'error
                        (list (format \"unknown op: %s\" op)))))))
        (t (signal 'error (list \"invalid expr\"))))))
  (unwind-protect
      (list
        (funcall 'neovm--test-arith-eval '(+ 1 2 3) nil)
        (funcall 'neovm--test-arith-eval
                 '(let ((x 10) (y 20)) (+ x y)) nil)
        (funcall 'neovm--test-arith-eval
                 '(let ((x 5))
                    (if x (* x x) 0))
                 nil)
        (funcall 'neovm--test-arith-eval
                 '(let ((a 3) (b 4))
                    (+ (* a a) (* b b)))
                 nil))
    (fmakunbound 'neovm--test-arith-eval)))";
    let expect = expect_test::expect![[r#""OK (6 30 25 25)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Event system / pub-sub
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp_event_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simple pub-sub event system
    let form = "(let ((handlers (make-hash-table))
                      (log nil))
                  (let ((on
                         (lambda (event handler)
                           (puthash event
                                    (cons handler
                                          (gethash event handlers nil))
                                    handlers)))
                        (emit
                         (lambda (event data)
                           (dolist (h (gethash event handlers nil))
                             (funcall h data)))))
                    ;; Register handlers
                    (funcall on 'click
                             (lambda (d)
                               (setq log (cons (list 'click-a d) log))))
                    (funcall on 'click
                             (lambda (d)
                               (setq log (cons (list 'click-b d) log))))
                    (funcall on 'hover
                             (lambda (d)
                               (setq log (cons (list 'hover d) log))))
                    ;; Emit events
                    (funcall emit 'click 'button-1)
                    (funcall emit 'hover 'menu)
                    (funcall emit 'click 'button-2)
                    (funcall emit 'keypress nil)
                    (nreverse log)))";
    let expect = expect_test::expect![[
        r#""OK ((click-b button-1) (click-a button-1) (hover menu) (click-b button-2) (click-a button-2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Visitor pattern over heterogeneous tree
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interp_visitor_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Visit a tree of different node types
    let form = "(progn
  (fset 'neovm--test-visit
    (lambda (node visitor)
      (let ((type (car node)))
        (let ((result (funcall visitor type node)))
          (when (memq type '(group seq))
            (dolist (child (cdr node))
              (funcall 'neovm--test-visit child visitor)))
          result))))
  (unwind-protect
      (let ((tree '(group
                     (text . \"hello\")
                     (seq
                      (num . 42)
                      (num . 7))
                     (text . \"world\")))
            (counts (make-hash-table)))
        (funcall 'neovm--test-visit tree
                 (lambda (type _node)
                   (puthash type
                            (1+ (gethash type counts 0))
                            counts)))
        (let ((result nil))
          (maphash (lambda (k v)
                     (setq result (cons (cons k v) result)))
                   counts)
          (sort result (lambda (a b)
                         (string-lessp (symbol-name (car a))
                                       (symbol-name (car b)))))))
    (fmakunbound 'neovm--test-visit)))";
    let expect = expect_test::expect![[r#""OK ((group . 1) (num . 2) (seq . 1) (text . 2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

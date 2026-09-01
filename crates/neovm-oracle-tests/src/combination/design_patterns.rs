//! Complex software design patterns implemented in Elisp:
//! strategy pattern, chain of responsibility, builder pattern,
//! visitor pattern (double dispatch), decorator pattern,
//! and memento pattern (save/restore state).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Strategy pattern: interchangeable algorithms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dp_strategy_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A "sorter context" that accepts different comparison strategies.
    // Each strategy is a closure. The context uses the strategy to
    // perform an insertion sort.
    let form = r#"(let ((make-sorter
                     (lambda (compare-fn)
                       (lambda (items)
                         ;; Insertion sort using the strategy
                         (let ((sorted nil))
                           (dolist (item items)
                             (let ((inserted nil)
                                   (result nil)
                                   (rest sorted))
                               (while (and rest (not inserted))
                                 (if (funcall compare-fn item (car rest))
                                     (progn
                                       (setq result (append (nreverse result)
                                                            (cons item rest)))
                                       (setq inserted t))
                                   (setq result (cons (car rest) result))
                                   (setq rest (cdr rest))))
                               (if inserted
                                   (setq sorted result)
                                 (setq sorted (append (nreverse result)
                                                      (list item))))))
                           sorted)))))
      ;; Strategies
      (let ((ascending  (funcall make-sorter (lambda (a b) (< a b))))
            (descending (funcall make-sorter (lambda (a b) (> a b))))
            (by-abs     (funcall make-sorter (lambda (a b) (< (abs a) (abs b)))))
            (data '(3 -1 4 -1 5 -9 2 6)))
        (list
          (funcall ascending data)
          (funcall descending data)
          (funcall by-abs data)
          ;; Strategy for strings by length
          (let ((by-length (funcall make-sorter
                                    (lambda (a b) (< (length a) (length b))))))
            (funcall by-length '("elephant" "cat" "a" "dogs" "be"))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((-9 -1 -1 2 3 4 5 6) (6 5 4 3 2 -1 -1 -9) (-1 -1 2 3 4 5 6 -9) (\"a\" \"be\" \"cat\" \"dogs\" \"elephant\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Chain of responsibility pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dp_chain_of_responsibility() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A chain of handlers; each either handles a request or passes it
    // to the next handler. Request is a plist with :type and :data.
    let form = r#"(let ((make-handler
                     (lambda (name can-handle-fn handle-fn next)
                       (lambda (request)
                         (if (funcall can-handle-fn request)
                             (funcall handle-fn request name)
                           (if next
                               (funcall next request)
                             (list 'unhandled (plist-get request :type))))))))
      ;; Build chain: auth -> validation -> processing -> nil
      (let* ((processor
              (funcall make-handler "processor"
                       (lambda (r) (eq (plist-get r :type) 'compute))
                       (lambda (r name)
                         (list 'processed name
                               (* (plist-get r :data) 2)))
                       nil))
             (validator
              (funcall make-handler "validator"
                       (lambda (r) (eq (plist-get r :type) 'validate))
                       (lambda (r name)
                         (let ((val (plist-get r :data)))
                           (if (and (numberp val) (> val 0))
                               (list 'valid name val)
                             (list 'invalid name val))))
                       processor))
             (auth
              (funcall make-handler "auth"
                       (lambda (r) (eq (plist-get r :type) 'auth))
                       (lambda (r name)
                         (let ((token (plist-get r :data)))
                           (if (equal token "secret123")
                               (list 'authenticated name)
                             (list 'denied name))))
                       validator)))
        (list
          ;; Auth request
          (funcall auth '(:type auth :data "secret123"))
          (funcall auth '(:type auth :data "wrong"))
          ;; Validation request (passes through auth)
          (funcall auth '(:type validate :data 42))
          (funcall auth '(:type validate :data -5))
          ;; Compute request (passes through auth and validator)
          (funcall auth '(:type compute :data 21))
          ;; Unknown request (unhandled)
          (funcall auth '(:type unknown :data nil)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((authenticated \"auth\") (denied \"auth\") (valid \"validator\" 42) (invalid \"validator\" -5) (processed \"processor\" 42) (unhandled unknown))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Builder pattern (fluent-style API)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dp_builder_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A query builder that accumulates clauses and produces a
    // structured query representation. Each method returns the
    // builder (alist) for chaining.
    let form = r#"(let ((make-query-builder
                     (lambda ()
                       (list (cons 'table nil)
                             (cons 'fields nil)
                             (cons 'conditions nil)
                             (cons 'order nil)
                             (cons 'limit nil))))
                    (qb-set
                     (lambda (builder key val)
                       (mapcar (lambda (pair)
                                 (if (eq (car pair) key)
                                     (cons key val)
                                   pair))
                               builder)))
                    (qb-append
                     (lambda (builder key val)
                       (mapcar (lambda (pair)
                                 (if (eq (car pair) key)
                                     (cons key (append (cdr pair) (list val)))
                                   pair))
                               builder)))
                    (qb-build
                     (lambda (builder)
                       (let ((tbl (cdr (assq 'table builder)))
                             (flds (cdr (assq 'fields builder)))
                             (conds (cdr (assq 'conditions builder)))
                             (ord (cdr (assq 'order builder)))
                             (lim (cdr (assq 'limit builder))))
                         (list
                          :select (or flds '("*"))
                          :from tbl
                          :where conds
                          :order-by ord
                          :limit lim)))))
      ;; Build a query fluent-style
      (let* ((q1 (funcall make-query-builder))
             (q2 (funcall qb-set q1 'table "users"))
             (q3 (funcall qb-append q2 'fields "name"))
             (q4 (funcall qb-append q3 'fields "email"))
             (q5 (funcall qb-append q4 'conditions '(age > 18)))
             (q6 (funcall qb-append q5 'conditions '(active = t)))
             (q7 (funcall qb-set q6 'order "name"))
             (q8 (funcall qb-set q7 'limit 10))
             (result (funcall qb-build q8)))
        ;; Also build a simpler query
        (let* ((s1 (funcall make-query-builder))
               (s2 (funcall qb-set s1 'table "logs"))
               (s3 (funcall qb-set s2 'limit 100))
               (simple (funcall qb-build s3)))
          (list result simple
                ;; Original builder unchanged (immutable)
                (funcall qb-build q1)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:select (\"name\" \"email\") :from \"users\" :where ((age > 18) (active = t)) :order-by \"name\" :limit 10) (:select (\"*\") :from \"logs\" :where nil :order-by nil :limit 100) (:select (\"*\") :from nil :where nil :order-by nil :limit nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Visitor pattern (double dispatch on tree nodes)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dp_visitor_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // AST nodes represented as tagged lists. A visitor dispatches
    // based on node type. We implement two visitors: one that
    // evaluates the expression tree, another that pretty-prints it.
    let form = r#"(let ((visit nil)
                    (eval-visitor nil)
                    (print-visitor nil))
      ;; Generic visit: dispatch on (car node) to the appropriate
      ;; visitor method
      (setq visit
            (lambda (visitor node)
              (let ((type (car node))
                    (handler nil))
                (setq handler (cdr (assq type visitor)))
                (if handler
                    (funcall handler node)
                  (list 'unknown-node type)))))
      ;; Context visitor
      (setq eval-visitor
            (list
             (cons 'num (lambda (node) (cadr node)))
             (cons 'add (lambda (node)
                          (+ (funcall visit eval-visitor (cadr node))
                             (funcall visit eval-visitor (caddr node)))))
             (cons 'mul (lambda (node)
                          (* (funcall visit eval-visitor (cadr node))
                             (funcall visit eval-visitor (caddr node)))))
             (cons 'neg (lambda (node)
                          (- 0 (funcall visit eval-visitor (cadr node)))))))
      ;; Printer visitor
      (setq print-visitor
            (list
             (cons 'num (lambda (node)
                          (number-to-string (cadr node))))
             (cons 'add (lambda (node)
                          (format "(%s + %s)"
                                  (funcall visit print-visitor (cadr node))
                                  (funcall visit print-visitor (caddr node)))))
             (cons 'mul (lambda (node)
                          (format "(%s * %s)"
                                  (funcall visit print-visitor (cadr node))
                                  (funcall visit print-visitor (caddr node)))))
             (cons 'neg (lambda (node)
                          (format "(-%s)"
                                  (funcall visit print-visitor (cadr node)))))))
      ;; Build AST: (3 + 4) * -(2 + 5)
      (let ((tree '(mul (add (num 3) (num 4))
                        (neg (add (num 2) (num 5))))))
        (list
          (funcall visit eval-visitor tree)
          (funcall visit print-visitor tree)
          ;; Simpler tree: 10 + 20
          (funcall visit eval-visitor '(add (num 10) (num 20)))
          (funcall visit print-visitor '(add (num 10) (num 20)))
          ;; Single number
          (funcall visit eval-visitor '(num 42))
          (funcall visit print-visitor '(num 42))
          ;; Nested negation
          (funcall visit eval-visitor '(neg (neg (num 7))))
          (funcall visit print-visitor '(neg (neg (num 7)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (-49 \"((3 + 4) * (-(2 + 5)))\" 30 \"(10 + 20)\" 42 \"42\" 7 \"(-(-7))\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Decorator pattern (wrapping functions with added behavior)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dp_decorator_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Decorators wrap a function, adding behavior before/after the
    // original call. Multiple decorators can be stacked.
    let form = r#"(let ((make-logging-decorator
                     (lambda (name fn)
                       (let ((call-log nil))
                         (list
                          ;; Decorated function
                          (lambda (&rest args)
                            (setq call-log
                                  (cons (list 'call name args) call-log))
                            (let ((result (apply fn args)))
                              (setq call-log
                                    (cons (list 'return name result) call-log))
                              result))
                          ;; Log accessor
                          (lambda () (nreverse call-log))))))
                    (make-caching-decorator
                     (lambda (fn)
                       (let ((cache (make-hash-table :test 'equal)))
                         (lambda (&rest args)
                           (let ((key (prin1-to-string args)))
                             (let ((cached (gethash key cache)))
                               (or cached
                                   (let ((result (apply fn args)))
                                     (puthash key result cache)
                                     result))))))))
                    (make-validation-decorator
                     (lambda (pred error-msg fn)
                       (lambda (&rest args)
                         (dolist (arg args)
                           (unless (funcall pred arg)
                             (error "%s: %S" error-msg arg)))
                         (apply fn args)))))
      ;; Base function
      (let ((add (lambda (a b) (+ a b))))
        ;; Stack decorators: validate -> cache -> log -> add
        (let* ((validated-add
                (funcall make-validation-decorator
                         #'numberp "Not a number" add))
               (cached-add
                (funcall make-caching-decorator validated-add))
               (logged (funcall make-logging-decorator "add" cached-add))
               (decorated-add (car logged))
               (get-log (cadr logged)))
          (list
            ;; Normal calls
            (funcall decorated-add 3 4)
            (funcall decorated-add 10 20)
            ;; Cached call (same args)
            (funcall decorated-add 3 4)
            ;; Check log
            (funcall get-log)
            ;; Validation failure
            (condition-case err
                (funcall decorated-add "x" 1)
              (error (cadr err)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (7 30 7 ((call \"add\" (3 4)) (return \"add\" 7) (call \"add\" (10 20)) (return \"add\" 30) (call \"add\" (3 4)) (return \"add\" 7)) \"Not a number: \\\"x\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Memento pattern (save/restore state snapshots)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dp_memento_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An "editor" object with text and cursor position. Mementos
    // capture snapshots that can be restored. Implements undo/redo
    // via a memento stack.
    let form = r#"(let ((text "")
                    (cursor 0)
                    (undo-stack nil)
                    (redo-stack nil))
      (let ((save-memento
             (lambda ()
               (list (cons 'text text)
                     (cons 'cursor cursor))))
            (restore-memento
             (lambda (memento)
               (setq text (cdr (assq 'text memento)))
               (setq cursor (cdr (assq 'cursor memento)))))
            (editor-insert
             (lambda (s)
               (setq undo-stack (cons (funcall save-memento) undo-stack))
               (setq redo-stack nil)
               (setq text (concat (substring text 0 cursor)
                                  s
                                  (substring text cursor)))
               (setq cursor (+ cursor (length s)))))
            (editor-delete
             (lambda (n)
               (setq undo-stack (cons (funcall save-memento) undo-stack))
               (setq redo-stack nil)
               (let ((del-end (min (+ cursor n) (length text))))
                 (setq text (concat (substring text 0 cursor)
                                    (substring text del-end))))))
            (editor-move
             (lambda (pos)
               (setq cursor (max 0 (min pos (length text))))))
            (editor-undo
             (lambda ()
               (when undo-stack
                 (setq redo-stack (cons (funcall save-memento) redo-stack))
                 (funcall restore-memento (car undo-stack))
                 (setq undo-stack (cdr undo-stack)))))
            (editor-redo
             (lambda ()
               (when redo-stack
                 (setq undo-stack (cons (funcall save-memento) undo-stack))
                 (funcall restore-memento (car redo-stack))
                 (setq redo-stack (cdr redo-stack)))))
            (editor-state
             (lambda ()
               (list text cursor))))
        ;; Perform edits
        (funcall editor-insert "Hello")
        (let ((s1 (funcall editor-state)))
          (funcall editor-insert " World")
          (let ((s2 (funcall editor-state)))
            (funcall editor-move 5)
            (funcall editor-insert ",")
            (let ((s3 (funcall editor-state)))
              ;; Undo: remove the comma
              (funcall editor-undo)
              (let ((s4 (funcall editor-state)))
                ;; Undo: remove " World"
                (funcall editor-undo)
                (let ((s5 (funcall editor-state)))
                  ;; Redo: re-add " World"
                  (funcall editor-redo)
                  (let ((s6 (funcall editor-state)))
                    ;; New edit clears redo stack
                    (funcall editor-insert "!")
                    (let ((s7 (funcall editor-state)))
                      ;; Redo should do nothing now
                      (funcall editor-redo)
                      (let ((s8 (funcall editor-state)))
                        (list s1 s2 s3 s4 s5 s6 s7
                              (equal s7 s8))))))))))))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable save-memento)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

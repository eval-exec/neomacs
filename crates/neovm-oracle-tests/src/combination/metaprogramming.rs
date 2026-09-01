//! Complex oracle tests for metaprogramming patterns in Elisp.
//!
//! Tests code generation, struct-like macros, DSL for state machines,
//! template expansion, reflective property systems, and macroexpand
//! inspection.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Code that generates code (defun generators)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_meta_defun_generator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A macro that generates a family of predicate functions from a spec
    let form = r#"(progn
                    (defmacro neovm--test-defpredicates (prefix &rest specs)
                      "Generate predicate functions from SPECS.
Each spec is (NAME PRED-BODY)."
                      (cons 'progn
                            (mapcar
                             (lambda (spec)
                               (let ((name (intern (concat (symbol-name prefix)
                                                           "-"
                                                           (symbol-name (car spec)))))
                                     (body (cadr spec)))
                                 `(fset ',name (lambda (x) ,body))))
                             specs)))
                    (unwind-protect
                        (progn
                          (neovm--test-defpredicates neovm--test-check
                            (positive (> x 0))
                            (negative (< x 0))
                            (zero (= x 0))
                            (even (= (% x 2) 0))
                            (odd (/= (% x 2) 0)))
                          (list
                           ;; Test generated predicates
                           (mapcar (lambda (n)
                                     (list n
                                           (funcall 'neovm--test-check-positive n)
                                           (funcall 'neovm--test-check-negative n)
                                           (funcall 'neovm--test-check-zero n)
                                           (funcall 'neovm--test-check-even n)
                                           (funcall 'neovm--test-check-odd n)))
                                   '(-3 -2 -1 0 1 2 3))
                           ;; Use predicates as higher-order functions
                           (let ((nums '(-5 -4 -3 -2 -1 0 1 2 3 4 5)))
                             (list
                              (length (seq-filter 'neovm--test-check-positive nums))
                              (length (seq-filter 'neovm--test-check-even nums))))))
                      (fmakunbound 'neovm--test-defpredicates)
                      (fmakunbound 'neovm--test-check-positive)
                      (fmakunbound 'neovm--test-check-negative)
                      (fmakunbound 'neovm--test-check-zero)
                      (fmakunbound 'neovm--test-check-even)
                      (fmakunbound 'neovm--test-check-odd)))"#;
    let expect = expect_test::expect![[
        r#""OK (((-3 nil t nil nil t) (-2 nil t nil t nil) (-1 nil t nil nil t) (0 nil nil t t nil) (1 t nil nil nil t) (2 t nil nil t nil) (3 t nil nil nil t)) (5 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Struct-like macro system (accessors, constructors)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_meta_struct_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full struct system: constructor, accessors, predicate, copier
    let form = r#"(progn
                    (defmacro neovm--test-defrecord (name &rest fields)
                      "Define a record type with constructor, accessors, predicate, and copier."
                      (let* ((name-str (symbol-name name))
                             (make-fn (intern (concat "neovm--test-make-" name-str)))
                             (pred-fn (intern (concat "neovm--test-" name-str "-p")))
                             (copy-fn (intern (concat "neovm--test-copy-" name-str)))
                             (tag (intern (concat "neovm--test-" name-str)))
                             (n (length fields))
                             (accessor-defs
                              (let ((i 0) (acc nil))
                                (dolist (f fields)
                                  (let ((getter (intern (concat "neovm--test-" name-str
                                                                "-" (symbol-name f))))
                                        (setter (intern (concat "neovm--test-set-" name-str
                                                                "-" (symbol-name f)))))
                                    (setq acc
                                          (cons `(fset ',getter
                                                       (lambda (obj) (aref obj ,(1+ i))))
                                                (cons `(fset ',setter
                                                             (lambda (obj val)
                                                               (aset obj ,(1+ i) val)
                                                               val))
                                                      acc)))
                                  (setq i (1+ i)))
                                (nreverse acc))))
                        `(progn
                           ;; Constructor
                           (fset ',make-fn
                                 (lambda (&rest args)
                                   (let ((v (make-vector ,(1+ n) nil)))
                                     (aset v 0 ',tag)
                                     (let ((i 1))
                                       (dolist (a args)
                                         (aset v i a)
                                         (setq i (1+ i))))
                                     v)))
                           ;; Predicate
                           (fset ',pred-fn
                                 (lambda (obj)
                                   (and (vectorp obj)
                                        (> (length obj) 0)
                                        (eq (aref obj 0) ',tag))))
                           ;; Copier
                           (fset ',copy-fn
                                 (lambda (obj)
                                   (copy-sequence obj)))
                           ;; Accessors
                           ,@accessor-defs
                           ',name)))
                    (unwind-protect
                        (progn
                          (neovm--test-defrecord person name age email)
                          (let ((alice (funcall 'neovm--test-make-person
                                                "Alice" 30 "alice@example.com"))
                                (bob (funcall 'neovm--test-make-person
                                              "Bob" 25 "bob@example.com")))
                            (let ((alice-copy (funcall 'neovm--test-copy-person alice)))
                              ;; Mutate the copy
                              (funcall 'neovm--test-set-person-age alice-copy 31)
                              (list
                               ;; Predicate
                               (funcall 'neovm--test-person-p alice)
                               (funcall 'neovm--test-person-p "not-a-person")
                               (funcall 'neovm--test-person-p (vector 'wrong 1 2 3))
                               ;; Accessors
                               (funcall 'neovm--test-person-name alice)
                               (funcall 'neovm--test-person-age alice)
                               (funcall 'neovm--test-person-email bob)
                               ;; Copy independence
                               (funcall 'neovm--test-person-age alice)
                               (funcall 'neovm--test-person-age alice-copy)
                               ;; Setter
                               (funcall 'neovm--test-set-person-name bob "Robert")
                               (funcall 'neovm--test-person-name bob)))))
                      (fmakunbound 'neovm--test-defrecord)
                      (fmakunbound 'neovm--test-make-person)
                      (fmakunbound 'neovm--test-person-p)
                      (fmakunbound 'neovm--test-copy-person)
                      (fmakunbound 'neovm--test-person-name)
                      (fmakunbound 'neovm--test-person-age)
                      (fmakunbound 'neovm--test-person-email)
                      (fmakunbound 'neovm--test-set-person-name)
                      (fmakunbound 'neovm--test-set-person-age)
                      (fmakunbound 'neovm--test-set-person-email)))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// DSL for defining state machines
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_meta_state_machine_dsl() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // State machine DSL: define states, transitions, and run inputs
    let form = r#"(let ((make-sm nil)
                        (sm-step nil)
                        (sm-run nil))
                    ;; State machine: (current-state . transitions-alist)
                    ;; transitions-alist: ((state . ((input . next-state) ...)) ...)
                    (setq make-sm
                          (lambda (initial transitions)
                            (cons initial transitions)))
                    ;; Step: take one input, return new SM or nil if invalid
                    (setq sm-step
                          (lambda (sm input)
                            (let* ((state (car sm))
                                   (transitions (cdr sm))
                                   (state-trans (cdr (assq state transitions)))
                                   (next (cdr (assq input state-trans))))
                              (if next
                                  (cons next transitions)
                                nil))))
                    ;; Run: process a list of inputs, return (final-state . history)
                    (setq sm-run
                          (lambda (sm inputs)
                            (let ((history (list (car sm)))
                                  (current sm))
                              (dolist (input inputs)
                                (let ((next (funcall sm-step current input)))
                                  (if next
                                      (progn
                                        (setq current next)
                                        (setq history (cons (car current) history)))
                                    ;; Stay in current state on invalid input
                                    (setq history (cons (car current) history)))))
                              (cons (car current) (nreverse history)))))
                    ;; Define a turnstile state machine
                    ;;   locked --coin--> unlocked --push--> locked
                    ;;   locked --push--> locked (stays)
                    ;;   unlocked --coin--> unlocked (stays)
                    (let ((turnstile
                           (funcall make-sm 'locked
                                    '((locked . ((coin . unlocked)
                                                 (push . locked)))
                                      (unlocked . ((coin . unlocked)
                                                   (push . locked)))))))
                      ;; Define a traffic light: red -> green -> yellow -> red
                      (let ((traffic
                             (funcall make-sm 'red
                                      '((red . ((next . green)))
                                        (green . ((next . yellow)))
                                        (yellow . ((next . red)))))))
                        (list
                         ;; Turnstile: coin then push
                         (funcall sm-run turnstile '(coin push))
                         ;; Turnstile: push without coin (stays locked)
                         (funcall sm-run turnstile '(push push coin push))
                         ;; Turnstile: multiple coins then push
                         (funcall sm-run turnstile '(coin coin coin push))
                         ;; Traffic light: full cycle
                         (funcall sm-run traffic '(next next next))
                         ;; Traffic light: two cycles
                         (funcall sm-run traffic
                                  '(next next next next next next))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((locked locked unlocked locked) (locked locked locked locked unlocked locked) (locked locked unlocked unlocked unlocked locked) (red red green yellow red) (red red green yellow red green yellow red))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Template-based code expansion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_meta_template_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Template engine: replace $var placeholders in s-expression templates
    let form = r#"(let ((template-expand nil)
                        (template-subst nil))
                    ;; Substitute variables in a single form
                    (setq template-subst
                          (lambda (form env)
                            (cond
                             ;; Symbol starting with $ is a template variable
                             ((and (symbolp form)
                                   (string-prefix-p "$" (symbol-name form)))
                              (let ((binding (assq form env)))
                                (if binding (cdr binding) form)))
                             ;; Recurse into lists
                             ((consp form)
                              (cons (funcall template-subst (car form) env)
                                    (funcall template-subst (cdr form) env)))
                             ;; Recurse into vectors
                             ((vectorp form)
                              (apply #'vector
                                     (mapcar (lambda (x)
                                               (funcall template-subst x env))
                                             (append form nil))))
                             ;; Everything else passes through
                             (t form))))
                    ;; Expand a template with multiple environments
                    (setq template-expand
                          (lambda (template envs)
                            (mapcar (lambda (env)
                                      (funcall template-subst template env))
                                    envs)))
                    (list
                     ;; Simple substitution
                     (funcall template-subst
                              '(defun $name ($arg) (+ $arg $offset))
                              '(($name . add-ten) ($arg . x) ($offset . 10)))
                     ;; Expand template for multiple configs
                     (funcall template-expand
                              '(list $name $value)
                              '((($name . "width") ($value . 80))
                                (($name . "height") ($value . 24))
                                (($name . "depth") ($value . 8))))
                     ;; Nested template
                     (funcall template-subst
                              '(let (($var $init))
                                 (while (< $var $limit)
                                   ($body $var)
                                   (setq $var (+ $var $step))))
                              '(($var . i) ($init . 0) ($limit . 10)
                                ($step . 2) ($body . print)))
                     ;; Vector in template
                     (funcall template-subst
                              '(vector $a $b $c)
                              '(($a . 1) ($b . 2) ($c . 3)))
                     ;; Unbound variables pass through
                     (funcall template-subst
                              '(list $known $unknown)
                              '(($known . 42)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((defun add-ten (x) (+ x 10)) ((list \"width\" 80) (list \"height\" 24) (list \"depth\" 8)) (let ((i 0)) (while (< i 10) (print i) (setq i (+ i 2)))) (vector 1 2 3) (list 42 $unknown))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Reflective property system (inspect and modify at runtime)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_meta_reflective_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Object system with runtime-inspectable properties using plists
    let form = r#"(let ((make-object nil)
                        (obj-get nil)
                        (obj-set nil)
                        (obj-has nil)
                        (obj-keys nil)
                        (obj-merge nil)
                        (obj-map-values nil))
                    ;; Object is a cons: (type . plist)
                    (setq make-object
                          (lambda (type &rest props)
                            (cons type props)))
                    (setq obj-get
                          (lambda (obj key)
                            (plist-get (cdr obj) key)))
                    (setq obj-set
                          (lambda (obj key val)
                            (let ((plist (cdr obj)))
                              (setcdr obj (plist-put plist key val))
                              val)))
                    (setq obj-has
                          (lambda (obj key)
                            (plist-member (cdr obj) key)))
                    (setq obj-keys
                          (lambda (obj)
                            (let ((plist (cdr obj))
                                  (keys nil))
                              (while plist
                                (setq keys (cons (car plist) keys))
                                (setq plist (cddr plist)))
                              (nreverse keys))))
                    ;; Merge: second object's properties override first
                    (setq obj-merge
                          (lambda (obj1 obj2)
                            (let ((result (copy-sequence (cdr obj1))))
                              (let ((plist (cdr obj2)))
                                (while plist
                                  (setq result (plist-put result
                                                          (car plist)
                                                          (cadr plist)))
                                  (setq plist (cddr plist))))
                              (cons (car obj1) result))))
                    ;; Map a function over all values
                    (setq obj-map-values
                          (lambda (obj fn)
                            (let ((plist (cdr obj))
                                  (result nil))
                              (while plist
                                (setq result
                                      (append result
                                              (list (car plist)
                                                    (funcall fn (cadr plist)))))
                                (setq plist (cddr plist)))
                              (cons (car obj) result))))
                    ;; Build and manipulate objects
                    (let ((config (funcall make-object 'config
                                          :width 80 :height 24 :color "blue")))
                      ;; Modify
                      (funcall obj-set config :height 30)
                      (funcall obj-set config :font "mono")
                      (let ((defaults (funcall make-object 'config
                                               :width 120 :depth 8 :color "red")))
                        (let ((merged (funcall obj-merge config defaults)))
                          (list
                           ;; Get values
                           (funcall obj-get config :width)
                           (funcall obj-get config :height)
                           (funcall obj-get config :font)
                           ;; Has property
                           (if (funcall obj-has config :color) t nil)
                           (if (funcall obj-has config :missing) t nil)
                           ;; Keys
                           (funcall obj-keys config)
                           ;; Merged: config props win, defaults fill gaps
                           (funcall obj-get merged :width)
                           (funcall obj-get merged :color)
                           (funcall obj-get merged :depth)
                           ;; Map values: double all numbers in a numeric object
                           (let ((nums (funcall make-object 'nums
                                                :a 1 :b 2 :c 3)))
                             (funcall obj-map-values nums
                                      (lambda (v) (* v 2)))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (80 30 \"mono\" t nil (:width :height :color :font) 120 \"red\" 8 (nums :a 2 :b 4 :c 6))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Macro debugging (macroexpand inspection)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_meta_macroexpand_inspection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test macroexpand and macroexpand-all on various macro forms
    let form = r#"(progn
                    (defmacro neovm--test-my-when (cond &rest body)
                      `(if ,cond (progn ,@body) nil))
                    (defmacro neovm--test-my-unless (cond &rest body)
                      `(neovm--test-my-when (not ,cond) ,@body))
                    (defmacro neovm--test-my-and2 (a b)
                      `(if ,a ,b nil))
                    (unwind-protect
                        (list
                         ;; macroexpand-1: one step only
                         (macroexpand-1 '(neovm--test-my-when t (print 1)))
                         ;; macroexpand: full expansion of outermost
                         (macroexpand '(neovm--test-my-unless nil (+ 1 2)))
                         ;; macroexpand on non-macro form returns it unchanged
                         (macroexpand '(+ 1 2))
                         ;; Nested macro: unless expands to when, then to if
                         (macroexpand '(neovm--test-my-unless (= x 0) (/ 1 x)))
                         ;; macroexpand-1 on nested only does one level
                         (macroexpand-1 '(neovm--test-my-unless (= x 0) (/ 1 x)))
                         ;; Verify expansion is evaluable
                         (eval (macroexpand '(neovm--test-my-when t (+ 10 20))))
                         ;; Compound macro expansion
                         (macroexpand
                          '(neovm--test-my-and2
                            (neovm--test-my-when t 'yes)
                            42)))
                      (fmakunbound 'neovm--test-my-when)
                      (fmakunbound 'neovm--test-my-unless)
                      (fmakunbound 'neovm--test-my-and2)))"#;
    let expect = expect_test::expect![[
        r#""OK ((if t (progn (print 1)) nil) (if (not nil) (progn (+ 1 2)) nil) (+ 1 2) (if (not (= x 0)) (progn (/ 1 x)) nil) (neovm--test-my-when (not (= x 0)) (/ 1 x)) 30 (if (neovm--test-my-when t 'yes) 42 nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Code walking and transformation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_meta_code_walker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Walk an s-expression tree and apply transformations
    let form = r#"(let ((walk nil)
                        (transform nil))
                    ;; Walk: apply fn to every node in an s-expression
                    (setq walk
                          (lambda (fn form)
                            (let ((result (funcall fn form)))
                              (if (consp result)
                                  (mapcar (lambda (sub) (funcall walk fn sub))
                                          result)
                                result))))
                    ;; Transform: replace symbols according to a rename map
                    (setq transform
                          (lambda (rename-map form)
                            (funcall walk
                                     (lambda (node)
                                       (if (symbolp node)
                                           (let ((renamed (assq node rename-map)))
                                             (if renamed (cdr renamed) node))
                                         node))
                                     form)))
                    (list
                     ;; Rename variables in a form
                     (funcall transform
                              '((x . tmp-x) (y . tmp-y))
                              '(let ((x 1) (y 2)) (+ x y)))
                     ;; Rename function names
                     (funcall transform
                              '((add . my-add) (mul . my-mul))
                              '(progn (add 1 2) (mul 3 (add 4 5))))
                     ;; Walk to collect all symbols
                     (let ((symbols nil))
                       (funcall walk
                                (lambda (node)
                                  (when (symbolp node)
                                    (setq symbols (cons node symbols)))
                                  node)
                                '(if (> x 0) (+ x 1) (- x 1)))
                       (nreverse symbols))
                     ;; Walk to count nodes
                     (let ((count 0))
                       (funcall walk
                                (lambda (node)
                                  (setq count (1+ count))
                                  node)
                                '(defun foo (a b) (+ (* a a) (* b b))))
                       count)
                     ;; Constant folding: replace (+ lit lit) with result
                     (funcall walk
                              (lambda (node)
                                (if (and (consp node)
                                         (eq (car node) '+)
                                         (numberp (cadr node))
                                         (numberp (caddr node))
                                         (null (cdddr node)))
                                    (+ (cadr node) (caddr node))
                                  node))
                              '(list (+ 1 2) (+ 3 4) (+ x 5)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((let ((tmp-x 1) (tmp-y 2)) (+ tmp-x tmp-y)) (progn (my-add 1 2) (my-mul 3 (my-add 4 5))) (if > x + x - x) 16 (list 3 7 (+ x 5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Method dispatch table (OOP-like with macros)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_meta_method_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simple method dispatch using alist-based vtable
    let form = r#"(let ((make-class nil)
                        (instantiate nil)
                        (send nil))
                    ;; Class: (name . methods-alist)
                    (setq make-class
                          (lambda (name &rest method-specs)
                            (let ((methods nil))
                              (while method-specs
                                (let ((mname (car method-specs))
                                      (mfn (cadr method-specs)))
                                  (setq methods (cons (cons mname mfn) methods))
                                  (setq method-specs (cddr method-specs))))
                              (cons name methods))))
                    ;; Instance: (class . state-plist)
                    (setq instantiate
                          (lambda (class &rest init-args)
                            (cons class init-args)))
                    ;; Send message to instance
                    (setq send
                          (lambda (instance method &rest args)
                            (let* ((class (car instance))
                                   (methods (cdr class))
                                   (mfn (cdr (assq method methods))))
                              (if mfn
                                  (apply mfn instance args)
                                (error "No method %s" method)))))
                    ;; Define a counter class
                    (let ((counter-class
                           (funcall make-class 'counter
                                    'get (lambda (self)
                                           (or (plist-get (cdr self) :count) 0))
                                    'inc (lambda (self &optional n)
                                           (let ((cur (or (plist-get (cdr self) :count) 0)))
                                             (setcdr self
                                                     (plist-put (cdr self) :count
                                                                (+ cur (or n 1))))
                                             (+ cur (or n 1))))
                                    'reset (lambda (self)
                                             (setcdr self
                                                     (plist-put (cdr self) :count 0))
                                             0))))
                      (let ((c1 (funcall instantiate counter-class))
                            (c2 (funcall instantiate counter-class)))
                        ;; Increment c1 several times
                        (funcall send c1 'inc)
                        (funcall send c1 'inc)
                        (funcall send c1 'inc 5)
                        ;; Increment c2 differently
                        (funcall send c2 'inc 10)
                        (let ((result
                               (list
                                ;; c1 count: 1+1+5 = 7
                                (funcall send c1 'get)
                                ;; c2 count: 10
                                (funcall send c2 'get)
                                ;; Independence
                                (not (eq (funcall send c1 'get)
                                         (funcall send c2 'get))))))
                          ;; Reset c1
                          (funcall send c1 'reset)
                          (append result
                                  (list
                                   (funcall send c1 'get)
                                   (funcall send c2 'get)))))))"#;
    let expect = expect_test::expect![[r#""OK (7 10 t 0 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

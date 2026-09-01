//! Oracle parity tests for comprehensive `when` and `unless` patterns:
//! truthy/falsy conditions, multiple body forms, return values, nesting,
//! complex conditions (and/or/not), interaction with let/iteration/side effects.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// when with various truthy values (not just t)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_when_unless_comp_when_truthy_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // In Elisp, everything except nil is truthy.
    let form = r#"(list
                    (when t 'from-t)
                    (when 42 'from-number)
                    (when "hello" 'from-string)
                    (when '(a b) 'from-list)
                    (when 'symbol 'from-symbol)
                    (when 0 'from-zero)
                    (when "" 'from-empty-string)
                    (when [1 2] 'from-vector))"#;
    let expect = expect_test::expect![[
        r#""OK (from-t from-number from-string from-list from-symbol from-zero from-empty-string from-vector)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// when and unless with nil and explicit falsy condition
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_when_unless_comp_falsy_conditions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                    (when nil 'should-not-appear)
                    (unless nil 'unless-nil-runs)
                    (unless t 'should-not-appear)
                    (when (car nil) 'should-not-appear)
                    (unless (car nil) 'unless-car-nil-runs)
                    (when (and nil t) 'should-not-appear)
                    (unless (and nil t) 'unless-and-nil-runs))"#;
    let expect = expect_test::expect![[
        r#""OK (nil unless-nil-runs nil nil unless-car-nil-runs nil unless-and-nil-runs)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// when/unless with multiple body forms, return value is last form
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_when_unless_comp_multiple_body_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((side-a nil)
                        (side-b nil))
                    (let ((when-result
                           (when t
                             (setq side-a 'first-effect)
                             (setq side-b 'second-effect)
                             (+ 10 20 30))))
                      (let ((unless-result
                             (unless nil
                               (setq side-a (cons 'third-effect side-a))
                               (setq side-b (cons 'fourth-effect side-b))
                               (* 3 7))))
                        (list :when-result when-result
                              :unless-result unless-result
                              :side-a side-a
                              :side-b side-b))))"#;
    let expect = expect_test::expect![[
        r#""OK (:when-result 60 :unless-result 21 :side-a (third-effect . first-effect) :side-b (fourth-effect . second-effect))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// when/unless return values: when returns nil on false, unless returns nil on true
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_when_unless_comp_return_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                    ;; when: condition true -> last body form value
                    (when t 1 2 3)
                    ;; when: condition false -> nil
                    (when nil 1 2 3)
                    ;; unless: condition false -> last body form value
                    (unless nil 'a 'b 'c)
                    ;; unless: condition true -> nil
                    (unless t 'a 'b 'c)
                    ;; when with no body forms, condition true -> nil
                    (when t)
                    ;; unless with no body forms, condition false -> nil
                    (unless nil)
                    ;; Nested: return value propagation
                    (when t (when t (when t 'deep)))
                    (when t (unless nil (when t 'deep-mixed))))"#;
    let expect = expect_test::expect![[r#""OK (3 nil c nil nil nil deep deep-mixed)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested when/unless forming complex control flow
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_when_unless_comp_nested_control_flow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
                    (dolist (n '(-5 -1 0 1 5 10 15 20 25))
                      (let ((label
                             (when (numberp n)
                               (unless (< n 0)
                                 (when (> n 0)
                                   (unless (> n 20)
                                     (when (= (% n 5) 0)
                                       (format "divisible-by-5:%d" n))))))))
                        (setq results (cons (cons n label) results))))
                    (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((-5) (-1) (0) (1) (5 . \"divisible-by-5:5\") (10 . \"divisible-by-5:10\") (15 . \"divisible-by-5:15\") (20 . \"divisible-by-5:20\") (25))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// when/unless with complex conditions: and, or, not combinations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_when_unless_comp_complex_conditions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
                    (dolist (x '(0 1 2 3 4 5 6 7 8 9 10 11 12 15 20 30))
                      (let ((tag nil))
                        ;; and: multiple conditions all true
                        (when (and (> x 3) (< x 10) (= (% x 2) 0))
                          (setq tag (cons 'even-mid tag)))
                        ;; or: at least one condition true
                        (when (or (= x 0) (= x 5) (= x 10) (= x 15))
                          (setq tag (cons 'milestone tag)))
                        ;; not: negate a condition
                        (unless (not (> x 7))
                          (setq tag (cons 'above-7 tag)))
                        ;; Combined: (and (or ...) (not ...))
                        (when (and (or (> x 10) (< x 3))
                                   (not (= x 0)))
                          (setq tag (cons 'extreme-nonzero tag)))
                        (setq results (cons (cons x (nreverse tag)) results))))
                    (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 milestone) (1 extreme-nonzero) (2 extreme-nonzero) (3) (4 even-mid) (5 milestone) (6 even-mid) (7) (8 even-mid above-7) (9 above-7) (10 milestone above-7) (11 above-7 extreme-nonzero) (12 above-7 extreme-nonzero) (15 milestone above-7 extreme-nonzero) (20 above-7 extreme-nonzero) (30 above-7 extreme-nonzero))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// when inside let: ensure proper scoping of let bindings in condition and body
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_when_unless_comp_when_inside_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((outer 100))
                    (let ((inner 200))
                      (list
                       ;; when condition references both scopes
                       (when (> (+ outer inner) 250)
                         (let ((local (* outer inner)))
                           (list :product local :sum (+ outer inner))))
                       ;; let* with when in init form
                       (let* ((a 10)
                              (b (when (> a 5) (* a 3)))
                              (c (unless (> a 20) (+ a 7))))
                         (list :a a :b b :c c))
                       ;; when with let binding that shadows outer
                       (when t
                         (let ((outer 999))
                           (list :shadowed outer)))
                       ;; after when, outer is restored
                       (list :outer-restored outer))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:product 20000 :sum 300) (:a 10 :b 30 :c 17) (:shadowed 999) (:outer-restored 100))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// unless with side effects: only executed when condition is false
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_when_unless_comp_unless_side_effects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((log nil)
                        (counter 0))
                    ;; unless runs body only when condition is nil/false
                    (unless nil
                      (setq counter (1+ counter))
                      (setq log (cons 'nil-branch log)))
                    (unless t
                      (setq counter (1+ counter))
                      (setq log (cons 't-branch log)))
                    (unless (> 3 5)
                      (setq counter (1+ counter))
                      (setq log (cons 'false-comparison log)))
                    (unless (< 3 5)
                      (setq counter (1+ counter))
                      (setq log (cons 'true-comparison log)))
                    ;; Only 2 of 4 unless blocks should have executed
                    (list :counter counter :log (nreverse log)))"#;
    let expect = expect_test::expect![[r#""OK (:counter 2 :log (nil-branch false-comparison))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// when/unless in iteration bodies: filtering and classification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_when_unless_comp_in_iteration_bodies() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use when/unless inside dolist and dotimes for filtering and categorization.
    let form = r#"(let ((evens nil)
                        (odds nil)
                        (fizzbuzz nil))
                    ;; dotimes with when/unless for even/odd partition
                    (dotimes (i 12)
                      (when (= (% i 2) 0)
                        (setq evens (cons i evens)))
                      (unless (= (% i 2) 0)
                        (setq odds (cons i odds))))
                    ;; dolist with when for fizzbuzz classification
                    (dolist (n '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
                      (let ((entry
                             (cond
                              ((and (= (% n 3) 0) (= (% n 5) 0)) "fizzbuzz")
                              ((= (% n 3) 0) "fizz")
                              ((= (% n 5) 0) "buzz")
                              (t n))))
                        (when (stringp entry)
                          (setq fizzbuzz (cons (cons n entry) fizzbuzz)))))
                    (list :evens (nreverse evens)
                          :odds (nreverse odds)
                          :fizzbuzz (nreverse fizzbuzz)))"#;
    let expect = expect_test::expect![[
        r#""OK (:evens (0 2 4 6 8 10) :odds (1 3 5 7 9 11) :fizzbuzz ((3 . \"fizz\") (5 . \"buzz\") (6 . \"fizz\") (9 . \"fizz\") (10 . \"buzz\") (12 . \"fizz\") (15 . \"fizzbuzz\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// when/unless with string and list predicates as conditions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_when_unless_comp_predicate_conditions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((items '(42 "hello" nil (1 2 3) t [a b] 0 "" foo))
                        (results nil))
                    (dolist (item items)
                      (let ((tags nil))
                        (when (numberp item) (setq tags (cons :number tags)))
                        (when (stringp item) (setq tags (cons :string tags)))
                        (when (listp item) (setq tags (cons :list tags)))
                        (when (symbolp item) (setq tags (cons :symbol tags)))
                        (when (vectorp item) (setq tags (cons :vector tags)))
                        (when (null item) (setq tags (cons :null tags)))
                        (unless (null item) (setq tags (cons :non-null tags)))
                        (unless (atom item) (setq tags (cons :non-atom tags)))
                        (setq results (cons (cons item (nreverse tags)) results))))
                    (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((42 :number :non-null) (\"hello\" :string :non-null) (nil :list :symbol :null) ((1 2 3) :list :non-null :non-atom) (t :symbol :non-null) ([a b] :vector :non-null) (0 :number :non-null) (\"\" :string :non-null) (foo :symbol :non-null))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// when/unless used for guard clauses pattern (early return logic)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_when_unless_comp_guard_clause_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate guard-clause pattern: validate inputs, bail early on failure.
    let form = r#"(progn
  (fset 'neovm--test-validate-and-process
    (lambda (input)
      (catch 'validation-error
        ;; Guard: must be a list
        (unless (listp input)
          (throw 'validation-error (list :error "not a list" :input input)))
        ;; Guard: must be non-empty
        (when (null input)
          (throw 'validation-error (list :error "empty list")))
        ;; Guard: first element must be a number
        (unless (numberp (car input))
          (throw 'validation-error (list :error "first element not a number"
                                         :got (type-of (car input)))))
        ;; Guard: must have at least 2 elements
        (when (null (cdr input))
          (throw 'validation-error (list :error "need at least 2 elements")))
        ;; All guards passed, process
        (let ((sum 0))
          (dolist (x input)
            (when (numberp x) (setq sum (+ sum x))))
          (list :ok t :sum sum :count (length input))))))
  (unwind-protect
      (list
        (funcall 'neovm--test-validate-and-process '(1 2 3 4 5))
        (funcall 'neovm--test-validate-and-process "not-a-list")
        (funcall 'neovm--test-validate-and-process nil)
        (funcall 'neovm--test-validate-and-process '(hello world))
        (funcall 'neovm--test-validate-and-process '(42)))
    (fmakunbound 'neovm--test-validate-and-process)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:ok t :sum 15 :count 5) (:error \"not a list\" :input \"not-a-list\") (:error \"empty list\") (:error \"first element not a number\" :got symbol) (:error \"need at least 2 elements\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// when/unless chained: building a configuration validator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_when_unless_comp_config_validator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((validate-config
                         (lambda (config)
                           (let ((errors nil)
                                 (warnings nil))
                             ;; Check required fields
                             (unless (plist-get config :name)
                               (setq errors (cons "missing :name" errors)))
                             (unless (plist-get config :port)
                               (setq errors (cons "missing :port" errors)))
                             ;; Type checks
                             (when (plist-get config :port)
                               (unless (numberp (plist-get config :port))
                                 (setq errors (cons ":port must be a number" errors)))
                               (when (numberp (plist-get config :port))
                                 (when (< (plist-get config :port) 1024)
                                   (setq warnings (cons "port < 1024 requires root" warnings)))
                                 (when (> (plist-get config :port) 65535)
                                   (setq errors (cons "port out of range" errors)))))
                             ;; Optional field warnings
                             (unless (plist-get config :timeout)
                               (setq warnings (cons "no :timeout, using default 30s" warnings)))
                             (when (and (plist-get config :timeout)
                                        (numberp (plist-get config :timeout))
                                        (> (plist-get config :timeout) 300))
                               (setq warnings (cons "timeout > 300s is very long" warnings)))
                             (list :valid (null errors)
                                   :errors (nreverse errors)
                                   :warnings (nreverse warnings))))))
                    (list
                     (funcall validate-config '(:name "app" :port 8080 :timeout 30))
                     (funcall validate-config '(:name "app" :port 80))
                     (funcall validate-config '(:port "not-a-number"))
                     (funcall validate-config '(:name "app" :port 99999))
                     (funcall validate-config '(:name "app" :port 3000 :timeout 600))
                     (funcall validate-config nil)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:valid t :errors nil :warnings nil) (:valid t :errors nil :warnings (\"port < 1024 requires root\" \"no :timeout, using default 30s\")) (:valid nil :errors (\"missing :name\" \":port must be a number\") :warnings (\"no :timeout, using default 30s\")) (:valid nil :errors (\"port out of range\") :warnings (\"no :timeout, using default 30s\")) (:valid t :errors nil :warnings (\"timeout > 300s is very long\")) (:valid nil :errors (\"missing :name\" \"missing :port\") :warnings (\"no :timeout, using default 30s\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

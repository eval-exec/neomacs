//! Oracle parity tests for monoid-based computations in Elisp:
//! monoid definition (binary op + identity), string monoid (concatenation),
//! list monoid (append), sum/product monoids, monoid fold (mconcat),
//! endomorphism monoid (function composition), and free monoid.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Monoid infrastructure: generic monoid operations with verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_monoid_definition_and_laws() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Define a monoid as (op . identity), then verify the three monoid laws:
    //   1. Left identity:  op(e, x) = x
    //   2. Right identity: op(x, e) = x
    //   3. Associativity:  op(op(a, b), c) = op(a, op(b, c))
    let form = r#"(progn
  ;; A monoid is represented as (op . identity)
  (fset 'neovm--monoid-op       (lambda (m) (car m)))
  (fset 'neovm--monoid-identity (lambda (m) (cdr m)))
  (fset 'neovm--monoid-combine
    (lambda (m a b) (funcall (funcall 'neovm--monoid-op m) a b)))

  ;; Verify monoid laws for a given monoid and test values
  (fset 'neovm--monoid-check-laws
    (lambda (m vals equal-fn)
      "Check monoid laws. VALS is list of test values, EQUAL-FN compares."
      (let ((op (funcall 'neovm--monoid-op m))
            (e  (funcall 'neovm--monoid-identity m))
            (all-pass t))
        ;; Left identity: op(e, x) = x
        (dolist (x vals)
          (unless (funcall equal-fn (funcall op e x) x)
            (setq all-pass nil)))
        ;; Right identity: op(x, e) = x
        (dolist (x vals)
          (unless (funcall equal-fn (funcall op x e) x)
            (setq all-pass nil)))
        ;; Associativity: op(op(a,b),c) = op(a,op(b,c))
        ;; Test all triples from vals
        (dolist (a vals)
          (dolist (b vals)
            (dolist (c vals)
              (unless (funcall equal-fn
                              (funcall op (funcall op a b) c)
                              (funcall op a (funcall op b c)))
                (setq all-pass nil)))))
        all-pass)))

  (unwind-protect
      (let* (;; Sum monoid: (+ . 0)
             (sum-m   (cons (lambda (a b) (+ a b)) 0))
             ;; Product monoid: (* . 1)
             (prod-m  (cons (lambda (a b) (* a b)) 1))
             ;; String monoid: (concat . "")
             (str-m   (cons (lambda (a b) (concat a b)) ""))
             ;; List monoid: (append . nil)
             (list-m  (cons (lambda (a b) (append a b)) nil)))
        (list
          ;; Check laws for each monoid
          (funcall 'neovm--monoid-check-laws sum-m  '(0 1 5 -3 100) 'equal)
          (funcall 'neovm--monoid-check-laws prod-m '(1 2 3 -1 0) 'equal)
          (funcall 'neovm--monoid-check-laws str-m  '("" "a" "bc" "def") 'string=)
          (funcall 'neovm--monoid-check-laws list-m '(nil (1) (2 3) (4 5 6)) 'equal)))
    (fmakunbound 'neovm--monoid-op)
    (fmakunbound 'neovm--monoid-identity)
    (fmakunbound 'neovm--monoid-combine)
    (fmakunbound 'neovm--monoid-check-laws)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// String monoid: concatenation with identity ""
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_monoid_string_concatenation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // String monoid used for building formatted output through folding.
    let form = r#"(progn
  (fset 'neovm--str-mconcat
    (lambda (strings)
      "Fold a list of strings using the string monoid."
      (let ((result ""))
        (dolist (s strings)
          (setq result (concat result s)))
        result)))

  (fset 'neovm--str-intersperse
    (lambda (sep strings)
      "Join strings with separator, using monoid fold."
      (if (null strings)
          ""
        (let ((result (car strings)))
          (dolist (s (cdr strings))
            (setq result (concat result sep s)))
          result))))

  (unwind-protect
      (list
        ;; Basic fold
        (funcall 'neovm--str-mconcat '("Hello" " " "World" "!"))
        ;; Empty list → identity
        (funcall 'neovm--str-mconcat nil)
        ;; Singleton
        (funcall 'neovm--str-mconcat '("alone"))
        ;; Intersperse
        (funcall 'neovm--str-intersperse ", " '("alpha" "beta" "gamma" "delta"))
        (funcall 'neovm--str-intersperse "-" '("2026" "03" "02"))
        (funcall 'neovm--str-intersperse "/" nil)
        (funcall 'neovm--str-intersperse "/" '("only"))
        ;; Nested fold: build a CSV row
        (funcall 'neovm--str-intersperse ","
                 (mapcar (lambda (n) (number-to-string n)) '(1 2 3 4 5))))
    (fmakunbound 'neovm--str-mconcat)
    (fmakunbound 'neovm--str-intersperse)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello World!\" \"\" \"alone\" \"alpha, beta, gamma, delta\" \"2026-03-02\" \"\" \"only\" \"1,2,3,4,5\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// List monoid: append with identity nil
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_monoid_list_append() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // List monoid used for flattening nested structures and collecting results.
    let form = r#"(progn
  ;; mconcat for list monoid: flatten a list of lists
  (fset 'neovm--list-mconcat
    (lambda (lists)
      (let ((result nil))
        (dolist (lst lists)
          (setq result (append result lst)))
        result)))

  ;; Use list monoid to implement flatmap
  (fset 'neovm--list-flatmap
    (lambda (f xs)
      "Apply F to each element of XS, then mconcat the results."
      (funcall 'neovm--list-mconcat (mapcar f xs))))

  (unwind-protect
      (list
        ;; Basic mconcat
        (funcall 'neovm--list-mconcat '((1 2) (3 4 5) (6)))
        ;; With empty lists
        (funcall 'neovm--list-mconcat '((1) nil (2 3) nil nil (4)))
        ;; All empty
        (funcall 'neovm--list-mconcat '(nil nil nil))
        ;; Empty input
        (funcall 'neovm--list-mconcat nil)
        ;; Flatmap: expand each number to a range
        (funcall 'neovm--list-flatmap
                 (lambda (n) (let ((result nil) (i 1))
                               (while (<= i n)
                                 (setq result (cons i result))
                                 (setq i (1+ i)))
                               (nreverse result)))
                 '(3 1 4 2))
        ;; Flatmap: duplicate each element
        (funcall 'neovm--list-flatmap
                 (lambda (x) (list x x))
                 '(a b c d))
        ;; Flatmap: filter+transform (only even, then square)
        (funcall 'neovm--list-flatmap
                 (lambda (x) (if (= (% x 2) 0) (list (* x x)) nil))
                 '(1 2 3 4 5 6 7 8)))
    (fmakunbound 'neovm--list-mconcat)
    (fmakunbound 'neovm--list-flatmap)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6) (1 2 3 4) nil nil (1 2 3 1 1 2 3 4 1 2) (a a b b c c d d) (4 16 36 64))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sum and product monoids with mconcat
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_monoid_sum_product() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Sum monoid (+ , 0) and Product monoid (* , 1) with generic mconcat.
    let form = r#"(progn
  ;; Generic mconcat: given (op . identity) and a list, fold
  (fset 'neovm--generic-mconcat
    (lambda (monoid values)
      (let ((op (car monoid))
            (result (cdr monoid)))
        (dolist (v values)
          (setq result (funcall op result v)))
        result)))

  ;; Also implement mconcat with power (repeated combine)
  (fset 'neovm--monoid-power
    (lambda (monoid x n)
      "Compute x combined with itself N times using the monoid."
      (let ((op (car monoid))
            (result (cdr monoid)))
        (let ((i 0))
          (while (< i n)
            (setq result (funcall op result x))
            (setq i (1+ i))))
        result)))

  (unwind-protect
      (let ((sum-m  (cons '+ 0))
            (prod-m (cons '* 1))
            (max-m  (cons (lambda (a b) (max a b)) most-negative-fixnum))
            (min-m  (cons (lambda (a b) (min a b)) most-positive-fixnum)))
        (list
          ;; Sum of 1..10
          (funcall 'neovm--generic-mconcat sum-m '(1 2 3 4 5 6 7 8 9 10))
          ;; Product of 1..6 (6!)
          (funcall 'neovm--generic-mconcat prod-m '(1 2 3 4 5 6))
          ;; Sum of empty list → identity 0
          (funcall 'neovm--generic-mconcat sum-m nil)
          ;; Product of empty → 1
          (funcall 'neovm--generic-mconcat prod-m nil)
          ;; Max monoid
          (funcall 'neovm--generic-mconcat max-m '(3 7 2 9 1 8 4))
          ;; Min monoid
          (funcall 'neovm--generic-mconcat min-m '(3 7 2 9 1 8 4))
          ;; Power: 5 + 5 + 5 + 5 = 20
          (funcall 'neovm--monoid-power sum-m 5 4)
          ;; Power: 2 * 2 * 2 * 2 * 2 = 32
          (funcall 'neovm--monoid-power prod-m 2 5)
          ;; Power: "" repeated 3 times = ""
          (funcall 'neovm--monoid-power (cons (lambda (a b) (concat a b)) "") "ha" 3)))
    (fmakunbound 'neovm--generic-mconcat)
    (fmakunbound 'neovm--monoid-power)))"#;
    let expect = expect_test::expect![[r#""OK (55 720 0 1 9 1 20 32 \"hahaha\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Endomorphism monoid: function composition with identity function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_monoid_endomorphism_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Endomorphism monoid: functions from A->A under composition.
    // op = compose, identity = id function.
    // mconcat builds a pipeline of transformations.
    let form = r#"(progn
  ;; Compose two functions
  (fset 'neovm--endo-compose
    (lambda (f g) (lambda (x) (funcall f (funcall g x)))))

  ;; Fold a list of endomorphisms into one using composition monoid
  (fset 'neovm--endo-mconcat
    (lambda (fns)
      "Compose all functions in FNS left-to-right (last applied first)."
      (let ((result (lambda (x) x)))  ;; identity
        (dolist (f fns)
          (setq result (funcall 'neovm--endo-compose f result)))
        result)))

  ;; Reverse fold: apply first function first (pipeline order)
  (fset 'neovm--endo-pipeline
    (lambda (fns)
      "Compose FNS so that the first in the list is applied first."
      (funcall 'neovm--endo-mconcat (reverse fns))))

  (unwind-protect
      (let* ((add1    (lambda (x) (+ x 1)))
             (double  (lambda (x) (* x 2)))
             (square  (lambda (x) (* x x)))
             (negate  (lambda (x) (- x)))
             ;; Pipeline: first add1, then double, then square
             ;; square(double(add1(x)))
             (pipe1 (funcall 'neovm--endo-pipeline (list add1 double square)))
             ;; Reverse pipeline: first square, then double, then add1
             (pipe2 (funcall 'neovm--endo-pipeline (list square double add1))))
        (list
          ;; pipe1: square(double(add1(3))) = square(double(4)) = square(8) = 64
          (funcall pipe1 3)
          ;; pipe1: square(double(add1(0))) = square(2) = 4
          (funcall pipe1 0)
          ;; pipe2: add1(double(square(3))) = add1(double(9)) = add1(18) = 19
          (funcall pipe2 3)
          ;; Empty pipeline = identity
          (funcall (funcall 'neovm--endo-pipeline nil) 42)
          ;; Single function pipeline
          (funcall (funcall 'neovm--endo-pipeline (list negate)) 7)
          ;; Compose negate with itself = identity
          (funcall (funcall 'neovm--endo-mconcat (list negate negate)) 5)
          ;; Power via repeated composition: add1 five times
          (let ((add5 (funcall 'neovm--endo-mconcat
                               (list add1 add1 add1 add1 add1))))
            (funcall add5 10))
          ;; Double three times = multiply by 8
          (let ((times8 (funcall 'neovm--endo-mconcat
                                 (list double double double))))
            (funcall times8 3))))
    (fmakunbound 'neovm--endo-compose)
    (fmakunbound 'neovm--endo-mconcat)
    (fmakunbound 'neovm--endo-pipeline)))"#;
    let expect = expect_test::expect![[r#""OK (19 1 64 42 -7 5 15 24)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Free monoid: sequences of symbols under concatenation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_monoid_free_monoid() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The free monoid over an alphabet is the set of all finite sequences
    // of alphabet symbols, with concatenation as the operation and the empty
    // sequence as identity.  We implement it as lists.
    let form = r#"(progn
  ;; Free monoid operations
  (fset 'neovm--free-empty    (lambda () nil))
  (fset 'neovm--free-unit     (lambda (x) (list x)))
  (fset 'neovm--free-combine  (lambda (a b) (append a b)))

  ;; Homomorphism from free monoid to another monoid
  ;; h(unit(x)) = f(x), h(combine(a,b)) = op(h(a), h(b))
  (fset 'neovm--free-homomorphism
    (lambda (f op identity word)
      "Map each symbol through F, combine with OP starting from IDENTITY."
      (let ((result identity))
        (dolist (sym word)
          (setq result (funcall op result (funcall f sym))))
        result)))

  (unwind-protect
      (let* ((w1 (funcall 'neovm--free-unit 'a))
             (w2 (funcall 'neovm--free-combine
                          (funcall 'neovm--free-unit 'a)
                          (funcall 'neovm--free-unit 'b)))
             (w3 (funcall 'neovm--free-combine w2 (funcall 'neovm--free-unit 'c)))
             (empty (funcall 'neovm--free-empty)))
        (list
          ;; Basic operations
          w1 w2 w3 empty
          ;; Identity laws
          (equal (funcall 'neovm--free-combine empty w3) w3)
          (equal (funcall 'neovm--free-combine w3 empty) w3)
          ;; Associativity
          (equal (funcall 'neovm--free-combine
                          (funcall 'neovm--free-combine w1 w2) w3)
                 (funcall 'neovm--free-combine
                          w1 (funcall 'neovm--free-combine w2 w3)))
          ;; Homomorphism to string monoid: map each symbol to its name
          (funcall 'neovm--free-homomorphism
                   (lambda (s) (symbol-name s))
                   (lambda (a b) (concat a b))
                   ""
                   '(h e l l o))
          ;; Homomorphism to sum monoid: map each symbol to its length
          (funcall 'neovm--free-homomorphism
                   (lambda (s) (length (symbol-name s)))
                   '+
                   0
                   '(hello world foo))
          ;; Homomorphism to list monoid: map each to a singleton pair
          (funcall 'neovm--free-homomorphism
                   (lambda (s) (list (cons s (length (symbol-name s)))))
                   'append
                   nil
                   '(cat dog bird))))
    (fmakunbound 'neovm--free-empty)
    (fmakunbound 'neovm--free-unit)
    (fmakunbound 'neovm--free-combine)
    (fmakunbound 'neovm--free-homomorphism)))"#;
    let expect = expect_test::expect![[
        r#""OK ((a) (a b) (a b c) nil t t t \"hello\" 13 ((cat . 3) (dog . 3) (bird . 4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Monoid-based aggregation: building reports from data
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_monoid_aggregation_report() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use multiple monoids simultaneously to aggregate statistics from a
    // list of records.  Each record is (name score grade).
    let form = r#"(progn
  ;; Multi-monoid aggregator: run several monoids in parallel
  (fset 'neovm--multi-mconcat
    (lambda (monoids values extract-fns)
      "MONOIDS: list of (op . identity).
       VALUES: list of records.
       EXTRACT-FNS: list of functions, one per monoid, to extract value from record.
       Returns list of aggregated results."
      (let ((accums (mapcar 'cdr monoids))
            (ops    (mapcar 'car monoids)))
        (dolist (rec values)
          (let ((i 0))
            (while (< i (length monoids))
              (let ((extracted (funcall (nth i extract-fns) rec)))
                (setcar (nthcdr i accums)
                        (funcall (nth i ops) (nth i accums) extracted)))
              (setq i (1+ i)))))
        accums)))

  (unwind-protect
      (let ((records '(("Alice"   95 A)
                        ("Bob"     72 B)
                        ("Charlie" 88 A)
                        ("Diana"   91 A)
                        ("Eve"     65 C)))
            ;; Monoids: count, sum-of-scores, max-score, name-list
            (monoids (list (cons '+ 0)                             ;; count
                           (cons '+ 0)                             ;; sum of scores
                           (cons (lambda (a b) (max a b)) 0)      ;; max score
                           (cons 'append nil)))                    ;; list of names
            (extractors (list (lambda (_r) 1)                      ;; count: always 1
                              (lambda (r) (nth 1 r))               ;; score
                              (lambda (r) (nth 1 r))               ;; score for max
                              (lambda (r) (list (nth 0 r))))))     ;; name as singleton
        (let ((result (funcall 'neovm--multi-mconcat
                               monoids records extractors)))
          (list
            result
            ;; Verify: count=5, sum=411, max=95, names=5 items
            (nth 0 result)
            (nth 1 result)
            (nth 2 result)
            (length (nth 3 result))
            ;; Average (integer division)
            (/ (nth 1 result) (nth 0 result)))))
    (fmakunbound 'neovm--multi-mconcat)))"#;
    let expect = expect_test::expect![[
        r#""OK ((5 411 95 (\"Alice\" \"Bob\" \"Charlie\" \"Diana\" \"Eve\")) 5 411 95 5 82)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

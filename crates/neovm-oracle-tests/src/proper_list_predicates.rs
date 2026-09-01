//! Oracle parity tests for advanced proper-list-p, listp, nlistp, consp, atom:
//! dotted pairs, circular list detection, improper lists, deeply nested
//! structures, combined with safe-length, and type discrimination trees.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// proper-list-p with dotted pairs, improper lists, circular lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proper_list_predicates_dotted_and_circular() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test proper-list-p against various list shapes: proper, dotted, circular,
    // nil, atoms, and deeply nested dotted tails.
    let form = r#"(let* ((proper '(1 2 3 4 5))
                         (dotted1 '(a . b))
                         (dotted2 '(1 2 . 3))
                         (dotted3 (cons 'x (cons 'y (cons 'z 42))))
                         ;; Circular list of length 3
                         (circ (list 'a 'b 'c)))
                    (setcdr (last circ) circ)
                    (list
                      ;; Proper lists
                      (proper-list-p nil)
                      (proper-list-p proper)
                      (proper-list-p '(single))
                      (proper-list-p (make-list 50 t))
                      ;; Dotted / improper lists
                      (proper-list-p dotted1)
                      (proper-list-p dotted2)
                      (proper-list-p dotted3)
                      (proper-list-p (cons nil 42))
                      ;; Circular list
                      (proper-list-p circ)
                      ;; Atoms
                      (proper-list-p 42)
                      (proper-list-p "hello")
                      (proper-list-p 'symbol)
                      (proper-list-p t)
                      (proper-list-p [1 2 3])))"#;
    let expect =
        expect_test::expect![[r#""OK (0 5 1 50 nil nil nil nil nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// listp, nlistp, consp, atom: exhaustive type discrimination
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proper_list_predicates_type_discrimination() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Classify a wide range of values using all five predicates, building
    // a type signature tuple for each value.
    let form = r#"(let ((values (list nil
                                      t
                                      42
                                      3.14
                                      "string"
                                      'symbol
                                      '(1 2 3)
                                      '(a . b)
                                      (cons nil nil)
                                      [1 2 3]
                                      (make-hash-table)
                                      ?A
                                      :keyword)))
                    (mapcar (lambda (v)
                              (list v
                                    (listp v)
                                    (nlistp v)
                                    (consp v)
                                    (atom v)
                                    (proper-list-p v)
                                    ;; Verify: listp = (not nlistp)
                                    (eq (listp v) (not (nlistp v)))
                                    ;; Verify: consp = (not atom) for non-nil
                                    ;; (nil is both listp and atom)
                                    (if (null v) 'nil-special
                                      (eq (consp v) (not (atom v))))))
                            values))"#;
    let expect = expect_test::expect![[
        r#""OK ((nil t nil nil t 0 t nil-special) (t nil t nil t nil t t) (42 nil t nil t nil t t) (3.14 nil t nil t nil t t) (\"string\" nil t nil t nil t t) (symbol nil t nil t nil t t) ((1 2 3) t nil t nil 3 t t) ((a . b) t nil t nil nil t t) ((nil) t nil t nil 1 t t) ([1 2 3] nil t nil t nil t t) (#s(hash-table) nil t nil t nil t t) (65 nil t nil t nil t t) (:keyword nil t nil t nil t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Deeply nested list structures with mixed proper/improper tails
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proper_list_predicates_deeply_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build progressively deeper structures and test predicates at each level.
    // Also test structures where nesting is a mix of proper and dotted lists.
    let form = r#"(let* (;; Deep proper nesting: ((((1))))
                         (deep1 (list (list (list (list 1)))))
                         ;; Deep with dotted innermost: (((1 . 2)))
                         (deep2 (list (list (cons 1 2))))
                         ;; Build a chain: (a (b (c (d (e . f)))))
                         (chain (list 'a (list 'b (list 'c (list 'd (cons 'e 'f))))))
                         ;; Proper chain: (a (b (c (d (e nil)))))
                         (proper-chain (list 'a (list 'b (list 'c (list 'd (list 'e nil)))))))
                    (list
                      ;; All outer levels are proper lists
                      (proper-list-p deep1)
                      (proper-list-p (car deep1))
                      (proper-list-p (caar deep1))
                      (proper-list-p (caaar deep1))
                      ;; deep2: outer is proper, inner is dotted
                      (proper-list-p deep2)
                      (proper-list-p (car deep2))
                      (proper-list-p (caar deep2))
                      ;; chain: each level is proper list, but innermost element is dotted
                      (proper-list-p chain)
                      (proper-list-p (cadr chain))
                      (consp (car (last (car (last (cadr chain))))))
                      (proper-list-p (car (last (car (last (cadr chain))))))
                      ;; proper-chain: all levels proper
                      (proper-list-p proper-chain)
                      (proper-list-p (cadr proper-chain))
                      ;; Test listp/consp on nested elements
                      (listp deep1)
                      (consp deep1)
                      (listp (caaar deep1))
                      (consp (caaar deep1))
                      (atom (caaaar deep1))))"#;
    let expect = expect_test::expect![[r#""OK (1 1 1 1 1 1 nil 2 2 t 2 2 2 t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// safe-length combined with proper-list-p for structural analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proper_list_predicates_safe_length_analysis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use safe-length and proper-list-p together to classify and measure
    // various list structures, including building a structure classifier.
    let form = r#"(progn
  (fset 'neovm--test-classify-structure
    (lambda (obj)
      "Classify OBJ as proper-list, dotted-list, circular, or atom.
       Returns (type safe-len details)."
      (cond
        ((not (consp obj))
         (list 'atom (safe-length obj) (type-of obj)))
        ((proper-list-p obj)
         (list 'proper-list (safe-length obj) (length obj)))
        (t
         ;; It's a cons but not proper. Is it dotted or circular?
         ;; If safe-length returns more than we can traverse, it might be circular.
         ;; For dotted lists, safe-length counts up to the non-nil cdr.
         (let ((sl (safe-length obj)))
           ;; Try to detect circularity: walk with tortoise-hare
           (let ((slow obj) (fast obj) (is-circular nil) (steps 0))
             (while (and (consp fast) (consp (cdr fast)) (< steps 1000))
               (setq slow (cdr slow)
                     fast (cddr fast)
                     steps (1+ steps))
               (when (eq slow fast)
                 (setq is-circular t)
                 (setq fast nil)))  ;; break
             (if is-circular
                 (list 'circular sl 'detected)
               (list 'dotted-list sl (cdr (last obj))))))))))

  (unwind-protect
      (let* ((structures
               (list nil
                     '(1 2 3)
                     '(a . b)
                     '(x y . z)
                     (cons 1 (cons 2 (cons 3 4)))
                     (make-list 20 'q)
                     42
                     "hello"
                     [vec]
                     (cons nil nil))))
        ;; Also test a circular list
        (let ((circ (list 'p 'q 'r)))
          (setcdr (last circ) circ)
          (let ((circ-result (funcall 'neovm--test-classify-structure circ)))
            (list
              (mapcar (lambda (s) (funcall 'neovm--test-classify-structure s))
                      structures)
              circ-result
              ;; Verify: for proper lists, safe-length == length
              (let ((check-results nil))
                (dolist (s structures)
                  (when (proper-list-p s)
                    (setq check-results
                          (cons (= (safe-length s) (length s))
                                check-results))))
                (nreverse check-results))))))
    (fmakunbound 'neovm--test-classify-structure)))"#;
    let expect = expect_test::expect![[
        r#""OK (((atom 0 symbol) (proper-list 3 3) (dotted-list 1 b) (dotted-list 2 z) (dotted-list 3 4) (proper-list 20 20) (atom 0 integer) (atom 0 string) (atom 0 vector) (proper-list 1 1)) (circular 5 detected) (t t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Type discrimination tree: dispatching on list predicates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proper_list_predicates_dispatch_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a multi-level dispatch tree that uses consp, atom, listp,
    // proper-list-p to route values through different processing paths.
    let form = r#"(progn
  (fset 'neovm--test-process-value
    (lambda (v)
      "Process V through a type-dispatch tree returning a description."
      (cond
        ;; nil is special: it's a list, atom, and proper-list
        ((null v) '(nil-value empty-proper-list atom))
        ;; Proper list: sum if all numbers, concat if all strings, else describe
        ((and (consp v) (proper-list-p v))
         (cond
           ((let ((all-num t))
              (dolist (x v) (unless (numberp x) (setq all-num nil)))
              all-num)
            (list 'number-list (apply '+ v) (length v)))
           ((let ((all-str t))
              (dolist (x v) (unless (stringp x) (setq all-str nil)))
              all-str)
            (list 'string-list (mapconcat 'identity v ",") (length v)))
           (t (list 'mixed-proper-list (length v)
                    (mapcar 'type-of v)))))
        ;; Dotted pair (cons but not proper)
        ((consp v)
         (list 'dotted-pair (car v) (cdr v)
               (consp (car v)) (consp (cdr v))))
        ;; Number
        ((numberp v)
         (list 'number v (* v v)))
        ;; String
        ((stringp v)
         (list 'string v (length v)))
        ;; Vector
        ((vectorp v)
         (list 'vector (length v)))
        ;; Symbol
        ((symbolp v)
         (list 'symbol (symbol-name v)))
        (t (list 'unknown (type-of v))))))

  (unwind-protect
      (let ((test-values
              (list nil
                    '(1 2 3 4 5)
                    '("hello" "world")
                    '(1 "mixed" t)
                    '(a . b)
                    (cons '(1 2) '(3 . 4))
                    42
                    "test"
                    [v e c]
                    'my-symbol
                    t
                    :keyword
                    '(10 20 . 30))))
        (mapcar (lambda (v) (funcall 'neovm--test-process-value v))
                test-values))
    (fmakunbound 'neovm--test-process-value)))"#;
    let expect = expect_test::expect![[
        r#""OK ((nil-value empty-proper-list atom) (number-list 15 5) (string-list \"hello,world\" 2) (mixed-proper-list 3 (integer string symbol)) (dotted-pair a b nil nil) (dotted-pair (1 2) (3 . 4) t t) (number 42 1764) (string \"test\" 4) (vector 3) (symbol \"my-symbol\") (symbol \"t\") (symbol \":keyword\") (dotted-pair 10 (20 . 30) nil t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Predicate consistency: verify invariants across many values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proper_list_predicates_invariant_checks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify fundamental invariants of list predicates:
    // 1. (listp x) == (or (null x) (consp x))
    // 2. (nlistp x) == (not (listp x))
    // 3. (atom x) == (not (consp x))
    // 4. proper-list-p implies listp
    // 5. consp implies listp
    // 6. For nil: (listp nil) = t, (consp nil) = nil, (atom nil) = t
    let form = r#"(let ((values (list nil t 0 1 -1 3.14 "s" 'sym :kw
                                      '(a) '(1 2 3) '(a . b)
                                      (cons nil nil) (cons 1 2)
                                      '(1 2 . 3) [1] (make-list 5 'x)))
                        (all-pass t)
                        (details nil))
                    (dolist (v values)
                      (let* ((inv1 (eq (listp v) (or (null v) (consp v))))
                             (inv2 (eq (nlistp v) (not (listp v))))
                             (inv3 (eq (atom v) (not (consp v))))
                             (inv4 (if (proper-list-p v) (listp v) t))
                             (inv5 (if (consp v) (listp v) t))
                             (pass (and inv1 inv2 inv3 inv4 inv5)))
                        (unless pass (setq all-pass nil))
                        (setq details
                              (cons (list v inv1 inv2 inv3 inv4 inv5 pass)
                                    details))))
                    (list all-pass (nreverse details)))"#;
    let expect = expect_test::expect![[
        r#""OK (t ((nil t t t t t t) (t t t t t t t) (0 t t t t t t) (1 t t t t t t) (-1 t t t t t t) (3.14 t t t t t t) (\"s\" t t t t t t) (sym t t t t t t) (:kw t t t t t t) ((a) t t t t t t) ((1 2 3) t t t t t t) ((a . b) t t t t t t) ((nil) t t t t t t) ((1 . 2) t t t t t t) ((1 2 . 3) t t t t t t) ([1] t t t t t t) ((x x x x x) t t t t t t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Building and filtering heterogeneous collections with predicates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proper_list_predicates_filter_heterogeneous() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a collection of mixed types, then filter using each predicate
    // to partition into groups, verifying mutual exclusivity and coverage.
    let form = r#"(let* ((items (list nil 1 "a" '(x y z) '(p . q) 'sym
                                      (cons 1 (cons 2 3)) t [vec]
                                      '((nested) list) 3.14
                                      (cons nil nil) :kw '(a b . c)))
                         ;; Filter into buckets
                         (consp-items nil)
                         (atom-items nil)
                         (proper-lists nil)
                         (improper-lists nil))
                    (dolist (item items)
                      (if (consp item)
                          (progn
                            (setq consp-items (cons item consp-items))
                            (if (proper-list-p item)
                                (setq proper-lists (cons item proper-lists))
                              (setq improper-lists (cons item improper-lists))))
                        (setq atom-items (cons item atom-items))))
                    (list
                      ;; Counts
                      (length (nreverse consp-items))
                      (length (nreverse atom-items))
                      (length (nreverse proper-lists))
                      (length (nreverse improper-lists))
                      ;; consp + atom = total (mutual exclusivity)
                      (= (+ (length consp-items) (length atom-items))
                         (length items))
                      ;; proper + improper = consp (partition of consp)
                      (= (+ (length proper-lists) (length improper-lists))
                         (length consp-items))
                      ;; nil is atom AND proper-list but NOT consp
                      (list (atom nil) (proper-list-p nil) (consp nil) (listp nil))
                      ;; Every atom is nlistp, except nil
                      (let ((check t))
                        (dolist (a atom-items)
                          (unless (null a)
                            (unless (nlistp a) (setq check nil))))
                        check)))"#;
    let expect = expect_test::expect![[r#""OK (6 8 3 3 nil nil (t 0 nil t) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

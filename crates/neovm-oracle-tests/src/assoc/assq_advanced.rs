//! Advanced oracle parity tests for association list lookup primitives.
//!
//! Tests `assoc` with TESTFN, `assq` vs `assoc` differences, `rassoc`,
//! `rassq`, `assoc-default`, nested alists, and complex alist patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// assoc with TESTFN parameter (custom comparison)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_assoc_with_testfn() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // assoc accepts an optional TESTFN for custom comparison
    let form = r#"(list
                   ;; Default: uses equal
                   (assoc "hello" '(("hello" . 1) ("world" . 2)))
                   ;; With string= as testfn (same behavior as default for strings)
                   (assoc "hello" '(("hello" . 1) ("world" . 2)) #'string=)
                   ;; Case-insensitive comparison via custom testfn
                   (assoc "HELLO"
                          '(("hello" . 1) ("world" . 2))
                          (lambda (a b) (string-equal (downcase a) (downcase b))))
                   ;; Numeric comparison with tolerance
                   (assoc 3.01
                          '((1.0 . "one") (2.0 . "two") (3.0 . "three"))
                          (lambda (a b) (< (abs (- a b)) 0.1)))
                   ;; No match with strict testfn
                   (assoc 3.5
                          '((1.0 . "one") (2.0 . "two") (3.0 . "three"))
                          (lambda (a b) (< (abs (- a b)) 0.1)))
                   ;; eq as testfn: like assq
                   (assoc 'b '((a . 1) (b . 2) (c . 3)) #'eq))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"hello\" . 1) (\"hello\" . 1) (\"hello\" . 1) (3.0 . \"three\") nil (b . 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// assq vs assoc: eq vs equal on various key types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_assq_vs_assoc_key_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // assq uses eq (identity), assoc uses equal (structural)
    let form = r#"(let ((alist '(("alpha" . 1) ("beta" . 2) ("gamma" . 3))))
                    (list
                     ;; assoc finds string keys (equal compares by content)
                     (assoc "beta" alist)
                     ;; assq does NOT find string keys (eq compares identity)
                     (assq "beta" alist)
                     ;; Both find symbol keys (symbols are interned, eq works)
                     (let ((sym-alist '((x . 10) (y . 20) (z . 30))))
                       (list (assoc 'y sym-alist)
                             (assq 'y sym-alist)))
                     ;; Both find integer keys (fixnums are eq by value)
                     (let ((int-alist '((1 . "a") (2 . "b") (3 . "c"))))
                       (list (assoc 2 int-alist)
                             (assq 2 int-alist)))
                     ;; assoc finds list keys, assq does not
                     (let ((list-alist '(((1 2) . "pair1") ((3 4) . "pair2"))))
                       (list (assoc '(1 2) list-alist)
                             (assq '(1 2) list-alist)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"beta\" . 2) nil ((y . 20) (y . 20)) ((2 . \"b\") (2 . \"b\")) (((1 2) . \"pair1\") nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// rassoc / rassq: reverse lookup by value
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rassoc_rassq_reverse_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // rassoc searches by value (cdr) using equal; rassq uses eq
    let form = r#"(let ((alist '((a . "alpha") (b . "beta") (c . "gamma")
                                  (d . "delta") (e . "alpha"))))
                    (list
                     ;; rassoc: finds first pair with matching value
                     (rassoc "alpha" alist)
                     (rassoc "gamma" alist)
                     (rassoc "missing" alist)
                     ;; rassq: uses eq, won't find string values
                     (rassq "alpha" alist)
                     ;; rassq works with symbol values
                     (let ((sym-alist '(("x" . found) ("y" . lost) ("z" . found))))
                       (list (rassq 'found sym-alist)
                             (rassq 'lost sym-alist)
                             (rassq 'none sym-alist)))
                     ;; rassoc with numeric values
                     (let ((num-alist '((a . 1) (b . 2) (c . 3) (d . 2))))
                       (list (rassoc 2 num-alist)
                             (rassq 2 num-alist)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((a . \"alpha\") (c . \"gamma\") nil nil ((\"x\" . found) (\"y\" . lost) nil) ((b . 2) (b . 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// assoc-default behavior with missing keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_assoc_default_behavior() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // assoc-default returns the cdr of the found pair, or a default value
    let form = r#"(list
                   ;; Basic: returns cdr of found pair
                   (assoc-default 'b '((a . 1) (b . 2) (c . 3)))
                   ;; Missing key: returns nil by default
                   (assoc-default 'z '((a . 1) (b . 2)))
                   ;; String keys
                   (assoc-default "hello" '(("hello" . "world") ("foo" . "bar")))
                   ;; With custom testfn
                   (assoc-default "HELLO"
                                  '(("hello" . "world") ("foo" . "bar"))
                                  (lambda (a b) (string-equal (downcase a) (downcase b))))
                   ;; Key in value position (reversed alist matching)
                   (assoc-default 'b '((a . 1) (b . 2) (c . 3)))
                   ;; With numeric keys and default return
                   (assoc-default 99 '((1 . "one") (2 . "two")))
                   ;; assoc-default with default parameter
                   (assoc-default 'missing '((a . 1)) nil 'fallback))"#;
    let expect = expect_test::expect![[r#""OK (2 nil \"world\" \"world\" 2 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested alist lookups (alist of alists)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nested_alist_lookups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two-level nested alist: outer keys map to inner alists
    let form = r#"(let ((db '((users . ((alice . ((age . 30) (role . admin)))
                                         (bob . ((age . 25) (role . user)))))
                              (config . ((debug . t)
                                         (version . "1.0")
                                         (limits . ((max-conn . 100)
                                                    (timeout . 30))))))))
                    (let ((lookup
                           (lambda (keys db)
                             (let ((result db))
                               (dolist (key keys)
                                 (setq result (cdr (assq key result))))
                               result))))
                      (list
                       ;; Single level lookup
                       (cdr (assq 'users db))
                       ;; Two level lookup
                       (funcall lookup '(users alice age) db)
                       (funcall lookup '(users bob role) db)
                       ;; Three level lookup
                       (funcall lookup '(config limits max-conn) db)
                       (funcall lookup '(config limits timeout) db)
                       ;; Missing intermediate key returns nil
                       (funcall lookup '(users charlie age) db)
                       ;; Top-level config values
                       (funcall lookup '(config debug) db)
                       (funcall lookup '(config version) db))))"#;
    let expect = expect_test::expect![[
        r#""OK (((alice (age . 30) (role . admin)) (bob (age . 25) (role . user))) 30 user 100 30 nil t \"1.0\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: alist-based multimap (multiple values per key)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_multimap() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multimap: each key can map to multiple values stored as a list
    let form = r#"(let ((mm-add nil)
                        (mm-get nil)
                        (mm-remove nil)
                        (mm-keys nil))
                    ;; Add a value to a key's list (returns new multimap)
                    (setq mm-add
                          (lambda (mm key val)
                            (let ((existing (assq key mm)))
                              (if existing
                                  (progn
                                    (setcdr existing (cons val (cdr existing)))
                                    mm)
                                (cons (cons key (list val)) mm)))))
                    ;; Get all values for a key
                    (setq mm-get
                          (lambda (mm key)
                            (cdr (assq key mm))))
                    ;; Remove a specific value from a key
                    (setq mm-remove
                          (lambda (mm key val)
                            (let ((entry (assq key mm)))
                              (when entry
                                (setcdr entry (delq val (cdr entry)))
                                ;; Remove entry if no values left
                                (if (null (cdr entry))
                                    (delq entry mm)
                                  mm)))))
                    ;; Get all keys
                    (setq mm-keys
                          (lambda (mm)
                            (mapcar #'car mm)))
                    ;; Build and query a multimap
                    (let ((mm nil))
                      (setq mm (funcall mm-add mm 'tag '(post1)))
                      (setq mm (funcall mm-add mm 'tag '(post2)))
                      (setq mm (funcall mm-add mm 'tag '(post3)))
                      (setq mm (funcall mm-add mm 'category '(tech)))
                      (setq mm (funcall mm-add mm 'category '(science)))
                      (let ((result-before
                             (list
                              (funcall mm-get mm 'tag)
                              (funcall mm-get mm 'category)
                              (funcall mm-keys mm))))
                        ;; Remove one value
                        (setq mm (funcall mm-remove mm 'tag '(post2)))
                        (list
                         result-before
                         (funcall mm-get mm 'tag)
                         (funcall mm-get mm 'category)))))"#;
    let expect = expect_test::expect![
        r#""OK ((((post3) (post2) (post1)) ((science) (tech)) (category tag)) ((post3) (post2) (post1)) ((science) (tech)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: alist merge with conflict resolution strategies
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_merge_strategies() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Merge two alists with different conflict resolution strategies
    let form = r#"(let ((alist-merge nil))
                    ;; Merge alists: strategy is 'left, 'right, or a function
                    (setq alist-merge
                          (lambda (a1 a2 strategy)
                            (let ((result (copy-alist a1)))
                              (dolist (pair a2)
                                (let ((existing (assq (car pair) result)))
                                  (if existing
                                      (cond
                                       ((eq strategy 'left) nil)
                                       ((eq strategy 'right)
                                        (setcdr existing (cdr pair)))
                                       ((functionp strategy)
                                        (setcdr existing
                                                (funcall strategy
                                                         (cdr existing)
                                                         (cdr pair)))))
                                    (setq result (cons (cons (car pair) (cdr pair))
                                                       result)))))
                              result)))
                    (let ((defaults '((width . 80) (height . 24) (color . blue)))
                          (overrides '((width . 120) (color . red) (font . mono))))
                      ;; Sort results for deterministic comparison
                      (let ((sort-alist
                             (lambda (al)
                               (sort (copy-alist al)
                                     (lambda (a b)
                                       (string< (symbol-name (car a))
                                                (symbol-name (car b))))))))
                        (list
                         ;; Keep left on conflict
                         (funcall sort-alist
                                  (funcall alist-merge defaults overrides 'left))
                         ;; Keep right on conflict
                         (funcall sort-alist
                                  (funcall alist-merge defaults overrides 'right))
                         ;; Sum on conflict (for numeric values)
                         (funcall sort-alist
                                  (funcall alist-merge
                                           '((a . 1) (b . 2) (c . 3))
                                           '((b . 10) (c . 20) (d . 30))
                                           (lambda (old new) (+ old new))))))))"#;
    let expect = expect_test::expect![
        r#""OK (((color . blue) (font . mono) (height . 24) (width . 80)) ((color . red) (font . mono) (height . 24) (width . 120)) ((a . 1) (b . 12) (c . 23) (d . 30)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: alist-based query engine with predicate matching
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_query_engine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simple query engine over a list of alist records
    let form = r#"(let ((records '(((name . "Alice") (age . 30) (dept . "eng"))
                                   ((name . "Bob") (age . 25) (dept . "eng"))
                                   ((name . "Carol") (age . 35) (dept . "sales"))
                                   ((name . "Dave") (age . 28) (dept . "eng"))
                                   ((name . "Eve") (age . 40) (dept . "sales"))))
                        (query-where nil)
                        (query-select nil)
                        (query-order-by nil))
                    ;; Filter records by predicate on a field
                    (setq query-where
                          (lambda (recs field pred)
                            (let ((result nil))
                              (dolist (r recs)
                                (let ((val (cdr (assoc field r))))
                                  (when (funcall pred val)
                                    (setq result (cons r result)))))
                              (nreverse result))))
                    ;; Select specific fields from records
                    (setq query-select
                          (lambda (recs fields)
                            (mapcar (lambda (r)
                                      (let ((selected nil))
                                        (dolist (f fields)
                                          (let ((pair (assoc f r)))
                                            (when pair
                                              (setq selected
                                                    (cons pair selected)))))
                                        (nreverse selected)))
                                    recs)))
                    ;; Sort records by a numeric field
                    (setq query-order-by
                          (lambda (recs field direction)
                            (sort (copy-sequence recs)
                                  (lambda (a b)
                                    (let ((va (cdr (assoc field a)))
                                          (vb (cdr (assoc field b))))
                                      (if (eq direction 'asc)
                                          (< va vb)
                                        (> va vb)))))))
                    (list
                     ;; WHERE dept = "eng"
                     (funcall query-select
                              (funcall query-where records 'dept
                                       (lambda (v) (string= v "eng")))
                              '(name age))
                     ;; WHERE age > 30 ORDER BY age DESC, select name
                     (funcall query-select
                              (funcall query-order-by
                                       (funcall query-where records 'age
                                                (lambda (v) (> v 30)))
                                       'age 'desc)
                              '(name))
                     ;; COUNT WHERE dept = "eng"
                     (length (funcall query-where records 'dept
                                      (lambda (v) (string= v "eng"))))
                     ;; Chained: eng dept, age > 26, sorted ascending
                     (funcall query-select
                              (funcall query-order-by
                                       (funcall query-where
                                                (funcall query-where records
                                                         'dept
                                                         (lambda (v) (string= v "eng")))
                                                'age
                                                (lambda (v) (> v 26)))
                                       'age 'asc)
                              '(name age))))"#;
    let expect = expect_test::expect![[
        r#""OK ((((name . \"Alice\") (age . 30)) ((name . \"Bob\") (age . 25)) ((name . \"Dave\") (age . 28))) (((name . \"Eve\")) ((name . \"Carol\"))) 3 (((name . \"Dave\") (age . 28)) ((name . \"Alice\") (age . 30))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

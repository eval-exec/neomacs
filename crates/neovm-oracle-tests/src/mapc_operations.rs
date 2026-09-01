//! Oracle parity tests for `mapc` — the side-effect-only mapping function.
//!
//! Covers: return value semantics, accumulation patterns, comparison with
//! `mapcar`, nested list traversal, hash-table population from alists,
//! and complex buffer-style operations via mapc.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// mapc returns the original list, not mapped values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapc_returns_original_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // mapc must return its LIST argument unchanged, not a list of results.
    // We test by comparing the returned value with eq (identity check)
    // and also by inspecting its printed form.
    let form = r#"(let* ((original '(10 20 30 40 50))
                          (returned (mapc (lambda (x) (* x x)) original)))
                    (list (eq original returned)
                          returned
                          (length returned)))"#;
    let expect = expect_test::expect![[r#""OK (t (10 20 30 40 50) 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapc with accumulation side effects (summation + product)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapc_accumulate_side_effects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use mapc to accumulate a running sum and a running product into
    // separate variables.  Also track the call count.
    let form = r#"(let ((sum 0)
                        (product 1)
                        (call-count 0))
                    (mapc (lambda (x)
                            (setq sum (+ sum x))
                            (setq product (* product x))
                            (setq call-count (1+ call-count)))
                          '(2 3 5 7 11))
                    (list sum product call-count))"#;
    let expect = expect_test::expect![[r#""OK (28 2310 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapc vs mapcar: return value difference
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapc_vs_mapcar_return_difference() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // mapcar collects results, mapc discards them and returns original list.
    // Run the same lambda over the same list with both, compare.
    let form = r#"(let* ((input '(1 2 3 4 5))
                          (mapcar-result (mapcar (lambda (x) (* x 10)) input))
                          (collector nil)
                          (mapc-result (mapc (lambda (x)
                                               (setq collector (cons (* x 10) collector)))
                                             input)))
                    (list
                     ;; mapcar returns new list of transformed values
                     mapcar-result
                     ;; mapc returns the original input list
                     (eq mapc-result input)
                     mapc-result
                     ;; but we can still collect side effects
                     (nreverse collector)
                     ;; both produce the same effective values
                     (equal mapcar-result (nreverse (let ((c nil))
                                                     (mapc (lambda (x) (setq c (cons (* x 10) c))) input)
                                                     c)))))"#;
    let expect =
        expect_test::expect![[r#""OK ((10 20 30 40 50) t (1 2 3 4 5) (10 20 30 40 50) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapc with complex lambda (string processing + conditional logic)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapc_complex_lambda_string_processing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Process a list of "key:value" strings, categorise by prefix,
    // and accumulate results into separate bins via mapc.
    let form = r#"(progn
  (defvar neovm--test-mapc-errors nil)
  (defvar neovm--test-mapc-warnings nil)
  (defvar neovm--test-mapc-info nil)

  (unwind-protect
      (progn
        (setq neovm--test-mapc-errors nil
              neovm--test-mapc-warnings nil
              neovm--test-mapc-info nil)
        (mapc (lambda (entry)
                (let* ((colon-pos (string-match ":" entry))
                       (level (if colon-pos (substring entry 0 colon-pos) "unknown"))
                       (msg (if colon-pos (substring entry (1+ colon-pos)) entry)))
                  (cond
                   ((string= level "ERROR")
                    (setq neovm--test-mapc-errors
                          (cons msg neovm--test-mapc-errors)))
                   ((string= level "WARN")
                    (setq neovm--test-mapc-warnings
                          (cons msg neovm--test-mapc-warnings)))
                   (t
                    (setq neovm--test-mapc-info
                          (cons msg neovm--test-mapc-info))))))
              '("ERROR:disk full" "INFO:started" "WARN:low memory"
                "ERROR:timeout" "INFO:connected" "INFO:ready"
                "WARN:high cpu" "ERROR:crash"))
        (list (nreverse neovm--test-mapc-errors)
              (nreverse neovm--test-mapc-warnings)
              (nreverse neovm--test-mapc-info)
              (+ (length neovm--test-mapc-errors)
                 (length neovm--test-mapc-warnings)
                 (length neovm--test-mapc-info))))
    (makunbound 'neovm--test-mapc-errors)
    (makunbound 'neovm--test-mapc-warnings)
    (makunbound 'neovm--test-mapc-info)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"disk full\" \"timeout\" \"crash\") (\"low memory\" \"high cpu\") (\"started\" \"connected\" \"ready\") 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapc over nested lists with recursive side effects
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapc_nested_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Flatten a nested list structure using mapc + recursion,
    // accumulating leaf values into a flat list.
    let form = r#"(progn
  (defvar neovm--test-mapc-flat nil)

  (fset 'neovm--test-mapc-flatten
    (lambda (lst)
      (mapc (lambda (elem)
              (if (listp elem)
                  (funcall 'neovm--test-mapc-flatten elem)
                (setq neovm--test-mapc-flat
                      (cons elem neovm--test-mapc-flat))))
            lst)))

  (unwind-protect
      (progn
        (setq neovm--test-mapc-flat nil)
        (funcall 'neovm--test-mapc-flatten
                 '(1 (2 3) (4 (5 6)) ((7 (8 9)) 10)))
        (let ((result (nreverse neovm--test-mapc-flat)))
          ;; Should be (1 2 3 4 5 6 7 8 9 10)
          (list result
                (length result)
                (apply '+ result))))
    (makunbound 'neovm--test-mapc-flat)
    (fmakunbound 'neovm--test-mapc-flatten)))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5 6 7 8 9 10) 10 55)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapc to populate a hash table from an alist (complex)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapc_populate_hash_table_from_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a frequency table from a word list using mapc,
    // then extract sorted results.
    let form = r#"(let ((freq (make-hash-table :test 'equal)))
  ;; Count word frequencies
  (mapc (lambda (word)
          (puthash word
                   (1+ (gethash word freq 0))
                   freq))
        '("apple" "banana" "apple" "cherry" "banana" "apple"
          "date" "cherry" "banana" "apple" "elderberry" "date"))
  ;; Extract into alist and sort by count descending
  (let ((pairs nil))
    (maphash (lambda (k v)
               (setq pairs (cons (cons k v) pairs)))
             freq)
    (setq pairs (sort pairs (lambda (a b)
                              (> (cdr a) (cdr b)))))
    (list (mapcar 'car pairs)
          (mapcar 'cdr pairs)
          (hash-table-count freq)
          ;; Verify total
          (let ((total 0))
            (mapc (lambda (p) (setq total (+ total (cdr p)))) pairs)
            total))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"apple\" \"banana\" \"date\" \"cherry\" \"elderberry\") (4 3 2 2 1) 5 12)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapc chained with other mapping functions in a pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapc_chained_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Pipeline: use mapc to validate and filter, mapcar to transform,
    // mapconcat to format.
    let form = r#"(let ((valid nil)
                        (invalid-count 0))
  ;; Phase 1: validate with mapc (side effects only)
  (mapc (lambda (entry)
          (if (and (consp entry)
                   (stringp (car entry))
                   (numberp (cdr entry))
                   (> (cdr entry) 0))
              (setq valid (cons entry valid))
            (setq invalid-count (1+ invalid-count))))
        '(("alice" . 95) ("bob" . -1) ("carol" . 87)
          (42 . "bad") ("dave" . 73) ("eve" . 0)
          ("frank" . 91) nil ("grace" . 88)))
  (setq valid (nreverse valid))
  ;; Phase 2: transform with mapcar
  (let ((graded (mapcar (lambda (entry)
                          (let ((name (car entry))
                                (score (cdr entry)))
                            (list name score
                                  (cond ((>= score 90) "A")
                                        ((>= score 80) "B")
                                        ((>= score 70) "C")
                                        (t "F")))))
                        valid)))
    ;; Phase 3: format with mapconcat
    (list (mapconcat (lambda (r)
                       (format "%s:%s(%s)" (nth 0 r) (nth 1 r) (nth 2 r)))
                     graded ", ")
          invalid-count
          (length graded))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"alice:95(A), carol:87(B), dave:73(C), frank:91(A), grace:88(B)\" 4 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapc with unwind-protect ensuring cleanup on error
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapc_with_error_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use mapc inside condition-case to handle errors mid-iteration,
    // collecting successes and error info separately.
    let form = r#"(let ((successes nil)
                        (failures nil))
  (mapc (lambda (pair)
          (condition-case err
              (let ((result (/ (car pair) (cdr pair))))
                (setq successes (cons (cons pair result) successes)))
            (arith-error
             (setq failures (cons (cons pair 'div-by-zero) failures)))
            (wrong-type-argument
             (setq failures (cons (cons pair 'type-error) failures)))))
        '((100 . 5) (42 . 0) (81 . 3) (7 . 0) (60 . 4) (33 . 11)))
  (list (nreverse (mapcar (lambda (s) (cdr s)) successes))
        (length failures)
        (mapcar (lambda (f) (cdr f)) (nreverse failures))))"#;
    let expect = expect_test::expect![[r#""OK ((20 27 15 3) 2 (div-by-zero div-by-zero))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapc with stateful closure counter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapc_stateful_closure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create a closure that tracks state across mapc calls
    let form = r#"(let ((state (list 0 0 0)))
  ;; state = (count sum max)
  (mapc (lambda (x)
          (setcar state (1+ (car state)))
          (setcar (cdr state) (+ (cadr state) x))
          (when (> x (caddr state))
            (setcar (cddr state) x)))
        '(14 7 23 3 42 18 9 35 1 28))
  ;; Extract statistics
  (list (car state)
        (cadr state)
        (caddr state)
        ;; Compute average (integer division)
        (/ (cadr state) (car state))))"#;
    let expect = expect_test::expect![[r#""OK (10 180 42 18)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

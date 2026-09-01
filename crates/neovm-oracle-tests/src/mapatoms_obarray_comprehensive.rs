//! Comprehensive oracle parity tests for `mapatoms` and obarray operations:
//! mapatoms with custom obarrays, collecting symbols with specific properties,
//! intern/unintern in custom obarrays, obarray size effects, mapatoms with
//! side effects (setting properties on each symbol), mapatoms counting/filtering,
//! nested mapatoms calls, obarray interactions with intern-soft, and mapatoms
//! with condition-case for error symbols.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// mapatoms collecting symbols with specific name patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapatoms_collect_by_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Intern symbols with different prefixes into a custom obarray,
    // then use mapatoms to collect only those matching a given prefix.
    let form = r#"(let ((ob (make-vector 31 0)))
  ;; Intern symbols with various prefixes
  (dolist (name '("neovm--moc-alpha-one-9912"
                  "neovm--moc-alpha-two-9912"
                  "neovm--moc-beta-one-9912"
                  "neovm--moc-beta-two-9912"
                  "neovm--moc-beta-three-9912"
                  "neovm--moc-gamma-only-9912"))
    (intern name ob))
  ;; Collect only "beta" symbols
  (let ((beta-syms nil)
        (all-syms nil))
    (mapatoms (lambda (sym)
                (let ((name (symbol-name sym)))
                  (setq all-syms (cons name all-syms))
                  (when (string-match-p "beta" name)
                    (setq beta-syms (cons name beta-syms)))))
              ob)
    (list (sort beta-syms #'string<)
          (length all-syms)
          ;; Verify no spurious symbols
          (= (length all-syms) 6))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"neovm--moc-beta-one-9912\" \"neovm--moc-beta-three-9912\" \"neovm--moc-beta-two-9912\") 6 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapatoms setting and reading symbol properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapatoms_set_plist_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use mapatoms to set a property on each symbol based on its name length,
    // then read properties back with a second mapatoms pass.
    let form = r#"(let ((ob (make-vector 13 0)))
  (dolist (name '("neovm--moc-sp-a-8834"
                  "neovm--moc-sp-ab-8834"
                  "neovm--moc-sp-abc-8834"
                  "neovm--moc-sp-abcd-8834"))
    (intern name ob))
  ;; First pass: set :namelen property on each symbol
  (mapatoms (lambda (sym)
              (put sym 'neovm--namelen (length (symbol-name sym))))
            ob)
  ;; Second pass: collect (name . length) pairs
  (let ((result nil))
    (mapatoms (lambda (sym)
                (setq result
                      (cons (cons (symbol-name sym)
                                  (get sym 'neovm--namelen))
                            result)))
              ob)
    ;; Sort for deterministic output and verify lengths
    (let ((sorted (sort result (lambda (a b) (string< (car a) (car b))))))
      (list sorted
            ;; Verify each length matches actual name length
            (cl-every (lambda (pair)
                        (= (cdr pair) (length (car pair))))
                      sorted)))))"#;
    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapatoms counting and filtering with accumulator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapatoms_count_filter_accumulate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Count symbols, filter by name length, accumulate a hash of names.
    let form = r#"(let ((ob (make-vector 7 0)))
  ;; Intern symbols of varying name lengths
  (dolist (name '("neovm--moc-cf-x-3301"
                  "neovm--moc-cf-xy-3301"
                  "neovm--moc-cf-xyz-3301"
                  "neovm--moc-cf-xyzw-3301"
                  "neovm--moc-cf-xyzwv-3301"
                  "neovm--moc-cf-xyzwvu-3301"
                  "neovm--moc-cf-xyzwvut-3301"
                  "neovm--moc-cf-xyzwvuts-3301"))
    (intern name ob))
  (let ((count 0)
        (long-names nil)
        (total-len 0))
    (mapatoms (lambda (sym)
                (let* ((name (symbol-name sym))
                       (len (length name)))
                  (setq count (1+ count))
                  (setq total-len (+ total-len len))
                  (when (> len 26)
                    (setq long-names (cons name long-names)))))
              ob)
    (list count
          (sort long-names #'string<)
          total-len
          ;; Verify count matches what we interned
          (= count 8))))"#;
    let expect = expect_test::expect![[r#""OK (8 (\"neovm--moc-cf-xyzwvuts-3301\") 188 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// intern/unintern cycle in custom obarray
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapatoms_intern_unintern_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Intern, unintern some, re-intern, verify final state with mapatoms.
    let form = r#"(let ((ob (make-vector 11 0)))
  ;; Phase 1: intern 5 symbols
  (dolist (name '("neovm--moc-iu-a-5541"
                  "neovm--moc-iu-b-5541"
                  "neovm--moc-iu-c-5541"
                  "neovm--moc-iu-d-5541"
                  "neovm--moc-iu-e-5541"))
    (intern name ob))
  ;; Phase 2: unintern b and d
  (unintern "neovm--moc-iu-b-5541" ob)
  (unintern "neovm--moc-iu-d-5541" ob)
  ;; Phase 3: re-intern b with a new symbol, add f
  (intern "neovm--moc-iu-b-5541" ob)
  (intern "neovm--moc-iu-f-5541" ob)
  ;; Collect final state
  (let ((names nil))
    (mapatoms (lambda (sym)
                (setq names (cons (symbol-name sym) names)))
              ob)
    (list (sort names #'string<)
          ;; d should still be absent
          (null (intern-soft "neovm--moc-iu-d-5541" ob))
          ;; b should be present again
          (not (null (intern-soft "neovm--moc-iu-b-5541" ob)))
          ;; f should be present
          (not (null (intern-soft "neovm--moc-iu-f-5541" ob)))
          ;; Count should be 5 (a, b-new, c, e, f)
          (= (length names) 5))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"neovm--moc-iu-a-5541\" \"neovm--moc-iu-b-5541\" \"neovm--moc-iu-c-5541\" \"neovm--moc-iu-e-5541\" \"neovm--moc-iu-f-5541\") t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested mapatoms: obarray of obarrays
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapatoms_nested_obarrays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create multiple obarrays, collect from each using nested mapatoms style.
    let form = r#"(let ((ob1 (make-vector 7 0))
      (ob2 (make-vector 7 0))
      (ob3 (make-vector 7 0)))
  ;; Populate each obarray with different symbols
  (dolist (n '("neovm--moc-no-p-2201" "neovm--moc-no-q-2201"))
    (intern n ob1))
  (dolist (n '("neovm--moc-no-r-2201" "neovm--moc-no-s-2201" "neovm--moc-no-t-2201"))
    (intern n ob2))
  (dolist (n '("neovm--moc-no-u-2201"))
    (intern n ob3))
  ;; Collect from all three, concatenate
  (let ((all-names nil))
    (dolist (ob (list ob1 ob2 ob3))
      (mapatoms (lambda (sym)
                  (setq all-names (cons (symbol-name sym) all-names)))
                ob))
    (list (sort all-names #'string<)
          (= (length all-names) 6))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"neovm--moc-no-p-2201\" \"neovm--moc-no-q-2201\" \"neovm--moc-no-r-2201\" \"neovm--moc-no-s-2201\" \"neovm--moc-no-t-2201\" \"neovm--moc-no-u-2201\") t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// intern-soft interactions with intern and unintern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapatoms_intern_soft_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Comprehensive intern-soft tests: before intern, after intern,
    // after unintern, with symbol objects, re-intern identity.
    let form = r#"(let ((ob (make-vector 17 0)))
  (let ((before-intern (intern-soft "neovm--moc-ise-x-7712" ob)))
    ;; intern it
    (let ((sym (intern "neovm--moc-ise-x-7712" ob)))
      ;; intern-soft now returns the symbol
      (let ((after-intern (intern-soft "neovm--moc-ise-x-7712" ob)))
        ;; They should be eq
        (let ((eq-check (eq sym after-intern)))
          ;; intern-soft with symbol object
          (let ((by-sym (intern-soft sym ob)))
            ;; unintern it
            (unintern "neovm--moc-ise-x-7712" ob)
            (let ((after-unintern (intern-soft "neovm--moc-ise-x-7712" ob)))
              ;; Re-intern: creates NEW symbol, not eq to old
              (let ((new-sym (intern "neovm--moc-ise-x-7712" ob)))
                (list (null before-intern)           ; nil before intern
                      (symbolp after-intern)         ; symbol after intern
                      eq-check                       ; eq to original
                      (eq by-sym sym)                ; intern-soft by sym works
                      (null after-unintern)           ; nil after unintern
                      (symbolp new-sym)              ; new symbol after re-intern
                      (equal (symbol-name new-sym)
                             "neovm--moc-ise-x-7712") ; same name
                      )))))))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Obarray size effects: hash collision stress test
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapatoms_obarray_size_collision_stress() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use a tiny obarray (size 1) to force all symbols into same bucket.
    // Verify all operations still work correctly with heavy collisions.
    let form = r#"(let ((ob (make-vector 1 0))
      (names (mapcar (lambda (i)
                       (format "neovm--moc-stress-%03d-4489" i))
                     (number-sequence 0 19))))
  ;; Intern all
  (dolist (n names) (intern n ob))
  ;; Verify all findable
  (let ((all-found (cl-every (lambda (n) (not (null (intern-soft n ob)))) names)))
    ;; Unintern every other one
    (let ((removed nil)
          (kept nil))
      (let ((i 0))
        (dolist (n names)
          (if (= (% i 2) 0)
              (progn (unintern n ob)
                     (setq removed (cons n removed)))
            (setq kept (cons n kept)))
          (setq i (1+ i))))
      ;; Verify removed are gone, kept are present
      (let ((removed-gone (cl-every (lambda (n) (null (intern-soft n ob))) removed))
            (kept-present (cl-every (lambda (n) (not (null (intern-soft n ob)))) kept)))
        ;; Collect via mapatoms
        (let ((collected nil))
          (mapatoms (lambda (sym)
                      (setq collected (cons (symbol-name sym) collected)))
                    ob)
          (list all-found
                removed-gone
                kept-present
                (= (length collected) (length kept))
                (equal (sort (copy-sequence collected) #'string<)
                       (sort (copy-sequence kept) #'string<))))))))"#;
    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapatoms with condition-case: handle errors during iteration
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapatoms_with_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Set some symbols to have values, some void. Use mapatoms to
    // safely access values via condition-case, collecting successes and errors.
    let form = r#"(let ((ob (make-vector 11 0)))
  ;; Intern symbols
  (let ((s1 (intern "neovm--moc-cc-a-6623" ob))
        (s2 (intern "neovm--moc-cc-b-6623" ob))
        (s3 (intern "neovm--moc-cc-c-6623" ob))
        (s4 (intern "neovm--moc-cc-d-6623" ob)))
    ;; Set values for some, leave others void
    (set s1 42)
    (set s3 "hello")
    ;; s2 and s4 are void
    (let ((successes nil)
          (errors nil))
      (mapatoms (lambda (sym)
                  (condition-case _err
                      (progn
                        (symbol-value sym)
                        (setq successes (cons (symbol-name sym) successes)))
                    (void-variable
                     (setq errors (cons (symbol-name sym) errors)))))
                ob)
      (list (sort successes #'string<)
            (sort errors #'string<)
            (= (+ (length successes) (length errors)) 4)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"neovm--moc-cc-a-6623\" \"neovm--moc-cc-c-6623\") (\"neovm--moc-cc-b-6623\" \"neovm--moc-cc-d-6623\") t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapatoms building a symbol dependency graph
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapatoms_build_dependency_graph() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Symbols store a "depends-on" property pointing to other symbol names.
    // Use mapatoms to build a dependency adjacency list.
    let form = r#"(let ((ob (make-vector 13 0)))
  ;; Create symbols and set dependency properties
  (let ((sa (intern "neovm--moc-dg-a-1192" ob))
        (sb (intern "neovm--moc-dg-b-1192" ob))
        (sc (intern "neovm--moc-dg-c-1192" ob))
        (sd (intern "neovm--moc-dg-d-1192" ob)))
    (put sa 'neovm--depends '("neovm--moc-dg-b-1192" "neovm--moc-dg-c-1192"))
    (put sb 'neovm--depends '("neovm--moc-dg-c-1192"))
    (put sc 'neovm--depends nil)
    (put sd 'neovm--depends '("neovm--moc-dg-a-1192" "neovm--moc-dg-b-1192")))
  ;; Build adjacency list via mapatoms
  (let ((graph nil))
    (mapatoms (lambda (sym)
                (let ((deps (get sym 'neovm--depends)))
                  (setq graph
                        (cons (cons (symbol-name sym)
                                    (or deps '()))
                              graph))))
              ob)
    ;; Sort for determinism
    (let ((sorted (sort graph (lambda (a b) (string< (car a) (car b))))))
      ;; Also compute in-degree for each node
      (let ((in-degrees nil))
        (dolist (entry sorted)
          (dolist (dep (cdr entry))
            (let ((existing (assoc dep in-degrees)))
              (if existing
                  (setcdr existing (1+ (cdr existing)))
                (setq in-degrees (cons (cons dep 1) in-degrees))))))
        (list sorted
              (sort in-degrees (lambda (a b) (string< (car a) (car b)))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"neovm--moc-dg-a-1192\" \"neovm--moc-dg-b-1192\" \"neovm--moc-dg-c-1192\") (\"neovm--moc-dg-b-1192\" \"neovm--moc-dg-c-1192\") (\"neovm--moc-dg-c-1192\") (\"neovm--moc-dg-d-1192\" \"neovm--moc-dg-a-1192\" \"neovm--moc-dg-b-1192\")) ((\"neovm--moc-dg-a-1192\" . 1) (\"neovm--moc-dg-b-1192\" . 2) (\"neovm--moc-dg-c-1192\" . 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapatoms with symbol-function: collecting callable symbols
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapatoms_collect_callable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Set some symbols to have function bindings, collect only those that
    // are fboundp via mapatoms.
    let form = r#"(let ((ob (make-vector 11 0)))
  (let ((s1 (intern "neovm--moc-callable-a-3344" ob))
        (s2 (intern "neovm--moc-callable-b-3344" ob))
        (s3 (intern "neovm--moc-callable-c-3344" ob))
        (s4 (intern "neovm--moc-callable-d-3344" ob))
        (s5 (intern "neovm--moc-callable-e-3344" ob)))
    ;; Bind functions to some
    (fset s1 (lambda (x) (+ x 1)))
    (fset s3 (lambda (x y) (* x y)))
    (fset s5 (lambda () "constant"))
    ;; s2, s4 are not fboundp
    (unwind-protect
        (let ((callable nil)
              (not-callable nil))
          (mapatoms (lambda (sym)
                      (if (fboundp sym)
                          (setq callable (cons (symbol-name sym) callable))
                        (setq not-callable (cons (symbol-name sym) not-callable))))
                    ob)
          (list (sort callable #'string<)
                (sort not-callable #'string<)
                (= (length callable) 3)
                (= (length not-callable) 2)))
      ;; Cleanup
      (fmakunbound s1)
      (fmakunbound s3)
      (fmakunbound s5))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"neovm--moc-callable-a-3344\" \"neovm--moc-callable-c-3344\" \"neovm--moc-callable-e-3344\") (\"neovm--moc-callable-b-3344\" \"neovm--moc-callable-d-3344\") t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapatoms to compute symbol name statistics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapatoms_name_statistics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute min/max/average name length and most common first character
    // after the common prefix.
    let form = r#"(let ((ob (make-vector 7 0)))
  (dolist (name '("neovm--moc-ns-apple-1123"
                  "neovm--moc-ns-avocado-1123"
                  "neovm--moc-ns-banana-1123"
                  "neovm--moc-ns-blueberry-1123"
                  "neovm--moc-ns-cherry-1123"
                  "neovm--moc-ns-coconut-1123"
                  "neovm--moc-ns-date-1123"))
    (intern name ob))
  (let ((min-len most-positive-fixnum)
        (max-len 0)
        (total-len 0)
        (count 0)
        (char-freq nil))
    (mapatoms (lambda (sym)
                (let* ((name (symbol-name sym))
                       (len (length name))
                       ;; Extract the fruit part (after "neovm--moc-ns-")
                       (fruit-part (substring name 14)))
                  (setq count (1+ count))
                  (setq total-len (+ total-len len))
                  (when (< len min-len) (setq min-len len))
                  (when (> len max-len) (setq max-len len))
                  ;; Count first char of fruit part
                  (let* ((ch (aref fruit-part 0))
                         (existing (assq ch char-freq)))
                    (if existing
                        (setcdr existing (1+ (cdr existing)))
                      (setq char-freq (cons (cons ch 1) char-freq))))))
              ob)
    (list min-len max-len count
          ;; Sort char frequencies by char for determinism
          (sort char-freq (lambda (a b) (< (car a) (car b)))))))"#;
    let expect = expect_test::expect![[r#""OK (23 28 7 ((97 . 2) (98 . 2) (99 . 2) (100 . 1)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapatoms with symbol value transformation pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapatoms_value_transform_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Set initial numeric values, then apply a series of transformations
    // via mapatoms passes: double, add 10, square if < 1000.
    let form = r#"(let ((ob (make-vector 7 0)))
  (let ((syms (mapcar (lambda (pair)
                        (let ((s (intern (car pair) ob)))
                          (set s (cdr pair))
                          s))
                      '(("neovm--moc-vt-a-9981" . 3)
                        ("neovm--moc-vt-b-9981" . 7)
                        ("neovm--moc-vt-c-9981" . 15)
                        ("neovm--moc-vt-d-9981" . 20)
                        ("neovm--moc-vt-e-9981" . 1)))))
    ;; Pass 1: double each value
    (mapatoms (lambda (sym)
                (set sym (* 2 (symbol-value sym))))
              ob)
    ;; Snapshot after doubling
    (let ((after-double nil))
      (mapatoms (lambda (sym)
                  (setq after-double
                        (cons (cons (symbol-name sym) (symbol-value sym))
                              after-double)))
                ob)
      ;; Pass 2: add 10
      (mapatoms (lambda (sym)
                  (set sym (+ 10 (symbol-value sym))))
                ob)
      ;; Pass 3: square if < 1000
      (mapatoms (lambda (sym)
                  (let ((v (symbol-value sym)))
                    (when (< v 1000)
                      (set sym (* v v)))))
                ob)
      ;; Final snapshot
      (let ((final-vals nil))
        (mapatoms (lambda (sym)
                    (setq final-vals
                          (cons (cons (symbol-name sym) (symbol-value sym))
                                final-vals)))
                  ob)
        (list (sort after-double (lambda (a b) (string< (car a) (car b))))
              (sort final-vals (lambda (a b) (string< (car a) (car b)))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"neovm--moc-vt-a-9981\" . 6) (\"neovm--moc-vt-b-9981\" . 14) (\"neovm--moc-vt-c-9981\" . 30) (\"neovm--moc-vt-d-9981\" . 40) (\"neovm--moc-vt-e-9981\" . 2)) ((\"neovm--moc-vt-a-9981\" . 256) (\"neovm--moc-vt-b-9981\" . 576) (\"neovm--moc-vt-c-9981\" . 1600) (\"neovm--moc-vt-d-9981\" . 2500) (\"neovm--moc-vt-e-9981\" . 144)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapatoms with multiple obarrays and cross-obarray symbol movement
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapatoms_cross_obarray_movement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Move symbols from one obarray to another based on a predicate.
    let form = r#"(let ((source (make-vector 11 0))
      (dest (make-vector 11 0)))
  ;; Populate source with labeled symbols
  (dolist (name '("neovm--moc-cam-keep-a-7782"
                  "neovm--moc-cam-move-b-7782"
                  "neovm--moc-cam-keep-c-7782"
                  "neovm--moc-cam-move-d-7782"
                  "neovm--moc-cam-move-e-7782"))
    (let ((s (intern name source)))
      (set s (length name))))
  ;; Collect symbols to move (those with "move" in name)
  (let ((to-move nil))
    (mapatoms (lambda (sym)
                (when (string-match-p "move" (symbol-name sym))
                  (setq to-move (cons sym to-move))))
              source)
    ;; Move them: intern in dest, unintern from source
    (dolist (sym to-move)
      (let ((new-sym (intern (symbol-name sym) dest)))
        (set new-sym (symbol-value sym))
        (unintern sym source)))
    ;; Verify source and dest contents
    (let ((source-names nil) (dest-names nil))
      (mapatoms (lambda (sym)
                  (setq source-names (cons (symbol-name sym) source-names)))
                source)
      (mapatoms (lambda (sym)
                  (setq dest-names (cons (symbol-name sym) dest-names)))
                dest)
      (list (sort source-names #'string<)
            (sort dest-names #'string<)
            (= (length source-names) 2)
            (= (length dest-names) 3)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"neovm--moc-cam-keep-a-7782\" \"neovm--moc-cam-keep-c-7782\") (\"neovm--moc-cam-move-b-7782\" \"neovm--moc-cam-move-d-7782\" \"neovm--moc-cam-move-e-7782\") t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapatoms on empty obarray and single-element obarray
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapatoms_empty_and_singleton() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Edge cases: empty obarray, single symbol, intern then immediately unintern.
    let form = r#"(let ((empty-ob (make-vector 7 0))
      (single-ob (make-vector 7 0))
      (ghost-ob (make-vector 7 0)))
  ;; Test empty obarray
  (let ((empty-count 0))
    (mapatoms (lambda (_sym) (setq empty-count (1+ empty-count)))
              empty-ob)
    ;; Test single-element obarray
    (intern "neovm--moc-es-only-3998" single-ob)
    (let ((single-names nil))
      (mapatoms (lambda (sym)
                  (setq single-names (cons (symbol-name sym) single-names)))
                single-ob)
      ;; Test ghost: intern then unintern
      (intern "neovm--moc-es-ghost-3998" ghost-ob)
      (unintern "neovm--moc-es-ghost-3998" ghost-ob)
      (let ((ghost-count 0))
        (mapatoms (lambda (_sym) (setq ghost-count (1+ ghost-count)))
                  ghost-ob)
        (list empty-count
              single-names
              ghost-count
              ;; Verify invariants
              (= empty-count 0)
              (equal single-names '("neovm--moc-es-only-3998"))
              (= ghost-count 0))))))"#;
    let expect = expect_test::expect![[r#""OK (0 (\"neovm--moc-es-only-3998\") 0 t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapatoms to build a reverse index (value -> symbol names)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapatoms_reverse_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple symbols share the same value. Build a reverse index
    // mapping values to lists of symbol names.
    let form = r#"(let ((ob (make-vector 11 0)))
  ;; Set up: several symbols share values
  (let ((pairs '(("neovm--moc-ri-a-2234" . :red)
                 ("neovm--moc-ri-b-2234" . :blue)
                 ("neovm--moc-ri-c-2234" . :red)
                 ("neovm--moc-ri-d-2234" . :green)
                 ("neovm--moc-ri-e-2234" . :blue)
                 ("neovm--moc-ri-f-2234" . :red)
                 ("neovm--moc-ri-g-2234" . :green))))
    (dolist (pair pairs)
      (let ((s (intern (car pair) ob)))
        (set s (cdr pair))))
    ;; Build reverse index: value -> sorted list of names
    (let ((index nil))
      (mapatoms (lambda (sym)
                  (let* ((val (symbol-value sym))
                         (name (symbol-name sym))
                         (existing (assq val index)))
                    (if existing
                        (setcdr existing (cons name (cdr existing)))
                      (setq index (cons (cons val (list name)) index)))))
                ob)
      ;; Sort each value's name list, and sort index by value name
      (let ((sorted-index
             (mapcar (lambda (entry)
                       (cons (car entry) (sort (cdr entry) #'string<)))
                     index)))
        (sort sorted-index
              (lambda (a b) (string< (symbol-name (car a))
                                     (symbol-name (car b)))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:blue \"neovm--moc-ri-b-2234\" \"neovm--moc-ri-e-2234\") (:green \"neovm--moc-ri-d-2234\" \"neovm--moc-ri-g-2234\") (:red \"neovm--moc-ri-a-2234\" \"neovm--moc-ri-c-2234\" \"neovm--moc-ri-f-2234\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapatoms with side-effectful symbol-value update and chain reaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapatoms_chain_computation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each symbol holds a computation instruction. Process instructions
    // in sorted order, accumulating a running total.
    let form = r#"(let ((ob (make-vector 7 0)))
  ;; Symbols encode operations: (op . operand)
  (let ((ops '(("neovm--moc-ch-01-5512" . (+ . 10))
               ("neovm--moc-ch-02-5512" . (* . 3))
               ("neovm--moc-ch-03-5512" . (+ . 5))
               ("neovm--moc-ch-04-5512" . (* . 2))
               ("neovm--moc-ch-05-5512" . (- . 7)))))
    (dolist (entry ops)
      (let ((s (intern (car entry) ob)))
        (set s (cdr entry))))
    ;; Collect all operation entries, sort by name for deterministic order
    (let ((entries nil))
      (mapatoms (lambda (sym)
                  (setq entries
                        (cons (cons (symbol-name sym) (symbol-value sym))
                              entries)))
                ob)
      (let ((sorted (sort entries (lambda (a b) (string< (car a) (car b))))))
        ;; Apply operations in order starting from 0
        (let ((result 0))
          (dolist (entry sorted)
            (let ((op (cadr entry))
                  (operand (cddr entry)))
              (cond
               ((eq op '+) (setq result (+ result operand)))
               ((eq op '*) (setq result (* result operand)))
               ((eq op '-) (setq result (- result operand))))))
          (list sorted result))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"neovm--moc-ch-01-5512\" + . 10) (\"neovm--moc-ch-02-5512\" * . 3) (\"neovm--moc-ch-03-5512\" + . 5) (\"neovm--moc-ch-04-5512\" * . 2) (\"neovm--moc-ch-05-5512\" - . 7)) 63)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

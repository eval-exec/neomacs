//! Comprehensive oracle parity tests for obarray operations:
//! make-vector custom obarrays, intern/intern-soft with custom obarrays,
//! unintern, mapatoms with custom obarrays, obarray size effects,
//! symbol collision behavior, obarray-make/get/put/remove/size (Emacs 30+),
//! and large-scale intern/lookup patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Custom obarray with various vector sizes and intern behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_comprehensive_custom_sizes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that obarrays of different sizes all work correctly for intern/intern-soft.
    // Smaller obarrays mean more hash collisions; verify correctness regardless.
    let form = r#"(let ((results nil))
  (dolist (size '(1 3 7 17 61 127))
    (let ((ob (make-vector size 0)))
      ;; Intern several symbols
      (intern "neovm--ocp-a-8821" ob)
      (intern "neovm--ocp-b-8821" ob)
      (intern "neovm--ocp-c-8821" ob)
      (intern "neovm--ocp-d-8821" ob)
      (intern "neovm--ocp-e-8821" ob)
      (let ((found-a (not (null (intern-soft "neovm--ocp-a-8821" ob))))
            (found-e (not (null (intern-soft "neovm--ocp-e-8821" ob))))
            (absent  (null (intern-soft "neovm--ocp-missing-8821" ob)))
            ;; Re-intern returns same symbol
            (eq-check (eq (intern "neovm--ocp-a-8821" ob)
                          (intern "neovm--ocp-a-8821" ob))))
        (setq results (cons (list size found-a found-e absent eq-check)
                            results)))))
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 t t t t) (3 t t t t) (7 t t t t) (17 t t t t) (61 t t t t) (127 t t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// unintern from custom obarray
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_comprehensive_unintern_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Unintern a symbol from a custom obarray and verify it's gone,
    // while other symbols remain. Test unintern by name and by symbol object.
    let form = r#"(let ((ob (make-vector 17 0)))
  (let ((s1 (intern "neovm--ocp-un-a-4452" ob))
        (s2 (intern "neovm--ocp-un-b-4452" ob))
        (s3 (intern "neovm--ocp-un-c-4452" ob)))
    ;; All present initially
    (let ((before (list (not (null (intern-soft "neovm--ocp-un-a-4452" ob)))
                        (not (null (intern-soft "neovm--ocp-un-b-4452" ob)))
                        (not (null (intern-soft "neovm--ocp-un-c-4452" ob))))))
      ;; Unintern by name
      (unintern "neovm--ocp-un-b-4452" ob)
      (let ((after-name (list (not (null (intern-soft "neovm--ocp-un-a-4452" ob)))
                              (null (intern-soft "neovm--ocp-un-b-4452" ob))
                              (not (null (intern-soft "neovm--ocp-un-c-4452" ob))))))
        ;; Unintern by symbol object
        (unintern s3 ob)
        (let ((after-sym (list (not (null (intern-soft "neovm--ocp-un-a-4452" ob)))
                               (null (intern-soft "neovm--ocp-un-b-4452" ob))
                               (null (intern-soft "neovm--ocp-un-c-4452" ob)))))
          ;; Re-intern the uninterned name creates a NEW symbol (not eq to old)
          (let ((s2-new (intern "neovm--ocp-un-b-4452" ob)))
            (list before after-name after-sym
                  (symbolp s2-new)
                  ;; New symbol has same name
                  (equal (symbol-name s2-new) "neovm--ocp-un-b-4452"))))))))"#;
    let expect = expect_test::expect![[r#""OK ((t t t) (t t t) (t t t) t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapatoms on custom obarray
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_comprehensive_mapatoms_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use mapatoms to collect all symbol names from a custom obarray,
    // verify the exact set matches what was interned.
    let form = r#"(let ((ob (make-vector 11 0))
      (names '("neovm--ocp-ma-x-7719" "neovm--ocp-ma-y-7719"
               "neovm--ocp-ma-z-7719" "neovm--ocp-ma-w-7719")))
  ;; Intern all names
  (dolist (n names) (intern n ob))
  ;; Collect via mapatoms
  (let ((collected nil))
    (mapatoms (lambda (sym)
                (setq collected (cons (symbol-name sym) collected)))
              ob)
    ;; Sort both for deterministic comparison
    (list (sort (copy-sequence names) #'string<)
          (sort collected #'string<)
          (= (length collected) (length names)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"neovm--ocp-ma-w-7719\" \"neovm--ocp-ma-x-7719\" \"neovm--ocp-ma-y-7719\" \"neovm--ocp-ma-z-7719\") (\"neovm--ocp-ma-w-7719\" \"neovm--ocp-ma-x-7719\" \"neovm--ocp-ma-y-7719\" \"neovm--ocp-ma-z-7719\") t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapatoms with side effects on custom obarray
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_comprehensive_mapatoms_set_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use mapatoms to set symbol values in a custom obarray, then read them back.
    let form = r#"(let ((ob (make-vector 7 0)))
  (intern "neovm--ocp-msv-alpha-3301" ob)
  (intern "neovm--ocp-msv-beta-3301" ob)
  (intern "neovm--ocp-msv-gamma-3301" ob)
  ;; Set each symbol's value to its name length via mapatoms
  (mapatoms (lambda (sym)
              (set sym (length (symbol-name sym))))
            ob)
  ;; Read back values
  (let ((results nil))
    (mapatoms (lambda (sym)
                (setq results (cons (cons (symbol-name sym) (symbol-value sym))
                                    results)))
              ob)
    (sort results (lambda (a b) (string< (car a) (car b))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"neovm--ocp-msv-alpha-3301\" . 25) (\"neovm--ocp-msv-beta-3301\" . 24) (\"neovm--ocp-msv-gamma-3301\" . 25))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// intern-soft with custom obarray: isolation between obarrays
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_comprehensive_isolation_between_obarrays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two custom obarrays are fully isolated: same name interned in
    // different obarrays yields different symbol objects.
    let form = r#"(let ((ob1 (make-vector 13 0))
      (ob2 (make-vector 13 0)))
  (let ((s1 (intern "neovm--ocp-iso-shared-6658" ob1))
        (s2 (intern "neovm--ocp-iso-shared-6658" ob2))
        (s3 (intern "neovm--ocp-iso-only1-6658" ob1)))
    ;; Same name, different obarrays => NOT eq
    (let ((not-eq (not (eq s1 s2))))
      ;; Each found only in its own obarray
      (list not-eq
            (eq (intern-soft "neovm--ocp-iso-shared-6658" ob1) s1)
            (eq (intern-soft "neovm--ocp-iso-shared-6658" ob2) s2)
            ;; s3 only in ob1
            (not (null (intern-soft "neovm--ocp-iso-only1-6658" ob1)))
            (null (intern-soft "neovm--ocp-iso-only1-6658" ob2))
            ;; Set different values
            (progn (set s1 100) (set s2 200)
                   (list (symbol-value s1) (symbol-value s2)))
            ;; symbol-name is the same for both
            (equal (symbol-name s1) (symbol-name s2))))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t (100 200) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// unintern return value and unintern non-existent
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_comprehensive_unintern_return_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // unintern returns t if the symbol was present, nil if not.
    // Test both cases, and test unintern from default obarray.
    let form = r#"(let ((ob (make-vector 11 0)))
  (intern "neovm--ocp-urv-present-2207" ob)
  (let ((ret-present (unintern "neovm--ocp-urv-present-2207" ob))
        (ret-absent  (unintern "neovm--ocp-urv-missing-2207" ob)))
    ;; Unintern again after already removed
    (let ((ret-again (unintern "neovm--ocp-urv-present-2207" ob)))
      (list ret-present ret-absent ret-again))))"#;
    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Large-scale intern and lookup: many symbols in a small obarray
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_comprehensive_large_scale_intern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Intern 50 symbols into a small obarray (size 7) forcing heavy collisions,
    // then verify all are findable.
    let form = r#"(let ((ob (make-vector 7 0))
      (prefix "neovm--ocp-lsi-")
      (count 50)
      (i 0)
      (names nil))
  ;; Generate and intern names
  (while (< i count)
    (let ((name (concat prefix (number-to-string i))))
      (setq names (cons name names))
      (intern name ob))
    (setq i (1+ i)))
  (setq names (nreverse names))
  ;; Verify all are found
  (let ((all-found t)
        (j 0))
    (while (< j count)
      (let ((name (nth j names)))
        (unless (intern-soft name ob)
          (setq all-found nil)))
      (setq j (1+ j)))
    ;; Count via mapatoms
    (let ((atom-count 0))
      (mapatoms (lambda (_) (setq atom-count (1+ atom-count))) ob)
      ;; Verify a few specific ones
      (list all-found
            atom-count
            (symbol-name (intern-soft (concat prefix "0") ob))
            (symbol-name (intern-soft (concat prefix "25") ob))
            (symbol-name (intern-soft (concat prefix "49") ob))
            (null (intern-soft (concat prefix "50") ob))))))"#;
    let expect = expect_test::expect![[
        r#""OK (t 50 \"neovm--ocp-lsi-0\" \"neovm--ocp-lsi-25\" \"neovm--ocp-lsi-49\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Symbol collision behavior: names that hash to same bucket
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_comprehensive_collision_behavior() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // With obarray size 1, ALL symbols hash to the same bucket.
    // Verify intern/lookup/unintern still works correctly in this worst case.
    let form = r#"(let ((ob (make-vector 1 0)))
  (let ((s1 (intern "neovm--ocp-coll-aa-5593" ob))
        (s2 (intern "neovm--ocp-coll-bb-5593" ob))
        (s3 (intern "neovm--ocp-coll-cc-5593" ob))
        (s4 (intern "neovm--ocp-coll-dd-5593" ob)))
    ;; All distinct symbols
    (let ((all-distinct (and (not (eq s1 s2)) (not (eq s2 s3))
                             (not (eq s3 s4)) (not (eq s1 s4)))))
      ;; All findable
      (let ((all-found (and (eq (intern-soft "neovm--ocp-coll-aa-5593" ob) s1)
                            (eq (intern-soft "neovm--ocp-coll-bb-5593" ob) s2)
                            (eq (intern-soft "neovm--ocp-coll-cc-5593" ob) s3)
                            (eq (intern-soft "neovm--ocp-coll-dd-5593" ob) s4))))
        ;; Unintern middle one
        (unintern "neovm--ocp-coll-bb-5593" ob)
        (let ((after-unintern
               (list (not (null (intern-soft "neovm--ocp-coll-aa-5593" ob)))
                     (null (intern-soft "neovm--ocp-coll-bb-5593" ob))
                     (not (null (intern-soft "neovm--ocp-coll-cc-5593" ob)))
                     (not (null (intern-soft "neovm--ocp-coll-dd-5593" ob))))))
          ;; Unintern first one
          (unintern s1 ob)
          ;; Remaining two still findable
          (list all-distinct all-found after-unintern
                (null (intern-soft "neovm--ocp-coll-aa-5593" ob))
                (not (null (intern-soft "neovm--ocp-coll-cc-5593" ob)))
                (not (null (intern-soft "neovm--ocp-coll-dd-5593" ob)))))))))"#;
    let expect = expect_test::expect![[r#""OK (t t (t t t t) t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// obarray-make/get/put/remove/size (Emacs 30+, guarded with fboundp)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_comprehensive_emacs30_obarray_api() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test obarray-make, obarray-get, obarray-put, obarray-remove, obarray-size
    // if available (Emacs 30+). If not available, return a known sentinel.
    let form = r#"(if (fboundp 'obarray-make)
    (let ((ob (obarray-make 16)))
      ;; Put some symbols
      (obarray-put ob "neovm--ocp-e30-alpha-1182")
      (obarray-put ob "neovm--ocp-e30-beta-1182")
      (obarray-put ob "neovm--ocp-e30-gamma-1182")
      (let ((size-after-put (obarray-size ob))
            ;; Get returns the symbol
            (got-alpha (symbolp (obarray-get ob "neovm--ocp-e30-alpha-1182")))
            (got-beta  (symbolp (obarray-get ob "neovm--ocp-e30-beta-1182")))
            ;; Missing returns nil
            (got-miss  (null (obarray-get ob "neovm--ocp-e30-missing-1182"))))
        ;; Remove one
        (obarray-remove ob "neovm--ocp-e30-beta-1182")
        (let ((size-after-remove (obarray-size ob))
              (removed-beta (null (obarray-get ob "neovm--ocp-e30-beta-1182")))
              ;; Others still present
              (still-alpha (symbolp (obarray-get ob "neovm--ocp-e30-alpha-1182")))
              (still-gamma (symbolp (obarray-get ob "neovm--ocp-e30-gamma-1182"))))
          (list size-after-put got-alpha got-beta got-miss
                size-after-remove removed-beta still-alpha still-gamma))))
  ;; Fallback if obarray-make not available
  '(not-available))"#;
    let expect = expect_test::expect![[r#""OK (4 t t t 4 t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapatoms counting on default obarray (subset check)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_comprehensive_mapatoms_default_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // mapatoms on default obarray: verify known built-in symbols are present.
    // We can't compare full counts (may vary), but we can check membership.
    let form = r#"(let ((found-car nil)
      (found-cdr nil)
      (found-cons nil)
      (found-list nil)
      (found-nil nil)
      (total 0))
  (mapatoms (lambda (sym)
              (setq total (1+ total))
              (cond
               ((eq sym 'car)  (setq found-car t))
               ((eq sym 'cdr)  (setq found-cdr t))
               ((eq sym 'cons) (setq found-cons t))
               ((eq sym 'list) (setq found-list t))
               ((eq sym 'nil)  (setq found-nil t)))))
  (list found-car found-cdr found-cons found-list found-nil
        (> total 100)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Symbol properties across custom obarrays
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_comprehensive_symbol_properties_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Set properties on symbols in custom obarrays and verify isolation.
    let form = r#"(let ((ob1 (make-vector 11 0))
      (ob2 (make-vector 11 0)))
  (let ((s1 (intern "neovm--ocp-prop-x-9914" ob1))
        (s2 (intern "neovm--ocp-prop-x-9914" ob2)))
    ;; Set different properties on same-named symbols in different obarrays
    (put s1 'color 'red)
    (put s1 'weight 100)
    (put s2 'color 'blue)
    (put s2 'weight 200)
    (put s2 'extra 'bonus)
    ;; Also set function and value
    (set s1 42)
    (fset s1 (lambda () "from-ob1"))
    (set s2 99)
    (fset s2 (lambda () "from-ob2"))
    (list
     ;; Properties are isolated
     (get s1 'color) (get s2 'color)
     (get s1 'weight) (get s2 'weight)
     (get s1 'extra) (get s2 'extra)
     ;; Values are isolated
     (symbol-value s1) (symbol-value s2)
     ;; Functions are isolated
     (funcall s1) (funcall s2)
     ;; Symbol plists
     (length (symbol-plist s1))
     (length (symbol-plist s2)))))"#;
    let expect = expect_test::expect![[
        r#""OK (red blue 100 200 nil bonus 42 99 \"from-ob1\" \"from-ob2\" 4 6)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Intern with empty string name
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_comprehensive_empty_string_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // It's valid to intern a symbol with an empty string name.
    let form = r#"(let ((ob (make-vector 11 0)))
  (let ((s (intern "" ob)))
    (list
     (symbolp s)
     (equal (symbol-name s) "")
     ;; Re-interning empty string returns same symbol
     (eq (intern "" ob) s)
     ;; intern-soft finds it
     (eq (intern-soft "" ob) s))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Oracle parity tests for obarray and symbol interning with complex patterns:
//! custom obarrays via make-vector, intern/intern-soft with custom obarrays,
//! unintern from custom obarrays, mapatoms on custom obarrays,
//! namespace isolation using separate obarrays, and symbol table statistics.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// make-vector creates an obarray, intern places symbols into it
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_custom_intern_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create a custom obarray with make-vector, intern symbols into it,
    // and verify they are found by intern-soft in that obarray but NOT
    // in the default obarray (using unique names).
    let form = r#"(let ((my-ob (make-vector 17 0)))
  (let ((s1 (intern "neovm--obpat-alpha-3917" my-ob))
        (s2 (intern "neovm--obpat-beta-3917" my-ob))
        (s3 (intern "neovm--obpat-gamma-3917" my-ob)))
    (list
     ;; All are symbols
     (symbolp s1) (symbolp s2) (symbolp s3)
     ;; intern-soft finds them in my-ob
     (eq (intern-soft "neovm--obpat-alpha-3917" my-ob) s1)
     (eq (intern-soft "neovm--obpat-beta-3917" my-ob) s2)
     (eq (intern-soft "neovm--obpat-gamma-3917" my-ob) s3)
     ;; intern-soft does NOT find them in default obarray
     (null (intern-soft "neovm--obpat-alpha-3917"))
     (null (intern-soft "neovm--obpat-beta-3917"))
     (null (intern-soft "neovm--obpat-gamma-3917"))
     ;; Re-interning the same name yields the same symbol (eq)
     (eq (intern "neovm--obpat-alpha-3917" my-ob) s1)
     ;; symbol-name roundtrip
     (symbol-name s1)
     (symbol-name s2)
     (symbol-name s3))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t t t t t t \"neovm--obpat-alpha-3917\" \"neovm--obpat-beta-3917\" \"neovm--obpat-gamma-3917\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// intern-soft with custom obarray: absent vs present
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_custom_intern_soft_absent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // intern-soft on a custom obarray returns nil for names that were never
    // interned there, even if they exist in the default obarray.
    let form = r#"(let ((my-ob (make-vector 11 0)))
  ;; "car" exists in default obarray but NOT in my-ob
  (list
   ;; "car" is interned in default obarray
   (not (null (intern-soft "car")))
   ;; "car" is NOT in our custom obarray
   (null (intern-soft "car" my-ob))
   ;; Intern "car" into custom obarray -- creates a DIFFERENT symbol
   (let ((my-car (intern "car" my-ob)))
     (list
      ;; It's a symbol with name "car"
      (symbolp my-car)
      (equal (symbol-name my-car) "car")
      ;; But it is NOT eq to the default 'car symbol
      (eq my-car 'car)
      ;; Now intern-soft finds it in my-ob
      (eq (intern-soft "car" my-ob) my-car)
      ;; The default obarray still has the original
      (eq (intern-soft "car") 'car)))))"#;
    let expect = expect_test::expect![[r#""OK (t t (t t nil t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// unintern removes a symbol from a custom obarray
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_unintern_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // unintern a symbol from a custom obarray, then verify intern-soft
    // no longer finds it.
    let form = r#"(let ((my-ob (make-vector 13 0)))
  (let ((s1 (intern "neovm--obpat-uni-a-8823" my-ob))
        (s2 (intern "neovm--obpat-uni-b-8823" my-ob))
        (s3 (intern "neovm--obpat-uni-c-8823" my-ob)))
    ;; Before unintern: all found
    (let ((before (list
                   (not (null (intern-soft "neovm--obpat-uni-a-8823" my-ob)))
                   (not (null (intern-soft "neovm--obpat-uni-b-8823" my-ob)))
                   (not (null (intern-soft "neovm--obpat-uni-c-8823" my-ob))))))
      ;; Unintern s2 by name
      (let ((result-unintern (unintern "neovm--obpat-uni-b-8823" my-ob)))
        ;; After unintern: s1 and s3 still found, s2 gone
        (let ((after (list
                      (not (null (intern-soft "neovm--obpat-uni-a-8823" my-ob)))
                      (null (intern-soft "neovm--obpat-uni-b-8823" my-ob))
                      (not (null (intern-soft "neovm--obpat-uni-c-8823" my-ob))))))
          ;; Re-interning the uninterned name creates a new symbol
          (let ((s2-new (intern "neovm--obpat-uni-b-8823" my-ob)))
            (list
             before
             result-unintern
             after
             ;; New symbol is NOT eq to the old one
             (eq s2 s2-new)
             ;; But has the same name
             (equal (symbol-name s2) (symbol-name s2-new)))))))))"#;
    let expect = expect_test::expect![[r#""OK ((t t t) t (t t t) nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapatoms iterates over all symbols in a custom obarray
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_mapatoms_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use mapatoms on a custom obarray to collect all symbol names.
    let form = r#"(let ((my-ob (make-vector 7 0)))
  ;; Intern several symbols
  (intern "neovm--obpat-map-x" my-ob)
  (intern "neovm--obpat-map-y" my-ob)
  (intern "neovm--obpat-map-z" my-ob)
  (intern "neovm--obpat-map-w" my-ob)
  ;; Collect all symbol names via mapatoms
  (let ((names nil))
    (mapatoms (lambda (sym) (setq names (cons (symbol-name sym) names)))
              my-ob)
    ;; Sort for deterministic comparison
    (let ((sorted (sort names #'string<)))
      (list
       ;; Should have exactly 4 symbols
       (length sorted)
       ;; All expected names present
       sorted
       ;; Verify each individually
       (member "neovm--obpat-map-x" sorted)
       (member "neovm--obpat-map-y" sorted)
       (member "neovm--obpat-map-z" sorted)
       (member "neovm--obpat-map-w" sorted)))))"#;
    let expect = expect_test::expect![[
        r#""OK (4 (\"neovm--obpat-map-w\" \"neovm--obpat-map-x\" \"neovm--obpat-map-y\" \"neovm--obpat-map-z\") (\"neovm--obpat-map-x\" \"neovm--obpat-map-y\" \"neovm--obpat-map-z\") (\"neovm--obpat-map-y\" \"neovm--obpat-map-z\") (\"neovm--obpat-map-z\") (\"neovm--obpat-map-w\" \"neovm--obpat-map-x\" \"neovm--obpat-map-y\" \"neovm--obpat-map-z\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Symbol properties are independent per obarray
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_property_isolation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Symbols with the same name in different obarrays have independent plists.
    let form = r#"(let ((ob1 (make-vector 11 0))
      (ob2 (make-vector 11 0)))
  (let ((s1 (intern "neovm--obpat-prop-test" ob1))
        (s2 (intern "neovm--obpat-prop-test" ob2)))
    ;; Set different properties on each
    (put s1 'color 'red)
    (put s1 'size 42)
    (put s2 'color 'blue)
    (put s2 'shape 'circle)
    ;; Set values on each
    (set s1 100)
    (set s2 200)
    (list
     ;; They are NOT eq (different obarrays)
     (eq s1 s2)
     ;; Same name
     (equal (symbol-name s1) (symbol-name s2))
     ;; Different properties
     (get s1 'color)
     (get s2 'color)
     (get s1 'size)
     (get s2 'size)
     (get s1 'shape)
     (get s2 'shape)
     ;; Different values
     (symbol-value s1)
     (symbol-value s2)
     ;; Plist comparison
     (symbol-plist s1)
     (symbol-plist s2))))"#;
    let expect = expect_test::expect![[
        r#""OK (nil t red blue 42 nil nil circle 100 200 (color red size 42) (color blue shape circle))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: implementing namespaces with separate obarrays
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_namespace_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a namespace system where each namespace has its own obarray,
    // with intern, lookup, define, and cross-namespace import operations.
    let form = r#"(progn
  ;; Namespace registry: hash-table mapping ns-name -> obarray
  (fset 'neovm--obns-make-registry
    (lambda ()
      (make-hash-table :test 'equal)))

  ;; Create a new namespace
  (fset 'neovm--obns-create
    (lambda (registry name)
      (let ((ob (make-vector 17 0)))
        (puthash name ob registry)
        ob)))

  ;; Define a value in a namespace
  (fset 'neovm--obns-define
    (lambda (registry ns-name sym-name value)
      (let ((ob (gethash ns-name registry)))
        (when ob
          (let ((sym (intern sym-name ob)))
            (set sym value)
            sym)))))

  ;; Lookup a value in a namespace
  (fset 'neovm--obns-lookup
    (lambda (registry ns-name sym-name)
      (let ((ob (gethash ns-name registry)))
        (when ob
          (let ((sym (intern-soft sym-name ob)))
            (when sym
              (if (boundp sym) (symbol-value sym) 'unbound)))))))

  ;; List all symbols in a namespace
  (fset 'neovm--obns-list-symbols
    (lambda (registry ns-name)
      (let ((ob (gethash ns-name registry))
            (names nil))
        (when ob
          (mapatoms (lambda (s) (push (symbol-name s) names)) ob))
        (sort names #'string<))))

  ;; Import a symbol from one namespace to another
  (fset 'neovm--obns-import
    (lambda (registry from-ns to-ns sym-name)
      (let ((from-ob (gethash from-ns registry))
            (to-ob (gethash to-ns registry)))
        (when (and from-ob to-ob)
          (let ((from-sym (intern-soft sym-name from-ob)))
            (when (and from-sym (boundp from-sym))
              (let ((to-sym (intern sym-name to-ob)))
                (set to-sym (symbol-value from-sym))
                t)))))))

  (unwind-protect
      (let ((reg (funcall 'neovm--obns-make-registry)))
        ;; Create two namespaces
        (funcall 'neovm--obns-create reg "math")
        (funcall 'neovm--obns-create reg "util")
        ;; Define values in "math"
        (funcall 'neovm--obns-define reg "math" "pi" 314)
        (funcall 'neovm--obns-define reg "math" "e" 271)
        (funcall 'neovm--obns-define reg "math" "phi" 161)
        ;; Define values in "util"
        (funcall 'neovm--obns-define reg "util" "version" 1)
        (funcall 'neovm--obns-define reg "util" "debug" nil)
        (list
         ;; Lookup within namespace
         (funcall 'neovm--obns-lookup reg "math" "pi")
         (funcall 'neovm--obns-lookup reg "math" "e")
         (funcall 'neovm--obns-lookup reg "util" "version")
         ;; Cross-namespace: "pi" not in "util"
         (funcall 'neovm--obns-lookup reg "util" "pi")
         ;; List symbols
         (funcall 'neovm--obns-list-symbols reg "math")
         (funcall 'neovm--obns-list-symbols reg "util")
         ;; Import "pi" from "math" to "util"
         (funcall 'neovm--obns-import reg "math" "util" "pi")
         ;; Now "pi" is in "util" too
         (funcall 'neovm--obns-lookup reg "util" "pi")
         ;; Non-existent namespace
         (funcall 'neovm--obns-lookup reg "missing" "pi")
         ;; Non-existent symbol
         (funcall 'neovm--obns-lookup reg "math" "tau")))
    (fmakunbound 'neovm--obns-make-registry)
    (fmakunbound 'neovm--obns-create)
    (fmakunbound 'neovm--obns-define)
    (fmakunbound 'neovm--obns-lookup)
    (fmakunbound 'neovm--obns-list-symbols)
    (fmakunbound 'neovm--obns-import)))"#;
    let expect = expect_test::expect![[
        r#""OK (314 271 1 nil (\"e\" \"phi\" \"pi\") (\"debug\" \"version\") t 314 nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: symbol table statistics across obarrays
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_statistics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute statistics about a custom obarray: count symbols, count
    // bound symbols, count symbols with functions, average name length,
    // and find the longest/shortest name.
    let form = r#"(let ((my-ob (make-vector 19 0)))
  ;; Populate with various symbols, some bound, some with functions
  (let ((s1 (intern "a" my-ob))
        (s2 (intern "bb" my-ob))
        (s3 (intern "ccc" my-ob))
        (s4 (intern "dddd" my-ob))
        (s5 (intern "eeeee" my-ob))
        (s6 (intern "ffffff" my-ob))
        (s7 (intern "g" my-ob)))
    ;; Bind some values
    (set s1 10)
    (set s3 30)
    (set s5 50)
    (set s7 70)
    ;; Set some functions
    (fset s2 (lambda (x) x))
    (fset s4 (lambda (x y) (+ x y)))
    (fset s6 (lambda () nil))
    ;; Collect statistics via mapatoms
    (let ((total 0)
          (bound-count 0)
          (fboundp-count 0)
          (name-lengths nil)
          (longest-name "")
          (shortest-name nil))
      (mapatoms
       (lambda (sym)
         (setq total (1+ total))
         (when (boundp sym) (setq bound-count (1+ bound-count)))
         (when (fboundp sym) (setq fboundp-count (1+ fboundp-count)))
         (let ((name (symbol-name sym)))
           (push (length name) name-lengths)
           (when (> (length name) (length longest-name))
             (setq longest-name name))
           (when (or (null shortest-name) (< (length name) (length shortest-name)))
             (setq shortest-name name))))
       my-ob)
      (let ((total-len (apply #'+ name-lengths)))
        (list
         ;; Total symbols
         total
         ;; Bound symbols count
         bound-count
         ;; fboundp symbols count
         fboundp-count
         ;; Longest and shortest name
         longest-name
         shortest-name
         ;; Total name length (for avg calculation)
         total-len
         ;; Sorted name lengths
         (sort name-lengths #'<))))))"#;
    let expect = expect_test::expect![[r#""OK (7 4 3 \"ffffff\" \"g\" 22 (1 1 2 3 4 5 6))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Different obarray sizes and hash distribution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_obarray_different_sizes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that obarrays of various sizes all work correctly for intern/lookup.
    let form = r#"(let ((sizes '(1 3 7 13 31))
      (results nil))
  (dolist (sz sizes)
    (let ((ob (make-vector sz 0)))
      ;; Intern 10 symbols into this obarray
      (let ((names '("neovm--obs-aa" "neovm--obs-bb" "neovm--obs-cc"
                     "neovm--obs-dd" "neovm--obs-ee" "neovm--obs-ff"
                     "neovm--obs-gg" "neovm--obs-hh" "neovm--obs-ii"
                     "neovm--obs-jj")))
        ;; Intern all
        (dolist (n names) (intern n ob))
        ;; Count how many we can find
        (let ((found 0))
          (dolist (n names)
            (when (intern-soft n ob)
              (setq found (1+ found))))
          ;; Unintern the first 3
          (unintern "neovm--obs-aa" ob)
          (unintern "neovm--obs-bb" ob)
          (unintern "neovm--obs-cc" ob)
          (let ((found-after 0))
            (dolist (n names)
              (when (intern-soft n ob)
                (setq found-after (1+ found-after))))
            (push (list sz found found-after) results))))))
  (nreverse results))"#;
    let expect = expect_test::expect![[r#""OK ((1 10 7) (3 10 7) (7 10 7) (13 10 7) (31 10 7))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

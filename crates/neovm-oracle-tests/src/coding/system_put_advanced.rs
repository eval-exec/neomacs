//! Advanced oracle parity tests for coding-system-put and coding-system-plist.
//!
//! Tests property modification, retrieval, multiple property keys,
//! property deletion, type preservation, and a complex coding system
//! property inspection report builder.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// coding-system-put modifying a property and reading it back
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_put_basic_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Set a custom property on utf-8, read it back, then clean up
    let form = r#"(progn
  (coding-system-put 'utf-8 :neovm-cspa-test-key 'neovm-cspa-test-val)
  (unwind-protect
      (let* ((plist (coding-system-plist 'utf-8))
             (got (plist-get plist :neovm-cspa-test-key)))
        (list
         (eq got 'neovm-cspa-test-val)
         (coding-system-p 'utf-8)
         ;; Verify coding system still works after modification
         (string= "hello"
                  (decode-coding-string
                   (encode-coding-string "hello" 'utf-8)
                   'utf-8))))
    ;; Cleanup: remove the property by setting nil
    (coding-system-put 'utf-8 :neovm-cspa-test-key nil)))"#;
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// coding-system-plist verifying changes with various value types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_put_value_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that coding-system-put preserves different value types
    let form = r#"(unwind-protect
    (progn
      ;; Integer value
      (coding-system-put 'utf-8 :neovm-cspa-int-prop 42)
      ;; String value
      (coding-system-put 'utf-8 :neovm-cspa-str-prop "test-string")
      ;; List value
      (coding-system-put 'utf-8 :neovm-cspa-list-prop '(a b c))
      ;; Cons cell value
      (coding-system-put 'utf-8 :neovm-cspa-cons-prop '(1 . 2))
      ;; nil value (effectively removes)
      (coding-system-put 'utf-8 :neovm-cspa-nil-prop nil)
      (let ((plist (coding-system-plist 'utf-8)))
        (list
         (plist-get plist :neovm-cspa-int-prop)
         (plist-get plist :neovm-cspa-str-prop)
         (plist-get plist :neovm-cspa-list-prop)
         (plist-get plist :neovm-cspa-cons-prop)
         (plist-get plist :neovm-cspa-nil-prop)
         ;; Type checks
         (integerp (plist-get plist :neovm-cspa-int-prop))
         (stringp (plist-get plist :neovm-cspa-str-prop))
         (listp (plist-get plist :neovm-cspa-list-prop)))))
  ;; Cleanup all properties
  (coding-system-put 'utf-8 :neovm-cspa-int-prop nil)
  (coding-system-put 'utf-8 :neovm-cspa-str-prop nil)
  (coding-system-put 'utf-8 :neovm-cspa-list-prop nil)
  (coding-system-put 'utf-8 :neovm-cspa-cons-prop nil)
  (coding-system-put 'utf-8 :neovm-cspa-nil-prop nil))"#;
    let expect = expect_test::expect![[r#""OK (42 \"test-string\" (a b c) (1 . 2) nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// coding-system-put with various property keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_put_various_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test multiple property keys on different coding systems
    let form = r#"(unwind-protect
    (progn
      ;; Set properties on different coding systems
      (coding-system-put 'utf-8 :neovm-cspa-key-a 'value-a)
      (coding-system-put 'latin-1 :neovm-cspa-key-b 'value-b)
      (coding-system-put 'raw-text :neovm-cspa-key-c 'value-c)
      ;; Set multiple properties on one coding system
      (coding-system-put 'utf-8 :neovm-cspa-key-d 'value-d)
      (coding-system-put 'utf-8 :neovm-cspa-key-e 'value-e)
      (let ((utf8-plist (coding-system-plist 'utf-8))
            (latin1-plist (coding-system-plist 'latin-1))
            (raw-plist (coding-system-plist 'raw-text)))
        (list
         ;; Each coding system has its own property
         (plist-get utf8-plist :neovm-cspa-key-a)
         (plist-get latin1-plist :neovm-cspa-key-b)
         (plist-get raw-plist :neovm-cspa-key-c)
         ;; utf-8 has multiple custom properties
         (plist-get utf8-plist :neovm-cspa-key-d)
         (plist-get utf8-plist :neovm-cspa-key-e)
         ;; Cross-check: latin-1 does NOT have utf-8's property
         (plist-get latin1-plist :neovm-cspa-key-a)
         ;; Overwrite existing property
         (progn
           (coding-system-put 'utf-8 :neovm-cspa-key-a 'overwritten)
           (plist-get (coding-system-plist 'utf-8) :neovm-cspa-key-a)))))
  ;; Cleanup
  (coding-system-put 'utf-8 :neovm-cspa-key-a nil)
  (coding-system-put 'utf-8 :neovm-cspa-key-d nil)
  (coding-system-put 'utf-8 :neovm-cspa-key-e nil)
  (coding-system-put 'latin-1 :neovm-cspa-key-b nil)
  (coding-system-put 'raw-text :neovm-cspa-key-c nil))"#;
    let expect =
        expect_test::expect![[r#""OK (value-a value-b value-c value-d value-e nil overwritten)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// coding-system-plist intrinsic properties inspection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_put_inspect_intrinsic_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Read well-known intrinsic properties from various coding systems
    let form = r#"(let ((systems '(utf-8 latin-1 raw-text no-conversion)))
  (mapcar
   (lambda (cs)
     (let ((plist (coding-system-plist cs)))
       (list
        cs
        ;; :name should be the coding system symbol
        (plist-get plist :name)
        ;; :mnemonic should be an integer (character code)
        (integerp (plist-get plist :mnemonic))
        ;; Check if :coding-type is present
        (not (null (plist-get plist :coding-type)))
        ;; :charset-list should be a list or t
        (let ((cl (plist-get plist :charset-list)))
          (or (listp cl) (eq cl t)))
        ;; plist itself should be a list
        (listp plist)
        ;; plist length should be even (key-value pairs)
        (= 0 (mod (length plist) 2)))))
   systems))"#;
    let expect = expect_test::expect![[
        r#""OK ((utf-8 utf-8 t t t t t) (latin-1 iso-latin-1 t t t t t) (raw-text raw-text t t t t t) (no-conversion no-conversion t t t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// coding-system-put property overwrite and deletion semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_put_overwrite_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test overwrite-then-read and delete-then-read semantics
    let form = r#"(unwind-protect
    (let ((results nil))
      ;; Step 1: Set initial value
      (coding-system-put 'utf-8 :neovm-cspa-ow-test 'initial)
      (setq results
            (cons (plist-get (coding-system-plist 'utf-8) :neovm-cspa-ow-test)
                  results))
      ;; Step 2: Overwrite with different value
      (coding-system-put 'utf-8 :neovm-cspa-ow-test 'second)
      (setq results
            (cons (plist-get (coding-system-plist 'utf-8) :neovm-cspa-ow-test)
                  results))
      ;; Step 3: Overwrite with integer
      (coding-system-put 'utf-8 :neovm-cspa-ow-test 999)
      (setq results
            (cons (plist-get (coding-system-plist 'utf-8) :neovm-cspa-ow-test)
                  results))
      ;; Step 4: Overwrite with a nested structure
      (coding-system-put 'utf-8 :neovm-cspa-ow-test '((nested . structure) (with . alist)))
      (setq results
            (cons (plist-get (coding-system-plist 'utf-8) :neovm-cspa-ow-test)
                  results))
      ;; Step 5: Delete by setting nil
      (coding-system-put 'utf-8 :neovm-cspa-ow-test nil)
      (setq results
            (cons (plist-get (coding-system-plist 'utf-8) :neovm-cspa-ow-test)
                  results))
      (nreverse results))
  (coding-system-put 'utf-8 :neovm-cspa-ow-test nil))"#;
    let expect = expect_test::expect![[
        r#""OK (initial second 999 ((nested . structure) (with . alist)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: coding system property inspection report
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_put_inspection_report() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a comprehensive report of coding system properties, including
    // custom annotations, category verification, and cross-system comparison
    let form = r#"(progn
  (fset 'neovm--cspa-build-report
    (lambda (cs-list)
      "Build a property report for each coding system in CS-LIST."
      (let ((report nil))
        (dolist (cs cs-list)
          (let* ((plist (coding-system-plist cs))
                 (name (plist-get plist :name))
                 (mnemonic (plist-get plist :mnemonic))
                 (coding-type (plist-get plist :coding-type))
                 (charset-list (plist-get plist :charset-list))
                 (base (coding-system-base cs))
                 (eol-type (coding-system-eol-type cs))
                 ;; Count the number of properties
                 (prop-count (/ (length plist) 2))
                 ;; Extract all property keys
                 (keys nil)
                 (rest plist))
            (while rest
              (setq keys (cons (car rest) keys))
              (setq rest (cddr rest)))
            (setq keys (nreverse keys))
            (setq report
                  (cons (list
                         :cs cs
                         :name name
                         :mnemonic-char-p (characterp mnemonic)
                         :coding-type coding-type
                         :charset-count (if (listp charset-list)
                                            (length charset-list)
                                          (if (eq charset-list t) -1 0))
                         :base base
                         :eol-type-type (cond
                                         ((vectorp eol-type) 'vector)
                                         ((integerp eol-type) 'integer)
                                         (t 'other))
                         :prop-count prop-count
                         :has-name-key (if (memq :name keys) t nil)
                         :same-base-p (eq base cs))
                        report))))
        (nreverse report))))

  (unwind-protect
      (let* ((systems '(utf-8 utf-8-unix utf-8-dos latin-1
                         raw-text no-conversion))
             (report (funcall 'neovm--cspa-build-report systems))
             ;; Add custom annotations to utf-8 and verify they appear
             (_ (coding-system-put 'utf-8 :neovm-cspa-annotation "test-note"))
             (annotated-plist (coding-system-plist 'utf-8))
             (annotation (plist-get annotated-plist :neovm-cspa-annotation))
             ;; Verify structural invariants across the report
             (all-have-name (let ((ok t))
                              (dolist (entry report)
                                (unless (plist-get entry :has-name-key)
                                  (setq ok nil)))
                              ok))
             (base-systems (let ((bases nil))
                             (dolist (entry report)
                               (when (plist-get entry :same-base-p)
                                 (setq bases (cons (plist-get entry :cs) bases))))
                             (nreverse bases))))
        (list
         :report-length (length report)
         :all-have-name all-have-name
         :base-systems base-systems
         :annotation annotation
         :utf8-base (coding-system-base 'utf-8-unix)
         :latin1-base (coding-system-base 'latin-1)))
    (coding-system-put 'utf-8 :neovm-cspa-annotation nil)
    (fmakunbound 'neovm--cspa-build-report)))"#;
    let expect = expect_test::expect![[
        r#""OK (:report-length 6 :all-have-name t :base-systems (utf-8 raw-text no-conversion) :annotation \"test-note\" :utf8-base utf-8 :latin1-base iso-latin-1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: coding-system-put with hash-table-backed property registry
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_put_hash_registry() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a hash table registry of coding system properties,
    // then query it for cross-referencing
    let form = r#"(progn
  (defvar neovm--cspa-registry nil)

  (fset 'neovm--cspa-register
    (lambda (cs-list)
      "Register coding system properties into a hash table."
      (let ((reg (make-hash-table :test 'eq)))
        (dolist (cs cs-list)
          (let* ((plist (coding-system-plist cs))
                 (entry (list
                         :coding-type (plist-get plist :coding-type)
                         :mnemonic (plist-get plist :mnemonic)
                         :base (coding-system-base cs)
                         :eol (coding-system-eol-type cs))))
            (puthash cs entry reg)))
        reg)))

  (fset 'neovm--cspa-query-by-type
    (lambda (reg target-type)
      "Find all coding systems with a given :coding-type."
      (let ((result nil))
        (maphash (lambda (k v)
                   (when (eq (plist-get v :coding-type) target-type)
                     (setq result (cons k result))))
                 reg)
        (sort result (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  (unwind-protect
      (let* ((systems '(utf-8 utf-8-unix latin-1 raw-text no-conversion))
             (reg (funcall 'neovm--cspa-register systems))
             ;; Query by type
             (utf-entries (funcall 'neovm--cspa-query-by-type reg 'utf-8))
             (raw-entries (funcall 'neovm--cspa-query-by-type reg 'raw-text))
             ;; Cross-reference: for each entry, check if base is also registered
             (cross-ref
              (let ((refs nil))
                (maphash (lambda (k v)
                           (let ((base (plist-get v :base)))
                             (setq refs (cons (list k base (not (null (gethash base reg))))
                                              refs))))
                         reg)
                (sort refs (lambda (a b) (string< (symbol-name (car a))
                                                   (symbol-name (car b))))))))
        (list
         :total (hash-table-count reg)
         :utf-systems utf-entries
         :raw-systems raw-entries
         :cross-ref cross-ref))
    (fmakunbound 'neovm--cspa-register)
    (fmakunbound 'neovm--cspa-query-by-type)
    (makunbound 'neovm--cspa-registry)))"#;
    let expect = expect_test::expect![[
        r#""OK (:total 5 :utf-systems (utf-8 utf-8-unix) :raw-systems (no-conversion raw-text) :cross-ref ((latin-1 iso-latin-1 nil) (no-conversion no-conversion t) (raw-text raw-text t) (utf-8 utf-8 t) (utf-8-unix utf-8 t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

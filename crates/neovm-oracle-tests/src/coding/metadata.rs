//! Oracle parity tests for coding-system metadata and query primitives.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// coding-system-p for various coding systems
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_p_standard_systems() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test coding-system-p for utf-8, latin-1, raw-text, undecided, and
    // their EOL variants, plus a non-existent system.
    let form = r#"(list
  (coding-system-p 'utf-8)
  (coding-system-p 'utf-8-unix)
  (coding-system-p 'utf-8-dos)
  (coding-system-p 'utf-8-mac)
  (coding-system-p 'latin-1)
  (coding-system-p 'raw-text)
  (coding-system-p 'undecided)
  (coding-system-p 'no-such-coding-system-xyz)
  (coding-system-p nil)
  (coding-system-p 42))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// coding-system-base extracting base from variants
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_base_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extracting the base coding system from EOL variants.
    // utf-8-unix -> utf-8, latin-1-dos -> iso-latin-1 (or latin-1), etc.
    let form = r#"(list
  (coding-system-base 'utf-8)
  (coding-system-base 'utf-8-unix)
  (coding-system-base 'utf-8-dos)
  (coding-system-base 'utf-8-mac)
  (coding-system-base 'latin-1)
  (coding-system-base 'raw-text)
  (coding-system-base 'raw-text-unix)
  (coding-system-base 'undecided)
  (coding-system-base 'undecided-unix))"#;
    let expect = expect_test::expect![[
        r#""OK (utf-8 utf-8 utf-8 utf-8 iso-latin-1 raw-text raw-text undecided undecided)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// coding-system-eol-type (unix/dos/mac variants)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_eol_type_all_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // EOL type: 0=unix, 1=dos, 2=mac, or a vector of 3 sub-systems for undecided
    let form = r#"(list
  (coding-system-eol-type 'utf-8-unix)
  (coding-system-eol-type 'utf-8-dos)
  (coding-system-eol-type 'utf-8-mac)
  (let ((eol (coding-system-eol-type 'utf-8)))
    (if (vectorp eol)
        (list 'vector (length eol))
      eol))
  (coding-system-eol-type 'raw-text-unix)
  (let ((eol (coding-system-eol-type 'undecided)))
    (if (vectorp eol)
        (list 'vector (length eol))
      eol)))"#;
    let expect = expect_test::expect![[r#""OK (0 1 2 (vector 3) 0 (vector 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// coding-system-aliases listing aliases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_aliases_membership() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that aliases of utf-8 include known variants,
    // and that the base name is always in the alias list.
    let form = r#"(let ((aliases (coding-system-aliases 'utf-8)))
  (list
    (consp aliases)
    (if (memq 'utf-8 aliases) t nil)
    (if (memq 'utf-8-unix aliases) t nil)
    ;; raw-text aliases should contain raw-text itself
    (let ((rt-aliases (coding-system-aliases 'raw-text)))
      (if (memq 'raw-text rt-aliases) t nil))
    ;; latin-1 aliases
    (let ((l1-aliases (coding-system-aliases 'latin-1)))
      (consp l1-aliases))))"#;
    let expect = expect_test::expect![[r#""OK (t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// coding-system-plist property inspection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_plist_detailed_inspection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Inspect several properties of coding system plists and verify
    // structural consistency across multiple systems.
    let form = r#"(let ((utf8-pl (coding-system-plist 'utf-8))
       (raw-pl (coding-system-plist 'raw-text))
       (latin-pl (coding-system-plist 'latin-1)))
  (list
    ;; :name property should match the coding system
    (plist-get utf8-pl :name)
    (plist-get raw-pl :name)
    ;; :mnemonic should be an integer (character)
    (integerp (plist-get utf8-pl :mnemonic))
    (integerp (plist-get raw-pl :mnemonic))
    ;; :coding-type property
    (plist-get utf8-pl :coding-type)
    (plist-get raw-pl :coding-type)
    ;; Verify plist is a proper list with even length
    (= 0 (% (length utf8-pl) 2))
    (= 0 (% (length raw-pl) 2))))"#;
    let expect = expect_test::expect![[r#""OK (utf-8 raw-text t t utf-8 raw-text t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// check-coding-system with valid and error cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_check_coding_system_valid_and_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // check-coding-system returns the coding system for valid ones,
    // signals coding-system-error for invalid.
    let form = r#"(list
  (check-coding-system 'utf-8)
  (check-coding-system 'latin-1)
  (check-coding-system 'raw-text)
  (check-coding-system 'undecided)
  (check-coding-system 'utf-8-unix)
  (check-coding-system nil))"#;
    let expect =
        expect_test::expect![[r#""OK (utf-8 latin-1 raw-text undecided utf-8-unix nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_check_coding_system_error_on_invalid() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (coding-system-error totally-bogus-coding-xyz)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(check-coding-system 'totally-bogus-coding-xyz)",
        expect,
    );
    assert_err_kind(&oracle, &neovm, "coding-system-error");
}

// ---------------------------------------------------------------------------
// coding-system-priority-list inspection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_priority_list_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Priority list should be non-empty, all entries should be valid coding systems,
    // and we can verify a few structural properties.
    let form = r#"(let ((plist (coding-system-priority-list)))
  (list
    (consp plist)
    ;; All entries are valid coding systems
    (let ((all-valid t))
      (dolist (cs (if (> (length plist) 10)
                      (let ((result nil) (i 0))
                        (while (and (< i 10) plist)
                          (setq result (cons (car plist) result)
                                plist (cdr plist)
                                i (1+ i)))
                        (nreverse result))
                    plist))
        (unless (coding-system-p cs)
          (setq all-valid nil)))
      all-valid)
    ;; Length is positive
    (> (length (coding-system-priority-list)) 0)))"#;
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: round-trip metadata consistency check
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_metadata_roundtrip_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // For each of several coding systems, verify that:
    // 1. coding-system-p returns t
    // 2. coding-system-base of the base returns itself
    // 3. The base appears in its own aliases list
    // 4. The plist :name matches the base
    let form = r#"(let ((systems '(utf-8 latin-1 raw-text undecided))
       (results nil))
  (dolist (cs systems)
    (let* ((base (coding-system-base cs))
           (aliases (coding-system-aliases cs))
           (pl (coding-system-plist cs)))
      (setq results
            (cons (list
                    cs
                    (coding-system-p cs)
                    (eq (coding-system-base base) base)
                    (if (memq base aliases) t nil)
                    (eq (plist-get pl :name) base))
                  results))))
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((utf-8 t t t t) (latin-1 t t t t) (raw-text t t t t) (undecided t t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: build a coding system classification table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_metadata_classification_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Classify coding systems by their :coding-type and :charset-list properties.
    // Build an alist mapping coding-type -> count.
    let form = r#"(let ((systems '(utf-8 utf-8-unix utf-8-dos utf-8-mac
                          latin-1 raw-text raw-text-unix
                          undecided undecided-unix))
       (type-counts nil))
  (dolist (cs systems)
    (let* ((pl (coding-system-plist cs))
           (ctype (plist-get pl :coding-type))
           (entry (assq ctype type-counts)))
      (if entry
          (setcdr entry (1+ (cdr entry)))
        (setq type-counts (cons (cons ctype 1) type-counts)))))
  ;; Sort by count descending for deterministic output
  (sort type-counts (lambda (a b) (> (cdr a) (cdr b)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((utf-8 . 4) (undecided . 2) (raw-text . 2) (charset . 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

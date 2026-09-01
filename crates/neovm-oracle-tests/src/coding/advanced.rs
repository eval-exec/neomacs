//! Advanced oracle parity tests for coding system primitives:
//! coding-system-p, check-coding-system, base/eol-type, aliases,
//! plist/put, priority-list, encode/decode roundtrips, and
//! cross-coding-system comparisons.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// coding-system-p for various coding systems
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_advanced_system_p_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (coding-system-p 'utf-8)
  (coding-system-p 'utf-8-unix)
  (coding-system-p 'utf-8-dos)
  (coding-system-p 'utf-8-mac)
  (coding-system-p 'latin-1)
  (coding-system-p 'iso-8859-1)
  (coding-system-p 'raw-text)
  (coding-system-p 'undecided)
  (coding-system-p 'no-conversion)
  (coding-system-p 'nonexistent-coding-system-xyz)
  (coding-system-p nil)
  (coding-system-p 42)
  (coding-system-p "utf-8"))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t nil t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// check-coding-system success and error cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_advanced_check_coding_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Successful checks return the coding system symbol
    let form = r#"(list
  (check-coding-system 'utf-8)
  (check-coding-system 'utf-8-unix)
  (check-coding-system 'latin-1)
  (check-coding-system 'raw-text)
  (check-coding-system 'no-conversion)
  (check-coding-system nil))"#;
    let expect =
        expect_test::expect![[r#""OK (utf-8 utf-8-unix latin-1 raw-text no-conversion nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_coding_advanced_check_coding_system_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (coding-system-error nonexistent-coding-xyz)""#]];
    let (oracle_bad, neovm_bad) = crate::common::eval_oracle_and_neovm_expect(
        "(check-coding-system 'nonexistent-coding-xyz)",
        expect,
    );
    assert_err_kind(&oracle_bad, &neovm_bad, "coding-system-error");

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbolp 42)""#]];
    let (oracle_type, neovm_type) =
        crate::common::eval_oracle_and_neovm_expect("(check-coding-system 42)", expect);
    assert_err_kind(&oracle_type, &neovm_type, "wrong-type-argument");
}

// ---------------------------------------------------------------------------
// coding-system-base and coding-system-eol-type
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_advanced_base_and_eol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Base extraction
  (coding-system-base 'utf-8-unix)
  (coding-system-base 'utf-8-dos)
  (coding-system-base 'utf-8-mac)
  (coding-system-base 'utf-8)
  (coding-system-base 'latin-1-unix)
  ;; EOL type: 0=unix, 1=dos, 2=mac, or vector for auto-detect
  (coding-system-eol-type 'utf-8-unix)
  (coding-system-eol-type 'utf-8-dos)
  (coding-system-eol-type 'utf-8-mac)
  ;; Base coding system returns a vector of eol variants
  (vectorp (coding-system-eol-type 'utf-8)))"#;
    let expect = expect_test::expect![[r#""OK (utf-8 utf-8 utf-8 utf-8 iso-latin-1 0 1 2 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// coding-system-aliases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_advanced_aliases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Aliases should be a list containing the coding system itself
    let form = r#"(let ((aliases (coding-system-aliases 'utf-8)))
  (list
   (listp aliases)
   ;; utf-8 should be in its own alias list
   (if (memq 'utf-8 aliases) t nil)
   ;; All aliases should be valid coding systems
   (let ((all-valid t))
     (dolist (a aliases)
       (unless (coding-system-p a)
         (setq all-valid nil)))
     all-valid)
   ;; latin-1 aliases
   (let ((l1-aliases (coding-system-aliases 'latin-1)))
     (if (memq 'latin-1 l1-aliases) t nil))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// coding-system-plist / coding-system-put roundtrip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_advanced_plist_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Read plist properties and modify with put
    let form = r#"(let ((plist (coding-system-plist 'utf-8)))
  (let ((name (plist-get plist :name))
        (has-mnemonic (integerp (plist-get plist :mnemonic)))
        ;; Put a custom property and read it back
        (_ (coding-system-put 'utf-8 :neovm-test-prop 'test-value))
        (readback (plist-get (coding-system-plist 'utf-8) :neovm-test-prop)))
    ;; Clean up the custom property
    (coding-system-put 'utf-8 :neovm-test-prop nil)
    (list name has-mnemonic readback)))"#;
    let expect = expect_test::expect![[r#""OK (utf-8 t test-value)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// coding-system-priority-list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_advanced_priority_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((plist (coding-system-priority-list)))
  (list
   ;; Should be a non-empty list
   (consp plist)
   ;; All entries should be valid coding systems
   (let ((all-valid t))
     (dolist (cs plist)
       (unless (coding-system-p cs)
         (setq all-valid nil)))
     all-valid)
   ;; Should contain at least utf-8
   (let ((has-utf8 nil))
     (dolist (cs plist)
       (when (eq (coding-system-base cs) 'utf-8)
         (setq has-utf8 t)))
     has-utf8)
   ;; Length should be positive
   (> (length plist) 0)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// encode-coding-string / decode-coding-string roundtrips
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_advanced_encode_decode_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; ASCII roundtrip with utf-8
  (let* ((orig "hello world")
         (encoded (encode-coding-string orig 'utf-8))
         (decoded (decode-coding-string encoded 'utf-8)))
    (string= orig decoded))
  ;; UTF-8 with non-ASCII characters
  (let* ((orig "cafe\u0301")
         (encoded (encode-coding-string orig 'utf-8))
         (decoded (decode-coding-string encoded 'utf-8)))
    (string= orig decoded))
  ;; Empty string roundtrip
  (let* ((orig "")
         (encoded (encode-coding-string orig 'utf-8))
         (decoded (decode-coding-string encoded 'utf-8)))
    (string= orig decoded))
  ;; raw-text preserves bytes
  (let* ((orig "test\n")
         (encoded (encode-coding-string orig 'raw-text))
         (decoded (decode-coding-string encoded 'raw-text)))
    (string= orig decoded))
  ;; no-conversion
  (let* ((orig "binary data")
         (encoded (encode-coding-string orig 'no-conversion))
         (decoded (decode-coding-string encoded 'no-conversion)))
    (string= orig decoded))
  ;; Encoding produces same length for pure ASCII in utf-8
  (= (length (encode-coding-string "abc" 'utf-8)) 3))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Comprehensive oracle parity tests for coding system operations:
//! coding-system-p, check-coding-system, coding-system-base,
//! coding-system-eol-type, coding-system-aliases, coding-system-plist,
//! coding-system-put, coding-system-priority-list,
//! encode-coding-string/decode-coding-string roundtrips,
//! UTF-8, Latin-1, ASCII, ISO-8859-* systems.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// coding-system-p for various coding systems
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_comprehensive_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Valid coding systems
  (coding-system-p 'utf-8)
  (coding-system-p 'utf-8-unix)
  (coding-system-p 'utf-8-dos)
  (coding-system-p 'utf-8-mac)
  (coding-system-p 'latin-1)
  (coding-system-p 'latin-1-unix)
  (coding-system-p 'iso-8859-1)
  (coding-system-p 'ascii)
  (coding-system-p 'raw-text)
  (coding-system-p 'no-conversion)
  (coding-system-p 'emacs-internal)
  (coding-system-p 'undecided)
  ;; Invalid coding systems
  (coding-system-p 'neovm-nonexistent-coding)
  (coding-system-p nil)
  (coding-system-p t)
  (coding-system-p 42)
  (coding-system-p "utf-8"))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t nil t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// check-coding-system error behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_comprehensive_check_valid() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; check-coding-system returns the coding system if valid
  (check-coding-system 'utf-8)
  (check-coding-system 'utf-8-unix)
  (check-coding-system 'latin-1)
  (check-coding-system 'raw-text)
  (check-coding-system 'no-conversion)
  (check-coding-system 'undecided)
  ;; nil means no coding system, which is valid (returns nil)
  (check-coding-system nil))"#;
    let expect = expect_test::expect![[
        r#""OK (utf-8 utf-8-unix latin-1 raw-text no-conversion undecided nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_coding_system_comprehensive_check_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; check-coding-system signals error for invalid inputs
  (condition-case err
      (check-coding-system 'neovm-bogus-coding-987)
    (coding-system-error (list 'coding-system-error (cadr err)))
    (error (list 'error)))
  ;; Non-symbol non-nil triggers wrong-type-argument
  (condition-case err
      (check-coding-system 42)
    (wrong-type-argument (list 'wrong-type-argument))
    (error (list 'other-error)))
  (condition-case err
      (check-coding-system "utf-8")
    (wrong-type-argument (list 'wrong-type-argument))
    (error (list 'other-error))))"#;
    let expect = expect_test::expect![[
        r#""OK ((coding-system-error neovm-bogus-coding-987) (wrong-type-argument) (wrong-type-argument))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// coding-system-base extracting base name
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_comprehensive_base() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Unix/DOS/Mac variants all have the same base
  (coding-system-base 'utf-8)
  (coding-system-base 'utf-8-unix)
  (coding-system-base 'utf-8-dos)
  (coding-system-base 'utf-8-mac)
  ;; All should be equal
  (eq (coding-system-base 'utf-8-unix) (coding-system-base 'utf-8-dos))
  (eq (coding-system-base 'utf-8-unix) (coding-system-base 'utf-8-mac))
  ;; Latin-1 variants
  (coding-system-base 'latin-1)
  (coding-system-base 'latin-1-unix)
  (coding-system-base 'latin-1-dos)
  ;; Raw text
  (coding-system-base 'raw-text)
  (coding-system-base 'raw-text-unix)
  ;; No conversion
  (coding-system-base 'no-conversion)
  ;; undecided
  (coding-system-base 'undecided)
  (coding-system-base 'undecided-unix))"#;
    let expect = expect_test::expect![[
        r#""OK (utf-8 utf-8 utf-8 utf-8 t t iso-latin-1 iso-latin-1 iso-latin-1 raw-text raw-text no-conversion undecided undecided)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// coding-system-eol-type variants
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_comprehensive_eol_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; EOL type: 0=unix(LF), 1=dos(CRLF), 2=mac(CR)
  ;; -unix variants
  (coding-system-eol-type 'utf-8-unix)
  (coding-system-eol-type 'latin-1-unix)
  (coding-system-eol-type 'raw-text-unix)
  ;; -dos variants
  (coding-system-eol-type 'utf-8-dos)
  (coding-system-eol-type 'latin-1-dos)
  ;; -mac variants
  (coding-system-eol-type 'utf-8-mac)
  ;; Base coding system without eol suffix returns a vector of 3 variants
  (vectorp (coding-system-eol-type 'utf-8))
  (let ((eol (coding-system-eol-type 'utf-8)))
    (when (vectorp eol)
      (list (aref eol 0) (aref eol 1) (aref eol 2))))
  ;; no-conversion is always unix-like
  (coding-system-eol-type 'no-conversion)
  ;; undecided
  (vectorp (coding-system-eol-type 'undecided)))"#;
    let expect =
        expect_test::expect![[r#""OK (0 0 0 1 1 2 t (utf-8-unix utf-8-dos utf-8-mac) 0 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// coding-system-aliases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_comprehensive_aliases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; coding-system-aliases returns a list of aliases
  (listp (coding-system-aliases 'utf-8))
  ;; utf-8 should be in its own alias list
  (memq 'utf-8 (coding-system-aliases 'utf-8))
  ;; latin-1 has aliases (often includes iso-8859-1, iso-latin-1, etc.)
  (listp (coding-system-aliases 'latin-1))
  (not (null (coding-system-aliases 'latin-1)))
  ;; Check that latin-1 is alias of iso-8859-1 or vice versa
  (let ((lat-aliases (coding-system-aliases 'latin-1)))
    (or (memq 'latin-1 lat-aliases)
        (memq 'iso-8859-1 lat-aliases)))
  ;; raw-text aliases
  (listp (coding-system-aliases 'raw-text))
  ;; no-conversion
  (listp (coding-system-aliases 'no-conversion))
  ;; Each alias should itself be a valid coding system
  (let ((aliases (coding-system-aliases 'utf-8)))
    (let ((all-valid t))
      (dolist (a aliases)
        (unless (coding-system-p a)
          (setq all-valid nil)))
      all-valid)))"#;
    let expect =
        expect_test::expect![[r#""OK (t (utf-8 mule-utf-8 cp65001) t t (latin-1) t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// coding-system-plist and coding-system-put
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_comprehensive_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; coding-system-plist returns a plist
  (listp (coding-system-plist 'utf-8))
  ;; The plist should have :mime-charset or mime-charset for utf-8
  (let ((pl (coding-system-plist 'utf-8)))
    (not (null pl)))
  ;; Various coding system plists are non-empty
  (not (null (coding-system-plist 'latin-1)))
  (not (null (coding-system-plist 'raw-text)))
  ;; coding-system-get retrieves properties
  (let ((charset (coding-system-get 'utf-8 :mime-charset)))
    ;; utf-8 should report utf-8 as MIME charset
    (if charset (symbolp charset) t))
  ;; coding-system-get for :mnemonic
  (let ((mnemonic (coding-system-get 'utf-8 :mnemonic)))
    (if mnemonic t t))  ;; just verify it doesn't error
  ;; coding-system-get with unknown property returns nil
  (coding-system-get 'utf-8 :neovm-nonexistent-prop))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// coding-system-priority-list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_comprehensive_priority_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; coding-system-priority-list returns a list of coding systems
  (listp (coding-system-priority-list))
  ;; It should be non-empty
  (not (null (coding-system-priority-list)))
  ;; All entries should be valid coding systems
  (let ((plist (coding-system-priority-list))
        (all-valid t))
    (dolist (cs plist)
      (unless (coding-system-p cs)
        (setq all-valid nil)))
    all-valid)
  ;; utf-8 should typically be in the priority list
  (not (null (memq 'utf-8 (coding-system-priority-list))))
  ;; Length should be reasonable (more than 1)
  (> (length (coding-system-priority-list)) 1))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// encode-coding-string / decode-coding-string roundtrips
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_comprehensive_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; UTF-8 roundtrip: ASCII
  (let* ((s "Hello, World!")
         (enc (encode-coding-string s 'utf-8))
         (dec (decode-coding-string enc 'utf-8)))
    (string= s dec))
  ;; UTF-8 roundtrip: accented characters
  (let* ((s "caf\u00E9 na\u00EFve")
         (enc (encode-coding-string s 'utf-8))
         (dec (decode-coding-string enc 'utf-8)))
    (string= s dec))
  ;; UTF-8 roundtrip: CJK
  (let* ((s "\u4F60\u597D\u4E16\u754C")
         (enc (encode-coding-string s 'utf-8))
         (dec (decode-coding-string enc 'utf-8)))
    (string= s dec))
  ;; UTF-8 roundtrip: mixed widths
  (let* ((s "A\u00E9\u4E2D\U0001F600")
         (enc (encode-coding-string s 'utf-8))
         (dec (decode-coding-string enc 'utf-8)))
    (list (string= s dec)
          (length s)
          (string-bytes enc)))
  ;; Latin-1 roundtrip
  (let* ((s "caf\u00E9")
         (enc (encode-coding-string s 'latin-1))
         (dec (decode-coding-string enc 'latin-1)))
    (string= s dec))
  ;; Latin-1: each char in the range 0-255 encodes to single byte
  (let* ((s "\u00C0\u00D1\u00FF")
         (enc (encode-coding-string s 'latin-1)))
    (= (string-bytes enc) 3))
  ;; raw-text roundtrip
  (let* ((s "hello\nworld\ttab")
         (enc (encode-coding-string s 'raw-text))
         (dec (decode-coding-string enc 'raw-text)))
    (string= s dec))
  ;; no-conversion roundtrip
  (let* ((s "test123")
         (enc (encode-coding-string s 'no-conversion))
         (dec (decode-coding-string enc 'no-conversion)))
    (string= s dec))
  ;; Empty string roundtrip across all codings
  (let ((codings '(utf-8 latin-1 raw-text no-conversion)))
    (mapcar (lambda (cs)
              (string= "" (decode-coding-string
                           (encode-coding-string "" cs) cs)))
            codings))
  ;; utf-8-unix vs utf-8-dos: encoding of newlines
  (let* ((s "a\nb")
         (enc-unix (encode-coding-string s 'utf-8-unix))
         (enc-dos (encode-coding-string s 'utf-8-dos)))
    (list (string-bytes enc-unix)
          (string-bytes enc-dos))))"#;
    let expect = expect_test::expect![[r#""OK (t t t (t 4 10) t t t t (t t t t) (3 4))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// UTF-8, Latin-1, ASCII systems: byte count differences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_comprehensive_byte_counts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--csc-byte-info
    (lambda (s coding)
      "Return (char-count byte-count) for encoding S with CODING."
      (let ((enc (encode-coding-string s coding)))
        (list (length s) (string-bytes enc)))))

  (unwind-protect
      (list
       ;; ASCII: same byte count for all codings
       (funcall 'neovm--csc-byte-info "hello" 'utf-8)
       (funcall 'neovm--csc-byte-info "hello" 'latin-1)
       (funcall 'neovm--csc-byte-info "hello" 'raw-text)
       ;; Latin-1 char: 1 byte in latin-1, 2 in utf-8
       (funcall 'neovm--csc-byte-info "\u00E9" 'utf-8)
       (funcall 'neovm--csc-byte-info "\u00E9" 'latin-1)
       ;; CJK: 3 bytes per char in utf-8
       (funcall 'neovm--csc-byte-info "\u4E2D\u6587" 'utf-8)
       ;; Emoji: 4 bytes in utf-8
       (funcall 'neovm--csc-byte-info "\U0001F600" 'utf-8)
       ;; Greek: 2 bytes per char in utf-8
       (funcall 'neovm--csc-byte-info "\u03B1\u03B2\u03B3" 'utf-8)
       ;; Mixed: verify total bytes
       (let ((s "A\u00E9\u4E2D"))
         (list
          (funcall 'neovm--csc-byte-info s 'utf-8)    ;; 1+2+3 = 6 bytes
          (length s))))                                  ;; 3 chars
    (fmakunbound 'neovm--csc-byte-info)))"#;
    let expect = expect_test::expect![[
        r#""OK ((5 5) (5 5) (5 5) (1 2) (1 1) (2 6) (1 4) (3 6) ((3 6) 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: coding system metadata exploration
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_comprehensive_metadata() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--csc-describe
    (lambda (cs)
      "Collect coding system metadata: base, eol-type, aliases count, plist keys."
      (condition-case nil
          (let* ((base (coding-system-base cs))
                 (eol (coding-system-eol-type cs))
                 (aliases (coding-system-aliases cs))
                 (plist (coding-system-plist cs))
                 (plist-keys
                  (let ((keys nil) (pl plist))
                    (while pl
                      (setq keys (cons (car pl) keys))
                      (setq pl (cddr pl)))
                    (nreverse keys))))
            (list base
                  (if (vectorp eol) 'vector-eol eol)
                  (length aliases)
                  (> (length plist-keys) 0)))
        (error (list 'error cs)))))

  (unwind-protect
      (list
       (funcall 'neovm--csc-describe 'utf-8)
       (funcall 'neovm--csc-describe 'utf-8-unix)
       (funcall 'neovm--csc-describe 'utf-8-dos)
       (funcall 'neovm--csc-describe 'latin-1)
       (funcall 'neovm--csc-describe 'latin-1-unix)
       (funcall 'neovm--csc-describe 'raw-text)
       (funcall 'neovm--csc-describe 'no-conversion)
       (funcall 'neovm--csc-describe 'undecided)
       ;; Verify consistency: base of unix variant = base of dos variant
       (eq (coding-system-base 'utf-8-unix)
           (coding-system-base 'utf-8-dos))
       (eq (coding-system-base 'latin-1-unix)
           (coding-system-base 'latin-1-dos)))
    (fmakunbound 'neovm--csc-describe)))"#;
    let expect = expect_test::expect![[
        r#""OK ((utf-8 vector-eol 3 t) (utf-8 0 3 t) (utf-8 1 3 t) (iso-latin-1 vector-eol 3 t) (iso-latin-1 0 3 t) (raw-text vector-eol 1 t) (no-conversion 0 2 t) (undecided vector-eol 1 t) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: encoding and detecting string properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_coding_system_comprehensive_string_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; multibyte-string-p on encoded strings
  (multibyte-string-p "hello")
  (multibyte-string-p (encode-coding-string "hello" 'utf-8))
  (multibyte-string-p (decode-coding-string "hello" 'utf-8))
  ;; encode produces unibyte, decode produces multibyte
  (not (multibyte-string-p (encode-coding-string "\u00E9" 'utf-8)))
  (multibyte-string-p (decode-coding-string
                        (encode-coding-string "\u00E9" 'utf-8) 'utf-8))
  ;; string-as-unibyte / string-as-multibyte interactions
  (let* ((s "test")
         (enc (encode-coding-string s 'utf-8)))
    (list
     (multibyte-string-p s)
     (multibyte-string-p enc)
     (string= s (decode-coding-string enc 'utf-8))))
  ;; find-coding-systems-string: find codings that can encode a string
  (let ((codings (find-coding-systems-string "hello")))
    (list (listp codings)
          (not (null codings))
          ;; undecided should be in the list (it can handle anything)
          (not (null (memq 'undecided codings)))))
  ;; find-coding-systems-string for Latin-1 range
  (let ((codings (find-coding-systems-string "\u00E9")))
    (list (not (null (memq 'utf-8 codings)))
          (not (null (memq 'latin-1 codings))))))"#;
    let expect = expect_test::expect![[r#""OK (nil nil t t t (nil nil t) (t t t) (t nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

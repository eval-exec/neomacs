//! Advanced oracle parity tests for character literals and character operations.
//!
//! Tests escape sequences, control/meta modifiers, char-to-string roundtrips,
//! character arithmetic, character class testing, and a char-based cipher.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// All escape sequences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_literal_all_escape_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test every named escape sequence and verify their integer values
    let form = r#"(list ?\n ?\t ?\r ?\0 ?\a ?\f ?\v ?\e ?\d ?\s
                        (= ?\n 10)
                        (= ?\t 9)
                        (= ?\r 13)
                        (= ?\0 0)
                        (= ?\a 7)
                        (= ?\f 12)
                        (= ?\v 11)
                        (= ?\e 27)
                        (= ?\d 127)
                        (= ?\s 32))"#;
    let expect = expect_test::expect![r#""OK (10 9 13 0 7 12 11 27 127 32 t t t t t t t t t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Control characters ?\C-a through ?\C-z
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_literal_control_chars_full_alphabet() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // ?\C-a = 1, ?\C-b = 2, ..., ?\C-z = 26
    // Also verify the relationship: ?\C-x = (- ?x 96)
    let form = r#"(let ((ctrl-vals (list ?\C-a ?\C-b ?\C-c ?\C-d ?\C-e
                                         ?\C-f ?\C-g ?\C-h ?\C-i ?\C-j
                                         ?\C-k ?\C-l ?\C-m ?\C-n ?\C-o
                                         ?\C-p ?\C-q ?\C-r ?\C-s ?\C-t
                                         ?\C-u ?\C-v ?\C-w ?\C-x ?\C-y
                                         ?\C-z))
                        (expected nil))
                    ;; Build expected: 1 through 26
                    (let ((i 1))
                      (while (<= i 26)
                        (setq expected (cons i expected))
                        (setq i (1+ i))))
                    (setq expected (nreverse expected))
                    (list (equal ctrl-vals expected)
                          ;; Verify specific well-known control chars
                          (= ?\C-a 1)    ;; SOH
                          (= ?\C-g 7)    ;; BEL (same as ?\a)
                          (= ?\C-h 8)    ;; BS (same as ?\b backspace)
                          (= ?\C-i 9)    ;; TAB (same as ?\t)
                          (= ?\C-j 10)   ;; LF (same as ?\n)
                          (= ?\C-m 13)   ;; CR (same as ?\r)
                          (= ?\C-[ 27))) ;; ESC (same as ?\e)
"#;
    let expect = expect_test::expect![r#""OK (t t t t t t t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Meta modifier and combined modifiers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_literal_meta_and_combined_modifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Meta sets bit 27 (2^27 = 134217728), Control-Meta combines both
    let form = r#"(let ((meta-a ?\M-a)
                        (meta-z ?\M-z)
                        (ctrl-meta-a ?\C-\M-a)
                        (meta-ctrl-a ?\M-\C-a)
                        (shift-a ?\S-a)
                        (hyper-a ?\H-a)
                        (super-a ?\s-a)
                        (meta-bit (lsh 1 27))
                        (ctrl-bit 0)
                        (shift-bit (lsh 1 25)))
                    (list
                      ;; Meta adds 2^27 to base char code
                      meta-a
                      (= meta-a (+ ?a meta-bit))
                      ;; ?\C-\M-a and ?\M-\C-a should be the same
                      (= ctrl-meta-a meta-ctrl-a)
                      ;; ctrl-meta-a has both control (char becomes 1) and meta bit
                      ctrl-meta-a
                      ;; Various modifier chars
                      (list meta-z shift-a hyper-a super-a)))"#;
    let expect = expect_test::expect![
        r#""OK (134217825 t t 134217729 (134217850 33554529 16777313 8388705))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-to-string / string-to-char roundtrip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_literal_roundtrip_char_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Round-trip through char-to-string and string-to-char for various char types
    let form = r#"(let ((chars (list ?a ?Z ?0 ?! ?\n ?\t ?é ?日 ?😀 ?\s))
                        (results nil))
                    (dolist (c chars)
                      (let* ((s (char-to-string c))
                             (back (string-to-char s))
                             (match (= c back)))
                        (setq results (cons (list c s back match) results))))
                    ;; Also test: string-to-char of multi-char string gives first char
                    (let ((first-of-multi (string-to-char "hello"))
                          (first-of-utf8 (string-to-char "日本語")))
                      (list (nreverse results)
                            (= first-of-multi ?h)
                            (= first-of-utf8 ?日)
                            ;; string-to-char of empty string is 0
                            (= (string-to-char "") 0))))"#;
    let expect = expect_test::expect![[
        r#""OK (((97 \"a\" 97 t) (90 \"Z\" 90 t) (48 \"0\" 48 t) (33 \"!\" 33 t) (10 \"\\n\" 10 t) (9 \"\t\" 9 t) (233 \"é\" 233 t) (26085 \"日\" 26085 t) (128512 \"😀\" 128512 t) (32 \" \" 32 t)) t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Character arithmetic and comparisons
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_literal_arithmetic_and_ranges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Characters are integers: arithmetic, comparison, ranges
    let form = r#"(let ((digit-range (- ?9 ?0))
                        (upper-range (- ?Z ?A))
                        (lower-range (- ?z ?a))
                        (case-offset (- ?a ?A)))
                    ;; Build alphabet by arithmetic
                    (let ((alphabet nil)
                          (digits nil))
                      (let ((i 0))
                        (while (< i 26)
                          (setq alphabet (cons (+ ?a i) alphabet))
                          (setq i (1+ i))))
                      (setq alphabet (nreverse alphabet))
                      (let ((i 0))
                        (while (< i 10)
                          (setq digits (cons (+ ?0 i) digits))
                          (setq i (1+ i))))
                      (setq digits (nreverse digits))
                      (list digit-range       ;; 9
                            upper-range        ;; 25
                            lower-range        ;; 25
                            case-offset        ;; 32
                            ;; Verify char range arithmetic
                            (= (length alphabet) 26)
                            (= (length digits) 10)
                            ;; First and last
                            (= (car alphabet) ?a)
                            (= (car (last alphabet)) ?z)
                            (= (car digits) ?0)
                            (= (car (last digits)) ?9)
                            ;; Convert uppercase to lowercase by adding offset
                            (= (+ ?A case-offset) ?a)
                            (= (+ ?M case-offset) ?m))))"#;
    let expect = expect_test::expect![r#""OK (9 25 25 32 t t t t t t t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Character class testing (alpha, digit, upper, lower, etc.)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_literal_character_class_testing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement character class predicates and classify a mixed string
    let form = r#"(let ((char-alpha-p
                         (lambda (c)
                           (or (and (>= c ?a) (<= c ?z))
                               (and (>= c ?A) (<= c ?Z)))))
                        (char-digit-p
                         (lambda (c)
                           (and (>= c ?0) (<= c ?9))))
                        (char-upper-p
                         (lambda (c)
                           (and (>= c ?A) (<= c ?Z))))
                        (char-lower-p
                         (lambda (c)
                           (and (>= c ?a) (<= c ?z))))
                        (char-alnum-p
                         (lambda (c)
                           (or (and (>= c ?a) (<= c ?z))
                               (and (>= c ?A) (<= c ?Z))
                               (and (>= c ?0) (<= c ?9)))))
                        (char-space-p
                         (lambda (c)
                           (or (= c ?\s) (= c ?\t) (= c ?\n)
                               (= c ?\r) (= c ?\f) (= c ?\v)))))
                    ;; Classify each char in a mixed string
                    (let ((s "aB3 \t!@")
                          (result nil)
                          (i 0))
                      (while (< i (length s))
                        (let ((c (aref s i)))
                          (setq result
                                (cons (list
                                       (char-to-string c)
                                       (if (funcall char-alpha-p c) 'alpha nil)
                                       (if (funcall char-digit-p c) 'digit nil)
                                       (if (funcall char-upper-p c) 'upper nil)
                                       (if (funcall char-lower-p c) 'lower nil)
                                       (if (funcall char-alnum-p c) 'alnum nil)
                                       (if (funcall char-space-p c) 'space nil))
                                      result)))
                        (setq i (1+ i)))
                      (nreverse result)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"a\" alpha nil nil lower alnum nil) (\"B\" alpha nil upper nil alnum nil) (\"3\" nil digit nil nil alnum nil) (\" \" nil nil nil nil nil space) (\"\t\" nil nil nil nil nil space) (\"!\" nil nil nil nil nil nil) (\"@\" nil nil nil nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hex and octal escape sequences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_literal_hex_and_octal_escapes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test hex (\x), unicode (\u, \U), and octal (\NNN) char escapes
    let form = r#"(list
                    ;; Hex escapes
                    ?\x41               ;; 'A' = 65
                    ?\x61               ;; 'a' = 97
                    ?\x0a               ;; newline = 10
                    ;; Verify they match the normal chars
                    (= ?\x41 ?A)
                    (= ?\x61 ?a)
                    (= ?\x0a ?\n)
                    ;; Octal escapes
                    ?\101               ;; 'A' = 65 octal 101
                    ?\141               ;; 'a' = 97 octal 141
                    ?\012               ;; newline = 10 octal 012
                    (= ?\101 ?A)
                    (= ?\141 ?a)
                    (= ?\012 ?\n)
                    ;; Unicode escapes in strings, verify via string-to-char
                    (= (string-to-char "\u00e9") ?é)
                    (= (string-to-char "\u65e5") ?日))"#;
    let expect = expect_test::expect![r#""OK (65 97 10 t t t 65 97 10 t t t t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: Vigenere cipher using char operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_literal_vigenere_cipher() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Vigenere cipher: polyalphabetic substitution using a keyword
    // Only encrypts letters; non-letters pass through unchanged
    let form = r#"(let ((vigenere-encrypt
                         (lambda (plaintext key)
                           (let ((result nil)
                                 (key-len (length key))
                                 (key-idx 0)
                                 (i 0))
                             (while (< i (length plaintext))
                               (let ((c (aref plaintext i))
                                     (k (- (downcase (aref key (% key-idx key-len))) ?a)))
                                 (cond
                                  ((and (>= c ?a) (<= c ?z))
                                   (setq result
                                         (cons (+ ?a (% (+ (- c ?a) k) 26))
                                               result))
                                   (setq key-idx (1+ key-idx)))
                                  ((and (>= c ?A) (<= c ?Z))
                                   (setq result
                                         (cons (+ ?A (% (+ (- c ?A) k) 26))
                                               result))
                                   (setq key-idx (1+ key-idx)))
                                  (t
                                   (setq result (cons c result)))))
                               (setq i (1+ i)))
                             (concat (nreverse result)))))
                        (vigenere-decrypt
                         (lambda (ciphertext key)
                           (let ((result nil)
                                 (key-len (length key))
                                 (key-idx 0)
                                 (i 0))
                             (while (< i (length ciphertext))
                               (let ((c (aref ciphertext i))
                                     (k (- (downcase (aref key (% key-idx key-len))) ?a)))
                                 (cond
                                  ((and (>= c ?a) (<= c ?z))
                                   (setq result
                                         (cons (+ ?a (% (+ (- c ?a) (- 26 k)) 26))
                                               result))
                                   (setq key-idx (1+ key-idx)))
                                  ((and (>= c ?A) (<= c ?Z))
                                   (setq result
                                         (cons (+ ?A (% (+ (- c ?A) (- 26 k)) 26))
                                               result))
                                   (setq key-idx (1+ key-idx)))
                                  (t
                                   (setq result (cons c result)))))
                               (setq i (1+ i)))
                             (concat (nreverse result))))))
                    (let* ((plain "Hello, World!")
                           (key "secret")
                           (encrypted (funcall vigenere-encrypt plain key))
                           (decrypted (funcall vigenere-decrypt encrypted key)))
                      (list encrypted
                            decrypted
                            (string= decrypted plain)
                            ;; Verify different keys produce different ciphertext
                            (let ((enc2 (funcall vigenere-encrypt plain "other")))
                              (not (string= encrypted enc2))))))"#;
    let expect = expect_test::expect![[r#""OK (\"Zincs, Pgvnu!\" \"Hello, World!\" t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

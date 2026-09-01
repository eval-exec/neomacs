//! Oracle parity tests for encoding / serialization algorithms
//! implemented in pure Elisp.
//!
//! Covers: simplified base64, URL percent-encoding, Roman numeral
//! encode/decode, Morse code encode/decode, prefix-free encoding,
//! simple checksum.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Simplified Base64 encoding (pure Elisp, ASCII subset)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encoding_base64_simple() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Encode bytes into base64 alphabet manually.
    // Only handles complete 3-byte groups for simplicity.
    let form = r#"(let ((b64-table "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"))
                    (let ((encode
                           (lambda (input)
                             (let ((result nil)
                                   (i 0)
                                   (len (length input)))
                               ;; Process 3-byte groups
                               (while (<= (+ i 2) (1- len))
                                 (let* ((b0 (aref input i))
                                        (b1 (aref input (1+ i)))
                                        (b2 (aref input (+ i 2)))
                                        (n (+ (ash b0 16) (ash b1 8) b2)))
                                   (setq result
                                         (cons (aref b64-table (logand (ash n -18) 63))
                                               result))
                                   (setq result
                                         (cons (aref b64-table (logand (ash n -12) 63))
                                               result))
                                   (setq result
                                         (cons (aref b64-table (logand (ash n -6) 63))
                                               result))
                                   (setq result
                                         (cons (aref b64-table (logand n 63))
                                               result))
                                   (setq i (+ i 3))))
                               ;; Handle remaining 1 or 2 bytes
                               (let ((remaining (- len i)))
                                 (cond
                                   ((= remaining 2)
                                    (let* ((b0 (aref input i))
                                           (b1 (aref input (1+ i)))
                                           (n (+ (ash b0 16) (ash b1 8))))
                                      (setq result
                                            (cons ?=
                                                  (cons (aref b64-table (logand (ash n -6) 63))
                                                        (cons (aref b64-table (logand (ash n -12) 63))
                                                              (cons (aref b64-table (logand (ash n -18) 63))
                                                                    result)))))))
                                   ((= remaining 1)
                                    (let* ((b0 (aref input i))
                                           (n (ash b0 16)))
                                      (setq result
                                            (cons ?=
                                                  (cons ?=
                                                        (cons (aref b64-table (logand (ash n -12) 63))
                                                              (cons (aref b64-table (logand (ash n -18) 63))
                                                                    result)))))))))
                               (concat (nreverse result))))))
                      (list
                        (funcall encode "Man")
                        (funcall encode "Ma")
                        (funcall encode "M")
                        (funcall encode "Hello!"))))"#;
    let expect = expect_test::expect![[r#""OK (\"TWFu\" \"TWE=\" \"TQ==\" \"SGVsbG8h\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// URL percent-encoding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encoding_url_percent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Percent-encode a string: keep [A-Za-z0-9_.-~], encode rest as %XX.
    let form = r#"(let ((hex "0123456789ABCDEF")
                        (safe-p
                         (lambda (ch)
                           (or (and (>= ch ?A) (<= ch ?Z))
                               (and (>= ch ?a) (<= ch ?z))
                               (and (>= ch ?0) (<= ch ?9))
                               (= ch ?_) (= ch ?.) (= ch ?-) (= ch ?~)))))
                    (let ((url-encode
                           (lambda (s)
                             (let ((result nil))
                               (dotimes (i (length s))
                                 (let ((ch (aref s i)))
                                   (if (funcall safe-p ch)
                                       (setq result (cons ch result))
                                     (setq result
                                           (cons (aref hex (logand ch 15))
                                                 (cons (aref hex (logand (ash ch -4) 15))
                                                       (cons ?% result)))))))
                               (concat (nreverse result)))))
                          (url-decode
                           (lambda (s)
                             (let ((result nil) (i 0) (len (length s)))
                               (while (< i len)
                                 (let ((ch (aref s i)))
                                   (if (and (= ch ?%)
                                            (< (+ i 2) len))
                                       (let* ((hi (aref s (1+ i)))
                                              (lo (aref s (+ i 2)))
                                              (hex-val
                                               (lambda (c)
                                                 (cond
                                                   ((and (>= c ?0) (<= c ?9)) (- c ?0))
                                                   ((and (>= c ?A) (<= c ?F)) (+ 10 (- c ?A)))
                                                   ((and (>= c ?a) (<= c ?f)) (+ 10 (- c ?a)))
                                                   (t 0)))))
                                         (setq result
                                               (cons (+ (ash (funcall hex-val hi) 4)
                                                        (funcall hex-val lo))
                                                     result))
                                         (setq i (+ i 3)))
                                     (setq result (cons ch result)
                                           i (1+ i)))))
                               (concat (nreverse result))))))
                      ;; Roundtrip test
                      (let ((inputs '("hello world" "foo@bar.com" "a=1&b=2"
                                      "100% done!" "/path/to/file")))
                        (mapcar (lambda (s)
                                  (let ((encoded (funcall url-encode s)))
                                    (list encoded
                                          (equal s (funcall url-decode encoded)))))
                                inputs))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"hello%20world\" t) (\"foo%40bar.com\" t) (\"a%3D1%26b%3D2\" t) (\"100%25%20done%21\" t) (\"%2Fpath%2Fto%2Ffile\" t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Roman numeral encoding/decoding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encoding_roman_numerals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Integer to Roman and back.
    let form = r#"(let ((to-roman
                         (lambda (n)
                           (let ((table '((1000 . "M") (900 . "CM") (500 . "D")
                                          (400 . "CD") (100 . "C") (90 . "XC")
                                          (50 . "L") (40 . "XL") (10 . "X")
                                          (9 . "IX") (5 . "V") (4 . "IV") (1 . "I")))
                                 (result ""))
                             (dolist (pair table)
                               (while (>= n (car pair))
                                 (setq result (concat result (cdr pair))
                                       n (- n (car pair)))))
                             result)))
                        (from-roman
                         (lambda (s)
                           (let ((values '((?M . 1000) (?D . 500) (?C . 100)
                                           (?L . 50) (?X . 10) (?V . 5) (?I . 1)))
                                 (total 0)
                                 (prev 0))
                             (let ((i (1- (length s))))
                               (while (>= i 0)
                                 (let ((val (cdr (assq (aref s i) values))))
                                   (if (< val prev)
                                       (setq total (- total val))
                                     (setq total (+ total val)))
                                   (setq prev val))
                                 (setq i (1- i))))
                             total))))
                    ;; Test encode, decode, roundtrip
                    (let ((nums '(1 4 9 14 42 99 399 944 1776 2024 3999)))
                      (mapcar (lambda (n)
                                (let ((roman (funcall to-roman n)))
                                  (list n roman (funcall from-roman roman))))
                              nums)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 \"I\" 1) (4 \"IV\" 4) (9 \"IX\" 9) (14 \"XIV\" 14) (42 \"XLII\" 42) (99 \"XCIX\" 99) (399 \"CCCXCIX\" 399) (944 \"CMXLIV\" 944) (1776 \"MDCCLXXVI\" 1776) (2024 \"MMXXIV\" 2024) (3999 \"MMMCMXCIX\" 3999))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Morse code encoding/decoding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encoding_morse_code() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((morse-table
                         '((?A . ".-") (?B . "-...") (?C . "-.-.") (?D . "-..")
                           (?E . ".") (?F . "..-.") (?G . "--.") (?H . "....")
                           (?I . "..") (?J . ".---") (?K . "-.-") (?L . ".-..")
                           (?M . "--") (?N . "-.") (?O . "---") (?P . ".--.")
                           (?Q . "--.-") (?R . ".-.") (?S . "...") (?T . "-")
                           (?U . "..-") (?V . "...-") (?W . ".--") (?X . "-..-")
                           (?Y . "-.--") (?Z . "--..") (?0 . "-----")
                           (?1 . ".----") (?2 . "..---") (?3 . "...--")
                           (?4 . "....-") (?5 . ".....") (?6 . "-....")
                           (?7 . "--...") (?8 . "---..") (?9 . "----."))))
                    (let ((encode
                           (lambda (text)
                             (let ((result nil)
                                   (utext (upcase text)))
                               (dotimes (i (length utext))
                                 (let* ((ch (aref utext i))
                                        (code (cdr (assq ch morse-table))))
                                   (when code
                                     (setq result (cons code result)))))
                               (mapconcat 'identity (nreverse result) " "))))
                          ;; Build reverse table for decoding
                          (reverse-table (make-hash-table :test 'equal)))
                      (dolist (pair morse-table)
                        (puthash (cdr pair) (car pair) reverse-table))
                      (let ((decode
                             (lambda (morse)
                               (let ((codes nil)
                                     (current "")
                                     (i 0)
                                     (len (length morse)))
                                 (while (< i len)
                                   (let ((ch (aref morse i)))
                                     (if (= ch ?\s)
                                         (progn
                                           (when (> (length current) 0)
                                             (setq codes (cons current codes)
                                                   current ""))
                                           )
                                       (setq current (concat current (char-to-string ch)))))
                                   (setq i (1+ i)))
                                 (when (> (length current) 0)
                                   (setq codes (cons current codes)))
                                 (concat
                                   (mapcar (lambda (c)
                                             (or (gethash c reverse-table) ??))
                                           (nreverse codes)))))))
                        ;; Test roundtrip
                        (let ((words '("HELLO" "SOS" "ELISP42")))
                          (mapcar (lambda (w)
                                    (let ((encoded (funcall encode w)))
                                      (list w encoded
                                            (funcall decode encoded))))
                                  words)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"HELLO\" \".... . .-.. .-.. ---\" \"HELLO\") (\"SOS\" \"... --- ...\" \"SOS\") (\"ELISP42\" \". .-.. .. ... .--. ....- ..---\" \"ELISP42\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Prefix-free variable-length encoding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encoding_prefix_free() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simple Elias gamma coding: encode positive integer n as
    // floor(log2(n)) zeros followed by n in binary.
    // E.g.: 1->"1", 2->"010", 3->"011", 4->"00100", 10->"0001010"
    let form = r#"(let ((gamma-encode
                         (lambda (n)
                           (if (<= n 0)
                               (signal 'error '("positive only"))
                             ;; Find binary representation
                             (let ((bits nil)
                                   (tmp n)
                                   (len 0))
                               (while (> tmp 0)
                                 (setq bits (cons (% tmp 2) bits)
                                       tmp (/ tmp 2)
                                       len (1+ len)))
                               ;; Prepend (len-1) zeros
                               (let ((prefix nil))
                                 (dotimes (_ (1- len))
                                   (setq prefix (cons ?0 prefix)))
                                 (concat (nreverse prefix)
                                         (mapconcat
                                           (lambda (b) (number-to-string b))
                                           bits "")))))))
                        (gamma-decode
                         (lambda (s)
                           ;; Count leading zeros
                           (let ((zeros 0)
                                 (i 0)
                                 (len (length s)))
                             (while (and (< i len) (= (aref s i) ?0))
                               (setq zeros (1+ zeros)
                                     i (1+ i)))
                             ;; Read (zeros+1) bits as binary number
                             (let ((n 0)
                                   (bits-to-read (1+ zeros)))
                               (dotimes (_ bits-to-read)
                                 (when (< i len)
                                   (setq n (+ (* n 2)
                                              (- (aref s i) ?0))
                                         i (1+ i))))
                               n)))))
                    (let ((test-values '(1 2 3 4 5 10 16 31 42 100)))
                      (mapcar (lambda (n)
                                (let ((encoded (funcall gamma-encode n)))
                                  (list n encoded
                                        (funcall gamma-decode encoded))))
                              test-values)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 \"1\" 1) (2 \"010\" 2) (3 \"011\" 3) (4 \"00100\" 4) (5 \"00101\" 5) (10 \"0001010\" 10) (16 \"000010000\" 16) (31 \"000011111\" 31) (42 \"00000101010\" 42) (100 \"0000001100100\" 100))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Simple checksum (CRC-like XOR-shift)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_encoding_checksum_xor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simple 8-bit checksum: XOR all bytes, then rotate and XOR with
    // length for a slightly stronger hash.
    let form = r#"(let ((checksum
                         (lambda (s)
                           (let ((hash 0)
                                 (len (length s)))
                             (dotimes (i len)
                               (let ((byte (aref s i)))
                                 ;; XOR with byte
                                 (setq hash (logxor hash byte))
                                 ;; Rotate left by 1 within 8 bits
                                 (setq hash
                                       (logand
                                         (logior (ash hash 1)
                                                 (ash hash -7))
                                         255))))
                             ;; Mix in length
                             (logand (logxor hash len) 255)))))
                    ;; Different strings should (likely) produce different checksums.
                    ;; Same string should always produce same checksum.
                    (let ((inputs '("hello" "world" "hello" "foo" "bar" "baz"
                                    "" "a" "ab" "abc")))
                      (mapcar (lambda (s)
                                (cons s (funcall checksum s)))
                              inputs)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"hello\" . 82) (\"world\" . 247) (\"hello\" . 82) (\"foo\" . 83) (\"bar\" . 113) (\"baz\" . 97) (\"\" . 0) (\"a\" . 195) (\"ab\" . 67) (\"abc\" . 71))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

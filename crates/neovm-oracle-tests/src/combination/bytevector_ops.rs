//! Oracle parity tests for operations on unibyte strings as byte vectors:
//! string-bytes, aref on unibyte strings, building binary data,
//! byte manipulation (XOR, rotate), checksum computation,
//! base64-like encoding/decoding, hex string conversion.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// string-bytes and aref on unibyte strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bytevector_string_bytes_aref() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((s1 (string-to-unibyte "hello"))
         (s2 (make-string 5 0))
         (s3 (unibyte-string 72 101 108 108 111)))
  ;; Populate s2 manually
  (dotimes (i 5)
    (aset s2 i (+ 65 i)))
  (list
    ;; string-bytes on unibyte strings
    (string-bytes s1)
    (string-bytes s2)
    (string-bytes s3)
    (string-bytes "")
    ;; aref to read individual bytes
    (aref s1 0)
    (aref s1 4)
    (aref s2 0)
    (aref s2 4)
    (aref s3 0)
    ;; Confirm they match expected ASCII values
    (= (aref s1 0) ?h)
    (= (aref s3 0) ?H)
    (= (aref s2 0) ?A)
    ;; Build a byte vector by reading all bytes
    (let ((bytes nil))
      (dotimes (i (length s1))
        (setq bytes (cons (aref s1 i) bytes)))
      (nreverse bytes))
    ;; Unibyte-string roundtrip
    (equal s3 (unibyte-string 72 101 108 108 111))
    ;; Length vs string-bytes for unibyte
    (= (length s1) (string-bytes s1))
    ;; Comparison
    (string= s1 s3)
    (string< s2 s1)))"#;
    let expect = expect_test::expect![[
        r#""OK (5 5 5 0 104 111 65 69 72 t t t (104 101 108 108 111) t t nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Building binary data: constructing byte sequences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bytevector_build_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-make-bytes
    (lambda (byte-list)
      "Construct a unibyte string from a list of byte values (0-255)."
      (apply #'unibyte-string byte-list)))

  (fset 'neovm--test-bytes-to-list
    (lambda (s)
      "Convert a unibyte string to a list of byte values."
      (let ((result nil))
        (dotimes (i (length s))
          (setq result (cons (aref s i) result)))
        (nreverse result))))

  (fset 'neovm--test-pack-uint16-be
    (lambda (n)
      "Pack a 16-bit unsigned integer as 2 bytes big-endian."
      (unibyte-string (logand (ash n -8) 255)
                      (logand n 255))))

  (fset 'neovm--test-unpack-uint16-be
    (lambda (s offset)
      "Unpack a 16-bit unsigned integer from 2 bytes big-endian."
      (+ (ash (aref s offset) 8)
         (aref s (1+ offset)))))

  (fset 'neovm--test-pack-uint32-be
    (lambda (n)
      "Pack a 32-bit unsigned integer as 4 bytes big-endian."
      (unibyte-string (logand (ash n -24) 255)
                      (logand (ash n -16) 255)
                      (logand (ash n -8) 255)
                      (logand n 255))))

  (fset 'neovm--test-unpack-uint32-be
    (lambda (s offset)
      "Unpack a 32-bit unsigned integer from 4 bytes big-endian."
      (+ (ash (aref s offset) 24)
         (ash (aref s (1+ offset)) 16)
         (ash (aref s (+ offset 2)) 8)
         (aref s (+ offset 3)))))

  (unwind-protect
      (list
        ;; Build and decompose
        (funcall 'neovm--test-bytes-to-list
                 (funcall 'neovm--test-make-bytes '(0 127 128 255)))
        ;; Pack/unpack uint16
        (let* ((n 0)
               (packed (funcall 'neovm--test-pack-uint16-be n)))
          (funcall 'neovm--test-unpack-uint16-be packed 0))
        (let* ((n 256)
               (packed (funcall 'neovm--test-pack-uint16-be n)))
          (funcall 'neovm--test-unpack-uint16-be packed 0))
        (let* ((n 65535)
               (packed (funcall 'neovm--test-pack-uint16-be n)))
          (funcall 'neovm--test-unpack-uint16-be packed 0))
        ;; Pack/unpack uint32
        (let* ((n 305419896)  ;; 0x12345678
               (packed (funcall 'neovm--test-pack-uint32-be n))
               (bytes (funcall 'neovm--test-bytes-to-list packed)))
          (list bytes (funcall 'neovm--test-unpack-uint32-be packed 0)))
        ;; Roundtrip multiple uint16 values
        (let ((values '(0 1 255 256 1000 32767 65535)))
          (mapcar (lambda (v)
                    (let ((packed (funcall 'neovm--test-pack-uint16-be v)))
                      (= v (funcall 'neovm--test-unpack-uint16-be packed 0))))
                  values))
        ;; Concatenate packed values into a byte buffer
        (let* ((buf (concat (funcall 'neovm--test-pack-uint16-be 1000)
                            (funcall 'neovm--test-pack-uint16-be 2000)
                            (funcall 'neovm--test-pack-uint16-be 3000))))
          (list (funcall 'neovm--test-unpack-uint16-be buf 0)
                (funcall 'neovm--test-unpack-uint16-be buf 2)
                (funcall 'neovm--test-unpack-uint16-be buf 4))))
    (fmakunbound 'neovm--test-make-bytes)
    (fmakunbound 'neovm--test-bytes-to-list)
    (fmakunbound 'neovm--test-pack-uint16-be)
    (fmakunbound 'neovm--test-unpack-uint16-be)
    (fmakunbound 'neovm--test-pack-uint32-be)
    (fmakunbound 'neovm--test-unpack-uint32-be)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 127 128 255) 0 256 65535 ((18 52 86 120) 305419896) (t t t t t t t) (1000 2000 3000))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Byte manipulation: XOR, rotate, shift
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bytevector_xor_rotate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-xor-bytes
    (lambda (s1 s2)
      "XOR two same-length unibyte strings byte-by-byte."
      (let ((len (min (length s1) (length s2)))
            (result nil))
        (dotimes (i len)
          (setq result (cons (logxor (aref s1 i) (aref s2 i)) result)))
        (apply #'unibyte-string (nreverse result)))))

  (fset 'neovm--test-rotate-byte-left
    (lambda (byte n)
      "Rotate BYTE left by N bits within 8 bits."
      (let ((shift (% n 8)))
        (logand (logior (ash byte shift)
                        (ash byte (- shift 8)))
                255))))

  (fset 'neovm--test-rotate-byte-right
    (lambda (byte n)
      "Rotate BYTE right by N bits within 8 bits."
      (let ((shift (% n 8)))
        (logand (logior (ash byte (- shift))
                        (ash byte (- 8 shift)))
                255))))

  (fset 'neovm--test-rotate-bytes
    (lambda (s n direction)
      "Rotate each byte in unibyte string S by N bits."
      (let ((result nil))
        (dotimes (i (length s))
          (let ((b (aref s i)))
            (setq result
                  (cons (if (eq direction 'left)
                            (funcall 'neovm--test-rotate-byte-left b n)
                          (funcall 'neovm--test-rotate-byte-right b n))
                        result))))
        (apply #'unibyte-string (nreverse result)))))

  (unwind-protect
      (list
        ;; XOR: same string XOR itself = all zeros
        (let* ((s (unibyte-string 65 66 67 68))
               (xored (funcall 'neovm--test-xor-bytes s s)))
          (let ((result nil))
            (dotimes (i (length xored))
              (setq result (cons (aref xored i) result)))
            (nreverse result)))
        ;; XOR with key and back (encryption/decryption roundtrip)
        (let* ((plaintext (string-to-unibyte "HELLO"))
               (key (unibyte-string 42 42 42 42 42))
               (encrypted (funcall 'neovm--test-xor-bytes plaintext key))
               (decrypted (funcall 'neovm--test-xor-bytes encrypted key)))
          (list (not (string= plaintext encrypted))
                (string= plaintext decrypted)))
        ;; Rotate byte left
        (funcall 'neovm--test-rotate-byte-left #b10000001 1)  ;; -> #b00000011
        (funcall 'neovm--test-rotate-byte-left #b10000001 3)  ;; -> #b00001100 + 1 shifted
        (funcall 'neovm--test-rotate-byte-left 255 3)         ;; 255 stays 255
        ;; Rotate byte right
        (funcall 'neovm--test-rotate-byte-right #b10000001 1) ;; -> #b11000000
        (funcall 'neovm--test-rotate-byte-right 1 1)          ;; -> #b10000000
        ;; Rotate all bytes in a string and back
        (let* ((original (unibyte-string 170 85 255 0 128))
               (rotated (funcall 'neovm--test-rotate-bytes original 3 'left))
               (restored (funcall 'neovm--test-rotate-bytes rotated 3 'right)))
          (string= original restored))
        ;; XOR-based simple stream cipher
        (let* ((msg (string-to-unibyte "secret"))
               (key-seed 137)
               (key-bytes nil))
          (dotimes (i (length msg))
            (setq key-bytes
                  (cons (logand (+ key-seed (* i 31)) 255)
                        key-bytes)))
          (let* ((key-str (apply #'unibyte-string (nreverse key-bytes)))
                 (enc (funcall 'neovm--test-xor-bytes msg key-str))
                 (dec (funcall 'neovm--test-xor-bytes enc key-str)))
            (list (string= msg dec)
                  (not (string= msg enc))))))
    (fmakunbound 'neovm--test-xor-bytes)
    (fmakunbound 'neovm--test-rotate-byte-left)
    (fmakunbound 'neovm--test-rotate-byte-right)
    (fmakunbound 'neovm--test-rotate-bytes)))"#;
    let expect = expect_test::expect![[r#""OK ((0 0 0 0) (t t) 3 12 255 192 128 t (t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Checksum computation: Fletcher-16 and Adler-32-like
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bytevector_checksum() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-fletcher16
    (lambda (s)
      "Compute Fletcher-16 checksum of unibyte string S."
      (let ((sum1 0) (sum2 0))
        (dotimes (i (length s))
          (setq sum1 (% (+ sum1 (aref s i)) 255))
          (setq sum2 (% (+ sum2 sum1) 255)))
        (+ (ash sum2 8) sum1))))

  (fset 'neovm--test-adler32
    (lambda (s)
      "Compute Adler-32-like checksum of unibyte string S."
      (let ((a 1) (b 0)
            (mod-val 65521))
        (dotimes (i (length s))
          (setq a (% (+ a (aref s i)) mod-val))
          (setq b (% (+ b a) mod-val)))
        (+ (ash b 16) a))))

  (fset 'neovm--test-crc8
    (lambda (s)
      "Simple CRC-8 (polynomial 0x07)."
      (let ((crc 0))
        (dotimes (i (length s))
          (setq crc (logxor crc (aref s i)))
          (dotimes (_ 8)
            (if (> (logand crc #x80) 0)
                (setq crc (logand (logxor (ash crc 1) #x07) 255))
              (setq crc (logand (ash crc 1) 255)))))
        crc)))

  (unwind-protect
      (list
        ;; Fletcher-16 on various inputs
        (funcall 'neovm--test-fletcher16 (string-to-unibyte ""))
        (funcall 'neovm--test-fletcher16 (string-to-unibyte "a"))
        (funcall 'neovm--test-fletcher16 (string-to-unibyte "abcdef"))
        (funcall 'neovm--test-fletcher16 (string-to-unibyte "hello world"))
        ;; Same string always produces same checksum
        (= (funcall 'neovm--test-fletcher16 (string-to-unibyte "test"))
           (funcall 'neovm--test-fletcher16 (string-to-unibyte "test")))
        ;; Different strings produce different checksums (usually)
        (not (= (funcall 'neovm--test-fletcher16 (string-to-unibyte "hello"))
                (funcall 'neovm--test-fletcher16 (string-to-unibyte "world"))))
        ;; Adler-32
        (funcall 'neovm--test-adler32 (string-to-unibyte ""))
        (funcall 'neovm--test-adler32 (string-to-unibyte "a"))
        (funcall 'neovm--test-adler32 (string-to-unibyte "Wikipedia"))
        ;; CRC-8
        (funcall 'neovm--test-crc8 (string-to-unibyte ""))
        (funcall 'neovm--test-crc8 (string-to-unibyte "A"))
        (funcall 'neovm--test-crc8 (string-to-unibyte "123456789"))
        ;; Verify determinism
        (= (funcall 'neovm--test-crc8 (string-to-unibyte "xyz"))
           (funcall 'neovm--test-crc8 (string-to-unibyte "xyz")))
        ;; Compare all three checksums on same input
        (let ((input (string-to-unibyte "checksum test")))
          (list (funcall 'neovm--test-fletcher16 input)
                (funcall 'neovm--test-adler32 input)
                (funcall 'neovm--test-crc8 input))))
    (fmakunbound 'neovm--test-fletcher16)
    (fmakunbound 'neovm--test-adler32)
    (fmakunbound 'neovm--test-crc8)))"#;
    let expect = expect_test::expect![[
        r#""OK (0 24929 8279 6752 t t 1 6422626 300286872 0 192 244 t (21816 608044340 76))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hex string conversion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bytevector_hex_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-bytes-to-hex
    (lambda (s)
      "Convert unibyte string to hex string representation."
      (let ((hex-chars "0123456789abcdef")
            (result nil))
        (dotimes (i (length s))
          (let ((b (aref s i)))
            (setq result
                  (cons (aref hex-chars (logand b 15))
                        (cons (aref hex-chars (logand (ash b -4) 15))
                              result)))))
        (concat (nreverse result)))))

  (fset 'neovm--test-hex-to-bytes
    (lambda (hex-str)
      "Convert hex string to unibyte string."
      (let ((result nil)
            (i 0)
            (len (length hex-str))
            (hex-val (lambda (c)
                       (cond
                         ((and (>= c ?0) (<= c ?9)) (- c ?0))
                         ((and (>= c ?a) (<= c ?f)) (+ 10 (- c ?a)))
                         ((and (>= c ?A) (<= c ?F)) (+ 10 (- c ?A)))
                         (t 0)))))
        (while (< (1+ i) len)
          (let ((hi (funcall hex-val (aref hex-str i)))
                (lo (funcall hex-val (aref hex-str (1+ i)))))
            (setq result (cons (+ (ash hi 4) lo) result)))
          (setq i (+ i 2)))
        (apply #'unibyte-string (nreverse result)))))

  (fset 'neovm--test-hex-dump
    (lambda (s width)
      "Create a hex dump with offset, hex bytes, and ASCII representation."
      (let ((lines nil)
            (i 0)
            (len (length s)))
        (while (< i len)
          (let ((hex-part nil)
                (ascii-part nil)
                (end (min (+ i width) len)))
            (let ((j i))
              (while (< j end)
                (let ((b (aref s j)))
                  (setq hex-part
                        (cons (funcall 'neovm--test-bytes-to-hex
                                       (unibyte-string b))
                              hex-part))
                  (setq ascii-part
                        (cons (if (and (>= b 32) (<= b 126))
                                  (char-to-string b)
                                ".")
                              ascii-part)))
                (setq j (1+ j))))
            (setq lines
                  (cons (list i
                              (mapconcat #'identity (nreverse hex-part) " ")
                              (apply #'concat (nreverse ascii-part)))
                        lines)))
          (setq i (+ i width)))
        (nreverse lines))))

  (unwind-protect
      (list
        ;; Basic hex conversion
        (funcall 'neovm--test-bytes-to-hex (unibyte-string 0))
        (funcall 'neovm--test-bytes-to-hex (unibyte-string 255))
        (funcall 'neovm--test-bytes-to-hex (unibyte-string 171 205))
        (funcall 'neovm--test-bytes-to-hex (string-to-unibyte "Hello"))
        ;; Hex to bytes
        (let ((bytes (funcall 'neovm--test-hex-to-bytes "48656c6c6f")))
          (list bytes (string= bytes (string-to-unibyte "Hello"))))
        ;; Roundtrip: bytes -> hex -> bytes
        (let* ((original (unibyte-string 0 127 128 255 42 99))
               (hex (funcall 'neovm--test-bytes-to-hex original))
               (restored (funcall 'neovm--test-hex-to-bytes hex)))
          (list hex (string= original restored)))
        ;; Empty string roundtrip
        (funcall 'neovm--test-bytes-to-hex (string-to-unibyte ""))
        ;; Hex dump
        (funcall 'neovm--test-hex-dump
                 (string-to-unibyte "Hello, World!")
                 8)
        ;; Roundtrip on all single-byte values 0-15
        (let ((all-match t))
          (dotimes (i 16)
            (let* ((b (unibyte-string i))
                   (hex (funcall 'neovm--test-bytes-to-hex b))
                   (back (funcall 'neovm--test-hex-to-bytes hex)))
              (unless (string= b back)
                (setq all-match nil))))
          all-match))
    (fmakunbound 'neovm--test-bytes-to-hex)
    (fmakunbound 'neovm--test-hex-to-bytes)
    (fmakunbound 'neovm--test-hex-dump)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"00\" \"ff\" \"abcd\" \"48656c6c6f\" (\"Hello\" t) (\"007f80ff2a63\" t) \"\" ((0 \"48 65 6c 6c 6f 2c 20 57\" \"Hello, W\") (8 \"6f 72 6c 64 21\" \"orld!\")) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Base64-like encoding/decoding on byte vectors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bytevector_base64_like() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-b64-encode
    (lambda (input)
      "Base64-encode unibyte string INPUT."
      (let ((table "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/")
            (result nil)
            (i 0)
            (len (length input)))
        ;; Process 3-byte groups
        (while (<= (+ i 2) (1- len))
          (let* ((b0 (aref input i))
                 (b1 (aref input (1+ i)))
                 (b2 (aref input (+ i 2)))
                 (n (+ (ash b0 16) (ash b1 8) b2)))
            (setq result
                  (cons (aref table (logand n 63))
                        (cons (aref table (logand (ash n -6) 63))
                              (cons (aref table (logand (ash n -12) 63))
                                    (cons (aref table (logand (ash n -18) 63))
                                          result)))))
            (setq i (+ i 3))))
        (let ((remaining (- len i)))
          (cond
            ((= remaining 2)
             (let* ((b0 (aref input i))
                    (b1 (aref input (1+ i)))
                    (n (+ (ash b0 16) (ash b1 8))))
               (setq result
                     (cons ?=
                           (cons (aref table (logand (ash n -6) 63))
                                 (cons (aref table (logand (ash n -12) 63))
                                       (cons (aref table (logand (ash n -18) 63))
                                             result)))))))
            ((= remaining 1)
             (let* ((b0 (aref input i))
                    (n (ash b0 16)))
               (setq result
                     (cons ?=
                           (cons ?=
                                 (cons (aref table (logand (ash n -12) 63))
                                       (cons (aref table (logand (ash n -18) 63))
                                             result)))))))))
        (concat (nreverse result)))))

  (fset 'neovm--test-b64-decode
    (lambda (encoded)
      "Base64-decode string ENCODED back to unibyte string."
      (let ((table (make-hash-table))
            (alphabet "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"))
        (dotimes (i (length alphabet))
          (puthash (aref alphabet i) i table))
        (let ((result nil)
              (i 0)
              (len (length encoded)))
          ;; Process 4-char groups
          (while (< (+ i 3) len)
            (let* ((c0 (gethash (aref encoded i) table 0))
                   (c1 (gethash (aref encoded (1+ i)) table 0))
                   (c2-char (aref encoded (+ i 2)))
                   (c3-char (aref encoded (+ i 3)))
                   (c2 (if (= c2-char ?=) 0 (gethash c2-char table 0)))
                   (c3 (if (= c3-char ?=) 0 (gethash c3-char table 0)))
                   (n (+ (ash c0 18) (ash c1 12) (ash c2 6) c3)))
              (setq result (cons (logand (ash n -16) 255) result))
              (unless (= c2-char ?=)
                (setq result (cons (logand (ash n -8) 255) result)))
              (unless (= c3-char ?=)
                (setq result (cons (logand n 255) result))))
            (setq i (+ i 4)))
          (apply #'unibyte-string (nreverse result))))))

  (unwind-protect
      (list
        ;; Standard test vectors
        (funcall 'neovm--test-b64-encode (string-to-unibyte "Man"))
        (funcall 'neovm--test-b64-encode (string-to-unibyte "Ma"))
        (funcall 'neovm--test-b64-encode (string-to-unibyte "M"))
        (funcall 'neovm--test-b64-encode (string-to-unibyte ""))
        (funcall 'neovm--test-b64-encode (string-to-unibyte "Hello, World!"))
        ;; Roundtrip tests
        (let ((test-strings '("" "a" "ab" "abc" "abcd" "Hello!" "The quick brown fox")))
          (mapcar (lambda (s)
                    (let* ((input (string-to-unibyte s))
                           (encoded (funcall 'neovm--test-b64-encode input))
                           (decoded (funcall 'neovm--test-b64-decode encoded)))
                      (list s encoded (string= input decoded))))
                  test-strings))
        ;; Binary data roundtrip
        (let* ((binary (unibyte-string 0 1 127 128 254 255))
               (encoded (funcall 'neovm--test-b64-encode binary))
               (decoded (funcall 'neovm--test-b64-decode encoded)))
          (list encoded (string= binary decoded))))
    (fmakunbound 'neovm--test-b64-encode)
    (fmakunbound 'neovm--test-b64-decode)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"TWFu\" \"TWE=\" \"TQ==\" \"\" \"SGVsbG8sIFdvcmxkIQ==\" ((\"\" \"\" t) (\"a\" \"YQ==\" t) (\"ab\" \"YWI=\" t) (\"abc\" \"YWJj\" t) (\"abcd\" \"YWJjZA==\" t) (\"Hello!\" \"SGVsbG8h\" t) (\"The quick brown fox\" \"VGhlIHF1aWNrIGJyb3duIGZveA==\" t)) (\"AAF/gP7/\" t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Byte-level pattern search and replace
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bytevector_pattern_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-byte-find
    (lambda (haystack needle)
      "Find first occurrence of NEEDLE bytes in HAYSTACK. Return index or nil."
      (let ((hlen (length haystack))
            (nlen (length needle))
            (found nil)
            (i 0))
        (when (> nlen 0)
          (while (and (not found) (<= (+ i nlen) hlen))
            (let ((match t) (j 0))
              (while (and match (< j nlen))
                (unless (= (aref haystack (+ i j)) (aref needle j))
                  (setq match nil))
                (setq j (1+ j)))
              (if match
                  (setq found i)
                (setq i (1+ i))))))
        found)))

  (fset 'neovm--test-byte-find-all
    (lambda (haystack needle)
      "Find all non-overlapping occurrences of NEEDLE in HAYSTACK."
      (let ((hlen (length haystack))
            (nlen (length needle))
            (positions nil)
            (i 0))
        (when (> nlen 0)
          (while (<= (+ i nlen) hlen)
            (let ((match t) (j 0))
              (while (and match (< j nlen))
                (unless (= (aref haystack (+ i j)) (aref needle j))
                  (setq match nil))
                (setq j (1+ j)))
              (if match
                  (progn (setq positions (cons i positions))
                         (setq i (+ i nlen)))
                (setq i (1+ i))))))
        (nreverse positions))))

  (fset 'neovm--test-byte-replace
    (lambda (data old-bytes new-bytes)
      "Replace all occurrences of OLD-BYTES with NEW-BYTES in DATA."
      (let ((positions (funcall 'neovm--test-byte-find-all data old-bytes))
            (olen (length old-bytes))
            (parts nil)
            (prev 0))
        (dolist (pos positions)
          (when (> pos prev)
            (setq parts (cons (substring data prev pos) parts)))
          (setq parts (cons new-bytes parts))
          (setq prev (+ pos olen)))
        (when (< prev (length data))
          (setq parts (cons (substring data prev) parts)))
        (apply #'concat (nreverse parts)))))

  (unwind-protect
      (let ((data (string-to-unibyte "AABBAABBCCAABB")))
        (list
          ;; Find first occurrence
          (funcall 'neovm--test-byte-find data (string-to-unibyte "BB"))
          (funcall 'neovm--test-byte-find data (string-to-unibyte "CC"))
          (funcall 'neovm--test-byte-find data (string-to-unibyte "ZZ"))
          ;; Find all occurrences
          (funcall 'neovm--test-byte-find-all data (string-to-unibyte "AA"))
          (funcall 'neovm--test-byte-find-all data (string-to-unibyte "BB"))
          ;; Replace
          (funcall 'neovm--test-byte-replace
                   data
                   (string-to-unibyte "AA")
                   (string-to-unibyte "XX"))
          (funcall 'neovm--test-byte-replace
                   data
                   (string-to-unibyte "BB")
                   (string-to-unibyte "Y"))
          ;; Replace with different length
          (funcall 'neovm--test-byte-replace
                   (string-to-unibyte "aXbXcXd")
                   (string-to-unibyte "X")
                   (string-to-unibyte "---"))
          ;; No match: original returned
          (funcall 'neovm--test-byte-replace
                   data
                   (string-to-unibyte "ZZZ")
                   (string-to-unibyte "W"))
          ;; Edge: pattern at start and end
          (funcall 'neovm--test-byte-find
                   (string-to-unibyte "ABC") (string-to-unibyte "A"))
          (funcall 'neovm--test-byte-find
                   (string-to-unibyte "ABC") (string-to-unibyte "C"))))
    (fmakunbound 'neovm--test-byte-find)
    (fmakunbound 'neovm--test-byte-find-all)
    (fmakunbound 'neovm--test-byte-replace)))"#;
    let expect = expect_test::expect![[
        r#""OK (2 8 nil (0 4 10) (2 6 12) \"XXBBXXBBCCXXBB\" \"AAYAAYCCAAY\" \"a---b---c---d\" \"AABBAABBCCAABB\" 0 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Byte histogram and frequency analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bytevector_histogram() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-byte-histogram
    (lambda (s)
      "Compute frequency of each byte value in unibyte string S.
       Returns sorted alist of (byte . count) for non-zero counts."
      (let ((counts (make-vector 256 0)))
        (dotimes (i (length s))
          (let ((b (aref s i)))
            (aset counts b (1+ (aref counts b)))))
        (let ((result nil))
          (dotimes (i 256)
            (when (> (aref counts i) 0)
              (setq result (cons (cons i (aref counts i)) result))))
          (sort (nreverse result)
                (lambda (a b) (> (cdr a) (cdr b))))))))

  (fset 'neovm--test-byte-entropy
    (lambda (s)
      "Estimate Shannon entropy of byte distribution in S (returns integer approx * 100)."
      (let* ((len (length s))
             (hist (funcall 'neovm--test-byte-histogram s))
             (entropy 0))
        (when (> len 0)
          (dolist (pair hist)
            (let* ((count (cdr pair))
                   (freq (/ (float count) len))
                   ;; Approximate -p*log2(p) using -p*log(p)/log(2)
                   ;; log(p) = log(count/len) = log(count) - log(len)
                   (log-freq (- (log (float count)) (log (float len))))
                   (contribution (* -1.0 freq (/ log-freq (log 2.0)))))
              (setq entropy (+ entropy contribution)))))
        ;; Return integer * 100 for easy comparison
        (round (* entropy 100)))))

  (unwind-protect
      (list
        ;; Histogram of simple string
        (funcall 'neovm--test-byte-histogram (string-to-unibyte "aabbbcccc"))
        ;; Histogram of binary data
        (funcall 'neovm--test-byte-histogram (unibyte-string 0 0 0 1 1 2))
        ;; Empty string
        (funcall 'neovm--test-byte-histogram (string-to-unibyte ""))
        ;; Single byte repeated
        (funcall 'neovm--test-byte-histogram (make-string 10 ?X))
        ;; Entropy: uniform distribution (high entropy)
        (funcall 'neovm--test-byte-entropy
                 (apply #'unibyte-string
                        (let ((lst nil))
                          (dotimes (i 64)
                            (setq lst (cons (% i 16) lst)))
                          (nreverse lst))))
        ;; Entropy: all same byte (zero entropy)
        (funcall 'neovm--test-byte-entropy (make-string 100 ?A))
        ;; Entropy: two equally frequent bytes
        (funcall 'neovm--test-byte-entropy
                 (apply #'unibyte-string
                        (let ((lst nil))
                          (dotimes (i 100)
                            (setq lst (cons (if (= (% i 2) 0) 0 1) lst)))
                          (nreverse lst))))
        ;; Verify histogram counts sum to string length
        (let* ((s (string-to-unibyte "the quick brown fox"))
               (hist (funcall 'neovm--test-byte-histogram s))
               (total 0))
          (dolist (pair hist) (setq total (+ total (cdr pair))))
          (= total (length s))))
    (fmakunbound 'neovm--test-byte-histogram)
    (fmakunbound 'neovm--test-byte-entropy)))"#;
    let expect = expect_test::expect![[
        r#""OK (((99 . 4) (98 . 3) (97 . 2)) ((0 . 3) (1 . 2) (2 . 1)) nil ((88 . 10)) 400 0 100 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

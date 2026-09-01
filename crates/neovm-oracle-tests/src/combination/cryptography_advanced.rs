//! Oracle parity tests for advanced cryptography patterns in pure Elisp.
//!
//! Covers: Caesar cipher with arbitrary shift, Vigenere cipher,
//! substitution cipher with key, XOR encryption/decryption roundtrip,
//! DJB2 and FNV-1a hash functions, HMAC construction, base64
//! encode/decode, and hex encode/decode.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// 1. Caesar cipher with arbitrary shift and roundtrip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_crypto_adv_caesar_cipher() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((caesar-encrypt
         (lambda (text shift)
           (let ((result nil))
             (dotimes (i (length text))
               (let ((ch (aref text i)))
                 (cond
                  ((and (>= ch ?a) (<= ch ?z))
                   (setq result (cons (+ ?a (% (+ (- ch ?a) shift) 26)) result)))
                  ((and (>= ch ?A) (<= ch ?Z))
                   (setq result (cons (+ ?A (% (+ (- ch ?A) shift) 26)) result)))
                  (t (setq result (cons ch result))))))
             (concat (nreverse result)))))
        (caesar-decrypt
         (lambda (text shift)
           (let ((result nil))
             (dotimes (i (length text))
               (let ((ch (aref text i)))
                 (cond
                  ((and (>= ch ?a) (<= ch ?z))
                   (setq result (cons (+ ?a (% (+ (- ch ?a) (- 26 shift)) 26)) result)))
                  ((and (>= ch ?A) (<= ch ?Z))
                   (setq result (cons (+ ?A (% (+ (- ch ?A) (- 26 shift)) 26)) result)))
                  (t (setq result (cons ch result))))))
             (concat (nreverse result))))))
  (list
   ;; Basic encrypt/decrypt
   (funcall caesar-encrypt "Hello World" 3)
   (funcall caesar-decrypt "Khoor Zruog" 3)
   ;; Roundtrip with various shifts
   (equal "attack at dawn"
          (funcall caesar-decrypt (funcall caesar-encrypt "attack at dawn" 13) 13))
   (equal "Zebra Zoo"
          (funcall caesar-decrypt (funcall caesar-encrypt "Zebra Zoo" 7) 7))
   ;; Shift of 0 is identity
   (funcall caesar-encrypt "unchanged" 0)
   ;; Shift of 26 is identity
   (equal "abc" (funcall caesar-encrypt "abc" 26))
   ;; ROT13 roundtrip
   (equal "secret" (funcall caesar-encrypt (funcall caesar-encrypt "secret" 13) 13))
   ;; Preserves non-alpha characters
   (funcall caesar-encrypt "Hello, World! 123" 5)
   ;; All shifts produce different text for non-trivial input
   (let ((results nil))
     (dotimes (s 5)
       (setq results (cons (funcall caesar-encrypt "abc" (1+ s)) results)))
     (nreverse results))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Khoor Zruog\" \"Hello World\" t t \"unchanged\" t t \"Mjqqt, Btwqi! 123\" (\"bcd\" \"cde\" \"def\" \"efg\" \"fgh\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 2. Vigenere cipher
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_crypto_adv_vigenere() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((vigenere-process
         (lambda (text key encrypt-p)
           (let ((result nil)
                 (key-lower (downcase key))
                 (ki 0)
                 (key-len (length key)))
             (dotimes (i (length text))
               (let ((ch (aref text i)))
                 (cond
                  ((and (>= ch ?a) (<= ch ?z))
                   (let ((shift (- (aref key-lower (% ki key-len)) ?a)))
                     (when (not encrypt-p) (setq shift (- 26 shift)))
                     (setq result (cons (+ ?a (% (+ (- ch ?a) shift) 26)) result))
                     (setq ki (1+ ki))))
                  ((and (>= ch ?A) (<= ch ?Z))
                   (let ((shift (- (aref key-lower (% ki key-len)) ?a)))
                     (when (not encrypt-p) (setq shift (- 26 shift)))
                     (setq result (cons (+ ?A (% (+ (- ch ?A) shift) 26)) result))
                     (setq ki (1+ ki))))
                  (t (setq result (cons ch result))))))
             (concat (nreverse result))))))
  (let ((encrypt (lambda (t k) (funcall vigenere-process t k t)))
        (decrypt (lambda (t k) (funcall vigenere-process t k nil))))
    (list
     ;; Classic example
     (funcall encrypt "ATTACKATDAWN" "LEMON")
     ;; Decrypt it back
     (funcall decrypt (funcall encrypt "ATTACKATDAWN" "LEMON") "LEMON")
     ;; Roundtrip with mixed case
     (equal "Hello World"
            (funcall decrypt (funcall encrypt "Hello World" "key") "key"))
     ;; Key "a" (shift 0) is identity
     (funcall encrypt "unchanged" "a")
     ;; Different keys produce different ciphertexts
     (let ((c1 (funcall encrypt "same text" "alpha"))
           (c2 (funcall encrypt "same text" "beta")))
       (list (not (equal c1 c2)) c1 c2))
     ;; Preserves spaces and punctuation
     (funcall encrypt "Hello, World!" "secret"))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"LXFOPVEFRNHR\" \"ATTACKATDAWN\" t \"unchanged\" (t \"slbl teii\" \"tefe uiqt\") \"Zincs, Pgvnu!\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 3. Substitution cipher with keyed alphabet
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_crypto_adv_substitution_cipher() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((make-cipher-alphabet
         ;; Build substitution alphabet from keyword:
         ;; keyword letters first (unique), then remaining letters
         (lambda (keyword)
           (let ((seen (make-vector 26 nil))
                 (result nil))
             ;; Add keyword chars
             (dotimes (i (length keyword))
               (let ((idx (- (aref (downcase keyword) i) ?a)))
                 (when (and (>= idx 0) (< idx 26) (not (aref seen idx)))
                   (aset seen idx t)
                   (setq result (cons (+ ?a idx) result)))))
             ;; Add remaining
             (dotimes (i 26)
               (unless (aref seen i)
                 (setq result (cons (+ ?a i) result))))
             (concat (nreverse result))))))
  (let ((sub-encrypt
         (lambda (text cipher-alpha)
           (let ((result nil))
             (dotimes (i (length text))
               (let ((ch (aref text i)))
                 (cond
                  ((and (>= ch ?a) (<= ch ?z))
                   (setq result (cons (aref cipher-alpha (- ch ?a)) result)))
                  ((and (>= ch ?A) (<= ch ?Z))
                   (setq result (cons (- (+ ?A (- (aref cipher-alpha (- ch ?A)) ?a)))
                                      result)))
                  (t (setq result (cons ch result))))))
             (concat (nreverse result)))))
        (sub-decrypt
         (lambda (text cipher-alpha)
           (let ((reverse-alpha (make-string 26 ?a)))
             (dotimes (i 26)
               (aset reverse-alpha (- (aref cipher-alpha i) ?a) (+ ?a i)))
             (let ((result nil))
               (dotimes (i (length text))
                 (let ((ch (aref text i)))
                   (cond
                    ((and (>= ch ?a) (<= ch ?z))
                     (setq result (cons (aref reverse-alpha (- ch ?a)) result)))
                    (t (setq result (cons ch result))))))
               (concat (nreverse result)))))))
    (let ((alpha (funcall make-cipher-alphabet "zebra")))
      (list
       ;; The generated cipher alphabet
       alpha
       ;; Encrypt and decrypt
       (let ((encrypted (funcall sub-encrypt "hello world" alpha)))
         (list encrypted
               (funcall sub-decrypt encrypted alpha)
               (equal "hello world"
                      (funcall sub-decrypt encrypted alpha))))
       ;; Different keyword, different result
       (let ((alpha2 (funcall make-cipher-alphabet "python")))
         (not (equal (funcall sub-encrypt "test" alpha)
                     (funcall sub-encrypt "test" alpha2))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"zebracdfghijklmnopqstuvwxy\" (\"fajjm vmpjr\" \"hello world\" t) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 4. XOR encryption with multi-byte key and stream properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_crypto_adv_xor_stream() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((xor-crypt
         (lambda (data key)
           (let ((result nil)
                 (klen (length key)))
             (dotimes (i (length data))
               (setq result (cons (logxor (aref data i)
                                          (aref key (% i klen)))
                                  result)))
             (concat (nreverse result))))))
  (list
   ;; Roundtrip: xor(xor(m, k), k) = m
   (let ((msg "The quick brown fox jumps over the lazy dog"))
     (equal msg (funcall xor-crypt (funcall xor-crypt msg "secretkey") "secretkey")))
   ;; XOR with itself = all zeros
   (let ((s "hello"))
     (let ((xored (funcall xor-crypt s s)))
       (let ((all-zero t))
         (dotimes (i (length xored))
           (unless (= 0 (aref xored i))
             (setq all-zero nil)))
         all-zero)))
   ;; XOR with all-zero key = identity
   (let ((msg "test123"))
     (equal msg (funcall xor-crypt msg (make-string 3 0))))
   ;; Single byte key = simple XOR
   (let ((msg "AB"))
     (list (aref (funcall xor-crypt msg (string 255)) 0)
           (aref (funcall xor-crypt msg (string 255)) 1)))
   ;; Different keys produce different ciphertexts
   (not (equal (funcall xor-crypt "data" "key1")
               (funcall xor-crypt "data" "key2")))
   ;; Ciphertext length = plaintext length
   (= (length (funcall xor-crypt "abcdefgh" "xyz"))
      (length "abcdefgh"))
   ;; Encrypt empty string
   (funcall xor-crypt "" "key")))"#;
    let expect = expect_test::expect![[r#""OK (t t t (190 189) t t \"\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 5. DJB2 and FNV-1a hash functions with collision analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_crypto_adv_hash_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((djb2
         (lambda (str)
           (let ((h 5381) (mask (1- (ash 1 28))))
             (dotimes (i (length str))
               (setq h (logand (+ (* h 33) (aref str i)) mask)))
             h)))
        (fnv1a
         (lambda (str)
           ;; FNV-1a: hash = (hash XOR byte) * FNV_prime
           ;; Using 28-bit version: offset=0x01000193 is too big, use simpler params
           (let ((h 2166136261)  ;; FNV offset basis (truncated to 28 bits)
                 (prime 16777619)
                 (mask (1- (ash 1 28))))
             (setq h (logand h mask))
             (dotimes (i (length str))
               (setq h (logand (* (logxor h (aref str i)) prime) mask)))
             h))))
  (let ((inputs '("" "a" "b" "ab" "ba" "abc" "hello" "world"
                   "The quick brown fox" "test" "test1" "test2")))
    (let ((djb2-hashes (mapcar djb2 inputs))
          (fnv1a-hashes (mapcar fnv1a inputs)))
      (list
       ;; All hashes are non-negative
       (mapcar (lambda (h) (>= h 0)) djb2-hashes)
       (mapcar (lambda (h) (>= h 0)) fnv1a-hashes)
       ;; Deterministic: same input -> same hash
       (= (funcall djb2 "hello") (funcall djb2 "hello"))
       (= (funcall fnv1a "hello") (funcall fnv1a "hello"))
       ;; "ab" vs "ba" differ for both
       (not (= (funcall djb2 "ab") (funcall djb2 "ba")))
       (not (= (funcall fnv1a "ab") (funcall fnv1a "ba")))
       ;; DJB2 and FNV1a produce different hashes (different algorithms)
       (not (= (funcall djb2 "hello") (funcall fnv1a "hello")))
       ;; Actual hash values for exact parity
       djb2-hashes
       fnv1a-hashes))))"#;
    let expect = expect_test::expect![[
        r#""OK ((t t t t t t t t t t t t) (t t t t t t t t t t t t) t t t t t (5381 177670 177671 5863208 5863240 193485963 261238937 10958189 130284792 211708005 7042358 7042359) (18652613 67905836 118238693 220530122 204187340 172484875 262089899 128182419 239953890 265318885 153301180 203634037))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 6. HMAC-like construction with two different hash functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_crypto_adv_hmac_construction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((djb2
         (lambda (str)
           (let ((h 5381) (mask (1- (ash 1 28))))
             (dotimes (i (length str))
               (setq h (logand (+ (* h 33) (aref str i)) mask)))
             h)))
        (pad-key
         (lambda (key block-size pad-byte)
           (let ((result nil))
             (dotimes (i block-size)
               (let ((k (if (< i (length key)) (aref key i) 0)))
                 (setq result (cons (logxor k pad-byte) result))))
             (concat (nreverse result))))))
  (let ((hmac
         (lambda (key msg)
           (let* ((bs 16)
                  (ipad (funcall pad-key key bs #x36))
                  (opad (funcall pad-key key bs #x5c))
                  (inner (funcall djb2 (concat ipad msg))))
             (funcall djb2 (concat opad (number-to-string inner)))))))
    (list
     ;; Deterministic
     (= (funcall hmac "key" "msg") (funcall hmac "key" "msg"))
     ;; Different key -> different HMAC
     (not (= (funcall hmac "key1" "msg") (funcall hmac "key2" "msg")))
     ;; Different msg -> different HMAC
     (not (= (funcall hmac "key" "msg1") (funcall hmac "key" "msg2")))
     ;; Empty key
     (integerp (funcall hmac "" "hello"))
     ;; Empty message
     (integerp (funcall hmac "key" ""))
     ;; Non-negative
     (>= (funcall hmac "key" "msg") 0)
     ;; Actual values for parity
     (funcall hmac "secret" "hello world")
     (funcall hmac "another-key" "hello world")
     (funcall hmac "secret" "different message"))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t 264289475 181632647 106049473)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 7. Base64 encode/decode
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_crypto_adv_base64() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use Emacs built-in base64-encode-string / base64-decode-string
    let form = r#"(list
  ;; Encode known values
  (base64-encode-string "")
  (base64-encode-string "f")
  (base64-encode-string "fo")
  (base64-encode-string "foo")
  (base64-encode-string "foob")
  (base64-encode-string "fooba")
  (base64-encode-string "foobar")
  ;; Roundtrip
  (equal "Hello, World!"
         (base64-decode-string (base64-encode-string "Hello, World!")))
  (equal "The quick brown fox"
         (base64-decode-string (base64-encode-string "The quick brown fox")))
  ;; Encode with no-line-break argument
  (base64-encode-string "a]b" t)
  ;; Binary data (all byte values 0-9)
  (let ((bin (concat (number-sequence 0 9))))
    (equal bin (base64-decode-string (base64-encode-string bin))))
  ;; Padding verification: length 1 -> 4 chars with ==, length 2 -> 4 chars with =
  (length (base64-encode-string "a"))
  (length (base64-encode-string "ab"))
  (length (base64-encode-string "abc")))"#;
    let expect = expect_test::expect![[
        r#""OK (\"\" \"Zg==\" \"Zm8=\" \"Zm9v\" \"Zm9vYg==\" \"Zm9vYmE=\" \"Zm9vYmFy\" t t \"YV1i\" t 4 4 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 8. Hex encode/decode
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_crypto_adv_hex_codec() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((hex-encode
         (lambda (str)
           (let ((result nil)
                 (hex-chars "0123456789abcdef"))
             (dotimes (i (length str))
               (let ((b (aref str i)))
                 (setq result (cons (aref hex-chars (logand (ash b -4) 15)) result))
                 (setq result (cons (aref hex-chars (logand b 15)) result))))
             (concat (nreverse result)))))
        (hex-decode
         (lambda (hex)
           (let ((result nil)
                 (i 0))
             (while (< i (length hex))
               (let* ((hi-ch (aref hex i))
                      (lo-ch (aref hex (1+ i)))
                      (hex-val
                       (lambda (c)
                         (cond
                          ((and (>= c ?0) (<= c ?9)) (- c ?0))
                          ((and (>= c ?a) (<= c ?f)) (+ 10 (- c ?a)))
                          ((and (>= c ?A) (<= c ?F)) (+ 10 (- c ?A)))
                          (t 0)))))
                 (setq result (cons (logior (ash (funcall hex-val hi-ch) 4)
                                            (funcall hex-val lo-ch))
                                    result))
                 (setq i (+ i 2))))
             (concat (nreverse result))))))
  (list
   ;; Encode known strings
   (funcall hex-encode "")
   (funcall hex-encode "A")
   (funcall hex-encode "AB")
   (funcall hex-encode "Hello")
   ;; Hex of known bytes
   (funcall hex-encode (string 0 255 127 128))
   ;; Roundtrip
   (equal "Hello, World!"
          (funcall hex-decode (funcall hex-encode "Hello, World!")))
   (equal "test data 123"
          (funcall hex-decode (funcall hex-encode "test data 123")))
   ;; Empty roundtrip
   (equal "" (funcall hex-decode (funcall hex-encode "")))
   ;; Encoded length is always 2x input
   (= (length (funcall hex-encode "abcde")) 10)
   ;; Decode specific hex values
   (funcall hex-decode "48656c6c6f")  ;; "Hello"
   (funcall hex-decode "00ff7f80")))"#;
    let expect = expect_test::expect![[
        r#""OK (\"\" \"41\" \"4142\" \"48656c6c6f\" \"00ff7f80\" t t t t \"Hello\" \"\\0ÿ\u{7f}\u{80}\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 9. Simple stream cipher with LCG-based keystream
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_crypto_adv_stream_cipher_lcg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((stream-cipher
         ;; XOR plaintext with pseudo-random keystream from LCG
         (lambda (text seed)
           (let ((state seed)
                 (result nil)
                 (lcg-a 1103515245)
                 (lcg-c 12345)
                 (lcg-m (ash 1 24)))
             (dotimes (i (length text))
               (setq state (% (+ (* lcg-a state) lcg-c) lcg-m))
               (let ((key-byte (logand (ash state -8) 255)))
                 (setq result (cons (logxor (aref text i) key-byte) result))))
             (concat (nreverse result))))))
  (list
   ;; Roundtrip: same seed decrypts
   (let ((msg "Secret message for testing"))
     (equal msg (funcall stream-cipher
                         (funcall stream-cipher msg 42) 42)))
   ;; Different seeds produce different ciphertexts
   (not (equal (funcall stream-cipher "hello" 1)
               (funcall stream-cipher "hello" 2)))
   ;; Same seed, same message -> same ciphertext (deterministic)
   (equal (funcall stream-cipher "test" 99)
          (funcall stream-cipher "test" 99))
   ;; Length preserved
   (= (length (funcall stream-cipher "abcdefgh" 7)) 8)
   ;; Empty string
   (funcall stream-cipher "" 42)
   ;; Multiple messages with same key produce unique ciphertexts
   ;; (different messages, same seed)
   (let ((c1 (funcall stream-cipher "aaa" 5))
         (c2 (funcall stream-cipher "bbb" 5)))
     (not (equal c1 c2)))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t \"\" t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

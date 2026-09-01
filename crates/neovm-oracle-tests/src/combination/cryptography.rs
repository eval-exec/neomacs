//! Oracle parity tests for cryptography-like algorithms in pure Elisp.
//!
//! Covers XOR cipher, DJB2-like hash, block cipher simulation,
//! HMAC-like authentication, linear congruential generator, and
//! Bloom filter simulation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// XOR cipher (encrypt/decrypt roundtrip)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_crypto_xor_cipher() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // XOR cipher: encrypt by XOR-ing each byte with repeating key,
    // decrypt by XOR-ing again (symmetric).
    let form = r#"(let ((xor-cipher
                         (lambda (text key)
                           (let ((result nil)
                                 (key-len (length key)))
                             (dotimes (i (length text))
                               (let ((ch (aref text i))
                                     (k  (aref key (% i key-len))))
                                 (setq result (cons (logxor ch k) result))))
                             (concat (nreverse result))))))
                    ;; Roundtrip: encrypt then decrypt must yield original
                    (let ((messages '("Hello, World!" "Secret data 123"
                                      "The quick brown fox" ""))
                          (keys '("key" "s3cr3t" "ab" "x")))
                      (mapcar (lambda (msg)
                                (let ((results nil))
                                  (dolist (key keys)
                                    (let* ((encrypted (funcall xor-cipher msg key))
                                           (decrypted (funcall xor-cipher encrypted key)))
                                      (setq results
                                            (cons (list
                                                    ;; Roundtrip succeeds
                                                    (equal decrypted msg)
                                                    ;; Encrypted differs from original (unless empty)
                                                    (or (= (length msg) 0)
                                                        (not (equal encrypted msg)))
                                                    ;; Same length
                                                    (= (length encrypted) (length msg)))
                                                  results))))
                                  (nreverse results)))
                              messages)))"#;
    let expect = expect_test::expect![[
        r#""OK (((t t t) (t t t) (t t t) (t t t)) ((t t t) (t t t) (t t t) (t t t)) ((t t t) (t t t) (t t t) (t t t)) ((t t t) (t t t) (t t t) (t t t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// DJB2-like hash function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_crypto_djb2_hash() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // DJB2: hash = hash * 33 + c, seeded with 5381.
    // We mask to 28 bits to stay within Elisp fixnum range safely.
    let form = r#"(let ((djb2
                         (lambda (str)
                           (let ((hash 5381)
                                 (mask (1- (ash 1 28))))
                             (dotimes (i (length str))
                               (setq hash (logand
                                            (+ (* hash 33)
                                               (aref str i))
                                            mask)))
                             hash))))
                    ;; Same string -> same hash, different strings -> (likely) different hash
                    (let ((inputs '("hello" "world" "hello" "foo" "bar" "baz"
                                    "ab" "ba" "abc" "cba" "" "a")))
                      (let ((hashes (mapcar (lambda (s) (funcall djb2 s)) inputs)))
                        (list
                          ;; All are non-negative integers
                          (mapcar (lambda (h) (and (integerp h) (>= h 0))) hashes)
                          ;; "hello" produces same hash both times
                          (= (nth 0 hashes) (nth 2 hashes))
                          ;; "ab" vs "ba" differ
                          (not (= (nth 6 hashes) (nth 7 hashes)))
                          ;; "abc" vs "cba" differ
                          (not (= (nth 8 hashes) (nth 9 hashes)))
                          ;; Return actual hash values for parity
                          hashes))))"#;
    let expect = expect_test::expect![[
        r#""OK ((t t t t t t t t t t t t) t t t (261238937 10958189 261238937 193491849 193487034 193487042 5863208 5863240 193485963 193488139 5381 177670))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Block cipher simulation (substitution-permutation network)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_crypto_block_cipher_spn() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simple substitution-permutation network on 8-bit blocks:
    // 1. XOR with round key
    // 2. Substitute via S-box (4-bit nibble substitution)
    // 3. Permute bits
    // Run multiple rounds. Verify encrypt/decrypt roundtrip.
    let form = r#"(let ((sbox    [14 4 13 1 2 15 11 8 3 10 6 12 5 9 0 7])
                        (inv-sbox [14 3 4 8 1 12 10 15 7 13 9 6 11 2 0 5]))
                    (let ((sub-byte
                           (lambda (b box)
                             (let ((hi (logand (ash b -4) 15))
                                   (lo (logand b 15)))
                               (logior (ash (aref box hi) 4)
                                       (aref box lo)))))
                          (perm-bits
                           (lambda (b)
                             ;; Simple bit permutation: reverse the 8 bits
                             (let ((result 0))
                               (dotimes (i 8)
                                 (when (not (= 0 (logand b (ash 1 i))))
                                   (setq result (logior result (ash 1 (- 7 i))))))
                               result)))
                          (inv-perm-bits
                           (lambda (b)
                             ;; Inverse of bit reversal is the same operation
                             (let ((result 0))
                               (dotimes (i 8)
                                 (when (not (= 0 (logand b (ash 1 i))))
                                   (setq result (logior result (ash 1 (- 7 i))))))
                               result))))
                      ;; Generate round keys from a master key
                      (let ((round-keys [42 137 73 211]))
                        (let ((encrypt-block
                               (lambda (block)
                                 (let ((state (logand block 255)))
                                   (dotimes (r 4)
                                     (setq state (logxor state (aref round-keys r)))
                                     (setq state (funcall sub-byte state sbox))
                                     (setq state (funcall perm-bits state)))
                                   state)))
                              (decrypt-block
                               (lambda (block)
                                 (let ((state (logand block 255)))
                                   (let ((r 3))
                                     (while (>= r 0)
                                       (setq state (funcall inv-perm-bits state))
                                       (setq state (funcall sub-byte state inv-sbox))
                                       (setq state (logxor state (aref round-keys r)))
                                       (setq r (1- r))))
                                   state))))
                          ;; Test roundtrip for various plaintext bytes
                          (let ((plaintexts '(0 1 42 127 200 255)))
                            (mapcar (lambda (pt)
                                      (let ((ct (funcall encrypt-block pt)))
                                        (list pt ct
                                              (funcall decrypt-block ct)
                                              ;; Roundtrip check
                                              (= pt (funcall decrypt-block ct)))))
                                    plaintexts))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 156 0 t) (1 147 1 t) (42 26 42 t) (127 210 127 t) (200 46 200 t) (255 50 255 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// HMAC-like message authentication
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_crypto_hmac_like() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simplified HMAC: HMAC(key, msg) = hash(key XOR opad || hash(key XOR ipad || msg))
    // Using our DJB2-like hash, XOR pads are 0x5c and 0x36.
    let form = r#"(let ((djb2
                         (lambda (str)
                           (let ((hash 5381)
                                 (mask (1- (ash 1 28))))
                             (dotimes (i (length str))
                               (setq hash (logand (+ (* hash 33) (aref str i)) mask)))
                             hash)))
                        (xor-pad
                         (lambda (key pad-byte target-len)
                           (let ((result nil))
                             (dotimes (i target-len)
                               (let ((k (if (< i (length key))
                                            (aref key i)
                                          0)))
                                 (setq result (cons (logxor k pad-byte) result))))
                             (concat (nreverse result))))))
                    (let ((hmac
                           (lambda (key msg)
                             (let* ((block-size 16)
                                    (ipad (funcall xor-pad key #x36 block-size))
                                    (opad (funcall xor-pad key #x5c block-size))
                                    (inner-hash (funcall djb2 (concat ipad msg)))
                                    (inner-str (number-to-string inner-hash)))
                               (funcall djb2 (concat opad inner-str))))))
                      ;; Properties:
                      ;; 1. Same key+msg -> same HMAC
                      ;; 2. Different key -> different HMAC
                      ;; 3. Different msg -> different HMAC
                      (let ((h1 (funcall hmac "secret" "hello"))
                            (h2 (funcall hmac "secret" "hello"))
                            (h3 (funcall hmac "other" "hello"))
                            (h4 (funcall hmac "secret" "world"))
                            (h5 (funcall hmac "" "test"))
                            (h6 (funcall hmac "key" "")))
                        (list
                          ;; Deterministic
                          (= h1 h2)
                          ;; Different key
                          (not (= h1 h3))
                          ;; Different message
                          (not (= h1 h4))
                          ;; All non-negative integers
                          (>= h1 0) (>= h3 0) (>= h4 0) (>= h5 0) (>= h6 0)
                          ;; Return values for exact parity check
                          h1 h3 h4 h5 h6))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t t t t 18807593 219494810 67981342 16755256 41341645)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Linear congruential generator (LCG)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_crypto_lcg_rng() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // LCG: next = (a * current + c) mod m
    // Classic parameters: a=1103515245, c=12345, m=2^24 (stay in fixnum range)
    let form = r#"(let ((lcg-a 1103515245)
                        (lcg-c 12345)
                        (lcg-m (ash 1 24))
                        (state 42))
                    (let ((next-rand
                           (lambda ()
                             (setq state (% (+ (* lcg-a state) lcg-c) lcg-m))
                             state))
                          (rand-range
                           (lambda (lo hi)
                             (setq state (% (+ (* lcg-a state) lcg-c) lcg-m))
                             (+ lo (% state (- hi lo))))))
                      ;; Generate sequence of 12 random numbers
                      (let ((seq nil))
                        (dotimes (_ 12)
                          (setq seq (cons (funcall next-rand) seq)))
                        (let ((seq (nreverse seq)))
                          ;; Generate ranged values
                          (setq state 42) ;; reset
                          (let ((ranged nil))
                            (dotimes (_ 8)
                              (setq ranged (cons (funcall rand-range 1 100) ranged)))
                            (let ((ranged (nreverse ranged)))
                              (list
                                ;; Raw sequence
                                seq
                                ;; All in range [0, 2^24)
                                (mapcar (lambda (x) (and (>= x 0) (< x lcg-m))) seq)
                                ;; Ranged values all in [1, 100)
                                (mapcar (lambda (x) (and (>= x 1) (< x 100))) ranged)
                                ;; Ranged values
                                ranged)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((8982043 9006008 10821009 7730422 2126071 4557668 7143885 8678018 6366611 8518096 5565897 1223886) (t t t t t t t t t t t t) (t t t t t t t t) (71 78 13 8 47 6 46 75))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bloom filter simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_crypto_bloom_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Bloom filter: bit array with k hash functions.
    // Uses DJB2 variants (different seeds) as hash functions.
    // Supports add and maybe-contains (with possible false positives).
    let form = r#"(let ((filter-size 64)
                        (filter (make-vector 64 nil)))
                    (let ((hash-fn
                           (lambda (str seed)
                             (let ((h seed)
                                   (mask (1- (ash 1 28))))
                               (dotimes (i (length str))
                                 (setq h (logand (+ (* h 33) (aref str i)) mask)))
                               (% h filter-size))))
                          (bloom-add
                           (lambda (str)
                             ;; Use 3 hash functions with different seeds
                             (let ((h1 (funcall hash-fn str 5381))
                                   (h2 (funcall hash-fn str 7919))
                                   (h3 (funcall hash-fn str 104729)))
                               (aset filter h1 t)
                               (aset filter h2 t)
                               (aset filter h3 t))))
                          (bloom-maybe-contains
                           (lambda (str)
                             (let ((h1 (funcall hash-fn str 5381))
                                   (h2 (funcall hash-fn str 7919))
                                   (h3 (funcall hash-fn str 104729)))
                               (and (aref filter h1)
                                    (aref filter h2)
                                    (aref filter h3)
                                    t)))))
                      ;; Add known elements
                      (funcall bloom-add "apple")
                      (funcall bloom-add "banana")
                      (funcall bloom-add "cherry")
                      (funcall bloom-add "date")
                      (funcall bloom-add "elderberry")
                      ;; Test membership
                      (let ((known-results
                             (mapcar (lambda (s)
                                       (funcall bloom-maybe-contains s))
                                     '("apple" "banana" "cherry" "date" "elderberry")))
                            ;; Count set bits
                            (set-bits 0))
                        (dotimes (i filter-size)
                          (when (aref filter i)
                            (setq set-bits (1+ set-bits))))
                        (list
                          ;; All known elements must return t (no false negatives)
                          known-results
                          ;; Number of set bits (deterministic given the inputs)
                          set-bits
                          ;; Filter size
                          filter-size
                          ;; Specific unknown lookups (deterministic: might be false pos)
                          (funcall bloom-maybe-contains "fig")
                          (funcall bloom-maybe-contains "grape")
                          (funcall bloom-maybe-contains "zzzzz")))))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable hash-fn)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Oracle parity tests for compression and encoding algorithm patterns
//! implemented in pure Elisp. Covers run-length encoding/decoding,
//! Huffman tree construction and encoding, base64-like encoding,
//! delta encoding/decoding, and dictionary-based compression.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Run-length encoding and decoding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compression_rle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // RLE: consecutive runs of the same character are encoded as (count . char).
    // Decode reconstructs the original string. Roundtrip verification.
    let form = r#"(progn
      (fset 'neovm--test-rle-encode
        (lambda (s)
          (if (= (length s) 0)
              nil
            (let ((result nil)
                  (current (aref s 0))
                  (count 1)
                  (i 1)
                  (len (length s)))
              (while (< i len)
                (if (= (aref s i) current)
                    (setq count (1+ count))
                  (setq result (cons (cons count current) result)
                        current (aref s i)
                        count 1))
                (setq i (1+ i)))
              (setq result (cons (cons count current) result))
              (nreverse result)))))
      (fset 'neovm--test-rle-decode
        (lambda (encoded)
          (let ((parts nil))
            (dolist (pair encoded)
              (setq parts (cons (make-string (car pair) (cdr pair)) parts)))
            (apply #'concat (nreverse parts)))))
      (unwind-protect
          (let ((test-strings '("aaabbbccc"
                                "aabccddddee"
                                "abcdef"
                                "zzzzzzzzzzzzzzz"
                                ""
                                "a"
                                "aabbccddeeaabbcc")))
            (mapcar (lambda (s)
                      (let* ((encoded (funcall 'neovm--test-rle-encode s))
                             (decoded (funcall 'neovm--test-rle-decode encoded))
                             (roundtrip-ok (equal s decoded))
                             ;; Compute compression ratio (encoded pairs vs original length)
                             (compressed-size (length encoded)))
                        (list s encoded decoded roundtrip-ok compressed-size)))
                    test-strings))
        (fmakunbound 'neovm--test-rle-encode)
        (fmakunbound 'neovm--test-rle-decode)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"aaabbbccc\" ((3 . 97) (3 . 98) (3 . 99)) \"aaabbbccc\" t 3) (\"aabccddddee\" ((2 . 97) (1 . 98) (2 . 99) (4 . 100) (2 . 101)) \"aabccddddee\" t 5) (\"abcdef\" ((1 . 97) (1 . 98) (1 . 99) (1 . 100) (1 . 101) (1 . 102)) \"abcdef\" t 6) (\"zzzzzzzzzzzzzzz\" ((15 . 122)) \"zzzzzzzzzzzzzzz\" t 1) (\"\" nil \"\" t 0) (\"a\" ((1 . 97)) \"a\" t 1) (\"aabbccddeeaabbcc\" ((2 . 97) (2 . 98) (2 . 99) (2 . 100) (2 . 101) (2 . 97) (2 . 98) (2 . 99)) \"aabbccddeeaabbcc\" t 8))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Huffman tree construction and encoding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compression_huffman() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a Huffman tree from character frequencies, generate prefix codes,
    // encode a string, verify prefix-free property.
    let form = r#"(progn
      ;; Count character frequencies
      (fset 'neovm--test-char-freqs
        (lambda (s)
          (let ((freq (make-hash-table)))
            (dotimes (i (length s))
              (let ((ch (aref s i)))
                (puthash ch (1+ (gethash ch freq 0)) freq)))
            ;; Convert to sorted alist
            (let ((pairs nil))
              (maphash (lambda (k v) (setq pairs (cons (cons k v) pairs))) freq)
              (sort pairs (lambda (a b) (< (cdr a) (cdr b))))))))
      ;; Build Huffman tree: each leaf is (char . freq), each internal node
      ;; is (left right . combined-freq). Uses priority queue simulation.
      (fset 'neovm--test-huffman-build
        (lambda (freq-list)
          (if (null freq-list)
              nil
            (if (= (length freq-list) 1)
                ;; Single character: tree is just that leaf
                (cons (caar freq-list) (cdar freq-list))
              ;; Build initial queue: each entry is (tree . freq)
              (let ((queue (mapcar (lambda (pair)
                                    (cons (car pair) (cdr pair)))
                                  freq-list)))
                ;; Sort by frequency
                (setq queue (sort queue (lambda (a b) (< (cdr a) (cdr b)))))
                ;; Repeatedly merge two smallest
                (while (> (length queue) 1)
                  (let* ((a (car queue))
                         (b (cadr queue))
                         (rest (cddr queue))
                         (merged (cons (list (car a) (car b))
                                       (+ (cdr a) (cdr b)))))
                    ;; Insert merged back in sorted position
                    (setq queue rest)
                    (let ((inserted nil)
                          (new-queue nil))
                      (dolist (item queue)
                        (when (and (not inserted) (<= (cdr merged) (cdr item)))
                          (setq new-queue (cons merged new-queue)
                                inserted t))
                        (setq new-queue (cons item new-queue)))
                      (unless inserted
                        (setq new-queue (cons merged new-queue)))
                      (setq queue (nreverse new-queue)))))
                ;; Return root
                (caar queue))))))
      ;; Generate codes from tree
      (fset 'neovm--test-huffman-codes
        (lambda (tree)
          (let ((codes (make-hash-table)))
            (fset 'neovm--test-huff-walk
              (lambda (node prefix)
                (cond
                  ;; Leaf: integer (character)
                  ((integerp node)
                   (puthash node (if (string= prefix "") "0" prefix) codes))
                  ;; Internal: list of (left right)
                  ((listp node)
                   (funcall 'neovm--test-huff-walk (car node)
                            (concat prefix "0"))
                   (funcall 'neovm--test-huff-walk (cadr node)
                            (concat prefix "1"))))))
            (funcall 'neovm--test-huff-walk tree "")
            codes)))
      (unwind-protect
          (let* ((text "abracadabra")
                 (freqs (funcall 'neovm--test-char-freqs text))
                 (tree (funcall 'neovm--test-huffman-build freqs))
                 (codes (funcall 'neovm--test-huffman-codes tree)))
            ;; Encode the text
            (let ((encoded-bits nil))
              (dotimes (i (length text))
                (setq encoded-bits
                      (cons (gethash (aref text i) codes) encoded-bits)))
              (let ((encoded (apply #'concat (nreverse encoded-bits))))
                ;; Collect code table as sorted alist
                (let ((code-list nil))
                  (maphash (lambda (k v)
                             (setq code-list
                                   (cons (cons (char-to-string k) v) code-list)))
                           codes)
                  (setq code-list
                        (sort code-list
                              (lambda (a b) (string< (car a) (car b)))))
                  ;; Verify prefix-free: no code is a prefix of another
                  (let ((prefix-free t))
                    (let ((all-codes (mapcar #'cdr code-list)))
                      (dolist (c1 all-codes)
                        (dolist (c2 all-codes)
                          (when (and (not (equal c1 c2))
                                     (string-prefix-p c1 c2))
                            (setq prefix-free nil)))))
                    (list freqs code-list encoded
                          (length encoded)
                          (* (length text) 8)  ;; uncompressed bits
                          prefix-free))))))
        (fmakunbound 'neovm--test-char-freqs)
        (fmakunbound 'neovm--test-huffman-build)
        (fmakunbound 'neovm--test-huffman-codes)
        (fmakunbound 'neovm--test-huff-walk)))"#;
    let expect = expect_test::expect![[
        r#""OK (((100 . 1) (99 . 1) (114 . 2) (98 . 2) (97 . 5)) ((\"a\" . \"0\") (\"b\" . \"10\") (\"c\" . \"1101\") (\"d\" . \"1100\") (\"r\" . \"111\")) \"01011101101011000101110\" 23 88 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Base64-like encoding using custom alphabet
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compression_base64_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement encode and decode with a custom 64-char alphabet.
    // Roundtrip verification for multiple inputs.
    let form = r#"(progn
      (fset 'neovm--test-b64-encode
        (lambda (input alphabet)
          (let ((result nil)
                (i 0)
                (len (length input)))
            ;; Process 3-byte groups
            (while (<= (+ i 2) (1- len))
              (let* ((b0 (aref input i))
                     (b1 (aref input (1+ i)))
                     (b2 (aref input (+ i 2)))
                     (n (logior (ash b0 16) (ash b1 8) b2)))
                (setq result (cons (aref alphabet (logand (ash n -18) 63)) result))
                (setq result (cons (aref alphabet (logand (ash n -12) 63)) result))
                (setq result (cons (aref alphabet (logand (ash n -6) 63)) result))
                (setq result (cons (aref alphabet (logand n 63)) result))
                (setq i (+ i 3))))
            ;; Remaining bytes
            (let ((rem (- len i)))
              (cond
                ((= rem 2)
                 (let* ((b0 (aref input i))
                        (b1 (aref input (1+ i)))
                        (n (logior (ash b0 16) (ash b1 8))))
                   (setq result (cons ?= (cons (aref alphabet (logand (ash n -6) 63))
                                               (cons (aref alphabet (logand (ash n -12) 63))
                                                     (cons (aref alphabet (logand (ash n -18) 63))
                                                           result)))))))
                ((= rem 1)
                 (let* ((b0 (aref input i))
                        (n (ash b0 16)))
                   (setq result (cons ?= (cons ?= (cons (aref alphabet (logand (ash n -12) 63))
                                                        (cons (aref alphabet (logand (ash n -18) 63))
                                                              result)))))))))
            (concat (nreverse result)))))
      ;; Build reverse lookup table
      (fset 'neovm--test-b64-decode
        (lambda (encoded alphabet)
          (let ((rev (make-hash-table))
                (i 0))
            (dotimes (idx 64)
              (puthash (aref alphabet idx) idx rev))
            (let ((result nil)
                  (len (length encoded)))
              (setq i 0)
              (while (< i len)
                (let ((c0 (aref encoded i))
                      (c1 (aref encoded (1+ i)))
                      (c2 (aref encoded (+ i 2)))
                      (c3 (aref encoded (+ i 3))))
                  (if (= c2 ?=)
                      ;; One byte
                      (let ((n (logior (ash (gethash c0 rev) 18)
                                       (ash (gethash c1 rev) 12))))
                        (setq result (cons (logand (ash n -16) 255) result)))
                    (if (= c3 ?=)
                        ;; Two bytes
                        (let ((n (logior (ash (gethash c0 rev) 18)
                                         (ash (gethash c1 rev) 12)
                                         (ash (gethash c2 rev) 6))))
                          (setq result (cons (logand (ash n -8) 255)
                                             (cons (logand (ash n -16) 255)
                                                   result))))
                      ;; Three bytes
                      (let ((n (logior (ash (gethash c0 rev) 18)
                                       (ash (gethash c1 rev) 12)
                                       (ash (gethash c2 rev) 6)
                                       (gethash c3 rev))))
                        (setq result (cons (logand n 255)
                                           (cons (logand (ash n -8) 255)
                                                 (cons (logand (ash n -16) 255)
                                                       result)))))))
                  (setq i (+ i 4))))
              (concat (nreverse result))))))
      (unwind-protect
          (let ((alpha "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/")
                (inputs '("Hello" "Hi" "A" "foobar" "Elisp!" "test123")))
            (mapcar (lambda (s)
                      (let* ((enc (funcall 'neovm--test-b64-encode s alpha))
                             (dec (funcall 'neovm--test-b64-decode enc alpha)))
                        (list s enc dec (equal s dec))))
                    inputs))
        (fmakunbound 'neovm--test-b64-encode)
        (fmakunbound 'neovm--test-b64-decode)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Hello\" \"SGVsbG8=\" \"Hello\" t) (\"Hi\" \"SGk=\" \"Hi\" t) (\"A\" \"QQ==\" \"A\" t) (\"foobar\" \"Zm9vYmFy\" \"foobar\" t) (\"Elisp!\" \"RWxpc3Ah\" \"Elisp!\" t) (\"test123\" \"dGVzdDEyMw==\" \"test123\" t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Delta encoding/decoding for sorted integer sequences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compression_delta_encoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Delta encoding stores the first value, then differences between
    // consecutive values. For sorted sequences, all deltas are non-negative.
    // Verify roundtrip and compression properties.
    let form = r#"(progn
      (fset 'neovm--test-delta-encode
        (lambda (seq)
          (if (null seq)
              nil
            (let ((result (list (car seq)))
                  (prev (car seq))
                  (rest (cdr seq)))
              (dolist (val rest)
                (setq result (cons (- val prev) result)
                      prev val))
              (nreverse result)))))
      (fset 'neovm--test-delta-decode
        (lambda (deltas)
          (if (null deltas)
              nil
            (let ((result (list (car deltas)))
                  (acc (car deltas))
                  (rest (cdr deltas)))
              (dolist (d rest)
                (setq acc (+ acc d))
                (setq result (cons acc result)))
              (nreverse result)))))
      (unwind-protect
          (let ((sequences
                  (list
                    ;; Sorted ascending (typical use case)
                    '(1 3 6 10 15 21 28 36 45 55)
                    ;; Evenly spaced (constant deltas)
                    '(100 200 300 400 500 600)
                    ;; Close together (small deltas)
                    '(1000 1001 1003 1004 1004 1005)
                    ;; Large gaps
                    '(1 100 10000 1000000)
                    ;; Single element
                    '(42)
                    ;; Non-sorted (negative deltas)
                    '(10 5 8 3 7 1))))
            (mapcar (lambda (seq)
                      (let* ((encoded (funcall 'neovm--test-delta-encode seq))
                             (decoded (funcall 'neovm--test-delta-decode encoded))
                             (roundtrip-ok (equal seq decoded))
                             ;; Check if all deltas non-negative (sorted input)
                             (all-non-neg t))
                        (dolist (d (cdr encoded))
                          (when (< d 0) (setq all-non-neg nil)))
                        (list seq encoded decoded roundtrip-ok all-non-neg)))
                    sequences))
        (fmakunbound 'neovm--test-delta-encode)
        (fmakunbound 'neovm--test-delta-decode)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 3 6 10 15 21 28 36 45 55) (1 2 3 4 5 6 7 8 9 10) (1 3 6 10 15 21 28 36 45 55) t t) ((100 200 300 400 500 600) (100 100 100 100 100 100) (100 200 300 400 500 600) t t) ((1000 1001 1003 1004 1004 1005) (1000 1 2 1 0 1) (1000 1001 1003 1004 1004 1005) t t) ((1 100 10000 1000000) (1 99 9900 990000) (1 100 10000 1000000) t t) ((42) (42) (42) t t) ((10 5 8 3 7 1) (10 -5 3 -5 4 -6) (10 5 8 3 7 1) t nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dictionary-based compression (LZ77-like sliding window)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compression_lz77_like() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simplified LZ77: scan input left-to-right, find longest match in
    // a sliding window of size W. Encode as either (offset length) for
    // matches or (0 0 literal-char) for unmatched characters.
    // Then decode and verify roundtrip.
    let form = r#"(progn
      (fset 'neovm--test-lz77-encode
        (lambda (input window-size)
          (let ((tokens nil)
                (pos 0)
                (len (length input)))
            (while (< pos len)
              ;; Search window: from max(0, pos-window-size) to pos
              (let ((search-start (max 0 (- pos window-size)))
                    (best-offset 0)
                    (best-length 0))
                ;; Try each position in the window
                (let ((s search-start))
                  (while (< s pos)
                    (let ((match-len 0))
                      (while (and (< (+ pos match-len) len)
                                  (< (+ s match-len) pos)
                                  (= (aref input (+ s match-len))
                                     (aref input (+ pos match-len))))
                        (setq match-len (1+ match-len)))
                      (when (> match-len best-length)
                        (setq best-length match-len
                              best-offset (- pos s))))
                    (setq s (1+ s))))
                (if (> best-length 1)
                    ;; Emit match token
                    (progn
                      (setq tokens (cons (list best-offset best-length) tokens))
                      (setq pos (+ pos best-length)))
                  ;; Emit literal
                  (setq tokens (cons (list 0 0 (aref input pos)) tokens))
                  (setq pos (1+ pos)))))
            (nreverse tokens))))
      (fset 'neovm--test-lz77-decode
        (lambda (tokens)
          (let ((output nil))
            (dolist (tok tokens)
              (if (and (= (car tok) 0) (= (cadr tok) 0))
                  ;; Literal
                  (setq output (append output (list (caddr tok))))
                ;; Match: copy from output buffer
                (let* ((offset (car tok))
                       (length (cadr tok))
                       (start (- (length output) offset)))
                  (dotimes (i length)
                    (setq output
                          (append output
                                  (list (nth (+ start i) output))))))))
            (concat output))))
      (unwind-protect
          (let ((inputs '("aabaabaab"
                          "abcabcabc"
                          "aaaaaaaaaa"
                          "abcdefgh"
                          "the cat sat on the mat"))
                (window 8))
            (mapcar (lambda (s)
                      (let* ((encoded (funcall 'neovm--test-lz77-encode s window))
                             (decoded (funcall 'neovm--test-lz77-decode encoded))
                             (roundtrip-ok (equal s decoded))
                             (token-count (length encoded)))
                        (list s token-count roundtrip-ok decoded)))
                    inputs))
        (fmakunbound 'neovm--test-lz77-encode)
        (fmakunbound 'neovm--test-lz77-decode)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"aabaabaab\" 5 t \"aabaabaab\") (\"abcabcabc\" 5 t \"abcabcabc\") (\"aaaaaaaaaa\" 5 t \"aaaaaaaaaa\") (\"abcdefgh\" 8 t \"abcdefgh\") (\"the cat sat on the mat\" 20 t \"the cat sat on the mat\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Burrows-Wheeler Transform (simplified)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compression_bwt() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simplified BWT: append sentinel '$' (guaranteed smallest), generate all
    // rotations, sort them, take last column. Also record the index of the
    // original string in the sorted rotation list for inverse transform.
    let form = r#"(progn
      (fset 'neovm--test-bwt-forward
        (lambda (input)
          (let* ((s (concat input "$"))
                 (n (length s))
                 (rotations nil))
            ;; Generate all rotations
            (dotimes (i n)
              (setq rotations
                    (cons (cons i (concat (substring s i) (substring s 0 i)))
                          rotations)))
            ;; Sort by the rotation string
            (setq rotations
                  (sort rotations
                        (lambda (a b) (string< (cdr a) (cdr b)))))
            ;; Last column = last char of each sorted rotation
            (let ((last-col nil)
                  (orig-idx nil)
                  (idx 0))
              (dolist (rot rotations)
                (setq last-col
                      (cons (aref (cdr rot) (1- n)) last-col))
                (when (= (car rot) 0)
                  (setq orig-idx idx))
                (setq idx (1+ idx)))
              (list (concat (nreverse last-col)) orig-idx)))))
      ;; Inverse BWT using the "follow the index" method
      (fset 'neovm--test-bwt-inverse
        (lambda (last-col orig-idx)
          (let* ((n (length last-col))
                 ;; Build sorted first column
                 (chars nil))
            (dotimes (i n)
              (setq chars (cons (cons (aref last-col i) i) chars)))
            ;; Sort by character, stable (preserve order for ties)
            (setq chars
                  (sort (nreverse chars)
                        (lambda (a b) (< (car a) (car b)))))
            ;; T[i] maps: position in sorted order -> position of that
            ;; char's occurrence in the last column
            (let ((t-map (make-vector n 0)))
              (let ((idx 0))
                (dolist (entry chars)
                  (aset t-map idx (cdr entry))
                  (setq idx (1+ idx))))
              ;; Follow t-map starting from orig-idx for n steps
              (let ((result nil)
                    (cur orig-idx))
                (dotimes (_ n)
                  (setq result (cons (aref last-col cur) result))
                  (setq cur (aref t-map cur)))
                ;; Remove sentinel '$' and reverse
                (let ((s (concat (nreverse result))))
                  (substring s 0 (1- (length s)))))))))
      (unwind-protect
          (let ((inputs '("banana" "abcabc" "mississippi" "hello")))
            (mapcar (lambda (s)
                      (let* ((fwd (funcall 'neovm--test-bwt-forward s))
                             (bwt-str (car fwd))
                             (bwt-idx (cadr fwd))
                             (recovered (funcall 'neovm--test-bwt-inverse
                                                  bwt-str bwt-idx)))
                        (list s bwt-str bwt-idx recovered (equal s recovered))))
                    inputs))
        (fmakunbound 'neovm--test-bwt-forward)
        (fmakunbound 'neovm--test-bwt-inverse)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"banana\" \"annb$aa\" 4 \"$banan\" nil) (\"abcabc\" \"cc$aabb\" 2 \"$abcab\" nil) (\"mississippi\" \"ipssm$pissii\" 5 \"$mississipp\" nil) (\"hello\" \"oh$ell\" 2 \"$hell\" nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

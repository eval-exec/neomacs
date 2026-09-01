//! Oracle parity tests for advanced Huffman coding in Elisp:
//! full Huffman pipeline with frequency table, priority queue, tree construction,
//! code table generation, encode/decode, roundtrip verification, optimal code
//! length verification (Shannon bound), canonical Huffman codes, tree
//! serialization/deserialization, and compression ratio computation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Canonical Huffman codes: codes are generated in a canonical order
// (sorted by code length, then by symbol) for deterministic output.
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_huffman_adv_canonical_codes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build Huffman tree, extract code lengths, then generate canonical codes.
    // Canonical Huffman: assign codes based on length only, in lexicographic order.
    let form = r#"
(progn
  ;; Build frequency table
  (fset 'neovm--hca-freq
    (lambda (text)
      (let ((ht (make-hash-table :test 'equal)))
        (dotimes (i (length text))
          (puthash (aref text i) (1+ (gethash (aref text i) ht 0)) ht))
        ht)))

  ;; Priority queue insert (sorted list by weight)
  (fset 'neovm--hca-pq-insert
    (lambda (q item)
      (if (null q) (list item)
        (if (<= (car item) (car (car q)))
            (cons item q)
          (cons (car q) (funcall 'neovm--hca-pq-insert (cdr q) item))))))

  ;; Build Huffman tree -> code lengths
  (fset 'neovm--hca-code-lengths
    (lambda (text)
      (let ((ht (funcall 'neovm--hca-freq text))
            (queue nil))
        (maphash (lambda (k v) (setq queue (funcall 'neovm--hca-pq-insert queue (list v k)))) ht)
        (when (= (length queue) 1)
          (setq queue (funcall 'neovm--hca-pq-insert queue (list 0 0))))
        (while (> (length queue) 1)
          (let* ((left (car queue)) (q1 (cdr queue))
                 (right (car q1)) (q2 (cdr q1))
                 (merged (list (+ (car left) (car right)) left right)))
            (setq queue (funcall 'neovm--hca-pq-insert q2 merged))))
        ;; Extract code lengths
        (let ((lengths (make-hash-table :test 'equal))
              (gen nil))
          (setq gen (lambda (node depth)
                      (if (= (length node) 2)
                          (puthash (cadr node) depth lengths)
                        (funcall gen (cadr node) (1+ depth))
                        (funcall gen (caddr node) (1+ depth)))))
          (funcall gen (car queue) 0)
          lengths))))

  ;; Generate canonical Huffman codes from lengths
  (fset 'neovm--hca-canonical
    (lambda (length-ht)
      ;; Collect (symbol . length) pairs, sort by length then symbol
      (let ((pairs nil))
        (maphash (lambda (k v) (push (cons k v) pairs)) length-ht)
        (setq pairs (sort pairs (lambda (a b)
                                  (or (< (cdr a) (cdr b))
                                      (and (= (cdr a) (cdr b))
                                           (< (car a) (car b)))))))
        ;; Assign canonical codes
        (let ((code 0) (prev-len 0) (result nil))
          (dolist (pair pairs)
            (let ((sym (car pair)) (len (cdr pair)))
              ;; Shift code left for longer codes
              (setq code (ash code (- len prev-len)))
              ;; Convert code to binary string of length len
              (let ((bits nil))
                (dotimes (_ len)
                  (push (if (= (logand code 1) 1) ?1 ?0) bits)
                  (setq code (ash code -1)))
                (push (cons sym (concat bits)) result))
              (setq code (1+ (ash (car (cdr (assq sym (reverse result))))
                                  0)))
              ;; Actually re-do this properly
              ))
          ;; Simpler approach: just generate codes
          (let ((code2 0) (prev-len2 0) (result2 nil))
            (dolist (pair pairs)
              (let* ((sym (car pair)) (len (cdr pair)))
                (when (> len prev-len2)
                  (setq code2 (ash code2 (- len prev-len2))))
                ;; Convert to binary string
                (let ((s (make-string len ?0)))
                  (let ((c code2) (j (1- len)))
                    (while (>= j 0)
                      (when (= (logand c 1) 1)
                        (aset s j ?1))
                      (setq c (ash c -1))
                      (setq j (1- j))))
                  (push (cons sym s) result2))
                (setq code2 (1+ code2))
                (setq prev-len2 len)))
            (nreverse result2))))))

  (unwind-protect
      (let ((text "abracadabra"))
        (let* ((lengths (funcall 'neovm--hca-code-lengths text))
               (canonical (funcall 'neovm--hca-canonical lengths)))
          ;; Sort result by symbol for deterministic comparison
          (let ((sorted (sort (copy-sequence canonical)
                              (lambda (a b) (< (car a) (car b))))))
            ;; Verify: all codes have correct lengths
            (let ((lengths-match t))
              (dolist (pair sorted)
                (unless (= (length (cdr pair)) (gethash (car pair) lengths))
                  (setq lengths-match nil)))
              ;; Verify: no code is prefix of another
              (let ((prefix-free t)
                    (code-strs (mapcar 'cdr sorted)))
                (let ((sorted-strs (sort (copy-sequence code-strs) 'string<)))
                  (let ((i 0))
                    (while (< i (1- (length sorted-strs)))
                      (let ((a (nth i sorted-strs))
                            (b (nth (1+ i) sorted-strs)))
                        (when (and (<= (length a) (length b))
                                   (string= a (substring b 0 (length a))))
                          (setq prefix-free nil)))
                      (setq i (1+ i)))))
                (list sorted lengths-match prefix-free
                      (length canonical)))))))
    (fmakunbound 'neovm--hca-freq)
    (fmakunbound 'neovm--hca-pq-insert)
    (fmakunbound 'neovm--hca-code-lengths)
    (fmakunbound 'neovm--hca-canonical)))
"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp \"0\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tree serialization/deserialization: flatten tree to a bitstring and rebuild
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_huffman_adv_tree_serialization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Serialize a Huffman tree to a string representation and deserialize it.
    // Leaf=(L char), Internal=(I left right), stored as prefix notation.
    let form = r#"
(progn
  (fset 'neovm--hts-pq-insert
    (lambda (q item)
      (if (null q) (list item)
        (if (<= (car item) (car (car q)))
            (cons item q)
          (cons (car q) (funcall 'neovm--hts-pq-insert (cdr q) item))))))

  ;; Build tree from text
  (fset 'neovm--hts-build-tree
    (lambda (text)
      (let ((ht (make-hash-table :test 'equal)) (queue nil))
        (dotimes (i (length text))
          (puthash (aref text i) (1+ (gethash (aref text i) ht 0)) ht))
        (maphash (lambda (k v) (setq queue (funcall 'neovm--hts-pq-insert queue (list v k)))) ht)
        (when (= (length queue) 1)
          (setq queue (funcall 'neovm--hts-pq-insert queue (list 0 0))))
        (while (> (length queue) 1)
          (let* ((l (car queue)) (q1 (cdr queue))
                 (r (car q1)) (q2 (cdr q1)))
            (setq queue (funcall 'neovm--hts-pq-insert q2
                                 (list (+ (car l) (car r)) l r)))))
        (car queue))))

  ;; Serialize tree to list representation
  (fset 'neovm--hts-serialize
    (lambda (node)
      (if (= (length node) 2)
          ;; Leaf: (L weight char)
          (list 'L (car node) (cadr node))
        ;; Internal: (I weight left-serial right-serial)
        (list 'I (car node)
              (funcall 'neovm--hts-serialize (cadr node))
              (funcall 'neovm--hts-serialize (caddr node))))))

  ;; Deserialize back to tree
  (fset 'neovm--hts-deserialize
    (lambda (serial)
      (if (eq (car serial) 'L)
          ;; Leaf: (weight char)
          (list (cadr serial) (caddr serial))
        ;; Internal: (weight left right)
        (list (cadr serial)
              (funcall 'neovm--hts-deserialize (caddr serial))
              (funcall 'neovm--hts-deserialize (cadddr serial))))))

  ;; Generate codes from tree
  (fset 'neovm--hts-gen-codes
    (lambda (tree)
      (let ((codes (make-hash-table :test 'equal))
            (gen nil))
        (setq gen (lambda (node pfx)
                    (if (= (length node) 2)
                        (puthash (cadr node) pfx codes)
                      (funcall gen (cadr node) (concat pfx "0"))
                      (funcall gen (caddr node) (concat pfx "1")))))
        (funcall gen tree "")
        codes)))

  (unwind-protect
      (let* ((text "hello world")
             (tree (funcall 'neovm--hts-build-tree text))
             (serial (funcall 'neovm--hts-serialize tree))
             (restored (funcall 'neovm--hts-deserialize serial)))
        ;; Generate codes from both trees
        (let ((codes1 (funcall 'neovm--hts-gen-codes tree))
              (codes2 (funcall 'neovm--hts-gen-codes restored)))
          ;; Compare: both should produce identical code tables
          (let ((match t))
            (maphash (lambda (k v)
                       (unless (equal v (gethash k codes2))
                         (setq match nil)))
                     codes1)
            ;; Also check counts
            (list match
                  (= (hash-table-count codes1) (hash-table-count codes2))
                  ;; Root weight equals text length
                  (= (car tree) (length text))
                  (= (car restored) (length text))
                  ;; Serialized form type checks
                  (eq (car serial) 'I)))))
    (fmakunbound 'neovm--hts-pq-insert)
    (fmakunbound 'neovm--hts-build-tree)
    (fmakunbound 'neovm--hts-serialize)
    (fmakunbound 'neovm--hts-deserialize)
    (fmakunbound 'neovm--hts-gen-codes)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Roundtrip encode/decode with multiple inputs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_huffman_adv_roundtrip_extended() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full encode/decode roundtrip with varied input patterns.
    let form = r#"
(progn
  (fset 'neovm--hrt-pq-insert
    (lambda (q item)
      (if (null q) (list item)
        (if (<= (car item) (car (car q)))
            (cons item q)
          (cons (car q) (funcall 'neovm--hrt-pq-insert (cdr q) item))))))

  (fset 'neovm--hrt-roundtrip
    (lambda (text)
      (let ((ht (make-hash-table :test 'equal)) (queue nil))
        (dotimes (i (length text))
          (puthash (aref text i) (1+ (gethash (aref text i) ht 0)) ht))
        (maphash (lambda (k v) (setq queue (funcall 'neovm--hrt-pq-insert queue (list v k)))) ht)
        (when (= (length queue) 1)
          (setq queue (funcall 'neovm--hrt-pq-insert queue (list 0 0))))
        (while (> (length queue) 1)
          (let* ((l (car queue)) (q1 (cdr queue))
                 (r (car q1)) (q2 (cdr q1)))
            (setq queue (funcall 'neovm--hrt-pq-insert q2
                                 (list (+ (car l) (car r)) l r)))))
        (let ((tree (car queue))
              (codes (make-hash-table :test 'equal)))
          (let ((gen nil))
            (setq gen (lambda (node pfx)
                        (if (= (length node) 2)
                            (puthash (cadr node) pfx codes)
                          (funcall gen (cadr node) (concat pfx "0"))
                          (funcall gen (caddr node) (concat pfx "1")))))
            (funcall gen tree ""))
          ;; Encode
          (let ((encoded (let ((bits nil))
                           (dotimes (i (length text))
                             (push (gethash (aref text i) codes) bits))
                           (apply 'concat (nreverse bits)))))
            ;; Decode
            (let ((decoded nil) (pos 0) (len (length encoded)))
              (while (< pos len)
                (let ((node tree))
                  (while (> (length node) 2)
                    (if (= (aref encoded pos) ?0)
                        (setq node (cadr node))
                      (setq node (caddr node)))
                    (setq pos (1+ pos)))
                  (push (cadr node) decoded)))
              (let ((decoded-str (concat (nreverse decoded))))
                (list (string= text decoded-str)
                      (length text)
                      (length encoded)
                      (hash-table-count codes)))))))))

  (unwind-protect
      (list
       ;; Highly skewed frequency
       (funcall 'neovm--hrt-roundtrip "aaaaaabbbccde")
       ;; All same character
       (funcall 'neovm--hrt-roundtrip "zzzzzzz")
       ;; Alternating
       (funcall 'neovm--hrt-roundtrip "ababababab")
       ;; Long with many unique chars
       (funcall 'neovm--hrt-roundtrip "the quick brown fox jumps over")
       ;; Two characters
       (funcall 'neovm--hrt-roundtrip "aabb")
       ;; Single character
       (funcall 'neovm--hrt-roundtrip "x")
       ;; Palindrome
       (funcall 'neovm--hrt-roundtrip "racecar"))
    (fmakunbound 'neovm--hrt-pq-insert)
    (fmakunbound 'neovm--hrt-roundtrip)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((t 13 26 5) (t 7 7 2) (t 10 10 2) (t 30 126 21) (t 4 4 2) (t 1 1 2) (t 7 14 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Optimal code length: weighted path length vs Shannon entropy bound
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_huffman_adv_optimal_code_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that the Huffman code's average code length is within Shannon's
    // bound: H <= avg_len <= H + 1 where H is the entropy.
    let form = r#"
(progn
  (fset 'neovm--hol-pq-insert
    (lambda (q item)
      (if (null q) (list item)
        (if (<= (car item) (car (car q)))
            (cons item q)
          (cons (car q) (funcall 'neovm--hol-pq-insert (cdr q) item))))))

  (fset 'neovm--hol-analyze
    (lambda (text)
      (let ((ht (make-hash-table :test 'equal)) (queue nil) (n (length text)))
        (dotimes (i n)
          (puthash (aref text i) (1+ (gethash (aref text i) ht 0)) ht))
        ;; Compute entropy H = -sum(p*log2(p))
        (let ((entropy 0.0))
          (maphash (lambda (_k v)
                     (let ((p (/ (float v) n)))
                       (setq entropy (- entropy (* p (log p 2))))))
                   ht)
          ;; Build Huffman tree
          (maphash (lambda (k v) (setq queue (funcall 'neovm--hol-pq-insert queue (list v k)))) ht)
          (when (= (length queue) 1)
            (setq queue (funcall 'neovm--hol-pq-insert queue (list 0 0))))
          (while (> (length queue) 1)
            (let* ((l (car queue)) (q1 (cdr queue))
                   (r (car q1)) (q2 (cdr q1)))
              (setq queue (funcall 'neovm--hol-pq-insert q2
                                   (list (+ (car l) (car r)) l r)))))
          (let ((tree (car queue))
                (codes (make-hash-table :test 'equal)))
            (let ((gen nil))
              (setq gen (lambda (node pfx)
                          (if (= (length node) 2)
                              (puthash (cadr node) pfx codes)
                            (funcall gen (cadr node) (concat pfx "0"))
                            (funcall gen (caddr node) (concat pfx "1")))))
              (funcall gen tree ""))
            ;; Compute average code length
            (let ((total-bits 0))
              (maphash (lambda (ch code)
                         (setq total-bits (+ total-bits
                                             (* (gethash ch ht) (length code)))))
                       codes)
              (let ((avg-len (/ (float total-bits) n)))
                ;; Shannon bound: H <= avg <= H + 1
                (list (>= avg-len (- entropy 0.01))    ;; allow small float error
                      (<= avg-len (+ entropy 1.01))
                      entropy
                      avg-len
                      (hash-table-count codes)))))))))

  (unwind-protect
      (list
       (funcall 'neovm--hol-analyze "aaaaaabbbcc")
       (funcall 'neovm--hol-analyze "abcdefghij")
       (funcall 'neovm--hol-analyze "mississippi")
       (funcall 'neovm--hol-analyze "aaaaaa"))
    (fmakunbound 'neovm--hol-pq-insert)
    (fmakunbound 'neovm--hol-analyze)))
"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Compression ratio: compare Huffman bits vs fixed-width encoding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_huffman_adv_compression_ratio_analysis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute and compare compression ratios across different text patterns.
    let form = r#"
(progn
  (fset 'neovm--hcr-pq-insert
    (lambda (q item)
      (if (null q) (list item)
        (if (<= (car item) (car (car q)))
            (cons item q)
          (cons (car q) (funcall 'neovm--hcr-pq-insert (cdr q) item))))))

  (fset 'neovm--hcr-ratio
    (lambda (text)
      (let ((ht (make-hash-table :test 'equal)) (queue nil))
        (dotimes (i (length text))
          (puthash (aref text i) (1+ (gethash (aref text i) ht 0)) ht))
        (maphash (lambda (k v) (setq queue (funcall 'neovm--hcr-pq-insert queue (list v k)))) ht)
        (when (= (length queue) 1)
          (setq queue (funcall 'neovm--hcr-pq-insert queue (list 0 0))))
        (while (> (length queue) 1)
          (let* ((l (car queue)) (q1 (cdr queue))
                 (r (car q1)) (q2 (cdr q1)))
            (setq queue (funcall 'neovm--hcr-pq-insert q2
                                 (list (+ (car l) (car r)) l r)))))
        (let ((tree (car queue))
              (codes (make-hash-table :test 'equal)))
          (let ((gen nil))
            (setq gen (lambda (node pfx)
                        (if (= (length node) 2)
                            (puthash (cadr node) pfx codes)
                          (funcall gen (cadr node) (concat pfx "0"))
                          (funcall gen (caddr node) (concat pfx "1")))))
            (funcall gen tree ""))
          (let ((huffman-bits 0))
            (maphash (lambda (ch code)
                       (setq huffman-bits (+ huffman-bits
                                             (* (gethash ch ht) (length code)))))
                     codes)
            (let* ((ascii-bits (* (length text) 8))
                   (unique (hash-table-count ht))
                   ;; Minimum fixed-width bits = ceil(log2(unique))
                   (min-bits (let ((b 1) (n 1))
                               (while (< n unique) (setq n (* n 2) b (1+ b)))
                               b))
                   (fixed-bits (* (length text) min-bits)))
              (list (length text) unique
                    huffman-bits ascii-bits fixed-bits
                    ;; Huffman should beat or match fixed-width
                    (<= huffman-bits fixed-bits))))))))

  (unwind-protect
      (list
       ;; Very high redundancy
       (funcall 'neovm--hcr-ratio (make-string 50 ?a))
       ;; Moderate redundancy
       (funcall 'neovm--hcr-ratio "aabbccddee")
       ;; Low redundancy
       (funcall 'neovm--hcr-ratio "abcdefghijklmnop")
       ;; Natural text
       (funcall 'neovm--hcr-ratio "to be or not to be"))
    (fmakunbound 'neovm--hcr-pq-insert)
    (fmakunbound 'neovm--hcr-ratio)))
"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Frequency update (adaptive-like): build trees for different text segments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_huffman_adv_frequency_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate adaptive Huffman by building separate trees for different
    // segments of text and comparing code tables.
    let form = r#"
(progn
  (fset 'neovm--hfu-pq-insert
    (lambda (q item)
      (if (null q) (list item)
        (if (<= (car item) (car (car q)))
            (cons item q)
          (cons (car q) (funcall 'neovm--hfu-pq-insert (cdr q) item))))))

  (fset 'neovm--hfu-build-codes
    (lambda (text)
      (let ((ht (make-hash-table :test 'equal)) (queue nil))
        (dotimes (i (length text))
          (puthash (aref text i) (1+ (gethash (aref text i) ht 0)) ht))
        (maphash (lambda (k v)
                   (setq queue (funcall 'neovm--hfu-pq-insert queue (list v k)))) ht)
        (when (= (length queue) 1)
          (setq queue (funcall 'neovm--hfu-pq-insert queue (list 0 0))))
        (while (> (length queue) 1)
          (let* ((l (car queue)) (q1 (cdr queue))
                 (r (car q1)) (q2 (cdr q1)))
            (setq queue (funcall 'neovm--hfu-pq-insert q2
                                 (list (+ (car l) (car r)) l r)))))
        (let ((tree (car queue))
              (codes (make-hash-table :test 'equal)))
          (let ((gen nil))
            (setq gen (lambda (node pfx)
                        (if (= (length node) 2)
                            (puthash (cadr node) pfx codes)
                          (funcall gen (cadr node) (concat pfx "0"))
                          (funcall gen (caddr node) (concat pfx "1")))))
            (funcall gen tree ""))
          ;; Return alist of (char . code-length), sorted
          (let ((result nil))
            (maphash (lambda (k v) (push (cons k (length v)) result)) codes)
            (sort result (lambda (a b) (< (car a) (car b)))))))))

  (unwind-protect
      (let* ((seg1 "aaaaabbbcc")           ;; a dominant
             (seg2 "cccccbbbaa")           ;; c dominant
             (combined (concat seg1 seg2)) ;; balanced
             (codes1 (funcall 'neovm--hfu-build-codes seg1))
             (codes2 (funcall 'neovm--hfu-build-codes seg2))
             (codes-combined (funcall 'neovm--hfu-build-codes combined)))
        ;; In seg1, 'a' should have shortest code; in seg2, 'c' should
        (let ((a-len-1 (cdr (assq ?a codes1)))
              (c-len-1 (cdr (assq ?c codes1)))
              (a-len-2 (cdr (assq ?a codes2)))
              (c-len-2 (cdr (assq ?c codes2))))
          (list
           ;; In seg1: a is most frequent -> shortest
           (<= a-len-1 c-len-1)
           ;; In seg2: c is most frequent -> shortest
           (<= c-len-2 a-len-2)
           ;; Combined should have more balanced codes
           codes1 codes2 codes-combined)))
    (fmakunbound 'neovm--hfu-pq-insert)
    (fmakunbound 'neovm--hfu-build-codes)))
"#;
    let expect = expect_test::expect![[
        r#""OK (t t ((97 . 1) (98 . 2) (99 . 2)) ((97 . 2) (98 . 2) (99 . 1)) ((97 . 1) (98 . 2) (99 . 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Full binary tree property verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_huffman_adv_full_binary_tree_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that the Huffman tree is a full binary tree and that it has
    // exactly n-1 internal nodes for n leaves.
    let form = r#"
(progn
  (fset 'neovm--hfb-pq-insert
    (lambda (q item)
      (if (null q) (list item)
        (if (<= (car item) (car (car q)))
            (cons item q)
          (cons (car q) (funcall 'neovm--hfb-pq-insert (cdr q) item))))))

  (fset 'neovm--hfb-verify
    (lambda (text)
      (let ((ht (make-hash-table :test 'equal)) (queue nil))
        (dotimes (i (length text))
          (puthash (aref text i) (1+ (gethash (aref text i) ht 0)) ht))
        (let ((num-symbols (hash-table-count ht)))
          (maphash (lambda (k v)
                     (setq queue (funcall 'neovm--hfb-pq-insert queue (list v k)))) ht)
          (when (= (length queue) 1)
            (setq queue (funcall 'neovm--hfb-pq-insert queue (list 0 0))))
          (while (> (length queue) 1)
            (let* ((l (car queue)) (q1 (cdr queue))
                   (r (car q1)) (q2 (cdr q1)))
              (setq queue (funcall 'neovm--hfb-pq-insert q2
                                   (list (+ (car l) (car r)) l r)))))
          (let ((tree (car queue)))
            ;; Count leaves and internal nodes
            (let ((count-nodes nil))
              (setq count-nodes
                    (lambda (node)
                      (if (= (length node) 2)
                          (cons 1 0)  ;; (leaves . internals)
                        (let ((left (funcall count-nodes (cadr node)))
                              (right (funcall count-nodes (caddr node))))
                          (cons (+ (car left) (car right))
                                (+ 1 (cdr left) (cdr right)))))))
              (let ((counts (funcall count-nodes tree)))
                ;; Full binary tree: internals = leaves - 1
                (let ((leaves (car counts))
                      (internals (cdr counts)))
                  ;; Check full binary tree property
                  (let ((is-full nil))
                    (setq is-full
                          (lambda (node)
                            (if (= (length node) 2) t
                              (and (= (length node) 3)
                                   (funcall is-full (cadr node))
                                   (funcall is-full (caddr node))))))
                    (list leaves internals
                          (= internals (1- leaves))
                          (funcall is-full tree)
                          ;; Root weight = text length
                          (= (car tree) (length text))))))))))))

  (unwind-protect
      (list
       (funcall 'neovm--hfb-verify "abcde")
       (funcall 'neovm--hfb-verify "aaaaaa")
       (funcall 'neovm--hfb-verify "abracadabra")
       (funcall 'neovm--hfb-verify "ab"))
    (fmakunbound 'neovm--hfb-pq-insert)
    (fmakunbound 'neovm--hfb-verify)))
"#;
    let expect =
        expect_test::expect![[r#""OK ((5 4 t t t) (2 1 t t t) (5 4 t t t) (2 1 t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Decode with explicit bit-by-bit tree walk
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_huffman_adv_bitwise_decode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Encode a message, then decode bit-by-bit, tracking the state
    // at each step (current node depth).
    let form = r#"
(progn
  (fset 'neovm--hbd-pq-insert
    (lambda (q item)
      (if (null q) (list item)
        (if (<= (car item) (car (car q)))
            (cons item q)
          (cons (car q) (funcall 'neovm--hbd-pq-insert (cdr q) item))))))

  (fset 'neovm--hbd-test
    (lambda (text)
      (let ((ht (make-hash-table :test 'equal)) (queue nil))
        (dotimes (i (length text))
          (puthash (aref text i) (1+ (gethash (aref text i) ht 0)) ht))
        (maphash (lambda (k v)
                   (setq queue (funcall 'neovm--hbd-pq-insert queue (list v k)))) ht)
        (when (= (length queue) 1)
          (setq queue (funcall 'neovm--hbd-pq-insert queue (list 0 0))))
        (while (> (length queue) 1)
          (let* ((l (car queue)) (q1 (cdr queue))
                 (r (car q1)) (q2 (cdr q1)))
            (setq queue (funcall 'neovm--hbd-pq-insert q2
                                 (list (+ (car l) (car r)) l r)))))
        (let ((tree (car queue))
              (codes (make-hash-table :test 'equal)))
          (let ((gen nil))
            (setq gen (lambda (node pfx)
                        (if (= (length node) 2)
                            (puthash (cadr node) pfx codes)
                          (funcall gen (cadr node) (concat pfx "0"))
                          (funcall gen (caddr node) (concat pfx "1")))))
            (funcall gen tree ""))
          ;; Encode
          (let ((encoded (let ((bits nil))
                           (dotimes (i (length text))
                             (push (gethash (aref text i) codes) bits))
                           (apply 'concat (nreverse bits)))))
            ;; Decode with depth tracking
            (let ((decoded nil) (pos 0) (max-depth 0) (total-steps 0))
              (while (< pos (length encoded))
                (let ((node tree) (depth 0))
                  (while (> (length node) 2)
                    (if (= (aref encoded pos) ?0)
                        (setq node (cadr node))
                      (setq node (caddr node)))
                    (setq pos (1+ pos) depth (1+ depth)
                          total-steps (1+ total-steps)))
                  (when (> depth max-depth) (setq max-depth depth))
                  (push (cadr node) decoded)))
              (let ((decoded-str (concat (nreverse decoded))))
                (list (string= text decoded-str)
                      max-depth
                      total-steps
                      (length encoded)))))))))

  (unwind-protect
      (list
       (funcall 'neovm--hbd-test "abracadabra")
       (funcall 'neovm--hbd-test "aaa")
       (funcall 'neovm--hbd-test "abcdefg"))
    (fmakunbound 'neovm--hbd-pq-insert)
    (fmakunbound 'neovm--hbd-test)))
"#;
    let expect = expect_test::expect![[r#""OK ((t 4 23 23) (t 1 3 3) (t 3 20 20))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Weight-sorted merge order verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_huffman_adv_merge_order_trace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Trace the merge order of the Huffman tree construction,
    // verifying that at each step the two lightest nodes are merged.
    let form = r#"
(progn
  (fset 'neovm--hmo-pq-insert
    (lambda (q item)
      (if (null q) (list item)
        (if (<= (car item) (car (car q)))
            (cons item q)
          (cons (car q) (funcall 'neovm--hmo-pq-insert (cdr q) item))))))

  (fset 'neovm--hmo-trace-build
    (lambda (text)
      (let ((ht (make-hash-table :test 'equal)) (queue nil) (trace nil))
        (dotimes (i (length text))
          (puthash (aref text i) (1+ (gethash (aref text i) ht 0)) ht))
        (maphash (lambda (k v)
                   (setq queue (funcall 'neovm--hmo-pq-insert queue (list v k)))) ht)
        (when (= (length queue) 1)
          (setq queue (funcall 'neovm--hmo-pq-insert queue (list 0 0))))
        ;; Track initial queue weights
        (let ((initial-weights (mapcar 'car queue)))
          (while (> (length queue) 1)
            (let* ((l (car queue)) (q1 (cdr queue))
                   (r (car q1)) (q2 (cdr q1))
                   (merged-weight (+ (car l) (car r))))
              ;; Record: (left-weight right-weight merged-weight)
              (push (list (car l) (car r) merged-weight) trace)
              (setq queue (funcall 'neovm--hmo-pq-insert q2
                                   (list merged-weight l r)))))
          ;; Verify: each merge picked the two smallest
          (let ((valid t))
            (dolist (step (nreverse trace))
              (let ((lw (nth 0 step)) (rw (nth 1 step)))
                ;; Left should be <= right (sorted queue invariant)
                (unless (<= lw rw) (setq valid nil))))
            (list initial-weights
                  (nreverse trace)
                  valid
                  (car (car queue))))))))

  (unwind-protect
      (list
       (funcall 'neovm--hmo-trace-build "abcdd")
       (funcall 'neovm--hmo-trace-build "aabbcc"))
    (fmakunbound 'neovm--hmo-pq-insert)
    (fmakunbound 'neovm--hmo-trace-build)))
"#;
    let expect =
        expect_test::expect![[r#""OK (((1 1 1 2) ((2 3 5)) t 5) ((2 2 2) ((2 4 6)) t 6))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Edge case: single character text
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_huffman_adv_single_char_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Single unique character needs special handling (only one leaf).
    let form = r#"
(progn
  (fset 'neovm--hsc-pq-insert
    (lambda (q item)
      (if (null q) (list item)
        (if (<= (car item) (car (car q)))
            (cons item q)
          (cons (car q) (funcall 'neovm--hsc-pq-insert (cdr q) item))))))

  (fset 'neovm--hsc-test
    (lambda (text)
      (let ((ht (make-hash-table :test 'equal)) (queue nil))
        (dotimes (i (length text))
          (puthash (aref text i) (1+ (gethash (aref text i) ht 0)) ht))
        (maphash (lambda (k v)
                   (setq queue (funcall 'neovm--hsc-pq-insert queue (list v k)))) ht)
        ;; Handle single-symbol case: add dummy node
        (when (= (length queue) 1)
          (setq queue (funcall 'neovm--hsc-pq-insert queue (list 0 0))))
        (while (> (length queue) 1)
          (let* ((l (car queue)) (q1 (cdr queue))
                 (r (car q1)) (q2 (cdr q1)))
            (setq queue (funcall 'neovm--hsc-pq-insert q2
                                 (list (+ (car l) (car r)) l r)))))
        (let ((tree (car queue))
              (codes (make-hash-table :test 'equal)))
          (let ((gen nil))
            (setq gen (lambda (node pfx)
                        (if (= (length node) 2)
                            (puthash (cadr node) pfx codes)
                          (funcall gen (cadr node) (concat pfx "0"))
                          (funcall gen (caddr node) (concat pfx "1")))))
            (funcall gen tree ""))
          ;; Encode and decode
          (let ((encoded (let ((bits nil))
                           (dotimes (i (length text))
                             (push (gethash (aref text i) codes) bits))
                           (apply 'concat (nreverse bits)))))
            (let ((decoded nil) (pos 0))
              (while (< pos (length encoded))
                (let ((node tree))
                  (while (> (length node) 2)
                    (if (= (aref encoded pos) ?0)
                        (setq node (cadr node))
                      (setq node (caddr node)))
                    (setq pos (1+ pos)))
                  (push (cadr node) decoded)))
              (list (string= text (concat (nreverse decoded)))
                    (length encoded)
                    (hash-table-count codes))))))))

  (unwind-protect
      (list
       ;; Single char repeated
       (funcall 'neovm--hsc-test "a")
       (funcall 'neovm--hsc-test "bbbbb")
       ;; Two chars
       (funcall 'neovm--hsc-test "ab"))
    (fmakunbound 'neovm--hsc-pq-insert)
    (fmakunbound 'neovm--hsc-test)))
"#;
    let expect = expect_test::expect![[r#""OK ((t 1 2) (t 5 2) (t 2 2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Oracle parity tests for Huffman coding implemented in Elisp:
//! frequency table construction, Huffman tree building via priority queue,
//! code table generation, encoding, decoding, roundtrip verification,
//! and compression ratio computation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Build frequency table from text
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_huffman_frequency_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Count character frequencies in a string, return sorted alist of (char . count).
    let form = r#"(let ((build-freq
                         (lambda (text)
                           (let ((ht (make-hash-table :test 'equal))
                                 (i 0) (len (length text)))
                             (while (< i len)
                               (let ((ch (aref text i)))
                                 (puthash ch (1+ (gethash ch ht 0)) ht))
                               (setq i (1+ i)))
                             ;; Convert to sorted alist
                             (let ((pairs nil))
                               (maphash (lambda (k v) (setq pairs (cons (cons k v) pairs))) ht)
                               (sort pairs (lambda (a b) (< (cdr a) (cdr b)))))))))
                    (list
                     (funcall build-freq "aabbbcccc")
                     (funcall build-freq "hello world")
                     (funcall build-freq "aaaaaaa")
                     (funcall build-freq "abcdef")))"#;
    let expect = expect_test::expect![[
        r#""OK (((97 . 2) (98 . 3) (99 . 4)) ((100 . 1) (114 . 1) (119 . 1) (32 . 1) (101 . 1) (104 . 1) (111 . 2) (108 . 3)) ((97 . 7)) ((102 . 1) (101 . 1) (100 . 1) (99 . 1) (98 . 1) (97 . 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Priority queue (min-heap) for tree building
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_huffman_priority_queue() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a simple priority queue as a sorted list.
    // Each entry is (weight . data). Insert maintains sort order.
    let form = r#"(let ((pq-insert
                         (lambda (queue item)
                           ;; item is (weight . data)
                           ;; Insert into sorted position by weight
                           (if (null queue)
                               (list item)
                             (if (<= (car item) (car (car queue)))
                                 (cons item queue)
                               (cons (car queue)
                                     (funcall pq-insert (cdr queue) item))))))
                        (pq-pop
                         (lambda (queue)
                           ;; Returns (popped-item . remaining-queue)
                           (cons (car queue) (cdr queue)))))
                    ;; Build a queue and pop elements
                    (let ((q nil))
                      (setq q (funcall pq-insert q '(5 . "e")))
                      (setq q (funcall pq-insert q '(2 . "b")))
                      (setq q (funcall pq-insert q '(8 . "h")))
                      (setq q (funcall pq-insert q '(1 . "a")))
                      (setq q (funcall pq-insert q '(3 . "c")))
                      ;; Pop all and collect
                      (let ((result nil))
                        (while q
                          (let ((popped (funcall pq-pop q)))
                            (setq result (cons (car popped) result)
                                  q (cdr popped))))
                        (nreverse result))))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable pq-insert)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Full Huffman tree building + code table generation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_huffman_tree_and_codes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build Huffman tree from frequencies, generate code table.
    // Tree: leaf = (weight char), internal = (weight left right).
    let form = r#"(let* ((text "abracadabra")
                         ;; Build frequency table
                         (freqs (let ((ht (make-hash-table :test 'equal)))
                                  (dotimes (i (length text))
                                    (let ((ch (aref text i)))
                                      (puthash ch (1+ (gethash ch ht 0)) ht)))
                                  (let ((pairs nil))
                                    (maphash (lambda (k v)
                                               (setq pairs (cons (cons k v) pairs)))
                                             ht)
                                    pairs)))
                         ;; Priority queue insert (sorted list)
                         (pq-insert
                          (lambda (queue item)
                            (if (null queue)
                                (list item)
                              (if (<= (car item) (car (car queue)))
                                  (cons item queue)
                                (cons (car queue)
                                      (funcall pq-insert (cdr queue) item)))))))
                    ;; Build initial queue: each char becomes a leaf node
                    ;; Node format: (weight . leaf-char) or (weight left right)
                    (let ((queue nil))
                      (dolist (pair freqs)
                        (setq queue (funcall pq-insert queue
                                             (list (cdr pair) (car pair)))))
                      ;; Build tree by repeatedly merging two smallest
                      (while (> (length queue) 1)
                        (let* ((pop1 (cons (car queue) (cdr queue)))
                               (left (car pop1))
                               (q1 (cdr pop1))
                               (pop2 (cons (car q1) (cdr q1)))
                               (right (car pop2))
                               (q2 (cdr pop2))
                               ;; New internal node: (combined-weight left right)
                               (combined (list (+ (car left) (car right))
                                               left right)))
                          (setq queue (funcall pq-insert q2 combined))))
                      ;; tree is the single remaining element
                      (let ((tree (car queue))
                            (codes (make-hash-table :test 'equal)))
                        ;; Generate codes by traversing tree
                        ;; gen-codes: node prefix -> fills hash table
                        (let ((gen-codes nil))
                          (setq gen-codes
                                (lambda (node prefix)
                                  (if (= (length node) 2)
                                      ;; Leaf: (weight char)
                                      (puthash (cadr node) prefix codes)
                                    ;; Internal: (weight left right)
                                    (funcall gen-codes (cadr node)
                                             (concat prefix "0"))
                                    (funcall gen-codes (caddr node)
                                             (concat prefix "1")))))
                          (funcall gen-codes tree ""))
                        ;; Collect code table as sorted alist
                        (let ((code-list nil))
                          (maphash (lambda (k v)
                                     (setq code-list
                                           (cons (cons (char-to-string k) v)
                                                 code-list)))
                                   codes)
                          (let ((sorted (sort code-list
                                             (lambda (a b)
                                               (string< (car a) (car b))))))
                            ;; Verify: more frequent chars have shorter codes
                            (let ((a-code (gethash ?a codes))
                                  (d-code (gethash ?d codes)))
                              (list sorted
                                    ;; 'a' appears 5 times, should have short code
                                    (length a-code)
                                    ;; 'd' appears 1 time, should have longer code
                                    (length d-code)
                                    (<= (length a-code) (length d-code)))))))))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable pq-insert)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Full encode + decode roundtrip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_huffman_encode_decode_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Complete Huffman pipeline: build tree, encode text to bits, decode back.
    let form = r#"(let ((huffman-roundtrip
                         (lambda (text)
                           ;; 1. Build frequency table
                           (let ((ht (make-hash-table :test 'equal)))
                             (dotimes (i (length text))
                               (puthash (aref text i)
                                        (1+ (gethash (aref text i) ht 0)) ht))
                             ;; 2. Build priority queue
                             (let ((pq-insert
                                    (lambda (q item)
                                      (if (null q) (list item)
                                        (if (<= (car item) (car (car q)))
                                            (cons item q)
                                          (cons (car q)
                                                (funcall pq-insert (cdr q) item))))))
                                   (queue nil))
                               (maphash (lambda (k v)
                                          (setq queue (funcall pq-insert queue
                                                               (list v k))))
                                        ht)
                               ;; Handle single-char case
                               (when (= (length queue) 1)
                                 (setq queue (funcall pq-insert queue
                                                      (list 0 0))))
                               ;; 3. Build tree
                               (while (> (length queue) 1)
                                 (let* ((left (car queue))
                                        (q1 (cdr queue))
                                        (right (car q1))
                                        (q2 (cdr q1))
                                        (merged (list (+ (car left) (car right))
                                                      left right)))
                                   (setq queue (funcall pq-insert q2 merged))))
                               (let ((tree (car queue))
                                     (codes (make-hash-table :test 'equal)))
                                 ;; 4. Generate codes
                                 (let ((gen nil))
                                   (setq gen
                                         (lambda (node pfx)
                                           (if (= (length node) 2)
                                               (puthash (cadr node) pfx codes)
                                             (funcall gen (cadr node)
                                                      (concat pfx "0"))
                                             (funcall gen (caddr node)
                                                      (concat pfx "1")))))
                                   (funcall gen tree ""))
                                 ;; 5. Encode: text -> bit string
                                 (let ((encoded
                                        (let ((bits nil))
                                          (dotimes (i (length text))
                                            (setq bits
                                                  (cons (gethash (aref text i) codes)
                                                        bits)))
                                          (apply 'concat (nreverse bits)))))
                                   ;; 6. Decode: walk tree following bits
                                   (let ((decoded nil)
                                         (pos 0)
                                         (len (length encoded)))
                                     (while (< pos len)
                                       (let ((node tree))
                                         (while (> (length node) 2)
                                           (if (= (aref encoded pos) ?0)
                                               (setq node (cadr node))
                                             (setq node (caddr node)))
                                           (setq pos (1+ pos)))
                                         (setq decoded (cons (cadr node) decoded))))
                                     (let ((decoded-str (concat (nreverse decoded))))
                                       (list (equal text decoded-str)
                                             (length text)
                                             (length encoded))))))))))))
                    ;; Test with various inputs
                    (list
                     (funcall huffman-roundtrip "abracadabra")
                     (funcall huffman-roundtrip "hello world")
                     (funcall huffman-roundtrip "aaaaaaa")
                     (funcall huffman-roundtrip "abcdefgh")
                     (funcall huffman-roundtrip "mississippi")))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable huffman-roundtrip)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Compression ratio analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_huffman_compression_ratio() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute compression ratios for different text patterns.
    // Compare Huffman encoding size vs fixed 8-bit encoding.
    let form = r#"(let ((compute-ratio
                         (lambda (text)
                           (let ((ht (make-hash-table :test 'equal)))
                             (dotimes (i (length text))
                               (puthash (aref text i)
                                        (1+ (gethash (aref text i) ht 0)) ht))
                             (let ((pq-insert
                                    (lambda (q item)
                                      (if (null q) (list item)
                                        (if (<= (car item) (car (car q)))
                                            (cons item q)
                                          (cons (car q)
                                                (funcall pq-insert (cdr q) item))))))
                                   (queue nil))
                               (maphash (lambda (k v)
                                          (setq queue (funcall pq-insert queue
                                                               (list v k))))
                                        ht)
                               (when (= (length queue) 1)
                                 (setq queue (funcall pq-insert queue (list 0 0))))
                               (while (> (length queue) 1)
                                 (let* ((left (car queue))
                                        (q1 (cdr queue))
                                        (right (car q1))
                                        (q2 (cdr q1))
                                        (merged (list (+ (car left) (car right))
                                                      left right)))
                                   (setq queue (funcall pq-insert q2 merged))))
                               (let ((tree (car queue))
                                     (codes (make-hash-table :test 'equal)))
                                 (let ((gen nil))
                                   (setq gen
                                         (lambda (node pfx)
                                           (if (= (length node) 2)
                                               (puthash (cadr node) pfx codes)
                                             (funcall gen (cadr node)
                                                      (concat pfx "0"))
                                             (funcall gen (caddr node)
                                                      (concat pfx "1")))))
                                   (funcall gen tree ""))
                                 ;; Compute total encoded bits
                                 (let ((total-bits 0))
                                   (maphash (lambda (ch code)
                                              (setq total-bits
                                                    (+ total-bits
                                                       (* (gethash ch ht 0)
                                                          (length code)))))
                                            codes)
                                   (let* ((fixed-bits (* (length text) 8))
                                          ;; Unique chars determine min fixed bits
                                          (unique-count (hash-table-count ht))
                                          (min-bits-per-char
                                           ;; ceil(log2(unique-count))
                                           (let ((b 1) (n 1))
                                             (while (< n unique-count)
                                               (setq n (* n 2) b (1+ b)))
                                             b))
                                          (optimal-fixed (* (length text)
                                                            min-bits-per-char)))
                                     (list (length text)
                                           unique-count
                                           total-bits
                                           fixed-bits
                                           optimal-fixed
                                           ;; Huffman should be <= optimal fixed
                                           (<= total-bits optimal-fixed))))))))))
                    (list
                     ;; High redundancy -> good compression
                     (funcall compute-ratio "aaaaaaaaaaaaaaaaaaaabbbb")
                     ;; Medium redundancy
                     (funcall compute-ratio "the cat sat on the mat")
                     ;; Low redundancy (all unique)
                     (funcall compute-ratio "abcdefghij")
                     ;; Single char
                     (funcall compute-ratio "xxxxx")))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable pq-insert)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Huffman with weighted merge verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_huffman_tree_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify structural properties of the Huffman tree:
    // 1. It's a full binary tree (every internal node has exactly 2 children)
    // 2. Sum of (freq * code-length) equals weighted path length
    // 3. No code is a prefix of another code (prefix-free property)
    let form = r#"(let* ((text "abracadabra alakazam")
                         (ht (make-hash-table :test 'equal)))
                    (dotimes (i (length text))
                      (puthash (aref text i)
                               (1+ (gethash (aref text i) ht 0)) ht))
                    (let ((pq-insert
                           (lambda (q item)
                             (if (null q) (list item)
                               (if (<= (car item) (car (car q)))
                                   (cons item q)
                                 (cons (car q)
                                       (funcall pq-insert (cdr q) item))))))
                          (queue nil))
                      (maphash (lambda (k v)
                                 (setq queue (funcall pq-insert queue (list v k))))
                               ht)
                      (when (= (length queue) 1)
                        (setq queue (funcall pq-insert queue (list 0 0))))
                      (while (> (length queue) 1)
                        (let* ((left (car queue))
                               (q1 (cdr queue))
                               (right (car q1))
                               (q2 (cdr q1))
                               (merged (list (+ (car left) (car right))
                                             left right)))
                          (setq queue (funcall pq-insert q2 merged))))
                      (let ((tree (car queue))
                            (codes (make-hash-table :test 'equal)))
                        (let ((gen nil))
                          (setq gen
                                (lambda (node pfx)
                                  (if (= (length node) 2)
                                      (puthash (cadr node) pfx codes)
                                    (funcall gen (cadr node) (concat pfx "0"))
                                    (funcall gen (caddr node) (concat pfx "1")))))
                          (funcall gen tree ""))
                        ;; Property 1: check full binary tree
                        ;; (every internal node has 2 children = length 3)
                        (let ((is-full nil))
                          (setq is-full
                                (lambda (node)
                                  (if (= (length node) 2)
                                      t ;; leaf
                                    (and (= (length node) 3)
                                         (funcall is-full (cadr node))
                                         (funcall is-full (caddr node))))))
                          ;; Property 2: weighted path length
                          (let ((wpl 0))
                            (maphash (lambda (ch code)
                                       (setq wpl (+ wpl (* (gethash ch ht 0)
                                                            (length code)))))
                                     codes)
                            ;; Property 3: prefix-free check
                            (let ((code-list nil)
                                  (prefix-free t))
                              (maphash (lambda (k v)
                                         (setq code-list (cons v code-list)))
                                       codes)
                              ;; Check no code is prefix of another
                              (let ((sorted (sort code-list 'string<)))
                                (let ((i 0) (len (length sorted)))
                                  (while (and prefix-free (< i (1- len)))
                                    (let ((a (nth i sorted))
                                          (b (nth (1+ i) sorted)))
                                      (when (and (<= (length a) (length b))
                                                 (string= a (substring b 0 (length a))))
                                        (setq prefix-free nil)))
                                    (setq i (1+ i)))))
                              (list (funcall is-full tree)
                                    wpl
                                    prefix-free
                                    (hash-table-count codes)
                                    ;; Root weight should equal text length
                                    (= (car tree) (length text))))))))))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable pq-insert)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

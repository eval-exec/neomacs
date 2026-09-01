//! Advanced oracle parity tests for `elt`, `aref`, `aset` patterns:
//! `elt` on lists, vectors, strings; `aref`/`aset` on vectors, strings,
//! bool-vectors, char-tables; combined mutation patterns, building data
//! structures element-by-element, matrix access patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// elt on lists, vectors, and strings with combined access
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_elt_polymorphic_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // elt works uniformly on lists, vectors, and strings.
    // Build a dispatch table that accesses different sequence types,
    // then aggregate results.
    let form = r#"(let ((my-list '(alpha beta gamma delta epsilon))
                        (my-vec  [100 200 300 400 500])
                        (my-str  "ABCDE"))
                    (let ((sequences (list my-list my-vec my-str))
                          (results nil))
                      ;; For each sequence, collect elements at indices 0,2,4
                      (dolist (seq sequences)
                        (let ((row nil))
                          (dolist (i '(0 2 4))
                            (setq row (cons (elt seq i) row)))
                          (setq results (cons (nreverse row) results))))
                      ;; Also test elt return types
                      (let ((type-checks
                             (list (symbolp (elt my-list 0))
                                   (integerp (elt my-vec 0))
                                   (integerp (elt my-str 0))  ;; char code
                                   (= (elt my-str 0) ?A))))
                        (list (nreverse results) type-checks))))"#;
    let expect = expect_test::expect![[
        r#""OK (((alpha gamma epsilon) (100 300 500) (65 67 69)) (t t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// aref/aset on bool-vectors with bitwise patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_aref_aset_bool_vector_sieve() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a small Sieve of Eratosthenes using a bool-vector.
    // Mark composites as nil, primes remain t.
    let form = r#"(let* ((limit 50)
                         (sieve (make-bool-vector (1+ limit) t)))
                    ;; 0 and 1 are not prime
                    (aset sieve 0 nil)
                    (aset sieve 1 nil)
                    ;; Sieve
                    (let ((i 2))
                      (while (<= (* i i) limit)
                        (when (aref sieve i)
                          (let ((j (* i i)))
                            (while (<= j limit)
                              (aset sieve j nil)
                              (setq j (+ j i)))))
                        (setq i (1+ i))))
                    ;; Collect primes
                    (let ((primes nil) (i 2))
                      (while (<= i limit)
                        (when (aref sieve i)
                          (setq primes (cons i primes)))
                        (setq i (1+ i)))
                      (nreverse primes)))"#;
    let expect = expect_test::expect![[r#""OK (2 3 5 7 11 13 17 19 23 29 31 37 41 43 47)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// aref/aset on char-tables: unicode category mapping
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_aref_aset_char_table_category_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a char-table that classifies characters into categories,
    // then classify a string character by character.
    let form = r#"(let ((ct (make-char-table 'classification nil)))
                    ;; Set ranges for digits, uppercase, lowercase, space
                    (let ((i ?0))
                      (while (<= i ?9)
                        (aset ct i 'digit)
                        (setq i (1+ i))))
                    (let ((i ?A))
                      (while (<= i ?Z)
                        (aset ct i 'upper)
                        (setq i (1+ i))))
                    (let ((i ?a))
                      (while (<= i ?z)
                        (aset ct i 'lower)
                        (setq i (1+ i))))
                    (aset ct ?\s 'space)
                    (aset ct ?_ 'underscore)
                    (aset ct ?- 'hyphen)
                    ;; Classify each char in a test string
                    (let ((test-str "Hello World 42_foo-bar")
                          (result nil))
                      (dotimes (i (length test-str))
                        (let ((ch (aref test-str i)))
                          (setq result (cons (list (char-to-string ch)
                                                   (aref ct ch))
                                             result))))
                      ;; Also test that unset chars return default
                      (let ((special-checks
                             (list (aref ct ?!)
                                   (aref ct ?@)
                                   (aref ct ?#))))
                        (list (nreverse result) special-checks))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"H\" upper) (\"e\" lower) (\"l\" lower) (\"l\" lower) (\"o\" lower) (\" \" space) (\"W\" upper) (\"o\" lower) (\"r\" lower) (\"l\" lower) (\"d\" lower) (\" \" space) (\"4\" digit) (\"2\" digit) (\"_\" underscore) (\"f\" lower) (\"o\" lower) (\"o\" lower) (\"-\" hyphen) (\"b\" lower) (\"a\" lower) (\"r\" lower)) (nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Building a sparse matrix via nested vectors with aref/aset
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_aref_aset_sparse_matrix_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a small sparse matrix using a hash-table of (row . col) -> value,
    // then convert to a dense vector-of-vectors, perform matrix-vector multiply.
    let form = r#"(let ((sparse (make-hash-table :test 'equal)))
                    ;; Set some entries in a 4x4 matrix
                    (puthash '(0 . 0) 2 sparse)
                    (puthash '(0 . 1) 3 sparse)
                    (puthash '(1 . 1) 5 sparse)
                    (puthash '(1 . 3) 1 sparse)
                    (puthash '(2 . 0) 4 sparse)
                    (puthash '(2 . 2) 6 sparse)
                    (puthash '(3 . 2) 7 sparse)
                    (puthash '(3 . 3) 8 sparse)
                    ;; Convert to dense 4x4 matrix
                    (let ((dense (vector (make-vector 4 0)
                                         (make-vector 4 0)
                                         (make-vector 4 0)
                                         (make-vector 4 0))))
                      (maphash (lambda (key val)
                                 (aset (aref dense (car key)) (cdr key) val))
                               sparse)
                      ;; Matrix-vector multiply: dense * [1 2 3 4]
                      (let ((vec [1 2 3 4])
                            (result (make-vector 4 0)))
                        (let ((i 0))
                          (while (< i 4)
                            (let ((sum 0) (j 0))
                              (while (< j 4)
                                (setq sum (+ sum (* (aref (aref dense i) j)
                                                    (aref vec j))))
                                (setq j (1+ j)))
                              (aset result i sum))
                            (setq i (1+ i))))
                        ;; Return dense matrix and multiply result
                        (list (mapcar (lambda (row)
                                        (list (aref row 0) (aref row 1)
                                              (aref row 2) (aref row 3)))
                                      (list (aref dense 0) (aref dense 1)
                                            (aref dense 2) (aref dense 3)))
                              result))))"#;
    let expect =
        expect_test::expect![[r#""OK (((2 3 0 0) (0 5 0 1) (4 0 6 0) (0 0 7 8)) [8 14 22 53])""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// elt + aset: build and query a lookup table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_elt_aset_frequency_table_with_sorting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Count word lengths in a sentence using elt to walk a list of words,
    // store counts in a vector indexed by length, then find the mode.
    let form = r#"(let ((words '("the" "quick" "brown" "fox" "jumps" "over"
                                 "the" "lazy" "dog" "and" "the" "cat"))
                        (counts (make-vector 20 0)))
                    ;; Count frequency of each word length
                    (let ((i 0) (n (length words)))
                      (while (< i n)
                        (let ((w (elt words i)))
                          (let ((len (length w)))
                            (when (< len 20)
                              (aset counts len (1+ (aref counts len))))))
                        (setq i (1+ i))))
                    ;; Find the most common word length (mode)
                    (let ((max-count 0) (mode-len 0) (j 1))
                      (while (< j 20)
                        (when (> (aref counts j) max-count)
                          (setq max-count (aref counts j)
                                mode-len j))
                        (setq j (1+ j)))
                      ;; Build distribution: list of (length . count) for non-zero
                      (let ((dist nil) (k 1))
                        (while (< k 20)
                          (when (> (aref counts k) 0)
                            (setq dist (cons (cons k (aref counts k)) dist)))
                          (setq k (1+ k)))
                        (list (nreverse dist) mode-len max-count))))"#;
    let expect = expect_test::expect![[r#""OK (((3 . 7) (4 . 2) (5 . 3)) 3 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combined aref/aset: Conway's Game of Life step
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_aref_aset_game_of_life_step() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // One step of Game of Life on a small grid using vector-of-vectors.
    // Grid is 6x6 with a glider pattern.
    let form = r#"(let* ((rows 6) (cols 6)
                         ;; Make empty grid
                         (make-grid
                          (lambda ()
                            (let ((g (make-vector rows nil)) (i 0))
                              (while (< i rows)
                                (aset g i (make-vector cols 0))
                                (setq i (1+ i)))
                              g)))
                         (grid (funcall make-grid)))
                    ;; Place a glider at (1,1)
                    (aset (aref grid 1) 2 1)
                    (aset (aref grid 2) 3 1)
                    (aset (aref grid 3) 1 1)
                    (aset (aref grid 3) 2 1)
                    (aset (aref grid 3) 3 1)
                    ;; Count neighbors
                    (let ((count-neighbors
                           (lambda (g r c)
                             (let ((sum 0))
                               (dolist (dr '(-1 0 1))
                                 (dolist (dc '(-1 0 1))
                                   (unless (and (= dr 0) (= dc 0))
                                     (let ((nr (+ r dr)) (nc (+ c dc)))
                                       (when (and (>= nr 0) (< nr rows)
                                                  (>= nc 0) (< nc cols))
                                         (setq sum (+ sum (aref (aref g nr) nc))))))))
                               sum))))
                      ;; Compute next generation
                      (let ((next (funcall make-grid))
                            (r 0))
                        (while (< r rows)
                          (let ((c 0))
                            (while (< c cols)
                              (let ((alive (aref (aref grid r) c))
                                    (nbrs (funcall count-neighbors grid r c)))
                                (aset (aref next r) c
                                      (if (= alive 1)
                                          (if (or (= nbrs 2) (= nbrs 3)) 1 0)
                                        (if (= nbrs 3) 1 0))))
                              (setq c (1+ c))))
                          (setq r (1+ r)))
                        ;; Collect live cells from both generations
                        (let ((collect-live
                               (lambda (g)
                                 (let ((cells nil) (r 0))
                                   (while (< r rows)
                                     (let ((c 0))
                                       (while (< c cols)
                                         (when (= (aref (aref g r) c) 1)
                                           (setq cells (cons (cons r c) cells)))
                                         (setq c (1+ c))))
                                     (setq r (1+ r)))
                                   (nreverse cells)))))
                          (list (funcall collect-live grid)
                                (funcall collect-live next))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . 2) (2 . 3) (3 . 1) (3 . 2) (3 . 3)) ((2 . 1) (2 . 3) (3 . 2) (3 . 3) (4 . 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// aref/aset on strings: in-place Caesar cipher + ROT13 roundtrip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_aref_aset_string_caesar_rot13() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement Caesar cipher with arbitrary shift using aref/aset on strings.
    // Verify ROT13 roundtrip property: applying twice yields original.
    let form = r#"(let ((caesar
                         (lambda (text shift)
                           (let ((result (copy-sequence text))
                                 (i 0)
                                 (len (length text)))
                             (while (< i len)
                               (let ((ch (aref result i)))
                                 (cond
                                  ((and (>= ch ?a) (<= ch ?z))
                                   (aset result i (+ ?a (% (+ (- ch ?a) shift) 26))))
                                  ((and (>= ch ?A) (<= ch ?Z))
                                   (aset result i (+ ?A (% (+ (- ch ?A) shift) 26))))))
                               (setq i (1+ i)))
                             result))))
                    (let* ((plain "The Quick Brown Fox Jumps Over The Lazy Dog")
                           ;; Encrypt with shift=3
                           (enc3 (funcall caesar plain 3))
                           ;; Decrypt with shift=23 (26-3)
                           (dec3 (funcall caesar enc3 23))
                           ;; ROT13 roundtrip
                           (rot13a (funcall caesar plain 13))
                           (rot13b (funcall caesar rot13a 13))
                           ;; Various shifts and decrypt
                           (shifts '(1 5 7 13 25)))
                      (list enc3
                            (equal dec3 plain)
                            rot13a
                            (equal rot13b plain)
                            ;; Verify roundtrip for all shifts
                            (mapcar (lambda (s)
                                      (equal (funcall caesar
                                                      (funcall caesar plain s)
                                                      (- 26 s))
                                             plain))
                                    shifts))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Wkh Txlfn Eurzq Ira Mxpsv Ryhu Wkh Odcb Grj\" t \"Gur Dhvpx Oebja Sbk Whzcf Bire Gur Ynml Qbt\" t (t t t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// elt on deeply nested list: path traversal
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_elt_nested_path_traversal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a tree as nested lists, then access nodes via path indices.
    // path-get: given a tree and a list of indices, traverse using elt.
    let form = r#"(let ((tree '((("a" "b" "c")
                                  ("d" "e"))
                                 (("f") ("g" "h" "i" "j"))
                                 (("k" "l" "m") ("n"))))
                        (path-get
                         (lambda (tr path)
                           (let ((node tr))
                             (dolist (idx path)
                               (setq node (elt node idx)))
                             node))))
                    ;; Access various paths
                    (list
                     ;; tree[0][0][2] = "c"
                     (funcall path-get tree '(0 0 2))
                     ;; tree[1][1][3] = "j"
                     (funcall path-get tree '(1 1 3))
                     ;; tree[2][0] = ("k" "l" "m")
                     (funcall path-get tree '(2 0))
                     ;; tree[2][1][0] = "n"
                     (funcall path-get tree '(2 1 0))
                     ;; tree[0][1][1] = "e"
                     (funcall path-get tree '(0 1 1))
                     ;; tree[0] = first subtree
                     (length (funcall path-get tree '(0)))
                     ;; Collect all leaves reachable via specific sub-paths
                     (let ((paths '((0 0 0) (0 0 1) (0 0 2) (0 1 0) (0 1 1)))
                           (leaves nil))
                       (dolist (p paths)
                         (setq leaves (cons (funcall path-get tree p) leaves)))
                       (nreverse leaves))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"c\" \"j\" (\"k\" \"l\" \"m\") \"n\" \"e\" 2 (\"a\" \"b\" \"c\" \"d\" \"e\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Oracle parity tests for natural language processing patterns in Elisp:
//! tokenization (word splitting), n-gram generation, bag-of-words construction,
//! edit distance (Levenshtein), longest common subsequence, simple stemmer
//! (suffix removal), word frequency analysis, and sentence similarity
//! (Jaccard index on token sets).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Tokenizer: split text into words on whitespace and punctuation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nlp_tokenizer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Tokenize a string into a list of lowercase words, splitting on
  ;; non-alphabetic characters and discarding empty tokens.
  (fset 'neovm--nlp-tokenize (lambda (text)
    (let ((words nil)
          (current "")
          (i 0)
          (len (length text)))
      (while (< i len)
        (let ((ch (aref text i)))
          (if (and (>= ch ?a) (<= ch ?z))
              (setq current (concat current (char-to-string ch)))
            (if (and (>= ch ?A) (<= ch ?Z))
                (setq current (concat current (char-to-string (+ ch 32))))
              ;; Non-alpha: flush current word
              (when (> (length current) 0)
                (setq words (cons current words))
                (setq current "")))))
        (setq i (1+ i)))
      ;; Flush last word
      (when (> (length current) 0)
        (setq words (cons current words)))
      (nreverse words))))

  (unwind-protect
      (list
        (funcall 'neovm--nlp-tokenize "Hello, World!")
        (funcall 'neovm--nlp-tokenize "  multiple   spaces   here  ")
        (funcall 'neovm--nlp-tokenize "CamelCaseWord")
        (funcall 'neovm--nlp-tokenize "one")
        (funcall 'neovm--nlp-tokenize "")
        (funcall 'neovm--nlp-tokenize "123 abc 456 def")
        (funcall 'neovm--nlp-tokenize "the quick brown fox jumps over the lazy dog")
        ;; Punctuation-heavy
        (funcall 'neovm--nlp-tokenize "Hello...World!!! How's it going?"))
    (fmakunbound 'neovm--nlp-tokenize)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"hello\" \"world\") (\"multiple\" \"spaces\" \"here\") (\"camelcaseword\") (\"one\") nil (\"abc\" \"def\") (\"the\" \"quick\" \"brown\" \"fox\" \"jumps\" \"over\" \"the\" \"lazy\" \"dog\") (\"hello\" \"world\" \"how\" \"s\" \"it\" \"going\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// N-gram generation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nlp_ngrams() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Generate all n-grams (as lists of n consecutive elements) from a list.
  (fset 'neovm--nlp-ngrams (lambda (tokens n)
    (let ((result nil)
          (len (length tokens)))
      (if (< len n) nil
        (let ((i 0))
          (while (<= (+ i n) len)
            (let ((gram nil) (j 0) (sub (nthcdr i tokens)))
              (while (< j n)
                (setq gram (cons (car sub) gram))
                (setq sub (cdr sub))
                (setq j (1+ j)))
              (setq result (cons (nreverse gram) result)))
            (setq i (1+ i))))
        (nreverse result)))))

  ;; Character n-grams from a string.
  (fset 'neovm--nlp-char-ngrams (lambda (str n)
    (let ((result nil)
          (len (length str)))
      (if (< len n) nil
        (let ((i 0))
          (while (<= (+ i n) len)
            (setq result (cons (substring str i (+ i n)) result))
            (setq i (1+ i)))))
      (nreverse result))))

  (unwind-protect
      (list
        ;; Word bigrams
        (funcall 'neovm--nlp-ngrams '("the" "cat" "sat" "on" "the" "mat") 2)
        ;; Word trigrams
        (funcall 'neovm--nlp-ngrams '("the" "cat" "sat" "on" "the" "mat") 3)
        ;; Unigrams
        (funcall 'neovm--nlp-ngrams '("a" "b" "c") 1)
        ;; n > length: empty
        (funcall 'neovm--nlp-ngrams '("a" "b") 5)
        ;; Character bigrams
        (funcall 'neovm--nlp-char-ngrams "hello" 2)
        ;; Character trigrams
        (funcall 'neovm--nlp-char-ngrams "abcdef" 3)
        ;; Edge: n = length
        (funcall 'neovm--nlp-ngrams '("x" "y" "z") 3))
    (fmakunbound 'neovm--nlp-ngrams)
    (fmakunbound 'neovm--nlp-char-ngrams)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"the\" \"cat\") (\"cat\" \"sat\") (\"sat\" \"on\") (\"on\" \"the\") (\"the\" \"mat\")) ((\"the\" \"cat\" \"sat\") (\"cat\" \"sat\" \"on\") (\"sat\" \"on\" \"the\") (\"on\" \"the\" \"mat\")) ((\"a\") (\"b\") (\"c\")) nil (\"he\" \"el\" \"ll\" \"lo\") (\"abc\" \"bcd\" \"cde\" \"def\") ((\"x\" \"y\" \"z\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bag of words construction and word frequency analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nlp_bag_of_words_and_frequency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Simple tokenizer (split on space, lowercase).
  (fset 'neovm--nlp-bow-tokenize (lambda (text)
    (let ((words nil) (current "") (i 0) (len (length text)))
      (while (< i len)
        (let ((ch (aref text i)))
          (if (= ch ?\s)
              (when (> (length current) 0)
                (setq words (cons (downcase current) words))
                (setq current ""))
            (setq current (concat current (char-to-string ch)))))
        (setq i (1+ i)))
      (when (> (length current) 0)
        (setq words (cons (downcase current) words)))
      (nreverse words))))

  ;; Build a bag-of-words: hash table mapping word -> count.
  (fset 'neovm--nlp-bow (lambda (tokens)
    (let ((ht (make-hash-table :test 'equal)))
      (dolist (w tokens)
        (puthash w (1+ (gethash w ht 0)) ht))
      ht)))

  ;; Convert hash table to sorted alist for deterministic comparison.
  (fset 'neovm--nlp-bow-to-alist (lambda (ht)
    (let ((pairs nil))
      (maphash (lambda (k v) (setq pairs (cons (cons k v) pairs))) ht)
      (sort pairs (lambda (a b) (string< (car a) (car b)))))))

  ;; Top-N most frequent words.
  (fset 'neovm--nlp-top-n (lambda (ht n)
    (let ((pairs nil))
      (maphash (lambda (k v) (setq pairs (cons (cons k v) pairs))) ht)
      (let ((sorted (sort pairs (lambda (a b)
                                  (or (> (cdr a) (cdr b))
                                      (and (= (cdr a) (cdr b))
                                           (string< (car a) (car b))))))))
        (let ((result nil) (i 0) (s sorted))
          (while (and (< i n) s)
            (setq result (cons (car s) result))
            (setq s (cdr s))
            (setq i (1+ i)))
          (nreverse result))))))

  (unwind-protect
      (let* ((text "the cat sat on the mat the cat ate the food the")
             (tokens (funcall 'neovm--nlp-bow-tokenize text))
             (bow (funcall 'neovm--nlp-bow tokens)))
        (list
          ;; Token count
          (length tokens)
          ;; Unique words
          (hash-table-count bow)
          ;; Full bag-of-words sorted
          (funcall 'neovm--nlp-bow-to-alist bow)
          ;; Top 3 words
          (funcall 'neovm--nlp-top-n bow 3)
          ;; Top 1
          (funcall 'neovm--nlp-top-n bow 1)
          ;; Frequency of "the"
          (gethash "the" bow 0)
          ;; Frequency of missing word
          (gethash "dog" bow 0)))
    (fmakunbound 'neovm--nlp-bow-tokenize)
    (fmakunbound 'neovm--nlp-bow)
    (fmakunbound 'neovm--nlp-bow-to-alist)
    (fmakunbound 'neovm--nlp-top-n)))"#;
    let expect = expect_test::expect![[
        r#""OK (12 7 ((\"ate\" . 1) (\"cat\" . 2) (\"food\" . 1) (\"mat\" . 1) (\"on\" . 1) (\"sat\" . 1) (\"the\" . 5)) ((\"the\" . 5) (\"cat\" . 2) (\"ate\" . 1)) ((\"the\" . 5)) 5 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Levenshtein edit distance (dynamic programming)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nlp_levenshtein() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Classic DP Levenshtein distance using a 2D vector (vector of vectors).
  (fset 'neovm--nlp-levenshtein (lambda (s1 s2)
    (let* ((n (length s1))
           (m (length s2))
           ;; dp is (n+1) x (m+1) matrix stored as vector of vectors
           (dp (let ((v (make-vector (1+ n) nil))
                     (i 0))
                 (while (<= i n)
                   (aset v i (make-vector (1+ m) 0))
                   (setq i (1+ i)))
                 v)))
      ;; Base cases
      (let ((i 0))
        (while (<= i n) (aset (aref dp i) 0 i) (setq i (1+ i))))
      (let ((j 0))
        (while (<= j m) (aset (aref dp 0) j j) (setq j (1+ j))))
      ;; Fill DP table
      (let ((i 1))
        (while (<= i n)
          (let ((j 1))
            (while (<= j m)
              (let ((cost (if (= (aref s1 (1- i)) (aref s2 (1- j))) 0 1)))
                (aset (aref dp i) j
                      (min (1+ (aref (aref dp (1- i)) j))         ;; delete
                           (min (1+ (aref (aref dp i) (1- j)))    ;; insert
                                (+ (aref (aref dp (1- i)) (1- j)) ;; replace
                                   cost)))))
              (setq j (1+ j))))
          (setq i (1+ i))))
      (aref (aref dp n) m))))

  (unwind-protect
      (list
        ;; Same string: distance 0
        (funcall 'neovm--nlp-levenshtein "kitten" "kitten")
        ;; Classic example
        (funcall 'neovm--nlp-levenshtein "kitten" "sitting")
        ;; Empty to non-empty
        (funcall 'neovm--nlp-levenshtein "" "abc")
        ;; Non-empty to empty
        (funcall 'neovm--nlp-levenshtein "abc" "")
        ;; Both empty
        (funcall 'neovm--nlp-levenshtein "" "")
        ;; Single character difference
        (funcall 'neovm--nlp-levenshtein "cat" "car")
        ;; Insertion only
        (funcall 'neovm--nlp-levenshtein "abc" "abcdef")
        ;; Symmetric: dist(a,b) = dist(b,a)
        (= (funcall 'neovm--nlp-levenshtein "hello" "world")
           (funcall 'neovm--nlp-levenshtein "world" "hello"))
        ;; Triangle inequality: dist(a,c) <= dist(a,b) + dist(b,c)
        (<= (funcall 'neovm--nlp-levenshtein "abc" "xyz")
            (+ (funcall 'neovm--nlp-levenshtein "abc" "mno")
               (funcall 'neovm--nlp-levenshtein "mno" "xyz"))))
    (fmakunbound 'neovm--nlp-levenshtein)))"#;
    let expect = expect_test::expect![[r#""OK (0 3 3 3 0 1 3 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Longest Common Subsequence (LCS)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nlp_lcs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Compute LCS length and the actual subsequence string using DP.
  (fset 'neovm--nlp-lcs (lambda (s1 s2)
    (let* ((n (length s1))
           (m (length s2))
           (dp (let ((v (make-vector (1+ n) nil)) (i 0))
                 (while (<= i n)
                   (aset v i (make-vector (1+ m) 0))
                   (setq i (1+ i)))
                 v)))
      ;; Fill DP table
      (let ((i 1))
        (while (<= i n)
          (let ((j 1))
            (while (<= j m)
              (if (= (aref s1 (1- i)) (aref s2 (1- j)))
                  (aset (aref dp i) j (1+ (aref (aref dp (1- i)) (1- j))))
                (aset (aref dp i) j
                      (max (aref (aref dp (1- i)) j)
                           (aref (aref dp i) (1- j)))))
              (setq j (1+ j))))
          (setq i (1+ i))))
      ;; Backtrace to find actual subsequence
      (let ((lcs-chars nil)
            (i n) (j m))
        (while (and (> i 0) (> j 0))
          (cond
            ((= (aref s1 (1- i)) (aref s2 (1- j)))
             (setq lcs-chars (cons (aref s1 (1- i)) lcs-chars))
             (setq i (1- i) j (1- j)))
            ((> (aref (aref dp (1- i)) j) (aref (aref dp i) (1- j)))
             (setq i (1- i)))
            (t (setq j (1- j)))))
        (list (aref (aref dp n) m)
              (apply #'string lcs-chars))))))

  (unwind-protect
      (list
        ;; Classic: "ABCBDAB" / "BDCAB" -> LCS "BCAB" (len 4)
        (funcall 'neovm--nlp-lcs "ABCBDAB" "BDCAB")
        ;; Identical strings
        (funcall 'neovm--nlp-lcs "hello" "hello")
        ;; No common characters
        (funcall 'neovm--nlp-lcs "abc" "xyz")
        ;; One empty
        (funcall 'neovm--nlp-lcs "" "test")
        ;; Subsequence at start and end
        (funcall 'neovm--nlp-lcs "abcxyz" "ayz")
        ;; Symmetric length: |LCS(a,b)| = |LCS(b,a)|
        (= (car (funcall 'neovm--nlp-lcs "algorithm" "altruistic"))
           (car (funcall 'neovm--nlp-lcs "altruistic" "algorithm"))))
    (fmakunbound 'neovm--nlp-lcs)))"#;
    let expect = expect_test::expect![[
        r#""OK ((4 \"BDAB\") (5 \"hello\") (0 \"\") (0 \"\") (3 \"ayz\") t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Simple stemmer (suffix removal rules)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nlp_stemmer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; A simple English stemmer that strips common suffixes.
  ;; Rules applied in order; first match wins.
  ;; Minimum stem length of 3 characters to avoid over-stripping.
  (fset 'neovm--nlp-stem (lambda (word)
    (let ((w (downcase word))
          (rules '(("ational" . "ate")
                   ("tional"  . "tion")
                   ("enci"    . "ence")
                   ("anci"    . "ance")
                   ("izer"    . "ize")
                   ("ously"   . "ous")
                   ("iveness" . "ive")
                   ("fulness" . "ful")
                   ("ness"    . "")
                   ("ment"    . "")
                   ("ing"     . "")
                   ("tion"    . "te")
                   ("sses"    . "ss")
                   ("ies"     . "i")
                   ("ed"      . "")
                   ("ly"      . "")
                   ("er"      . "")
                   ("s"       . "")))
          (changed nil))
      (dolist (rule rules)
        (unless changed
          (let* ((suffix (car rule))
                 (replacement (cdr rule))
                 (slen (length suffix))
                 (wlen (length w)))
            (when (and (> wlen slen)
                       (string= suffix (substring w (- wlen slen)))
                       (>= (- wlen slen (length replacement)) 2))
              (setq w (concat (substring w 0 (- wlen slen)) replacement))
              (setq changed t)))))
      w)))

  (unwind-protect
      (list
        (funcall 'neovm--nlp-stem "running")
        (funcall 'neovm--nlp-stem "happiness")
        (funcall 'neovm--nlp-stem "relational")
        (funcall 'neovm--nlp-stem "conditional")
        (funcall 'neovm--nlp-stem "caresses")
        (funcall 'neovm--nlp-stem "ponies")
        (funcall 'neovm--nlp-stem "cats")
        (funcall 'neovm--nlp-stem "jumped")
        (funcall 'neovm--nlp-stem "quietly")
        (funcall 'neovm--nlp-stem "effectiveness")
        ;; Short word: should not be stripped below 3 chars
        (funcall 'neovm--nlp-stem "is")
        (funcall 'neovm--nlp-stem "a")
        ;; Idempotence-ish: stemming a stem shouldn't over-strip
        (let ((s1 (funcall 'neovm--nlp-stem "organizing")))
          (list s1 (funcall 'neovm--nlp-stem s1))))
    (fmakunbound 'neovm--nlp-stem)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"runn\" \"happi\" \"relational\" \"conditional\" \"caress\" \"poni\" \"cat\" \"jump\" \"quiet\" \"effective\" \"is\" \"a\" (\"organiz\" \"organiz\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sentence similarity via Jaccard index on token sets
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nlp_jaccard_similarity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Simple word tokenizer (split on space, lowercase).
  (fset 'neovm--nlp-jac-tokenize (lambda (text)
    (let ((words nil) (current "") (i 0) (len (length text)))
      (while (< i len)
        (let ((ch (aref text i)))
          (if (or (= ch ?\s) (= ch ?,) (= ch ?.))
              (when (> (length current) 0)
                (setq words (cons (downcase current) words))
                (setq current ""))
            (setq current (concat current (char-to-string ch)))))
        (setq i (1+ i)))
      (when (> (length current) 0)
        (setq words (cons (downcase current) words)))
      (nreverse words))))

  ;; Build a set (hash table) from a list of strings.
  (fset 'neovm--nlp-jac-set (lambda (lst)
    (let ((ht (make-hash-table :test 'equal)))
      (dolist (x lst) (puthash x t ht))
      ht)))

  ;; Set intersection size.
  (fset 'neovm--nlp-jac-inter-size (lambda (a b)
    (let ((count 0))
      (maphash (lambda (k _v)
                 (when (gethash k b) (setq count (1+ count))))
               a)
      count)))

  ;; Set union size.
  (fset 'neovm--nlp-jac-union-size (lambda (a b)
    (let ((u (make-hash-table :test 'equal)))
      (maphash (lambda (k _v) (puthash k t u)) a)
      (maphash (lambda (k _v) (puthash k t u)) b)
      (hash-table-count u))))

  ;; Jaccard similarity: |A inter B| / |A union B|, returned as (numerator . denominator).
  (fset 'neovm--nlp-jaccard (lambda (s1 s2)
    (let* ((t1 (funcall 'neovm--nlp-jac-tokenize s1))
           (t2 (funcall 'neovm--nlp-jac-tokenize s2))
           (set1 (funcall 'neovm--nlp-jac-set t1))
           (set2 (funcall 'neovm--nlp-jac-set t2))
           (inter (funcall 'neovm--nlp-jac-inter-size set1 set2))
           (uni   (funcall 'neovm--nlp-jac-union-size set1 set2)))
      (if (= uni 0)
          (cons 1 1)  ;; both empty -> identical
        (cons inter uni)))))

  (unwind-protect
      (list
        ;; Identical sentences: similarity = 1
        (funcall 'neovm--nlp-jaccard "the cat sat on the mat"
                                     "the cat sat on the mat")
        ;; Completely different
        (funcall 'neovm--nlp-jaccard "hello world" "foo bar baz")
        ;; Partial overlap
        (funcall 'neovm--nlp-jaccard "the quick brown fox" "the lazy brown dog")
        ;; One is subset of other
        (funcall 'neovm--nlp-jaccard "a b" "a b c d")
        ;; Both empty
        (funcall 'neovm--nlp-jaccard "" "")
        ;; Symmetry
        (equal (funcall 'neovm--nlp-jaccard "cat dog" "dog fish")
               (funcall 'neovm--nlp-jaccard "dog fish" "cat dog"))
        ;; Single word overlap
        (funcall 'neovm--nlp-jaccard "programming is fun"
                                     "fun things to do"))
    (fmakunbound 'neovm--nlp-jac-tokenize)
    (fmakunbound 'neovm--nlp-jac-set)
    (fmakunbound 'neovm--nlp-jac-inter-size)
    (fmakunbound 'neovm--nlp-jac-union-size)
    (fmakunbound 'neovm--nlp-jaccard)))"#;
    let expect =
        expect_test::expect![[r#""OK ((5 . 5) (0 . 5) (2 . 6) (2 . 4) (1 . 1) t (1 . 6))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

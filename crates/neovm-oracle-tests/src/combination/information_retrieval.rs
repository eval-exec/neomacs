//! Oracle parity tests for information retrieval patterns in Elisp:
//! TF-IDF computation, inverted index construction, boolean query
//! evaluation (AND/OR/NOT), cosine similarity between document vectors,
//! BM25 scoring, document ranking, simple tokenizer with stop-word removal.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Tokenizer with stop-word removal and term frequency computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ir_tokenizer_and_term_frequency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Simple whitespace-based tokenizer that lowercases and removes stop words
  (defvar neovm--test-ir-stop-words
    '("the" "a" "an" "is" "are" "was" "were" "in" "on" "at" "to" "of" "and" "or" "it" "this" "that"))

  (fset 'neovm--test-ir-tokenize
    (lambda (text)
      "Split TEXT on whitespace, downcase, remove stop words and empty strings."
      (let ((words (split-string (downcase text) "[ \t\n,.]+" t))
            (result nil))
        (dolist (w words)
          (unless (member w neovm--test-ir-stop-words)
            (setq result (cons w result))))
        (nreverse result))))

  (fset 'neovm--test-ir-term-freq
    (lambda (tokens)
      "Compute term frequency as hash-table: term -> count."
      (let ((tf (make-hash-table :test 'equal)))
        (dolist (tok tokens)
          (puthash tok (1+ (gethash tok tf 0)) tf))
        tf)))

  (fset 'neovm--test-ir-tf-to-alist
    (lambda (tf)
      "Convert TF hash-table to sorted alist."
      (let ((pairs nil))
        (maphash (lambda (k v) (setq pairs (cons (cons k v) pairs))) tf)
        (sort pairs (lambda (a b) (string< (car a) (car b)))))))

  (unwind-protect
      (list
        ;; Basic tokenization
        (funcall 'neovm--test-ir-tokenize "The quick brown fox")
        ;; Stop word removal
        (funcall 'neovm--test-ir-tokenize "This is a test of the system")
        ;; Term frequency
        (funcall 'neovm--test-ir-tf-to-alist
          (funcall 'neovm--test-ir-term-freq
            (funcall 'neovm--test-ir-tokenize
              "cat dog cat bird dog cat")))
        ;; Empty input
        (funcall 'neovm--test-ir-tokenize "")
        ;; All stop words
        (funcall 'neovm--test-ir-tokenize "the a an is are"))
    (makunbound 'neovm--test-ir-stop-words)
    (fmakunbound 'neovm--test-ir-tokenize)
    (fmakunbound 'neovm--test-ir-term-freq)
    (fmakunbound 'neovm--test-ir-tf-to-alist)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"quick\" \"brown\" \"fox\") (\"test\" \"system\") ((\"bird\" . 1) (\"cat\" . 3) (\"dog\" . 2)) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Inverted index construction and boolean query evaluation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ir_inverted_index_and_boolean_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--test-ir-stop-words2
    '("the" "a" "an" "is" "are" "was" "in" "on" "to" "of" "and" "or" "it"))

  (fset 'neovm--test-ir-tokenize2
    (lambda (text)
      (let ((words (split-string (downcase text) "[ \t\n,.]+" t))
            (result nil))
        (dolist (w words)
          (unless (member w neovm--test-ir-stop-words2)
            (setq result (cons w result))))
        (nreverse result))))

  ;; Build inverted index: term -> sorted list of doc-ids
  (fset 'neovm--test-ir-build-index
    (lambda (docs)
      "DOCS is alist of (doc-id . text). Return inverted index hash-table."
      (let ((index (make-hash-table :test 'equal)))
        (dolist (doc docs)
          (let ((doc-id (car doc))
                (tokens (funcall 'neovm--test-ir-tokenize2 (cdr doc))))
            (dolist (tok tokens)
              (let ((posting (gethash tok index)))
                (unless (memq doc-id posting)
                  (puthash tok (cons doc-id posting) index))))))
        ;; Sort postings
        (maphash (lambda (k v) (puthash k (sort v '<) index)) index)
        index)))

  ;; Boolean query: AND, OR, NOT
  (fset 'neovm--test-ir-intersect
    (lambda (list1 list2)
      "Sorted intersection."
      (let ((result nil))
        (dolist (x list1)
          (when (memq x list2)
            (setq result (cons x result))))
        (sort result '<))))

  (fset 'neovm--test-ir-union-lists
    (lambda (list1 list2)
      "Sorted union."
      (let ((result (copy-sequence list1)))
        (dolist (x list2)
          (unless (memq x result)
            (setq result (cons x result))))
        (sort result '<))))

  (fset 'neovm--test-ir-difference
    (lambda (list1 list2)
      "Sorted difference: list1 - list2."
      (let ((result nil))
        (dolist (x list1)
          (unless (memq x list2)
            (setq result (cons x result))))
        (sort result '<))))

  (fset 'neovm--test-ir-query
    (lambda (index all-docs query)
      "Evaluate boolean QUERY against INDEX.
       QUERY is (AND t1 t2), (OR t1 t2), (NOT t1), or a string term."
      (cond
        ((stringp query)
         (gethash query index nil))
        ((eq (car query) 'AND)
         (funcall 'neovm--test-ir-intersect
                  (funcall 'neovm--test-ir-query index all-docs (nth 1 query))
                  (funcall 'neovm--test-ir-query index all-docs (nth 2 query))))
        ((eq (car query) 'OR)
         (funcall 'neovm--test-ir-union-lists
                  (funcall 'neovm--test-ir-query index all-docs (nth 1 query))
                  (funcall 'neovm--test-ir-query index all-docs (nth 2 query))))
        ((eq (car query) 'NOT)
         (funcall 'neovm--test-ir-difference
                  all-docs
                  (funcall 'neovm--test-ir-query index all-docs (nth 1 query)))))))

  (unwind-protect
      (let* ((docs '((1 . "cat sat on the mat")
                     (2 . "dog chased the cat")
                     (3 . "bird flew over the mat")
                     (4 . "cat and dog are friends")
                     (5 . "mat was on the floor")))
             (index (funcall 'neovm--test-ir-build-index docs))
             (all-doc-ids '(1 2 3 4 5)))
        (list
          ;; Single term query
          (funcall 'neovm--test-ir-query index all-doc-ids "cat")
          (funcall 'neovm--test-ir-query index all-doc-ids "mat")
          (funcall 'neovm--test-ir-query index all-doc-ids "dog")
          ;; AND query
          (funcall 'neovm--test-ir-query index all-doc-ids
                   '(AND "cat" "dog"))
          ;; OR query
          (funcall 'neovm--test-ir-query index all-doc-ids
                   '(OR "bird" "dog"))
          ;; NOT query
          (funcall 'neovm--test-ir-query index all-doc-ids
                   '(NOT "cat"))
          ;; Complex: (cat AND mat) OR dog
          (funcall 'neovm--test-ir-query index all-doc-ids
                   '(OR (AND "cat" "mat") "dog"))
          ;; Non-existent term
          (funcall 'neovm--test-ir-query index all-doc-ids "elephant")
          ;; NOT of non-existent returns all
          (funcall 'neovm--test-ir-query index all-doc-ids
                   '(NOT "elephant"))))
    (makunbound 'neovm--test-ir-stop-words2)
    (fmakunbound 'neovm--test-ir-tokenize2)
    (fmakunbound 'neovm--test-ir-build-index)
    (fmakunbound 'neovm--test-ir-intersect)
    (fmakunbound 'neovm--test-ir-union-lists)
    (fmakunbound 'neovm--test-ir-difference)
    (fmakunbound 'neovm--test-ir-query)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 4) (1 3 5) (2 4) (2 4) (2 3 4) (3 5) (1 2 4) nil (1 2 3 4 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// TF-IDF computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ir_tfidf_computation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-ir-tokenize3
    (lambda (text)
      (split-string (downcase text) "[ \t\n,.]+" t)))

  ;; TF: term frequency = count(term, doc) / total_terms_in_doc
  (fset 'neovm--test-ir-compute-tf
    (lambda (term tokens)
      (let ((count 0) (total (length tokens)))
        (dolist (tok tokens)
          (when (string= tok term)
            (setq count (1+ count))))
        (if (= total 0) 0.0
          (/ (float count) total)))))

  ;; IDF: inverse document frequency = log(N / df(term))
  ;; where df(term) = number of docs containing term
  (fset 'neovm--test-ir-compute-idf
    (lambda (term all-token-lists)
      (let ((n (length all-token-lists))
            (df 0))
        (dolist (tokens all-token-lists)
          (when (member term tokens)
            (setq df (1+ df))))
        (if (= df 0) 0.0
          (log (/ (float n) df))))))

  ;; TF-IDF for a term in a doc
  (fset 'neovm--test-ir-tfidf
    (lambda (term doc-tokens all-token-lists)
      (* (funcall 'neovm--test-ir-compute-tf term doc-tokens)
         (funcall 'neovm--test-ir-compute-idf term all-token-lists))))

  (unwind-protect
      (let* ((doc1-tokens (funcall 'neovm--test-ir-tokenize3 "cat sat cat mat"))
             (doc2-tokens (funcall 'neovm--test-ir-tokenize3 "dog chased cat"))
             (doc3-tokens (funcall 'neovm--test-ir-tokenize3 "bird flew over"))
             (all-tokens (list doc1-tokens doc2-tokens doc3-tokens)))
        (list
          ;; TF of "cat" in doc1: 2/4 = 0.5
          (funcall 'neovm--test-ir-compute-tf "cat" doc1-tokens)
          ;; TF of "cat" in doc2: 1/3
          (funcall 'neovm--test-ir-compute-tf "cat" doc2-tokens)
          ;; TF of "cat" in doc3: 0
          (funcall 'neovm--test-ir-compute-tf "cat" doc3-tokens)
          ;; IDF of "cat": log(3/2) since it appears in 2 of 3 docs
          ;; Just check it's positive and < log(3)
          (let ((idf-cat (funcall 'neovm--test-ir-compute-idf "cat" all-tokens)))
            (list (> idf-cat 0) (< idf-cat (log 3.0))))
          ;; IDF of "bird": log(3/1) = log(3) since it appears in 1 doc
          ;; Higher IDF = rarer = more discriminating
          (> (funcall 'neovm--test-ir-compute-idf "bird" all-tokens)
             (funcall 'neovm--test-ir-compute-idf "cat" all-tokens))
          ;; TF-IDF: "cat" in doc1 should be higher than "cat" in doc2
          ;; because TF is higher in doc1
          (> (funcall 'neovm--test-ir-tfidf "cat" doc1-tokens all-tokens)
             (funcall 'neovm--test-ir-tfidf "cat" doc2-tokens all-tokens))
          ;; TF-IDF of absent term is 0
          (= (funcall 'neovm--test-ir-tfidf "elephant" doc1-tokens all-tokens) 0.0)))
    (fmakunbound 'neovm--test-ir-tokenize3)
    (fmakunbound 'neovm--test-ir-compute-tf)
    (fmakunbound 'neovm--test-ir-compute-idf)
    (fmakunbound 'neovm--test-ir-tfidf)))"#;
    let expect = expect_test::expect![[r#""OK (0.5 0.3333333333333333 0.0 (t t) t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cosine similarity between document TF-IDF vectors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ir_cosine_similarity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Dot product of two hash-table vectors
  (fset 'neovm--test-ir-dot-product
    (lambda (vec1 vec2)
      (let ((sum 0.0))
        (maphash (lambda (k v)
                   (let ((v2 (gethash k vec2 0.0)))
                     (setq sum (+ sum (* v v2)))))
                 vec1)
        sum)))

  ;; Magnitude of a hash-table vector
  (fset 'neovm--test-ir-magnitude
    (lambda (vec)
      (let ((sum 0.0))
        (maphash (lambda (k v)
                   (setq sum (+ sum (* v v))))
                 vec)
        (sqrt sum))))

  ;; Cosine similarity
  (fset 'neovm--test-ir-cosine-sim
    (lambda (vec1 vec2)
      (let ((mag1 (funcall 'neovm--test-ir-magnitude vec1))
            (mag2 (funcall 'neovm--test-ir-magnitude vec2)))
        (if (or (= mag1 0.0) (= mag2 0.0)) 0.0
          (/ (funcall 'neovm--test-ir-dot-product vec1 vec2)
             (* mag1 mag2))))))

  ;; Build a TF vector from tokens
  (fset 'neovm--test-ir-tf-vector
    (lambda (tokens)
      (let ((tf (make-hash-table :test 'equal))
            (total (float (length tokens))))
        (dolist (tok tokens)
          (puthash tok (1+ (gethash tok tf 0)) tf))
        ;; Normalize by total
        (maphash (lambda (k v) (puthash k (/ (float v) total) tf)) tf)
        tf)))

  (unwind-protect
      (let* ((v1 (funcall 'neovm--test-ir-tf-vector
                           '("cat" "dog" "cat" "bird")))
             (v2 (funcall 'neovm--test-ir-tf-vector
                           '("cat" "dog" "fish")))
             (v3 (funcall 'neovm--test-ir-tf-vector
                           '("car" "truck" "bus")))
             (v4 (funcall 'neovm--test-ir-tf-vector
                           '("cat" "dog" "cat" "bird"))))  ;; same as v1
        (list
          ;; Identical documents: cosine sim = 1.0
          (let ((sim (funcall 'neovm--test-ir-cosine-sim v1 v4)))
            (< (abs (- sim 1.0)) 0.001))
          ;; Similar documents (share some terms): 0 < sim < 1
          (let ((sim (funcall 'neovm--test-ir-cosine-sim v1 v2)))
            (list (> sim 0.0) (< sim 1.0)))
          ;; Completely different: sim = 0
          (let ((sim (funcall 'neovm--test-ir-cosine-sim v1 v3)))
            (< (abs sim) 0.001))
          ;; Symmetry: sim(a,b) = sim(b,a)
          (let ((sim-ab (funcall 'neovm--test-ir-cosine-sim v1 v2))
                (sim-ba (funcall 'neovm--test-ir-cosine-sim v2 v1)))
            (< (abs (- sim-ab sim-ba)) 0.001))
          ;; Empty vector
          (let ((empty (make-hash-table :test 'equal)))
            (funcall 'neovm--test-ir-cosine-sim v1 empty))))
    (fmakunbound 'neovm--test-ir-dot-product)
    (fmakunbound 'neovm--test-ir-magnitude)
    (fmakunbound 'neovm--test-ir-cosine-sim)
    (fmakunbound 'neovm--test-ir-tf-vector)))"#;
    let expect = expect_test::expect![[r#""OK (t (t t) t t 0.0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// BM25 scoring
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ir_bm25_scoring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; BM25 parameters
  (defvar neovm--test-ir-bm25-k1 1.2)
  (defvar neovm--test-ir-bm25-b 0.75)

  ;; BM25 score for a single term in a single document
  ;; score(q, D) = IDF(q) * (f(q,D) * (k1 + 1)) / (f(q,D) + k1 * (1 - b + b * |D|/avgdl))
  ;; IDF(q) = log((N - n(q) + 0.5) / (n(q) + 0.5) + 1)
  (fset 'neovm--test-ir-bm25-idf
    (lambda (n-docs doc-freq)
      "BM25 IDF component."
      (log (+ (/ (+ (- n-docs doc-freq) 0.5)
                 (+ doc-freq 0.5))
              1.0))))

  (fset 'neovm--test-ir-bm25-term-score
    (lambda (tf doc-len avg-dl n-docs doc-freq)
      "BM25 score for one term in one document."
      (let ((idf (funcall 'neovm--test-ir-bm25-idf n-docs doc-freq))
            (tf-component
             (/ (* (float tf) (+ neovm--test-ir-bm25-k1 1.0))
                (+ (float tf)
                   (* neovm--test-ir-bm25-k1
                      (+ (- 1.0 neovm--test-ir-bm25-b)
                         (* neovm--test-ir-bm25-b
                            (/ (float doc-len) avg-dl))))))))
        (* idf tf-component))))

  ;; Score a document against a multi-term query
  (fset 'neovm--test-ir-bm25-score
    (lambda (query-terms doc-tokens all-doc-tokens)
      "Score DOC-TOKENS against QUERY-TERMS using BM25."
      (let* ((n-docs (length all-doc-tokens))
             (total-len 0)
             (doc-len (length doc-tokens)))
        ;; Compute avg document length
        (dolist (dt all-doc-tokens)
          (setq total-len (+ total-len (length dt))))
        (let ((avg-dl (/ (float total-len) n-docs))
              (score 0.0))
          ;; For each query term, compute BM25 contribution
          (dolist (qterm query-terms)
            ;; Term frequency in this doc
            (let ((tf 0) (df 0))
              (dolist (tok doc-tokens)
                (when (string= tok qterm) (setq tf (1+ tf))))
              ;; Document frequency
              (dolist (dt all-doc-tokens)
                (when (member qterm dt) (setq df (1+ df))))
              (when (> tf 0)
                (setq score (+ score
                               (funcall 'neovm--test-ir-bm25-term-score
                                        tf doc-len avg-dl n-docs df))))))
          score))))

  (unwind-protect
      (let* ((d1 '("quick" "brown" "fox" "jumps"))
             (d2 '("lazy" "brown" "dog" "sleeps"))
             (d3 '("quick" "fox" "quick" "fox"))
             (all (list d1 d2 d3)))
        (list
          ;; Single-term query "fox"
          ;; d1 has 1 "fox", d3 has 2 "fox", d2 has 0
          (> (funcall 'neovm--test-ir-bm25-score '("fox") d3 all)
             (funcall 'neovm--test-ir-bm25-score '("fox") d1 all))
          (= (funcall 'neovm--test-ir-bm25-score '("fox") d2 all) 0.0)
          ;; Multi-term query "quick fox": d3 should score highest (2 quick + 2 fox)
          (> (funcall 'neovm--test-ir-bm25-score '("quick" "fox") d3 all)
             (funcall 'neovm--test-ir-bm25-score '("quick" "fox") d1 all))
          ;; Rare term "lazy" has higher IDF than common "brown"
          (> (funcall 'neovm--test-ir-bm25-idf 3 1)
             (funcall 'neovm--test-ir-bm25-idf 3 2))
          ;; BM25 scores are non-negative
          (>= (funcall 'neovm--test-ir-bm25-score '("brown") d1 all) 0.0)
          ;; Unknown query term scores 0
          (= (funcall 'neovm--test-ir-bm25-score '("elephant") d1 all) 0.0)))
    (makunbound 'neovm--test-ir-bm25-k1)
    (makunbound 'neovm--test-ir-bm25-b)
    (fmakunbound 'neovm--test-ir-bm25-idf)
    (fmakunbound 'neovm--test-ir-bm25-term-score)
    (fmakunbound 'neovm--test-ir-bm25-score)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Document ranking: rank documents by relevance
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ir_document_ranking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Simple scoring: sum of TF * IDF for each query term
  (fset 'neovm--test-ir-simple-score
    (lambda (query-terms doc-tokens n-docs doc-freqs)
      "Score using simple TF-IDF sum. DOC-FREQS is hash: term -> df."
      (let ((score 0.0)
            (total (float (length doc-tokens))))
        (when (> total 0)
          (dolist (qt query-terms)
            (let ((tf 0))
              (dolist (tok doc-tokens)
                (when (string= tok qt) (setq tf (1+ tf))))
              (when (> tf 0)
                (let* ((df (gethash qt doc-freqs 1))
                       (idf (log (/ (float n-docs) df)))
                       (term-tf (/ (float tf) total)))
                  (setq score (+ score (* term-tf idf))))))))
        score)))

  ;; Rank documents: return list of (doc-id . score) sorted descending
  (fset 'neovm--test-ir-rank
    (lambda (query docs)
      "QUERY is string, DOCS is alist (id . text). Return ranked (id . score) list."
      (let* ((query-terms (split-string (downcase query) "[ \t]+" t))
             (doc-token-list
              (mapcar (lambda (d)
                        (cons (car d)
                              (split-string (downcase (cdr d)) "[ \t,.]+" t)))
                      docs))
             (n (length docs))
             ;; Compute document frequencies
             (df-table (make-hash-table :test 'equal))
             (scores nil))
        ;; Build DF table
        (dolist (dt doc-token-list)
          (let ((seen (make-hash-table :test 'equal)))
            (dolist (tok (cdr dt))
              (unless (gethash tok seen)
                (puthash tok t seen)
                (puthash tok (1+ (gethash tok df-table 0)) df-table)))))
        ;; Score each document
        (dolist (dt doc-token-list)
          (let ((score (funcall 'neovm--test-ir-simple-score
                                query-terms (cdr dt) n df-table)))
            (setq scores (cons (cons (car dt) score) scores))))
        ;; Sort by score descending
        (sort scores (lambda (a b) (> (cdr a) (cdr b)))))))

  (unwind-protect
      (let ((docs '((1 . "Emacs is a powerful text editor")
                    (2 . "Vim is another text editor")
                    (3 . "Visual Studio Code is a popular editor")
                    (4 . "Emacs Lisp is the extension language of Emacs")
                    (5 . "Python is a popular programming language"))))
        (list
          ;; Query "emacs": doc4 has 2 mentions, doc1 has 1
          (let ((ranked (funcall 'neovm--test-ir-rank "emacs" docs)))
            (mapcar 'car ranked))
          ;; Query "text editor": docs 1,2 should rank high
          (let ((ranked (funcall 'neovm--test-ir-rank "text editor" docs)))
            ;; First two results should be docs 1 and 2 (in some order)
            (let ((top2 (list (car (nth 0 ranked)) (car (nth 1 ranked)))))
              (list (memq 1 top2) (memq 2 top2))))
          ;; Query "popular": docs 3 and 5 mention "popular"
          (let ((ranked (funcall 'neovm--test-ir-rank "popular" docs)))
            (let ((top2-ids (list (car (nth 0 ranked)) (car (nth 1 ranked)))))
              (list (or (memq 3 top2-ids) (memq 5 top2-ids)))))
          ;; Query with no matching terms: all scores 0
          (let ((ranked (funcall 'neovm--test-ir-rank "quantum physics" docs)))
            (let ((all-zero t))
              (dolist (r ranked)
                (unless (= (cdr r) 0.0)
                  (setq all-zero nil)))
              all-zero))))
    (fmakunbound 'neovm--test-ir-simple-score)
    (fmakunbound 'neovm--test-ir-rank)))"#;
    let expect = expect_test::expect![[r#""OK ((4 1 5 3 2) ((1) (2 1)) ((3)) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Jaccard similarity for set-based document comparison
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ir_jaccard_similarity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Convert token list to a set (sorted unique list)
  (fset 'neovm--test-ir-to-set
    (lambda (tokens)
      (let ((seen (make-hash-table :test 'equal))
            (result nil))
        (dolist (tok tokens)
          (unless (gethash tok seen)
            (puthash tok t seen)
            (setq result (cons tok result))))
        (sort result 'string<))))

  ;; Jaccard similarity = |A ∩ B| / |A ∪ B|
  (fset 'neovm--test-ir-jaccard
    (lambda (set1 set2)
      (let ((inter 0) (union-count 0)
            (all (make-hash-table :test 'equal)))
        ;; Count intersection
        (let ((s2-hash (make-hash-table :test 'equal)))
          (dolist (x set2) (puthash x t s2-hash))
          (dolist (x set1)
            (when (gethash x s2-hash)
              (setq inter (1+ inter)))))
        ;; Count union
        (dolist (x set1) (puthash x t all))
        (dolist (x set2) (puthash x t all))
        (maphash (lambda (k v) (setq union-count (1+ union-count))) all)
        (if (= union-count 0) 0.0
          (/ (float inter) union-count)))))

  (unwind-protect
      (let ((s1 (funcall 'neovm--test-ir-to-set '("cat" "dog" "bird" "fish")))
            (s2 (funcall 'neovm--test-ir-to-set '("cat" "dog" "hamster")))
            (s3 (funcall 'neovm--test-ir-to-set '("car" "truck" "bus")))
            (s4 (funcall 'neovm--test-ir-to-set '("cat" "dog" "bird" "fish"))))
        (list
          ;; Identical sets: Jaccard = 1.0
          (let ((sim (funcall 'neovm--test-ir-jaccard s1 s4)))
            (< (abs (- sim 1.0)) 0.001))
          ;; Partial overlap: 0 < Jaccard < 1
          ;; s1 and s2 share cat, dog (2 shared, union = 5)
          (let ((sim (funcall 'neovm--test-ir-jaccard s1 s2)))
            (list (> sim 0.0) (< sim 1.0)
                  ;; Should be 2/5 = 0.4
                  (< (abs (- sim 0.4)) 0.001)))
          ;; Disjoint sets: Jaccard = 0
          (let ((sim (funcall 'neovm--test-ir-jaccard s1 s3)))
            (< (abs sim) 0.001))
          ;; Symmetry
          (let ((sim-ab (funcall 'neovm--test-ir-jaccard s1 s2))
                (sim-ba (funcall 'neovm--test-ir-jaccard s2 s1)))
            (< (abs (- sim-ab sim-ba)) 0.001))
          ;; Empty set
          (funcall 'neovm--test-ir-jaccard nil nil)
          (funcall 'neovm--test-ir-jaccard s1 nil)))
    (fmakunbound 'neovm--test-ir-to-set)
    (fmakunbound 'neovm--test-ir-jaccard)))"#;
    let expect = expect_test::expect![[r#""OK (t (t t t) t t 0.0 0.0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Complex oracle parity tests for text analysis combinations:
//! word frequency counting with top-N extraction, readability scoring,
//! text summarization, n-gram analysis, concordance indexing,
//! and Markov chain text model construction.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Word frequency counter with top-N extraction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_analysis_word_frequency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((text "the cat sat on the mat the cat ate the rat the dog chased the cat and the rat ran from the dog")
         (words (split-string (downcase text) " "))
         ;; Build frequency table using alist
         (freq nil))
  (dolist (w words)
    (let ((entry (assoc w freq)))
      (if entry
          (setcdr entry (1+ (cdr entry)))
        (setq freq (cons (cons w 1) freq)))))
  ;; Sort by frequency descending
  (let* ((sorted (sort (copy-sequence freq)
                       (lambda (a b)
                         (> (cdr a) (cdr b)))))
         ;; Top 5
         (top5 (let ((result nil) (ptr sorted) (i 0))
                 (while (and ptr (< i 5))
                   (setq result (cons (car ptr) result))
                   (setq ptr (cdr ptr))
                   (setq i (1+ i)))
                 (nreverse result)))
         ;; Total word count
         (total (length words))
         ;; Unique word count
         (unique (length freq))
         ;; Most frequent word
         (most-freq (car sorted))
         ;; Words appearing exactly once (hapax legomena)
         (hapax (let ((h nil))
                  (dolist (entry freq)
                    (when (= (cdr entry) 1)
                      (setq h (cons (car entry) h))))
                  (sort h #'string<))))
    (list top5 total unique most-freq hapax)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"the\" . 8) (\"cat\" . 3) (\"dog\" . 2) (\"rat\" . 2) (\"from\" . 1)) 23 12 (\"the\" . 8) (\"and\" \"ate\" \"chased\" \"from\" \"mat\" \"on\" \"ran\" \"sat\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Readability score calculator (Flesch-like: sentence/word/syllable counting)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_analysis_readability_score() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((text "The cat sat on the mat. It was a sunny day. The children played in the park while their parents watched. Education is the most powerful weapon which you can use to change the world.")
         ;; Count sentences (by period, exclamation, question mark)
         (sentence-count
          (let ((count 0) (i 0) (len (length text)))
            (while (< i len)
              (let ((ch (aref text i)))
                (when (or (= ch ?.) (= ch ?!) (= ch ??))
                  (setq count (1+ count))))
              (setq i (1+ i)))
            count))
         ;; Count words
         (words (split-string text "[ \t\n.!?,;:]+"))
         (word-count (length (seq-filter (lambda (w) (> (length w) 0)) words)))
         ;; Count syllables in a word (simplified: count vowel groups)
         (count-syllables
          (lambda (word)
            (let* ((w (downcase word))
                   (len (length w))
                   (count 0)
                   (in-vowel nil)
                   (vowels "aeiou"))
              (dotimes (i len)
                (let ((is-v (not (null (seq-position vowels (aref w i))))))
                  (when (and is-v (not in-vowel))
                    (setq count (1+ count)))
                  (setq in-vowel is-v)))
              (max 1 count))))
         ;; Total syllables
         (total-syllables
          (let ((s 0))
            (dolist (w words)
              (when (> (length w) 0)
                (setq s (+ s (funcall count-syllables w)))))
            s))
         ;; Average word length
         (avg-word-length
          (let ((total-chars 0) (wc 0))
            (dolist (w words)
              (when (> (length w) 0)
                (setq total-chars (+ total-chars (length w)))
                (setq wc (1+ wc))))
            (if (> wc 0) (/ (float total-chars) wc) 0.0)))
         ;; Words per sentence
         (words-per-sentence
          (if (> sentence-count 0)
              (/ (float word-count) sentence-count)
            0.0))
         ;; Syllables per word
         (syllables-per-word
          (if (> word-count 0)
              (/ (float total-syllables) word-count)
            0.0))
         ;; Simplified Flesch Reading Ease approximation
         ;; 206.835 - 1.015 * (words/sentences) - 84.6 * (syllables/words)
         (flesch (- 206.835
                    (* 1.015 words-per-sentence)
                    (* 84.6 syllables-per-word))))
  (list sentence-count word-count total-syllables
        (> avg-word-length 0)
        (> words-per-sentence 0)
        (> syllables-per-word 0)
        ;; Flesch score should be between 0 and 100 for normal text
        (> flesch 0)
        (< flesch 120)))"#;
    let expect = expect_test::expect![[r#""OK (4 35 48 t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Text summarization: extract sentences containing key terms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_analysis_extractive_summary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((text "Emacs is a powerful text editor. It supports many programming languages. Lisp is the extension language of Emacs. Users can customize Emacs with Elisp. The buffer is the fundamental data structure. Windows display buffer contents. Frames contain one or more windows. Keybindings map keys to commands.")
         ;; Split into sentences
         (sentences
          (let ((parts nil) (start 0) (i 0) (len (length text)))
            (while (< i len)
              (when (= (aref text i) ?.)
                (let ((s (string-trim (substring text start (1+ i)))))
                  (when (> (length s) 0)
                    (setq parts (cons s parts))))
                (setq start (1+ i)))
              (setq i (1+ i)))
            (nreverse parts)))
         ;; Key terms to look for
         (key-terms '("emacs" "lisp" "buffer"))
         ;; Score each sentence by number of key terms it contains
         (scored-sentences
          (mapcar
           (lambda (sent)
             (let ((score 0)
                   (lower-sent (downcase sent)))
               (dolist (term key-terms)
                 (when (string-match-p (regexp-quote term) lower-sent)
                   (setq score (1+ score))))
               (cons score sent)))
           sentences))
         ;; Sort by score descending
         (sorted (sort (copy-sequence scored-sentences)
                       (lambda (a b) (> (car a) (car b)))))
         ;; Extract top 3 sentences
         (top3 (let ((result nil) (ptr sorted) (i 0))
                 (while (and ptr (< i 3))
                   (when (> (car (car ptr)) 0)
                     (setq result (cons (cdr (car ptr)) result)))
                   (setq ptr (cdr ptr))
                   (setq i (1+ i)))
                 (nreverse result)))
         ;; Sentences with no key terms
         (no-match (seq-filter (lambda (s) (= (car s) 0)) scored-sentences))
         (no-match-texts (mapcar #'cdr no-match)))
  (list (length sentences)
        (length top3)
        top3
        no-match-texts
        ;; Verify top sentences actually contain key terms
        (let ((all-have-terms t))
          (dolist (s top3)
            (let ((has-any nil))
              (dolist (term key-terms)
                (when (string-match-p (regexp-quote term) (downcase s))
                  (setq has-any t)))
              (unless has-any (setq all-have-terms nil))))
          all-have-terms)))"#;
    let expect = expect_test::expect![[
        r#""OK (8 3 (\"Lisp is the extension language of Emacs.\" \"Users can customize Emacs with Elisp.\" \"Emacs is a powerful text editor.\") (\"It supports many programming languages.\" \"Frames contain one or more windows.\" \"Keybindings map keys to commands.\") t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// N-gram analysis (bigrams and trigrams)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_analysis_ngrams() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((text "the cat sat on the mat the cat ate the fish")
         (words (split-string (downcase text) " "))
         ;; Generate bigrams
         (bigrams
          (let ((result nil) (ptr words))
            (while (cdr ptr)
              (setq result (cons (list (car ptr) (car (cdr ptr))) result))
              (setq ptr (cdr ptr)))
            (nreverse result)))
         ;; Generate trigrams
         (trigrams
          (let ((result nil) (ptr words))
            (while (cdr (cdr ptr))
              (setq result (cons (list (car ptr) (car (cdr ptr)) (car (cdr (cdr ptr)))) result))
              (setq ptr (cdr ptr)))
            (nreverse result)))
         ;; Count bigram frequencies
         (bigram-freq nil))
  (dolist (bg bigrams)
    (let* ((key (format "%s %s" (car bg) (car (cdr bg))))
           (entry (assoc key bigram-freq)))
      (if entry
          (setcdr entry (1+ (cdr entry)))
        (setq bigram-freq (cons (cons key 1) bigram-freq)))))
  ;; Sort bigram frequencies
  (let* ((sorted-bg (sort (copy-sequence bigram-freq)
                          (lambda (a b) (> (cdr a) (cdr b)))))
         ;; Count trigram frequencies
         (trigram-freq nil))
    (dolist (tg trigrams)
      (let* ((key (format "%s %s %s" (car tg) (car (cdr tg)) (car (cdr (cdr tg)))))
             (entry (assoc key trigram-freq)))
        (if entry
            (setcdr entry (1+ (cdr entry)))
          (setq trigram-freq (cons (cons key 1) trigram-freq)))))
    (let ((sorted-tg (sort (copy-sequence trigram-freq)
                           (lambda (a b) (> (cdr a) (cdr b))))))
      (list
       (length bigrams)
       (length trigrams)
       ;; Top 3 bigrams
       (let ((top nil) (p sorted-bg) (i 0))
         (while (and p (< i 3))
           (setq top (cons (car p) top))
           (setq p (cdr p))
           (setq i (1+ i)))
         (nreverse top))
       ;; Top 3 trigrams
       (let ((top nil) (p sorted-tg) (i 0))
         (while (and p (< i 3))
           (setq top (cons (car p) top))
           (setq p (cdr p))
           (setq i (1+ i)))
         (nreverse top))
       ;; Unique bigram count
       (length bigram-freq)
       ;; Unique trigram count
       (length trigram-freq)))))"#;
    let expect = expect_test::expect![[
        r#""OK (10 9 ((\"the cat\" . 2) (\"the fish\" . 1) (\"ate the\" . 1)) ((\"ate the fish\" . 1) (\"cat ate the\" . 1) (\"the cat ate\" . 1)) 9 9)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Concordance index: word positions in text
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_analysis_concordance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((text "to be or not to be that is the question to be is to exist")
         (words (split-string (downcase text) " "))
         ;; Build concordance: word -> list of positions (0-indexed)
         (concordance nil)
         (pos 0))
  (dolist (w words)
    (let ((entry (assoc w concordance)))
      (if entry
          (setcdr entry (append (cdr entry) (list pos)))
        (setq concordance (cons (cons w (list pos)) concordance))))
    (setq pos (1+ pos)))
  ;; Sort concordance alphabetically
  (let* ((sorted (sort (copy-sequence concordance)
                       (lambda (a b) (string< (car a) (car b)))))
         ;; Find words that appear more than once
         (repeated (seq-filter (lambda (entry) (> (length (cdr entry)) 1)) sorted))
         ;; Find words that appear exactly once
         (unique-words (seq-filter (lambda (entry) (= (length (cdr entry)) 1)) sorted))
         ;; Context window: for each occurrence of "be", get surrounding words
         (be-contexts
          (let ((be-positions (cdr (assoc "be" concordance)))
                (ctx nil))
            (dolist (p be-positions)
              (let ((left (if (> p 0) (nth (1- p) words) nil))
                    (right (nth (1+ p) words)))
                (setq ctx (cons (list left "be" right) ctx))))
            (nreverse ctx))))
    (list
     (length sorted)
     ;; Positions of "to"
     (cdr (assoc "to" concordance))
     ;; Positions of "be"
     (cdr (assoc "be" concordance))
     ;; Repeated words
     (mapcar #'car repeated)
     ;; Words appearing once
     (mapcar #'car unique-words)
     ;; Context windows for "be"
     be-contexts
     ;; Total word count
     (length words))))"#;
    let expect = expect_test::expect![[
        r#""OK (9 (0 4 10 13) (1 5 11) (\"be\" \"is\" \"to\") (\"exist\" \"not\" \"or\" \"question\" \"that\" \"the\") ((\"to\" \"be\" \"or\") (\"to\" \"be\" \"that\") (\"to\" \"be\" \"is\")) 15)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Markov chain text model (build transition table from text)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_analysis_markov_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a first-order Markov chain transition table from text,
    // then verify statistical properties (no random generation, just table building)
    let form = r#"(let* ((text "the cat sat the cat ate the dog sat the dog ran the cat ran")
         (words (split-string (downcase text) " "))
         ;; Build transition table: word -> alist of (next-word . count)
         (transitions nil))
  ;; Walk through consecutive word pairs
  (let ((ptr words))
    (while (cdr ptr)
      (let* ((current (car ptr))
             (next-word (car (cdr ptr)))
             (entry (assoc current transitions)))
        (if entry
            ;; Update existing transition table for this word
            (let ((next-entry (assoc next-word (cdr entry))))
              (if next-entry
                  (setcdr next-entry (1+ (cdr next-entry)))
                (setcdr entry (cons (cons next-word 1) (cdr entry)))))
          ;; New word in transition table
          (setq transitions (cons (cons current (list (cons next-word 1))) transitions))))
      (setq ptr (cdr ptr))))
  ;; Sort transition table alphabetically
  (let* ((sorted-trans (sort (copy-sequence transitions)
                             (lambda (a b) (string< (car a) (car b)))))
         ;; For each word, sort its transitions by count desc
         (sorted-full
          (mapcar (lambda (entry)
                    (cons (car entry)
                          (sort (copy-sequence (cdr entry))
                                (lambda (a b) (> (cdr a) (cdr b))))))
                  sorted-trans))
         ;; Most likely next word for each word
         (most-likely
          (mapcar (lambda (entry)
                    (cons (car entry) (car (cdr entry))))
                  sorted-full))
         ;; Total transitions count
         (total-trans (let ((sum 0))
                        (dolist (entry transitions)
                          (dolist (t-entry (cdr entry))
                            (setq sum (+ sum (cdr t-entry)))))
                        sum))
         ;; Number of unique words with transitions
         (unique-count (length transitions))
         ;; Verify: total transitions = number of word pairs
         (expected-pairs (1- (length words))))
    (list
     sorted-full
     most-likely
     total-trans
     unique-count
     expected-pairs
     (= total-trans expected-pairs))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"ate\" (\"the\" . 1)) (\"cat\" (\"ran\" . 1) (\"ate\" . 1) (\"sat\" . 1)) (\"dog\" (\"ran\" . 1) (\"sat\" . 1)) (\"ran\" (\"the\" . 1)) (\"sat\" (\"the\" . 2)) (\"the\" (\"cat\" . 3) (\"dog\" . 2))) ((\"ate\" \"the\" . 1) (\"cat\" \"ran\" . 1) (\"dog\" \"ran\" . 1) (\"ran\" \"the\" . 1) (\"sat\" \"the\" . 2) (\"the\" \"cat\" . 3)) 14 6 14 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

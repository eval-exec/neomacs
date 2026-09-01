//! Oracle parity tests for a Markov chain text generator implemented in Elisp:
//! building transition tables from text, generating sequences, N-gram support,
//! probability distributions, most common transitions, and state space analysis.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Build bigram transition table from word list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_markov_build_transition_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a transition table: for each word, record which words follow it
    // and how often. Result is a hash table mapping word -> alist of (next . count).
    let form = r#"(progn
  (fset 'neovm--mc-build-table
    (lambda (words)
      "Build bigram transition table from list of words."
      (let ((table (make-hash-table))
            (prev nil))
        (dolist (w words)
          (when prev
            (let* ((transitions (gethash prev table nil))
                   (entry (assq w transitions)))
              (if entry
                  (setcdr entry (1+ (cdr entry)))
                (puthash prev (cons (cons w 1) transitions) table))))
          (setq prev w))
        table)))

  (fset 'neovm--mc-table-to-sorted-alist
    (lambda (table)
      "Convert transition table to a sorted alist for deterministic comparison."
      (let ((result nil))
        (maphash (lambda (state transitions)
                   (setq result
                         (cons (cons state
                                     (sort (copy-sequence transitions)
                                           (lambda (a b)
                                             (string< (symbol-name (car a))
                                                      (symbol-name (car b))))))
                               result)))
                 table)
        (sort result (lambda (a b)
                       (string< (symbol-name (car a))
                                (symbol-name (car b))))))))

  (unwind-protect
      (let* ((text '(the cat sat on the mat the cat ate the fish
                     the dog sat on the mat the dog ate the bone))
             (table (funcall 'neovm--mc-build-table text)))
        (list
          ;; Sorted representation of the transition table
          (funcall 'neovm--mc-table-to-sorted-alist table)
          ;; Number of unique states
          (hash-table-count table)
          ;; Transitions from "the"
          (sort (copy-sequence (gethash 'the table))
                (lambda (a b) (string< (symbol-name (car a))
                                       (symbol-name (car b)))))
          ;; Transitions from "cat"
          (sort (copy-sequence (gethash 'cat table))
                (lambda (a b) (string< (symbol-name (car a))
                                       (symbol-name (car b)))))))
    (fmakunbound 'neovm--mc-build-table)
    (fmakunbound 'neovm--mc-table-to-sorted-alist)))"#;
    let expect = expect_test::expect![[
        r#""OK (((ate (the . 2)) (cat (ate . 1) (sat . 1)) (dog (ate . 1) (sat . 1)) (fish (the . 1)) (mat (the . 2)) (on (the . 2)) (sat (on . 2)) (the (bone . 1) (cat . 2) (dog . 2) (fish . 1) (mat . 2))) 8 ((bone . 1) (cat . 2) (dog . 2) (fish . 1) (mat . 2)) ((ate . 1) (sat . 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Generate sequence from transition table (deterministic: most likely next)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_markov_generate_deterministic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generate a sequence by always choosing the most frequent next state.
    // When tied, choose alphabetically first. This makes the output deterministic.
    let form = r#"(progn
  (fset 'neovm--mc-build
    (lambda (words)
      (let ((table (make-hash-table)) (prev nil))
        (dolist (w words)
          (when prev
            (let* ((tr (gethash prev table nil))
                   (entry (assq w tr)))
              (if entry (setcdr entry (1+ (cdr entry)))
                (puthash prev (cons (cons w 1) tr) table))))
          (setq prev w))
        table)))

  (fset 'neovm--mc-most-likely
    (lambda (transitions)
      "Return the most likely next state (highest count, then alphabetical)."
      (let ((best nil) (best-count -1))
        (dolist (entry transitions)
          (when (or (> (cdr entry) best-count)
                    (and (= (cdr entry) best-count)
                         (string< (symbol-name (car entry))
                                  (symbol-name best))))
            (setq best (car entry))
            (setq best-count (cdr entry))))
        best)))

  (fset 'neovm--mc-generate
    (lambda (table start n)
      "Generate N words starting from START, always picking most likely next."
      (let ((result (list start))
            (current start)
            (i 0))
        (while (< i n)
          (let ((transitions (gethash current table nil)))
            (if (null transitions)
                (setq i n)  ;; Stop if no transitions
              (let ((next (funcall 'neovm--mc-most-likely transitions)))
                (setq result (cons next result))
                (setq current next)
                (setq i (1+ i))))))
        (nreverse result))))

  (unwind-protect
      (let* ((text '(the cat sat on the mat the cat ate the fish
                     the dog sat on the mat the dog ate the bone))
             (table (funcall 'neovm--mc-build text)))
        (list
          ;; Generate 5 words starting from "the"
          (funcall 'neovm--mc-generate table 'the 5)
          ;; Generate 5 words starting from "cat"
          (funcall 'neovm--mc-generate table 'cat 5)
          ;; Generate from "dog"
          (funcall 'neovm--mc-generate table 'dog 5)
          ;; Generate from "sat"
          (funcall 'neovm--mc-generate table 'sat 3)
          ;; Generate from a state with no transitions
          (funcall 'neovm--mc-generate table 'bone 3)))
    (fmakunbound 'neovm--mc-build)
    (fmakunbound 'neovm--mc-most-likely)
    (fmakunbound 'neovm--mc-generate)))"#;
    let expect = expect_test::expect![[
        r#""OK ((the cat ate the cat ate) (cat ate the cat ate the) (dog ate the cat ate the) (sat on the cat) (bone))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// N-gram (trigram) transition table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_markov_trigram_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a trigram table: pairs of consecutive words map to next word.
    // State is represented as a cons cell (word1 . word2).
    let form = r#"(progn
  (fset 'neovm--mc-build-trigram
    (lambda (words)
      "Build trigram transition table. State = (prev-prev . prev)."
      (let ((table (make-hash-table :test 'equal))
            (pp nil) (prev nil))
        (dolist (w words)
          (when (and pp prev)
            (let* ((state (cons pp prev))
                   (tr (gethash state table nil))
                   (entry (assq w tr)))
              (if entry (setcdr entry (1+ (cdr entry)))
                (puthash state (cons (cons w 1) tr) table))))
          (setq pp prev)
          (setq prev w))
        table)))

  (fset 'neovm--mc-trigram-to-sorted
    (lambda (table)
      (let ((result nil))
        (maphash (lambda (state transitions)
                   (setq result
                         (cons (cons state
                                     (sort (copy-sequence transitions)
                                           (lambda (a b)
                                             (string< (symbol-name (car a))
                                                      (symbol-name (car b))))))
                               result)))
                 table)
        (sort result (lambda (a b)
                       (let ((sa (concat (symbol-name (caar a)) "-"
                                         (symbol-name (cdar a))))
                             (sb (concat (symbol-name (caar b)) "-"
                                         (symbol-name (cdar b)))))
                         (string< sa sb)))))))

  (unwind-protect
      (let* ((text '(I like to eat fish I like to eat cake
                     I like to play games I want to eat fish))
             (table (funcall 'neovm--mc-build-trigram text)))
        (list
          ;; Full trigram table sorted
          (funcall 'neovm--mc-trigram-to-sorted table)
          ;; Number of unique bigram states
          (hash-table-count table)
          ;; Lookup specific trigram transitions
          (gethash '(like . to) table)
          (gethash '(to . eat) table)))
    (fmakunbound 'neovm--mc-build-trigram)
    (fmakunbound 'neovm--mc-trigram-to-sorted)))"#;
    let expect = expect_test::expect![[
        r#""OK ((((I . like) (to . 3)) ((I . want) (to . 1)) ((cake . I) (like . 1)) ((eat . cake) (I . 1)) ((eat . fish) (I . 1)) ((fish . I) (like . 1)) ((games . I) (want . 1)) ((like . to) (eat . 2) (play . 1)) ((play . games) (I . 1)) ((to . eat) (cake . 1) (fish . 2)) ((to . play) (games . 1)) ((want . to) (eat . 1))) 12 ((play . 1) (eat . 2)) ((cake . 1) (fish . 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Probability distribution from frequency counts
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_markov_probability_distribution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Convert raw frequency counts into probability distributions (as percentages
    // to avoid floating-point comparison issues).
    let form = r#"(progn
  (fset 'neovm--mc-build-freq
    (lambda (words)
      (let ((table (make-hash-table)) (prev nil))
        (dolist (w words)
          (when prev
            (let* ((tr (gethash prev table nil))
                   (entry (assq w tr)))
              (if entry (setcdr entry (1+ (cdr entry)))
                (puthash prev (cons (cons w 1) tr) table))))
          (setq prev w))
        table)))

  (fset 'neovm--mc-probabilities
    (lambda (transitions)
      "Convert frequency alist to probability percentages (integer 0-100)."
      (let ((total 0))
        (dolist (entry transitions)
          (setq total (+ total (cdr entry))))
        (if (= total 0) nil
          (let ((result nil))
            (dolist (entry transitions)
              (setq result
                    (cons (cons (car entry)
                                (/ (* 100 (cdr entry)) total))
                          result)))
            (sort result (lambda (a b)
                           (string< (symbol-name (car a))
                                    (symbol-name (car b))))))))))

  (fset 'neovm--mc-all-probs
    (lambda (table)
      "Return sorted alist of (state . prob-distribution) for all states."
      (let ((result nil))
        (maphash (lambda (state transitions)
                   (setq result
                         (cons (cons state
                                     (funcall 'neovm--mc-probabilities transitions))
                               result)))
                 table)
        (sort result (lambda (a b)
                       (string< (symbol-name (car a))
                                (symbol-name (car b))))))))

  (unwind-protect
      (let* ((text '(a b c a b d a b c a c d a b c b c a))
             (table (funcall 'neovm--mc-build-freq text)))
        (list
          ;; All probability distributions
          (funcall 'neovm--mc-all-probs table)
          ;; Specific: what follows 'a?
          (funcall 'neovm--mc-probabilities (gethash 'a table))
          ;; Specific: what follows 'b?
          (funcall 'neovm--mc-probabilities (gethash 'b table))
          ;; Verify raw counts
          (let ((counts nil))
            (maphash (lambda (k v)
                       (let ((total 0))
                         (dolist (entry v) (setq total (+ total (cdr entry))))
                         (setq counts (cons (cons k total) counts))))
                     table)
            (sort counts (lambda (a b)
                           (string< (symbol-name (car a))
                                    (symbol-name (car b))))))))
    (fmakunbound 'neovm--mc-build-freq)
    (fmakunbound 'neovm--mc-probabilities)
    (fmakunbound 'neovm--mc-all-probs)))"#;
    let expect = expect_test::expect![[
        r#""OK (((a (b . 80) (c . 20)) (b (c . 80) (d . 20)) (c (a . 60) (b . 20) (d . 20)) (d (a . 100))) ((b . 80) (c . 20)) ((c . 80) (d . 20)) ((a . 5) (b . 5) (c . 5) (d . 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Most common transitions (top-K analysis)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_markov_most_common_transitions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Find the top-K most common transitions across all states, and
    // identify "hub" states (states with the most outgoing transitions).
    let form = r#"(progn
  (fset 'neovm--mc-build2
    (lambda (words)
      (let ((table (make-hash-table)) (prev nil))
        (dolist (w words)
          (when prev
            (let* ((tr (gethash prev table nil))
                   (entry (assq w tr)))
              (if entry (setcdr entry (1+ (cdr entry)))
                (puthash prev (cons (cons w 1) tr) table))))
          (setq prev w))
        table)))

  (fset 'neovm--mc-all-transitions
    (lambda (table)
      "Flatten all transitions into a list of (from to count) triples."
      (let ((result nil))
        (maphash (lambda (state transitions)
                   (dolist (entry transitions)
                     (setq result
                           (cons (list state (car entry) (cdr entry))
                                 result))))
                 table)
        result)))

  (fset 'neovm--mc-top-k
    (lambda (triples k)
      "Return top K transitions sorted by count descending, then alphabetically."
      (let ((sorted (sort (copy-sequence triples)
                          (lambda (a b)
                            (or (> (nth 2 a) (nth 2 b))
                                (and (= (nth 2 a) (nth 2 b))
                                     (string< (symbol-name (nth 0 a))
                                              (symbol-name (nth 0 b)))))))))
        (let ((result nil) (i 0))
          (while (and (< i k) sorted)
            (setq result (cons (car sorted) result))
            (setq sorted (cdr sorted))
            (setq i (1+ i)))
          (nreverse result)))))

  (fset 'neovm--mc-hub-states
    (lambda (table)
      "Find states sorted by number of distinct outgoing transitions."
      (let ((hubs nil))
        (maphash (lambda (state transitions)
                   (setq hubs (cons (cons state (length transitions)) hubs)))
                 table)
        (sort hubs (lambda (a b)
                     (or (> (cdr a) (cdr b))
                         (and (= (cdr a) (cdr b))
                              (string< (symbol-name (car a))
                                       (symbol-name (car b))))))))))

  (unwind-protect
      (let* ((text '(the cat sat on the mat the cat ate the fish
                     the dog sat on the mat the dog ate the bone
                     the cat sat on the floor the dog ran to the park))
             (table (funcall 'neovm--mc-build2 text))
             (all (funcall 'neovm--mc-all-transitions table)))
        (list
          ;; Top 5 most frequent transitions
          (funcall 'neovm--mc-top-k all 5)
          ;; Hub states (most diverse outgoing transitions)
          (funcall 'neovm--mc-hub-states table)
          ;; Total number of unique transitions
          (length all)
          ;; Total transition count
          (let ((total 0))
            (dolist (tr all) (setq total (+ total (nth 2 tr))))
            total)))
    (fmakunbound 'neovm--mc-build2)
    (fmakunbound 'neovm--mc-all-transitions)
    (fmakunbound 'neovm--mc-top-k)
    (fmakunbound 'neovm--mc-hub-states)))"#;
    let expect = expect_test::expect![[
        r#""OK (((on the 3) (sat on 3) (the cat 3) (the dog 3) (ate the 2)) ((the . 7) (dog . 3) (cat . 2) (ate . 1) (bone . 1) (fish . 1) (floor . 1) (mat . 1) (on . 1) (ran . 1) (sat . 1) (to . 1)) 21 33)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// State space exploration: reachability, cycles, absorbing states
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_markov_state_space_exploration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Analyze the state space of the Markov chain:
    // - Find all reachable states from a given start
    // - Identify absorbing states (no outgoing transitions)
    // - Identify transient vs recurring states
    let form = r#"(progn
  (fset 'neovm--mc-build3
    (lambda (words)
      (let ((table (make-hash-table)) (prev nil))
        (dolist (w words)
          (when prev
            (let* ((tr (gethash prev table nil))
                   (entry (assq w tr)))
              (if entry (setcdr entry (1+ (cdr entry)))
                (puthash prev (cons (cons w 1) tr) table))))
          (setq prev w))
        table)))

  (fset 'neovm--mc-reachable
    (lambda (table start)
      "BFS to find all states reachable from START."
      (let ((visited (make-hash-table))
            (queue (list start))
            (result nil))
        (puthash start t visited)
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (setq result (cons current result))
            (dolist (entry (gethash current table nil))
              (unless (gethash (car entry) visited)
                (puthash (car entry) t visited)
                (setq queue (append queue (list (car entry))))))))
        (sort result (lambda (a b)
                       (string< (symbol-name a) (symbol-name b)))))))

  (fset 'neovm--mc-absorbing-states
    (lambda (table all-states)
      "Find states with no outgoing transitions."
      (let ((absorbing nil))
        (dolist (s all-states)
          (when (null (gethash s table nil))
            (setq absorbing (cons s absorbing))))
        (sort absorbing (lambda (a b)
                          (string< (symbol-name a) (symbol-name b)))))))

  (fset 'neovm--mc-all-states
    (lambda (table words)
      "Collect all unique states (both sources and destinations)."
      (let ((states (make-hash-table)))
        (dolist (w words) (puthash w t states))
        (let ((result nil))
          (maphash (lambda (k v) (setq result (cons k result))) states)
          (sort result (lambda (a b)
                         (string< (symbol-name a) (symbol-name b))))))))

  (fset 'neovm--mc-self-loops
    (lambda (table)
      "Find states that can transition to themselves."
      (let ((result nil))
        (maphash (lambda (state transitions)
                   (dolist (entry transitions)
                     (when (eq (car entry) state)
                       (setq result (cons (cons state (cdr entry)) result)))))
                 table)
        (sort result (lambda (a b)
                       (string< (symbol-name (car a))
                                (symbol-name (car b))))))))

  (unwind-protect
      (let* ((text '(a b c d a b e f a b c d b c a e f g))
             (table (funcall 'neovm--mc-build3 text))
             (all (funcall 'neovm--mc-all-states table text)))
        (list
          ;; All unique states
          all
          ;; Reachable from 'a
          (funcall 'neovm--mc-reachable table 'a)
          ;; Reachable from 'e
          (funcall 'neovm--mc-reachable table 'e)
          ;; Absorbing states (terminal words like 'g)
          (funcall 'neovm--mc-absorbing-states table all)
          ;; Self-loops (if any)
          (funcall 'neovm--mc-self-loops table)
          ;; Out-degree for each state
          (let ((degrees nil))
            (dolist (s all)
              (setq degrees
                    (cons (cons s (length (gethash s table nil)))
                          degrees)))
            (sort degrees (lambda (a b)
                            (string< (symbol-name (car a))
                                     (symbol-name (car b))))))))
    (fmakunbound 'neovm--mc-build3)
    (fmakunbound 'neovm--mc-reachable)
    (fmakunbound 'neovm--mc-absorbing-states)
    (fmakunbound 'neovm--mc-all-states)
    (fmakunbound 'neovm--mc-self-loops)))"#;
    let expect = expect_test::expect![[
        r#""OK ((a b c d e f g) (a b c d e f g) (a b c d e f g) (g) nil ((a . 2) (b . 2) (c . 2) (d . 2) (e . 1) (f . 2) (g . 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

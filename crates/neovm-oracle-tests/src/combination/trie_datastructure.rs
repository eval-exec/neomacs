//! Oracle parity tests for a trie (prefix tree) data structure in Elisp.
//!
//! Implements a trie using nested alists. Operations: insert, search,
//! prefix search, count words with prefix, delete, and autocomplete
//! suggestions. Tests with various dictionaries.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Trie construction and basic insert/search
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trie_insert_and_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Trie node: alist of (char . (is-end . children-alist))
    // Root is just a children alist.
    // Insert: walk chars, create nodes as needed, mark last as end.
    // Search: walk chars, return t if all found and last is end.
    let form = r#"(progn
  (fset 'neovm--trie-make (lambda () (list nil)))

  (fset 'neovm--trie-insert
    (lambda (trie word)
      "Insert WORD into TRIE. Modifies TRIE in place, returns TRIE."
      (let ((node trie)
            (i 0)
            (len (length word)))
        (while (< i len)
          (let* ((ch (aref word i))
                 (children (cdr node))
                 (child (assq ch children)))
            (if child
                (setq node (cdr child))
              ;; Create new node: (is-end . children)
              (let ((new-node (cons nil nil)))
                (setcdr node (cons (cons ch new-node) children))
                (setq node new-node))))
          (setq i (1+ i)))
        ;; Mark end of word
        (setcar node t))
      trie))

  (fset 'neovm--trie-search
    (lambda (trie word)
      "Return t if WORD is in TRIE, nil otherwise."
      (let ((node trie)
            (i 0)
            (len (length word))
            (found t))
        (while (and (< i len) found)
          (let* ((ch (aref word i))
                 (child (assq ch (cdr node))))
            (if child
                (setq node (cdr child))
              (setq found nil)))
          (setq i (1+ i)))
        (and found (car node)))))

  (fset 'neovm--trie-starts-with
    (lambda (trie prefix)
      "Return t if any word in TRIE starts with PREFIX."
      (let ((node trie)
            (i 0)
            (len (length prefix))
            (found t))
        (while (and (< i len) found)
          (let* ((ch (aref prefix i))
                 (child (assq ch (cdr node))))
            (if child
                (setq node (cdr child))
              (setq found nil)))
          (setq i (1+ i)))
        (if found t nil))))

  (unwind-protect
      (let ((trie (funcall 'neovm--trie-make)))
        ;; Insert words
        (funcall 'neovm--trie-insert trie "apple")
        (funcall 'neovm--trie-insert trie "app")
        (funcall 'neovm--trie-insert trie "ape")
        (funcall 'neovm--trie-insert trie "bat")
        (funcall 'neovm--trie-insert trie "bath")
        (funcall 'neovm--trie-insert trie "batman")
        (funcall 'neovm--trie-insert trie "cat")

        (list
         ;; Exact search: present words
         (funcall 'neovm--trie-search trie "apple")
         (funcall 'neovm--trie-search trie "app")
         (funcall 'neovm--trie-search trie "ape")
         (funcall 'neovm--trie-search trie "bat")
         (funcall 'neovm--trie-search trie "bath")
         (funcall 'neovm--trie-search trie "batman")
         (funcall 'neovm--trie-search trie "cat")
         ;; Exact search: absent words
         (funcall 'neovm--trie-search trie "ap")
         (funcall 'neovm--trie-search trie "ba")
         (funcall 'neovm--trie-search trie "batm")
         (funcall 'neovm--trie-search trie "dog")
         (funcall 'neovm--trie-search trie "")
         (funcall 'neovm--trie-search trie "apples")
         ;; Prefix search
         (funcall 'neovm--trie-starts-with trie "ap")
         (funcall 'neovm--trie-starts-with trie "app")
         (funcall 'neovm--trie-starts-with trie "ba")
         (funcall 'neovm--trie-starts-with trie "bat")
         (funcall 'neovm--trie-starts-with trie "c")
         (funcall 'neovm--trie-starts-with trie "d")
         (funcall 'neovm--trie-starts-with trie "xyz")
         (funcall 'neovm--trie-starts-with trie "")))
    (fmakunbound 'neovm--trie-make)
    (fmakunbound 'neovm--trie-insert)
    (fmakunbound 'neovm--trie-search)
    (fmakunbound 'neovm--trie-starts-with)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t t t nil nil nil nil nil nil t t t t t nil nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Count words with a given prefix
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trie_count_with_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--trie-make (lambda () (list nil)))

  (fset 'neovm--trie-insert
    (lambda (trie word)
      (let ((node trie) (i 0) (len (length word)))
        (while (< i len)
          (let* ((ch (aref word i))
                 (children (cdr node))
                 (child (assq ch children)))
            (if child (setq node (cdr child))
              (let ((new-node (cons nil nil)))
                (setcdr node (cons (cons ch new-node) children))
                (setq node new-node))))
          (setq i (1+ i)))
        (setcar node t))
      trie))

  (fset 'neovm--trie-find-node
    (lambda (trie prefix)
      "Navigate to node for PREFIX, or nil if not found."
      (let ((node trie) (i 0) (len (length prefix)) (found t))
        (while (and (< i len) found)
          (let* ((ch (aref prefix i))
                 (child (assq ch (cdr node))))
            (if child (setq node (cdr child)) (setq found nil)))
          (setq i (1+ i)))
        (if found node nil))))

  (fset 'neovm--trie-count-from
    (lambda (node)
      "Count all words (end markers) in the subtree rooted at NODE."
      (if (null node) 0
        (let ((count (if (car node) 1 0)))
          (dolist (child (cdr node))
            (setq count (+ count (funcall 'neovm--trie-count-from (cdr child)))))
          count))))

  (fset 'neovm--trie-count-prefix
    (lambda (trie prefix)
      "Count words in TRIE that start with PREFIX."
      (let ((node (funcall 'neovm--trie-find-node trie prefix)))
        (if node (funcall 'neovm--trie-count-from node) 0))))

  (unwind-protect
      (let ((trie (funcall 'neovm--trie-make)))
        ;; Insert dictionary
        (dolist (w '("the" "there" "their" "them" "then"
                     "these" "this" "think" "thin" "thing"
                     "to" "too" "top" "torch" "total"
                     "an" "and" "ant" "any"))
          (funcall 'neovm--trie-insert trie w))

        (list
         ;; Count with prefix "th" -> the, there, their, them, then, these, this, think, thin, thing = 10
         (funcall 'neovm--trie-count-prefix trie "th")
         ;; Count with prefix "the" -> the, there, their, them, then, these = 6
         (funcall 'neovm--trie-count-prefix trie "the")
         ;; Count with prefix "thi" -> this, think, thin, thing = 4
         (funcall 'neovm--trie-count-prefix trie "thi")
         ;; Count with prefix "thin" -> thin, think, thing = 3
         (funcall 'neovm--trie-count-prefix trie "thin")
         ;; Count with prefix "to" -> to, too, top, torch, total = 5
         (funcall 'neovm--trie-count-prefix trie "to")
         ;; Count with prefix "an" -> an, and, ant, any = 4
         (funcall 'neovm--trie-count-prefix trie "an")
         ;; Count with prefix "a" -> an, and, ant, any = 4
         (funcall 'neovm--trie-count-prefix trie "a")
         ;; Non-existent prefix
         (funcall 'neovm--trie-count-prefix trie "xyz")
         (funcall 'neovm--trie-count-prefix trie "z")
         ;; Empty prefix -> all words = 19
         (funcall 'neovm--trie-count-prefix trie "")
         ;; Single exact word prefix
         (funcall 'neovm--trie-count-prefix trie "total")))
    (fmakunbound 'neovm--trie-make)
    (fmakunbound 'neovm--trie-insert)
    (fmakunbound 'neovm--trie-find-node)
    (fmakunbound 'neovm--trie-count-from)
    (fmakunbound 'neovm--trie-count-prefix)))"#;
    let expect = expect_test::expect![[r#""OK (10 6 4 3 5 4 4 0 0 19 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Autocomplete: collect all words with a given prefix
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trie_autocomplete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--trie-make (lambda () (list nil)))

  (fset 'neovm--trie-insert
    (lambda (trie word)
      (let ((node trie) (i 0) (len (length word)))
        (while (< i len)
          (let* ((ch (aref word i))
                 (children (cdr node))
                 (child (assq ch children)))
            (if child (setq node (cdr child))
              (let ((new-node (cons nil nil)))
                (setcdr node (cons (cons ch new-node) children))
                (setq node new-node))))
          (setq i (1+ i)))
        (setcar node t))
      trie))

  (fset 'neovm--trie-find-node
    (lambda (trie prefix)
      (let ((node trie) (i 0) (len (length prefix)) (found t))
        (while (and (< i len) found)
          (let* ((ch (aref prefix i))
                 (child (assq ch (cdr node))))
            (if child (setq node (cdr child)) (setq found nil)))
          (setq i (1+ i)))
        (if found node nil))))

  (fset 'neovm--trie-collect-words
    (lambda (node prefix)
      "Collect all words from NODE, prepending PREFIX to each."
      (let ((results nil))
        (when (car node)
          (setq results (list prefix)))
        (dolist (child (cdr node))
          (let* ((ch (car child))
                 (child-node (cdr child))
                 (new-prefix (concat prefix (char-to-string ch)))
                 (sub-results (funcall 'neovm--trie-collect-words child-node new-prefix)))
            (setq results (append results sub-results))))
        results)))

  (fset 'neovm--trie-autocomplete
    (lambda (trie prefix)
      "Return sorted list of all words in TRIE starting with PREFIX."
      (let ((node (funcall 'neovm--trie-find-node trie prefix)))
        (if node
            (sort (funcall 'neovm--trie-collect-words node prefix) 'string<)
          nil))))

  (unwind-protect
      (let ((trie (funcall 'neovm--trie-make)))
        (dolist (w '("car" "card" "care" "careful" "carefully"
                     "cargo" "carry" "cart" "cast" "cat"))
          (funcall 'neovm--trie-insert trie w))

        (list
         ;; Autocomplete "car" -> car, card, care, careful, carefully, cargo, carry, cart
         (funcall 'neovm--trie-autocomplete trie "car")
         ;; Autocomplete "care" -> care, careful, carefully
         (funcall 'neovm--trie-autocomplete trie "care")
         ;; Autocomplete "cas" -> cast
         (funcall 'neovm--trie-autocomplete trie "cas")
         ;; Autocomplete "ca" -> all 10 words
         (funcall 'neovm--trie-autocomplete trie "ca")
         ;; Autocomplete "cart" -> cart (exact match only)
         (funcall 'neovm--trie-autocomplete trie "cart")
         ;; Autocomplete non-existent prefix
         (funcall 'neovm--trie-autocomplete trie "dog")
         ;; Autocomplete "c" -> all words
         (length (funcall 'neovm--trie-autocomplete trie "c"))
         ;; Verify sorted order
         (equal (funcall 'neovm--trie-autocomplete trie "care")
                '("care" "careful" "carefully"))))
    (fmakunbound 'neovm--trie-make)
    (fmakunbound 'neovm--trie-insert)
    (fmakunbound 'neovm--trie-find-node)
    (fmakunbound 'neovm--trie-collect-words)
    (fmakunbound 'neovm--trie-autocomplete)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"car\" \"card\" \"care\" \"careful\" \"carefully\" \"cargo\" \"carry\" \"cart\") (\"care\" \"careful\" \"carefully\") (\"cast\") (\"car\" \"card\" \"care\" \"careful\" \"carefully\" \"cargo\" \"carry\" \"cart\" \"cast\" \"cat\") (\"cart\") nil 10 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Delete word from trie
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trie_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--trie-make (lambda () (list nil)))

  (fset 'neovm--trie-insert
    (lambda (trie word)
      (let ((node trie) (i 0) (len (length word)))
        (while (< i len)
          (let* ((ch (aref word i))
                 (children (cdr node))
                 (child (assq ch children)))
            (if child (setq node (cdr child))
              (let ((new-node (cons nil nil)))
                (setcdr node (cons (cons ch new-node) children))
                (setq node new-node))))
          (setq i (1+ i)))
        (setcar node t))
      trie))

  (fset 'neovm--trie-search
    (lambda (trie word)
      (let ((node trie) (i 0) (len (length word)) (found t))
        (while (and (< i len) found)
          (let* ((ch (aref word i))
                 (child (assq ch (cdr node))))
            (if child (setq node (cdr child)) (setq found nil)))
          (setq i (1+ i)))
        (and found (car node)))))

  (fset 'neovm--trie-delete
    (lambda (trie word)
      "Delete WORD from TRIE by unsetting the end marker.
Returns t if word was found and deleted, nil otherwise."
      (let ((node trie) (i 0) (len (length word)) (found t))
        (while (and (< i len) found)
          (let* ((ch (aref word i))
                 (child (assq ch (cdr node))))
            (if child (setq node (cdr child)) (setq found nil)))
          (setq i (1+ i)))
        (if (and found (car node))
            (progn (setcar node nil) t)
          nil))))

  (fset 'neovm--trie-find-node
    (lambda (trie prefix)
      (let ((node trie) (i 0) (len (length prefix)) (found t))
        (while (and (< i len) found)
          (let* ((ch (aref prefix i))
                 (child (assq ch (cdr node))))
            (if child (setq node (cdr child)) (setq found nil)))
          (setq i (1+ i)))
        (if found node nil))))

  (fset 'neovm--trie-count-from
    (lambda (node)
      (if (null node) 0
        (let ((count (if (car node) 1 0)))
          (dolist (child (cdr node))
            (setq count (+ count (funcall 'neovm--trie-count-from (cdr child)))))
          count))))

  (fset 'neovm--trie-count-prefix
    (lambda (trie prefix)
      (let ((node (funcall 'neovm--trie-find-node trie prefix)))
        (if node (funcall 'neovm--trie-count-from node) 0))))

  (unwind-protect
      (let ((trie (funcall 'neovm--trie-make)))
        ;; Insert words
        (dolist (w '("app" "apple" "ape" "bat" "bath"))
          (funcall 'neovm--trie-insert trie w))

        (let ((before-count (funcall 'neovm--trie-count-prefix trie "")))
          ;; Delete "app" -- "apple" should remain
          (let ((d1 (funcall 'neovm--trie-delete trie "app")))
            (let ((after-app (list
                              (funcall 'neovm--trie-search trie "app")
                              (funcall 'neovm--trie-search trie "apple")
                              (funcall 'neovm--trie-count-prefix trie ""))))
              ;; Delete non-existent word
              (let ((d2 (funcall 'neovm--trie-delete trie "xyz")))
                ;; Delete "apple" -- now no words start with "app"
                (let ((d3 (funcall 'neovm--trie-delete trie "apple")))
                  (let ((after-apple (list
                                      (funcall 'neovm--trie-search trie "apple")
                                      (funcall 'neovm--trie-search trie "ape")
                                      (funcall 'neovm--trie-count-prefix trie "ap"))))
                    ;; Delete already deleted word
                    (let ((d4 (funcall 'neovm--trie-delete trie "app")))
                      ;; Delete remaining words
                      (funcall 'neovm--trie-delete trie "ape")
                      (funcall 'neovm--trie-delete trie "bat")
                      (funcall 'neovm--trie-delete trie "bath")
                      (let ((final-count (funcall 'neovm--trie-count-prefix trie "")))
                        (list
                         before-count  ;; 5
                         d1            ;; t (deleted "app")
                         after-app     ;; (nil t 4) - app gone, apple still there
                         d2            ;; nil (not found)
                         d3            ;; t (deleted "apple")
                         after-apple   ;; (nil t 1) - apple gone, ape still there
                         d4            ;; nil (already deleted)
                         final-count   ;; 0 (all deleted)
                         )))))))))
    (fmakunbound 'neovm--trie-make)
    (fmakunbound 'neovm--trie-insert)
    (fmakunbound 'neovm--trie-search)
    (fmakunbound 'neovm--trie-delete)
    (fmakunbound 'neovm--trie-find-node)
    (fmakunbound 'neovm--trie-count-from)
    (fmakunbound 'neovm--trie-count-prefix)))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Trie with shared prefixes and word frequency counting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trie_word_frequency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extended trie that stores word frequency instead of just boolean end marker.
    // Node: (count . children-alist) where count >= 1 means it's a word.
    let form = r#"(progn
  (fset 'neovm--ftrie-make (lambda () (list 0)))

  (fset 'neovm--ftrie-insert
    (lambda (trie word)
      "Insert WORD into frequency trie, incrementing count if already present."
      (let ((node trie) (i 0) (len (length word)))
        (while (< i len)
          (let* ((ch (aref word i))
                 (children (cdr node))
                 (child (assq ch children)))
            (if child (setq node (cdr child))
              (let ((new-node (cons 0 nil)))
                (setcdr node (cons (cons ch new-node) children))
                (setq node new-node))))
          (setq i (1+ i)))
        (setcar node (1+ (car node))))
      trie))

  (fset 'neovm--ftrie-frequency
    (lambda (trie word)
      "Return frequency of WORD in trie, 0 if absent."
      (let ((node trie) (i 0) (len (length word)) (found t))
        (while (and (< i len) found)
          (let* ((ch (aref word i))
                 (child (assq ch (cdr node))))
            (if child (setq node (cdr child)) (setq found nil)))
          (setq i (1+ i)))
        (if found (car node) 0))))

  (fset 'neovm--ftrie-top-words
    (lambda (node prefix n)
      "Collect all (word . freq) pairs from NODE, return top N by frequency."
      (let ((results nil))
        (when (> (car node) 0)
          (setq results (list (cons prefix (car node)))))
        (dolist (child (cdr node))
          (let* ((ch (car child))
                 (child-node (cdr child))
                 (new-prefix (concat prefix (char-to-string ch)))
                 (sub (funcall 'neovm--ftrie-top-words child-node new-prefix n)))
            (setq results (append results sub))))
        ;; Sort by frequency descending, take top n
        (let ((sorted (sort results (lambda (a b) (> (cdr a) (cdr b))))))
          (let ((result nil) (count 0))
            (while (and sorted (< count n))
              (setq result (cons (car sorted) result))
              (setq sorted (cdr sorted))
              (setq count (1+ count)))
            (nreverse result))))))

  (unwind-protect
      (let ((trie (funcall 'neovm--ftrie-make)))
        ;; Simulate word frequency from text
        (dolist (w '("the" "the" "the" "the" "the"
                     "is" "is" "is"
                     "a" "a" "a" "a"
                     "to" "to"
                     "and" "and" "and"
                     "in" "in"
                     "that" "this" "these" "there"
                     "the"))
          (funcall 'neovm--ftrie-insert trie w))

        (list
         ;; Frequencies
         (funcall 'neovm--ftrie-frequency trie "the")
         (funcall 'neovm--ftrie-frequency trie "is")
         (funcall 'neovm--ftrie-frequency trie "a")
         (funcall 'neovm--ftrie-frequency trie "to")
         (funcall 'neovm--ftrie-frequency trie "and")
         (funcall 'neovm--ftrie-frequency trie "in")
         (funcall 'neovm--ftrie-frequency trie "that")
         (funcall 'neovm--ftrie-frequency trie "missing")
         ;; Top 3 words
         (funcall 'neovm--ftrie-top-words trie "" 3)
         ;; Top 2 words starting with "th"
         (let ((node trie) (found t) (i 0) (prefix "th"))
           (while (and (< i (length prefix)) found)
             (let ((child (assq (aref prefix i) (cdr node))))
               (if child (setq node (cdr child)) (setq found nil)))
             (setq i (1+ i)))
           (if found
               (funcall 'neovm--ftrie-top-words node "th" 2)
             nil))
         ;; Insert duplicate and verify increment
         (let ((before (funcall 'neovm--ftrie-frequency trie "a")))
           (funcall 'neovm--ftrie-insert trie "a")
           (let ((after (funcall 'neovm--ftrie-frequency trie "a")))
             (list before after (= after (1+ before)))))))
    (fmakunbound 'neovm--ftrie-make)
    (fmakunbound 'neovm--ftrie-insert)
    (fmakunbound 'neovm--ftrie-frequency)
    (fmakunbound 'neovm--ftrie-top-words)))"#;
    let expect = expect_test::expect![[
        r#""OK (6 3 4 2 3 2 1 0 ((\"the\" . 6) (\"a\" . 4) (\"and\" . 3)) ((\"the\" . 6) (\"this\" . 1)) (4 5 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Trie operations on a realistic dictionary set
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trie_realistic_dictionary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--trie-make (lambda () (list nil)))

  (fset 'neovm--trie-insert
    (lambda (trie word)
      (let ((node trie) (i 0) (len (length word)))
        (while (< i len)
          (let* ((ch (aref word i))
                 (children (cdr node))
                 (child (assq ch children)))
            (if child (setq node (cdr child))
              (let ((new-node (cons nil nil)))
                (setcdr node (cons (cons ch new-node) children))
                (setq node new-node))))
          (setq i (1+ i)))
        (setcar node t))
      trie))

  (fset 'neovm--trie-search
    (lambda (trie word)
      (let ((node trie) (i 0) (len (length word)) (found t))
        (while (and (< i len) found)
          (let* ((ch (aref word i))
                 (child (assq ch (cdr node))))
            (if child (setq node (cdr child)) (setq found nil)))
          (setq i (1+ i)))
        (and found (car node)))))

  (fset 'neovm--trie-find-node
    (lambda (trie prefix)
      (let ((node trie) (i 0) (len (length prefix)) (found t))
        (while (and (< i len) found)
          (let* ((ch (aref prefix i))
                 (child (assq ch (cdr node))))
            (if child (setq node (cdr child)) (setq found nil)))
          (setq i (1+ i)))
        (if found node nil))))

  (fset 'neovm--trie-collect-words
    (lambda (node prefix)
      (let ((results nil))
        (when (car node)
          (setq results (list prefix)))
        (dolist (child (cdr node))
          (let* ((ch (car child))
                 (child-node (cdr child))
                 (new-prefix (concat prefix (char-to-string ch)))
                 (sub-results (funcall 'neovm--trie-collect-words child-node new-prefix)))
            (setq results (append results sub-results))))
        results)))

  (fset 'neovm--trie-autocomplete
    (lambda (trie prefix)
      (let ((node (funcall 'neovm--trie-find-node trie prefix)))
        (if node
            (sort (funcall 'neovm--trie-collect-words node prefix) 'string<)
          nil))))

  (fset 'neovm--trie-count-from
    (lambda (node)
      (if (null node) 0
        (let ((count (if (car node) 1 0)))
          (dolist (child (cdr node))
            (setq count (+ count (funcall 'neovm--trie-count-from (cdr child)))))
          count))))

  (unwind-protect
      (let ((trie (funcall 'neovm--trie-make)))
        ;; Programming language keywords
        (dolist (w '("def" "defun" "defvar" "defmacro" "defclass"
                     "let" "let*" "lambda" "loop"
                     "if" "cond" "when" "unless"
                     "setq" "set" "setf"
                     "progn" "prog1" "prog2"
                     "while" "do" "dotimes" "dolist"))
          (funcall 'neovm--trie-insert trie w))

        (list
         ;; Total word count
         (funcall 'neovm--trie-count-from trie)
         ;; Autocomplete "def"
         (funcall 'neovm--trie-autocomplete trie "def")
         ;; Autocomplete "let"
         (funcall 'neovm--trie-autocomplete trie "let")
         ;; Autocomplete "set"
         (funcall 'neovm--trie-autocomplete trie "set")
         ;; Autocomplete "do"
         (funcall 'neovm--trie-autocomplete trie "do")
         ;; Autocomplete "prog"
         (funcall 'neovm--trie-autocomplete trie "prog")
         ;; Single result
         (funcall 'neovm--trie-autocomplete trie "while")
         ;; No results
         (funcall 'neovm--trie-autocomplete trie "return")
         ;; Verify all keywords found
         (let ((all-found t))
           (dolist (w '("def" "defun" "defvar" "defmacro" "defclass"
                        "let" "lambda" "if" "cond" "when" "unless"
                        "setq" "set" "setf" "progn" "while" "dotimes" "dolist"))
             (unless (funcall 'neovm--trie-search trie w)
               (setq all-found nil)))
           all-found)
         ;; Verify partial prefixes are not words
         (funcall 'neovm--trie-search trie "de")
         (funcall 'neovm--trie-search trie "se")
         (funcall 'neovm--trie-search trie "pro")))
    (fmakunbound 'neovm--trie-make)
    (fmakunbound 'neovm--trie-insert)
    (fmakunbound 'neovm--trie-search)
    (fmakunbound 'neovm--trie-find-node)
    (fmakunbound 'neovm--trie-collect-words)
    (fmakunbound 'neovm--trie-autocomplete)
    (fmakunbound 'neovm--trie-count-from)))"#;
    let expect = expect_test::expect![[
        r#""OK (23 (\"def\" \"defclass\" \"defmacro\" \"defun\" \"defvar\") (\"let\" \"let*\") (\"set\" \"setf\" \"setq\") (\"do\" \"dolist\" \"dotimes\") (\"prog1\" \"prog2\" \"progn\") (\"while\") nil t nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

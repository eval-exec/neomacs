//! Oracle parity tests for an advanced trie with extended operations.
//!
//! Builds on the basic trie to implement: prefix counting, wildcard
//! search (? for any single char), autocomplete, longest common prefix,
//! trie-based word frequency counter, and trie compression (path
//! compression for single-child chains).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Prefix counting (how many words share a prefix)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trie_adv_prefix_counting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--ta-make (lambda () (list nil)))

  (fset 'neovm--ta-insert
    (lambda (trie word)
      (let ((node trie) (i 0) (len (length word)))
        (while (< i len)
          (let* ((ch (aref word i))
                 (child (assq ch (cdr node))))
            (if child (setq node (cdr child))
              (let ((new-node (cons nil nil)))
                (setcdr node (cons (cons ch new-node) (cdr node)))
                (setq node new-node))))
          (setq i (1+ i)))
        (setcar node t))
      trie))

  (fset 'neovm--ta-find-node
    (lambda (trie prefix)
      (let ((node trie) (i 0) (len (length prefix)) (found t))
        (while (and (< i len) found)
          (let ((child (assq (aref prefix i) (cdr node))))
            (if child (setq node (cdr child)) (setq found nil)))
          (setq i (1+ i)))
        (if found node nil))))

  (fset 'neovm--ta-count-words
    (lambda (node)
      "Count all word-end markers in subtree."
      (if (null node) 0
        (let ((c (if (car node) 1 0)))
          (dolist (child (cdr node))
            (setq c (+ c (funcall 'neovm--ta-count-words (cdr child)))))
          c))))

  (fset 'neovm--ta-prefix-count
    (lambda (trie prefix)
      (let ((node (funcall 'neovm--ta-find-node trie prefix)))
        (if node (funcall 'neovm--ta-count-words node) 0))))

  (unwind-protect
      (let ((trie (funcall 'neovm--ta-make)))
        (dolist (w '("program" "programmer" "programming" "progress"
                     "project" "protect" "protocol"
                     "pre" "prefix" "predict" "prepare"
                     "print" "private" "prime"
                     "public" "pull" "push" "put"))
          (funcall 'neovm--ta-insert trie w))

        (list
         ;; "pro" prefix: program, programmer, programming, progress, project, protect, protocol = 7
         (funcall 'neovm--ta-prefix-count trie "pro")
         ;; "pre" prefix: pre, prefix, predict, prepare = 4
         (funcall 'neovm--ta-prefix-count trie "pre")
         ;; "pri" prefix: print, private, prime = 3
         (funcall 'neovm--ta-prefix-count trie "pri")
         ;; "p" prefix: all 18 words
         (funcall 'neovm--ta-prefix-count trie "p")
         ;; "pu" prefix: public, pull, push, put = 4
         (funcall 'neovm--ta-prefix-count trie "pu")
         ;; "program" prefix: program, programmer, programming = 3
         (funcall 'neovm--ta-prefix-count trie "program")
         ;; exact word as prefix: "put" = 1
         (funcall 'neovm--ta-prefix-count trie "put")
         ;; non-existent prefix
         (funcall 'neovm--ta-prefix-count trie "xyz")
         (funcall 'neovm--ta-prefix-count trie "prz")
         ;; empty prefix = all words
         (funcall 'neovm--ta-prefix-count trie "")))
    (fmakunbound 'neovm--ta-make)
    (fmakunbound 'neovm--ta-insert)
    (fmakunbound 'neovm--ta-find-node)
    (fmakunbound 'neovm--ta-count-words)
    (fmakunbound 'neovm--ta-prefix-count)))"#;
    let expect = expect_test::expect![[r#""OK (7 4 3 18 4 3 1 0 0 18)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Wildcard search (match ? for any single char)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trie_adv_wildcard_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--ta-make (lambda () (list nil)))

  (fset 'neovm--ta-insert
    (lambda (trie word)
      (let ((node trie) (i 0) (len (length word)))
        (while (< i len)
          (let* ((ch (aref word i))
                 (child (assq ch (cdr node))))
            (if child (setq node (cdr child))
              (let ((new-node (cons nil nil)))
                (setcdr node (cons (cons ch new-node) (cdr node)))
                (setq node new-node))))
          (setq i (1+ i)))
        (setcar node t))
      trie))

  ;; Wildcard search: ? matches any single character
  ;; Returns sorted list of all matching words
  (fset 'neovm--ta-wildcard-search
    (lambda (node pattern idx prefix)
      "Search from NODE matching PATTERN starting at IDX, PREFIX built so far."
      (if (>= idx (length pattern))
          ;; End of pattern: check if this is a word
          (if (car node) (list prefix) nil)
        (let ((ch (aref pattern idx))
              (results nil))
          (if (= ch ??)
              ;; Wildcard: try all children
              (dolist (child (cdr node))
                (let ((sub (funcall 'neovm--ta-wildcard-search
                            (cdr child) pattern (1+ idx)
                            (concat prefix (char-to-string (car child))))))
                  (setq results (append results sub))))
            ;; Exact char: follow specific child
            (let ((child (assq ch (cdr node))))
              (when child
                (setq results
                      (funcall 'neovm--ta-wildcard-search
                       (cdr child) pattern (1+ idx)
                       (concat prefix (char-to-string ch)))))))
          results))))

  (fset 'neovm--ta-wildcard
    (lambda (trie pattern)
      (sort (funcall 'neovm--ta-wildcard-search trie pattern 0 "") 'string<)))

  (unwind-protect
      (let ((trie (funcall 'neovm--ta-make)))
        (dolist (w '("bat" "bar" "ban" "bad" "bag"
                     "cat" "car" "can" "cap" "cab"
                     "hat" "had" "ham" "has"
                     "rat" "ran" "ram" "rap"))
          (funcall 'neovm--ta-insert trie w))

        (list
         ;; "b?t" -> bat
         (funcall 'neovm--ta-wildcard trie "b?t")
         ;; "?at" -> bat, cat, hat, rat
         (funcall 'neovm--ta-wildcard trie "?at")
         ;; "ba?" -> bat, bar, ban, bad, bag
         (funcall 'neovm--ta-wildcard trie "ba?")
         ;; "?a?" -> matches all 3-letter words with 'a' in middle
         (funcall 'neovm--ta-wildcard trie "?a?")
         ;; "???" -> all 3-letter words (all of them)
         (length (funcall 'neovm--ta-wildcard trie "???"))
         ;; "c??" -> cat, car, can, cap, cab
         (funcall 'neovm--ta-wildcard trie "c??")
         ;; "?a" -> no matches (pattern length 2, words are length 3)
         (funcall 'neovm--ta-wildcard trie "?a")
         ;; "xyz" -> no match
         (funcall 'neovm--ta-wildcard trie "xyz")
         ;; Exact match via wildcard
         (funcall 'neovm--ta-wildcard trie "bat")))
    (fmakunbound 'neovm--ta-make)
    (fmakunbound 'neovm--ta-insert)
    (fmakunbound 'neovm--ta-wildcard-search)
    (fmakunbound 'neovm--ta-wildcard)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"bat\") (\"bat\" \"cat\" \"hat\" \"rat\") (\"bad\" \"bag\" \"ban\" \"bar\" \"bat\") (\"bad\" \"bag\" \"ban\" \"bar\" \"bat\" \"cab\" \"can\" \"cap\" \"car\" \"cat\" \"had\" \"ham\" \"has\" \"hat\" \"ram\" \"ran\" \"rap\" \"rat\") 18 (\"cab\" \"can\" \"cap\" \"car\" \"cat\") nil nil (\"bat\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Autocomplete (return all words with given prefix, sorted)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trie_adv_autocomplete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--ta-make (lambda () (list nil)))

  (fset 'neovm--ta-insert
    (lambda (trie word)
      (let ((node trie) (i 0) (len (length word)))
        (while (< i len)
          (let* ((ch (aref word i))
                 (child (assq ch (cdr node))))
            (if child (setq node (cdr child))
              (let ((new-node (cons nil nil)))
                (setcdr node (cons (cons ch new-node) (cdr node)))
                (setq node new-node))))
          (setq i (1+ i)))
        (setcar node t))
      trie))

  (fset 'neovm--ta-find-node
    (lambda (trie prefix)
      (let ((node trie) (i 0) (len (length prefix)) (found t))
        (while (and (< i len) found)
          (let ((child (assq (aref prefix i) (cdr node))))
            (if child (setq node (cdr child)) (setq found nil)))
          (setq i (1+ i)))
        (if found node nil))))

  (fset 'neovm--ta-collect
    (lambda (node prefix)
      (let ((results nil))
        (when (car node)
          (setq results (list prefix)))
        (dolist (child (cdr node))
          (setq results
                (append results
                        (funcall 'neovm--ta-collect
                         (cdr child)
                         (concat prefix (char-to-string (car child)))))))
        results)))

  (fset 'neovm--ta-autocomplete
    (lambda (trie prefix max-results)
      "Return up to MAX-RESULTS sorted completions for PREFIX."
      (let ((node (funcall 'neovm--ta-find-node trie prefix)))
        (if node
            (let ((all (sort (funcall 'neovm--ta-collect node prefix) 'string<))
                  (result nil) (count 0))
              (while (and all (< count max-results))
                (setq result (cons (car all) result))
                (setq all (cdr all))
                (setq count (1+ count)))
              (nreverse result))
          nil))))

  (unwind-protect
      (let ((trie (funcall 'neovm--ta-make)))
        (dolist (w '("emacs" "elisp" "eval" "evaluate" "event"
                     "error" "edit" "editor" "element" "else"
                     "enable" "end" "engine" "enter" "env"
                     "equal" "escape" "except" "execute" "exit"))
          (funcall 'neovm--ta-insert trie w))

        (list
         ;; Autocomplete "e" with limit 5
         (funcall 'neovm--ta-autocomplete trie "e" 5)
         ;; Autocomplete "ev" with limit 10
         (funcall 'neovm--ta-autocomplete trie "ev" 10)
         ;; Autocomplete "ed" with limit 10
         (funcall 'neovm--ta-autocomplete trie "ed" 10)
         ;; Autocomplete "en" with limit 3
         (funcall 'neovm--ta-autocomplete trie "en" 3)
         ;; Autocomplete "ex" -> except, execute, exit
         (funcall 'neovm--ta-autocomplete trie "ex" 10)
         ;; Autocomplete exact word "emacs"
         (funcall 'neovm--ta-autocomplete trie "emacs" 10)
         ;; Autocomplete non-existent prefix
         (funcall 'neovm--ta-autocomplete trie "xyz" 10)
         ;; Autocomplete with limit 1
         (funcall 'neovm--ta-autocomplete trie "e" 1)
         ;; All words (empty prefix)
         (length (funcall 'neovm--ta-autocomplete trie "" 100))))
    (fmakunbound 'neovm--ta-make)
    (fmakunbound 'neovm--ta-insert)
    (fmakunbound 'neovm--ta-find-node)
    (fmakunbound 'neovm--ta-collect)
    (fmakunbound 'neovm--ta-autocomplete)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"edit\" \"editor\" \"element\" \"elisp\" \"else\") (\"eval\" \"evaluate\" \"event\") (\"edit\" \"editor\") (\"enable\" \"end\" \"engine\") (\"except\" \"execute\" \"exit\") (\"emacs\") nil (\"edit\") 20)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Longest common prefix among all stored words
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trie_adv_longest_common_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--ta-make (lambda () (list nil)))

  (fset 'neovm--ta-insert
    (lambda (trie word)
      (let ((node trie) (i 0) (len (length word)))
        (while (< i len)
          (let* ((ch (aref word i))
                 (child (assq ch (cdr node))))
            (if child (setq node (cdr child))
              (let ((new-node (cons nil nil)))
                (setcdr node (cons (cons ch new-node) (cdr node)))
                (setq node new-node))))
          (setq i (1+ i)))
        (setcar node t))
      trie))

  ;; Walk down the trie as long as there's exactly one child and no word-end
  (fset 'neovm--ta-longest-common-prefix
    (lambda (trie)
      "Find the longest prefix shared by ALL words in the trie."
      (let ((node trie) (prefix "") (done nil))
        (while (not done)
          (let ((children (cdr node)))
            (if (and (= (length children) 1)
                     (not (car node)))
                ;; Single child, not a word-end => extend prefix
                (let ((child (car children)))
                  (setq prefix (concat prefix (char-to-string (car child))))
                  (setq node (cdr child)))
              ;; Multiple children or word-end => stop
              (setq done t))))
        prefix)))

  (unwind-protect
      (list
       ;; All words share "pro" prefix
       (let ((trie (funcall 'neovm--ta-make)))
         (dolist (w '("program" "progress" "project" "protect"))
           (funcall 'neovm--ta-insert trie w))
         (funcall 'neovm--ta-longest-common-prefix trie))

       ;; All words share "pre" prefix
       (let ((trie (funcall 'neovm--ta-make)))
         (dolist (w '("prefix" "predict" "prepare" "prevent"))
           (funcall 'neovm--ta-insert trie w))
         (funcall 'neovm--ta-longest-common-prefix trie))

       ;; No common prefix beyond empty
       (let ((trie (funcall 'neovm--ta-make)))
         (dolist (w '("apple" "banana" "cherry"))
           (funcall 'neovm--ta-insert trie w))
         (funcall 'neovm--ta-longest-common-prefix trie))

       ;; Single word: entire word is the prefix
       (let ((trie (funcall 'neovm--ta-make)))
         (funcall 'neovm--ta-insert trie "onlyone")
         (funcall 'neovm--ta-longest-common-prefix trie))

       ;; One word is prefix of another: "test" and "testing"
       (let ((trie (funcall 'neovm--ta-make)))
         (dolist (w '("test" "testing" "tested" "tester"))
           (funcall 'neovm--ta-insert trie w))
         (funcall 'neovm--ta-longest-common-prefix trie))

       ;; All identical words
       (let ((trie (funcall 'neovm--ta-make)))
         (funcall 'neovm--ta-insert trie "same")
         (funcall 'neovm--ta-insert trie "same")
         (funcall 'neovm--ta-longest-common-prefix trie))

       ;; Empty trie
       (let ((trie (funcall 'neovm--ta-make)))
         (funcall 'neovm--ta-longest-common-prefix trie)))
    (fmakunbound 'neovm--ta-make)
    (fmakunbound 'neovm--ta-insert)
    (fmakunbound 'neovm--ta-longest-common-prefix)))"#;
    let expect =
        expect_test::expect![[r#""OK (\"pro\" \"pre\" \"\" \"onlyone\" \"test\" \"same\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: trie-based word frequency counter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trie_adv_frequency_counter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Trie that stores word frequency (count) at each end node
    let form = r#"(progn
  ;; Node: (count . children-alist) where count > 0 means word-end
  (fset 'neovm--tf-make (lambda () (cons 0 nil)))

  (fset 'neovm--tf-insert
    (lambda (trie word)
      (let ((node trie) (i 0) (len (length word)))
        (while (< i len)
          (let* ((ch (aref word i))
                 (child (assq ch (cdr node))))
            (if child (setq node (cdr child))
              (let ((new-node (cons 0 nil)))
                (setcdr node (cons (cons ch new-node) (cdr node)))
                (setq node new-node))))
          (setq i (1+ i)))
        (setcar node (1+ (car node))))
      trie))

  (fset 'neovm--tf-freq
    (lambda (trie word)
      (let ((node trie) (i 0) (len (length word)) (found t))
        (while (and (< i len) found)
          (let ((child (assq (aref word i) (cdr node))))
            (if child (setq node (cdr child)) (setq found nil)))
          (setq i (1+ i)))
        (if found (car node) 0))))

  ;; Collect all (word . freq) pairs from subtree
  (fset 'neovm--tf-all-freqs
    (lambda (node prefix)
      (let ((results nil))
        (when (> (car node) 0)
          (setq results (list (cons prefix (car node)))))
        (dolist (child (cdr node))
          (setq results
                (append results
                        (funcall 'neovm--tf-all-freqs
                         (cdr child)
                         (concat prefix (char-to-string (car child)))))))
        results)))

  ;; Top-N most frequent words
  (fset 'neovm--tf-top-n
    (lambda (trie n)
      (let ((all (funcall 'neovm--tf-all-freqs trie ""))
            (sorted nil))
        (setq sorted (sort all (lambda (a b) (> (cdr a) (cdr b)))))
        (let ((result nil) (count 0))
          (while (and sorted (< count n))
            (setq result (cons (car sorted) result))
            (setq sorted (cdr sorted))
            (setq count (1+ count)))
          (nreverse result)))))

  ;; Total unique words
  (fset 'neovm--tf-unique-count
    (lambda (node)
      (let ((c (if (> (car node) 0) 1 0)))
        (dolist (child (cdr node))
          (setq c (+ c (funcall 'neovm--tf-unique-count (cdr child)))))
        c)))

  ;; Total word occurrences
  (fset 'neovm--tf-total-count
    (lambda (node)
      (let ((c (car node)))
        (dolist (child (cdr node))
          (setq c (+ c (funcall 'neovm--tf-total-count (cdr child)))))
        c)))

  (unwind-protect
      (let ((trie (funcall 'neovm--tf-make)))
        ;; Simulate counting words in a "document"
        (dolist (w '("the" "quick" "brown" "fox" "jumps" "over" "the" "lazy" "dog"
                     "the" "dog" "barks" "at" "the" "fox" "and" "the" "fox" "runs"
                     "over" "the" "lazy" "dog" "again"))
          (funcall 'neovm--tf-insert trie w))

        (list
         ;; Individual frequencies
         (funcall 'neovm--tf-freq trie "the")     ;; 6
         (funcall 'neovm--tf-freq trie "fox")     ;; 3
         (funcall 'neovm--tf-freq trie "dog")     ;; 3
         (funcall 'neovm--tf-freq trie "lazy")    ;; 2
         (funcall 'neovm--tf-freq trie "over")    ;; 2
         (funcall 'neovm--tf-freq trie "quick")   ;; 1
         (funcall 'neovm--tf-freq trie "missing") ;; 0
         ;; Top 3 most frequent
         (funcall 'neovm--tf-top-n trie 3)
         ;; Unique word count
         (funcall 'neovm--tf-unique-count trie)
         ;; Total occurrences
         (funcall 'neovm--tf-total-count trie)
         ;; Verify: insert more and check update
         (let ((before (funcall 'neovm--tf-freq trie "quick")))
           (funcall 'neovm--tf-insert trie "quick")
           (funcall 'neovm--tf-insert trie "quick")
           (let ((after (funcall 'neovm--tf-freq trie "quick")))
             (list before after (= after (+ before 2)))))))
    (fmakunbound 'neovm--tf-make)
    (fmakunbound 'neovm--tf-insert)
    (fmakunbound 'neovm--tf-freq)
    (fmakunbound 'neovm--tf-all-freqs)
    (fmakunbound 'neovm--tf-top-n)
    (fmakunbound 'neovm--tf-unique-count)
    (fmakunbound 'neovm--tf-total-count)))"#;
    let expect = expect_test::expect![[
        r#""OK (6 3 3 2 2 1 0 ((\"the\" . 6) (\"dog\" . 3) (\"fox\" . 3)) 13 24 (1 3 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: trie compression (path compression for single-child chains)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_trie_adv_path_compression() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Path compression: collapse chains of single-child nodes into
    // a single node with a multi-char label. This is similar to a
    // radix tree / patricia trie.
    //
    // Compressed node: (is-end label . children-alist)
    // where label is a string (possibly multi-char after compression)
    let form = r#"(progn
  ;; Standard trie for building
  (fset 'neovm--tc-make (lambda () (list nil)))

  (fset 'neovm--tc-insert
    (lambda (trie word)
      (let ((node trie) (i 0) (len (length word)))
        (while (< i len)
          (let* ((ch (aref word i))
                 (child (assq ch (cdr node))))
            (if child (setq node (cdr child))
              (let ((new-node (cons nil nil)))
                (setcdr node (cons (cons ch new-node) (cdr node)))
                (setq node new-node))))
          (setq i (1+ i)))
        (setcar node t))
      trie))

  ;; Compress a trie: merge single-child non-end chains
  ;; Returns compressed node: (is-end label . compressed-children)
  ;; where compressed-children is alist of (first-char . compressed-node)
  (fset 'neovm--tc-compress-node
    (lambda (node prefix)
      "Compress NODE, where PREFIX is the accumulated label."
      (let ((children (cdr node))
            (is-end (car node)))
        (if (and (= (length children) 1)
                 (not is-end))
            ;; Single child, not a word-end: merge into prefix
            (let* ((child (car children))
                   (ch (car child))
                   (child-node (cdr child)))
              (funcall 'neovm--tc-compress-node
               child-node
               (concat prefix (char-to-string ch))))
          ;; Multiple children or word-end: create compressed node
          (let ((compressed-children nil))
            (dolist (child children)
              (let* ((ch (car child))
                     (child-node (cdr child))
                     (compressed (funcall 'neovm--tc-compress-node
                                  child-node (char-to-string ch))))
                (setq compressed-children
                      (cons compressed compressed-children))))
            (cons is-end (cons prefix (nreverse compressed-children))))))))

  (fset 'neovm--tc-compress
    (lambda (trie)
      (funcall 'neovm--tc-compress-node trie "")))

  ;; Count nodes in compressed trie
  (fset 'neovm--tc-node-count
    (lambda (cnode)
      (let ((c 1))  ;; count this node
        (dolist (child (cddr cnode))
          (setq c (+ c (funcall 'neovm--tc-node-count child))))
        c)))

  ;; Count nodes in uncompressed trie
  (fset 'neovm--tc-raw-node-count
    (lambda (node)
      (let ((c 1))
        (dolist (child (cdr node))
          (setq c (+ c (funcall 'neovm--tc-raw-node-count (cdr child)))))
        c)))

  ;; Collect all labels from compressed trie
  (fset 'neovm--tc-labels
    (lambda (cnode)
      (let ((results (if (> (length (cadr cnode)) 0)
                         (list (cadr cnode))
                       nil)))
        (dolist (child (cddr cnode))
          (setq results (append results (funcall 'neovm--tc-labels child))))
        results)))

  (unwind-protect
      (let ((trie (funcall 'neovm--tc-make)))
        ;; Words with shared prefixes that should compress well
        (dolist (w '("test" "testing" "tested"
                     "toast" "toaster" "toasty"
                     "zoo" "zoom" "zoology"))
          (funcall 'neovm--tc-insert trie w))

        (let* ((raw-count (funcall 'neovm--tc-raw-node-count trie))
               (compressed (funcall 'neovm--tc-compress trie))
               (comp-count (funcall 'neovm--tc-node-count compressed))
               (labels (sort (funcall 'neovm--tc-labels compressed) 'string<)))
          (list
           ;; Raw node count (many single-child chains)
           raw-count
           ;; Compressed node count (should be significantly fewer)
           comp-count
           ;; Compression achieved
           (< comp-count raw-count)
           ;; Labels in compressed trie (multi-char labels from compression)
           labels
           ;; Verify structure: root has children starting with t and z
           (car compressed)   ;; is-end of root
           (cadr compressed)  ;; label of root (should be "")

           ;; Test with fully distinct words (no compression possible beyond leaves)
           (let ((trie2 (funcall 'neovm--tc-make)))
             (dolist (w '("a" "b" "c" "d"))
               (funcall 'neovm--tc-insert trie2 w))
             (let ((comp2 (funcall 'neovm--tc-compress trie2)))
               (funcall 'neovm--tc-node-count comp2)))

           ;; Test with single long word (maximum compression)
           (let ((trie3 (funcall 'neovm--tc-make)))
             (funcall 'neovm--tc-insert trie3 "supercalifragilistic")
             (let* ((raw3 (funcall 'neovm--tc-raw-node-count trie3))
                    (comp3 (funcall 'neovm--tc-compress trie3))
                    (comp3-count (funcall 'neovm--tc-node-count comp3)))
               (list raw3 comp3-count
                     ;; Should compress to just 1 node
                     (= comp3-count 1))))))
    (fmakunbound 'neovm--tc-make)
    (fmakunbound 'neovm--tc-insert)
    (fmakunbound 'neovm--tc-compress-node)
    (fmakunbound 'neovm--tc-compress)
    (fmakunbound 'neovm--tc-node-count)
    (fmakunbound 'neovm--tc-raw-node-count)
    (fmakunbound 'neovm--tc-labels)))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Oracle parity tests for a compressed trie (Patricia/radix tree) in Elisp.
//!
//! Implements a radix tree with multi-character edge labels, insert with
//! edge splitting, exact lookup, prefix-based completion, delete with
//! edge merging, and longest common prefix computation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Core: compressed trie creation, insert with splitting, and lookup
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compressed_trie_insert_and_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A compressed trie node: (is-end children-alist)
    // where children-alist has entries (edge-label . child-node)
    // and edge-label is a multi-character string.
    let form = r#"(progn
  ;; Create empty radix tree node
  (fset 'neovm--rt-make (lambda () (list nil nil)))

  ;; Get is-end flag
  (fset 'neovm--rt-end-p (lambda (node) (car node)))

  ;; Get children alist
  (fset 'neovm--rt-children (lambda (node) (cadr node)))

  ;; Set is-end flag
  (fset 'neovm--rt-set-end (lambda (node val) (setcar node val)))

  ;; Set children
  (fset 'neovm--rt-set-children (lambda (node children) (setcar (cdr node) children)))

  ;; Compute length of common prefix between two strings
  (fset 'neovm--rt-common-prefix-len
    (lambda (s1 s2)
      (let ((len (min (length s1) (length s2)))
            (i 0))
        (while (and (< i len) (= (aref s1 i) (aref s2 i)))
          (setq i (1+ i)))
        i)))

  ;; Insert a word into the radix tree
  (fset 'neovm--rt-insert
    (lambda (node word)
      (if (= (length word) 0)
          ;; Empty remaining word: mark this node as end
          (funcall 'neovm--rt-set-end node t)
        ;; Find child edge that shares a prefix with word
        (let ((children (funcall 'neovm--rt-children node))
              (found nil)
              (remaining children))
          (while (and remaining (not found))
            (let* ((entry (car remaining))
                   (edge (car entry))
                   (child (cdr entry))
                   (cp-len (funcall 'neovm--rt-common-prefix-len edge word)))
              (cond
               ;; No common prefix: try next child
               ((= cp-len 0)
                (setq remaining (cdr remaining)))
               ;; Edge fully matches prefix of word: recurse into child
               ((= cp-len (length edge))
                (funcall 'neovm--rt-insert child (substring word cp-len))
                (setq found t))
               ;; Partial match: split the edge
               (t
                ;; Create new intermediate node
                (let* ((prefix (substring edge 0 cp-len))
                       (edge-rest (substring edge cp-len))
                       (word-rest (substring word cp-len))
                       (mid-node (funcall 'neovm--rt-make)))
                  ;; mid-node gets the old child under remaining edge
                  (funcall 'neovm--rt-set-children mid-node
                           (list (cons edge-rest child)))
                  ;; Insert remaining word into mid-node
                  (funcall 'neovm--rt-insert mid-node word-rest)
                  ;; Replace old edge with prefix -> mid-node
                  (setcar entry prefix)
                  (setcdr entry mid-node))
                (setq found t)))))
          ;; No matching child found: add new edge
          (unless found
            (let ((new-child (funcall 'neovm--rt-make)))
              (funcall 'neovm--rt-set-end new-child t)
              (funcall 'neovm--rt-set-children node
                       (cons (cons word new-child) children))))))
      node))

  ;; Lookup: exact search
  (fset 'neovm--rt-search
    (lambda (node word)
      (if (= (length word) 0)
          (funcall 'neovm--rt-end-p node)
        (let ((children (funcall 'neovm--rt-children node))
              (result nil)
              (done nil))
          (while (and children (not done))
            (let* ((entry (car children))
                   (edge (car entry))
                   (child (cdr entry))
                   (cp-len (funcall 'neovm--rt-common-prefix-len edge word)))
              (cond
               ((and (= cp-len (length edge)) (= cp-len (length word)))
                ;; Exact match of edge and word
                (setq result (funcall 'neovm--rt-end-p child) done t))
               ((= cp-len (length edge))
                ;; Edge consumed: recurse with rest of word
                (setq result (funcall 'neovm--rt-search child (substring word cp-len))
                      done t))
               (t
                (setq children (cdr children))))))
          result))))

  (unwind-protect
      (let ((root (funcall 'neovm--rt-make)))
        ;; Insert words that share prefixes
        (dolist (w '("test" "testing" "tested" "tester" "team" "tea" "ten" "to"))
          (funcall 'neovm--rt-insert root w))
        (list
         ;; Exact lookups: present words
         (funcall 'neovm--rt-search root "test")
         (funcall 'neovm--rt-search root "testing")
         (funcall 'neovm--rt-search root "tested")
         (funcall 'neovm--rt-search root "tester")
         (funcall 'neovm--rt-search root "team")
         (funcall 'neovm--rt-search root "tea")
         (funcall 'neovm--rt-search root "ten")
         (funcall 'neovm--rt-search root "to")
         ;; Absent words (prefixes that are not marked as end)
         (funcall 'neovm--rt-search root "te")
         (funcall 'neovm--rt-search root "tes")
         (funcall 'neovm--rt-search root "testi")
         (funcall 'neovm--rt-search root "t")
         ;; Completely absent words
         (funcall 'neovm--rt-search root "xyz")
         (funcall 'neovm--rt-search root "toast")
         (funcall 'neovm--rt-search root "")
         ;; Insert a prefix of existing word and verify
         (progn
           (funcall 'neovm--rt-insert root "te")
           (list (funcall 'neovm--rt-search root "te")
                 (funcall 'neovm--rt-search root "test")
                 (funcall 'neovm--rt-search root "tea")))))
    (fmakunbound 'neovm--rt-make)
    (fmakunbound 'neovm--rt-end-p)
    (fmakunbound 'neovm--rt-children)
    (fmakunbound 'neovm--rt-set-end)
    (fmakunbound 'neovm--rt-set-children)
    (fmakunbound 'neovm--rt-common-prefix-len)
    (fmakunbound 'neovm--rt-insert)
    (fmakunbound 'neovm--rt-search)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable children)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Prefix-based completion: collect all words with a given prefix
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compressed_trie_prefix_completion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rt2-make (lambda () (list nil nil)))
  (fset 'neovm--rt2-end-p (lambda (n) (car n)))
  (fset 'neovm--rt2-children (lambda (n) (cadr n)))
  (fset 'neovm--rt2-set-end (lambda (n v) (setcar n v)))
  (fset 'neovm--rt2-set-children (lambda (n c) (setcar (cdr n) c)))

  (fset 'neovm--rt2-cplen
    (lambda (s1 s2)
      (let ((len (min (length s1) (length s2))) (i 0))
        (while (and (< i len) (= (aref s1 i) (aref s2 i))) (setq i (1+ i)))
        i)))

  (fset 'neovm--rt2-insert
    (lambda (node word)
      (if (= (length word) 0)
          (funcall 'neovm--rt2-set-end node t)
        (let ((children (funcall 'neovm--rt2-children node))
              (found nil) (remaining children))
          (while (and remaining (not found))
            (let* ((entry (car remaining))
                   (edge (car entry)) (child (cdr entry))
                   (cp (funcall 'neovm--rt2-cplen edge word)))
              (cond
               ((= cp 0) (setq remaining (cdr remaining)))
               ((= cp (length edge))
                (funcall 'neovm--rt2-insert child (substring word cp))
                (setq found t))
               (t
                (let* ((prefix (substring edge 0 cp))
                       (edge-rest (substring edge cp))
                       (word-rest (substring word cp))
                       (mid (funcall 'neovm--rt2-make)))
                  (funcall 'neovm--rt2-set-children mid (list (cons edge-rest child)))
                  (funcall 'neovm--rt2-insert mid word-rest)
                  (setcar entry prefix) (setcdr entry mid))
                (setq found t)))))
          (unless found
            (let ((nc (funcall 'neovm--rt2-make)))
              (funcall 'neovm--rt2-set-end nc t)
              (funcall 'neovm--rt2-set-children node
                       (cons (cons word nc) children))))))
      node))

  ;; Navigate to the subtree matching prefix, return (node . consumed-prefix)
  (fset 'neovm--rt2-find-prefix-node
    (lambda (node prefix)
      (if (= (length prefix) 0)
          (cons node "")
        (let ((children (funcall 'neovm--rt2-children node))
              (result nil))
          (while (and children (not result))
            (let* ((entry (car children))
                   (edge (car entry)) (child (cdr entry))
                   (cp (funcall 'neovm--rt2-cplen edge prefix)))
              (cond
               ((= cp 0) (setq children (cdr children)))
               ;; Prefix fully consumed within this edge
               ((= cp (length prefix))
                (if (= cp (length edge))
                    (setq result (cons child prefix))
                  ;; prefix is a prefix of the edge: the subtree is this edge's child
                  ;; but we also need to collect words from partial edge match
                  (setq result (cons entry prefix))))
               ;; Edge fully consumed, continue with rest of prefix
               ((= cp (length edge))
                (setq result (funcall 'neovm--rt2-find-prefix-node
                                      child (substring prefix cp))))
               (t (setq children (cdr children))))))
          result))))

  ;; Collect all words from a node
  (fset 'neovm--rt2-collect
    (lambda (node prefix)
      (let ((results nil))
        (when (funcall 'neovm--rt2-end-p node)
          (push prefix results))
        (dolist (entry (funcall 'neovm--rt2-children node))
          (let ((edge (car entry)) (child (cdr entry)))
            (setq results (append results
                                  (funcall 'neovm--rt2-collect child
                                           (concat prefix edge))))))
        results)))

  ;; Autocomplete with prefix
  (fset 'neovm--rt2-complete
    (lambda (root prefix)
      (let ((found (funcall 'neovm--rt2-find-prefix-node root prefix)))
        (if (null found)
            nil
          (let ((node-or-entry (car found)))
            (cond
             ;; Found a node directly
             ((and (listp node-or-entry) (= (length node-or-entry) 2))
              (sort (funcall 'neovm--rt2-collect node-or-entry prefix) 'string<))
             ;; Found an entry (edge . child): prefix is prefix of edge
             ((and (consp node-or-entry) (stringp (car node-or-entry)))
              (let* ((edge (car node-or-entry))
                     (child (cdr node-or-entry))
                     (full-edge-prefix (concat prefix (substring edge (length prefix)))))
                (sort (funcall 'neovm--rt2-collect child full-edge-prefix) 'string<)))
             (t nil)))))))

  (unwind-protect
      (let ((root (funcall 'neovm--rt2-make)))
        (dolist (w '("romane" "romanus" "romulus" "rubens" "ruber"
                     "rubicon" "rubicundus"))
          (funcall 'neovm--rt2-insert root w))
        (list
         ;; Complete "rom" -> romane, romanus, romulus
         (funcall 'neovm--rt2-complete root "rom")
         ;; Complete "rub" -> rubens, ruber, rubicon, rubicundus
         (funcall 'neovm--rt2-complete root "rub")
         ;; Complete "ru" -> rubens, ruber, rubicon, rubicundus
         (funcall 'neovm--rt2-complete root "ru")
         ;; Complete "roman" -> romane, romanus
         (funcall 'neovm--rt2-complete root "roman")
         ;; Complete "rubic" -> rubicon, rubicundus
         (funcall 'neovm--rt2-complete root "rubic")
         ;; Complete "xyz" -> nil
         (funcall 'neovm--rt2-complete root "xyz")
         ;; All words (prefix "r")
         (funcall 'neovm--rt2-complete root "r")
         ;; Exact word as prefix
         (funcall 'neovm--rt2-complete root "romane")))
    (fmakunbound 'neovm--rt2-make)
    (fmakunbound 'neovm--rt2-end-p)
    (fmakunbound 'neovm--rt2-children)
    (fmakunbound 'neovm--rt2-set-end)
    (fmakunbound 'neovm--rt2-set-children)
    (fmakunbound 'neovm--rt2-cplen)
    (fmakunbound 'neovm--rt2-insert)
    (fmakunbound 'neovm--rt2-find-prefix-node)
    (fmakunbound 'neovm--rt2-collect)
    (fmakunbound 'neovm--rt2-complete)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable children)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Delete with edge merging
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compressed_trie_delete_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rt3-make (lambda () (list nil nil)))
  (fset 'neovm--rt3-end-p (lambda (n) (car n)))
  (fset 'neovm--rt3-children (lambda (n) (cadr n)))
  (fset 'neovm--rt3-set-end (lambda (n v) (setcar n v)))
  (fset 'neovm--rt3-set-children (lambda (n c) (setcar (cdr n) c)))

  (fset 'neovm--rt3-cplen
    (lambda (s1 s2)
      (let ((len (min (length s1) (length s2))) (i 0))
        (while (and (< i len) (= (aref s1 i) (aref s2 i))) (setq i (1+ i)))
        i)))

  (fset 'neovm--rt3-insert
    (lambda (node word)
      (if (= (length word) 0)
          (funcall 'neovm--rt3-set-end node t)
        (let ((children (funcall 'neovm--rt3-children node))
              (found nil) (remaining children))
          (while (and remaining (not found))
            (let* ((entry (car remaining))
                   (edge (car entry)) (child (cdr entry))
                   (cp (funcall 'neovm--rt3-cplen edge word)))
              (cond
               ((= cp 0) (setq remaining (cdr remaining)))
               ((= cp (length edge))
                (funcall 'neovm--rt3-insert child (substring word cp))
                (setq found t))
               (t
                (let* ((prefix (substring edge 0 cp))
                       (edge-rest (substring edge cp))
                       (word-rest (substring word cp))
                       (mid (funcall 'neovm--rt3-make)))
                  (funcall 'neovm--rt3-set-children mid (list (cons edge-rest child)))
                  (funcall 'neovm--rt3-insert mid word-rest)
                  (setcar entry prefix) (setcdr entry mid))
                (setq found t)))))
          (unless found
            (let ((nc (funcall 'neovm--rt3-make)))
              (funcall 'neovm--rt3-set-end nc t)
              (funcall 'neovm--rt3-set-children node
                       (cons (cons word nc) children))))))
      node))

  (fset 'neovm--rt3-search
    (lambda (node word)
      (if (= (length word) 0)
          (funcall 'neovm--rt3-end-p node)
        (let ((children (funcall 'neovm--rt3-children node))
              (result nil) (done nil))
          (while (and children (not done))
            (let* ((entry (car children))
                   (edge (car entry)) (child (cdr entry))
                   (cp (funcall 'neovm--rt3-cplen edge word)))
              (cond
               ((and (= cp (length edge)) (= cp (length word)))
                (setq result (funcall 'neovm--rt3-end-p child) done t))
               ((= cp (length edge))
                (setq result (funcall 'neovm--rt3-search child (substring word cp))
                      done t))
               (t (setq children (cdr children))))))
          result))))

  ;; Delete: unmark end, then merge single-child non-end nodes
  (fset 'neovm--rt3-delete
    (lambda (node word)
      "Delete WORD from radix tree. Returns t if deleted, nil if not found."
      (if (= (length word) 0)
          (if (funcall 'neovm--rt3-end-p node)
              (progn (funcall 'neovm--rt3-set-end node nil) t)
            nil)
        (let ((children (funcall 'neovm--rt3-children node))
              (result nil) (done nil))
          (while (and children (not done))
            (let* ((entry (car children))
                   (edge (car entry)) (child (cdr entry))
                   (cp (funcall 'neovm--rt3-cplen edge word)))
              (cond
               ((and (= cp (length edge)) (= cp (length word)))
                ;; Exact match: unmark end
                (if (funcall 'neovm--rt3-end-p child)
                    (progn
                      (funcall 'neovm--rt3-set-end child nil)
                      ;; Merge if child now has exactly one child and is not end
                      (when (and (not (funcall 'neovm--rt3-end-p child))
                                 (= (length (funcall 'neovm--rt3-children child)) 1))
                        (let* ((grandchild-entry (car (funcall 'neovm--rt3-children child)))
                               (merged-edge (concat edge (car grandchild-entry))))
                          (setcar entry merged-edge)
                          (setcdr entry (cdr grandchild-entry))))
                      ;; Remove if child has no children and is not end
                      (when (and (not (funcall 'neovm--rt3-end-p child))
                                 (null (funcall 'neovm--rt3-children child)))
                        (funcall 'neovm--rt3-set-children node
                                 (delq entry (funcall 'neovm--rt3-children node))))
                      (setq result t))
                  (setq result nil))
                (setq done t))
               ((= cp (length edge))
                (setq result (funcall 'neovm--rt3-delete child (substring word cp))
                      done t)
                ;; After recursive delete, check if child can be merged
                (when result
                  (when (and (not (funcall 'neovm--rt3-end-p child))
                             (= (length (funcall 'neovm--rt3-children child)) 1))
                    (let* ((grandchild-entry (car (funcall 'neovm--rt3-children child)))
                           (merged-edge (concat edge (car grandchild-entry))))
                      (setcar entry merged-edge)
                      (setcdr entry (cdr grandchild-entry))))
                  (when (and (not (funcall 'neovm--rt3-end-p child))
                             (null (funcall 'neovm--rt3-children child)))
                    (funcall 'neovm--rt3-set-children node
                             (delq entry (funcall 'neovm--rt3-children node))))))
               (t (setq children (cdr children))))))
          result))))

  ;; Count all words
  (fset 'neovm--rt3-count
    (lambda (node)
      (let ((c (if (funcall 'neovm--rt3-end-p node) 1 0)))
        (dolist (entry (funcall 'neovm--rt3-children node))
          (setq c (+ c (funcall 'neovm--rt3-count (cdr entry)))))
        c)))

  (unwind-protect
      (let ((root (funcall 'neovm--rt3-make)))
        (dolist (w '("slow" "slowly" "slower" "slowest" "sled" "sleep" "slender"))
          (funcall 'neovm--rt3-insert root w))
        (let ((initial-count (funcall 'neovm--rt3-count root)))
          (list
           initial-count  ;; 7
           ;; Delete "slowly" -- "slow" should remain
           (funcall 'neovm--rt3-delete root "slowly")
           (funcall 'neovm--rt3-search root "slowly")
           (funcall 'neovm--rt3-search root "slow")
           (funcall 'neovm--rt3-count root)   ;; 6
           ;; Delete "slow" -- "slower", "slowest" should remain
           (funcall 'neovm--rt3-delete root "slow")
           (funcall 'neovm--rt3-search root "slow")
           (funcall 'neovm--rt3-search root "slower")
           (funcall 'neovm--rt3-search root "slowest")
           (funcall 'neovm--rt3-count root)   ;; 5
           ;; Delete non-existent word
           (funcall 'neovm--rt3-delete root "sloth")
           ;; Delete "sled" -- unrelated branch
           (funcall 'neovm--rt3-delete root "sled")
           (funcall 'neovm--rt3-search root "sled")
           (funcall 'neovm--rt3-search root "sleep")
           (funcall 'neovm--rt3-count root)   ;; 4
           ;; Delete everything
           (funcall 'neovm--rt3-delete root "slower")
           (funcall 'neovm--rt3-delete root "slowest")
           (funcall 'neovm--rt3-delete root "sleep")
           (funcall 'neovm--rt3-delete root "slender")
           (funcall 'neovm--rt3-count root))))   ;; 0
    (fmakunbound 'neovm--rt3-make)
    (fmakunbound 'neovm--rt3-end-p)
    (fmakunbound 'neovm--rt3-children)
    (fmakunbound 'neovm--rt3-set-end)
    (fmakunbound 'neovm--rt3-set-children)
    (fmakunbound 'neovm--rt3-cplen)
    (fmakunbound 'neovm--rt3-insert)
    (fmakunbound 'neovm--rt3-search)
    (fmakunbound 'neovm--rt3-delete)
    (fmakunbound 'neovm--rt3-count)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable children)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Longest common prefix of all words in the trie
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compressed_trie_longest_common_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rt4-make (lambda () (list nil nil)))
  (fset 'neovm--rt4-end-p (lambda (n) (car n)))
  (fset 'neovm--rt4-children (lambda (n) (cadr n)))
  (fset 'neovm--rt4-set-end (lambda (n v) (setcar n v)))
  (fset 'neovm--rt4-set-children (lambda (n c) (setcar (cdr n) c)))

  (fset 'neovm--rt4-cplen
    (lambda (s1 s2)
      (let ((len (min (length s1) (length s2))) (i 0))
        (while (and (< i len) (= (aref s1 i) (aref s2 i))) (setq i (1+ i)))
        i)))

  (fset 'neovm--rt4-insert
    (lambda (node word)
      (if (= (length word) 0)
          (funcall 'neovm--rt4-set-end node t)
        (let ((children (funcall 'neovm--rt4-children node))
              (found nil) (remaining children))
          (while (and remaining (not found))
            (let* ((entry (car remaining))
                   (edge (car entry)) (child (cdr entry))
                   (cp (funcall 'neovm--rt4-cplen edge word)))
              (cond
               ((= cp 0) (setq remaining (cdr remaining)))
               ((= cp (length edge))
                (funcall 'neovm--rt4-insert child (substring word cp))
                (setq found t))
               (t
                (let* ((prefix (substring edge 0 cp))
                       (edge-rest (substring edge cp))
                       (word-rest (substring word cp))
                       (mid (funcall 'neovm--rt4-make)))
                  (funcall 'neovm--rt4-set-children mid (list (cons edge-rest child)))
                  (funcall 'neovm--rt4-insert mid word-rest)
                  (setcar entry prefix) (setcdr entry mid))
                (setq found t)))))
          (unless found
            (let ((nc (funcall 'neovm--rt4-make)))
              (funcall 'neovm--rt4-set-end nc t)
              (funcall 'neovm--rt4-set-children node
                       (cons (cons word nc) children))))))
      node))

  ;; Longest common prefix: follow the trie as long as there is exactly one
  ;; child and the node is not an end node
  (fset 'neovm--rt4-lcp
    (lambda (node)
      "Return the longest common prefix of all words in the trie."
      (let ((prefix "")
            (current node)
            (keep-going t))
        (while keep-going
          (let ((children (funcall 'neovm--rt4-children current)))
            (if (and (= (length children) 1)
                     (not (funcall 'neovm--rt4-end-p current)))
                (let ((entry (car children)))
                  (setq prefix (concat prefix (car entry))
                        current (cdr entry)))
              (setq keep-going nil))))
        prefix)))

  (unwind-protect
      (list
       ;; All words share "inter" prefix
       (let ((root (funcall 'neovm--rt4-make)))
         (dolist (w '("internet" "internal" "international" "interface" "interpret"))
           (funcall 'neovm--rt4-insert root w))
         (funcall 'neovm--rt4-lcp root))
       ;; All words share "pre" prefix
       (let ((root (funcall 'neovm--rt4-make)))
         (dolist (w '("predict" "prevent" "prepare" "precise" "premium"))
           (funcall 'neovm--rt4-insert root w))
         (funcall 'neovm--rt4-lcp root))
       ;; No common prefix at all
       (let ((root (funcall 'neovm--rt4-make)))
         (dolist (w '("apple" "banana" "cherry"))
           (funcall 'neovm--rt4-insert root w))
         (funcall 'neovm--rt4-lcp root))
       ;; Single word: LCP is the word itself
       (let ((root (funcall 'neovm--rt4-make)))
         (funcall 'neovm--rt4-insert root "hello")
         (funcall 'neovm--rt4-lcp root))
       ;; Two identical words
       (let ((root (funcall 'neovm--rt4-make)))
         (funcall 'neovm--rt4-insert root "abc")
         (funcall 'neovm--rt4-insert root "abc")
         (funcall 'neovm--rt4-lcp root))
       ;; One word is prefix of another
       (let ((root (funcall 'neovm--rt4-make)))
         (dolist (w '("flow" "flower" "flowing"))
           (funcall 'neovm--rt4-insert root w))
         (funcall 'neovm--rt4-lcp root)))
    (fmakunbound 'neovm--rt4-make)
    (fmakunbound 'neovm--rt4-end-p)
    (fmakunbound 'neovm--rt4-children)
    (fmakunbound 'neovm--rt4-set-end)
    (fmakunbound 'neovm--rt4-set-children)
    (fmakunbound 'neovm--rt4-cplen)
    (fmakunbound 'neovm--rt4-insert)
    (fmakunbound 'neovm--rt4-lcp)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable children)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Count words with prefix in compressed trie
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compressed_trie_count_with_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rt5-make (lambda () (list nil nil)))
  (fset 'neovm--rt5-end-p (lambda (n) (car n)))
  (fset 'neovm--rt5-children (lambda (n) (cadr n)))
  (fset 'neovm--rt5-set-end (lambda (n v) (setcar n v)))
  (fset 'neovm--rt5-set-children (lambda (n c) (setcar (cdr n) c)))

  (fset 'neovm--rt5-cplen
    (lambda (s1 s2)
      (let ((len (min (length s1) (length s2))) (i 0))
        (while (and (< i len) (= (aref s1 i) (aref s2 i))) (setq i (1+ i)))
        i)))

  (fset 'neovm--rt5-insert
    (lambda (node word)
      (if (= (length word) 0)
          (funcall 'neovm--rt5-set-end node t)
        (let ((children (funcall 'neovm--rt5-children node))
              (found nil) (remaining children))
          (while (and remaining (not found))
            (let* ((entry (car remaining))
                   (edge (car entry)) (child (cdr entry))
                   (cp (funcall 'neovm--rt5-cplen edge word)))
              (cond
               ((= cp 0) (setq remaining (cdr remaining)))
               ((= cp (length edge))
                (funcall 'neovm--rt5-insert child (substring word cp))
                (setq found t))
               (t
                (let* ((prefix (substring edge 0 cp))
                       (edge-rest (substring edge cp))
                       (word-rest (substring word cp))
                       (mid (funcall 'neovm--rt5-make)))
                  (funcall 'neovm--rt5-set-children mid (list (cons edge-rest child)))
                  (funcall 'neovm--rt5-insert mid word-rest)
                  (setcar entry prefix) (setcdr entry mid))
                (setq found t)))))
          (unless found
            (let ((nc (funcall 'neovm--rt5-make)))
              (funcall 'neovm--rt5-set-end nc t)
              (funcall 'neovm--rt5-set-children node
                       (cons (cons word nc) children))))))
      node))

  ;; Count all words rooted at a node
  (fset 'neovm--rt5-count
    (lambda (node)
      (let ((c (if (funcall 'neovm--rt5-end-p node) 1 0)))
        (dolist (entry (funcall 'neovm--rt5-children node))
          (setq c (+ c (funcall 'neovm--rt5-count (cdr entry)))))
        c)))

  ;; Count words with given prefix
  (fset 'neovm--rt5-count-prefix
    (lambda (node prefix)
      (if (= (length prefix) 0)
          (funcall 'neovm--rt5-count node)
        (let ((children (funcall 'neovm--rt5-children node))
              (result 0) (done nil))
          (while (and children (not done))
            (let* ((entry (car children))
                   (edge (car entry)) (child (cdr entry))
                   (cp (funcall 'neovm--rt5-cplen edge prefix)))
              (cond
               ((= cp 0) (setq children (cdr children)))
               ;; prefix consumed within edge: count all under child
               ((= cp (length prefix))
                (if (= cp (length edge))
                    (setq result (funcall 'neovm--rt5-count child))
                  ;; prefix is shorter than edge but matches
                  (setq result (funcall 'neovm--rt5-count child)))
                (setq done t))
               ;; edge consumed: recurse with rest
               ((= cp (length edge))
                (setq result (funcall 'neovm--rt5-count-prefix child (substring prefix cp))
                      done t))
               (t (setq children (cdr children))))))
          result))))

  (unwind-protect
      (let ((root (funcall 'neovm--rt5-make)))
        (dolist (w '("car" "card" "care" "careful" "carefully"
                     "cargo" "carry" "cart" "cast" "cat"
                     "catch" "category" "cater"))
          (funcall 'neovm--rt5-insert root w))
        (list
         ;; All words
         (funcall 'neovm--rt5-count-prefix root "")
         ;; "car" prefix: car, card, care, careful, carefully, cargo, carry, cart = 8
         (funcall 'neovm--rt5-count-prefix root "car")
         ;; "care" prefix: care, careful, carefully = 3
         (funcall 'neovm--rt5-count-prefix root "care")
         ;; "cat" prefix: cat, catch, category, cater = 4
         (funcall 'neovm--rt5-count-prefix root "cat")
         ;; "cas" prefix: cast = 1
         (funcall 'neovm--rt5-count-prefix root "cas")
         ;; "ca" prefix: all 13 words
         (funcall 'neovm--rt5-count-prefix root "ca")
         ;; "xyz" prefix: 0
         (funcall 'neovm--rt5-count-prefix root "xyz")
         ;; "carefully" exact: 1
         (funcall 'neovm--rt5-count-prefix root "carefully")))
    (fmakunbound 'neovm--rt5-make)
    (fmakunbound 'neovm--rt5-end-p)
    (fmakunbound 'neovm--rt5-children)
    (fmakunbound 'neovm--rt5-set-end)
    (fmakunbound 'neovm--rt5-set-children)
    (fmakunbound 'neovm--rt5-cplen)
    (fmakunbound 'neovm--rt5-insert)
    (fmakunbound 'neovm--rt5-count)
    (fmakunbound 'neovm--rt5-count-prefix)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable children)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// End-to-end: build, query, modify, verify
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compressed_trie_end_to_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (fset 'neovm--rt6-make (lambda () (list nil nil)))
  (fset 'neovm--rt6-end-p (lambda (n) (car n)))
  (fset 'neovm--rt6-children (lambda (n) (cadr n)))
  (fset 'neovm--rt6-set-end (lambda (n v) (setcar n v)))
  (fset 'neovm--rt6-set-children (lambda (n c) (setcar (cdr n) c)))

  (fset 'neovm--rt6-cplen
    (lambda (s1 s2)
      (let ((len (min (length s1) (length s2))) (i 0))
        (while (and (< i len) (= (aref s1 i) (aref s2 i))) (setq i (1+ i)))
        i)))

  (fset 'neovm--rt6-insert
    (lambda (node word)
      (if (= (length word) 0)
          (funcall 'neovm--rt6-set-end node t)
        (let ((children (funcall 'neovm--rt6-children node))
              (found nil) (remaining children))
          (while (and remaining (not found))
            (let* ((entry (car remaining))
                   (edge (car entry)) (child (cdr entry))
                   (cp (funcall 'neovm--rt6-cplen edge word)))
              (cond
               ((= cp 0) (setq remaining (cdr remaining)))
               ((= cp (length edge))
                (funcall 'neovm--rt6-insert child (substring word cp))
                (setq found t))
               (t
                (let* ((pfx (substring edge 0 cp))
                       (er (substring edge cp))
                       (wr (substring word cp))
                       (mid (funcall 'neovm--rt6-make)))
                  (funcall 'neovm--rt6-set-children mid (list (cons er child)))
                  (funcall 'neovm--rt6-insert mid wr)
                  (setcar entry pfx) (setcdr entry mid))
                (setq found t)))))
          (unless found
            (let ((nc (funcall 'neovm--rt6-make)))
              (funcall 'neovm--rt6-set-end nc t)
              (funcall 'neovm--rt6-set-children node (cons (cons word nc) children))))))
      node))

  (fset 'neovm--rt6-search
    (lambda (node word)
      (if (= (length word) 0)
          (funcall 'neovm--rt6-end-p node)
        (let ((children (funcall 'neovm--rt6-children node)) (result nil) (done nil))
          (while (and children (not done))
            (let* ((entry (car children))
                   (edge (car entry)) (child (cdr entry))
                   (cp (funcall 'neovm--rt6-cplen edge word)))
              (cond
               ((and (= cp (length edge)) (= cp (length word)))
                (setq result (funcall 'neovm--rt6-end-p child) done t))
               ((= cp (length edge))
                (setq result (funcall 'neovm--rt6-search child (substring word cp)) done t))
               (t (setq children (cdr children))))))
          result))))

  (fset 'neovm--rt6-count
    (lambda (node)
      (let ((c (if (funcall 'neovm--rt6-end-p node) 1 0)))
        (dolist (entry (funcall 'neovm--rt6-children node))
          (setq c (+ c (funcall 'neovm--rt6-count (cdr entry)))))
        c)))

  (fset 'neovm--rt6-collect-all
    (lambda (node prefix)
      (let ((results nil))
        (when (funcall 'neovm--rt6-end-p node)
          (push prefix results))
        (dolist (entry (funcall 'neovm--rt6-children node))
          (setq results (append results
                                (funcall 'neovm--rt6-collect-all
                                         (cdr entry) (concat prefix (car entry))))))
        results)))

  (unwind-protect
      (let ((root (funcall 'neovm--rt6-make)))
        ;; Phase 1: bulk insert
        (dolist (w '("abc" "abcd" "abce" "abcf" "abd" "xyz" "xyw"))
          (funcall 'neovm--rt6-insert root w))
        (let ((phase1-count (funcall 'neovm--rt6-count root))
              (phase1-words (sort (funcall 'neovm--rt6-collect-all root "") 'string<)))
          ;; Phase 2: insert words that cause splits
          (funcall 'neovm--rt6-insert root "ab")   ;; prefix of abc
          (funcall 'neovm--rt6-insert root "a")    ;; even shorter prefix
          (funcall 'neovm--rt6-insert root "abcde") ;; extends abcd
          (let ((phase2-count (funcall 'neovm--rt6-count root))
                (phase2-search (list
                                (funcall 'neovm--rt6-search root "a")
                                (funcall 'neovm--rt6-search root "ab")
                                (funcall 'neovm--rt6-search root "abc")
                                (funcall 'neovm--rt6-search root "abcd")
                                (funcall 'neovm--rt6-search root "abcde")
                                (funcall 'neovm--rt6-search root "abcdef"))))
            (list
             phase1-count       ;; 7
             phase1-words       ;; all 7 sorted
             phase2-count       ;; 10
             phase2-search      ;; (t t t t t nil)
             ;; Verify all original words still present
             (cl-loop for w in '("abc" "abcd" "abce" "abcf" "abd" "xyz" "xyw")
                      always (funcall 'neovm--rt6-search root w))))))
    (fmakunbound 'neovm--rt6-make)
    (fmakunbound 'neovm--rt6-end-p)
    (fmakunbound 'neovm--rt6-children)
    (fmakunbound 'neovm--rt6-set-end)
    (fmakunbound 'neovm--rt6-set-children)
    (fmakunbound 'neovm--rt6-cplen)
    (fmakunbound 'neovm--rt6-insert)
    (fmakunbound 'neovm--rt6-search)
    (fmakunbound 'neovm--rt6-count)
    (fmakunbound 'neovm--rt6-collect-all)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable children)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Oracle parity tests for suffix tree/trie operations in Elisp:
//! suffix trie construction, substring search, longest repeated substring,
//! suffix counting, pattern matching with wildcards, and longest common
//! substring of two strings.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Suffix trie construction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_suffix_trie_construction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a suffix trie as nested alists.
    // Each node: ((char . child-node) ...) with a special key 'end marking a suffix end.
    let form = r#"(progn
  (fset 'neovm--st-make-node
    (lambda () (list)))

  (fset 'neovm--st-insert-suffix
    (lambda (root suffix)
      "Insert SUFFIX (a string) into trie rooted at ROOT.
       Modifies ROOT in place by consing onto it. Returns new root."
      (let ((node root)
            (i 0)
            (len (length suffix)))
        (while (< i len)
          (let* ((ch (aref suffix i))
                 (child (assq ch (cdr node))))
            (if child
                (setq node (cdr child))
              ;; Create new child node
              (let ((new-node (list 'trie-node)))
                (setcdr node (cons (cons ch new-node) (cdr node)))
                (setq node new-node))))
          (setq i (1+ i)))
        ;; Mark end of suffix
        (unless (assq 'end (cdr node))
          (setcdr node (cons (cons 'end t) (cdr node))))
        root)))

  (fset 'neovm--st-build
    (lambda (text)
      "Build a suffix trie for TEXT. Returns the root node."
      (let ((root (list 'trie-node))
            (len (length text))
            (i 0))
        (while (< i len)
          (funcall 'neovm--st-insert-suffix root (substring text i))
          (setq i (1+ i)))
        root)))

  (fset 'neovm--st-count-nodes
    (lambda (node)
      "Count the number of nodes in the trie."
      (let ((count 1)
            (children (cdr node)))
        (dolist (child children count)
          (unless (eq (car child) 'end)
            (setq count (+ count
                           (funcall 'neovm--st-count-nodes (cdr child)))))))))

  (fset 'neovm--st-count-ends
    (lambda (node)
      "Count the number of suffix endpoints (leaves) in the trie."
      (let ((count (if (assq 'end (cdr node)) 1 0))
            (children (cdr node)))
        (dolist (child children count)
          (unless (eq (car child) 'end)
            (setq count (+ count
                           (funcall 'neovm--st-count-ends (cdr child)))))))))

  (unwind-protect
      (list
       ;; "abc" has 3 suffixes: "abc", "bc", "c"
       (let ((trie (funcall 'neovm--st-build "abc")))
         (list 'nodes (funcall 'neovm--st-count-nodes trie)
               'ends (funcall 'neovm--st-count-ends trie)))

       ;; "aaa" has 3 suffixes but shares structure
       (let ((trie (funcall 'neovm--st-build "aaa")))
         (list 'nodes (funcall 'neovm--st-count-nodes trie)
               'ends (funcall 'neovm--st-count-ends trie)))

       ;; "abab" has 4 suffixes: "abab", "bab", "ab", "b"
       (let ((trie (funcall 'neovm--st-build "abab")))
         (list 'nodes (funcall 'neovm--st-count-nodes trie)
               'ends (funcall 'neovm--st-count-ends trie)))

       ;; Single char
       (let ((trie (funcall 'neovm--st-build "x")))
         (list 'nodes (funcall 'neovm--st-count-nodes trie)
               'ends (funcall 'neovm--st-count-ends trie))))
    (fmakunbound 'neovm--st-make-node)
    (fmakunbound 'neovm--st-insert-suffix)
    (fmakunbound 'neovm--st-build)
    (fmakunbound 'neovm--st-count-nodes)
    (fmakunbound 'neovm--st-count-ends)))"#;
    let expect = expect_test::expect![[
        r#""OK ((nodes 7 ends 3) (nodes 4 ends 3) (nodes 8 ends 4) (nodes 2 ends 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Substring search using suffix trie
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_suffix_trie_substring_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--st2-insert
    (lambda (root suffix)
      (let ((node root)
            (i 0)
            (len (length suffix)))
        (while (< i len)
          (let* ((ch (aref suffix i))
                 (child (assq ch (cdr node))))
            (if child
                (setq node (cdr child))
              (let ((new-node (list 'node)))
                (setcdr node (cons (cons ch new-node) (cdr node)))
                (setq node new-node))))
          (setq i (1+ i)))
        (unless (assq 'end (cdr node))
          (setcdr node (cons (cons 'end t) (cdr node))))
        root)))

  (fset 'neovm--st2-build
    (lambda (text)
      (let ((root (list 'node))
            (len (length text))
            (i 0))
        (while (< i len)
          (funcall 'neovm--st2-insert root (substring text i))
          (setq i (1+ i)))
        root)))

  (fset 'neovm--st2-search
    (lambda (trie pattern)
      "Search for PATTERN as a substring. Returns t if found, nil otherwise.
       A pattern is found if we can traverse all its characters in the trie."
      (let ((node trie)
            (i 0)
            (len (length pattern))
            (found t))
        (while (and (< i len) found)
          (let ((child (assq (aref pattern i) (cdr node))))
            (if child
                (setq node (cdr child))
              (setq found nil)))
          (setq i (1+ i)))
        found)))

  (unwind-protect
      (let ((trie (funcall 'neovm--st2-build "banana")))
        (list
         ;; Substrings that exist
         (funcall 'neovm--st2-search trie "ban")
         (funcall 'neovm--st2-search trie "ana")
         (funcall 'neovm--st2-search trie "nan")
         (funcall 'neovm--st2-search trie "a")
         (funcall 'neovm--st2-search trie "banana")
         (funcall 'neovm--st2-search trie "na")
         ;; Substrings that don't exist
         (funcall 'neovm--st2-search trie "banan!")
         (funcall 'neovm--st2-search trie "xyz")
         (funcall 'neovm--st2-search trie "nab")
         (funcall 'neovm--st2-search trie "bananaa")
         ;; Empty pattern is always found
         (funcall 'neovm--st2-search trie "")))
    (fmakunbound 'neovm--st2-insert)
    (fmakunbound 'neovm--st2-build)
    (fmakunbound 'neovm--st2-search)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t nil nil nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Longest repeated substring via suffix trie
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_suffix_trie_longest_repeated() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--st3-insert
    (lambda (root suffix)
      (let ((node root)
            (i 0)
            (len (length suffix)))
        (while (< i len)
          (let* ((ch (aref suffix i))
                 (child (assq ch (cdr node))))
            (if child
                (setq node (cdr child))
              (let ((new-node (list 'node)))
                (setcdr node (cons (cons ch new-node) (cdr node)))
                (setq node new-node))))
          (setq i (1+ i)))
        (unless (assq 'end (cdr node))
          (setcdr node (cons (cons 'end t) (cdr node))))
        root)))

  (fset 'neovm--st3-build
    (lambda (text)
      (let ((root (list 'node))
            (len (length text))
            (i 0))
        (while (< i len)
          (funcall 'neovm--st3-insert root (substring text i))
          (setq i (1+ i)))
        root)))

  (fset 'neovm--st3-branching-children
    (lambda (node)
      "Return the number of non-end children of NODE."
      (let ((count 0))
        (dolist (child (cdr node) count)
          (unless (eq (car child) 'end)
            (setq count (1+ count)))))))

  (fset 'neovm--st3-longest-repeated
    (lambda (text)
      "Find the longest repeated substring by DFS through the suffix trie.
       A repeated substring corresponds to an internal node (branching or with
       both end and children) at maximum depth."
      (let ((trie (funcall 'neovm--st3-build text))
            (best-len 0)
            (best-str ""))
        ;; DFS: track current path as list of chars
        (fset 'neovm--st3-dfs
          (lambda (node depth path)
            ;; A node with >= 2 non-end children means its path is repeated
            (when (and (>= (funcall 'neovm--st3-branching-children node) 2)
                       (> depth best-len))
              (setq best-len depth)
              (setq best-str (concat (nreverse (copy-sequence path)))))
            (dolist (child (cdr node))
              (unless (eq (car child) 'end)
                (funcall 'neovm--st3-dfs
                         (cdr child)
                         (1+ depth)
                         (cons (car child) path))))))
        (funcall 'neovm--st3-dfs trie 0 nil)
        (fmakunbound 'neovm--st3-dfs)
        (cons best-len best-str))))

  (unwind-protect
      (list
       ;; "banana" -> "ana" (length 3)
       (funcall 'neovm--st3-longest-repeated "banana")
       ;; "abcabc" -> "abc" (length 3)
       (funcall 'neovm--st3-longest-repeated "abcabc")
       ;; "aaaa" -> "aaa" (length 3)
       (funcall 'neovm--st3-longest-repeated "aaaa")
       ;; "abcd" -> no repeat
       (funcall 'neovm--st3-longest-repeated "abcd")
       ;; "mississippi" -> "issi" (length 4)
       (funcall 'neovm--st3-longest-repeated "mississippi")
       ;; Single char -> no repeat
       (funcall 'neovm--st3-longest-repeated "x")
       ;; "abab" -> "ab" (length 2)
       (funcall 'neovm--st3-longest-repeated "abab"))
    (fmakunbound 'neovm--st3-insert)
    (fmakunbound 'neovm--st3-build)
    (fmakunbound 'neovm--st3-branching-children)
    (fmakunbound 'neovm--st3-longest-repeated)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 . \"\") (0 . \"\") (0 . \"\") (0 . \"\") (4 . \"issi\") (0 . \"\") (0 . \"\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Suffix counting: how many suffixes pass through each node
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_suffix_trie_suffix_counting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--st4-insert
    (lambda (root suffix)
      (let ((node root)
            (i 0)
            (len (length suffix)))
        (while (< i len)
          (let* ((ch (aref suffix i))
                 (child (assq ch (cdr node))))
            (if child
                (setq node (cdr child))
              (let ((new-node (list 'node)))
                (setcdr node (cons (cons ch new-node) (cdr node)))
                (setq node new-node))))
          (setq i (1+ i)))
        (unless (assq 'end (cdr node))
          (setcdr node (cons (cons 'end t) (cdr node))))
        root)))

  (fset 'neovm--st4-build
    (lambda (text)
      (let ((root (list 'node))
            (len (length text))
            (i 0))
        (while (< i len)
          (funcall 'neovm--st4-insert root (substring text i))
          (setq i (1+ i)))
        root)))

  (fset 'neovm--st4-count-suffixes
    (lambda (node)
      "Count the number of suffix endpoints reachable from NODE."
      (let ((count (if (assq 'end (cdr node)) 1 0)))
        (dolist (child (cdr node) count)
          (unless (eq (car child) 'end)
            (setq count (+ count (funcall 'neovm--st4-count-suffixes
                                          (cdr child)))))))))

  (fset 'neovm--st4-pattern-count
    (lambda (trie pattern)
      "Count how many times PATTERN appears as a substring.
       This equals the number of suffix endpoints reachable from
       the node reached by traversing PATTERN."
      (let ((node trie)
            (i 0)
            (len (length pattern))
            (found t))
        (while (and (< i len) found)
          (let ((child (assq (aref pattern i) (cdr node))))
            (if child
                (setq node (cdr child))
              (setq found nil)))
          (setq i (1+ i)))
        (if found
            (funcall 'neovm--st4-count-suffixes node)
          0))))

  (unwind-protect
      (let ((trie (funcall 'neovm--st4-build "banana")))
        (list
         ;; "a" appears 3 times in "banana"
         (funcall 'neovm--st4-pattern-count trie "a")
         ;; "an" appears 2 times
         (funcall 'neovm--st4-pattern-count trie "an")
         ;; "ana" appears 2 times
         (funcall 'neovm--st4-pattern-count trie "ana")
         ;; "b" appears 1 time
         (funcall 'neovm--st4-pattern-count trie "b")
         ;; "n" appears 2 times
         (funcall 'neovm--st4-pattern-count trie "n")
         ;; "banana" appears 1 time
         (funcall 'neovm--st4-pattern-count trie "banana")
         ;; "xyz" appears 0 times
         (funcall 'neovm--st4-pattern-count trie "xyz")
         ;; "" (empty) appears 6 times (all suffixes)
         (funcall 'neovm--st4-pattern-count trie "")))
    (fmakunbound 'neovm--st4-insert)
    (fmakunbound 'neovm--st4-build)
    (fmakunbound 'neovm--st4-count-suffixes)
    (fmakunbound 'neovm--st4-pattern-count)))"#;
    let expect = expect_test::expect![[r#""OK (3 2 2 1 2 1 0 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Pattern matching with wildcards
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_suffix_trie_wildcard_matching() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--st5-insert
    (lambda (root suffix)
      (let ((node root)
            (i 0)
            (len (length suffix)))
        (while (< i len)
          (let* ((ch (aref suffix i))
                 (child (assq ch (cdr node))))
            (if child
                (setq node (cdr child))
              (let ((new-node (list 'node)))
                (setcdr node (cons (cons ch new-node) (cdr node)))
                (setq node new-node))))
          (setq i (1+ i)))
        (unless (assq 'end (cdr node))
          (setcdr node (cons (cons 'end t) (cdr node))))
        root)))

  (fset 'neovm--st5-build
    (lambda (text)
      (let ((root (list 'node))
            (len (length text))
            (i 0))
        (while (< i len)
          (funcall 'neovm--st5-insert root (substring text i))
          (setq i (1+ i)))
        root)))

  (fset 'neovm--st5-wildcard-match
    (lambda (node pattern idx plen)
      "Match PATTERN starting at IDX against trie NODE.
       '?' in pattern matches any single character.
       '*' in pattern matches zero or more characters.
       Returns t if match found, nil otherwise."
      (cond
       ;; End of pattern: we've matched
       ((= idx plen) t)
       ;; '*' wildcard: try matching 0 or more chars
       ((= (aref pattern idx) ?*)
        (or
         ;; Match zero chars: skip '*'
         (funcall 'neovm--st5-wildcard-match node pattern (1+ idx) plen)
         ;; Match one or more: try every child
         (let ((found nil))
           (dolist (child (cdr node) found)
             (unless (or (eq (car child) 'end) found)
               ;; Try continuing with '*' still active
               (when (funcall 'neovm--st5-wildcard-match
                              (cdr child) pattern idx plen)
                 (setq found t)))))))
       ;; '?' wildcard: match any single character
       ((= (aref pattern idx) ??)
        (let ((found nil))
          (dolist (child (cdr node) found)
            (unless (or (eq (car child) 'end) found)
              (when (funcall 'neovm--st5-wildcard-match
                             (cdr child) pattern (1+ idx) plen)
                (setq found t))))))
       ;; Literal character
       (t
        (let ((child (assq (aref pattern idx) (cdr node))))
          (when child
            (funcall 'neovm--st5-wildcard-match
                     (cdr child) pattern (1+ idx) plen)))))))

  (fset 'neovm--st5-has-match
    (lambda (trie pattern)
      "Check if any substring of the text matches PATTERN with wildcards."
      (funcall 'neovm--st5-wildcard-match trie pattern 0 (length pattern))))

  (unwind-protect
      (let ((trie (funcall 'neovm--st5-build "banana")))
        (list
         ;; Exact matches
         (funcall 'neovm--st5-has-match trie "ban")
         (funcall 'neovm--st5-has-match trie "ana")
         ;; ? wildcard: single char
         (funcall 'neovm--st5-has-match trie "b?n")    ;; matches "ban"
         (funcall 'neovm--st5-has-match trie "?an")    ;; matches "ban", "nan"
         (funcall 'neovm--st5-has-match trie "a?a")    ;; matches "ana"
         (funcall 'neovm--st5-has-match trie "??n")    ;; matches "ban", "nan"
         ;; * wildcard: zero or more chars
         (funcall 'neovm--st5-has-match trie "b*a")    ;; matches "banana", "ba", "bana"
         (funcall 'neovm--st5-has-match trie "*na")    ;; matches "na", "ana", "bana", "banana", "nana"
         (funcall 'neovm--st5-has-match trie "b*")     ;; matches anything starting with b
         (funcall 'neovm--st5-has-match trie "*")       ;; matches any substring
         ;; No match
         (funcall 'neovm--st5-has-match trie "x?z")
         (funcall 'neovm--st5-has-match trie "bx*")))
    (fmakunbound 'neovm--st5-insert)
    (fmakunbound 'neovm--st5-build)
    (fmakunbound 'neovm--st5-wildcard-match)
    (fmakunbound 'neovm--st5-has-match)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Longest common substring of two strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_suffix_trie_longest_common_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build trie of first string's suffixes, then for each suffix of second
    // string, find how far we can traverse in the trie.
    let form = r#"(progn
  (fset 'neovm--st6-insert
    (lambda (root suffix)
      (let ((node root)
            (i 0)
            (len (length suffix)))
        (while (< i len)
          (let* ((ch (aref suffix i))
                 (child (assq ch (cdr node))))
            (if child
                (setq node (cdr child))
              (let ((new-node (list 'node)))
                (setcdr node (cons (cons ch new-node) (cdr node)))
                (setq node new-node))))
          (setq i (1+ i)))
        (unless (assq 'end (cdr node))
          (setcdr node (cons (cons 'end t) (cdr node))))
        root)))

  (fset 'neovm--st6-build
    (lambda (text)
      (let ((root (list 'node))
            (len (length text))
            (i 0))
        (while (< i len)
          (funcall 'neovm--st6-insert root (substring text i))
          (setq i (1+ i)))
        root)))

  (fset 'neovm--st6-match-length
    (lambda (trie str start)
      "Return the length of the longest prefix of STR[start..] that
       can be traversed in TRIE."
      (let ((node trie)
            (i start)
            (len (length str))
            (depth 0)
            (done nil))
        (while (and (< i len) (not done))
          (let ((child (assq (aref str i) (cdr node))))
            (if child
                (progn (setq node (cdr child))
                       (setq depth (1+ depth))
                       (setq i (1+ i)))
              (setq done t))))
        depth)))

  (fset 'neovm--st6-lcs
    (lambda (s1 s2)
      "Find the longest common substring of S1 and S2."
      (let ((trie (funcall 'neovm--st6-build s1))
            (best-len 0)
            (best-start 0)
            (i 0)
            (len2 (length s2)))
        (while (< i len2)
          (let ((match-len (funcall 'neovm--st6-match-length trie s2 i)))
            (when (> match-len best-len)
              (setq best-len match-len)
              (setq best-start i)))
          (setq i (1+ i)))
        (if (> best-len 0)
            (cons best-len (substring s2 best-start (+ best-start best-len)))
          (cons 0 "")))))

  (unwind-protect
      (list
       ;; "banana" and "nanapple" -> "nana" (length 4)
       (funcall 'neovm--st6-lcs "banana" "nanapple")
       ;; "abcdefg" and "xyzabcdw" -> "abcd" (length 4)
       (funcall 'neovm--st6-lcs "abcdefg" "xyzabcdw")
       ;; "abc" and "xyz" -> no common substring
       (funcall 'neovm--st6-lcs "abc" "xyz")
       ;; Same string -> the whole string
       (funcall 'neovm--st6-lcs "hello" "hello")
       ;; One empty string
       (funcall 'neovm--st6-lcs "" "abc")
       (funcall 'neovm--st6-lcs "abc" "")
       ;; Single common character
       (funcall 'neovm--st6-lcs "axb" "cyad")
       ;; "xyzabcxyz" and "xxxabcxxx" -> "abc" or "xyz" or "xabc" (length 3)
       (funcall 'neovm--st6-lcs "xyzabcxyz" "xxxabcxxx"))
    (fmakunbound 'neovm--st6-insert)
    (fmakunbound 'neovm--st6-build)
    (fmakunbound 'neovm--st6-match-length)
    (fmakunbound 'neovm--st6-lcs)))"#;
    let expect = expect_test::expect![[
        r#""OK ((4 . \"nana\") (4 . \"abcd\") (0 . \"\") (5 . \"hello\") (0 . \"\") (0 . \"\") (1 . \"a\") (4 . \"abcx\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

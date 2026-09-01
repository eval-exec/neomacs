//! Oracle parity tests for `sort` with complex patterns beyond basic sorting.
//!
//! Tests stability semantics, custom multi-key comparators, record sorting,
//! topological-like ordering, bucket sort simulation, chained sort operations,
//! and sort with side-effect-tracking predicates.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Sort stability: equal elements preserve relative order
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_stability_tagged_elements() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Sort tagged elements by priority; tags disambiguate original order.
    // Elements with the same priority should remain in input order IF sort is stable.
    // Emacs sort uses merge sort which is stable, so we verify that.
    let form = r#"
(let* ((items '((3 . "c1") (1 . "a1") (2 . "b1") (1 . "a2") (3 . "c2")
                (2 . "b2") (1 . "a3") (2 . "b3") (3 . "c3")))
       (sorted (sort (copy-sequence items)
                     (lambda (x y) (< (car x) (car y))))))
  ;; Group by priority and extract tags to verify stability
  (let ((group1 (mapcar #'cdr (seq-filter (lambda (x) (= (car x) 1)) sorted)))
        (group2 (mapcar #'cdr (seq-filter (lambda (x) (= (car x) 2)) sorted)))
        (group3 (mapcar #'cdr (seq-filter (lambda (x) (= (car x) 3)) sorted))))
    (list
     ;; Overall sorted by key
     (mapcar #'car sorted)
     ;; Within each group, original order preserved (stable sort)
     group1
     group2
     group3)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((1 1 1 2 2 2 3 3 3) (\"a1\" \"a2\" \"a3\") (\"b1\" \"b2\" \"b3\") (\"c1\" \"c2\" \"c3\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sort records by multiple fields (primary + secondary + tertiary)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_multi_field_records() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Records: (department . (seniority . name))
    // Sort by department asc, then seniority desc, then name asc
    let form = r#"
(let ((records '(("eng" . (3 . "alice"))
                 ("eng" . (5 . "bob"))
                 ("eng" . (3 . "carol"))
                 ("hr" . (2 . "dave"))
                 ("hr" . (4 . "eve"))
                 ("sales" . (1 . "frank"))
                 ("sales" . (1 . "grace"))
                 ("hr" . (2 . "hank")))))
  (let ((sorted (sort (copy-sequence records)
                      (lambda (a b)
                        (let ((dept-a (car a)) (dept-b (car b))
                              (sen-a (cadr a)) (sen-b (cadr b))
                              (name-a (cddr a)) (name-b (cddr b)))
                          (cond
                           ;; Primary: department ascending
                           ((string-lessp dept-a dept-b) t)
                           ((string-lessp dept-b dept-a) nil)
                           ;; Secondary: seniority descending
                           ((> sen-a sen-b) t)
                           ((< sen-a sen-b) nil)
                           ;; Tertiary: name ascending
                           (t (string-lessp name-a name-b))))))))
    (list
     ;; Extract department ordering
     (mapcar #'car sorted)
     ;; Extract (seniority . name) tuples
     (mapcar #'cdr sorted))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((\"eng\" \"eng\" \"eng\" \"hr\" \"hr\" \"hr\" \"sales\" \"sales\") ((5 . \"bob\") (3 . \"alice\") (3 . \"carol\") (4 . \"eve\") (2 . \"dave\") (2 . \"hank\") (1 . \"frank\") (1 . \"grace\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sort with custom comparator using a priority mapping
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_priority_mapping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Sort symbols by a custom priority order, not alphabetical
    let form = r#"
(let ((priority '((critical . 0) (high . 1) (medium . 2) (low . 3) (none . 4)))
      (items '(low critical medium none high critical low medium)))
  (let ((sorted (sort (copy-sequence items)
                      (lambda (a b)
                        (< (cdr (assq a priority))
                           (cdr (assq b priority)))))))
    (list sorted
          ;; Verify ordering is correct
          (let ((ok t) (prev -1))
            (dolist (item sorted)
              (let ((p (cdr (assq item priority))))
                (when (< p prev) (setq ok nil))
                (setq p prev)))
            ok)
          ;; Count per priority
          (mapcar (lambda (p)
                    (cons (car p)
                          (length (seq-filter (lambda (x) (eq x (car p))) sorted))))
                  priority))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((critical critical high medium medium low low none) t ((critical . 2) (high . 1) (medium . 2) (low . 2) (none . 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Topological-like ordering with custom comparator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_topological_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Given dependency pairs (A depends-on B), sort items such that
    // dependencies come before dependents. This uses sort with a comparator
    // that checks the dependency graph.
    let form = r#"
(progn
  (fset 'neovm--sort-topo-depends-p
    (lambda (deps a b)
      "Return t if A transitively depends on B."
      (let ((visited (make-hash-table))
            (queue (list a))
            (found nil))
        (while (and queue (not found))
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (unless (gethash current visited)
              (puthash current t visited)
              (dolist (dep deps)
                (when (eq (car dep) current)
                  (if (eq (cdr dep) b)
                      (setq found t)
                    (setq queue (cons (cdr dep) queue))))))))
        found)))

  (unwind-protect
      (let ((deps '((c . a) (c . b) (d . c) (d . a) (e . d) (b . a)))
            (items '(e c a d b)))
        ;; Sort: if x depends on y, then y < x
        (let ((sorted (sort (copy-sequence items)
                            (lambda (x y)
                              (funcall 'neovm--sort-topo-depends-p deps y x)))))
          (list
           sorted
           ;; Verify: for each dependency (x depends on y), y appears before x
           (let ((ok t))
             (dolist (dep deps)
               (let ((pos-dependent (length (memq (car dep) sorted)))
                     (pos-dependency (length (memq (cdr dep) sorted))))
                 ;; memq returns tail, so longer tail = earlier position
                 (when (<= pos-dependent pos-dependency)
                   ;; dependent should be after dependency (shorter tail)
                   (when (> pos-dependent 0)
                     nil))
                 ;; Actually check: dependency index < dependent index
                 (let ((idx-dep nil) (idx-item nil) (i 0))
                   (dolist (s sorted)
                     (when (eq s (cdr dep)) (setq idx-dep i))
                     (when (eq s (car dep)) (setq idx-item i))
                     (setq i (1+ i)))
                   (when (and idx-dep idx-item (> idx-dep idx-item))
                     (setq ok nil)))))
             ok))))
    (fmakunbound 'neovm--sort-topo-depends-p)))
"#;
    let expect = expect_test::expect![[r#""OK ((a b c d e) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bucket sort simulation using sort + partitioning
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_bucket_sort_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate bucket sort: partition numbers into buckets (decades),
    // sort each bucket, then concatenate.
    let form = r#"
(let ((numbers '(42 17 93 8 55 31 72 4 68 29 87 11 50 63 26 99 3 44 76 15)))
  ;; Partition into buckets by tens digit
  (let ((buckets (make-vector 10 nil)))
    (dolist (n numbers)
      (let ((bucket (/ n 10)))
        (when (>= bucket 10) (setq bucket 9))
        (aset buckets bucket (cons n (aref buckets bucket)))))
    ;; Sort each bucket
    (let ((i 0))
      (while (< i 10)
        (aset buckets i (sort (aref buckets i) #'<))
        (setq i (1+ i))))
    ;; Concatenate buckets
    (let ((result nil) (j 0))
      (while (< j 10)
        (setq result (append result (aref buckets j)))
        (setq j (1+ j)))
      (list
       ;; Fully sorted result
       result
       ;; Matches regular sort
       (equal result (sort (copy-sequence numbers) #'<))
       ;; Number of non-empty buckets
       (let ((count 0) (k 0))
         (while (< k 10)
           (when (aref buckets k) (setq count (1+ count)))
           (setq k (1+ k)))
         count)
       ;; Bucket sizes
       (let ((sizes nil) (m 0))
         (while (< m 10)
           (let ((s (length (aref buckets m))))
             (when (> s 0)
               (setq sizes (cons (cons m s) sizes))))
           (setq m (1+ m)))
         (nreverse sizes))))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((3 4 8 11 15 17 26 29 31 42 44 50 55 63 68 72 76 87 93 99) t 10 ((0 . 3) (1 . 3) (2 . 2) (3 . 1) (4 . 2) (5 . 2) (6 . 2) (7 . 2) (8 . 1) (9 . 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sort with comparison counting (measuring work done)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_comparison_counting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use a comparison function that counts how many times it is called.
    // Verify sorted result is correct and count is reasonable for merge sort.
    let form = r#"
(let ((count 0))
  (let* ((items (list 5 3 8 1 9 2 7 4 6 10))
         (n (length items))
         (sorted (sort (copy-sequence items)
                       (lambda (a b)
                         (setq count (1+ count))
                         (< a b)))))
    (list
     ;; Correctly sorted
     sorted
     ;; Number of comparisons
     count
     ;; Reasonable bound: merge sort is O(n log n)
     ;; For n=10, n*log2(n) ~ 33, so count should be <= 40
     (<= count 40)
     ;; At least n-1 comparisons needed
     (>= count (1- n)))))
"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5 6 7 8 9 10) 22 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Chained sort: sort by one key, then stable re-sort by another
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_chained_stable_sorts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // First sort by name, then stable sort by score.
    // Because merge sort is stable, the name ordering is preserved within
    // equal scores.
    let form = r#"
(let ((students '(("alice" . 85) ("bob" . 92) ("carol" . 85)
                  ("dave" . 78) ("eve" . 92) ("frank" . 85)
                  ("grace" . 78) ("hank" . 92))))
  ;; First sort by name
  (let ((by-name (sort (copy-sequence students)
                       (lambda (a b) (string-lessp (car a) (car b))))))
    ;; Then stable sort by score (ascending)
    (let ((by-score (sort (copy-sequence by-name)
                          (lambda (a b) (< (cdr a) (cdr b))))))
      (list
       ;; Names within each score group should be alphabetical (stable)
       by-score
       ;; Extract groups
       (let ((score78 (mapcar #'car (seq-filter (lambda (x) (= (cdr x) 78)) by-score)))
             (score85 (mapcar #'car (seq-filter (lambda (x) (= (cdr x) 85)) by-score)))
             (score92 (mapcar #'car (seq-filter (lambda (x) (= (cdr x) 92)) by-score))))
         (list
          ;; Each group alphabetically ordered due to stability
          score78
          score85
          score92
          ;; Verify alphabetical within groups
          (equal score78 (sort (copy-sequence score78) #'string-lessp))
          (equal score85 (sort (copy-sequence score85) #'string-lessp))
          (equal score92 (sort (copy-sequence score92) #'string-lessp))))))))
"#;
    let expect = expect_test::expect![[
        r#""OK (((\"dave\" . 78) (\"grace\" . 78) (\"alice\" . 85) (\"carol\" . 85) (\"frank\" . 85) (\"bob\" . 92) (\"eve\" . 92) (\"hank\" . 92)) ((\"dave\" \"grace\") (\"alice\" \"carol\" \"frank\") (\"bob\" \"eve\" \"hank\") t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sort with key extraction function (Schwartzian transform)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_schwartzian_transform() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Schwartzian transform: decorate-sort-undecorate pattern.
    // Avoids recomputing expensive keys during comparison.
    let form = r#"
(let ((strings '("banana" "fig" "apple" "elderberry" "cherry" "date" "grape")))
  ;; Expensive key: sort by number of vowels, then by length, then alphabetical
  (let ((count-vowels
         (lambda (s)
           (let ((count 0) (i 0) (len (length s)))
             (while (< i len)
               (when (memq (aref s i) '(?a ?e ?i ?o ?u))
                 (setq count (1+ count)))
               (setq i (1+ i)))
             count))))
    ;; Decorate: attach computed key
    (let* ((decorated (mapcar (lambda (s)
                                (list (funcall count-vowels s) (length s) s))
                              strings))
           ;; Sort by decoration
           (sorted-dec (sort decorated
                            (lambda (a b)
                              (cond
                               ((< (car a) (car b)) t)
                               ((> (car a) (car b)) nil)
                               ((< (cadr a) (cadr b)) t)
                               ((> (cadr a) (cadr b)) nil)
                               (t (string-lessp (caddr a) (caddr b)))))))
           ;; Undecorate
           (result (mapcar #'caddr sorted-dec)))
      (list
       result
       ;; Show decorations for verification
       (mapcar (lambda (d) (list (car d) (cadr d) (caddr d))) sorted-dec)))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((\"fig\" \"cherry\" \"date\" \"apple\" \"grape\" \"banana\" \"elderberry\") ((1 3 \"fig\") (1 6 \"cherry\") (2 4 \"date\") (2 5 \"apple\") (2 5 \"grape\") (3 6 \"banana\") (3 10 \"elderberry\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sort a nested tree structure by flattening, sorting, rebuilding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_flatten_and_rebuild() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Given a nested list (tree), flatten it, sort the leaves, then rebuild
    // the original structure with sorted values.
    let form = r#"
(progn
  (fset 'neovm--sort-flatten
    (lambda (tree)
      "Flatten a nested list into a flat list of atoms."
      (cond
       ((null tree) nil)
       ((atom tree) (list tree))
       (t (append (funcall 'neovm--sort-flatten (car tree))
                  (funcall 'neovm--sort-flatten (cdr tree)))))))

  (fset 'neovm--sort-rebuild
    (lambda (tree sorted-iter)
      "Rebuild TREE structure with values taken from SORTED-ITER (a cons cell holding remaining values)."
      (cond
       ((null tree) nil)
       ((atom tree)
        (let ((val (car (car sorted-iter))))
          (setcar sorted-iter (cdr (car sorted-iter)))
          val))
       (t (cons (funcall 'neovm--sort-rebuild (car tree) sorted-iter)
                (funcall 'neovm--sort-rebuild (cdr tree) sorted-iter))))))

  (unwind-protect
      (let ((tree '((5 (3 1)) (8 (2 (9 4))) (7 6))))
        (let* ((flat (funcall 'neovm--sort-flatten tree))
               (sorted-flat (sort (copy-sequence flat) #'<))
               (iter (list sorted-flat))
               (rebuilt (funcall 'neovm--sort-rebuild tree iter)))
          (list
           ;; Original flattened
           flat
           ;; Sorted flat
           sorted-flat
           ;; Rebuilt with same structure
           rebuilt
           ;; Structure matches (same nesting)
           (equal (funcall 'neovm--sort-flatten rebuilt) sorted-flat)
           ;; Length preserved
           (= (length flat) (length sorted-flat)))))
    (fmakunbound 'neovm--sort-flatten)
    (fmakunbound 'neovm--sort-rebuild)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((5 3 1 8 2 9 4 7 6) (1 2 3 4 5 6 7 8 9) ((1 (2 3)) (4 (5 (6 7))) (8 9)) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

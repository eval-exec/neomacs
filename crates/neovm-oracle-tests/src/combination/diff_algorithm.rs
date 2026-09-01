//! Oracle parity tests for a simple diff algorithm implemented in Elisp:
//! longest common subsequence (LCS), edit distance with operations,
//! diff hunk generation (add/delete/change), patch application to
//! reconstruct target, and three-way merge basics.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Longest common subsequence (LCS) via dynamic programming
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_diff_lcs_dynamic_programming() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (fset 'neovm--diff-lcs
    (lambda (seq-a seq-b)
      "Compute LCS of two lists using DP. Returns the LCS as a list."
      (let* ((m (length seq-a))
             (n (length seq-b))
             ;; dp table: (m+1) x (n+1) matrix stored as vector of vectors
             (dp (make-vector (1+ m) nil)))
        ;; Initialize dp table
        (let ((i 0))
          (while (<= i m)
            (aset dp i (make-vector (1+ n) 0))
            (setq i (1+ i))))
        ;; Fill dp table
        (let ((i 1))
          (while (<= i m)
            (let ((j 1))
              (while (<= j n)
                (if (equal (nth (1- i) seq-a) (nth (1- j) seq-b))
                    (aset (aref dp i) j (1+ (aref (aref dp (1- i)) (1- j))))
                  (aset (aref dp i) j
                        (max (aref (aref dp (1- i)) j)
                             (aref (aref dp i) (1- j)))))
                (setq j (1+ j))))
            (setq i (1+ i))))
        ;; Backtrack to find the actual LCS
        (let ((result nil)
              (i m) (j n))
          (while (and (> i 0) (> j 0))
            (cond
             ((equal (nth (1- i) seq-a) (nth (1- j) seq-b))
              (setq result (cons (nth (1- i) seq-a) result))
              (setq i (1- i) j (1- j)))
             ((> (aref (aref dp (1- i)) j)
                 (aref (aref dp i) (1- j)))
              (setq i (1- i)))
             (t (setq j (1- j)))))
          (list :length (aref (aref dp m) n)
                :lcs result)))))

  (unwind-protect
      (list
       ;; Basic LCS
       (funcall 'neovm--diff-lcs '(a b c d e f) '(a c d f))
       ;; Identical sequences
       (funcall 'neovm--diff-lcs '(x y z) '(x y z))
       ;; No common elements
       (funcall 'neovm--diff-lcs '(a b c) '(d e f))
       ;; One empty
       (funcall 'neovm--diff-lcs '() '(a b c))
       (funcall 'neovm--diff-lcs '(a b c) '())
       ;; Longer example
       (funcall 'neovm--diff-lcs '(1 2 3 4 5 6 7 8) '(2 4 6 8 10))
       ;; String-like diff (characters as symbols)
       (funcall 'neovm--diff-lcs '(h e l l o w o r l d) '(h e l p w o r k)))
    (fmakunbound 'neovm--diff-lcs)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((:length 4 :lcs (a c d f)) (:length 3 :lcs (x y z)) (:length 0 :lcs nil) (:length 0 :lcs nil) (:length 0 :lcs nil) (:length 4 :lcs (2 4 6 8)) (:length 6 :lcs (h e l w o r)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Edit distance with operation tracking (Levenshtein + edit script)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_diff_edit_distance_with_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (fset 'neovm--diff-edit-distance
    (lambda (src dst)
      "Compute edit distance between two lists with operation tracking.
       Returns (:distance N :ops ((op . item) ...))."
      (let* ((m (length src))
             (n (length dst))
             (dp (make-vector (1+ m) nil))
             (ops (make-vector (1+ m) nil)))
        ;; Initialize
        (let ((i 0))
          (while (<= i m)
            (aset dp i (make-vector (1+ n) 0))
            (aset ops i (make-vector (1+ n) nil))
            (setq i (1+ i))))
        ;; Base cases
        (let ((i 0))
          (while (<= i m)
            (aset (aref dp i) 0 i)
            (aset (aref ops i) 0
                  (if (= i 0) nil
                    (append (aref (aref ops (1- i)) 0)
                            (list (cons 'delete (nth (1- i) src))))))
            (setq i (1+ i))))
        (let ((j 0))
          (while (<= j n)
            (aset (aref dp 0) j j)
            (aset (aref ops 0) j
                  (if (= j 0) nil
                    (append (aref (aref ops 0) (1- j))
                            (list (cons 'insert (nth (1- j) dst))))))
            (setq j (1+ j))))
        ;; Fill DP table
        (let ((i 1))
          (while (<= i m)
            (let ((j 1))
              (while (<= j n)
                (let ((cost (if (equal (nth (1- i) src) (nth (1- j) dst)) 0 1))
                      (del (1+ (aref (aref dp (1- i)) j)))
                      (ins (1+ (aref (aref dp i) (1- j))))
                      (sub (+ (aref (aref dp (1- i)) (1- j))
                              (if (equal (nth (1- i) src) (nth (1- j) dst)) 0 1))))
                  (cond
                   ((<= sub (min del ins))
                    (aset (aref dp i) j sub)
                    (aset (aref ops i) j
                          (append (aref (aref ops (1- i)) (1- j))
                                  (if (= cost 0)
                                      (list (cons 'keep (nth (1- i) src)))
                                    (list (list 'replace (nth (1- i) src) (nth (1- j) dst)))))))
                   ((<= del ins)
                    (aset (aref dp i) j del)
                    (aset (aref ops i) j
                          (append (aref (aref ops (1- i)) j)
                                  (list (cons 'delete (nth (1- i) src))))))
                   (t
                    (aset (aref dp i) j ins)
                    (aset (aref ops i) j
                          (append (aref (aref ops i) (1- j))
                                  (list (cons 'insert (nth (1- j) dst))))))))
                (setq j (1+ j))))
            (setq i (1+ i))))
        (list :distance (aref (aref dp m) n)
              :ops (aref (aref ops m) n)))))

  (unwind-protect
      (list
       ;; Simple edit
       (funcall 'neovm--diff-edit-distance '(a b c) '(a x c))
       ;; Insertion only
       (funcall 'neovm--diff-edit-distance '(a c) '(a b c))
       ;; Deletion only
       (funcall 'neovm--diff-edit-distance '(a b c) '(a c))
       ;; Complete replacement
       (funcall 'neovm--diff-edit-distance '(a b) '(x y))
       ;; Identical
       (funcall 'neovm--diff-edit-distance '(a b c) '(a b c))
       ;; Empty to non-empty
       (funcall 'neovm--diff-edit-distance '() '(a b))
       ;; Non-empty to empty
       (funcall 'neovm--diff-edit-distance '(a b) '()))
    (fmakunbound 'neovm--diff-edit-distance)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((:distance 1 :ops ((keep . a) (replace b x) (keep . c))) (:distance 1 :ops ((keep . a) (insert . b) (keep . c))) (:distance 1 :ops ((keep . a) (delete . b) (keep . c))) (:distance 2 :ops ((replace a x) (replace b y))) (:distance 0 :ops ((keep . a) (keep . b) (keep . c))) (:distance 2 :ops ((insert . a) (insert . b))) (:distance 2 :ops ((delete . a) (delete . b))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Diff hunk generation: produce add/delete/change hunks
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_diff_hunk_generation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (fset 'neovm--diff-lcs-indices
    (lambda (seq-a seq-b)
      "Return LCS as list of (index-in-a . index-in-b) pairs."
      (let* ((m (length seq-a))
             (n (length seq-b))
             (dp (make-vector (1+ m) nil)))
        (let ((i 0))
          (while (<= i m)
            (aset dp i (make-vector (1+ n) 0))
            (setq i (1+ i))))
        (let ((i 1))
          (while (<= i m)
            (let ((j 1))
              (while (<= j n)
                (if (equal (nth (1- i) seq-a) (nth (1- j) seq-b))
                    (aset (aref dp i) j (1+ (aref (aref dp (1- i)) (1- j))))
                  (aset (aref dp i) j
                        (max (aref (aref dp (1- i)) j)
                             (aref (aref dp i) (1- j)))))
                (setq j (1+ j))))
            (setq i (1+ i))))
        (let ((result nil) (i m) (j n))
          (while (and (> i 0) (> j 0))
            (cond
             ((equal (nth (1- i) seq-a) (nth (1- j) seq-b))
              (setq result (cons (cons (1- i) (1- j)) result))
              (setq i (1- i) j (1- j)))
             ((> (aref (aref dp (1- i)) j)
                 (aref (aref dp i) (1- j)))
              (setq i (1- i)))
             (t (setq j (1- j)))))
          result))))

  (fset 'neovm--diff-generate-hunks
    (lambda (src dst)
      "Generate diff hunks between SRC and DST lists.
       Each hunk is (type start-a end-a start-b end-b items).
       Types: add, delete, change."
      (let ((lcs-pairs (funcall 'neovm--diff-lcs-indices src dst))
            (hunks nil)
            (ai 0) (bi 0))
        ;; Walk through LCS matches, generating hunks for gaps
        (dolist (pair lcs-pairs)
          (let ((ma (car pair))
                (mb (cdr pair)))
            (when (or (< ai ma) (< bi mb))
              ;; There's a gap before this match
              (let ((del-items nil) (add-items nil))
                (let ((k ai))
                  (while (< k ma)
                    (setq del-items (cons (nth k src) del-items))
                    (setq k (1+ k))))
                (let ((k bi))
                  (while (< k mb)
                    (setq add-items (cons (nth k dst) add-items))
                    (setq k (1+ k))))
                (cond
                 ((and del-items add-items)
                  (setq hunks (cons (list 'change ai ma bi mb
                                          (nreverse del-items)
                                          (nreverse add-items))
                                    hunks)))
                 (del-items
                  (setq hunks (cons (list 'delete ai ma
                                          (nreverse del-items))
                                    hunks)))
                 (add-items
                  (setq hunks (cons (list 'add bi mb
                                          (nreverse add-items))
                                    hunks))))))
            (setq ai (1+ ma) bi (1+ mb))))
        ;; Trailing gap after last match
        (let ((del-items nil) (add-items nil))
          (let ((k ai))
            (while (< k (length src))
              (setq del-items (cons (nth k src) del-items))
              (setq k (1+ k))))
          (let ((k bi))
            (while (< k (length dst))
              (setq add-items (cons (nth k dst) add-items))
              (setq k (1+ k))))
          (cond
           ((and del-items add-items)
            (setq hunks (cons (list 'change ai (length src) bi (length dst)
                                    (nreverse del-items)
                                    (nreverse add-items))
                              hunks)))
           (del-items
            (setq hunks (cons (list 'delete ai (length src)
                                    (nreverse del-items))
                              hunks)))
           (add-items
            (setq hunks (cons (list 'add bi (length dst)
                                    (nreverse add-items))
                              hunks)))))
        (nreverse hunks))))

  (unwind-protect
      (list
       ;; Lines changed in middle
       (funcall 'neovm--diff-generate-hunks
                '(a b c d e) '(a b x y e))
       ;; Lines added
       (funcall 'neovm--diff-generate-hunks
                '(a b c) '(a b c d e))
       ;; Lines deleted
       (funcall 'neovm--diff-generate-hunks
                '(a b c d e) '(a c e))
       ;; Mixed operations
       (funcall 'neovm--diff-generate-hunks
                '(a b c d e f g) '(a x c d y z g))
       ;; Identical
       (funcall 'neovm--diff-generate-hunks
                '(a b c) '(a b c))
       ;; Completely different
       (funcall 'neovm--diff-generate-hunks
                '(a b c) '(x y z)))
    (fmakunbound 'neovm--diff-lcs-indices)
    (fmakunbound 'neovm--diff-generate-hunks)))
"#;
    let expect = expect_test::expect![[
        r#""OK (((change 2 4 2 4 (c d) (x y))) ((add 3 5 (d e))) ((delete 1 2 (b)) (delete 3 4 (d))) ((change 1 2 1 2 (b) (x)) (change 4 6 4 6 (e f) (y z))) nil ((change 0 3 0 3 (a b c) (x y z))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Patch application: apply edit operations to reconstruct target from source
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_diff_patch_application() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (fset 'neovm--diff-compute-patch
    (lambda (src dst)
      "Compute a patch: list of (op pos . data) instructions."
      (let* ((m (length src))
             (n (length dst))
             (dp (make-vector (1+ m) nil)))
        (let ((i 0))
          (while (<= i m)
            (aset dp i (make-vector (1+ n) 0))
            (setq i (1+ i))))
        (let ((i 1))
          (while (<= i m)
            (let ((j 1))
              (while (<= j n)
                (if (equal (nth (1- i) src) (nth (1- j) dst))
                    (aset (aref dp i) j (1+ (aref (aref dp (1- i)) (1- j))))
                  (aset (aref dp i) j
                        (max (aref (aref dp (1- i)) j)
                             (aref (aref dp i) (1- j)))))
                (setq j (1+ j))))
            (setq i (1+ i))))
        ;; Backtrack to build patch
        (let ((patch nil)
              (i m) (j n))
          (while (or (> i 0) (> j 0))
            (cond
             ((and (> i 0) (> j 0)
                   (equal (nth (1- i) src) (nth (1- j) dst)))
              (setq patch (cons (list 'keep (1- i) (nth (1- i) src)) patch))
              (setq i (1- i) j (1- j)))
             ((and (> j 0)
                   (or (= i 0)
                       (> (aref (aref dp i) (1- j))
                          (aref (aref dp (1- i)) j))))
              (setq patch (cons (list 'insert (1- j) (nth (1- j) dst)) patch))
              (setq j (1- j)))
             (t
              (setq patch (cons (list 'delete (1- i) (nth (1- i) src)) patch))
              (setq i (1- i)))))
          patch))))

  (fset 'neovm--diff-apply-patch
    (lambda (src patch)
      "Apply a patch to SRC to produce the target."
      (let ((result nil))
        (dolist (op patch)
          (let ((type (car op))
                (item (nth 2 op)))
            (cond
             ((eq type 'keep) (setq result (cons item result)))
             ((eq type 'insert) (setq result (cons item result)))
             ;; delete: skip the item
             )))
        (nreverse result))))

  (unwind-protect
      (let* ((tests (list
                     (cons '(a b c d e) '(a b x d e))
                     (cons '(1 2 3) '(1 2 3 4 5))
                     (cons '(x y z w) '(y z))
                     (cons '(a b c) '(d e f))
                     (cons '() '(a b c))
                     (cons '(a b c) '())))
             (results nil))
        (dolist (test tests)
          (let* ((src (car test))
                 (dst (cdr test))
                 (patch (funcall 'neovm--diff-compute-patch src dst))
                 (reconstructed (funcall 'neovm--diff-apply-patch src patch)))
            (setq results
                  (cons (list :src src :dst dst
                              :patch-len (length patch)
                              :reconstructed reconstructed
                              :match (equal reconstructed dst))
                        results))))
        (nreverse results))
    (fmakunbound 'neovm--diff-compute-patch)
    (fmakunbound 'neovm--diff-apply-patch)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((:src (a b c d e) :dst (a b x d e) :patch-len 6 :reconstructed (a b x d e) :match t) (:src (1 2 3) :dst (1 2 3 4 5) :patch-len 5 :reconstructed (1 2 3 4 5) :match t) (:src (x y z w) :dst (y z) :patch-len 4 :reconstructed (y z) :match t) (:src (a b c) :dst (d e f) :patch-len 6 :reconstructed (d e f) :match t) (:src nil :dst (a b c) :patch-len 3 :reconstructed (a b c) :match t) (:src (a b c) :dst nil :patch-len 3 :reconstructed nil :match t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Three-way merge: base, ours, theirs -> merged result
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_diff_three_way_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Simple LCS for merge
  (fset 'neovm--diff3-lcs
    (lambda (a b)
      "Return LCS elements of lists A and B."
      (let* ((m (length a)) (n (length b))
             (dp (make-vector (1+ m) nil)))
        (let ((i 0))
          (while (<= i m)
            (aset dp i (make-vector (1+ n) 0))
            (setq i (1+ i))))
        (let ((i 1))
          (while (<= i m)
            (let ((j 1))
              (while (<= j n)
                (if (equal (nth (1- i) a) (nth (1- j) b))
                    (aset (aref dp i) j (1+ (aref (aref dp (1- i)) (1- j))))
                  (aset (aref dp i) j
                        (max (aref (aref dp (1- i)) j)
                             (aref (aref dp i) (1- j)))))
                (setq j (1+ j))))
            (setq i (1+ i))))
        (let ((result nil) (i m) (j n))
          (while (and (> i 0) (> j 0))
            (cond
             ((equal (nth (1- i) a) (nth (1- j) b))
              (setq result (cons (nth (1- i) a) result))
              (setq i (1- i) j (1- j)))
             ((> (aref (aref dp (1- i)) j)
                 (aref (aref dp i) (1- j)))
              (setq i (1- i)))
             (t (setq j (1- j)))))
          result))))

  (fset 'neovm--diff3-changes
    (lambda (base modified)
      "Return list of changes: ((base-idx mod-idx element) ...) for matching,
       plus deletions and insertions."
      (let ((lcs (funcall 'neovm--diff3-lcs base modified))
            (changes nil)
            (bi 0) (mi 0))
        (dolist (common lcs)
          ;; Skip to this common element
          (while (and (< bi (length base))
                      (not (equal (nth bi base) common)))
            (setq changes (cons (list 'del bi (nth bi base)) changes))
            (setq bi (1+ bi)))
          (while (and (< mi (length modified))
                      (not (equal (nth mi modified) common)))
            (setq changes (cons (list 'add mi (nth mi modified)) changes))
            (setq mi (1+ mi)))
          (setq changes (cons (list 'keep bi mi common) changes))
          (setq bi (1+ bi) mi (1+ mi)))
        ;; Trailing
        (while (< bi (length base))
          (setq changes (cons (list 'del bi (nth bi base)) changes))
          (setq bi (1+ bi)))
        (while (< mi (length modified))
          (setq changes (cons (list 'add mi (nth mi modified)) changes))
          (setq mi (1+ mi)))
        (nreverse changes))))

  (fset 'neovm--diff3-merge
    (lambda (base ours theirs)
      "Three-way merge. If both sides change the same region, mark conflict."
      (let ((our-changes (funcall 'neovm--diff3-changes base ours))
            (their-changes (funcall 'neovm--diff3-changes base theirs))
            (result nil)
            (conflict nil))
        ;; Simple merge strategy: walk through base, apply non-conflicting changes
        (let ((bi 0)
              (our-adds (let ((h (make-hash-table))) h))
              (our-dels (let ((h (make-hash-table))) h))
              (their-adds (let ((h (make-hash-table))) h))
              (their-dels (let ((h (make-hash-table))) h)))
          ;; Index changes by base position
          (dolist (c our-changes)
            (when (eq (car c) 'del) (puthash (nth 1 c) t our-dels))
            (when (eq (car c) 'add)
              (let ((existing (gethash (nth 1 c) our-adds)))
                (puthash (nth 1 c)
                         (append (or existing nil) (list (nth 2 c)))
                         our-adds))))
          (dolist (c their-changes)
            (when (eq (car c) 'del) (puthash (nth 1 c) t their-dels))
            (when (eq (car c) 'add)
              (let ((existing (gethash (nth 1 c) their-adds)))
                (puthash (nth 1 c)
                         (append (or existing nil) (list (nth 2 c)))
                         their-adds))))
          ;; Walk base
          (while (< bi (length base))
            (let ((our-del (gethash bi our-dels))
                  (their-del (gethash bi their-dels)))
              (cond
               ;; Both delete: apply deletion
               ((and our-del their-del) nil)
               ;; Only ours deletes: apply if theirs didn't change
               (our-del
                (if their-del nil
                  nil))  ;; delete it
               ;; Only theirs deletes
               (their-del nil)
               ;; Neither deletes: keep
               (t (setq result (cons (nth bi base) result)))))
            (setq bi (1+ bi))))
        (list :merged (nreverse result)
              :has-conflict conflict))))

  (unwind-protect
      (list
       ;; Non-conflicting: ours adds, theirs adds at different points
       (let ((base '(a b c d e))
             (ours '(a b c d e f))
             (theirs '(a b c d e)))
         (funcall 'neovm--diff3-merge base ours theirs))
       ;; Both delete same element
       (let ((base '(a b c d e))
             (ours '(a c d e))
             (theirs '(a c d e)))
         (funcall 'neovm--diff3-merge base ours theirs))
       ;; One modifies, other untouched
       (let ((base '(a b c))
             (ours '(a x c))
             (theirs '(a b c)))
         (funcall 'neovm--diff3-merge base ours theirs))
       ;; Identical modifications
       (let ((base '(a b c))
             (ours '(a b c))
             (theirs '(a b c)))
         (funcall 'neovm--diff3-merge base ours theirs))
       ;; Changes from LCS
       (funcall 'neovm--diff3-changes '(a b c d e) '(a x c y e))
       (funcall 'neovm--diff3-lcs '(a b c d e f) '(b d f)))
    (fmakunbound 'neovm--diff3-lcs)
    (fmakunbound 'neovm--diff3-changes)
    (fmakunbound 'neovm--diff3-merge)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((:merged (a b c d e) :has-conflict nil) (:merged (a c d e) :has-conflict nil) (:merged (a c) :has-conflict nil) (:merged (a b c) :has-conflict nil) ((keep 0 0 a) (del 1 b) (add 1 x) (keep 2 2 c) (del 3 d) (add 3 y) (keep 4 4 e)) (b d f))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Line-based unified diff generation from string content
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_diff_unified_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (fset 'neovm--diff-split-lines
    (lambda (str)
      "Split a string into a list of lines."
      (let ((lines nil) (start 0) (len (length str)))
        (let ((i 0))
          (while (< i len)
            (when (= (aref str i) ?\n)
              (setq lines (cons (substring str start i) lines))
              (setq start (1+ i)))
            (setq i (1+ i))))
        (when (< start len)
          (setq lines (cons (substring str start) lines)))
        (nreverse lines))))

  (fset 'neovm--diff-line-lcs
    (lambda (a b)
      "LCS of line lists, returns matched index pairs."
      (let* ((m (length a)) (n (length b))
             (dp (make-vector (1+ m) nil)))
        (let ((i 0))
          (while (<= i m)
            (aset dp i (make-vector (1+ n) 0))
            (setq i (1+ i))))
        (let ((i 1))
          (while (<= i m)
            (let ((j 1))
              (while (<= j n)
                (if (string= (nth (1- i) a) (nth (1- j) b))
                    (aset (aref dp i) j (1+ (aref (aref dp (1- i)) (1- j))))
                  (aset (aref dp i) j
                        (max (aref (aref dp (1- i)) j)
                             (aref (aref dp i) (1- j)))))
                (setq j (1+ j))))
            (setq i (1+ i))))
        (let ((pairs nil) (i m) (j n))
          (while (and (> i 0) (> j 0))
            (cond
             ((string= (nth (1- i) a) (nth (1- j) b))
              (setq pairs (cons (cons (1- i) (1- j)) pairs))
              (setq i (1- i) j (1- j)))
             ((> (aref (aref dp (1- i)) j)
                 (aref (aref dp i) (1- j)))
              (setq i (1- i)))
             (t (setq j (1- j)))))
          pairs))))

  (fset 'neovm--diff-unified
    (lambda (old-text new-text)
      "Generate a simple unified diff between OLD-TEXT and NEW-TEXT strings."
      (let* ((old-lines (funcall 'neovm--diff-split-lines old-text))
             (new-lines (funcall 'neovm--diff-split-lines new-text))
             (matches (funcall 'neovm--diff-line-lcs old-lines new-lines))
             (output nil)
             (oi 0) (ni 0))
        (dolist (m matches)
          (let ((om (car m)) (nm (cdr m)))
            ;; Lines deleted from old
            (while (< oi om)
              (setq output (cons (concat "-" (nth oi old-lines)) output))
              (setq oi (1+ oi)))
            ;; Lines added in new
            (while (< ni nm)
              (setq output (cons (concat "+" (nth ni new-lines)) output))
              (setq ni (1+ ni)))
            ;; Common line
            (setq output (cons (concat " " (nth oi old-lines)) output))
            (setq oi (1+ oi) ni (1+ ni))))
        ;; Trailing
        (while (< oi (length old-lines))
          (setq output (cons (concat "-" (nth oi old-lines)) output))
          (setq oi (1+ oi)))
        (while (< ni (length new-lines))
          (setq output (cons (concat "+" (nth ni new-lines)) output))
          (setq ni (1+ ni)))
        (nreverse output))))

  (unwind-protect
      (list
       (funcall 'neovm--diff-unified
                "alpha\nbeta\ngamma\ndelta"
                "alpha\nbeta-modified\ngamma\nepsilon")
       (funcall 'neovm--diff-unified
                "line1\nline2\nline3"
                "line1\nline2\nline3")
       (funcall 'neovm--diff-unified
                "a\nb\nc"
                "x\ny\nz")
       (funcall 'neovm--diff-unified
                ""
                "new1\nnew2")
       (funcall 'neovm--diff-split-lines "hello\nworld\nfoo"))
    (fmakunbound 'neovm--diff-split-lines)
    (fmakunbound 'neovm--diff-line-lcs)
    (fmakunbound 'neovm--diff-unified)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((\" alpha\" \"-beta\" \"+beta-modified\" \" gamma\" \"-delta\" \"+epsilon\") (\" line1\" \" line2\" \" line3\") (\"-a\" \"-b\" \"-c\" \"+x\" \"+y\" \"+z\") (\"+new1\" \"+new2\") (\"hello\" \"world\" \"foo\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Diff statistics: count adds, deletes, changes across a diff
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_diff_statistics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (fset 'neovm--diff-stats-lcs-len
    (lambda (a b)
      "Return just the LCS length."
      (let* ((m (length a)) (n (length b))
             (dp (make-vector (1+ m) nil)))
        (let ((i 0))
          (while (<= i m)
            (aset dp i (make-vector (1+ n) 0))
            (setq i (1+ i))))
        (let ((i 1))
          (while (<= i m)
            (let ((j 1))
              (while (<= j n)
                (if (equal (nth (1- i) a) (nth (1- j) b))
                    (aset (aref dp i) j (1+ (aref (aref dp (1- i)) (1- j))))
                  (aset (aref dp i) j
                        (max (aref (aref dp (1- i)) j)
                             (aref (aref dp i) (1- j)))))
                (setq j (1+ j))))
            (setq i (1+ i))))
        (aref (aref dp m) n))))

  (fset 'neovm--diff-stats
    (lambda (src dst)
      "Compute diff statistics between SRC and DST."
      (let* ((lcs-len (funcall 'neovm--diff-stats-lcs-len src dst))
             (src-len (length src))
             (dst-len (length dst))
             (deletions (- src-len lcs-len))
             (insertions (- dst-len lcs-len))
             (similarity (if (= (max src-len dst-len) 0) 100
                           (/ (* lcs-len 100) (max src-len dst-len)))))
        (list :src-len src-len
              :dst-len dst-len
              :lcs-len lcs-len
              :deletions deletions
              :insertions insertions
              :total-changes (+ deletions insertions)
              :similarity-pct similarity))))

  (unwind-protect
      (list
       ;; Partial overlap
       (funcall 'neovm--diff-stats '(a b c d e f) '(a c e g h))
       ;; Identical
       (funcall 'neovm--diff-stats '(1 2 3 4 5) '(1 2 3 4 5))
       ;; Completely different
       (funcall 'neovm--diff-stats '(a b c) '(x y z))
       ;; One empty
       (funcall 'neovm--diff-stats '() '(a b c d))
       (funcall 'neovm--diff-stats '(a b c d) '())
       ;; Both empty
       (funcall 'neovm--diff-stats '() '())
       ;; Long sequences with partial match
       (funcall 'neovm--diff-stats
                '(1 2 3 4 5 6 7 8 9 10)
                '(1 3 5 7 9 11 13)))
    (fmakunbound 'neovm--diff-stats-lcs-len)
    (fmakunbound 'neovm--diff-stats)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((:src-len 6 :dst-len 5 :lcs-len 3 :deletions 3 :insertions 2 :total-changes 5 :similarity-pct 50) (:src-len 5 :dst-len 5 :lcs-len 5 :deletions 0 :insertions 0 :total-changes 0 :similarity-pct 100) (:src-len 3 :dst-len 3 :lcs-len 0 :deletions 3 :insertions 3 :total-changes 6 :similarity-pct 0) (:src-len 0 :dst-len 4 :lcs-len 0 :deletions 0 :insertions 4 :total-changes 4 :similarity-pct 0) (:src-len 4 :dst-len 0 :lcs-len 0 :deletions 4 :insertions 0 :total-changes 4 :similarity-pct 0) (:src-len 0 :dst-len 0 :lcs-len 0 :deletions 0 :insertions 0 :total-changes 0 :similarity-pct 100) (:src-len 10 :dst-len 7 :lcs-len 5 :deletions 5 :insertions 2 :total-changes 7 :similarity-pct 50))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

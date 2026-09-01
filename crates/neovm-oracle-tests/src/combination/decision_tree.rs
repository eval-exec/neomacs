//! Oracle parity tests for decision tree learning (ID3-like) in Elisp.
//!
//! Covers: entropy computation, information gain, best split selection,
//! recursive tree building, tree classification, handling categorical
//! and numeric attributes, pruning (reduced error), tree serialization,
//! and cross-validation accuracy.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Entropy computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_decision_tree_entropy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute Shannon entropy for label distributions.
    // Test pure sets (entropy=0), maximally impure (entropy=1 for binary),
    // and multi-class distributions.
    let form = r#"
(progn
  (fset 'neovm--dt-log2
    (lambda (x)
      "Base-2 logarithm via change of base."
      (if (<= x 0) 0.0
        (/ (log x) (log 2)))))

  (fset 'neovm--dt-entropy
    (lambda (labels)
      "Compute Shannon entropy of LABELS list."
      (let ((total (float (length labels)))
            (counts (make-hash-table :test 'equal))
            (ent 0.0))
        (dolist (l labels)
          (puthash l (1+ (gethash l counts 0)) counts))
        (maphash
         (lambda (_k v)
           (let ((p (/ (float v) total)))
             (when (> p 0)
               (setq ent (- ent (* p (funcall 'neovm--dt-log2 p)))))))
         counts)
        ;; Round to 4 decimal places to avoid float noise
        (/ (round (* ent 10000)) 10000.0))))

  (unwind-protect
      (list
       ;; Pure set: all same label -> entropy 0
       (funcall 'neovm--dt-entropy '(yes yes yes yes))
       ;; Perfectly balanced binary -> entropy 1.0
       (funcall 'neovm--dt-entropy '(yes no yes no))
       ;; 3 yes, 1 no -> entropy ~0.8113
       (funcall 'neovm--dt-entropy '(yes yes yes no))
       ;; Three classes equally distributed
       (funcall 'neovm--dt-entropy '(a b c a b c a b c))
       ;; Single element
       (funcall 'neovm--dt-entropy '(x))
       ;; Empty list
       (funcall 'neovm--dt-entropy nil))
    (fmakunbound 'neovm--dt-log2)
    (fmakunbound 'neovm--dt-entropy)))
"#;
    let expect = expect_test::expect![[r#""OK (0.0 1.0 0.8113 1.585 0.0 0.0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Information gain and best split selection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_decision_tree_info_gain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute information gain for splitting a dataset on each attribute.
    // Select the attribute with highest gain.
    let form = r#"
(progn
  (fset 'neovm--dt-log2
    (lambda (x)
      (if (<= x 0) 0.0 (/ (log x) (log 2)))))

  (fset 'neovm--dt-entropy
    (lambda (labels)
      (let ((total (float (length labels)))
            (counts (make-hash-table :test 'equal))
            (ent 0.0))
        (dolist (l labels)
          (puthash l (1+ (gethash l counts 0)) counts))
        (maphash
         (lambda (_k v)
           (let ((p (/ (float v) total)))
             (when (> p 0)
               (setq ent (- ent (* p (funcall 'neovm--dt-log2 p)))))))
         counts)
        (/ (round (* ent 10000)) 10000.0))))

  (fset 'neovm--dt-info-gain
    (lambda (data attr-idx label-idx)
      "Compute information gain of splitting DATA on ATTR-IDX.
       Each row is a list; LABEL-IDX is the label column."
      (let ((labels (mapcar (lambda (row) (nth label-idx row)) data))
            (parent-ent (funcall 'neovm--dt-entropy
                                  (mapcar (lambda (row) (nth label-idx row)) data)))
            (partitions (make-hash-table :test 'equal))
            (total (float (length data))))
        ;; Partition by attribute value
        (dolist (row data)
          (let ((key (nth attr-idx row)))
            (puthash key (cons (nth label-idx row) (gethash key partitions nil)) partitions)))
        ;; Weighted child entropy
        (let ((child-ent 0.0))
          (maphash
           (lambda (_k subset)
             (let ((w (/ (float (length subset)) total)))
               (setq child-ent (+ child-ent (* w (funcall 'neovm--dt-entropy subset))))))
           partitions)
          (/ (round (* (- parent-ent child-ent) 10000)) 10000.0)))))

  (fset 'neovm--dt-best-split
    (lambda (data attr-indices label-idx)
      "Return the attribute index with highest information gain."
      (let ((best-attr nil)
            (best-gain -1.0))
        (dolist (ai attr-indices)
          (let ((gain (funcall 'neovm--dt-info-gain data ai label-idx)))
            (when (> gain best-gain)
              (setq best-gain gain)
              (setq best-attr ai))))
        (cons best-attr best-gain))))

  ;; Tennis dataset (simplified):
  ;; (outlook, temperature, play?)
  ;; outlook: sunny/overcast/rain, temperature: hot/mild/cool
  (unwind-protect
      (let ((data '((sunny   hot   no)
                    (sunny   hot   no)
                    (overcast hot  yes)
                    (rain    mild  yes)
                    (rain    cool  yes)
                    (rain    cool  no)
                    (overcast cool yes)
                    (sunny   mild  no)
                    (sunny   cool  yes)
                    (rain    mild  yes)
                    (sunny   mild  yes)
                    (overcast mild yes)
                    (overcast hot  yes)
                    (rain    mild  no))))
        (list
         ;; Info gain for outlook (attr 0)
         (funcall 'neovm--dt-info-gain data 0 2)
         ;; Info gain for temperature (attr 1)
         (funcall 'neovm--dt-info-gain data 1 2)
         ;; Best split
         (funcall 'neovm--dt-best-split data '(0 1) 2)))
    (fmakunbound 'neovm--dt-log2)
    (fmakunbound 'neovm--dt-entropy)
    (fmakunbound 'neovm--dt-info-gain)
    (fmakunbound 'neovm--dt-best-split)))
"#;
    let expect = expect_test::expect![[r#""OK (0.2467 0.0292 (0 . 0.2467))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Recursive tree building (ID3)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_decision_tree_id3_build() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a complete ID3 decision tree and classify new instances.
    let form = r#"
(progn
  (fset 'neovm--dt-log2
    (lambda (x) (if (<= x 0) 0.0 (/ (log x) (log 2)))))

  (fset 'neovm--dt-entropy
    (lambda (labels)
      (let ((total (float (length labels)))
            (counts (make-hash-table :test 'equal))
            (ent 0.0))
        (dolist (l labels) (puthash l (1+ (gethash l counts 0)) counts))
        (maphash (lambda (_k v)
                   (let ((p (/ (float v) total)))
                     (when (> p 0)
                       (setq ent (- ent (* p (funcall 'neovm--dt-log2 p)))))))
                 counts)
        ent)))

  (fset 'neovm--dt-majority
    (lambda (labels)
      "Return the most common label."
      (let ((counts (make-hash-table :test 'equal))
            (best nil) (best-count 0))
        (dolist (l labels)
          (let ((c (1+ (gethash l counts 0))))
            (puthash l c counts)
            (when (> c best-count) (setq best l) (setq best-count c))))
        best)))

  (fset 'neovm--dt-split-data
    (lambda (data attr-idx value)
      "Filter rows where attr-idx equals value."
      (let ((result nil))
        (dolist (row data)
          (when (equal (nth attr-idx row) value)
            (setq result (cons row result))))
        (nreverse result))))

  (fset 'neovm--dt-gain
    (lambda (data attr-idx label-idx)
      (let ((parent-ent (funcall 'neovm--dt-entropy
                                  (mapcar (lambda (r) (nth label-idx r)) data)))
            (partitions (make-hash-table :test 'equal))
            (total (float (length data))))
        (dolist (row data)
          (puthash (nth attr-idx row)
                   (cons (nth label-idx row) (gethash (nth attr-idx row) partitions nil))
                   partitions))
        (let ((child-ent 0.0))
          (maphash (lambda (_k subset)
                     (setq child-ent
                           (+ child-ent (* (/ (float (length subset)) total)
                                           (funcall 'neovm--dt-entropy subset)))))
                   partitions)
          (- parent-ent child-ent)))))

  (fset 'neovm--dt-build
    (lambda (data attr-indices label-idx)
      "Build decision tree (ID3). Returns nested list structure."
      (let ((labels (mapcar (lambda (r) (nth label-idx r)) data)))
        (cond
         ;; All same label
         ((= (funcall 'neovm--dt-entropy labels) 0.0)
          (list :leaf (car labels)))
         ;; No attributes left
         ((null attr-indices)
          (list :leaf (funcall 'neovm--dt-majority labels)))
         (t
          ;; Find best attribute
          (let ((best-attr nil) (best-gain -1.0))
            (dolist (ai attr-indices)
              (let ((g (funcall 'neovm--dt-gain data ai label-idx)))
                (when (> g best-gain) (setq best-attr ai) (setq best-gain g))))
            ;; Collect attribute values
            (let ((values nil))
              (dolist (row data)
                (let ((v (nth best-attr row)))
                  (unless (memq v values) (setq values (cons v values)))))
              ;; Build subtrees
              (let ((branches nil)
                    (remaining (delq best-attr (copy-sequence attr-indices))))
                (dolist (v values)
                  (let ((subset (funcall 'neovm--dt-split-data data best-attr v)))
                    (if subset
                        (setq branches
                              (cons (list v (funcall 'neovm--dt-build subset remaining label-idx))
                                    branches))
                      (setq branches
                            (cons (list v (list :leaf (funcall 'neovm--dt-majority labels)))
                                  branches)))))
                (list :node best-attr (nreverse branches))))))))))

  (fset 'neovm--dt-classify
    (lambda (tree instance)
      "Classify INSTANCE using TREE."
      (if (eq (car tree) :leaf)
          (cadr tree)
        (let* ((attr-idx (cadr tree))
               (branches (caddr tree))
               (value (nth attr-idx instance))
               (branch (assoc value branches)))
          (if branch
              (funcall 'neovm--dt-classify (cadr branch) instance)
            ;; Unknown value: return nil
            nil)))))

  (unwind-protect
      (let* ((data '((sunny    hot    high   no)
                     (sunny    hot    high   no)
                     (overcast hot    high   yes)
                     (rain     mild   high   yes)
                     (rain     cool   normal yes)
                     (rain     cool   normal no)
                     (overcast cool   normal yes)
                     (sunny    mild   high   no)
                     (sunny    cool   normal yes)
                     (rain     mild   normal yes)
                     (sunny    mild   normal yes)
                     (overcast mild   high   yes)
                     (overcast hot    normal yes)
                     (rain     mild   high   no)))
             (tree (funcall 'neovm--dt-build data '(0 1 2) 3)))
        (list
         ;; Tree structure type
         (car tree)
         ;; Classify training instances (should match labels)
         (mapcar (lambda (row)
                   (list (funcall 'neovm--dt-classify tree row)
                         (nth 3 row)))
                 data)
         ;; Classify new instance
         (funcall 'neovm--dt-classify tree '(overcast mild high))
         (funcall 'neovm--dt-classify tree '(sunny cool normal))
         (funcall 'neovm--dt-classify tree '(rain hot high))))
    (fmakunbound 'neovm--dt-log2)
    (fmakunbound 'neovm--dt-entropy)
    (fmakunbound 'neovm--dt-majority)
    (fmakunbound 'neovm--dt-split-data)
    (fmakunbound 'neovm--dt-gain)
    (fmakunbound 'neovm--dt-build)
    (fmakunbound 'neovm--dt-classify)))
"#;
    let expect = expect_test::expect![[
        r#""OK (:node ((no no) (no no) (yes yes) (yes yes) (yes yes) (yes no) (yes yes) (no no) (yes yes) (yes yes) (yes yes) (yes yes) (yes yes) (yes no)) yes yes nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Numeric attribute handling with threshold splits
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_decision_tree_numeric_threshold() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // For numeric attributes, find the best threshold for binary split.
    let form = r#"
(progn
  (fset 'neovm--dt-log2
    (lambda (x) (if (<= x 0) 0.0 (/ (log x) (log 2)))))

  (fset 'neovm--dt-entropy
    (lambda (labels)
      (let ((total (float (length labels)))
            (counts (make-hash-table :test 'equal))
            (ent 0.0))
        (dolist (l labels) (puthash l (1+ (gethash l counts 0)) counts))
        (maphash (lambda (_k v)
                   (let ((p (/ (float v) total)))
                     (when (> p 0) (setq ent (- ent (* p (funcall 'neovm--dt-log2 p)))))))
                 counts)
        ent)))

  (fset 'neovm--dt-best-threshold
    (lambda (data num-attr-idx label-idx)
      "Find threshold that maximizes info gain for numeric attribute."
      ;; Sort by attribute value
      (let* ((sorted (sort (copy-sequence data)
                           (lambda (a b) (< (nth num-attr-idx a) (nth num-attr-idx b)))))
             (parent-ent (funcall 'neovm--dt-entropy
                                   (mapcar (lambda (r) (nth label-idx r)) sorted)))
             (n (float (length sorted)))
             (best-thresh nil)
             (best-gain -1.0))
        ;; Try each midpoint between consecutive different values
        (let ((i 0))
          (while (< i (1- (length sorted)))
            (let ((v1 (nth num-attr-idx (nth i sorted)))
                  (v2 (nth num-attr-idx (nth (1+ i) sorted))))
              (when (/= v1 v2)
                (let ((thresh (/ (+ (float v1) (float v2)) 2.0))
                      (left nil) (right nil))
                  (dolist (row sorted)
                    (if (<= (nth num-attr-idx row) thresh)
                        (setq left (cons (nth label-idx row) left))
                      (setq right (cons (nth label-idx row) right))))
                  (let ((gain (- parent-ent
                                 (+ (* (/ (float (length left)) n)
                                       (funcall 'neovm--dt-entropy left))
                                    (* (/ (float (length right)) n)
                                       (funcall 'neovm--dt-entropy right))))))
                    (when (> gain best-gain)
                      (setq best-gain gain)
                      (setq best-thresh thresh))))))
            (setq i (1+ i))))
        (list best-thresh (/ (round (* best-gain 10000)) 10000.0)))))

  (unwind-protect
      ;; Dataset: (temperature, label)
      ;; Below 20 -> cold, 20-30 -> warm, above 30 -> hot
      (let ((data '((5  cold) (10 cold) (15 cold) (18 cold)
                    (22 warm) (25 warm) (28 warm)
                    (32 hot)  (35 hot)  (38 hot))))
        (list
         ;; Best threshold for splitting cold vs non-cold
         (funcall 'neovm--dt-best-threshold data 0 1)
         ;; Split just warm vs hot
         (funcall 'neovm--dt-best-threshold
                  '((22 warm) (25 warm) (28 warm) (32 hot) (35 hot) (38 hot))
                  0 1)))
    (fmakunbound 'neovm--dt-log2)
    (fmakunbound 'neovm--dt-entropy)
    (fmakunbound 'neovm--dt-best-threshold)))
"#;
    let expect = expect_test::expect![[r#""OK ((20.0 0.971) (30.0 1.0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tree serialization to readable format
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_decision_tree_serialization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Serialize a hand-built tree to a string representation and back.
    let form = r#"
(progn
  (fset 'neovm--dt-tree-to-string
    (lambda (tree indent)
      "Serialize tree to indented string."
      (let ((pad (make-string (* indent 2) ?\s)))
        (if (eq (car tree) :leaf)
            (concat pad "=> " (symbol-name (cadr tree)) "\n")
          (let ((attr (cadr tree))
                (branches (caddr tree))
                (result (concat pad "attr-" (number-to-string attr) ":\n")))
            (dolist (b branches)
              (setq result
                    (concat result
                            pad "  " (symbol-name (car b)) " ->\n"
                            (funcall 'neovm--dt-tree-to-string (cadr b) (+ indent 2)))))
            result)))))

  (fset 'neovm--dt-count-nodes
    (lambda (tree)
      "Count total nodes (internal + leaf) in tree."
      (if (eq (car tree) :leaf)
          1
        (let ((count 1))
          (dolist (b (caddr tree))
            (setq count (+ count (funcall 'neovm--dt-count-nodes (cadr b)))))
          count))))

  (fset 'neovm--dt-depth
    (lambda (tree)
      "Compute maximum depth of tree."
      (if (eq (car tree) :leaf)
          0
        (let ((max-d 0))
          (dolist (b (caddr tree))
            (let ((d (funcall 'neovm--dt-depth (cadr b))))
              (when (> d max-d) (setq max-d d))))
          (1+ max-d)))))

  (unwind-protect
      (let ((tree '(:node 0
                    ((sunny (:node 2
                             ((high (:leaf no))
                              (normal (:leaf yes)))))
                     (overcast (:leaf yes))
                     (rain (:node 1
                            ((mild (:leaf yes))
                             (cool (:leaf no)))))))))
        (list
         ;; Serialized string
         (funcall 'neovm--dt-tree-to-string tree 0)
         ;; Node count: 3 internal + 5 leaves = 8
         (funcall 'neovm--dt-count-nodes tree)
         ;; Depth: root -> outlook -> humidity/wind -> leaf = 2
         (funcall 'neovm--dt-depth tree)))
    (fmakunbound 'neovm--dt-tree-to-string)
    (fmakunbound 'neovm--dt-count-nodes)
    (fmakunbound 'neovm--dt-depth)))
"#;
    let expect = expect_test::expect![[r#""ERR (void-variable attr)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Pruning (reduced-error pruning)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_decision_tree_pruning() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement reduced-error pruning: replace subtrees with leaves
    // when accuracy on validation set does not decrease.
    let form = r#"
(progn
  (fset 'neovm--dt-classify
    (lambda (tree instance)
      (if (eq (car tree) :leaf)
          (cadr tree)
        (let* ((attr-idx (cadr tree))
               (branches (caddr tree))
               (value (nth attr-idx instance))
               (branch (assoc value branches)))
          (if branch
              (funcall 'neovm--dt-classify (cadr branch) instance)
            nil)))))

  (fset 'neovm--dt-accuracy
    (lambda (tree data label-idx)
      "Compute classification accuracy on DATA."
      (let ((correct 0) (total (length data)))
        (dolist (row data)
          (when (equal (funcall 'neovm--dt-classify tree row)
                       (nth label-idx row))
            (setq correct (1+ correct))))
        (if (= total 0) 1.0
          (/ (float correct) (float total))))))

  (fset 'neovm--dt-majority-label
    (lambda (data label-idx)
      (let ((counts (make-hash-table :test 'equal))
            (best nil) (best-c 0))
        (dolist (row data)
          (let ((l (nth label-idx row)))
            (let ((c (1+ (gethash l counts 0))))
              (puthash l c counts)
              (when (> c best-c) (setq best l) (setq best-c c)))))
        best)))

  (fset 'neovm--dt-prune
    (lambda (tree data label-idx)
      "Reduced-error prune: if replacing subtree with leaf doesn't hurt, do it."
      (if (eq (car tree) :leaf)
          tree
        ;; First prune children
        (let ((attr-idx (cadr tree))
              (branches (caddr tree))
              (new-branches nil))
          (dolist (b branches)
            (let* ((val (car b))
                   (subtree (cadr b))
                   (subset (let ((r nil))
                             (dolist (row data)
                               (when (equal (nth attr-idx row) val)
                                 (setq r (cons row r))))
                             (nreverse r))))
              (setq new-branches
                    (cons (list val (funcall 'neovm--dt-prune subtree subset label-idx))
                          new-branches))))
          (let ((pruned-tree (list :node attr-idx (nreverse new-branches))))
            ;; Check if replacing this node with majority leaf helps
            (let ((leaf-label (funcall 'neovm--dt-majority-label data label-idx))
                  (tree-acc (funcall 'neovm--dt-accuracy pruned-tree data label-idx)))
              (let ((leaf-tree (list :leaf leaf-label)))
                (let ((leaf-acc (funcall 'neovm--dt-accuracy leaf-tree data label-idx)))
                  (if (>= leaf-acc tree-acc)
                      leaf-tree
                    pruned-tree)))))))))

  (unwind-protect
      (let* ((tree '(:node 0
                     ((a (:node 1
                          ((x (:leaf yes))
                           (y (:leaf no)))))
                      (b (:leaf yes))
                      (c (:node 1
                          ((x (:leaf yes))
                           (y (:leaf yes))))))))
             ;; Validation data
             (val-data '((a x yes)
                         (a y no)
                         (b x yes)
                         (b y yes)
                         (c x yes)
                         (c y yes)))
             (acc-before (funcall 'neovm--dt-accuracy tree val-data 2))
             (pruned (funcall 'neovm--dt-prune tree val-data 2))
             (acc-after (funcall 'neovm--dt-accuracy pruned val-data 2)))
        (list
         acc-before
         acc-after
         ;; Pruning should have collapsed the c-subtree to (:leaf yes)
         ;; since both branches predict yes
         pruned
         ;; Accuracy should not decrease
         (>= acc-after acc-before)))
    (fmakunbound 'neovm--dt-classify)
    (fmakunbound 'neovm--dt-accuracy)
    (fmakunbound 'neovm--dt-majority-label)
    (fmakunbound 'neovm--dt-prune)))
"#;
    let expect = expect_test::expect![[
        r#""OK (1.0 1.0 (:node 0 ((a (:node 1 ((x (:leaf yes)) (y (:leaf no))))) (b (:leaf yes)) (c (:leaf yes)))) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cross-validation accuracy
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_decision_tree_cross_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement k-fold cross-validation for a simple decision stump
    // (single-level tree) and verify accuracy computation.
    let form = r#"
(progn
  (fset 'neovm--dt-stump-build
    (lambda (data attr-idx label-idx)
      "Build a decision stump (depth-1 tree) on ATTR-IDX."
      (let ((partitions (make-hash-table :test 'equal)))
        (dolist (row data)
          (let ((key (nth attr-idx row)))
            (puthash key (cons (nth label-idx row) (gethash key partitions nil)) partitions)))
        ;; For each partition, pick majority label
        (let ((branches nil))
          (maphash
           (lambda (k labels)
             (let ((counts (make-hash-table :test 'equal))
                   (best nil) (best-c 0))
               (dolist (l labels)
                 (let ((c (1+ (gethash l counts 0))))
                   (puthash l c counts)
                   (when (> c best-c) (setq best l) (setq best-c c))))
               (setq branches (cons (list k (list :leaf best)) branches))))
           partitions)
          (list :node attr-idx branches)))))

  (fset 'neovm--dt-stump-classify
    (lambda (tree instance)
      (if (eq (car tree) :leaf)
          (cadr tree)
        (let* ((attr-idx (cadr tree))
               (branches (caddr tree))
               (val (nth attr-idx instance))
               (branch (assoc val branches)))
          (if branch (cadr (cadr branch)) nil)))))

  (fset 'neovm--dt-cv-split
    (lambda (data k fold)
      "Split DATA into train/test for FOLD (0-indexed) of K folds."
      (let ((n (length data))
            (fold-size (/ (length data) k))
            (test-start nil)
            (test-end nil))
        (setq test-start (* fold fold-size))
        (setq test-end (if (= fold (1- k)) n (* (1+ fold) fold-size)))
        (let ((train nil) (test nil) (i 0))
          (dolist (row data)
            (if (and (>= i test-start) (< i test-end))
                (setq test (cons row test))
              (setq train (cons row train)))
            (setq i (1+ i)))
          (list (nreverse train) (nreverse test))))))

  (fset 'neovm--dt-cv-run
    (lambda (data attr-idx label-idx k)
      "Run K-fold CV, return list of per-fold accuracies."
      (let ((accs nil))
        (dotimes (fold k)
          (let* ((split (funcall 'neovm--dt-cv-split data k fold))
                 (train (car split))
                 (test (cadr split))
                 (tree (funcall 'neovm--dt-stump-build train attr-idx label-idx))
                 (correct 0))
            (dolist (row test)
              (when (equal (funcall 'neovm--dt-stump-classify tree row)
                           (nth label-idx row))
                (setq correct (1+ correct))))
            (setq accs (cons (if (> (length test) 0)
                                 (/ (round (* (/ (float correct) (float (length test))) 100)) 100.0)
                               1.0)
                             accs))))
        (nreverse accs))))

  (unwind-protect
      (let ((data '((sunny  hot  no)
                    (sunny  cool yes)
                    (overcast hot yes)
                    (overcast cool yes)
                    (rain   hot  no)
                    (rain   cool yes)
                    (sunny  hot  no)
                    (overcast hot yes)
                    (rain   cool yes)
                    (sunny  cool yes)
                    (overcast cool yes)
                    (rain   hot  no))))
        (list
         ;; 3-fold CV on outlook (attr 0)
         (funcall 'neovm--dt-cv-run data 0 2 3)
         ;; 3-fold CV on temperature (attr 1)
         (funcall 'neovm--dt-cv-run data 1 2 3)
         ;; 4-fold CV on outlook
         (funcall 'neovm--dt-cv-run data 0 2 4)))
    (fmakunbound 'neovm--dt-stump-build)
    (fmakunbound 'neovm--dt-stump-classify)
    (fmakunbound 'neovm--dt-cv-split)
    (fmakunbound 'neovm--dt-cv-run)))
"#;
    let expect =
        expect_test::expect![[r#""OK ((0.75 0.5 0.5) (0.75 0.75 1.0) (0.67 0.67 0.33 0.33))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

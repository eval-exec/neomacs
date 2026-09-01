//! Complex algorithmic challenge oracle parity tests: N-Queens, knapsack,
//! longest increasing subsequence, balanced parentheses, Huffman encoding,
//! and shortest path.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// N-Queens solver (backtracking)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algo_challenge_nqueens() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Count the number of solutions to the N-Queens problem for N=4 and N=5.
    // Uses a vector to represent queen placement (queens[row] = col).
    let form = r#"(progn
      (fset 'neovm--nq-safe-p
        (lambda (queens row col)
          (let ((safe t) (i 0))
            (while (and safe (< i row))
              (let ((qcol (aref queens i)))
                (when (or (= qcol col)
                          (= (abs (- qcol col)) (- row i)))
                  (setq safe nil)))
              (setq i (1+ i)))
            safe)))
      (fset 'neovm--nq-solve
        (lambda (n queens row)
          (if (= row n) 1
            (let ((count 0) (col 0))
              (while (< col n)
                (when (funcall 'neovm--nq-safe-p queens row col)
                  (aset queens row col)
                  (setq count (+ count
                                 (funcall 'neovm--nq-solve n queens (1+ row)))))
                (setq col (1+ col)))
              count))))
      (unwind-protect
          (list
           (funcall 'neovm--nq-solve 4 (make-vector 4 -1) 0)
           (funcall 'neovm--nq-solve 5 (make-vector 5 -1) 0)
           (funcall 'neovm--nq-solve 6 (make-vector 6 -1) 0))
        (fmakunbound 'neovm--nq-safe-p)
        (fmakunbound 'neovm--nq-solve)))"#;
    let expect = expect_test::expect![[r#""OK (2 10 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 0/1 Knapsack problem (dynamic programming)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algo_challenge_knapsack() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((weights [2 3 4 5])
                        (values  [3 4 5 6])
                        (capacity 8))
                   (let* ((n (length weights))
                          (dp (make-vector (1+ n) nil)))
                     ;; Initialize dp[i] = vector of 0s length (capacity+1)
                     (dotimes (i (1+ n))
                       (aset dp i (make-vector (1+ capacity) 0)))
                     ;; Fill the DP table
                     (let ((i 1))
                       (while (<= i n)
                         (let ((j 0))
                           (while (<= j capacity)
                             (let ((wi (aref weights (1- i)))
                                   (vi (aref values (1- i))))
                               (if (> wi j)
                                   (aset (aref dp i) j (aref (aref dp (1- i)) j))
                                 (aset (aref dp i) j
                                       (max (aref (aref dp (1- i)) j)
                                            (+ vi (aref (aref dp (1- i)) (- j wi)))))))
                             (setq j (1+ j))))
                         (setq i (1+ i))))
                     ;; Backtrack to find which items were selected
                     (let ((items nil) (i n) (j capacity))
                       (while (> i 0)
                         (when (not (= (aref (aref dp i) j)
                                       (aref (aref dp (1- i)) j)))
                           (setq items (cons i items))
                           (setq j (- j (aref weights (1- i)))))
                         (setq i (1- i)))
                       (list (aref (aref dp n) capacity) items))))"#;
    let expect = expect_test::expect![[r#""OK (10 (2 4))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Longest increasing subsequence (O(n^2) DP)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algo_challenge_lis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((arr [10 9 2 5 3 7 101 18 4 6 8]))
                   (let* ((n (length arr))
                          (dp (make-vector n 1))
                          (parent (make-vector n -1)))
                     ;; Fill DP
                     (let ((i 1))
                       (while (< i n)
                         (let ((j 0))
                           (while (< j i)
                             (when (and (< (aref arr j) (aref arr i))
                                        (> (+ (aref dp j) 1) (aref dp i)))
                               (aset dp i (+ (aref dp j) 1))
                               (aset parent i j))
                             (setq j (1+ j))))
                         (setq i (1+ i))))
                     ;; Find the max length and its index
                     (let ((max-len 0) (max-idx 0) (i 0))
                       (while (< i n)
                         (when (> (aref dp i) max-len)
                           (setq max-len (aref dp i)
                                 max-idx i))
                         (setq i (1+ i)))
                       ;; Reconstruct the subsequence
                       (let ((seq nil) (idx max-idx))
                         (while (>= idx 0)
                           (setq seq (cons (aref arr idx) seq))
                           (setq idx (aref parent idx)))
                         (list max-len seq)))))"#;
    let expect = expect_test::expect![[r#""OK (5 (2 3 4 6 8))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Balanced parentheses generation (Catalan number verification)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algo_challenge_balanced_parens() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generate all balanced parentheses strings for n pairs,
    // verify count matches the Catalan number.
    let form = r#"(progn
      (defvar neovm--bp-results nil)
      (fset 'neovm--bp-gen
        (lambda (n open close current)
          (if (= (length current) (* 2 n))
              (setq neovm--bp-results (cons current neovm--bp-results))
            (progn
              (when (< open n)
                (funcall 'neovm--bp-gen n (1+ open) close (concat current "(")))
              (when (< close open)
                (funcall 'neovm--bp-gen n open (1+ close) (concat current ")")))))))
      ;; Catalan number: C(n) = (2n)! / ((n+1)! * n!)
      (fset 'neovm--bp-fact
        (lambda (n)
          (if (<= n 1) 1 (* n (funcall 'neovm--bp-fact (1- n))))))
      (fset 'neovm--bp-catalan
        (lambda (n)
          (/ (funcall 'neovm--bp-fact (* 2 n))
             (* (funcall 'neovm--bp-fact (1+ n))
                (funcall 'neovm--bp-fact n)))))
      (unwind-protect
          (list
           ;; n=1: ()
           (progn (setq neovm--bp-results nil)
                  (funcall 'neovm--bp-gen 1 0 0 "")
                  (list (length neovm--bp-results)
                        (= (length neovm--bp-results) (funcall 'neovm--bp-catalan 1))
                        (sort neovm--bp-results #'string<)))
           ;; n=2: ()(), (())
           (progn (setq neovm--bp-results nil)
                  (funcall 'neovm--bp-gen 2 0 0 "")
                  (list (length neovm--bp-results)
                        (= (length neovm--bp-results) (funcall 'neovm--bp-catalan 2))
                        (sort neovm--bp-results #'string<)))
           ;; n=3: 5 solutions
           (progn (setq neovm--bp-results nil)
                  (funcall 'neovm--bp-gen 3 0 0 "")
                  (list (length neovm--bp-results)
                        (= (length neovm--bp-results) (funcall 'neovm--bp-catalan 3))
                        (sort neovm--bp-results #'string<)))
           ;; n=4: 14 solutions
           (progn (setq neovm--bp-results nil)
                  (funcall 'neovm--bp-gen 4 0 0 "")
                  (list (length neovm--bp-results)
                        (= (length neovm--bp-results) (funcall 'neovm--bp-catalan 4)))))
        (fmakunbound 'neovm--bp-gen)
        (fmakunbound 'neovm--bp-fact)
        (fmakunbound 'neovm--bp-catalan)
        (makunbound 'neovm--bp-results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 t (\"()\")) (2 t (\"(())\" \"()()\")) (5 t (\"((()))\" \"(()())\" \"(())()\" \"()(())\" \"()()()\")) (14 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Huffman encoding tree construction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algo_challenge_huffman() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a Huffman tree from character frequencies and extract codes.
    // Priority queue via sorted insertion into a list.
    let form = r#"(progn
      ;; Insert node into sorted list (by frequency, ascending)
      (fset 'neovm--huf-insert
        (lambda (node queue)
          (if (or (null queue) (< (car node) (caar queue)))
              (cons node queue)
            (cons (car queue) (funcall 'neovm--huf-insert node (cdr queue))))))
      ;; Build Huffman tree: queue is list of (freq . data)
      ;; leaf: (freq . char), internal: (freq left . right)
      (fset 'neovm--huf-build
        (lambda (queue)
          (if (null (cdr queue))
              (car queue)
            (let* ((a (car queue))
                   (b (cadr queue))
                   (rest (cddr queue))
                   (merged (cons (+ (car a) (car b)) (cons a b))))
              (funcall 'neovm--huf-build
                       (funcall 'neovm--huf-insert merged rest))))))
      ;; Extract codes: returns list of (char . code-string)
      (fset 'neovm--huf-codes
        (lambda (tree prefix)
          (cond
           ;; leaf node: (freq . char)
           ((and (numberp (car tree)) (characterp (cdr tree)))
            (list (cons (cdr tree) prefix)))
           ;; internal node: (freq left . right)
           (t
            (let ((left (cadr tree))
                  (right (cddr tree)))
              (append
               (funcall 'neovm--huf-codes left (concat prefix "0"))
               (funcall 'neovm--huf-codes right (concat prefix "1"))))))))
      (unwind-protect
          (let* ((freqs '((45 . ?a) (13 . ?b) (12 . ?c) (16 . ?d) (9 . ?e) (5 . ?f)))
                 (tree (funcall 'neovm--huf-build freqs))
                 (codes (funcall 'neovm--huf-codes tree "")))
            ;; Sort codes by character for deterministic output
            (let ((sorted (sort codes (lambda (a b) (< (car a) (car b))))))
              ;; Return code lengths and total weighted path length
              (let ((total 0))
                (dolist (c sorted)
                  (let ((freq (car (assq (car c) (mapcar (lambda (f) (cons (cdr f) (car f))) freqs)))))
                    (setq total (+ total (* freq (length (cdr c)))))))
                (list (mapcar (lambda (c) (cons (car c) (length (cdr c)))) sorted)
                      total))))
        (fmakunbound 'neovm--huf-insert)
        (fmakunbound 'neovm--huf-build)
        (fmakunbound 'neovm--huf-codes)))"#;
    let expect = expect_test::expect![[
        r#""OK (((97 . 2) (98 . 2) (99 . 3) (100 . 3) (101 . 3) (102 . 3)) 1596)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dijkstra-like shortest path
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algo_challenge_shortest_path() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Dijkstra's algorithm using alists for adjacency and a sorted
    // priority queue.
    let form = r#"(progn
      ;; Graph as alist of alists: (node . ((neighbor . weight) ...))
      ;; Insert into priority queue sorted by distance
      (fset 'neovm--dj-pq-insert
        (lambda (item queue)
          (if (or (null queue) (< (cdr item) (cdar queue)))
              (cons item queue)
            (cons (car queue)
                  (funcall 'neovm--dj-pq-insert item (cdr queue))))))
      (fset 'neovm--dj-shortest
        (lambda (graph start)
          (let ((dist (list (cons start 0)))
                (visited nil)
                (pq (list (cons start 0))))
            (while pq
              (let* ((current (car pq))
                     (node (car current))
                     (d (cdr current)))
                (setq pq (cdr pq))
                (unless (memq node visited)
                  (setq visited (cons node visited))
                  ;; Relax neighbors
                  (let ((neighbors (cdr (assq node graph))))
                    (dolist (edge neighbors)
                      (let* ((nbr (car edge))
                             (w (cdr edge))
                             (new-dist (+ d w))
                             (old (cdr (assq nbr dist))))
                        (when (or (null old) (< new-dist old))
                          (setq dist
                                (cons (cons nbr new-dist)
                                      (assq-delete-all nbr dist)))
                          (setq pq
                                (funcall 'neovm--dj-pq-insert
                                         (cons nbr new-dist) pq)))))))))
            dist)))
      (unwind-protect
          (let ((graph '((A (B . 1) (C . 4))
                         (B (A . 1) (C . 2) (D . 5))
                         (C (A . 4) (B . 2) (D . 1))
                         (D (B . 5) (C . 1) (E . 3))
                         (E (D . 3)))))
            (let ((result (funcall 'neovm--dj-shortest graph 'A)))
              ;; Sort by node symbol for deterministic output
              (sort result (lambda (a b) (string< (symbol-name (car a))
                                                  (symbol-name (car b)))))))
        (fmakunbound 'neovm--dj-pq-insert)
        (fmakunbound 'neovm--dj-shortest)))"#;
    let expect = expect_test::expect![[r#""OK ((A . 0) (B . 1) (C . 3) (D . 4) (E . 7))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Topological sort (Kahn's algorithm)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algo_challenge_topological_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((edges '((A . B) (A . C) (B . D) (C . D) (D . E) (B . E))))
      ;; Build adjacency list and in-degree count
      (let ((adj nil) (in-deg nil) (nodes nil))
        ;; Collect all nodes
        (dolist (e edges)
          (unless (memq (car e) nodes) (setq nodes (cons (car e) nodes)))
          (unless (memq (cdr e) nodes) (setq nodes (cons (cdr e) nodes))))
        ;; Initialize in-degree to 0
        (dolist (n nodes)
          (setq in-deg (cons (cons n 0) in-deg))
          (setq adj (cons (cons n nil) adj)))
        ;; Fill adjacency and in-degree
        (dolist (e edges)
          (let ((from (car e)) (to (cdr e)))
            (let ((a (assq from adj)))
              (setcdr a (cons to (cdr a))))
            (let ((d (assq to in-deg)))
              (setcdr d (1+ (cdr d))))))
        ;; Kahn's algorithm: queue starts with nodes of in-degree 0
        (let ((queue nil) (result nil))
          ;; Use sorted initial queue for deterministic output
          (dolist (n nodes)
            (when (= (cdr (assq n in-deg)) 0)
              (setq queue (cons n queue))))
          (setq queue (sort queue (lambda (a b) (string< (symbol-name a) (symbol-name b)))))
          (while queue
            (let ((node (car queue)))
              (setq queue (cdr queue))
              (setq result (cons node result))
              ;; Decrease in-degree of neighbors
              (dolist (nbr (cdr (assq node adj)))
                (let ((d (assq nbr in-deg)))
                  (setcdr d (1- (cdr d)))
                  (when (= (cdr d) 0)
                    ;; Insert in sorted position for determinism
                    (let ((name (symbol-name nbr))
                          (inserted nil)
                          (new-q nil))
                      (dolist (q queue)
                        (when (and (not inserted)
                                   (string< name (symbol-name q)))
                          (setq new-q (cons nbr new-q)
                                inserted t))
                        (setq new-q (cons q new-q)))
                      (unless inserted
                        (setq new-q (cons nbr new-q)))
                      (setq queue (nreverse new-q))))))))
          (nreverse result))))"#;
    let expect = expect_test::expect![[r#""OK (A B C D E)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

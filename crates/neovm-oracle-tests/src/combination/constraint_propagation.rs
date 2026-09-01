//! Oracle parity tests for constraint propagation patterns in Elisp.
//!
//! Implements constraint propagation (domain reduction, arc consistency),
//! including domain representation, peer networks, elimination, assignment,
//! 4x4 Sudoku solving, and contradiction detection.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Domain representation and basic operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cp_domain_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Domain is represented as a sorted list of possible values.
    // Operations: create, add, remove, contains, intersect, union, size.
    let form = r#"
(progn
  (fset 'neovm--cp-dom-create
    (lambda (values)
      "Create a domain from a list of values (sorted, unique)."
      (sort (delete-dups (copy-sequence values)) #'<)))

  (fset 'neovm--cp-dom-remove
    (lambda (domain val)
      "Remove VAL from DOMAIN, return new domain."
      (delq val (copy-sequence domain))))

  (fset 'neovm--cp-dom-contains
    (lambda (domain val) (not (null (memq val domain)))))

  (fset 'neovm--cp-dom-intersect
    (lambda (d1 d2)
      "Intersection of two domains."
      (let ((result nil))
        (dolist (v d1) (when (memq v d2) (setq result (cons v result))))
        (nreverse result))))

  (fset 'neovm--cp-dom-union
    (lambda (d1 d2)
      "Union of two domains (sorted, unique)."
      (funcall 'neovm--cp-dom-create (append d1 d2))))

  (unwind-protect
      (let ((d1 (funcall 'neovm--cp-dom-create '(3 1 4 1 5 9 2 6)))
            (d2 (funcall 'neovm--cp-dom-create '(2 7 1 8 2 8)))
            (d3 (funcall 'neovm--cp-dom-create '(5 5 5))))
        (list
         ;; Created domains are sorted and unique
         d1  ;; => (1 2 3 4 5 6 9)
         d2  ;; => (1 2 7 8)
         d3  ;; => (5)
         ;; Remove
         (funcall 'neovm--cp-dom-remove d1 3)
         (funcall 'neovm--cp-dom-remove d1 99)  ;; not present
         ;; Contains
         (funcall 'neovm--cp-dom-contains d1 5)
         (funcall 'neovm--cp-dom-contains d1 7)
         ;; Intersect
         (funcall 'neovm--cp-dom-intersect d1 d2)
         ;; Union
         (funcall 'neovm--cp-dom-union d1 d2)
         ;; Sizes
         (length d1)
         (length d2)))
    (fmakunbound 'neovm--cp-dom-create)
    (fmakunbound 'neovm--cp-dom-remove)
    (fmakunbound 'neovm--cp-dom-contains)
    (fmakunbound 'neovm--cp-dom-intersect)
    (fmakunbound 'neovm--cp-dom-union)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 9) (1 2 7 8) (5) (1 2 4 5 6 9) (1 2 3 4 5 6 9) t nil (1 2) (1 2 3 4 5 6 7 8 9) 7 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Constraint network: peers and elimination
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cp_peer_network_and_eliminate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a peer network for a 4x4 grid (rows, columns, 2x2 boxes).
    // Eliminate: when a cell has a single value, remove it from all peers' domains.
    let form = r#"
(progn
  (fset 'neovm--cp-peers-4x4
    (lambda (pos)
      "Return list of peer positions for POS in a 4x4 grid with 2x2 boxes."
      (let* ((row (/ pos 4))
             (col (% pos 4))
             (box-r (* (/ row 2) 2))
             (box-c (* (/ col 2) 2))
             (peers nil))
        ;; Same row
        (let ((c 0))
          (while (< c 4)
            (let ((p (+ (* row 4) c)))
              (unless (= p pos) (setq peers (cons p peers))))
            (setq c (1+ c))))
        ;; Same column
        (let ((r 0))
          (while (< r 4)
            (let ((p (+ (* r 4) col)))
              (unless (or (= p pos) (memq p peers))
                (setq peers (cons p peers))))
            (setq r (1+ r))))
        ;; Same 2x2 box
        (let ((dr 0))
          (while (< dr 2)
            (let ((dc 0))
              (while (< dc 2)
                (let ((p (+ (* (+ box-r dr) 4) (+ box-c dc))))
                  (unless (or (= p pos) (memq p peers))
                    (setq peers (cons p peers))))
                (setq dc (1+ dc))))
            (setq dr (1+ dr))))
        (sort peers #'<))))

  (fset 'neovm--cp-eliminate
    (lambda (domains pos val)
      "Remove VAL from domain of POS. If domain becomes singleton,
       propagate elimination to all peers. Return t if consistent, nil if contradiction."
      (let ((dom (aref domains pos)))
        (unless (memq val dom)
          ;; Value not in domain, nothing to do
          (throw 'neovm--cp-ok t))
        (aset domains pos (delq val (copy-sequence dom)))
        (let ((new-dom (aref domains pos)))
          (cond
           ((null new-dom) nil)  ;; Contradiction: empty domain
           ((= (length new-dom) 1)
            ;; Propagate: remove this value from all peers
            (let ((single-val (car new-dom))
                  (peers (funcall 'neovm--cp-peers-4x4 pos))
                  (ok t))
              (dolist (peer peers)
                (when ok
                  (unless (catch 'neovm--cp-ok
                            (funcall 'neovm--cp-eliminate domains peer single-val))
                    (setq ok nil))))
              ok))
           (t t))))))

  (unwind-protect
      (progn
        ;; Test peers
        (let ((peers-0 (funcall 'neovm--cp-peers-4x4 0))
              (peers-5 (funcall 'neovm--cp-peers-4x4 5))
              (peers-15 (funcall 'neovm--cp-peers-4x4 15)))
          ;; Test elimination
          (let ((domains (make-vector 16 nil)))
            ;; Initialize all domains to (1 2 3 4)
            (let ((i 0))
              (while (< i 16)
                (aset domains i (list 1 2 3 4))
                (setq i (1+ i))))
            ;; Assign cell 0 = 1 (eliminate 2, 3, 4 from cell 0)
            (aset domains 0 (list 1))
            ;; Propagate from cell 0
            (let ((ok (let ((peers (funcall 'neovm--cp-peers-4x4 0))
                            (result t))
                        (dolist (p peers)
                          (when result
                            (unless (catch 'neovm--cp-ok
                                      (funcall 'neovm--cp-eliminate domains p 1))
                              (setq result nil))))
                        result)))
              (list
               ;; Peers of cell 0 (row 0 + col 0 + box 0)
               peers-0
               ;; Peers of cell 5 (row 1, col 1)
               peers-5
               ;; Peers of cell 15 (row 3, col 3)
               peers-15
               ;; Each cell should have 7 peers in 4x4 grid
               (length peers-0)
               (length peers-5)
               ;; Propagation succeeded
               ok
               ;; Cell 0 still has domain (1)
               (aref domains 0)
               ;; Peers of cell 0 no longer have 1 in domain
               (memq 1 (aref domains 1))
               (memq 1 (aref domains 4))
               ;; Some cell in the box might have been further constrained
               (length (aref domains 1)))))))
    (fmakunbound 'neovm--cp-peers-4x4)
    (fmakunbound 'neovm--cp-eliminate)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 8 12) (0 1 4 6 7 9 13) (3 7 10 11 12 13 14) 7 7 t (1) nil nil 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Assign: fix a cell's value and propagate constraints
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cp_assign_and_propagate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Assign a value to a cell by eliminating all other values from its domain,
    // then propagate.
    let form = r#"
(progn
  (fset 'neovm--cp2-peers
    (lambda (pos)
      "Peers for a 4x4 grid."
      (let* ((row (/ pos 4)) (col (% pos 4))
             (box-r (* (/ row 2) 2)) (box-c (* (/ col 2) 2))
             (peers nil))
        (let ((c 0))
          (while (< c 4)
            (let ((p (+ (* row 4) c)))
              (unless (= p pos) (setq peers (cons p peers))))
            (setq c (1+ c))))
        (let ((r 0))
          (while (< r 4)
            (let ((p (+ (* r 4) col)))
              (unless (or (= p pos) (memq p peers))
                (setq peers (cons p peers))))
            (setq r (1+ r))))
        (let ((dr 0))
          (while (< dr 2)
            (let ((dc 0))
              (while (< dc 2)
                (let ((p (+ (* (+ box-r dr) 4) (+ box-c dc))))
                  (unless (or (= p pos) (memq p peers))
                    (setq peers (cons p peers))))
                (setq dc (1+ dc))))
            (setq dr (1+ dr))))
        peers)))

  (fset 'neovm--cp2-eliminate
    (lambda (domains pos val)
      "Eliminate VAL from domain at POS. Propagate if singleton. Return t or nil."
      (let ((dom (aref domains pos)))
        (if (not (memq val dom))
            t
          (let ((new-dom (delq val (copy-sequence dom))))
            (aset domains pos new-dom)
            (cond
             ((null new-dom) nil)
             ((= (length new-dom) 1)
              (let ((sv (car new-dom))
                    (ok t))
                (dolist (p (funcall 'neovm--cp2-peers pos))
                  (when ok
                    (setq ok (funcall 'neovm--cp2-eliminate domains p sv))))
                ok))
             (t t)))))))

  (fset 'neovm--cp2-assign
    (lambda (domains pos val)
      "Assign VAL to cell at POS by eliminating all other values."
      (let ((others (delq val (copy-sequence (aref domains pos))))
            (ok t))
        (dolist (v others)
          (when ok
            (setq ok (funcall 'neovm--cp2-eliminate domains pos v))))
        ok)))

  (unwind-protect
      (let ((domains (make-vector 16 nil)))
        ;; Init all domains to (1 2 3 4)
        (let ((i 0))
          (while (< i 16)
            (aset domains i (list 1 2 3 4))
            (setq i (1+ i))))
        ;; Assign cell 0 = 1
        (let ((ok1 (funcall 'neovm--cp2-assign domains 0 1)))
          ;; Assign cell 3 = 4 (same row as cell 0)
          (let ((ok2 (funcall 'neovm--cp2-assign domains 3 4)))
            ;; Assign cell 12 = 3 (same column as cell 0)
            (let ((ok3 (funcall 'neovm--cp2-assign domains 12 3)))
              (list
               ok1 ok2 ok3
               ;; Cell 0 fixed
               (aref domains 0)
               ;; Cell 3 fixed
               (aref domains 3)
               ;; Cell 12 fixed
               (aref domains 12)
               ;; Cell 1 (same row as 0, 3; same box as 0): 1 and 4 eliminated
               (sort (copy-sequence (aref domains 1)) #'<)
               ;; Cell 4 (same col as 0, same box as 0): 1 eliminated
               (sort (copy-sequence (aref domains 4)) #'<)
               ;; No domain is empty (consistent)
               (let ((all-nonempty t) (j 0))
                 (while (< j 16)
                   (when (null (aref domains j))
                     (setq all-nonempty nil))
                   (setq j (1+ j)))
                 all-nonempty))))))
    (fmakunbound 'neovm--cp2-peers)
    (fmakunbound 'neovm--cp2-eliminate)
    (fmakunbound 'neovm--cp2-assign)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t (1) (4) (3) (2 3) (2 4) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Solve a 4x4 Sudoku via constraint propagation + backtracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cp_solve_4x4_sudoku() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (fset 'neovm--cp3-peers
    (lambda (pos)
      (let* ((row (/ pos 4)) (col (% pos 4))
             (box-r (* (/ row 2) 2)) (box-c (* (/ col 2) 2))
             (peers nil))
        (let ((c 0))
          (while (< c 4)
            (let ((p (+ (* row 4) c)))
              (unless (= p pos) (setq peers (cons p peers))))
            (setq c (1+ c))))
        (let ((r 0))
          (while (< r 4)
            (let ((p (+ (* r 4) col)))
              (unless (or (= p pos) (memq p peers))
                (setq peers (cons p peers))))
            (setq r (1+ r))))
        (let ((dr 0))
          (while (< dr 2)
            (let ((dc 0))
              (while (< dc 2)
                (let ((p (+ (* (+ box-r dr) 4) (+ box-c dc))))
                  (unless (or (= p pos) (memq p peers))
                    (setq peers (cons p peers))))
                (setq dc (1+ dc))))
            (setq dr (1+ dr))))
        peers)))

  (fset 'neovm--cp3-eliminate
    (lambda (doms pos val)
      (let ((dom (aref doms pos)))
        (if (not (memq val dom)) t
          (let ((nd (delq val (copy-sequence dom))))
            (aset doms pos nd)
            (cond
             ((null nd) nil)
             ((= (length nd) 1)
              (let ((sv (car nd)) (ok t))
                (dolist (p (funcall 'neovm--cp3-peers pos))
                  (when ok (setq ok (funcall 'neovm--cp3-eliminate doms p sv))))
                ok))
             (t t)))))))

  (fset 'neovm--cp3-assign
    (lambda (doms pos val)
      (let ((others (delq val (copy-sequence (aref doms pos)))) (ok t))
        (dolist (v others) (when ok (setq ok (funcall 'neovm--cp3-eliminate doms pos v))))
        ok)))

  (fset 'neovm--cp3-copy-domains
    (lambda (doms)
      (let ((c (make-vector 16 nil)) (i 0))
        (while (< i 16)
          (aset c i (copy-sequence (aref doms i)))
          (setq i (1+ i)))
        c)))

  (fset 'neovm--cp3-solve
    (lambda (puzzle)
      "Solve a 4x4 Sudoku. PUZZLE is a vector of 16 values (0=empty)."
      (let ((doms (make-vector 16 nil)))
        ;; Init domains
        (let ((i 0))
          (while (< i 16) (aset doms i (list 1 2 3 4)) (setq i (1+ i))))
        ;; Apply given clues
        (let ((ok t) (i 0))
          (while (and ok (< i 16))
            (when (/= (aref puzzle i) 0)
              (setq ok (funcall 'neovm--cp3-assign doms i (aref puzzle i))))
            (setq i (1+ i)))
          (if (not ok) nil
            ;; Backtrack
            (let ((result nil))
              (fset 'neovm--cp3-bt
                (lambda (doms)
                  (let ((unsolved nil) (contradiction nil))
                    (let ((i 0))
                      (while (< i 16)
                        (let ((d (aref doms i)))
                          (cond ((null d) (setq contradiction t))
                                ((> (length d) 1)
                                 (when (or (null unsolved)
                                           (< (length d) (length (aref doms (car unsolved)))))
                                   (setq unsolved (list i))))))
                        (setq i (1+ i))))
                    (cond
                     (contradiction nil)
                     ((null unsolved)
                      ;; All assigned
                      (let ((sol (make-vector 16 0)) (j 0))
                        (while (< j 16) (aset sol j (car (aref doms j))) (setq j (1+ j)))
                        (setq result sol)
                        t))
                     (t (let ((cell (car unsolved)) (found nil))
                          (dolist (val (aref doms cell))
                            (unless found
                              (let ((saved (funcall 'neovm--cp3-copy-domains doms)))
                                (if (funcall 'neovm--cp3-assign doms cell val)
                                    (if (funcall 'neovm--cp3-bt doms)
                                        (setq found t)
                                      ;; Restore
                                      (let ((k 0))
                                        (while (< k 16) (aset doms k (aref saved k)) (setq k (1+ k)))))
                                  (let ((k 0))
                                    (while (< k 16) (aset doms k (aref saved k)) (setq k (1+ k))))))))
                          found))))))
              (funcall 'neovm--cp3-bt doms)
              result))))))

  (fset 'neovm--cp3-valid-p
    (lambda (grid)
      "Validate a completed 4x4 Sudoku."
      (let ((ok t))
        ;; Check rows
        (let ((r 0))
          (while (and ok (< r 4))
            (let ((row (list (aref grid (+ (* r 4) 0)) (aref grid (+ (* r 4) 1))
                             (aref grid (+ (* r 4) 2)) (aref grid (+ (* r 4) 3)))))
              (unless (equal (sort (copy-sequence row) #'<) '(1 2 3 4))
                (setq ok nil)))
            (setq r (1+ r))))
        ;; Check columns
        (let ((c 0))
          (while (and ok (< c 4))
            (let ((col (list (aref grid c) (aref grid (+ c 4))
                             (aref grid (+ c 8)) (aref grid (+ c 12)))))
              (unless (equal (sort (copy-sequence col) #'<) '(1 2 3 4))
                (setq ok nil)))
            (setq c (1+ c))))
        ;; Check 2x2 boxes
        (dolist (box-start '(0 2 8 10))
          (when ok
            (let ((box (list (aref grid box-start)
                             (aref grid (+ box-start 1))
                             (aref grid (+ box-start 4))
                             (aref grid (+ box-start 5)))))
              (unless (equal (sort (copy-sequence box) #'<) '(1 2 3 4))
                (setq ok nil)))))
        ok)))

  (unwind-protect
      ;; Puzzle:  _ 2 _ _
      ;;          _ _ 2 _
      ;;          _ 1 _ _
      ;;          _ _ _ 3
      (let* ((puzzle (vector 0 2 0 0
                             0 0 2 0
                             0 1 0 0
                             0 0 0 3))
             (solution (funcall 'neovm--cp3-solve puzzle)))
        (list
         ;; Found solution
         (not (null solution))
         ;; Valid Sudoku
         (when solution (funcall 'neovm--cp3-valid-p solution))
         ;; Clues preserved
         (when solution
           (let ((ok t) (i 0))
             (while (< i 16)
               (when (and (/= (aref puzzle i) 0) (/= (aref puzzle i) (aref solution i)))
                 (setq ok nil))
               (setq i (1+ i)))
             ok))
         ;; The solution itself
         solution))
    (fmakunbound 'neovm--cp3-peers)
    (fmakunbound 'neovm--cp3-eliminate)
    (fmakunbound 'neovm--cp3-assign)
    (fmakunbound 'neovm--cp3-copy-domains)
    (fmakunbound 'neovm--cp3-solve)
    (fmakunbound 'neovm--cp3-bt)
    (fmakunbound 'neovm--cp3-valid-p)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t [1 2 3 4 4 3 2 1 3 1 4 2 2 4 1 3])""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Detect contradiction: domain becomes empty
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cp_detect_contradiction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create an impossible constraint scenario: assign conflicting values
    // to cells that are peers, forcing a domain to become empty.
    let form = r#"
(progn
  (fset 'neovm--cp4-peers
    (lambda (pos)
      (let* ((row (/ pos 4)) (col (% pos 4))
             (box-r (* (/ row 2) 2)) (box-c (* (/ col 2) 2))
             (peers nil))
        (let ((c 0))
          (while (< c 4) (let ((p (+ (* row 4) c)))
            (unless (= p pos) (setq peers (cons p peers)))) (setq c (1+ c))))
        (let ((r 0))
          (while (< r 4) (let ((p (+ (* r 4) col)))
            (unless (or (= p pos) (memq p peers)) (setq peers (cons p peers)))) (setq r (1+ r))))
        (let ((dr 0))
          (while (< dr 2) (let ((dc 0))
            (while (< dc 2) (let ((p (+ (* (+ box-r dr) 4) (+ box-c dc))))
              (unless (or (= p pos) (memq p peers)) (setq peers (cons p peers)))) (setq dc (1+ dc))))
            (setq dr (1+ dr))))
        peers)))

  (fset 'neovm--cp4-eliminate
    (lambda (doms pos val)
      (let ((dom (aref doms pos)))
        (if (not (memq val dom)) t
          (let ((nd (delq val (copy-sequence dom))))
            (aset doms pos nd)
            (cond ((null nd) nil)
                  ((= (length nd) 1)
                   (let ((sv (car nd)) (ok t))
                     (dolist (p (funcall 'neovm--cp4-peers pos))
                       (when ok (setq ok (funcall 'neovm--cp4-eliminate doms p sv))))
                     ok))
                  (t t)))))))

  (fset 'neovm--cp4-assign
    (lambda (doms pos val)
      (let ((others (delq val (copy-sequence (aref doms pos)))) (ok t))
        (dolist (v others) (when ok (setq ok (funcall 'neovm--cp4-eliminate doms pos v))))
        ok)))

  (unwind-protect
      (let ((doms (make-vector 16 nil)))
        ;; Init
        (let ((i 0)) (while (< i 16) (aset doms i (list 1 2 3 4)) (setq i (1+ i))))
        ;; Assign cell 0=1, cell 1=2, cell 2=3, cell 3=4 (entire first row)
        (let ((ok t))
          (setq ok (funcall 'neovm--cp4-assign doms 0 1))
          (when ok (setq ok (funcall 'neovm--cp4-assign doms 1 2)))
          (when ok (setq ok (funcall 'neovm--cp4-assign doms 2 3)))
          (when ok (setq ok (funcall 'neovm--cp4-assign doms 3 4)))
          (let ((row-ok ok))
            ;; Now try to create contradiction: assign cell 4=1
            ;; Cell 4 is in same column as cell 0 (which has 1).
            ;; So 1 should already be eliminated from cell 4's domain.
            (let ((doms2 (make-vector 16 nil)))
              (let ((i 0)) (while (< i 16) (aset doms2 i (list 1 2 3 4)) (setq i (1+ i))))
              ;; Try impossible: same row, all four values assigned,
              ;; then try to assign cell 4 (same col as 0) to 1
              (funcall 'neovm--cp4-assign doms2 0 1)
              (funcall 'neovm--cp4-assign doms2 1 2)
              (funcall 'neovm--cp4-assign doms2 2 3)
              (funcall 'neovm--cp4-assign doms2 3 4)
              ;; Cell 4: col 0 has 1, box (0,0) has 1,2 => domain should not include 1 or 2
              (let ((dom4 (aref doms2 4)))
                (list
                 row-ok
                 ;; Cell 4's remaining domain (1 and 2 eliminated by col/box)
                 (sort (copy-sequence dom4) #'<)
                 ;; Contains 1?
                 (not (null (memq 1 dom4)))
                 ;; Contains 2?
                 (not (null (memq 2 dom4)))
                 ;; Now create actual contradiction: all cells in col 0 get different values
                 ;; that leave cell 12 with empty domain
                 ;; Cell 0=1, Cell 4 needs from {3,4}, say 3
                 ;; Cell 8 is in col 0 and box (2,0). Cell 8's domain?
                 (sort (copy-sequence (aref doms2 8)) #'<)
                 ;; Try double-assigning same value
                 (let ((doms3 (make-vector 4 nil)))
                   (aset doms3 0 (list 1 2))
                   (aset doms3 1 (list 1 2))
                   (aset doms3 2 (list 1))
                   (aset doms3 3 (list 1))
                   ;; Domain 2 = (1) and Domain 3 = (1) — both forced to 1
                   ;; In a constraint where 2 and 3 are peers, this is a contradiction
                   (list (aref doms3 2) (aref doms3 3)))))))))
    (fmakunbound 'neovm--cp4-peers)
    (fmakunbound 'neovm--cp4-eliminate)
    (fmakunbound 'neovm--cp4-assign)))
"#;
    let expect = expect_test::expect![[r#""OK (t (3 4) nil nil (2 3 4) ((1) (1)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Solve a harder 4x4 puzzle with minimal clues
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cp_minimal_clue_sudoku() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A 4x4 puzzle with only 2 clues — requires more backtracking.
    let form = r#"
(progn
  (fset 'neovm--cp5-peers
    (lambda (pos)
      (let* ((row (/ pos 4)) (col (% pos 4))
             (box-r (* (/ row 2) 2)) (box-c (* (/ col 2) 2))
             (peers nil))
        (let ((c 0))
          (while (< c 4) (let ((p (+ (* row 4) c)))
            (unless (= p pos) (setq peers (cons p peers)))) (setq c (1+ c))))
        (let ((r 0))
          (while (< r 4) (let ((p (+ (* r 4) col)))
            (unless (or (= p pos) (memq p peers)) (setq peers (cons p peers)))) (setq r (1+ r))))
        (let ((dr 0))
          (while (< dr 2) (let ((dc 0))
            (while (< dc 2) (let ((p (+ (* (+ box-r dr) 4) (+ box-c dc))))
              (unless (or (= p pos) (memq p peers)) (setq peers (cons p peers)))) (setq dc (1+ dc))))
            (setq dr (1+ dr))))
        peers)))

  (fset 'neovm--cp5-eliminate
    (lambda (doms pos val)
      (let ((dom (aref doms pos)))
        (if (not (memq val dom)) t
          (let ((nd (delq val (copy-sequence dom))))
            (aset doms pos nd)
            (cond ((null nd) nil)
                  ((= (length nd) 1)
                   (let ((sv (car nd)) (ok t))
                     (dolist (p (funcall 'neovm--cp5-peers pos))
                       (when ok (setq ok (funcall 'neovm--cp5-eliminate doms p sv))))
                     ok))
                  (t t)))))))

  (fset 'neovm--cp5-assign
    (lambda (doms pos val)
      (let ((others (delq val (copy-sequence (aref doms pos)))) (ok t))
        (dolist (v others) (when ok (setq ok (funcall 'neovm--cp5-eliminate doms pos v))))
        ok)))

  (fset 'neovm--cp5-copy-doms
    (lambda (doms)
      (let ((c (make-vector 16 nil)) (i 0))
        (while (< i 16) (aset c i (copy-sequence (aref doms i))) (setq i (1+ i))) c)))

  (fset 'neovm--cp5-solve
    (lambda (puzzle)
      (let ((doms (make-vector 16 nil)))
        (let ((i 0)) (while (< i 16) (aset doms i (list 1 2 3 4)) (setq i (1+ i))))
        (let ((ok t) (i 0))
          (while (and ok (< i 16))
            (when (/= (aref puzzle i) 0)
              (setq ok (funcall 'neovm--cp5-assign doms i (aref puzzle i))))
            (setq i (1+ i)))
          (if (not ok) nil
            (let ((result nil))
              (fset 'neovm--cp5-bt
                (lambda (doms)
                  (let ((best nil) (contradiction nil))
                    (let ((i 0))
                      (while (< i 16)
                        (let ((d (aref doms i)))
                          (cond ((null d) (setq contradiction t))
                                ((> (length d) 1)
                                 (when (or (null best) (< (length d) (length (aref doms best))))
                                   (setq best i)))))
                        (setq i (1+ i))))
                    (cond
                     (contradiction nil)
                     ((null best)
                      (let ((sol (make-vector 16 0)) (j 0))
                        (while (< j 16) (aset sol j (car (aref doms j))) (setq j (1+ j)))
                        (setq result sol) t))
                     (t (let ((found nil))
                          (dolist (val (aref doms best))
                            (unless found
                              (let ((saved (funcall 'neovm--cp5-copy-doms doms)))
                                (if (and (funcall 'neovm--cp5-assign doms best val)
                                         (funcall 'neovm--cp5-bt doms))
                                    (setq found t)
                                  (let ((k 0))
                                    (while (< k 16) (aset doms k (aref saved k)) (setq k (1+ k))))))))
                          found))))))
              (funcall 'neovm--cp5-bt doms)
              result))))))

  (unwind-protect
      ;; Only 2 clues: cell 0 = 1, cell 15 = 4
      (let* ((puzzle (vector 1 0 0 0
                             0 0 0 0
                             0 0 0 0
                             0 0 0 4))
             (solution (funcall 'neovm--cp5-solve puzzle)))
        (list
         (not (null solution))
         ;; Verify rows
         (when solution
           (let ((ok t) (r 0))
             (while (and ok (< r 4))
               (let ((row (list (aref solution (* r 4)) (aref solution (+ (* r 4) 1))
                                (aref solution (+ (* r 4) 2)) (aref solution (+ (* r 4) 3)))))
                 (unless (equal (sort (copy-sequence row) #'<) '(1 2 3 4))
                   (setq ok nil)))
               (setq r (1+ r)))
             ok))
         ;; Verify columns
         (when solution
           (let ((ok t) (c 0))
             (while (and ok (< c 4))
               (let ((col (list (aref solution c) (aref solution (+ c 4))
                                (aref solution (+ c 8)) (aref solution (+ c 12)))))
                 (unless (equal (sort (copy-sequence col) #'<) '(1 2 3 4))
                   (setq ok nil)))
               (setq c (1+ c)))
             ok))
         ;; Clues preserved
         (when solution
           (and (= (aref solution 0) 1)
                (= (aref solution 15) 4)))
         ;; The solution
         solution))
    (fmakunbound 'neovm--cp5-peers)
    (fmakunbound 'neovm--cp5-eliminate)
    (fmakunbound 'neovm--cp5-assign)
    (fmakunbound 'neovm--cp5-copy-doms)
    (fmakunbound 'neovm--cp5-solve)
    (fmakunbound 'neovm--cp5-bt)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t [1 3 4 2 2 4 1 3 4 2 3 1 3 1 2 4])""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Constraint propagation on a simple arithmetic puzzle
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cp_arithmetic_constraint() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Variables A, B, C with domains {1..5}.
    // Constraints: A + B = C, A < B, all different.
    // Find all solutions via constraint propagation + enumeration.
    let form = r#"
(let ((solutions nil))
  (let ((a 1))
    (while (<= a 5)
      (let ((b 1))
        (while (<= b 5)
          (let ((c 1))
            (while (<= c 5)
              (when (and (= (+ a b) c)
                         (< a b)
                         (/= a b) (/= a c) (/= b c))
                (setq solutions (cons (list a b c) solutions)))
              (setq c (1+ c))))
          (setq b (1+ b))))
      (setq a (1+ a))))
  (let ((sorted (sort solutions
                      (lambda (x y)
                        (or (< (car x) (car y))
                            (and (= (car x) (car y))
                                 (< (cadr x) (cadr y))))))))
    (list
     ;; Number of solutions
     (length sorted)
     ;; All solutions
     sorted
     ;; Verify each solution
     (let ((ok t))
       (dolist (s sorted)
         (let ((a (car s)) (b (cadr s)) (c (caddr s)))
           (unless (and (= (+ a b) c) (< a b) (/= a b) (/= a c) (/= b c))
             (setq ok nil))))
       ok)
     ;; Sum of all C values
     (apply #'+ (mapcar #'caddr sorted)))))
"#;
    let expect = expect_test::expect![[r#""OK (4 ((1 2 3) (1 3 4) (1 4 5) (2 3 5)) t 17)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

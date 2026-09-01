//! Oracle parity tests for advanced destructive list operations:
//! nconc with many arguments, nreverse in-place, nconc+nreverse for
//! efficient list building, nbutlast, sort destructiveness, interaction
//! with shared structure, and building data structures with destructive ops.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// nconc with many arguments and edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_nreverse_many_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // nconc with 0 through 8 arguments, including nil in various positions,
    // non-list final argument (creating dotted result), and verifying
    // that the first non-nil list is mutated in place.
    let form = r#"(list
  ;; 0 args
  (nconc)
  ;; 1 arg
  (nconc (list 1 2))
  ;; 2 args
  (let ((a (list 1 2)) (b (list 3 4)))
    (let ((r (nconc a b)))
      (list r (eq r a) (length a))))
  ;; 6 args including nils
  (nconc nil (list 'a) nil (list 'b 'c) nil (list 'd))
  ;; 8 args, all single-element
  (let ((l1 (list 1)) (l2 (list 2)) (l3 (list 3)) (l4 (list 4))
        (l5 (list 5)) (l6 (list 6)) (l7 (list 7)) (l8 (list 8)))
    (let ((r (nconc l1 l2 l3 l4 l5 l6 l7 l8)))
      (list r (length r) (eq r l1))))
  ;; Non-list last arg creates dotted result
  (nconc (list 'x 'y) 'z)
  ;; All nils except the last which is an atom
  (nconc nil nil nil 99)
  ;; First arg nil, second a list
  (let ((b (list 10 20)))
    (let ((r (nconc nil b)))
      (list r (eq r b)))))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (1 2) ((1 2 3 4) t 4) (a b c d) ((1 2 3 4 5 6 7 8) 8 t) (x y . z) 99 ((10 20) t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// nreverse in-place reversal with structural verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_nreverse_inplace_reversal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify nreverse destructively reverses, that the returned value
    // points to what was formerly the last cons, and that the original
    // variable may no longer point to the head.
    let form = r#"(list
  ;; Basic nreverse
  (nreverse (list 1 2 3 4 5))
  ;; nreverse of single element
  (nreverse (list 42))
  ;; nreverse of nil
  (nreverse nil)
  ;; Double nreverse restores order (on fresh list each time)
  (nreverse (nreverse (list 'a 'b 'c 'd)))
  ;; Original variable after nreverse: it points to old head which
  ;; is now the last cons cell with cdr=nil
  (let ((orig (list 1 2 3 4 5)))
    (let ((rev (nreverse orig)))
      ;; orig now points to (1) -- the old head, now tail
      (list rev orig (length rev) (car orig) (cdr orig))))
  ;; nreverse with two elements
  (let ((pair (list 'a 'b)))
    (let ((rev (nreverse pair)))
      (list rev (car rev) (cadr rev))))
  ;; nreverse preserves element identity (eq on elements)
  (let* ((obj1 (list 'inner1))
         (obj2 (list 'inner2))
         (lst (list obj1 obj2)))
    (let ((rev (nreverse lst)))
      (list (eq (car rev) obj2) (eq (cadr rev) obj1)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((5 4 3 2 1) (42) nil (a b c d) ((5 4 3 2 1) (1) 5 1 nil) ((b a) b a) (t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Classic cons+nreverse idiom for efficient list building
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_nreverse_efficient_building() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement several algorithms using the cons-then-nreverse pattern
    // and verify results match the equivalent mapcar/append approach.
    let form = r#"(progn
  ;; Method 1: Filter + transform using cons + nreverse
  (fset 'neovm--test-filter-transform-consrev
    (lambda (lst pred transform)
      (let ((acc nil))
        (dolist (x lst)
          (when (funcall pred x)
            (setq acc (cons (funcall transform x) acc))))
        (nreverse acc))))

  ;; Method 2: Same using nconc (less efficient but comparable)
  (fset 'neovm--test-filter-transform-nconc
    (lambda (lst pred transform)
      (let ((acc nil))
        (dolist (x lst)
          (when (funcall pred x)
            (setq acc (nconc acc (list (funcall transform x))))))
        acc)))

  ;; Method 3: Using mapcar + delq for comparison
  (fset 'neovm--test-filter-transform-mapcar
    (lambda (lst pred transform)
      (let ((sentinel (make-symbol "sentinel")))
        (delq sentinel
              (mapcar (lambda (x)
                        (if (funcall pred x)
                            (funcall transform x)
                          sentinel))
                      lst)))))

  (unwind-protect
      (let ((data '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
            (pred 'evenp)
            (xform (lambda (x) (* x x))))
        (let ((r1 (funcall 'neovm--test-filter-transform-consrev data pred xform))
              (r2 (funcall 'neovm--test-filter-transform-nconc data pred xform))
              (r3 (funcall 'neovm--test-filter-transform-mapcar data pred xform)))
          (list r1 r2 r3
                (equal r1 r2)
                (equal r1 r3)
                ;; Build a flat list from nested structure using cons+nreverse
                (let ((nested '((1 2) (3) () (4 5 6) (7)))
                      (flat nil))
                  (dolist (sub nested)
                    (dolist (x sub)
                      (setq flat (cons x flat))))
                  (nreverse flat))
                ;; Build alist from two lists using cons+nreverse
                (let ((keys '(a b c d e))
                      (vals '(1 2 3 4 5))
                      (pairs nil))
                  (while (and keys vals)
                    (setq pairs (cons (cons (car keys) (car vals)) pairs)
                          keys (cdr keys)
                          vals (cdr vals)))
                  (nreverse pairs)))))
    (fmakunbound 'neovm--test-filter-transform-consrev)
    (fmakunbound 'neovm--test-filter-transform-nconc)
    (fmakunbound 'neovm--test-filter-transform-mapcar)))"#;
    let expect = expect_test::expect![[
        r#""OK ((4 16 36 64 100 144 196) (4 16 36 64 100 144 196) (4 16 36 64 100 144 196) t t (1 2 3 4 5 6 7) ((a . 1) (b . 2) (c . 3) (d . 4) (e . 5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// nbutlast combined with nconc for queue/deque patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_nreverse_nbutlast_deque() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a double-ended queue using destructive operations:
    // nconc for push-back, cons for push-front, nbutlast for pop-back,
    // cdr for pop-front.
    let form = r#"(let ((deque nil)
                        (log nil))
                    ;; Push-back: 1 2 3
                    (setq deque (nconc deque (list 1)))
                    (setq deque (nconc deque (list 2)))
                    (setq deque (nconc deque (list 3)))
                    (setq log (cons (copy-sequence deque) log))
                    ;; Push-front: 0
                    (setq deque (cons 0 deque))
                    (setq log (cons (copy-sequence deque) log))
                    ;; Pop-back: remove 3
                    (let ((back (car (last deque))))
                      (setq deque (nbutlast deque))
                      (setq log (cons (list 'popped-back back (copy-sequence deque)) log)))
                    ;; Pop-front: remove 0
                    (let ((front (car deque)))
                      (setq deque (cdr deque))
                      (setq log (cons (list 'popped-front front (copy-sequence deque)) log)))
                    ;; Push-back many: 10 20 30
                    (setq deque (nconc deque (list 10 20 30)))
                    (setq log (cons (copy-sequence deque) log))
                    ;; Pop-back 2
                    (nbutlast deque 2)
                    (setq log (cons (copy-sequence deque) log))
                    ;; nreverse the deque
                    (setq deque (nreverse deque))
                    (setq log (cons (copy-sequence deque) log))
                    (nreverse log))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3) (0 1 2 3) (popped-back 3 (0 1 2)) (popped-front 0 (1 2)) (1 2 10 20 30) (1 2 10) (10 2 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// sort destructiveness and interaction with structure
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_nreverse_sort_destructive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify sort is destructive, that the original list is modified,
    // and show how to use copy-sequence before sort to preserve original.
    // Then combine sort with nconc to merge sorted sublists.
    let form = r#"(list
  ;; sort is destructive
  (let ((data (list 5 3 1 4 2)))
    (let ((sorted (sort data '<)))
      ;; data may not point to head of sorted list
      (list sorted (length sorted))))
  ;; Preserve original with copy-sequence
  (let ((orig (list 5 3 1 4 2)))
    (let ((sorted (sort (copy-sequence orig) '<)))
      (list orig sorted)))
  ;; Sort strings
  (sort (list "banana" "apple" "cherry" "date") 'string<)
  ;; Sort by custom predicate: by absolute value
  (sort (list -3 1 -5 2 -1 4) (lambda (a b) (< (abs a) (abs b))))
  ;; Merge two pre-sorted lists using nconc then sort
  (let ((sorted1 (list 1 3 5 7 9))
        (sorted2 (list 2 4 6 8 10)))
    (sort (nconc sorted1 sorted2) '<))
  ;; Sort of single element and nil
  (list (sort (list 42) '<) (sort nil '<))
  ;; Sort stability test: sort by first element of pairs, check order
  (let ((pairs (list '(2 . a) '(1 . b) '(3 . c) '(1 . d) '(2 . e))))
    (sort (copy-sequence pairs) (lambda (a b) (< (car a) (car b))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 2 3 4 5) 5) ((5 3 1 4 2) (1 2 3 4 5)) (\"apple\" \"banana\" \"cherry\" \"date\") (1 -1 2 -3 4 -5) (1 2 3 4 5 6 7 8 9 10) ((42) nil) ((1 . b) (1 . d) (2 . a) (2 . e) (3 . c)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Shared structure: nconc aliasing and mutation visibility
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_nreverse_shared_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When multiple lists share a tail via nconc, mutations through one
    // alias are visible through others. Test this with multiple scenarios.
    let form = r#"(let* ((tail (list 100 200 300))
                         (a (list 1 2))
                         (b (list 3 4)))
                    ;; nconc both onto the same tail
                    (nconc a tail)
                    (nconc b tail)
                    ;; a = (1 2 100 200 300), b = (3 4 100 200 300)
                    ;; They share the same tail cons cells
                    (let ((before-a (copy-sequence a))
                          (before-b (copy-sequence b)))
                      ;; Mutate through tail
                      (setcar tail 999)
                      (list
                        ;; Both see the mutation
                        a b
                        ;; The tail cell is eq in both
                        (eq (nthcdr 2 a) tail)
                        (eq (nthcdr 2 b) tail)
                        (eq (nthcdr 2 a) (nthcdr 2 b))
                        ;; Before snapshots (copied, unaffected)
                        before-a before-b
                        ;; Now nreverse tail -- this breaks the sharing!
                        ;; After nreverse, tail points to what was the last cell
                        (let ((rev-tail (nreverse (copy-sequence tail))))
                          rev-tail))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 999 200 300) (3 4 999 200 300) t t t (1 2 100 200 300) (3 4 100 200 300) (300 200 999))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Building complex data structures efficiently with destructive ops
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_nreverse_build_structures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build an adjacency list representation of a graph using
    // destructive operations, then traverse it.
    let form = r#"(progn
  (fset 'neovm--test-add-edge
    (lambda (graph from to)
      "Add directed edge FROM->TO in adjacency list GRAPH (alist)."
      (let ((entry (assq from graph)))
        (if entry
            (progn (setcdr entry (cons to (cdr entry))) graph)
          (cons (list from to) graph)))))

  (fset 'neovm--test-neighbors
    (lambda (graph node)
      (let ((entry (assq node graph)))
        (if entry (cdr entry) nil))))

  (fset 'neovm--test-bfs
    (lambda (graph start)
      "BFS traversal from START. Returns visit order."
      (let ((visited nil)
            (queue (list start))
            (order nil))
        (while queue
          (let ((node (car queue)))
            (setq queue (cdr queue))
            (unless (memq node visited)
              (setq visited (cons node visited))
              (setq order (cons node order))
              ;; Enqueue neighbors using nconc
              (let ((nbrs (funcall 'neovm--test-neighbors graph node)))
                (setq queue (nconc queue (copy-sequence nbrs)))))))
        (nreverse order))))

  (unwind-protect
      (let ((graph nil))
        ;; Build graph: a->b, a->c, b->d, c->d, d->e, c->e
        (setq graph (funcall 'neovm--test-add-edge graph 'a 'b))
        (setq graph (funcall 'neovm--test-add-edge graph 'a 'c))
        (setq graph (funcall 'neovm--test-add-edge graph 'b 'd))
        (setq graph (funcall 'neovm--test-add-edge graph 'c 'd))
        (setq graph (funcall 'neovm--test-add-edge graph 'd 'e))
        (setq graph (funcall 'neovm--test-add-edge graph 'c 'e))
        (list
          ;; Neighbors
          (sort (copy-sequence (funcall 'neovm--test-neighbors graph 'a))
                (lambda (a b) (string< (symbol-name a) (symbol-name b))))
          (sort (copy-sequence (funcall 'neovm--test-neighbors graph 'c))
                (lambda (a b) (string< (symbol-name a) (symbol-name b))))
          ;; BFS from a
          (funcall 'neovm--test-bfs graph 'a)
          ;; BFS from d
          (funcall 'neovm--test-bfs graph 'd)
          ;; Edge count (sum of neighbor list lengths)
          (let ((total 0))
            (dolist (entry graph)
              (setq total (+ total (length (cdr entry)))))
            total)
          ;; Build sorted edge list using cons+nreverse
          (let ((edges nil))
            (dolist (entry graph)
              (let ((from (car entry)))
                (dolist (to (cdr entry))
                  (setq edges (cons (list from to) edges)))))
            (sort (nreverse edges)
                  (lambda (a b)
                    (or (string< (symbol-name (car a)) (symbol-name (car b)))
                        (and (eq (car a) (car b))
                             (string< (symbol-name (cadr a))
                                      (symbol-name (cadr b))))))))))
    (fmakunbound 'neovm--test-add-edge)
    (fmakunbound 'neovm--test-neighbors)
    (fmakunbound 'neovm--test-bfs)))"#;
    let expect = expect_test::expect![[
        r#""OK ((b c) (d e) (a c b e d) (d e) 6 ((a b) (a c) (b d) (c d) (c e) (d e)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

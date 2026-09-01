//! Oracle parity tests for a 2-3 finger tree implementation in Elisp.
//!
//! A finger tree is a functional data structure that provides O(1) amortized
//! push/pop from both ends, O(log n) concatenation, and O(log n) indexed
//! access. The tree consists of:
//! - empty: nil
//! - single: (single . val)
//! - deep: (deep prefix middle suffix) where prefix/suffix are digit lists
//!   (1-4 elements) and middle is a finger tree of nodes (2-3 element tuples).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

/// Returns Elisp code defining the finger tree functions.
fn finger_tree_preamble() -> &'static str {
    r#"
  ;; ================================================================
  ;; 2-3 Finger Tree Implementation
  ;; ================================================================

  ;; Representation:
  ;;   empty  = nil
  ;;   single = (single . VAL)
  ;;   deep   = (deep PREFIX MIDDLE SUFFIX)
  ;;     where PREFIX, SUFFIX are lists of 1-4 elements
  ;;     and MIDDLE is a finger tree of node2/node3 elements
  ;;   node2 = (node2 A B)
  ;;   node3 = (node3 A B C)

  ;; --- Constructors ---

  (fset 'neovm--test-ft-empty (lambda () nil))
  (fset 'neovm--test-ft-single (lambda (v) (cons 'single v)))
  (fset 'neovm--test-ft-deep
    (lambda (pr mid sf)
      (list 'deep pr mid sf)))

  ;; --- Predicates ---

  (fset 'neovm--test-ft-empty-p (lambda (t_) (null t_)))
  (fset 'neovm--test-ft-single-p
    (lambda (t_) (and (consp t_) (eq (car t_) 'single))))
  (fset 'neovm--test-ft-deep-p
    (lambda (t_) (and (consp t_) (eq (car t_) 'deep))))

  ;; --- Accessors for deep ---

  (fset 'neovm--test-ft-prefix (lambda (t_) (nth 1 t_)))
  (fset 'neovm--test-ft-middle (lambda (t_) (nth 2 t_)))
  (fset 'neovm--test-ft-suffix (lambda (t_) (nth 3 t_)))

  ;; --- Node constructors ---

  (fset 'neovm--test-ft-node2 (lambda (a b) (list 'node2 a b)))
  (fset 'neovm--test-ft-node3 (lambda (a b c) (list 'node3 a b c)))

  ;; --- Convert node to list ---

  (fset 'neovm--test-ft-node-to-list
    (lambda (nd)
      (cond
       ((eq (car nd) 'node2) (list (nth 1 nd) (nth 2 nd)))
       ((eq (car nd) 'node3) (list (nth 1 nd) (nth 2 nd) (nth 3 nd)))
       (t (error "Not a node: %S" nd)))))

  ;; --- Push left (cons-like) ---

  (fset 'neovm--test-ft-push-l
    (lambda (val tree)
      (cond
       ;; empty -> single
       ((funcall 'neovm--test-ft-empty-p tree)
        (funcall 'neovm--test-ft-single val))

       ;; single -> deep with one-element prefix and suffix
       ((funcall 'neovm--test-ft-single-p tree)
        (funcall 'neovm--test-ft-deep
                 (list val) nil (list (cdr tree))))

       ;; deep: prefix has <4 elements -> just prepend
       ((< (length (funcall 'neovm--test-ft-prefix tree)) 4)
        (funcall 'neovm--test-ft-deep
                 (cons val (funcall 'neovm--test-ft-prefix tree))
                 (funcall 'neovm--test-ft-middle tree)
                 (funcall 'neovm--test-ft-suffix tree)))

       ;; deep: prefix has 4 elements -> push a node3 into middle
       (t
        (let ((pr (funcall 'neovm--test-ft-prefix tree)))
          (funcall 'neovm--test-ft-deep
                   (list val (nth 0 pr))
                   (funcall 'neovm--test-ft-push-l
                            (funcall 'neovm--test-ft-node3
                                     (nth 1 pr) (nth 2 pr) (nth 3 pr))
                            (funcall 'neovm--test-ft-middle tree))
                   (funcall 'neovm--test-ft-suffix tree)))))))

  ;; --- Push right (snoc-like) ---

  (fset 'neovm--test-ft-push-r
    (lambda (tree val)
      (cond
       ((funcall 'neovm--test-ft-empty-p tree)
        (funcall 'neovm--test-ft-single val))

       ((funcall 'neovm--test-ft-single-p tree)
        (funcall 'neovm--test-ft-deep
                 (list (cdr tree)) nil (list val)))

       ((< (length (funcall 'neovm--test-ft-suffix tree)) 4)
        (funcall 'neovm--test-ft-deep
                 (funcall 'neovm--test-ft-prefix tree)
                 (funcall 'neovm--test-ft-middle tree)
                 (append (funcall 'neovm--test-ft-suffix tree) (list val))))

       (t
        (let ((sf (funcall 'neovm--test-ft-suffix tree)))
          (funcall 'neovm--test-ft-deep
                   (funcall 'neovm--test-ft-prefix tree)
                   (funcall 'neovm--test-ft-push-r
                            (funcall 'neovm--test-ft-middle tree)
                            (funcall 'neovm--test-ft-node3
                                     (nth 0 sf) (nth 1 sf) (nth 2 sf)))
                   (list (nth 3 sf) val)))))))

  ;; --- Build a deep tree from a list of elements as prefix ---

  (fset 'neovm--test-ft-deep-l
    (lambda (pr mid sf)
      (cond
       ;; Non-empty prefix: normal deep
       (pr (funcall 'neovm--test-ft-deep pr mid sf))
       ;; Empty prefix: borrow from middle
       ((funcall 'neovm--test-ft-empty-p mid)
        ;; middle is empty; build from suffix
        (cond
         ((null sf) nil)
         ((= (length sf) 1) (funcall 'neovm--test-ft-single (car sf)))
         (t (funcall 'neovm--test-ft-deep
                     (list (car sf)) nil (cdr sf)))))
       ;; Middle is non-empty: pop a node from it
       (t
        (let ((head-result (funcall 'neovm--test-ft-pop-l mid)))
          (let ((node (car head-result))
                (mid2 (cdr head-result)))
            (funcall 'neovm--test-ft-deep
                     (funcall 'neovm--test-ft-node-to-list node)
                     mid2 sf)))))))

  (fset 'neovm--test-ft-deep-r
    (lambda (pr mid sf)
      (cond
       (sf (funcall 'neovm--test-ft-deep pr mid sf))
       ((funcall 'neovm--test-ft-empty-p mid)
        (cond
         ((null pr) nil)
         ((= (length pr) 1) (funcall 'neovm--test-ft-single (car pr)))
         (t (funcall 'neovm--test-ft-deep
                     (butlast pr) nil (last pr)))))
       (t
        (let ((last-result (funcall 'neovm--test-ft-pop-r mid)))
          (let ((node (car last-result))
                (mid2 (cdr last-result)))
            (funcall 'neovm--test-ft-deep
                     pr mid2
                     (funcall 'neovm--test-ft-node-to-list node))))))))

  ;; --- Pop left: returns (value . new-tree) ---

  (fset 'neovm--test-ft-pop-l
    (lambda (tree)
      (cond
       ((funcall 'neovm--test-ft-empty-p tree)
        (error "pop-l from empty tree"))

       ((funcall 'neovm--test-ft-single-p tree)
        (cons (cdr tree) nil))

       (t
        (let ((pr (funcall 'neovm--test-ft-prefix tree))
              (mid (funcall 'neovm--test-ft-middle tree))
              (sf (funcall 'neovm--test-ft-suffix tree)))
          (cons (car pr)
                (funcall 'neovm--test-ft-deep-l
                         (cdr pr) mid sf)))))))

  ;; --- Pop right: returns (value . new-tree) ---

  (fset 'neovm--test-ft-pop-r
    (lambda (tree)
      (cond
       ((funcall 'neovm--test-ft-empty-p tree)
        (error "pop-r from empty tree"))

       ((funcall 'neovm--test-ft-single-p tree)
        (cons (cdr tree) nil))

       (t
        (let ((pr (funcall 'neovm--test-ft-prefix tree))
              (mid (funcall 'neovm--test-ft-middle tree))
              (sf (funcall 'neovm--test-ft-suffix tree)))
          (cons (car (last sf))
                (funcall 'neovm--test-ft-deep-r
                         pr mid (butlast sf))))))))

  ;; --- Convert tree to list (in-order traversal) ---

  (fset 'neovm--test-ft-to-list
    (lambda (tree)
      (cond
       ((funcall 'neovm--test-ft-empty-p tree) nil)
       ((funcall 'neovm--test-ft-single-p tree) (list (cdr tree)))
       (t
        (append
         (funcall 'neovm--test-ft-prefix tree)
         ;; Flatten middle: each element is a node
         (let ((mid-list (funcall 'neovm--test-ft-to-list
                                  (funcall 'neovm--test-ft-middle tree)))
               (result nil))
           (dolist (nd mid-list)
             (setq result (append result
                                  (funcall 'neovm--test-ft-node-to-list nd))))
           result)
         (funcall 'neovm--test-ft-suffix tree))))))

  ;; --- Build from list ---

  (fset 'neovm--test-ft-from-list
    (lambda (lst)
      (let ((tree nil))
        (dolist (x lst)
          (setq tree (funcall 'neovm--test-ft-push-r tree x)))
        tree)))

  ;; --- Size ---

  (fset 'neovm--test-ft-size
    (lambda (tree)
      (length (funcall 'neovm--test-ft-to-list tree))))

  ;; --- Head and Last (peek) ---

  (fset 'neovm--test-ft-head
    (lambda (tree)
      (car (funcall 'neovm--test-ft-pop-l tree))))

  (fset 'neovm--test-ft-last
    (lambda (tree)
      (car (funcall 'neovm--test-ft-pop-r tree))))
"#
}

fn finger_tree_cleanup() -> &'static str {
    r#"
    (fmakunbound 'neovm--test-ft-empty)
    (fmakunbound 'neovm--test-ft-single)
    (fmakunbound 'neovm--test-ft-deep)
    (fmakunbound 'neovm--test-ft-empty-p)
    (fmakunbound 'neovm--test-ft-single-p)
    (fmakunbound 'neovm--test-ft-deep-p)
    (fmakunbound 'neovm--test-ft-prefix)
    (fmakunbound 'neovm--test-ft-middle)
    (fmakunbound 'neovm--test-ft-suffix)
    (fmakunbound 'neovm--test-ft-node2)
    (fmakunbound 'neovm--test-ft-node3)
    (fmakunbound 'neovm--test-ft-node-to-list)
    (fmakunbound 'neovm--test-ft-push-l)
    (fmakunbound 'neovm--test-ft-push-r)
    (fmakunbound 'neovm--test-ft-deep-l)
    (fmakunbound 'neovm--test-ft-deep-r)
    (fmakunbound 'neovm--test-ft-pop-l)
    (fmakunbound 'neovm--test-ft-pop-r)
    (fmakunbound 'neovm--test-ft-to-list)
    (fmakunbound 'neovm--test-ft-from-list)
    (fmakunbound 'neovm--test-ft-size)
    (fmakunbound 'neovm--test-ft-head)
    (fmakunbound 'neovm--test-ft-last)
"#
}

// ---------------------------------------------------------------------------
// Test 1: Push left and push right — basic construction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_finger_tree_push_left_right() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (let* ((t0 (funcall 'neovm--test-ft-empty))
             ;; Push left: 3 2 1 -> should give (1 2 3)
             (t1 (funcall 'neovm--test-ft-push-l 1 t0))
             (t2 (funcall 'neovm--test-ft-push-l 2 t1))
             (t3 (funcall 'neovm--test-ft-push-l 3 t2))
             ;; Push right: start fresh, push 10 20 30 -> (10 20 30)
             (t4 (funcall 'neovm--test-ft-push-r t0 10))
             (t5 (funcall 'neovm--test-ft-push-r t4 20))
             (t6 (funcall 'neovm--test-ft-push-r t5 30))
             ;; Mixed: push-l 0 then push-r 99
             (t7 (funcall 'neovm--test-ft-push-r
                          (funcall 'neovm--test-ft-push-l 0 t3) 99)))
        (list
         (funcall 'neovm--test-ft-to-list t1)
         (funcall 'neovm--test-ft-to-list t2)
         (funcall 'neovm--test-ft-to-list t3)
         (funcall 'neovm--test-ft-to-list t4)
         (funcall 'neovm--test-ft-to-list t6)
         (funcall 'neovm--test-ft-to-list t7)
         (funcall 'neovm--test-ft-size t7)))
    {cleanup}))"#,
        preamble = finger_tree_preamble(),
        cleanup = finger_tree_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 2: Pop left and pop right
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_finger_tree_pop_left_right() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (let* ((tree (funcall 'neovm--test-ft-from-list '(a b c d e)))
             ;; Pop left repeatedly
             (r1 (funcall 'neovm--test-ft-pop-l tree))
             (r2 (funcall 'neovm--test-ft-pop-l (cdr r1)))
             (r3 (funcall 'neovm--test-ft-pop-l (cdr r2)))
             ;; Pop right repeatedly from original
             (r4 (funcall 'neovm--test-ft-pop-r tree))
             (r5 (funcall 'neovm--test-ft-pop-r (cdr r4)))
             ;; Pop until single
             (one-elem (funcall 'neovm--test-ft-from-list '(42)))
             (r6 (funcall 'neovm--test-ft-pop-l one-elem)))
        (list
         ;; Values popped from left: a, b, c
         (list (car r1) (car r2) (car r3))
         ;; Remaining after 3 left pops: (d e)
         (funcall 'neovm--test-ft-to-list (cdr r3))
         ;; Values popped from right: e, d
         (list (car r4) (car r5))
         ;; Remaining after 2 right pops: (a b c)
         (funcall 'neovm--test-ft-to-list (cdr r5))
         ;; Pop from single: value is 42, tree is empty
         (car r6)
         (funcall 'neovm--test-ft-empty-p (cdr r6))))
    {cleanup}))"#,
        preamble = finger_tree_preamble(),
        cleanup = finger_tree_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 3: Push many elements (triggers node promotion into middle)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_finger_tree_many_elements() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (let* (;; Build a tree of 20 elements by push-l
             (tree-l (let ((t nil) (i 20))
                       (while (> i 0)
                         (setq t (funcall 'neovm--test-ft-push-l i t))
                         (setq i (1- i)))
                       t))
             ;; Build a tree of 20 elements by push-r
             (tree-r (funcall 'neovm--test-ft-from-list
                              '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20)))
             ;; Verify both produce the same list
             (list-l (funcall 'neovm--test-ft-to-list tree-l))
             (list-r (funcall 'neovm--test-ft-to-list tree-r)))
        (list
         ;; Both should be (1 2 3 ... 20)
         (equal list-l list-r)
         (length list-l)
         (funcall 'neovm--test-ft-head tree-l)
         (funcall 'neovm--test-ft-last tree-l)
         ;; Pop all from left, collect
         (let ((t tree-l) (acc nil))
           (while (not (funcall 'neovm--test-ft-empty-p t))
             (let ((r (funcall 'neovm--test-ft-pop-l t)))
               (setq acc (cons (car r) acc))
               (setq t (cdr r))))
           (nreverse acc))))
    {cleanup}))"#,
        preamble = finger_tree_preamble(),
        cleanup = finger_tree_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 4: from-list and to-list roundtrip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_finger_tree_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (let ((test-lists
             (list
              nil
              '(1)
              '(1 2)
              '(1 2 3)
              '(1 2 3 4)
              '(1 2 3 4 5)
              '(a b c d e f g h)
              '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))))
        (mapcar
         (lambda (lst)
           (let* ((tree (funcall 'neovm--test-ft-from-list lst))
                  (result (funcall 'neovm--test-ft-to-list tree)))
             (list (equal lst result)
                   (length result))))
         test-lists))
    {cleanup}))"#,
        preamble = finger_tree_preamble(),
        cleanup = finger_tree_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 5: Using finger tree as deque
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_finger_tree_deque_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}
  (unwind-protect
      (let ((deque (funcall 'neovm--test-ft-empty))
            (log nil))
        ;; Simulate a deque with interleaved push/pop from both ends
        ;; Push right: 1 2 3
        (setq deque (funcall 'neovm--test-ft-push-r deque 1))
        (setq deque (funcall 'neovm--test-ft-push-r deque 2))
        (setq deque (funcall 'neovm--test-ft-push-r deque 3))
        (setq log (cons (funcall 'neovm--test-ft-to-list deque) log))

        ;; Push left: 0 -1
        (setq deque (funcall 'neovm--test-ft-push-l 0 deque))
        (setq deque (funcall 'neovm--test-ft-push-l -1 deque))
        (setq log (cons (funcall 'neovm--test-ft-to-list deque) log))

        ;; Pop left: should get -1
        (let ((r (funcall 'neovm--test-ft-pop-l deque)))
          (setq log (cons (list 'pop-l (car r)) log))
          (setq deque (cdr r)))

        ;; Pop right: should get 3
        (let ((r (funcall 'neovm--test-ft-pop-r deque)))
          (setq log (cons (list 'pop-r (car r)) log))
          (setq deque (cdr r)))

        ;; Push right: 10 20
        (setq deque (funcall 'neovm--test-ft-push-r deque 10))
        (setq deque (funcall 'neovm--test-ft-push-r deque 20))
        (setq log (cons (funcall 'neovm--test-ft-to-list deque) log))

        ;; Pop left twice: 0, 1
        (let ((r1 (funcall 'neovm--test-ft-pop-l deque)))
          (setq deque (cdr r1))
          (let ((r2 (funcall 'neovm--test-ft-pop-l deque)))
            (setq deque (cdr r2))
            (setq log (cons (list 'pop-l (car r1) (car r2)) log))))

        ;; Final state
        (setq log (cons (funcall 'neovm--test-ft-to-list deque) log))
        (nreverse log))
    {cleanup}))"#,
        preamble = finger_tree_preamble(),
        cleanup = finger_tree_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 6: Indexed access via repeated pop
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_finger_tree_indexed_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}

  ;; nth-element: O(n) via repeated pop (not ideal, but tests correctness)
  (fset 'neovm--test-ft-nth
    (lambda (tree idx)
      (let ((t tree) (i 0))
        (while (< i idx)
          (setq t (cdr (funcall 'neovm--test-ft-pop-l t)))
          (setq i (1+ i)))
        (car (funcall 'neovm--test-ft-pop-l t)))))

  ;; reverse a finger tree
  (fset 'neovm--test-ft-reverse
    (lambda (tree)
      (let ((t tree) (acc (funcall 'neovm--test-ft-empty)))
        (while (not (funcall 'neovm--test-ft-empty-p t))
          (let ((r (funcall 'neovm--test-ft-pop-l t)))
            (setq acc (funcall 'neovm--test-ft-push-l (car r) acc))
            (setq t (cdr r))))
        acc)))

  (unwind-protect
      (let* ((tree (funcall 'neovm--test-ft-from-list
                             '(10 20 30 40 50 60 70 80 90 100))))
        (list
         ;; Access each index
         (funcall 'neovm--test-ft-nth tree 0)
         (funcall 'neovm--test-ft-nth tree 4)
         (funcall 'neovm--test-ft-nth tree 9)
         ;; Size
         (funcall 'neovm--test-ft-size tree)
         ;; Reverse and verify
         (let ((rev (funcall 'neovm--test-ft-reverse tree)))
           (list
            (funcall 'neovm--test-ft-to-list rev)
            (funcall 'neovm--test-ft-head rev)
            (funcall 'neovm--test-ft-last rev)))
         ;; Access reversed
         (funcall 'neovm--test-ft-nth
                  (funcall 'neovm--test-ft-reverse tree) 0)
         ;; Build from alternating push-l and push-r
         (let ((t (funcall 'neovm--test-ft-empty)))
           (setq t (funcall 'neovm--test-ft-push-r t 'mid))
           (setq t (funcall 'neovm--test-ft-push-l 'left t))
           (setq t (funcall 'neovm--test-ft-push-r t 'right))
           (setq t (funcall 'neovm--test-ft-push-l 'far-left t))
           (setq t (funcall 'neovm--test-ft-push-r t 'far-right))
           (funcall 'neovm--test-ft-to-list t))))
    (fmakunbound 'neovm--test-ft-nth)
    (fmakunbound 'neovm--test-ft-reverse)
    {cleanup}))"#,
        preamble = finger_tree_preamble(),
        cleanup = finger_tree_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Test 7: Concatenation via drain-and-push
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_finger_tree_concatenation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {preamble}

  ;; Simple concatenation: drain t2 from left, push into t1 from right
  (fset 'neovm--test-ft-concat
    (lambda (t1 t2)
      (let ((result t1) (src t2))
        (while (not (funcall 'neovm--test-ft-empty-p src))
          (let ((r (funcall 'neovm--test-ft-pop-l src)))
            (setq result (funcall 'neovm--test-ft-push-r result (car r)))
            (setq src (cdr r))))
        result)))

  (unwind-protect
      (let* ((t1 (funcall 'neovm--test-ft-from-list '(1 2 3)))
             (t2 (funcall 'neovm--test-ft-from-list '(4 5 6)))
             (t3 (funcall 'neovm--test-ft-from-list '(7 8 9 10)))
             ;; concat t1+t2
             (c12 (funcall 'neovm--test-ft-concat t1 t2))
             ;; concat (t1+t2)+t3
             (c123 (funcall 'neovm--test-ft-concat c12 t3))
             ;; concat empty + t1
             (ce1 (funcall 'neovm--test-ft-concat
                           (funcall 'neovm--test-ft-empty) t1))
             ;; concat t1 + empty
             (c1e (funcall 'neovm--test-ft-concat
                           t1 (funcall 'neovm--test-ft-empty))))
        (list
         (funcall 'neovm--test-ft-to-list c12)
         (funcall 'neovm--test-ft-to-list c123)
         (funcall 'neovm--test-ft-to-list ce1)
         (funcall 'neovm--test-ft-to-list c1e)
         (funcall 'neovm--test-ft-size c123)
         ;; Verify head/last of concatenated
         (funcall 'neovm--test-ft-head c123)
         (funcall 'neovm--test-ft-last c123)))
    (fmakunbound 'neovm--test-ft-concat)
    {cleanup}))"#,
        preamble = finger_tree_preamble(),
        cleanup = finger_tree_cleanup()
    );
    crate::common::assert_oracle_parity(&form);
}

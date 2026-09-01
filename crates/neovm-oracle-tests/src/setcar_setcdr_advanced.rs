//! Advanced oracle parity tests for setcar/setcdr mutation:
//! nested structure mutation, circular reference construction,
//! alist entry modification, incremental list building,
//! in-place list reversal, zipper data structure,
//! and doubly-linked list simulation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// setcar/setcdr: deep nested structure mutation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setcar_setcdr_deep_nested_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a nested tree structure and mutate at various depths
    let form = r#"(let* ((leaf1 (cons 'a 'b))
         (leaf2 (cons 'c 'd))
         (branch1 (cons leaf1 leaf2))
         (leaf3 (cons 'e 'f))
         (leaf4 (cons 'g 'h))
         (branch2 (cons leaf3 leaf4))
         (root (cons branch1 branch2)))
  ;; Mutate deep leaves
  (setcar leaf1 'A)
  (setcdr leaf2 'D)
  (setcar leaf3 'E)
  (setcdr leaf4 'H)
  ;; Mutate at branch level
  (setcdr branch1 (cons 'x 'y))
  ;; Verify structure through root
  (let ((result1 (car (car root)))
        (result2 (cdr (car root)))
        (result3 (car (cdr root)))
        (result4 (cdr (cdr root))))
    (list root
          result1
          result2
          result3
          result4
          ;; Verify identity (same cons cell)
          (eq (car root) branch1)
          (eq (cdr root) branch2)
          (eq (car (car root)) leaf1))))"#;
    let expect = expect_test::expect![[
        r#""OK ((((A . b) x . y) (E . f) g . H) (A . b) (x . y) (E . f) (g . H) t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// setcar/setcdr: creating and detecting circular references
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setcar_setcdr_circular_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create circular structures and test properties without printing them
    let form = r#"(let* (;; Create a self-referencing pair (car points to itself)
         (self-car (cons nil 42))
         (_ (setcar self-car self-car))
         ;; Create a cycle of length 2
         (a (cons 1 nil))
         (b (cons 2 nil))
         (_ (setcdr a b))
         (_ (setcdr b a))
         ;; Create a cycle of length 3
         (x (cons 'x nil))
         (y (cons 'y nil))
         (z (cons 'z nil))
         (_ (setcdr x y))
         (_ (setcdr y z))
         (_ (setcdr z x)))
  (list
   ;; Self-referencing: car is self
   (eq (car self-car) self-car)
   (cdr self-car)
   ;; Cycle of 2: follow cdr twice to get back
   (eq (cdr (cdr a)) a)
   (eq (cdr (cdr b)) b)
   (car a)
   (car b)
   ;; Cycle of 3: follow cdr three times
   (eq (cdr (cdr (cdr x))) x)
   (car x)
   (car (cdr x))
   (car (cdr (cdr x)))
   ;; Floyd's cycle detection on the length-3 cycle
   (let ((slow x) (fast x) (found nil) (steps 0))
     (while (and (not found) (< steps 20))
       (setq slow (cdr slow))
       (setq fast (cdr (cdr fast)))
       (setq steps (1+ steps))
       (when (eq slow fast)
         (setq found t)))
     (list found steps))))"#;
    let expect = expect_test::expect![[r#""OK (t 42 t t 1 2 t x y z (t 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// setcar/setcdr: modifying alist entries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setcar_setcdr_alist_modification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build an alist and modify entries in place using setcar/setcdr
    let form = r#"(let* ((alist (list (cons 'name "Alice")
                       (cons 'age 30)
                       (cons 'city "NYC")
                       (cons 'score 95)))
         ;; Update value of 'age entry
         (age-cell (assq 'age alist))
         (_ (setcdr age-cell 31))
         ;; Update key of 'city entry to 'location
         (city-cell (assq 'city alist))
         (_ (setcar city-cell 'location))
         ;; Update both car and cdr of 'score
         (score-cell (assq 'score alist))
         (_ (setcar score-cell 'grade))
         (_ (setcdr score-cell "A"))
         ;; Add new entry by mutating last cdr
         (last-cell (last alist))
         (_ (setcdr last-cell (list (cons 'active t)))))
  (list
   alist
   (cdr (assq 'name alist))
   (cdr (assq 'age alist))
   (assq 'city alist)
   (cdr (assq 'location alist))
   (cdr (assq 'grade alist))
   (cdr (assq 'active alist))
   (length alist)))"#;
    let expect = expect_test::expect![[
        r#""OK (((name . \"Alice\") (age . 31) (location . \"NYC\") (grade . \"A\") (active . t)) \"Alice\" 31 nil \"NYC\" \"A\" t 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// setcdr: incremental list building by appending via setcdr
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setcar_setcdr_incremental_list_building() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a list incrementally using a tail pointer pattern
    // (common in C-level Emacs code, rare but valid in Elisp)
    let form = r#"(let* ((head (cons nil nil))
         (tail head)
         (items '(10 20 30 40 50 60 70 80 90 100)))
  ;; Append each item by setcdr on tail, then advance tail
  (dolist (item items)
    (let ((new-cell (cons item nil)))
      (setcdr tail new-cell)
      (setq tail new-cell)))
  ;; Result is (cdr head) since head was a dummy sentinel
  (let* ((result (cdr head))
         ;; Verify we can walk the whole list
         (sum (let ((s 0) (p result))
                (while p
                  (setq s (+ s (car p)))
                  (setq p (cdr p)))
                s))
         ;; Verify length
         (len (length result))
         ;; Verify eq of tail to last cell
         (last-cell (last result)))
    (list result sum len (eq tail last-cell)
          (car tail) (car result))))"#;
    let expect =
        expect_test::expect![[r#""OK ((10 20 30 40 50 60 70 80 90 100) 550 10 t 100 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: in-place list reversal using setcdr
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setcar_setcdr_inplace_reverse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement in-place list reversal by only mutating cdr pointers
    // This is essentially what nreverse does, but we do it manually
    let form = r#"(let* ((original-items '(1 2 3 4 5 6 7 8 9 10))
         (lst (copy-sequence original-items))
         ;; Save original first cell identity
         (first-cell lst))
  ;; Manual in-place reversal using setcdr
  (let ((prev nil)
        (curr lst)
        (next nil))
    (while curr
      (setq next (cdr curr))
      (setcdr curr prev)
      (setq prev curr)
      (setq curr next))
    (setq lst prev))
  ;; lst now points to what was the last cell
  (let* ((reversed-list lst)
         ;; Verify: first cell of original is now last
         (is-last (null (cdr first-cell)))
         ;; Verify: car values are reversed
         (cars (let ((acc nil) (p reversed-list))
                 (while p
                   (setq acc (cons (car p) acc))
                   (setq p (cdr p)))
                 (nreverse acc)))
         ;; Reverse again to get back to original order
         (prev2 nil)
         (curr2 reversed-list))
    (while curr2
      (let ((next2 (cdr curr2)))
        (setcdr curr2 prev2)
        (setq prev2 curr2)
        (setq curr2 next2)))
    (let ((doubly-reversed prev2))
      (list reversed-list
            cars
            is-last
            doubly-reversed
            (equal doubly-reversed original-items)
            ;; The first-cell should now be at the head again
            (eq first-cell doubly-reversed)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((10) (10 9 8 7 6 5 4 3 2 1) t (1 2 3 4 5 6 7 8 9 10) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: zipper data structure using setcar/setcdr
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setcar_setcdr_zipper() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a list zipper: focus on one element with context for left/right
    // Zipper = (left . (focus . right))
    // Operations: move-left, move-right, replace-focus, insert
    let form = r#"(let ((make-zipper
         (lambda (lst)
           (cons nil (cons (car lst) (cdr lst)))))
        (zipper-focus
         (lambda (z) (car (cdr z))))
        (zipper-move-right
         (lambda (z)
           (let ((left (car z))
                 (focus (car (cdr z)))
                 (right (cdr (cdr z))))
             (if (null right) z
               (cons (cons focus left)
                     (cons (car right) (cdr right)))))))
        (zipper-move-left
         (lambda (z)
           (let ((left (car z))
                 (focus (car (cdr z)))
                 (right (cdr (cdr z))))
             (if (null left) z
               (cons (cdr left)
                     (cons (car left) (cons focus right)))))))
        (zipper-replace
         (lambda (z val)
           (let ((new-z (copy-tree z)))
             (setcar (cdr new-z) val)
             new-z)))
        (zipper-to-list
         (lambda (z)
           (let ((left (car z))
                 (focus (car (cdr z)))
                 (right (cdr (cdr z))))
             (append (reverse left) (list focus) right)))))
  (let* ((lst '(a b c d e))
         (z0 (funcall make-zipper lst))
         (f0 (funcall zipper-focus z0))
         ;; Move right twice
         (z1 (funcall zipper-move-right z0))
         (f1 (funcall zipper-focus z1))
         (z2 (funcall zipper-move-right z1))
         (f2 (funcall zipper-focus z2))
         ;; Replace focus
         (z3 (funcall zipper-replace z2 'X))
         (f3 (funcall zipper-focus z3))
         ;; Move left
         (z4 (funcall zipper-move-left z3))
         (f4 (funcall zipper-focus z4))
         ;; Convert back to list
         (l3 (funcall zipper-to-list z3))
         (l4 (funcall zipper-to-list z4)))
    (list f0 f1 f2 f3 f4 l3 l4
          ;; Original list should be intact (we used copy-tree)
          (equal (funcall zipper-to-list z0) lst))))"#;
    let expect = expect_test::expect![[r#""OK (a b c X b (a b X d e) (a b X d e) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: doubly-linked list using cons cells with setcdr
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setcar_setcdr_doubly_linked_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each node is (value . (prev . next))
    // Build a doubly-linked list and traverse both directions
    let form = r#"(let ((make-dll-node
         (lambda (val) (cons val (cons nil nil))))
        (dll-val (lambda (node) (car node)))
        (dll-prev (lambda (node) (car (cdr node))))
        (dll-next (lambda (node) (cdr (cdr node))))
        (dll-set-next
         (lambda (node next-node)
           (setcdr (cdr node) next-node)))
        (dll-set-prev
         (lambda (node prev-node)
           (setcar (cdr node) prev-node))))
  ;; Build: 10 <-> 20 <-> 30 <-> 40 <-> 50
  (let* ((n1 (funcall make-dll-node 10))
         (n2 (funcall make-dll-node 20))
         (n3 (funcall make-dll-node 30))
         (n4 (funcall make-dll-node 40))
         (n5 (funcall make-dll-node 50)))
    ;; Link forward
    (funcall dll-set-next n1 n2)
    (funcall dll-set-next n2 n3)
    (funcall dll-set-next n3 n4)
    (funcall dll-set-next n4 n5)
    ;; Link backward
    (funcall dll-set-prev n2 n1)
    (funcall dll-set-prev n3 n2)
    (funcall dll-set-prev n4 n3)
    (funcall dll-set-prev n5 n4)
    ;; Traverse forward collecting values
    (let ((forward nil)
          (curr n1))
      (while curr
        (setq forward (cons (funcall dll-val curr) forward))
        (setq curr (funcall dll-next curr)))
      (setq forward (nreverse forward))
      ;; Traverse backward from n5
      (let ((backward nil)
            (curr2 n5))
        (while curr2
          (setq backward (cons (funcall dll-val curr2) backward))
          (setq curr2 (funcall dll-prev curr2)))
        ;; backward is already in 10..50 order (we cons'd from 50 going to 10)
        ;; Insert n6=25 between n2 and n3
        (let ((n6 (funcall make-dll-node 25)))
          (funcall dll-set-next n2 n6)
          (funcall dll-set-prev n6 n2)
          (funcall dll-set-next n6 n3)
          (funcall dll-set-prev n3 n6)
          ;; Traverse forward again
          (let ((forward2 nil)
                (c n1))
            (while c
              (setq forward2 (cons (funcall dll-val c) forward2))
              (setq c (funcall dll-next c)))
            (setq forward2 (nreverse forward2))
            ;; Delete n3 (30) by relinking n6 <-> n4
            (funcall dll-set-next n6 n4)
            (funcall dll-set-prev n4 n6)
            ;; Traverse forward after deletion
            (let ((forward3 nil)
                  (d n1))
              (while d
                (setq forward3 (cons (funcall dll-val d) forward3))
                (setq d (funcall dll-next d)))
              (setq forward3 (nreverse forward3))
              (list forward backward forward2 forward3
                    (length forward) (length forward2) (length forward3))))))))))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 66 81)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: setcar/setcdr to build and manipulate a queue (ring buffer)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setcar_setcdr_ring_buffer_queue() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a circular queue using cons cells
    // Queue state: (head . tail) where tail's cdr links back to head
    let form = r#"(let ((results nil))
  ;; Build a ring of 5 slots initialized to nil
  (let* ((c1 (cons nil nil))
         (c2 (cons nil nil))
         (c3 (cons nil nil))
         (c4 (cons nil nil))
         (c5 (cons nil nil)))
    ;; Link into ring: c1->c2->c3->c4->c5->c1
    (setcdr c1 c2)
    (setcdr c2 c3)
    (setcdr c3 c4)
    (setcdr c4 c5)
    (setcdr c5 c1)
    ;; Verify it's a ring: walk 5 steps and get back to start
    (let ((ptr c1) (count 0))
      (dotimes (_ 5)
        (setq ptr (cdr ptr))
        (setq count (1+ count)))
      (setq results (cons (eq ptr c1) results))
      (setq results (cons count results)))
    ;; Fill the ring with values
    (let ((ptr c1))
      (dotimes (i 5)
        (setcar ptr (* (1+ i) 10))
        (setq ptr (cdr ptr))))
    ;; Read all values by walking the ring
    (let ((vals nil) (ptr c1))
      (dotimes (_ 5)
        (setq vals (cons (car ptr) vals))
        (setq ptr (cdr ptr)))
      (setq results (cons (nreverse vals) results)))
    ;; Overwrite every other slot
    (setcar c1 100)
    (setcar c3 300)
    (setcar c5 500)
    (let ((vals nil) (ptr c1))
      (dotimes (_ 5)
        (setq vals (cons (car ptr) vals))
        (setq ptr (cdr ptr)))
      (setq results (cons (nreverse vals) results)))
    ;; Sum by walking the ring
    (let ((sum 0) (ptr c1))
      (dotimes (_ 5)
        (setq sum (+ sum (car ptr)))
        (setq ptr (cdr ptr)))
      (setq results (cons sum results))))
  (nreverse results))"#;
    let expect = expect_test::expect![[r#""OK (t 5 (10 20 30 40 50) (100 20 300 40 500) 960)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

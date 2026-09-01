//! Oracle parity tests for a ring buffer (circular buffer) data structure
//! implemented in pure Elisp using vectors.
//!
//! Operations: create with capacity, push (overwrites oldest when full),
//! pop, peek, full?, empty?, size, iterate all elements in order.
//! Tests with various capacities, overflow scenarios, and stress patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Core ring buffer: create, push, pop, peek, size, empty?, full?
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ring_buffer_core_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Ring buffer stored as (vector capacity head tail count).
    // Push writes at tail, advances tail mod capacity, increments count.
    // When full, push overwrites oldest (advances head too).
    // Pop reads from head, advances head mod capacity, decrements count.
    let form = r#"(progn
  (fset 'neovm--rb-create
    (lambda (cap)
      "Create ring buffer with capacity CAP."
      (list (make-vector cap nil) cap 0 0 0)))

  (fset 'neovm--rb-capacity (lambda (rb) (nth 1 rb)))
  (fset 'neovm--rb-head     (lambda (rb) (nth 2 rb)))
  (fset 'neovm--rb-tail     (lambda (rb) (nth 3 rb)))
  (fset 'neovm--rb-count    (lambda (rb) (nth 4 rb)))
  (fset 'neovm--rb-empty-p  (lambda (rb) (= (nth 4 rb) 0)))
  (fset 'neovm--rb-full-p   (lambda (rb) (= (nth 4 rb) (nth 1 rb))))

  (fset 'neovm--rb-push
    (lambda (rb val)
      "Push VAL into ring buffer. Overwrites oldest if full. Returns new rb."
      (let* ((vec (nth 0 rb))
             (cap (nth 1 rb))
             (head (nth 2 rb))
             (tail (nth 3 rb))
             (cnt (nth 4 rb)))
        (aset vec tail val)
        (let ((new-tail (% (1+ tail) cap)))
          (if (= cnt cap)
              ;; Full: overwrite oldest, advance head
              (list vec cap (% (1+ head) cap) new-tail cap)
            ;; Not full: just advance tail, increment count
            (list vec cap head new-tail (1+ cnt)))))))

  (fset 'neovm--rb-pop
    (lambda (rb)
      "Pop oldest element. Returns (value . new-rb). Nil value if empty."
      (if (funcall 'neovm--rb-empty-p rb)
          (cons nil rb)
        (let* ((vec (nth 0 rb))
               (cap (nth 1 rb))
               (head (nth 2 rb))
               (tail (nth 3 rb))
               (cnt (nth 4 rb))
               (val (aref vec head))
               (new-head (% (1+ head) cap)))
          (cons val (list vec cap new-head tail (1- cnt)))))))

  (fset 'neovm--rb-peek
    (lambda (rb)
      "Return oldest element without removing, or nil if empty."
      (if (funcall 'neovm--rb-empty-p rb)
          nil
        (aref (nth 0 rb) (nth 2 rb)))))

  (fset 'neovm--rb-to-list
    (lambda (rb)
      "Return all elements in order (oldest first) as a list."
      (let* ((vec (nth 0 rb))
             (cap (nth 1 rb))
             (head (nth 2 rb))
             (cnt (nth 4 rb))
             (result nil)
             (i 0))
        (while (< i cnt)
          (setq result (cons (aref vec (% (+ head i) cap)) result))
          (setq i (1+ i)))
        (nreverse result))))

  (unwind-protect
      (let ((rb (funcall 'neovm--rb-create 4)))
        (let ((e0 (funcall 'neovm--rb-empty-p rb))
              (f0 (funcall 'neovm--rb-full-p rb))
              (s0 (funcall 'neovm--rb-count rb)))
          ;; Push 3 elements
          (setq rb (funcall 'neovm--rb-push rb 10))
          (setq rb (funcall 'neovm--rb-push rb 20))
          (setq rb (funcall 'neovm--rb-push rb 30))
          (let ((s3 (funcall 'neovm--rb-count rb))
                (e3 (funcall 'neovm--rb-empty-p rb))
                (f3 (funcall 'neovm--rb-full-p rb))
                (pk3 (funcall 'neovm--rb-peek rb))
                (l3 (funcall 'neovm--rb-to-list rb)))
            ;; Pop one
            (let* ((r1 (funcall 'neovm--rb-pop rb))
                   (v1 (car r1)))
              (setq rb (cdr r1))
              (let ((s2 (funcall 'neovm--rb-count rb))
                    (pk2 (funcall 'neovm--rb-peek rb))
                    (l2 (funcall 'neovm--rb-to-list rb)))
                (list e0 f0 s0 s3 e3 f3 pk3 l3 v1 s2 pk2 l2))))))
    (fmakunbound 'neovm--rb-create)
    (fmakunbound 'neovm--rb-capacity)
    (fmakunbound 'neovm--rb-head)
    (fmakunbound 'neovm--rb-tail)
    (fmakunbound 'neovm--rb-count)
    (fmakunbound 'neovm--rb-empty-p)
    (fmakunbound 'neovm--rb-full-p)
    (fmakunbound 'neovm--rb-push)
    (fmakunbound 'neovm--rb-pop)
    (fmakunbound 'neovm--rb-peek)
    (fmakunbound 'neovm--rb-to-list)))"#;
    let expect =
        expect_test::expect![[r#""OK (t nil 0 3 nil nil 10 (10 20 30) 10 2 20 (20 30))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Overflow: push beyond capacity, oldest elements overwritten
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ring_buffer_overflow_overwrite() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Push more elements than capacity and verify oldest are overwritten
    let form = r#"(progn
  (fset 'neovm--rb-create
    (lambda (cap) (list (make-vector cap nil) cap 0 0 0)))
  (fset 'neovm--rb-empty-p (lambda (rb) (= (nth 4 rb) 0)))
  (fset 'neovm--rb-full-p  (lambda (rb) (= (nth 4 rb) (nth 1 rb))))
  (fset 'neovm--rb-count   (lambda (rb) (nth 4 rb)))
  (fset 'neovm--rb-push
    (lambda (rb val)
      (let* ((vec (nth 0 rb)) (cap (nth 1 rb))
             (head (nth 2 rb)) (tail (nth 3 rb)) (cnt (nth 4 rb)))
        (aset vec tail val)
        (let ((nt (% (1+ tail) cap)))
          (if (= cnt cap)
              (list vec cap (% (1+ head) cap) nt cap)
            (list vec cap head nt (1+ cnt)))))))
  (fset 'neovm--rb-peek
    (lambda (rb) (if (= (nth 4 rb) 0) nil (aref (nth 0 rb) (nth 2 rb)))))
  (fset 'neovm--rb-to-list
    (lambda (rb)
      (let* ((vec (nth 0 rb)) (cap (nth 1 rb))
             (head (nth 2 rb)) (cnt (nth 4 rb))
             (result nil) (i 0))
        (while (< i cnt)
          (setq result (cons (aref vec (% (+ head i) cap)) result))
          (setq i (1+ i)))
        (nreverse result))))

  (unwind-protect
      (let ((rb (funcall 'neovm--rb-create 3)))
        ;; Fill to capacity: 1, 2, 3
        (setq rb (funcall 'neovm--rb-push rb 1))
        (setq rb (funcall 'neovm--rb-push rb 2))
        (setq rb (funcall 'neovm--rb-push rb 3))
        (let ((full3 (funcall 'neovm--rb-full-p rb))
              (list3 (funcall 'neovm--rb-to-list rb))
              (peek3 (funcall 'neovm--rb-peek rb)))
          ;; Push 4: overwrites 1, buffer is [4,2,3] with head at 2
          (setq rb (funcall 'neovm--rb-push rb 4))
          (let ((list4 (funcall 'neovm--rb-to-list rb))
                (peek4 (funcall 'neovm--rb-peek rb))
                (cnt4 (funcall 'neovm--rb-count rb)))
            ;; Push 5: overwrites 2, buffer is [4,5,3] with head at 3
            (setq rb (funcall 'neovm--rb-push rb 5))
            ;; Push 6: overwrites 3
            (setq rb (funcall 'neovm--rb-push rb 6))
            (let ((list6 (funcall 'neovm--rb-to-list rb))
                  (peek6 (funcall 'neovm--rb-peek rb)))
              ;; Push 7, 8, 9: complete wrap-around cycle
              (setq rb (funcall 'neovm--rb-push rb 7))
              (setq rb (funcall 'neovm--rb-push rb 8))
              (setq rb (funcall 'neovm--rb-push rb 9))
              (list full3 list3 peek3
                    list4 peek4 cnt4
                    list6 peek6
                    (funcall 'neovm--rb-to-list rb)
                    (funcall 'neovm--rb-count rb))))))
    (fmakunbound 'neovm--rb-create)
    (fmakunbound 'neovm--rb-empty-p)
    (fmakunbound 'neovm--rb-full-p)
    (fmakunbound 'neovm--rb-count)
    (fmakunbound 'neovm--rb-push)
    (fmakunbound 'neovm--rb-peek)
    (fmakunbound 'neovm--rb-to-list)))"#;
    let expect = expect_test::expect![[r#""OK (t (1 2 3) 1 (2 3 4) 2 3 (4 5 6) 4 (7 8 9) 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interleaved push/pop with capacity 1 (degenerate case)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ring_buffer_capacity_one() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Capacity-1 ring buffer: always full after first push, each push
    // overwrites the single element.
    let form = r#"(progn
  (fset 'neovm--rb-create
    (lambda (cap) (list (make-vector cap nil) cap 0 0 0)))
  (fset 'neovm--rb-empty-p (lambda (rb) (= (nth 4 rb) 0)))
  (fset 'neovm--rb-full-p  (lambda (rb) (= (nth 4 rb) (nth 1 rb))))
  (fset 'neovm--rb-count   (lambda (rb) (nth 4 rb)))
  (fset 'neovm--rb-push
    (lambda (rb val)
      (let* ((vec (nth 0 rb)) (cap (nth 1 rb))
             (head (nth 2 rb)) (tail (nth 3 rb)) (cnt (nth 4 rb)))
        (aset vec tail val)
        (let ((nt (% (1+ tail) cap)))
          (if (= cnt cap)
              (list vec cap (% (1+ head) cap) nt cap)
            (list vec cap head nt (1+ cnt)))))))
  (fset 'neovm--rb-pop
    (lambda (rb)
      (if (= (nth 4 rb) 0) (cons nil rb)
        (let* ((vec (nth 0 rb)) (cap (nth 1 rb))
               (head (nth 2 rb)) (tail (nth 3 rb)) (cnt (nth 4 rb))
               (val (aref vec head)))
          (cons val (list vec cap (% (1+ head) cap) tail (1- cnt)))))))
  (fset 'neovm--rb-peek
    (lambda (rb) (if (= (nth 4 rb) 0) nil (aref (nth 0 rb) (nth 2 rb)))))
  (fset 'neovm--rb-to-list
    (lambda (rb)
      (let* ((vec (nth 0 rb)) (cap (nth 1 rb))
             (head (nth 2 rb)) (cnt (nth 4 rb))
             (result nil) (i 0))
        (while (< i cnt)
          (setq result (cons (aref vec (% (+ head i) cap)) result))
          (setq i (1+ i)))
        (nreverse result))))

  (unwind-protect
      (let ((rb (funcall 'neovm--rb-create 1)))
        (let ((e0 (funcall 'neovm--rb-empty-p rb))
              (f0 (funcall 'neovm--rb-full-p rb)))
          ;; Push first element
          (setq rb (funcall 'neovm--rb-push rb 'alpha))
          (let ((l1 (funcall 'neovm--rb-to-list rb))
                (f1 (funcall 'neovm--rb-full-p rb)))
            ;; Push overwrites
            (setq rb (funcall 'neovm--rb-push rb 'beta))
            (let ((l2 (funcall 'neovm--rb-to-list rb)))
              ;; Pop
              (let* ((r (funcall 'neovm--rb-pop rb))
                     (v (car r)))
                (setq rb (cdr r))
                (let ((e-after (funcall 'neovm--rb-empty-p rb)))
                  ;; Push again, pop again
                  (setq rb (funcall 'neovm--rb-push rb 'gamma))
                  (let* ((r2 (funcall 'neovm--rb-pop rb))
                         (v2 (car r2)))
                    (setq rb (cdr r2))
                    ;; Push 5 times in a row, only last survives
                    (setq rb (funcall 'neovm--rb-push rb 1))
                    (setq rb (funcall 'neovm--rb-push rb 2))
                    (setq rb (funcall 'neovm--rb-push rb 3))
                    (setq rb (funcall 'neovm--rb-push rb 4))
                    (setq rb (funcall 'neovm--rb-push rb 5))
                    (list e0 f0 l1 f1 l2 v e-after v2
                          (funcall 'neovm--rb-to-list rb)
                          (funcall 'neovm--rb-count rb)))))))))
    (fmakunbound 'neovm--rb-create)
    (fmakunbound 'neovm--rb-empty-p)
    (fmakunbound 'neovm--rb-full-p)
    (fmakunbound 'neovm--rb-count)
    (fmakunbound 'neovm--rb-push)
    (fmakunbound 'neovm--rb-pop)
    (fmakunbound 'neovm--rb-peek)
    (fmakunbound 'neovm--rb-to-list)))"#;
    let expect = expect_test::expect![[r#""OK (t nil (alpha) t (beta) beta t gamma (5) 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interleaved push and pop with medium capacity
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ring_buffer_interleaved_push_pop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Interleave pushes and pops to exercise head/tail wrap-around
    // without always being full or empty.
    let form = r#"(progn
  (fset 'neovm--rb-create
    (lambda (cap) (list (make-vector cap nil) cap 0 0 0)))
  (fset 'neovm--rb-count (lambda (rb) (nth 4 rb)))
  (fset 'neovm--rb-push
    (lambda (rb val)
      (let* ((vec (nth 0 rb)) (cap (nth 1 rb))
             (head (nth 2 rb)) (tail (nth 3 rb)) (cnt (nth 4 rb)))
        (aset vec tail val)
        (let ((nt (% (1+ tail) cap)))
          (if (= cnt cap)
              (list vec cap (% (1+ head) cap) nt cap)
            (list vec cap head nt (1+ cnt)))))))
  (fset 'neovm--rb-pop
    (lambda (rb)
      (if (= (nth 4 rb) 0) (cons nil rb)
        (let* ((vec (nth 0 rb)) (cap (nth 1 rb))
               (head (nth 2 rb)) (tail (nth 3 rb)) (cnt (nth 4 rb))
               (val (aref vec head)))
          (cons val (list vec cap (% (1+ head) cap) tail (1- cnt)))))))
  (fset 'neovm--rb-to-list
    (lambda (rb)
      (let* ((vec (nth 0 rb)) (cap (nth 1 rb))
             (head (nth 2 rb)) (cnt (nth 4 rb))
             (result nil) (i 0))
        (while (< i cnt)
          (setq result (cons (aref vec (% (+ head i) cap)) result))
          (setq i (1+ i)))
        (nreverse result))))

  (unwind-protect
      (let ((rb (funcall 'neovm--rb-create 5))
            (snapshots nil))
        ;; Push 3 items
        (setq rb (funcall 'neovm--rb-push rb 'a))
        (setq rb (funcall 'neovm--rb-push rb 'b))
        (setq rb (funcall 'neovm--rb-push rb 'c))
        (setq snapshots (cons (funcall 'neovm--rb-to-list rb) snapshots))
        ;; Pop 2
        (let* ((r1 (funcall 'neovm--rb-pop rb)) (v1 (car r1)))
          (setq rb (cdr r1))
          (let* ((r2 (funcall 'neovm--rb-pop rb)) (v2 (car r2)))
            (setq rb (cdr r2))
            (setq snapshots (cons (list v1 v2 (funcall 'neovm--rb-to-list rb)) snapshots))
            ;; Push 4 more (wraps tail around)
            (setq rb (funcall 'neovm--rb-push rb 'd))
            (setq rb (funcall 'neovm--rb-push rb 'e))
            (setq rb (funcall 'neovm--rb-push rb 'f))
            (setq rb (funcall 'neovm--rb-push rb 'g))
            (setq snapshots (cons (funcall 'neovm--rb-to-list rb) snapshots))
            ;; Pop 3, push 2
            (let* ((r3 (funcall 'neovm--rb-pop rb)))
              (setq rb (cdr r3))
              (let* ((r4 (funcall 'neovm--rb-pop rb)))
                (setq rb (cdr r4))
                (let* ((r5 (funcall 'neovm--rb-pop rb)))
                  (setq rb (cdr r5))
                  (setq rb (funcall 'neovm--rb-push rb 'h))
                  (setq rb (funcall 'neovm--rb-push rb 'i))
                  (setq snapshots (cons (list (car r3) (car r4) (car r5)
                                              (funcall 'neovm--rb-to-list rb)
                                              (funcall 'neovm--rb-count rb))
                                        snapshots))
                  (nreverse snapshots)))))))
    (fmakunbound 'neovm--rb-create)
    (fmakunbound 'neovm--rb-count)
    (fmakunbound 'neovm--rb-push)
    (fmakunbound 'neovm--rb-pop)
    (fmakunbound 'neovm--rb-to-list)))"#;
    let expect =
        expect_test::expect![[r#""OK ((a b c) (a b (c)) (c d e f g) (c d e (f g h i) 4))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Pop from empty buffer, push-pop-push cycles
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ring_buffer_empty_pop_and_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test popping from empty buffer (should return nil and not corrupt state),
    // and multiple fill-drain cycles.
    let form = r#"(progn
  (fset 'neovm--rb-create
    (lambda (cap) (list (make-vector cap nil) cap 0 0 0)))
  (fset 'neovm--rb-empty-p (lambda (rb) (= (nth 4 rb) 0)))
  (fset 'neovm--rb-full-p  (lambda (rb) (= (nth 4 rb) (nth 1 rb))))
  (fset 'neovm--rb-count   (lambda (rb) (nth 4 rb)))
  (fset 'neovm--rb-push
    (lambda (rb val)
      (let* ((vec (nth 0 rb)) (cap (nth 1 rb))
             (head (nth 2 rb)) (tail (nth 3 rb)) (cnt (nth 4 rb)))
        (aset vec tail val)
        (let ((nt (% (1+ tail) cap)))
          (if (= cnt cap)
              (list vec cap (% (1+ head) cap) nt cap)
            (list vec cap head nt (1+ cnt)))))))
  (fset 'neovm--rb-pop
    (lambda (rb)
      (if (= (nth 4 rb) 0) (cons nil rb)
        (let* ((vec (nth 0 rb)) (cap (nth 1 rb))
               (head (nth 2 rb)) (tail (nth 3 rb)) (cnt (nth 4 rb))
               (val (aref vec head)))
          (cons val (list vec cap (% (1+ head) cap) tail (1- cnt)))))))
  (fset 'neovm--rb-to-list
    (lambda (rb)
      (let* ((vec (nth 0 rb)) (cap (nth 1 rb))
             (head (nth 2 rb)) (cnt (nth 4 rb))
             (result nil) (i 0))
        (while (< i cnt)
          (setq result (cons (aref vec (% (+ head i) cap)) result))
          (setq i (1+ i)))
        (nreverse result))))

  (unwind-protect
      (let ((rb (funcall 'neovm--rb-create 3))
            (results nil))
        ;; Pop from empty
        (let* ((r0 (funcall 'neovm--rb-pop rb)))
          (setq results (cons (list 'empty-pop (car r0)
                                    (funcall 'neovm--rb-empty-p (cdr r0)))
                              results))
          (setq rb (cdr r0)))
        ;; Cycle 1: fill completely, drain completely
        (setq rb (funcall 'neovm--rb-push rb 10))
        (setq rb (funcall 'neovm--rb-push rb 20))
        (setq rb (funcall 'neovm--rb-push rb 30))
        (setq results (cons (list 'cycle1-full
                                  (funcall 'neovm--rb-to-list rb)
                                  (funcall 'neovm--rb-full-p rb))
                            results))
        (let* ((p1 (funcall 'neovm--rb-pop rb)))
          (setq rb (cdr p1))
          (let* ((p2 (funcall 'neovm--rb-pop rb)))
            (setq rb (cdr p2))
            (let* ((p3 (funcall 'neovm--rb-pop rb)))
              (setq rb (cdr p3))
              (setq results (cons (list 'cycle1-drain (car p1) (car p2) (car p3)
                                        (funcall 'neovm--rb-empty-p rb))
                                  results)))))
        ;; Cycle 2: fill and drain again (tests wrap-around after first cycle)
        (setq rb (funcall 'neovm--rb-push rb 40))
        (setq rb (funcall 'neovm--rb-push rb 50))
        (setq results (cons (list 'cycle2-partial (funcall 'neovm--rb-to-list rb))
                            results))
        (let* ((p4 (funcall 'neovm--rb-pop rb)))
          (setq rb (cdr p4))
          (setq rb (funcall 'neovm--rb-push rb 60))
          (setq rb (funcall 'neovm--rb-push rb 70))
          (setq results (cons (list 'cycle2-mixed (car p4)
                                    (funcall 'neovm--rb-to-list rb)
                                    (funcall 'neovm--rb-count rb))
                              results)))
        (nreverse results))
    (fmakunbound 'neovm--rb-create)
    (fmakunbound 'neovm--rb-empty-p)
    (fmakunbound 'neovm--rb-full-p)
    (fmakunbound 'neovm--rb-count)
    (fmakunbound 'neovm--rb-push)
    (fmakunbound 'neovm--rb-pop)
    (fmakunbound 'neovm--rb-to-list)))"#;
    let expect = expect_test::expect![[
        r#""OK ((empty-pop nil t) (cycle1-full (10 20 30) t) (cycle1-drain 10 20 30 t) (cycle2-partial (40 50)) (cycle2-mixed 40 (50 60 70) 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Large capacity with sequential fill and selective drain
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ring_buffer_large_sequential() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Push a sequence of numbers, overflow, then drain and verify ordering.
    let form = r#"(progn
  (fset 'neovm--rb-create
    (lambda (cap) (list (make-vector cap nil) cap 0 0 0)))
  (fset 'neovm--rb-count (lambda (rb) (nth 4 rb)))
  (fset 'neovm--rb-push
    (lambda (rb val)
      (let* ((vec (nth 0 rb)) (cap (nth 1 rb))
             (head (nth 2 rb)) (tail (nth 3 rb)) (cnt (nth 4 rb)))
        (aset vec tail val)
        (let ((nt (% (1+ tail) cap)))
          (if (= cnt cap)
              (list vec cap (% (1+ head) cap) nt cap)
            (list vec cap head nt (1+ cnt)))))))
  (fset 'neovm--rb-pop
    (lambda (rb)
      (if (= (nth 4 rb) 0) (cons nil rb)
        (let* ((vec (nth 0 rb)) (cap (nth 1 rb))
               (head (nth 2 rb)) (tail (nth 3 rb)) (cnt (nth 4 rb))
               (val (aref vec head)))
          (cons val (list vec cap (% (1+ head) cap) tail (1- cnt)))))))
  (fset 'neovm--rb-to-list
    (lambda (rb)
      (let* ((vec (nth 0 rb)) (cap (nth 1 rb))
             (head (nth 2 rb)) (cnt (nth 4 rb))
             (result nil) (i 0))
        (while (< i cnt)
          (setq result (cons (aref vec (% (+ head i) cap)) result))
          (setq i (1+ i)))
        (nreverse result))))

  (unwind-protect
      (let ((rb (funcall 'neovm--rb-create 8))
            (i 0))
        ;; Push 20 items into capacity-8 buffer; last 8 should survive
        (while (< i 20)
          (setq rb (funcall 'neovm--rb-push rb i))
          (setq i (1+ i)))
        (let ((after-fill (funcall 'neovm--rb-to-list rb))
              (count-fill (funcall 'neovm--rb-count rb)))
          ;; Drain 4 elements
          (let ((drained nil) (j 0))
            (while (< j 4)
              (let* ((r (funcall 'neovm--rb-pop rb)))
                (setq drained (cons (car r) drained))
                (setq rb (cdr r)))
              (setq j (1+ j)))
            (let ((after-drain (funcall 'neovm--rb-to-list rb))
                  (drained-rev (nreverse drained)))
              ;; Push 4 more
              (setq rb (funcall 'neovm--rb-push rb 100))
              (setq rb (funcall 'neovm--rb-push rb 101))
              (setq rb (funcall 'neovm--rb-push rb 102))
              (setq rb (funcall 'neovm--rb-push rb 103))
              (list after-fill count-fill drained-rev after-drain
                    (funcall 'neovm--rb-to-list rb)
                    (funcall 'neovm--rb-count rb))))))
    (fmakunbound 'neovm--rb-create)
    (fmakunbound 'neovm--rb-count)
    (fmakunbound 'neovm--rb-push)
    (fmakunbound 'neovm--rb-pop)
    (fmakunbound 'neovm--rb-to-list)))"#;
    let expect = expect_test::expect![[
        r#""OK ((12 13 14 15 16 17 18 19) 8 (12 13 14 15) (16 17 18 19) (16 17 18 19 100 101 102 103) 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Ring buffer with mixed data types (symbols, strings, numbers, lists)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ring_buffer_mixed_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Store heterogeneous data and verify correct retrieval order.
    let form = r#"(progn
  (fset 'neovm--rb-create
    (lambda (cap) (list (make-vector cap nil) cap 0 0 0)))
  (fset 'neovm--rb-count (lambda (rb) (nth 4 rb)))
  (fset 'neovm--rb-push
    (lambda (rb val)
      (let* ((vec (nth 0 rb)) (cap (nth 1 rb))
             (head (nth 2 rb)) (tail (nth 3 rb)) (cnt (nth 4 rb)))
        (aset vec tail val)
        (let ((nt (% (1+ tail) cap)))
          (if (= cnt cap)
              (list vec cap (% (1+ head) cap) nt cap)
            (list vec cap head nt (1+ cnt)))))))
  (fset 'neovm--rb-pop
    (lambda (rb)
      (if (= (nth 4 rb) 0) (cons nil rb)
        (let* ((vec (nth 0 rb)) (cap (nth 1 rb))
               (head (nth 2 rb)) (tail (nth 3 rb)) (cnt (nth 4 rb))
               (val (aref vec head)))
          (cons val (list vec cap (% (1+ head) cap) tail (1- cnt)))))))
  (fset 'neovm--rb-to-list
    (lambda (rb)
      (let* ((vec (nth 0 rb)) (cap (nth 1 rb))
             (head (nth 2 rb)) (cnt (nth 4 rb))
             (result nil) (i 0))
        (while (< i cnt)
          (setq result (cons (aref vec (% (+ head i) cap)) result))
          (setq i (1+ i)))
        (nreverse result))))

  (unwind-protect
      (let ((rb (funcall 'neovm--rb-create 4)))
        ;; Push various types
        (setq rb (funcall 'neovm--rb-push rb 42))
        (setq rb (funcall 'neovm--rb-push rb "hello"))
        (setq rb (funcall 'neovm--rb-push rb 'symbol))
        (setq rb (funcall 'neovm--rb-push rb '(1 2 3)))
        (let ((full-list (funcall 'neovm--rb-to-list rb)))
          ;; Overflow with more mixed types
          (setq rb (funcall 'neovm--rb-push rb nil))
          (setq rb (funcall 'neovm--rb-push rb t))
          (let ((overflow-list (funcall 'neovm--rb-to-list rb)))
            ;; Pop all and collect with type checks
            (let ((popped nil) (i 0))
              (while (< i 4)
                (let* ((r (funcall 'neovm--rb-pop rb)))
                  (setq popped (cons (list (car r) (type-of (car r))) popped))
                  (setq rb (cdr r)))
                (setq i (1+ i)))
              (list full-list overflow-list (nreverse popped)
                    (funcall 'neovm--rb-count rb))))))
    (fmakunbound 'neovm--rb-create)
    (fmakunbound 'neovm--rb-count)
    (fmakunbound 'neovm--rb-push)
    (fmakunbound 'neovm--rb-pop)
    (fmakunbound 'neovm--rb-to-list)))"#;
    let expect = expect_test::expect![[
        r#""OK ((42 \"hello\" symbol (1 2 3)) (symbol (1 2 3) nil t) ((symbol symbol) ((1 2 3) cons) (nil symbol) (t symbol)) 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

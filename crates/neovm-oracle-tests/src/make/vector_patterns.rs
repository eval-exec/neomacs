//! Advanced oracle parity tests for `make-vector`, `vector`, and `vconcat` patterns.
//!
//! Tests vector creation with various sizes and init values, nested vectors
//! as matrices, vector slicing via subseq, reversal, rotation, stack/queue
//! simulation, and vector comprehension patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Nested vectors as 2D matrix with row/column operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_matrix_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a matrix as a vector of row vectors. Implement transpose,
    // row sum, column sum, and matrix-vector multiply.
    let form = r#"(progn
  (fset 'neovm--vm-make-matrix
    (lambda (rows cols init)
      (let ((m (make-vector rows nil)))
        (dotimes (r rows)
          (aset m r (make-vector cols init)))
        m)))

  (fset 'neovm--vm-mref
    (lambda (m r c)
      (aref (aref m r) c)))

  (fset 'neovm--vm-mset
    (lambda (m r c val)
      (aset (aref m r) c val)))

  (fset 'neovm--vm-rows (lambda (m) (length m)))
  (fset 'neovm--vm-cols (lambda (m) (length (aref m 0))))

  (fset 'neovm--vm-transpose
    (lambda (m)
      (let* ((rows (funcall 'neovm--vm-rows m))
             (cols (funcall 'neovm--vm-cols m))
             (mt (funcall 'neovm--vm-make-matrix cols rows 0)))
        (dotimes (r rows)
          (dotimes (c cols)
            (funcall 'neovm--vm-mset mt c r (funcall 'neovm--vm-mref m r c))))
        mt)))

  (fset 'neovm--vm-row-sums
    (lambda (m)
      (let* ((rows (funcall 'neovm--vm-rows m))
             (cols (funcall 'neovm--vm-cols m))
             (sums (make-vector rows 0)))
        (dotimes (r rows)
          (let ((s 0))
            (dotimes (c cols)
              (setq s (+ s (funcall 'neovm--vm-mref m r c))))
            (aset sums r s)))
        sums)))

  (fset 'neovm--vm-col-sums
    (lambda (m)
      (let* ((rows (funcall 'neovm--vm-rows m))
             (cols (funcall 'neovm--vm-cols m))
             (sums (make-vector cols 0)))
        (dotimes (c cols)
          (let ((s 0))
            (dotimes (r rows)
              (setq s (+ s (funcall 'neovm--vm-mref m r c))))
            (aset sums c s)))
        sums)))

  (fset 'neovm--vm-mat-vec-mult
    (lambda (m v)
      "Multiply matrix M (n x k) by column vector V (length k)."
      (let* ((rows (funcall 'neovm--vm-rows m))
             (cols (funcall 'neovm--vm-cols m))
             (result (make-vector rows 0)))
        (dotimes (r rows)
          (let ((s 0))
            (dotimes (c cols)
              (setq s (+ s (* (funcall 'neovm--vm-mref m r c) (aref v c)))))
            (aset result r s)))
        result)))

  (unwind-protect
      (let ((m (funcall 'neovm--vm-make-matrix 3 4 0)))
        ;; Fill with a pattern: m[r][c] = r*10 + c
        (dotimes (r 3)
          (dotimes (c 4)
            (funcall 'neovm--vm-mset m r c (+ (* r 10) c))))
        (let ((mt (funcall 'neovm--vm-transpose m))
              (rs (funcall 'neovm--vm-row-sums m))
              (cs (funcall 'neovm--vm-col-sums m))
              (v [1 2 3 4])
              (mv nil))
          (setq mv (funcall 'neovm--vm-mat-vec-mult m v))
          (list
            ;; Original matrix row 0
            (append (aref m 0) nil)
            ;; Row 1
            (append (aref m 1) nil)
            ;; Row 2
            (append (aref m 2) nil)
            ;; Transpose dimensions
            (list (funcall 'neovm--vm-rows mt) (funcall 'neovm--vm-cols mt))
            ;; Transpose row 0 (was column 0)
            (append (aref mt 0) nil)
            ;; Row sums
            (append rs nil)
            ;; Column sums
            (append cs nil)
            ;; Matrix-vector product
            (append mv nil))))
    (fmakunbound 'neovm--vm-make-matrix)
    (fmakunbound 'neovm--vm-mref)
    (fmakunbound 'neovm--vm-mset)
    (fmakunbound 'neovm--vm-rows)
    (fmakunbound 'neovm--vm-cols)
    (fmakunbound 'neovm--vm-transpose)
    (fmakunbound 'neovm--vm-row-sums)
    (fmakunbound 'neovm--vm-col-sums)
    (fmakunbound 'neovm--vm-mat-vec-mult)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 2 3) (10 11 12 13) (20 21 22 23) (4 3) (0 10 20) (6 46 86) (30 33 36 39) (20 120 220))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Vector slicing, reversal, and rotation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_slice_reverse_rotate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement subseq-based slicing, in-place reversal, and rotation
    // (left and right) using vector operations.
    let form = r#"(progn
  (fset 'neovm--vp-reverse-vec
    (lambda (v)
      "Return a new reversed vector."
      (let* ((len (length v))
             (result (make-vector len nil)))
        (dotimes (i len)
          (aset result i (aref v (- len 1 i))))
        result)))

  (fset 'neovm--vp-rotate-left
    (lambda (v k)
      "Rotate vector V left by K positions."
      (let* ((len (length v))
             (k (% k len))
             (result (make-vector len nil)))
        (dotimes (i len)
          (aset result i (aref v (% (+ i k) len))))
        result)))

  (fset 'neovm--vp-rotate-right
    (lambda (v k)
      "Rotate vector V right by K positions."
      (funcall 'neovm--vp-rotate-left v (- (length v) (% k (length v))))))

  (fset 'neovm--vp-slice
    (lambda (v start end)
      "Extract sub-vector from START to END (exclusive)."
      (let* ((len (- end start))
             (result (make-vector len nil)))
        (dotimes (i len)
          (aset result i (aref v (+ start i))))
        result)))

  (fset 'neovm--vp-concat-vecs
    (lambda (v1 v2)
      "Concatenate two vectors."
      (vconcat v1 v2)))

  (unwind-protect
      (let ((v [10 20 30 40 50 60 70 80]))
        (list
          ;; Reverse
          (append (funcall 'neovm--vp-reverse-vec v) nil)
          ;; Double reverse = original
          (equal v (funcall 'neovm--vp-reverse-vec
                            (funcall 'neovm--vp-reverse-vec v)))
          ;; Rotate left by 3
          (append (funcall 'neovm--vp-rotate-left v 3) nil)
          ;; Rotate right by 3
          (append (funcall 'neovm--vp-rotate-right v 3) nil)
          ;; Rotate left by length = identity
          (equal v (funcall 'neovm--vp-rotate-left v (length v)))
          ;; Rotate left + right = identity
          (equal v (funcall 'neovm--vp-rotate-right
                            (funcall 'neovm--vp-rotate-left v 5) 5))
          ;; Slicing
          (append (funcall 'neovm--vp-slice v 2 5) nil)
          (append (funcall 'neovm--vp-slice v 0 1) nil)
          (append (funcall 'neovm--vp-slice v 7 8) nil)
          ;; Slice + concat = original
          (let* ((a (funcall 'neovm--vp-slice v 0 4))
                 (b (funcall 'neovm--vp-slice v 4 8)))
            (equal v (funcall 'neovm--vp-concat-vecs a b)))
          ;; Three-way rotation via reverse (Juggling algorithm)
          ;; rotate-left(v, k) = reverse(reverse(v[0..k]) ++ reverse(v[k..n]))
          (let* ((k 3)
                 (n (length v))
                 (left (funcall 'neovm--vp-reverse-vec (funcall 'neovm--vp-slice v 0 k)))
                 (right (funcall 'neovm--vp-reverse-vec (funcall 'neovm--vp-slice v k n)))
                 (combined (funcall 'neovm--vp-concat-vecs left right))
                 (rotated (funcall 'neovm--vp-reverse-vec combined)))
            (equal rotated (funcall 'neovm--vp-rotate-left v k)))))
    (fmakunbound 'neovm--vp-reverse-vec)
    (fmakunbound 'neovm--vp-rotate-left)
    (fmakunbound 'neovm--vp-rotate-right)
    (fmakunbound 'neovm--vp-slice)
    (fmakunbound 'neovm--vp-concat-vecs)))"#;
    let expect = expect_test::expect![[
        r#""OK ((80 70 60 50 40 30 20 10) t (40 50 60 70 80 10 20 30) (60 70 80 10 20 30 40 50) t t (30 40 50) (10) (80) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Vector as stack and queue
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_stack_queue() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement stack (LIFO) and queue (FIFO) using vectors with
    // a top/head/tail pointer. Fixed capacity.
    let form = r#"(progn
  ;; Stack: push to top, pop from top
  (fset 'neovm--vp-stack-new
    (lambda (capacity)
      (list (make-vector capacity nil) 0 capacity)))

  (fset 'neovm--vp-stack-push
    (lambda (stack val)
      (let ((buf (nth 0 stack))
            (top (nth 1 stack)))
        (aset buf top val)
        (setcar (nthcdr 1 stack) (1+ top))
        stack)))

  (fset 'neovm--vp-stack-pop
    (lambda (stack)
      (let* ((top (1- (nth 1 stack)))
             (buf (nth 0 stack))
             (val (aref buf top)))
        (setcar (nthcdr 1 stack) top)
        val)))

  (fset 'neovm--vp-stack-size
    (lambda (stack) (nth 1 stack)))

  ;; Queue: circular buffer with head and tail indices
  (fset 'neovm--vp-queue-new
    (lambda (capacity)
      (list (make-vector (1+ capacity) nil) 0 0 (1+ capacity))))

  (fset 'neovm--vp-queue-enqueue
    (lambda (q val)
      (let ((buf (nth 0 q))
            (tail (nth 2 q))
            (cap (nth 3 q)))
        (aset buf tail val)
        (setcar (nthcdr 2 q) (% (1+ tail) cap))
        q)))

  (fset 'neovm--vp-queue-dequeue
    (lambda (q)
      (let* ((buf (nth 0 q))
             (head (nth 1 q))
             (cap (nth 3 q))
             (val (aref buf head)))
        (setcar (nthcdr 1 q) (% (1+ head) cap))
        val)))

  (fset 'neovm--vp-queue-size
    (lambda (q)
      (let ((head (nth 1 q))
            (tail (nth 2 q))
            (cap (nth 3 q)))
        (% (+ (- tail head) cap) cap))))

  (unwind-protect
      (let ((stk (funcall 'neovm--vp-stack-new 10))
            (que (funcall 'neovm--vp-queue-new 10)))
        ;; Stack operations
        (funcall 'neovm--vp-stack-push stk 'a)
        (funcall 'neovm--vp-stack-push stk 'b)
        (funcall 'neovm--vp-stack-push stk 'c)
        (funcall 'neovm--vp-stack-push stk 'd)
        (let ((s1 (funcall 'neovm--vp-stack-size stk))
              (p1 (funcall 'neovm--vp-stack-pop stk))
              (p2 (funcall 'neovm--vp-stack-pop stk))
              (s2 (funcall 'neovm--vp-stack-size stk)))
          ;; Queue operations
          (funcall 'neovm--vp-queue-enqueue que 10)
          (funcall 'neovm--vp-queue-enqueue que 20)
          (funcall 'neovm--vp-queue-enqueue que 30)
          (funcall 'neovm--vp-queue-enqueue que 40)
          (funcall 'neovm--vp-queue-enqueue que 50)
          (let ((q1 (funcall 'neovm--vp-queue-size que))
                (d1 (funcall 'neovm--vp-queue-dequeue que))
                (d2 (funcall 'neovm--vp-queue-dequeue que))
                (q2 (funcall 'neovm--vp-queue-size que)))
            ;; Enqueue more after dequeue (wrap-around test)
            (funcall 'neovm--vp-queue-enqueue que 60)
            (funcall 'neovm--vp-queue-enqueue que 70)
            (let ((remaining nil))
              (while (> (funcall 'neovm--vp-queue-size que) 0)
                (setq remaining (cons (funcall 'neovm--vp-queue-dequeue que) remaining)))
              (list
                ;; Stack: LIFO order
                (list s1 p1 p2 s2)
                ;; Remaining stack items (pop rest)
                (list (funcall 'neovm--vp-stack-pop stk)
                      (funcall 'neovm--vp-stack-pop stk))
                ;; Queue: FIFO order
                (list q1 d1 d2 q2)
                ;; Remaining queue items (wrap-around included)
                (nreverse remaining))))))
    (fmakunbound 'neovm--vp-stack-new)
    (fmakunbound 'neovm--vp-stack-push)
    (fmakunbound 'neovm--vp-stack-pop)
    (fmakunbound 'neovm--vp-stack-size)
    (fmakunbound 'neovm--vp-queue-new)
    (fmakunbound 'neovm--vp-queue-enqueue)
    (fmakunbound 'neovm--vp-queue-dequeue)
    (fmakunbound 'neovm--vp-queue-size)))"#;
    let expect = expect_test::expect![[r#""OK ((4 d c 2) (b a) (5 10 20 3) (30 40 50 60 70))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Vector comprehension / map / filter / reduce patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_comprehension_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Functional-style vector operations: map, filter, reduce, zip, flat-map.
    let form = r#"(progn
  (fset 'neovm--vp-vmap
    (lambda (fn v)
      (let* ((len (length v))
             (result (make-vector len nil)))
        (dotimes (i len)
          (aset result i (funcall fn (aref v i))))
        result)))

  (fset 'neovm--vp-vfilter
    (lambda (pred v)
      (let ((tmp nil))
        (dotimes (i (length v))
          (when (funcall pred (aref v i))
            (setq tmp (cons (aref v i) tmp))))
        (vconcat (nreverse tmp)))))

  (fset 'neovm--vp-vreduce
    (lambda (fn init v)
      (let ((acc init))
        (dotimes (i (length v))
          (setq acc (funcall fn acc (aref v i))))
        acc)))

  (fset 'neovm--vp-vzip
    (lambda (v1 v2)
      (let* ((len (min (length v1) (length v2)))
             (result (make-vector len nil)))
        (dotimes (i len)
          (aset result i (cons (aref v1 i) (aref v2 i))))
        result)))

  (fset 'neovm--vp-vflatmap
    (lambda (fn v)
      "Apply FN to each element (returns a vector), concat all results."
      (let ((parts nil))
        (dotimes (i (length v))
          (setq parts (cons (funcall fn (aref v i)) parts)))
        (apply #'vconcat (nreverse parts)))))

  (unwind-protect
      (let ((nums [1 2 3 4 5 6 7 8 9 10]))
        (list
          ;; Map: square each element
          (append (funcall 'neovm--vp-vmap (lambda (x) (* x x)) nums) nil)
          ;; Filter: keep evens
          (append (funcall 'neovm--vp-vfilter #'evenp nums) nil)
          ;; Reduce: sum
          (funcall 'neovm--vp-vreduce #'+ 0 nums)
          ;; Reduce: product
          (funcall 'neovm--vp-vreduce #'* 1 [1 2 3 4 5])
          ;; Zip
          (append (funcall 'neovm--vp-vzip [a b c] [1 2 3]) nil)
          ;; Zip with different lengths (shorter wins)
          (append (funcall 'neovm--vp-vzip [x y z] [10 20]) nil)
          ;; Flatmap: each number n -> [n n*10]
          (append (funcall 'neovm--vp-vflatmap
                           (lambda (x) (vector x (* x 10)))
                           [1 2 3]) nil)
          ;; Composition: filter evens, square them, sum
          (funcall 'neovm--vp-vreduce #'+ 0
                   (funcall 'neovm--vp-vmap (lambda (x) (* x x))
                            (funcall 'neovm--vp-vfilter #'evenp nums)))
          ;; Map over strings (vector of chars)
          (concat (funcall 'neovm--vp-vmap
                           (lambda (ch) (if (and (>= ch ?a) (<= ch ?z))
                                            (- ch 32) ch))
                           "hello"))))
    (fmakunbound 'neovm--vp-vmap)
    (fmakunbound 'neovm--vp-vfilter)
    (fmakunbound 'neovm--vp-vreduce)
    (fmakunbound 'neovm--vp-vzip)
    (fmakunbound 'neovm--vp-vflatmap)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 4 9 16 25 36 49 64 81 100) (2 4 6 8 10) 55 120 ((a . 1) (b . 2) (c . 3)) ((x . 10) (y . 20)) (1 10 2 20 3 30) 220 \"HELLO\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// vconcat advanced: building vectors from heterogeneous sources
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vconcat_advanced_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Complex vconcat usage: interleaving, partitioning, flattening nested,
    // and building vectors incrementally.
    let form = r#"(progn
  (fset 'neovm--vp-interleave
    (lambda (v1 v2)
      "Interleave elements of V1 and V2: [a1 b1 a2 b2 ...]."
      (let ((parts nil)
            (len (min (length v1) (length v2))))
        (dotimes (i len)
          (setq parts (cons (vector (aref v2 i)) parts))
          (setq parts (cons (vector (aref v1 i)) parts)))
        ;; Append remaining from longer vector
        (let ((longer (if (> (length v1) (length v2)) v1 v2))
              (from len))
          (when (> (length longer) len)
            (dotimes (i (- (length longer) len))
              (setq parts (cons (vector (aref longer (+ from i))) parts)))))
        (apply #'vconcat (nreverse parts)))))

  (fset 'neovm--vp-partition
    (lambda (v size)
      "Split V into sub-vectors of SIZE. Last chunk may be smaller."
      (let ((chunks nil)
            (i 0)
            (len (length v)))
        (while (< i len)
          (let* ((end (min (+ i size) len))
                 (chunk (make-vector (- end i) nil)))
            (dotimes (j (- end i))
              (aset chunk j (aref v (+ i j))))
            (setq chunks (cons chunk chunks))
            (setq i end)))
        (nreverse chunks))))

  (fset 'neovm--vp-flatten
    (lambda (nested)
      "Flatten a list of vectors into one vector."
      (apply #'vconcat nested)))

  (unwind-protect
      (list
        ;; Interleave equal-length
        (append (funcall 'neovm--vp-interleave [1 2 3] [a b c]) nil)
        ;; Interleave unequal
        (append (funcall 'neovm--vp-interleave [1 2 3 4 5] [a b]) nil)
        ;; Partition
        (mapcar (lambda (chunk) (append chunk nil))
                (funcall 'neovm--vp-partition [1 2 3 4 5 6 7 8 9 10] 3))
        ;; Partition exact multiple
        (mapcar (lambda (chunk) (append chunk nil))
                (funcall 'neovm--vp-partition [1 2 3 4 5 6] 2))
        ;; Partition size 1
        (mapcar (lambda (chunk) (append chunk nil))
                (funcall 'neovm--vp-partition [a b c] 1))
        ;; Flatten partitioned = original
        (equal [1 2 3 4 5 6 7 8 9 10]
               (funcall 'neovm--vp-flatten
                        (funcall 'neovm--vp-partition [1 2 3 4 5 6 7 8 9 10] 4)))
        ;; Build vector incrementally with vconcat
        (let ((result []))
          (dotimes (i 5)
            (setq result (vconcat result (vector (* i i)))))
          (append result nil))
        ;; vconcat with bool-vector and string
        (append (vconcat [1 2] "AB" [3 4]) nil))
    (fmakunbound 'neovm--vp-interleave)
    (fmakunbound 'neovm--vp-partition)
    (fmakunbound 'neovm--vp-flatten)))"#;
    let expect = expect_test::expect![[
        r#""OK ((a 1 b 2 c 3) (a 1 b 2 3 4 5) ((1 2 3) (4 5 6) (7 8 9) (10)) ((1 2) (3 4) (5 6)) ((a) (b) (c)) t (0 1 4 9 16) (1 2 65 66 3 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Vector-based ring buffer with overwrite semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_ring_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A fixed-size ring buffer that overwrites oldest entries when full.
    // Supports add, peek-newest, peek-oldest, contents-in-order.
    let form = r#"(progn
  (fset 'neovm--vp-ring-new
    (lambda (capacity)
      "Create ring buffer: (vector head count capacity)."
      (list (make-vector capacity nil) 0 0 capacity)))

  (fset 'neovm--vp-ring-add
    (lambda (ring val)
      (let* ((buf (nth 0 ring))
             (head (nth 1 ring))
             (count (nth 2 ring))
             (cap (nth 3 ring))
             (write-pos (% (+ head count) cap)))
        (if (< count cap)
            (progn
              (aset buf write-pos val)
              (setcar (nthcdr 2 ring) (1+ count)))
          ;; Full: overwrite oldest, advance head
          (aset buf head val)
          (setcar (nthcdr 1 ring) (% (1+ head) cap)))
        ring)))

  (fset 'neovm--vp-ring-contents
    (lambda (ring)
      "Return contents oldest-to-newest as a list."
      (let* ((buf (nth 0 ring))
             (head (nth 1 ring))
             (count (nth 2 ring))
             (cap (nth 3 ring))
             (result nil))
        (dotimes (i count)
          (setq result (cons (aref buf (% (+ head i) cap)) result)))
        (nreverse result))))

  (fset 'neovm--vp-ring-newest
    (lambda (ring)
      (let* ((buf (nth 0 ring))
             (head (nth 1 ring))
             (count (nth 2 ring))
             (cap (nth 3 ring)))
        (aref buf (% (+ head count -1) cap)))))

  (fset 'neovm--vp-ring-oldest
    (lambda (ring)
      (aref (nth 0 ring) (nth 1 ring))))

  (unwind-protect
      (let ((r (funcall 'neovm--vp-ring-new 4)))
        ;; Add 1 2 3 (not yet full)
        (funcall 'neovm--vp-ring-add r 1)
        (funcall 'neovm--vp-ring-add r 2)
        (funcall 'neovm--vp-ring-add r 3)
        (let ((c1 (funcall 'neovm--vp-ring-contents r))
              (n1 (funcall 'neovm--vp-ring-newest r))
              (o1 (funcall 'neovm--vp-ring-oldest r)))
          ;; Fill to capacity
          (funcall 'neovm--vp-ring-add r 4)
          (let ((c2 (funcall 'neovm--vp-ring-contents r)))
            ;; Overwrite: add 5, 6, 7 (wraps around)
            (funcall 'neovm--vp-ring-add r 5)
            (funcall 'neovm--vp-ring-add r 6)
            (funcall 'neovm--vp-ring-add r 7)
            (let ((c3 (funcall 'neovm--vp-ring-contents r))
                  (n3 (funcall 'neovm--vp-ring-newest r))
                  (o3 (funcall 'neovm--vp-ring-oldest r)))
              ;; Add more to fully cycle
              (funcall 'neovm--vp-ring-add r 8)
              (funcall 'neovm--vp-ring-add r 9)
              (let ((c4 (funcall 'neovm--vp-ring-contents r)))
                (list
                  c1 n1 o1      ;; (1 2 3) 3 1
                  c2             ;; (1 2 3 4)
                  c3 n3 o3      ;; (4 5 6 7) 7 4
                  c4))))))       ;; (6 7 8 9)
    (fmakunbound 'neovm--vp-ring-new)
    (fmakunbound 'neovm--vp-ring-add)
    (fmakunbound 'neovm--vp-ring-contents)
    (fmakunbound 'neovm--vp-ring-newest)
    (fmakunbound 'neovm--vp-ring-oldest)))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3) 3 1 (1 2 3 4) (4 5 6 7) 7 4 (6 7 8 9))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Vector-based sparse set (fast membership + iteration)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_sparse_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Sparse set: O(1) add/remove/contains, O(n) iteration.
    // Two vectors: dense (packed elements) and sparse (element -> dense index).
    let form = r#"(progn
  (fset 'neovm--vp-sset-new
    (lambda (universe-size)
      "Create sparse set for elements 0..universe-size-1."
      (list (make-vector universe-size -1)   ;; sparse: element -> index in dense
            (make-vector universe-size nil)   ;; dense: packed elements
            0                                 ;; count
            universe-size)))

  (fset 'neovm--vp-sset-contains
    (lambda (ss elem)
      (let ((sparse (nth 0 ss))
            (dense (nth 1 ss))
            (count (nth 2 ss)))
        (let ((idx (aref sparse elem)))
          (and (>= idx 0) (< idx count) (= (aref dense idx) elem))))))

  (fset 'neovm--vp-sset-add
    (lambda (ss elem)
      (unless (funcall 'neovm--vp-sset-contains ss elem)
        (let ((sparse (nth 0 ss))
              (dense (nth 1 ss))
              (count (nth 2 ss)))
          (aset dense count elem)
          (aset sparse elem count)
          (setcar (nthcdr 2 ss) (1+ count))))
      ss))

  (fset 'neovm--vp-sset-remove
    (lambda (ss elem)
      (when (funcall 'neovm--vp-sset-contains ss elem)
        (let* ((sparse (nth 0 ss))
               (dense (nth 1 ss))
               (count (nth 2 ss))
               (idx (aref sparse elem))
               (last-elem (aref dense (1- count))))
          ;; Swap with last
          (aset dense idx last-elem)
          (aset sparse last-elem idx)
          (aset sparse elem -1)
          (setcar (nthcdr 2 ss) (1- count))))
      ss))

  (fset 'neovm--vp-sset-elements
    (lambda (ss)
      (let ((dense (nth 1 ss))
            (count (nth 2 ss))
            (result nil))
        (dotimes (i count)
          (setq result (cons (aref dense i) result)))
        (sort (nreverse result) #'<))))

  (unwind-protect
      (let ((ss (funcall 'neovm--vp-sset-new 20)))
        ;; Add elements
        (funcall 'neovm--vp-sset-add ss 5)
        (funcall 'neovm--vp-sset-add ss 12)
        (funcall 'neovm--vp-sset-add ss 3)
        (funcall 'neovm--vp-sset-add ss 18)
        (funcall 'neovm--vp-sset-add ss 7)
        (let ((e1 (funcall 'neovm--vp-sset-elements ss))
              (c1 (nth 2 ss)))
          ;; Membership tests
          (let ((m1 (funcall 'neovm--vp-sset-contains ss 5))
                (m2 (funcall 'neovm--vp-sset-contains ss 6))
                (m3 (funcall 'neovm--vp-sset-contains ss 18)))
            ;; Remove element
            (funcall 'neovm--vp-sset-remove ss 12)
            (let ((e2 (funcall 'neovm--vp-sset-elements ss))
                  (c2 (nth 2 ss)))
              ;; Add duplicate (should be no-op)
              (funcall 'neovm--vp-sset-add ss 5)
              (let ((c3 (nth 2 ss)))
                ;; Remove and re-add
                (funcall 'neovm--vp-sset-remove ss 3)
                (funcall 'neovm--vp-sset-add ss 0)
                (funcall 'neovm--vp-sset-add ss 19)
                (let ((e3 (funcall 'neovm--vp-sset-elements ss)))
                  (list
                    e1 c1       ;; (3 5 7 12 18), 5
                    m1 m2 m3    ;; t nil t
                    e2 c2       ;; (3 5 7 18), 4
                    c3           ;; 4 (duplicate no-op)
                    e3)))))))    ;; (0 5 7 18 19)
    (fmakunbound 'neovm--vp-sset-new)
    (fmakunbound 'neovm--vp-sset-contains)
    (fmakunbound 'neovm--vp-sset-add)
    (fmakunbound 'neovm--vp-sset-remove)
    (fmakunbound 'neovm--vp-sset-elements)))"#;
    let expect =
        expect_test::expect![[r#""OK ((3 5 7 12 18) 5 t nil t (3 5 7 18) 4 4 (0 5 7 18 19))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

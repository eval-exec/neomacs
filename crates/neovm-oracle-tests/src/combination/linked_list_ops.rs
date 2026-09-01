//! Oracle parity tests for complex linked list operations.
//!
//! Covers: list rotation, list interleaving (zip), run-length encoding/decoding,
//! list partitioning by predicate, group-by operation, sliding window over list,
//! and frequency histogram construction.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// List rotation: left and right by N positions, with wrap-around
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_linkedlist_rotation_advanced() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Rotate left and right with wrap-around, double-rotation for identity,
    // rotation of sublists, and rotation combined with reversal.
    let form = r#"(progn
  (fset 'neovm--ll-rotate-left
    (lambda (lst n)
      (if (or (null lst) (zerop (length lst))) lst
        (let* ((len (length lst))
               (k (mod n len)))
          (if (zerop k) (copy-sequence lst)
            (append (nthcdr k lst)
                    (let ((prefix nil) (i 0) (l lst))
                      (while (< i k)
                        (setq prefix (cons (car l) prefix)
                              l (cdr l) i (1+ i)))
                      (nreverse prefix))))))))
  (fset 'neovm--ll-rotate-right
    (lambda (lst n)
      (if (or (null lst) (zerop (length lst))) lst
        (funcall 'neovm--ll-rotate-left lst (- (length lst) (mod n (length lst)))))))
  (unwind-protect
      (list
        ;; Basic left rotation
        (funcall 'neovm--ll-rotate-left '(1 2 3 4 5) 2)
        ;; Right rotation
        (funcall 'neovm--ll-rotate-right '(1 2 3 4 5) 2)
        ;; Rotation by length = identity
        (funcall 'neovm--ll-rotate-left '(a b c d) 4)
        ;; Rotation by more than length wraps
        (funcall 'neovm--ll-rotate-left '(a b c) 7)
        ;; Rotate right then left by same amount = identity
        (let ((lst '(x y z w)))
          (equal lst (funcall 'neovm--ll-rotate-left
                              (funcall 'neovm--ll-rotate-right lst 3) 3)))
        ;; Rotate empty list
        (funcall 'neovm--ll-rotate-left nil 5)
        ;; Rotate single element
        (funcall 'neovm--ll-rotate-left '(42) 100)
        ;; Successive rotations: rotate-left by 1, N times = rotate-left by N
        (let ((lst '(a b c d e f))
              (result '(a b c d e f)))
          (dotimes (_ 3)
            (setq result (funcall 'neovm--ll-rotate-left result 1)))
          (equal result (funcall 'neovm--ll-rotate-left lst 3))))
    (fmakunbound 'neovm--ll-rotate-left)
    (fmakunbound 'neovm--ll-rotate-right)))"#;
    let expect =
        expect_test::expect![[r#""OK ((3 4 5 1 2) (4 5 1 2 3) (a b c d) (b c a) t nil (42) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// List interleaving: zip, zip-longest, and multi-list interleave
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_linkedlist_interleave_zip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Interleave elements from multiple lists, handling unequal lengths,
    // and deinterleave back.
    let form = r#"(progn
  (fset 'neovm--ll-interleave
    (lambda (a b)
      "Interleave elements: (1 2 3) (a b c) -> (1 a 2 b 3 c)"
      (let ((result nil))
        (while (or a b)
          (when a
            (setq result (cons (car a) result)
                  a (cdr a)))
          (when b
            (setq result (cons (car b) result)
                  b (cdr b))))
        (nreverse result))))
  (fset 'neovm--ll-deinterleave
    (lambda (lst)
      "Split interleaved list back into two: (1 a 2 b) -> ((1 2) (a b))"
      (let ((evens nil) (odds nil) (idx 0))
        (dolist (x lst)
          (if (= (mod idx 2) 0)
              (setq evens (cons x evens))
            (setq odds (cons x odds)))
          (setq idx (1+ idx)))
        (list (nreverse evens) (nreverse odds)))))
  (fset 'neovm--ll-zip-longest
    (lambda (a b default)
      "Zip with padding for shorter list using DEFAULT."
      (let ((result nil)
            (la (length a)) (lb (length b))
            (maxlen (max (length a) (length b))))
        (dotimes (i maxlen)
          (setq result (cons (cons (if (< i la) (nth i a) default)
                                   (if (< i lb) (nth i b) default))
                             result)))
        (nreverse result))))
  (unwind-protect
      (list
        ;; Basic interleave
        (funcall 'neovm--ll-interleave '(1 2 3) '(a b c))
        ;; Unequal lengths
        (funcall 'neovm--ll-interleave '(1 2 3 4 5) '(a b))
        (funcall 'neovm--ll-interleave '(x) '(1 2 3 4))
        ;; Empty lists
        (funcall 'neovm--ll-interleave nil '(a b c))
        (funcall 'neovm--ll-interleave '(1 2) nil)
        ;; Deinterleave roundtrip
        (let* ((a '(1 2 3)) (b '(a b c))
               (merged (funcall 'neovm--ll-interleave a b))
               (split (funcall 'neovm--ll-deinterleave merged)))
          (list (equal (car split) a) (equal (cadr split) b)))
        ;; Zip-longest with default
        (funcall 'neovm--ll-zip-longest '(1 2 3) '(a b c d e) 'nil)
        ;; Multi-round interleave: interleave 3 lists by chaining
        (let* ((l1 '(1 4 7)) (l2 '(2 5 8)) (l3 '(3 6 9))
               (merged12 (funcall 'neovm--ll-interleave l1 l2)))
          ;; Not a true 3-way interleave, but test the chaining
          (funcall 'neovm--ll-interleave merged12 l3)))
    (fmakunbound 'neovm--ll-interleave)
    (fmakunbound 'neovm--ll-deinterleave)
    (fmakunbound 'neovm--ll-zip-longest)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 a 2 b 3 c) (1 a 2 b 3 4 5) (x 1 2 3 4) (a b c) (1 2) (t t) ((1 . a) (2 . b) (3 . c) (nil . d) (nil . e)) (1 3 2 6 4 9 5 7 8))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Run-length encoding and decoding with nested structures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_linkedlist_rle_advanced() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // RLE encode/decode, verify roundtrip, handle nested list elements,
    // compute compression ratio.
    let form = r#"(progn
  (fset 'neovm--ll-rle-encode
    (lambda (lst)
      (if (null lst) nil
        (let ((result nil) (cur (car lst)) (cnt 1))
          (dolist (x (cdr lst))
            (if (equal x cur)
                (setq cnt (1+ cnt))
              (setq result (cons (list cur cnt) result)
                    cur x cnt 1)))
          (nreverse (cons (list cur cnt) result))))))
  (fset 'neovm--ll-rle-decode
    (lambda (encoded)
      (let ((result nil))
        (dolist (pair encoded)
          (let ((elem (car pair)) (n (cadr pair)))
            (dotimes (_ n)
              (setq result (cons elem result)))))
        (nreverse result))))
  (fset 'neovm--ll-rle-compression-ratio
    (lambda (lst)
      "Ratio of encoded length to original length."
      (if (null lst) 0
        (let ((orig-len (length lst))
              (enc-len (length (funcall 'neovm--ll-rle-encode lst))))
          (list orig-len enc-len)))))
  (unwind-protect
      (list
        ;; Basic encode
        (funcall 'neovm--ll-rle-encode '(a a a b b c c c c d))
        ;; Decode
        (funcall 'neovm--ll-rle-decode '((a 3) (b 2) (c 4) (d 1)))
        ;; Roundtrip
        (let ((orig '(x x y y y z x x x x)))
          (equal orig (funcall 'neovm--ll-rle-decode
                               (funcall 'neovm--ll-rle-encode orig))))
        ;; No consecutive duplicates
        (funcall 'neovm--ll-rle-encode '(a b c d e))
        ;; All identical
        (funcall 'neovm--ll-rle-encode '(q q q q q q q q))
        ;; String elements (equal comparison)
        (funcall 'neovm--ll-rle-encode '("hi" "hi" "lo" "lo" "lo" "hi"))
        ;; Compression ratio comparison
        (let ((repetitive '(a a a a a a a a a a b b b b b))
              (diverse '(a b c d e f g h i j)))
          (list (funcall 'neovm--ll-rle-compression-ratio repetitive)
                (funcall 'neovm--ll-rle-compression-ratio diverse)))
        ;; Empty and single-element
        (list (funcall 'neovm--ll-rle-encode nil)
              (funcall 'neovm--ll-rle-encode '(z))))
    (fmakunbound 'neovm--ll-rle-encode)
    (fmakunbound 'neovm--ll-rle-decode)
    (fmakunbound 'neovm--ll-rle-compression-ratio)))"#;
    let expect = expect_test::expect![[
        r#""OK (((a 3) (b 2) (c 4) (d 1)) (a a a b b c c c c d) t ((a 1) (b 1) (c 1) (d 1) (e 1)) ((q 8)) ((\"hi\" 2) (\"lo\" 3) (\"hi\" 1)) ((15 2) (10 10)) (nil ((z 1))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// List partitioning: split by predicate into multiple sublists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_linkedlist_partition_by_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Partition list into sublists based on a predicate, preserving order.
    // Also partition into N buckets by a classifier function.
    let form = r#"(progn
  (fset 'neovm--ll-partition
    (lambda (pred lst)
      "Split LST into (passing . failing) based on PRED."
      (let ((pass nil) (fail nil))
        (dolist (x lst)
          (if (funcall pred x)
              (setq pass (cons x pass))
            (setq fail (cons x fail))))
        (cons (nreverse pass) (nreverse fail)))))
  (fset 'neovm--ll-partition-n
    (lambda (classifier lst)
      "Partition LST into buckets keyed by CLASSIFIER result.
       Returns alist of (key . elements)."
      (let ((buckets nil))
        (dolist (x lst)
          (let* ((key (funcall classifier x))
                 (bucket (assoc key buckets)))
            (if bucket
                (setcdr bucket (append (cdr bucket) (list x)))
              (setq buckets (cons (cons key (list x)) buckets)))))
        (nreverse buckets))))
  (fset 'neovm--ll-chunk
    (lambda (lst n)
      "Split LST into sublists of size N."
      (let ((result nil) (current nil) (count 0))
        (dolist (x lst)
          (setq current (cons x current) count (1+ count))
          (when (= count n)
            (setq result (cons (nreverse current) result)
                  current nil count 0)))
        (when current
          (setq result (cons (nreverse current) result)))
        (nreverse result))))
  (unwind-protect
      (list
        ;; Partition evens and odds
        (funcall 'neovm--ll-partition 'evenp '(1 2 3 4 5 6 7 8 9 10))
        ;; Partition positives and non-positives
        (funcall 'neovm--ll-partition
                 (lambda (x) (> x 0))
                 '(-3 -1 0 2 5 -7 8 0 1))
        ;; All pass
        (funcall 'neovm--ll-partition 'numberp '(1 2 3 4 5))
        ;; None pass
        (funcall 'neovm--ll-partition 'stringp '(1 2 3 4 5))
        ;; Partition-n: group numbers by remainder mod 3
        (funcall 'neovm--ll-partition-n
                 (lambda (x) (mod x 3))
                 '(1 2 3 4 5 6 7 8 9 10 11 12))
        ;; Verify: union of partitions equals original (sorted)
        (let* ((lst '(5 3 8 1 9 2 7 4 6))
               (parts (funcall 'neovm--ll-partition (lambda (x) (> x 5)) lst))
               (reconstructed (append (car parts) (cdr parts))))
          (equal (sort (copy-sequence lst) '<)
                 (sort (copy-sequence reconstructed) '<)))
        ;; Chunk into sublists of size 3
        (funcall 'neovm--ll-chunk '(a b c d e f g h) 3)
        ;; Chunk with exact division
        (funcall 'neovm--ll-chunk '(1 2 3 4 5 6) 2))
    (fmakunbound 'neovm--ll-partition)
    (fmakunbound 'neovm--ll-partition-n)
    (fmakunbound 'neovm--ll-chunk)))"#;
    let expect = expect_test::expect![[
        r#""OK (((2 4 6 8 10) 1 3 5 7 9) ((2 5 8 1) -3 -1 0 -7 0) ((1 2 3 4 5)) (nil 1 2 3 4 5) ((1 1 4 7 10) (2 2 5 8 11) (0 3 6 9 12)) t ((a b c) (d e f) (g h)) ((1 2) (3 4) (5 6)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Group-by operation: group list elements by a key function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_linkedlist_group_by() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Group elements by a key function using hash tables for O(n) grouping,
    // then sort groups by key for deterministic output.
    let form = r#"(progn
  (fset 'neovm--ll-group-by
    (lambda (key-fn lst)
      "Group elements of LST by KEY-FN. Returns sorted alist."
      (let ((groups (make-hash-table :test 'equal)))
        (dolist (x lst)
          (let ((k (funcall key-fn x)))
            (puthash k (cons x (gethash k groups nil)) groups)))
        (let ((result nil))
          (maphash (lambda (k v)
                     (setq result (cons (cons k (nreverse v)) result)))
                   groups)
          (sort result (lambda (a b)
                         (string< (format "%s" (car a))
                                  (format "%s" (car b)))))))))
  (unwind-protect
      (list
        ;; Group numbers by parity
        (funcall 'neovm--ll-group-by
                 (lambda (x) (if (= (mod x 2) 0) 'even 'odd))
                 '(1 2 3 4 5 6 7 8))
        ;; Group strings by length
        (funcall 'neovm--ll-group-by
                 'length
                 '("a" "bb" "ccc" "dd" "e" "fff" "gg"))
        ;; Group by first character
        (funcall 'neovm--ll-group-by
                 (lambda (s) (aref s 0))
                 '("apple" "avocado" "banana" "blueberry" "cherry"))
        ;; All same key
        (funcall 'neovm--ll-group-by
                 (lambda (_) 'same)
                 '(1 2 3 4 5))
        ;; All unique keys
        (funcall 'neovm--ll-group-by
                 'identity
                 '(a b c d e))
        ;; Empty list
        (funcall 'neovm--ll-group-by 'identity nil)
        ;; Group numbers by sign
        (funcall 'neovm--ll-group-by
                 (lambda (x) (cond ((> x 0) 'pos) ((< x 0) 'neg) (t 'zero)))
                 '(3 -1 0 5 -2 0 7 -4 0 1)))
    (fmakunbound 'neovm--ll-group-by)))"#;
    let expect = expect_test::expect![[
        r#""OK (((even 2 4 6 8) (odd 1 3 5 7)) ((1 \"a\" \"e\") (2 \"bb\" \"dd\" \"gg\") (3 \"ccc\" \"fff\")) ((97 \"apple\" \"avocado\") (98 \"banana\" \"blueberry\") (99 \"cherry\")) ((same 1 2 3 4 5)) ((a a) (b b) (c c) (d d) (e e)) nil ((neg -1 -2 -4) (pos 3 5 7 1) (zero 0 0 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sliding window: extract all windows of size K from a list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_linkedlist_sliding_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generate all contiguous sublists of size K, compute moving average,
    // and find window with maximum sum.
    let form = r#"(progn
  (fset 'neovm--ll-windows
    (lambda (lst k)
      "Return all contiguous sublists of size K from LST."
      (if (< (length lst) k) nil
        (let ((result nil) (n (- (length lst) (1- k))))
          (dotimes (i n)
            (let ((window nil) (tail (nthcdr i lst)))
              (dotimes (_ k)
                (setq window (cons (car tail) window)
                      tail (cdr tail)))
              (setq result (cons (nreverse window) result))))
          (nreverse result)))))
  (fset 'neovm--ll-moving-avg
    (lambda (lst k)
      "Compute moving average with window size K."
      (let ((windows (funcall 'neovm--ll-windows lst k)))
        (mapcar (lambda (w)
                  (/ (float (apply '+ w)) k))
                windows))))
  (fset 'neovm--ll-max-window
    (lambda (lst k)
      "Find the window of size K with the maximum sum."
      (let ((windows (funcall 'neovm--ll-windows lst k))
            (best nil) (best-sum nil))
        (dolist (w windows)
          (let ((s (apply '+ w)))
            (when (or (null best-sum) (> s best-sum))
              (setq best w best-sum s))))
        (list best best-sum))))
  (unwind-protect
      (list
        ;; All windows of size 3
        (funcall 'neovm--ll-windows '(1 2 3 4 5 6) 3)
        ;; Window size = list length (just the list itself)
        (funcall 'neovm--ll-windows '(a b c) 3)
        ;; Window size > list length
        (funcall 'neovm--ll-windows '(a b) 5)
        ;; Window size 1 = each element
        (funcall 'neovm--ll-windows '(x y z) 1)
        ;; Moving average
        (funcall 'neovm--ll-moving-avg '(10 20 30 40 50) 3)
        ;; Max sum window
        (funcall 'neovm--ll-max-window '(1 3 -1 5 -2 8 4 -3) 3)
        ;; Number of windows = length - k + 1
        (let* ((lst '(a b c d e f g h))
               (k 4)
               (windows (funcall 'neovm--ll-windows lst k)))
          (= (length windows) (- (length lst) (1- k)))))
    (fmakunbound 'neovm--ll-windows)
    (fmakunbound 'neovm--ll-moving-avg)
    (fmakunbound 'neovm--ll-max-window)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 2 3) (2 3 4) (3 4 5) (4 5 6)) ((a b c)) nil ((x) (y) (z)) (20.0 30.0 40.0) ((5 -2 8) 11) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Frequency histogram: count occurrences and build sorted histogram
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_linkedlist_frequency_histogram() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a frequency histogram, find top-k elements, compute mode and
    // unique count, and verify the histogram sums to the original length.
    let form = r#"(progn
  (fset 'neovm--ll-histogram
    (lambda (lst)
      "Build frequency histogram as sorted alist (element . count)."
      (let ((ht (make-hash-table :test 'equal)))
        (dolist (x lst)
          (puthash x (1+ (or (gethash x ht) 0)) ht))
        (let ((pairs nil))
          (maphash (lambda (k v) (setq pairs (cons (cons k v) pairs))) ht)
          ;; Sort by count descending, then by key for stability
          (sort pairs (lambda (a b)
                        (or (> (cdr a) (cdr b))
                            (and (= (cdr a) (cdr b))
                                 (string< (format "%s" (car a))
                                          (format "%s" (car b)))))))))))
  (fset 'neovm--ll-top-k
    (lambda (lst k)
      "Return top K most frequent elements."
      (let ((hist (funcall 'neovm--ll-histogram lst))
            (result nil) (i 0))
        (while (and hist (< i k))
          (setq result (cons (car hist) result)
                hist (cdr hist) i (1+ i)))
        (nreverse result))))
  (fset 'neovm--ll-mode
    (lambda (lst)
      "Find the mode (most frequent element)."
      (if (null lst) nil
        (caar (funcall 'neovm--ll-histogram lst)))))
  (unwind-protect
      (list
        ;; Basic histogram
        (funcall 'neovm--ll-histogram '(a b a c b a d c a b))
        ;; Top-2 elements
        (funcall 'neovm--ll-top-k '(a b a c b a d c a b) 2)
        ;; Mode
        (funcall 'neovm--ll-mode '(1 2 2 3 3 3 4 4 4 4))
        ;; All unique (all counts = 1)
        (funcall 'neovm--ll-histogram '(x y z))
        ;; Verify sum of counts = original length
        (let* ((lst '(a b c a b a d e a b c))
               (hist (funcall 'neovm--ll-histogram lst))
               (total (apply '+ (mapcar 'cdr hist))))
          (= total (length lst)))
        ;; Unique count from histogram
        (let* ((hist (funcall 'neovm--ll-histogram
                              '(1 1 2 2 2 3 4 4 5))))
          (length hist))
        ;; String frequency
        (funcall 'neovm--ll-histogram
                 '("the" "cat" "sat" "on" "the" "mat" "the" "cat"))
        ;; Empty list
        (funcall 'neovm--ll-histogram nil))
    (fmakunbound 'neovm--ll-histogram)
    (fmakunbound 'neovm--ll-top-k)
    (fmakunbound 'neovm--ll-mode)))"#;
    let expect = expect_test::expect![[
        r#""OK (((a . 4) (b . 3) (c . 2) (d . 1)) ((a . 4) (b . 3)) 4 ((x . 1) (y . 1) (z . 1)) t 5 ((\"the\" . 3) (\"cat\" . 2) (\"mat\" . 1) (\"on\" . 1) (\"sat\" . 1)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Oracle parity tests for list algorithm patterns.
//!
//! Covers: merging two sorted lists, removing duplicates preserving first
//! occurrence, rotating a list by N positions, partitioning around a pivot,
//! zip/unzip operations, run-length encoding/decoding, and finding the
//! longest increasing subsequence via patience sorting.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Merge two sorted lists maintaining order
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_listalgo_merge_sorted() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Merge two already-sorted lists into one sorted list, handling
    // duplicates, empty lists, and lists of different lengths
    let form = "(progn
  (fset 'neovm--test-merge-sorted
    (lambda (a b)
      (cond
        ((null a) b)
        ((null b) a)
        ((<= (car a) (car b))
         (cons (car a) (funcall 'neovm--test-merge-sorted (cdr a) b)))
        (t (cons (car b) (funcall 'neovm--test-merge-sorted a (cdr b)))))))
  (fset 'neovm--test-merge-k-sorted
    (lambda (lists)
      ;; Merge k sorted lists by pairwise reduction
      (if (null lists) nil
        (let ((result (car lists)))
          (dolist (lst (cdr lists))
            (setq result (funcall 'neovm--test-merge-sorted result lst)))
          result))))
  (unwind-protect
      (list
        ;; Basic merge
        (funcall 'neovm--test-merge-sorted '(1 3 5 7) '(2 4 6 8))
        ;; With duplicates
        (funcall 'neovm--test-merge-sorted '(1 2 2 5) '(2 3 5 6))
        ;; One empty
        (funcall 'neovm--test-merge-sorted '() '(1 2 3))
        (funcall 'neovm--test-merge-sorted '(4 5 6) '())
        ;; Both empty
        (funcall 'neovm--test-merge-sorted '() '())
        ;; Different lengths
        (funcall 'neovm--test-merge-sorted '(1) '(2 3 4 5 6 7))
        ;; Merge k sorted lists
        (funcall 'neovm--test-merge-k-sorted
                 '((1 5 9) (2 6 10) (3 7 11) (4 8 12)))
        ;; Verify: merging sorted halves of a list produces sorted whole
        (let* ((sorted '(1 2 3 4 5 6 7 8 9 10))
               (left '(1 2 3 4 5))
               (right '(6 7 8 9 10)))
          (equal sorted (funcall 'neovm--test-merge-sorted left right))))
    (fmakunbound 'neovm--test-merge-sorted)
    (fmakunbound 'neovm--test-merge-k-sorted)))";
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8) (1 2 2 2 3 5 5 6) (1 2 3) (4 5 6) nil (1 2 3 4 5 6 7) (1 2 3 4 5 6 7 8 9 10 11 12) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Remove duplicates preserving first occurrence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_listalgo_remove_duplicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Remove duplicates while preserving the order of first occurrences,
    // using a hash table for O(n) lookup
    let form = "(progn
  (fset 'neovm--test-remove-dups
    (lambda (lst)
      (let ((seen (make-hash-table :test 'equal))
            (result nil))
        (dolist (x lst)
          (unless (gethash x seen)
            (puthash x t seen)
            (setq result (cons x result))))
        (nreverse result))))
  (fset 'neovm--test-remove-dups-naive
    (lambda (lst)
      ;; O(n^2) version using member for comparison
      (let ((result nil))
        (dolist (x lst)
          (unless (member x result)
            (setq result (cons x result))))
        (nreverse result))))
  (unwind-protect
      (list
        ;; Basic dedup
        (funcall 'neovm--test-remove-dups '(1 2 3 2 1 4 3 5))
        ;; All same
        (funcall 'neovm--test-remove-dups '(a a a a a))
        ;; No duplicates
        (funcall 'neovm--test-remove-dups '(1 2 3 4 5))
        ;; Empty
        (funcall 'neovm--test-remove-dups '())
        ;; String elements
        (funcall 'neovm--test-remove-dups '(\"hello\" \"world\" \"hello\" \"foo\" \"world\"))
        ;; Verify hash and naive produce same result
        (equal (funcall 'neovm--test-remove-dups '(c b a c b d a e))
               (funcall 'neovm--test-remove-dups-naive '(c b a c b d a e)))
        ;; Mixed types
        (funcall 'neovm--test-remove-dups '(1 \"a\" 2 \"a\" 1 3)))
    (fmakunbound 'neovm--test-remove-dups)
    (fmakunbound 'neovm--test-remove-dups-naive)))";
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5) (a) (1 2 3 4 5) nil (\"hello\" \"world\" \"foo\") t (1 \"a\" 2 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Rotate list by N positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_listalgo_rotate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Rotate list left by N positions: (rotate '(1 2 3 4 5) 2) => (3 4 5 1 2)
    // Also support negative rotation (rotate right)
    let form = "(progn
  (fset 'neovm--test-rotate
    (lambda (lst n)
      (if (or (null lst) (= n 0)) lst
        (let* ((len (length lst))
               ;; Normalize: handle negative and overflow
               (k (% n len))
               (k (if (< k 0) (+ k len) k)))
          (if (= k 0) lst
            (append (nthcdr k lst)
                    (let ((result nil) (i 0) (l lst))
                      (while (< i k)
                        (setq result (cons (car l) result)
                              l (cdr l) i (1+ i)))
                      (nreverse result))))))))
  (unwind-protect
      (list
        ;; Rotate left by 2
        (funcall 'neovm--test-rotate '(1 2 3 4 5) 2)
        ;; Rotate left by 0 (identity)
        (funcall 'neovm--test-rotate '(1 2 3 4 5) 0)
        ;; Rotate by length (identity)
        (funcall 'neovm--test-rotate '(1 2 3 4 5) 5)
        ;; Rotate by more than length (wraps)
        (funcall 'neovm--test-rotate '(1 2 3 4 5) 7)
        ;; Rotate right by 1 (negative)
        (funcall 'neovm--test-rotate '(1 2 3 4 5) -1)
        ;; Rotate single element
        (funcall 'neovm--test-rotate '(42) 3)
        ;; Empty list
        (funcall 'neovm--test-rotate '() 5)
        ;; Verify: rotate left by k then right by k = identity
        (let ((orig '(a b c d e f)))
          (equal orig
                 (funcall 'neovm--test-rotate
                          (funcall 'neovm--test-rotate orig 3) -3))))
    (fmakunbound 'neovm--test-rotate)))";
    let expect = expect_test::expect![[
        r#""OK ((3 4 5 1 2) (1 2 3 4 5) (1 2 3 4 5) (3 4 5 1 2) (5 1 2 3 4) (42) nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Partition list around a pivot
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_listalgo_partition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Partition into (less equal greater) three-way split around a pivot,
    // then verify stability and that concatenation yields the original elements
    let form = "(progn
  (fset 'neovm--test-partition3
    (lambda (lst pivot)
      ;; Returns (less equal greater) as three lists
      (let ((less nil) (eq-part nil) (greater nil))
        (dolist (x lst)
          (cond
            ((< x pivot) (setq less (cons x less)))
            ((= x pivot) (setq eq-part (cons x eq-part)))
            (t (setq greater (cons x greater)))))
        (list (nreverse less) (nreverse eq-part) (nreverse greater)))))
  (fset 'neovm--test-quickselect
    (lambda (lst k)
      ;; Find k-th smallest element using partition
      (if (null lst) nil
        (let* ((pivot (car lst))
               (parts (funcall 'neovm--test-partition3 lst pivot))
               (less (car parts))
               (eq-part (cadr parts))
               (greater (caddr parts))
               (nl (length less))
               (ne (length eq-part)))
          (cond
            ((< k nl)
             (funcall 'neovm--test-quickselect less k))
            ((< k (+ nl ne))
             pivot)
            (t (funcall 'neovm--test-quickselect greater (- k nl ne))))))))
  (unwind-protect
      (list
        ;; Basic partition
        (funcall 'neovm--test-partition3 '(3 1 4 1 5 9 2 6 5 3) 4)
        ;; Pivot not in list
        (funcall 'neovm--test-partition3 '(1 3 5 7 9) 4)
        ;; All elements equal to pivot
        (funcall 'neovm--test-partition3 '(5 5 5 5) 5)
        ;; Single element
        (funcall 'neovm--test-partition3 '(42) 42)
        ;; Verify concatenation preserves all elements
        (let* ((input '(8 3 7 1 6 2 9 4 5))
               (parts (funcall 'neovm--test-partition3 input 5))
               (reconstructed (append (car parts) (cadr parts) (caddr parts))))
          (equal (sort (copy-sequence input) '<)
                 (sort (copy-sequence reconstructed) '<)))
        ;; Quickselect: find median
        (let ((lst '(9 1 5 3 7 2 8 4 6)))
          (list
            (funcall 'neovm--test-quickselect lst 0)   ;; min
            (funcall 'neovm--test-quickselect lst 4)   ;; median
            (funcall 'neovm--test-quickselect lst 8)))) ;; max
    (fmakunbound 'neovm--test-partition3)
    (fmakunbound 'neovm--test-quickselect)))";
    let expect = expect_test::expect![[
        r#""OK (((3 1 1 2 3) (4) (5 9 6 5)) ((1 3) nil (5 7 9)) (nil (5 5 5 5) nil) (nil (42) nil) t (1 5 9))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Zip and unzip lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_listalgo_zip_unzip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // zip: combine two lists into list of pairs
    // zip-with: combine with a function
    // unzip: split list of pairs back into two lists
    let form = "(progn
  (fset 'neovm--test-zip
    (lambda (a b)
      (let ((result nil))
        (while (and a b)
          (setq result (cons (cons (car a) (car b)) result)
                a (cdr a) b (cdr b)))
        (nreverse result))))
  (fset 'neovm--test-zip-with
    (lambda (f a b)
      (let ((result nil))
        (while (and a b)
          (setq result (cons (funcall f (car a) (car b)) result)
                a (cdr a) b (cdr b)))
        (nreverse result))))
  (fset 'neovm--test-unzip
    (lambda (pairs)
      (let ((firsts nil) (seconds nil))
        (dolist (p pairs)
          (setq firsts (cons (car p) firsts)
                seconds (cons (cdr p) seconds)))
        (cons (nreverse firsts) (nreverse seconds)))))
  (fset 'neovm--test-zip3
    (lambda (a b c)
      (let ((result nil))
        (while (and a b c)
          (setq result (cons (list (car a) (car b) (car c)) result)
                a (cdr a) b (cdr b) c (cdr c)))
        (nreverse result))))
  (unwind-protect
      (list
        ;; Basic zip
        (funcall 'neovm--test-zip '(1 2 3) '(a b c))
        ;; Unequal lengths: truncates to shorter
        (funcall 'neovm--test-zip '(1 2 3 4 5) '(a b))
        ;; zip-with addition
        (funcall 'neovm--test-zip-with '+ '(1 2 3) '(10 20 30))
        ;; zip-with string concat
        (funcall 'neovm--test-zip-with 'concat '(\"a\" \"b\" \"c\") '(\"1\" \"2\" \"3\"))
        ;; Unzip roundtrip
        (let* ((zipped (funcall 'neovm--test-zip '(x y z) '(1 2 3)))
               (unzipped (funcall 'neovm--test-unzip zipped)))
          (list (equal (car unzipped) '(x y z))
                (equal (cdr unzipped) '(1 2 3))))
        ;; Zip 3 lists
        (funcall 'neovm--test-zip3 '(1 2 3) '(a b c) '(x y z))
        ;; Empty list handling
        (funcall 'neovm--test-zip '() '(1 2 3))
        (funcall 'neovm--test-zip '(1 2) '()))
    (fmakunbound 'neovm--test-zip)
    (fmakunbound 'neovm--test-zip-with)
    (fmakunbound 'neovm--test-unzip)
    (fmakunbound 'neovm--test-zip3)))";
    let expect = expect_test::expect![[
        r#""OK (((1 . a) (2 . b) (3 . c)) ((1 . a) (2 . b)) (11 22 33) (\"a1\" \"b2\" \"c3\") (t t) ((1 a x) (2 b y) (3 c z)) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Run-length encoding and decoding of lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_listalgo_rle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Encode: (a a a b b c) -> ((a . 3) (b . 2) (c . 1))
    // Decode: reverse operation
    // Also: modified RLE that only encodes runs > 1
    let form = "(progn
  (fset 'neovm--test-rle-encode
    (lambda (lst)
      (if (null lst) nil
        (let ((result nil)
              (current (car lst))
              (count 1))
          (dolist (x (cdr lst))
            (if (equal x current)
                (setq count (1+ count))
              (setq result (cons (cons current count) result)
                    current x count 1)))
          (setq result (cons (cons current count) result))
          (nreverse result)))))
  (fset 'neovm--test-rle-decode
    (lambda (encoded)
      (let ((result nil))
        (dolist (pair encoded)
          (let ((elem (car pair))
                (n (cdr pair)))
            (dotimes (_ n)
              (setq result (cons elem result)))))
        (nreverse result))))
  (fset 'neovm--test-rle-compact
    (lambda (lst)
      ;; Only compress runs of 2+, leave singletons as atoms
      (if (null lst) nil
        (let ((result nil)
              (current (car lst))
              (count 1))
          (dolist (x (cdr lst))
            (if (equal x current)
                (setq count (1+ count))
              (setq result (cons (if (= count 1) current
                                   (cons current count))
                                 result)
                    current x count 1)))
          (setq result (cons (if (= count 1) current
                               (cons current count))
                             result))
          (nreverse result)))))
  (unwind-protect
      (list
        ;; Basic encode
        (funcall 'neovm--test-rle-encode '(a a a b b c c c c d))
        ;; Decode back
        (funcall 'neovm--test-rle-decode
                 (funcall 'neovm--test-rle-encode '(a a a b b c c c c d)))
        ;; No runs (all different)
        (funcall 'neovm--test-rle-encode '(a b c d e))
        ;; All same
        (funcall 'neovm--test-rle-encode '(x x x x x x))
        ;; Compact encoding (singletons stay as atoms)
        (funcall 'neovm--test-rle-compact '(a a b c c c d e e))
        ;; Roundtrip verification
        (let ((original '(1 1 1 2 3 3 4 4 4 4 5)))
          (equal original
                 (funcall 'neovm--test-rle-decode
                          (funcall 'neovm--test-rle-encode original))))
        ;; Empty list
        (funcall 'neovm--test-rle-encode '())
        ;; Single element
        (funcall 'neovm--test-rle-encode '(z)))
    (fmakunbound 'neovm--test-rle-encode)
    (fmakunbound 'neovm--test-rle-decode)
    (fmakunbound 'neovm--test-rle-compact)))";
    let expect = expect_test::expect![[
        r#""OK (((a . 3) (b . 2) (c . 4) (d . 1)) (a a a b b c c c c d) ((a . 1) (b . 1) (c . 1) (d . 1) (e . 1)) ((x . 6)) ((a . 2) b (c . 3) d (e . 2)) t nil ((z . 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Longest increasing subsequence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_listalgo_longest_increasing_subsequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Find longest strictly increasing subsequence using patience sorting
    // with binary search on tail values, then backtrack to reconstruct
    let form = "(progn
  (fset 'neovm--test-lis
    (lambda (lst)
      ;; DP approach: for each element, find length of LIS ending at that element
      ;; tails[i] = smallest tail element for increasing subsequence of length i+1
      ;; Also track predecessors for reconstruction
      (if (null lst) nil
        (let* ((vec (vconcat lst))
               (n (length vec))
               (dp (make-vector n 1))       ;; dp[i] = LIS length ending at i
               (pred (make-vector n -1))     ;; pred[i] = previous index in LIS
               (max-len 1)
               (max-idx 0))
          ;; Fill DP table
          (let ((i 1))
            (while (< i n)
              (let ((j 0))
                (while (< j i)
                  (when (and (< (aref vec j) (aref vec i))
                             (> (+ (aref dp j) 1) (aref dp i)))
                    (aset dp i (+ (aref dp j) 1))
                    (aset pred i j))
                  (setq j (1+ j)))
                (when (> (aref dp i) max-len)
                  (setq max-len (aref dp i)
                        max-idx i)))
              (setq i (1+ i))))
          ;; Backtrack to reconstruct LIS
          (let ((result nil)
                (idx max-idx))
            (while (>= idx 0)
              (setq result (cons (aref vec idx) result)
                    idx (aref pred idx)))
            result)))))
  (fset 'neovm--test-is-increasing
    (lambda (lst)
      ;; Verify that a list is strictly increasing
      (if (or (null lst) (null (cdr lst))) t
        (let ((ok t)
              (prev (car lst))
              (rest (cdr lst)))
          (while (and ok rest)
            (when (<= (car rest) prev)
              (setq ok nil))
            (setq prev (car rest)
                  rest (cdr rest)))
          ok))))
  (unwind-protect
      (list
        ;; Classic example
        (funcall 'neovm--test-lis '(10 9 2 5 3 7 101 18))
        ;; Already sorted
        (funcall 'neovm--test-lis '(1 2 3 4 5))
        ;; Reverse sorted (LIS length = 1)
        (funcall 'neovm--test-lis '(5 4 3 2 1))
        ;; All equal (LIS length = 1)
        (funcall 'neovm--test-lis '(7 7 7 7 7))
        ;; Single element
        (funcall 'neovm--test-lis '(42))
        ;; Verify output is actually increasing
        (funcall 'neovm--test-is-increasing
                 (funcall 'neovm--test-lis '(3 1 8 2 5 4 7 6 9)))
        ;; Verify length is correct for known case
        (length (funcall 'neovm--test-lis '(0 8 4 12 2 10 6 14 1 9 5 13 3 11 7 15))))
    (fmakunbound 'neovm--test-lis)
    (fmakunbound 'neovm--test-is-increasing)))";
    let expect = expect_test::expect![[r#""OK ((2 5 7 101) (1 2 3 4 5) (5) (7) (42) t 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Group-by / frequency count on lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_listalgo_group_by_frequency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Group elements by a key function and count frequencies,
    // then sort groups by frequency descending
    let form = "(progn
  (fset 'neovm--test-frequencies
    (lambda (lst)
      ;; Count occurrences of each element, return alist sorted by count desc
      (let ((counts (make-hash-table :test 'equal)))
        (dolist (x lst)
          (puthash x (1+ (or (gethash x counts) 0)) counts))
        (let ((pairs nil))
          (maphash (lambda (k v) (setq pairs (cons (cons k v) pairs))) counts)
          (sort pairs (lambda (a b) (> (cdr a) (cdr b))))))))
  (fset 'neovm--test-group-by
    (lambda (fn lst)
      ;; Group elements by key function, return alist of (key . elements)
      (let ((groups (make-hash-table :test 'equal)))
        (dolist (x lst)
          (let ((key (funcall fn x)))
            (puthash key (cons x (or (gethash key groups) nil)) groups)))
        (let ((result nil))
          (maphash (lambda (k v) (setq result (cons (cons k (nreverse v)) result))) groups)
          (sort result (lambda (a b) (string-lessp
                                       (format \"%s\" (car a))
                                       (format \"%s\" (car b)))))))))
  (unwind-protect
      (list
        ;; Frequency count
        (funcall 'neovm--test-frequencies '(a b a c b a d c a))
        ;; Group numbers by even/odd
        (funcall 'neovm--test-group-by
                 (lambda (x) (if (= (% x 2) 0) 'even 'odd))
                 '(1 2 3 4 5 6 7 8 9 10))
        ;; Group strings by first character
        (funcall 'neovm--test-group-by
                 (lambda (s) (substring s 0 1))
                 '(\"apple\" \"avocado\" \"banana\" \"blueberry\" \"cherry\" \"apricot\"))
        ;; Empty list
        (funcall 'neovm--test-frequencies '())
        ;; All unique
        (funcall 'neovm--test-frequencies '(1 2 3 4 5)))
    (fmakunbound 'neovm--test-frequencies)
    (fmakunbound 'neovm--test-group-by)))";
    let expect = expect_test::expect![[
        r#""OK (((a . 4) (c . 2) (b . 2) (d . 1)) ((even 2 4 6 8 10) (odd 1 3 5 7 9)) ((\"a\" \"apple\" \"avocado\" \"apricot\") (\"b\" \"banana\" \"blueberry\") (\"c\" \"cherry\")) nil ((5 . 1) (4 . 1) (3 . 1) (2 . 1) (1 . 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

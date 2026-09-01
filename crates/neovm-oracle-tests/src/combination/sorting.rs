//! Oracle parity tests for sorting algorithm implementations in Elisp.
//!
//! Tests quicksort with custom pivot, insertion sort on lists,
//! counting sort, radix sort for strings, stability verification,
//! and multi-key sorting.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Quicksort with median-of-three pivot selection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sorting_quicksort_median_pivot() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
  ;; Quicksort using median-of-three pivot selection
  ;; Uses vectors for in-place partitioning, converted from/to lists
  (fset 'neovm--test-vec-swap
    (lambda (v i j)
      (let ((tmp (aref v i)))
        (aset v i (aref v j))
        (aset v j tmp))))

  (fset 'neovm--test-median-of-three
    (lambda (v lo hi)
      (let* ((mid (/ (+ lo hi) 2))
             (a (aref v lo))
             (b (aref v mid))
             (c (aref v hi)))
        (cond
          ((or (and (<= a b) (<= b c))
               (and (<= c b) (<= b a))) mid)
          ((or (and (<= b a) (<= a c))
               (and (<= c a) (<= a b))) lo)
          (t hi)))))

  (fset 'neovm--test-partition
    (lambda (v lo hi)
      (let ((pivot-idx (funcall 'neovm--test-median-of-three v lo hi)))
        ;; Move pivot to end
        (funcall 'neovm--test-vec-swap v pivot-idx hi)
        (let ((pivot (aref v hi))
              (store lo))
          (let ((i lo))
            (while (< i hi)
              (when (< (aref v i) pivot)
                (funcall 'neovm--test-vec-swap v i store)
                (setq store (1+ store)))
              (setq i (1+ i))))
          (funcall 'neovm--test-vec-swap v store hi)
          store))))

  (fset 'neovm--test-qsort
    (lambda (v lo hi)
      (when (< lo hi)
        (let ((p (funcall 'neovm--test-partition v lo hi)))
          (funcall 'neovm--test-qsort v lo (1- p))
          (funcall 'neovm--test-qsort v (1+ p) hi)))))

  (fset 'neovm--test-quicksort
    (lambda (lst)
      (let* ((v (apply #'vector lst))
             (n (length v)))
        (when (> n 1)
          (funcall 'neovm--test-qsort v 0 (1- n)))
        (append v nil))))

  (unwind-protect
      (list
        ;; Basic sort
        (funcall 'neovm--test-quicksort '(5 3 8 1 9 2 7 4 6))
        ;; Already sorted
        (funcall 'neovm--test-quicksort '(1 2 3 4 5))
        ;; Reverse sorted
        (funcall 'neovm--test-quicksort '(9 8 7 6 5 4 3 2 1))
        ;; Duplicates
        (funcall 'neovm--test-quicksort '(3 1 4 1 5 9 2 6 5 3 5))
        ;; Single element
        (funcall 'neovm--test-quicksort '(42))
        ;; Two elements
        (funcall 'neovm--test-quicksort '(7 2))
        ;; All same
        (funcall 'neovm--test-quicksort '(4 4 4 4 4))
        ;; Compare with built-in sort
        (equal (funcall 'neovm--test-quicksort '(38 27 43 3 9 82 10))
               (sort (list 38 27 43 3 9 82 10) #'<)))
    (fmakunbound 'neovm--test-vec-swap)
    (fmakunbound 'neovm--test-median-of-three)
    (fmakunbound 'neovm--test-partition)
    (fmakunbound 'neovm--test-qsort)
    (fmakunbound 'neovm--test-quicksort)))";
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8 9) (1 2 3 4 5) (1 2 3 4 5 6 7 8 9) (1 1 2 3 3 4 5 5 5 6 9) (42) (2 7) (4 4 4 4 4) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Insertion sort on lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sorting_insertion_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
  ;; Insertion sort: build sorted list by inserting each element
  (fset 'neovm--test-insert-sorted
    (lambda (elem sorted pred)
      (if (or (null sorted)
              (funcall pred elem (car sorted)))
          (cons elem sorted)
        (cons (car sorted)
              (funcall 'neovm--test-insert-sorted
                       elem (cdr sorted) pred)))))

  (fset 'neovm--test-insertion-sort
    (lambda (lst pred)
      (let ((result nil))
        (dolist (x lst)
          (setq result (funcall 'neovm--test-insert-sorted x result pred)))
        result)))

  (unwind-protect
      (list
        ;; Sort numbers ascending
        (funcall 'neovm--test-insertion-sort '(64 34 25 12 22 11 90) #'<)
        ;; Sort numbers descending
        (funcall 'neovm--test-insertion-sort '(64 34 25 12 22 11 90) #'>)
        ;; Sort strings
        (funcall 'neovm--test-insertion-sort
                 '(\"banana\" \"apple\" \"cherry\" \"date\" \"elderberry\")
                 #'string<)
        ;; Empty list
        (funcall 'neovm--test-insertion-sort nil #'<)
        ;; Single element
        (funcall 'neovm--test-insertion-sort '(1) #'<)
        ;; Already sorted
        (funcall 'neovm--test-insertion-sort '(1 2 3 4 5) #'<)
        ;; Verify against built-in
        (equal (funcall 'neovm--test-insertion-sort '(5 3 1 4 2) #'<)
               (sort (list 5 3 1 4 2) #'<)))
    (fmakunbound 'neovm--test-insert-sorted)
    (fmakunbound 'neovm--test-insertion-sort)))";
    let expect = expect_test::expect![[
        r#""OK ((11 12 22 25 34 64 90) (90 64 34 25 22 12 11) (\"apple\" \"banana\" \"cherry\" \"date\" \"elderberry\") nil (1) (1 2 3 4 5) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Counting sort (integer arrays, known range)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sorting_counting_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
  ;; Counting sort for non-negative integers with known max value
  (fset 'neovm--test-counting-sort
    (lambda (lst max-val)
      (let ((counts (make-vector (1+ max-val) 0)))
        ;; Count occurrences
        (dolist (x lst)
          (aset counts x (1+ (aref counts x))))
        ;; Build sorted result
        (let ((result nil)
              (i max-val))
          (while (>= i 0)
            (let ((c (aref counts i)))
              (while (> c 0)
                (setq result (cons i result))
                (setq c (1- c))))
            (setq i (1- i)))
          result))))

  (unwind-protect
      (list
        ;; Basic
        (funcall 'neovm--test-counting-sort '(4 2 2 8 3 3 1) 9)
        ;; With zeros
        (funcall 'neovm--test-counting-sort '(0 5 0 3 0 2 1) 5)
        ;; All same value
        (funcall 'neovm--test-counting-sort '(3 3 3 3 3) 5)
        ;; Single element
        (funcall 'neovm--test-counting-sort '(7) 9)
        ;; Already sorted
        (funcall 'neovm--test-counting-sort '(0 1 2 3 4 5) 5)
        ;; Large range, few elements
        (funcall 'neovm--test-counting-sort '(99 1 50 25 75) 99)
        ;; Verify parity with built-in
        (let ((data '(4 2 2 8 3 3 1)))
          (equal (funcall 'neovm--test-counting-sort data 9)
                 (sort (copy-sequence data) #'<))))
    (fmakunbound 'neovm--test-counting-sort)))";
    let expect = expect_test::expect![[
        r#""OK ((1 2 2 3 3 4 8) (0 0 0 1 2 3 5) (3 3 3 3 3) (7) (0 1 2 3 4 5) (1 25 50 75 99) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Radix sort for fixed-width strings (LSD radix sort on characters)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sorting_radix_sort_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; LSD Radix sort for equal-length strings
  ;; Sorts by last character first, working left
  (fset 'neovm--test-radix-sort-strings
    (lambda (strings)
      (if (or (null strings) (null (cdr strings)))
          strings
        (let* ((width (length (car strings)))
               (sorted strings)
               (pos (1- width)))
          ;; Process from rightmost to leftmost character
          (while (>= pos 0)
            ;; Stable sort by character at position pos
            ;; Use counting sort on character codes (0-127 ASCII)
            (let ((buckets (make-vector 128 nil)))
              (dolist (s sorted)
                (let ((ch (aref s pos)))
                  (aset buckets ch (cons s (aref buckets ch)))))
              ;; Collect from buckets in order
              (setq sorted nil)
              (let ((i 127))
                (while (>= i 0)
                  (dolist (s (aref buckets i))
                    (setq sorted (cons s sorted)))
                  (setq i (1- i)))))
            (setq pos (1- pos)))
          sorted))))

  (unwind-protect
      (list
        ;; 3-char strings
        (funcall 'neovm--test-radix-sort-strings
                 '("dog" "cat" "ant" "bat" "cow" "ape" "bee"))
        ;; 4-char strings
        (funcall 'neovm--test-radix-sort-strings
                 '("pear" "plum" "kiwi" "lime" "date" "fig!" "acai"))
        ;; Already sorted
        (funcall 'neovm--test-radix-sort-strings
                 '("aaa" "bbb" "ccc" "ddd"))
        ;; Reverse sorted
        (funcall 'neovm--test-radix-sort-strings
                 '("ddd" "ccc" "bbb" "aaa"))
        ;; Duplicates
        (funcall 'neovm--test-radix-sort-strings
                 '("abc" "xyz" "abc" "def" "xyz"))
        ;; Verify against built-in sort
        (equal (funcall 'neovm--test-radix-sort-strings
                        '("dog" "cat" "ant" "bat" "cow"))
               (sort (list "dog" "cat" "ant" "bat" "cow") #'string<)))
    (fmakunbound 'neovm--test-radix-sort-strings)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"ant\" \"ape\" \"bat\" \"bee\" \"cat\" \"cow\" \"dog\") (\"acai\" \"date\" \"fig!\" \"kiwi\" \"lime\" \"pear\" \"plum\") (\"aaa\" \"bbb\" \"ccc\" \"ddd\") (\"aaa\" \"bbb\" \"ccc\" \"ddd\") (\"abc\" \"abc\" \"def\" \"xyz\" \"xyz\") t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Stable sort verification: preserve order of equal elements
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sorting_stability_verification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Tag each element with an original index, sort by value only,
    // then check that equal-valued elements retain their original order
    let form = "(progn
  (fset 'neovm--test-tag-with-indices
    (lambda (lst)
      (let ((result nil) (idx 0))
        (dolist (x lst)
          (setq result (cons (cons x idx) result))
          (setq idx (1+ idx)))
        (nreverse result))))

  (fset 'neovm--test-is-stable
    (lambda (tagged-sorted)
      ;; For each pair of adjacent elements with equal keys,
      ;; verify indices are in ascending order
      (let ((stable t)
            (rest tagged-sorted))
        (while (and stable (cdr rest))
          (let ((cur (car rest))
                (nxt (cadr rest)))
            (when (and (= (car cur) (car nxt))
                       (> (cdr cur) (cdr nxt)))
              (setq stable nil)))
          (setq rest (cdr rest)))
        stable)))

  ;; Insertion sort is inherently stable
  (fset 'neovm--test-stable-sort
    (lambda (tagged pred)
      (let ((result nil))
        (dolist (pair tagged)
          (let ((inserted nil)
                (acc nil)
                (remaining result))
            (while (and remaining (not inserted))
              (if (funcall pred (car pair) (caar remaining))
                  (progn
                    (setq result (nconc (nreverse acc) (cons pair remaining)))
                    (setq inserted t))
                (setq acc (cons (car remaining) acc))
                (setq remaining (cdr remaining))))
            (unless inserted
              (setq result (nconc (nreverse acc) (list pair))))))
        result)))

  (unwind-protect
      (let* ((data '(3 1 4 1 5 9 2 6 5 3 5 8 9 7 9))
             (tagged (funcall 'neovm--test-tag-with-indices data))
             (sorted (funcall 'neovm--test-stable-sort tagged #'<)))
        (list
          ;; Sorted values are correct
          (mapcar #'car sorted)
          ;; Stability check passes
          (funcall 'neovm--test-is-stable sorted)
          ;; Original indices for value=5 should be in order
          (let ((fives nil))
            (dolist (pair sorted)
              (when (= (car pair) 5)
                (setq fives (cons (cdr pair) fives))))
            (nreverse fives))
          ;; Original indices for value=9
          (let ((nines nil))
            (dolist (pair sorted)
              (when (= (car pair) 9)
                (setq nines (cons (cdr pair) nines))))
            (nreverse nines))
          ;; Total count preserved
          (length sorted)))
    (fmakunbound 'neovm--test-tag-with-indices)
    (fmakunbound 'neovm--test-is-stable)
    (fmakunbound 'neovm--test-stable-sort)))";
    let expect =
        expect_test::expect![[r#""OK ((1 1 2 3 3 4 5 5 5 6 7 8 9 9 9) t (4 8 10) (5 12 14) 15)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-key sort: sort records by multiple fields
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sorting_multi_key_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Sort employee records: primary by department (ascending),
    // secondary by level (descending), tertiary by name (ascending)
    let form = r#"(progn
  (fset 'neovm--test-make-employee
    (lambda (dept level name salary)
      (list dept level name salary)))

  (fset 'neovm--test-multi-key-compare
    (lambda (a b)
      (let ((dept-a (nth 0 a)) (dept-b (nth 0 b))
            (level-a (nth 1 a)) (level-b (nth 1 b))
            (name-a (nth 2 a)) (name-b (nth 2 b)))
        (cond
          ;; Primary: department ascending
          ((string< dept-a dept-b) t)
          ((string< dept-b dept-a) nil)
          ;; Secondary: level descending (higher level first)
          ((> level-a level-b) t)
          ((< level-a level-b) nil)
          ;; Tertiary: name ascending
          (t (string< name-a name-b))))))

  (unwind-protect
      (let ((employees
              (list
                (funcall 'neovm--test-make-employee "eng" 3 "Carol" 90000)
                (funcall 'neovm--test-make-employee "eng" 5 "Alice" 150000)
                (funcall 'neovm--test-make-employee "sales" 2 "Eve" 70000)
                (funcall 'neovm--test-make-employee "eng" 3 "Bob" 95000)
                (funcall 'neovm--test-make-employee "sales" 4 "Dave" 110000)
                (funcall 'neovm--test-make-employee "sales" 4 "Frank" 105000)
                (funcall 'neovm--test-make-employee "hr" 3 "Grace" 85000)
                (funcall 'neovm--test-make-employee "hr" 1 "Heidi" 55000)
                (funcall 'neovm--test-make-employee "eng" 5 "Ivan" 145000))))
        (let ((sorted (sort (copy-sequence employees)
                            'neovm--test-multi-key-compare)))
          (list
            ;; Full sorted result
            sorted
            ;; Extract just the names in sorted order
            (mapcar (lambda (e) (nth 2 e)) sorted)
            ;; First employee should be eng dept, level 5
            (nth 0 (car sorted))
            (nth 1 (car sorted))
            ;; Group counts by department
            (let ((counts nil))
              (dolist (e sorted)
                (let ((dept (nth 0 e))
                      (existing (assoc (nth 0 e) counts)))
                  (if existing
                      (setcdr existing (1+ (cdr existing)))
                    (setq counts (cons (cons dept 1) counts)))))
              (sort counts (lambda (a b) (string< (car a) (car b))))))))
    (fmakunbound 'neovm--test-make-employee)
    (fmakunbound 'neovm--test-multi-key-compare)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"eng\" 5 \"Alice\" 150000) (\"eng\" 5 \"Ivan\" 145000) (\"eng\" 3 \"Bob\" 95000) (\"eng\" 3 \"Carol\" 90000) (\"hr\" 3 \"Grace\" 85000) (\"hr\" 1 \"Heidi\" 55000) (\"sales\" 4 \"Dave\" 110000) (\"sales\" 4 \"Frank\" 105000) (\"sales\" 2 \"Eve\" 70000)) (\"Alice\" \"Ivan\" \"Bob\" \"Carol\" \"Grace\" \"Heidi\" \"Dave\" \"Frank\" \"Eve\") \"eng\" 5 ((\"eng\" . 4) (\"hr\" . 2) (\"sales\" . 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

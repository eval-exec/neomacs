//! Comprehensive oracle parity tests for sorting operations.
//!
//! Tests `sort` with all comparator types, `seq-sort`, `seq-sort-by`,
//! `cl-sort` with `:key`, sorting nested structures, side effects in
//! comparators, destructive vs non-destructive sort, edge cases,
//! `cl-stable-sort`, and `cl-merge`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// sort with all standard comparator types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_comprehensive_all_comparators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; < ascending numbers
  (sort (list 9 3 7 1 5 8 2 6 4 10) '<)
  ;; > descending numbers
  (sort (list 9 3 7 1 5 8 2 6 4 10) '>)
  ;; string< lexicographic ascending
  (sort (list "fig" "apple" "cherry" "banana" "date" "elderberry" "grape") 'string<)
  ;; string> lexicographic descending
  (sort (list "fig" "apple" "cherry" "banana" "date") 'string>)
  ;; string-lessp (case-insensitive)
  (sort (list "Banana" "apple" "Cherry" "DATE" "elderberry") 'string-lessp)
  ;; Custom: sort by modular arithmetic (mod n 3) then by value
  (sort (list 1 2 3 4 5 6 7 8 9)
        (lambda (a b)
          (let ((ma (% a 3)) (mb (% b 3)))
            (or (< ma mb)
                (and (= ma mb) (< a b))))))
  ;; Custom: reverse string sort by length then alphabetical
  (sort (list "aa" "bbb" "c" "dddd" "ee" "f" "ggg")
        (lambda (a b)
          (or (> (length a) (length b))
              (and (= (length a) (length b))
                   (string< a b))))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8 9 10) (10 9 8 7 6 5 4 3 2 1) (\"apple\" \"banana\" \"cherry\" \"date\" \"elderberry\" \"fig\" \"grape\") (\"fig\" \"date\" \"cherry\" \"banana\" \"apple\") (\"Banana\" \"Cherry\" \"DATE\" \"apple\" \"elderberry\") (3 6 9 1 4 7 2 5 8) (\"dddd\" \"bbb\" \"ggg\" \"aa\" \"ee\" \"c\" \"f\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// sort on vectors (Emacs 29+ sort supports vectors)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_comprehensive_vector_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Sort a vector of numbers
  (sort [5 3 8 1 9 2 7 4 6] '<)
  ;; Sort a vector of strings
  (sort ["cherry" "apple" "banana"] 'string<)
  ;; Sort empty vector
  (sort [] '<)
  ;; Sort single element vector
  (sort [42] '<)
  ;; Sort with custom predicate on vector
  (sort [10 -3 7 -8 2 -1 5]
        (lambda (a b) (< (abs a) (abs b))))
  ;; Verify vector sort returns a vector (using type-of or vectorp)
  (vectorp (sort [3 1 2] '<)))
"#;
    let expect = expect_test::expect![[
        r#""OK ([1 2 3 4 5 6 7 8 9] [\"apple\" \"banana\" \"cherry\"] [] [42] [-1 2 -3 5 7 -8 10] t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// sort with :key parameter (Emacs 29+)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_comprehensive_key_parameter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Sort alist by car using :key
  (sort (list '(3 . "c") '(1 . "a") '(2 . "b")) :key 'car :lessp '<)
  ;; Sort alist by cdr using :key
  (sort (list '(1 . "cherry") '(2 . "apple") '(3 . "banana"))
        :key 'cdr :lessp 'string<)
  ;; Sort by string length using :key
  (sort (list "fig" "apple" "cherry" "banana" "date")
        :key 'length :lessp '<)
  ;; Sort with :key and :reverse
  (sort (list 5 3 8 1 9 2 7 4 6) :key 'identity :lessp '< :reverse t)
  ;; Sort nested lists by second element
  (sort (list '(a 3) '(b 1) '(c 4) '(d 1) '(e 5))
        :key 'cadr :lessp '<)
  ;; Sort by absolute value using :key
  (sort (list -5 3 -8 1 -9 2 -7 4 -6)
        :key 'abs :lessp '<))
"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . \"a\") (2 . \"b\") (3 . \"c\")) ((2 . \"apple\") (3 . \"banana\") (1 . \"cherry\")) (\"fig\" \"date\" \"apple\" \"cherry\" \"banana\") (9 8 7 6 5 4 3 2 1) ((b 1) (d 1) (a 3) (c 4) (e 5)) (1 2 3 4 -5 -6 -7 -8 -9))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// sort on strings (character sorting)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_comprehensive_string_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Sort characters in a string
  (sort "zyxwvutsrqponmlkjihgfedcba" '<)
  ;; Sort already sorted string
  (sort "abcdef" '<)
  ;; Sort string with repeated chars
  (sort "baaabbbccc" '<)
  ;; Sort empty string
  (sort "" '<)
  ;; Sort single char string
  (sort "z" '<)
  ;; Sort string descending
  (sort "hello" '>)
  ;; Verify string sort returns a string
  (stringp (sort "cba" '<)))
"#;
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument list-or-vector-p \"zyxwvutsrqponmlkjihgfedcba\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-sort: non-destructive sorting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_comprehensive_seq_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(require 'seq)
(let ((original (list 5 3 8 1 9 2 7 4 6)))
  (let ((sorted (seq-sort '< original)))
    (list
      ;; Result is sorted
      sorted
      ;; Original is NOT modified (non-destructive)
      original
      ;; Result equals expected
      (equal sorted '(1 2 3 4 5 6 7 8 9))
      ;; Works on vectors too
      (seq-sort '< [5 3 1 4 2])
      ;; Works with string<
      (seq-sort 'string< '("cherry" "apple" "banana"))
      ;; Empty sequence
      (seq-sort '< nil)
      ;; Single element
      (seq-sort '< '(42)))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8 9) (5 3 8 1 9 2 7 4 6) t [1 2 3 4 5] (\"apple\" \"banana\" \"cherry\") nil (42))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-sort-by: sort by extracted key
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_comprehensive_seq_sort_by() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(require 'seq)
(list
  ;; Sort alists by car
  (seq-sort-by 'car '< '((3 . x) (1 . y) (2 . z)))
  ;; Sort strings by length
  (seq-sort-by 'length '< '("fig" "apple" "cherry" "date"))
  ;; Sort by absolute value
  (seq-sort-by 'abs '< '(-5 3 -1 7 -9 2))
  ;; Sort by last element of sub-lists
  (seq-sort-by (lambda (x) (car (last x))) '<
               '((a 3) (b 1) (c 4) (d 1) (e 5)))
  ;; Sort on vectors
  (seq-sort-by 'identity '< [5 3 1 4 2])
  ;; Sort with string key extraction
  (seq-sort-by 'symbol-name 'string< '(cherry apple banana date)))
"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . y) (2 . z) (3 . x)) (\"fig\" \"date\" \"apple\" \"cherry\") (-1 2 3 -5 7 -9) ((b 1) (d 1) (a 3) (c 4) (e 5)) [1 2 3 4 5] (apple banana cherry date))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-sort with :key parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_comprehensive_cl_sort_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(require 'cl-lib)
(list
  ;; cl-sort with :key
  (cl-sort (list '(3 . "c") '(1 . "a") '(2 . "b")) '< :key 'car)
  ;; cl-sort with string predicate and :key
  (cl-sort (list '("fig" . 1) '("apple" . 2) '("cherry" . 3))
           'string< :key 'car)
  ;; cl-sort on a vector
  (cl-sort (vector 5 3 8 1 9) '< :key 'identity)
  ;; cl-sort with lambda key
  (cl-sort (list '(a 3 x) '(b 1 y) '(c 4 z) '(d 2 w))
           '< :key 'cadr)
  ;; cl-sort with abs key
  (cl-sort (list -5 3 -1 7 -9 2) '< :key 'abs)
  ;; cl-sort empty list
  (cl-sort nil '< :key 'identity)
  ;; cl-sort single element
  (cl-sort (list 42) '< :key 'identity))
"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . \"a\") (2 . \"b\") (3 . \"c\")) ((\"apple\" . 2) (\"cherry\" . 3) (\"fig\" . 1)) [1 3 5 8 9] ((b 1 y) (d 2 w) (a 3 x) (c 4 z)) (-1 2 3 -5 7 -9) nil (42))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-stable-sort
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_comprehensive_cl_stable_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(require 'cl-lib)
(let ((data (list '(1 . "a1") '(2 . "b1") '(1 . "a2") '(3 . "c1")
                  '(2 . "b2") '(1 . "a3") '(3 . "c2") '(2 . "b3"))))
  (let ((sorted (cl-stable-sort (copy-sequence data) '< :key 'car)))
    (list
      ;; Overall sorted by key
      (mapcar 'car sorted)
      ;; Within group 1: stable order preserved
      (mapcar 'cdr (seq-filter (lambda (x) (= (car x) 1)) sorted))
      ;; Within group 2: stable order preserved
      (mapcar 'cdr (seq-filter (lambda (x) (= (car x) 2)) sorted))
      ;; Within group 3: stable order preserved
      (mapcar 'cdr (seq-filter (lambda (x) (= (car x) 3)) sorted))
      ;; Also test vector stable sort
      (cl-stable-sort (vector 3 1 4 1 5 9 2 6) '<))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((1 1 1 2 2 2 3 3) (\"a1\" \"a2\" \"a3\") (\"b1\" \"b2\" \"b3\") (\"c1\" \"c2\") [1 1 2 3 4 5 6 9])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-merge: merge two sorted sequences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_comprehensive_cl_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(require 'cl-lib)
(list
  ;; Merge two sorted lists
  (cl-merge 'list '(1 3 5 7 9) '(2 4 6 8 10) '<)
  ;; Merge with one empty
  (cl-merge 'list '(1 2 3) nil '<)
  ;; Merge with other empty
  (cl-merge 'list nil '(4 5 6) '<)
  ;; Merge both empty
  (cl-merge 'list nil nil '<)
  ;; Merge with duplicates
  (cl-merge 'list '(1 2 3 3 4) '(2 3 4 5 5) '<)
  ;; Merge strings
  (cl-merge 'list '("apple" "cherry" "fig") '("banana" "date" "grape") 'string<)
  ;; Merge into vector type
  (cl-merge 'vector '(1 3 5) '(2 4 6) '<)
  ;; Merge single element lists
  (cl-merge 'list '(1) '(2) '<)
  (cl-merge 'list '(2) '(1) '<))
"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8 9 10) (1 2 3) (4 5 6) nil (1 2 2 3 3 3 4 4 5 5) (\"apple\" \"banana\" \"cherry\" \"date\" \"fig\" \"grape\") [1 2 3 4 5 6] (1 2) (1 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sorting nested structures by different keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_comprehensive_nested_structures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Records: (name age score city)
  (defvar neovm--test-sort-records
    (list '("Alice" 30 95 "NYC")
          '("Bob" 25 87 "LA")
          '("Carol" 35 95 "NYC")
          '("Dave" 28 92 "Chicago")
          '("Eve" 30 87 "LA")
          '("Frank" 25 92 "NYC")
          '("Grace" 35 88 "Chicago")
          '("Hank" 28 95 "LA")))

  (unwind-protect
      (list
        ;; Sort by name
        (mapcar 'car (sort (copy-sequence neovm--test-sort-records)
                           (lambda (a b) (string< (car a) (car b)))))
        ;; Sort by age ascending, then name
        (mapcar (lambda (r) (list (car r) (cadr r)))
                (sort (copy-sequence neovm--test-sort-records)
                      (lambda (a b)
                        (or (< (cadr a) (cadr b))
                            (and (= (cadr a) (cadr b))
                                 (string< (car a) (car b)))))))
        ;; Sort by score descending, then age ascending
        (mapcar (lambda (r) (list (car r) (nth 2 r) (cadr r)))
                (sort (copy-sequence neovm--test-sort-records)
                      (lambda (a b)
                        (or (> (nth 2 a) (nth 2 b))
                            (and (= (nth 2 a) (nth 2 b))
                                 (< (cadr a) (cadr b)))))))
        ;; Sort by city then score descending
        (mapcar (lambda (r) (list (nth 3 r) (car r) (nth 2 r)))
                (sort (copy-sequence neovm--test-sort-records)
                      (lambda (a b)
                        (or (string< (nth 3 a) (nth 3 b))
                            (and (string= (nth 3 a) (nth 3 b))
                                 (> (nth 2 a) (nth 2 b))))))))
    (makunbound 'neovm--test-sort-records)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Alice\" \"Bob\" \"Carol\" \"Dave\" \"Eve\" \"Frank\" \"Grace\" \"Hank\") ((\"Bob\" 25) (\"Frank\" 25) (\"Dave\" 28) (\"Hank\" 28) (\"Alice\" 30) (\"Eve\" 30) (\"Carol\" 35) (\"Grace\" 35)) ((\"Hank\" 95 28) (\"Alice\" 95 30) (\"Carol\" 95 35) (\"Frank\" 92 25) (\"Dave\" 92 28) (\"Grace\" 88 35) (\"Bob\" 87 25) (\"Eve\" 87 30)) ((\"Chicago\" \"Dave\" 92) (\"Chicago\" \"Grace\" 88) (\"LA\" \"Hank\" 95) (\"LA\" \"Bob\" 87) (\"LA\" \"Eve\" 87) (\"NYC\" \"Alice\" 95) (\"NYC\" \"Carol\" 95) (\"NYC\" \"Frank\" 92)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sorting with side effects in comparator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_comprehensive_side_effects_in_comparator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((comparison-log nil)
      (comparison-count 0))
  (let ((data (list 4 2 7 1 5 3 6)))
    (let ((sorted (sort (copy-sequence data)
                        (lambda (a b)
                          (setq comparison-count (1+ comparison-count))
                          (setq comparison-log
                                (cons (list a b (< a b)) comparison-log))
                          (< a b)))))
      (list
        ;; Sorted result
        sorted
        ;; Number of comparisons performed
        comparison-count
        ;; First few comparisons logged
        (let ((n (min 5 (length comparison-log))))
          (last comparison-log n))
        ;; Verify all comparisons are valid (both args from original data)
        (let ((ok t))
          (dolist (entry comparison-log)
            (unless (and (memq (car entry) data)
                         (memq (cadr entry) data))
              (setq ok nil)))
          ok)))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7) 12 ((1 2 t) (1 4 t) (7 4 nil) (7 2 nil) (2 4 t)) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Destructive vs non-destructive (copy-sequence) sort
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_comprehensive_destructive_behavior() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((original (list 5 3 8 1 9 2 7 4 6)))
  ;; Save a copy of the original values
  (let ((saved-values (copy-sequence original)))
    ;; Sort a copy (non-destructive pattern)
    (let ((sorted-copy (sort (copy-sequence original) '<)))
      ;; Sort the original (destructive)
      (let ((sorted-orig (sort original '<)))
        (list
          ;; sorted copy is correct
          (equal sorted-copy '(1 2 3 4 5 6 7 8 9))
          ;; sorted orig is correct
          (equal sorted-orig '(1 2 3 4 5 6 7 8 9))
          ;; saved values still intact
          saved-values
          ;; The result of sort IS the destructively modified list
          (eq sorted-orig original))))))
"#;
    let expect = expect_test::expect![[r#""OK (t t (5 3 8 1 9 2 7 4 6) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Edge cases: already sorted, reverse sorted, all equal, two elements
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_comprehensive_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Already sorted ascending
  (sort (list 1 2 3 4 5 6 7 8 9 10) '<)
  ;; Already sorted descending (sort ascending)
  (sort (list 10 9 8 7 6 5 4 3 2 1) '<)
  ;; All elements equal
  (sort (list 5 5 5 5 5 5 5) '<)
  ;; Two elements in order
  (sort (list 1 2) '<)
  ;; Two elements reversed
  (sort (list 2 1) '<)
  ;; Large range of values
  (sort (list 1000000 -1000000 0 999999 -999999 1 -1) '<)
  ;; Alternating pattern
  (sort (list 1 10 2 9 3 8 4 7 5 6) '<)
  ;; Duplicate-heavy list
  (sort (list 3 1 3 1 3 1 2 2 2) '<)
  ;; Negative numbers
  (sort (list -5 -1 -9 -3 -7 -2 -8 -4 -6) '<)
  ;; Mixed positive, negative, zero
  (sort (list 0 -1 1 -2 2 -3 3 0 0) '<))
"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8 9 10) (1 2 3 4 5 6 7 8 9 10) (5 5 5 5 5 5 5) (1 2) (1 2) (-1000000 -999999 -1 0 1 999999 1000000) (1 2 3 4 5 6 7 8 9 10) (1 1 1 2 2 2 3 3 3) (-9 -8 -7 -6 -5 -4 -3 -2 -1) (-3 -2 -1 0 0 0 1 2 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sort with :in-place keyword
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_comprehensive_in_place() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; sort with :in-place nil (default, returns new)
  (let ((v [5 3 1 4 2]))
    (let ((result (sort v '< :in-place t)))
      (list result (equal result [1 2 3 4 5]))))
  ;; sort with :reverse
  (sort (list 1 2 3 4 5) :lessp '< :reverse t)
  ;; Combining :key :lessp :reverse
  (sort (list '(3 . "c") '(1 . "a") '(5 . "e") '(2 . "b") '(4 . "d"))
        :key 'car :lessp '< :reverse t))
"#;
    let expect = expect_test::expect![[r#""ERR (error \"Invalid argument list\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Implementing merge sort manually and comparing with built-in
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_comprehensive_manual_merge_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (fset 'neovm--test-msort-merge
    (lambda (left right pred)
      "Merge two sorted lists."
      (let ((result nil))
        (while (and left right)
          (if (funcall pred (car left) (car right))
              (progn (setq result (cons (car left) result))
                     (setq left (cdr left)))
            (progn (setq result (cons (car right) result))
                   (setq right (cdr right)))))
        (nconc (nreverse result) (or left right)))))

  (fset 'neovm--test-msort
    (lambda (lst pred)
      "Merge sort implementation."
      (if (or (null lst) (null (cdr lst)))
          lst
        (let* ((mid (/ (length lst) 2))
               (left nil) (right nil) (i 0))
          (dolist (x lst)
            (if (< i mid)
                (setq left (cons x left))
              (setq right (cons x right)))
            (setq i (1+ i)))
          (setq left (nreverse left))
          (setq right (nreverse right))
          (funcall 'neovm--test-msort-merge
                   (funcall 'neovm--test-msort left pred)
                   (funcall 'neovm--test-msort right pred)
                   pred)))))

  (unwind-protect
      (let ((data '(38 27 43 3 9 82 10 55 17 41 23 67 5 91 33 12 76 50 4 29)))
        (list
          ;; Manual merge sort
          (funcall 'neovm--test-msort data '<)
          ;; Built-in sort
          (sort (copy-sequence data) '<)
          ;; They agree
          (equal (funcall 'neovm--test-msort (copy-sequence data) '<)
                 (sort (copy-sequence data) '<))
          ;; Descending
          (equal (funcall 'neovm--test-msort (copy-sequence data) '>)
                 (sort (copy-sequence data) '>))))
    (fmakunbound 'neovm--test-msort-merge)
    (fmakunbound 'neovm--test-msort)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((3 4 5 9 10 12 17 23 27 29 33 38 41 43 50 55 67 76 82 91) (3 4 5 9 10 12 17 23 27 29 33 38 41 43 50 55 67 76 82 91) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sort with complex key extraction: hash-table frequency sort
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_comprehensive_frequency_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((data '(4 2 7 2 3 4 4 1 2 3 7 7 7 1 5)))
  ;; Build frequency table
  (let ((freq (make-hash-table)))
    (dolist (x data)
      (puthash x (1+ (gethash x freq 0)) freq))
    ;; Get unique elements
    (let ((uniq nil))
      (maphash (lambda (k v) (setq uniq (cons k uniq))) freq)
      ;; Sort by frequency descending, then by value ascending
      (let ((sorted-by-freq
             (sort uniq
                   (lambda (a b)
                     (let ((fa (gethash a freq))
                           (fb (gethash b freq)))
                       (or (> fa fb)
                           (and (= fa fb) (< a b))))))))
        (list
          ;; Elements sorted by frequency
          sorted-by-freq
          ;; Their frequencies
          (mapcar (lambda (x) (cons x (gethash x freq))) sorted-by-freq)
          ;; Expand back to full sorted list
          (let ((result nil))
            (dolist (x sorted-by-freq)
              (dotimes (_ (gethash x freq))
                (setq result (cons x result))))
            (nreverse result)))))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((7 2 4 1 3 5) ((7 . 4) (2 . 3) (4 . 3) (1 . 2) (3 . 2) (5 . 1)) (7 7 7 7 2 2 2 4 4 4 1 1 3 3 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Comprehensive oracle parity tests for `seq.el` operations:
//! `seq-take`, `seq-drop`, `seq-take-while`, `seq-drop-while`,
//! `seq-concatenate`, `seq-into`, `seq-empty-p`, `seq-sort`,
//! `seq-partition`, `seq-group-by`, `seq-map`, `seq-filter`,
//! `seq-reduce`, `seq-find`, `seq-some`, `seq-every-p`,
//! `seq-count`, `seq-uniq`, `seq-elt`, `seq-length`,
//! and complex multi-type pipelines.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// seq-take, seq-drop on lists, vectors, strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_take_drop_all_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; seq-take on list
  (seq-take '(a b c d e f) 3)
  (seq-take '(a b c) 0)
  (seq-take '(a b c) 5)  ;; more than length
  (seq-take nil 3)        ;; empty list
  ;; seq-take on vector
  (seq-take [10 20 30 40 50] 2)
  (seq-take [10 20 30] 0)
  (seq-take [10 20 30] 10)
  ;; seq-take on string
  (seq-take "hello world" 5)
  (seq-take "abc" 0)
  (seq-take "" 3)
  ;; seq-drop on list
  (seq-drop '(a b c d e f) 3)
  (seq-drop '(a b c) 0)
  (seq-drop '(a b c) 5)
  (seq-drop nil 2)
  ;; seq-drop on vector
  (seq-drop [10 20 30 40 50] 2)
  (seq-drop [10 20] 0)
  (seq-drop [10 20] 5)
  ;; seq-drop on string
  (seq-drop "hello world" 6)
  (seq-drop "abc" 0)
  (seq-drop "abc" 10)
  ;; Complementary: (append (seq-take s n) (seq-drop s n)) = s
  (let ((s '(1 2 3 4 5)))
    (equal s (append (seq-take s 3) (seq-drop s 3))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((a b c) nil (a b c) nil [10 20] [] [10 20 30] \"hello\" \"\" \"\" (d e f) (a b c) nil nil [30 40 50] [10 20] [] \"world\" \"abc\" \"\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_seq_take_drop_nonpositive_identity_and_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'seq)
  (let ((lst (list 'a 'b 'c))
        (vec [a b c])
        (str (copy-sequence "abc")))
    (list
      ;; GNU seq-take returns an empty same-type sequence for N <= 0.
      (seq-take lst -1)
      (seq-take lst 0)
      (seq-take vec -1)
      (seq-take vec 0)
      (seq-take str -1)
      (seq-take str 0)
      ;; GNU seq-drop returns SEQUENCE itself for N <= 0.
      (eq (seq-drop lst -1) lst)
      (eq (seq-drop lst 0) lst)
      (eq (seq-drop vec -1) vec)
      (eq (seq-drop vec 0) vec)
      (eq (seq-drop str -1) str)
      (eq (seq-drop str 0) str)
      ;; Dropping past end returns an empty same-type sequence.
      (seq-drop lst 99)
      (seq-drop vec 99)
      (seq-drop str 99))))"#;
    let expect =
        expect_test::expect![[r#""OK (nil nil [] [] \"\" \"\" t t t t t t nil [] \"\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-take-while, seq-drop-while with complex predicates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_take_while_drop_while_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; seq-take-while: take while ascending
  (seq-take-while (let ((prev -999))
                    (lambda (x)
                      (prog1 (> x prev)
                        (setq prev x))))
                  '(1 3 5 7 4 8 9))
  ;; seq-take-while: all match
  (seq-take-while #'cl-evenp '(2 4 6 8))
  ;; seq-take-while: none match
  (seq-take-while #'cl-evenp '(1 3 5))
  ;; seq-take-while: empty
  (seq-take-while #'identity nil)
  ;; seq-take-while on vector
  (seq-take-while (lambda (x) (< x 10)) [1 5 8 12 3 7])
  ;; seq-take-while on string (lowercase chars)
  (seq-take-while (lambda (c) (and (>= c ?a) (<= c ?z))) "helloWorld")
  ;; seq-drop-while: drop while even
  (seq-drop-while #'cl-evenp '(2 4 6 7 8 9))
  ;; seq-drop-while: all match (drops everything)
  (seq-drop-while #'numberp '(1 2 3))
  ;; seq-drop-while: none match (drops nothing)
  (seq-drop-while #'stringp '(1 2 3))
  ;; seq-drop-while on vector
  (seq-drop-while (lambda (x) (< x 5)) [1 2 3 4 5 6 7])
  ;; seq-drop-while on string
  (seq-drop-while (lambda (c) (= c ? )) "   hello")
  ;; Complementary: (append (take-while p s) (drop-while p s)) = s
  (let ((s '(2 4 6 7 8 10)))
    (equal s (append (seq-take-while #'cl-evenp s)
                     (seq-drop-while #'cl-evenp s))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 3 5 7) (2 4 6 8) nil nil [1 5 8] \"hello\" (7 8 9) nil (1 2 3) [5 6 7] \"hello\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_seq_take_drop_while_call_boundary_and_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'seq)
  (let ((lst '(1 2 3 0 4))
        (vec [1 2 3 0 4])
        (str (copy-sequence "abcDef")))
    (list
      ;; GNU seq--count-successive calls PRED through the first failing
      ;; element, then stops.
      (let ((calls nil))
        (list
         (seq-take-while (lambda (x)
                           (push x calls)
                           (> x 0))
                         lst)
         (nreverse calls)))
      (let ((calls nil))
        (list
         (seq-drop-while (lambda (x)
                           (push x calls)
                           (> x 0))
                         vec)
         (nreverse calls)))
      (let ((calls nil))
        (list
         (seq-take-while (lambda (c)
                           (push c calls)
                           (and (>= c ?a) (<= c ?z)))
                         str)
         (nreverse calls)))
      ;; If the first predicate result is nil, seq-drop-while delegates to
      ;; seq-drop with N=0 and returns the original sequence object.
      (eq (seq-drop-while #'null lst) lst)
      (eq (seq-drop-while #'null vec) vec)
      (eq (seq-drop-while #'null str) str)
      ;; Predicate errors propagate from seq--count-successive.
      (condition-case err
          (seq-take-while (lambda (_x)
                            (signal 'wrong-type-argument '(integerp bad)))
                          lst)
        (error (list (car err) (cadr err))))
      (condition-case err
          (seq-drop-while (lambda (_x)
                            (signal 'wrong-type-argument '(integerp bad)))
                          lst)
        (error (list (car err) (cadr err)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 2 3) (1 2 3 0)) ([0 4] (1 2 3 0)) (\"abc\" (97 98 99 68)) t t t (wrong-type-argument integerp) (wrong-type-argument integerp))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-empty-p on various types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_empty_p_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; Empty cases
  (seq-empty-p nil)
  (seq-empty-p '())
  (seq-empty-p [])
  (seq-empty-p "")
  ;; Non-empty cases
  (seq-empty-p '(1))
  (seq-empty-p [0])
  (seq-empty-p "x")
  ;; Result of filtering everything out
  (seq-empty-p (seq-filter (lambda (x) (> x 100)) '(1 2 3)))
  ;; Result of seq-take 0
  (seq-empty-p (seq-take '(1 2 3) 0))
  ;; Result of seq-drop all
  (seq-empty-p (seq-drop '(1 2 3) 3))
  ;; seq-length of empty
  (seq-length nil)
  (seq-length [])
  (seq-length "")
  ;; seq-length of non-empty
  (seq-length '(a b c d e))
  (seq-length [1 2 3])
  (seq-length "hello")))"#;
    let expect = expect_test::expect![[r#""OK (t t t t nil nil nil t t t 0 0 0 5 3 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-reduce with various accumulator patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_reduce_accumulator_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; Build reversed list
  (seq-reduce (lambda (acc x) (cons x acc)) '(1 2 3 4 5) nil)
  ;; Running maximum
  (seq-reduce #'max '(3 7 2 9 1 8) 0)
  ;; Count elements matching predicate
  (seq-reduce (lambda (acc x) (if (cl-evenp x) (1+ acc) acc))
              '(1 2 3 4 5 6 7 8 9 10) 0)
  ;; Flatten one level
  (seq-reduce (lambda (acc x) (append acc x))
              '((1 2) (3 4) (5 6)) nil)
  ;; Build string from chars
  (seq-reduce (lambda (acc c) (concat acc (char-to-string c)))
              '(?h ?e ?l ?l ?o) "")
  ;; Partition into even/odd using reduce
  (seq-reduce (lambda (acc x)
                (if (cl-evenp x)
                    (list (cons x (car acc)) (cadr acc))
                  (list (car acc) (cons x (cadr acc)))))
              '(1 2 3 4 5 6 7 8)
              '(nil nil))
  ;; Reduce on vector
  (seq-reduce (lambda (acc x) (+ acc (* x x))) [1 2 3 4 5] 0)
  ;; Reduce on string (build char frequency alist)
  (let ((freqs (seq-reduce
                (lambda (acc c)
                  (let ((entry (assq c acc)))
                    (if entry
                        (progn (setcdr entry (1+ (cdr entry))) acc)
                      (cons (cons c 1) acc))))
                "mississippi" nil)))
    (sort freqs (lambda (a b) (< (car a) (car b)))))
  ;; Reduce empty sequence returns initial value
  (seq-reduce #'+ nil 42)))"#;
    let expect = expect_test::expect![[
        r#""OK ((5 4 3 2 1) 9 5 (1 2 3 4 5 6) \"hello\" ((8 6 4 2) (7 5 3 1)) 55 ((105 . 4) (109 . 1) (112 . 2) (115 . 4)) 42)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-find, seq-some, seq-every-p: edge cases and return values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_find_some_every_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; seq-find returns the element, not the predicate result
  (seq-find (lambda (x) (> x 10)) '(5 8 12 15))
  ;; seq-find with default when not found
  (seq-find (lambda (x) (> x 100)) '(1 2 3) 'default-val)
  ;; seq-find returns nil without default when not found
  (seq-find (lambda (x) (> x 100)) '(1 2 3))
  ;; seq-find on nil element: tricky because nil is also "not found"
  (seq-find #'null '(1 2 nil 3))
  ;; seq-find with default distinguishes nil-found from not-found
  (seq-find #'null '(1 2 3) 'not-found)
  ;; seq-some returns the predicate's return value (not the element)
  (seq-some (lambda (x) (and (> x 5) (* x 10))) '(1 3 6 8))
  ;; seq-some with #'identity: first truthy element
  (seq-some #'identity '(nil nil 42 nil 99))
  ;; seq-some all nil
  (seq-some #'identity '(nil nil nil))
  ;; seq-every-p on mixed types
  (seq-every-p #'numberp '(1 2 3.0 4))
  (seq-every-p #'numberp '(1 2 "three" 4))
  ;; seq-every-p vacuous truth
  (seq-every-p #'stringp nil)
  (seq-every-p #'stringp [])
  ;; Nested: seq-every-p checking sub-lists
  (seq-every-p (lambda (sub)
                 (seq-every-p #'numberp sub))
               '((1 2 3) (4 5 6) (7 8 9)))
  (seq-every-p (lambda (sub)
                 (seq-every-p #'numberp sub))
               '((1 2 3) (4 "x" 6) (7 8 9)))))"#;
    let expect = expect_test::expect![[
        r#""OK (12 default-val nil nil not-found 60 42 nil t nil t t t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-count with complex predicates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_count_complex_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; Basic count
  (seq-count #'cl-evenp '(1 2 3 4 5 6 7 8 9 10))
  ;; Count elements greater than mean
  (let* ((data '(3 7 2 9 1 8 4 6 5 10))
         (mean (/ (seq-reduce #'+ data 0) (seq-length data))))
    (seq-count (lambda (x) (> x mean)) data))
  ;; Count on vector
  (seq-count (lambda (x) (= 0 (% x 3))) [1 3 5 6 9 10 12 15])
  ;; Count vowels in string
  (seq-count (lambda (c) (memq c '(?a ?e ?i ?o ?u)))
             "the quick brown fox")
  ;; Count with stateful predicate (count ascending pairs)
  (let ((prev nil))
    (seq-count (lambda (x)
                 (prog1 (and prev (> x prev))
                   (setq prev x)))
               '(1 3 2 5 4 7 6)))
  ;; Count on empty
  (seq-count #'identity nil)
  ;; Count where predicate always true
  (seq-count #'identity '(t t t t))
  ;; Count where predicate always false
  (seq-count #'null '(1 2 3 4))))"#;
    let expect = expect_test::expect![[r#""OK (5 5 5 5 3 0 4 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_seq_count_predicate_values_calls_and_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'seq)
  (list
    ;; GNU seq-count increments for any non-nil predicate value, not only t.
    (seq-count (lambda (x) (and (> x 1) (list 'truth x))) '(0 1 2 3))
    ;; Predicate is called once per element in sequence order.
    (let ((calls nil))
      (list
       (seq-count (lambda (x)
                    (push x calls)
                    (memq x '(b d)))
                  '(a b c d))
       (nreverse calls)))
    ;; Empty sequences return 0 without calling the predicate.
    (let ((called nil))
      (list
       (seq-count (lambda (_x) (setq called t)) nil)
       called))
    ;; Strings are iterated as characters.
    (seq-count (lambda (c) (and (>= c ?0) (<= c ?9))) "a1b23")
    ;; Predicate errors propagate unchanged.
    (condition-case err
        (seq-count (lambda (_x) (signal 'wrong-type-argument '(integerp bad))) '(1 2))
      (error (list (car err) (cadr err))))))"#;
    let expect = expect_test::expect![[
        r#""OK (2 (2 (a b c d)) (0 nil) 3 (wrong-type-argument integerp))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-uniq with custom test functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_uniq_custom_test_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; Default equality (eq for symbols)
  (seq-uniq '(a b a c b d c e))
  ;; With #'equal for structural equality
  (seq-uniq '((1 2) (3 4) (1 2) (5 6) (3 4)) #'equal)
  ;; Case-insensitive string dedup
  (seq-uniq '("Hello" "hello" "HELLO" "World" "world")
            (lambda (a b) (string= (downcase a) (downcase b))))
  ;; Dedup by modular equivalence
  (seq-uniq '(1 11 2 12 3 13 4 14 5 15)
            (lambda (a b) (= (% a 10) (% b 10))))
  ;; Dedup by first character
  (seq-uniq '("apple" "avocado" "banana" "blueberry" "cherry" "coconut")
            (lambda (a b) (= (aref a 0) (aref b 0))))
  ;; seq-uniq preserves first occurrence order
  (seq-uniq '(3 1 4 1 5 9 2 6 5 3 5))
  ;; seq-uniq on vector
  (seq-uniq [1 2 2 3 3 3 4 4 4 4])
  ;; seq-uniq on empty
  (seq-uniq nil)
  ;; seq-uniq single element
  (seq-uniq '(42))))"#;
    let expect = expect_test::expect![[
        r#""OK ((a b c d e) ((1 2) (3 4) (5 6)) (\"Hello\" \"World\") (1 2 3 4 5) (\"apple\" \"banana\" \"cherry\") (3 1 4 5 9 2 6) (1 2 3 4) nil (42))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-sort and seq-sort-by comprehensive
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_sort_sort_by_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; seq-sort: stable sort check (equal elements preserve order)
  (let ((data '((a . 3) (b . 1) (c . 3) (d . 1) (e . 2))))
    (seq-map #'car
             (seq-sort (lambda (x y) (< (cdr x) (cdr y))) data)))
  ;; seq-sort on vector returns vector (actually returns list in seq.el)
  (seq-sort #'< [5 3 1 4 2])
  ;; seq-sort with string comparison
  (seq-sort #'string< '("delta" "alpha" "charlie" "bravo" "echo"))
  ;; seq-sort-by: sort records by field
  (seq-sort-by #'cadr #'<
               '((alice 85) (bob 92) (carol 78) (dave 95) (eve 88)))
  ;; seq-sort-by: sort strings by reverse
  (seq-sort-by (lambda (s) (concat (nreverse (string-to-list s))))
               #'string<
               '("abc" "xyz" "mno" "def"))
  ;; seq-sort-by: sort by absolute value
  (seq-sort-by #'abs #'< '(-5 3 -1 4 -2 0 7 -6))
  ;; seq-sort-by: sort by length then alphabetically for ties
  (seq-sort (lambda (a b)
              (or (< (length a) (length b))
                  (and (= (length a) (length b))
                       (string< a b))))
            '("fig" "date" "apple" "kiwi" "banana" "cherry" "pear"))
  ;; seq-sort empty
  (seq-sort #'< nil)))"#;
    let expect = expect_test::expect![[
        r#""OK ((b d e a c) [1 2 3 4 5] (\"alpha\" \"bravo\" \"charlie\" \"delta\" \"echo\") ((carol 78) (alice 85) (eve 88) (bob 92) (dave 95)) (\"abc\" \"def\" \"mno\" \"xyz\") (0 -1 -2 3 4 -5 -6 7) (\"fig\" \"date\" \"kiwi\" \"pear\" \"apple\" \"banana\" \"cherry\") nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-group-by and seq-partition comprehensive
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_group_by_partition_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; GNU seq-partition: split into same-type chunks of length N.
  (seq-partition '(1 2 3 4 5 6 7 8 9 10) 4)
  (seq-partition '(1 2 3) 10)
  (seq-partition nil 2)
  (seq-partition [1 8 3 9 2 7 4 6] 3)
  (seq-partition "abcdefg" 3)
  (seq-partition '(1 2 3) 0)
  (seq-partition '(1 2 3) -2)
  ;; seq-group-by: group numbers by sign
  (let ((groups (seq-group-by (lambda (x) (cond ((> x 0) 'pos)
                                                  ((< x 0) 'neg)
                                                  (t 'zero)))
                               '(-3 0 5 -1 7 0 -2 4 0 8))))
    (sort (seq-map (lambda (g) (cons (car g) (sort (cdr g) #'<)))
                    groups)
          (lambda (a b) (string< (symbol-name (car a))
                                  (symbol-name (car b))))))
  ;; seq-group-by: group strings by first character
  (let ((groups (seq-group-by (lambda (s) (aref s 0))
                               '("apple" "avocado" "banana" "blueberry"
                                 "cherry" "coconut" "apricot"))))
    (sort groups (lambda (a b) (< (car a) (car b)))))
  ;; seq-group-by: group by remainder mod 3
  (let ((groups (seq-group-by (lambda (x) (% x 3))
                               '(1 2 3 4 5 6 7 8 9 10 11 12))))
    (sort groups (lambda (a b) (< (car a) (car b)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 2 3 4) (5 6 7 8) (9 10)) ((1 2 3)) nil ([1 8 3] [9 2 7] [4 6]) (\"abc\" \"def\" \"g\") nil nil ((neg -3 -2 -1) (pos 4 5 7 8) (zero 0 0 0)) ((97 \"apple\" \"avocado\" \"apricot\") (98 \"banana\" \"blueberry\") (99 \"cherry\" \"coconut\")) ((0 3 6 9 12) (1 1 4 7 10) (2 2 5 8 11)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-concatenate and seq-into: type conversions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_concatenate_into_advanced() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; seq-concatenate: multiple lists into vector
  (seq-concatenate 'vector '(1 2 3) '(4 5 6) '(7 8 9))
  ;; seq-concatenate: mix types into list
  (seq-concatenate 'list [1 2 3] '(4 5 6) [7 8 9])
  ;; seq-concatenate: strings
  (seq-concatenate 'string "hello" " " "world" "!")
  ;; seq-concatenate: single sequence
  (seq-concatenate 'list '(1 2 3))
  ;; seq-concatenate: empty sequences
  (seq-concatenate 'list nil nil '(1 2) nil)
  ;; seq-into: round-trip list -> vector -> list
  (let* ((original '(1 2 3 4 5))
         (as-vec (seq-into original 'vector))
         (back (seq-into as-vec 'list)))
    (list (equal original back) as-vec back))
  ;; seq-into: string -> list -> string
  (let* ((s "hello")
         (chars (seq-into s 'list))
         (back (seq-into chars 'string)))
    (list (equal s back) chars))
  ;; seq-into: filtered then converted
  (seq-into (seq-filter #'cl-evenp '(1 2 3 4 5 6)) 'vector)
  ;; seq-into: empty
  (seq-into nil 'vector)
  (seq-into [] 'list)))"#;
    let expect = expect_test::expect![[
        r#""OK ([1 2 3 4 5 6 7 8 9] (1 2 3 4 5 6 7 8 9) \"hello world!\" (1 2 3) (1 2) (t [1 2 3 4 5] (1 2 3 4 5)) (t (104 101 108 108 111)) [2 4 6] [] nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: multi-step transformation pipeline across types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_complex_pipeline_across_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A complex pipeline that exercises many seq operations together,
    // converting between types and using nested seq calls.
    let form = r#"((require (quote cl-lib)) let ((inventory
        '((:name "Widget A" :price 25 :qty 100 :category "electronics")
          (:name "Widget B" :price 50 :qty 30  :category "electronics")
          (:name "Gadget X" :price 15 :qty 200 :category "toys")
          (:name "Gadget Y" :price 75 :qty 10  :category "toys")
          (:name "Doohick"  :price 5  :qty 500 :category "misc")
          (:name "Thingamajig" :price 120 :qty 5 :category "electronics")
          (:name "Whatsit"  :price 35 :qty 80  :category "misc"))))
  (let ((get-price (lambda (item) (plist-get item :price)))
        (get-qty   (lambda (item) (plist-get item :qty)))
        (get-cat   (lambda (item) (plist-get item :category)))
        (get-name  (lambda (item) (plist-get item :name))))
    (list
      ;; 1. Total inventory value
      (seq-reduce (lambda (acc item)
                    (+ acc (* (funcall get-price item)
                              (funcall get-qty item))))
                  inventory 0)
      ;; 2. Most expensive item name
      (funcall get-name
               (seq-reduce (lambda (best item)
                             (if (> (funcall get-price item)
                                    (funcall get-price best))
                                 item best))
                           (cdr inventory) (car inventory)))
      ;; 3. Categories with item counts, sorted
      (let ((groups (seq-group-by get-cat inventory)))
        (seq-sort-by #'car #'string<
                     (seq-map (lambda (g)
                                (cons (car g) (length (cdr g))))
                              groups)))
      ;; 4. Items with value > 2000, sorted by value desc
      (seq-map get-name
               (seq-sort-by (lambda (item)
                              (* (funcall get-price item)
                                 (funcall get-qty item)))
                            #'>
                            (seq-filter (lambda (item)
                                          (> (* (funcall get-price item)
                                                (funcall get-qty item))
                                             2000))
                                        inventory)))
      ;; 5. Average price
      (/ (seq-reduce (lambda (acc item) (+ acc (funcall get-price item)))
                     inventory 0)
         (seq-length inventory))
      ;; 6. Any item over $100?
      (and (seq-some (lambda (item) (> (funcall get-price item) 100))
                     inventory)
           t)
      ;; 7. All items have positive quantity?
      (seq-every-p (lambda (item) (> (funcall get-qty item) 0))
                   inventory))))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Extended oracle parity tests for `seq.el` operations:
//! seq-map-indexed, seq-do, seq-let, seq-into, seq-concatenate,
//! seq-mapcat, seq-sort-by, seq-group-by, seq-min, seq-max,
//! seq-position, seq-contains-p, seq-difference, seq-intersection,
//! seq-subseq. Tests with lists, vectors, and strings.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// seq-map-indexed and seq-do with side-effects
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_ext_map_indexed_and_do() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // seq-map-indexed passes index as second arg.
    // seq-do is for side-effects only (returns nil).
    let form = r#"(let ((log nil))
  (list
    ;; seq-map-indexed on a list: (element . index) pairs
    (seq-map-indexed (lambda (elt idx) (cons elt idx)) '(a b c d e))
    ;; seq-map-indexed on a vector
    (seq-map-indexed (lambda (elt idx) (list idx (* elt elt))) [3 5 7 9])
    ;; seq-map-indexed on a string: (char . index)
    (seq-map-indexed (lambda (ch idx) (cons (char-to-string ch) idx)) "hello")
    ;; seq-do: accumulate side effects, returns nil
    (progn
      (seq-do (lambda (x) (setq log (cons (* x x) log))) '(1 2 3 4 5))
      (list (nreverse log)))
    ;; seq-map-indexed with empty sequence
    (seq-map-indexed (lambda (e i) (cons e i)) nil)
    ;; seq-map-indexed building a hash-like alist from vector
    (seq-map-indexed
      (lambda (name idx)
        (list :id idx :name name))
      ["alice" "bob" "carol"])))"#;
    let expect = expect_test::expect![[
        r#""OK (((a . 0) (b . 1) (c . 2) (d . 3) (e . 4)) ((0 9) (1 25) (2 49) (3 81)) ((\"h\" . 0) (\"e\" . 1) (\"l\" . 2) (\"l\" . 3) (\"o\" . 4)) ((1 4 9 16 25)) nil ((:id 0 :name \"alice\") (:id 1 :name \"bob\") (:id 2 :name \"carol\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_seq_ext_iteration_short_circuit_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU seq-some, seq-every-p, and seq-find use catch/throw to stop as soon
    // as the result is known.  seq-count and seq-reduce traverse strictly
    // left-to-right through seq-doseq.
    let form = r#"(progn
  (require 'seq)
  (let ((log nil))
    (list
      (seq-some (lambda (x)
                  (push x log)
                  (and (> x 2) (list 'hit x)))
                '(1 2 3 4))
      (nreverse log)
      (setq log nil)
      (seq-every-p (lambda (x)
                     (push x log)
                     (< x 3))
                   '(1 2 3 4))
      (nreverse log)
      (setq log nil)
      (seq-find (lambda (x)
                  (push x log)
                  (zerop (% x 3)))
                '(1 2 3 4 5)
                'fallback)
      (nreverse log)
      (seq-find (lambda (_x) nil) '(1 2) 'fallback)
      (setq log nil)
      (seq-count (lambda (x)
                   (push x log)
                   (zerop (% x 2)))
                 [1 2 3 4])
      (nreverse log)
      (setq log nil)
      (seq-reduce (lambda (acc x)
                    (push (list acc x) log)
                    (+ acc x))
                  '(1 2 3)
                  10)
      (nreverse log)
      (condition-case err
          (seq-some (lambda (x)
                      (if (= x 2)
                          (signal 'wrong-type-argument '(integerp bad))
                        nil))
                    '(1 2 3))
        (error (list (car err) (cadr err)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((hit 3) (1 2 3) nil nil (1 2 3) nil 3 (1 2 3) fallback nil 2 (1 2 3 4) nil 16 ((10 1) (11 2) (13 3)) (wrong-type-argument integerp))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-let: destructure sequences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_ext_let_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // seq-let binds variables to elements of a sequence.
    let form = r#"(list
  ;; Basic list destructuring
  (seq-let (a b c) '(1 2 3)
    (+ a b c))
  ;; Destructure vector
  (seq-let (x y z) [10 20 30]
    (* x (+ y z)))
  ;; Destructure string (chars)
  (seq-let (a b c) "xyz"
    (list a b c))
  ;; More elements than bindings: extras ignored
  (seq-let (first second) '(10 20 30 40 50)
    (+ first second))
  ;; Fewer elements than bindings: extra vars are nil
  (seq-let (a b c d e) '(1 2)
    (list a b c d e))
  ;; Nested computation with destructured values
  (seq-let (op x y) '(+ 10 20)
    (cond
      ((eq op '+) (+ x y))
      ((eq op '-) (- x y))
      ((eq op '*) (* x y))
      (t nil)))
  ;; Empty sequence
  (seq-let (a b) nil
    (list a b)))"#;
    let expect =
        expect_test::expect![[r#""OK (6 500 (120 121 122) 30 (1 2 nil nil nil) 30 (nil nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_seq_ext_let_setq_rest_destructuring_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU seq-let/seq-setq build pcase `(seq ...)' patterns.  &rest is backed
    // by seq-drop, so the rest binding keeps the source sequence type.
    let form = r#"(progn
  (require 'seq)
  (list
    (macroexpand-1 '(seq-let (a b &rest rest) [1 2 3 4]
                      (list a b rest)))
    (seq-let (a b &rest rest) '(1 2 3 4)
      (list a b rest))
    (seq-let (a b &rest rest) [1 2 3 4]
      (list a b rest))
    (seq-let (a b &rest rest) "abcd"
      (list a b rest))
    (let ((a nil) (b nil) (rest nil))
      (seq-setq (a b &rest rest) '(x y z w))
      (list a b rest))
    (let ((a nil) (b nil) (rest nil))
      (seq-setq (a b &rest rest) [x y z w])
      (list a b rest))))"#;
    let expect = expect_test::expect![[
        r#""OK ((pcase-let (((seq a b &rest rest) [1 2 3 4])) (list a b rest)) (1 2 (3 4)) (1 2 [3 4]) (97 98 \"cd\") (x y (z w)) (x y [z w]))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-into and seq-concatenate: type conversion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_ext_into_and_concatenate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // seq-into converts a sequence to a specified type.
    // seq-concatenate concatenates sequences into a target type.
    let form = r#"(list
  ;; seq-into: list -> vector
  (seq-into '(1 2 3 4 5) 'vector)
  ;; seq-into: vector -> list
  (seq-into [10 20 30] 'list)
  ;; seq-into: string -> list (chars)
  (seq-into "hello" 'list)
  ;; seq-into: list of chars -> string
  (seq-into '(?h ?e ?l ?l ?o) 'string)
  ;; seq-concatenate: merge multiple sequences into a list
  (seq-concatenate 'list '(1 2) [3 4] '(5 6))
  ;; seq-concatenate: merge into vector
  (seq-concatenate 'vector '(1 2 3) [4 5 6])
  ;; seq-concatenate: merge into string
  (seq-concatenate 'string "hello" " " "world")
  ;; seq-into with empty
  (seq-into nil 'vector)
  ;; seq-concatenate with mixed types into list
  (seq-concatenate 'list "ab" [99 100] '(1 2)))"#;
    let expect = expect_test::expect![[
        r#""OK ([1 2 3 4 5] (10 20 30) (104 101 108 108 111) \"hello\" (1 2 3 4 5 6) [1 2 3 4 5 6] \"hello world\" [] (97 98 99 100 1 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_seq_ext_core_copy_reverse_remove_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU seq.el defines seq-first via seq-elt, seq-rest via seq-drop,
    // seq-copy via copy-sequence, seq-reverse preserving sequence type, and
    // seq-remove-at-position as two seq-subseq calls plus seq-concatenate.
    let form = r#"(progn
  (require 'seq)
  (list
    (seq-first [a b])
    (seq-rest [a b c])
    (seq-rest "abc")
    (condition-case err
        (seq-first nil)
      (error (list (car err) (cadr err))))
    (condition-case err
        (seq-first "")
      (error (list (car err) (cadr err))))
    (let ((x (list (list 1) 2)))
      (list (equal x (seq-copy x))
            (eq x (seq-copy x))
            (eq (car x) (car (seq-copy x)))))
    (let ((v [1 2 3]))
      (list (equal v (seq-copy v))
            (eq v (seq-copy v))))
    (let ((s "abc"))
      (list (equal s (seq-copy s))
            (eq s (seq-copy s))))
    (seq-reverse '(1 2 3))
    (seq-reverse [1 2 3])
    (seq-reverse "abc")
    (seq-remove-at-position '(a b c d) 2)
    (seq-remove-at-position [a b c d] 1)
    (seq-remove-at-position "abcd" 1)
    (condition-case err
        (seq-into-sequence 42)
      (error (list (car err) (cadr err))))
    (condition-case err
        (seq-remove-at-position '(a b c) 9)
      (error (list (car err) (cadr err))))
    (seq-remove-at-position '(a b c) -1)))"#;
    let expect = expect_test::expect![[
        r#""OK (a [b c] \"bc\" nil (args-out-of-range \"\") (t nil t) (t nil) (t nil) (3 2 1) [3 2 1] \"cba\" (a b d) [a c d] \"acd\" (error \"Cannot convert 42 into a sequence\") (error \"End index out of bounds: 9\") (a b a b c))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-mapcat: map then concatenate
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_ext_mapcat_type_and_error_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU seq-mapcat is implemented as seq-map followed by seq-concatenate.
    // The mapper must therefore return sequences, and TYPE controls the final
    // seq-concatenate target type.
    let form = r#"(progn
  (require 'seq)
  (list
    ;; Default TYPE is list.
    (seq-mapcat (lambda (x) (list x (- x))) '(1 2 3))
    ;; Mapper may return vectors; optional TYPE chooses vector output.
    (seq-mapcat (lambda (x) (vector x (* x 10))) [1 2 3] 'vector)
    ;; String output is produced by concatenating character sequences.
    (seq-mapcat (lambda (c) (list c ?-)) "ab" 'string)
    ;; Empty mapper results concatenate to the empty sequence of TYPE.
    (seq-mapcat (lambda (_x) nil) '(1 2 3) 'list)
    (seq-mapcat (lambda (_x) nil) '(1 2 3) 'vector)
    (seq-mapcat (lambda (_x) nil) '(1 2 3) 'string)
    ;; A non-sequence mapper result signals through seq-into-sequence.
    (condition-case err
        (seq-mapcat (lambda (x) x) '(1 2))
      (error (list (car err) (cadr err))))
    ;; Invalid output TYPE signals through seq-concatenate.
    (condition-case err
        (seq-mapcat (lambda (x) (list x)) '(1 2) 'hash-table)
      (error (list (car err) (cadr err))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 -1 2 -2 3 -3) [1 10 2 20 3 30] \"a-b-\" nil [] \"\" (error \"Cannot convert 1 into a sequence\") (error \"Not a sequence type name: hash-table\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-position and seq-contains-p
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_ext_position_and_contains() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // seq-position returns the index of an element.
    // seq-contains-p checks if an element exists.
    let form = r#"(list
  ;; seq-position: find element in list
  (seq-position '(a b c d e) 'c)
  ;; seq-position: not found returns nil
  (seq-position '(a b c) 'z)
  ;; seq-position: first occurrence
  (seq-position '(1 2 3 2 1) 2)
  ;; seq-position in vector
  (seq-position [10 20 30 40] 30)
  ;; seq-position with custom test function
  (seq-position '("apple" "banana" "cherry") "BANANA"
    (lambda (a b) (string= (downcase a) (downcase b))))
  ;; seq-contains-p: element exists
  (seq-contains-p '(1 2 3 4 5) 3)
  ;; seq-contains-p: element missing
  (seq-contains-p '(1 2 3 4 5) 99)
  ;; seq-contains-p on vector
  (seq-contains-p [a b c d] 'c)
  ;; seq-contains-p on string (char)
  (seq-contains-p "hello" ?l)
  ;; seq-position at boundaries
  (list
    (seq-position '(10 20 30) 10)
    (seq-position '(10 20 30) 30)
    (seq-position nil 1)))"#;
    let expect = expect_test::expect![[r#""OK (2 nil 1 2 1 t nil t t (0 2 nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_seq_ext_contains_obsolete_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU seq-contains is obsolete but still part of the Elisp contract.  It
    // calls TESTFN as (TESTFN ELT ELEMENT) and returns the matching element,
    // unlike seq-contains-p which calls (TESTFN ELEMENT ELT) and returns the
    // predicate's non-nil value.
    let form = r#"(progn
  (require 'seq)
  (list
    (seq-contains '(a b c) 'b)
    (seq-contains '(a nil c) nil)
    (seq-contains [10 20 30] 20)
    (seq-contains "abc" ?b)
    (seq-contains '(1 2 3) 2
                  (lambda (target element)
                    (list target element)))
    (seq-contains-p '(1 2 3) 2
                    (lambda (element target)
                      (list element target)))
    (let ((calls nil))
      (list (seq-contains '(a b c) 'b
                          (lambda (target element)
                            (push (list target element) calls)
                            (eq target element)))
            (nreverse calls)))
    (condition-case err
        (seq-contains '(1 2 3) 2
                      (lambda (_target _element)
                        (signal 'wrong-type-argument '(integerp bad))))
      (error (list (car err) (cadr err))))))"#;
    let expect = expect_test::expect![[
        r#""OK (b nil 20 98 1 (1 2) (b ((b a) (b b))) (wrong-type-argument integerp))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-difference, seq-intersection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_ext_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // seq-difference: elements in first but not second.
    // seq-intersection: elements in both.
    let form = r#"(list
  ;; seq-difference: basic
  (seq-sort #'< (seq-difference '(1 2 3 4 5) '(2 4 6)))
  ;; seq-intersection: basic
  (seq-sort #'< (seq-intersection '(1 2 3 4 5) '(2 4 6 8)))
  ;; With vectors
  (seq-sort #'< (seq-difference [10 20 30 40] [20 40 50]))
  (seq-sort #'< (seq-intersection [10 20 30 40] [20 40 50]))
  ;; Empty cases
  (seq-difference '(1 2 3) nil)
  (seq-difference nil '(1 2 3))
  (seq-intersection '(1 2 3) nil)
  ;; Identical sets
  (seq-sort #'< (seq-intersection '(1 2 3) '(3 2 1)))
  (seq-difference '(1 2 3) '(3 2 1))
  ;; Custom test function: case-insensitive strings
  (let ((strs1 '("Apple" "Banana" "Cherry"))
        (strs2 '("banana" "date" "cherry")))
    (list
      (seq-difference strs1 strs2
        (lambda (a b) (string= (downcase a) (downcase b))))
      (seq-intersection strs1 strs2
        (lambda (a b) (string= (downcase a) (downcase b)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 3 5) (2 4) (10 30) (20 40) (1 2 3) nil nil (1 2 3) nil ((\"Apple\") (\"Banana\" \"Cherry\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_seq_ext_set_operations_order_and_type_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU set operations always return lists.  seq-union de-duplicates while
    // preserving first-seen order; intersection/difference preserve elements
    // from sequence1, including duplicates.
    let form = r#"(progn
  (require 'seq)
  (list
    ;; Union de-duplicates, keeping first appearances from sequence1 then sequence2.
    (seq-union '(a b a c) '(b d a e))
    ;; Intersection preserves sequence1 order and duplicate matching elements.
    (seq-intersection '(a b a c b) '(b a))
    ;; Difference preserves sequence1 order and duplicate nonmatching elements.
    (seq-difference '(a b a c b) '(b))
    ;; Vector inputs still return a list.
    (list (seq-union [1 2 1] [2 3 1])
          (type-of (seq-union [1 2 1] [2 3 1])))
    ;; String inputs are sequences of character codes and still return lists.
    (seq-intersection "abacad" "ca")
    (seq-difference "abacad" "ca")
    ;; Custom test function participates in union de-duplication.
    (let ((calls nil))
      (list
        (seq-union '("A" "b") '("a" "C")
                   (lambda (a b)
                     (push (list a b) calls)
                     (string= (downcase a) (downcase b))))
        calls))))"#;
    let expect = expect_test::expect![[
        r#""OK ((a b c d e) (a b a b) (a a c) ((1 2 3) cons) (97 97 99 97) (98 100) ((\"A\" \"b\" \"C\") ((\"A\" \"C\") (\"b\" \"C\") (\"A\" \"a\") (\"b\" \"a\") (\"A\" \"b\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_seq_ext_set_equal_p_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU seq-set-equal-p is two seq-every-p passes through seq-contains-p.
    // It ignores order and duplicates, returns canonical t/nil, and uses the
    // optional equality predicate only for containment checks.
    let form = r#"(progn
  (require 'seq)
  (list
    ;; Order and duplicate count are irrelevant.
    (seq-set-equal-p '(1 2 2) '(2 1))
    ;; Missing elements make the sets unequal.
    (seq-set-equal-p '(1 2) '(1 2 3))
    ;; Vectors and strings are valid sequence inputs.
    (seq-set-equal-p [1 2] [2 1])
    (seq-set-equal-p "aba" "ba")
    ;; Custom test function may return any non-nil value; final result is t.
    (seq-set-equal-p '("A" "b") '("a" "B")
                     (lambda (a b)
                       (and (string= (downcase a) (downcase b)) 'matched)))
    ;; Failed equality short-circuits through seq-every-p.
    (let ((calls nil))
      (list
        (seq-set-equal-p '(a b) '(b c)
                         (lambda (a b)
                           (push (list a b) calls)
                           (eq a b)))
        calls))))"#;
    let expect = expect_test::expect![[r#""OK (t nil t t t (nil ((c a) (b a))))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-subseq with negative indices and all types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_ext_subseq_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // seq-subseq: comprehensive tests with lists, vectors, strings,
    // negative indices, boundary cases.
    let form = r#"(list
  ;; List: normal range
  (seq-subseq '(a b c d e f g) 2 5)
  ;; List: from start
  (seq-subseq '(a b c d e) 0 3)
  ;; List: to end
  (seq-subseq '(a b c d e) 3)
  ;; List: negative start (from end)
  (seq-subseq '(a b c d e f) -3)
  ;; List: negative start and end
  (seq-subseq '(a b c d e f) -4 -1)
  ;; Vector
  (seq-subseq [1 2 3 4 5 6 7 8 9 10] 3 7)
  ;; Vector: negative
  (seq-subseq [1 2 3 4 5] -2)
  ;; String
  (seq-subseq "hello world" 6)
  (seq-subseq "hello world" 0 5)
  (seq-subseq "abcdef" -3 -1)
  ;; Empty result
  (seq-subseq '(1 2 3) 1 1)
  ;; Full copy
  (seq-subseq '(1 2 3) 0)
  ;; Single element
  (seq-subseq '(a b c d e) 2 3))"#;
    let expect = expect_test::expect![[
        r#""OK ((c d e) (a b c) (d e) (d e f) (c d e) [4 5 6 7] [4 5] \"world\" \"hello\" \"de\" nil (1 2 3) (c))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_seq_ext_subseq_bounds_error_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU seq-subseq delegates strings/vectors to substring, but handles lists
    // itself.  That means list bounds produce plain errors with exact messages,
    // while string/vector bounds produce args-out-of-range.
    let form = r#"(progn
  (require 'seq)
  (list
    ;; List start bounds.
    (condition-case err
        (seq-subseq '(a b c) 4)
      (error (list (car err) (cadr err))))
    (condition-case err
        (seq-subseq '(a b c) -4)
      (error (list (car err) (cadr err))))
    ;; List end bounds.
    (condition-case err
        (seq-subseq '(a b c) 1 5)
      (error (list (car err) (cadr err))))
    (condition-case err
        (seq-subseq '(a b c) 2 1)
      (error (list (car err) (cadr err))))
    ;; Vector/string route through substring and therefore signal args-out-of-range.
    (condition-case err
        (seq-subseq [a b c] 4)
      (error (list (car err) (cadr err))))
    (condition-case err
        (seq-subseq "abc" -4)
      (error (list (car err) (cadr err))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((error \"Start index out of bounds: 4\") (error \"Start index out of bounds: -4\") (error \"End index out of bounds: 5\") (error \"End index out of bounds: 1\") (args-out-of-range [a b c]) (args-out-of-range \"abc\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: multi-step data pipeline with seq operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_ext_pipeline_word_frequency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Word frequency analysis pipeline using seq operations.
    // 1. Split text into words, 2. group by word, 3. count per group,
    // 4. sort by frequency descending, 5. extract top-N, 6. compute stats.
    let form = r#"(let* ((words '("the" "cat" "sat" "on" "the" "mat"
                          "the" "cat" "on" "the" "mat" "sat"
                          "on" "the" "dog" "sat" "the" "cat"))
        ;; Step 1: Group by identity (word)
        (grouped (seq-group-by #'identity words))
        ;; Step 2: Build (word . count) alist
        (freqs (seq-map (lambda (g) (cons (car g) (length (cdr g))))
                        grouped))
        ;; Step 3: Sort by frequency descending
        (sorted (seq-sort-by #'cdr (lambda (a b) (> a b)) freqs))
        ;; Step 4: Top 3 most frequent
        (top3 (seq-subseq sorted 0 (min 3 (length sorted))))
        ;; Step 5: Total words
        (total (seq-reduce (lambda (acc pair) (+ acc (cdr pair))) freqs 0))
        ;; Step 6: Unique count
        (unique-count (length freqs))
        ;; Step 7: Words appearing more than twice
        (frequent (seq-filter (lambda (pair) (> (cdr pair) 2)) freqs))
        ;; Step 8: Words appearing exactly once
        (hapax (seq-filter (lambda (pair) (= (cdr pair) 1)) freqs)))
  (list
    ;; Top 3 words
    (seq-map #'car top3)
    ;; Their counts
    (seq-map #'cdr top3)
    ;; Total word count
    total
    ;; Unique word count
    unique-count
    ;; Frequent words (sorted)
    (seq-sort-by #'car (lambda (a b) (string-lessp (symbol-name a) (symbol-name b)))
                 frequent)
    ;; Hapax legomena (words appearing once)
    (seq-map #'car hapax)))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbolp \"sat\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: seq-reduce building complex structures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_ext_reduce_complex_accumulations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use seq-reduce to build complex structures from sequences:
    // running stats, nested transformations, transposing.
    let form = r#"(let* ((data '(4 7 2 9 1 8 3 6 5 10))
        ;; Running min, max, sum, count
        (stats (seq-reduce
                (lambda (acc x)
                  (list (min (nth 0 acc) x)
                        (max (nth 1 acc) x)
                        (+ (nth 2 acc) x)
                        (1+ (nth 3 acc))))
                (cdr data)
                (list (car data) (car data) (car data) 1)))
        ;; Build histogram: count occurrences of (x mod 3)
        (histogram
          (let ((h (seq-reduce
                    (lambda (acc x)
                      (let* ((key (% x 3))
                             (existing (assq key acc)))
                        (if existing
                            (progn (setcdr existing (1+ (cdr existing))) acc)
                          (cons (cons key 1) acc))))
                    data nil)))
            (sort h (lambda (a b) (< (car a) (car b))))))
        ;; Partition into runs of ascending values
        (runs (let ((result (seq-reduce
                             (lambda (acc x)
                               (let ((current-run (car acc))
                                     (finished (cdr acc)))
                                 (if (or (null current-run)
                                         (>= x (car (last current-run))))
                                     (cons (append current-run (list x)) finished)
                                   (cons (list x) (cons current-run finished)))))
                             data
                             '(nil))))
                (nreverse (cons (car result) (cdr result)))))
        ;; Zip two sequences using seq-mapn then reduce to alist
        (pairs (seq-mapn #'cons '(a b c d) '(1 2 3 4))))
  (list
    ;; Stats: (min max sum count)
    stats
    ;; Mean (integer division)
    (/ (nth 2 stats) (nth 3 stats))
    ;; Histogram of x mod 3
    histogram
    ;; Ascending runs
    runs
    ;; Zipped pairs
    pairs))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 10 55 10) 5 ((0 . 3) (1 . 4) (2 . 3)) ((4 7) (2 9) (1 8) (3 6) (5 10)) ((a . 1) (b . 2) (c . 3) (d . 4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

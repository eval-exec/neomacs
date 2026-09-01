//! Oracle parity tests for comprehensive list manipulation:
//! `nconc` edge cases, `nreverse` vs `reverse`, `append` with various arg
//! counts, `last`/`butlast`/`nbutlast` with N parameter, `take`,
//! `member` vs `memq`, `delete` vs `delq`, and `sort` with complex
//! comparison functions.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// nconc with 0, 1, 2, 3+ lists, nil at various positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nconc_comprehensive_arg_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                    ;; Zero args
                    (nconc)
                    ;; One arg
                    (nconc (list 1 2 3))
                    (nconc nil)
                    ;; Two args — both lists
                    (nconc (list 'a 'b) (list 'c 'd))
                    ;; Two args — first nil
                    (nconc nil (list 10 20))
                    ;; Two args — second nil
                    (nconc (list 10 20) nil)
                    ;; Two args — both nil
                    (nconc nil nil)
                    ;; Three args
                    (nconc (list 1) (list 2) (list 3))
                    ;; Three args with nils scattered
                    (nconc nil (list 'a) nil)
                    (nconc (list 'x) nil (list 'y))
                    (nconc nil nil (list 'z))
                    ;; Four args with all-nil prefix
                    (nconc nil nil nil (list 42))
                    ;; Five args
                    (nconc (list 1) (list 2) (list 3) (list 4) (list 5))
                    ;; Many nils
                    (nconc nil nil nil nil nil)
                    ;; Non-list final arg (dotted pair result)
                    (nconc (list 1 2) 3)
                    ;; Non-list final arg after nils
                    (nconc nil nil 'terminal)
                    ;; Single element lists
                    (nconc (list 'a) (list 'b) (list 'c) (list 'd))
                    ;; nconc is destructive — verify via fresh lists
                    (let ((a (list 1 2))
                          (b (list 3 4))
                          (c (list 5 6)))
                      (let ((result (nconc a b c)))
                        (list result
                              ;; a's cdr chain now extends through b and c
                              (length a)
                              (nthcdr 1 a)))))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (1 2 3) nil (a b c d) (10 20) (10 20) nil (1 2 3) (a) (x y) (z) (42) (1 2 3 4 5) nil (1 2 . 3) terminal (a b c d) ((1 2 3 4 5 6) 6 (2 3 4 5 6)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// nreverse vs reverse — destructive vs non-destructive semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nreverse_vs_reverse_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                    ;; Basic reverse
                    (reverse '(1 2 3 4 5))
                    (reverse nil)
                    (reverse '(solo))
                    (reverse '(a b))
                    ;; reverse preserves original
                    (let ((orig (list 10 20 30)))
                      (let ((rev (reverse orig)))
                        (list orig rev (eq orig rev))))
                    ;; nreverse on fresh list
                    (nreverse (list 1 2 3 4 5))
                    (nreverse nil)
                    (nreverse (list 'only))
                    ;; nreverse is destructive — original structure modified
                    (let ((orig (list 'a 'b 'c 'd)))
                      (let ((result (nreverse orig)))
                        ;; orig now points to the old first cons, which is the new last
                        (list result
                              ;; car of what orig points to is still 'a
                              (car orig)
                              ;; but cdr is now nil (it's the tail)
                              (cdr orig))))
                    ;; reverse on strings
                    (reverse "hello")
                    (reverse "")
                    (reverse "a")
                    ;; reverse on vectors
                    (reverse [1 2 3 4])
                    (reverse [])
                    ;; Nested list — only top level reversed
                    (reverse '((1 2) (3 4) (5 6)))
                    ;; Bool vectors
                    (reverse (bool-vector t nil t nil t)))"#;
    let expect = expect_test::expect![[
        r#""OK ((5 4 3 2 1) nil (solo) (b a) ((10 20 30) (30 20 10) nil) (5 4 3 2 1) nil (only) ((d c b a) a nil) \"olleh\" \"\" \"a\" [4 3 2 1] [] ((5 6) (3 4) (1 2)) #&5\"\u{15}\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_reverse_nreverse_sequence_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Fnreverse mutates vectors and bool-vectors in place,
    // delegates strings to Freverse, and reports invalid objects as `arrayp'.
    // GNU src/fns.c:Freverse reports invalid objects as `sequencep'.
    let form = r#"
(let* ((s (propertize "abé" 'face 'bold))
       (s-rev (reverse s))
       (s-nrev (nreverse s))
       (v [a b c])
       (v-result (nreverse v))
       (bits (bool-vector t nil nil t))
       (bits-result (nreverse bits)))
  (list
   (eq s-rev s)
   s-rev
   (text-properties-at 0 s-rev)
   (eq s-nrev s)
   s-nrev
   (text-properties-at 0 s-nrev)
   (eq v-result v)
   v
   (eq bits-result bits)
   (list (aref bits 0) (aref bits 1) (aref bits 2) (aref bits 3))
   (condition-case err
       (nreverse 42)
     (error (list (car err) (cdr err))))
   (condition-case err
       (reverse 42)
     (error (list (car err) (cdr err))))
   (condition-case err
       (nreverse (record 'neovm--test-record 'a 'b))
     (error (list (car err) (cdr err))))
   (condition-case err
       (reverse (record 'neovm--test-record 'a 'b))
     (error (list (car err) (cdr err))))))
"#;
    let expect = expect_test::expect![[
        r#""OK (nil \"éba\" nil nil \"éba\" nil t [c b a] t (t nil nil t) (wrong-type-argument (arrayp 42)) (wrong-type-argument (sequencep 42)) (wrong-type-argument (arrayp #s(neovm--test-record a b))) (wrong-type-argument (sequencep #s(neovm--test-record a b))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// append with various arg counts and nil handling
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_append_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                    ;; Zero args
                    (append)
                    ;; One arg — returns it (not copied for non-list final)
                    (append '(1 2 3))
                    (append nil)
                    ;; Two args
                    (append '(a b) '(c d))
                    (append nil '(x y))
                    (append '(x y) nil)
                    ;; Three args
                    (append '(1) '(2) '(3))
                    (append nil nil '(z))
                    (append '(a) nil '(b))
                    ;; Many args
                    (append '(1) '(2) '(3) '(4) '(5) '(6))
                    ;; Non-list final arg (creates dotted list)
                    (append '(a b) 'c)
                    (append nil 42)
                    ;; append vs nconc: append copies all but last
                    (let ((x (list 1 2))
                          (y (list 3 4)))
                      (let ((result (append x y)))
                        (list result
                              ;; x is NOT modified
                              x
                              ;; first cons of result is a copy, not eq to x's first
                              (eq (car x) (car result))
                              ;; but last arg (y) shares structure
                              (eq y (nthcdr 2 result)))))
                    ;; append with strings (converts to list of chars)
                    (append "abc" nil)
                    (append "hi" '(33))
                    ;; append with vectors
                    (append [1 2 3] nil)
                    (append [4 5] '(6 7))
                    ;; Mixed: list + vector + string
                    (append '(a) [b] "c" nil))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (1 2 3) nil (a b c d) (x y) (x y) (1 2 3) (z) (a b) (1 2 3 4 5 6) (a b . c) 42 ((1 2 3 4) (1 2) t t) (97 98 99) (104 105 33) (1 2 3) (4 5 6 7) (a b 99))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_append_bool_vector_and_final_tail_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Fappend delegates to concat_to_list.  That path accepts
    // bool-vectors as non-final sequences, but always uses the final argument
    // as an unaltered tail, even when that tail is itself a bool-vector.
    let form = r#"
(let ((tail (bool-vector t nil)))
  (list
   (append (bool-vector t nil t) nil)
   (let ((result (append nil tail)))
     (list (eq result tail) result))
   (let ((result (append nil nil tail)))
     (list (eq result tail) result))
   (append [a b] tail)
   (append (bool-vector t nil) 'tail)
   (condition-case err
       (append 42 nil)
     (error (list (car err) (cdr err))))
   (condition-case err
       (append (cons 'a 'bad-tail) nil)
     (error (list (car err) (cdr err))))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((t nil t) (t #&2\"\u{1}\") (t #&2\"\u{1}\") (a b . #&2\"\u{1}\") (t nil . tail) (wrong-type-argument (sequencep 42)) (wrong-type-argument (listp bad-tail)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// last with N parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_last_with_n_parameter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((lst '(a b c d e f)))
                    (list
                     ;; last with no N (default: returns last cons)
                     (last lst)
                     (last nil)
                     (last '(solo))
                     ;; last with N=1 (same as no arg)
                     (last lst 1)
                     ;; last with N=0 (returns nil — no tail elements)
                     (last lst 0)
                     ;; last with N=2 (last two conses)
                     (last lst 2)
                     ;; last with N=3
                     (last lst 3)
                     ;; last with N >= length (returns whole list)
                     (last lst 6)
                     (last lst 7)
                     (last lst 100)
                     ;; last on short lists
                     (last '(x) 0)
                     (last '(x) 1)
                     (last '(x) 2)
                     (last '(x y) 1)
                     (last '(x y) 2)
                     (last '(x y) 3)
                     ;; last returns a tail — eq to nthcdr
                     (let ((l (list 1 2 3 4 5)))
                       (eq (last l 3) (nthcdr 2 l)))
                     ;; last on dotted list
                     (last '(a b . c))
                     (last '(a b . c) 2)))"#;
    let expect = expect_test::expect![[
        r#""OK ((f) nil (solo) (f) nil (e f) (d e f) (a b c d e f) (a b c d e f) (a b c d e f) nil (x) (x) (y) (x y) (x y) t (b . c) (a b . c))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// butlast and nbutlast with N parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_butlast_nbutlast_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                    ;; butlast default (N=1)
                    (butlast '(a b c d e))
                    (butlast '(solo))
                    (butlast nil)
                    ;; butlast with explicit N
                    (butlast '(1 2 3 4 5) 0)
                    (butlast '(1 2 3 4 5) 1)
                    (butlast '(1 2 3 4 5) 2)
                    (butlast '(1 2 3 4 5) 3)
                    (butlast '(1 2 3 4 5) 4)
                    (butlast '(1 2 3 4 5) 5)
                    (butlast '(1 2 3 4 5) 6)
                    (butlast '(1 2 3 4 5) 100)
                    ;; butlast is non-destructive
                    (let ((orig (list 10 20 30 40)))
                      (let ((bl (butlast orig 2)))
                        (list orig bl (length orig))))
                    ;; nbutlast is destructive
                    (let ((orig (list 'a 'b 'c 'd 'e)))
                      (let ((result (nbutlast orig 2)))
                        ;; result is the modified original
                        (list result
                              (eq orig result)
                              (length result))))
                    ;; nbutlast edge cases
                    (nbutlast (list 1) 0)
                    (nbutlast (list 1) 1)
                    (nbutlast (list 1 2) 1)
                    (nbutlast (list 1 2) 2)
                    (nbutlast (list 1 2) 3)
                    (nbutlast nil)
                    ;; Combining butlast and last should reconstruct
                    (let ((lst '(1 2 3 4 5)))
                      (equal lst (append (butlast lst 2) (last lst 2)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((a b c d) nil nil (1 2 3 4 5) (1 2 3 4) (1 2 3) (1 2) (1) nil nil nil ((10 20 30 40) (10 20) 4) ((a b c) t 3) (1) nil (1) nil nil nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// take with various N values including edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_take_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                    ;; Basic take
                    (take 3 '(a b c d e))
                    (take 0 '(a b c d e))
                    (take 1 '(a b c d e))
                    (take 5 '(a b c d e))
                    ;; N exceeds length
                    (take 10 '(1 2 3))
                    (take 100 nil)
                    ;; take 0 of anything
                    (take 0 nil)
                    (take 0 '(x))
                    ;; Negative N
                    (take -1 '(a b c))
                    (take -5 '(1 2 3))
                    ;; take from nil
                    (take 3 nil)
                    ;; take returns a fresh list (not eq)
                    (let ((orig (list 1 2 3 4 5)))
                      (let ((taken (take 3 orig)))
                        (list taken
                              (eq orig taken)
                              ;; original unchanged
                              orig)))
                    ;; take + nthcdr should reconstruct
                    (let ((lst '(a b c d e f)))
                      (equal lst (append (take 3 lst) (nthcdr 3 lst))))
                    ;; take on dotted list
                    (take 2 '(a b . c))
                    (take 3 '(a b . c)))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp c)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// member vs memq with custom test, various types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_member_memq_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                    ;; memq with symbols (eq comparison)
                    (memq 'c '(a b c d e))
                    (memq 'z '(a b c d e))
                    (memq nil '(a nil b))
                    (memq t '(nil t nil))
                    ;; memq with fixnums (eq works for small ints)
                    (memq 3 '(1 2 3 4 5))
                    (memq 99 '(1 2 3))
                    ;; memq does NOT match strings (eq, not equal)
                    (memq "hello" '("hello" "world"))
                    ;; member uses equal — matches strings
                    (member "hello" '("hello" "world"))
                    (member "missing" '("hello" "world"))
                    ;; member matches list values
                    (member '(1 2) '((0 1) (1 2) (2 3)))
                    ;; memq does not match list copies
                    (memq '(1 2) '((0 1) (1 2) (2 3)))
                    ;; member returns the tail starting at match
                    (member 3 '(1 2 3 4 5))
                    (memq 'b '(a b c d))
                    ;; member on nil list
                    (member 'x nil)
                    (memq 'x nil)
                    ;; member with vector values
                    (member [1 2] '([0 1] [1 2] [2 3]))
                    ;; Nested use: is element in multiple lists?
                    (let ((lists '((1 2 3) (4 5 6) (7 8 9) (3 6 9))))
                      (mapcar (lambda (lst) (if (memq 3 lst) t nil))
                              lists))
                    ;; member for deduplication pattern
                    (let ((input '(a b c b a d c e a))
                          (result nil))
                      (dolist (x input)
                        (unless (memq x result)
                          (setq result (cons x result))))
                      (nreverse result)))"#;
    let expect = expect_test::expect![[
        r#""OK ((c d e) nil (nil b) (t nil) (3 4 5) nil nil (\"hello\" \"world\") nil ((1 2) (2 3)) nil (3 4 5) (b c d) nil nil ([1 2] [2 3]) (t nil nil t) (a b c d e))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete vs delq — destructive removal semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_delq_destructive_behavior() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                    ;; delq basic — removes by eq
                    (delq 'b (list 'a 'b 'c 'b 'd))
                    (delq 'z (list 'a 'b 'c))
                    (delq nil (list 'a nil 'b nil 'c))
                    ;; delq with numbers (eq works for fixnums)
                    (delq 3 (list 1 2 3 4 3 5))
                    ;; delq does not match string copies
                    (delq "hello" (list "hello" "world"))
                    ;; delete uses equal — matches strings
                    (delete "hello" (list "hello" "world" "hello"))
                    ;; delete with list elements
                    (delete '(1 2) (list '(0 1) '(1 2) '(2 3) '(1 2)))
                    ;; Removing from head — delq/delete may return different pointer
                    (let ((lst (list 1 2 3 4)))
                      (let ((result (delq 1 lst)))
                        ;; result may not be eq to lst if head removed
                        (list result)))
                    ;; Removing all elements
                    (delq 'x (list 'x 'x 'x))
                    (delete 1 (list 1 1 1 1))
                    ;; Empty list
                    (delq 'a nil)
                    (delete 'a nil)
                    ;; Single element — match
                    (delq 42 (list 42))
                    ;; Single element — no match
                    (delq 42 (list 99))
                    ;; Removing from middle doesn't affect head pointer
                    (let ((lst (list 'a 'b 'c 'd)))
                      (let ((result (delq 'b lst)))
                        (list result (eq lst result))))
                    ;; delete on vectors (works in Emacs 28+)
                    (delete 3 (vector 1 2 3 4 3 5))
                    ;; delete on strings
                    (delete ?a "abacada"))"#;
    let expect = expect_test::expect![[
        r#""OK ((a c d) (a b c) (a b c) (1 2 4 5) (\"hello\" \"world\") (\"world\") ((0 1) (2 3)) ((2 3 4)) nil nil nil nil nil (99) ((a c d) t) [1 2 4 5] \"bcd\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_delete_sequence_identity_and_string_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Fdelete has separate list, vector, and string branches.
    // For strings, non-character ELTs and absent characters return the
    // original string object.  Character deletion builds a fresh string and
    // does not copy text properties.
    let form = r#"
(let* ((s (propertize "abac" 'face 'bold 'tag 'source))
       (no-char (delete 'a s))
       (absent (delete ?z s))
       (deleted (delete ?a s))
       (v [1 2 3])
       (v-absent (delete 9 v))
       (v-deleted (delete 2 v)))
  (list
   (eq no-char s)
   no-char
   (text-properties-at 0 no-char)
   (eq absent s)
   absent
   (text-properties-at 0 absent)
   (eq deleted s)
   deleted
   (text-properties-at 0 deleted)
   (text-properties-at 1 deleted)
   (eq v-absent v)
   v-absent
   (eq v-deleted v)
   v-deleted
   (condition-case err
       (delete 'x 42)
     (error (list (car err) (cdr err))))))
"#;
    let expect = expect_test::expect![[
        r#""OK (t #(\"abac\" 0 4 (face bold tag source)) (face bold tag source) t #(\"abac\" 0 4 (face bold tag source)) (face bold tag source) nil \"bc\" nil nil t [1 2 3] nil [1 3] (wrong-type-argument (sequencep 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// sort with complex comparison functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sort_complex_comparators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                    ;; Sort by absolute value
                    (sort (list 3 -1 4 -1 5 -9 2 -6)
                          (lambda (a b) (< (abs a) (abs b))))
                    ;; Sort strings by length, then alphabetically
                    (sort (list "fig" "apple" "banana" "kiwi" "date" "elderberry")
                          (lambda (a b)
                            (or (< (length a) (length b))
                                (and (= (length a) (length b))
                                     (string< a b)))))
                    ;; Sort alist by value descending
                    (let ((al (list (cons 'a 30) (cons 'b 10) (cons 'c 50)
                                    (cons 'd 20) (cons 'e 40))))
                      (sort al (lambda (x y) (> (cdr x) (cdr y)))))
                    ;; Sort nested lists by second element
                    (sort (list '(alice 88) '(bob 95) '(carol 72) '(dave 88) '(eve 100))
                          (lambda (a b)
                            (or (> (cadr a) (cadr b))
                                (and (= (cadr a) (cadr b))
                                     (string< (symbol-name (car a))
                                              (symbol-name (car b)))))))
                    ;; Sort with :key parameter
                    (sort (list '(3 "c") '(1 "a") '(2 "b") '(5 "e") '(4 "d"))
                          :key #'car :lessp #'<)
                    ;; Sort with :reverse
                    (sort (list 5 3 1 4 2) :lessp #'< :reverse t)
                    ;; Sort preserves equal elements order (stability check)
                    ;; Items with same key should maintain relative order
                    (let ((data (list (cons 1 'a) (cons 2 'b) (cons 1 'c)
                                      (cons 3 'd) (cons 2 'e) (cons 1 'f))))
                      (sort data (lambda (x y) (< (car x) (car y)))))
                    ;; Sort by multiple criteria using :key
                    (sort (list "banana" "Apple" "cherry" "avocado" "Blueberry")
                          :key #'downcase :lessp #'string<))"#;
    let expect = expect_test::expect![[
        r#""OK ((-1 -1 2 3 4 5 -6 -9) (\"fig\" \"date\" \"kiwi\" \"apple\" \"banana\" \"elderberry\") ((c . 50) (e . 40) (a . 30) (d . 20) (b . 10)) ((eve 100) (bob 95) (alice 88) (dave 88) (carol 72)) ((1 \"a\") (2 \"b\") (3 \"c\") (4 \"d\") (5 \"e\")) (5 4 3 2 1) ((1 . a) (1 . c) (1 . f) (2 . b) (2 . e) (3 . d)) (\"Apple\" \"avocado\" \"banana\" \"Blueberry\" \"cherry\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

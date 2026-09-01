//! Divergence tests: subr + sequence + mapping + record deep combo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_sequence_map_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((2 3 4 5 6) t (1 4 9 16) t (a b c) \"hello world\" t \"1-2-3\" t (11 21 31) nil (\"a\" \"b\" \"c\") t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (mapcar '1+ '(1 2 3 4 5))
        (equal (mapcar '1+ '(1 2 3 4 5)) '(2 3 4 5 6))
        (mapcar (lambda (x) (* x x)) '(1 2 3 4))
        (equal (mapcar (lambda (x) (* x x)) '(1 2 3 4)) '(1 4 9 16))
        (mapc (lambda (x) nil) '(a b c))
        (mapconcat 'symbol-name '(hello world) " ")
        (string= (mapconcat 'symbol-name '(hello world) " ") "hello world")
        (mapconcat 'number-to-string '(1 2 3) "-")
        (string= (mapconcat 'number-to-string '(1 2 3) "-") "1-2-3")
        (seq-map #'1+ [10 20 30])
        (equal (seq-map #'1+ [10 20 30]) [11 21 31])
        (seq-map #'symbol-name '(a b c))
        (equal (seq-map #'symbol-name '(a b c)) '("a" "b" "c")))) "#,
        expect,
    );
}

#[test]
fn divergence_sequence_filter_reduce() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-evenp)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (seq-filter #'cl-evenp '(1 2 3 4 5 6))
        (equal (seq-filter #'cl-evenp '(1 2 3 4 5 6)) '(2 4 6))
        (seq-filter #'symbolp '(1 a 2 b 3 c))
        (equal (seq-filter #'symbolp '(1 a 2 b 3 c)) '(a b c))
        (seq-remove #'cl-evenp '(1 2 3 4 5 6))
        (equal (seq-remove #'cl-evenp '(1 2 3 4 5 6)) '(1 3 5))
        (seq-reduce #'+ '(1 2 3 4 5) 0)
        (= (seq-reduce #'+ '(1 2 3 4 5) 0) 15)
        (seq-reduce #'* '(1 2 3 4 5) 1)
        (= (seq-reduce #'* '(1 2 3 4 5) 1) 120)
        (seq-group-by #'cl-evenp '(1 2 3 4 5))
        (equal (seq-group-by #'cl-evenp '(1 2 3 4 5))
               '((nil 1 3 5) (t 2 4))))) "#,
        expect,
    );
}

#[test]
fn divergence_sequence_sort_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((1 1 2 3 4 5 6 9) t (\"apple\" \"banana\" \"cherry\") t (1 2 3) t (1 2 3 4 5) t (3 4) t (1 2) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (seq-sort #'< '(3 1 4 1 5 9 2 6))
        (equal (seq-sort #'< '(3 1 4 1 5 9 2 6)) '(1 1 2 3 4 5 6 9))
        (seq-sort #'string< '("banana" "apple" "cherry"))
        (equal (seq-sort #'string< '("banana" "apple" "cherry"))
               '("apple" "banana" "cherry"))
        (seq-uniq '(1 2 3 2 1 3 2))
        (equal (seq-uniq '(1 2 3 2 1 3 2)) '(1 2 3))
        (seq-union '(1 2 3) '(3 4 5))
        (equal (seq-union '(1 2 3) '(3 4 5)) '(1 2 3 4 5))
        (seq-intersection '(1 2 3 4) '(3 4 5 6))
        (equal (seq-intersection '(1 2 3 4) '(3 4 5 6)) '(3 4))
        (seq-difference '(1 2 3 4) '(3 4 5 6))
        (equal (seq-difference '(1 2 3 4) '(3 4 5 6)) '(1 2)))) "#,
        expect,
    );
}

#[test]
fn divergence_record_type_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct (test-rto-xxx (:type list) :named)
    x y z)
  (let ((r (test-rto-xxx :x 10 :y 20 :z 30)))
    (list (test-rto-xxx-p r)
          (eq (test-rto-xxx-p r) t)
          (test-rto-xxx-x r)
          (= (test-rto-xxx-x r) 10)
          (test-rto-xxx-y r)
          (= (test-rto-xxx-y r) 20)
          (test-rto-xxx-z r)
          (= (test-rto-xxx-z r) 30)
          (setf (test-rto-xxx-x r) 99)
          (= (test-rto-xxx-x r) 99)
          (= (test-rto-xxx-y r) 20)))) "#,
        expect,
    );
}

#[test]
fn divergence_subr_string_manipulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t t t t t nil t t t t \"hello\" t \"hello\" t \"hello\" t \"hi   \" t \"hello\" t \"hello emacs\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (string= "hello" "hello")
        (not (string= "hello" "Hello"))
        (string-equal "hello" "hello")
        (string< "apple" "banana")
        (not (string< "banana" "apple"))
        (string-version-lessp "file1" "file10")
        (string-version-lessp "file2" "file10")
        (not (string-version-lessp "file2" "file10"))
        (string-prefix-p "hel" "hello")
        (not (string-prefix-p "hel" "HELLO"))
        (string-suffix-p "llo" "hello")
        (not (string-suffix-p "llo" "HELLO"))
        (string-trim "  hello  ")
        (string= (string-trim "  hello  ") "hello")
        (string-trim-left "xxxhello" "x+")
        (string= (string-trim-left "xxxhello" "x+") "hello")
        (string-trim-right "helloxxx" "x+")
        (string= (string-trim-right "helloxxx" "x+") "hello")
        (string-pad "hi" 5)
        (string= (string-pad "hi" 5) "hi   ")
        (string-chop-newline "hello\n")
        (string= (string-chop-newline "hello\n") "hello")
        (string-replace "world" "emacs" "hello world")
        (string= (string-replace "world" "emacs" "hello world") "hello emacs"))) "#,
        expect,
    );
}

#[test]
fn divergence_sequence_search_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (3 t t 2 t nil t 1 t (2 3 4 5) t (2 3) t (3 4 5) t c t 20 t 3 t 4 t (1 2 3) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (seq-contains '(1 2 3 4 5) 3)
        (= (seq-contains '(1 2 3 4 5) 3) 3)
        (not (seq-contains '(1 2 3 4 5) 6))
        (seq-position '(a b c d e) 'c)
        (= (seq-position '(a b c d e) 'c) 2)
        (seq-position '(a b c d e) 'z)
        (null (seq-position '(a b c d e) 'z))
        (seq-first '(1 2 3 4 5))
        (= (seq-first '(1 2 3 4 5)) 1)
        (seq-rest '(1 2 3 4 5))
        (equal (seq-rest '(1 2 3 4 5)) '(2 3 4 5))
        (seq-subseq '(1 2 3 4 5) 1 3)
        (equal (seq-subseq '(1 2 3 4 5) 1 3) '(2 3))
        (seq-subseq '(1 2 3 4 5) 2)
        (equal (seq-subseq '(1 2 3 4 5) 2) '(3 4 5))
        (seq-elt '(a b c d) 2)
        (eq (seq-elt '(a b c d) 2) 'c)
        (seq-elt [10 20 30 40] 1)
        (= (seq-elt [10 20 30 40] 1) 20)
        (seq-length '(1 2 3))
        (= (seq-length '(1 2 3)) 3)
        (seq-length [1 2 3 4])
        (= (seq-length [1 2 3 4]) 4)
        (seq-copy '(1 2 3))
        (equal (seq-copy '(1 2 3)) '(1 2 3)))) "#,
        expect,
    );
}

#[test]
fn divergence_subr_list_operations_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (1 t (2 3 4 5) t 1 t (2 3 4 5) t 1 t 3 t 5 t (3 4 5) t (5) t (1 2 3) t (1 2 3) t (5 4 3 2 1) t 5 t 5 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((lst '(1 2 3 4 5)))
    (list (car lst) (= (car lst) 1)
          (cdr lst) (equal (cdr lst) '(2 3 4 5))
          (car-safe lst) (= (car-safe lst) 1)
          (cdr-safe lst) (equal (cdr-safe lst) '(2 3 4 5))
          (nth 0 lst) (= (nth 0 lst) 1)
          (nth 2 lst) (= (nth 2 lst) 3)
          (nth 4 lst) (= (nth 4 lst) 5)
          (nthcdr 2 lst) (equal (nthcdr 2 lst) '(3 4 5))
          (last lst) (equal (last lst) '(5))
          (butlast lst 2) (equal (butlast lst 2) '(1 2 3))
          (nbutlast (copy-sequence lst) 2) (equal (nbutlast (copy-sequence lst) 2) '(1 2 3))
          (reverse lst) (equal (reverse lst) '(5 4 3 2 1))
          (length lst) (= (length lst) 5)
          (safe-length lst) (= (safe-length lst) 5)))) "#,
        expect,
    );
}

#[test]
fn divergence_subr_alist_plist_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function acons)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((al '((a . 1) (b . 2) (c . 3))))
    (list (assoc 'a al)
          (equal (assoc 'a al) '(a . 1))
          (cdr (assoc 'b al))
          (= (cdr (assoc 'b al)) 2)
          (assq 'c al)
          (equal (assq 'c al) '(c . 3))
          (rassoc 2 al)
          (equal (rassoc 2 al) '(b . 2))
          (not (assoc 'z al))
          (acons 'd 4 al)
          (equal (cdr (assoc 'd (acons 'd 4 al))) 4)
          (copy-alist al)
          (equal (copy-alist al) al)
          (not (eq (copy-alist al) al))))) "#,
        expect,
    );
}

#[test]
fn divergence_sequence_do_each() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-oddp)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((result nil))
    (seq-do (lambda (x) (push x result)) '(1 2 3 4 5))
    (list (nreverse result)
          (equal (nreverse result) '(1 2 3 4 5))
          (let ((sum 0))
            (seq-do (lambda (x) (setq sum (+ sum x))) '(10 20 30))
            sum)
          (= (let ((sum 0))
               (seq-do (lambda (x) (setq sum (+ sum x))) '(10 20 30))
               sum)
             60)
          (seq-do (lambda (x) nil) [1 2 3])
          (seq-count #'cl-oddp '(1 2 3 4 5))
          (= (seq-count #'cl-oddp '(1 2 3 4 5)) 3)
          (seq-count #'symbolp '(a 1 b 2 c 3))
          (= (seq-count #'symbolp '(a 1 b 2 c 3)) 3)))) "#,
        expect,
    );
}

#[test]
fn divergence_subr_number_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (5 t 1 t 42 t 42 t 1024 t 4.0 t 15 t 7 t 6 t 256 t -1 t 2 t 3 t 4 t 4 t 3 t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (max 1 5 3 2 4) (= (max 1 5 3 2 4) 5)
        (min 1 5 3 2 4) (= (min 1 5 3 2 4) 1)
        (abs -42) (= (abs -42) 42)
        (abs 42) (= (abs 42) 42)
        (expt 2 10) (= (expt 2 10) 1024)
        (sqrt 16) (= (sqrt 16) 4)
        (logand 255 15) (= (logand 255 15) 15)
        (logior 1 2 4) (= (logior 1 2 4) 7)
        (logxor 5 3) (= (logxor 5 3) 6)
        (lsh 1 8) (= (lsh 1 8) 256)
        (ash -1 -1) (= (ash -1 -1) -1)
        (mod 17 5) (= (mod 17 5) 2)
        (floor 3.7) (= (floor 3.7) 3)
        (ceiling 3.2) (= (ceiling 3.2) 4)
        (round 3.5) (= (round 3.5) 4)
        (truncate 3.7) (= (truncate 3.7) 3)
        (integerp (random))
        (>= (random 10) 0)
        (< (random 10) 10))) "#,
        expect,
    );
}

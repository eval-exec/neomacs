//! Divergence tests: list manipulation + sequence + map + assoc combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_dolist_accumulate_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 4 9 16 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((input '(1 2 3 4 5))
        (result nil))
    (dolist (x input (nreverse result))
      (push (* x x) result)))) "#,
        expect,
    );
}

#[test]
fn divergence_mapcar_mapconcat_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((6 15 24) \"6-15-24\" t t 45 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let* ((lists '((1 2 3) (4 5 6) (7 8 9)))
         (sums (mapcar (lambda (lst) (apply '+ lst)) lists))
         (str (mapconcat 'number-to-string sums "-")))
    (list sums str
          (equal sums '(6 15 24))
          (string= str "6-15-24")
          (apply '+ sums)
          (= (apply '+ sums) 45)))) "#,
        expect,
    );
}

#[test]
fn divergence_assoc_rassq_delq() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((b . 2) t (b . 2) t (c . 3) t 1 t 2 4 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((alist '((a . 1) (b . 2) (c . 3) (d . 2))))
    (list (assoc 'b alist)
          (equal (assoc 'b alist) '(b . 2))
          (rassoc 2 alist)
          (equal (rassoc 2 alist) '(b . 2))
          (assq 'c alist)
          (equal (assq 'c alist) '(c . 3))
          (cdr (assoc 'a alist))
          (= (cdr (assoc 'a alist)) 1)
          (assoc-default 'd alist)
          (length alist)
          (= (length alist) 4)))) "#,
        expect,
    );
}

#[test]
fn divergence_nreverse_safe_reverse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t (1 2 3 4 5) t (c b a) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((l1 (list 1 2 3 4 5))
        (l2 (list 'a 'b 'c)))
    (let ((r1 (reverse l1))
          (r2 (reverse l2)))
      (list (equal r1 '(5 4 3 2 1))
            (equal r2 '(c b a))
            l1
            (equal l1 '(1 2 3 4 5))
            (nreverse l2)
            (equal l2 '(c)))))) "#,
        expect,
    );
}

#[test]
fn divergence_sort_stable_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((1 . \"a\") (1 . \"d\") (2 . \"b\") (3 . \"c\") (3 . \"e\")) t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((pairs '((3 . "c") (1 . "a") (2 . "b") (1 . "d") (3 . "e"))))
    (let ((sorted (sort (copy-alist pairs)
                        (lambda (a b) (< (car a) (car b))))))
      (list sorted
            (= (caar sorted) 1)
            (<= (car (nth 0 sorted)) (car (nth 1 sorted)))
            (<= (car (nth 1 sorted)) (car (nth 2 sorted)))
            (<= (car (nth 2 sorted)) (car (nth 3 sorted))))))) "#,
        expect,
    );
}

#[test]
fn divergence_sequence_map_reduce() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK ((11 21 31 41 51) nil 150 t (30 40 50) nil t t 5 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((v [10 20 30 40 50]))
    (list (seq-map #'1+ v)
          (equal (seq-map #'1+ v) [11 21 31 41 51])
          (seq-reduce #'+ v 0)
          (= (seq-reduce #'+ v 0) 150)
          (seq-filter (lambda (x) (> x 25)) v)
          (equal (seq-filter (lambda (x) (> x 25)) v) [30 40 50])
          (seq-some (lambda (x) (> x 40)) v)
          (seq-every-p (lambda (x) (> x 5)) v)
          (seq-length v)
          (= (seq-length v) 5)))) "#,
        expect,
    );
}

#[test]
fn divergence_list_circular_shared_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t 99 99 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((shared (list 1 2 3)))
    (let ((tree (list shared shared)))
      (list (eq (car tree) (cadr tree))
            (equal (car tree) '(1 2 3))
            (eq (car tree) (cadr tree))
            (setcar (car tree) 99)
            (car (cadr tree))
            (= (car (cadr tree)) 99)
            (= (caar tree) 99))))) "#,
        expect,
    );
}

#[test]
fn divergence_plist_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (1 t 2 t nil t (:c 3 :d 4) (:a 1 :b 2 :c 3 :d 4) 4 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((pl '(:a 1 :b 2 :c 3)))
    (list (plist-get pl :a)
          (= (plist-get pl :a) 1)
          (plist-get pl :b)
          (= (plist-get pl :b) 2)
          (plist-get pl :d)
          (null (plist-get pl :d))
          (plist-member pl :c)
          (plist-put pl :d 4)
          (plist-get (plist-put pl :d 4) :d)
          (= (plist-get (plist-put pl :d 4) :d) 4)))) "#,
        expect,
    );
}

#[test]
fn divergence_push_pop_nthcdr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 2 1 nil t t t t (c d e) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((stack nil))
    (push 1 stack)
    (push 2 stack)
    (push 3 stack)
    (let ((p1 (pop stack))
          (p2 (pop stack))
          (p3 (pop stack)))
      (list p1 p2 p3 stack
            (= p1 3) (= p2 2) (= p3 1)
            (null stack)
            (nthcdr 2 '(a b c d e))
            (equal (nthcdr 2 '(a b c d e)) '(c d e)))))) "#,
        expect,
    );
}

#[test]
fn divergence_number_sequence_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8 9 10) t t t 55 t (1 4 9 16 25 36 49 64 81 100) t (0 0.5 1.0) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((seq (number-sequence 1 10)))
    (list seq
          (= (length seq) 10)
          (= (car seq) 1)
          (= (car (last seq)) 10)
          (apply '+ seq)
          (= (apply '+ seq) 55)
          (mapcar (lambda (x) (* x x)) seq)
          (equal (mapcar (lambda (x) (* x x)) '(1 2 3))
                 '(1 4 9))
          (number-sequence 0 1 0.5)
          (equal (number-sequence 0 1 0.5) '(0.0 0.5 1.0))))) "#,
        expect,
    );
}

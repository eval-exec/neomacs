//! Divergence tests: cl-seq + cl-loop + sort + reverse + mapcar deep combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_cl_sort_stable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-sort)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let* ((pairs '((3 . "c") (1 . "a") (2 . "b") (1 . "d") (3 . "e") (2 . "f")))
         (sorted (cl-sort (copy-sequence pairs) #'< :key #'car)))
    (list sorted
          (= (car (nth 0 sorted)) 1)
          (= (car (nth 1 sorted)) 1)
          (= (car (nth 2 sorted)) 2)
          (= (car (nth 3 sorted)) 2)
          (= (car (nth 4 sorted)) 3)
          (= (car (nth 5 sorted)) 3)
          (= (length sorted) 6)))) "#,
        expect,
    );
}

#[test]
fn deficiency_cl_loop_accumulate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((nums '(1 2 3 4 5 6 7 8 9 10)))
    (list (cl-loop for n in nums sum n) (= (cl-loop for n in nums sum n) 55)
          (cl-loop for n in nums count (> n 5)) (= (cl-loop for n in nums count (> n 5)) 5)
          (cl-loop for n in nums maximize n) (= (cl-loop for n in nums maximize n) 10)
          (cl-loop for n in nums minimize n) (= (cl-loop for n in nums minimize n) 1)
          (cl-loop for n in nums when (> n 3) sum n) (= (cl-loop for n in nums when (> n 3) sum n) 49)
          (cl-loop for n in nums when (cl-oddp n) collect n) (equal (cl-loop for n in nums when (cl-oddp n) collect n) '(1 3 5 7 9))
          (cl-loop for n in nums when (cl-evenp n) collect n) (equal (cl-loop for n in nums when (cl-evenp n) collect n) '(2 4 6 8 10))))) "#,
        expect,
    );
}

#[test]
fn deficiency_cl_remove_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-remove)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((lst '(a b c a d a e)))
    (list (cl-remove 'a lst) (equal (cl-remove 'a lst) '(b c d e))
          (cl-remove 'b lst) (equal (cl-remove 'b lst) '(a c a d a e))
          (cl-remove-if #'numberp lst) (equal (cl-remove-if #'numberp lst) lst)
          (cl-remove-if-not #'symbolp lst) (equal (cl-remove-if-not #'symbolp lst) lst)
          (cl-position 'a lst) (= (cl-position 'a lst) 0)
          (cl-position 'd lst) (= (cl-position 'd lst) 4)
          (cl-position 'z lst) (null (cl-position 'z lst))))) "#,
        expect,
    );
}

#[test]
fn deficiency_cl_subseq_reverse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-subseq)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((v [1 2 3 4 5 6 7 8 9 10])
        (l '(a b c d e f g h i j)))
    (list (cl-subseq v 0 5) (equal (cl-subseq v 0 5) [1 2 3 4 5])
          (cl-subseq v 5) (equal (cl-subseq v 5) [6 7 8 9 10])
          (reverse l) (equal (reverse l) '(j i h g f e d c b a))
          (nreverse (copy-sequence l)) (equal (nreverse (copy-sequence l)) '(j i h g f e d c b a))
          (length v) (= (length v) 10)
          (length l) (= (length l) 10)
          (cl-subseq l 3 7) (equal (cl-subseq l 3 7) '(d e f g))))) "#,
        expect,
    );
}

#[test]
fn deficiency_cl_assoc_rassoc() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((a . 1) t (c . 3) t nil t (b . 2) t (e . 5) t nil t (d . 4) t 3 t 42 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((alist '((a . 1) (b . 2) (c . 3) (d . 4) (e . 5))))
    (list (assoc 'a alist) (equal (assoc 'a alist) '(a . 1))
          (assoc 'c alist) (equal (assoc 'c alist) '(c . 3))
          (assoc 'z alist) (null (assoc 'z alist))
          (rassoc 2 alist) (equal (rassoc 2 alist) '(b . 2))
          (rassoc 5 alist) (equal (rassoc 5 alist) '(e . 5))
          (rassoc 99 alist) (null (rassoc 99 alist))
          (assq 'd alist) (equal (assq 'd alist) '(d . 4))
          (alist-get 'c alist) (= (alist-get 'c alist) 3)
          (alist-get 'z alist 42) (= (alist-get 'z alist 42) 42)))) "#,
        expect,
    );
}

#[test]
fn deficiency_mapcar_mapconcat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((2 3 4 5 6) t (1 4 9 16 25) t \"1-2-3-4-5\" t (\"1\" \"2\" \"3\" \"4\" \"5\") t (1 2 3 4 5) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((nums '(1 2 3 4 5)))
    (list (mapcar #'1+ nums) (equal (mapcar #'1+ nums) '(2 3 4 5 6))
          (mapcar (lambda (n) (* n n)) nums) (equal (mapcar (lambda (n) (* n n)) nums) '(1 4 9 16 25))
          (mapconcat (lambda (n) (number-to-string n)) nums "-")
          (string= (mapconcat (lambda (n) (number-to-string n)) nums "-") "1-2-3-4-5")
          (mapcar #'number-to-string nums) (equal (mapcar #'number-to-string nums) '("1" "2" "3" "4" "5"))
          (mapc (lambda (_n) nil) nums) (equal (mapc (lambda (_n) nil) nums) nums)))) "#,
        expect,
    );
}

#[test]
fn deficiency_cl_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-merge)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((a '(1 3 5 7 9))
        (b '(2 4 6 8 10)))
    (let ((merged (cl-merge 'list (copy-sequence a) (copy-sequence b) #'<)))
      (list merged
            (equal merged '(1 2 3 4 5 6 7 8 9 10))
            (= (length merged) 10)
            (= (nth 0 merged) 1)
            (= (nth 9 merged) 10))))) "#,
        expect,
    );
}

#[test]
fn deficiency_cl_reduce() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-reduce)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((nums '(1 2 3 4 5 6 7 8 9 10)))
    (list (cl-reduce #'+ nums) (= (cl-reduce #'+ nums) 55)
          (cl-reduce #'* '(1 2 3 4 5)) (= (cl-reduce #'* '(1 2 3 4 5)) 120)
          (cl-reduce #'max nums) (= (cl-reduce #'max nums) 10)
          (cl-reduce #'min nums) (= (cl-reduce #'min nums) 1)
          (cl-reduce #'+ nums :initial-value 100) (= (cl-reduce #'+ nums :initial-value 100) 155)
          (cl-replace (make-list 5 0) nums) (equal (cl-replace (make-list 5 0) nums) '(1 2 3 4 5))))) "#,
        expect,
    );
}

#[test]
fn deficiency_cl_tree_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function tree-copy)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((tree '(1 (2 (3 4)) (5 (6 (7 8))))))
    (list (tree-copy tree) (equal (tree-copy tree) tree)
          (not (eq (tree-copy tree) tree))
          (flatten-tree tree) (equal (flatten-tree tree) '(1 2 3 4 5 6 7 8))
          (flatten-tree '(a (b) ((c d)) nil)) (equal (flatten-tree '(a (b) ((c d)) nil)) '(a b c d))))) "#,
        expect,
    );
}

#[test]
fn deficiency_cl_loop_for_vectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((v [10 20 30 40 50])
        (result nil))
    (cl-loop for x across v
             for i from 0
             do (push (cons i x) result))
    (setq result (nreverse result))
    (list result
          (= (length result) 5)
          (equal (car result) '(0 . 10))
          (equal (nth 4 result) '(4 . 50))
          (cl-loop for x across v sum x) (= (cl-loop for x across v sum x) 150)
          (cl-loop for x across v maximize x) (= (cl-loop for x across v maximize x) 50)
          (cl-loop for x across v minimize x) (= (cl-loop for x across v minimize x) 10)))) "#,
        expect,
    );
}

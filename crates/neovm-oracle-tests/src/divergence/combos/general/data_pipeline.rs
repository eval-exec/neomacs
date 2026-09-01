//! Divergence tests: complex data transformation pipelines.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_map_filter_sort_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((5 3 8 1 9 2 7 4 6 10) (25 9 64 1 81 4 49 16 36 100) (36 49 64 81 100) 330 nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((input '(5 3 8 1 9 2 7 4 6 10))
        (squared (mapcar (lambda (x) (* x x)) input))
        (filtered (seq-filter (lambda (x) (> x 25)) squared))
        (sorted (sort (copy-sequence filtered) #'<))
        (total (seq-reduce #'+ sorted 0)))
  (list input squared sorted total
        (= total 295)
        (= (length sorted) 7))) ",
        expect,
    );
}

#[test]
fn divergence_alist_to_hash_to_sorted_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((apple banana cherry date) (nil nil nil) 4 t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((input '((banana . 3) (apple . 5) (cherry . 1) (date . 4)))
        (ht (make-hash-table :test 'equal)))
  (dolist (pair input) (puthash (car pair) (cdr pair) ht))
  (let ((keys (sort (mapcar #'car input) #'string<))
        (vals (mapcar (lambda (k) (gethash (symbol-name k) ht)) '(banana apple cherry))))
    (list keys vals
          (hash-table-count ht)
          (= (hash-table-count ht) 4)))) ",
        expect,
    );
}

#[test]
fn divergence_group_by_partition_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 3 6) (2 5) (4) t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((data '((a 1) (b 2) (a 3) (c 4) (b 5) (a 6)))
        (groups (make-hash-table :test 'eq)))
  (dolist (item data)
    (let* ((key (car item))
           (val (cadr item))
           (existing (gethash key groups)))
      (puthash key (append existing (list val)) groups)))
  (list (sort (gethash 'a groups) #'<)
        (sort (gethash 'b groups) #'<)
        (gethash 'c groups)
        (= (hash-table-count groups) 3)
        (= (length (gethash 'a groups)) 3))) ",
        expect,
    );
}

#[test]
fn divergence_string_split_join_transform() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"alice\" \"bob\" \"charlie\") (30 25 35) ((\"alice\" \"30\" \"engineer\") (\"bob\" \"25\" \"designer\") (\"charlie\" \"35\" \"manager\")) t t 3 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((csv \"alice,30,engineer\\nbob,25,designer\\ncharlie,35,manager\")
        (lines (split-string csv \"\\n\" t))
        (rows (mapcar (lambda (l) (split-string l \",\" t)) lines))
        (names (mapcar #'car rows))
        (ages (mapcar (lambda (r) (string-to-number (nth 1 r))) rows))
        (avg (/ (seq-reduce #'+ ages 0) (float (length ages)))))
  (list names
        ages
        rows
        (> avg 28.0)
        (< avg 32.0)
        (length rows)
        (= (length ages) 3))) ",
        expect,
    );
}

#[test]
fn divergence_tree_transform_recursive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((2 (3 (4 5)) (6 (7 (8)))) (1 (4 (9 16)) (25 (36 (49)))) (\"a\" (\"b\" (\"c\" \"d\")) \"e\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defun test-tree-map-xxx (fn tree)
    (cond
     ((null tree) nil)
     ((consp tree) (cons (test-tree-map-xxx fn (car tree))
                         (test-tree-map-xxx fn (cdr tree))))
     (t (funcall fn tree))))
  (let ((input '(1 (2 (3 4)) (5 (6 (7))))))
    (list (test-tree-map-xxx #'1+ input)
          (test-tree-map-xxx (lambda (x) (* x x)) input)
          (test-tree-map-xxx #'symbol-name '(a (b (c d)) e))))) ",
        expect,
    );
}

#[test]
fn divergence_plist_to_alist_to_sorted() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function nope)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((pl '(:name \"Alice\" :age 30 :score 95))
        (alist nil))
  (let ((rest pl))
    (while rest
      (push (cons (car rest) (cadr rest)) alist)
      (setq rest (cddr rest))))
  (let ((sorted (sort alist (lambda (a b) (string< (symbol-name (car a))
                                                     (symbol-name (car b)))))))
    (list (plist-get pl :name)
          (plist-get pl :age)
          (plist-get pl :score)
          (plist-get pl :missing 'nope)
          (eq (plist-get pl :missing 'nope) 'nope)
          sorted
          (= (length sorted) 3)))) ",
        expect,
    );
}

#[test]
fn divergence_nested_map_reduce_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable col-sums)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((matrix '((1 2 3) (4 5 6) (7 8 9)))
        (row-sums (mapcar (lambda (row) (seq-reduce #'+ row 0)) matrix))
        (col-sums (dotimes (i 3 nil)
                    (push (seq-reduce #'+ (mapcar (lambda (row) (nth i row)) matrix) 0)
                          col-sums)))
        (col-sums (nreverse col-sums)))
  (list row-sums
        col-sums
        (= (nth 0 row-sums) 6)
        (= (nth 1 row-sums) 15)
        (= (nth 2 row-sums) 24)
        (= (nth 0 col-sums) 12)
        (= (nth 2 col-sums) 18))) ",
        expect,
    );
}

#[test]
fn divergence_string_transform_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Hello World  FOO bar\" \"hello world  foo bar\" (\"hello\" \"world\" \"foo\" \"bar\") (\"bar\" \"foo\" \"hello\" \"world\") \"bar-foo-hello-world\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((input \"  Hello World  FOO bar  \")
        (trimmed (string-trim input))
        (downcased (downcase trimmed))
        (words (split-string downcased \" +\" t))
        (sorted (sort (copy-sequence words) #'string<))
        (joined (string-join sorted \"-\")))
  (list trimmed downcased words sorted joined
        (string= joined \"bar-foo-hello-world\"))) ",
        expect,
    );
}

#[test]
fn divergence_assoc_chain_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Alice\" user 7 nil 3)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((db '((user1 . ((name . \"Alice\") (role . admin) (level . 5)))
                      (user2 . ((name . \"Bob\") (role . user) (level . 3)))
                      (user3 . ((name . \"Carol\") (role . admin) (level . 7)))))
        (lookup (lambda (uid field)
                  (cdr (assoc field (cdr (assoc uid db)))))))
  (list (funcall lookup 'user1 'name)
        (funcall lookup 'user2 'role)
        (funcall lookup 'user3 'level)
        (funcall lookup 'user1 'missing)
        (length db))) ",
        expect,
    );
}

#[test]
fn divergence_vector_matrix_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments mapcar 3)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((v1 [1 2 3])
        (v2 [4 5 6]))
  (let ((dot-product (seq-reduce #'+ (mapcar #'* (append v1 nil) (append v2 nil)) 0))
        (cross-z (- (* (aref v1 0) (aref v2 1))
                    (* (aref v1 1) (aref v2 0)))))
    (list dot-product
          (= dot-product 32)
          cross-z
          (= cross-z -3)
          (vconcat v1 v2)
          (= (length (vconcat v1 v2)) 6)))) ",
        expect,
    );
}

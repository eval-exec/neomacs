//! Divergence tests: remaining stress - large buffers, deep nesting.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_large_buffer_insert_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1007 nil 2006)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (make-string 1000 ?A))
  (insert "TARGET")
  (insert (make-string 1000 ?B))
  (goto-char 1)
  (search-forward "TARGET")
  (list (point)
        (= (point) 1001)
        (buffer-size)))"#,
        expect,
    );
}

#[test]
fn divergence_deeply_nested_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 4 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((a 1))
  (let ((b (+ a 1)))
    (let ((c (+ b 1)))
      (let ((d (+ c 1)))
        (let ((e (+ d 1)))
          (list a b c d e))))))"#,
        expect,
    );
}

#[test]
fn divergence_deeply_nested_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Invalid error symbol\" outer-error)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case e1
  (condition-case e2
    (condition-case e3
        (error "deep error")
      (error (signal 'outer-error (list e3))))
    (error (signal 'outer-error (list e2))))
  (outer-error (list 'caught (cdr (car (cdr e1))))))"#,
        expect,
    );
}

#[test]
fn divergence_large_list_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (100 51 100 (5 4 3 2 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((lst (number-sequence 1 100)))
  (list (length lst)
        (nth 50 lst)
        (car (last lst))
        (reverse (take 5 lst))))"#,
        expect,
    );
}

#[test]
fn divergence_many_consecutive_inserts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (50 1 51)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (dotimes (i 50)
    (insert (format "line%d\n" i)))
  (list (count-lines 1 (point-max))
        (goto-char 1)
        (line-number-at-pos (point-max))))"#,
        expect,
    );
}

#[test]
fn divergence_nested_save_excursion_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (goto-char 5)
  (save-excursion
    (goto-char 1)
    (save-excursion
      (goto-char 10)
      (insert "X"))
    (list (point)))
  (list (point)))"#,
        expect,
    );
}

#[test]
fn divergence_large_hash_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (100 \"val50\" \"val99\" missing)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ht (make-hash-table :test 'eql)))
  (dotimes (i 100)
    (puthash i (format "val%d" i) ht))
  (list (hash-table-count ht)
        (gethash 50 ht)
        (gethash 99 ht)
        (gethash 100 ht 'missing)))"#,
        expect,
    );
}

#[test]
fn divergence_many_text_property_regions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((bold \"region0\") (bold \"region1\") (bold \"region2\") 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (make-string 50 ?X))
  (dotimes (i 10)
    (put-text-property (1+ (* i 5)) (+ 4 (* i 5))
                       'face (list 'bold (format "region%d" i))))
  (list (get-text-property 3 'face)
        (get-text-property 8 'face)
        (get-text-property 13 'face)
        (next-property-change 1)))"#,
        expect,
    );
}

#[test]
fn divergence_undo_many_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t \"012345678910111213141516171819\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq buffer-undo-list nil)
  (dotimes (i 20)
    (goto-char (point-max))
    (insert (number-to-string i))
    (undo-boundary))
  (let ((boundary-count (length (seq-filter #'null buffer-undo-list))))
    (list (> boundary-count 0)
          (<= boundary-count 20)
          (buffer-string))))"#,
        expect,
    );
}

#[test]
fn divergence_many_overlays_on_same_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 4 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (make-string 20 ?X))
  (let (ovs)
    (dotimes (i 10)
      (let ((ov (make-overlay (1+ i) (+ 5 i))))
        (overlay-put ov 'priority i)
        (push ov ovs)))
    (list (length (overlays-in 1 20))
          (length (overlays-at 5))
          (>= (length (overlays-at 5)) 5))))"#,
        expect,
    );
}

//! Divergence tests: stress tests with large data, deep recursion, many objects.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_large_buffer_many_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-evenp)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (dotimes (_ 100) (insert \"abcdefghijklmnopqrstuvwxyz \"))
  (let ((count 0))
    (dotimes (i 50)
      (let ((ov (make-overlay (+ 1 (* i 27)) (+ 10 (* i 27)))))
        (overlay-put ov 'priority i)
        (overlay-put ov 'face (if (cl-evenp i) 'bold 'italic))
        (cl-incf count)))
    (list count
          (length (overlays-in 1 100))
          (overlay-get (car (overlays-at 50)) 'priority)
          (>= (length (overlays-in 1 (point-max))) 10)))) ",
        expect,
    );
}

#[test]
fn divergence_deep_recursive_accumulator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5050 125250 t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defun test-deep-sum-xxx (n acc)
    (if (<= n 0) acc
      (test-deep-sum-xxx (1- n) (+ acc n))))
  (list (test-deep-sum-xxx 100 0)
        (test-deep-sum-xxx 500 0)
        (= (test-deep-sum-xxx 100 0) 5050))) ",
        expect,
    );
}

#[test]
fn divergence_many_interleaved_textprops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-evenp)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert (make-string 200 ?x))
  (dotimes (i 100)
    (put-text-property (1+ (* i 2)) (+ 2 (* i 2))
                       'idx i)
    (put-text-property (1+ (* i 2)) (+ 2 (* i 2))
                       'parity (if (cl-evenp i) 'even 'odd)))
  (let ((even-count 0) (odd-count 0))
    (dotimes (i 100)
      (if (eq (get-text-property (1+ (* i 2)) 'parity) 'even)
          (cl-incf even-count)
        (cl-incf odd-count)))
    (list even-count odd-count
          (= even-count 50)
          (= odd-count 50)
          (= (get-text-property 1 'idx) 0)
          (= (get-text-property 199 'idx) 99)))) ",
        expect,
    );
}

#[test]
fn divergence_large_list_map_filter_reduce() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-evenp)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((nums (number-sequence 1 1000))
        (squares (mapcar (lambda (x) (* x x)) nums))
        (evens (seq-filter #'cl-evenp nums))
        (total (seq-reduce #'+ evens 0))
        (sum-sq (seq-reduce #'+ (seq-filter #'cl-evenp squares) 0)))
  (list (length nums)
        (length evens)
        (= total 250500)
        (= (nth 999 squares) 1000000)
        (> sum-sq 0))) ",
        expect,
    );
}

#[test]
fn divergence_many_nested_let_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 4 5 6 7 8 9 10 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((a 1))
  (let ((b (+ a 1)))
    (let ((c (+ b 1)))
      (let ((d (+ c 1)))
        (let ((e (+ d 1)))
          (let ((f (+ e 1)))
            (let ((g (+ f 1)))
              (let ((h (+ g 1)))
                (let ((i (+ h 1)))
                  (let ((j (+ i 1)))
                    (list a b c d e f g h i j
                          (= j 10)
                          (= (+ a b c d e f g h i j) 55)))))))))))) ",
        expect,
    );
}

#[test]
fn divergence_many_hash_table_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (200 1764 t missing t nil 0)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((ht (make-hash-table :test 'equal :size 500)))
  (dotimes (i 200)
    (puthash (format \"key-%04d\" i) (* i i) ht))
  (list (hash-table-count ht)
        (gethash \"key-0042\" ht)
        (= (gethash \"key-0042\" ht) 1764)
        (gethash \"key-9999\" ht 'missing)
        (eq (gethash \"key-9999\" ht 'missing) 'missing)
        (dotimes (i 200) (remhash (format \"key-%04d\" i) ht))
        (hash-table-count ht))) ",
        expect,
    );
}

#[test]
fn divergence_large_string_search_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (200 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (dotimes (_ 50)
    (insert \"The quick brown fox jumps over the lazy dog. \"))
  (goto-char 1)
  (let ((count 0))
    (while (re-search-forward \"\\\\(quick\\\\|lazy\\\\|brown\\\\|fox\\\\)\" nil t)
      (cl-incf count)
      (replace-match \"REDACTED\" t))
    (list count
          (>= count 100)
          (= (count-matches \"REDACTED\" 1 (point-max)) count)))) ",
        expect,
    );
}

#[test]
fn divergence_deep_catch_throw_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 7)""#]];
    crate::common::assert_oracle_parity_expect(
        "(catch 'done
  (dotimes (i 10)
    (catch (intern (format \"level-%d\" i))
      (dotimes (j 10)
        (when (and (= i 5) (= j 7))
          (throw 'done (list i j))))))) ",
        expect,
    );
}

#[test]
fn divergence_many_buffer_ops_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (101 t t 2055)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"START\")
  (dotimes (i 100)
    (goto-char (point-max))
    (insert (format \"\\nLine %03d: %s\" i (make-string (mod i 20) ?x))))
  (goto-char 1)
  (let ((line-count 0))
    (while (not (eobp))
      (cl-incf line-count)
      (forward-line 1))
    (list line-count
          (>= line-count 100)
          (= (line-number-at-pos (point-max)) line-count)
          (buffer-size)))) ",
        expect,
    );
}

#[test]
fn divergence_many_undo_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"BASE-0-1-2-3-4-5-6-7-8-9\" \"BASE-0-1-2-3-4-5-6-7-8\" \"BASE-0-1-2-3-4-5-6-7-8\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"BASE\")
  (dotimes (i 10)
    (undo-boundary)
    (goto-char (point-max))
    (insert (format \"-%d\" i)))
  (let ((s1 (buffer-string)))
    (dotimes (_ 5)
      (primitive-undo 1 buffer-undo-list))
    (let ((s2 (buffer-string)))
      (dotimes (_ 5)
        (primitive-undo 1 buffer-undo-list))
      (list s1 s2 (buffer-string))))) ",
        expect,
    );
}

//! Deep combo: dolist + dotimes + mapc + mapcar + nreverse + destructive ops.
//! Tests iteration patterns with accumulation and side effects.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_dolist_with_buffer_insert_and_collect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"dli\")))\n\
         (with-current-buffer buf\n\
         (let ((result nil))\n\
         (dolist (x '(alpha beta gamma delta))\n\
         (insert (symbol-name x))\n\
         (put-text-property (point-min) (point) 'item x)\n\
         (push (cons (point) x) result))\n\
         (list (nreverse result)\n\
         (buffer-string)\n\
         (get-text-property 1 'item)\n\
         (get-text-property 10 'item))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_dotimes_with_vector_building() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((v (make-vector 10 nil)))\n\
         (dotimes (i 10)\n\
         (aset v i (* i i)))\n\
         (list (aref v 0) (aref v 3) (aref v 5) (aref v 9)\n\
         (length v)\n\
         (append v nil)))",
        expect,
    );
}

#[test]
fn deficiency_mapc_with_side_effects_on_hash() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ht (make-hash-table :test 'eql))\n\
         (sum 0))\n\
         (dotimes (i 10) (puthash i (1+ i) ht))\n\
         (mapc (lambda (key)\n\
         (setq sum (+ sum (gethash key ht))))\n\
         (cl-loop for k being the hash-keys of ht collect k))\n\
         (list sum (hash-table-count ht))))",
        expect,
    );
}

#[test]
fn deficiency_mapcar_with_index_via_number_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-mapcar)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((items '(a b c d e)))\n\
         (mapcar (lambda (pair)\n\
         (list (car pair) (cdr pair)))\n\
         (cl-mapcar 'cons items (number-sequence 1 5)))))",
        expect,
    );
}

#[test]
fn deficiency_nreverse_build_pattern_with_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"[one]\" \"[two]\" \"[three]\" \"[four]\" \"[five]\") \"[one] [two] [three] [four] [five]\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((acc nil))\n\
         (dolist (s '(\"one\" \"two\" \"three\" \"four\" \"five\"))\n\
         (push (format \"[%s]\" s) acc))\n\
         (let ((result (nreverse acc)))\n\
         (list result\n\
         (mapconcat 'identity result \" \")))))",
        expect,
    );
}

#[test]
fn deficiency_nested_dolist_matrix_build() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((0 1 2 3) (4 5 6 7) (8 9 10 11))""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((matrix nil))\n\
         (dotimes (i 3)\n\
         (let ((row nil))\n\
         (dotimes (j 4)\n\
         (push (+ (* i 4) j) row))\n\
         (push (nreverse row) matrix)))\n\
         (nreverse matrix)))",
        expect,
    );
}

#[test]
fn deficiency_mapcan_with_filter_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-mapcan)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-mapcan (lambda (x)\n\
         (when (cl-oddp x)\n\
         (list (* x x))))\n\
         '(1 2 3 4 5 6 7 8 9 10)))",
        expect,
    );
}

#[test]
fn deficiency_reduce_with_custom_accumulator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-reduce)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-reduce (lambda (acc x)\n\
         (cons (car acc) (cons x (cdr acc))))\n\
         '(a b c d e)\n\
         :initial-value '(0)))",
        expect,
    );
}

#[test]
fn deficiency_dolist_with_hash_table_keys_and_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ht (make-hash-table :test 'equal))\n\
         (buf (generate-new-buffer \"dhb\")))\n\
         (puthash \"alpha\" 1 ht)\n\
         (puthash \"beta\" 2 ht)\n\
         (puthash \"gamma\" 3 ht)\n\
         (with-current-buffer buf\n\
         (let ((keys (sort (cl-loop for k being the hash-keys of ht collect k)\n\
         #'string<)))\n\
         (dolist (k keys)\n\
         (insert (format \"%s=%d\\n\" k (gethash k ht)))))\n\
         (list (buffer-string)\n\
         (hash-table-count ht)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_mapcar_over_string_with_char_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function map)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((s \"Hello World\"))\n\
         (list (mapcar (lambda (c) (if (eq c ? ) '- (downcase c)))\n\
         (append s nil))\n\
         (map 'string (lambda (c) (if (eq c ? ) ?_ (upcase c))) s))))",
        expect,
    );
}

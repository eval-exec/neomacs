//! Deep stress: format + prin1 + read + print-circle + object identity + buffer combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_print_circle_shared_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((shared (list 1 2 3)))\n\
         (let ((cell (list shared shared)))\n\
         (let ((printed (prin1-to-string cell)))\n\
         (let ((read-back (read printed)))\n\
         (list printed\n\
         (equal read-back cell)\n\
         (= (length read-back) 2))))))",
        expect,
    );
}

#[test]
fn deficiency_format_multibyte_buffer_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"fmb\")))\n\
         (with-current-buffer buf\n\
         (let ((items '((\"apple\" . 3) (\"banana\" . 5) (\"\\u00e9clair\" . 2) (\"\\u4e16\\u754c\" . 1))))\n\
         (dolist (item items)\n\
         (insert (format \"%-10s x %d = %d\\n\"\n\
         (car item) (cdr item)\n\
         (* (length (car item)) (cdr item)))))\n\
         (put-text-property 1 15 'section 'fruit)\n\
         (let ((s (buffer-string)))\n\
         (list s\n\
         (length s)\n\
         (string-bytes s)\n\
         (get-text-property 1 'section))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_prin1_read_roundtrip_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"prr\")))\n\
         (with-current-buffer buf\n\
         (let* ((m1 (copy-marker 5))\n\
         (m2 (copy-marker 10))\n\
         (data (list 'markers (marker-position m1) (marker-position m2)\n\
         'buffer (buffer-name)\n\
         'text \"hello world\"))\n\
         (printed (prin1-to-string data))\n\
         (read-back (read printed)))\n\
         (list (equal read-back data)\n\
         (nth 1 read-back)\n\
         (nth 2 read-back)\n\
         (nth 4 read-back)\n\
         (nth 6 read-back)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_format_propertize_buffer_insert_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"fpu\")))\n\
         (with-current-buffer buf\n\
         (let ((rows '((1 \"Alice\" 95) (2 \"Bob\" 87) (3 \"\\u4e2d\\u6587\" 92))))\n\
         (dolist (row rows)\n\
         (let ((line (apply #'format \"%3d | %-10s | %3d\\n\"\n\
         (mapcar (lambda (x) x) row))))\n\
         (insert (propertize line 'row (car row)))))\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (re-search-forward \"Bob\")\n\
         (replace-match \"ROBERT\")\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (r1 (get-text-property 1 'row))\n\
         (r20 (get-text-property 20 'row)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s r1 r20\n\
         (buffer-string)\n\
         (get-text-property 1 'row)\n\
         (get-text-property 20 'row)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_hash_table_eq_vs_equal_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((h-eq (make-hash-table :test 'eq))\n\
         (h-equal (make-hash-table :test 'equal)))\n\
         (let ((sym 'hello)\n\
         (str \"hello\"))\n\
         (puthash sym 'symbol h-eq)\n\
         (puthash str 'string h-equal)\n\
         (puthash (intern \"hello\") 'interned h-eq)\n\
         (list (gethash sym h-eq)\n\
         (gethash str h-equal)\n\
         (gethash 'hello h-eq)\n\
         (gethash \"hello\" h-equal)\n\
         (= (hash-table-count h-eq) 2)\n\
         (= (hash-table-count h-equal) 1))))",
        expect,
    );
}

#[test]
fn deficiency_object_identity_eq_after_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((sym1 'test-sym)\n\
         (sym2 (intern \"test-sym\"))\n\
         (str1 \"test\")\n\
         (str2 (copy-sequence \"test\"))\n\
         (lst1 (list 1 2 3))\n\
         (lst2 (list 1 2 3)))\n\
         (list (eq sym1 sym2)\n\
         (eq str1 str2)\n\
         (equal str1 str2)\n\
         (eq lst1 lst2)\n\
         (equal lst1 lst2)\n\
         (eql 42 42)\n\
         (eql 42 42.0)))",
        expect,
    );
}

#[test]
fn deficiency_read_from_buffer_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 28 35)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"rfb\")))\n\
         (with-current-buffer buf\n\
         (insert \"(hello world) (foo bar) (1 2 3)\")\n\
         (put-text-property 1 15 'group 'first)\n\
         (put-text-property 16 27 'group 'second)\n\
         (put-text-property 28 35 'group 'third)\n\
         (let ((forms nil))\n\
         (goto-char 1)\n\
         (condition-case nil\n\
         (while t\n\
         (push (read buf) forms))\n\
         (end-of-file nil))\n\
         (let ((result (nreverse forms)))\n\
         (list result\n\
         (= (length result) 3)\n\
         (get-text-property 1 'group)\n\
         (get-text-property 16 'group)\n\
         (get-text-property 28 'group))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_buffer_string_props_format_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"bsf\")))\n\
         (with-current-buffer buf\n\
         (insert \"key1:val1\\nkey2:val2\\nkey3:val3\")\n\
         (put-text-property 1 5 'field 'key)\n\
         (put-text-property 6 10 'field 'val)\n\
         (put-text-property 11 15 'field 'key)\n\
         (put-text-property 16 20 'field 'val)\n\
         (put-text-property 21 25 'field 'key)\n\
         (put-text-property 26 30 'field 'val)\n\
         (let ((pairs (split-string (buffer-string) \"\\n\" t)))\n\
         (let ((parsed (mapcar (lambda (p)\n\
         (let ((parts (split-string p \":\")))\n\
         (cons (car parts) (cadr parts))))\n\
         pairs)))\n\
         (let ((formatted (mapconcat (lambda (p)\n\
         (format \"[%s=>%s]\" (car p) (cdr p)))\n\
         parsed \" \")))\n\
         (list formatted\n\
         (get-text-property 1 'field)\n\
         (get-text-property 6 'field)\n\
         (length parsed))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_obarray_intern_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((new-ob (make-vector 127 0)))\n\
         (intern \"alpha\" new-ob)\n\
         (intern \"beta\" new-ob)\n\
         (intern \"gamma\" new-ob)\n\
         (let ((count 0))\n\
         (mapatoms (lambda (_) (setq count (1+ count))) new-ob)\n\
         (list count\n\
         (intern-soft \"alpha\" new-ob)\n\
         (intern-soft \"delta\" new-ob)\n\
         (eq (intern \"alpha\" new-ob) (intern \"alpha\" new-ob))\n\
         (= count 3))))",
        expect,
    );
}

#[test]
fn deficiency_cl_defstruct_print_read_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-defstruct (test-record\n\
         (:constructor make-test-rec)\n\
         (:type list))\n\
         name age score)\n\
         (let ((rec (make-test-rec :name \"Alice\" :age 30 :score 95)))\n\
         (let ((printed (prin1-to-string rec)))\n\
         (let ((read-back (read printed)))\n\
         (list printed\n\
         (equal read-back rec)\n\
         (test-record-name read-back)\n\
         (test-record-age read-back)\n\
         (test-record-score read-back)\n\
         (test-record-name rec)))))",
        expect,
    );
}

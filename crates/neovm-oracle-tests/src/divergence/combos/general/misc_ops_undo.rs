//! Deep stress: regexp-opt + replace-regexp-in-string + cl-defmethod + undo combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_replace_regexp_in_string_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"Hello <world> and <universe> plus <galaxy>\" t 42)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let* ((input \"Hello [world] and [universe] plus [galaxy]\")\n\
         (result (replace-regexp-in-string \"\\\\[\\\\([^]]*\\\\)\\\\]\" \"<\\\\1>\" input)))\n\
         (list result\n\
         (string= result \"Hello <world> and <universe> plus <galaxy>\")\n\
         (length result))))",
        expect,
    );
}

#[test]
fn deficiency_mapcan_over_intervals_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"miu\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAA-BBB-CCC-DDD-EEE\")\n\
         (put-text-property 1 4 'group 1)\n\
         (put-text-property 4 5 'group 'sep)\n\
         (put-text-property 5 8 'group 2)\n\
         (put-text-property 8 9 'group 'sep)\n\
         (put-text-property 9 12 'group 3)\n\
         (put-text-property 12 13 'group 'sep)\n\
         (put-text-property 13 16 'group 4)\n\
         (put-text-property 16 17 'group 'sep)\n\
         (put-text-property 17 20 'group 5)\n\
         (let ((groups (split-string (buffer-string) \"-\")))\n\
         (undo-boundary)\n\
         (erase-buffer)\n\
         (insert (mapconcat #'identity groups \";\"))\n\
         (undo-boundary)\n\
         (let ((s (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list groups s\n\
         (buffer-string)\n\
         (get-text-property 1 'group)\n\
         (get-text-property 5 'group))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_apply_partial_with_buffer_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"apb\")))\n\
         (with-current-buffer buf\n\
         (insert \"initial content\")\n\
         (put-text-property 1 16 'state 'original)\n\
         (let ((inserter (apply-partially #'insert \"PREFIX:\")))\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (funcall inserter)\n\
         (put-text-property 1 7 'state 'prefix)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (s1 (get-text-property 1 'state))\n\
         (s7 (get-text-property 7 'state)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s s1 s7\n\
         (buffer-string)\n\
         (get-text-property 1 'state)\n\
         (get-text-property 7 'state))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_compose_region_undo_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"cru\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGHIJ\")\n\
         (put-text-property 1 6 'half 'first)\n\
         (put-text-property 6 11 'half 'second)\n\
         (undo-boundary)\n\
         (compose-region 3 7 nil)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (c3 (get-text-property 3 'composition))\n\
         (h3 (get-text-property 3 'half))\n\
         (h7 (get-text-property 7 'half)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s (and c3 t) h3 h7\n\
         (buffer-string)\n\
         (get-text-property 3 'composition)\n\
         (get-text-property 3 'half)\n\
         (get-text-property 7 'half)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_decompose_region_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"dru\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGH\")\n\
         (compose-region 1 8 nil)\n\
         (put-text-property 1 8 'comp 'yes)\n\
         (undo-boundary)\n\
         (decompose-region 1 8)\n\
         (undo-boundary)\n\
         (let ((c1 (get-text-property 1 'composition))\n\
         (p1 (get-text-property 1 'comp)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list c1 p1\n\
         (get-text-property 1 'composition)\n\
         (get-text-property 1 'comp)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_cl_coerce_with_buffer_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-coerce)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((vec [1 2 3 4 5]))\n\
         (let ((lst (cl-coerce vec 'list))\n\
         (str (cl-coerce vec 'string))\n\
         (back (cl-coerce lst 'vector)))\n\
         (list lst\n\
         (= (length lst) 5)\n\
         (equal back vec)\n\
         (equal vec back)))))",
        expect,
    );
}

#[test]
fn deficiency_nested_set_buffer_undo_chains() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf1)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf1 (generate-new-buffer \"ns1\"))\n\
         (buf2 (generate-new-buffer \"ns2\"))\n\
         (buf3 (generate-new-buffer \"ns3\")))\n\
         (with-current-buffer buf1\n\
         (insert \"ONE\")\n\
         (put-text-property 1 4 'buf 1)\n\
         (undo-boundary)\n\
         (with-current-buffer buf2\n\
         (insert \"TWO\")\n\
         (put-text-property 1 4 'buf 2)\n\
         (undo-boundary)\n\
         (with-current-buffer buf3\n\
         (insert \"THREE\")\n\
         (put-text-property 1 5 'buf 3)\n\
         (undo-boundary)\n\
         (insert \"MORE\")\n\
         (put-text-property 6 10 'buf 3)\n\
         (undo-boundary))))\n\
         (with-current-buffer buf3 (primitive-undo 1 buffer-undo-list))\n\
         (with-current-buffer buf2 (primitive-undo 1 buffer-undo-list))\n\
         (with-current-buffer buf1 (primitive-undo 1 buffer-undo-list))\n\
         (list (with-current-buffer buf1 (buffer-string))\n\
         (with-current-buffer buf1 (get-text-property 1 'buf))\n\
         (with-current-buffer buf2 (buffer-string))\n\
         (with-current-buffer buf2 (get-text-property 1 'buf))\n\
         (with-current-buffer buf3 (buffer-string))\n\
         (with-current-buffer buf3 (get-text-property 1 'buf))))\n\
         (kill-buffer buf1)\n\
         (kill-buffer buf2)\n\
         (kill-buffer buf3)))",
        expect,
    );
}

#[test]
fn deficiency_string_props_buffer_insert_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"spb\")))\n\
         (with-current-buffer buf\n\
         (let ((s1 (propertize \"HELLO\" 'face 'bold 'priority 1))\n\
         (s2 (propertize \"WORLD\" 'face 'italic 'priority 2)))\n\
         (insert s1 \" \" s2)\n\
         (undo-boundary)\n\
         (goto-char 6)\n\
         (delete-char 1)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (f1 (get-text-property 1 'face))\n\
         (f6 (get-text-property 6 'face))\n\
         (p1 (get-text-property 1 'priority))\n\
         (p6 (get-text-property 6 'priority)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s f1 f6 p1 p6\n\
         (buffer-string)\n\
         (get-text-property 1 'face)\n\
         (get-text-property 6 'face)\n\
         (get-text-property 1 'priority)\n\
         (get-text-property 6 'priority))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_format_buffer_substring_multiline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"fbm\")))\n\
         (with-current-buffer buf\n\
         (insert \"Name: Alice\\nAge: 30\\nScore: 95\\n\")\n\
         (put-text-property 1 5 'field 'key)\n\
         (put-text-property 6 11 'field 'val)\n\
         (put-text-property 12 15 'field 'key)\n\
         (put-text-property 16 18 'field 'val)\n\
         (put-text-property 19 24 'field 'key)\n\
         (put-text-property 25 27 'field 'val)\n\
         (let ((lines (split-string (buffer-string) \"\\n\" t)))\n\
         (let ((parsed (mapcar (lambda (line)\n\
         (let ((parts (split-string line \": \" t)))\n\
         (cons (car parts) (cadr parts))))\n\
         lines)))\n\
         (list parsed\n\
         (length parsed)\n\
         (get-text-property 1 'field)\n\
         (get-text-property 6 'field)\n\
         (get-text-property 19 'field))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_vconcat_mapcar_with_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"vmo\")))\n\
         (with-current-buffer buf\n\
         (insert \"The quick brown fox\")\n\
         (let* ((words (split-string (buffer-string)))\n\
         (lengths (mapcar #'length words))\n\
         (vec (vconcat lengths))\n\
         (sorted (sort (copy-sequence lengths) #'>)))\n\
         (list vec sorted\n\
         (aref vec 0) (aref vec 3)\n\
         (= (length vec) 4)\n\
         (= (aref vec 0) 3)\n\
         (= (aref vec 1) 5)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

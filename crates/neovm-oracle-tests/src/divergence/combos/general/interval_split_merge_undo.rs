//! Deep stress: undo interval split + merge + boundary crossing edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_undo_insert_at_exact_prop_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uib\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAAAAAAAABBBBBBBBBB\")\n\
         (put-text-property 1 11 'side 'left)\n\
         (put-text-property 11 21 'side 'right)\n\
         (undo-boundary)\n\
         (goto-char 11)\n\
         (insert \"XXXXX\")\n\
         (put-text-property 11 16 'side 'center)\n\
         (undo-boundary)\n\
         (let ((scan (cl-loop for i from 1 to (buffer-size)\n\
         collect (cons i (get-text-property i 'side)))))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list scan\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (cons i (get-text-property i 'side)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_delete_at_exact_prop_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"udb\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAABBBBCCCCDDDD\")\n\
         (put-text-property 1 5 'quad 'a)\n\
         (put-text-property 5 9 'quad 'b)\n\
         (put-text-property 9 13 'quad 'c)\n\
         (put-text-property 13 17 'quad 'd)\n\
         (undo-boundary)\n\
         (delete-region 5 9)\n\
         (undo-boundary)\n\
         (let ((scan (cl-loop for i from 1 to (buffer-size)\n\
         collect (cons i (get-text-property i 'quad)))))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list scan\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (cons i (get-text-property i 'quad)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_replace_spanning_two_intervals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"urt\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAABBBCCC\")\n\
         (put-text-property 1 4 'zone 1)\n\
         (put-text-property 4 7 'zone 2)\n\
         (put-text-property 7 10 'zone 3)\n\
         (undo-boundary)\n\
         (goto-char 3)\n\
         (re-search-forward \"BB\")\n\
         (replace-match \"YY\")\n\
         (undo-boundary)\n\
         (let ((scan (cl-loop for i from 1 to (buffer-size)\n\
         collect (cons i (get-text-property i 'zone)))))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list scan\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (cons i (get-text-property i 'zone)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_after_set_text_properties_then_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ust\")))\n\
         (with-current-buffer buf\n\
         (insert \"XXXXXXXXXXXX\")\n\
         (undo-boundary)\n\
         (set-text-properties 1 13 '(layer 1 style plain))\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"YYYY\")\n\
         (undo-boundary)\n\
         (let ((scan (cl-loop for i from 1 to (buffer-size)\n\
         collect (list (get-text-property i 'layer)\n\
         (get-text-property i 'style)))))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list scan\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (list (get-text-property i 'layer)\n\
         (get-text-property i 'style)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_multiple_overlapping_sets() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"umo\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGHIJ\")\n\
         (undo-boundary)\n\
         (put-text-property 1 10 'a 1)\n\
         (undo-boundary)\n\
         (put-text-property 3 8 'b 2)\n\
         (undo-boundary)\n\
         (put-text-property 5 6 'c 3)\n\
         (undo-boundary)\n\
         (goto-char 4)\n\
         (insert \"XXXX\")\n\
         (undo-boundary)\n\
         (let ((scan\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (list i (get-text-property i 'a)\n\
         (get-text-property i 'b)\n\
         (get-text-property i 'c)))))\n\
         (primitive-undo 4 buffer-undo-list)\n\
         (list scan\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (list i (get-text-property i 'a)\n\
         (get-text-property i 'b)\n\
         (get-text-property i 'c)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_after_add_single_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uap\")))\n\
         (with-current-buffer buf\n\
         (insert \"HELLO WORLD\")\n\
         (put-text-property 1 6 'base 'first)\n\
         (put-text-property 6 12 'base 'second)\n\
         (undo-boundary)\n\
         (add-text-properties 3 9 '(extra t bonus yes))\n\
         (undo-boundary)\n\
         (let ((scan (cl-loop for i from 1 to 11\n\
         collect (list (get-text-property i 'base)\n\
         (get-text-property i 'extra)\n\
         (get-text-property i 'bonus)))))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list scan\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to 11\n\
         collect (list (get-text-property i 'base)\n\
         (get-text-property i 'extra)\n\
         (get-text-property i 'bonus)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_property_change_on_single_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"usc\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDE\")\n\
         (undo-boundary)\n\
         (put-text-property 1 2 'mark 'a)\n\
         (undo-boundary)\n\
         (put-text-property 2 3 'mark 'b)\n\
         (undo-boundary)\n\
         (put-text-property 3 4 'mark 'c)\n\
         (undo-boundary)\n\
         (put-text-property 4 5 'mark 'd)\n\
         (undo-boundary)\n\
         (put-text-property 5 6 'mark 'e)\n\
         (undo-boundary)\n\
         (let ((before (cl-loop for i from 1 to 5\n\
         collect (get-text-property i 'mark))))\n\
         (primitive-undo 5 buffer-undo-list)\n\
         (list before\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to 5\n\
         collect (get-text-property i 'mark))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_with_propertize_string_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ups\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAA\")\n\
         (put-text-property 1 5 'layer 0)\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert (propertize \"BBBB\" 'layer 1))\n\
         (undo-boundary)\n\
         (goto-char 9)\n\
         (insert (propertize \"CCCC\" 'layer 2))\n\
         (undo-boundary)\n\
         (let ((scan (cl-loop for i from 1 to (buffer-size)\n\
         collect (cons i (get-text-property i 'layer)))))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list scan\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (cons i (get-text-property i 'layer)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_interval_merge_after_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uim\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAABBBCCC\")\n\
         (put-text-property 1 4 'val 'a)\n\
         (put-text-property 4 7 'val 'b)\n\
         (put-text-property 7 10 'val 'c)\n\
         (undo-boundary)\n\
         (delete-region 4 7)\n\
         (undo-boundary)\n\
         (let ((before-undo\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (cons i (get-text-property i 'val)))))\n\
         (put-text-property 4 7 'val 'merged)\n\
         (undo-boundary)\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list before-undo\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (cons i (get-text-property i 'val)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_after_full_interval_scan() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ufs\")))\n\
         (with-current-buffer buf\n\
         (insert \"AABBCCDDEEFFGGHHIIJJKKLLMMNNOOPP\")\n\
         (dotimes (i 16)\n\
         (let ((start (+ 1 (* i 2))))\n\
         (put-text-property start (+ start 2) 'pair (1+ i))))\n\
         (let ((full-scan\n\
         (cl-loop for pos = 1 then next\n\
         while pos\n\
         for next = (next-single-property-change pos 'pair)\n\
         collect (list pos (get-text-property pos 'pair))\n\
         while next)))\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (delete-region 5 9)\n\
         (undo-boundary)\n\
         (let ((after-delete\n\
         (cl-loop for pos = 1 then next\n\
         while pos\n\
         for next = (next-single-property-change pos 'pair)\n\
         collect (list pos (get-text-property pos 'pair))\n\
         while next)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (let ((after-undo\n\
         (cl-loop for pos = 1 then next\n\
         while pos\n\
         for next = (next-single-property-change pos 'pair)\n\
         collect (list pos (get-text-property pos 'pair))\n\
         while next)))\n\
         (list (length full-scan)\n\
         (length after-delete)\n\
         (length after-undo)\n\
         (buffer-string))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

//! Deep stress: primitive-undo with property-only changes + insert+prop combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_undo_property_only_change_no_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"upo\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGHIJ\")\n\
         (undo-boundary)\n\
         (put-text-property 1 6 'color 'red)\n\
         (undo-boundary)\n\
         (put-text-property 6 11 'color 'blue)\n\
         (undo-boundary)\n\
         (put-text-property 3 8 'style 'bold)\n\
         (undo-boundary)\n\
         (let ((c1 (get-text-property 1 'color))\n\
         (c6 (get-text-property 6 'color))\n\
         (s3 (get-text-property 3 'style))\n\
         (s8 (get-text-property 8 'style)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list c1 c6 s3 s8\n\
         (buffer-string)\n\
         (get-text-property 1 'color)\n\
         (get-text-property 6 'color)\n\
         (get-text-property 3 'style)\n\
         (get-text-property 8 'style)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list (buffer-string)\n\
         (get-text-property 1 'color)\n\
         (get-text-property 6 'color))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_insert_then_set_props_separate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uis\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAA\")\n\
         (undo-boundary)\n\
         (put-text-property 1 5 'layer 1)\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"BBBB\")\n\
         (undo-boundary)\n\
         (put-text-property 5 9 'layer 2)\n\
         (undo-boundary)\n\
         (goto-char 9)\n\
         (insert \"CCCC\")\n\
         (undo-boundary)\n\
         (put-text-property 9 13 'layer 3)\n\
         (undo-boundary)\n\
         (let ((layers (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'layer))))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (let ((after-1 (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'layer))))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (let ((after-2 (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'layer))))\n\
         (list layers after-1 after-2 (buffer-string)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_delete_then_prop_then_undo_both() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"udt\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGHIJKLMNO\")\n\
         (put-text-property 1 16 'orig t)\n\
         (undo-boundary)\n\
         (delete-region 5 11)\n\
         (undo-boundary)\n\
         (put-text-property 5 10 'modified t)\n\
         (undo-boundary)\n\
         (let ((s1 (buffer-string))\n\
         (o5 (get-text-property 5 'orig))\n\
         (m5 (get-text-property 5 'modified)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s1 o5 m5\n\
         (buffer-string)\n\
         (get-text-property 5 'orig)\n\
         (get-text-property 5 'modified)\n\
         (get-text-property 5 'orig))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_prop_change_on_empty_buffer_then_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ueb\")))\n\
         (with-current-buffer buf\n\
         (insert \"HELLO\")\n\
         (put-text-property 1 6 'word 'greeting)\n\
         (undo-boundary)\n\
         (erase-buffer)\n\
         (undo-boundary)\n\
         (insert \"GOODBYE\")\n\
         (put-text-property 1 8 'word 'farewell)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (w1 (get-text-property 1 'word)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s w1\n\
         (buffer-string)\n\
         (get-text-property 1 'word)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_overlapping_prop_changes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uop\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAAAAAAAAAA\")\n\
         (undo-boundary)\n\
         (put-text-property 1 13 'a 1)\n\
         (undo-boundary)\n\
         (put-text-property 3 10 'b 2)\n\
         (undo-boundary)\n\
         (put-text-property 5 8 'c 3)\n\
         (undo-boundary)\n\
         (put-text-property 6 7 'd 4)\n\
         (undo-boundary)\n\
         (let ((scan\n\
         (cl-loop for i from 1 to 13\n\
         collect (list (get-text-property i 'a)\n\
         (get-text-property i 'b)\n\
         (get-text-property i 'c)\n\
         (get-text-property i 'd)))))\n\
         (primitive-undo 4 buffer-undo-list)\n\
         (list scan\n\
         (cl-loop for i from 1 to 13\n\
         collect (list (get-text-property i 'a)\n\
         (get-text-property i 'b)\n\
         (get-text-property i 'c)\n\
         (get-text-property i 'd)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_prop_then_insert_crossing_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"upi\")))\n\
         (with-current-buffer buf\n\
         (insert \"XXXYYYZZZ\")\n\
         (put-text-property 1 4 'zone 'x)\n\
         (put-text-property 4 7 'zone 'y)\n\
         (put-text-property 7 10 'zone 'z)\n\
         (undo-boundary)\n\
         (put-text-property 3 5 'zone 'overlap)\n\
         (undo-boundary)\n\
         (goto-char 4)\n\
         (insert \"MMMM\")\n\
         (undo-boundary)\n\
         (let ((scan\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'zone))))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list scan\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'zone))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_many_small_prop_changes_then_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"umc\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGHIJ\")\n\
         (dotimes (i 10)\n\
         (undo-boundary)\n\
         (put-text-property (1+ i) (+ 2 i) 'idx i))\n\
         (let ((before\n\
         (cl-loop for i from 1 to 10\n\
         collect (get-text-property i 'idx))))\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"INSERT\")\n\
         (undo-boundary)\n\
         (let ((after\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'idx))))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (let ((after-undo\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'idx))))\n\
         (list before after after-undo (buffer-string)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_replace_preserving_different_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"urp\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAABBBCCCDDDEEE\")\n\
         (put-text-property 1 4 'owner 1)\n\
         (put-text-property 4 7 'owner 2)\n\
         (put-text-property 7 10 'owner 3)\n\
         (put-text-property 10 13 'owner 4)\n\
         (put-text-property 13 16 'owner 5)\n\
         (put-text-property 1 16 'global 'all)\n\
         (undo-boundary)\n\
         (goto-char 4)\n\
         (re-search-forward \"BBB\")\n\
         (replace-match \"XXX\")\n\
         (undo-boundary)\n\
         (let ((o4 (get-text-property 4 'owner))\n\
         (o7 (get-text-property 7 'owner))\n\
         (g4 (get-text-property 4 'global)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list o4 o7 g4\n\
         (buffer-string)\n\
         (get-text-property 4 'owner)\n\
         (get-text-property 7 'owner)\n\
         (get-text-property 4 'global)\n\
         (get-text-property 7 'global)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_with_remove_list_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"url\")))\n\
         (with-current-buffer buf\n\
         (insert \"TEXT WITH MULTIPLE PROPERTIES\")\n\
         (put-text-property 1 30 'face 'bold)\n\
         (put-text-property 1 30 'mouse-face 'highlight)\n\
         (put-text-property 1 30 'help-echo \"text here\")\n\
         (put-text-property 1 30 'keymap (make-sparse-keymap))\n\
         (undo-boundary)\n\
         (remove-text-properties 10 20 '(face nil mouse-face nil))\n\
         (undo-boundary)\n\
         (let ((f10 (get-text-property 10 'face))\n\
         (m10 (get-text-property 10 'mouse-face))\n\
         (h10 (get-text-property 10 'help-echo))\n\
         (k10 (get-text-property 10 'keymap)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list f10 m10 (and h10 t) (and k10 t)\n\
         (buffer-string)\n\
         (get-text-property 10 'face)\n\
         (get-text-property 10 'mouse-face)\n\
         (get-text-property 10 'help-echo)\n\
         (get-text-property 10 'keymap)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_interleaved_prop_and_text_changes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uit\")))\n\
         (with-current-buffer buf\n\
         (insert \"START\")\n\
         (put-text-property 1 6 'step 0)\n\
         (undo-boundary)\n\
         (goto-char 6)\n\
         (insert \"-A\")\n\
         (undo-boundary)\n\
         (put-text-property 7 8 'step 1)\n\
         (undo-boundary)\n\
         (goto-char 8)\n\
         (insert \"-B\")\n\
         (undo-boundary)\n\
         (put-text-property 9 10 'step 2)\n\
         (undo-boundary)\n\
         (goto-char 10)\n\
         (insert \"-C\")\n\
         (undo-boundary)\n\
         (put-text-property 11 12 'step 3)\n\
         (undo-boundary)\n\
         (let ((steps (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'step))))\n\
         (primitive-undo 6 buffer-undo-list)\n\
         (list steps\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'step))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

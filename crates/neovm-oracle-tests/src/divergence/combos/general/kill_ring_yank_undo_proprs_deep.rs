//! Deep combo: kill-ring + yank + undo + text properties + markers.
//! Tests kill/yank cycle with property preservation and undo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_kill_yank_preserves_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"kyp\")))\n\
         (with-current-buffer buf\n\
         (insert \"hello world\")\n\
         (put-text-property 1 6 'face 'bold)\n\
         (put-text-property 6 12 'face 'italic)\n\
         (kill-region 1 6)\n\
         (goto-char (point-max))\n\
         (yank)\n\
         (list (buffer-string)\n\
         (get-text-property 1 'face)\n\
         (get-text-property 6 'face)\n\
         (get-text-property 7 'face)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_kill_yank_undo_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"kyu\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAA.BBBB.CCCC\")\n\
         (put-text-property 1 5 'zone 'a)\n\
         (put-text-property 6 10 'zone 'b)\n\
         (put-text-property 11 15 'zone 'c)\n\
         (undo-boundary)\n\
         (kill-region 6 10)\n\
         (undo-boundary)\n\
         (goto-char (point-max))\n\
         (yank)\n\
         (undo-boundary)\n\
         (let ((after-yank (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (let ((after-undo (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list after-yank after-undo\n\
         (buffer-string)\n\
         (get-text-property 6 'zone)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_kill_append_multiple_yanks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"kam\")))\n\
         (with-current-buffer buf\n\
         (insert \"first second third\")\n\
         (kill-region 1 6)\n\
         (kill-append (buffer-substring 7 13) nil)\n\
         (erase-buffer)\n\
         (yank)\n\
         (buffer-string))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_kill_line_yank_with_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"kly\")))\n\
         (with-current-buffer buf\n\
         (insert \"line1\\nline2\\nline3\")\n\
         (put-text-property 1 6 'ln 1)\n\
         (put-text-property 7 12 'ln 2)\n\
         (put-text-property 13 18 'ln 3)\n\
         (goto-char 1)\n\
         (kill-line)\n\
         (goto-char (point-max))\n\
         (yank)\n\
         (list (buffer-string)\n\
         (get-text-property 1 'ln)\n\
         (get-text-property 7 'ln)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_kill_region_rectangle_yank() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"kry\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAA111\\nBBB222\\nCCC333\")\n\
         (put-text-property 1 4 'col 'left)\n\
         (put-text-property 4 7 'col 'right)\n\
         (kill-rectangle 1 4)\n\
         (goto-char (point-max))\n\
         (yank-rectangle)\n\
         (list (buffer-string)\n\
         (get-text-property 1 'col)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_kill_yank_across_buffers_with_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((b1 (generate-new-buffer \"ky1\"))\n\
         (b2 (generate-new-buffer \"ky2\")))\n\
         (with-current-buffer b1\n\
         (insert \"IMPORTANT\")\n\
         (put-text-property 1 10 'tag 'source)\n\
         (kill-region 1 10))\n\
         (with-current-buffer b2\n\
         (insert \"target: \")\n\
         (put-text-property 1 9 'role 'prefix)\n\
         (yank)\n\
         (list (buffer-string)\n\
         (get-text-property 1 'role)\n\
         (get-text-property 9 'tag)))\n\
         (kill-buffer b1) (kill-buffer b2)))",
        expect,
    );
}

#[test]
fn deficiency_kill_ring_max_and_nth() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"krn\")))\n\
         (with-current-buffer buf\n\
         (insert \"first\")\n\
         (kill-region 1 6)\n\
         (insert \"second\")\n\
         (kill-region 1 7)\n\
         (insert \"third\")\n\
         (kill-region 1 6)\n\
         (list (current-kill 0)\n\
         (current-kill 1)\n\
         (current-kill 2)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_yank_pop_after_double_yank() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    crate::common::assert_oracle_parity(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ypo\")))\n\
         (with-current-buffer buf\n\
         (insert \"alpha\")\n\
         (kill-region 1 6)\n\
         (insert \"beta\")\n\
         (kill-region 1 5)\n\
         (insert \"gamma\")\n\
         (kill-region 1 6)\n\
         (yank)\n\
         (yank-pop 1)\n\
         (buffer-string))\n\
         (kill-buffer buf)))",
    );
}

#[test]
fn deficiency_kill_sentence_with_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 35 42)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ksp\")))\n\
         (with-current-buffer buf\n\
         (insert \"First sentence. Second sentence. Third.\")\n\
         (put-text-property 1 17 'sent 1)\n\
         (put-text-property 17 35 'sent 2)\n\
         (put-text-property 35 42 'sent 3)\n\
         (goto-char 17)\n\
         (kill-sentence 1)\n\
         (list (buffer-string)\n\
         (get-text-property 1 'sent)\n\
         (get-text-property 17 'sent)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_kill_word_multiple_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"kwm\")))\n\
         (with-current-buffer buf\n\
         (insert \"the quick brown fox jumps over\")\n\
         (put-text-property 1 4 'w 1)\n\
         (put-text-property 5 10 'w 2)\n\
         (put-text-property 11 16 'w 3)\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (kill-word 1)\n\
         (undo-boundary)\n\
         (kill-word 1)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s (buffer-string)\n\
         (get-text-property 5 'w))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

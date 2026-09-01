//! Deep stress: undo-boundary + primitive-undo + text prop interval edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_undo_20_step_edit_session_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"u20\")))\n\
         (with-current-buffer buf\n\
         (insert \"START\")\n\
         (put-text-property 1 6 'gen 0)\n\
         (undo-boundary)\n\
         (dotimes (i 10)\n\
         (goto-char (point-max))\n\
         (insert (format \"-G%d\" (1+ i)))\n\
         (put-text-property (- (point) 3) (point) 'gen (1+ i))\n\
         (undo-boundary))\n\
         (let ((s (buffer-string))\n\
         (gens (cl-loop for i from 1 to (1- (point-max))\n\
         collect (get-text-property i 'gen))))\n\
         (dotimes (_ 5)\n\
         (primitive-undo 1 buffer-undo-list))\n\
         (list s gens\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (1- (point-max))\n\
         collect (get-text-property i 'gen)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_alternating_insert_delete_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 16 21)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uai\")))\n\
         (with-current-buffer buf\n\
         (insert \"MIDDLE\")\n\
         (put-text-property 1 7 'pos 'center)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (insert \"LEFT\")\n\
         (put-text-property 1 5 'pos 'left)\n\
         (undo-boundary)\n\
         (goto-char (point-max))\n\
         (insert \"RIGHT\")\n\
         (put-text-property (point-max) (+ (point-max) 5) 'pos 'right)\n\
         (undo-boundary)\n\
         (delete-region 1 5)\n\
         (undo-boundary)\n\
         (delete-region 8 13)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (p1 (get-text-property 1 'pos))\n\
         (p4 (get-text-property 4 'pos)))\n\
         (primitive-undo 4 buffer-undo-list)\n\
         (list s p1 p4\n\
         (buffer-string)\n\
         (get-text-property 1 'pos)\n\
         (get-text-property 5 'pos)\n\
         (get-text-property 7 'pos)\n\
         (get-text-property 8 'pos)\n\
         (get-text-property 12 'pos))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_propertize_after_each_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"upa\")))\n\
         (with-current-buffer buf\n\
         (insert \"A\")\n\
         (put-text-property 1 2 'step 1)\n\
         (undo-boundary)\n\
         (goto-char 2)\n\
         (insert \"B\")\n\
         (put-text-property 2 3 'step 2)\n\
         (undo-boundary)\n\
         (goto-char 3)\n\
         (insert \"C\")\n\
         (put-text-property 3 4 'step 3)\n\
         (undo-boundary)\n\
         (goto-char 4)\n\
         (insert \"D\")\n\
         (put-text-property 4 5 'step 4)\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"E\")\n\
         (put-text-property 5 6 'step 5)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (steps (list (get-text-property 1 'step)\n\
         (get-text-property 2 'step)\n\
         (get-text-property 3 'step)\n\
         (get-text-property 4 'step)\n\
         (get-text-property 5 'step))))\n\
         (primitive-undo 5 buffer-undo-list)\n\
         (list s steps\n\
         (buffer-string)\n\
         (list (get-text-property 1 'step)\n\
         (get-text-property 2 'step))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_replace_then_propertize_then_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"urp\")))\n\
         (with-current-buffer buf\n\
         (insert \"The quick brown fox\")\n\
         (put-text-property 1 19 'original t)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (re-search-forward \"quick\")\n\
         (replace-match \"SLOW\")\n\
         (undo-boundary)\n\
         (put-text-property 5 9 'replaced t)\n\
         (undo-boundary)\n\
         (delete-region 5 9)\n\
         (undo-boundary)\n\
         (insert \"FAST\")\n\
         (put-text-property 5 9 'final t)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (p5 (get-text-property 5 'replaced))\n\
         (p5f (get-text-property 5 'final))\n\
         (p1 (get-text-property 1 'original)))\n\
         (primitive-undo 4 buffer-undo-list)\n\
         (list s p5 p5f p1\n\
         (buffer-string)\n\
         (get-text-property 5 'replaced)\n\
         (get-text-property 5 'final)\n\
         (get-text-property 1 'original)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_multiple_overlays_same_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uos\")))\n\
         (with-current-buffer buf\n\
         (insert \"XXXXXXXXXXXXXXXXXXXX\")\n\
         (let ((ovs (cl-loop for i from 1 to 18 by 3\n\
         collect (let ((ov (make-overlay i (+ i 3))))\n\
         (overlay-put ov 'idx (1+ (/ (1- i) 3)))\n\
         ov))))\n\
         (undo-boundary)\n\
         (goto-char 10)\n\
         (insert \"YYYY\")\n\
         (undo-boundary)\n\
         (let ((before-undo\n\
         (list (buffer-string)\n\
         (cl-loop for ov in ovs\n\
         collect (list (overlay-start ov)\n\
         (overlay-end ov)\n\
         (overlay-get ov 'idx))))))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list before-undo\n\
         (buffer-string)\n\
         (cl-loop for ov in ovs\n\
         collect (list (overlay-start ov)\n\
         (overlay-end ov)\n\
         (overlay-get ov 'idx))))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_delete_then_reinsert_different_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 13 13)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"udr\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAABBBBCCCCDDDD\")\n\
         (put-text-property 1 5 'owner 'a)\n\
         (put-text-property 5 9 'owner 'b)\n\
         (put-text-property 9 13 'owner 'c)\n\
         (put-text-property 13 17 'owner 'd)\n\
         (undo-boundary)\n\
         (delete-region 5 13)\n\
         (undo-boundary)\n\
         (insert \"EFGHIJKL\")\n\
         (put-text-property 5 13 'owner 'new)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (o5 (get-text-property 5 'owner))\n\
         (o9 (get-text-property 9 'owner))\n\
         (o13 (get-text-property 13 'owner)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s o5 o9 o13\n\
         (buffer-string)\n\
         (get-text-property 5 'owner)\n\
         (get-text-property 9 'owner)\n\
         (get-text-property 13 'owner)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_kill_ring_yank_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 66 75)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uky\")))\n\
         (with-current-buffer buf\n\
         (insert \"[header]some text here[/header]body content goes here[footer]end[/footer]\")\n\
         (put-text-property 1 9 'tag 'header-open)\n\
         (put-text-property 9 24 'tag 'content)\n\
         (put-text-property 24 33 'tag 'header-close)\n\
         (put-text-property 33 54 'tag 'body)\n\
         (put-text-property 54 62 'tag 'footer-open)\n\
         (put-text-property 62 66 'tag 'footer-content)\n\
         (put-text-property 66 75 'tag 'footer-close)\n\
         (undo-boundary)\n\
         (kill-region 9 24)\n\
         (undo-boundary)\n\
         (goto-char 33)\n\
         (yank)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (t9 (get-text-property 9 'tag))\n\
         (t33 (get-text-property 33 'tag)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s t9 t33\n\
         (buffer-string)\n\
         (get-text-property 9 'tag)\n\
         (get-text-property 24 'tag)\n\
         (get-text-property 33 'tag)\n\
         (get-text-property 54 'tag)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_narrow_insert_widen_insert_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 14 14)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uni\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGHIJKLMNOPQRSTUVWXYZ\")\n\
         (put-text-property 1 14 'half 'first)\n\
         (put-text-property 14 27 'half 'second)\n\
         (let ((m (copy-marker 14)))\n\
         (undo-boundary)\n\
         (narrow-to-region 5 20)\n\
         (undo-boundary)\n\
         (goto-char (point-min))\n\
         (insert \"PPP\")\n\
         (put-text-property (point-min) (+ (point-min) 3) 'added 'narrow)\n\
         (undo-boundary)\n\
         (widen)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (insert \"QQQ\")\n\
         (put-text-property 1 4 'added 'wide)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (a1 (get-text-property 1 'added))\n\
         (a4 (get-text-property 4 'added))\n\
         (h8 (get-text-property 8 'half))\n\
         (h17 (get-text-property 17 'half)))\n\
         (primitive-undo 4 buffer-undo-list)\n\
         (list s a1 a4 h8 h17 (marker-position m)\n\
         (buffer-string)\n\
         (get-text-property 1 'half)\n\
         (get-text-property 14 'half)\n\
         (marker-position m))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_with_nested_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"unc\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAABBBCCC\")\n\
         (put-text-property 1 4 'grp 'a)\n\
         (put-text-property 4 7 'grp 'b)\n\
         (put-text-property 7 10 'grp 'c)\n\
         (undo-boundary)\n\
         (condition-case nil\n\
         (progn\n\
         (goto-char 4)\n\
         (insert \"MMM\")\n\
         (put-text-property 4 7 'grp 'inserted)\n\
         (undo-boundary)\n\
         (condition-case nil\n\
         (progn\n\
         (goto-char 10)\n\
         (delete-region 10 13)\n\
         (put-text-property 7 10 'grp 'truncated)\n\
         (undo-boundary))\n\
         (error nil)))\n\
         (error nil))\n\
         (let ((s (buffer-string))\n\
         (g4 (get-text-property 4 'grp))\n\
         (g7 (get-text-property 7 'grp)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list s g4 g7\n\
         (buffer-string)\n\
         (get-text-property 1 'grp)\n\
         (get-text-property 4 'grp)\n\
         (get-text-property 7 'grp)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_with_dolist_insert_propertize() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"udip\"))\n\
         (items '((1 . \"alpha\") (2 . \"beta\") (3 . \"gamma\") (4 . \"delta\") (5 . \"epsilon\"))))\n\
         (with-current-buffer buf\n\
         (dolist (item items)\n\
         (let ((start (point)))\n\
         (insert (format \"[%d:%s]\" (car item) (cdr item)))\n\
         (put-text-property start (point) 'rank (car item))\n\
         (undo-boundary)))\n\
         (let ((s (buffer-string))\n\
         (ranks (cl-loop for i from 1 to (buffer-size)\n\
         when (get-text-property i 'rank)\n\
         collect (get-text-property i 'rank))))\n\
         (primitive-undo 3 buffer-undo-list)\n\
         (list s ranks\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         when (get-text-property i 'rank)\n\
         collect (get-text-property i 'rank)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

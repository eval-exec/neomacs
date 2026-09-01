//! Deep combo: overlay priority + overlapping + evaporate + lazy highlighting.
//! Tests overlay ordering, merging, and lifecycle semantics.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_overlay_priority_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"opo\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGHIJ\")\n\
         (let ((ov1 (make-overlay 3 8))\n\
         (ov2 (make-overlay 3 8))\n\
         (ov3 (make-overlay 3 8)))\n\
         (overlay-put ov1 'priority 10)\n\
         (overlay-put ov2 'priority 5)\n\
         (overlay-put ov3 'priority 20)\n\
         (overlay-put ov1 'label 'low)\n\
         (overlay-put ov2 'label 'mid)\n\
         (overlay-put ov3 'label 'high)\n\
         (let ((at5 (overlays-at 5)))\n\
         (list (length at5)\n\
         (mapcar (lambda (ov) (overlay-get ov 'label))\n\
         (sort (copy-sequence at5)\n\
         (lambda (a b)\n\
         (< (or (overlay-get a 'priority) 0)\n\
         (or (overlay-get b 'priority) 0)))))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_overlapping_overlays_with_different_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ood\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAABBBCCCDDD\")\n\
         (let ((ov1 (make-overlay 1 7))\n\
         (ov2 (make-overlay 5 10))\n\
         (ov3 (make-overlay 8 13)))\n\
         (overlay-put ov1 'tag 'first)\n\
         (overlay-put ov2 'tag 'second)\n\
         (overlay-put ov3 'tag 'third)\n\
         (list (length (overlays-at 3))\n\
         (length (overlays-at 6))\n\
         (length (overlays-at 9))\n\
         (length (overlays-in 1 13))\n\
         (mapcar (lambda (ov) (overlay-get ov 'tag))\n\
         (overlays-in 5 7))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_overlay_evaporate_on_empty_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"oee\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAXXXXBBB\")\n\
         (let ((ov1 (make-overlay 4 8)))\n\
         (overlay-put ov1 'evaporate t)\n\
         (overlay-put ov1 'data 'middle)\n\
         (let ((before (overlay-start ov1)))\n\
         (delete-region 4 8)\n\
         (list before\n\
         (overlay-start ov1)\n\
         (overlay-end ov1)\n\
         (overlay-get ov1 'data)\n\
         (buffer-string))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_overlay_evaporate_nil_survives_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"oen\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAXXXXBBB\")\n\
         (let ((ov (make-overlay 4 8)))\n\
         (overlay-put ov 'evaporate nil)\n\
         (overlay-put ov 'data 'persist)\n\
         (delete-region 4 8)\n\
         (list (overlay-start ov)\n\
         (overlay-end ov)\n\
         (overlay-get ov 'data)\n\
         (buffer-string)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_overlay_move_with_insertion_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"omi\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGH\")\n\
         (let ((ov (make-overlay 3 6)))\n\
         (overlay-put ov 'data 'target)\n\
         (goto-char 3)\n\
         (insert \"XX\")\n\
         (let ((after-insert (list (overlay-start ov) (overlay-end ov))))\n\
         (goto-char 9)\n\
         (insert \"YY\")\n\
         (list after-insert\n\
         (overlay-start ov) (overlay-end ov)\n\
         (overlay-get ov 'data)\n\
         (buffer-string)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_overlay_after_replace_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"orm\")))\n\
         (with-current-buffer buf\n\
         (insert \"alpha beta gamma delta\")\n\
         (let ((ov (make-overlay 7 11)))\n\
         (overlay-put ov 'word 'target)\n\
         (goto-char 1)\n\
         (re-search-forward \"beta\")\n\
         (replace-match \"BETA\")\n\
         (list (overlay-start ov) (overlay-end ov)\n\
         (overlay-get ov 'word)\n\
         (buffer-string)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_delete_overlay_vs_set_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"dov\")))\n\
         (with-current-buffer buf\n\
         (insert \"CONTENT\")\n\
         (let ((ov1 (make-overlay 1 4))\n\
         (ov2 (make-overlay 5 8)))\n\
         (overlay-put ov1 'tag 'first)\n\
         (overlay-put ov2 'tag 'second)\n\
         (let ((count-before (length (overlays-in 1 8))))\n\
         (delete-overlay ov1)\n\
         (let ((count-after (length (overlays-in 1 8))))\n\
         (list count-before count-after\n\
         (overlay-start ov1)\n\
         (overlay-start ov2)\n\
         (overlay-get ov1 'tag))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_overlay_in_narrowed_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"oni\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAABBBCCCCDDDDEEEE\")\n\
         (let ((ov (make-overlay 5 13)))\n\
         (overlay-put ov 'role 'mid)\n\
         (narrow-to-region 4 14)\n\
         (list (overlay-start ov)\n\
         (overlay-end ov)\n\
         (overlay-get ov 'role)\n\
         (length (overlays-in (point-min) (point-max))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_overlay_after_buffer_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"obs\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAABBBCCC\")\n\
         (let ((ov (make-overlay 4 7)))\n\
         (overlay-put ov 'tag 'mid)\n\
         (let ((sub (buffer-substring 3 8))\n\
         (sub-no-props (buffer-substring-no-properties 3 8)))\n\
         (list sub sub-no-props\n\
         (overlay-start ov) (overlay-end ov)\n\
         (overlay-get ov 'tag))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_many_overlays_stress_with_priorities() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mos\")))\n\
         (with-current-buffer buf\n\
         (insert (make-string 100 ?X))\n\
         (let ((ovs (cl-loop for i from 0 to 9\n\
         collect (let ((ov (make-overlay (+ 1 (* i 10)) (+ 11 (* i 10)))))\n\
         (overlay-put ov 'priority (1+ i))\n\
         (overlay-put ov 'idx i)\n\
         ov))))\n\
         (list (length ovs)\n\
         (length (overlays-in 1 101))\n\
         (cl-loop for ov in ovs\n\
         collect (list (overlay-get ov 'idx)\n\
         (overlay-start ov)\n\
         (overlay-end ov)\n\
         (overlay-get ov 'priority))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

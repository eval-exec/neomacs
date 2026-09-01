//! Deep combo: indirect-buffer + base-buffer + shared text + separate locals.
//! Tests indirect buffer semantics with text modifications and local state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_indirect_buffer_shares_text_with_base() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments buffer-string 1)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((base (generate-new-buffer \"ib1\")))\n\
         (with-current-buffer base\n\
         (insert \"SHARED-TEXT\"))\n\
         (let ((ind (make-indirect-buffer base \"ib1-ind\")))\n\
         (with-current-buffer ind\n\
         (goto-char 1)\n\
         (insert \"MODIFIED-\"))\n\
         (list (buffer-string (with-current-buffer base (current-buffer)))\n\
         (buffer-string ind)\n\
         (eq base (buffer-base ind))))\n\
         (kill-buffer base)))",
        expect,
    );
}

#[test]
fn deficiency_indirect_buffer_separate_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments buffer-string 1)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((base (generate-new-buffer \"sp1\")))\n\
         (with-current-buffer base\n\
         (insert \"ABCDEFGHIJ\")\n\
         (goto-char 3))\n\
         (let ((ind (make-indirect-buffer base \"sp1-ind\")))\n\
         (with-current-buffer ind\n\
         (goto-char 7))\n\
         (list (with-current-buffer base (point))\n\
         (with-current-buffer ind (point))\n\
         (buffer-string base)))\n\
         (kill-buffer base)))",
        expect,
    );
}

#[test]
fn deficiency_indirect_buffer_separate_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((base (generate-new-buffer \"sn1\")))\n\
         (with-current-buffer base\n\
         (insert \"AAABBBCCCCDDDEEE\"))\n\
         (let ((ind (make-indirect-buffer base \"sn1-ind\")))\n\
         (with-current-buffer ind\n\
         (narrow-to-region 4 12))\n\
         (list (with-current-buffer base\n\
         (list (point-min) (point-max)))\n\
         (with-current-buffer ind\n\
         (list (point-min) (point-max)\n\
         (buffer-string)))))\n\
         (kill-buffer base)))",
        expect,
    );
}

#[test]
fn deficiency_indirect_buffer_text_props_shared() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function buffer-base)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((base (generate-new-buffer \"tp1\")))\n\
         (with-current-buffer base\n\
         (insert \"ALPHA-BETA-GAMMA\")\n\
         (put-text-property 1 6 'zone 'a)\n\
         (put-text-property 7 12 'zone 'b))\n\
         (let ((ind (make-indirect-buffer base \"tp1-ind\")))\n\
         (with-current-buffer ind\n\
         (put-text-property 7 12 'zone 'modified))\n\
         (list (get-text-property 7 'zone base)\n\
         (get-text-property 7 'zone ind)\n\
         (eq base (buffer-base ind))))\n\
         (kill-buffer base)))",
        expect,
    );
}

#[test]
fn deficiency_indirect_buffer_overlays_separate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((base (generate-new-buffer \"os1\")))\n\
         (with-current-buffer base\n\
         (insert \"CONTENT-HERE\")\n\
         (let ((ov-base (make-overlay 1 8)))\n\
         (overlay-put ov-base 'source 'base)))\n\
         (let ((ind (make-indirect-buffer base \"os1-ind\")))\n\
         (with-current-buffer ind\n\
         (let ((ov-ind (make-overlay 9 13)))\n\
         (overlay-put ov-ind 'source 'ind)))\n\
         (list (with-current-buffer base\n\
         (length (overlays-in 1 13)))\n\
         (with-current-buffer ind\n\
         (length (overlays-in 1 13)))))\n\
         (kill-buffer base)))",
        expect,
    );
}

#[test]
fn deficiency_indirect_buffer_separate_buffer_locals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (defvar ibl-test-var 'global)\n\
         (make-variable-buffer-local 'ibl-test-var)\n\
         (let ((base (generate-new-buffer \"ibl\")))\n\
         (with-current-buffer base\n\
         (setq ibl-test-var 'base-val))\n\
         (let ((ind (make-indirect-buffer base \"ibl-ind\")))\n\
         (with-current-buffer ind\n\
         (setq ibl-test-var 'ind-val))\n\
         (list (buffer-local-value 'ibl-test-var base)\n\
         (buffer-local-value 'ibl-test-var ind)))\n\
         (kill-buffer base)))",
        expect,
    );
}

#[test]
fn deficiency_indirect_buffer_with_markers_in_both() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((base (generate-new-buffer \"mk1\")))\n\
         (with-current-buffer base\n\
         (insert \"1234567890\")\n\
         (let ((m-base (copy-marker 5)))\n\
         (let ((ind (make-indirect-buffer base \"mk1-ind\")))\n\
         (with-current-buffer ind\n\
         (goto-char 3)\n\
         (insert \"XX\"))\n\
         (list (marker-position m-base)\n\
         (with-current-buffer base (buffer-string))\n\
         (with-current-buffer ind (buffer-string))))))\n\
         (kill-buffer base)))",
        expect,
    );
}

#[test]
fn deficiency_indirect_buffer_undo_separate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments buffer-string 1)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((base (generate-new-buffer \"un1\")))\n\
         (with-current-buffer base\n\
         (insert \"ORIGINAL-TEXT\"))\n\
         (let ((ind (make-indirect-buffer base \"un1-ind\")))\n\
         (with-current-buffer ind\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (insert \"MOD-\"))\n\
         (list (buffer-string base)\n\
         (buffer-string ind)))\n\
         (kill-buffer base)))",
        expect,
    );
}

#[test]
fn deficiency_two_indirect_buffers_same_base_edit_conflict() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments buffer-string 1)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((base (generate-new-buffer \"2ib\")))\n\
         (with-current-buffer base\n\
         (insert \"ABCDEFGHIJ\"))\n\
         (let ((ind1 (make-indirect-buffer base \"2ib-1\"))\n\
         (ind2 (make-indirect-buffer base \"2ib-2\")))\n\
         (with-current-buffer ind1\n\
         (goto-char 5)\n\
         (insert \"1\"))\n\
         (with-current-buffer ind2\n\
         (goto-char 3)\n\
         (insert \"2\"))\n\
         (list (buffer-string base)\n\
         (buffer-string ind1)\n\
         (buffer-string ind2)\n\
         (eq (buffer-base ind1) (buffer-base ind2))))\n\
         (kill-buffer base)))",
        expect,
    );
}

#[test]
fn deficiency_indirect_buffer_kill_base_kills_indirect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((base (generate-new-buffer \"kbi\")))\n\
         (with-current-buffer base (insert \"test\"))\n\
         (let ((ind (make-indirect-buffer base \"kbi-ind\")))\n\
         (let ((alive-before (and (buffer-live-p base)\n\
         (buffer-live-p ind))))\n\
         (kill-buffer base)\n\
         (list alive-before\n\
         (buffer-live-p base)\n\
         (buffer-live-p ind))))))",
        expect,
    );
}

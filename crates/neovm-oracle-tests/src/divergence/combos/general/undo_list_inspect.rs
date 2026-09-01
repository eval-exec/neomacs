//! Deep stress: undo-boundary merging + buffer-undo-list inspection + undo groups.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_undo_list_structure_inspection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uls\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAA\")\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"BBBB\")\n\
         (undo-boundary)\n\
         (goto-char 9)\n\
         (insert \"CCCC\")\n\
         (undo-boundary)\n\
         (let ((ul (length buffer-undo-list)))\n\
         (goto-char 1)\n\
         (delete-region 1 4)\n\
         (undo-boundary)\n\
         (let ((ul2 (length buffer-undo-list)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list ul ul2\n\
         (buffer-string)\n\
         (> ul2 ul)\n\
         (= (buffer-size) 12))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_boundary_noop_between() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ubn\")))\n\
         (with-current-buffer buf\n\
         (insert \"HELLO\")\n\
         (put-text-property 1 6 'orig t)\n\
         (undo-boundary)\n\
         (undo-boundary)\n\
         (undo-boundary)\n\
         (goto-char 6)\n\
         (insert \" WORLD\")\n\
         (put-text-property 6 12 'added t)\n\
         (undo-boundary)\n\
         (let ((ul (length buffer-undo-list)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list ul\n\
         (buffer-string)\n\
         (get-text-property 1 'orig)\n\
         (get-text-property 6 'added)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_after_setq_buffer_undo_list_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"usn\")))\n\
         (with-current-buffer buf\n\
         (insert \"INITIAL\")\n\
         (put-text-property 1 8 'state 'initial)\n\
         (undo-boundary)\n\
         (setq buffer-undo-list nil)\n\
         (goto-char 1)\n\
         (insert \"PREFIX\")\n\
         (put-text-property 1 7 'state 'prefix)\n\
         (undo-boundary)\n\
         (let ((ul (length buffer-undo-list)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list ul\n\
         (buffer-string)\n\
         (get-text-property 1 'state)\n\
         (get-text-property 7 'state)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_amalgamating_changes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uac\")))\n\
         (with-current-buffer buf\n\
         (buffer-enable-undo)\n\
         (insert \"START\")\n\
         (undo-boundary)\n\
         (insert \"A\")\n\
         (insert \"B\")\n\
         (insert \"C\")\n\
         (let ((before (buffer-string)))\n\
         (undo-boundary)\n\
         (insert \"D\")\n\
         (insert \"E\")\n\
         (undo-boundary)\n\
         (let ((ul (length buffer-undo-list)))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (list before ul\n\
         (buffer-string))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_inhibition_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 15 21)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uiw\")))\n\
         (with-current-buffer buf\n\
         (insert \"CONTENT\")\n\
         (put-text-property 1 8 'level 1)\n\
         (undo-boundary)\n\
         (let ((buffer-undo-list t))\n\
         (goto-char 1)\n\
         (insert \"PREFIX\")\n\
         (put-text-property 1 7 'level 0))\n\
         (undo-boundary)\n\
         (goto-char 15)\n\
         (insert \"SUFFIX\")\n\
         (put-text-property 15 21 'level 2)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (l1 (get-text-property 1 'level))\n\
         (l8 (get-text-property 8 'level))\n\
         (l15 (get-text-property 15 'level)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s l1 l8 l15\n\
         (buffer-string)\n\
         (get-text-property 1 'level)\n\
         (get-text-property 8 'level)\n\
         (get-text-property 15 'level)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_after_kill_all_undo_records() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uka\")))\n\
         (with-current-buffer buf\n\
         (insert \"FIRST\")\n\
         (put-text-property 1 6 'ver 1)\n\
         (undo-boundary)\n\
         (goto-char 1)\n\
         (insert \"SECOND\")\n\
         (put-text-property 1 7 'ver 2)\n\
         (undo-boundary)\n\
         (setq buffer-undo-list nil)\n\
         (goto-char 1)\n\
         (insert \"THIRD\")\n\
         (put-text-property 1 6 'ver 3)\n\
         (undo-boundary)\n\
         (let ((ul (length buffer-undo-list)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list ul\n\
         (buffer-string)\n\
         (get-text-property 1 'ver)\n\
         (get-text-property 7 'ver)\n\
         (get-text-property 13 'ver)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_complex_nested_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ucn\")))\n\
         (with-current-buffer buf\n\
         (insert \"()\")\n\
         (put-text-property 1 2 'type 'paren)\n\
         (undo-boundary)\n\
         (goto-char 2)\n\
         (insert \"A B C\")\n\
         (put-text-property 2 7 'type 'content)\n\
         (undo-boundary)\n\
         (goto-char 3)\n\
         (insert \"1\")\n\
         (undo-boundary)\n\
         (goto-char 6)\n\
         (insert \"2\")\n\
         (undo-boundary)\n\
         (goto-char 9)\n\
         (insert \"3\")\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (types (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'type))))\n\
         (primitive-undo 3 buffer-undo-list)\n\
         (list s types\n\
         (buffer-string)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'type))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_buffer_changed_since() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ubc\")))\n\
         (with-current-buffer buf\n\
         (let ((t1 (buffer-modified-tick)))\n\
         (insert \"AAA\")\n\
         (let ((t2 (buffer-modified-tick)))\n\
         (put-text-property 1 4 'p 1)\n\
         (let ((t3 (buffer-modified-tick)))\n\
         (undo-boundary)\n\
         (delete-region 1 4)\n\
         (let ((t4 (buffer-modified-tick)))\n\
         (list (< t1 t2)\n\
         (< t2 t3)\n\
         (< t3 t4)\n\
         (buffer-string))))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_after_multiple_erase_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ume\")))\n\
         (with-current-buffer buf\n\
         (insert \"VERSION1\")\n\
         (put-text-property 1 9 'ver 1)\n\
         (undo-boundary)\n\
         (erase-buffer)\n\
         (insert \"VERSION2\")\n\
         (put-text-property 1 9 'ver 2)\n\
         (undo-boundary)\n\
         (erase-buffer)\n\
         (insert \"VERSION3\")\n\
         (put-text-property 1 9 'ver 3)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (let ((s2 (buffer-string)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s s2\n\
         (buffer-string)\n\
         (get-text-property 1 'ver))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_undo_with_overlays_recorded() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"uwr\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAAAAAAAA\")\n\
         (let ((ov1 (make-overlay 1 6))\n\
         (ov2 (make-overlay 6 11)))\n\
         (overlay-put ov1 'tag 'first)\n\
         (overlay-put ov2 'tag 'second)\n\
         (undo-boundary)\n\
         (goto-char 3)\n\
         (insert \"XXXX\")\n\
         (overlay-put ov1 'modified t)\n\
         (undo-boundary)\n\
         (let ((o1s (overlay-start ov1))\n\
         (o1e (overlay-end ov1))\n\
         (o2s (overlay-start ov2))\n\
         (o2e (overlay-end ov2))\n\
         (o1m (overlay-get ov1 'modified)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list o1s o1e o2s o2e o1m\n\
         (overlay-start ov1) (overlay-end ov1)\n\
         (overlay-start ov2) (overlay-end ov2)\n\
         (overlay-get ov1 'modified)\n\
         (overlay-get ov2 'tag)\n\
         (buffer-string))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

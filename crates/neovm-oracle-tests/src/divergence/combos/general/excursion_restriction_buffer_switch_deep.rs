//! Deep combo: save-excursion + save-restriction + point tracking + buffer switch.
//! Tests excursion semantics with narrowing and buffer-local point.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_save_excursion_point_after_insert_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"sep\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGHIJ\")\n\
         (put-text-property 1 6 'zone 'left)\n\
         (put-text-property 6 11 'zone 'right)\n\
         (let ((m (copy-marker 5)))\n\
         (goto-char 3)\n\
         (save-excursion\n\
         (goto-char 7)\n\
         (insert \"XXX\")\n\
         (delete-region 1 2))\n\
         (list (point) (marker-position m)\n\
         (buffer-string)\n\
         (get-text-property 3 'zone))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_save_restriction_with_nested_inserts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"srn\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAA-BBBB-CCCC-DDDD\")\n\
         (put-text-property 1 5 'grp 'a)\n\
         (put-text-property 6 10 'grp 'b)\n\
         (put-text-property 11 15 'grp 'c)\n\
         (narrow-to-region 6 15)\n\
         (save-restriction\n\
         (widen)\n\
         (goto-char 1)\n\
         (insert \"PREFIX\")\n\
         (let ((wide-string (buffer-string)))\n\
         (list wide-string (point) (buffer-size))))\n\
         (list (point-min) (point-max)\n\
         (buffer-string)\n\
         (get-text-property (point-min) 'grp))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_save_excursion_across_buffer_switch_with_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable b1)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((b1 (generate-new-buffer \"se1\"))\n\
         (b2 (generate-new-buffer \"se2\")))\n\
         (with-current-buffer b1\n\
         (insert \"BUFFER-ONE\")\n\
         (narrow-to-region 1 7)\n\
         (goto-char 4))\n\
         (with-current-buffer b2\n\
         (insert \"BUFFER-TWO\")\n\
         (narrow-to-region 1 7)\n\
         (goto-char 3))\n\
         (save-excursion\n\
         (set-buffer b1)\n\
         (goto-char (point-max))\n\
         (insert \"X\"))\n\
         (list (with-current-buffer b1 (list (point) (buffer-string)))\n\
         (with-current-buffer b2 (list (point) (buffer-string)))))\n\
         (kill-buffer b1) (kill-buffer b2)))",
        expect,
    );
}

#[test]
fn deficiency_nested_save_excursion_with_marker_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"nse\")))\n\
         (with-current-buffer buf\n\
         (insert \"0123456789\")\n\
         (let ((m5 (copy-marker 5))\n\
         (m8 (copy-marker 8)))\n\
         (goto-char 3)\n\
         (save-excursion\n\
         (goto-char 5)\n\
         (insert \"QQ\")\n\
         (save-excursion\n\
         (goto-char 1)\n\
         (insert \"RR\"))\n\
         (list (point) (marker-position m5) (marker-position m8)))\n\
         (list (point) (marker-position m5) (marker-position m8)\n\
         (buffer-string)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_save_excursion_with_overlay_modification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"sov\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAABBBCCC\")\n\
         (let ((ov (make-overlay 4 7)))\n\
         (overlay-put ov 'tag 'middle)\n\
         (put-text-property 1 4 'part 'head)\n\
         (put-text-property 4 7 'part 'mid)\n\
         (put-text-property 7 10 'part 'tail)\n\
         (save-excursion\n\
         (goto-char 4)\n\
         (delete-region 4 7)\n\
         (insert \"XXXXXX\")\n\
         (overlay-put ov 'tag 'replaced))\n\
         (list (point)\n\
         (overlay-start ov) (overlay-end ov)\n\
         (overlay-get ov 'tag)\n\
         (get-text-property 4 'part)\n\
         (buffer-string)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_save_restriction_with_undo_after_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"sru\")))\n\
         (with-current-buffer buf\n\
         (insert \"LINE1\\nLINE2\\nLINE3\\nLINE4\")\n\
         (put-text-property 1 6 'line 1)\n\
         (put-text-property 7 12 'line 2)\n\
         (put-text-property 13 18 'line 3)\n\
         (narrow-to-region 7 18)\n\
         (undo-boundary)\n\
         (save-restriction\n\
         (widen)\n\
         (goto-char 1)\n\
         (delete-region 1 6)\n\
         (undo-boundary))\n\
         (list (point-min) (point-max)\n\
         (buffer-string)\n\
         (get-text-property (point-min) 'line))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_excursion_with_kill_ring_save() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ekr\")))\n\
         (with-current-buffer buf\n\
         (insert \"The quick brown fox jumps\")\n\
         (put-text-property 1 4 'pos 'start)\n\
         (put-text-property 5 10 'pos 'adj)\n\
         (save-excursion\n\
         (goto-char 5)\n\
         (kill-region 5 10))\n\
         (list (point)\n\
         (buffer-string)\n\
         (get-text-property 4 'pos)\n\
         (get-text-property 5 'pos)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_triple_nested_save_excursion_point_restoration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"tne\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGHIJ\")\n\
         (goto-char 2)\n\
         (save-excursion\n\
         (goto-char 4)\n\
         (insert \"1\")\n\
         (save-excursion\n\
         (goto-char 7)\n\
         (insert \"2\")\n\
         (save-excursion\n\
         (goto-char 10)\n\
         (insert \"3\"))\n\
         (list (point) (buffer-string)))\n\
         (list (point) (buffer-string))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_save_buffer_state_with_props_and_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"sbs\")))\n\
         (with-current-buffer buf\n\
         (insert \"ALPHA-BETA-GAMMA\")\n\
         (put-text-property 1 6 'sec 1)\n\
         (put-text-property 7 12 'sec 2)\n\
         (put-text-property 12 17 'sec 3)\n\
         (let ((ov (make-overlay 7 12)))\n\
         (overlay-put ov 'type 'middle)\n\
         (goto-char 1)\n\
         (push-mark 12 t)\n\
         (let ((m (mark t)))\n\
         (save-excursion\n\
         (delete-region 7 12)\n\
         (insert \"DELTA\"))\n\
         (list (point) (mark t)\n\
         (overlay-start ov) (overlay-end ov)\n\
         (overlay-get ov 'type)\n\
         (get-text-property 7 'sec)\n\
         (buffer-string)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_excursion_restore_after_narrow_widen_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ern\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAA.BBBB.CCCC.DDDD.EEEE\")\n\
         (put-text-property 1 5 'block 'a)\n\
         (put-text-property 6 10 'block 'b)\n\
         (goto-char 3)\n\
         (narrow-to-region 6 20)\n\
         (goto-char 8)\n\
         (save-excursion\n\
         (save-restriction\n\
         (widen)\n\
         (goto-char 1)\n\
         (insert \"ZZ\")))\n\
         (list (point) (point-min) (point-max)\n\
         (buffer-string)\n\
         (get-text-property (point-min) 'block))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

//! Deep stress: text property interval edge cases + property-change + undo corruption.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_property_change_scan_after_replace_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"pcs\")))\n\
         (with-current-buffer buf\n\
         (insert \"11112222333344445555666677778888\")\n\
         (dotimes (i 8)\n\
         (let ((start (+ 1 (* i 4))))\n\
         (put-text-property start (+ start 4) 'block (1+ i))))\n\
         (let ((before\n\
         (cl-loop for pos = 1 then next\n\
         while pos\n\
         for next = (next-single-property-change pos 'block)\n\
         collect (list pos (get-text-property pos 'block))\n\
         while next)))\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (delete-region 5 9)\n\
         (insert \"AAAA\")\n\
         (put-text-property 5 9 'block 'replaced)\n\
         (undo-boundary)\n\
         (let ((after-replace\n\
         (cl-loop for pos = 1 then next\n\
         while pos\n\
         for next = (next-single-property-change pos 'block)\n\
         collect (list pos (get-text-property pos 'block))\n\
         while next)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (let ((after-undo\n\
         (cl-loop for pos = 1 then next\n\
         while pos\n\
         for next = (next-single-property-change pos 'block)\n\
         collect (list pos (get-text-property pos 'block))\n\
         while next)))\n\
         (list before after-replace after-undo (buffer-string))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_propertize_gap_insert_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"pgi\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAACCCCAAAA\")\n\
         (put-text-property 1 5 'color 'red)\n\
         (put-text-property 5 9 'color 'blue)\n\
         (put-text-property 9 13 'color 'red)\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"BBBB\")\n\
         (put-text-property 5 9 'color 'green)\n\
         (undo-boundary)\n\
         (let ((scan\n\
         (cl-loop for pos = 1 then next\n\
         while pos\n\
         for next = (next-single-property-change pos 'color)\n\
         collect (list pos (get-text-property pos 'color))\n\
         while next)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (let ((after-undo\n\
         (cl-loop for pos = 1 then next\n\
         while pos\n\
         for next = (next-single-property-change pos 'color)\n\
         collect (list pos (get-text-property pos 'color))\n\
         while next)))\n\
         (list scan after-undo (buffer-string))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_text_property_any_not_all_after_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"tpa\")))\n\
         (with-current-buffer buf\n\
         (insert \"NNNNYYYNNNNYYYNNNN\")\n\
         (put-text-property 1 5 'flag nil)\n\
         (put-text-property 5 8 'flag t)\n\
         (put-text-property 8 12 'flag nil)\n\
         (put-text-property 12 15 'flag t)\n\
         (put-text-property 15 19 'flag nil)\n\
         (let ((has-yes (text-property-any 1 19 'flag t))\n\
         (all-yes (text-property-not-all 1 19 'flag nil)))\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (delete-region 5 8)\n\
         (undo-boundary)\n\
         (let ((has-yes2 (text-property-any 1 (point-max) 'flag t))\n\
         (all-yes2 (text-property-not-all 1 (point-max) 'flag nil)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (let ((has-yes3 (text-property-any 1 (point-max) 'flag t))\n\
         (all-yes3 (text-property-not-all 1 (point-max) 'flag nil)))\n\
         (list has-yes all-yes has-yes2 all-yes2 has-yes3 all-yes3\n\
         (buffer-string)))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_remove_text_properties_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"rtp\")))\n\
         (with-current-buffer buf\n\
         (insert \"PPPPQQQQRRRRSSSS\")\n\
         (put-text-property 1 5 'owner 'p)\n\
         (put-text-property 5 9 'owner 'q)\n\
         (put-text-property 9 13 'owner 'r)\n\
         (put-text-property 13 17 'owner 's)\n\
         (undo-boundary)\n\
         (remove-text-properties 5 13 '(owner nil))\n\
         (undo-boundary)\n\
         (let ((scan\n\
         (cl-loop for i from 1 to 17\n\
         collect (get-text-property i 'owner))))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list scan\n\
         (cl-loop for i from 1 to 17\n\
         collect (get-text-property i 'owner))\n\
         (buffer-string))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_add_text_properties_merge_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"atm\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAAAAAAAAAAAAAAAAAA\")\n\
         (put-text-property 1 11 'layer 1)\n\
         (put-text-property 11 21 'layer 2)\n\
         (undo-boundary)\n\
         (add-text-properties 5 15 '(highlight t emphasis t))\n\
         (undo-boundary)\n\
         (let ((h5 (get-text-property 5 'highlight))\n\
         (e5 (get-text-property 5 'emphasis))\n\
         (l5 (get-text-property 5 'layer))\n\
         (h15 (get-text-property 15 'highlight))\n\
         (e15 (get-text-property 15 'emphasis))\n\
         (l15 (get-text-property 15 'layer)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list h5 e5 l5 h15 e15 l15\n\
         (get-text-property 5 'highlight)\n\
         (get-text-property 5 'emphasis)\n\
         (get-text-property 5 'layer)\n\
         (get-text-property 15 'highlight)\n\
         (get-text-property 15 'emphasis)\n\
         (get-text-property 15 'layer))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_set_text_properties_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"stp\")))\n\
         (with-current-buffer buf\n\
         (insert \"XXXXXXXXXXYYYYYYYYYY\")\n\
         (put-text-property 1 11 'side 'left)\n\
         (put-text-property 11 21 'side 'right)\n\
         (undo-boundary)\n\
         (set-text-properties 5 16 '(middle t zone central))\n\
         (undo-boundary)\n\
         (let ((s5 (get-text-property 5 'side))\n\
         (m5 (get-text-property 5 'middle))\n\
         (z5 (get-text-property 5 'zone))\n\
         (s1 (get-text-property 1 'side))\n\
         (m1 (get-text-property 1 'middle))\n\
         (s17 (get-text-property 17 'side))\n\
         (m17 (get-text-property 17 'middle)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s5 m5 z5 s1 m1 s17 m17\n\
         (get-text-property 5 'side)\n\
         (get-text-property 5 'middle)\n\
         (get-text-property 5 'zone)\n\
         (get-text-property 1 'side)\n\
         (get-text-property 1 'middle)\n\
         (get-text-property 17 'side)\n\
         (get-text-property 17 'middle)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_previous_property_change_after_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ppc\")))\n\
         (with-current-buffer buf\n\
         (insert \"AABBCCDDEEFFGGHH\")\n\
         (put-text-property 1 3 'pair 1)\n\
         (put-text-property 3 5 'pair 2)\n\
         (put-text-property 5 7 'pair 3)\n\
         (put-text-property 7 9 'pair 4)\n\
         (put-text-property 9 11 'pair 5)\n\
         (put-text-property 11 13 'pair 6)\n\
         (put-text-property 13 15 'pair 7)\n\
         (put-text-property 15 17 'pair 8)\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (insert \"XX\")\n\
         (put-text-property 5 7 'pair 'inserted)\n\
         (undo-boundary)\n\
         (let ((prev-from-end (previous-single-property-change 17 'pair))\n\
         (prev-from-7 (previous-single-property-change 7 'pair)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (let ((prev-after-undo (previous-single-property-change 17 'pair))\n\
         (prev-after-undo-7 (previous-single-property-change 7 'pair)))\n\
         (list prev-from-end prev-from-7 prev-after-undo prev-after-undo-7\n\
         (buffer-string)\n\
         (get-text-property 5 'pair))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_propertize_sticky_front_rear_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments put-text-property 3)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"sfr\")))\n\
         (with-current-buffer buf\n\
         (insert \"FRONTMIDDLETAIL\")\n\
         (put-text-property 1 6 'pos 'front)\n\
         (put-text-property 6 12 'pos 'middle)\n\
         (put-text-property 12 16 'pos 'tail)\n\
         (put-text-property 6 'rear-nonsticky '(pos))\n\
         (put-text-property 12 'rear-nonsticky '(pos))\n\
         (undo-boundary)\n\
         (goto-char 6)\n\
         (insert \"INSERT\")\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (p6 (get-text-property 6 'pos))\n\
         (p12 (get-text-property 12 'pos))\n\
         (rn6 (get-text-property 6 'rear-nonsticky))\n\
         (rn12 (get-text-property 12 'rear-nonsticky)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s p6 p12 rn6 rn12\n\
         (buffer-string)\n\
         (get-text-property 6 'pos)\n\
         (get-text-property 12 'pos)\n\
         (get-text-property 6 'rear-nonsticky)\n\
         (get-text-property 12 'rear-nonsticky)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_many_small_prop_intervals_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"msp\")))\n\
         (with-current-buffer buf\n\
         (dotimes (i 20)\n\
         (insert (format \"%c\" (+ 65 i))))\n\
         (dotimes (i 20)\n\
         (let ((pos (1+ i)))\n\
         (put-text-property pos (1+ pos) 'char-code (+ 65 i))))\n\
         (let ((before\n\
         (cl-loop for i from 1 to 20\n\
         collect (get-text-property i 'char-code))))\n\
         (undo-boundary)\n\
         (goto-char 5)\n\
         (delete-region 5 10)\n\
         (undo-boundary)\n\
         (goto-char 12)\n\
         (insert \"ZZZZ\")\n\
         (undo-boundary)\n\
         (let ((after-ops\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'char-code))))\n\
         (primitive-undo 2 buffer-undo-list)\n\
         (let ((after-undo\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         collect (get-text-property i 'char-code))))\n\
         (list before after-ops after-undo (buffer-string))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

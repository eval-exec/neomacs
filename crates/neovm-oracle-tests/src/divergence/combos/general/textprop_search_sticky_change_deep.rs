//! Deep combo: text-property search + next/previous-single-property-change + stickiness.
//! Tests property boundary traversal and insertion behavior with stickiness.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_next_single_property_change_across_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"nsp\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAABBBBCCCCDDDD\")\n\
         (put-text-property 1 5 'zone 1)\n\
         (put-text-property 5 9 'zone 2)\n\
         (put-text-property 9 13 'zone 3)\n\
         (put-text-property 13 17 'zone 4)\n\
         (list (next-single-property-change 1 'zone)\n\
         (next-single-property-change 5 'zone)\n\
         (next-single-property-change 9 'zone)\n\
         (next-single-property-change 13 'zone)\n\
         (next-single-property-change 14 'zone)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_previous_single_property_change_backwards() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"psp\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAABBBBCCCCDDDD\")\n\
         (put-text-property 1 5 'zone 1)\n\
         (put-text-property 5 9 'zone 2)\n\
         (put-text-property 9 13 'zone 3)\n\
         (put-text-property 13 17 'zone 4)\n\
         (list (previous-single-property-change 16 'zone)\n\
         (previous-single-property-change 12 'zone)\n\
         (previous-single-property-change 8 'zone)\n\
         (previous-single-property-change 4 'zone)\n\
         (previous-single-property-change 1 'zone)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_text_property_stickiness_front_rear() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"stk\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGH\")\n\
         (put-text-property 3 6 'sticky 'front)\n\
         (goto-char 3)\n\
         (insert \"X\")\n\
         (let ((after-insert (get-text-property 3 'sticky)))\n\
         (goto-char 7)\n\
         (insert \"Y\")\n\
         (list after-insert\n\
         (get-text-property 3 'sticky)\n\
         (get-text-property 4 'sticky)\n\
         (get-text-property 7 'sticky)\n\
         (get-text-property 8 'sticky))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_next_property_change_with_nil_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"npn\")))\n\
         (with-current-buffer buf\n\
         (insert \"aaaBBBcccDDD\")\n\
         (put-text-property 4 7 'face 'bold)\n\
         (put-text-property 10 13 'face 'italic)\n\
         (list (next-single-property-change 1 'face)\n\
         (next-single-property-change 7 'face)\n\
         (next-single-property-change 1 'face buf)\n\
         (next-single-property-change 7 'face buf)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_get_text_property_at_boundary_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"gtp\")))\n\
         (with-current-buffer buf\n\
         (insert \"AABBCCDD\")\n\
         (put-text-property 1 3 'grp 'a)\n\
         (put-text-property 3 5 'grp 'b)\n\
         (put-text-property 5 7 'grp 'c)\n\
         (put-text-property 7 9 'grp 'd)\n\
         (list (get-text-property 1 'grp)\n\
         (get-text-property 2 'grp)\n\
         (get-text-property 3 'grp)\n\
         (get-text-property 4 'grp)\n\
         (get-text-property 5 'grp)\n\
         (get-text-property 6 'grp)\n\
         (get-text-property 7 'grp)\n\
         (get-text-property 8 'grp)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_text_property_not_all_with_nested_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"tpn\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAAAAAAA\")\n\
         (put-text-property 1 10 'level 1)\n\
         (put-text-property 3 7 'level 2)\n\
         (put-text-property 4 6 'level 3)\n\
         (list (text-property-not-all 1 10 'level 1)\n\
         (text-property-not-all 1 10 'level 2)\n\
         (text-property-not-all 1 3 'level 1)\n\
         (text-property-not-all 4 6 'level 3)\n\
         (text-property-any 1 10 'level 2)\n\
         (text-property-any 1 10 'level 3)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_add_text_properties_merges_intervals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"atp\")))\n\
         (with-current-buffer buf\n\
         (insert \"XXXXXXXXXXX\")\n\
         (add-text-properties 1 6 '(face bold color red))\n\
         (add-text-properties 4 11 '(face italic size large))\n\
         (list (get-text-property 1 'face)\n\
         (get-text-property 3 'face)\n\
         (get-text-property 5 'face)\n\
         (get-text-property 8 'face)\n\
         (get-text-property 1 'color)\n\
         (get-text-property 5 'color)\n\
         (get-text-property 8 'color)\n\
         (get-text-property 1 'size)\n\
         (get-text-property 5 'size)\n\
         (get-text-property 8 'size)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_remove_text_properties_partial() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"rtp\")))\n\
         (with-current-buffer buf\n\
         (insert \"HELLO-WORLD\")\n\
         (add-text-properties 1 12 '(face bold color red size large))\n\
         (remove-text-properties 4 9 '(face nil))\n\
         (list (get-text-property 1 'face)\n\
         (get-text-property 4 'face)\n\
         (get-text-property 6 'face)\n\
         (get-text-property 10 'face)\n\
         (get-text-property 1 'color)\n\
         (get-text-property 6 'color)\n\
         (get-text-property 6 'size)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_set_text_properties_replaces_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"stp\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAAAAAAAA\")\n\
         (add-text-properties 1 11 '(face bold color red))\n\
         (set-text-properties 4 8 '(face italic))\n\
         (list (get-text-property 1 'face)\n\
         (get-text-property 1 'color)\n\
         (get-text-property 4 'face)\n\
         (get-text-property 4 'color)\n\
         (get-text-property 7 'face)\n\
         (get-text-property 9 'face)\n\
         (get-text-property 9 'color)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_property_search_with_overlay_and_prop_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"poi\")))\n\
         (with-current-buffer buf\n\
         (insert \"AABBCCDDEE\")\n\
         (put-text-property 1 6 'src 'text)\n\
         (let ((ov (make-overlay 3 8)))\n\
         (overlay-put ov 'src 'overlay))\n\
         (list (get-text-property 1 'src)\n\
         (get-text-property 3 'src)\n\
         (get-text-property 5 'src)\n\
         (get-text-property 7 'src)\n\
         (get-text-property 9 'src)\n\
         (next-single-property-change 1 'src)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

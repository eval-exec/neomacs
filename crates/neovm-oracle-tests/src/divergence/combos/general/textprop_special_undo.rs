//! Deep stress: keymap + category + rear-nonsticky + button + text-prop undo combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_text_prop_keymap_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"tpk\"))\n\
         (map (make-sparse-keymap)))\n\
         (define-key map \"a\" '(lambda () (interactive) (insert \"A\")))\n\
         (with-current-buffer buf\n\
         (insert \"click here for action\")\n\
         (put-text-property 1 21 'keymap map)\n\
         (put-text-property 1 6 'face 'link)\n\
         (put-text-property 7 11 'face 'default)\n\
         (put-text-property 12 21 'face 'bold)\n\
         (undo-boundary)\n\
         (goto-char 7)\n\
         (insert \"BUTTON\")\n\
         (put-text-property 7 13 'face 'highlight)\n\
         (undo-boundary)\n\
         (let ((km (get-text-property 1 'keymap))\n\
         (f1 (get-text-property 1 'face))\n\
         (f7 (get-text-property 7 'face)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list (and km t)\n\
         f1 f7\n\
         (buffer-string)\n\
         (and (get-text-property 1 'keymap) t)\n\
         (get-text-property 1 'face)\n\
         (get-text-property 7 'face)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_category_properties_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"cpu\")))\n\
         (with-current-buffer buf\n\
         (insert \"ABCDEFGH\")\n\
         (let ((cat-1 (make-char-table 'category-table nil))\n\
         (cat-2 (make-char-table 'category-table nil)))\n\
         (put-text-property 1 5 'category cat-1)\n\
         (put-text-property 5 9 'category cat-2)\n\
         (undo-boundary)\n\
         (goto-char 3)\n\
         (insert \"XXX\")\n\
         (put-text-property 3 6 'category cat-2)\n\
         (undo-boundary)\n\
         (let ((c1 (get-text-property 1 'category))\n\
         (c3 (get-text-property 3 'category))\n\
         (c6 (get-text-property 6 'category))\n\
         (c8 (get-text-property 8 'category)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list c1 c3 c6 c8\n\
         (buffer-string)\n\
         (get-text-property 1 'category)\n\
         (get-text-property 3 'category)\n\
         (get-text-property 5 'category)\n\
         (get-text-property 8 'category))))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_button_type_props_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 27 33)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"btp\")))\n\
         (with-current-buffer buf\n\
         (insert \"[Click Me] for more info [Help]\")\n\
         (put-text-property 1 11 'button t)\n\
         (put-text-property 1 11 'face 'link)\n\
         (put-text-property 12 26 'face 'default)\n\
         (put-text-property 27 33 'button t)\n\
         (put-text-property 27 33 'face 'link)\n\
         (undo-boundary)\n\
         (goto-char 12)\n\
         (insert \"EXPANDED \")\n\
         (put-text-property 12 21 'face 'highlight)\n\
         (undo-boundary)\n\
         (let ((b1 (get-text-property 1 'button))\n\
         (f1 (get-text-property 1 'face))\n\
         (f12 (get-text-property 12 'face))\n\
         (b27 (get-text-property 27 'button)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list b1 f1 f12 b27\n\
         (buffer-string)\n\
         (get-text-property 1 'button)\n\
         (get-text-property 1 'face)\n\
         (get-text-property 12 'face)\n\
         (get-text-property 27 'button)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_field_property_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"fpu\")))\n\
         (with-current-buffer buf\n\
         (insert \"Name: ________ Age: __ City: ________\")\n\
         (put-text-property 1 5 'field 'label)\n\
         (put-text-property 7 14 'field 'name-input)\n\
         (put-text-property 15 18 'field 'label)\n\
         (put-text-property 20 22 'field 'age-input)\n\
         (put-text-property 23 28 'field 'label)\n\
         (put-text-property 30 37 'field 'city-input)\n\
         (undo-boundary)\n\
         (goto-char 7)\n\
         (delete-region 7 14)\n\
         (insert \"ALICE\")\n\
         (put-text-property 7 12 'field 'name-input)\n\
         (undo-boundary)\n\
         (let ((s (buffer-string))\n\
         (f7 (get-text-property 7 'field))\n\
         (f1 (get-text-property 1 'field)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list s f7 f1\n\
         (buffer-string)\n\
         (get-text-property 7 'field)\n\
         (get-text-property 1 'field)\n\
         (get-text-property 15 'field)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_invisible_property_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"ipu\")))\n\
         (with-current-buffer buf\n\
         (insert \"SHOW1hide1SHOW2hide2SHOW3hide3\")\n\
         (put-text-property 1 6 'invisible nil)\n\
         (put-text-property 6 11 'invisible 'hidden)\n\
         (put-text-property 11 16 'invisible nil)\n\
         (put-text-property 16 21 'invisible 'hidden)\n\
         (put-text-property 21 26 'invisible nil)\n\
         (put-text-property 26 31 'invisible 'hidden)\n\
         (undo-boundary)\n\
         (remove-text-properties 6 21 '(invisible nil))\n\
         (undo-boundary)\n\
         (let ((i6 (get-text-property 6 'invisible))\n\
         (i16 (get-text-property 16 'invisible))\n\
         (i26 (get-text-property 26 'invisible)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list i6 i16 i26\n\
         (buffer-string)\n\
         (get-text-property 6 'invisible)\n\
         (get-text-property 16 'invisible)\n\
         (get-text-property 26 'invisible)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_intangible_property_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"itp\")))\n\
         (with-current-buffer buf\n\
         (insert \"BEFORE[SKIP]AFTER[SKIP]END\")\n\
         (put-text-property 1 6 'intangible nil)\n\
         (put-text-property 7 11 'intangible t)\n\
         (put-text-property 12 17 'intangible nil)\n\
         (put-text-property 18 22 'intangible t)\n\
         (put-text-property 23 26 'intangible nil)\n\
         (undo-boundary)\n\
         (goto-char 7)\n\
         (delete-region 7 11)\n\
         (insert \"KEEP\")\n\
         (put-text-property 7 11 'intangible nil)\n\
         (undo-boundary)\n\
         (let ((i7 (get-text-property 7 'intangible))\n\
         (i12 (get-text-property 12 'intangible)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list i7 i12\n\
         (buffer-string)\n\
         (get-text-property 7 'intangible)\n\
         (get-text-property 12 'intangible)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_mouse_face_property_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mfu\")))\n\
         (with-current-buffer buf\n\
         (insert \"item1 item2 item3 item4\")\n\
         (put-text-property 1 6 'mouse-face 'highlight)\n\
         (put-text-property 7 12 'mouse-face 'highlight)\n\
         (put-text-property 13 18 'mouse-face 'highlight)\n\
         (put-text-property 19 24 'mouse-face 'highlight)\n\
         (put-text-property 1 24 'help-echo \"hover text\")\n\
         (undo-boundary)\n\
         (goto-char 7)\n\
         (delete-region 7 12)\n\
         (insert \"NEW\")\n\
         (put-text-property 7 10 'mouse-face 'bold)\n\
         (undo-boundary)\n\
         (let ((m1 (get-text-property 1 'mouse-face))\n\
         (m7 (get-text-property 7 'mouse-face))\n\
         (h1 (get-text-property 1 'help-echo)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list m1 m7 h1\n\
         (buffer-string)\n\
         (get-text-property 1 'mouse-face)\n\
         (get-text-property 7 'mouse-face)\n\
         (get-text-property 1 'help-echo)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_local_map_property_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 17 25)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"lmu\"))\n\
         (map1 (make-sparse-keymap))\n\
         (map2 (make-sparse-keymap)))\n\
         (define-key map1 \"x\" '(lambda () (interactive) (insert \"X1\")))\n\
         (define-key map2 \"y\" '(lambda () (interactive) (insert \"Y2\")))\n\
         (with-current-buffer buf\n\
         (insert \"REGION1 REGION2 REGION3\")\n\
         (put-text-property 1 9 'local-map map1)\n\
         (put-text-property 9 17 'local-map map2)\n\
         (put-text-property 17 25 'local-map map1)\n\
         (undo-boundary)\n\
         (goto-char 9)\n\
         (insert \"INSERTED\")\n\
         (put-text-property 9 17 'local-map map2)\n\
         (undo-boundary)\n\
         (let ((lm1 (get-text-property 1 'local-map))\n\
         (lm9 (get-text-property 9 'local-map))\n\
         (lm17 (get-text-property 17 'local-map)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list (and lm1 t) (and lm9 t) (and lm17 t)\n\
         (buffer-string)\n\
         (and (get-text-property 1 'local-map) t)\n\
         (and (get-text-property 9 'local-map) t)\n\
         (and (get-text-property 17 'local-map) t)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_multiple_prop_set_undo_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mps\")))\n\
         (with-current-buffer buf\n\
         (insert \"AAAAAAAAAAAA\")\n\
         (undo-boundary)\n\
         (put-text-property 1 13 'prop-a 'val-a)\n\
         (undo-boundary)\n\
         (put-text-property 1 13 'prop-b 'val-b)\n\
         (undo-boundary)\n\
         (put-text-property 1 7 'prop-c 'val-c)\n\
         (undo-boundary)\n\
         (put-text-property 7 13 'prop-d 'val-d)\n\
         (undo-boundary)\n\
         (let ((pa (get-text-property 1 'prop-a))\n\
         (pb (get-text-property 1 'prop-b))\n\
         (pc (get-text-property 1 'prop-c))\n\
         (pd7 (get-text-property 7 'prop-d)))\n\
         (primitive-undo 4 buffer-undo-list)\n\
         (list pa pb pc pd7\n\
         (buffer-string)\n\
         (get-text-property 1 'prop-a)\n\
         (get-text-property 1 'prop-b)\n\
         (get-text-property 1 'prop-c)\n\
         (get-text-property 7 'prop-d)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_rear_nonsticky_insert_gap_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"rng\")))\n\
         (with-current-buffer buf\n\
         (insert \"FRONTMIDDLEREAR\")\n\
         (put-text-property 1 6 'zone 'front)\n\
         (put-text-property 6 6 'rear-nonsticky '(zone))\n\
         (put-text-property 6 12 'zone 'middle)\n\
         (put-text-property 12 12 'rear-nonsticky '(zone))\n\
         (put-text-property 12 16 'zone 'rear)\n\
         (undo-boundary)\n\
         (goto-char 6)\n\
         (insert \"GAP\")\n\
         (undo-boundary)\n\
         (let ((z1 (get-text-property 1 'zone))\n\
         (z6 (get-text-property 6 'zone))\n\
         (z9 (get-text-property 9 'zone))\n\
         (z12 (get-text-property 12 'zone))\n\
         (rn6 (get-text-property 6 'rear-nonsticky))\n\
         (rn9 (get-text-property 9 'rear-nonsticky)))\n\
         (primitive-undo 1 buffer-undo-list)\n\
         (list z1 z6 z9 z12 rn6 rn9\n\
         (buffer-string)\n\
         (get-text-property 1 'zone)\n\
         (get-text-property 6 'zone)\n\
         (get-text-property 6 'rear-nonsticky)\n\
         (get-text-property 12 'zone)\n\
         (get-text-property 12 'rear-nonsticky)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

//! Deep combo: keymap + define-key + lookup-key + where-is-internal + command properties.
//! Tests keymap resolution, inheritance, and command introspection.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_make_keymap_define_and_lookup_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (insert-a insert-b nil (keymap (24 . combo-command)) combo-command)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((map (make-keymap)))\n\
         (define-key map \"a\" 'insert-a)\n\
         (define-key map \"b\" 'insert-b)\n\
         (define-key map \"\\C-c\\C-x\" 'combo-command)\n\
         (list (lookup-key map \"a\")\n\
         (lookup-key map \"b\")\n\
         (lookup-key map \"c\")\n\
         (lookup-key map \"\\C-c\")\n\
         (lookup-key map \"\\C-c\\C-x\"))))",
        expect,
    );
}

#[test]
fn deficiency_make_sparse_keymap_with_parent_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (child-a parent-b child-c nil (keymap (98 . parent-b) (97 . parent-a)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((parent (make-sparse-keymap)))\n\
         (define-key parent \"a\" 'parent-a)\n\
         (define-key parent \"b\" 'parent-b)\n\
         (let ((child (make-sparse-keymap)))\n\
         (set-keymap-parent child parent)\n\
         (define-key child \"a\" 'child-a)\n\
         (define-key child \"c\" 'child-c)\n\
         (list (lookup-key child \"a\")\n\
         (lookup-key child \"b\")\n\
         (lookup-key child \"c\")\n\
         (lookup-key child \"d\")\n\
         (keymap-parent child)))))",
        expect,
    );
}

#[test]
fn deficiency_keymap_prefix_key_and_nested_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((map (make-sparse-keymap)))\n\
         (let ((prefix-map (make-sparse-keymap)))\n\
         (define-key prefix-map \"a\" 'prefix-a)\n\
         (define-key prefix-map \"b\" 'prefix-b)\n\
         (define-key map \"\\C-c\" prefix-map)\n\
         (list (lookup-key map \"\\C-c\")\n\
         (lookup-key map \"\\C-ca\")\n\
         (lookup-key map \"\\C-cb\")\n\
         (keymapp (lookup-key map \"\\C-c\"))\n\
         (keymapp (lookup-key map \"\\C-ca\")))))",
        expect,
    );
}

#[test]
fn deficiency_keymap_meta_and_function_key_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (self-a meta-a ctrl-a help-fn meta-f2 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((map (make-sparse-keymap)))\n\
         (define-key map [?a] 'self-a)\n\
         (define-key map [\\M-a] 'meta-a)\n\
         (define-key map [\\C-a] 'ctrl-a)\n\
         (define-key map [f1] 'help-fn)\n\
         (define-key map [M-f2] 'meta-f2)\n\
         (list (lookup-key map [?a])\n\
         (lookup-key map [\\M-a])\n\
         (lookup-key map [\\C-a])\n\
         (lookup-key map [f1])\n\
         (lookup-key map [M-f2])\n\
         (lookup-key map [f3]))))",
        expect,
    );
}

#[test]
fn deficiency_define_key_with_remap_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((map (make-sparse-keymap)))\n\
         (define-key map [remap self-insert-command] 'my-insert)\n\
         (list (lookup-key map [remap self-insert-command])\n\
         (lookup-key map \"a\")))",
        expect,
    );
}

#[test]
fn deficiency_keymap_where_is_internal_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (([3 97]) ([3 98]) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((map (make-sparse-keymap)))\n\
         (define-key map \"\\C-ca\" 'test-cmd-a)\n\
         (define-key map \"\\C-cb\" 'test-cmd-b)\n\
         (define-key map \"\\C-x\\C-f\" 'test-find-file)\n\
         (let ((seqs (where-is-internal 'test-cmd-a map)))\n\
         (list seqs\n\
         (where-is-internal 'test-cmd-b map)\n\
         (where-is-internal 'test-cmd-a (make-sparse-keymap))))))",
        expect,
    );
}

#[test]
fn deficiency_copy_keymap_deep_vs_shallow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (cmd-a cmd-a-modified cmd-b cmd-b)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((orig (make-sparse-keymap)))\n\
         (define-key orig \"a\" 'cmd-a)\n\
         (define-key orig \"b\" 'cmd-b)\n\
         (let ((copy (copy-keymap orig)))\n\
         (define-key copy \"a\" 'cmd-a-modified)\n\
         (list (lookup-key orig \"a\")\n\
         (lookup-key copy \"a\")\n\
         (lookup-key orig \"b\")\n\
         (lookup-key copy \"b\")))))",
        expect,
    );
}

#[test]
fn deficiency_minor_mode_map_alist_keymap_precedence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (from-map1 from-map2 nil from-map2-b)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((map1 (make-sparse-keymap))\n\
         (map2 (make-sparse-keymap)))\n\
         (define-key map1 \"a\" 'from-map1)\n\
         (define-key map2 \"a\" 'from-map2)\n\
         (define-key map2 \"b\" 'from-map2-b)\n\
         (let ((minor-mode-map-alist (list (cons 'mode2 map2)\n\
         (cons 'mode1 map1))))\n\
         (list (lookup-key map1 \"a\")\n\
         (lookup-key map2 \"a\")\n\
         (lookup-key map1 \"b\")\n\
         (lookup-key map2 \"b\")))))",
        expect,
    );
}

#[test]
fn deficiency_keymap_unbind_and_rebind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((first second third) first nil third-modified)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((map (make-sparse-keymap)))\n\
         (define-key map \"a\" 'first)\n\
         (define-key map \"b\" 'second)\n\
         (define-key map \"c\" 'third)\n\
         (let ((bindings (list (lookup-key map \"a\")\n\
         (lookup-key map \"b\")\n\
         (lookup-key map \"c\"))))\n\
         (define-key map \"b\" nil)\n\
         (define-key map \"c\" 'third-modified)\n\
         (list bindings\n\
         (lookup-key map \"a\")\n\
         (lookup-key map \"b\")\n\
         (lookup-key map \"c\")))))",
        expect,
    );
}

#[test]
fn deficiency_key_description_and_key_vector_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"C-c C-x a\" \"M-x\" \"C-M-a\" 3 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((k1 (kbd \"C-c C-x a\"))\n\
         (k2 (kbd \"M-x\"))\n\
         (k3 (kbd \"C-M-a\")))\n\
         (list (key-description k1)\n\
         (key-description k2)\n\
         (key-description k3)\n\
         (length k1)\n\
         (length k2)\n\
         (length k3))))",
        expect,
    );
}

//! Oracle parity for keymap operations: `define-key`, `lookup-key`,
//! `keymapp`, `make-sparse-keymap`, `make-keymap`, `keymap-parent`.
//!
//! GNU src/keymap.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_keymapp_on_keymap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(keymapp (make-sparse-keymap))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_keymapp_on_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(keymapp nil)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_make_sparse_keymap_creates_keymap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(keymapp (make-sparse-keymap))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_make_keymap_creates_keymap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(keymapp (make-keymap))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_lookup_key_undefined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(lookup-key (make-sparse-keymap) "a")"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_define_key_and_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (let ((km (make-sparse-keymap))) (define-key km "a" 'forward-char) (commandp (lookup-key km "a"))))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_keymap_parent_default_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(keymap-parent (make-sparse-keymap))"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_set_keymap_parent_returns_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (let ((child (make-sparse-keymap)) (parent (make-sparse-keymap))) (set-keymap-parent child parent) (eq parent (keymap-parent child))))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_define_key_on_composed_keymap_mutates_first_component() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (image-next-line nil image-next-line)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(let* ((parent (make-sparse-keymap))
                 (child (make-sparse-keymap))
                 (composed (make-composed-keymap child parent)))
            (define-key parent [remap evil-append] 'ignore)
            (define-key composed [remap evil-next-line] 'image-next-line)
            (list (lookup-key child [remap evil-next-line])
                  (lookup-key parent [remap evil-next-line])
                  (lookup-key composed [remap evil-next-line])))"#,
        expect,
    );
    assert_ok_eq("(image-next-line nil image-next-line)", &o, &n);
}

#[test]
fn oracle_define_key_on_inherited_composed_prefix_mutates_child_aux() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (doc-next-line image-next-line doc-next-line)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(let* ((special (make-sparse-keymap))
                 (special-aux (make-sparse-keymap "Auxiliary keymap for Normal state"))
                 (image (make-sparse-keymap))
                 (doc (make-sparse-keymap))
                 (new (make-sparse-keymap "Auxiliary keymap for Normal state"))
                 (key [normal-state]))
            (define-key special key special-aux)
            (define-key special-aux [remap evil-next-line] 'image-next-line)
            (set-keymap-parent image special)
            (set-keymap-parent doc image)
            (define-key doc [menu-bar docview]
              (list 'menu-item "DocView" (make-sparse-keymap)))
            (define-key doc key new)
            (let ((aux (lookup-key doc key)))
              (define-key aux [remap evil-next-line] 'doc-next-line)
              (list (lookup-key new [remap evil-next-line])
                    (lookup-key special-aux [remap evil-next-line])
                    (lookup-key aux [remap evil-next-line]))))"#,
        expect,
    );
    assert_ok_eq("(doc-next-line image-next-line doc-next-line)", &o, &n);
}

#[test]
fn oracle_define_key_character_range_binds_each_character() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (range-cmd range-cmd range-cmd nil)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(let ((map (make-sparse-keymap)))
            (define-key map [(?a . ?c)] 'range-cmd)
            (list (lookup-key map [?a])
                  (lookup-key map [?b])
                  (lookup-key map [?c])
                  (lookup-key map [?d])))"#,
        expect,
    );
    assert_ok_eq("(range-cmd range-cmd range-cmd nil)", &o, &n);
}

#[test]
fn oracle_define_key_character_range_replaces_existing_char_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (range-cmd range-cmd range-cmd nil)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(let ((map (make-sparse-keymap)))
            (define-key map [?a] 'old-a)
            (define-key map [?b] 'old-b)
            (define-key map [(?a . ?c)] 'range-cmd)
            (list (lookup-key map [?a])
                  (lookup-key map [?b])
                  (lookup-key map [?c])
                  (lookup-key map [?d])))"#,
        expect,
    );
    assert_ok_eq("(range-cmd range-cmd range-cmd nil)", &o, &n);
}

#[test]
fn oracle_define_key_remove_preserves_neighbor_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (a-cmd nil c-cmd)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(let ((map (make-sparse-keymap)))
            (define-key map "a" 'a-cmd)
            (define-key map "b" 'b-cmd)
            (define-key map "c" 'c-cmd)
            (define-key map "b" nil t)
            (list (lookup-key map "a")
                  (lookup-key map "b")
                  (lookup-key map "c")))"#,
        expect,
    );
    assert_ok_eq("(a-cmd nil c-cmd)", &o, &n);
}

#[test]
fn oracle_keymap_symbol_modifier_order_is_canonical() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (symbol-cmd symbol-cmd)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(let ((map (make-sparse-keymap)))
            (define-key map [M-C-left] 'symbol-cmd)
            (list (lookup-key map [C-M-left])
                  (lookup-key map [M-C-left])))"#,
        expect,
    );
    assert_ok_eq("(symbol-cmd symbol-cmd)", &o, &n);
}

/// Parent filter evaluation may collect a freshly resolved child prefix.
#[test]
fn oracle_keymap_parent_filter_collection_preserves_ephemeral_child_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((child (make-sparse-keymap))
      (parent (make-sparse-keymap)))
  (define-key child [f2]
    '(menu-item "child" nil :filter
                (lambda (_)
                  (let ((prefix (make-sparse-keymap)))
                    (define-key prefix [?q] 'neovm--child-filter-command)
                    prefix))))
  (define-key parent [f2]
    '(menu-item "parent" nil :filter
                (lambda (_)
                  (make-list 32 nil)
                  (garbage-collect)
                  (let ((prefix (make-sparse-keymap)))
                    (define-key prefix [?r] 'neovm--parent-filter-command)
                    prefix))))
  (set-keymap-parent child parent)
  (list (lookup-key child [f2 ?q])
        (lookup-key child [f2 ?r])))
"#;
    let expect = expect_test::expect![[
        r#""OK (neovm--child-filter-command neovm--parent-filter-command)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// A later composed member's filter may collect an earlier prefix result.
#[test]
fn oracle_keymap_composed_filter_collection_preserves_ephemeral_member_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((child (make-sparse-keymap))
      (parent (make-sparse-keymap)))
  (define-key child [f2]
    '(menu-item "child" nil :filter
                (lambda (_)
                  (let ((prefix (make-sparse-keymap)))
                    (define-key prefix [?q] 'neovm--child-filter-command)
                    prefix))))
  (define-key parent [f2]
    '(menu-item "parent" nil :filter
                (lambda (_)
                  (make-list 32 nil)
                  (garbage-collect)
                  (let ((prefix (make-sparse-keymap)))
                    (define-key prefix [?r] 'neovm--parent-filter-command)
                    prefix))))
  (let ((composed (make-composed-keymap (list child parent))))
    (list (lookup-key composed [f2 ?q])
          (lookup-key composed [f2 ?r]))))
"#;
    let expect = expect_test::expect![[
        r#""OK (neovm--child-filter-command neovm--parent-filter-command)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

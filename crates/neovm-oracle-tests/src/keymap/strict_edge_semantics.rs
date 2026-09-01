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

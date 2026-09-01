//! Oracle parity for use-global-map, use-local-map, buffer-swap-text.
//! GNU src/keymap.c, src/buffer.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

// --- use-global-map ---

#[test]
fn oracle_use_global_map_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(use-global-map (current-global-map))"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_use_global_map_keeps_global_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (use-global-map (current-global-map)) (keymapp (current-global-map)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_use_global_map_nil_signals_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"wrong-type-argument\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(condition-case err (progn (use-global-map nil) nil) (error (symbol-name (car err))))"#,
        expect,
    );
    assert_ok_eq("\"wrong-type-argument\"", &o, &n);
}

// --- use-local-map ---

#[test]
fn oracle_use_local_map_nil_allowed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(use-local-map nil)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_use_local_map_sets_and_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (use-local-map (make-sparse-keymap)) nil)"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_use_local_map_sets_local_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (use-local-map (make-sparse-keymap)) (keymapp (current-local-map)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_use_local_map_non_keymap_signals_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"wrong-type-argument\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(condition-case err (progn (use-local-map 42) nil) (error (symbol-name (car err))))"#,
        expect,
    );
    assert_ok_eq("\"wrong-type-argument\"", &o, &n);
}

// --- buffer-swap-text ---

#[test]
fn oracle_buffer_swap_text_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    // Create two buffers, swap text between them
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq b1 (get-buffer-create "*swap-src*")) (set-buffer b1) (erase-buffer) (insert "hello") (setq b2 (get-buffer-create "*swap-dst*")) (set-buffer b2) (erase-buffer) (insert "world") (buffer-swap-text b1))"#,
        expect,
    );
    // Return value is always nil
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_buffer_swap_text_swaps_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    // After swap, the other buffer's text is now in b2
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq b1 (get-buffer-create "*swap-src2*")) (set-buffer b1) (erase-buffer) (insert "hello") (setq b2 (get-buffer-create "*swap-dst2*")) (set-buffer b2) (erase-buffer) (insert "world") (buffer-swap-text b1) (buffer-string))"#,
        expect,
    );
    // b2 now has b1's old content "hello"
    assert_ok_eq("\"hello\"", &o, &n);
}

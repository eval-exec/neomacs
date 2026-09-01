//! Strict combo oracle probes, batch 80: keymap operations — define-prefix-
//! command, make-composed-keymap, key-binding/global/local-key-binding.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_p4_prefix_command_and_key_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (self-insert-command probe-prefix t bar)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((map (make-sparse-keymap)))
  (define-key map "a" 'foo)
  (define-prefix-command 'probe-prefix)
  (define-key map "\C-c" 'probe-prefix)
  (define-key probe-prefix "b" 'bar)
  (list (key-binding "a" nil nil (list map))
        (lookup-key map "\C-c")
        (keymapp (lookup-key map "\C-c"))
        (lookup-key (lookup-key map "\C-c") "b")))
"##,
        expect,
    );
}

#[test]
fn div_p4_composed_keymap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (from-parent from-child t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((parent (make-sparse-keymap))
      (child (make-sparse-keymap)))
  (define-key parent "a" 'from-parent)
  (define-key child "b" 'from-child)
  (let ((composed (make-composed-keymap child parent)))
    (list (lookup-key composed "a")
          (lookup-key composed "b")
          (keymapp composed))))
"##,
        expect,
    );
}

#[test]
fn div_p4_global_local_key_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (find-file keyboard-quit t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (global-key-binding "\C-x\C-f")
      (global-key-binding "\C-g")
      (not (null (global-key-binding "\C-x")))
      (eq (key-binding "\C-g") 'keyboard-quit))
"##,
        expect,
    );
}

#[test]
fn div_p4_keymap_parent_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (m1-x m2-y t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((m1 (make-sparse-keymap))
      (m2 (make-sparse-keymap)))
  (define-key m1 "x" 'm1-x)
  (define-key m2 "y" 'm2-y)
  (set-keymap-parent m1 m2)
  (list (lookup-key m1 "x")
        (lookup-key m1 "y")
        (eq (keymap-parent m1) m2)))
"##,
        expect,
    );
}

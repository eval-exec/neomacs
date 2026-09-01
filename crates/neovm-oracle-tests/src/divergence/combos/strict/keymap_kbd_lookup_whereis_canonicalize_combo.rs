//! Strict combo oracle probes, batch 156: keymap internals. define-key with
//! kbd string and raw vector forms, lookup-key partial-prefix vs exact-miss
//! (returns int depth or nil), where-is-internal, key-binding, current-active-
//! maps lookup order, and keymap-prompt / keymap-canonicalize.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_keymap_define_lookup_kbd_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((map (make-sparse-keymap)))
  (define-key map (kbd "C-c C-a") 'cmd-a)
  (define-key map (kbd "C-c C-b") 'cmd-b)
  (define-key map (kbd "M-x") 'extended)
  (define-key map [f5] 'refresh)
  (define-key map (kbd "<f6>") 'f6-cmd)
  (list (lookup-key map (kbd "C-c C-a"))
        (lookup-key map "\C-c\C-a")
        (lookup-key map (kbd "C-c C-c"))
        (lookup-key map (kbd "C-c"))
        (lookup-key map [f5])
        (lookup-key map (kbd "<f6>"))
        (lookup-key map [f7])
        (eq (lookup-key map (kbd "M-x")) 'extended)))
"##;
    let expect = expect_test::expect![[
        r#""OK (cmd-a cmd-a nil (keymap (2 . cmd-b) (1 . cmd-a)) refresh f6-cmd nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_whereis_internal_key_binding_active_maps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((map (make-sparse-keymap)))
  (define-key map (kbd "C-c C-a") 'probe-whereis)
  (define-key map (kbd "C-c C-a C-d") 'probe-deeper)
  (define-key map [?\C-c ?\C-b] 'probe-cb)
  (list (where-is-internal 'probe-whereis map)
        (where-is-internal 'probe-deeper map)
        (where-is-internal 'undefined map)
        (key-binding 'probe-whereis)
        (eq (lookup-key map (kbd "C-c C-a C-d")) 'probe-deeper)
        (lookup-key map (kbd "C-c C-a C-d"))
        (let ((active (current-active-maps)))
          (consp active))
        (keymapp map)
        (keymapp 'not-a-map)))
"##;
    let expect = expect_test::expect![[
        r#""ERR (error \"Key sequence C-c C-a C-d starts with non-prefix key C-c C-a\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_keymap_prefix_text_parent_keymap_canonicalize() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((parent (make-sparse-keymap))
       (child (make-keymap)))
  (define-key parent "a" 'parent-a)
  (define-key child "b" 'child-b)
  (set-keymap-parent child parent)
  (list (lookup-key child "a")
        (lookup-key child "b")
        (eq (keymap-parent child) parent)
        (lookup-key parent "b")
        (keymap-prompt child)
        (let ((c2 (make-composed-keymap child nil)))
          (lookup-key c2 "b"))
        (lookup-key child "c")
        (lookup-key child [9])
        (lookup-key parent "a")))
"##;
    let expect =
        expect_test::expect![[r#""OK (parent-a child-b t nil nil child-b nil nil parent-a)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

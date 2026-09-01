//! Divergence tests: keymap + command + mode interaction combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_keymap_inheritance_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (grandparent-cmd parent-cmd parent-only-cmd child-cmd nil t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((grandparent (make-sparse-keymap))
        (parent (make-sparse-keymap))
        (child (make-sparse-keymap)))
  (define-key grandparent "a" 'grandparent-cmd)
  (define-key grandparent "b" 'shared-cmd)
  (define-key parent "b" 'parent-cmd)
  (define-key parent "c" 'parent-only-cmd)
  (set-keymap-parent parent grandparent)
  (set-keymap-parent child parent)
  (define-key child "d" 'child-cmd)
  (list (lookup-key child "a")
        (lookup-key child "b")
        (lookup-key child "c")
        (lookup-key child "d")
        (lookup-key child "e")
        (eq (keymap-parent child) parent)
        (eq (keymap-parent parent) grandparent))) "#,
        expect,
    );
}

#[test]
fn divergence_remapped_commands() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((map (make-sparse-keymap)))
  (define-key map [remap self-insert-command] 'ignore)
  (list (command-remapping 'self-insert-command)
        (eq (command-remapping 'self-insert-command) 'ignore)
        (define-key map [remap self-insert-command] nil)
        (not (command-remapping 'self-insert-command)))) "#,
        expect,
    );
}

#[test]
fn divergence_minor_mode_keymap_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (cmd-from-map1 cmd-from-map2 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((map1 (make-sparse-keymap))
        (map2 (make-sparse-keymap)))
  (define-key map1 "x" 'cmd-from-map1)
  (define-key map2 "x" 'cmd-from-map2)
  (set (make-local-variable 'minor-mode-overriding-map-alist)
       (list (cons 'test-mode1 map1)))
  (set (make-local-variable 'minor-mode-map-alist)
       (list (cons 'test-mode2 map2)))
  (list (lookup-key map1 "x")
        (lookup-key map2 "x")
        (eq (lookup-key map1 "x") 'cmd-from-map1)
        (eq (lookup-key map2 "x") 'cmd-from-map2))) "#,
        expect,
    );
}

#[test]
fn divergence_active_keymaps_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((maps (current-active-maps)))
  (list (>= (length maps) 1)
        (cl-every #'keymapp maps)
        (memq (current-global-map) maps))) "#,
        expect,
    );
}

#[test]
fn divergence_keymap_prompt_and_binding_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 . cmd-c) cmd-a cmd-c 5 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((map (make-sparse-keymap "Test Menu")))
  (define-key map "a" 'cmd-a)
  (define-key map "b" 'cmd-b)
  (define-key map [1] 'cmd-c)
  (list (cadr map)
        (lookup-key map "a")
        (lookup-key map [1])
        (length map)
        (>= (length map) 3))) "#,
        expect,
    );
}

#[test]
fn divergence_command_loop_with_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t (interactive nil) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-interp-cmd-xxx ()
    (interactive)
    (insert "cmd-ran"))
  (list (commandp 'test-interp-cmd-xxx)
        (interactive-form 'test-interp-cmd-xxx)
        (string= (format "%S" (interactive-form 'test-interp-cmd-xxx))
                 "(interactive)"))) "#,
        expect,
    );
}

#[test]
fn divergence_describe_bindings_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((keymap (113 . save-buffers-kill-terminal) (115 . save-buffer) (120 . exchange-point-and-mark)) t exchange-point-and-mark t nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((map (make-sparse-keymap)))
  (define-key map "x" 'exchange-point-and-mark)
  (define-key map "s" 'save-buffer)
  (define-key map "q" 'save-buffers-kill-terminal)
  (use-local-map map)
  (list (current-local-map)
        (keymapp (current-local-map))
        (lookup-key (current-local-map) "x")
        (eq (lookup-key (current-local-map) "x") 'exchange-point-and-mark)
        (use-local-map nil)
        (not (current-local-map)))) "#,
        expect,
    );
}

#[test]
fn divergence_key_translation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function events-to-keys)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((events (listify-key-sequence "\C-x\C-f")))
  (list events
        (length events)
        (eq (aref (events-to-keys events) 0) ?\C-x)
        (key-description events)
        (key-description "\C-x\C-f")
        (string= (key-description "\C-x\C-f") "C-x C-f"))) "#,
        expect,
    );
}

#[test]
fn divergence_accessed_keymaps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (cmd-a cmd-b t 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((map (make-keymap)))
  (define-key map "\C-c\C-a" 'cmd-a)
  (define-key map "\C-c\C-b" 'cmd-b)
  (list (lookup-key map "\C-c\C-a")
        (lookup-key map "\C-c\C-b")
        (keymapp (lookup-key map "\C-c"))
        (length (lookup-key map "\C-c")))) "#,
        expect,
    );
}

#[test]
fn divergence_substitute_key_definitions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (new-cmd-xxx new-cmd-xxx other-cmd t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((map (make-sparse-keymap)))
  (define-key map "a" 'old-cmd-xxx)
  (define-key map "b" 'old-cmd-xxx)
  (define-key map "c" 'other-cmd)
  (substitute-key-definition 'old-cmd-xxx 'new-cmd-xxx map)
  (list (lookup-key map "a")
        (lookup-key map "b")
        (lookup-key map "c")
        (eq (lookup-key map "a") 'new-cmd-xxx)
        (eq (lookup-key map "c") 'other-cmd))) "#,
        expect,
    );
}

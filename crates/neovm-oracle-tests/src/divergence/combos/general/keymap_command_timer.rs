//! Divergence combo tests: keymap × command × timer × buffer × advice.
//!
//! Stresses keymap lookup, command remapping, advice wrapping, and
//! timer callbacks that observe or mutate keymap state. Designed to
//! find unknown divergences at the intersection of key binding lookup
//! and the command loop's this-command lifecycle.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ---------------------------------------------------------------------------
// key-binding with nested keymaps
// ---------------------------------------------------------------------------

#[test]
fn combo_key_binding_lookup_through_parent_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (p-override gp-only p-only c-only nil p-override gp-only gp-cmd)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((gp (make-sparse-keymap))
        (p  (make-sparse-keymap))
        (c  (make-sparse-keymap)))
    (define-key gp "a" 'gp-cmd)
    (define-key gp "b" 'gp-only)
    (define-key p  "a" 'p-override)
    (define-key p  "c" 'p-only)
    (define-key c  "d" 'c-only)
    (set-keymap-parent p gp)
    (set-keymap-parent c p)
    (list (lookup-key c "a")
          (lookup-key c "b")
          (lookup-key c "c")
          (lookup-key c "d")
          (lookup-key c "e")
          (lookup-key p "a")
          (lookup-key p "b")
          (lookup-key gp "a"))))"#,
        expect,
    );
}

#[test]
fn combo_key_binding_with_char_and_vector_input() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (x-cmd y-cmd ret-cmd x-cmd y-cmd)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((map (make-sparse-keymap)))
    (define-key map "x" 'x-cmd)
    (define-key map [?y] 'y-cmd)
    (define-key map [return] 'ret-cmd)
    (list (lookup-key map "x")
          (lookup-key map [?y])
          (lookup-key map [return])
          (lookup-key map (vconcat [?x]))
          (lookup-key map (vconcat [?y])))))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// key-binding and command-remapping interaction
// ---------------------------------------------------------------------------

#[test]
fn combo_key_binding_with_remap_and_command_execute() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil forward-char)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((map (make-sparse-keymap))
        (observed nil))
    (define-key map [remap ignore] 'forward-char)
    (use-global-map map)
    (setq this-command nil this-original-command nil)
    (command-execute 'ignore)
    (setq observed (list this-command this-original-command
                         (command-remapping 'ignore)))
    (use-global-map (make-sparse-keymap))
    observed))"#,
        expect,
    );
}

#[test]
fn combo_multiple_remap_layers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (backward-char goto-char nil t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((map1 (make-sparse-keymap))
        (map2 (make-sparse-keymap)))
    (define-key map1 [remap forward-char] 'backward-char)
    (define-key map2 [remap backward-char] 'goto-char)
    (use-global-map map1)
    (let ((r1 (command-remapping 'forward-char)))
      (use-global-map map2)
      (let ((r2 (command-remapping 'backward-char))
            (r3 (command-remapping 'forward-char)))
        (use-global-map (make-sparse-keymap))
        (list r1 r2 r3
              (eq r1 'backward-char)
              (eq r2 'goto-char)
              (eq r3 'backward-char))))))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// key-description and this-command-keys formatting edge cases
// ---------------------------------------------------------------------------

#[test]
fn combo_key_description_multibyte_and_special_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"C-a\" \"M-x\" \"C-M-x\" \"C-x C-f\" \"<return>\" \"<tab>\" \"<escape>\" \"<backspace>\" \"<delete>\" \"C-x C-c\" \"SPC\" \"SPC h\" \"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (key-description [?\C-a])
        (key-description [?\M-x])
        (key-description [?\C-\M-x])
        (key-description [?\C-x ?\C-f])
        (key-description [return])
        (key-description [tab])
        (key-description [escape])
        (key-description [backspace])
        (key-description [delete])
        (key-description [?\C-x ?\C-c])
        (key-description [32])
        (key-description [32 104])
        (key-description [])))"#,
        expect,
    );
}

#[test]
fn combo_single_key_description_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"KEY must be an integer, cons, symbol, or string\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (single-key-description ?a)
        (single-key-description ?A)
        (single-key-description 32)
        (single-key-description ?\C-a)
        (single-key-description ?\M-a)
        (single-key-description ?\C-\M-a)
        (single-key-description 'return)
        (single-key-description 'tab)
        (single-key-description 'escape)
        (single-key-description [?\C-x ?\C-f])))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// keymap-parent and accessor edge cases
// ---------------------------------------------------------------------------

#[test]
fn combo_keymap_parent_cycle_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((a-cmd b-cmd t nil) a-cmd nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((a (make-sparse-keymap))
        (b (make-sparse-keymap)))
    (define-key a "x" 'a-cmd)
    (define-key b "x" 'b-cmd)
    (set-keymap-parent a b)
    (let ((result (list (lookup-key a "x")
                        (lookup-key b "x")
                        (eq (keymap-parent a) b)
                        (keymap-parent b))))
      (set-keymap-parent a nil)
      (list result
            (lookup-key a "x")
            (keymap-parent a)))))"#,
        expect,
    );
}

#[test]
fn combo_keymap_prompt_and_metadata() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t t x-cmd)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((map (make-sparse-keymap)))
    (define-key map [menu-bar test] (cons "Test Menu" (make-sparse-keymap)))
    (define-key map "x" 'x-cmd)
    (list (keymap-prompt map)
          (keymapp map)
          (keymapp (lookup-key map [menu-bar test]))
          (lookup-key map "x"))))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// accessible-keymaps and map-keymap
// ---------------------------------------------------------------------------

#[test]
fn combo_accessible_keymaps_from_global_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((maps (accessible-keymaps (current-global-map))))
    (list (> (length maps) 0)
          (cl-every (lambda (m) (keymapp (cdr m))) maps)
          (cl-some (lambda (m) (equal (car m) [])) maps))))"#,
        expect,
    );
}

#[test]
fn combo_map_keymap_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((97 cmd-a) (98 cmd-b) (99 cmd-c))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((map (make-sparse-keymap))
        (bindings nil))
    (define-key map "a" 'cmd-a)
    (define-key map "b" 'cmd-b)
    (define-key map "c" 'cmd-c)
    (map-keymap (lambda (key binding)
                  (push (list key binding) bindings))
                map)
    (sort (nreverse bindings)
          (lambda (a b) (< (car a) (car b))))))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Advice + command-execute + this-command lifecycle
// ---------------------------------------------------------------------------

#[test]
fn combo_advice_around_command_sees_this_command() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((nil nil nil) (nil nil nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((before-snap nil)
        (after-snap nil)
        (inner-snap nil))
    (define-advice ignore
        (:around (fn &rest args) test-snap)
      (setq before-snap (list this-command real-this-command this-original-command))
      (apply fn args)
      (setq after-snap (list this-command real-this-command this-original-command)))
    (setq this-command nil real-this-command nil this-original-command nil)
    (command-execute 'ignore)
    (advice-remove 'ignore 'ignore--test-snap)
    (list before-snap after-snap)))"#,
        expect,
    );
}

#[test]
fn combo_advice_before_mutates_prefix_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument commandp snap-my-prefix)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((captured nil))
    (defun snap-my-prefix ()
      (setq captured current-prefix-arg))
    (define-advice snap-my-prefix
        (:before (&rest _) test-prefix)
      (setq prefix-arg '(16)))
    (setq prefix-arg nil)
    (command-execute 'snap-my-prefix)
    (advice-remove 'snap-my-prefix 'snap-my-prefix--test-prefix)
    captured))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Timer + keymap: timer modifies active keymaps
// ---------------------------------------------------------------------------

#[test]
fn combo_timer_modifies_global_keymap() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (self-insert-command timer-cmd t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((before nil) (after nil) (timer nil))
    (setq before (lookup-key (current-global-map) "z"))
    (setq timer (run-with-timer 0.1 nil
                  (lambda ()
                    (global-set-key "z" 'timer-cmd))))
    (sit-for 0.3)
    (cancel-timer timer)
    (setq after (lookup-key (current-global-map) "z"))
    (global-set-key "z" nil)
    (list before after (eq after 'timer-cmd))))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Complex cross-subsystem: buffer-local hook + timer + command-execute
// ---------------------------------------------------------------------------

#[test]
fn combo_buffer_local_post_command_hook_with_timer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (nil ((timer-callback #<killed buffer> test-cmd)))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-buflocal-hook"))
        (timer nil)
        (hook-trace nil)
        (timer-trace nil))
    (with-current-buffer buf
      (add-hook 'post-command-hook
                (lambda ()
                  (push (list 'hook-in-buf
                              (current-buffer)
                              this-command)
                        hook-trace))
                t t)
      (setq timer (run-with-timer 0.1 nil
                    (lambda ()
                      (push (list 'timer-callback
                                  (current-buffer)
                                  this-command)
                            timer-trace))))
      (setq this-command 'test-cmd)
      (command-execute 'ignore)
      (sit-for 0.3)
      (cancel-timer timer)
      (remove-hook 'post-command-hook nil t)
      (kill-buffer buf)
      (list (nreverse hook-trace) (nreverse timer-trace)))))"#,
        expect,
    );
}

#[test]
fn combo_buffer_local_variables_seen_by_timer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (buffer-local-value t \" combo-buflocal-var\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-buflocal-var"))
        (timer nil)
        (snap nil))
    (with-current-buffer buf
      (set (make-local-variable 'my-test-var) 'buffer-local-value))
    (setq timer (run-with-timer 0.1 nil
                  (lambda ()
                    (with-current-buffer buf
                      (setq snap (list my-test-var
                                       (local-variable-p 'my-test-var)
                                       (buffer-name)))))))
    (sit-for 0.3)
    (cancel-timer timer)
    (kill-buffer buf)
    snap))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Keyboard macro variables
// ---------------------------------------------------------------------------

#[test]
fn combo_keyboard_macro_variables_initial_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable executing-macro)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (boundp 'executing-kbd-macro)
        (symbol-value 'executing-kbd-macro)
        (boundp 'executing-macro)
        (symbol-value 'executing-macro)
        (null executing-kbd-macro)))"#,
        expect,
    );
}

#[test]
fn combo_defining_kbd_macro_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (boundp 'defining-kbd-macro)
        (symbol-value 'defining-kbd-macro)
        (not defining-kbd-macro)))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// last-command-event and last-event-frame
// ---------------------------------------------------------------------------

#[test]
fn combo_last_command_event_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (integerp last-command-event)
        (or (null last-command-event) (integerp last-command-event))
        (integer-or-marker-p last-command-event)))"#,
        expect,
    );
}

#[test]
fn combo_last_nonmenu_event_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (integer-or-marker-p last-nonmenu-event)
        (or (null last-nonmenu-event)
            (integerp last-nonmenu-event)
            (consp last-nonmenu-event)
            (eventp last-nonmenu-event))))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Complex: define-key with vectors, key translation, and lookup
// ---------------------------------------------------------------------------

#[test]
fn combo_define_key_vector_vs_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (find-file save-buffer find-file save-buffer t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((map (make-sparse-keymap)))
    (define-key map [?\C-x ?\C-f] 'find-file)
    (define-key map [?\C-x ?\C-s] 'save-buffer)
    (list (lookup-key map [?\C-x ?\C-f])
          (lookup-key map [?\C-x ?\C-s])
          (lookup-key map "\C-x\C-f")
          (lookup-key map "\C-x\C-s")
          (eq (lookup-key map [?\C-x ?\C-f]) 'find-file)
          (eq (lookup-key map "\C-x\C-f") 'find-file))))"#,
        expect,
    );
}

#[test]
fn combo_key_translation_map_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK [24]""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((result nil))
    (condition-case err
        (progn
          (define-key key-translation-map [f13] [?\C-x])
          (setq result (lookup-key key-translation-map [f13]))
          (define-key key-translation-map [f13] nil))
      (error (setq result (list 'error err))))
    result))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// current-active-maps depth
// ---------------------------------------------------------------------------

#[test]
fn combo_current_active_maps_with_minor_modes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((map1 (make-sparse-keymap))
        (map2 (make-sparse-keymap))
        (maps-before nil)
        (maps-after nil))
    (setq maps-before (current-active-maps))
    (define-key map1 "z" 'minor-cmd-1)
    (define-key map2 "z" 'minor-cmd-2)
    (let ((minor-mode-map-alist
           (list (cons 'fake-mode-1 map1)
                 (cons 'fake-mode-2 map2))))
      (setq maps-after (current-active-maps)))
    (list (>= (length maps-before) 1)
          (>= (length maps-after) 1)
          (>= (length maps-after) (length maps-before)))))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// keymap-char-table and dense vs sparse
// ---------------------------------------------------------------------------

#[test]
fn combo_make_dense_keymap_vs_sparse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t dense-cmd sparse-cmd t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((dense (make-keymap))
        (sparse (make-sparse-keymap)))
    (define-key dense "a" 'dense-cmd)
    (define-key sparse "a" 'sparse-cmd)
    (list (keymapp dense)
          (keymapp sparse)
          (lookup-key dense "a")
          (lookup-key sparse "a")
          (eq (lookup-key dense "a") 'dense-cmd)
          (eq (lookup-key sparse "a") 'sparse-cmd)
          (vectorp (car-safe dense))
          (consp (car-safe sparse)))))"#,
        expect,
    );
}

#[test]
fn combo_keymap_fallback_to_default_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument arrayp keymap)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((dense (make-keymap)))
    (define-key dense "a" 'explicit-cmd)
    (aset (car dense) ?b 'default-b-cmd)
    (list (lookup-key dense "a")
          (lookup-key dense "b")
          (lookup-key dense "c")
          (eq (lookup-key dense "a") 'explicit-cmd)
          (eq (lookup-key dense "b") 'default-b-cmd))))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// use-global-map / global-map identity
// ---------------------------------------------------------------------------

#[test]
fn combo_global_map_identity_and_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (test-cmd t Control-X-prefix)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((saved (current-global-map))
        (new (make-sparse-keymap)))
    (define-key new "X" 'test-cmd)
    (use-global-map new)
    (let ((result (list (lookup-key (current-global-map) "X")
                        (eq (current-global-map) new)
                        (lookup-key saved "\C-x"))))
      (use-global-map saved)
      result)))"#,
        expect,
    );
}

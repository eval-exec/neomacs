use super::menu_test_support::publish;
use super::*;

#[test]
fn gui_menu_deep_acyclic_menu_is_complete_or_reports_resource_exhaustion() {
    let mut eval = Context::new();
    let fixture = |levels| {
        format!(
            "(let ((map '(keymap (leaf menu-item \"Leaf\" ignore))) (level 0))
               (while (< level {levels})
                 (setq map (list 'keymap (list 'child 'menu-item \"Child\" map)))
                 (setq level (1+ level)))
               map)"
        )
    };
    let items = publish(&mut eval, &fixture(40)).unwrap();
    assert_eq!(items.len(), 41);
    assert_eq!(items.last().unwrap().label, "Leaf");
    assert_eq!(items.last().unwrap().depth, 40);

    let result = publish(&mut eval, &fixture(64));
    assert!(
        matches!(result, Err(Flow::Signal(data)) if data.symbol == intern("error")),
        "exhausting the nesting budget must not publish a partial menu"
    );
}

#[test]
fn gui_menu_cycle_is_reported_instead_of_silently_truncating() {
    let mut eval = Context::new();
    let result = publish(
        &mut eval,
        r#"(let ((map (make-sparse-keymap)))
        (define-key map [cycle] (list 'menu-item "Cycle" map))
        map)"#,
    );
    assert!(
        matches!(result, Err(Flow::Signal(data)) if data.symbol == intern("error")),
        "a cyclic source cannot be published as a successful truncated snapshot"
    );
}

#[test]
fn gui_menu_canonicalization_uses_gnu_safe_callback_guard() {
    let mut eval = crate::test_utils::runtime_startup_context();
    let items = publish(
        &mut eval,
        r#"'(keymap
        (keymap (option menu-item "Option" ignore
            :filter (lambda (def) (if inhibit-redisplay def (signal 'quit nil)))))
        (keymap (option menu-item "Inherited" ignore)))"#,
    )
    .unwrap();
    assert_eq!(items[0].label, "Option");
    let items = publish(
        &mut eval,
        r#"'(keymap
        (keymap (option menu-item "Option" ignore
            :filter (lambda (_) (signal 'error '("broken canonicalization")))))
        (keymap (option menu-item "Inherited" ignore)))"#,
    )
    .unwrap();
    assert!(
        items.is_empty(),
        "GNU mutes canonicalization errors rather than publishing a partial menu"
    );
}

#[test]
fn gui_menu_disabled_submenu_does_not_prepare_its_children() {
    let mut eval = Context::new();
    let items = publish(
        &mut eval,
        r#"'(keymap
        (parent menu-item "Unavailable" (keymap
            (child menu-item "Child" ignore :visible (signal 'quit nil))) :enable nil))"#,
    )
    .unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].submenu());
    assert!(!items[0].enabled());
}

#[test]
fn gui_menu_roots_button_form_even_when_a_later_property_detaches_it() {
    use neomacs_display_protocol::menu::{MenuCheckState, MenuIndicator};
    let mut eval = Context::new();
    let items = publish(
        &mut eval,
        r#"(progn
        (setq menu-shared-button (cons :toggle (list 'eq nil nil)))
        (list 'keymap
          (list 'option 'menu-item "Option" 'ignore
            :button menu-shared-button
            :visible '(progn (setcdr menu-shared-button nil) (garbage-collect) t))))"#,
    )
    .unwrap();
    assert_eq!(
        items[0].indicator(),
        MenuIndicator::Toggle(MenuCheckState::On)
    );
}

#[test]
fn gui_menu_enable_override_is_sampled_in_property_order() {
    let mut eval = Context::new();
    let items = publish(
        &mut eval,
        r#"'(keymap (option menu-item "Option" ignore
        :visible (progn (setq enable-disabled-menus-and-buttons t) t)
        :enable nil))"#,
    )
    .unwrap();
    assert!(
        items[0].enabled(),
        "GNU samples the override at :enable, after earlier property callbacks"
    );
}

#[test]
fn gui_menu_resolves_aliases_to_real_autoloaded_keymaps() {
    let mut eval = crate::test_utils::runtime_startup_context();
    assert_eq!(
        eval.eval_str("(autoloadp (symbol-function 'kmacro-keymap))")
            .unwrap(),
        Value::T
    );
    let items = publish(
        &mut eval,
        r#"(progn
        (defalias 'menu-macro-alias 'kmacro-keymap)
        '(keymap (macro menu-item "Keyboard Macro" menu-macro-alias)))"#,
    )
    .unwrap();
    assert!(items[0].submenu());
    assert_eq!(
        eval.eval_str("(autoloadp (symbol-function 'kmacro-keymap))")
            .unwrap(),
        Value::NIL
    );
}

#[test]
fn gui_menu_preparation_roots_bindings_across_property_gc() {
    use neomacs_display_protocol::menu::{MenuCheckState, MenuIndicator};
    let mut eval = crate::test_utils::runtime_startup_context();
    let items = publish(
        &mut eval,
        r#"`(keymap
        (first menu-item ,(list 'progn '(garbage-collect) (concat "First" " item")) ignore
            :button (:toggle . (progn (garbage-collect) t)))
        (second menu-item ,(concat "Second" " item") ignore))"#,
    )
    .unwrap();
    assert_eq!(
        items
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>(),
        vec!["First item", "Second item"]
    );
    assert_eq!(
        items[0].indicator(),
        MenuIndicator::Toggle(MenuCheckState::On)
    );
}

#[test]
fn gui_menu_legacy_cache_does_not_hide_the_real_command() {
    let mut eval = Context::new();
    let items = publish(
        &mut eval,
        r#"(progn
        (put 'legacy-command 'menu-enable nil)
        (let ((map (make-sparse-keymap)))
            (define-key map [24 19] 'legacy-command)
            (use-global-map map))
        '(keymap (legacy "Legacy" "Help" (nil . "obsolete") . legacy-command)))"#,
    )
    .unwrap();
    assert_eq!(items[0].shortcut, "C-x C-s");
}

#[test]
fn gui_menu_inherited_submenu_merges_children() {
    let mut eval = crate::test_utils::runtime_startup_context();
    let items = publish(&mut eval, r#"(let ((parent (make-sparse-keymap)) (child (make-sparse-keymap)))
        (define-key parent [shared] '(menu-item "Shared" (keymap (inherited menu-item "Inherited" ignore))))
        (define-key child [shared] '(menu-item "Shared" (keymap (local menu-item "Local" ignore))))
        (set-keymap-parent child parent)
        child)"#).unwrap();
    assert_eq!(
        items
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>(),
        vec!["Shared", "Local", "Inherited"]
    );
}

#[test]
fn gui_menu_accepts_a_symbolic_root_keymap() {
    let mut eval = Context::new();
    let items = publish(
        &mut eval,
        r#"(progn
        (fset 'menu-root '(keymap (action menu-item "Action" ignore)))
        'menu-root)"#,
    )
    .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "Action");
}

#[test]
fn gui_menu_native_result_cannot_activate_a_submenu_header() {
    let mut eval = Context::new();
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame)
        .unwrap()
        .set_window_system(Some(Value::symbol(gui_window_system_symbol())));
    let (tx, rx) = crossbeam_channel::unbounded();
    eval.input_rx = Some(rx);
    eval.set_display_host(Box::new(RecordingPopupHost::default()));
    let menu = eval
        .eval_str(
            r#"'(keymap
        (parent menu-item "Parent" (keymap (child menu-item "Child" ignore))))"#,
        )
        .unwrap();
    for index in [0, 1] {
        tx.send(crate::keyboard::InputEvent::MenuSelection { index, token: None })
            .unwrap();
    }
    let result = super::super::builtin_x_popup_menu(
        &mut eval,
        vec![Value::list(vec![Value::NIL, Value::NIL]), menu],
    )
    .unwrap();
    assert_eq!(
        list_to_vec(&result).unwrap(),
        vec![Value::symbol("parent"), Value::symbol("child")]
    );
}

#[test]
fn gui_menu_label_does_not_evaluate_command_only_button_state() {
    use neomacs_display_protocol::menu::MenuIndicator;
    let mut eval = Context::new();
    let items = publish(
        &mut eval,
        r#"'(keymap
        (label menu-item "Information" nil :button (:toggle . (signal 'quit nil))))"#,
    )
    .unwrap();
    assert!(!items[0].enabled());
    assert_eq!(items[0].indicator(), MenuIndicator::None);
}

#[test]
fn gui_menu_help_substitutes_keys_unless_text_property_inhibits_it() {
    let mut eval = crate::test_utils::runtime_startup_context();
    let items = publish(
        &mut eval,
        r#"(progn
        (let ((map (make-sparse-keymap)))
            (define-key map [24 19] 'menu-save)
            (use-global-map map))
        `(keymap
          (expanded menu-item "Expanded" ignore :help "Save with \\[menu-save]")
          (literal menu-item "Literal" ignore :help
            ,(propertize "Save with \\[menu-save]" 'help-echo-inhibit-substitution t))))"#,
    )
    .unwrap();
    assert_eq!(items[0].help.as_deref(), Some("Save with C-x C-s"));
    assert_eq!(items[1].help.as_deref(), Some(r"Save with \[menu-save]"));
}

#[test]
fn gui_default_file_edit_options_menus_publish_gnu_decorations() {
    use neomacs_display_protocol::menu::MenuIndicator;
    let mut eval = crate::test_utils::runtime_startup_context();
    let file = publish(&mut eval, "(progn (require 'menu-bar) menu-bar-file-menu)").unwrap();
    assert!(
        file.iter().any(|item| item.shortcut == "C-x C-s"),
        "File must expose Save's actual keyboard equivalent: {file:?}"
    );
    let edit = publish(&mut eval, "(progn (require 'facemenu) menu-bar-edit-menu)").unwrap();
    assert!(
        edit.iter()
            .any(|item| item.label == "Text Properties" && item.submenu()),
        "Edit must expose Text Properties as a submenu"
    );
    let options = publish(&mut eval, "menu-bar-options-menu").unwrap();
    assert!(
        options
            .iter()
            .any(|item| matches!(item.indicator(), MenuIndicator::Toggle(_))),
        "Options must expose toggle state"
    );
}

#[test]
fn gui_menu_shortcut_can_name_another_command_with_affixes() {
    let mut eval = Context::new();
    let items = publish(
        &mut eval,
        r#"(progn
        (let ((map (make-sparse-keymap)))
            (define-key map [24 19] 'menu-save)
            (use-global-map map))
        '(keymap (save menu-item "Save variant" menu-save-variant
            :keys (menu-save "[" . "]"))))"#,
    )
    .unwrap();
    assert_eq!(items[0].shortcut, "[C-x C-s]");
}

#[test]
fn gui_menu_inherits_parent_items_without_duplicate_overrides() {
    let mut eval = Context::new();
    let items = publish(
        &mut eval,
        r#"(let ((parent (make-sparse-keymap)) (child (make-sparse-keymap)))
        (define-key parent [shared] '(menu-item "Parent shared" ignore))
        (define-key parent [extra] '(menu-item "Parent extra" ignore))
        (define-key child [shared] '(menu-item "Child shared" ignore))
        (set-keymap-parent child parent)
        child)"#,
    )
    .unwrap();
    assert_eq!(
        items
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>(),
        vec!["Child shared", "Parent extra"]
    );
}

#[test]
fn gui_menu_legacy_enable_and_user_override_match_gnu() {
    let mut eval = Context::new();
    let source = r#"(progn (put 'legacy-command 'menu-enable '(eq nil t))
        '(keymap (legacy "Legacy" . legacy-command)))"#;
    assert!(!publish(&mut eval, source).unwrap()[0].enabled());
    eval.eval_str("(setq enable-disabled-menus-and-buttons t)")
        .unwrap();
    assert!(publish(&mut eval, source).unwrap()[0].enabled());
    assert!(
        publish(
            &mut eval,
            r#"'(keymap (extended menu-item "Extended" ignore :enable nil))"#
        )
        .unwrap()[0]
            .enabled()
    );
}

#[test]
fn gui_menu_function_shortcut_runs_once_during_preparation() {
    let mut eval = crate::test_utils::runtime_startup_context();
    let items = publish(&mut eval, r#"(progn
        (setq menu-shortcut-calls 0)
        '(keymap (save menu-item "Save" ignore
          :keys (lambda () (setq menu-shortcut-calls (1+ menu-shortcut-calls)) "Computed keys"))))"#).unwrap();
    assert_eq!(items[0].shortcut, "Computed keys");
    assert_eq!(
        eval.eval_str("menu-shortcut-calls").unwrap(),
        Value::fixnum(1)
    );
}

#[test]
fn gui_menu_key_sequence_hint_is_verified_against_current_bindings() {
    let mut eval = Context::new();
    eval.eval_str(
        r#"(let ((map (make-sparse-keymap)))
        (define-key map [1] 'menu-save)
        (define-key map [2] 'menu-save)
        (use-global-map map))"#,
    )
    .unwrap();
    let source = r#"'(keymap (save menu-item "Save" menu-save :key-sequence [1]))"#;
    assert_eq!(publish(&mut eval, source).unwrap()[0].shortcut, "C-a");
    eval.eval_str("(define-key (current-global-map) [1] 'other-command)")
        .unwrap();
    assert_eq!(publish(&mut eval, source).unwrap()[0].shortcut, "C-b");
}

#[test]
fn gui_menu_disabled_native_result_does_not_dispatch() {
    let mut eval = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    eval.input_rx = Some(rx);
    eval.set_display_host(Box::new(RecordingPopupHost::default()));
    let menu = eval
        .eval_str(
            r#"'(keymap
        (bad menu-item "Disabled" ignore :enable nil)
        (good menu-item "Enabled" ignore))"#,
        )
        .unwrap();
    for index in [0, 1] {
        tx.send(crate::keyboard::InputEvent::MenuSelection { index, token: None })
            .unwrap();
    }
    let result = super::super::builtin_x_popup_menu(
        &mut eval,
        vec![Value::list(vec![Value::NIL, Value::NIL]), menu],
    )
    .unwrap();
    assert_eq!(list_to_vec(&result).unwrap(), vec![Value::symbol("good")]);
}

#[test]
fn gui_menu_only_gnu_separator_names_are_recognized() {
    let mut eval = Context::new();
    let items = publish(
        &mut eval,
        r#"'(keymap
        (named menu-item "--double-line")
        (not-a-separator menu-item "--not-a-separator" ignore))"#,
    )
    .unwrap();
    assert!(items[0].separator());
    assert!(!items[1].separator());
    assert!(items[1].enabled());
}

#[test]
fn gui_menu_keymap_separator_is_not_a_disabled_text_row() {
    let mut eval = Context::new();
    let items = publish(
        &mut eval,
        r#"'(keymap
        (first menu-item "First" ignore)
        (separator menu-item "--")
        (last menu-item "Last" ignore))"#,
    )
    .unwrap();
    assert!(items[1].separator());
    assert!(!items[1].enabled());
    assert!(!items[1].submenu());
}

#[test]
fn gui_menu_filter_can_build_a_submenu() {
    let mut eval = Context::new();
    let items = publish(
        &mut eval,
        r#"'(keymap
        (filtered menu-item "Filtered" nil
          :filter (lambda (_) '(keymap (child menu-item "Child" ignore)))))"#,
    )
    .unwrap();
    assert!(items[0].submenu());
    assert_eq!(items.len(), 2);
    assert_eq!(items[1].label, "Child");
    assert_eq!(items[1].depth, 1);
}

#[test]
fn gui_menu_enable_predicate_preserves_disabled_checked_item() {
    use neomacs_display_protocol::menu::{MenuCheckState, MenuIndicator};
    let mut eval = Context::new();
    let items = publish(
        &mut eval,
        r#"'(keymap
        (option menu-item "Unavailable option" ignore :enable (eq nil t) :button (:toggle . t)))"#,
    )
    .unwrap();
    assert!(!items[0].enabled());
    assert_eq!(
        items[0].indicator(),
        MenuIndicator::Toggle(MenuCheckState::On)
    );
}

#[test]
fn gui_menu_property_errors_are_nil_but_quit_escapes() {
    use neomacs_display_protocol::menu::{MenuCheckState, MenuIndicator};
    let mut eval = Context::new();
    let items = publish(
        &mut eval,
        r#"'(keymap
        (bad menu-item "Bad predicate" ignore :button (:toggle . (signal 'error '("broken")))))"#,
    )
    .unwrap();
    assert_eq!(
        items[0].indicator(),
        MenuIndicator::Toggle(MenuCheckState::Off)
    );
    let error = publish(
        &mut eval,
        r#"'(keymap
        (quit menu-item "Quit predicate" ignore :button (:toggle . (signal 'quit nil))))"#,
    )
    .unwrap_err();
    assert!(matches!(error, Flow::Signal(data) if data.symbol == intern("quit")));
}

#[test]
fn gui_menu_visibility_skips_label_and_submenu_evaluation() {
    let mut eval = Context::new();
    let items = publish(
        &mut eval,
        r#"(progn
        (setq menu-hidden-label-ran nil)
        '(keymap
          (hidden menu-item (progn (setq menu-hidden-label-ran t) "Hidden") ignore :visible nil)
          (shown menu-item "Shown" ignore :visible t)))"#,
    )
    .unwrap();
    assert_eq!(
        items
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>(),
        vec!["Shown"]
    );
    assert_eq!(eval.eval_str("menu-hidden-label-ran").unwrap(), Value::NIL);
}

#[test]
fn gui_menu_computed_label_is_published() {
    let mut eval = Context::new();
    let items = publish(
        &mut eval,
        r#"'(keymap
        (name menu-item (concat "Dynamic" " label") ignore))"#,
    )
    .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "Dynamic label");
}

#[test]
fn gui_menu_shortcut_uses_current_command_binding() {
    let mut eval = Context::new();
    let items = publish(
        &mut eval,
        r#"(progn
      (let ((map (make-sparse-keymap)))
        (define-key map [24 19] 'menu-save)
        (use-global-map map))
      '(keymap (save menu-item "Save" menu-save)))"#,
    )
    .unwrap();
    assert_eq!(items[0].shortcut, "C-x C-s");
}

#[test]
fn gui_menu_explicit_shortcut_overrides_automatic_binding() {
    let mut eval = crate::test_utils::runtime_startup_context();
    let items = publish(
        &mut eval,
        r#"'(keymap
        (save menu-item "Save" ignore :keys "Custom shortcut"))"#,
    )
    .unwrap();
    assert_eq!(items[0].shortcut, "Custom shortcut");
}

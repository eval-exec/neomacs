use expect_test::expect;

use super::ParityBatchCase;

/// Turning the mode on is the whole installation step the README describes, and
/// it has to be a *global* switch that replaces `x-popup-menu' exactly once.
/// This walks the lifecycle a user goes through: enable, enable again by
/// accident, look at the mode from another buffer, toggle off, call
/// `x-popup-menu' while off (the real function runs and renders nothing),
/// toggle back on, and finally disable.
fn enabling_the_global_mode_advises_x_popup_menu_exactly_once_until_disabled() -> ParityBatchCase {
    ParityBatchCase::value(
        "enabling_the_global_mode_advises_x_popup_menu_exactly_once_until_disabled",
        r##"(progn
  (apm-test-setup)
  (let ((observed nil))
    (push (cons :initial (apm-test-mode-state)) observed)
    (ace-popup-menu-mode 1)
    (push (cons :enabled (apm-test-mode-state)) observed)
    (ace-popup-menu-mode 1)
    (push (cons :enabled-twice (apm-test-mode-state)) observed)
    (with-temp-buffer
      (push (cons :other-buffer (apm-test-mode-state)) observed))
    (ace-popup-menu-mode 'toggle)
    (push (cons :toggled-off (apm-test-mode-state)) observed)
    (push (cons :unadvised-call
                (list (x-popup-menu t apm-test-menu)
                      (length (apm-test-renderings))))
          observed)
    (ace-popup-menu-mode 'toggle)
    (push (cons :toggled-on (apm-test-mode-state)) observed)
    (ace-popup-menu-mode -1)
    (push (cons :disabled (apm-test-mode-state)) observed)
    (setq observed (nreverse observed))
    observed))"##,
        expect![
            "OK ((:initial :advised nil :advice-count 0 :mode nil :global-value nil :buffer-local nil) (:enabled :advised t :advice-count 1 :mode t :global-value t :buffer-local nil) (:enabled-twice :advised t :advice-count 1 :mode t :global-value t :buffer-local nil) (:other-buffer :advised t :advice-count 1 :mode t :global-value t :buffer-local nil) (:toggled-off :advised nil :advice-count 0 :mode nil :global-value nil :buffer-local nil) (:unadvised-call nil 0) (:toggled-on :advised t :advice-count 1 :mode t :global-value t :buffer-local nil) (:disabled :advised nil :advice-count 0 :mode nil :global-value nil :buffer-local nil))"
        ],
    )
}

fn every_avy_label_returns_the_value_of_the_menu_item_it_marks() -> ParityBatchCase {
    ParityBatchCase::value(
        "every_avy_label_returns_the_value_of_the_menu_item_it_marks",
        r##"(progn
  (apm-test-setup)
  (ace-popup-menu-mode 1)
  (let ((results nil))
    (dolist (key '("a" "s" "d" "f" "g"))
      (setq unread-command-events (listify-key-sequence (kbd key)))
      (push (list key (x-popup-menu t apm-test-menu) unread-command-events)
            results))
    (setq results (nreverse results))
    (list :selections results
          :renderings (length (apm-test-renderings))
          :rendering (car (apm-test-renderings))
          :menu-buffer-left (and (get-buffer "*ace-popup-menu*") t)
          :windows (length (window-list))
          :current (buffer-name))))"##,
        expect![[
            r#"OK (:selections (("a" rename-symbol nil) ("s" rename-file nil) ("d" extract-function nil) ("f" extract-variable nil) ("g" inline-variable nil)) :renderings 5 :rendering (:buffer "*ace-popup-menu*" :text "Refactor\n\nRename symbol\nRename file\n\nExtract function\nExtract variable\nInline variable" :runs (("Refactor" . avy-menu-title) ("\n\nRename symbol\nRename file\n\nExtract function\nExtract variable\nInline variable")) :cursor nil :window-buffer "*ace-popup-menu*") :menu-buffer-left nil :windows 1 :current "*apm-work*")"#
        ]],
    )
}

fn showing_pane_headers_changes_the_rendering_but_not_the_labels() -> ParityBatchCase {
    ParityBatchCase::value(
        "showing_pane_headers_changes_the_rendering_but_not_the_labels",
        r##"(progn
  (apm-test-setup)
  (ace-popup-menu-mode 1)
  (let ((observed nil))
    (setq unread-command-events (listify-key-sequence (kbd "d")))
    (push (list :without-headers ace-popup-menu-show-pane-header
                (x-popup-menu t apm-test-menu))
          observed)
    (setq ace-popup-menu-show-pane-header t)
    (setq unread-command-events (listify-key-sequence (kbd "d")))
    (push (list :with-headers ace-popup-menu-show-pane-header
                (x-popup-menu t apm-test-menu))
          observed)
    (setq observed (nreverse observed))
    (list :selections observed
          :renderings (apm-test-renderings))))"##,
        expect![[
            r#"OK (:selections ((:without-headers nil extract-function) (:with-headers t extract-function)) :renderings ((:buffer "*ace-popup-menu*" :text "Refactor\n\nRename symbol\nRename file\n\nExtract function\nExtract variable\nInline variable" :runs (("Refactor" . avy-menu-title) ("\n\nRename symbol\nRename file\n\nExtract function\nExtract variable\nInline variable")) :cursor nil :window-buffer "*ace-popup-menu*") (:buffer "*ace-popup-menu*" :text "Refactor\n\nRename\n\nRename symbol\nRename file\n\nExtract\n\nExtract function\nExtract variable\nInline variable" :runs (("Refactor" . avy-menu-title) ("\n\n") ("Rename" . avy-menu-pane-header) ("\n\nRename symbol\nRename file\n\n") ("Extract" . avy-menu-pane-header) ("\n\nExtract function\nExtract variable\nInline variable")) :cursor nil :window-buffer "*ace-popup-menu*")))"#
        ]],
    )
}

fn a_command_bound_to_a_key_pops_up_the_menu_and_restores_the_work_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_command_bound_to_a_key_pops_up_the_menu_and_restores_the_work_buffer",
        r##"(progn
  (apm-test-setup)
  (ace-popup-menu-mode 1)
  (execute-kbd-macro (kbd "C-c m d"))
  (list :result apm-test-result
        :current (buffer-name)
        :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :windows (length (window-list))
        :menu-buffer-left (and (get-buffer "*ace-popup-menu*") t)
        :renderings (apm-test-renderings)))"##,
        expect![[
            r#"OK (:result extract-function :current "*apm-work*" :text "Editing buffer, untouched by the menu.\n" :point 1 :windows 1 :menu-buffer-left nil :renderings ((:buffer "*ace-popup-menu*" :text "Refactor\n\nRename symbol\nRename file\n\nExtract function\nExtract variable\nInline variable" :runs (("Refactor" . avy-menu-title) ("\n\nRename symbol\nRename file\n\nExtract function\nExtract variable\nInline variable")) :cursor nil :window-buffer "*ace-popup-menu*")))"#
        ]],
    )
    .fresh_process()
}

fn the_documented_fallback_shapes_hand_the_call_to_the_original_function() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_documented_fallback_shapes_hand_the_call_to_the_original_function",
        r##"(progn
  (apm-test-setup)
  (let ((keymap (make-sparse-keymap "Keymap menu")))
    (define-key keymap [item] '(menu-item "Item" ignore))
    (let ((nil-position (ace-popup-menu #'apm-test-orig-fun nil apm-test-menu))
          (keymap-menu (ace-popup-menu #'apm-test-orig-fun t keymap))
          (keymap-list (ace-popup-menu #'apm-test-orig-fun t (list keymap keymap))))
      (setq unread-command-events (listify-key-sequence (kbd "s")))
      (let ((avy-path (ace-popup-menu #'apm-test-orig-fun t apm-test-menu)))
        (list :nil-position nil-position
              :keymap-menu keymap-menu
              :keymap-list keymap-list
              :avy-path avy-path
              :orig-calls (apm-test-orig-calls)
              :renderings (length (apm-test-renderings)))))))"##,
        expect![[
            r#"OK (:nil-position value-from-orig-fun :keymap-menu value-from-orig-fun :keymap-list value-from-orig-fun :avy-path rename-file :orig-calls ((:orig nil ("Refactor" ("Rename" ("Rename symbol" . rename-symbol) ("Rename file" . rename-file)) ("Extract" ("Extract function" . extract-function) ("Extract variable" . extract-variable) ("Inline variable" . inline-variable)))) (:orig t #1=(keymap (item menu-item "Item" ignore) "Keymap menu")) (:orig t (#1# #1#))) :renderings 1)"#
        ]],
    )
}

fn cancelling_the_menu_returns_nil_and_leaves_no_window_or_buffer_behind() -> ParityBatchCase {
    ParityBatchCase::value(
        "cancelling_the_menu_returns_nil_and_leaves_no_window_or_buffer_behind",
        r##"(progn
  (apm-test-setup)
  (ace-popup-menu-mode 1)
  (let ((results nil))
    (dolist (key '("C-g" "ESC"))
      (setq unread-command-events (listify-key-sequence (kbd key)))
      (push (list key
                  (condition-case failure (x-popup-menu t apm-test-menu)
                    (error (list :error failure))
                    (quit (list :quit failure)))
                  unread-command-events)
            results))
    (setq results (nreverse results))
    (list :aborts results
          :renderings (length (apm-test-renderings))
          :menu-buffer-left (and (get-buffer "*ace-popup-menu*") t)
          :windows (length (window-list))
          :current (buffer-name)
          :text (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect![[
            r#"OK (:aborts (("C-g" nil nil) ("ESC" nil nil)) :renderings 2 :menu-buffer-left nil :windows 1 :current "*apm-work*" :text "Editing buffer, untouched by the menu.\n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        enabling_the_global_mode_advises_x_popup_menu_exactly_once_until_disabled(),
        every_avy_label_returns_the_value_of_the_menu_item_it_marks(),
        showing_pane_headers_changes_the_rendering_but_not_the_labels(),
        a_command_bound_to_a_key_pops_up_the_menu_and_restores_the_work_buffer(),
        the_documented_fallback_shapes_hand_the_call_to_the_original_function(),
        cancelling_the_menu_returns_nil_and_leaves_no_window_or_buffer_behind(),
    ]
}

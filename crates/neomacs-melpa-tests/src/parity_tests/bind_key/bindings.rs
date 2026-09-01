use expect_test::expect;

use super::ParityBatchCase;

fn bind_key_binds_string_vector_and_remap_events_and_records_original_bindings() -> ParityBatchCase
{
    ParityBatchCase::value(
        "bind_key_binds_string_vector_and_remap_events_and_records_original_bindings",
        r##"(let ((personal-keybindings nil)
                    (map (make-sparse-keymap)))
               (define-key map (kbd "C-c a") #'beginning-of-line)
               (bind-key "C-c a" #'forward-line map)
               (bind-key [f8] #'ignore map)
               (bind-key [remap forward-char] #'backward-char map)
               (list
                (lookup-key map (kbd "C-c a"))
                (lookup-key map [f8])
                (lookup-key map [remap forward-char])
                personal-keybindings))"##,
        expect![[
            r#"OK (forward-line ignore backward-char ((("<remap> <forward-char>" . map) backward-char nil) (("<f8>" . map) ignore nil) (("C-c a" . map) forward-line beginning-of-line)))"#
        ]],
    )
}

fn bind_key_accepts_a_quoted_keymap_symbol_and_updates_an_existing_registry_entry()
-> ParityBatchCase {
    ParityBatchCase::value(
        "bind_key_accepts_a_quoted_keymap_symbol_and_updates_an_existing_registry_entry",
        r##"(progn
               (defvar neomacs-bind-key-test-map (make-sparse-keymap))
               (let ((personal-keybindings nil))
                 (bind-key "C-c q" #'forward-char
                           'neomacs-bind-key-test-map)
                 (bind-key "C-c q" #'backward-char
                           'neomacs-bind-key-test-map)
                 (list
                  (lookup-key neomacs-bind-key-test-map
                              (kbd "C-c q"))
                  personal-keybindings)))"##,
        expect![[
            r#"OK (backward-char ((("C-c q" . neomacs-bind-key-test-map) backward-char forward-char)))"#
        ]],
    )
}

fn bind_key_predicate_filter_tracks_live_state_and_preserves_registry_metadata() -> ParityBatchCase
{
    ParityBatchCase::value(
        "bind_key_predicate_filter_tracks_live_state_and_preserves_registry_metadata",
        r##"(let ((personal-keybindings nil)
                    (map (make-sparse-keymap)))
               (defvar neomacs-bind-key-enabled nil)
               (setq neomacs-bind-key-enabled nil)
               (bind-key "C-c p" #'forward-char map
                         neomacs-bind-key-enabled)
               (with-temp-buffer
                 (use-local-map map)
                 (let ((disabled (key-binding (kbd "C-c p"))))
                   (setq neomacs-bind-key-enabled t)
                   (list
                    disabled
                    (key-binding (kbd "C-c p"))
                    personal-keybindings))))"##,
        expect![[r#"OK (nil forward-char ((("C-c p" . map) forward-char nil)))"#]],
    )
}

fn unbind_key_removes_nested_empty_prefixes_and_its_personal_registry_entry() -> ParityBatchCase {
    ParityBatchCase::value(
        "unbind_key_removes_nested_empty_prefixes_and_its_personal_registry_entry",
        r##"(let ((personal-keybindings nil)
                    (map (make-sparse-keymap)))
               (bind-key "C-c x" #'ignore map)
               (let ((before
                      (list
                       (lookup-key map (kbd "C-c"))
                       (lookup-key map (kbd "C-c x"))
                       personal-keybindings)))
                 (unbind-key "C-c x" map)
                 (list
                  (list
                   (keymapp (nth 0 before))
                   (nth 1 before)
                   (length (nth 2 before)))
                  (lookup-key map (kbd "C-c"))
                  (lookup-key map (kbd "C-c x"))
                  personal-keybindings)))"##,
        expect![[r#"OK ((t ignore 1) nil 1 nil)"#]],
    )
}

fn unbind_key_removes_meta_bindings_stored_through_the_escape_prefix() -> ParityBatchCase {
    ParityBatchCase::value(
        "unbind_key_removes_meta_bindings_stored_through_the_escape_prefix",
        r##"(let ((personal-keybindings nil)
                    (map (make-sparse-keymap)))
               (bind-key "M-z" #'zap-to-char map)
               (let ((before
                      (list
                       (lookup-key map (kbd "M-z"))
                       (lookup-key map (kbd "ESC z")))))
                 (unbind-key "M-z" map)
                 (list
                  before
                  (lookup-key map (kbd "M-z"))
                  (lookup-key map (kbd "ESC z"))
                  personal-keybindings)))"##,
        expect![[r#"OK ((zap-to-char zap-to-char) nil 1 nil)"#]],
    )
}

fn bind_key_star_wins_over_a_local_map_through_the_emulation_map() -> ParityBatchCase {
    ParityBatchCase::value(
        "bind_key_star_wins_over_a_local_map_through_the_emulation_map",
        r##"(let ((personal-keybindings nil)
                    (local (make-sparse-keymap)))
               (define-key local (kbd "<f8>") #'backward-char)
               (bind-key* "<f8>" #'forward-char)
               (with-temp-buffer
                 (use-local-map local)
                 (list
                  override-global-mode
                  (lookup-key override-global-map (kbd "<f8>"))
                  (lookup-key local (kbd "<f8>"))
                  (key-binding (kbd "<f8>"))
                  personal-keybindings)))"##,
        expect![[
            r#"OK (t forward-char backward-char forward-char ((("<f8>" . override-global-map) forward-char nil)))"#
        ]],
    )
}

fn bind_keys_and_bind_keys_star_bind_multiple_commands_in_the_requested_maps() -> ParityBatchCase {
    ParityBatchCase::value(
        "bind_keys_and_bind_keys_star_bind_multiple_commands_in_the_requested_maps",
        r##"(progn
               (defvar neomacs-bind-keys-map (make-sparse-keymap))
               (let ((personal-keybindings nil))
                 (bind-keys
                  :map neomacs-bind-keys-map
                  ("a" . beginning-of-line)
                  ("e" . end-of-line))
                 (bind-keys*
                  ("C-c n" . next-line)
                  ("C-c p" . previous-line))
                 (list
                  (lookup-key neomacs-bind-keys-map "a")
                  (lookup-key neomacs-bind-keys-map "e")
                  (lookup-key override-global-map (kbd "C-c n"))
                  (lookup-key override-global-map (kbd "C-c p"))
                  (mapcar #'car personal-keybindings))))"##,
        expect![[
            r#"OK (beginning-of-line end-of-line next-line previous-line (("C-c p" . override-global-map) ("C-c n" . override-global-map) ("e" . neomacs-bind-keys-map) ("a" . neomacs-bind-keys-map)))"#
        ]],
    )
}

pub(super) fn bindings_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        bind_key_binds_string_vector_and_remap_events_and_records_original_bindings(),
        bind_key_accepts_a_quoted_keymap_symbol_and_updates_an_existing_registry_entry(),
        bind_key_predicate_filter_tracks_live_state_and_preserves_registry_metadata(),
        unbind_key_removes_nested_empty_prefixes_and_its_personal_registry_entry(),
        unbind_key_removes_meta_bindings_stored_through_the_escape_prefix(),
        bind_key_star_wins_over_a_local_map_through_the_emulation_map(),
        bind_keys_and_bind_keys_star_bind_multiple_commands_in_the_requested_maps(),
    ]
}

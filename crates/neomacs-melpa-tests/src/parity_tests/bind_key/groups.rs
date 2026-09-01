use expect_test::expect;

use super::ParityBatchCase;

fn bind_keys_prefix_map_sets_parent_binding_commands_docstring_and_menu_name() -> ParityBatchCase {
    ParityBatchCase::value(
        "bind_keys_prefix_map_sets_parent_binding_commands_docstring_and_menu_name",
        r##"(progn
               (defvar neomacs-bind-key-host-map
                 (make-sparse-keymap))
               (let ((personal-keybindings nil))
                 (bind-keys
                  :map neomacs-bind-key-host-map
                  :prefix-map neomacs-bind-key-prefix-map
                  :prefix "C-c b"
                  :prefix-docstring "Neomacs prefix documentation"
                  :menu-name "Neomacs Prefix"
                  ("a" . beginning-of-line)
                  ("e" . end-of-line))
                 (list
                  (lookup-key neomacs-bind-key-host-map
                              (kbd "C-c b"))
                  (lookup-key neomacs-bind-key-prefix-map "a")
                  (lookup-key neomacs-bind-key-prefix-map "e")
                  (get 'neomacs-bind-key-prefix-map
                       'variable-documentation)
                  (keymapp neomacs-bind-key-prefix-map)
                  (mapcar #'car personal-keybindings))))"##,
        expect![[
            r#"OK (neomacs-bind-key-prefix-map beginning-of-line end-of-line "Neomacs prefix documentation" t (("e" . neomacs-bind-key-prefix-map) ("a" . neomacs-bind-key-prefix-map) ("C-c b" . neomacs-bind-key-host-map)))"#
        ]],
    )
}

fn bind_keys_repeat_map_sets_continue_properties_but_not_exit_properties() -> ParityBatchCase {
    ParityBatchCase::value(
        "bind_keys_repeat_map_sets_continue_properties_but_not_exit_properties",
        r##"(progn
               (defun neomacs-bind-key-next () (interactive))
               (defun neomacs-bind-key-prev () (interactive))
               (defun neomacs-bind-key-quit () (interactive))
               (let ((personal-keybindings nil))
                 (bind-keys
                  :repeat-map neomacs-bind-key-repeat-map
                  :repeat-docstring "Neomacs repeat map"
                  ("n" . neomacs-bind-key-next)
                  :exit
                  ("q" . neomacs-bind-key-quit)
                  :continue
                  ("p" . neomacs-bind-key-prev))
                 (list
                  (lookup-key neomacs-bind-key-repeat-map "n")
                  (lookup-key neomacs-bind-key-repeat-map "q")
                  (lookup-key neomacs-bind-key-repeat-map "p")
                  (get 'neomacs-bind-key-next 'repeat-map)
                  (get 'neomacs-bind-key-quit 'repeat-map)
                  (get 'neomacs-bind-key-prev 'repeat-map)
                  (documentation-property
                   'neomacs-bind-key-repeat-map
                   'variable-documentation))))"##,
        expect![[
            r#"OK (neomacs-bind-key-next neomacs-bind-key-quit neomacs-bind-key-prev neomacs-bind-key-repeat-map nil neomacs-bind-key-repeat-map "Neomacs repeat map")"#
        ]],
    )
}

fn bind_keys_switches_maps_and_prefix_groups_without_reordering_bindings() -> ParityBatchCase {
    ParityBatchCase::value(
        "bind_keys_switches_maps_and_prefix_groups_without_reordering_bindings",
        r##"(progn
               (defvar neomacs-bind-key-map-one
                 (make-sparse-keymap))
               (defvar neomacs-bind-key-map-two
                 (make-sparse-keymap))
               (let ((personal-keybindings nil))
                 (bind-keys
                  :map neomacs-bind-key-map-one
                  ("A" . beginning-of-line)
                  :prefix "x"
                  :prefix-map neomacs-bind-key-prefix-one
                  ("a" . forward-char)
                  ("b" . backward-char)
                  :map neomacs-bind-key-map-two
                  ("Z" . end-of-line))
                 (list
                  (lookup-key neomacs-bind-key-map-one "A")
                  (lookup-key neomacs-bind-key-map-one "x")
                  (lookup-key neomacs-bind-key-prefix-one "a")
                  (lookup-key neomacs-bind-key-prefix-one "b")
                  (lookup-key neomacs-bind-key-map-two "Z")
                  (mapcar #'caar (reverse personal-keybindings)))))"##,
        expect![[
            r#"OK (beginning-of-line neomacs-bind-key-prefix-one forward-char backward-char end-of-line ("A" "x" "a" "b" "Z"))"#
        ]],
    )
}

fn bind_keys_package_defers_an_unbound_map_until_the_feature_loads() -> ParityBatchCase {
    ParityBatchCase::value(
        "bind_keys_package_defers_an_unbound_map_until_the_feature_loads",
        r##"(let ((personal-keybindings nil))
               (makunbound 'neomacs-deferred-bind-key-map)
               (bind-keys
                :package neomacs-deferred-bind-key-feature
                :map neomacs-deferred-bind-key-map
                ("x" . forward-char))
               (let ((before
                      (list
                       (boundp 'neomacs-deferred-bind-key-map)
                       (assoc
                        'neomacs-deferred-bind-key-feature
                        after-load-alist)
                       personal-keybindings)))
                 (setq neomacs-deferred-bind-key-map
                       (make-sparse-keymap))
                 (provide 'neomacs-deferred-bind-key-feature)
                 (list
                  (car before)
                  (and (nth 1 before) t)
                  (nth 2 before)
                  (lookup-key neomacs-deferred-bind-key-map "x")
                  personal-keybindings)))"##,
        expect![[
            r#"OK (nil t nil forward-char ((("x" . neomacs-deferred-bind-key-map) forward-char nil)))"#
        ]],
    )
}

fn bind_keys_rejects_a_prefix_without_a_prefix_map() -> ParityBatchCase {
    ParityBatchCase::signal(
        "bind_keys_rejects_a_prefix_without_a_prefix_map",
        r##"(bind-keys :prefix "C-c b"
               ("x" . forward-char))"##,
        expect![[r#"ERR (error "Both :prefix-map and :prefix must be supplied")"#]],
    )
}

fn bind_keys_rejects_repeat_exit_bindings_without_a_repeat_map() -> ParityBatchCase {
    ParityBatchCase::signal(
        "bind_keys_rejects_repeat_exit_bindings_without_a_repeat_map",
        r##"(bind-keys :exit
               ("q" . keyboard-quit))"##,
        expect![[r#"ERR (error ":continue and :exit require specifying :repeat-map")"#]],
    )
}

fn bind_keys_rejects_a_menu_name_without_a_prefix() -> ParityBatchCase {
    ParityBatchCase::signal(
        "bind_keys_rejects_a_menu_name_without_a_prefix",
        r##"(bind-keys :menu-name "Broken"
               ("x" . forward-char))"##,
        expect![[r#"ERR (error "If :menu-name is supplied, :prefix must be too")"#]],
    )
}

pub(super) fn groups_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        bind_keys_prefix_map_sets_parent_binding_commands_docstring_and_menu_name(),
        bind_keys_repeat_map_sets_continue_properties_but_not_exit_properties(),
        bind_keys_switches_maps_and_prefix_groups_without_reordering_bindings(),
        bind_keys_package_defers_an_unbound_map_until_the_feature_loads(),
        bind_keys_rejects_a_prefix_without_a_prefix_map(),
        bind_keys_rejects_repeat_exit_bindings_without_a_repeat_map(),
        bind_keys_rejects_a_menu_name_without_a_prefix(),
    ]
}

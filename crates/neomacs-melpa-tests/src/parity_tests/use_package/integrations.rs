use expect_test::expect;

use super::ParityBatchCase;

fn use_package_bind_registers_global_and_map_bindings_through_bind_key() -> ParityBatchCase {
    ParityBatchCase::value(
        "use_package_bind_registers_global_and_map_bindings_through_bind_key",
        r##"(progn
               (defvar neomacs-use-package-bind-map
                 (make-sparse-keymap))
               (let ((personal-keybindings nil))
                 (use-package neomacs-use-package-bind-library
                   :bind
                   (("C-c u" . neomacs-use-package-global-command)
                    :map neomacs-use-package-bind-map
                    ("x" . neomacs-use-package-map-command)))
                 (list
                  (lookup-key global-map (kbd "C-c u"))
                  (lookup-key neomacs-use-package-bind-map "x")
                  (autoloadp
                   (symbol-function
                    'neomacs-use-package-global-command))
                  (autoloadp
                   (symbol-function
                    'neomacs-use-package-map-command))
                  personal-keybindings)))"##,
        expect![[
            r#"OK (neomacs-use-package-global-command neomacs-use-package-map-command t t ((("x" . neomacs-use-package-bind-map) neomacs-use-package-map-command nil) (("C-c u") neomacs-use-package-global-command nil)))"#
        ]],
    )
}

fn use_package_bind_keymap_loads_a_real_library_and_then_dispatches_its_prefix() -> ParityBatchCase
{
    ParityBatchCase::value(
        "use_package_bind_keymap_loads_a_real_library_and_then_dispatches_its_prefix",
        r##"(let* ((root
                    (make-temp-file "use-package-keymap-" t))
                   (load-path (cons root load-path)))
               (unwind-protect
                   (progn
                     (with-temp-file
                         (expand-file-name
                          "neomacs-use-package-keymap.el" root)
                       (insert
                        "(defvar neomacs-use-package-prefix-map\n"
                        "  (let ((map (make-sparse-keymap)))\n"
                        "    (define-key map \"x\" #'forward-char)\n"
                        "    map))\n"
                        "(provide 'neomacs-use-package-keymap)\n"))
                     (use-package neomacs-use-package-keymap
                       :bind-keymap
                       ("C-c k" .
                        neomacs-use-package-prefix-map))
                     (let ((before
                            (featurep
                             'neomacs-use-package-keymap))
                           (command
                            (key-binding (kbd "C-c k"))))
                       (cl-letf
                           (((symbol-function
                              'this-command-keys-vector)
                             (lambda () (kbd "C-c k"))))
                         (funcall command))
                       (list
                        before
                        (featurep
                         'neomacs-use-package-keymap)
                        (lookup-key
                         neomacs-use-package-prefix-map "x")
                        (key-binding (kbd "C-c k x")))))
                 (delete-directory root t)))"##,
        expect![[r#"OK (nil t forward-char forward-char)"#]],
    )
}

fn use_package_custom_sets_the_value_and_exact_customization_metadata() -> ParityBatchCase {
    ParityBatchCase::value(
        "use_package_custom_sets_the_value_and_exact_customization_metadata",
        r##"(progn
               (defcustom neomacs-use-package-custom-variable
                 'initial "Test variable.")
               (use-package neomacs-use-package-custom-library
                 :no-require t
                 :custom
                 (neomacs-use-package-custom-variable
                  '(one two)
                  "Configured by parity test"))
               (list
                neomacs-use-package-custom-variable
                (get
                 'neomacs-use-package-custom-variable
                 'saved-value)
                (get
                 'neomacs-use-package-custom-variable
                 'customized-value)
                (get
                 'neomacs-use-package-custom-variable
                 'customized-variable-comment)
                (get
                 'neomacs-use-package-custom-variable
                 'custom-requests)))"##,
        expect![[r#"OK (#1=(one two) ('#1#) nil nil nil)"#]],
    )
}

fn use_package_custom_face_records_exact_face_spec_and_modified_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "use_package_custom_face_records_exact_face_spec_and_modified_state",
        r##"(progn
               (defface neomacs-use-package-face
                 '((t :foreground "black"))
                 "Parity face.")
               (use-package neomacs-use-package-face-library
                 :no-require t
                 :custom-face
                 (neomacs-use-package-face
                  ((t (:foreground "blue"
                       :weight bold)))))
               (list
                (get 'neomacs-use-package-face 'face-modified)
                (get 'neomacs-use-package-face 'customized-face)
                (face-attribute
                 'neomacs-use-package-face :foreground nil t)
                (face-attribute
                 'neomacs-use-package-face :weight nil t)))"##,
        expect![[r#"OK (t nil "blue" bold)"#]],
    )
}

fn use_package_ensure_calls_the_selected_install_boundary_with_normalized_arguments()
-> ParityBatchCase {
    ParityBatchCase::value(
        "use_package_ensure_calls_the_selected_install_boundary_with_normalized_arguments",
        r##"(let (calls)
               (cl-letf
                   (((symbol-function
                      'neomacs-use-package-ensure-function)
                     (lambda (name args state &optional no-refresh)
                       (push
                        (list name args state no-refresh)
                        calls)
                       t)))
                 (let ((use-package-ensure-function
                        'neomacs-use-package-ensure-function))
                   (eval
                    '(use-package
                         neomacs-use-package-ensure-target
                       :ensure dependency-a
                       :ensure
                       (dependency-b :pin "gnu")
                       :no-require t)))
                 (nreverse calls)))"##,
        expect![[
            r#"OK ((neomacs-use-package-ensure-target (dependency-a (dependency-b . "gnu")) nil nil))"#
        ]],
    )
}

fn use_package_load_path_expands_static_and_computed_paths_below_user_emacs_directory()
-> ParityBatchCase {
    ParityBatchCase::value(
        "use_package_load_path_expands_static_and_computed_paths_below_user_emacs_directory",
        r##"(let* ((user-emacs-directory
                    (file-name-as-directory
                     (make-temp-file
                      "use-package-load-path-" t)))
                   (original-load-path load-path))
               (unwind-protect
                   (progn
                     (use-package
                         neomacs-use-package-load-path
                       :no-require t
                       :load-path
                       ("one"
                        (lambda () '("two" "three"))))
                     (mapcar
                      (lambda (path)
                        (file-relative-name
                         path user-emacs-directory))
                      (seq-filter
                       (lambda (path)
                         (string-prefix-p
                          user-emacs-directory path))
                       load-path)))
                 (delete-directory
                  user-emacs-directory t)
                 (setq load-path original-load-path)))"##,
        expect![[r#"OK ("two" "one")"#]],
    )
}

pub(super) fn integrations_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        use_package_bind_registers_global_and_map_bindings_through_bind_key(),
        use_package_bind_keymap_loads_a_real_library_and_then_dispatches_its_prefix(),
        use_package_custom_sets_the_value_and_exact_customization_metadata(),
        use_package_custom_face_records_exact_face_spec_and_modified_state(),
        use_package_ensure_calls_the_selected_install_boundary_with_normalized_arguments(),
        use_package_load_path_expands_static_and_computed_paths_below_user_emacs_directory(),
    ]
}

use expect_test::expect;

use super::ParityBatchCase;

fn alchemist_project_discovers_nested_mix_root_skips_hex_boundaries_and_reuses_cache()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_project_discovers_nested_mix_root_skips_hex_boundaries_and_reuses_cache",
        r##"(let* ((sandbox
                           (file-name-as-directory
                            (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                          (project (expand-file-name "umbrella" sandbox))
                          (nested (expand-file-name
                                   "apps/web/lib/web/controllers" project))
                          (hex (expand-file-name
                                "deps/package/lib/deep" project))
                          (default-directory
                           (file-name-as-directory nested))
                          (alchemist-project-root-path-cache nil))
                      (make-directory nested t)
                      (make-directory hex t)
                      (with-temp-file
                          (expand-file-name "mix.exs" project)
                        (insert "defmodule Umbrella.MixProject do\nend\n"))
                      (with-temp-file
                          (expand-file-name "deps/package/.hex" project)
                        (insert "package"))
                      (let ((first (alchemist-project-root)))
                        (list
                         (file-relative-name first sandbox)
                         (alchemist-project-p)
                         (alchemist-project-name)
                         (let ((default-directory
                                (file-name-as-directory hex)))
                           (file-relative-name
                            (alchemist-project-root)
                            sandbox))
                         (progn
                           (delete-file
                            (expand-file-name "mix.exs" project))
                           (file-relative-name
                            (alchemist-project-root)
                            sandbox))
                         (file-relative-name
                          alchemist-project-root-path-cache
                          sandbox))))"##,
        expect![[r#"OK ("umbrella/" t "umbrella" "umbrella/" "umbrella/" "umbrella/")"#]],
    )
}

fn alchemist_project_maps_library_web_and_umbrella_files_to_real_tests_and_back() -> ParityBatchCase
{
    ParityBatchCase::value(
        "alchemist_project_maps_library_web_and_umbrella_files_to_real_tests_and_back",
        r##"(let* ((sandbox
                           (file-name-as-directory
                            (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                          (project (file-name-as-directory
                                    (expand-file-name "my_app" sandbox)))
                          (default-directory project)
                          (alchemist-project-root-path-cache nil)
                          opened)
                      (dolist
                          (file
                           '("mix.exs"
                             "lib/accounts/user.ex"
                             "test/accounts/user_test.exs"
                             "web/controllers/page_controller.ex"
                             "test/controllers/page_controller_test.exs"
                             "apps/admin/lib/admin/audit.ex"
                             "apps/admin/test/admin/audit_test.exs"
                             "apps/site/web/views/layout_view.ex"
                             "apps/site/test/views/layout_view_test.exs"))
                        (let ((path (expand-file-name file project)))
                          (make-directory
                           (file-name-directory path) t)
                          (with-temp-file path (insert file))))
                      (cl-letf
                          (((symbol-function 'alchemist-parity-open)
                            (lambda (file)
                              (setq opened
                                    (file-relative-name file project))
                              opened)))
                        (mapcar
                         (lambda (file)
                           (with-temp-buffer
                             (setq buffer-file-name
                                   (expand-file-name file project))
                             (if (string-match-p "_test\\.exs\\'" file)
                                 (alchemist-project-open-file-for-current-tests
                                  #'alchemist-parity-open)
                               (alchemist-project-open-tests-for-current-file
                                #'alchemist-parity-open))
                             (list file opened)))
                         '("lib/accounts/user.ex"
                           "test/accounts/user_test.exs"
                           "web/controllers/page_controller.ex"
                           "test/controllers/page_controller_test.exs"
                           "apps/admin/lib/admin/audit.ex"
                           "apps/admin/test/admin/audit_test.exs"
                           "apps/site/web/views/layout_view.ex"
                           "apps/site/test/views/layout_view_test.exs"))))"##,
        expect![[
            r#"OK (("lib/accounts/user.ex" "test/accounts/user_test.exs") ("test/accounts/user_test.exs" "lib/accounts/user.ex") ("web/controllers/page_controller.ex" "test/controllers/page_controller_test.exs") ("test/controllers/page_controller_test.exs" "web/controllers/page_controller.ex") ("apps/admin/lib/admin/audit.ex" "apps/admin/test/admin/audit_test.exs") ("apps/admin/test/admin/audit_test.exs" "apps/admin/test/admin/audit.ex") ("apps/site/web/views/layout_view.ex" "apps/site/test/views/layout_view_test.exs") ("apps/site/test/views/layout_view_test.exs" "apps/site/test/views/layout_view.ex"))"#
        ]],
    )
}

fn alchemist_project_creates_missing_test_with_real_module_boilerplate_and_cursor_position()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_project_creates_missing_test_with_real_module_boilerplate_and_cursor_position",
        r##"(let* ((sandbox
                           (file-name-as-directory
                            (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                          (project (file-name-as-directory
                                    (expand-file-name "billing" sandbox)))
                          (source (expand-file-name
                                   "lib/billing/invoice.ex" project))
                          (target (expand-file-name
                                   "test/billing/invoice_test.exs" project))
                          (default-directory project)
                          (alchemist-project-root-path-cache nil)
                          created)
                      (make-directory (file-name-directory source) t)
                      (with-temp-file
                          (expand-file-name "mix.exs" project)
                        (insert "mix"))
                      (with-temp-file source
                        (insert
                         "defmodule Billing.Invoice, do: def(total), do: total\n"))
                      (let ((source-buffer (find-file-noselect source)))
                        (unwind-protect
                            (cl-letf
                                (((symbol-function 'y-or-n-p)
                                  (lambda (&rest _) t))
                                 ((symbol-function 'find-file-other-window)
                                  (lambda (file)
                                    (setq created
                                          (find-file-noselect file)))))
                              (with-current-buffer source-buffer
                                (alchemist-project-open-tests-for-current-file
                                 #'ignore))
                              (with-current-buffer created
                                (list
                                 (file-relative-name
                                  buffer-file-name project)
                                 (buffer-string)
                                 (point)
                                 (line-number-at-pos)
                                 (current-indentation)
                                 (file-directory-p
                                  (file-name-directory target)))))
                          (when (buffer-live-p source-buffer)
                            (kill-buffer source-buffer))
                          (when (buffer-live-p created)
                            (set-buffer-modified-p nil)
                            (kill-buffer created)))))"##,
        expect![[
            r#"OK ("test/billing/invoice_test.exs" "defmodule Billing.InvoiceTest do\n  use ExUnit.Case\n\n  \nend\n" 55 4 2 t)"#
        ]],
    )
}

fn alchemist_project_create_file_turns_nested_user_path_into_real_module_and_editing_position()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_project_create_file_turns_nested_user_path_into_real_module_and_editing_position",
        r##"(let* ((sandbox
                           (file-name-as-directory
                            (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                          (project
                           (file-name-as-directory
                            (expand-file-name "inventory" sandbox)))
                          (target
                           (expand-file-name
                            "lib/inventory/stock_item.ex" project))
                          (default-directory project)
                          (alchemist-project-root-path-cache nil))
                      (make-directory project t)
                      (with-temp-file
                          (expand-file-name "mix.exs" project)
                        (insert "mix"))
                      (cl-letf
                          (((symbol-function 'read-file-name)
                            (lambda (&rest _) target)))
                        (save-current-buffer
                          (alchemist-project-create-file)
                          (let ((buffer (get-file-buffer target)))
                            (unwind-protect
                                (with-current-buffer buffer
                                  (list
                                   (file-relative-name
                                    buffer-file-name project)
                                   (buffer-string)
                                   (point)
                                   (line-number-at-pos)
                                   (current-indentation)
                                   (file-directory-p
                                    (file-name-directory target))))
                              (when (buffer-live-p buffer)
                                (set-buffer-modified-p nil)
                                (kill-buffer buffer)))))))"##,
        expect![[
            r#"OK ("lib/inventory/stock_item.ex" "defmodule Inventory.StockItem do\n  \nend\n" 36 2 2 t)"#
        ]],
    )
}

fn alchemist_file_recursively_lists_real_project_files_and_prompt_opens_selection()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alchemist_file_recursively_lists_real_project_files_and_prompt_opens_selection",
        r##"(let* ((sandbox
                           (file-name-as-directory
                            (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                          (project (file-name-as-directory
                                    (expand-file-name "catalog" sandbox)))
                          (default-directory project)
                          prompt candidates opened)
                      (dolist
                          (file
                           '("lib/a.ex" "lib/nested/b.ex"
                             "lib/nested/deep/c.ex" "lib/.hidden.ex"))
                        (let ((path (expand-file-name file project)))
                          (make-directory
                           (file-name-directory path) t)
                          (with-temp-file path (insert file))))
                      (cl-letf
                          (((symbol-function 'completing-read)
                            (lambda (actual-prompt actual-candidates
                                     &rest _)
                              (setq prompt actual-prompt
                                    candidates actual-candidates)
                              "lib/nested/deep/c.ex"))
                           ((symbol-function 'find-file)
                            (lambda (file)
                              (setq opened
                                    (file-relative-name file project)))))
                        (list
                         (alchemist-file-read-dir project "lib")
                         (alchemist-file-find-files project "lib")
                         prompt
                         candidates
                         opened)))"##,
        expect![[
            r#"OK (("lib/.hidden.ex" "lib/a.ex" "lib/nested/b.ex" "lib/nested/deep/c.ex") "lib/nested/deep/c.ex" "[catalog] lib: " ("lib/.hidden.ex" "lib/a.ex" "lib/nested/b.ex" "lib/nested/deep/c.ex") "lib/nested/deep/c.ex")"#
        ]],
    )
}

fn alchemist_phoenix_discovers_real_web_tree_router_routes_and_mode_activation() -> ParityBatchCase
{
    ParityBatchCase::value(
        "alchemist_phoenix_discovers_real_web_tree_router_routes_and_mode_activation",
        r##"(let* ((sandbox
                           (file-name-as-directory
                            (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
                          (project (file-name-as-directory
                                    (expand-file-name "phoenix_app" sandbox)))
                          (default-directory
                           (file-name-as-directory
                            (expand-file-name "web/controllers" project)))
                          (alchemist-project-root-path-cache nil)
                          events)
                      (dolist
                          (file
                           '("mix.exs" "web/router.ex"
                             "web/controllers/page_controller.ex"
                             "web/views/page_view.ex"
                             "web/templates/page/index.html.eex"
                             "web/static/app.js"))
                        (let ((path (expand-file-name file project)))
                          (make-directory
                           (file-name-directory path) t)
                          (with-temp-file path (insert file))))
                      (cl-letf
                          (((symbol-function 'alchemist-file-find-files)
                            (lambda (root directory)
                              (push
                               (list 'find
                                     (file-relative-name root sandbox)
                                     directory)
                               events)
                              directory))
                           ((symbol-function 'find-file)
                            (lambda (file)
                              (push
                               (list 'router
                                     (file-relative-name file project))
                               events)
                              'router))
                           ((symbol-function 'alchemist-mix-execute)
                            (lambda (command prefix)
                              (push
                               (list 'routes command prefix)
                               events)
                              'routes)))
                        (list
                         (alchemist-phoenix-project-p)
                         (alchemist-phoenix-find-web)
                         (alchemist-phoenix-find-controllers)
                         (alchemist-phoenix-find-templates)
                         (alchemist-phoenix-router)
                         (alchemist-phoenix-routes '(4))
                         (with-temp-buffer
                           (alchemist-phoenix-enable-mode)
                           alchemist-phoenix-mode)
                         (nreverse events))))"##,
        expect![[
            r#"OK (t "web" "web/controllers" "web/templates" router routes t ((find "phoenix_app/" "web") (find "phoenix_app/" "web/controllers") (find "phoenix_app/" "web/templates") (router "web/router.ex") (routes ("phoenix.routes") (4))))"#
        ]],
    )
}

pub(super) fn project_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alchemist_project_discovers_nested_mix_root_skips_hex_boundaries_and_reuses_cache(),
        alchemist_project_maps_library_web_and_umbrella_files_to_real_tests_and_back(),
        alchemist_project_creates_missing_test_with_real_module_boilerplate_and_cursor_position(),
        alchemist_project_create_file_turns_nested_user_path_into_real_module_and_editing_position(
        ),
        alchemist_file_recursively_lists_real_project_files_and_prompt_opens_selection(),
        alchemist_phoenix_discovers_real_web_tree_router_routes_and_mode_activation(),
    ]
}

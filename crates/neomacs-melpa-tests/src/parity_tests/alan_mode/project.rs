use expect_test::expect;

use super::ParityBatchCase;

fn alan_project_root_discovers_markers_in_documented_precedence_and_caches_the_result()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alan_project_root_discovers_markers_in_documented_precedence_and_caches_the_result",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                           (project (expand-file-name "project" root))
                           (nested (expand-file-name "src/deep" project))
                           (default-directory
                            (file-name-as-directory nested))
                           (alan-project-root nil)
                           alan-language-definition)
                      (make-directory nested t)
                      (with-temp-file
                          (expand-file-name "versions.json" project)
                        (insert "{}"))
                      (with-temp-file
                          (expand-file-name "project.json"
                                            (expand-file-name "src" project))
                        (insert "{}"))
                      (let ((without-language (alan-project-root)))
                        (setq alan-project-root nil
                              alan-language-definition "language")
                        (let ((with-language (alan-project-root)))
                          (delete-file
                           (expand-file-name
                            "project.json"
                            (expand-file-name "src" project)))
                          (list
                           (file-relative-name without-language root)
                           (file-relative-name with-language root)
                           (file-relative-name
                            (alan-project-root) root)
                           (equal with-language alan-project-root)))))"##,
        expect![[r#"OK ("project/" "project/src/" "project/src/" t)"#]],
    )
}

fn alan_script_discovery_walks_up_to_the_first_real_executable_project_script() -> ParityBatchCase {
    ParityBatchCase::value(
        "alan_script_discovery_walks_up_to_the_first_real_executable_project_script",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                           (project (expand-file-name "project" root))
                           (nested (expand-file-name "src/deep" project))
                           (script (expand-file-name "alan" project))
                           (default-directory
                            (file-name-as-directory nested))
                           (alan-project-root nil))
                      (make-directory nested t)
                      (with-temp-file
                          (expand-file-name "build.alan" project)
                        (insert "root"))
                      (with-temp-file script
                        (insert "#!/bin/sh\nexit 0\n"))
                      (set-file-modes script #o755)
                      (list
                       (file-relative-name
                        (alan-find-alan-script) root)
                       (alan-file-executable script)
                       (progn
                         (set-file-modes script #o644)
                         (alan-find-alan-script))))"##,
        expect![[r#"OK ("project/alan" "[ORACLE-SANDBOX]/project/alan" nil)"#]],
    )
}

fn alan_build_setup_constructs_real_compiler_pretty_printer_and_flycheck_boundaries()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alan_build_setup_constructs_real_compiler_pretty_printer_and_flycheck_boundaries",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                           (project (file-name-as-directory
                                     (expand-file-name "project" root)))
                           (source (expand-file-name
                                    "models/accounts/main.alan" project))
                           (compiler (expand-file-name
                                      "dependencies/dev/internals/alan/tools/compiler-project"
                                      project))
                           (printer (expand-file-name
                                     "dependencies/dev/internals/alan/tools/pretty-printer"
                                     project)))
                      (dolist (file (list source compiler printer))
                        (make-directory (file-name-directory file) t)
                        (with-temp-file file
                          (insert
                           (if (string-suffix-p ".alan" file)
                               "'root'\n"
                             "#!/bin/sh\n")))
                        (unless (string-suffix-p ".alan" file)
                          (set-file-modes file #o755)))
                      (with-temp-buffer
                        (setq buffer-file-name source
                              default-directory
                              (file-name-directory source)
                              alan-project-root project
                              alan-language-definition
                              "dependencies/dev/internals/alan/language"
                              alan-compiler-project-root "../..")
                        (alan-setup-build-system)
                        (mapcar
                         (lambda (value)
                           (and value
                                (replace-regexp-in-string
                                 (regexp-quote root) "[ROOT]" value t t)))
                         (list flycheck-alan-executable
                               alan--flycheck-language-definition
                               compile-command
                               alan-pretty-printer))))"##,
        expect![[
            r#"OK ("[ROOT]/project/dependencies/dev/internals/alan/tools/compiler-project" "[ROOT]/project/dependencies/dev/internals/alan/language" "[ROOT]/project/dependencies/dev/internals/alan/tools/compiler-project [ROOT]/project/dependencies/dev/internals/alan/language -C ../.. /dev/null " "[ROOT]/project/dependencies/dev/internals/alan/tools/pretty-printer [ROOT]/project/dependencies/dev/internals/alan/language  --allow-unresolved -C ../.. --file '[ROOT]/project/models/accounts/main.alan' -- 'models' 'accounts' 'main.alan'")"#
        ]],
    )
}

fn alan_relative_project_path_quotes_each_real_path_component_for_the_compiler() -> ParityBatchCase
{
    ParityBatchCase::value(
        "alan_relative_project_path_quotes_each_real_path_component_for_the_compiler",
        r##"(let ((alan-compiler-project-root
                           "/workspace/customer project"))
                      (list
                       (alan--file-path-to-relative-project-path
                        "/workspace/customer project/models/sales order/main.alan")
                       (alan--file-path-to-relative-project-path
                        "/workspace/customer project/root.alan")))"##,
        expect![[r#"OK ("'models' 'sales order' 'main.alan'" "'root.alan'")"#]],
    )
}

fn alan_lsp_discovery_and_server_command_use_real_project_layout_and_capture_path()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alan_lsp_discovery_and_server_command_use_real_project_layout_and_capture_path",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                           (project (file-name-as-directory
                                     (expand-file-name "project" root)))
                           (build (expand-file-name "build.alan" project))
                           (server (expand-file-name
                                    "dependencies/dev/internals/alan/tools/alan"
                                    project))
                           (default-directory project)
                           (alan-project-root project)
                           (alan-lsp-capture-path "trace.log"))
                      (make-directory (file-name-directory server) t)
                      (with-temp-file build (insert "root"))
                      (with-temp-file server
                        (insert "#!/bin/sh\n"))
                      (set-file-modes server #o755)
                      (let ((found (alan-lsp--find-command))
                            (command (alan-lsp--server-command)))
                        (list
                         (file-relative-name found root)
                         (cons
                          (file-relative-name (car command) root)
                          (cdr command))
                         (alan-eglot--server-command)
                         (alan-lsp-activate-alan-mode
                          build 'alan-schema-mode))))"##,
        expect![[
            r#"OK ("project/dependencies/dev/internals/alan/tools/alan" ("project/dependencies/dev/internals/alan/tools/alan" "--lsp" "--capture" "trace.log") ("[ORACLE-SANDBOX]/project/dependencies/dev/internals/alan/tools/alan" "--lsp" "--capture" "trace.log") "[ORACLE-SANDBOX]/project/dependencies/dev/internals/alan/tools/alan")"#
        ]],
    )
}

fn alan_lsp_and_eglot_registration_emit_the_exact_client_protocol_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "alan_lsp_and_eglot_registration_emit_the_exact_client_protocol_contract",
        r##"(progn
                      (defvar lsp-language-id-configuration)
                      (defvar eglot-server-programs)
                      (defvar alan-parity-registered)
                      (defvar alan-parity-connection)
                      (let (lsp-language-id-configuration
                            eglot-server-programs
                            alan-parity-registered
                            alan-parity-connection)
                        (cl-letf (((symbol-function 'lsp-stdio-connection)
                                   (lambda (command)
                                     (setq alan-parity-connection command)
                                     (list :stdio command)))
                                  ((symbol-function 'make-lsp-client)
                                   (lambda (&rest arguments)
                                     arguments))
                                  ((symbol-function 'lsp-register-client)
                                   (lambda (client)
                                     (setq alan-parity-registered client)
                                     'registered)))
                          (list
                           (alan-setup-lsp)
                           lsp-language-id-configuration
                           alan-parity-connection
                           alan-parity-registered
                           (alan-setup-eglot)
                           eglot-server-programs))))"##,
        expect![[
            r#"OK (registered (("\\.alan$" . "alan")) alan-lsp--server-command (:new-connection (:stdio alan-lsp--server-command) :activation-fn alan-lsp-activate-alan-mode :server-id alan-ls) #1=((alan-mode . alan-eglot--server-command)) #1#)"#
        ]],
    )
}

pub(super) fn project_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alan_project_root_discovers_markers_in_documented_precedence_and_caches_the_result(),
        alan_script_discovery_walks_up_to_the_first_real_executable_project_script(),
        alan_build_setup_constructs_real_compiler_pretty_printer_and_flycheck_boundaries(),
        alan_relative_project_path_quotes_each_real_path_component_for_the_compiler(),
        alan_lsp_discovery_and_server_command_use_real_project_layout_and_capture_path(),
        alan_lsp_and_eglot_registration_emit_the_exact_client_protocol_contract(),
    ]
}

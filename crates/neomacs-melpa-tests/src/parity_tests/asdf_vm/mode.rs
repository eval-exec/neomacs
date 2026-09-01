use expect_test::expect;

use super::ParityBatchCase;

fn asdf_vm_exec_path_injection_prepend_append_and_duplicates_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_exec_path_injection_prepend_append_and_duplicates_are_exact",
        r##"(mapcar
               (lambda (behaviour)
                 (let ((asdf-vm-path-injection-behaviour
                        behaviour)
                       (exec-path
                        '("/base/bin"
                          "/shared/bin")))
                   (list
                    behaviour
                    (asdf-vm--inject-exec-path
                     "/tool/bin"
                     "/shared/bin")
                    exec-path)))
               '(prepend append))"##,
        expect![[
            r#"OK ((prepend #1=("/tool/bin" "/shared/bin" "/base/bin" "/shared/bin") #1#) (append #2=("/base/bin" "/shared/bin" "/tool/bin" "/shared/bin") #2#))"#
        ]],
    )
}

fn asdf_vm_exec_path_injection_disabled_requires_discoverable_executable_without_mutation()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_exec_path_injection_disabled_requires_discoverable_executable_without_mutation",
        r##"(let ((asdf-vm-path-injection-behaviour
                    nil)
                   (asdf-vm-process-executable
                    "fixture-asdf")
                   (exec-path
                    '("/base/bin"))
                   found)
               (cl-letf
                   (((symbol-function
                      'executable-find)
                     (lambda (executable)
                       (setq found executable)
                       nil)))
                 (let ((missing
                        (asdf-vm-test-error-data
                         (lambda ()
                           (asdf-vm--inject-exec-path
                            "/tool/bin")))))
                   (cl-letf
                       (((symbol-function
                          'executable-find)
                         (lambda (executable)
                           (setq found
                                 (list
                                  found executable))
                           "/fixture/asdf")))
                     (list
                      missing
                      (asdf-vm--inject-exec-path
                       "/tool/bin")
                      exec-path
                      found)))))"##,
        expect![[
            r#"OK ((:error asdf-vm-no-exectuable-error nil) nil ("/base/bin") ("fixture-asdf" "fixture-asdf"))"#
        ]],
    )
}

fn asdf_vm_exec_path_cleanup_removes_every_matching_occurrence_or_is_noop_when_disabled()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_exec_path_cleanup_removes_every_matching_occurrence_or_is_noop_when_disabled",
        r##"(list
               (let ((asdf-vm-path-injection-behaviour
                      'prepend)
                     (exec-path
                      '("/tool/bin"
                        "/base/bin"
                        "/tool/bin"
                        "/other/bin")))
                 (list
                  (asdf-vm--clean-exec-path
                   "/tool/bin"
                   "/missing")
                  exec-path))
               (let ((asdf-vm-path-injection-behaviour
                      nil)
                     (exec-path
                      '("/tool/bin"
                        "/base/bin")))
                 (list
                  (asdf-vm--clean-exec-path
                   "/tool/bin")
                  exec-path)))"##,
        expect![[r#"OK ((nil ("/base/bin" "/other/bin")) (nil ("/tool/bin" "/base/bin")))"#]],
    )
}

fn asdf_vm_mode_activation_and_deactivation_round_trip_path_mode_line_and_environment_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_mode_activation_and_deactivation_round_trip_path_mode_line_and_environment_state",
        r##"(let ((asdf-vm-mode-line-format
                    "(Fixture-ASDF)")
                   (asdf-vm-path-injection-behaviour
                    'prepend)
                   (asdf-vm--shims-directory
                    "/fixture/asdf/shims")
                   (asdf-vm-config-file
                    "/new/config")
                   (asdf-vm-tool-versions-filename
                    ".fixture-versions")
                   (asdf-vm-dir
                    "/new/core")
                   (asdf-vm-data-dir
                    "/new/data")
                   (asdf-vm-concurrency
                    "8")
                   (exec-path
                    '("/base/bin"
                      "/usr/bin"))
                   (mode-line-misc-info
                    '("base"))
                   (asdf-vm-mode--state
                    nil))
               (setenv "PATH"
                       "/old/path")
               (setenv "ASDF_CONFIG_FILE"
                       "/old/config")
               (setenv "ASDF_TOOL_VERSIONS_FILENAME"
                       nil)
               (setenv "ASDF_DIR"
                       "/old/core")
               (setenv "ASDF_DATA_DIR"
                       nil)
               (setenv "ASDF_CONCURRENCY"
                       "auto")
               (asdf-vm-mode--activate)
               (let ((active
                      (list
                       mode-line-misc-info
                       exec-path
                       (getenv "PATH")
                       (mapcar
                        #'getenv
                        '("ASDF_CONFIG_FILE"
                          "ASDF_TOOL_VERSIONS_FILENAME"
                          "ASDF_DIR"
                          "ASDF_DATA_DIR"
                          "ASDF_CONCURRENCY"))
                       asdf-vm-mode--state)))
                 (asdf-vm-mode--deactivate)
                 (list
                  active
                  mode-line-misc-info
                  exec-path
                  (getenv "PATH")
                  (mapcar
                   #'getenv
                   '("ASDF_CONFIG_FILE"
                     "ASDF_TOOL_VERSIONS_FILENAME"
                     "ASDF_DIR"
                     "ASDF_DATA_DIR"
                     "ASDF_CONCURRENCY"))
                  asdf-vm-mode--state)))"##,
        expect![[
            r#"OK ((("(Fixture-ASDF)" . #1=("base")) ("/fixture/asdf/shims" "/base/bin" "/usr/bin") "/fixture/asdf/shims:/base/bin:/usr/bin" ("/new/config" ".fixture-versions" "/new/core" "/new/data" "8") ((asdf-vm-config-file "ASDF_CONFIG_FILE" "/old/config") (asdf-vm-tool-versions-filename "ASDF_TOOL_VERSIONS_FILENAME" nil) (asdf-vm-dir "ASDF_DIR" "/old/core") (asdf-vm-data-dir "ASDF_DATA_DIR" nil) (asdf-vm-concurrency "ASDF_CONCURRENCY" "auto"))) #1# ("/base/bin" "/usr/bin") "/base/bin:/usr/bin" ("/old/config" nil "/old/core" nil "auto") nil)"#
        ]],
    )
}

fn asdf_vm_global_minor_mode_invokes_activation_only_on_enable_and_deactivation_on_disable()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_global_minor_mode_invokes_activation_only_on_enable_and_deactivation_on_disable",
        r##"(let ((asdf-vm-mode nil)
                    calls)
               (cl-letf
                   (((symbol-function
                      'asdf-vm-mode--activate)
                     (lambda ()
                       (push :activate calls)
                       :active))
                    ((symbol-function
                      'asdf-vm-mode--deactivate)
                     (lambda ()
                       (push :deactivate calls)
                       :inactive)))
                 (list
                  (asdf-vm-mode 1)
                  asdf-vm-mode
                  (asdf-vm-mode 1)
                  asdf-vm-mode
                  (asdf-vm-mode -1)
                  asdf-vm-mode
                  (nreverse calls))))"##,
        expect!["OK (t t t t nil nil (:activate :activate :deactivate))"],
    )
}

fn asdf_vm_mode_enable_and_disable_user_commands_forward_their_exact_numeric_arguments()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_mode_enable_and_disable_user_commands_forward_their_exact_numeric_arguments",
        r##"(let (calls)
               (cl-letf
                   (((symbol-function
                      'asdf-vm-mode)
                     (lambda (&rest arguments)
                       (push arguments calls)
                       :mode-result)))
                 (list
                  (asdf-vm-mode-enable)
                  (asdf-vm-mode-disable)
                  (nreverse calls))))"##,
        expect!["OK (:mode-result :mode-result ((1) (1)))"],
    )
}

pub(super) fn mode_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asdf_vm_exec_path_injection_prepend_append_and_duplicates_are_exact(),
        asdf_vm_exec_path_injection_disabled_requires_discoverable_executable_without_mutation(),
        asdf_vm_exec_path_cleanup_removes_every_matching_occurrence_or_is_noop_when_disabled(),
        asdf_vm_mode_activation_and_deactivation_round_trip_path_mode_line_and_environment_state(),
        asdf_vm_global_minor_mode_invokes_activation_only_on_enable_and_deactivation_on_disable(),
        asdf_vm_mode_enable_and_disable_user_commands_forward_their_exact_numeric_arguments(),
    ]
}

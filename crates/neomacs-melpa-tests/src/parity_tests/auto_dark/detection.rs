use expect_test::expect;

use super::ParityBatchCase;

fn auto_dark_ns_applescript_adapter_uses_exact_program_and_truth_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_ns_applescript_adapter_uses_exact_program_and_truth_contract",
        r##"(let (calls)
          (cl-letf
              (((symbol-function 'ns-do-applescript)
                (lambda (program)
                  (push program calls)
                  auto-dark-test-ns-output)))
            (list
             (mapcar
              (lambda (output)
                (let ((auto-dark-test-ns-output
                       output))
                  (list
                   output
                   (auto-dark--is-dark-mode-ns))))
              '("true"
                "false"
                "\"true\""
                " true "
                ""
                nil))
             (length calls)
             (length
              (delete-dups calls))
             (car calls))))"##,
        expect![[
            r#"OK ((("true" t) ("false" nil) ("\"true\"" nil) (" true " nil) ("" nil) (nil nil)) 6 1 "tell application \"System Events\"\n        tell appearance preferences\n                if (dark mode) then\n                        return \"true\"\n                else\n                        return \"false\"\n                end if\n        end tell\nend tell")"#
        ]],
    )
}

fn auto_dark_mac_applescript_adapter_uses_quoted_truth_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_mac_applescript_adapter_uses_quoted_truth_contract",
        r##"(let (calls)
          (cl-letf
              (((symbol-function 'mac-do-applescript)
                (lambda (program)
                  (push program calls)
                  auto-dark-test-mac-output)))
            (list
             (mapcar
              (lambda (output)
                (let ((auto-dark-test-mac-output
                       output))
                  (list
                   output
                   (auto-dark--is-dark-mode-mac))))
              '("\"true\""
                "true"
                "\"false\""
                " \"true\" "
                ""
                nil))
             (length calls)
             (length
              (delete-dups calls))
             (car calls))))"##,
        expect![[
            r#"OK ((("\"true\"" t) ("true" nil) ("\"false\"" nil) (" \"true\" " nil) ("" nil) (nil nil)) 6 1 "tell application \"System Events\"\n        tell appearance preferences\n                if (dark mode) then\n                        return \"true\"\n                else\n                        return \"false\"\n                end if\n        end tell\nend tell")"#
        ]],
    )
}

fn auto_dark_current_applescript_mode_prefers_ns_then_mac_and_errors_without_support()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_current_applescript_mode_prefers_ns_then_mac_and_errors_without_support",
        r##"(let (events)
          (cl-letf
              (((symbol-function
                 'auto-dark--is-dark-mode-ns)
                (lambda ()
                  (push :ns events)
                  auto-dark-test-dark))
               ((symbol-function
                 'auto-dark--is-dark-mode-mac)
                (lambda ()
                  (push :mac events)
                  auto-dark-test-dark)))
            (let ((auto-dark-test-dark t))
              (cl-letf
                  (((symbol-function
                     'ns-do-applescript)
                    #'ignore)
                   ((symbol-function
                     'mac-do-applescript)
                    #'ignore))
                (let ((ns-result
                       (auto-dark--current-mode-applescript)))
                  (fmakunbound
                   'ns-do-applescript)
                  (let ((mac-result
                         (auto-dark--current-mode-applescript)))
                    (fmakunbound
                     'mac-do-applescript)
                    (list
                     ns-result
                     mac-result
                     (auto-dark-test-error-data
                      #'auto-dark--current-mode-applescript)
                     (nreverse events))))))))"##,
        expect![[
            r#"OK (dark dark (:error error ("No AppleScript support available in this Emacs build.  Try setting ‘auto-dark-allow-osascript‘ to t")) (:ns :mac))"#
        ]],
    )
}

fn auto_dark_shell_adapters_forward_exact_commands_and_parse_outputs() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_shell_adapters_forward_exact_commands_and_parse_outputs",
        r##"(let (calls)
          (cl-letf
              (((symbol-function
                 'shell-command-to-string)
                (lambda (command)
                  (push command calls)
                  (cond
                   ((string-prefix-p
                     "osascript "
                     command)
                    auto-dark-test-osascript-output)
                   ((string-prefix-p
                     "echo -n "
                     command)
                    auto-dark-test-termux-output)
                   (t
                    auto-dark-test-powershell-output)))))
            (let ((auto-dark-test-osascript-output
                   "  true\n")
                  (auto-dark-test-termux-output
                   "Night mode: yes")
                  (auto-dark-test-powershell-output
                   "PowerShell banner\n0\n"))
              (list
               (auto-dark--is-dark-mode-osascript)
               (auto-dark--is-dark-mode-termux)
               (auto-dark--is-dark-mode-powershell)
               (nreverse calls)))))"##,
        expect![[
            r#"OK (t t t ("osascript -e 'tell application \"System Events\" to tell appearance preferences to return dark mode'" "echo -n $(cmd uimode night 2>&1 </dev/null)" "powershell.exe -noprofile -noninteractive -nologo -ex bypass -command Get-ItemPropertyValue 'HKCU:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize' -Name AppsUseLightTheme"))"#
        ]],
    )
}

fn auto_dark_powershell_parser_selects_first_numeric_line_and_only_zero_is_dark() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dark_powershell_parser_selects_first_numeric_line_and_only_zero_is_dark",
        r##"(mapcar
          (lambda (output)
            (cl-letf
                (((symbol-function
                   'shell-command-to-string)
                  (lambda (_)
                    output)))
              (list
               output
               (auto-dark--is-dark-mode-powershell))))
          '("0"
            "1"
            "banner\n0\ntrailer"
            "banner\n  1  \n0"
            "00"
            "-1"
            "0.0"
            "noise 0"
            "\n\n"
            ""))"##,
        expect![[
            r#"OK (("0" t) ("1" nil) ("banner\n0\ntrailer" t) ("banner\n  1  \n0" nil) ("00" nil) ("-1" nil) ("0.0" nil) ("noise 0" nil) ("\n\n" nil) ("" nil))"#
        ]],
    )
}

fn auto_dark_winreg_adapter_forwards_exact_registry_query_and_type_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_winreg_adapter_forwards_exact_registry_query_and_type_contract",
        r##"(let (calls)
          (cl-letf
              (((symbol-function
                 'w32-read-registry)
                (lambda (&rest arguments)
                  (push arguments calls)
                  auto-dark-test-registry-value)))
            (list
             (mapcar
              (lambda (value)
                (let ((auto-dark-test-registry-value
                       value))
                  (list
                   value
                   (auto-dark--is-dark-mode-winreg))))
              '(0
                1
                "0"
                nil
                dark))
             (nreverse calls))))"##,
        expect![[
            r#"OK (((0 t) (1 nil) ("0" nil) (nil nil) (dark nil)) ((HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize" "AppsUseLightTheme") (HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize" "AppsUseLightTheme") (HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize" "AppsUseLightTheme") (HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize" "AppsUseLightTheme") (HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize" "AppsUseLightTheme")))"#
        ]],
    )
}

fn auto_dark_dbus_adapter_maps_portal_color_scheme_and_forwards_exact_call() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_dbus_adapter_maps_portal_color_scheme_and_forwards_exact_call",
        r##"(let (calls)
          (cl-letf
              (((symbol-function
                 'dbus-call-method)
                (lambda (&rest arguments)
                  (push arguments calls)
                  auto-dark-test-dbus-result)))
            (list
             (mapcar
              (lambda (result)
                (let ((auto-dark-test-dbus-result
                       result))
                  (list
                   result
                   (auto-dark-test-error-data
                    #'auto-dark--current-mode-dbus))))
              '(((1))
                ((0))
                ((2))
                ((3))
                (nil)
                nil))
             (nreverse calls))))"##,
        expect![[
            r#"OK (((((1)) (:ok dark)) (((0)) (:ok light)) (((2)) (:ok light)) (((3)) (:ok nil)) ((nil) (:ok nil)) (nil (:ok nil))) ((:session "org.freedesktop.portal.Desktop" "/org/freedesktop/portal/desktop" "org.freedesktop.portal.Settings" "Read" "org.freedesktop.appearance" "color-scheme") (:session "org.freedesktop.portal.Desktop" "/org/freedesktop/portal/desktop" "org.freedesktop.portal.Settings" "Read" "org.freedesktop.appearance" "color-scheme") (:session "org.freedesktop.portal.Desktop" "/org/freedesktop/portal/desktop" "org.freedesktop.portal.Settings" "Read" "org.freedesktop.appearance" "color-scheme") (:session "org.freedesktop.portal.Desktop" "/org/freedesktop/portal/desktop" "org.freedesktop.portal.Settings" "Read" "org.freedesktop.appearance" "color-scheme") (:session "org.freedesktop.portal.Desktop" "/org/freedesktop/portal/desktop" "org.freedesktop.portal.Settings" "Read" "org.freedesktop.appearance" "color-scheme") (:session "org.freedesktop.portal.Desktop" "/org/freedesktop/portal/desktop" "org.freedesktop.portal.Settings" "Read" "org.freedesktop.appearance" "color-scheme")))"#
        ]],
    )
}

fn auto_dark_current_system_mode_dispatches_every_configured_detector() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_current_system_mode_dispatches_every_configured_detector",
        r##"(let (events)
          (cl-letf
              (((symbol-function
                 'auto-dark--current-mode-applescript)
                (lambda ()
                  (push :applescript events)
                  'dark))
               ((symbol-function
                 'auto-dark--is-dark-mode-osascript)
                (lambda ()
                  (push :osascript events)
                  nil))
               ((symbol-function
                 'auto-dark--current-mode-dbus)
                (lambda ()
                  (push :dbus events)
                  'light))
               ((symbol-function
                 'auto-dark--is-dark-mode-powershell)
                (lambda ()
                  (push :powershell events)
                  t))
               ((symbol-function
                 'auto-dark--is-dark-mode-winreg)
                (lambda ()
                  (push :winreg events)
                  nil))
               ((symbol-function
                 'auto-dark--is-dark-mode-termux)
                (lambda ()
                  (push :termux events)
                  t))
               ((symbol-function 'frame-parameter)
                (lambda (&rest _)
                  nil))
               ((symbol-function
                 'frame-terminal-default-bg-mode)
                (lambda (&rest _)
                  nil)))
            (list
             (mapcar
              (lambda (method)
                (let ((auto-dark-detection-method
                       method)
                      (auto-dark--last-dark-mode-state
                       nil))
                  (list
                   method
                   (auto-dark--current-system-mode))))
              '(applescript
                osascript
                dbus
                powershell
                winreg
                termux))
             (nreverse events))))"##,
        expect![
            "OK (((applescript dark) (osascript light) (dbus light) (powershell dark) (winreg light) (termux dark)) (:applescript :osascript :dbus :powershell :winreg :termux))"
        ],
    )
}

fn auto_dark_current_system_mode_fallback_priority_and_warning_contract_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_current_system_mode_fallback_priority_and_warning_contract_match",
        r##"(cl-labels
          ((probe
            (frame terminal last)
            (cl-letf
                (((symbol-function 'frame-parameter)
                  (lambda (&rest _)
                    frame))
                 ((symbol-function
                   'frame-terminal-default-bg-mode)
                  (lambda (&rest _)
                    terminal)))
              (let ((auto-dark-detection-method
                     'unknown-method)
                    (auto-dark--last-dark-mode-state
                     last))
                (auto-dark-test-warning-data
                 #'auto-dark--current-system-mode)))))
          (list
           (probe 'dark 'light 'last)
           (probe nil 'light 'last)
           (probe nil nil 'dark)
           (probe nil nil nil)))"##,
        expect![[
            r#"OK ((dark nil) (light nil) (dark nil) ("Warning (auto-dark): couldn’t determine current system appearance" nil))"#
        ]],
    )
}

fn auto_dark_detection_method_feature_matrix_selects_each_supported_platform_path()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_detection_method_feature_matrix_selects_each_supported_platform_path",
        r##"(let (shell-calls dbus-calls)
          (cl-letf
              (((symbol-function 'ns-do-applescript)
                #'ignore)
               ((symbol-function
                 'dbus-list-activatable-names)
                (lambda (&rest arguments)
                  (push arguments dbus-calls)
                  auto-dark-test-dbus-names))
               ((symbol-function
                 'shell-command-to-string)
                (lambda (command)
                  (push command shell-calls)
                  auto-dark-test-shell-output)))
            (let ((darwin-builtin
                   (let ((system-type 'darwin)
                         (window-system 'ns)
                         (auto-dark-allow-osascript nil))
                     (auto-dark--determine-detection-method)))
                  (darwin-shell
                   (let ((system-type 'darwin)
                         (window-system 'x)
                         (auto-dark-allow-osascript t))
                     (auto-dark--determine-detection-method)))
                  (linux-dbus
                   (let ((system-type 'gnu/linux)
                         (features
                          (cons 'dbus features))
                         (auto-dark-test-dbus-names
                          '("org.freedesktop.portal.Desktop"))
                         (auto-dark-test-shell-output ""))
                     (auto-dark--determine-detection-method)))
                  (linux-termux
                   (let ((system-type 'gnu/linux)
                         (features
                          (cons 'dbus features))
                         (auto-dark-test-dbus-names nil)
                         (auto-dark-test-shell-output
                          "/data/data/com.termux/files/usr/bin/termux-fix-shebang"))
                     (auto-dark--determine-detection-method)))
                  (windows-powershell
                   (let ((system-type 'windows-nt)
                         (auto-dark-allow-powershell t))
                     (auto-dark--determine-detection-method)))
                  (windows-registry
                   (let ((system-type 'windows-nt)
                         (auto-dark-allow-powershell nil))
                     (auto-dark--determine-detection-method)))
                  (wsl-powershell
                   (let ((system-type 'gnu/linux)
                         (features
                          (remq 'dbus features))
                         (auto-dark-allow-powershell t)
                         (auto-dark-test-shell-output
                          "6.6.87.2-microsoft-standard-WSL2"))
                     (auto-dark--determine-detection-method))))
              (list
               darwin-builtin
               darwin-shell
               linux-dbus
               linux-termux
               windows-powershell
               windows-registry
               wsl-powershell
               (nreverse dbus-calls)
               (nreverse shell-calls)))))"##,
        expect![
            "OK (applescript osascript dbus termux powershell winreg powershell ((:session) (:session) (:session)) (\"command -v termux-fix-shebang\" \"command -v termux-fix-shebang\" \"uname -r\"))"
        ],
    )
}

fn auto_dark_detection_method_unsupported_platform_warns_and_returns_display_warning_result()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_detection_method_unsupported_platform_warns_and_returns_display_warning_result",
        r##"(let ((system-type 'berkeley-unix)
                               (window-system nil)
                               (features
                                (remq 'dbus features))
                               warnings)
          (cl-letf
              (((symbol-function 'display-warning)
                (lambda (&rest arguments)
                  (push arguments warnings)
                  :warning-result)))
            (list
             (auto-dark--determine-detection-method)
             (nreverse warnings))))"##,
        expect![[
            r#"OK ("Error (auto-dark): Could not determine a viable theme detection mechanism! You can use ‘auto-dark-toggle-appearance’ to manually switch between modes." nil)"#
        ]],
    )
    .fresh_process()
}

pub(super) fn detection_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_dark_ns_applescript_adapter_uses_exact_program_and_truth_contract(),
        auto_dark_mac_applescript_adapter_uses_quoted_truth_contract(),
        auto_dark_current_applescript_mode_prefers_ns_then_mac_and_errors_without_support(),
        auto_dark_shell_adapters_forward_exact_commands_and_parse_outputs(),
        auto_dark_powershell_parser_selects_first_numeric_line_and_only_zero_is_dark(),
        auto_dark_winreg_adapter_forwards_exact_registry_query_and_type_contract(),
        auto_dark_dbus_adapter_maps_portal_color_scheme_and_forwards_exact_call(),
        auto_dark_current_system_mode_dispatches_every_configured_detector(),
        auto_dark_current_system_mode_fallback_priority_and_warning_contract_match(),
        auto_dark_detection_method_feature_matrix_selects_each_supported_platform_path(),
        auto_dark_detection_method_unsupported_platform_warns_and_returns_display_warning_result(),
    ]
}

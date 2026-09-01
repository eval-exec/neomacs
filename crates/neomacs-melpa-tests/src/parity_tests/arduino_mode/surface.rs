use expect_test::expect;

use super::ParityBatchCase;

fn installed_descriptor_dependencies_and_files_identify_the_exact_melpa_build() -> ParityBatchCase {
    ParityBatchCase::value(
        "installed_descriptor_dependencies_and_files_identify_the_exact_melpa_build",
        r##"(let* ((descriptor
                          (cadr (assq 'arduino-mode package-alist)))
                         (source (getenv "NEOMACS_PACKAGE_SOURCE"))
                         (directory (file-name-directory source)))
                    (list
                     (featurep 'arduino-mode)
                     (package-desc-name descriptor)
                     (package-version-join
                      (package-desc-version descriptor))
                     (package-desc-reqs descriptor)
                     (package-desc-summary descriptor)
                     (file-name-nondirectory source)
                     (sort
                      (mapcar
                       #'file-name-nondirectory
                       (directory-files directory t "\\.el\\'"))
                      #'string<)))"##,
        expect![[
            r#"OK (t arduino-mode "20240527.1603" ((emacs (25 1)) (spinner (1 7 3))) "Major mode for editing Arduino code." "arduino-mode.el" ("arduino-mode-autoloads.el" "arduino-mode-init.el" "arduino-mode-pkg.el" "arduino-mode.el" "ede-arduino.el" "flycheck-arduino.el" "ob-arduino.el"))"#
        ]],
    )
}

fn main_module_complete_callable_surface_has_exact_arglists_and_command_status() -> ParityBatchCase
{
    ParityBatchCase::value(
        "main_module_complete_callable_surface_has_exact_arglists_and_command_status",
        r##"(mapcar
                    (lambda (symbol)
                      (list
                       symbol
                       (copy-tree
                        (help-function-arglist symbol t))
                       (commandp symbol)))
                    '(arduino-upload
                      arduino-verify
                      arduino-open-with-arduino
                      arduino-install-boards
                      arduino-install-library
                      arduino-serial-monitor
                      arduino-sketch-new
                      arduino-generate-include-path-file
                      arduino-mode))"##,
        expect![
            "OK ((arduino-upload nil t) (arduino-verify nil t) (arduino-open-with-arduino nil t) (arduino-install-boards (board) t) (arduino-install-library (library) t) (arduino-serial-monitor (port speed) t) (arduino-sketch-new (sketch) t) (arduino-generate-include-path-file nil t) (arduino-mode nil t))"
        ],
    )
}

fn main_module_custom_options_and_runtime_state_have_exact_metadata() -> ParityBatchCase {
    ParityBatchCase::value(
        "main_module_custom_options_and_runtime_state_have_exact_metadata",
        r##"(list
                    (mapcar
                     (lambda (symbol)
                       (list
                        symbol
                        (default-value symbol)
                        (get symbol 'custom-type)
                        (get symbol 'custom-group)
                        (documentation-property
                         symbol 'variable-documentation t)))
                     '(arduino-mode-home
                       arduino-font-lock-extra-types
                       arduino-executable
                       arduino-spinner-type))
                    (list
                     arduino-upload-process-buf
                     arduino-verify-process-buf
                     arduino-open-process-buf
                     (eq
                      arduino-font-lock-keywords
                      arduino-font-lock-keywords-3)
                     (syntax-table-p arduino-mode-syntax-table)
                     (abbrev-table-p arduino-mode-abbrev-table))
                    (get 'arduino-mode 'derived-mode-parent)
                    (get 'arduino 'custom-group)
                    (get 'arduino-mode 'custom-group))"##,
        expect![[
            r#"OK (((arduino-mode-home "~/Arduino" directory nil "The path of ARDUINO_HOME.") (arduino-font-lock-extra-types nil list nil "List of extra types (aside from type keywords) to recognize in Arduino mode.\nEach list item should be a regexp matching a single identifier.") (arduino-executable "arduino" string nil "The arduino program executable name.") (arduino-spinner-type progress-bar symbol nil "The spinner type for arduino processes.\n\nValue is a symbol.  The possible values are the symbols in the\n`spinner-types' variable.")) (nil nil nil t t t) c-mode ((arduino-mode custom-group) (arduino-font-lock-extra-types custom-variable) (arduino-executable custom-variable) (arduino-spinner-type custom-variable)) ((arduino-mode-home custom-variable)))"#
        ]],
    )
    .fresh_process()
}

fn mode_keymap_and_menu_expose_every_documented_operation() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_keymap_and_menu_expose_every_documented_operation",
        r##"(list
                    (eq
                     (keymap-parent arduino-mode-map)
                     c-mode-base-map)
                    (mapcar
                     (lambda (key)
                       (cons
                        key
                        (lookup-key arduino-mode-map (kbd key))))
                     '("C-c C-c" "C-c C-v"
                       "C-c C-m" "C-c C-x"))
                    (mapcar
                     (lambda (label)
                       (let ((item
                              (assoc label (cdr arduino-menu))))
                         (list
                          label
                          (and item
                               (aref (cdr item) 1))
                          (and item
                               (aref (cdr item) 2)))))
                     '("Upload" "Verify"
                       "Open with Arduino"
                       "Serial monitor")))"##,
        expect![[
            r#"OK (t (("C-c C-c" . arduino-upload) ("C-c C-v" . arduino-verify) ("C-c C-m" . arduino-serial-monitor) ("C-c C-x" . arduino-open-with-arduino)) (("Upload" nil nil) ("Verify" nil nil) ("Open with Arduino" nil nil) ("Serial monitor" nil nil)))"#
        ]],
    )
}

fn legacy_init_file_registers_both_extensions_and_two_lazy_entry_points() -> ParityBatchCase {
    ParityBatchCase::value(
        "legacy_init_file_registers_both_extensions_and_two_lazy_entry_points",
        r##"(list
                    (featurep 'arduino-mode)
                    (mapcar
                     (lambda (symbol)
                       (list
                        symbol
                        (fboundp symbol)
                        (and
                         (fboundp symbol)
                         (autoloadp (symbol-function symbol)))
                        (commandp symbol)))
                     '(arduino-mode
                       ede-arduino-preferences-file))
                    (mapcar
                     (lambda (filename)
                       (let ((buffer
                              (get-buffer-create
                               (concat " *arduino-init-" filename "*"))))
                         (unwind-protect
                             (with-current-buffer buffer
                               (setq buffer-file-name filename)
                               (set-auto-mode)
                               major-mode)
                           (kill-buffer buffer))))
                     '("Blink.ino" "Legacy.pde" "notes.txt")))"##,
        expect![
            "OK (nil ((arduino-mode t t t) (ede-arduino-preferences-file t t t)) (arduino-mode arduino-mode text-mode))"
        ],
    )
}

fn ede_module_complete_callable_surface_has_exact_arglists_and_command_status() -> ParityBatchCase {
    ParityBatchCase::value(
        "ede_module_complete_callable_surface_has_exact_arglists_and_command_status",
        r##"(mapcar
                    (lambda (symbol)
                      (list
                       symbol
                       (copy-tree
                        (help-function-arglist symbol t))
                       (commandp symbol)))
                    '(ede-arduino-sync
                      ede-arduino-read-prefs
                      ede-arduino
                      ede-arduino-find-install
                      ede-arduino-Arduino.mk
                      ede-arduino-Arduino-Version
                      ede-arduino-boards.txt
                      ede-arduino-libdir
                      ede-arduino-board-data
                      ede-arduino-root
                      ede-arduino-file
                      ede-arduino-load
                      ede-arduino-upload
                      cedet-arduino-serial-monitor
                      ede-arduino-guess-sketch
                      ede-arduino-guess-libs))"##,
        expect![
            "OK ((ede-arduino-sync nil t) (ede-arduino-read-prefs (prefsfile) nil) (ede-arduino nil t) (ede-arduino-find-install (&optional full-path) nil) (ede-arduino-Arduino.mk nil nil) (ede-arduino-Arduino-Version nil nil) (ede-arduino-boards.txt nil nil) (ede-arduino-libdir (&optional library) nil) (ede-arduino-board-data (boardname) nil) (ede-arduino-root (&optional dir basefile) nil) (ede-arduino-file (&optional dir) nil) (ede-arduino-load (dir &optional _rootproj) nil) (ede-arduino-upload nil t) (cedet-arduino-serial-monitor nil t) (ede-arduino-guess-sketch nil nil) (ede-arduino-guess-libs nil t))"
        ],
    )
}

fn ede_custom_options_classes_and_project_registration_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "ede_custom_options_classes_and_project_registration_are_exact",
        r##"(list
                    (mapcar
                     (lambda (symbol)
                       (list
                        symbol
                        (default-value symbol)
                        (get symbol 'custom-type)
                        (get symbol 'custom-group)))
                     '(ede-arduino-makefile-name
                       ede-arduino-make-command
                       ede-arduino-container-prefix
                       ede-arduino-preferences-file
                       ede-arduino-boards-file
                       ede-arduino-avrdude-baudrate
                       ede-arduino-arduino-command
                       ede-arduino-appdir))
                    (mapcar
                     (lambda (class)
                       (list
                        class
                        (class-p class)
                        (mapcar
                         #'eieio-slot-descriptor-name
                         (eieio-class-slots class))))
                     '(ede-arduino-prefs
                       ede-arduino-board
                       ede-arduino-target
                       ede-arduino-project))
                    (let ((entry
                           (seq-find
                            (lambda (item)
                              (and
                               (object-of-class-p
                                item 'ede-project-autoload)
                               (equal
                                (oref item name)
                                "Arduino sketch")))
                            ede-project-class-files)))
                      (and entry
                           (list
                            (oref entry file)
                            (oref entry proj-file)
                            (oref entry proj-root)
                            (oref entry load-type)
                            (oref entry class-sym)
                            (oref entry safe-p)
                            (oref entry new-p)))))"##,
        expect![[
            r#"OK (((ede-arduino-makefile-name "Makefile" file nil) (ede-arduino-make-command "make" file nil) (ede-arduino-container-prefix nil string nil) (ede-arduino-preferences-file "~/.arduino/preferences.txt" string nil) (ede-arduino-boards-file "hardware/arduino/avr/boards.txt" string nil) (ede-arduino-avrdude-baudrate nil string nil) (ede-arduino-arduino-command "arduino" string nil) (ede-arduino-appdir nil directory nil)) ((ede-arduino-prefs t (timestamp prefssize board port sketchbook boardobj)) (ede-arduino-board t (name protocol speed maximum-size mcu f_cpu core)) (ede-arduino-target t (expanded object-name name path source versionsource)) (ede-arduino-project t (expanded name version directory dirinode file rootproject subproj targets locate-obj tool-cache mailinglist web-site-url web-site-directory web-site-file ftp-site ftp-upload-site configurations configuration-default local-variables))) (ede-arduino ede-arduino-file ede-arduino-root ede-arduino-load ede-arduino-project t t))"#
        ]],
    )
}

pub(super) fn surface_arduino_mode_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        installed_descriptor_dependencies_and_files_identify_the_exact_melpa_build(),
        main_module_complete_callable_surface_has_exact_arglists_and_command_status(),
        main_module_custom_options_and_runtime_state_have_exact_metadata(),
        mode_keymap_and_menu_expose_every_documented_operation(),
    ]
}

pub(super) fn surface_arduino_init_batch_cases() -> Vec<ParityBatchCase> {
    vec![legacy_init_file_registers_both_extensions_and_two_lazy_entry_points()]
}

pub(super) fn surface_ede_arduino_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ede_module_complete_callable_surface_has_exact_arglists_and_command_status(),
        ede_custom_options_classes_and_project_registration_are_exact(),
    ]
}

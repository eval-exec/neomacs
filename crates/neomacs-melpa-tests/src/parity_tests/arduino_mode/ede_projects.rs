use expect_test::expect;

use super::ParityBatchCase;

fn project_root_and_primary_file_follow_real_sketchbook_directory_layout() -> ParityBatchCase {
    ParityBatchCase::value(
        "project_root_and_primary_file_follow_real_sketchbook_directory_layout",
        r##"(let* ((sketchbook
                          (make-temp-file
                           "arduino-sketchbook-" t))
                         (project
                          (expand-file-name
                           "Blink" sketchbook))
                         (subdir
                          (expand-file-name "src" project))
                         (pde
                          (expand-file-name
                           "Blink.pde" project))
                         (ino
                          (expand-file-name
                           "Blink.ino" project))
                         (prefs
                          (make-instance
                           'ede-arduino-prefs)))
                    (unwind-protect
                        (progn
                          (make-directory subdir t)
                          (with-temp-file pde
                            (insert "void setup() {}\n"))
                          (with-temp-file ino
                            (insert "void loop() {}\n"))
                          (oset
                           prefs sketchbook
                           (file-name-as-directory
                            sketchbook))
                          (cl-letf
                              (((symbol-function
                                 'ede-arduino-sync)
                                (lambda () prefs)))
                            (let ((pde-buffer
                                   (find-file-noselect
                                    pde)))
                              (unwind-protect
                                  (list
                                   (file-equal-p
                                    (ede-arduino-root
                                     subdir)
                                    project)
                                   (file-equal-p
                                    (ede-arduino-root
                                     subdir t)
                                    pde)
                                   (file-equal-p
                                    (ede-arduino-file
                                     subdir)
                                    pde)
                                   (progn
                                     (kill-buffer
                                      pde-buffer)
                                     (file-equal-p
                                      (ede-arduino-file
                                       subdir)
                                      ino)))
                                (when
                                    (buffer-live-p
                                     pde-buffer)
                                  (kill-buffer
                                   pde-buffer))))))
                      (delete-directory sketchbook t)))"##,
        expect!["OK (t t t t)"],
    )
}

fn project_root_rejects_paths_outside_sketchbook_and_nonexistent_projects() -> ParityBatchCase {
    ParityBatchCase::value(
        "project_root_rejects_paths_outside_sketchbook_and_nonexistent_projects",
        r##"(let* ((root
                          (make-temp-file
                           "arduino-root-boundaries-" t))
                         (sketchbook
                          (expand-file-name
                           "Sketches" root))
                         (outside
                          (expand-file-name
                           "Sketches-copy/Project" root))
                         (missing
                          (expand-file-name
                           "Sketches/Missing/src" root))
                         (prefs
                          (make-instance
                           'ede-arduino-prefs)))
                    (unwind-protect
                        (progn
                          (make-directory sketchbook t)
                          (make-directory outside t)
                          (oset
                           prefs sketchbook
                           (file-name-as-directory
                            sketchbook))
                          (cl-letf
                              (((symbol-function
                                 'ede-arduino-sync)
                                (lambda () prefs)))
                            (list
                             (ede-arduino-root outside)
                             (ede-arduino-root missing)
                             (ede-arduino-file outside))))
                      (delete-directory root t)))"##,
        expect!["OK (nil nil nil)"],
    )
}

fn project_loader_creates_registers_and_populates_a_new_ino_project() -> ParityBatchCase {
    ParityBatchCase::value(
        "project_loader_creates_registers_and_populates_a_new_ino_project",
        r##"(let* ((root
                          (make-temp-file
                           "arduino-project-load-" t))
                         (project-dir
                          (expand-file-name "Robot" root))
                         (ino
                          (expand-file-name
                           "Robot.ino" project-dir))
                         registered messages)
                    (unwind-protect
                        (progn
                          (make-directory project-dir t)
                          (with-temp-file ino
                            (insert "void loop() {}\n"))
                          (cl-letf
                              (((symbol-function
                                 'ede-arduino-root)
                                (lambda (&optional
                                         _dir _basefile)
                                  project-dir))
                               ((symbol-function
                                 'ede-directory-get-open-project)
                                (lambda (_root) nil))
                               ((symbol-function
                                 'ede-arduino-sync)
                                (lambda () :synced))
                               ((symbol-function
                                 'ede-add-project-to-global-list)
                                (lambda (project)
                                  (setq registered
                                        project)))
                               ((symbol-function 'message)
                                (lambda (format-string &rest args)
                                  (push
                                   (apply
                                    #'format
                                    format-string args)
                                   messages))))
                            (let ((result
                                   (ede-arduino-load
                                    project-dir)))
                              (list
                               (object-of-class-p
                                result
                                'ede-arduino-project)
                               (eq result registered)
                               (oref result name)
                               (file-equal-p
                                (oref result directory)
                                project-dir)
                               (file-equal-p
                                (oref result file)
                                ino)
                               (oref result targets)
                               (nreverse messages)))))
                      (delete-directory root t)))"##,
        expect![[
            r#"OK (t t "Robot" t t nil ("Creating new project" "Obsolete name argument \"Robot\" passed to ede-arduino-project constructor"))"#
        ]],
    )
}

fn project_loader_reuses_an_existing_open_project_without_registering_another() -> ParityBatchCase {
    ParityBatchCase::value(
        "project_loader_reuses_an_existing_open_project_without_registering_another",
        r##"(let* ((existing
                          (make-instance
                           'ede-arduino-project
                           :name "Existing"
                           :directory "/workspace/Existing/"
                           :file
                           "/workspace/Existing/Existing.ino"
                           :targets nil))
                         events)
                    (cl-letf
                        (((symbol-function 'ede-arduino-root)
                          (lambda (&optional _dir _basefile)
                            "/workspace/Existing"))
                         ((symbol-function
                           'ede-directory-get-open-project)
                          (lambda (_root) existing))
                         ((symbol-function 'ede-arduino-sync)
                          (lambda ()
                            (push :sync events)))
                         ((symbol-function
                           'ede-add-project-to-global-list)
                          (lambda (_project)
                            (push :register events)))
                         ((symbol-function 'message)
                          (lambda (format-string &rest args)
                            (push
                             (apply
                              #'format format-string args)
                             events))))
                      (let ((result
                             (ede-arduino-load
                              "/workspace/Existing/")))
                        (list
                         (eq result existing)
                         (nreverse events)))))"##,
        expect![[r#"OK (t (:sync "Opening existing project"))"#]],
    )
}

fn target_lookup_creates_one_directory_target_then_reuses_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "target_lookup_creates_one_directory_target_then_reuses_it",
        r##"(let* ((root
                          (make-temp-file
                           "arduino-target-" t))
                         (project
                          (make-instance
                           'ede-arduino-project
                           :name "TargetDemo"
                           :directory
                           (file-name-as-directory root)
                           :file
                           (expand-file-name
                            "TargetDemo.ino" root)
                           :targets nil)))
                    (unwind-protect
                        (let ((default-directory
                                (file-name-as-directory root)))
                          (let ((first
                                 (ede-find-target
                                  project
                                  (current-buffer)))
                                second)
                            (setq second
                                  (ede-find-target
                                   project
                                   (current-buffer)))
                            (list
                             (eq first second)
                             (length
                              (oref project targets))
                             (equal
                              (oref first name)
                              (file-name-nondirectory
                               (directory-file-name
                                root)))
                             (file-equal-p
                              (oref first path)
                              root)
                             (oref first source))))
                      (delete-directory root t)))"##,
        expect!["OK (t 1 t t nil)"],
    )
}

fn upload_and_target_compile_delegate_to_current_project_with_exact_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "upload_and_target_compile_delegate_to_current_project_with_exact_commands",
        r##"(let* ((project
                          (make-instance
                           'ede-arduino-project
                           :name "CompileDemo"
                           :directory "/workspace/CompileDemo/"
                           :file
                           "/workspace/CompileDemo/CompileDemo.ino"
                           :targets nil))
                         (target
                          (make-instance
                           'ede-arduino-target
                           :name "CompileDemo"
                           :path
                           "/workspace/CompileDemo/"
                           :source nil))
                         (ede-arduino-make-command
                          "gmake")
                         events)
                    (cl-letf
                        (((symbol-function 'ede-current-project)
                          (lambda () project))
                         ((symbol-function
                           'project-compile-project)
                          (lambda (proj &optional command)
                            (push
                             (list proj command)
                             events)
                            :compiled)))
                      (list
                       (ede-arduino-upload)
                       (project-compile-target
                        target "gmake verify")
                       (mapcar
                        (lambda (event)
                          (list
                           (eq
                            (car event)
                            project)
                           (cadr event)))
                        (nreverse events)))))"##,
        expect![[r#"OK (:compiled :compiled ((t "gmake all upload") (t "gmake verify")))"#]],
    )
}

fn project_compile_creates_makefile_before_invoking_requested_or_default_command() -> ParityBatchCase
{
    ParityBatchCase::value(
        "project_compile_creates_makefile_before_invoking_requested_or_default_command",
        r##"(let* ((project
                          (make-instance
                           'ede-arduino-project
                           :name "CompileDemo"
                           :directory "/workspace/CompileDemo/"
                           :file
                           "/workspace/CompileDemo/CompileDemo.ino"
                           :targets nil))
                         (ede-arduino-make-command
                          "gmake")
                         events)
                    (cl-letf
                        (((symbol-function
                           'ede-arduino-create-makefile)
                          (lambda (proj)
                            (push
                             (list :makefile proj)
                             events)))
                         ((symbol-function 'compile)
                          (lambda (command)
                            (push
                             (list :compile command)
                             events)
                            :compilation)))
                      (list
                       (project-compile-project
                        project "gmake all upload")
                       (project-compile-project project)
                       (mapcar
                        (lambda (event)
                          (if
                              (eq
                               (car event)
                               :makefile)
                              (list
                               :makefile
                               (eq
                                (cadr event)
                                project))
                            event))
                        (nreverse events)))))"##,
        expect![[
            r#"OK (:compilation :compilation ((:makefile t) (:compile "gmake all upload") (:makefile t) (:compile "gmake")))"#
        ]],
    )
}

fn project_debug_target_reports_the_explicit_unsupported_contract() -> ParityBatchCase {
    ParityBatchCase::signal(
        "project_debug_target_reports_the_explicit_unsupported_contract",
        r##"(let ((target
                         (make-instance
                          'ede-arduino-target
                          :name "Demo"
                          :path "/workspace/Demo/"
                          :source nil)))
                    (project-debug-target target))"##,
        expect![[r#"ERR (error "No Debugger support for Arduino")"#]],
    )
}

fn serial_monitor_uses_active_preference_port_and_switches_to_line_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "serial_monitor_uses_active_preference_port_and_switches_to_line_mode",
        r##"(let ((prefs
                         (make-instance
                          'ede-arduino-prefs))
                        events)
                    (oset prefs port "/dev/ttyACM7")
                    (cl-letf
                        (((symbol-function 'ede-arduino-sync)
                          (lambda ()
                            (push :sync events)
                            prefs))
                         ((symbol-function 'serial-term)
                          (lambda (port speed)
                            (push
                             (list :serial port speed)
                             events)
                            :terminal))
                         ((symbol-function 'term-line-mode)
                          (lambda ()
                            (push :line-mode events)
                            :line)))
                      (list
                       (cedet-arduino-serial-monitor)
                       (nreverse events))))"##,
        expect![[r#"OK (:line (:sync (:serial "/dev/ttyACM7" 9600) :line-mode))"#]],
    )
}

fn preprocessor_map_keeps_builtin_levels_and_merges_refreshed_semantic_defines() -> ParityBatchCase
{
    ParityBatchCase::value(
        "preprocessor_map_keeps_builtin_levels_and_merges_refreshed_semantic_defines",
        r##"(let ((target
                         (make-instance
                          'ede-arduino-target
                          :name "Demo"
                          :path "/workspace/Demo/"
                          :source nil))
                        events)
                    (cl-letf
                        (((symbol-function
                           'ede-arduino-find-install)
                          (lambda (&optional _full)
                            "/opt/arduino"))
                         ((symbol-function 'file-exists-p)
                          (lambda (file)
                            (string-suffix-p
                             "wiring.h" file)))
                         ((symbol-function
                           'semanticdb-file-table-object)
                          (lambda (file)
                            (push
                             (list :table file)
                             events)
                            'fake-table))
                         ((symbol-function
                           'semanticdb-needs-refresh-p)
                          (lambda (table)
                            (push
                             (list :needs-refresh table)
                             events)
                            t))
                         ((symbol-function
                           'semanticdb-refresh-table)
                          (lambda (table)
                            (push
                             (list :refresh table)
                             events)))
                         ((symbol-function 'eieio-oref)
                          (lambda (object slot)
                            (if
                                (and
                                 (eq object 'fake-table)
                                 (eq slot
                                     'lexical-table))
                                '(("CUSTOM_PIN" . "42"))
                              (error
                               "Unexpected oref: %S %S"
                               object slot)))))
                      (list
                       (ede-preprocessor-map target)
                       (nreverse events))))"##,
        expect![[
            r#"OK ((("HIGH" . "0x1") ("LOW" . "0x0") ("CUSTOM_PIN" . "42")) ((:table "/opt/arduino/hardware/arduino/cores/arduino/wiring.h") (:needs-refresh fake-table) (:refresh fake-table)))"#
        ]],
    )
}

fn system_include_path_combines_core_and_each_detected_library() -> ParityBatchCase {
    ParityBatchCase::value(
        "system_include_path_combines_core_and_each_detected_library",
        r##"(let ((target
                         (make-instance
                          'ede-arduino-target
                          :name "Demo"
                          :path "/workspace/Demo/"
                          :source nil))
                        events)
                    (cl-letf
                        (((symbol-function 'ede-arduino-sync)
                          (lambda ()
                            (push :sync events)
                            :prefs))
                         ((symbol-function
                           'ede-arduino-find-install)
                          (lambda (&optional _full)
                            "/opt/arduino"))
                         ((symbol-function
                           'ede-arduino-guess-libs)
                          (lambda ()
                            '("Servo"
                              "Ethernet/utility"))))
                      (list
                       (ede-system-include-path target)
                       (nreverse events))))"##,
        expect![[
            r#"OK (("/opt/arduino/hardware/arduino/cores/arduino" "/opt/arduino/libraries/Servo" "/opt/arduino/libraries/Ethernet/utility") (:sync))"#
        ]],
    )
}

fn sketch_guessing_prefers_pde_then_ino_and_reports_missing_primary_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "sketch_guessing_prefers_pde_then_ino_and_reports_missing_primary_file",
        r##"(let* ((root
                          (make-temp-file
                           "arduino-guess-sketch-" t))
                         (project
                          (make-instance
                           'ede-arduino-project
                           :name "Guess"
                           :directory
                           (file-name-as-directory root)
                           :file
                           (expand-file-name
                            "Guess.ino" root)
                           :targets nil))
                         (pde
                          (expand-file-name "Guess.pde" root))
                         (ino
                          (expand-file-name "Guess.ino" root))
                         (ede-object-project project))
                    (unwind-protect
                        (progn
                          (with-temp-file pde
                            (insert "pde"))
                          (with-temp-file ino
                            (insert "ino"))
                          (let ((first
                                 (file-name-nondirectory
                                  (ede-arduino-guess-sketch))))
                            (delete-file pde)
                            (let ((second
                                   (file-name-nondirectory
                                    (ede-arduino-guess-sketch))))
                              (delete-file ino)
                              (list
                               first second
                               (condition-case error-data
                                   (ede-arduino-guess-sketch)
                                 (error
                                  (list
                                   (car error-data)
                                   (string-match-p
                                    (concat
                                     "\\`Cannot guess primary "
                                     "sketch file for project "
                                     "#<ede-arduino-project ")
                                    (cadr error-data)))))))))
                      (delete-directory root t)))"##,
        expect![[r#"OK ("Guess.pde" "Guess.ino" (error 0))"#]],
    )
}

pub(super) fn ede_projects_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        project_root_and_primary_file_follow_real_sketchbook_directory_layout(),
        project_root_rejects_paths_outside_sketchbook_and_nonexistent_projects(),
        project_loader_creates_registers_and_populates_a_new_ino_project(),
        project_loader_reuses_an_existing_open_project_without_registering_another(),
        target_lookup_creates_one_directory_target_then_reuses_it(),
        upload_and_target_compile_delegate_to_current_project_with_exact_commands(),
        project_compile_creates_makefile_before_invoking_requested_or_default_command(),
        project_debug_target_reports_the_explicit_unsupported_contract(),
        serial_monitor_uses_active_preference_port_and_switches_to_line_mode(),
        preprocessor_map_keeps_builtin_levels_and_merges_refreshed_semantic_defines(),
        system_include_path_combines_core_and_each_detected_library(),
        sketch_guessing_prefers_pde_then_ino_and_reports_missing_primary_file(),
    ]
}

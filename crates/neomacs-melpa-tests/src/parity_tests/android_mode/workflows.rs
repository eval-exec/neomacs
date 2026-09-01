use expect_test::expect;

use super::ParityBatchCase;

fn opening_a_file_in_a_gradle_project_turns_the_mode_on_and_binds_its_prefix() -> ParityBatchCase {
    ParityBatchCase::value(
        "opening_a_file_in_a_gradle_project_turns_the_mode_on_and_binds_its_prefix",
        r##"
        ;; android-mode puts itself on `find-file-hook', so opening any file
        ;; under a project whose builder root file is present turns it on with
        ;; no configuration at all.  Every documented `C-c a' key has to reach
        ;; its command, and a file outside such a project must be left alone.
        (amd-test-with-project
         (list
          :root (amd-test-normalize (android-root))
          :builder android-mode-builder
          :root-file (plist-get android-mode-root-file-plist android-mode-builder)
          :mode-on android-mode
          :lighter (amd-test-copy (cadr (assq 'android-mode minor-mode-alist)))
          :bindings (sort (mapcar (lambda (spec)
                                    (list (amd-test-copy (car spec))
                                          (key-binding (read-kbd-macro
                                                        (concat android-mode-key-prefix
                                                                " " (car spec))))))
                                  android-mode-keys)
                          (lambda (a b) (string< (car a) (car b))))
          :outside (let ((other (progn
                                  (amd-test-write-file (amd-test-path "elsewhere/notes.txt")
                                                       "not an android project\n")
                                  (find-file-noselect (amd-test-path "elsewhere/notes.txt")))))
                     (unwind-protect
                         (with-current-buffer other
                           (list :mode android-mode :root (android-root)))
                       (kill-buffer other)))))
    "##,
        expect![[
            r#"OK (:root "[SANDBOX]/workspace/inventory/" :builder gradle :root-file "gradlew" :mode-on t :lighter " Android" :bindings (("C" android-build-clean) ("a" android-start-app) ("c" android-build-debug) ("d" android-start-ddms) ("e" android-start-emulator) ("i" android-build-install) ("l" android-logcat) ("r" android-build-reinstall) ("t" android-build-test) ("u" android-build-uninstall)) :outside (:mode nil :root nil))"#
        ]],
    )
}

fn building_runs_the_projects_own_gradle_wrapper_from_the_project_root() -> ParityBatchCase {
    ParityBatchCase::value(
        "building_runs_the_projects_own_gradle_wrapper_from_the_project_root",
        r##"
        ;; `C-c a c' builds a debug APK.  For the gradle builder that means
        ;; running the wrapper script checked into the project - not a tool on
        ;; PATH - from the project root, through `compile', so the user gets a
        ;; real compilation buffer.  `C-c a C' cleans with the same wrapper.
        ;; The wrapper here is a real shell script, so the compilation buffer
        ;; holds real output.
        (amd-test-with-project
         (list
          :debug
          (progn
            (execute-kbd-macro (read-kbd-macro (concat android-mode-key-prefix " c")))
            ;; Wait for the compilation process to die, not merely for its
            ;; output: the sentinel that writes the closing line runs last.
            (amd-test-wait
             (lambda () (and (get-buffer "*compilation*")
                             (not (get-buffer-process "*compilation*"))
                             (with-current-buffer "*compilation*"
                               (save-excursion (goto-char (point-min))
                                               (search-forward "actionable task" nil t))))))
            (with-current-buffer "*compilation*"
              (list :mode major-mode
                    :directory (amd-test-normalize default-directory)
                    :text (amd-test-normalize
                           (replace-regexp-in-string
                            "\\(started\\|finished\\) at .*" "\\1 at <TIME>"
                            (buffer-substring-no-properties (point-min) (point-max)))))))
          :clean
          (progn
            (execute-kbd-macro (read-kbd-macro (concat android-mode-key-prefix " C")))
            (amd-test-wait (lambda () (and (> (length (amd-test-commands)) 1)
                                           (not (get-buffer-process "*compilation*")))))
            (amd-test-commands))
          :install-and-uninstall
          (progn
            (android-gradle-installDebug)
            (amd-test-wait (lambda () (and (> (length (amd-test-commands)) 2)
                                           (not (get-buffer-process "*compilation*")))))
            (android-gradle-uninstallDebug)
            (amd-test-wait (lambda () (and (> (length (amd-test-commands)) 3)
                                           (not (get-buffer-process "*compilation*")))))
            (amd-test-commands))))
    "##,
        expect![[
            r#"OK (:debug (:mode compilation-mode :directory "[SANDBOX]/workspace/inventory/" :text "-*- mode: compilation; default-directory: \"[SANDBOX]/workspace/inventory/\" -*-\nCompilation started at <TIME>\n\n./gradlew assembleDebug\n> Task :app:assembleDebug\nBUILD SUCCESSFUL in 1s\n1 actionable task: 1 executed\n\nCompilation finished at <TIME>\n") :clean ("gradlew assembleDebug" "gradlew clean") :install-and-uninstall ("gradlew assembleDebug" "gradlew clean" "gradlew installDebug" "gradlew uninstallDebug"))"#
        ]],
    )
}

fn switching_the_builder_changes_both_the_root_file_and_the_command() -> ParityBatchCase {
    ParityBatchCase::value(
        "switching_the_builder_changes_both_the_root_file_and_the_command",
        r##"
        ;; `android-mode-builder' selects two things at once: which file marks
        ;; the project root, and which command the common build verbs run.  A
        ;; project with a manifest but no wrapper is an ant/maven project and
        ;; not a gradle one, and the same `C-c a t' has to become a different
        ;; command line for each.
        (let ((buffer nil))
          (unwind-protect
              (progn
                (amd-test-setup)
                (delete-file (expand-file-name "gradlew" (amd-test-path "workspace/inventory/")))
                (setq buffer (amd-test-visit))
                (list
                 :gradle-root-gone (let ((android-mode-builder 'gradle)) (android-root))
                 :ant (let ((android-mode-builder 'ant))
                        (list :root (amd-test-normalize (android-root))
                              :root-file (plist-get android-mode-root-file-plist 'ant)
                              :command (amd-test-copy
                                        (cdr (assq 'ant android-mode-build-command-alist)))))
                 :maven (let ((android-mode-builder 'maven))
                          (list :root (amd-test-normalize (android-root))
                                :command (amd-test-copy
                                          (cdr (assq 'maven android-mode-build-command-alist)))))
                 :ant-test (let ((android-mode-builder 'ant))
                             (android-build-test)
                             (amd-test-wait (lambda () (get-buffer "*compilation*")))
                             (with-current-buffer "*compilation*"
                               (nth 3 (split-string (buffer-substring-no-properties
                                                     (point-min) (point-max))
                                                    "\n"))))
                 :reinstall-unsupported
                 (let ((android-mode-builder 'gradle))
                   (condition-case error
                       (progn (android-build-reinstall) :no-signal)
                     (error (list :signal (car error) :data (cdr error)))))))
            (when (buffer-live-p buffer) (kill-buffer buffer))
            (amd-test-teardown)))
    "##,
        expect![[
            r#"OK (:gradle-root-gone nil :ant (:root "[SANDBOX]/workspace/inventory/" :root-file "AndroidManifest.xml" :command "ant -e") :maven (:root "[SANDBOX]/workspace/inventory/" :command "mvn") :ant-test "ant -e test" :reinstall-unsupported (:signal error :data ("gradle builder does not support reinstall")))"#
        ]],
    )
}

fn the_sdk_is_found_through_local_properties_then_the_environment() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_sdk_is_found_through_local_properties_then_the_environment",
        r##"
        ;; Every SDK tool is located relative to one directory, and android-mode
        ;; resolves it in a documented order: the project's own
        ;; `local.properties', then ANDROID_HOME, then the customization.  A
        ;; `local.properties' pointing at a directory that does not exist has to
        ;; be ignored rather than trusted, and with nothing left the package
        ;; must say so.
        (amd-test-with-project
         (list
          :from-environment (amd-test-normalize (android-local-sdk-dir))
          :from-local-properties
          (let ((second (amd-test-path "android-sdk-2/")))
            (amd-test-write-executable (expand-file-name "platform-tools/adb" second)
                                       amd-test-adb-script)
            (amd-test-write-file
             (expand-file-name "local.properties" (amd-test-path "workspace/inventory/"))
             (concat "sdk.dir=" (directory-file-name second) "\n"))
            (list :sdk (amd-test-normalize (android-local-sdk-dir))
                  :adb (amd-test-normalize (android-tool-path "adb"))))
          :ignores-a_missing_directory
          (progn
            (amd-test-write-file
             (expand-file-name "local.properties" (amd-test-path "workspace/inventory/"))
             (concat "sdk.dir=" (amd-test-path "no-such-sdk") "\n"))
            (amd-test-normalize (android-local-sdk-dir)))
          :falls-back-to-the-customization
          (progn (setenv "ANDROID_HOME" nil)
                 (amd-test-normalize (android-local-sdk-dir)))
          :without-any
          (let ((android-mode-sdk-dir nil))
            (condition-case error (android-local-sdk-dir)
              (error (list :signal (car error) :data (cdr error)))))
          :tools (list :found (amd-test-normalize (android-tool-path "adb"))
                       :emulator (amd-test-normalize (android-tool-path "emulator"))
                       :missing (condition-case error (android-tool-path "ddms")
                                  (error (list :signal (car error) :data (cdr error)))))))
    "##,
        expect![[
            r#"OK (:from-environment "[SANDBOX]/android-sdk" :from-local-properties (:sdk "[SANDBOX]/android-sdk-2" :adb "[SANDBOX]/android-sdk-2/platform-tools/adb") :ignores-a_missing_directory "[SANDBOX]/android-sdk" :falls-back-to-the-customization "[SANDBOX]/android-sdk" :without-any (:signal error :data ("No SDK directory found")) :tools (:found "[SANDBOX]/android-sdk/platform-tools/adb" :emulator "[SANDBOX]/android-sdk/emulator/emulator" :missing (:signal error :data ("Can’t find SDK tool: ddms"))))"#
        ]],
    )
}

fn logcat_renders_each_level_with_its_face_and_links_only_frames_that_exist() -> ParityBatchCase {
    ParityBatchCase::value(
        "logcat_renders_each_level_with_its_face_and_links_only_frames_that_exist",
        r##"
        ;; `C-c a l' starts `adb logcat' and renders it: the level letter and
        ;; the message in the level's face, the tag and pid in their own faces,
        ;; columns at the buffer's tab stops, and a line that does not parse in
        ;; the warning face rather than dropped.  A stack frame becomes a link
        ;; only when the source it names really exists under the project - the
        ;; fixture has one of each, and `RET' on the live one opens the file at
        ;; the right line.
        (amd-test-with-project
         (progn
           (execute-kbd-macro (read-kbd-macro (concat android-mode-key-prefix " l")))
           (amd-test-wait (lambda () (with-current-buffer android-logcat-buffer
                                       (save-excursion
                                         (goto-char (point-min))
                                         (search-forward "no level prefix" nil t)))))
           (with-current-buffer android-logcat-buffer
             (list
              :read-only buffer-read-only
              :tab-stops tab-stop-list
              :android-mode android-mode
              :local-map-is-logcat (eq (current-local-map) android-logcat-map)
              :text (buffer-substring-no-properties (point-min) (point-max))
              :faces (amd-test-faces (point-min) (point-max))
              :links (let (found (position (point-min)))
                       (while (< position (point-max))
                         (let ((next (next-single-property-change
                                      position 'filename nil (point-max))))
                           (when (get-text-property position 'filename)
                             (push (list (substring-no-properties
                                          (get-text-property position 'filename))
                                         (get-text-property position 'linenr)
                                         (get-text-property position 'follow-link))
                                   found))
                           (setq position next)))
                       (nreverse found))
              :opened (progn
                        (goto-char (point-min))
                        (search-forward "ReportActivity.render")
                        (goto-char (match-beginning 0))
                        (android-logcat-find-file)
                        (list :file (file-name-nondirectory (buffer-file-name))
                              :line (line-number-at-pos)
                              :text (buffer-substring-no-properties
                                     (line-beginning-position) (line-end-position))))
              :commands (amd-test-commands)))))
    "##,
        expect![[
            r#"OK (:read-only t :tab-stops (2 30) :android-mode t :local-map-is-logcat t :text "I ActivityManager(742)\11      Displayed com.warehouse.inventory/.MainActivity: +312ms\nD InventorySync(742)\11      syncing 3 widgets\nW InventorySync(742)\11      bucket cache is stale\nE AndroidRuntime(742)\11      FATAL EXCEPTION: main\nE AndroidRuntime(742)\11      java.lang.IllegalStateException: no report yet\nE AndroidRuntime(742)\11      \11at com.warehouse.inventory.ReportActivity.render(ReportActivity.java:7)\nE AndroidRuntime(742)\11      \11at com.warehouse.missing.Absent.gone(Absent.java:42)\nV InventorySync(742)\11      done\nthis line has no level prefix\n" :faces ((android-mode-info-face "I ") (font-lock-function-name-face "ActivityManager") (font-lock-constant-face "(742)\11      ") (android-mode-info-face "Displayed com.warehouse.inventory/.MainActivity: +312ms") (nil "\n") (android-mode-debug-face "D ") (font-lock-function-name-face "InventorySync") (font-lock-constant-face "(742)\11      ") (android-mode-debug-face "syncing 3 widgets") (nil "\n") (android-mode-warning-face "W ") (font-lock-function-name-face "InventorySync") (font-lock-constant-face "(742)\11      ") (android-mode-warning-face "bucket cache is stale") (nil "\n") (android-mode-error-face "E ") (font-lock-function-name-face "AndroidRuntime") (font-lock-constant-face "(742)\11      ") (android-mode-error-face "FATAL EXCEPTION: main") (nil "\n") (android-mode-error-face "E ") (font-lock-function-name-face "AndroidRuntime") (font-lock-constant-face "(742)\11      ") (android-mode-error-face "java.lang.IllegalStateException: no report yet") (nil "\n") (android-mode-error-face "E ") (font-lock-function-name-face "AndroidRuntime") (font-lock-constant-face "(742)\11      ") (android-mode-error-face "\11at com.warehouse.inventory.ReportActivity.render(ReportActivity.java:7)") (nil "\n") (android-mode-error-face "E ") (font-lock-function-name-face "AndroidRuntime") (font-lock-constant-face "(742)\11      ") (android-mode-error-face "\11at com.warehouse.missing.Absent.gone(Absent.java:42)") (nil "\n") (android-mode-verbose-face "V ") (font-lock-function-name-face "InventorySync") (font-lock-constant-face "(742)\11      ") (android-mode-verbose-face "done") (nil "\n") (font-lock-warning-face "this line has no level prefix") (nil "\n")) :links (("com/warehouse/inventory/ReportActivity.java" 7 t)) :opened (:file "ReportActivity.java" :line 7 :text "        throw new IllegalStateException(\"no report yet\");") :commands ("adb logcat"))"#
        ]],
    )
    .fresh_process()
}

fn filtering_logcat_announces_the_change_and_hides_later_lines() -> ParityBatchCase {
    ParityBatchCase::value(
        "filtering_logcat_announces_the_change_and_hides_later_lines",
        r##"
        ;; In the logcat buffer `f' sets a regexp filter, `c' clears it and `C'
        ;; erases the buffer.  Setting a filter has to announce itself in the
        ;; buffer and then apply to lines that arrive afterwards, leaving what
        ;; is already rendered alone; clearing has to announce that too.
        (amd-test-with-project
         (progn
           (android-logcat)
           (amd-test-wait (lambda () (with-current-buffer android-logcat-buffer
                                       (save-excursion
                                         (goto-char (point-min))
                                         (search-forward "no level prefix" nil t)))))
           (with-current-buffer android-logcat-buffer
             (list
              :filter-set
              (progn (android-logcat-set-filter "InventorySync")
                     (list :variable (amd-test-copy android-mode-log-filter-regexp)
                           :banner (buffer-substring-no-properties
                                    (save-excursion (goto-char (point-max))
                                                    (forward-line -4)
                                                    (point))
                                    (point-max))))
              :later-lines
              (progn (android-logcat-process-filter
                      nil "I/InventorySync(  742): kept by the filter\nI/Other(  742): dropped\n")
                     (buffer-substring-no-properties
                      (save-excursion (goto-char (point-max)) (forward-line -1) (point))
                      (point-max)))
              :cleared
              (progn (android-logcat-clear-filter)
                     (android-logcat-process-filter
                      nil "I/Other(  742): back again\n")
                     (list :variable (amd-test-copy android-mode-log-filter-regexp)
                           :tail (buffer-substring-no-properties
                                  (save-excursion (goto-char (point-max))
                                                  (forward-line -5)
                                                  (point))
                                  (point-max))))
              :erased
              (progn (android-logcat-erase-buffer)
                     (list :size (buffer-size) :read-only buffer-read-only))))))
    "##,
        expect![[
            r#"OK (:filter-set (:variable "InventorySync" :banner "\n\n*** Filter is changed to 'InventorySync' ***\n\n") :later-lines "I InventorySync(742)\11      kept by the filter\n" :cleared (:variable "" :tail "\n\n*** Filter is cleared ***\n\nI Other(742)\11\11      back again\n") :erased (:size 0 :read-only t))"#
        ]],
    )
}

fn starting_an_activity_reads_the_manifest_and_asks_adb_to_launch_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "starting_an_activity_reads_the_manifest_and_asks_adb_to_launch_it",
        r##"
        ;; `C-c a a' launches the activity of the buffer you are in when that
        ;; class is declared as a main activity in the real
        ;; `AndroidManifest.xml', and otherwise the first launcher activity.
        ;; The manifest is parsed for real, and the workflow pins two things
        ;; the package gets wrong rather than steering around them: the
        ;; category filter uses `cl-member-if', which returns the tail of the
        ;; list from the first match instead of the matches, so ScannerActivity
        ;; is offered despite having no MAIN action; and with no device
        ;; attached adb answers "adb: no devices/emulators found", which does
        ;; not match the package's `^Error: ' test, so the failure is reported
        ;; as success.
        (amd-test-with-project
         (list
          :package (amd-test-copy (android-project-package))
          :class-of-this-buffer (amd-test-copy (android-current-buffer-class-name))
          :main-activities (mapcar #'amd-test-copy (android-project-main-activities))
          :launcher-activities (mapcar #'amd-test-copy
                                       (android-project-main-activities "LAUNCHER"))
          ;; DEFAULT is the query that makes the defect unmistakable.  Exactly
          ;; one activity declares that category, so a filter returns one name;
          ;; a tail returns that activity and everything after it in the
          ;; manifest.  Without this the other two queries both return all
          ;; three and cannot tell a broken filter from one where everything
          ;; happens to match.
          :default-activities (mapcar #'amd-test-copy
                                      (android-project-main-activities "DEFAULT"))
          :activities-declaring-default 1
          :from-this-buffer
          (progn (execute-kbd-macro (read-kbd-macro (concat android-mode-key-prefix " a")))
                 (list :commands (amd-test-commands)
                       :messages (amd-test-messages "^Starting activity: .*$")))
          :from-a-non-activity-buffer
          (let ((other (amd-test-visit "src/com/warehouse/inventory/ReportActivity.java")))
            (unwind-protect
                (progn (android-start-app)
                       (car (last (amd-test-commands))))
              (kill-buffer other)))
          :with-no-device-attached
          (progn (amd-test-write-file (amd-test-path "recordings/no-device") "")
                 (amd-test-visit)
                 (condition-case error (progn (android-start-app) :reported-success)
                   (error (list :signal (car error) :data (cdr error)))))))
    "##,
        expect![[
            r#"OK (:package "com.warehouse.inventory" :class-of-this-buffer "com.warehouse.inventory.MainActivity" :main-activities ("com.warehouse.inventory.MainActivity" "com.warehouse.inventory.ReportActivity" "com.warehouse.tools.ScannerActivity") :launcher-activities ("com.warehouse.inventory.MainActivity" "com.warehouse.inventory.ReportActivity" "com.warehouse.tools.ScannerActivity") :default-activities ("com.warehouse.inventory.ReportActivity" "com.warehouse.tools.ScannerActivity") :activities-declaring-default 1 :from-this-buffer (:commands ("adb shell am start -n com.warehouse.inventory/com.warehouse.inventory.MainActivity") :messages ("Starting activity: com.warehouse.inventory.MainActivity")) :from-a-non-activity-buffer "adb shell am start -n com.warehouse.inventory/com.warehouse.inventory.ReportActivity" :with-no-device-attached :reported-success)"#
        ]],
    )
    .fresh_process()
}

fn the_emulator_starts_once_and_reports_when_it_is_already_running() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_emulator_starts_once_and_reports_when_it_is_already_running",
        r##"
        ;; `C-c a e' boots the configured AVD, and android-mode keeps one
        ;; process per name: asking again while it runs must not start a second
        ;; emulator, only say so.  With no AVD configured the list comes from
        ;; the SDK tool, and both that list and the platform list are parsed
        ;; out of its output.
        (amd-test-with-project
         (list
          :avds (mapcar #'amd-test-copy (android-list-avd))
          :targets (mapcar #'amd-test-copy (android-list-targets))
          :started
          (let ((android-mode-avd "Pixel_6_API_34"))
            (execute-kbd-macro (read-kbd-macro (concat android-mode-key-prefix " e")))
            (amd-test-wait (lambda () (seq-find (lambda (command)
                                                  (string-prefix-p "emulator" command))
                                                (amd-test-commands))))
            (list :command (seq-find (lambda (command) (string-prefix-p "emulator" command))
                                     (amd-test-commands))
                  :exclusive (mapcar #'symbol-name android-exclusive-processes)
                  :process-live (and (get-process "*android-emulator-Pixel_6_API_34*") t)))
          :asked-again
          (let ((android-mode-avd "Pixel_6_API_34"))
            (android-start-emulator)
            (list :launches (length (seq-filter (lambda (command)
                                                  (string-prefix-p "emulator" command))
                                                (amd-test-commands)))
                  :exclusive (mapcar #'symbol-name android-exclusive-processes)
                  :messages (amd-test-messages "^emulator .* already running$")))
          :ddms-is-not-installed
          (condition-case error (progn (android-start-ddms) :no-signal)
            (error (list :signal (car error) :data (cdr error))))))
    "##,
        expect![[
            r#"OK (:avds ("Pixel_6_API_34" "Nexus_5X_API_29") :targets ("android-34" "Google Inc.:Google APIs:34") :started (:command "emulator -avd Pixel_6_API_34" :exclusive ("*android-emulator-Pixel_6_API_34*") :process-live t) :asked-again (:launches 1 :exclusive ("*android-emulator-Pixel_6_API_34*") :messages ("emulator Pixel_6_API_34 already running")) :ddms-is-not-installed (:signal error :data ("Can’t find SDK tool: ddms")))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        opening_a_file_in_a_gradle_project_turns_the_mode_on_and_binds_its_prefix(),
        building_runs_the_projects_own_gradle_wrapper_from_the_project_root(),
        switching_the_builder_changes_both_the_root_file_and_the_command(),
        the_sdk_is_found_through_local_properties_then_the_environment(),
        logcat_renders_each_level_with_its_face_and_links_only_frames_that_exist(),
        filtering_logcat_announces_the_change_and_hides_later_lines(),
        starting_an_activity_reads_the_manifest_and_asks_adb_to_launch_it(),
        the_emulator_starts_once_and_reports_when_it_is_already_running(),
    ]
}

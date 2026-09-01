use expect_test::expect;

use super::ParityBatchCase;

fn quoted_include_uses_function_paths_and_later_duplicate_metadata() -> ParityBatchCase {
    let elisp_form = r####"
(cch354-test-run
 "quoted-function-paths"
 (lambda (root)
   (let* ((user (file-name-as-directory (expand-file-name "include/user" root)))
          (system (file-name-as-directory (expand-file-name "include/system" root)))
          (cch354-test-path-calls nil)
          (cch354-test-provider-root root)
          (cch354-test-user-path-value (list user))
          (cch354-test-system-path-value (list system))
          (company-c-headers-path-user #'cch354-test-user-path-provider)
          (company-c-headers-path-system #'cch354-test-system-path-provider))
     (cch354-test-write-file root "include/user/config.h"
                             "#define CONFIG_SOURCE \"user\"\n")
     (cch354-test-write-file root "include/user/concept.hpp"
                             "template<class T> concept Storable = true;\n")
     (cch354-test-write-file root "include/user/console.txt"
                             "filtered non-header\n")
     (cch354-test-write-file root "include/system/config.h"
                             "#define CONFIG_SOURCE \"system\"\n")
     (cch354-test-write-file root "include/system/console.h"
                             "void console_write(void);\n")
     (with-temp-buffer
         (cch354-test-prepare-company-buffer 'c++-mode
                                             "#include \"con  \"")
         (backward-char 3)
         (call-interactively #'company-c-headers)
         (let* ((target-index
                 (cl-position "\"config.h" company-candidates
                              :test (lambda (plain candidate)
                                      (equal plain
                                             (substring-no-properties candidate)))))
                (path-calls (reverse cch354-test-path-calls))
                (expected-path-calls
                 (list
                  (list :provider 'user :mode 'c++-mode
                        :directory "./" :line "#include \"con  \""
                        :point 14)
                  (list :provider 'system :mode 'c++-mode
                        :directory "./" :line "#include \"con  \""
                        :point 14)))
                (opened
                 (list :buffer (buffer-substring-no-properties
                                (point-min) (point-max))
                       :point (point)
                       :prefix company-prefix
                       :candidates (cch354-test-plain-candidates)
                       :candidate-data
                       (mapcar (lambda (candidate)
                                 (cch354-test-candidate candidate root))
                               company-candidates)
                       :path-calls path-calls
                       :backend company-backend
                       :tooltip (and (company-tooltip-visible-p) t))))
           (unless target-index
             (error "Company did not offer duplicate config.h"))
           (unless (equal path-calls expected-path-calls)
             (error "COMPANY-C-HEADERS path provider contract changed: %S"
                    path-calls))
           (company-select-next target-index)
           (let* ((chosen (nth company-selection company-candidates))
                  (chosen-data (cch354-test-candidate chosen root)))
             (company-complete-selection)
             (list :opened opened
                   :chosen chosen-data
                   :final
                   (list :buffer (buffer-substring-no-properties
                                  (point-min) (point-max))
                         :point (point)
                         :point-max (point-max)
                         :char-before (char-before)
                         :active (and company-candidates t)))))))))
"####;
    ParityBatchCase::value(
        "quoted-include-function-paths-and-later-duplicate-metadata",
        elisp_form,
        expect![[
            r##"OK (:result (:opened (:buffer "#include \"con  \"" :point 14 :prefix "\"con" :candidates ("\"concept.hpp" "\"config.h" "\"console.h") :candidate-data ((:text "\"concept.hpp" :directory "include/user/" :location ("include/user/concept.hpp" 1)) (:text "\"config.h" :directory "include/system/" :location ("include/system/config.h" 1)) (:text "\"console.h" :directory "include/system/" :location ("include/system/console.h" 1))) :path-calls ((:provider user :mode c++-mode :directory "./" :line "#include \"con  \"" :point 14) (:provider system :mode c++-mode :directory "./" :line "#include \"con  \"" :point 14)) :backend company-c-headers :tooltip nil) :chosen (:text "\"config.h" :directory "include/system/" :location ("include/system/config.h" 1)) :final (:buffer "#include \"config.h  \"" :point 22 :point-max 22 :char-before 34 :active nil)) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :current-buffer-restored t :window-restored t :this-command-restored t :company-timer-restored t :emulation-maps-restored t :electric-window-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn directory_completion_restarts_then_preserves_existing_system_delimiter() -> ParityBatchCase {
    let elisp_form = r####"
(cch354-test-run
 "nested-system-restart"
 (lambda (root)
   (let* ((system (file-name-as-directory (expand-file-name "sdk" root)))
          (company-c-headers-path-user nil)
          (company-c-headers-path-system (list system)))
     (cch354-test-write-file root "sdk/api/client.h"
                             "struct api_client;\n")
     (let ((selected-file
            (cch354-test-write-file root "sdk/api/client.hpp"
                                    "struct api_client_cpp {};\n")))
       (cch354-test-write-file root "sdk/api/internal/token.hxx"
                               "struct api_token {};\n")
       (with-temp-buffer
         (cch354-test-prepare-company-buffer 'c++-mode "#include <ap  >")
         (setq-local company-idle-delay 0)
         (backward-char 3)
         (call-interactively #'company-c-headers)
         (let ((directory-stage
                (list :prefix company-prefix
                      :candidates (cch354-test-plain-candidates)
                      :point (point)
                      :buffer (buffer-substring-no-properties
                               (point-min) (point-max)))))
           (company-complete-selection)
           (let ((after-directory
                  (list :buffer (buffer-substring-no-properties
                                 (point-min) (point-max))
                        :point (point)
                        :this-command this-command
                        :active (and company-candidates t))))
             ;; Deliver the real Company post-command path.  Company owns the
             ;; resulting timer and its ordinary command loop fires it.
             (run-hooks 'post-command-hook)
             (let ((timer company-timer))
               (unless (timerp timer)
                 (error "Company did not schedule the nested completion"))
               (cch354-test-wait-for-company
                (lambda ()
                  (and company-candidates
                       (equal company-prefix "<api/")))
                "nested api/ candidates")
               (unless (and (not (memq timer timer-list))
                            (not (memq timer timer-idle-list)))
                 (error "Company nested timer remained scheduled: %S" timer)))
             (let* ((nested-stage
                     (list :prefix company-prefix
                           :candidates (cch354-test-plain-candidates)
                           :candidate-data
                           (mapcar (lambda (candidate)
                                     (cch354-test-candidate candidate root))
                                   company-candidates)
                           :tooltip (and (company-tooltip-visible-p) t)))
                    (target-index
                     (cl-position "<api/client.hpp" company-candidates
                                  :test
                                  (lambda (plain candidate)
                                    (equal plain
                                           (substring-no-properties candidate)))))
                    (chosen nil)
                    (chosen-data nil)
                    (origin-window (selected-window))
                    (window-count-before (length (window-list)))
                    preview post-preview)
               (unless target-index
                 (error "Company did not restart inside api/"))
               (company-select-next target-index)
               (setq chosen (nth company-selection company-candidates)
                     chosen-data (cch354-test-candidate chosen root))
               (company-show-location)
               (let* ((header-buffer (get-file-buffer selected-file))
                      (header-window (and header-buffer
                                          (get-buffer-window header-buffer))))
                 (setq preview
                       (list :candidate chosen-data
                             :buffer-live (and header-buffer t)
                             :window-live (and header-window t)
                             :electric-window-owned
                             (and company--electric-saved-window-configuration t)
                             :window-selected-origin
                             (eq (selected-window) origin-window)
                             :header
                             (and header-buffer
                                  (with-current-buffer header-buffer
                                    (list
                                     :mode major-mode
                                     :point (point)
                                     :bytes
                                     (buffer-substring-no-properties
                                      (point-min) (point-max))))))))
               ;; Run the real pre-command lifecycle before the next public
               ;; Company command.  It restores the electric preview window.
               (let ((this-command 'company-complete-selection)
                     (real-this-command 'company-complete-selection)
                     (this-original-command 'company-complete-selection))
                 (run-hooks 'pre-command-hook)
                 (call-interactively #'company-complete-selection)
                 (run-hooks 'post-command-hook))
               (setq post-preview
                     (list
                      :window-count-restored
                      (= (length (window-list)) window-count-before)
                      :origin-window-selected
                      (eq (selected-window) origin-window)
                      :header-window-gone
                      (not (get-buffer-window (get-file-buffer selected-file)))
                      :electric-window-owned
                      (and company--electric-saved-window-configuration t)))
               (list :directory-stage directory-stage
                     :after-directory after-directory
                     :nested-stage nested-stage
                     :preview preview
                     :post-preview post-preview
                     :final
                     (list :buffer (buffer-substring-no-properties
                                    (point-min) (point-max))
                           :point (point)
                           :point-max (point-max)
                           :char-before (char-before)
                           :active (and company-candidates t)))))))))))
"####;
    ParityBatchCase::value(
        "directory-completion-restarts-and-preserves-system-delimiter",
        elisp_form,
        expect![[
            r##"OK (:result (:directory-stage (:prefix "<ap" :candidates ("<api/") :point 13 :buffer "#include <ap  >") :after-directory (:buffer "#include <api/  >" :point 15 :this-command self-insert-command :active nil) :nested-stage (:prefix "<api/" :candidates ("<api/client.h" "<api/client.hpp" "<api/internal/") :candidate-data ((:text "<api/client.h" :directory "sdk/api/" :location ("sdk/api/client.h" 1)) (:text "<api/client.hpp" :directory "sdk/api/" :location ("sdk/api/client.hpp" 1)) (:text "<api/internal/" :directory "sdk/api/" :location ("sdk/api/" 1))) :tooltip t) :preview (:candidate (:text "<api/client.hpp" :directory "sdk/api/" :location ("sdk/api/client.hpp" 1)) :buffer-live t :window-live t :electric-window-owned t :window-selected-origin t :header (:mode c++-mode :point 1 :bytes "struct api_client_cpp {};\n")) :post-preview (:window-count-restored t :origin-window-selected t :header-window-gone t :electric-window-owned nil) :final (:buffer "#include <api/client.hpp  >" :point 28 :point-max 28 :char-before 62 :active nil)) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :current-buffer-restored t :window-restored t :this-command-restored t :company-timer-restored t :emulation-maps-restored t :electric-window-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn real_language_modes_filter_extensions_imports_and_unsupported_contexts() -> ParityBatchCase {
    let elisp_form = r####"
(cch354-test-run
 "language-mode-boundaries"
 (lambda (root)
   (let* ((user (file-name-as-directory (expand-file-name "user" root)))
          (system (file-name-as-directory (expand-file-name "system" root)))
          (company-c-headers-path-user (list user))
          (company-c-headers-path-system (list system)))
     (cch354-test-write-file root "system/stdio.h" "int printf(const char *, ...);\n")
     (cch354-test-write-file root "system/vector" "// C++ extensionless\n")
     (cch354-test-write-file root "system/vector.hpp" "template<class T> struct vector;\n")
     (cch354-test-write-file root "system/vector.hxx" "struct vector_hxx;\n")
     (cch354-test-write-file root "system/vector.hh" "struct vector_hh;\n")
     (cch354-test-write-file root "system/vector.c" "int filtered_source;\n")
     (cch354-test-write-file root "system/readme.txt" "filtered\n")
     (cch354-test-write-file root "user/UIKit/UIKit.h" "@interface UIView @end\n")
     (cch354-test-write-file root "user/UIKit/UIKit.hpp" "filtered from ObjC\n")
     (cl-labels
         ((probe
           (mode contents)
           (with-temp-buffer
             (cch354-test-prepare-company-buffer mode contents)
             (let ((outcome
                    (cch354-test-capture
                     (lambda () (call-interactively #'company-c-headers)))))
               (when (plist-member outcome :value)
                 (setq outcome
                       (list :value-is-company-candidates
                             (eq (plist-get outcome :value)
                                 company-candidates))))
               (prog1
                   (list :mode major-mode
                         :contents (buffer-substring-no-properties
                                    (point-min) (point-max))
                         :outcome outcome
                         :point (point)
                         :prefix company-prefix
                         :candidates (cch354-test-plain-candidates)
                         :active (and company-candidates t))
                 (when company-candidates (company-abort)))))))
       (list :c-include (probe 'c-mode "#include <")
             :cplusplus-include (probe 'c++-mode "#include <")
             :objc-import (probe 'objc-mode "#import \"UIKit/")
             :spaced-directive (probe 'c++-mode "# include <")
             :missing-required-space (probe 'c++-mode "#include<")
             :closed-include (probe 'c++-mode "#include <stdio.h>")
             :unsupported (probe 'fundamental-mode "#include <")
             :commented (probe 'c++-mode "// #include <")
             :indented (probe 'c++-mode "  #include <"))))))
"####;
    ParityBatchCase::value(
        "real-language-modes-filter-extensions-import-and-contexts",
        elisp_form,
        expect![[
            r##"OK (:result (:c-include (:mode c-mode :contents "#include <" :outcome (:value-is-company-candidates t) :point 11 :prefix "<" :candidates ("<stdio.h") :active t) :cplusplus-include (:mode c++-mode :contents "#include <" :outcome (:value-is-company-candidates t) :point 11 :prefix "<" :candidates ("<stdio.h" "<vector" "<vector.hh" "<vector.hpp" "<vector.hxx") :active t) :objc-import (:mode objc-mode :contents "#import \"UIKit/" :outcome (:value-is-company-candidates t) :point 16 :prefix "\"UIKit/" :candidates ("\"UIKit/UIKit.h") :active t) :spaced-directive (:mode c++-mode :contents "# include <" :outcome (:value-is-company-candidates t) :point 12 :prefix "<" :candidates ("<stdio.h" "<vector" "<vector.hh" "<vector.hpp" "<vector.hxx") :active t) :missing-required-space (:mode c++-mode :contents "#include<" :outcome (:signal user-error :data ("Cannot complete at point") :message "Cannot complete at point") :point 10 :prefix nil :candidates nil :active nil) :closed-include (:mode c++-mode :contents "#include <stdio.h>" :outcome (:signal user-error :data ("Cannot complete at point") :message "Cannot complete at point") :point 19 :prefix nil :candidates nil :active nil) :unsupported (:mode fundamental-mode :contents "#include <" :outcome (:signal user-error :data ("Cannot complete at point") :message "Cannot complete at point") :point 11 :prefix nil :candidates nil :active nil) :commented (:mode c++-mode :contents "// #include <" :outcome (:signal user-error :data ("Cannot complete at point") :message "Cannot complete at point") :point 14 :prefix nil :candidates nil :active nil) :indented (:mode c++-mode :contents "  #include <" :outcome (:signal user-error :data ("Cannot complete at point") :message "Cannot complete at point") :point 13 :prefix nil :candidates nil :active nil)) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :current-buffer-restored t :window-restored t :this-command-restored t :company-timer-restored t :emulation-maps-restored t :electric-window-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn missing_and_failing_path_providers_leave_text_unchanged_then_recover() -> ParityBatchCase {
    let elisp_form = r####"
(cch354-test-run
 "path-failure-recovery"
 (lambda (root)
   (let* ((valid (file-name-as-directory (expand-file-name "valid" root)))
          (missing (file-name-as-directory (expand-file-name "missing" root)))
          (cch354-test-path-calls nil)
          (cch354-test-provider-root root)
          (company-c-headers-path-user nil)
          (company-c-headers-path-system (list missing)))
     (cch354-test-write-file root "valid/stdio.h"
                             "int puts(const char *);\n")
     (with-temp-buffer
       (cch354-test-prepare-company-buffer 'c-mode "#include <std")
       (let ((missing-outcome
              (cch354-test-capture
               (lambda () (call-interactively #'company-c-headers))))
             missing-state provider-interactive provider-protocol
             provider-state provider-calls recovery)
         (setq missing-state
               (list :buffer (buffer-substring-no-properties
                              (point-min) (point-max))
                     :point (point)
                     :active (and company-candidates t)))
         (setq company-c-headers-path-system
               #'cch354-test-failing-path-provider)
         (setq provider-interactive
               (cch354-test-capture
                (lambda () (call-interactively #'company-c-headers))))
         (setq provider-protocol
               (cch354-test-capture
                (lambda ()
                  (let ((company-backend 'company-c-headers))
                    (company-call-backend 'candidates "<std")))))
         (setq provider-state
               (list :buffer (buffer-substring-no-properties
                              (point-min) (point-max))
                     :point (point)
                     :backend company-backend
                     :prefix company-prefix
                     :candidates (cch354-test-plain-candidates)
                     :active (and company-candidates t)))
         (setq company-c-headers-path-system (list valid))
         (setq provider-calls (reverse cch354-test-path-calls))
         (let ((expected-path-calls
                (list
                 (list :provider 'failing :mode 'c-mode
                       :directory "./" :line "#include <std" :point 14)
                 (list :provider 'failing :mode 'c-mode
                       :directory "./" :line "#include <std" :point 14))))
           (unless (equal provider-calls expected-path-calls)
             (error "COMPANY-C-HEADERS failing provider contract changed: %S"
                    provider-calls)))
         (call-interactively #'company-c-headers)
         (setq recovery
               (list :prefix company-prefix
                     :candidates (cch354-test-plain-candidates)
                     :selected
                     (cch354-test-candidate
                      (nth company-selection company-candidates) root)))
         (company-complete-selection)
         (list :missing-outcome missing-outcome
               :missing-state missing-state
               :provider-interactive provider-interactive
               :provider-protocol provider-protocol
               :provider-state provider-state
               :provider-calls provider-calls
               :recovery recovery
               :final
               (list :buffer (buffer-substring-no-properties
                              (point-min) (point-max))
                     :point (point)
                     :active (and company-candidates t))))))))
"####;
    ParityBatchCase::value(
        "missing-and-failing-path-providers-preserve-text-and-recover",
        elisp_form,
        expect![[
            r##"OK (:result (:missing-outcome (:signal user-error :data ("Cannot complete at point") :message "Cannot complete at point") :missing-state (:buffer "#include <std" :point 14 :active nil) :provider-interactive (:signal user-error :data ("Cannot complete at point") :message "Cannot complete at point") :provider-protocol (:signal error :data ("Company: backend company-c-headers error \"owned project include provider unavailable\" with args (candidates <std)") :message "Company: backend company-c-headers error \"owned project include provider unavailable\" with args (candidates <std)") :provider-state (:buffer "#include <std" :point 14 :backend nil :prefix nil :candidates nil :active nil) :provider-calls ((:provider failing :mode c-mode :directory "./" :line "#include <std" :point 14) (:provider failing :mode c-mode :directory "./" :line "#include <std" :point 14)) :recovery (:prefix "<std" :candidates ("<stdio.h") :selected (:text "<stdio.h" :directory "valid/" :location ("valid/stdio.h" 1))) :final (:buffer "#include <stdio.h>" :point 19 :active nil)) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :current-buffer-restored t :window-restored t :this-command-restored t :company-timer-restored t :emulation-maps-restored t :electric-window-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        quoted_include_uses_function_paths_and_later_duplicate_metadata(),
        directory_completion_restarts_then_preserves_existing_system_delimiter(),
        real_language_modes_filter_extensions_imports_and_unsupported_contexts(),
        missing_and_failing_path_providers_leave_text_unchanged_then_recover(),
    ]
}

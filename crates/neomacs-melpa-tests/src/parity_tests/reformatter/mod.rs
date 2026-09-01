use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, REFORMATTER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const REFORMATTER_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const REFORMATTER_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'reformatter)

(defgroup reformatter-parity nil
  "Parity fixtures for reformatter."
  :group 'tools)

(defvar reformatter-parity-mode-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "C-c f") #'reformatter-parity-stdio-buffer)
    map))

(defvar-local reformatter-parity-prefix "default")
(defvar-local reformatter-parity-working-directory nil)
(defvar reformatter-parity-last-input-file nil)

(reformatter-define reformatter-parity-stdio
  :program "sh"
  :args '("-c" "sed 's/release/RELEASE/g; s/publish/PUBLISH/g'")
  :lighter " Fmt"
  :keymap reformatter-parity-mode-map
  :group 'reformatter-parity
  :interactive-modes (text-mode prog-mode))

(reformatter-define reformatter-parity-sort
  :program "sort"
  :args nil
  :mode nil)

(reformatter-define reformatter-parity-in-place
  :program "sh"
  :stdin nil
  :stdout nil
  :input-file (reformatter-temp-file "rs")
  :args
  (progn
    (setq reformatter-parity-last-input-file input-file)
    (list
     "-c"
     "printf '// extension=%s\\n' \"${1##*.}\" > \"$1.new\"; sed 's/alpha/ALPHA/g' \"$1\" >> \"$1.new\"; mv \"$1.new\" \"$1\""
     "formatter"
     input-file)))

(reformatter-define reformatter-parity-context
  :program "sh"
  :working-directory reformatter-parity-working-directory
  :args
  (list
   "-c"
   "printf '%s|%s\\n' \"$1\" \"$PWD\"; sed 's/task/TASK/g'"
   "formatter"
   reformatter-parity-prefix)
  :mode nil)

(reformatter-define reformatter-parity-exit-three
  :program "sh"
  :args '("-c" "sed 's/pending/accepted/g'; exit 3")
  :exit-code-success-p (lambda (code) (memq code '(0 3)))
  :mode nil)

(reformatter-define reformatter-parity-failure
  :program "sh"
  :args
  '("-c" "printf '\\033[31msyntax error at line 2\\033[0m\\n' >&2; exit 7")
  :mode nil)

(defun reformatter-parity-error-summary (name)
  (let ((buffer (get-buffer (format "*%s errors*" name))))
    (when buffer
      (with-current-buffer buffer
        (list
         :mode major-mode
         :read-only buffer-read-only
         :text (buffer-substring-no-properties
                (point-min) (point-max)))))))

(defun reformatter-parity-normalize-root (value root)
  (replace-regexp-in-string
   (regexp-quote (directory-file-name root))
   "[PROJECT]"
   value t t))
"####;

fn reformatter_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(REFORMATTER_MELPA_PIN, "reformatter.el")
        .expect("prepare pinned reformatter source below ./tmp")
        .with_prelude(REFORMATTER_TEST_PRELUDE)
        .with_timeout(REFORMATTER_TEST_TIMEOUT)
}

fn generated_region_command_formats_only_selection_and_exposes_mode_specific_command_contract()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (text-mode)
  (insert "header: release\nrelease then publish\nfooter: publish\n")
  (goto-char (point-min))
  (forward-line 1)
  (let* ((begin (point))
         (end (line-end-position))
         (footer (copy-marker
                  (save-excursion
                    (forward-line 1)
                    (point))
                  t)))
    (goto-char (+ begin 8))
    (reformatter-parity-stdio-region begin end)
    (list
     :buffer (buffer-string)
     :point (point)
     :point-text (thing-at-point 'word t)
     :footer-marker (marker-position footer)
     :footer-text
     (save-excursion
       (goto-char footer)
       (buffer-substring-no-properties
        (line-beginning-position) (line-end-position)))
     :commands
     (list
      :buffer-modes
      (and (fboundp 'command-modes)
           (copy-tree
            (command-modes 'reformatter-parity-stdio-buffer)))
      :region-modes
      (and (fboundp 'command-modes)
           (copy-tree
            (command-modes 'reformatter-parity-stdio-region)))
      :buffer-interactive
      (commandp 'reformatter-parity-stdio-buffer)
      :region-interactive
      (commandp 'reformatter-parity-stdio-region)))))
"##;
    let expect = expect![[
        r##"OK (:buffer "header: release\nRELEASE then PUBLISH\nfooter: publish\n" :point 25 :point-text "then" :footer-marker 38 :footer-text "footer: publish" :commands (:buffer-modes (text-mode prog-mode) :region-modes (text-mode prog-mode) :buffer-interactive t :region-interactive t))"##
    ]];
    ParityBatchCase::value(
        "generated_region_command_formats_only_selection_and_exposes_mode_specific_command_contract",
        elisp_form,
        expect,
    )
}

fn whole_buffer_sort_replacement_preserves_unicode_content_markers_and_modified_state()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "zeta release\nβeta publish\nalpha 发布\n")
  (goto-char (point-min))
  (search-forward "publish")
  (let ((tracked (copy-marker (point) t))
        (before-undo buffer-undo-list))
    (reformatter-parity-sort-buffer)
    (list
     :buffer (buffer-string)
     :point (point)
     :point-line (line-number-at-pos)
     :marker (marker-position tracked)
     :marker-line
     (save-excursion
       (goto-char tracked)
       (line-number-at-pos))
     :modified (buffer-modified-p)
     :undo-recorded (not (eq before-undo buffer-undo-list)))))
"##;
    let expect = expect![[
        r##"OK (:buffer "alpha 发布\nzeta release\nβeta publish\n" :point 26 :point-line 3 :marker 35 :marker-line 3 :modified t :undo-recorded t)"##
    ]];
    ParityBatchCase::value(
        "whole_buffer_sort_replacement_preserves_unicode_content_markers_and_modified_state",
        elisp_form,
        expect,
    )
}

fn file_backed_formatter_rewrites_pinned_extension_and_cleans_its_input_artifact() -> ParityBatchCase
{
    let elisp_form = r##"
(with-temp-buffer
  (setq reformatter-parity-last-input-file nil)
  (insert "fn alpha() {}\nlet alpha = 1;\n")
  (reformatter-parity-in-place-buffer)
  (list
   :buffer (buffer-string)
   :input-extension
   (file-name-extension reformatter-parity-last-input-file)
   :input-basename-sanitized
   (and
    (string-match-p
     (rx bos "reformatter" (+ any) ".rs" eos)
     (file-name-nondirectory
      reformatter-parity-last-input-file))
    t)
   :input-cleaned
   (not (file-exists-p
         reformatter-parity-last-input-file))))
"##;
    let expect = expect![[
        r##"OK (:buffer "// extension=rs\nfn ALPHA() {}\nlet ALPHA = 1;\n" :input-extension "rs" :input-basename-sanitized t :input-cleaned t)"##
    ]];
    ParityBatchCase::value(
        "file_backed_formatter_rewrites_pinned_extension_and_cleans_its_input_artifact",
        elisp_form,
        expect,
    )
}

fn buffer_local_arguments_and_working_directory_drive_real_formatter_invocation() -> ParityBatchCase
{
    let elisp_form = r##"
(let* ((root (make-temp-file "reformatter-context-" t))
       (workspace (expand-file-name "workspace/" root)))
  (unwind-protect
      (progn
        (make-directory workspace t)
        (with-temp-buffer
          (setq reformatter-parity-prefix "release-profile"
                reformatter-parity-working-directory workspace)
          (insert "task one\ntask two\n")
          (reformatter-parity-context-buffer)
          (list
           :buffer
           (reformatter-parity-normalize-root
            (buffer-string) root)
           :working-directory-exists
           (file-directory-p
            reformatter-parity-working-directory)
           :buffer-locals
           (list
            (local-variable-p 'reformatter-parity-prefix)
            (local-variable-p
             'reformatter-parity-working-directory)))))
    (delete-directory root t)))
"##;
    let expect = expect![[
        r##"OK (:buffer "release-profile|[PROJECT]/workspace\nTASK one\nTASK two\n" :working-directory-exists t :buffer-locals (t t))"##
    ]];
    ParityBatchCase::value(
        "buffer_local_arguments_and_working_directory_drive_real_formatter_invocation",
        elisp_form,
        expect,
    )
}

fn custom_exit_policy_accepts_transformed_stdout_from_documented_nonzero_status() -> ParityBatchCase
{
    let elisp_form = r##"
(with-temp-buffer
  (insert "pending build\npending tests\n")
  (let ((before (buffer-string)))
    (reformatter-parity-exit-three-buffer)
    (list
     :before before
     :after (buffer-string)
     :errors
     (reformatter-parity-error-summary
      'reformatter-parity-exit-three))))
"##;
    let expect = expect![[
        r##"OK (:before "pending build\npending tests\n" :after "accepted build\naccepted tests\n" :errors (:mode special-mode :read-only t :text ""))"##
    ]];
    ParityBatchCase::value(
        "custom_exit_policy_accepts_transformed_stdout_from_documented_nonzero_status",
        elisp_form,
        expect,
    )
}

fn failed_formatter_preserves_source_decodes_ansi_diagnostics_and_honors_display_policy()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (insert "valid line\nbroken syntax\n")
  (let ((before (buffer-string))
        displayed)
    (when (get-buffer "*reformatter-parity-failure errors*")
      (kill-buffer "*reformatter-parity-failure errors*"))
    (cl-letf (((symbol-function 'display-buffer)
               (lambda (buffer &rest _)
                 (push (buffer-name buffer) displayed)
                 buffer)))
      (reformatter-parity-failure-buffer nil)
      (let ((quiet
             (reformatter-parity-error-summary
              'reformatter-parity-failure)))
        (reformatter-parity-failure-region
         (point-min) (point-max) t)
        (list
         :source-before before
         :source-after (buffer-string)
         :quiet-errors quiet
         :displayed (nreverse displayed)
         :displayed-errors
         (reformatter-parity-error-summary
          'reformatter-parity-failure))))))
"##;
    let expect = expect![[
        r##"OK (:source-before "valid line\nbroken syntax\n" :source-after "valid line\nbroken syntax\n" :quiet-errors (:mode special-mode :read-only t :text "syntax error at line 2\n") :displayed ("*reformatter-parity-failure errors*") :displayed-errors (:mode special-mode :read-only t :text "syntax error at line 2\n"))"##
    ]];
    ParityBatchCase::value(
        "failed_formatter_preserves_source_decodes_ansi_diagnostics_and_honors_display_policy",
        elisp_form,
        expect,
    )
}

fn on_save_mode_formats_real_file_once_and_removes_buffer_local_hook_when_disabled()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (make-temp-file "reformatter-save-" t))
       (file (expand-file-name "release.txt" root))
       buffer
       result)
  (unwind-protect
      (progn
        (with-temp-file file
          (insert "release one\n"))
        (setq buffer (find-file-noselect file))
        (with-current-buffer buffer
          (goto-char (point-max))
          (insert "publish two\n")
          (reformatter-parity-stdio-on-save-mode 1)
          (let ((enabled
                 (list
                  :mode reformatter-parity-stdio-on-save-mode
                  :hook before-save-hook
                  :lighter
                  reformatter-parity-stdio-on-save-mode-lighter
                  :binding
                  (lookup-key
                   reformatter-parity-mode-map
                   (kbd "C-c f")))))
            (save-buffer)
            (let ((first-save
                   (with-temp-buffer
                     (insert-file-contents file)
                     (buffer-string))))
              (goto-char (point-max))
              (insert "release three\n")
              (reformatter-parity-stdio-on-save-mode 0)
              (let ((disabled
                     (list
                      :mode
                      reformatter-parity-stdio-on-save-mode
                      :hook before-save-hook)))
                (save-buffer)
                (setq result
                      (list
                       :enabled enabled
                       :first-save first-save
                       :disabled disabled
                       :second-save
                       (with-temp-buffer
                         (insert-file-contents file)
                         (buffer-string)))))))))
    (when (buffer-live-p buffer)
      (kill-buffer buffer))
    (delete-directory root t))
  result)
"##;
    let expect = expect![[
        r##"OK (:enabled (:mode t :hook (reformatter-parity-stdio-buffer t) :lighter " Fmt" :binding reformatter-parity-stdio-buffer) :first-save "RELEASE one\nPUBLISH two\n" :disabled (:mode nil :hook nil) :second-save "RELEASE one\nPUBLISH two\nrelease three\n")"##
    ]];
    ParityBatchCase::value(
        "on_save_mode_formats_real_file_once_and_removes_buffer_local_hook_when_disabled",
        elisp_form,
        expect,
    )
}

fn temporary_file_helpers_preserve_extensions_and_direct_operation_rejects_visited_file()
-> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (make-temp-file "reformatter-safety-" t))
       (visited (expand-file-name "source.toml" root))
       temp-default temp-visited temp-local rejection)
  (unwind-protect
      (progn
        (with-temp-file visited
          (insert "release = true\n"))
        (with-temp-buffer
          (setq default-directory root)
          (setq temp-default (reformatter-temp-file "yaml"))
          (set-visited-file-name visited t t)
          (insert "release = true\n")
          (setq temp-visited (reformatter-temp-file "ignored")
                temp-local
                (reformatter-temp-file-in-current-directory "ignored")
                rejection
                (condition-case error-data
                    (reformatter--do-region
                     'unsafe
                     (point-min) (point-max)
                     "sh" '("-c" "cat")
                     t t visited #'zerop nil)
                  (error
                   (list
                    (car error-data)
                    (error-message-string error-data))))))
        (list
         :default-extension (file-name-extension temp-default)
         :visited-extension (file-name-extension temp-visited)
         :local-extension (file-name-extension temp-local)
         :local-directory
         (equal (file-name-directory temp-local)
                (file-name-as-directory root))
         :reject-current-file rejection
         :visited-content
         (with-temp-buffer
           (insert-file-contents visited)
           (buffer-string))))
    (dolist (file (list temp-default temp-visited temp-local))
      (when (and file (file-exists-p file))
        (delete-file file)))
    (delete-directory root t)))
"##;
    let expect = expect![[
        r##"OK (:default-extension "yaml" :visited-extension "toml" :local-extension "toml" :local-directory t :reject-current-file (error "The reformatter must not operate on the current file in-place") :visited-content "release = true\n")"##
    ]];
    ParityBatchCase::value(
        "temporary_file_helpers_preserve_extensions_and_direct_operation_rejects_visited_file",
        elisp_form,
        expect,
    )
}

#[test]
fn reformatter_package_batch() {
    let cases = vec![
        generated_region_command_formats_only_selection_and_exposes_mode_specific_command_contract(
        ),
        whole_buffer_sort_replacement_preserves_unicode_content_markers_and_modified_state(),
        file_backed_formatter_rewrites_pinned_extension_and_cleans_its_input_artifact(),
        buffer_local_arguments_and_working_directory_drive_real_formatter_invocation(),
        custom_exit_policy_accepts_transformed_stdout_from_documented_nonzero_status(),
        failed_formatter_preserves_source_decodes_ansi_diagnostics_and_honors_display_policy(),
        on_save_mode_formats_real_file_once_and_removes_buffer_local_hook_when_disabled(),
        temporary_file_helpers_preserve_extensions_and_direct_operation_rejects_visited_file(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed reformatter parity test");
    assert_oracle_batch_cases(
        reformatter_oracle(),
        test_name,
        "reformatter_parity",
        &cases,
    );
}

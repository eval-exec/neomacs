use std::time::Duration;

use expect_test::expect;

use crate::{CLANG_FORMAT_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'clang-format)

(defun neomacs-clang-test-root ()
  "Return the isolated Clang-Format test root."
  (file-name-as-directory
   (expand-file-name "clang-format"
                     (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defun neomacs-clang-test-driver ()
  "Create a deterministic clang-format protocol peer and return its path."
  (let* ((root (neomacs-clang-test-root))
         (driver (expand-file-name "fake-clang-format" root)))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    (with-temp-file driver
      (insert
       "#!/bin/sh\n"
       "set -eu\n"
       "script_dir=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\n"
       "input_file=$script_dir/stdin\n"
       "trap 'rm -f \"$input_file\"' EXIT\n"
       "cat > \"$input_file\"\n"
       "byte_count=$(wc -c < \"$input_file\" | tr -d ' ')\n"
       "style=\n"
       "while [ \"$#\" -gt 0 ]; do\n"
       "  if [ -n \"${NEOMACS_CLANG_FORMAT_LOG:-}\" ]; then\n"
       "    printf '%s\\n' \"$1\" >> \"$NEOMACS_CLANG_FORMAT_LOG\"\n"
       "  fi\n"
       "  if [ \"$1\" = --style ] && [ \"$#\" -gt 1 ]; then\n"
       "    shift\n"
       "    style=$1\n"
       "    if [ -n \"${NEOMACS_CLANG_FORMAT_LOG:-}\" ]; then\n"
       "      printf '%s\\n' \"$1\" >> \"$NEOMACS_CLANG_FORMAT_LOG\"\n"
       "    fi\n"
       "  fi\n"
       "  shift\n"
       "done\n"
       "case $style in\n"
       "  neomacs-whole)\n"
       "    printf '%s\\n' \"<replacements incomplete_format=\\\"false\\\"><replacement offset=\\\"0\\\" length=\\\"$byte_count\\\">int main() {&#10;  int total = 1 + 2;&#10;  return total;&#10;}&#10;</replacement><cursor>37</cursor></replacements>\"\n"
       "    ;;\n"
       "  neomacs-region)\n"
       "    printf '%s\\n' '<replacements incomplete_format=\"false\"><replacement offset=\"30\" length=\"1\"> = </replacement><replacement offset=\"32\" length=\"1\"> + </replacement><cursor>30</cursor></replacements>'\n"
       "    ;;\n"
       "  neomacs-utf8)\n"
       "    printf '%s\\n' '<replacements incomplete_format=\"false\"><replacement offset=\"16\" length=\"15\">int value = 1;</replacement><cursor>25</cursor></replacements>'\n"
       "    ;;\n"
       "  neomacs-incomplete)\n"
       "    printf '%s\\n' 'recoverable parser diagnostic' >&2\n"
       "    printf '%s\\n' \"<replacements incomplete_format=\\\"true\\\"><replacement offset=\\\"0\\\" length=\\\"$byte_count\\\">int broken() { return 1; }&#10;</replacement><cursor>3</cursor></replacements>\"\n"
       "    ;;\n"
       "  neomacs-save)\n"
       "    printf '%s\\n' \"<replacements incomplete_format=\\\"false\\\"><replacement offset=\\\"0\\\" length=\\\"$byte_count\\\">int main() {&#10;  return 0;&#10;}&#10;</replacement><cursor>0</cursor></replacements>\"\n"
       "    ;;\n"
       "  neomacs-vc)\n"
       "    printf '%s\\n' \"<replacements incomplete_format=\\\"false\\\"><replacement offset=\\\"0\\\" length=\\\"$byte_count\\\">int main() {&#10;  int changed = 1;&#10;  return changed;&#10;}&#10;</replacement><cursor>0</cursor></replacements>\"\n"
       "    ;;\n"
       "  neomacs-error)\n"
       "    printf '%s\\n' 'deliberate formatter failure' >&2\n"
       "    exit 7\n"
       "    ;;\n"
       "  neomacs-malformed)\n"
       "    printf '%s\\n' '<replacements incomplete_format=\"false\"><replacement offset=\"0\">bad</replacement></replacements>'\n"
       "    ;;\n"
       "  *)\n"
       "    printf '%s\\n' \"unexpected style: $style\" >&2\n"
       "    exit 9\n"
       "    ;;\n"
       "esac\n"))
    (set-file-modes driver #o755)
    driver))

(defun neomacs-clang-test-start ()
  "Reset the fake formatter and argument log."
  (let* ((driver (neomacs-clang-test-driver))
         (log (expand-file-name "arguments" (neomacs-clang-test-root))))
    (setenv "NEOMACS_CLANG_FORMAT_LOG" log)
    (list driver log)))

(defun neomacs-clang-test-read-arguments (log)
  "Read LOG as normalized command arguments."
  (if (not (file-exists-p log))
      nil
    (with-temp-buffer
      (insert-file-contents log)
      (let ((root (regexp-quote (neomacs-clang-test-root))))
        (mapcar (lambda (argument)
                  (replace-regexp-in-string root "<ROOT>/" argument t t))
                (split-string (buffer-string) "\n" t))))))

(defun neomacs-clang-test-text-state ()
  "Return stable formatted buffer state."
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :line (line-number-at-pos)
        :column (current-column)))

(defun neomacs-clang-test-git (directory &rest arguments)
  "Run git with ARGUMENTS in DIRECTORY or signal an error."
  (let ((default-directory directory))
    (with-temp-buffer
      (let ((status (apply #'call-process "git" nil (current-buffer) nil arguments)))
        (unless (zerop status)
          (error "git %S failed: %s" arguments (buffer-string)))))))
"###;

fn package_contract_exposes_format_commands_configuration_and_save_mode() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'clang-format package-alist))))
  (with-temp-buffer
    (c++-mode)
    (let ((before (list clang-format-on-save-mode
                        (and (memq #'clang-format--on-save-buffer-hook
                                   before-save-hook) t))))
      (clang-format-on-save-mode 1)
      (let ((enabled (list clang-format-on-save-mode
                           (and (memq #'clang-format--on-save-buffer-hook
                                      before-save-hook) t))))
        (clang-format-on-save-mode -1)
        (list
         :package
         (list :name (package-desc-name descriptor)
               :version (package-version-join (package-desc-version descriptor))
               :requirements (package-desc-reqs descriptor)
               :feature (and (featurep 'clang-format) t))
         :commands
         (mapcar #'commandp
                 '(clang-format clang-format-region clang-format-buffer
                   clang-format-vc-diff clang-format-on-save-mode))
         :alias
         (eq (indirect-function 'clang-format)
             (indirect-function 'clang-format-region))
         :position-api
         (mapcar #'functionp
                 '(clang-format--bufferpos-to-filepos
                   clang-format--filepos-to-bufferpos))
         :defaults
         (list clang-format-style clang-format-fallback-style
               clang-format-on-save-p (stringp clang-format-executable))
         :mode (list :before before :enabled enabled
                     :disabled (list clang-format-on-save-mode
                                     (and (memq #'clang-format--on-save-buffer-hook
                                                before-save-hook) t))))))))
"###;
    let expected = expect![[
        r#"OK (:package (:name clang-format :version "20250223.1620" :requirements ((cl-lib (0 3))) :feature t) :commands (t t t t t) :alias t :position-api (t t) :defaults (nil "none" clang-format-on-save-check-config-exists t) :mode (:before (nil nil) :enabled (t t) :disabled (nil nil)))"#
    ]];
    ParityBatchCase::value(
        "package_contract_exposes_format_commands_configuration_and_save_mode",
        elisp_form,
        expected,
    )
}

fn whole_cpp_buffer_applies_xml_replacement_restores_cursor_and_forwards_policy() -> ParityBatchCase
{
    let elisp_form = r###"
(cl-destructuring-bind (driver log) (neomacs-clang-test-start)
  (with-temp-buffer
    (c++-mode)
    (insert "int main(){int total=1+2;return total;}")
    (search-backward "return")
    (let ((clang-format-executable driver)
          (clang-format-fallback-style "none")
          result)
      (setq result (clang-format-buffer "neomacs-whole" "service.cpp"))
      (list :result result
            :state (neomacs-clang-test-text-state)
            :arguments (neomacs-clang-test-read-arguments log)))))
"###;
    let expected = expect![[
        r#"OK (:result "(clang-format: success)" :state (:text "int main() {\n  int total = 1 + 2;\n  return total;\n}\n" :point 38 :line 3 :column 3) :arguments ("--output-replacements-xml" "--assume-filename" "service.cpp" "--style" "neomacs-whole" "--fallback-style" "none" "--offset" "0" "--length" "39" "--cursor" "25"))"#
    ]];
    ParityBatchCase::value(
        "whole_cpp_buffer_applies_xml_replacement_restores_cursor_and_forwards_policy",
        elisp_form,
        expected,
    )
}

fn selected_statement_formats_with_reverse_order_replacements_without_touching_neighbors()
-> ParityBatchCase {
    let elisp_form = r###"
(cl-destructuring-bind (driver log) (neomacs-clang-test-start)
  (with-temp-buffer
    (c++-mode)
    (insert "namespace checkout {\nint total=1+2;\nint untouched = 9;\n}\n")
    (goto-char (point-min))
    (forward-line 1)
    (let ((start (point))
          (end (line-end-position))
          result)
      (search-forward "total")
      (let ((clang-format-executable driver))
        (setq result
              (clang-format-region start end "neomacs-region" "checkout.cpp")))
      (list :result result
            :state (neomacs-clang-test-text-state)
            :arguments (neomacs-clang-test-read-arguments log)))))
"###;
    let expected = expect![[
        r#"OK (:result "(clang-format: success)" :state (:text "namespace checkout {\nint total = 1 + 2;\nint untouched = 9;\n}\n" :point 31 :line 2 :column 9) :arguments ("--output-replacements-xml" "--assume-filename" "checkout.cpp" "--style" "neomacs-region" "--fallback-style" "none" "--offset" "21" "--length" "14" "--cursor" "30"))"#
    ]];
    ParityBatchCase::value(
        "selected_statement_formats_with_reverse_order_replacements_without_touching_neighbors",
        elisp_form,
        expected,
    )
}

fn utf8_region_uses_file_byte_offsets_and_maps_the_returned_cursor_to_characters() -> ParityBatchCase
{
    let elisp_form = r###"
(cl-destructuring-bind (driver log) (neomacs-clang-test-start)
  (with-temp-buffer
    (c++-mode)
    (insert "// café 東京\nint  value=  1;")
    (goto-char (point-min))
    (forward-line 1)
    (let ((start (point))
          (end (line-end-position))
          result)
      (search-forward "value")
      (let ((clang-format-executable driver))
        (setq result
              (clang-format-region start end "neomacs-utf8" "unicode.cpp")))
      (list :result result
            :state (neomacs-clang-test-text-state)
            :arguments (neomacs-clang-test-read-arguments log)
            :byte-position (position-bytes (point))))))
"###;
    let expected = expect![[
        r#"OK (:result "(clang-format: success)" :state (:text "// café 東京\nint value = 1;" :point 21 :line 2 :column 9) :arguments ("--output-replacements-xml" "--assume-filename" "unicode.cpp" "--style" "neomacs-utf8" "--fallback-style" "none" "--offset" "16" "--length" "15" "--cursor" "26") :byte-position 26)"#
    ]];
    ParityBatchCase::value(
        "utf8_region_uses_file_byte_offsets_and_maps_the_returned_cursor_to_characters",
        elisp_form,
        expected,
    )
}

fn incomplete_formatter_result_applies_recovery_edits_and_reports_stderr() -> ParityBatchCase {
    let elisp_form = r###"
(cl-destructuring-bind (driver log) (neomacs-clang-test-start)
  (with-temp-buffer
    (c++-mode)
    (insert "int broken( {return 1;}")
    (goto-char (point-max))
    (let ((clang-format-executable driver)
          result)
      (setq result
            (clang-format-buffer "neomacs-incomplete" "broken.cpp"))
      (list :result result
            :state (neomacs-clang-test-text-state)
            :arguments (neomacs-clang-test-read-arguments log)))))
"###;
    let expected = expect![[
        r#"OK (:result "(clang-format: incomplete (syntax errors): recoverable parser diagnostic)" :state (:text "int broken() { return 1; }\n" :point 4 :line 1 :column 3) :arguments ("--output-replacements-xml" "--assume-filename" "broken.cpp" "--style" "neomacs-incomplete" "--fallback-style" "none" "--offset" "0" "--length" "23" "--cursor" "23"))"#
    ]];
    ParityBatchCase::value(
        "incomplete_formatter_result_applies_recovery_edits_and_reports_stderr",
        elisp_form,
        expected,
    )
}

fn project_config_detection_and_save_mode_format_only_eligible_buffers() -> ParityBatchCase {
    let elisp_form = r###"
(cl-destructuring-bind (driver log) (neomacs-clang-test-start)
  (let* ((root (neomacs-clang-test-root))
         (project (expand-file-name "project" root))
         (source (expand-file-name "src/main.cpp" project))
         (config (expand-file-name ".clang-format" project)))
    (make-directory (file-name-directory source) t)
    (with-temp-file config (insert "BasedOnStyle: LLVM\n"))
    (with-temp-buffer
      (c++-mode)
      (setq buffer-file-name source)
      (insert "int main(){return 0;}")
      (setq-local clang-format-executable driver)
      (setq-local clang-format-style "neomacs-save")
      (let ((eligible (clang-format-on-save-check-config-exists)))
        (clang-format-on-save-mode 1)
        (run-hooks 'before-save-hook)
        (let ((formatted (buffer-substring-no-properties (point-min) (point-max)))
              (enabled-hook (and (memq #'clang-format--on-save-buffer-hook
                                       before-save-hook) t)))
          (clang-format-on-save-mode -1)
          (goto-char (point-max))
          (insert "// pending edit")
          (run-hooks 'before-save-hook)
          (delete-file config)
          (let ((locate-dominating-stop-dir-regexp
                 (concat "\\`" (regexp-quote root) "\\'")))
            (list :eligible eligible
                :formatted formatted
                :enabled-hook enabled-hook
                :disabled-hook (and (memq #'clang-format--on-save-buffer-hook
                                          before-save-hook) t)
                :after-disabled-save
                (buffer-substring-no-properties (point-min) (point-max))
                :eligible-after-config-removal
                (clang-format-on-save-check-config-exists)
                :arguments (neomacs-clang-test-read-arguments log))))))))
"###;
    let expected = expect![[
        r#"OK (:eligible t :formatted "int main() {\n  return 0;\n}\n" :enabled-hook t :disabled-hook nil :after-disabled-save "int main() {\n  return 0;\n}\n// pending edit" :eligible-after-config-removal nil :arguments ("--output-replacements-xml" "--assume-filename" "<ROOT>/project/src/main.cpp" "--style" "neomacs-save" "--fallback-style" "none" "--offset" "0" "--length" "21" "--cursor" "21"))"#
    ]];
    ParityBatchCase::value(
        "project_config_detection_and_save_mode_format_only_eligible_buffers",
        elisp_form,
        expected,
    )
}

fn git_worktree_diff_formats_only_changed_line_ranges_from_head() -> ParityBatchCase {
    let elisp_form = r###"
(cl-destructuring-bind (driver log) (neomacs-clang-test-start)
  (let* ((root (neomacs-clang-test-root))
         (project (file-name-as-directory (expand-file-name "vc-project" root)))
         (source (expand-file-name "src/service.cpp" project))
         (baseline "int main() {\n  return 0;\n}\n")
         (changed "int main() {\nint changed=1;\nreturn changed;\n}"))
    (make-directory (file-name-directory source) t)
    (with-temp-file source (insert baseline))
    (neomacs-clang-test-git project "init" "--quiet" "--initial-branch=parity")
    (neomacs-clang-test-git project "config" "user.name" "Parity Bot")
    (neomacs-clang-test-git project "config" "user.email" "parity@example.invalid")
    (neomacs-clang-test-git project "add" "src/service.cpp")
    (neomacs-clang-test-git project "commit" "--quiet" "-m" "baseline")
    (let ((buffer (find-file-noselect source t)))
      (unwind-protect
          (with-current-buffer buffer
            (erase-buffer)
            (insert changed)
            (goto-char (point-max))
            (vc-refresh-state)
            (let ((clang-format-executable driver)
                  result)
              (setq result (clang-format-vc-diff "neomacs-vc" source))
              (list :result result
                    :state (neomacs-clang-test-text-state)
                    :arguments (neomacs-clang-test-read-arguments log)
                    :vc (list (substring-no-properties vc-mode)
                              (vc-backend source)
                              (vc-root-dir)))))
        (when (buffer-live-p buffer)
          (kill-buffer buffer))))))
"###;
    let expected = expect![[
        r#"OK (:result "(clang-format: success)" :state (:text "int main() {\n  int changed = 1;\n  return changed;\n}\n" :point 1 :line 1 :column 0) :arguments ("--output-replacements-xml" "--assume-filename" "<ROOT>/vc-project/src/service.cpp" "--style" "neomacs-vc" "--fallback-style" "none" "--lines=2:5" "--cursor" "45") :vc (" Git-parity" Git "[ORACLE-SANDBOX]/clang-format/vc-project/"))"#
    ]];
    ParityBatchCase::value(
        "git_worktree_diff_formats_only_changed_line_ranges_from_head",
        elisp_form,
        expected,
    )
}

fn process_and_protocol_failures_leave_source_unchanged_and_clean_temporary_files()
-> ParityBatchCase {
    let elisp_form = r###"
(cl-destructuring-bind (driver log) (neomacs-clang-test-start)
  (let* ((scratch (file-name-as-directory
                   (expand-file-name "scratch" (neomacs-clang-test-root))))
         (temporary-file-directory scratch))
    (make-directory scratch t)
    (with-temp-buffer
      (c++-mode)
      (insert "int main(){return 1;}")
      (let ((before (buffer-substring-no-properties (point-min) (point-max)))
            process-error protocol-error)
        (let ((clang-format-executable driver))
          (setq process-error
                (condition-case error
                    (progn
                      (clang-format-buffer "neomacs-error" "error.cpp")
                      :unexpected-success)
                  (error (error-message-string error))))
          (setq protocol-error
                (condition-case error
                    (progn
                      (clang-format-buffer "neomacs-malformed" "error.cpp")
                      :unexpected-success)
                  (error (error-message-string error)))))
        (list :process-error process-error
              :protocol-error protocol-error
              :unchanged (equal before
                                (buffer-substring-no-properties
                                 (point-min) (point-max)))
              :text (buffer-substring-no-properties (point-min) (point-max))
              :temporary-files (directory-files scratch nil
                                                directory-files-no-dot-files-regexp)
              :temporary-buffers
              (mapcar #'buffer-name
                      (cl-remove-if-not
                       (lambda (buffer)
                         (string-prefix-p " *clang-format-temp*"
                                          (buffer-name buffer)))
                       (buffer-list)))
              :arguments (neomacs-clang-test-read-arguments log))))))
"###;
    let expected = expect![[
        r#"OK (:process-error "(clang-format failed with code 7: deliberate formatter failure)" :protocol-error "<replacement> node does not have offset and length attributes" :unchanged t :text "int main(){return 1;}" :temporary-files nil :temporary-buffers nil :arguments ("--output-replacements-xml" "--assume-filename" "error.cpp" "--style" "neomacs-error" "--fallback-style" "none" "--offset" "0" "--length" "21" "--cursor" "21" "--output-replacements-xml" "--assume-filename" "error.cpp" "--style" "neomacs-malformed" "--fallback-style" "none" "--offset" "0" "--length" "21" "--cursor" "21"))"#
    ]];
    ParityBatchCase::value(
        "process_and_protocol_failures_leave_source_unchanged_and_clean_temporary_files",
        elisp_form,
        expected,
    )
}

#[test]
fn clang_format_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(CLANG_FORMAT_MELPA_PIN, "clang-format.el")
            .expect("prepare revision-pinned Clang-Format below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "clang-format-package-batch",
        "Clang-Format",
        &[
            package_contract_exposes_format_commands_configuration_and_save_mode(),
            whole_cpp_buffer_applies_xml_replacement_restores_cursor_and_forwards_policy(),
            selected_statement_formats_with_reverse_order_replacements_without_touching_neighbors(),
            utf8_region_uses_file_byte_offsets_and_maps_the_returned_cursor_to_characters(),
            incomplete_formatter_result_applies_recovery_edits_and_reports_stderr(),
            project_config_detection_and_save_mode_format_only_eligible_buffers(),
            git_worktree_diff_formats_only_changed_line_ranges_from_head(),
            process_and_protocol_failures_leave_source_unchanged_and_clean_temporary_files(),
        ],
    );
}

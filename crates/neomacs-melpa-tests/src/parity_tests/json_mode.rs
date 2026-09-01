use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, JSON_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'json-mode)

(defun neomacs-json-mode-test-buffer (text mode body)
  "Run BODY in a temporary MODE buffer containing TEXT."
  (with-temp-buffer
    (insert text)
    (funcall mode)
    (goto-char (point-min))
    (funcall body)))

(defun neomacs-json-mode-test-auto-mode (filename &optional contents)
  "Return the mode selected for FILENAME containing CONTENTS."
  (with-temp-buffer
    (setq buffer-file-name filename)
    (when contents (insert contents))
    (set-auto-mode)
    major-mode))

(defun neomacs-json-mode-test-face-at (needle offset)
  "Return the font-lock face OFFSET characters into NEEDLE."
  (save-excursion
    (goto-char (point-min))
    (search-forward needle)
    (get-text-property (+ (match-beginning 0) offset) 'face)))

(defun neomacs-json-mode-test-edit (text mode needle offset operation &optional argument)
  "Apply OPERATION to TEXT at OFFSET within NEEDLE and report editor state."
  (neomacs-json-mode-test-buffer
   text mode
   (lambda ()
     (search-forward needle)
     (goto-char (+ (match-beginning 0) offset))
     (let ((before (point))
           (kill-ring nil))
       (if argument
           (funcall operation argument)
         (funcall operation))
       (list :text (buffer-string)
             :before before
             :after (point)
             :mark (mark t)
             :kill (car-safe kill-ring))))))

(defun neomacs-json-mode-test-capture (function)
  "Return FUNCTION's value or exact signaled error data."
  (condition-case error-data
      (list :ok (funcall function))
    (error
     (list :error (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))
"####;

fn project_files_open_with_json_editor_configuration_and_semantic_highlighting() -> ParityBatchCase
{
    let elisp_form = r####"
(let ((descriptor (cadr (assq 'json-mode package-alist))))
 (list
 :package
 (list :name (package-desc-name descriptor)
       :version (package-version-join (package-desc-version descriptor))
       :requirements (package-desc-reqs descriptor)
       :feature (featurep 'json-mode))
 :files
 (mapcar
  (lambda (name)
    (list name
          (neomacs-json-mode-test-auto-mode
           (concat temporary-file-directory name))))
  '("service.json" "context.jsonld" ".babelrc" ".bowerrc" "composer.lock"))
 :magic
 (neomacs-json-mode-test-auto-mode
  (concat temporary-file-directory "configuration-without-extension") "{")
 :editor
 (neomacs-json-mode-test-buffer
  "{\"service.name\": \"café\", \"enabled\": true, \"retries\": -2.5}\n"
  #'json-mode
  (lambda ()
    (font-lock-ensure)
    (list
     :mode major-mode
     :name mode-name
     :derived (derived-mode-p 'javascript-mode)
     :forward forward-sexp-function
     :indent js-indent-level
     :keys
     (mapcar
      (lambda (key) (list key (lookup-key json-mode-map (kbd key))))
      '("C-c C-f" "C-c C-p" "C-c C-k" "C-c C-t" "C-c C-i" "C-c C-d"))
     :faces
     (list
      (neomacs-json-mode-test-face-at "service.name" 1)
      (neomacs-json-mode-test-face-at "café" 1)
      (neomacs-json-mode-test-face-at "true" 1)
      (neomacs-json-mode-test-face-at "-2.5" 1)))))))
"####;
    let expected = expect![[
        r#"OK (:package (:name json-mode :version "20240427.1245" :requirements ((json-snatcher (1 0 0)) (emacs (24 4))) :feature t) :files (("service.json" json-mode) ("context.jsonld" json-mode) (".babelrc" json-mode) (".bowerrc" json-mode) ("composer.lock" json-mode)) :magic json-mode :editor (:mode json-mode :name "JSON" :derived javascript-mode :forward json-mode-forward-sexp :indent 4 :keys (("C-c C-f" json-mode-beautify) ("C-c C-p" json-mode-show-path) ("C-c C-k" json-nullify-sexp) ("C-c C-t" json-toggle-boolean) ("C-c C-i" json-increment-number-at-point) ("C-c C-d" json-decrement-number-at-point)) :faces (font-lock-keyword-face font-lock-string-face font-lock-constant-face font-lock-constant-face)))"#
    ]];
    ParityBatchCase::value(
        "project_files_open_with_json_editor_configuration_and_semantic_highlighting",
        elisp_form,
        expected,
    )
}

fn format_whole_documents_and_only_the_users_active_selection() -> ParityBatchCase {
    let elisp_form = r####"
(let (whole selected)
  (setq whole
        (neomacs-json-mode-test-buffer
         "{\"service\":\"api\",\"ports\":[8080,8443],\"labels\":{\"tier\":\"edge\"}}"
         #'json-mode
         (lambda ()
           (goto-char 13)
           (json-mode-beautify (point-min) (point-max))
           (list :text (buffer-string)
                 :point (point)
                 :mark (mark t)
                 :modified (buffer-modified-p)))))
  (setq selected
        (neomacs-json-mode-test-buffer
         "{\"release\":true,\"targets\":[\"linux\",\"windows\"]}\n{\"untouched\":true}"
         #'json-mode
         (lambda ()
           (goto-char (point-min))
           (set-mark (line-end-position))
           (setq mark-active t transient-mark-mode t)
           (json-mode-beautify (region-beginning) (region-end))
           (list :text (buffer-string)
                 :point (point)
                 :mark (mark t)
                 :active mark-active))))
  (list :whole whole :selection selected))
"####;
    let expected = expect![[
        r#"OK (:whole (:text "{\n  \"service\": \"api\",\n  \"ports\": [\n    8080,\n    8443\n  ],\n  \"labels\": {\n    \"tier\": \"edge\"\n  }\n}" :point 17 :mark nil :modified t) :selection (:text "{\n  \"release\": true,\n  \"targets\": [\n    \"linux\",\n    \"windows\"\n  ]\n}\n{\"untouched\":true}" :point 1 :mark 69 :active t))"#
    ]];
    ParityBatchCase::value(
        "format_whole_documents_and_only_the_users_active_selection",
        elisp_form,
        expected,
    )
}

fn toggle_release_flags_without_rewriting_lookalikes_strings_or_comments() -> ParityBatchCase {
    let elisp_form = r####"
(list
 :true-at-start
 (neomacs-json-mode-test-edit
  "{\"enabled\":true}" #'json-mode "true" 0 #'json-toggle-boolean)
 :false-at-end
 (neomacs-json-mode-test-edit
  "{\"enabled\":false}" #'json-mode "false" 5 #'json-toggle-boolean)
 :identifier-lookalike
 (neomacs-json-mode-test-edit
  "{\"enabled\":trueValue}" #'json-mode "trueValue" 2 #'json-toggle-boolean)
 :quoted
 (neomacs-json-mode-test-edit
  "{\"note\":\"true\"}" #'json-mode "true" 2 #'json-toggle-boolean)
 :line-comment
 (neomacs-json-mode-test-edit
  "{\n // false means staged\n \"enabled\": true\n}" #'jsonc-mode "false" 2 #'json-toggle-boolean)
 :block-comment
 (neomacs-json-mode-test-edit
  "{/* true is documented */\"enabled\":false}" #'jsonc-mode "true" 1 #'json-toggle-boolean))
"####;
    let expected = expect![[
        r#"OK (:true-at-start (:text "{\"enabled\":false}" :before 12 :after 12 :mark nil :kill nil) :false-at-end (:text "{\"enabled\":true}" :before 17 :after 16 :mark nil :kill nil) :identifier-lookalike (:text "{\"enabled\":trueValue}" :before 14 :after 14 :mark nil :kill nil) :quoted (:text "{\"note\":\"true\"}" :before 12 :after 12 :mark nil :kill nil) :line-comment (:text "{\n // false means staged\n \"enabled\": true\n}" :before 9 :after 9 :mark nil :kill nil) :block-comment (:text "{/* true is documented */\"enabled\":false}" :before 6 :after 6 :mark nil :kill nil))"#
    ]];
    ParityBatchCase::value(
        "toggle_release_flags_without_rewriting_lookalikes_strings_or_comments",
        elisp_form,
        expected,
    )
}

fn adjust_integer_decimal_and_negative_settings_with_cursor_stability() -> ParityBatchCase {
    let elisp_form = r####"
(list
 :integer
 (neomacs-json-mode-test-edit
  "{\"workers\":9}" #'json-mode "9" 0 #'json-increment-number-at-point)
 :decimal-delta
 (neomacs-json-mode-test-edit
  "{\"timeout\":1.25}" #'json-mode "1.25" 2 #'json-increment-number-at-point 0.75)
 :negative-decrement
 (neomacs-json-mode-test-edit
  "{\"offset\":-3}" #'json-mode "-3" 1 #'json-decrement-number-at-point 4)
 :end-of-growing-number
 (neomacs-json-mode-test-edit
  "{\"retries\":99}" #'json-mode "99" 2 #'json-increment-number-at-point)
 :non-number
 (neomacs-json-mode-test-edit
  "{\"retries\":null}" #'json-mode "null" 2 #'json-increment-number-at-point))
"####;
    let expected = expect![[
        r#"OK (:integer (:text "{\"workers\":10}" :before 12 :after 12 :mark nil :kill nil) :decimal-delta (:text "{\"timeout\":2.0}" :before 14 :after 14 :mark nil :kill nil) :negative-decrement (:text "{\"offset\":-7}" :before 12 :after 12 :mark nil :kill nil) :end-of-growing-number (:text "{\"retries\":100}" :before 14 :after 14 :mark nil :kill nil) :non-number (:text "{\"retries\":null}" :before 14 :after 14 :mark nil :kill nil))"#
    ]];
    ParityBatchCase::value(
        "adjust_integer_decimal_and_negative_settings_with_cursor_stability",
        elisp_form,
        expected,
    )
}

fn replace_nested_values_and_objects_with_json_null_while_preserving_comments() -> ParityBatchCase {
    let elisp_form = r####"
(list
 :string-value
 (neomacs-json-mode-test-edit
  "{\"deployment\":{\"region\":\"us-east-1\",\"replicas\":3}}"
  #'json-mode "us-east-1" 4 #'json-nullify-sexp)
 :number-value
 (neomacs-json-mode-test-edit
  "{\"deployment\":{\"region\":\"us-east-1\",\"replicas\":3}}"
  #'json-mode "3" 0 #'json-nullify-sexp)
 :object-key
 (neomacs-json-mode-test-edit
  "{\"deployment\":{\"region\":\"us-east-1\",\"replicas\":3},\"keep\":true}"
  #'json-mode "region" 3 #'json-nullify-sexp)
 :array-element
 (neomacs-json-mode-test-edit
  "{\"targets\":[\"linux\",{\"os\":\"windows\",\"arch\":\"x86_64\"}]}"
  #'json-mode "windows" 3 #'json-nullify-sexp)
 :comment
 (neomacs-json-mode-test-edit
  "{\n // nullify false only after review\n \"release\": false\n}"
  #'jsonc-mode "false" 2 #'json-nullify-sexp))
"####;
    let expected = expect![[
        r#"OK (:string-value (:text "{\"deployment\":{\"region\":null,\"replicas\":3}}" :before 30 :after 29 :mark nil :kill "\"us-east-1\"") :number-value (:text "{\"deployment\":{\"region\":\"us-east-1\",\"replicas\":null}}" :before 48 :after 52 :mark nil :kill "3") :object-key (:text "{\"deployment\":null,\"keep\":true}" :before 20 :after 19 :mark nil :kill "{\"region\":\"us-east-1\",\"replicas\":3}") :array-element (:text "{\"targets\":[\"linux\",{\"os\":null,\"arch\":\"x86_64\"}]}" :before 31 :after 31 :mark nil :kill "\"windows\"") :comment (:text "{\n // nullify false only after review\n \"release\": false\n}" :before 17 :after 17 :mark nil :kill nil))"#
    ]];
    ParityBatchCase::value(
        "replace_nested_values_and_objects_with_json_null_while_preserving_comments",
        elisp_form,
        expected,
    )
}

fn inspect_and_copy_paths_for_nested_objects_and_array_values() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-json-mode-test-buffer
 "{\"deployments\":[{\"name\":\"api\",\"regions\":[\"iad\",\"fra\"]},{\"name\":\"worker\"}]}"
 #'json-mode
 (lambda ()
   (search-forward "fra")
   (backward-char 1)
   (let ((printed (with-output-to-string (json-mode-show-path)))
         (shown (current-message)))
     (setq kill-ring nil)
     (let ((copied-output (with-output-to-string (json-mode-kill-path))))
       (list :printed printed
             :shown shown
             :copied-output copied-output
             :kill-ring kill-ring
             :point (point)
             :cache-present
             (and (gethash (current-buffer) jsons-parsed) t)
             :region-count
             (length (gethash (current-buffer) jsons-parsed-regions)))))))
"####;
    let expected = expect![[
        r#"OK (:printed "[\"deployments\"][0][\"regions\"][1]" :shown nil :copied-output "[\"deployments\"][0][\"regions\"][1]" :kill-ring ("[\"deployments\"][0][\"regions\"][1]" "[\"deployments\"][0][\"regions\"][1]") :point 51 :cache-present t :region-count 8)"#
    ]];
    ParityBatchCase::value(
        "inspect_and_copy_paths_for_nested_objects_and_array_values",
        elisp_form,
        expected,
    )
}

fn navigate_balanced_json_and_jsonc_strings_comments_arrays_and_objects() -> ParityBatchCase {
    let elisp_form = r####"
(let (json jsonc)
  (setq json
        (neomacs-json-mode-test-buffer
         "{\"endpoint\":\"api.v2/users\",\"items\":[10,{\"ok\":true}]}"
         #'json-mode
         (lambda ()
           (let (steps)
             (search-forward "endpoint")
             (goto-char (match-beginning 0))
             (dotimes (_ 4)
               (let ((before (point)))
                 (forward-sexp 1)
                 (push (list (buffer-substring-no-properties before (point))
                             :from before :to (point))
                       steps)))
             (forward-sexp -2)
             (list :steps (nreverse steps)
                   :after-backward
                   (list :point (point)
                         :rest (buffer-substring-no-properties
                                (point) (min (+ (point) 12) (point-max)))))))))
  (setq jsonc
        (neomacs-json-mode-test-buffer
         "{\n // deployment target\n \"url\": \"https://api.example/v2\",\n /* fallback */ \"retry\": 3\n}"
         #'jsonc-mode
         (lambda ()
           (font-lock-ensure)
           (let ((line-state
                  (progn (search-forward "deployment") (syntax-ppss)))
                 (block-state
                  (progn (search-forward "fallback") (syntax-ppss)))
                 (url-state
                  (progn (goto-char (point-min))
                         (search-forward "https://")
                         (syntax-ppss))))
             (goto-char (point-min))
             (forward-sexp 1)
             (list :mode major-mode
                   :line-comment (list (nth 4 line-state) (nth 8 line-state))
                   :block-comment (list (nth 4 block-state) (nth 8 block-state))
                   :url-string (list (nth 3 url-state) (nth 4 url-state) (nth 8 url-state))
                   :whole-object-end (point)
                   :max (point-max))))))
  (list :json json :jsonc jsonc))
"####;
    let expected = expect![[
        r#"OK (:json (:steps (("endpoint" :from 3 :to 11) ("\":\"" :from 11 :to 14) ("api" :from 14 :to 17) (".v2/users" :from 17 :to 26)) :after-backward (:point 14 :rest "api.v2/users")) :jsonc (:mode jsonc-mode :line-comment (t 4) :block-comment (1 60) :url-string (34 nil 33) :whole-object-end 87 :max 87))"#
    ]];
    ParityBatchCase::value(
        "navigate_balanced_json_and_jsonc_strings_comments_arrays_and_objects",
        elisp_form,
        expected,
    )
}

fn custom_filename_associations_replace_only_the_packages_generated_entry() -> ParityBatchCase {
    let elisp_form = r####"
(let ((saved-value json-mode-auto-mode-list)
      (saved-entry json-mode--auto-mode-entry)
      (saved-alist (copy-tree auto-mode-alist)))
  (unwind-protect
      (progn
        (add-to-list 'auto-mode-alist '("\\.company-json\\'" . text-mode))
        (customize-set-variable
         'json-mode-auto-mode-list '("workspace.lock" ".service-config"))
        (let ((first-entry json-mode--auto-mode-entry)
              (first-modes
               (mapcar
                (lambda (name)
                  (list name
                        (neomacs-json-mode-test-auto-mode
                         (concat temporary-file-directory name))))
                '("workspace.lock" ".service-config" "legacy.lock" "x.company-json"))))
          (customize-set-variable
           'json-mode-auto-mode-list '("legacy.lock"))
          (list
           :first-entry first-entry
           :first-modes first-modes
           :old-entry-removed (not (member first-entry auto-mode-alist))
           :second-entry json-mode--auto-mode-entry
           :second-modes
           (mapcar
            (lambda (name)
              (list name
                    (neomacs-json-mode-test-auto-mode
                     (concat temporary-file-directory name))))
            '("workspace.lock" ".service-config" "legacy.lock" "x.company-json"))
           :custom-entry
           (and (member '("\\.company-json\\'" . text-mode) auto-mode-alist) t))))
    (setq-default json-mode-auto-mode-list saved-value)
    (setq json-mode--auto-mode-entry saved-entry
          auto-mode-alist saved-alist)))
"####;
    let expected = expect![[
        r#"OK (:first-entry ("\\(?:\\(?:\\.\\(?:json\\(?:ld\\)?\\|service-config\\)\\|workspace\\.lock\\)\\'\\)" . json-mode) :first-modes (("workspace.lock" json-mode) (".service-config" json-mode) ("legacy.lock" fundamental-mode) ("x.company-json" text-mode)) :old-entry-removed t :second-entry ("\\(?:\\(?:\\.json\\(?:ld\\)?\\|legacy\\.lock\\)\\'\\)" . json-mode) :second-modes (("workspace.lock" fundamental-mode) (".service-config" fundamental-mode) ("legacy.lock" json-mode) ("x.company-json" text-mode)) :custom-entry t)"#
    ]];
    ParityBatchCase::value(
        "custom_filename_associations_replace_only_the_packages_generated_entry",
        elisp_form,
        expected,
    )
}

fn malformed_json_reports_the_native_pretty_printer_error_without_partial_rewrite()
-> ParityBatchCase {
    let elisp_form = r####"
(neomacs-json-mode-test-buffer
 "{\"service\":\"api\",\"ports\":[8080"
 #'json-mode
 (lambda ()
   (let ((before (buffer-string)))
     (list
      :outcome
      (neomacs-json-mode-test-capture
       (lambda ()
         (json-mode-beautify (point-min) (point-max))))
      :before before
      :after (buffer-string)
      :point (point)))))
"####;
    let expected = expect![[
        r#"OK (:outcome (:error json-array-format :data ("," 0) :message "Bad JSON array: \",\", 0") :before "{\"service\":\"api\",\"ports\":[8080" :after "{\"service\":\"api\",\"ports\":[8080" :point 1)"#
    ]];
    ParityBatchCase::value(
        "malformed_json_reports_the_native_pretty_printer_error_without_partial_rewrite",
        elisp_form,
        expected,
    )
}

#[test]
fn json_mode_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(JSON_MODE_MELPA_PIN, "json-mode.el")
            .expect("prepare revision-pinned JSON Mode source below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "json-mode-package-batch",
        "JSON Mode",
        &[
            project_files_open_with_json_editor_configuration_and_semantic_highlighting(),
            format_whole_documents_and_only_the_users_active_selection(),
            toggle_release_flags_without_rewriting_lookalikes_strings_or_comments(),
            adjust_integer_decimal_and_negative_settings_with_cursor_stability(),
            replace_nested_values_and_objects_with_json_null_while_preserving_comments(),
            inspect_and_copy_paths_for_nested_objects_and_array_values(),
            navigate_balanced_json_and_jsonc_strings_comments_arrays_and_objects(),
            custom_filename_associations_replace_only_the_packages_generated_entry(),
            malformed_json_reports_the_native_pretty_printer_error_without_partial_rewrite(),
        ],
    );
}

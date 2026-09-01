use expect_test::expect;

use super::ParityBatchCase;

fn realistic_requirements_fontify_names_operators_versions_and_comments() -> ParityBatchCase {
    ParityBatchCase::value(
        "realistic_requirements_fontify_names_operators_versions_and_comments",
        r##"
(with-temp-buffer
  (neomacs-pip-requirements-test-fontify
   (concat
    "# Production dependencies\n"
    "Django==5.1.2\n"
    "uvicorn>=0.30.0\n"
    "my_package~=2.0.post1\n"
    "legacy.pkg!=1.4.*\n"
    "build-tool===release-candidate+λ\n"
    "--index-url https://packages.example.test/simple\n"))
  (list :mode major-mode
        :derived (derived-mode-p 'prog-mode)
        :comment-start comment-start
        :name (neomacs-pip-requirements-test-token "Django")
        :operators
        (mapcar #'neomacs-pip-requirements-test-token
                '("==" ">=" "~=" "!=" "==="))
        :versions
        (mapcar #'neomacs-pip-requirements-test-token
                '("5.1.2" "0.30.0" "2.0.post1" "1.4.*"
                  "release-candidate+λ"))
        :comment
        (neomacs-pip-requirements-test-token
         "Production dependencies")
        :text (buffer-substring-no-properties
               (point-min) (point-max))))
"##,
        expect![[
            r##"OK (:mode pip-requirements-mode :derived prog-mode :comment-start "#" :name ((:line 2 :face font-lock-variable-name-face)) :operators (((:line 2 :face font-lock-builtin-face) (:line 6 :face font-lock-builtin-face)) ((:line 3 :face font-lock-builtin-face)) ((:line 4 :face font-lock-builtin-face)) ((:line 5 :face font-lock-builtin-face)) ((:line 6 :face font-lock-builtin-face))) :versions (((:line 2 :face font-lock-constant-face)) ((:line 3 :face font-lock-constant-face)) ((:line 4 :face font-lock-constant-face)) ((:line 5 :face font-lock-constant-face)) ((:line 6 :face font-lock-constant-face))) :comment ((:line 1 :face font-lock-comment-face)) :text "# Production dependencies\nDjango==5.1.2\nuvicorn>=0.30.0\nmy_package~=2.0.post1\nlegacy.pkg!=1.4.*\nbuild-tool===release-candidate+λ\n--index-url https://packages.example.test/simple\n")"##
        ]],
    )
}

fn comment_commands_toggle_selected_dependencies_without_touching_neighbors() -> ParityBatchCase {
    ParityBatchCase::value(
        "comment_commands_toggle_selected_dependencies_without_touching_neighbors",
        r##"
(with-temp-buffer
  (neomacs-pip-requirements-test-fontify
   "requests==2.32.0\nrich==13.7.1\nruff==0.5.0\n")
  (goto-char (point-min))
  (forward-line 1)
  (comment-line 1)
  (let ((commented (buffer-substring-no-properties
                    (point-min) (point-max))))
    (comment-line 1)
    (list :commented commented
          :restored (buffer-substring-no-properties
                     (point-min) (point-max))
          :point-line (line-number-at-pos)
          :mode major-mode)))
"##,
        expect![[
            r#"OK (:commented "requests==2.32.0\n# rich==13.7.1\nruff==0.5.0\n" :restored "requests==2.32.0\n# rich==13.7.1\n# ruff==0.5.0\n" :point-line 4 :mode pip-requirements-mode)"#
        ]],
    )
}

fn built_in_completion_uses_symbol_bounds_and_current_package_catalog() -> ParityBatchCase {
    ParityBatchCase::value(
        "built_in_completion_uses_symbol_bounds_and_current_package_catalog",
        r##"
(let ((pip-packages '("requests" "requests-cache" "rich" "ruff" "urllib3")))
  (with-temp-buffer
    (insert "requ")
    (pip-requirements-mode)
    (goto-char (point-max))
    (let* ((capf (pip-requirements-complete-at-point))
           (start (nth 0 capf))
           (end (nth 1 capf))
           (table (nth 2 capf)))
      (list :bounds (list start end)
            :prefix (buffer-substring-no-properties start end)
            :matches (all-completions
                      (buffer-substring-no-properties start end)
                      table)
            :hook-count
            (cl-count #'pip-requirements-complete-at-point
                      completion-at-point-functions)))))
"##,
        expect![[
            r#"OK (:bounds (1 5) :prefix "requ" :matches ("requests" "requests-cache") :hook-count 1)"#
        ]],
    )
}

fn pypi_simple_index_callback_parses_hyphen_dot_and_underscore_names() -> ParityBatchCase {
    ParityBatchCase::value(
        "pypi_simple_index_callback_parses_hyphen_dot_and_underscore_names",
        r##"
(let ((pip-packages nil)
      (pip-http-buffer (generate-new-buffer " *pip-index-test*")))
  (with-current-buffer pip-http-buffer
    (insert
     "HTTP/1.1 200 OK\nContent-Type: text/html\n\n"
     "<html><body>\n"
     "<h1>Simple index</h1>\n"
     "<a href=\"requests/\">requests</a>\n"
     "<a href=\"zope.interface/\">zope.interface</a>\n"
     "<a href=\"my_package/\">my_package</a>\n"
     "<a href=\"naive-pkg/\">naive-pkg</a>\n"
     "</body></html>"))
  (pip-requirements-callback)
  (list :packages pip-packages
        :buffer-live (buffer-live-p pip-http-buffer)))
"##,
        expect![[
            r#"OK (:packages ("requests" "zope.interface" "my_package" "naive-pkg") :buffer-live nil)"#
        ]],
    )
}

fn fetch_packages_routes_configured_url_and_callback_through_http_boundary() -> ParityBatchCase {
    ParityBatchCase::value(
        "fetch_packages_routes_configured_url_and_callback_through_http_boundary",
        r##"
(let ((pip-requirements-index-url
       "https://packages.example.test/simple/")
      called)
  (cl-letf (((symbol-function 'url-retrieve)
             (lambda (url callback cbargs silent)
               (setq called
                     (list :url url :callback callback
                           :args cbargs :silent silent))
               (get-buffer-create " *pip-http-boundary*"))))
    (unwind-protect
        (list :result (buffer-name (pip-requirements-fetch-packages))
              :called called
              :global-buffer (buffer-name pip-http-buffer))
      (when (buffer-live-p pip-http-buffer)
        (kill-buffer pip-http-buffer)))))
"##,
        expect![[
            r#"OK (:result " *pip-http-boundary*" :called (:url "https://packages.example.test/simple/" :callback pip-requirements-callback :args nil :silent t) :global-buffer " *pip-http-boundary*")"#
        ]],
    )
}

fn auto_complete_setup_registers_mode_source_and_enables_available_frontend() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_setup_registers_mode_source_and_enables_available_frontend",
        r##"
(let ((old-modes (and (boundp 'ac-modes) ac-modes))
      (old-sources (and (boundp 'ac-sources) ac-sources))
      (old-mode (and (boundp 'auto-complete-mode) auto-complete-mode))
      (modes-bound (boundp 'ac-modes))
      (sources-bound (boundp 'ac-sources))
      (mode-bound (boundp 'auto-complete-mode))
      calls)
  (unwind-protect
      (progn
        (setq ac-modes '(text-mode)
              ac-sources '((candidates . existing-source))
              auto-complete-mode nil)
        (cl-letf (((symbol-function 'auto-complete-mode)
                   (lambda (&optional arg)
                     (setq auto-complete-mode
                           (if (or (null arg) (> arg 0)) t nil))
                     (push arg calls))))
          (pip-requirements-auto-complete-setup)
          (pip-requirements-auto-complete-setup)
          (list :modes ac-modes
                :sources ac-sources
                :enabled auto-complete-mode
                :calls (nreverse calls))))
    (if modes-bound (setq ac-modes old-modes) (makunbound 'ac-modes))
    (if sources-bound (setq ac-sources old-sources) (makunbound 'ac-sources))
    (if mode-bound
        (setq auto-complete-mode old-mode)
      (makunbound 'auto-complete-mode))))
"##,
        expect![
            "OK (:modes (pip-requirements-mode text-mode) :sources (((candidates . pip-packages)) (candidates . existing-source)) :enabled t :calls (nil))"
        ],
    )
}

fn file_name_patterns_activate_only_supported_requirements_files() -> ParityBatchCase {
    ParityBatchCase::value(
        "file_name_patterns_activate_only_supported_requirements_files",
        r##"
(mapcar
 (lambda (name)
   (cons name (assoc-default name auto-mode-alist #'string-match)))
 '("prod.pip"
   "requirements.txt"
   "requirements-dev.txt"
   "requirements.in"
   "requirements.md"
   "dependencies.txt"))
"##,
        expect![[
            r#"OK (("prod.pip" . pip-requirements-mode) ("requirements.txt" . pip-requirements-mode) ("requirements-dev.txt" . pip-requirements-mode) ("requirements.in" . pip-requirements-mode) ("requirements.md") ("dependencies.txt" . text-mode))"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        realistic_requirements_fontify_names_operators_versions_and_comments(),
        comment_commands_toggle_selected_dependencies_without_touching_neighbors(),
        built_in_completion_uses_symbol_bounds_and_current_package_catalog(),
        pypi_simple_index_callback_parses_hyphen_dot_and_underscore_names(),
        fetch_packages_routes_configured_url_and_callback_through_http_boundary(),
        auto_complete_setup_registers_mode_source_and_enables_available_frontend(),
        file_name_patterns_activate_only_supported_requirements_files(),
    ]
}

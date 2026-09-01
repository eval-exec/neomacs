use expect_test::expect;

use super::ParityBatchCase;

fn render_buffer_posts_markdown_and_shows_shr_preview() -> ParityBatchCase {
    ParityBatchCase::value(
        "render_buffer_posts_markdown_and_shows_shr_preview",
        r####"
(neomacs-gh-md-test-cleanup)
(let ((source (generate-new-buffer "release.md"))
      (gh-md-use-gfm nil)
      (gh-md-context nil)
      (gh-md-css-path nil)
      (gh-md-extra-header nil)
      outcome)
  (unwind-protect
      (with-current-buffer source
        (insert "# Release\n\nShip the **widget** with café notes.\n")
        (setq outcome
              (neomacs-gh-md-test-with-transport
               "<h1>Release</h1><p>Ship the <strong>widget</strong> with café notes.</p>"
               (lambda ()
                 (gh-md-render-buffer)
                 (neomacs-gh-md-test-view)))))
    (when (buffer-live-p source) (kill-buffer source)))
  (list :view (plist-get outcome :result)
        :request (car (plist-get outcome :requests))))
"####,
        expect![[
            r##"OK (:view (:name "*gh-md*" :mode eww-mode :text "Release\n\n\nShip\nthe\nwidget\nwith\ncafé\nnotes." :point 1 :read-only t :file nil :modified t) :request (:url "https://api.github.com/markdown" :method "POST" :data "{\"text\":\"# Release\\n\\nShip the **widget** with café notes.\\n\",\"mode\":\"markdown\",\"context\":null}" :silent silent))"##
        ]],
    )
}

fn render_region_sends_only_the_selected_markdown_slice() -> ParityBatchCase {
    ParityBatchCase::value(
        "render_region_sends_only_the_selected_markdown_slice",
        r####"
(neomacs-gh-md-test-cleanup)
(let ((source (generate-new-buffer "region.md"))
      (gh-md-use-gfm nil)
      (gh-md-context nil)
      outcome payload)
  (unwind-protect
      (with-current-buffer source
        (insert "IGNORE\n# Selected\n\nOnly this slice.\nTRAILING\n")
        (goto-char (point-min))
        (search-forward "# Selected")
        (beginning-of-line)
        (let ((begin (point)))
          (search-forward "slice.")
          (setq outcome
                (neomacs-gh-md-test-with-transport
                 "<h1>Selected</h1><p>Only this slice.</p>"
                 (lambda ()
                   (gh-md-render-region begin (point))
                   (neomacs-gh-md-test-view)))))
        (setq payload
              (json-read-from-string
               (plist-get (car (plist-get outcome :requests)) :data))))
    (when (buffer-live-p source) (kill-buffer source)))
  (list :view (plist-get outcome :result)
        :text (alist-get 'text payload)
        :mode (alist-get 'mode payload)
        :context (alist-get 'context payload)
        :url (plist-get (car (plist-get outcome :requests)) :url)
        :method (plist-get (car (plist-get outcome :requests)) :method)
        :silent (plist-get (car (plist-get outcome :requests)) :silent)))
"####,
        expect![[
            r##"OK (:view (:name "*gh-md*" :mode eww-mode :text "Selected\n\n\nOnly\nthis\nslice." :point 1 :read-only t :file nil :modified t) :text "# Selected\n\nOnly this slice." :mode "markdown" :context nil :url "https://api.github.com/markdown" :method "POST" :silent silent)"##
        ]],
    )
}

fn gfm_context_and_unicode_shape_the_github_api_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "gfm_context_and_unicode_shape_the_github_api_payload",
        r####"
(neomacs-gh-md-test-cleanup)
(let ((source (generate-new-buffer "gfm.md"))
      (gh-md-use-gfm t)
      (gh-md-context "acme/release-notes")
      outcome payload)
  (unwind-protect
      (with-current-buffer source
        (insert "See #42 — 日本語 and café\n")
        (setq outcome
              (neomacs-gh-md-test-with-transport
               "<p>See <a href=\"#\">#42</a> — 日本語 and café</p>"
               (lambda ()
                 (gh-md-convert-region (point-min) (point-max))
                 (neomacs-gh-md-test-view))))
        (setq payload
              (json-read-from-string
               (plist-get (car (plist-get outcome :requests)) :data))))
    (when (buffer-live-p source) (kill-buffer source)))
  (list :view (plist-get outcome :result)
        :payload (list :text (alist-get 'text payload)
                       :mode (alist-get 'mode payload)
                       :context (alist-get 'context payload))
        :raw-bytes
        (string-bytes (plist-get (car (plist-get outcome :requests)) :data))))
"####,
        expect![[
            r#"OK (:view (:name "*gh-md*" :mode eww-mode :text "See\n#42\n—\n日\n本\n語\nand\ncafé" :point 1 :read-only t :file nil :modified t) :payload (:text "See #42 — 日本語 and café\n" :mode "gfm" :context "acme/release-notes") :raw-bytes 88)"#
        ]],
    )
}

fn export_buffer_writes_html_with_css_and_extra_header() -> ParityBatchCase {
    ParityBatchCase::value(
        "export_buffer_writes_html_with_css_and_extra_header",
        r####"
(neomacs-gh-md-test-cleanup)
(let* ((root (file-name-as-directory
              (expand-file-name "gh-md-export"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source-path (expand-file-name "notes.md" root))
       (export-path (expand-file-name "notes.html" root))
       (source nil)
       (gh-md-use-gfm nil)
       (gh-md-context nil)
       (gh-md-css-path "https://docs.example.test/github.css")
       (gh-md-extra-header "<meta name=\"parity\" content=\"gh-md\">")
       outcome exported)
  (when (file-exists-p root) (delete-directory root t))
  (make-directory root t)
  (with-temp-file source-path (insert "# Export Me\n\nBody.\n"))
  (setq source (find-file-noselect source-path))
  (unwind-protect
      (with-current-buffer source
        (setq outcome
              (neomacs-gh-md-test-with-transport
               "<h1>Export Me</h1><p>Body.</p>"
               (lambda ()
                 (cl-letf (((symbol-function 'read-string)
                            (lambda (_prompt &optional initial &rest _)
                              (or initial export-path))))
                   (gh-md-export-buffer)
                   (with-current-buffer (find-buffer-visiting export-path)
                     (neomacs-gh-md-test-view (current-buffer)))))))
        (setq exported
              (with-temp-buffer
                (insert-file-contents export-path)
                (buffer-string))))
    (when (buffer-live-p source) (kill-buffer source))
    (let ((export-buffer (find-buffer-visiting export-path)))
      (when (buffer-live-p export-buffer) (kill-buffer export-buffer))))
  (list :view (plist-get outcome :result)
        :exists (file-exists-p export-path)
        :html exported
        :default-name
        (file-relative-name
         (with-temp-buffer
           (setq buffer-file-name source-path)
           (gh-md--export-file-name))
         (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
        :request-url (plist-get (car (plist-get outcome :requests)) :url)))
"####,
        expect![[
            r#"OK (:view (:name "notes.html" :mode mhtml-mode :text "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1, minimal-ui\">\n<link rel=\"stylesheet\" type=\"text/css\" media=\"all\" href=\"https://docs.example.test/github.css\">\n<meta name=\"parity\" content=\"gh-md\">\n<style>\nbody {\n  min-width: 200px;\n  max-width: 790px;\n  margin: 0 auto;\n  padding: 30px;\n}\n</style>\n</head>\n<body>\n<div class=\"markdown-body\">\n<h1>Export Me</h1><p>Body.</p>\n</div>\n</body>\n</html>" :point 465 :read-only nil :file "gh-md-export/notes.html" :modified nil) :exists t :html "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1, minimal-ui\">\n<link rel=\"stylesheet\" type=\"text/css\" media=\"all\" href=\"https://docs.example.test/github.css\">\n<meta name=\"parity\" content=\"gh-md\">\n<style>\nbody {\n  min-width: 200px;\n  max-width: 790px;\n  margin: 0 auto;\n  padding: 30px;\n}\n</style>\n</head>\n<body>\n<div class=\"markdown-body\">\n<h1>Export Me</h1><p>Body.</p>\n</div>\n</body>\n</html>\n" :default-name "gh-md-export/notes.html" :request-url "https://api.github.com/markdown")"#
        ]],
    )
}

fn export_region_uses_buffer_name_when_file_is_absent() -> ParityBatchCase {
    ParityBatchCase::value(
        "export_region_uses_buffer_name_when_file_is_absent",
        r####"
(neomacs-gh-md-test-cleanup)
(let* ((root (file-name-as-directory
              (expand-file-name "gh-md-export-region"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (export-path (expand-file-name "scratch-export.html" root))
       (source (generate-new-buffer "scratch-export.md"))
       (gh-md-css-path nil)
       (gh-md-extra-header nil)
       suggested outcome exported)
  (when (file-exists-p root) (delete-directory root t))
  (make-directory root t)
  (unwind-protect
      (with-current-buffer source
        (insert "alpha\n# Region Export\nbeta\n")
        (setq suggested (gh-md--export-file-name))
        (goto-char (point-min))
        (search-forward "# Region")
        (beginning-of-line)
        (let ((begin (point)))
          (search-forward "Export")
          (end-of-line)
          (setq outcome
                (neomacs-gh-md-test-with-transport
                 "<h1>Region Export</h1>"
                 (lambda ()
                   (cl-letf (((symbol-function 'read-string)
                              (lambda (prompt &optional initial &rest _)
                                (list :prompt prompt :initial initial)
                                export-path)))
                     (gh-md-export-region begin (point))
                     (neomacs-gh-md-test-view
                      (find-buffer-visiting export-path)))))))
        (setq exported
              (with-temp-buffer
                (insert-file-contents export-path)
                (buffer-string))))
    (when (buffer-live-p source) (kill-buffer source))
    (let ((export-buffer (find-buffer-visiting export-path)))
      (when (buffer-live-p export-buffer) (kill-buffer export-buffer))))
  (list :suggested suggested
        :view (plist-get outcome :result)
        :html exported
        :request-data
        (json-read-from-string
         (plist-get (car (plist-get outcome :requests)) :data))))
"####,
        expect![[
            r##"OK (:suggested "scratch-export.html" :view (:name "scratch-export.html" :mode mhtml-mode :text "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1, minimal-ui\">\n\n\n<style>\nbody {\n  min-width: 200px;\n  max-width: 790px;\n  margin: 0 auto;\n  padding: 30px;\n}\n</style>\n</head>\n<body>\n<div class=\"markdown-body\">\n<h1>Region Export</h1>\n</div>\n</body>\n</html>" :point 326 :read-only nil :file "gh-md-export-region/scratch-export.html" :modified nil) :html "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1, minimal-ui\">\n\n\n<style>\nbody {\n  min-width: 200px;\n  max-width: 790px;\n  margin: 0 auto;\n  padding: 30px;\n}\n</style>\n</head>\n<body>\n<div class=\"markdown-body\">\n<h1>Region Export</h1>\n</div>\n</body>\n</html>\n" :request-data ((text . "# Region Export") (mode . "markdown") (context)))"##
        ]],
    )
}

fn transport_errors_surface_through_message_without_preview_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "transport_errors_surface_through_message_without_preview_buffer",
        r####"
(neomacs-gh-md-test-cleanup)
(let ((source (generate-new-buffer "error.md"))
      messages outcome)
  (unwind-protect
      (with-current-buffer source
        (insert "# Broken\n")
        (cl-letf (((symbol-function 'message)
                   (lambda (format-string &rest args)
                     (let ((text (apply #'format format-string args)))
                       (push text messages)
                       text))))
          (setq outcome
                (neomacs-gh-md-test-with-transport
                 nil
                 (lambda ()
                   (gh-md-render-buffer)
                   (list :preview-live (buffer-live-p (get-buffer gh-md-buffer-name))
                         :preview-empty
                         (when (get-buffer gh-md-buffer-name)
                           (with-current-buffer gh-md-buffer-name
                             (= (point-min) (point-max))))))
                 (list :error '(error http 502))))))
    (when (buffer-live-p source) (kill-buffer source)))
  (list :messages (nreverse messages)
        :result (plist-get outcome :result)
        :requests (plist-get outcome :requests)))
"####,
        expect![[
            r##"OK (:messages ("peculiar error: 502") :result (:preview-live t :preview-empty t) :requests ((:url "https://api.github.com/markdown" :method "POST" :data "{\"text\":\"# Broken\\n\",\"mode\":\"markdown\",\"context\":null}" :silent silent)))"##
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        render_buffer_posts_markdown_and_shows_shr_preview(),
        render_region_sends_only_the_selected_markdown_slice(),
        gfm_context_and_unicode_shape_the_github_api_payload(),
        export_buffer_writes_html_with_css_and_extra_header(),
        export_region_uses_buffer_name_when_file_is_absent(),
        transport_errors_surface_through_message_without_preview_buffer(),
    ]
}

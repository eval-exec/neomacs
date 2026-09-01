use expect_test::expect;

use super::ParityBatchCase;

fn peruse_renders_installed_html_with_viewer_state_and_target_position() -> ParityBatchCase {
    ParityBatchCase::value(
        "peruse_renders_installed_html_with_viewer_state_and_target_position",
        r####"
(let ((doc (neomacs-devdocs-test-install-fixture "peruse"))
      (devdocs-fontify-code-blocks nil))
  (save-window-excursion
    (devdocs-peruse doc)
    (neomacs-devdocs-test-view)))
"####,
        expect![[
            r#"OK (:mode devdocs-mode :text "Widget\n\n\nCreate\na\nwidget.\n\n\nBuild\n\n\nconst x = Widget.build();\n\nOptions" :point (1 1 0) :stack ((nil "guide" nil)) :forward nil :docs ("widgetjs") :header "" :directory "" :read-only t :modified nil :bindings (("n" . devdocs-next-entry) ("p" . devdocs-previous-entry) ("]" . devdocs-next-page) ("[" . devdocs-previous-page) ("l" . devdocs-go-back) ("r" . devdocs-go-forward) ("w" . devdocs-copy-url) ("." . devdocs-goto-target)))"#
        ]],
    )
}

fn entry_and_page_navigation_preserve_history_and_enforce_boundaries() -> ParityBatchCase {
    ParityBatchCase::value(
        "entry_and_page_navigation_preserve_history_and_enforce_boundaries",
        r####"
(let ((doc (neomacs-devdocs-test-install-fixture "navigation"))
      states)
  (devdocs-goto-page doc "guide#widget")
  (with-current-buffer "*devdocs*"
    (push (cons 'first (neomacs-devdocs-test-view)) states)
    (devdocs-next-entry 1)
    (push (cons 'next-entry (neomacs-devdocs-test-view)) states)
    (devdocs-next-page 1)
    (push (cons 'next-page (neomacs-devdocs-test-view)) states)
    (devdocs-go-back)
    (push (cons 'back (neomacs-devdocs-test-view)) states)
    (devdocs-go-forward)
    (push (cons 'forward (neomacs-devdocs-test-view)) states)
    (push (cons 'boundary
                (condition-case err
                    (list :value (devdocs-next-page 1))
                  (error (list :signal (car err)
                               :message (error-message-string err)))))
          states))
  (nreverse states))
"####,
        expect![[
            r#"OK ((first :mode devdocs-mode :text "Widget\n\n\nCreate\na\nwidget.\n\n\nBuild\n\n\nconst x = Widget.build();\n\nOptions" :point (1 1 0) :stack (("Widget" "guide#widget" nil)) :forward nil :docs ("widgetjs") :header "" :directory "" :read-only t :modified nil :bindings (("n" . devdocs-next-entry) ("p" . devdocs-previous-entry) ("]" . devdocs-next-page) ("[" . devdocs-previous-page) ("l" . devdocs-go-back) ("r" . devdocs-go-forward) ("w" . devdocs-copy-url) ("." . devdocs-goto-target))) (next-entry :mode devdocs-mode :text "Widget\n\n\nCreate\na\nwidget.\n\n\nBuild\n\n\nconst x = Widget.build();\n\nOptions" :point (29 9 0) :stack (("Widget.build" "guide#build" nil) ("Widget" "guide#widget" nil)) :forward nil :docs ("widgetjs") :header "" :directory "" :read-only t :modified nil :bindings (("n" . devdocs-next-entry) ("p" . devdocs-previous-entry) ("]" . devdocs-next-page) ("[" . devdocs-previous-page) ("l" . devdocs-go-back) ("r" . devdocs-go-forward) ("w" . devdocs-copy-url) ("." . devdocs-goto-target))) (next-page :mode devdocs-mode :text "Options\n\n\nSet\nfast\nto\ntrue.\n\n\nWidget" :point (1 1 0) :stack ((nil "api" nil) ("Widget.build" "guide#build" nil) ("Widget" "guide#widget" nil)) :forward nil :docs ("widgetjs") :header "" :directory "" :read-only t :modified nil :bindings (("n" . devdocs-next-entry) ("p" . devdocs-previous-entry) ("]" . devdocs-next-page) ("[" . devdocs-previous-page) ("l" . devdocs-go-back) ("r" . devdocs-go-forward) ("w" . devdocs-copy-url) ("." . devdocs-goto-target))) (back :mode devdocs-mode :text "Widget\n\n\nCreate\na\nwidget.\n\n\nBuild\n\n\nconst x = Widget.build();\n\nOptions" :point (29 9 0) :stack (("Widget.build" "guide#build" nil) ("Widget" "guide#widget" nil)) :forward ("api") :docs ("widgetjs") :header "" :directory "" :read-only t :modified nil :bindings (("n" . devdocs-next-entry) ("p" . devdocs-previous-entry) ("]" . devdocs-next-page) ("[" . devdocs-previous-page) ("l" . devdocs-go-back) ("r" . devdocs-go-forward) ("w" . devdocs-copy-url) ("." . devdocs-goto-target))) (forward :mode devdocs-mode :text "Options\n\n\nSet\nfast\nto\ntrue.\n\n\nWidget" :point (1 1 0) :stack ((nil "api" nil) ("Widget.build" "guide#build" nil) ("Widget" "guide#widget" nil)) :forward nil :docs ("widgetjs") :header "" :directory "" :read-only t :modified nil :bindings (("n" . devdocs-next-entry) ("p" . devdocs-previous-entry) ("]" . devdocs-next-page) ("[" . devdocs-previous-page) ("l" . devdocs-go-back) ("r" . devdocs-go-forward) ("w" . devdocs-copy-url) ("." . devdocs-goto-target))) (boundary :signal user-error :message "No next page"))"#
        ]],
    )
}

fn internal_links_fragments_copy_url_and_bookmark_record_round_trip() -> ParityBatchCase {
    ParityBatchCase::value(
        "internal_links_fragments_copy_url_and_bookmark_record_round_trip",
        r####"
(let ((doc (neomacs-devdocs-test-install-fixture "links"))
      (devdocs-site-url "https://docs.example.test")
      (kill-ring nil))
  (devdocs-goto-page doc "guide")
  (with-current-buffer "*devdocs*"
    (devdocs--internal-url-handler "api#options")
    (devdocs-copy-url)
    (list :view (neomacs-devdocs-test-view)
          :url (car kill-ring)
          :bookmark (devdocs--make-bookmark)
          :expanded (mapcar (lambda (path)
                              (devdocs--path-expand path "guide#widget"))
                            '("#build" "api#options" "../root")))))
"####,
        expect![[
            r#"OK (:view (:mode devdocs-mode :text "Options\n\n\nSet\nfast\nto\ntrue.\n\n\nWidget" :point (1 1 0) :stack (("Options" "api#options" "options") (nil "guide" nil)) :forward nil :docs ("widgetjs") :header "" :directory "" :read-only t :modified nil :bindings (("n" . devdocs-next-entry) ("p" . devdocs-previous-entry) ("]" . devdocs-next-page) ("[" . devdocs-previous-page) ("l" . devdocs-go-back) ("r" . devdocs-go-forward) ("w" . devdocs-copy-url) ("." . devdocs-goto-target))) :url "https://docs.example.test/widgetjs/api#options" :bookmark ((defaults "widgetjs/api#options") (devdocs-doc (name . "Widget JS") (slug . "widgetjs") (type . "javascript") (version . "2.0") (mtime . 200)) (devdocs-path . "api#options") (filename . "") (buf . "*devdocs*") (handler . devdocs--bookmark-handler)) :expanded ("guide#build" "api#options" "root"))"#
        ]],
    )
}

fn lookup_disambiguates_duplicate_names_and_remembers_selected_documents() -> ParityBatchCase {
    ParityBatchCase::value(
        "lookup_disambiguates_duplicate_names_and_remembers_selected_documents",
        r####"
(neomacs-devdocs-test-reset "lookup")
(let* ((first (neomacs-devdocs-test-install-fixture "lookup"))
       (second (neomacs-devdocs-test-doc "widgetjs-next" "Widget JS" "3.0" "javascript" 300))
       (dir (expand-file-name "widgetjs-next" devdocs-data-dir))
       candidates metadata selected)
  (make-directory dir t)
  (with-temp-file (expand-file-name "metadata" dir)
    (prin1 (cons devdocs--data-format-version second) (current-buffer)))
  (with-temp-file (expand-file-name "index" dir)
    (prin1 '((entries . [((name . "Widget") (path . "new#widget") (type . "Classes"))])
             (pages . ["new"]) (types . ["Classes"]))
           (current-buffer)))
  (with-temp-file (expand-file-name "new.html" dir)
    (insert "<main><h1 id='widget'>Widget 3</h1><p>Next generation.</p></main>"))
  (with-temp-buffer
    (let ((devdocs-current-docs nil))
      (cl-letf (((symbol-function 'completing-read-multiple)
                 (lambda (_prompt collection &rest _)
                   (setq metadata collection)
                   '("widgetjs" "widgetjs-next")))
                ((symbol-function 'completing-read)
                 (lambda (_prompt collection &rest _)
                   (setq candidates (all-completions "" collection)
                         selected (car (last candidates)))
                   selected))
                ((symbol-function 'display-buffer)
                 (lambda (buffer &rest _) (get-buffer-window buffer t))))
        (devdocs-lookup t "Wid")
        (list :docs devdocs-current-docs
              :candidates (mapcar #'substring-no-properties candidates)
              :selected (substring-no-properties selected)
              :selected-data
              (let ((data (get-text-property 0 'devdocs--data selected)))
                (list (alist-get 'path data)
                      (alist-get 'slug (alist-get 'doc data))))
              :document-choices (mapcar #'car metadata)
              :view (neomacs-devdocs-test-view))))))
"####,
        expect![[
            r#"OK (:docs ("widgetjs" "widgetjs-next") :candidates ("Widget (1)" "Widget.build" "Options" "Widget (2)") :selected "Widget (2)" :selected-data ("new#widget" "widgetjs-next") :document-choices ("widgetjs" "widgetjs-next") :view (:mode devdocs-mode :text "Widget\n3\n\nNext\ngeneration." :point (1 1 0) :stack (("Widget" "new#widget" nil)) :forward nil :docs ("widgetjs-next") :header "" :directory "" :read-only t :modified nil :bindings (("n" . devdocs-next-entry) ("p" . devdocs-previous-entry) ("]" . devdocs-next-page) ("[" . devdocs-previous-page) ("l" . devdocs-go-back) ("r" . devdocs-go-forward) ("w" . devdocs-copy-url) ("." . devdocs-goto-target))))"#
        ]],
    )
}

fn install_and_delete_use_public_commands_with_only_network_transport_replaced() -> ParityBatchCase
{
    ParityBatchCase::value(
        "install_and_delete_use_public_commands_with_only_network_transport_replaced",
        r####"
(neomacs-devdocs-test-reset "install")
(let ((doc (neomacs-devdocs-test-doc "sample" "Sample" "1.0" "sample" 42))
      requests installed)
  (cl-letf (((symbol-function 'url-insert-file-contents)
             (lambda (url &rest _)
               (let ((database-response (null requests)))
                 (push url requests)
                 (insert
                  (if database-response
                      "{\"guide\":\"<main><h1 id='start'>Start</h1><p>Hello.</p></main>\",\"api\":\"<main><h1>API</h1></main>\"}"
                    "{\"entries\":[{\"name\":\"Start\",\"path\":\"guide#start\",\"type\":\"Guide\"}],\"types\":[\"Guide\"]}"))
                 (goto-char (point-min))))))
    (devdocs-install doc))
  (setq installed
        (list :metadata (devdocs--doc-metadata "sample")
              :pages (append (devdocs--index doc 'pages) nil)
              :entries (mapcar (lambda (entry)
                                 (list (alist-get 'name entry)
                                       (alist-get 'path entry)))
                               (devdocs--index doc 'entries))
              :files (neomacs-devdocs-test-files)
              :requests (nreverse requests)))
  (devdocs-delete doc)
  (list :installed installed
        :exists (file-exists-p (expand-file-name "sample" devdocs-data-dir))
        :remaining (neomacs-devdocs-test-files)))
"####,
        expect![[
            r#"OK (:installed (:metadata ((name . "Sample") (slug . "sample") (type . "sample") (version . "1.0") (mtime . 42)) :pages ("guide" "api") :entries (("Start" "guide#start")) :files ("sample/api.html" "sample/guide.html" "sample/index" "sample/metadata") :requests ("https://documents.devdocs.io/sample/db.json?42" "https://documents.devdocs.io/sample/index.json?42")) :exists nil :remaining nil)"#
        ]],
    )
}

fn metadata_version_and_missing_documents_report_actionable_user_errors() -> ParityBatchCase {
    ParityBatchCase::value(
        "metadata_version_and_missing_documents_report_actionable_user_errors",
        r####"
(neomacs-devdocs-test-reset "errors")
(let ((old (expand-file-name "old" devdocs-data-dir)))
  (make-directory old t)
  (with-temp-file (expand-file-name "metadata" old)
    (prin1 '(0 (name . "Old") (slug . "old")) (current-buffer)))
  (mapcar
   (lambda (probe)
     (condition-case err
         (list :value (funcall probe))
       (error (list :signal (car err)
                    :message (error-message-string err)))))
   (list (lambda () (devdocs--doc-metadata "missing"))
         (lambda () (devdocs--doc-metadata "old"))
         (lambda () (devdocs--read-document "Pick: "))
         (lambda () (devdocs-delete (neomacs-devdocs-test-doc "missing"))))))
"####,
        expect![[
            r#"OK ((:signal user-error :message "Document ‘missing’ is not installed") (:signal user-error :message "Please run ‘devdocs-update-all’") (:signal user-error :message "Please run ‘devdocs-update-all’") (:signal user-error :message "Document ‘missing’ is not installed"))"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        peruse_renders_installed_html_with_viewer_state_and_target_position(),
        entry_and_page_navigation_preserve_history_and_enforce_boundaries(),
        internal_links_fragments_copy_url_and_bookmark_record_round_trip(),
        lookup_disambiguates_duplicate_names_and_remembers_selected_documents(),
        install_and_delete_use_public_commands_with_only_network_transport_replaced(),
        metadata_version_and_missing_documents_report_actionable_user_errors(),
    ]
}

use expect_test::expect;

use super::ParityBatchCase;

fn completion_uses_real_capf_http_and_full_file_synchronization() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-tern-test-with-project
    "completion"
    '(("src/main.js" . "const deployment = {};\ndeployment.dep"))
    '((sync . ((ok . t)))
      (completions .
       (lambda (document)
         (let ((end (cdr (assq 'end (cdr (assq 'query document))))))
           `((start . ,(- end 3))
             (end . ,end)
             (completions . ["deployRelease"]))))))
  (goto-char (point-max))
  (goto-char (point-min))
  (insert "// unsaved deployment edit\n")
  (goto-char (point-max))
  (let (first-handoff)
    ;; Tern's HTTP callback calls `completion-in-region' asynchronously.
    ;; A batch editor has no command loop in which that first UI can read a
    ;; choice, so observe the exact GNU handoff without replacing any Tern
    ;; function.  The second public completion key uses Tern's real cache and
    ;; GNU's real completion engine to make the user-visible insertion.
    (let ((completion-in-region-function
           (lambda (start end collection &optional predicate)
             (setq first-handoff
                   (list :start start :end end
                         :collection (copy-tree collection)
                         :predicate predicate)))))
      (execute-kbd-macro (kbd "M-TAB"))
      (neomacs-tern-test-wait-for
       (lambda () first-handoff)
       "completion handoff"))
    (execute-kbd-macro (kbd "M-TAB"))
    (list
     :buffer (neomacs-tern-test-buffer-state)
     :first-handoff first-handoff
     :completion-cache (copy-tree tern-last-completions)
     :dirty-after-response tern-buffer-is-dirty
     :mode tern-mode
     :capf (memq #'tern-completion-at-point completion-at-point-functions)
     :requests neomacs-tern-test-requests)))
"####;
    let expect = expect![[
        r####"OK (:buffer (:file "main.js" :text "// unsaved deployment edit\nconst deployment = {};\ndeployment.deployRelease" :point 75 :line 3 :column 24 :modified t) :first-handoff (:start 62 :end 65 :collection ("deployRelease") :predicate nil) :completion-cache ("dep" 62 65 ("deployRelease")) :dirty-after-response (62 . 75) :mode t :capf (tern-completion-at-point tags-completion-at-point-function) :requests ((:request-line "POST / HTTP/1.1" :type sync :file nil :end nil :new-name nil :variable nil :include-keywords nil :prefer-function nil :files ((:type "full" :name "src/main.js" :offset nil :text "const deployment = {};\ndeployment.dep"))) (:request-line "POST / HTTP/1.1" :type completions :file "src/main.js" :end 64 :new-name nil :variable nil :include-keywords t :prefer-function nil :files ((:type "full" :name "src/main.js" :offset nil :text "// unsaved deployment edit\nconst deployment = {};\ndeployment.dep")))))"####
    ]];
    ParityBatchCase::value(
        "completion_uses_real_capf_http_and_full_file_synchronization",
        elisp_form,
        expect,
    )
}

fn type_and_argument_hints_follow_public_editor_routes() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-tern-test-with-project
    "type-and-arguments"
    '(("src/main.js" . "deployRelease(\"api\", 3, true);"))
    '((sync . ((ok . t)))
      (type . ((type . "fn(service: string, retries: number, notify: bool) -> Promise<boolean>")
               (name . "deployRelease")
               (exprName . "deployRelease"))))
  (goto-char (point-min))
  (search-forward "deployRelease")
  (neomacs-tern-test-with-message-observer
    (execute-kbd-macro (kbd "C-c C-c"))
    (neomacs-tern-test-wait-for
     (lambda () (= (length neomacs-tern-test-messages) 1))
     "type message")
    (let ((tern-update-argument-hints-timer 1))
      (goto-char (point-min))
      (search-forward "\"api")
      (execute-kbd-macro (kbd "C-f")))
    (neomacs-tern-test-wait-for
     (lambda () (= (length neomacs-tern-test-messages) 2))
    "argument hints")
    (list :messages neomacs-tern-test-messages
          :requests neomacs-tern-test-requests)))
"####;
    let expect = expect![[
        r####"OK (:messages ("fn(service: string, retries: number, notify: bool) -> Promise<boolean>" #("deployRelease(service: string, retries: number, notify: bool) -> Promise<boolean>" 0 13 (face font-lock-function-name-face) 14 21 (face highlight) 23 29 (face font-lock-type-face) 40 46 (face font-lock-type-face) 56 60 (face font-lock-type-face) 65 81 (face font-lock-type-face))) :requests ((:request-line "POST / HTTP/1.1" :type sync :file nil :end nil :new-name nil :variable nil :include-keywords nil :prefer-function nil :files ((:type "full" :name "src/main.js" :offset nil :text "deployRelease(\"api\", 3, true);"))) (:request-line "POST / HTTP/1.1" :type type :file "src/main.js" :end 13 :new-name nil :variable nil :include-keywords nil :prefer-function nil :files nil) (:request-line "POST / HTTP/1.1" :type type :file "src/main.js" :end 13 :new-name nil :variable nil :include-keywords nil :prefer-function t :files nil)))"####
    ]];
    ParityBatchCase::value(
        "type_and_argument_hints_follow_public_editor_routes",
        elisp_form,
        expect,
    )
}

fn repeated_documentation_key_opens_url_without_a_second_analyzer_request() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-tern-test-with-project
    "documentation"
    '(("src/main.js" . "deployRelease(\"api\");"))
    '((sync . ((ok . t)))
      (documentation .
       ((doc . "Deploy SERVICE with bounded retries and notifications.")
        (url . "https://docs.example.test/deployRelease"))))
  (goto-char (point-min))
  (search-forward "deployRelease")
  (let (opened-urls)
    (neomacs-tern-test-with-message-observer
      ;; Run both documented docs keypresses in one command loop.  The first
      ;; post-command phase waits for Tern's asynchronous response without
      ;; changing command identity, so the second key sees the real
      ;; `last-command' value and follows the URL branch.
      (let ((browse-url-browser-function
             (lambda (url &optional new-window)
               (setq opened-urls
                     (append opened-urls (list (list url new-window))))))
            (docs-command-count 0))
        (let ((post-command-hook
               (cons
                (lambda ()
                  (when (eq this-command 'tern-get-docs)
                    (setq docs-command-count (1+ docs-command-count))
                    (when (= docs-command-count 1)
                      (neomacs-tern-test-wait-for
                       (lambda ()
                         (and (= (length neomacs-tern-test-messages) 1)
                              (= (length neomacs-tern-test-requests) 2)
                              (equal
                               tern-last-docs-url
                               "https://docs.example.test/deployRelease")))
                       "first docs key's response and URL state"))))
                post-command-hook)))
          (execute-kbd-macro (kbd "C-c C-d C-c C-d"))))
      (neomacs-tern-test-wait-for
       (lambda () (neomacs-tern-test-http-idle-p buffers-before))
       "all documentation HTTP callbacks")
      (list :messages neomacs-tern-test-messages
            :opened opened-urls
            :docs-url-after-second-press tern-last-docs-url
            :requests neomacs-tern-test-requests))))
"####;
    let expect = expect![[
        r####"OK (:messages ("Deploy SERVICE with bounded retries and notifications.") :opened (("https://docs.example.test/deployRelease" nil)) :docs-url-after-second-press nil :requests ((:request-line "POST / HTTP/1.1" :type sync :file nil :end nil :new-name nil :variable nil :include-keywords nil :prefer-function nil :files ((:type "full" :name "src/main.js" :offset nil :text "deployRelease(\"api\");"))) (:request-line "POST / HTTP/1.1" :type documentation :file "src/main.js" :end 13 :new-name nil :variable nil :include-keywords nil :prefer-function nil :files nil)))"####
    ]];
    ParityBatchCase::value(
        "repeated_documentation_key_opens_url_without_a_second_analyzer_request",
        elisp_form,
        expect,
    )
}

fn definition_navigation_opens_the_real_file_and_pop_returns_to_origin() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-tern-test-with-project
    "definition"
    '(("src/main.js" . "const result = deployRelease(\"api\");\n")
      ("src/release.js" . "export function deployRelease(service) {\n  return service + \"-ready\";\n}\n"))
    '((sync . ((ok . t)))
      (definition . ((file . "src/release.js")
                     (start . 16) (end . 29)
                     (contextOffset . 16)
                     (context . "export function deployRelease(service) {"))))
  (goto-char (point-min))
  (search-forward "deployRelease")
  (let ((origin (neomacs-tern-test-buffer-state))
        prompted
        (definition-binding (key-binding (kbd "M-."))))
    (execute-kbd-macro (kbd "M-."))
    (neomacs-tern-test-wait-for
     (lambda ()
       (let ((buffer (find-buffer-visiting
                      (expand-file-name "src/release.js" root))))
         (and buffer (= (length tern-find-definition-stack) 1)
              (with-current-buffer buffer (= (point) 17)))))
     "definition file, target, and return stack")
    (let ((definition
           (with-current-buffer
               (find-buffer-visiting (expand-file-name "src/release.js" root))
             (neomacs-tern-test-buffer-state)))
          (stack-depth (length tern-find-definition-stack)))
      (when tern-find-definition-stack
        (call-interactively #'tern-pop-find-definition))
      (cl-letf (((symbol-function 'read-from-minibuffer)
                 (lambda (prompt &rest _arguments)
                   (setq prompted prompt)
                   "deployRelease")))
        (call-interactively #'tern-find-definition-by-name))
      (neomacs-tern-test-wait-for
       (lambda ()
         (and (= (length neomacs-tern-test-requests) 4)
              (= (length tern-find-definition-stack) 1)
              (let ((buffer
                     (find-buffer-visiting
                      (expand-file-name "src/release.js" root))))
                (and buffer
                     (eq (window-buffer (selected-window)) buffer)
                     (with-current-buffer buffer (= (point) 17))))))
       "definition-by-name target and return stack")
      (when tern-find-definition-stack
        (call-interactively #'tern-pop-find-definition))
      (list :binding definition-binding
            :prompt prompted
            :origin origin
            :definition definition
            :stack-depth stack-depth
            :returned (neomacs-tern-test-buffer-state)
            :stack-after-return (length tern-find-definition-stack)
            :requests neomacs-tern-test-requests))))
"####;
    let expect = expect![[
        r####"OK (:binding tern-find-definition :prompt "Variable: " :origin (:file "main.js" :text "const result = deployRelease(\"api\");\n" :point 29 :line 1 :column 28 :modified nil) :definition (:file "release.js" :text "export function deployRelease(service) {\n  return service + \"-ready\";\n}\n" :point 17 :line 1 :column 16 :modified nil) :stack-depth 1 :returned (:file "main.js" :text "const result = deployRelease(\"api\");\n" :point 29 :line 1 :column 28 :modified nil) :stack-after-return 0 :requests ((:request-line "POST / HTTP/1.1" :type sync :file nil :end nil :new-name nil :variable nil :include-keywords nil :prefer-function nil :files ((:type "full" :name "src/main.js" :offset nil :text "const result = deployRelease(\"api\");\n"))) (:request-line "POST / HTTP/1.1" :type definition :file "src/main.js" :end 28 :new-name nil :variable nil :include-keywords nil :prefer-function nil :files nil) (:request-line "POST / HTTP/1.1" :type sync :file nil :end nil :new-name nil :variable nil :include-keywords nil :prefer-function nil :files ((:type "full" :name "src/release.js" :offset nil :text "export function deployRelease(service) {\n  return service + \"-ready\";\n}\n"))) (:request-line "POST / HTTP/1.1" :type definition :file "src/main.js" :end 28 :new-name nil :variable "deployRelease" :include-keywords nil :prefer-function nil :files nil)))"####
    ]];
    ParityBatchCase::value(
        "definition_navigation_opens_the_real_file_and_pop_returns_to_origin",
        elisp_form,
        expect,
    )
}

fn rename_edits_multiple_real_project_files_and_preserves_navigation_context() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-tern-test-with-project
    "rename"
    '(("src/main.js" . "const first = deployRelease(\"api\");\nconst second = deployRelease(\"web\");\n")
      ("src/release.js" . "export function deployRelease(service) { return service; }\nexport const alias = deployRelease;\n"))
    '((sync . ((ok . t)))
      (rename . ((changes . [((file . "src/main.js")
                              (start . 51) (end . 64)
                              (text . "scheduleDeployment"))
                             ((file . "src/release.js")
                              (start . 80) (end . 93)
                              (text . "scheduleDeployment"))
                             ((file . "src/main.js")
                              (start . 14) (end . 27)
                              (text . "scheduleDeployment"))
                             ((file . "src/release.js")
                              (start . 16) (end . 29)
                              (text . "scheduleDeployment"))]))))
  (goto-char (point-min))
  (search-forward "deployRelease")
  (tern-rename-variable "scheduleDeployment")
  (neomacs-tern-test-wait-for
   (lambda ()
     (let ((buffer (find-buffer-visiting
                    (expand-file-name "src/release.js" root))))
       (and buffer
            (with-current-buffer buffer
              (string-match-p "scheduleDeployment" (buffer-string))))))
   "cross-file rename edits")
  (let ((main-buffer (find-buffer-visiting
                      (expand-file-name "src/main.js" root)))
        (release-buffer (find-buffer-visiting
                         (expand-file-name "src/release.js" root))))
    (list :selected (file-name-nondirectory (buffer-file-name))
          :main (with-current-buffer main-buffer
                  (neomacs-tern-test-buffer-state))
          :release (with-current-buffer release-buffer
                     (neomacs-tern-test-buffer-state))
          :main-dirty (with-current-buffer main-buffer tern-buffer-is-dirty)
          :release-dirty (with-current-buffer release-buffer tern-buffer-is-dirty)
          :requests neomacs-tern-test-requests)))
"####;
    let expect = expect![[
        r####"OK (:selected "main.js" :main (:file "main.js" :text "const first = scheduleDeployment(\"api\");\nconst second = scheduleDeployment(\"web\");\n" :point 15 :line 1 :column 14 :modified t) :release (:file "release.js" :text "export function scheduleDeployment(service) { return service; }\nexport const alias = scheduleDeployment;\n" :point 1 :line 1 :column 0 :modified t) :main-dirty (15 . 70) :release-dirty (17 . 99) :requests ((:request-line "POST / HTTP/1.1" :type sync :file nil :end nil :new-name nil :variable nil :include-keywords nil :prefer-function nil :files ((:type "full" :name "src/main.js" :offset nil :text "const first = deployRelease(\"api\");\nconst second = deployRelease(\"web\");\n"))) (:request-line "POST / HTTP/1.1" :type rename :file "src/main.js" :end 27 :new-name "scheduleDeployment" :variable nil :include-keywords nil :prefer-function nil :files nil)))"####
    ]];
    ParityBatchCase::value(
        "rename_edits_multiple_real_project_files_and_preserves_navigation_context",
        elisp_form,
        expect,
    )
}

fn full_file_rename_synchronizes_all_modified_project_buffers() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-tern-test-with-project
    "modified-siblings"
    '(("src/main.js" . "const result = deployRelease(\"api\");\n")
      ("src/release.js" . "export function deployRelease(service) { return service; }\n"))
    '((sync . ((ok . t)))
      (rename . ((changes . []))))
  (let* ((main-buffer (current-buffer))
         (release-buffer
          (find-file-noselect (expand-file-name "src/release.js" root))))
    (neomacs-tern-test-wait-for
     (lambda () (= (length neomacs-tern-test-requests) 2))
     "sibling buffer's initial full-file synchronization")
    (neomacs-tern-test-wait-for
     (lambda () (neomacs-tern-test-http-idle-p buffers-before))
     "sibling buffer's initial HTTP callback")
    (with-current-buffer release-buffer
      (goto-char (point-max))
      (insert "// unsaved release note\n"))
    (with-current-buffer main-buffer
      (goto-char (point-max))
      (insert "// unsaved main note\n")
      (goto-char (point-min))
      (search-forward "deployRelease")
      (tern-rename-variable "scheduleDeployment"))
    (neomacs-tern-test-wait-for
     (lambda () (= (length neomacs-tern-test-requests) 3))
     "full-file rename response")
    (neomacs-tern-test-wait-for
     (lambda () (neomacs-tern-test-http-idle-p buffers-before))
     "full-file rename HTTP callback")
    (list
     :selected (file-name-nondirectory (buffer-file-name))
     :main (with-current-buffer main-buffer
             (neomacs-tern-test-buffer-state))
     :release (with-current-buffer release-buffer
                (neomacs-tern-test-buffer-state))
     :main-dirty (with-current-buffer main-buffer tern-buffer-is-dirty)
     :release-dirty (with-current-buffer release-buffer tern-buffer-is-dirty)
     :requests neomacs-tern-test-requests)))
"####;
    let expect = expect![[
        r####"OK (:selected "main.js" :main (:file "main.js" :text "const result = deployRelease(\"api\");\n// unsaved main note\n" :point 29 :line 1 :column 28 :modified t) :release (:file "release.js" :text "export function deployRelease(service) { return service; }\n// unsaved release note\n" :point 84 :line 3 :column 0 :modified t) :main-dirty nil :release-dirty nil :requests ((:request-line "POST / HTTP/1.1" :type sync :file nil :end nil :new-name nil :variable nil :include-keywords nil :prefer-function nil :files ((:type "full" :name "src/main.js" :offset nil :text "const result = deployRelease(\"api\");\n"))) (:request-line "POST / HTTP/1.1" :type sync :file nil :end nil :new-name nil :variable nil :include-keywords nil :prefer-function nil :files ((:type "full" :name "src/release.js" :offset nil :text "export function deployRelease(service) { return service; }\n"))) (:request-line "POST / HTTP/1.1" :type rename :file "src/main.js" :end 28 :new-name "scheduleDeployment" :variable nil :include-keywords nil :prefer-function nil :files ((:type "full" :name "src/main.js" :offset nil :text "const result = deployRelease(\"api\");\n// unsaved main note\n") (:type "full" :name "src/release.js" :offset nil :text "export function deployRelease(service) { return service; }\n// unsaved release note\n")))))"####
    ]];
    ParityBatchCase::value(
        "full_file_rename_synchronizes_all_modified_project_buffers",
        elisp_form,
        expect,
    )
}

fn reference_highlighting_marks_only_current_file_occurrences() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-tern-test-with-project
    "references"
    '(("src/main.js" . "const deployRelease = () => true;\ndeployRelease();\n")
      ("src/other.js" . "deployRelease();\n"))
    '((sync . ((ok . t)))
      (refs . ((refs . [((file . "src/main.js") (start . 6) (end . 19))
                        ((file . "src/main.js") (start . 34) (end . 47))
                        ((file . "src/other.js") (start . 0) (end . 13))]))))
  (goto-char (point-min))
  (search-forward "deployRelease")
  (let ((tern-flash-timeout 0.05))
    (call-interactively #'tern-highlight-refs)
    (neomacs-tern-test-wait-for
     (lambda () (= (length (overlays-in (point-min) (point-max))) 2))
     "reference highlights")
    (let ((highlighted
           (mapcar
            (lambda (overlay)
              (list :start (overlay-start overlay)
                    :end (overlay-end overlay)
                    :text (buffer-substring-no-properties
                           (overlay-start overlay) (overlay-end overlay))
                    :face (overlay-get overlay 'face)))
            (sort (overlays-in (point-min) (point-max))
                  (lambda (left right)
                    (< (overlay-start left) (overlay-start right)))))))
      (neomacs-tern-test-wait-for
       (lambda () (null (overlays-in (point-min) (point-max))))
       "automatic reference highlight expiry")
      (let ((before-disable
             (list
              :capf (and (memq #'tern-completion-at-point
                               completion-at-point-functions) t)
              :after-change (and (memq #'tern-after-change
                                       after-change-functions) t)
              :post-command (and (memq #'tern-post-command post-command-hook) t)
              :buffer-list-update
              (and (memq #'tern-left-buffer buffer-list-update-hook) t)
              :idle-timer (timerp tern-idle-timer))))
        (tern-mode -1)
        (list :highlighted highlighted
              :remaining (length (overlays-in (point-min) (point-max)))
              :before-disable before-disable
              :mode tern-mode
              :capf (memq #'tern-completion-at-point completion-at-point-functions)
              :after-change (memq #'tern-after-change after-change-functions)
              :post-command (memq #'tern-post-command post-command-hook)
              :buffer-list-update
              (memq #'tern-left-buffer buffer-list-update-hook)
              :idle-timer tern-idle-timer
              :requests neomacs-tern-test-requests)))))
"####;
    let expect = expect![[
        r####"OK (:highlighted ((:start 7 :end 20 :text "deployRelease" :face highlight) (:start 35 :end 48 :text "deployRelease" :face highlight)) :remaining 0 :before-disable (:capf t :after-change t :post-command t :buffer-list-update t :idle-timer t) :mode nil :capf nil :after-change nil :post-command nil :buffer-list-update nil :idle-timer nil :requests ((:request-line "POST / HTTP/1.1" :type sync :file nil :end nil :new-name nil :variable nil :include-keywords nil :prefer-function nil :files ((:type "full" :name "src/main.js" :offset nil :text "const deployRelease = () => true;\ndeployRelease();\n"))) (:request-line "POST / HTTP/1.1" :type refs :file "src/main.js" :end 19 :new-name nil :variable nil :include-keywords nil :prefer-function nil :files nil)))"####
    ]];
    ParityBatchCase::value(
        "reference_highlighting_marks_only_current_file_occurrences",
        elisp_form,
        expect,
    )
}

fn large_dirty_file_sends_a_bounded_partial_document_with_adjusted_query_position()
-> ParityBatchCase {
    let elisp_form = r####"
(let ((source
       (concat
        (mapconcat
         #'identity
         (cl-loop for index below 500
                  collect (format "const padding%03d = %03d;" index index))
         "\n")
        "\nfunction deployRelease(service) { return service; }\n"
        "deployRelease(\"api\");\n")))
  (neomacs-tern-test-with-project
      "large-partial"
      (list (cons "src/main.js" source))
      '((sync . ((ok . t)))
        (type . ((type . "fn(service: string) -> string"))))
    (goto-char (point-max))
    (insert "// unsaved tail\n")
    (search-backward "deployRelease")
    (neomacs-tern-test-with-message-observer
      (execute-kbd-macro (kbd "C-c C-c"))
      (neomacs-tern-test-wait-for
       (lambda () (= (length neomacs-tern-test-messages) 1))
       "large-file type response")
      (list :messages neomacs-tern-test-messages
            :dirty-after-response tern-buffer-is-dirty
            :requests neomacs-tern-test-requests))))
"####;
    let expect = expect![[
        r####"OK (:messages ("fn(service: string) -> string") :dirty-after-response (12075 . 12091) :requests ((:request-line "POST / HTTP/1.1" :type sync :file nil :end nil :new-name nil :variable nil :include-keywords nil :prefer-function nil :files ((:type "full" :name "src/main.js" :offset nil :text (:length 12074 :sha256 "73348da799f6f7eb088e7fcb8d1f9a026999bf2476e0712e32aef84c1e865147" :prefix "const padding000 = 000;\nconst padding001" :suffix "yRelease(service) { return service; }\ndeployRelease(\"api\");\n")))) (:request-line "POST / HTTP/1.1" :type type :file "#0" :end 2020 :new-name nil :variable nil :include-keywords nil :prefer-function nil :files ((:type "part" :name "src/main.js" :offset 10032 :text (:length 2058 :sha256 "e45079cf3983dd00b78ce9751266dd03f6c1c60412c676f096da58fd8960900e" :prefix "const padding418 = 418;\nconst padding419" :suffix ") { return service; }\ndeployRelease(\"api\");\n// unsaved tail\n"))))))"####
    ]];
    ParityBatchCase::value(
        "large_dirty_file_sends_a_bounded_partial_document_with_adjusted_query_position",
        elisp_form,
        expect,
    )
}

fn server_failure_surfaces_the_package_error_without_editing_source() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-tern-test-with-project
    "failure"
    '(("src/main.js" . "deployRelease(\"api\");\n"))
    '((sync . ((ok . t)))
      (type . (raw "500 Internal Server Error" "analyzer unavailable")))
  (goto-char (point-min))
  (search-forward "deployRelease")
  (set-buffer-modified-p nil)
  (neomacs-tern-test-with-message-observer
    (execute-kbd-macro (kbd "C-c C-c"))
    (neomacs-tern-test-wait-for
     (lambda () (= (length neomacs-tern-test-messages) 1))
     "package failure message")
    (list :messages neomacs-tern-test-messages
          :buffer (neomacs-tern-test-buffer-state)
          :requests neomacs-tern-test-requests)))
"####;
    let expect = expect![[
        r####"OK (:messages ("Request failed: ((error http 500) . analyzer unavailable)") :buffer (:file "main.js" :text "deployRelease(\"api\");\n" :point 14 :line 1 :column 13 :modified nil) :requests ((:request-line "POST / HTTP/1.1" :type sync :file nil :end nil :new-name nil :variable nil :include-keywords nil :prefer-function nil :files ((:type "full" :name "src/main.js" :offset nil :text "deployRelease(\"api\");\n"))) (:request-line "POST / HTTP/1.1" :type type :file "src/main.js" :end 13 :new-name nil :variable nil :include-keywords nil :prefer-function nil :files nil)))"####
    ]];
    ParityBatchCase::value(
        "server_failure_surfaces_the_package_error_without_editing_source",
        elisp_form,
        expect,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        completion_uses_real_capf_http_and_full_file_synchronization(),
        type_and_argument_hints_follow_public_editor_routes(),
        repeated_documentation_key_opens_url_without_a_second_analyzer_request(),
        definition_navigation_opens_the_real_file_and_pop_returns_to_origin(),
        rename_edits_multiple_real_project_files_and_preserves_navigation_context(),
        full_file_rename_synchronizes_all_modified_project_buffers(),
        reference_highlighting_marks_only_current_file_occurrences(),
        large_dirty_file_sends_a_bounded_partial_document_with_adjusted_query_position(),
        server_failure_surfaces_the_package_error_without_editing_source(),
    ]
}

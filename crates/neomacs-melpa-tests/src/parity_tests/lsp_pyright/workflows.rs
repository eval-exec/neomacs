use expect_test::expect;

use super::ParityBatchCase;

fn configured_project_negotiates_edits_and_workspace_roots_over_real_stdio() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lsp-pyright-test-with-project "workspace-lifecycle" "multi-root"
  (neomacs-lsp-pyright-test-wait
   (lambda ()
     (and workspace
          (eq (lsp--workspace-status workspace) 'initialized)
          lsp-mode
          (neomacs-lsp-pyright-test-response
           wire-log "client->server" 701)
          (neomacs-lsp-pyright-test-messages-by-method
           wire-log "server->client" "$/progress")))
   "Pyright initialization, configuration, and progress")
  (goto-char (point-max))
  (insert "\nrelease = release_label(\"α-42\")\n")
  (neomacs-lsp-pyright-test-wait
   (lambda ()
     (neomacs-lsp-pyright-test-messages-by-method
      wire-log "client->server" "textDocument/didChange"))
   "the edited Python document to synchronize")
  (lsp-workspace-folders-add second-root)
  (setq second-source-buffer (find-file-noselect second-source-file))
  (with-current-buffer second-source-buffer
    (python-mode)
    (lsp)
    (setq second-workspace
          (lsp-find-workspace 'pyright second-source-file)))
  (neomacs-lsp-pyright-test-wait
   (lambda ()
     (and second-workspace
          (neomacs-lsp-pyright-test-messages-by-method
           wire-log "client->server" "workspace/didChangeWorkspaceFolders")
          (= (length (neomacs-lsp-pyright-test-messages-by-method
                      wire-log "client->server" "textDocument/didOpen"))
             2)))
   "the second Python project to reuse the Pyright workspace")
  (let* ((start (car (neomacs-lsp-pyright-test-read-json-lines start-log)))
         (initialize
          (car (neomacs-lsp-pyright-test-messages-by-method
                wire-log "client->server" "initialize")))
         (initialize-params
          (neomacs-lsp-pyright-test-json initialize "params"))
         (capabilities
          (neomacs-lsp-pyright-test-json initialize-params "capabilities"))
         (workspace-capabilities
          (neomacs-lsp-pyright-test-json capabilities "workspace"))
         (window-capabilities
          (neomacs-lsp-pyright-test-json capabilities "window"))
         (did-opens
          (neomacs-lsp-pyright-test-messages-by-method
           wire-log "client->server" "textDocument/didOpen"))
         (did-open (car did-opens))
         (second-did-open (cadr did-opens))
         (did-open-document
          (neomacs-lsp-pyright-test-json
           (neomacs-lsp-pyright-test-json did-open "params")
           "textDocument"))
         (did-change
          (car (neomacs-lsp-pyright-test-messages-by-method
                wire-log "client->server" "textDocument/didChange")))
         (change-params (neomacs-lsp-pyright-test-json did-change "params"))
         (folder-notifications
          (neomacs-lsp-pyright-test-messages-by-method
           wire-log "client->server" "workspace/didChangeWorkspaceFolders"))
         (folder-event
          (neomacs-lsp-pyright-test-json
           (neomacs-lsp-pyright-test-json
            (car folder-notifications) "params")
           "event"))
         (progress
          (neomacs-lsp-pyright-test-messages-by-method
           wire-log "server->client" "$/progress")))
    (list
     :workspace
     (list :status (lsp--workspace-status workspace)
           :server-id
           (lsp--client-server-id (lsp--workspace-client workspace))
           :priority
           (lsp--client-priority (lsp--workspace-client workspace))
           :multi-root
           (lsp--client-multi-root (lsp--workspace-client workspace))
           :mode lsp-mode
           :process-live
           (not (null (process-live-p (lsp--workspace-proc workspace))))
           :second-reused-workspace (eq workspace second-workspace)
           :second-reused-process
           (eq (lsp--workspace-proc workspace)
               (lsp--workspace-proc second-workspace))
           :roots
           (sort (mapcar (lambda (path) (file-relative-name path root))
                         (lsp-workspace-folders workspace))
                 #'string<)
           :buffers
           (sort
            (mapcar
             (lambda (buffer)
               (file-relative-name (buffer-file-name buffer) root))
             (lsp--workspace-buffers workspace))
            #'string<))
     :process
     (list :argv0 (neomacs-lsp-pyright-test-json start "argv0")
           :args (neomacs-lsp-pyright-test-json start "args")
           :cwd (file-relative-name
                 (neomacs-lsp-pyright-test-json start "cwd") root)
           :scenario (neomacs-lsp-pyright-test-json start "scenario")
           :flavor (neomacs-lsp-pyright-test-json start "flavor"))
     :initialize
     (let* ((client-info
             (neomacs-lsp-pyright-test-json initialize-params "clientInfo"))
            (folder
             (car (neomacs-lsp-pyright-test-json
                   initialize-params "workspaceFolders"))))
       (list
        :root-path (file-relative-name
                    (neomacs-lsp-pyright-test-json initialize-params "rootPath") root)
        :root-uri (neomacs-lsp-pyright-test-normalize-uri
                   (neomacs-lsp-pyright-test-json initialize-params "rootUri") root)
        :folder
        (list (neomacs-lsp-pyright-test-json folder "name")
              (neomacs-lsp-pyright-test-normalize-uri
               (neomacs-lsp-pyright-test-json folder "uri") root))
        :client
        (list (neomacs-lsp-pyright-test-json client-info "name")
              (stringp (neomacs-lsp-pyright-test-json client-info "version")))
        :process-id-integer (integerp
                             (neomacs-lsp-pyright-test-json
                              initialize-params "processId"))
        :initialization-options
        (neomacs-lsp-pyright-test-json initialize-params "initializationOptions")
        :selected-capabilities
        (list
         :apply-edit
         (neomacs-lsp-pyright-test-json workspace-capabilities "applyEdit")
         :configuration
         (neomacs-lsp-pyright-test-json workspace-capabilities "configuration")
         :workspace-folders
         (neomacs-lsp-pyright-test-json workspace-capabilities "workspaceFolders")
         :work-done-progress
         (neomacs-lsp-pyright-test-json window-capabilities "workDoneProgress"))))
     :opened
     (list :raw-uri (neomacs-lsp-pyright-test-json did-open-document "uri")
           :path (neomacs-lsp-pyright-test-normalize-uri
                  (neomacs-lsp-pyright-test-json did-open-document "uri") root)
           :language (neomacs-lsp-pyright-test-json
                      did-open-document "languageId")
           :version (neomacs-lsp-pyright-test-json
                     did-open-document "version")
           :text (neomacs-lsp-pyright-test-json did-open-document "text"))
     :edited
     (let* ((document
             (neomacs-lsp-pyright-test-json change-params "textDocument"))
            (change
             (car (neomacs-lsp-pyright-test-json
                   change-params "contentChanges")))
            (range (neomacs-lsp-pyright-test-json change "range")))
       (list :version (neomacs-lsp-pyright-test-json document "version")
             :range
             (mapcar
              (lambda (edge)
                (let ((position (neomacs-lsp-pyright-test-json range edge)))
                  (list (neomacs-lsp-pyright-test-json position "line")
                        (neomacs-lsp-pyright-test-json position "character"))))
              '("start" "end"))
             :range-length (neomacs-lsp-pyright-test-json change "rangeLength")
             :text (neomacs-lsp-pyright-test-json change "text")))
     :second-opened
     (let ((document
            (neomacs-lsp-pyright-test-json
             (neomacs-lsp-pyright-test-json second-did-open "params")
             "textDocument")))
       (list
        :path
        (neomacs-lsp-pyright-test-normalize-uri
         (neomacs-lsp-pyright-test-json document "uri") root)
        :language (neomacs-lsp-pyright-test-json document "languageId")
        :version (neomacs-lsp-pyright-test-json document "version")
        :text (neomacs-lsp-pyright-test-json document "text")))
     :folder-registration
     (list
      :session-roots
      (sort (mapcar (lambda (path) (file-relative-name path root))
                    (lsp-session-folders (lsp-session)))
            #'string<)
      :workspace-roots
      (sort (mapcar (lambda (path) (file-relative-name path root))
                    (lsp-workspace-folders workspace))
            #'string<)
      :protocol-notifications (length folder-notifications)
      :added
      (mapcar
       (lambda (folder)
         (list
          (neomacs-lsp-pyright-test-json folder "name")
          (neomacs-lsp-pyright-test-normalize-uri
           (neomacs-lsp-pyright-test-json folder "uri") root)))
       (neomacs-lsp-pyright-test-json folder-event "added"))
      :removed (neomacs-lsp-pyright-test-json folder-event "removed"))
     :progress
     (mapcar
      (lambda (message)
        (let* ((params (neomacs-lsp-pyright-test-json message "params"))
               (value (neomacs-lsp-pyright-test-json params "value")))
          (list (neomacs-lsp-pyright-test-json params "token")
                (neomacs-lsp-pyright-test-json value "kind")
                (neomacs-lsp-pyright-test-json value "title")
                (neomacs-lsp-pyright-test-json value "message")
                (neomacs-lsp-pyright-test-json value "percentage"))))
      progress))))
"####;
    let expected = expect![[
        r#"OK (:result (:workspace (:status initialized :server-id pyright :priority 2 :multi-root t :mode t :process-live t :second-reused-workspace t :second-reused-process t :roots ("analytics Ω" "shared service Ω") :buffers ("analytics Ω/src/report.py" "shared service Ω/src/stubs.py")) :process (:argv0 "pyright-langserver" :args ("--stdio") :cwd "analytics Ω" :scenario "multi-root" :flavor "pyright") :initialize (:root-path "analytics Ω" :root-uri "analytics Ω" :folder ("analytics Ω" "analytics Ω") :client ("emacs" t) :process-id-integer t :initialization-options nil :selected-capabilities (:apply-edit t :configuration t :workspace-folders t :work-done-progress t)) :opened (:raw-uri "file://[ORACLE-SANDBOX]/workspace-lifecycle/analytics%20%CE%A9/src/report.py" :path "analytics Ω/src/report.py" :language "python" :version 0 :text "import sys\nimport os\n\n\ndef release_label(name: str) -> str:\n    return f\"ready:{name}\"\n") :edited (:version 1 :range ((6 0) (6 0)) :range-length 0 :text "\nrelease = release_label(\"α-42\")\n") :second-opened (:path "shared service Ω/src/stubs.py" :language "python" :version 0 :text "from typing import Protocol\n\n\nclass Release(Protocol):\n    name: str\n") :folder-registration (:session-roots ("analytics Ω" "shared service Ω") :workspace-roots ("analytics Ω" "shared service Ω") :protocol-notifications 1 :added (("shared service Ω" "shared service Ω")) :removed nil) :progress (("pyright-analysis" "begin" "" nil nil) ("pyright-analysis" "report" nil "1 file to analyze" nil) ("pyright-analysis" "end" nil nil nil))) :fixture-errors nil :terminal ((configurationResponses . 3) (misses) (planExhausted . t) (terminal . "exit")) :wire-plan (("client->server" "initialize") ("client->server" "initialized") ("server->client" "workspace/configuration") ("client->server" "workspace/didChangeConfiguration") ("client->server" "textDocument/didOpen") ("server->client" "workspace/configuration") ("server->client" "window/workDoneProgress/create") ("server->client" "$/progress") ("server->client" "$/progress") ("server->client" "$/progress") ("client->server" "textDocument/didChange") ("client->server" "workspace/didChangeWorkspaceFolders") ("client->server" "textDocument/didOpen") ("client->server" "shutdown") ("client->server" "exit")))"#
    ]];
    ParityBatchCase::value(
        "configured_project_negotiates_edits_and_workspace_roots_over_real_stdio",
        elisp_form,
        expected,
    )
}

fn server_requested_configuration_preserves_nested_values_and_json_false() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lsp-pyright-test-with-project "workspace-configuration" "normal"
  (neomacs-lsp-pyright-test-wait
   (lambda ()
     (and (neomacs-lsp-pyright-test-response
           wire-log "client->server" 701)
          (neomacs-lsp-pyright-test-response
           wire-log "client->server" 702)))
   "both workspace/configuration responses")
  (let* ((python-response
          (neomacs-lsp-pyright-test-response wire-log "client->server" 700))
         (flavor-response
          (neomacs-lsp-pyright-test-response wire-log "client->server" 701))
         (configuration-requests
          (neomacs-lsp-pyright-test-messages-by-method
           wire-log "server->client" "workspace/configuration"))
         (python
          (car (neomacs-lsp-pyright-test-json python-response "result")))
         (pyright
          (car (neomacs-lsp-pyright-test-json flavor-response "result")))
         (analysis (neomacs-lsp-pyright-test-json python "analysis"))
         (pyright-analysis
          (neomacs-lsp-pyright-test-json pyright "analysis")))
    (list
     :requests
     (mapcar
      (lambda (request)
        (let ((item
               (car (neomacs-lsp-pyright-test-json
                     (neomacs-lsp-pyright-test-json request "params")
                     "items"))))
          (list
           (neomacs-lsp-pyright-test-json item "section")
           (neomacs-lsp-pyright-test-normalize-uri
            (neomacs-lsp-pyright-test-json item "scopeUri") root))))
      configuration-requests)
     :python
     (list
      :type-checking-mode
      (neomacs-lsp-pyright-test-json analysis "typeCheckingMode")
      :diagnostic-mode
      (neomacs-lsp-pyright-test-json analysis "diagnosticMode")
      :log-level (neomacs-lsp-pyright-test-json analysis "logLevel")
      :auto-search-paths
      (neomacs-lsp-pyright-test-json analysis "autoSearchPaths")
      :auto-import-completions
      (neomacs-lsp-pyright-test-json analysis "autoImportCompletions")
      :extra-paths (neomacs-lsp-pyright-test-json analysis "extraPaths")
      :python-path
      (neomacs-lsp-pyright-test-normalize-path
       (neomacs-lsp-pyright-test-json python "pythonPath") project-root)
      :venv-path
      (neomacs-lsp-pyright-test-normalize-path
       (neomacs-lsp-pyright-test-json python "venvPath") project-root)
      :severity
      (let ((severity
             (neomacs-lsp-pyright-test-json
              analysis "diagnosticSeverityOverrides")))
        (list
         (cons "reportMissingImports"
               (neomacs-lsp-pyright-test-json severity "reportMissingImports"))
         (cons "reportUnusedVariable"
               (neomacs-lsp-pyright-test-json severity "reportUnusedVariable"))))
     :pyright
     (list :disable-language-services
           (neomacs-lsp-pyright-test-json pyright "disableLanguageServices")
           :disable-organize-imports
           (neomacs-lsp-pyright-test-json pyright "disableOrganizeImports")
           :disable-tagged-hints
           (neomacs-lsp-pyright-test-json pyright "disableTaggedHints")
           :type-checking-mode
           (neomacs-lsp-pyright-test-json pyright "typeCheckingMode")
           :severity
           (let ((severity
                  (neomacs-lsp-pyright-test-json
                   pyright-analysis "diagnosticSeverityOverrides")))
             (list
              (cons "reportMissingImports"
                    (neomacs-lsp-pyright-test-json
                     severity "reportMissingImports"))
              (cons "reportUnusedVariable"
                    (neomacs-lsp-pyright-test-json
                     severity "reportUnusedVariable")))))))))
"####;
    let expected = expect![[
        r#"OK (:result (:requests (("python" "analytics Ω") ("pyright" "analytics Ω")) :python (:type-checking-mode "strict" :diagnostic-mode "workspace" :log-level "warning" :auto-search-paths :json-false :auto-import-completions :json-false :extra-paths ("src" "vendor types/λ") :python-path "envs/bin/python" :venv-path "envs/" :severity (("reportMissingImports" . "error") ("reportUnusedVariable" . :json-false)) :pyright (:disable-language-services :json-false :disable-organize-imports :json-false :disable-tagged-hints t :type-checking-mode "strict" :severity (("reportMissingImports" . "error") ("reportUnusedVariable" . :json-false))))) :fixture-errors nil :terminal ((configurationResponses . 3) (misses) (planExhausted . t) (terminal . "exit")) :wire-plan (("client->server" "initialize") ("client->server" "initialized") ("server->client" "workspace/configuration") ("client->server" "workspace/didChangeConfiguration") ("client->server" "textDocument/didOpen") ("server->client" "workspace/configuration") ("server->client" "window/workDoneProgress/create") ("server->client" "$/progress") ("server->client" "$/progress") ("server->client" "$/progress") ("client->server" "shutdown") ("client->server" "exit")))"#
    ]];
    ParityBatchCase::value(
        "server_requested_configuration_preserves_nested_values_and_json_false",
        elisp_form,
        expected,
    )
}

fn pyright_1_1_411_legacy_progress_string_preserves_the_pinned_callback_behavior() -> ParityBatchCase
{
    let elisp_form = r####"
(neomacs-lsp-pyright-test-with-project "legacy-progress" "legacy-progress"
  (neomacs-lsp-pyright-test-wait
   (lambda ()
     (and (neomacs-lsp-pyright-test-messages-by-method
           wire-log "server->client" "pyright/endProgress")
          (member "Pyright language server is analyzing...done"
                  neomacs-lsp-pyright-test-lsp-log-records)))
   "the complete Pyright 1.1.411 legacy progress sequence")
  (let* ((begin
          (car (neomacs-lsp-pyright-test-messages-by-method
                wire-log "server->client" "pyright/beginProgress")))
         (report
          (car (neomacs-lsp-pyright-test-messages-by-method
                wire-log "server->client" "pyright/reportProgress")))
         (end
          (car (neomacs-lsp-pyright-test-messages-by-method
                wire-log "server->client" "pyright/endProgress")))
         (initialize
          (car (neomacs-lsp-pyright-test-messages-by-method
                wire-log "client->server" "initialize")))
         (window-capabilities
          (neomacs-lsp-pyright-test-json
           (neomacs-lsp-pyright-test-json
            (neomacs-lsp-pyright-test-json initialize "params")
            "capabilities")
           "window")))
    (list
     :work-done-capability
     (neomacs-lsp-pyright-test-json
      window-capabilities "workDoneProgress")
     :wire
     (list
      (list (neomacs-lsp-pyright-test-json begin "method")
            (neomacs-lsp-pyright-test-json begin "params"))
      (list (neomacs-lsp-pyright-test-json report "method")
            (neomacs-lsp-pyright-test-json report "params"))
      (list (neomacs-lsp-pyright-test-json end "method")
            (neomacs-lsp-pyright-test-json end "params")))
     :package-progress-logs
     (reverse neomacs-lsp-pyright-test-lsp-log-records)
     :report-string-logged
     (member "1 file to analyze"
             neomacs-lsp-pyright-test-lsp-log-records)
     :handler-errors
     (reverse neomacs-lsp-pyright-test-demoted-errors)
     :workspace-status (lsp--workspace-status workspace))))
"####;
    let expected = expect![[
        r#"OK (:result (:work-done-capability :json-false :wire (("pyright/beginProgress" nil) ("pyright/reportProgress" "1 file to analyze") ("pyright/endProgress" nil)) :package-progress-logs ("Pyright language server is analyzing..." "Pyright language server is analyzing...done") :report-string-logged nil :handler-errors ("Error processing message (wrong-type-argument stringp 49).") :workspace-status initialized) :fixture-errors nil :terminal ((configurationResponses . 3) (misses) (planExhausted . t) (terminal . "exit")) :wire-plan (("client->server" "initialize") ("client->server" "initialized") ("server->client" "workspace/configuration") ("client->server" "workspace/didChangeConfiguration") ("client->server" "textDocument/didOpen") ("server->client" "workspace/configuration") ("server->client" "pyright/beginProgress") ("server->client" "pyright/reportProgress") ("server->client" "pyright/endProgress") ("client->server" "shutdown") ("client->server" "exit")))"#
    ]];
    ParityBatchCase::value(
        "pyright_1_1_411_legacy_progress_string_preserves_the_pinned_callback_behavior",
        elisp_form,
        expected,
    )
}

fn organize_imports_preserves_raw_package_uri_and_applies_the_server_edit() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lsp-pyright-test-with-project "organize-imports" "normal"
  (neomacs-lsp-pyright-test-wait
   (lambda ()
     (and (neomacs-lsp-pyright-test-response
           wire-log "client->server" 701)
          (neomacs-lsp-pyright-test-response
           wire-log "client->server" 702)))
   "the initialized Pyright workspace")
  (goto-char (point-min))
  (forward-line 1)
  (let ((before (buffer-substring-no-properties (point-min) (point-max)))
        (point-before (point)))
    (lsp-pyright-organize-imports)
    (let* ((did-open
            (car (neomacs-lsp-pyright-test-messages-by-method
                  wire-log "client->server" "textDocument/didOpen")))
           (opened-uri
            (neomacs-lsp-pyright-test-json
             (neomacs-lsp-pyright-test-json
              (neomacs-lsp-pyright-test-json did-open "params")
              "textDocument")
             "uri"))
           (execute
            (car (neomacs-lsp-pyright-test-messages-by-method
                  wire-log "client->server" "workspace/executeCommand")))
           (execute-params
            (neomacs-lsp-pyright-test-json execute "params"))
           (raw-argument
            (car (neomacs-lsp-pyright-test-json execute-params "arguments")))
           (apply-edit
            (car (neomacs-lsp-pyright-test-messages-by-method
                  wire-log "server->client" "workspace/applyEdit")))
           (apply-params
            (neomacs-lsp-pyright-test-json apply-edit "params"))
           (edit (neomacs-lsp-pyright-test-json apply-params "edit"))
           (changes (neomacs-lsp-pyright-test-json edit "changes"))
           (uri (car (car changes)))
           (text-edit (car (cdr (car changes))))
           (client-response
            (neomacs-lsp-pyright-test-response wire-log "client->server" 703)))
      (list
       :before before
       :point-before point-before
       :after (buffer-substring-no-properties (point-min) (point-max))
       :point-after (point)
       :modified (buffer-modified-p)
       :uris
       (list :did-open-encoded opened-uri
             :organize-raw raw-argument
             :raw-differs (not (equal opened-uri raw-argument))
             :same-path
             (file-equal-p (lsp--uri-to-path opened-uri)
                           (substring raw-argument (length "file://"))))
       :command (neomacs-lsp-pyright-test-json execute-params "command")
       :apply-edit
       (list
        :label (neomacs-lsp-pyright-test-json apply-params "label")
        :uri (neomacs-lsp-pyright-test-normalize-uri uri root)
        :range
        (let ((range (neomacs-lsp-pyright-test-json text-edit "range")))
          (mapcar
           (lambda (edge)
             (let ((position (neomacs-lsp-pyright-test-json range edge)))
               (list (neomacs-lsp-pyright-test-json position "line")
                     (neomacs-lsp-pyright-test-json position "character"))))
           '("start" "end")))
        :new-text (neomacs-lsp-pyright-test-json text-edit "newText")
        :client-applied
        (neomacs-lsp-pyright-test-json
         (neomacs-lsp-pyright-test-json client-response "result") "applied"))))))
"####;
    let expected = expect![[
        r#"OK (:result (:before "import sys\nimport os\n\n\ndef release_label(name: str) -> str:\n    return f\"ready:{name}\"\n" :point-before 12 :after "import os\nimport sys\n\n\ndef release_label(name: str) -> str:\n    return f\"ready:{name}\"\n" :point-after 11 :modified t :uris (:did-open-encoded "file://[ORACLE-SANDBOX]/organize-imports/analytics%20%CE%A9/src/report.py" :organize-raw "file://[ORACLE-SANDBOX]/organize-imports/analytics Ω/src/report.py" :raw-differs t :same-path t) :command "pyright.organizeimports" :apply-edit (:label "Organize imports" :uri "analytics Ω/src/report.py" :range ((0 0) (2 0)) :new-text "import os\nimport sys\n" :client-applied t)) :fixture-errors nil :terminal ((configurationResponses . 3) (misses) (planExhausted . t) (terminal . "exit")) :wire-plan (("client->server" "initialize") ("client->server" "initialized") ("server->client" "workspace/configuration") ("client->server" "workspace/didChangeConfiguration") ("client->server" "textDocument/didOpen") ("server->client" "workspace/configuration") ("server->client" "window/workDoneProgress/create") ("server->client" "$/progress") ("server->client" "$/progress") ("server->client" "$/progress") ("client->server" "workspace/executeCommand") ("server->client" "workspace/applyEdit") ("client->server" "textDocument/didChange") ("client->server" "shutdown") ("client->server" "exit")))"#
    ]];
    ParityBatchCase::value(
        "organize_imports_preserves_raw_package_uri_and_applies_the_server_edit",
        elisp_form,
        expected,
    )
}

fn organize_imports_surfaces_a_real_json_rpc_error_without_modifying_the_buffer() -> ParityBatchCase
{
    let elisp_form = r####"
(neomacs-lsp-pyright-test-with-project "organize-error" "organize-error"
  (neomacs-lsp-pyright-test-wait
   (lambda ()
     (and (neomacs-lsp-pyright-test-response
           wire-log "client->server" 701)
          (neomacs-lsp-pyright-test-response
           wire-log "client->server" 702)))
   "the initialized Pyright workspace")
  (let ((before (buffer-substring-no-properties (point-min) (point-max)))
        (outcome
         (condition-case error
             (list :value (lsp-pyright-organize-imports))
           (error
            (list :signal (car error)
                  :message (error-message-string error))))))
    (let* ((execute
            (car (neomacs-lsp-pyright-test-messages-by-method
                  wire-log "client->server" "workspace/executeCommand")))
           (params (neomacs-lsp-pyright-test-json execute "params")))
      (list
       :outcome outcome
       :command (neomacs-lsp-pyright-test-json params "command")
       :argument (car (neomacs-lsp-pyright-test-json params "arguments"))
       :unchanged
       (equal before (buffer-substring-no-properties (point-min) (point-max)))
       :modified (buffer-modified-p)
       :apply-edit-count
       (length (neomacs-lsp-pyright-test-messages-by-method
                wire-log "server->client" "workspace/applyEdit"))))))
"####;
    let expected = expect![[
        r#"OK (:result (:outcome (:signal error :message "‘workspace/executeCommand’ with ‘pyright.organizeimports’ failed.\n\n(error \"fixture refused to organize this module\")") :command "pyright.organizeimports" :argument "file://[ORACLE-SANDBOX]/organize-error/analytics Ω/src/report.py" :unchanged t :modified nil :apply-edit-count 0) :fixture-errors nil :terminal ((configurationResponses . 3) (misses) (planExhausted . t) (terminal . "exit")) :wire-plan (("client->server" "initialize") ("client->server" "initialized") ("server->client" "workspace/configuration") ("client->server" "workspace/didChangeConfiguration") ("client->server" "textDocument/didOpen") ("server->client" "workspace/configuration") ("server->client" "window/workDoneProgress/create") ("server->client" "$/progress") ("server->client" "$/progress") ("server->client" "$/progress") ("client->server" "workspace/executeCommand") ("client->server" "shutdown") ("client->server" "exit")))"#
    ]];
    ParityBatchCase::value(
        "organize_imports_surfaces_a_real_json_rpc_error_without_modifying_the_buffer",
        elisp_form,
        expected,
    )
}

fn python_discovery_obeys_real_filesystem_and_search_function_precedence() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-lsp-pyright-test-root "python-search"))
       (project-root (expand-file-name "typed service/" root))
       (source-directory (expand-file-name "src/deep/" project-root))
       (explicit (expand-file-name "envs/release/bin/python" project-root))
       (configured (expand-file-name "configured-env/bin/python" project-root))
       (venv (expand-file-name "venv/bin/python" project-root))
       (dot-venv (expand-file-name ".venv/bin/python" project-root))
       (path-bin (expand-file-name "path-bin/" root))
       (path-python (expand-file-name "fixture-python" path-bin))
       (process-environment (copy-sequence process-environment))
       (exec-path (cons path-bin exec-path)))
  (make-directory source-directory t)
  (mapc #'neomacs-lsp-pyright-test-write-python
        (list explicit configured venv dot-venv path-python))
  (setenv "PATH" (concat path-bin path-separator (getenv "PATH")))
  (unwind-protect
      (let ((default-directory source-directory))
        (list
         :explicit
         (let ((lsp-pyright-venv-path
                (file-name-directory (directory-file-name
                                      (file-name-directory explicit))))
               (lsp-pyright-venv-directory nil)
               (lsp-pyright-python-search-functions
                '(lsp-pyright--locate-python-venv
                  lsp-pyright--locate-python-python)))
           (file-relative-name (lsp-pyright-locate-python) root))
         :configured-directory
         (let ((lsp-pyright-venv-path nil)
               (lsp-pyright-venv-directory "configured-env/")
               (lsp-pyright-python-search-functions
                '(lsp-pyright--locate-python-venv
                  lsp-pyright--locate-python-python)))
           (file-relative-name (lsp-pyright-locate-python) root))
         :venv-before-dot-venv
         (let ((lsp-pyright-venv-path nil)
               (lsp-pyright-venv-directory nil)
               (lsp-pyright-python-search-functions
                '(lsp-pyright--locate-python-venv
                  lsp-pyright--locate-python-python)))
           (file-relative-name (lsp-pyright-locate-python) root))
         :missing-env-falls-through
         (let ((lsp-pyright-venv-path (expand-file-name "missing/" project-root))
               (lsp-pyright-venv-directory nil)
               (lsp-pyright-python-executable-cmd "fixture-python")
               (lsp-pyright-python-search-functions
                '(lsp-pyright--locate-python-venv
                  lsp-pyright--locate-python-python)))
           (file-relative-name (lsp-pyright-locate-python) root))
         :custom-path-first
         (let ((lsp-pyright-venv-path nil)
               (lsp-pyright-venv-directory nil)
               (lsp-pyright-python-executable-cmd "fixture-python")
               (lsp-pyright-python-search-functions
                '(lsp-pyright--locate-python-python
                  lsp-pyright--locate-python-venv)))
           (file-relative-name (lsp-pyright-locate-python) root))
         :none
         (let ((exec-path nil)
               (process-environment (copy-sequence process-environment))
               (lsp-pyright-venv-path (expand-file-name "missing/" project-root))
               (lsp-pyright-venv-directory nil)
               (lsp-pyright-python-executable-cmd "fixture-python")
               (lsp-pyright-python-search-functions
                '(lsp-pyright--locate-python-venv
                  lsp-pyright--locate-python-python)))
           (setenv "PATH" "")
           (lsp-pyright-locate-python))))
    (when (and (file-directory-p root)
               (file-in-directory-p
                root
                (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
      (delete-directory root t))))
"####;
    let expected = expect![[
        r#"OK (:explicit "typed service/envs/release/bin/python" :configured-directory "typed service/configured-env/bin/python" :venv-before-dot-venv "typed service/venv/bin/python" :missing-env-falls-through "path-bin/fixture-python" :custom-path-first "path-bin/fixture-python" :none nil)"#
    ]];
    ParityBatchCase::value(
        "python_discovery_obeys_real_filesystem_and_search_function_precedence",
        elisp_form,
        expected,
    )
}

fn missing_server_keeps_the_buffer_disconnected_and_emits_one_delegated_warning() -> ParityBatchCase
{
    let elisp_form = r####"
(let* ((root (neomacs-lsp-pyright-test-root "missing-server"))
       (project-root (expand-file-name "missing server project/" root))
       (source-file (expand-file-name "main.py" project-root))
       (empty-bin (expand-file-name "empty-bin/" root))
       (lsp--session (make-lsp-session))
       (lsp-session-file nil)
       (lsp-auto-guess-root t)
       (lsp-enable-suggest-server-download nil)
       (lsp-warn-no-matched-clients t)
       (lsp-server-install-dir (expand-file-name "server-install/" root))
       (process-environment (copy-sequence process-environment))
       (exec-path (list empty-bin))
       (origin-buffer (current-buffer))
       (source-buffer nil)
       (warnings nil)
       (observer
        (lambda (original format-string &rest arguments)
          (push (apply #'format format-string arguments) warnings)
          (apply original format-string arguments))))
  (make-directory (expand-file-name ".git/" project-root) t)
  (make-directory empty-bin t)
  (neomacs-lsp-pyright-test-write-file source-file "value: int = 42\n")
  (setenv "PATH" empty-bin)
  (advice-add 'lsp--warn :around observer)
  (unwind-protect
      (progn
        (setq source-buffer (find-file-noselect source-file))
        (with-current-buffer source-buffer
          (python-mode)
          (let ((result (lsp))
                (package-path-error
                 (condition-case error
                     (list :value (lsp-package-path 'pyright))
                   (error (list :signal (car error)
                                :message (error-message-string error))))))
            (list
             :result result
             :mode lsp-mode
             :workspaces (lsp-workspaces)
             :package-path package-path-error
             :warnings (nreverse warnings)
             :processes
             (mapcar #'process-name
                     (seq-filter
                      (lambda (process)
                        (string-prefix-p "pyright" (process-name process)))
                      (process-list)))))))
    (advice-remove 'lsp--warn observer)
    (when (buffer-live-p source-buffer)
      (kill-buffer source-buffer))
    (when (buffer-live-p origin-buffer)
      (set-buffer origin-buffer))
    (when (and (file-directory-p root)
               (file-in-directory-p
                root
                (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
      (delete-directory root t))))
"####;
    let expected = expect![[
        r#"OK (:result #("LSP :: The following servers support current file but automatic download is disabled: pyright\n(If you have already installed the server check *lsp-log*)." 0 3 (face warning)) :mode nil :workspaces nil :package-path (:signal error :message "The package pyright is not installed.  Unable to find pyright-langserver") :warnings ("The following servers support current file but automatic download is disabled: pyright\n(If you have already installed the server check *lsp-log*).") :processes nil)"#
    ]];
    ParityBatchCase::value(
        "missing_server_keeps_the_buffer_disconnected_and_emits_one_delegated_warning",
        elisp_form,
        expected,
    )
}

fn rejected_initialize_never_opens_the_document_and_drains_the_error_response() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lsp-pyright-test-with-project "initialize-error" "initialize-error"
  (neomacs-lsp-pyright-test-wait
   (lambda ()
     (and workspace
          (not (process-live-p (lsp--workspace-proc workspace)))))
   "the rejected Pyright process to stop")
  (let* ((initialize
          (car (neomacs-lsp-pyright-test-messages-by-method
                wire-log "client->server" "initialize")))
         (response
          (neomacs-lsp-pyright-test-response
           wire-log "server->client"
           (neomacs-lsp-pyright-test-json initialize "id")))
         (error (neomacs-lsp-pyright-test-json response "error")))
    (list
     :workspace-status (lsp--workspace-status workspace)
     :mode lsp-mode
     :process-status (process-status (lsp--workspace-proc workspace))
     :error
     (list (neomacs-lsp-pyright-test-json error "code")
           (neomacs-lsp-pyright-test-json error "message"))
     :initialize-count
     (length (neomacs-lsp-pyright-test-messages-by-method
              wire-log "client->server" "initialize"))
     :initialized-count
     (length (neomacs-lsp-pyright-test-messages-by-method
              wire-log "client->server" "initialized"))
     :opened-count
     (length (neomacs-lsp-pyright-test-messages-by-method
              wire-log "client->server" "textDocument/didOpen")))))
"####;
    let expected = expect![[
        r#"OK (:result (:workspace-status starting :mode t :process-status exit :error (-32002 "fixture rejected project configuration") :initialize-count 1 :initialized-count 0 :opened-count 0) :fixture-errors nil :terminal ((configurationResponses . 0) (misses) (planExhausted . t) (terminal . "initialize-error")) :wire-plan (("client->server" "initialize")))"#
    ]];
    ParityBatchCase::value(
        "rejected_initialize_never_opens_the_document_and_drains_the_error_response",
        elisp_form,
        expected,
    )
}

fn public_npm_install_creates_a_server_then_starts_a_real_workspace() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lsp-pyright-test-with-npm-install "npm-install-success" "success"
  (let ((events nil)
        (info-observer nil)
        (error-observer nil))
    (setq info-observer
          (lambda (original format-string &rest arguments)
            (push (list 'info (apply #'format format-string arguments)) events)
            (apply original format-string arguments))
          error-observer
          (lambda (original format-string &rest arguments)
            (push (list 'error (apply #'format format-string arguments)) events)
            (apply original format-string arguments)))
    (advice-add 'lsp--info :around info-observer)
    (advice-add 'lsp--error :around error-observer)
    (unwind-protect
        (progn
          (lsp-install-server nil 'pyright)
          (let ((download-started (lsp--client-download-in-progress? client)))
            (neomacs-lsp-pyright-test-wait
             (lambda () (not (lsp--client-download-in-progress? client)))
             "the npm installation completion timer")
            (let* ((installed-path (lsp-package-path 'pyright))
                   (install-buffer
                    (seq-find
                     (lambda (buffer)
                       (buffer-local-value 'lsp-installation-buffer-mode buffer))
                     (buffer-list))))
              (lsp)
              (setq workspace (lsp-find-workspace 'pyright source-file))
              (neomacs-lsp-pyright-test-wait
               (lambda ()
                 (and workspace
                      (eq (lsp--workspace-status workspace) 'initialized)
                      (neomacs-lsp-pyright-test-response
                       wire-log "client->server" 701)
                      (neomacs-lsp-pyright-test-response
                       wire-log "client->server" 702)))
               "the npm-installed Pyright server")
              (neomacs-lsp-pyright-test-stop-workspace workspace)
              (neomacs-lsp-pyright-test-wait
               (lambda () (neomacs-lsp-pyright-test-fixture-state wire-log))
               "the installed server's terminal state")
              (list
               :download-started download-started
               :download-finished
               (not (lsp--client-download-in-progress? client))
               :install-events (seq-take (nreverse events) 2)
               :installed
               (list :path (file-relative-name installed-path root)
                     :executable (file-executable-p installed-path)
                     :mode (file-modes installed-path))
               :npm
               (mapcar
                (lambda (record)
                  (list
                   :kind (neomacs-lsp-pyright-test-json record "kind")
                   :args
                   (mapcar
                    (lambda (argument)
                      (neomacs-lsp-pyright-test-normalize-path argument root))
                    (neomacs-lsp-pyright-test-json record "args"))
                   :cwd
                   (file-relative-name
                    (neomacs-lsp-pyright-test-json record "cwd") root)
                   :inside-emacs-mode
                   (neomacs-lsp-pyright-test-json record "insideEmacsMode")
                   :pager (neomacs-lsp-pyright-test-json record "pager")
                   :tmpdir-matches
                   (equal (file-truename
                           (neomacs-lsp-pyright-test-json record "tmpdir"))
                          (file-truename (getenv "TMPDIR")))))
                (neomacs-lsp-pyright-test-read-json-lines npm-log))
               :install-buffer
               (neomacs-lsp-pyright-test-install-buffer-summary
                install-buffer root)
               :workspace
               (list :status (lsp--workspace-status workspace)
                     :server-id
                     (lsp--client-server-id (lsp--workspace-client workspace)))
               :fixture-errors
               (neomacs-lsp-pyright-test-fixture-errors wire-log)
               :terminal (neomacs-lsp-pyright-test-fixture-state wire-log)
               :wire-plan (neomacs-lsp-pyright-test-wire-plan wire-log)))))
      (advice-remove 'lsp--info info-observer)
      (advice-remove 'lsp--error error-observer))))
"####;
    let expected = expect![[
        r#"OK (:value (:download-started t :download-finished t :install-events ((info "Download pyright started.") (info "Server pyright downloaded, auto-starting in 0 buffers.")) :installed (:path "server-install/npm/pyright/bin/pyright-langserver" :executable t :mode 493) :npm ((:kind "install" :args ("-g" "--prefix" "server-install/npm/pyright" "install" "pyright") :cwd "install project" :inside-emacs-mode "comint" :pager "" :tmpdir-matches t) (:kind "view" :args ("view" "pyright" "peerDependencies") :cwd "install project" :inside-emacs-mode "" :pager "fixture-pager" :tmpdir-matches t)) :install-buffer (:mode t :process nil :default-directory "install project/" :command "[ROOT]/npm-bin/npm -g --prefix [ROOT]/server-install/npm/pyright install pyright" :output ("installed pyright fixture") :status "finished") :workspace (:status initialized :server-id pyright) :fixture-errors nil :terminal ((configurationResponses . 3) (misses) (planExhausted . t) (terminal . "exit")) :wire-plan (("client->server" "initialize") ("client->server" "initialized") ("server->client" "workspace/configuration") ("client->server" "workspace/didChangeConfiguration") ("client->server" "textDocument/didOpen") ("server->client" "workspace/configuration") ("server->client" "window/workDoneProgress/create") ("server->client" "$/progress") ("server->client" "$/progress") ("server->client" "$/progress") ("client->server" "shutdown") ("client->server" "exit"))) :stdout "" :stderr "Warning (files): Missing ‘lexical-binding’ cookie in \"[ORACLE-WORKSPACE]/tmp/melpa/source-install-cache/lsp-pyright/20260507.1742/187e08caee4e1630a9975f492274c739f325392f/517749e477c16c0437cae029be71e672061a6c19/d31dec67631f14ef8be3ad6438e172a07298082b/home/.emacs.d/elpa/lv-20200507.1518/lv.el\".\nYou can add one with ‘M-x elisp-enable-lexical-binding RET’.\nSee ‘(elisp)Selecting Lisp Dialect’ and ‘(elisp)Converting to Lexical Binding’\nfor more information.\nCan’t guess python-indent-offset, using defaults: 4\nCan’t guess python-indent-offset, using defaults: 4\nComint finished\nlsp-session-file is nil, not persisting session.\n")"#
    ]];
    ParityBatchCase::value(
        "public_npm_install_creates_a_server_then_starts_a_real_workspace",
        elisp_form,
        expected,
    )
    .direct_command_loop()
}

fn failed_npm_install_resets_download_state_and_leaves_no_server_artifact() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lsp-pyright-test-with-npm-install "npm-install-failure" "failure"
  (let ((events nil)
        (info-observer nil)
        (error-observer nil))
    (setq info-observer
          (lambda (original format-string &rest arguments)
            (push (list 'info (apply #'format format-string arguments)) events)
            (apply original format-string arguments))
          error-observer
          (lambda (original format-string &rest arguments)
            (push (list 'error (apply #'format format-string arguments)) events)
            (apply original format-string arguments)))
    (advice-add 'lsp--info :around info-observer)
    (advice-add 'lsp--error :around error-observer)
    (unwind-protect
        (progn
          (lsp-install-server nil 'pyright)
          (let ((download-started (lsp--client-download-in-progress? client)))
            (neomacs-lsp-pyright-test-wait
             (lambda () (not (lsp--client-download-in-progress? client)))
             "the failed npm installation completion timer")
            (let* ((install-buffer
                    (seq-find
                     (lambda (buffer)
                       (buffer-local-value 'lsp-installation-buffer-mode buffer))
                     (buffer-list)))
                   (installed-path
                    (expand-file-name
                     (concat "bin/" lsp-pyright-langserver-command "-langserver")
                     npm-prefix))
                   (path-outcome
                    (condition-case error
                        (list :value (lsp-package-path 'pyright))
                      (error (list :signal (car error)
                                   :message (error-message-string error))))))
              (list
               :download-started download-started
               :download-finished
               (not (lsp--client-download-in-progress? client))
               :events (nreverse events)
               :artifact-exists (file-exists-p installed-path)
               :prefix-tree
               (sort
                (mapcar (lambda (path) (file-relative-name path npm-prefix))
                        (directory-files-recursively npm-prefix "." t))
                #'string<)
               :package-path path-outcome
               :npm
               (mapcar
                (lambda (record)
                  (list
                   :kind (neomacs-lsp-pyright-test-json record "kind")
                   :args
                   (mapcar
                    (lambda (argument)
                      (neomacs-lsp-pyright-test-normalize-path argument root))
                    (neomacs-lsp-pyright-test-json record "args"))
                   :cwd
                   (file-relative-name
                    (neomacs-lsp-pyright-test-json record "cwd") root)
                   :inside-emacs-mode
                   (neomacs-lsp-pyright-test-json record "insideEmacsMode")
                   :pager (neomacs-lsp-pyright-test-json record "pager")))
                (neomacs-lsp-pyright-test-read-json-lines npm-log))
               :install-buffer
               (neomacs-lsp-pyright-test-install-buffer-summary
                install-buffer root)
               :workspace (lsp-find-workspace 'pyright source-file)))))
      (advice-remove 'lsp--info info-observer)
      (advice-remove 'lsp--error error-observer))))
"####;
    let expected = expect![[
        r#"OK (:value (:download-started t :download-finished t :events ((info "Download pyright started.") (error "Server pyright install process failed with the following error message: exited abnormally with code 23.\nCheck `*lsp-install*' and `*lsp-log*' buffer.")) :artifact-exists nil :prefix-tree ("lib") :package-path (:signal error :message "The package pyright is not installed.  Unable to find pyright-langserver") :npm ((:kind "install" :args ("-g" "--prefix" "server-install/npm/pyright" "install" "pyright") :cwd "install project" :inside-emacs-mode "comint" :pager "")) :install-buffer (:mode t :process nil :default-directory "install project/" :command "[ROOT]/npm-bin/npm -g --prefix [ROOT]/server-install/npm/pyright install pyright" :output ("NEOMACS_FAKE_NPM: intentional install failure") :status "exited-abnormally-23") :workspace nil) :stdout "" :stderr "Warning (files): Missing ‘lexical-binding’ cookie in \"[ORACLE-WORKSPACE]/tmp/melpa/source-install-cache/lsp-pyright/20260507.1742/187e08caee4e1630a9975f492274c739f325392f/517749e477c16c0437cae029be71e672061a6c19/d31dec67631f14ef8be3ad6438e172a07298082b/home/.emacs.d/elpa/lv-20200507.1518/lv.el\".\nYou can add one with ‘M-x elisp-enable-lexical-binding RET’.\nSee ‘(elisp)Selecting Lisp Dialect’ and ‘(elisp)Converting to Lexical Binding’\nfor more information.\nCan’t guess python-indent-offset, using defaults: 4\nCan’t guess python-indent-offset, using defaults: 4\nComint exited abnormally with code 23\n")"#
    ]];
    ParityBatchCase::value(
        "failed_npm_install_resets_download_state_and_leaves_no_server_artifact",
        elisp_form,
        expected,
    )
    .direct_command_loop()
}

fn basedpyright_preload_selects_its_dependency_settings_progress_and_command() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lsp-pyright-test-with-project "basedpyright-workflow" "normal"
  (neomacs-lsp-pyright-test-wait
   (lambda ()
     (and (neomacs-lsp-pyright-test-response
           wire-log "client->server" 701)
          (neomacs-lsp-pyright-test-response
           wire-log "client->server" 702)))
   "the BasedPyright configuration response")
  (lsp-pyright-organize-imports)
  (let* ((start (car (neomacs-lsp-pyright-test-read-json-lines start-log)))
         (response
          (neomacs-lsp-pyright-test-response wire-log "client->server" 701))
         (settings (car (neomacs-lsp-pyright-test-json response "result")))
         (analysis (neomacs-lsp-pyright-test-json settings "analysis"))
         (inlay (neomacs-lsp-pyright-test-json analysis "inlayHints"))
         (execute
          (car (neomacs-lsp-pyright-test-messages-by-method
                wire-log "client->server" "workspace/executeCommand")))
         (execute-params (neomacs-lsp-pyright-test-json execute "params"))
         (did-open
          (car (neomacs-lsp-pyright-test-messages-by-method
                wire-log "client->server" "textDocument/didOpen")))
         (encoded-uri
          (neomacs-lsp-pyright-test-json
           (neomacs-lsp-pyright-test-json
            (neomacs-lsp-pyright-test-json did-open "params") "textDocument")
           "uri"))
         (raw-uri (car (neomacs-lsp-pyright-test-json
                        execute-params "arguments"))))
    (list
     :preloaded-command lsp-pyright-langserver-command
     :dependency (gethash 'pyright lsp--dependencies)
     :process
     (list (neomacs-lsp-pyright-test-json start "argv0")
           (neomacs-lsp-pyright-test-json start "args")
           (neomacs-lsp-pyright-test-json start "flavor"))
     :settings
     (list
      :disable-language-services
      (neomacs-lsp-pyright-test-json settings "disableLanguageServices")
      :disable-organize-imports
      (neomacs-lsp-pyright-test-json settings "disableOrganizeImports")
      :disable-tagged-hints
      (neomacs-lsp-pyright-test-json settings "disableTaggedHints")
      :type-checking-mode
      (neomacs-lsp-pyright-test-json settings "typeCheckingMode")
      :inlay-hints
      (list
       (neomacs-lsp-pyright-test-json inlay "variableTypes")
       (neomacs-lsp-pyright-test-json inlay "callArgumentNames")
       (neomacs-lsp-pyright-test-json inlay "functionReturnTypes")
       (neomacs-lsp-pyright-test-json inlay "genericTypes")))
     :command
     (list (neomacs-lsp-pyright-test-json execute-params "command")
           :encoded-uri encoded-uri
           :raw-uri raw-uri
           :raw-differs (not (equal encoded-uri raw-uri)))
     :final-buffer (buffer-substring-no-properties (point-min) (point-max)))))
"####;
    let expected = expect![[
        r#"OK (:result (:preloaded-command "basedpyright" :dependency ((:system "basedpyright-langserver") (:npm :package "basedpyright" :path "basedpyright-langserver")) :process ("basedpyright-langserver" ("--stdio") "basedpyright") :settings (:disable-language-services :json-false :disable-organize-imports :json-false :disable-tagged-hints t :type-checking-mode "strict" :inlay-hints (t :json-false t :json-false)) :command ("basedpyright.organizeimports" :encoded-uri "file://[ORACLE-SANDBOX]/basedpyright-workflow/analytics%20%CE%A9/src/report.py" :raw-uri "file://[ORACLE-SANDBOX]/basedpyright-workflow/analytics Ω/src/report.py" :raw-differs t) :final-buffer "import os\nimport sys\n\n\ndef release_label(name: str) -> str:\n    return f\"ready:{name}\"\n") :fixture-errors nil :terminal ((configurationResponses . 3) (misses) (planExhausted . t) (terminal . "exit")) :wire-plan (("client->server" "initialize") ("client->server" "initialized") ("server->client" "workspace/configuration") ("client->server" "workspace/didChangeConfiguration") ("client->server" "textDocument/didOpen") ("server->client" "workspace/configuration") ("server->client" "window/workDoneProgress/create") ("server->client" "$/progress") ("server->client" "$/progress") ("server->client" "$/progress") ("client->server" "workspace/executeCommand") ("server->client" "workspace/applyEdit") ("client->server" "textDocument/didChange") ("client->server" "shutdown") ("client->server" "exit")))"#
    ]];
    ParityBatchCase::value(
        "basedpyright_preload_selects_its_dependency_settings_progress_and_command",
        elisp_form,
        expected,
    )
}

pub(super) fn pyright_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        configured_project_negotiates_edits_and_workspace_roots_over_real_stdio(),
        server_requested_configuration_preserves_nested_values_and_json_false(),
        pyright_1_1_411_legacy_progress_string_preserves_the_pinned_callback_behavior(),
        organize_imports_preserves_raw_package_uri_and_applies_the_server_edit(),
        organize_imports_surfaces_a_real_json_rpc_error_without_modifying_the_buffer(),
        python_discovery_obeys_real_filesystem_and_search_function_precedence(),
        missing_server_keeps_the_buffer_disconnected_and_emits_one_delegated_warning(),
        rejected_initialize_never_opens_the_document_and_drains_the_error_response(),
        public_npm_install_creates_a_server_then_starts_a_real_workspace(),
        failed_npm_install_resets_download_state_and_leaves_no_server_artifact(),
    ]
}

pub(super) fn basedpyright_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![basedpyright_preload_selects_its_dependency_settings_progress_and_command()]
}

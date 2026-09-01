use expect_test::expect;

use super::ParityBatchCase;

fn opening_a_maven_java_project_negotiates_jdt_capabilities_over_real_stdio() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lsp-java-test-with-project "startup"
  (neomacs-lsp-java-test-wait
   (lambda ()
     (neomacs-lsp-java-test-wire-messages-by-method
      wire-log "textDocument/didOpen"))
   "the initial Java text document to open")
  (let* ((messages (neomacs-lsp-java-test-read-wire-log wire-log))
         (initialize (car (neomacs-lsp-java-test-wire-messages-by-method
                           wire-log "initialize")))
         (params (neomacs-lsp-java-test-json initialize "params"))
         (options (neomacs-lsp-java-test-json params "initializationOptions"))
         (extended (neomacs-lsp-java-test-json options "extendedClientCapabilities")))
    (list
     :buffer (file-relative-name (buffer-file-name) project-root)
     :major-mode major-mode
     :lsp-mode lsp-mode
     :workspace-status (lsp--workspace-status workspace)
     :server-id (lsp--client-server-id (lsp--workspace-client workspace))
     :process-live (not (null (process-live-p (lsp--workspace-proc workspace))))
     :root-uri (neomacs-lsp-java-test-normalize-uri
                (neomacs-lsp-java-test-json params "rootUri") root)
     :workspace-folders
     (mapcar
      (lambda (folder)
        (list
         (neomacs-lsp-java-test-json folder "name")
         (neomacs-lsp-java-test-normalize-uri
          (neomacs-lsp-java-test-json folder "uri") root)))
      (neomacs-lsp-java-test-json params "workspaceFolders"))
     :extended-capabilities
     (mapcar
      (lambda (key) (cons key (neomacs-lsp-java-test-json extended key)))
      '("progressReportProvider"
        "classFileContentsSupport"
        "overrideMethodsPromptSupport"
        "advancedOrganizeImportsSupport"
        "moveRefactoringSupport"
        "resolveAdditionalTextEditsSupport"))
     :wire-methods
     (delq nil
           (mapcar (lambda (message)
                     (neomacs-lsp-java-test-json message "method"))
                   messages)))))
"####;
    let expected = expect![[
        r##"OK (:buffer "src/main/java/example/DeploymentService.java" :major-mode java-mode :lsp-mode t :workspace-status initialized :server-id jdtls :process-live t :root-uri "deployment-service" :workspace-folders (("deployment-service" "deployment-service")) :extended-capabilities (("progressReportProvider" . t) ("classFileContentsSupport" . t) ("overrideMethodsPromptSupport" . t) ("advancedOrganizeImportsSupport" . t) ("moveRefactoringSupport" . t) ("resolveAdditionalTextEditsSupport" . t)) :wire-methods ("initialize" "initialized" "workspace/didChangeConfiguration" "textDocument/didOpen"))"##
    ]];
    ParityBatchCase::value(
        "opening_a_maven_java_project_negotiates_jdt_capabilities_over_real_stdio",
        elisp_form,
        expected,
    )
}

fn incremental_and_full_project_builds_report_real_async_lifecycle() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lsp-java-test-with-project "build-project"
  (let* ((messages nil)
         (observer
          (lambda (original format-string &rest arguments)
            (push (substring-no-properties
                   (apply #'format format-string arguments))
                  messages)
            (apply original format-string arguments)))
         incremental-start full-start result)
    (advice-add 'lsp--message :around observer)
    (unwind-protect
        (progn
          (lsp-java-build-project nil)
          (setq incremental-start
                (list
                 :status (substring-no-properties
                          (lsp--workspace-status-string workspace))
                 :face (get-text-property
                        0 'face (lsp--workspace-status-string workspace))))
          (neomacs-lsp-java-test-write-file
           (concat gate-root "-incremental") "continue\n")
          (neomacs-lsp-java-test-wait
           (lambda () (null (lsp--workspace-status-string workspace)))
           "the incremental build callback")

          (lsp-java-build-project '(4))
          (setq full-start
                (list
                 :status (substring-no-properties
                          (lsp--workspace-status-string workspace))
                 :face (get-text-property
                        0 'face (lsp--workspace-status-string workspace))))
          (neomacs-lsp-java-test-write-file
           (concat gate-root "-full") "continue\n")
          (neomacs-lsp-java-test-wait
           (lambda () (null (lsp--workspace-status-string workspace)))
           "the full build callback")
          (let ((builds
                 (neomacs-lsp-java-test-wire-messages-by-method
                  wire-log "java/buildWorkspace")))
            (setq result
                  (list
                   :incremental incremental-start
                   :full full-start
                   :messages
                   (seq-filter
                    (lambda (message)
                      (string-match-p "\\(?:Successfully build\\|Failed to build\\) project"
                                      message))
                    (nreverse messages))
                   :wire-params
                   (mapcar (lambda (message)
                             (neomacs-lsp-java-test-json message "params"))
                           builds)
                   :process-live
                   (not (null
                         (process-live-p
                          (lsp--workspace-proc workspace))))))))
      (advice-remove 'lsp--message observer))
    result))
"####;
    let expected = expect![[
        r##"OK (:incremental (:status "Building project..." :face success) :full (:status "Building project..." :face success) :messages ("LSP :: Successfully build project." "LSP :: Failed to build project.") :wire-params (:json-false t) :process-live t)"##
    ]];
    ParityBatchCase::value(
        "incremental_and_full_project_builds_report_real_async_lifecycle",
        elisp_form,
        expected,
    )
}

fn build_files_notify_jdt_while_source_files_reject_configuration_updates() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lsp-java-test-with-project "project-configuration"
  (let* ((source-error
          (condition-case error
              (progn (lsp-java-update-project-configuration) nil)
            (error (list (car error) (error-message-string error)))))
         (pom-buffer (find-file-noselect pom-file)))
    (with-current-buffer pom-buffer
      (lsp-java-update-project-configuration))
    (neomacs-lsp-java-test-wait
     (lambda ()
       (neomacs-lsp-java-test-wire-messages-by-method
        wire-log "java/projectConfigurationUpdate"))
     "the project configuration notification")
    (let* ((notification
            (car (neomacs-lsp-java-test-wire-messages-by-method
                  wire-log "java/projectConfigurationUpdate")))
           (params (neomacs-lsp-java-test-json notification "params")))
      (list
       :source-error source-error
       :process-live
       (not (null
             (process-live-p
              (lsp--workspace-proc workspace))))
       :build-buffer
       (list
        :file (file-relative-name (buffer-file-name pom-buffer) project-root)
        :modified (buffer-modified-p pom-buffer))
       :notification
       (list
        :method (neomacs-lsp-java-test-json notification "method")
        :uri (neomacs-lsp-java-test-normalize-uri
              (neomacs-lsp-java-test-json params "uri") project-root))))))
"####;
    let expected = expect![[
        r##"OK (:source-error (error "Update configuration could be called only from build file(pom.xml or gradle build file)") :process-live t :build-buffer (:file "pom.xml" :modified nil) :notification (:method "java/projectConfigurationUpdate" :uri "pom.xml"))"##
    ]];
    ParityBatchCase::value(
        "build_files_notify_jdt_while_source_files_reject_configuration_updates",
        elisp_form,
        expected,
    )
}

fn organize_imports_and_generate_to_string_apply_real_workspace_edits() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lsp-java-test-with-project "source-actions"
  (goto-char (point-min))
  (search-forward "DeploymentService")
  (let ((original (buffer-substring-no-properties (point-min) (point-max)))
        prompt-observations
        (prompt-call 0))
    (lsp-java-organize-imports)
    (neomacs-lsp-java-test-wait
     (lambda ()
       (not (string-match-p "import java.util.List"
                            (buffer-substring-no-properties
                             (point-min) (point-max)))))
     "the package-owned organize-imports callback to apply its edit")
    (let ((organized (buffer-substring-no-properties (point-min) (point-max)))
          (organized-point (point)))
      (cl-letf
          (((symbol-function 'lsp--completing-read)
            (lambda (prompt collection transform &rest _arguments)
              (push (list prompt (mapcar transform collection))
                    prompt-observations)
              (prog1 (and (= prompt-call 0) (car collection))
                (setq prompt-call (1+ prompt-call))))))
        (lsp-java-generate-to-string))
      (let* ((generated (buffer-substring-no-properties (point-min) (point-max)))
             (action-requests
              (neomacs-lsp-java-test-wire-messages-by-method
               wire-log "textDocument/codeAction"))
             (generate-request
              (car (neomacs-lsp-java-test-wire-messages-by-method
                    wire-log "java/generateToString")))
             (generate-params
              (neomacs-lsp-java-test-json generate-request "params")))
        (list
         :original original
         :organized (list :text organized :point organized-point)
         :generated
         (list :text generated
               :point (point)
               :modified (buffer-modified-p)
               :mode major-mode)
         :action-requests
         (mapcar
          (lambda (message)
            (let* ((params (neomacs-lsp-java-test-json message "params"))
                   (context (neomacs-lsp-java-test-json params "context")))
              (list
               :kind (car (neomacs-lsp-java-test-json context "only"))
               :context
               (neomacs-lsp-java-test-context-summary params project-root))))
          action-requests)
         :handler-requests
         (mapcar
          (lambda (method)
            (let* ((messages
                    (neomacs-lsp-java-test-wire-messages-by-method
                     wire-log method))
                   (params
                    (neomacs-lsp-java-test-json (car messages) "params"))
                   (context
                    (if (equal method "java/generateToString")
                        (neomacs-lsp-java-test-json params "context")
                      params)))
              (list
               :method method
               :count (length messages)
               :context
               (neomacs-lsp-java-test-context-summary
                context project-root))))
          '("java/organizeImports"
            "java/checkToStringStatus"
            "java/generateToString"))
         :field-prompt (nreverse prompt-observations)
         :selected-fields
         (mapcar
          (lambda (field)
            (list
             (neomacs-lsp-java-test-json field "name")
             (neomacs-lsp-java-test-json field "type")))
          (neomacs-lsp-java-test-json generate-params "fields"))
         :generated-context
         (neomacs-lsp-java-test-context-summary
          (neomacs-lsp-java-test-json generate-params "context")
          project-root))))))
"####;
    let expected = expect![[
        r##"OK (:original "package example;\n\nimport java.util.List;\n\npublic class DeploymentService {\n    private String release;\n    private String region;\n\n    public String deploy(String release) {\n        return \"ready:\" + release;\n    }\n}\n" :organized (:text "package example;\n\npublic class DeploymentService {\n    private String release;\n    private String region;\n\n    public String deploy(String release) {\n        return \"ready:\" + release;\n    }\n}\n" :point 49) :generated (:text "package example;\n\npublic class DeploymentService {\n    private String release;\n    private String region;\n\n    public String deploy(String release) {\n        return \"ready:\" + release;\n    }\n\n    @Override\n    public String toString() {\n        return \"DeploymentService{region='\" + region + \"'}\";\n    }\n}\n" :point 49 :modified t :mode java-mode) :action-requests ((:kind "source.organizeImports" :context (:uri "src/main/java/example/DeploymentService.java" :range ((4 30) (4 30)))) (:kind "source.generate.toString" :context (:uri "src/main/java/example/DeploymentService.java" :range ((2 30) (2 30))))) :handler-requests ((:method "java/organizeImports" :count 1 :context (:uri "src/main/java/example/DeploymentService.java" :range ((4 30) (4 30)))) (:method "java/checkToStringStatus" :count 1 :context (:uri "src/main/java/example/DeploymentService.java" :range ((2 30) (2 30)))) (:method "java/generateToString" :count 1 :context (:uri "src/main/java/example/DeploymentService.java" :range ((2 30) (2 30))))) :field-prompt (("Select fields to include (selected 2): " ("release: String ✓" "region: String ✓")) ("Select fields to include (selected 1): " ("release: String" "region: String ✓"))) :selected-fields (("region" "String")) :generated-context (:uri "src/main/java/example/DeploymentService.java" :range ((2 30) (2 30))))"##
    ]];
    ParityBatchCase::value(
        "organize_imports_and_generate_to_string_apply_real_workspace_edits",
        elisp_form,
        expected,
    )
}

fn opening_a_super_implementation_caches_class_source_and_preserves_xref_history() -> ParityBatchCase
{
    let elisp_form = r####"
(neomacs-lsp-java-test-with-project "super-implementation"
  (goto-char (point-min))
  (search-forward "DeploymentService")
  (let ((origin-buffer (current-buffer))
        (origin-point (point))
        (xref--history (xref--make-xref-history))
        (java-mode-hook (cons #'lsp java-mode-hook)))
    (lsp-java-open-super-implementation)
    (let* ((class-buffer (current-buffer))
           (class-file (buffer-file-name))
           (metadata-file
            (lsp-java--get-metadata-location class-file))
           (first-visit
            (list
             :file (file-relative-name class-file root)
             :text (buffer-substring-no-properties (point-min) (point-max))
             :line (line-number-at-pos)
             :column (current-column)
             :mode major-mode
             :read-only buffer-read-only
             :metadata
             (with-temp-buffer
               (insert-file-contents metadata-file)
               (buffer-string)))))
      (switch-to-buffer origin-buffer)
      (goto-char origin-point)
      (lsp-java-open-super-implementation)
      (let ((same-buffer (eq (current-buffer) class-buffer)))
        (xref-go-back)
        (let* ((find-links
                (neomacs-lsp-java-test-wire-messages-by-method
                 wire-log "java/findLinks"))
               (class-reads
                (neomacs-lsp-java-test-wire-messages-by-method
                 wire-log "java/classFileContents"))
               (first-link-params
                (neomacs-lsp-java-test-json (car find-links) "params"))
               (position-params
                (neomacs-lsp-java-test-json first-link-params "position")))
          (list
           :first-visit first-visit
           :cache-reused
           (list :same-buffer same-buffer
                 :find-link-requests (length find-links)
                 :class-content-requests (length class-reads))
           :request
           (list
            :type (neomacs-lsp-java-test-json first-link-params "type")
            :uri
            (neomacs-lsp-java-test-normalize-uri
             (neomacs-lsp-java-test-document-uri position-params)
             project-root)
            :position
            (neomacs-lsp-java-test-position
             (neomacs-lsp-java-test-json position-params "position")))
           :returned
           (list :same-buffer (eq (current-buffer) origin-buffer)
                 :point (point)
                 :line (line-number-at-pos)
                 :column (current-column))))))))
"####;
    let expected = expect![[
        r##"OK (:first-visit (:file "cache/java.lang.Object.java" :text "package java.lang;\npublic class Object {\n    public String toString() {\n        return getClass().getName();\n    }\n}\n" :line 2 :column 13 :mode java-mode :read-only t :metadata "jdt://contents/java.base/java/lang/Object.class?=java.base/java/lang/Object.class") :cache-reused (:same-buffer t :find-link-requests 2 :class-content-requests 1) :request (:type "superImplementation" :uri "src/main/java/example/DeploymentService.java" :position (4 30)) :returned (:same-buffer t :point 73 :line 5 :column 30))"##
    ]];
    ParityBatchCase::value(
        "opening_a_super_implementation_caches_class_source_and_preserves_xref_history",
        elisp_form,
        expected,
    )
}

fn type_hierarchy_expands_navigates_and_reports_a_missing_symbol() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lsp-java-test-with-project "type-hierarchy"
  (goto-char (point-min))
  (search-forward "DeploymentService")
  (let ((origin-buffer (current-buffer))
        initial expanded navigation missing)
    (lsp-java-type-hierarchy 2)
    (let ((hierarchy-buffer (current-buffer)))
      (setq initial
            (list
             :buffer (buffer-name)
             :text (buffer-substring-no-properties (point-min) (point-max))
             :mode major-mode
             :read-only buffer-read-only
             :selected (eq (window-buffer (selected-window)) hierarchy-buffer)
             :workspaces (length lsp--buffer-workspaces)))
      (goto-char (point-min))
      (search-forward "DeploymentService")
      (beginning-of-line)
      (treemacs-TAB-action)
      (neomacs-lsp-java-test-wait
       (lambda ()
         (and (string-match-p "RegionalDeploymentService"
                              (buffer-substring-no-properties
                               (point-min) (point-max)))
              (string-match-p "java.lang.Object"
                              (buffer-substring-no-properties
                               (point-min) (point-max)))))
       "both directions of the public type hierarchy")
      (let* ((text (buffer-substring-no-properties (point-min) (point-max)))
             (child-arrow (progn
                            (goto-char (point-min))
                            (search-forward "↓")
                            (1- (point))))
             (parent-arrow (progn
                             (goto-char (point-min))
                             (search-forward "↑")
                             (1- (point)))))
        (setq expanded
              (list :text text
                    :child-arrow-face (get-text-property child-arrow 'face)
                    :parent-arrow-face (get-text-property parent-arrow 'face))))
      (goto-char (point-min))
      (search-forward "RegionalDeploymentService")
      (beginning-of-line)
      (treemacs-RET-action)
      (setq navigation
            (list
             :file (file-relative-name (buffer-file-name) project-root)
             :text (buffer-substring-no-properties (point-min) (point-max))
             :line (line-number-at-pos)
             :column (current-column)
             :mode major-mode
             :same-hierarchy-buffer (eq (current-buffer) hierarchy-buffer))))
    (switch-to-buffer origin-buffer)
    (goto-char (point-min))
    (search-forward "DeploymentService")
    (neomacs-lsp-java-test-write-file
     (concat gate-root "-no-hierarchy") "missing\n")
    (setq missing
          (condition-case error
              (progn (lsp-java-type-hierarchy 2) nil)
            (user-error (list (car error) (error-message-string error)))))
    (let ((requests
           (neomacs-lsp-java-test-wire-messages-by-method
            wire-log "workspace/executeCommand")))
      (list
       :initial initial
       :expanded expanded
       :navigation navigation
       :missing missing
       :commands
       (mapcar
        (lambda (message)
          (let* ((params (neomacs-lsp-java-test-json message "params"))
                 (arguments (neomacs-lsp-java-test-json params "arguments"))
                 (payload
                  (json-parse-string
                   (car arguments)
                   :object-type 'alist
                   :array-type 'list
                   :null-object nil
                   :false-object :json-false))
                 (position (neomacs-lsp-java-test-json payload "position"))
                 (range (neomacs-lsp-java-test-json payload "range"))
                 (selection-range
                  (neomacs-lsp-java-test-json payload "selectionRange")))
            (list
             :command (neomacs-lsp-java-test-json params "command")
             :uri
             (neomacs-lsp-java-test-normalize-uri
              (if position
                  (neomacs-lsp-java-test-document-uri payload)
                (neomacs-lsp-java-test-json payload "uri"))
              project-root)
             :position
             (and position (neomacs-lsp-java-test-position position))
             :name (neomacs-lsp-java-test-json payload "name")
             :range (and range (neomacs-lsp-java-test-range range))
             :selection-range
             (and selection-range
                  (neomacs-lsp-java-test-range selection-range))
             :direction (nth 1 arguments)
             :resolve (nth 2 arguments))))
        requests)))))
"####;
    let expected = expect![[
        r##"OK (:initial (:buffer "*lsp-java-type-hierarchy*" :text "Hidden node\n▸   DeploymentService\n" :mode treemacs-mode :read-only t :selected t :workspaces 1) :expanded (:text "Hidden node\n▾   DeploymentService\n  ▸   RegionalDeploymentService ↓\n  ▸   java.lang.Object ↑\n" :child-arrow-face shadow :parent-arrow-face shadow) :navigation (:file "src/main/java/example/RegionalDeploymentService.java" :text "package example;\n\npublic class RegionalDeploymentService extends DeploymentService {\n    public String region() {\n        return \"north\";\n    }\n}\n" :line 3 :column 13 :mode java-mode :same-hierarchy-buffer nil) :missing (user-error "No class under point.") :commands ((:command "java.navigate.openTypeHierarchy" :uri "src/main/java/example/DeploymentService.java" :position (4 30) :name nil :range nil :selection-range nil :direction "2" :resolve "0") (:command "java.navigate.resolveTypeHierarchy" :uri "src/main/java/example/DeploymentService.java" :position nil :name "DeploymentService" :range ((4 0) (11 1)) :selection-range ((4 13) (4 30)) :direction "2" :resolve "1") (:command "java.navigate.openTypeHierarchy" :uri "src/main/java/example/DeploymentService.java" :position (4 30) :name nil :range nil :selection-range nil :direction "2" :resolve "0")))"##
    ]];
    ParityBatchCase::value(
        "type_hierarchy_expands_navigates_and_reports_a_missing_symbol",
        elisp_form,
        expected,
    )
}

fn java_test_browser_discovers_navigates_and_launches_a_junit_method() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-lsp-java-test-with-project "java-test-browser"
  (require 'lsp-jt)
  (let* ((test-id "deployment-service@example.DeploymentServiceTest#deploysRelease")
        (lsp-jt-results (ht))
        status-events
        (finished-count 0)
        (status-observer
         (lambda ()
           (let ((status (plist-get (gethash test-id lsp-jt-results) :status)))
             (push status status-events)
             (when (eq status :pass)
               (neomacs-lsp-java-test-write-file
                (concat gate-root "-junit-observed") "pass\n")))))
        (lsp-jt-status-updated-hook (list status-observer))
        (lsp-jt-test-run-finished-hook
         (list (lambda () (setq finished-count (1+ finished-count)))))
        (lsp-jt--refresh-timer nil)
        (dap-auto-configure-features nil)
        (dap-breakpoints-file (expand-file-name "dap-breakpoints" root))
        idle-timers-before cleanup-timer-count
        browser-buffer report-buffer initial expanded run-binding navigation report result)
    (unwind-protect
        (progn
          (lsp-jt-browser)
          (setq browser-buffer (current-buffer))
          (setq initial
                (list
                 :buffer (buffer-name)
                 :text (buffer-substring-no-properties (point-min) (point-max))
                 :mode major-mode
                 :minor-mode lsp-jt-mode
                 :read-only buffer-read-only
                 :selected (eq (window-buffer (selected-window)) browser-buffer)))

          (goto-char (point-min))
          (search-forward "deployment-service")
          (beginning-of-line)
          (treemacs-TAB-action)
          (neomacs-lsp-java-test-wait
           (lambda ()
             (string-match-p "example"
                             (buffer-substring-no-properties
                              (point-min) (point-max))))
           "the Java test package")
          (goto-char (point-min))
          (search-forward "example")
          (beginning-of-line)
          (treemacs-TAB-action)
          (neomacs-lsp-java-test-wait
           (lambda ()
             (string-match-p "DeploymentServiceTest"
                             (buffer-substring-no-properties
                              (point-min) (point-max))))
           "the Java test class")
          (goto-char (point-min))
          (search-forward "DeploymentServiceTest")
          (beginning-of-line)
          (treemacs-TAB-action)
          (neomacs-lsp-java-test-wait
           (lambda ()
             (string-match-p "deploysRelease()"
                             (buffer-substring-no-properties
                              (point-min) (point-max))))
           "the Java test method")
          (setq expanded
                (list
                 :text (buffer-substring-no-properties (point-min) (point-max))))

          (goto-char (point-min))
          (search-forward "deploysRelease")
          (backward-char 3)
          (setq run-binding (key-binding (kbd "x")))
          (setq idle-timers-before (copy-sequence timer-idle-list))
          (call-interactively run-binding)
          (neomacs-lsp-java-test-wait
           (lambda ()
             (and
              (eq (plist-get (gethash test-id lsp-jt-results) :status)
                  :pass)
              (eq (dap--debug-session-state (dap--cur-session))
                  'terminated)
              (neomacs-lsp-java-test-new-idle-timers
               idle-timers-before)))
           "the real DAP session, JUnit result, and scheduled cleanup")
          (when lsp-jt--refresh-timer
            (cancel-timer lsp-jt--refresh-timer)
            (setq lsp-jt--refresh-timer nil))
          (let ((cleanup-timers
                 (neomacs-lsp-java-test-new-idle-timers
                  idle-timers-before)))
            (setq cleanup-timer-count (length cleanup-timers))
            (unless (= cleanup-timer-count 2)
              (error "Expected exactly two Java-test cleanup timers, got %S"
                     cleanup-timers))
            (dolist (timer cleanup-timers)
              (timer-event-handler timer)))
          (neomacs-lsp-java-test-wait
           (lambda () (= finished-count 2))
           "the package-owned Java test cleanup hook")

          (lsp-jt-report-open)
          (setq report-buffer (current-buffer))
          (let* ((raw-report
                  (buffer-substring-no-properties (point-min) (point-max)))
                 (duration
                  (plist-get (gethash test-id lsp-jt-results) :duration))
                 (rendered-duration (format "%0.2fs" duration)))
            (setq report
                  (list
                   :buffer (buffer-name)
                   :text
                   (replace-regexp-in-string
                    "[0-9]+\\.[0-9][0-9]s" "[DURATION]" raw-report)
                   :mode major-mode
                   :read-only buffer-read-only
                   :duration-number (numberp duration)
                   :duration-nonnegative (>= duration 0)
                   :duration-rendered
                   (not (null (string-match-p
                               (regexp-quote rendered-duration)
                               raw-report))))))

          (pop-to-buffer browser-buffer)
          (goto-char (point-min))
          (search-forward "deploysRelease")
          (backward-char 3)
          (treemacs-RET-action)
          (let ((target-buffer (window-buffer (selected-window)))
                (caller-buffer (current-buffer)))
            (setq navigation
                  (with-current-buffer target-buffer
                    (list
                     :buffer (buffer-name)
                     :file (file-relative-name (buffer-file-name) project-root)
                     :text (buffer-substring-no-properties (point-min) (point-max))
                     :line (line-number-at-pos (window-point (selected-window)))
                     :column
                     (save-excursion
                       (goto-char (window-point (selected-window)))
                       (current-column))
                     :mode major-mode
                     :selected (eq (window-buffer (selected-window))
                                   target-buffer)
                     :caller-current-buffer (buffer-name caller-buffer)))))

          (let* ((execute-requests
                  (neomacs-lsp-java-test-wire-messages-by-method
                   wire-log "workspace/executeCommand"))
                 (requests
                  (seq-filter
                   (lambda (message)
                     (let* ((params
                             (neomacs-lsp-java-test-json message "params"))
                            (command
                             (neomacs-lsp-java-test-json params "command")))
                       (string-prefix-p "vscode.java.test." command)))
                   execute-requests))
                 (queries
                  (mapcar
                   (lambda (message)
                     (let* ((params
                             (neomacs-lsp-java-test-json message "params"))
                            (arguments
                             (neomacs-lsp-java-test-json params "arguments")))
                       (list
                        :command
                        (neomacs-lsp-java-test-json params "command")
                        :query
                        (json-parse-string
                         (car arguments)
                         :object-type 'alist
                         :array-type 'list
                         :null-object nil
                         :false-object :json-false))))
                   requests))
                 (junit (plist-get (car (last queries)) :query))
                 (dap-messages (neomacs-lsp-java-test-read-wire-log dap-log))
                 (dap-launch
                  (seq-find
                   (lambda (message)
                     (equal (neomacs-lsp-java-test-json message "command")
                            "launch"))
                   dap-messages))
                 (launch-arguments
                  (neomacs-lsp-java-test-json dap-launch "arguments"))
                 (start (neomacs-lsp-java-test-json junit "start"))
                 (end (neomacs-lsp-java-test-json junit "end")))
            (setq result
                  (list
                   :initial initial
                   :expanded expanded
                   :run
                   (list
                    :binding run-binding
                    :status-events (nreverse status-events)
                    :final-status
                    (plist-get (gethash test-id lsp-jt-results) :status)
                    :finished-count finished-count
                    :cleanup-timer-count cleanup-timer-count
                    :dap-state (dap--debug-session-state (dap--cur-session))
                    :dap-commands
                    (mapcar
                     (lambda (message)
                       (neomacs-lsp-java-test-json message "command"))
                     dap-messages)
                    :launch
                    (list
                     :type (neomacs-lsp-java-test-json launch-arguments "type")
                     :request (neomacs-lsp-java-test-json launch-arguments "request")
                     :main-class
                     (neomacs-lsp-java-test-json launch-arguments "mainClass")
                     :project
                     (neomacs-lsp-java-test-json launch-arguments "projectName")
                     :no-debug
                     (neomacs-lsp-java-test-json launch-arguments "noDebug")
                     :cwd
                     (file-name-nondirectory
                      (directory-file-name
                       (neomacs-lsp-java-test-json launch-arguments "cwd")))
                     :class-paths
                     (mapcar
                      (lambda (path) (file-relative-name path project-root))
                      (neomacs-lsp-java-test-json launch-arguments "classPaths"))
                     :args
                     (replace-regexp-in-string
                      "-port [0-9]+" "-port [PORT]"
                      (neomacs-lsp-java-test-json launch-arguments "args"))))
                   :report report
                   :navigation navigation
                   :search-queries
                   (mapcar
                    (lambda (entry)
                      (let ((query (plist-get entry :query)))
                        (list
                         (plist-get entry :command)
                         (neomacs-lsp-java-test-json query "level")
                         (neomacs-lsp-java-test-json query "fullName"))))
                    (butlast queries))
                   :junit-query
                   (list
                    :uri (neomacs-lsp-java-test-normalize-uri
                          (neomacs-lsp-java-test-json junit "uri") project-root)
                    :class (neomacs-lsp-java-test-json junit "classFullName")
                    :method (neomacs-lsp-java-test-json junit "testName")
                    :scope (neomacs-lsp-java-test-json junit "scope")
                    :kind (neomacs-lsp-java-test-json junit "testKind")
                    :project (neomacs-lsp-java-test-json junit "project")
                    :start (list (neomacs-lsp-java-test-json start "line")
                                 (neomacs-lsp-java-test-json start "character"))
                    :end (list (neomacs-lsp-java-test-json end "line")
                               (neomacs-lsp-java-test-json end "character"))))))
          result)
      (when lsp-jt--refresh-timer
        (cancel-timer lsp-jt--refresh-timer))
      (when (buffer-live-p report-buffer)
        (kill-buffer report-buffer))
      (when (buffer-live-p browser-buffer)
        (kill-buffer browser-buffer)))))
"####;
    let expected = expect![[
        r##"OK (:initial (:buffer "*Java Tests*" :text "Hidden node\n▸ deployment-service\n" :mode treemacs-mode :minor-mode t :read-only t :selected t) :expanded (:text "Hidden node\n▾ deployment-service\n  ▾   example\n    ▾   DeploymentServiceTest\n          deploysRelease()\n") :run (:binding lsp-jt-run :status-events (:pending :running :pass) :final-status :pass :finished-count 2 :cleanup-timer-count 2 :dap-state terminated :dap-commands ("initialize" "launch") :launch (:type "java" :request "launch" :main-class "org.eclipse.jdt.internal.junit.runner.RemoteTestRunner" :project "deployment-service" :no-debug t :cwd "deployment-service" :class-paths ("target/test-classes" "target/classes") :args "-version 3 -port [PORT] -test example.DeploymentServiceTest#deploysRelease()")) :report (:buffer "*Java Tests Results*" :text "Hidden node\n    deploysRelease example.DeploymentServiceTest     [DURATION]\n" :mode treemacs-mode :read-only t :duration-number t :duration-nonnegative t :duration-rendered t) :navigation (:buffer "DeploymentServiceTest.java" :file "src/test/java/example/DeploymentServiceTest.java" :text "package example;\n\npublic class DeploymentServiceTest {\n    public void deploysRelease() {\n        String result = new DeploymentService().deploy(\"release-42\");\n        if (!result.equals(\"ready:release-42\")) throw new AssertionError(result);\n    }\n}\n" :line 4 :column 16 :mode java-mode :selected t :caller-current-buffer "*Java Tests*") :search-queries (("vscode.java.test.search.items" 1 nil) ("vscode.java.test.search.items" 2 "example") ("vscode.java.test.search.items" 3 "example.DeploymentServiceTest")) :junit-query (:uri "src/test/java/example/DeploymentServiceTest.java" :class "example.DeploymentServiceTest" :method "deploysRelease()" :scope 4 :kind 1 :project "deployment-service" :start (3 16) :end (3 30)))"##
    ]];
    ParityBatchCase::value(
        "java_test_browser_discovers_navigates_and_launches_a_junit_method",
        elisp_form,
        expected,
    )
    .fresh_process()
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        opening_a_maven_java_project_negotiates_jdt_capabilities_over_real_stdio(),
        incremental_and_full_project_builds_report_real_async_lifecycle(),
        build_files_notify_jdt_while_source_files_reject_configuration_updates(),
        organize_imports_and_generate_to_string_apply_real_workspace_edits(),
        opening_a_super_implementation_caches_class_source_and_preserves_xref_history(),
        type_hierarchy_expands_navigates_and_reports_a_missing_symbol(),
        java_test_browser_discovers_navigates_and_launches_a_junit_method(),
    ]
}

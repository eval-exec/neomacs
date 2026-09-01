use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DAP_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(setq max-lisp-eval-depth 10000)
(require 'cl-lib)
(require 'dap-mode)
(require 'dap-launch)
(require 'dap-utils)
(require 'dap-variables)

(defun neomacs-dap-test-normalize (value)
  "Turn protocol hash tables and vectors into deterministic Lisp data."
  (cond
   ((hash-table-p value)
    (sort
     (let (entries)
       (maphash
        (lambda (key item)
          (push (cons key (neomacs-dap-test-normalize item)) entries))
        value)
       entries)
     (lambda (left right) (string< (format "%s" (car left))
                                  (format "%s" (car right))))))
   ((vectorp value) (mapcar #'neomacs-dap-test-normalize value))
   ((and (consp value) (not (listp (cdr value))))
    (cons (neomacs-dap-test-normalize (car value))
          (neomacs-dap-test-normalize (cdr value))))
   ((consp value) (mapcar #'neomacs-dap-test-normalize value))
   (t value)))
"####;

fn protocol_parser_reassembles_fragmented_unicode_and_pipelined_events() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((parser (make-dap--parser))
       (messages
        (list
         (dap--make-message
          '(:seq 1 :type "event" :event "output"
            :body (:category "stdout" :output "building λ…\n")))
         (dap--make-message
          '(:seq 2 :type "event" :event "stopped"
            :body (:reason "breakpoint" :threadId 7)))
         (dap--make-message
          '(:seq 3 :type "response" :request_seq 9 :success t
            :command "threads" :body (:threads [(:id 7 :name "主线程")])))))
       (wire (apply #'concat messages))
       (cuts '(1 9 27 41 64 83 109 137 171 211 257 10000))
       (offset 0)
       decoded parser-states)
  (dolist (cut cuts)
    (when (< offset (length wire))
      (let ((end (min (length wire) cut)))
        (when (> end offset)
          (setq decoded
                (nconc decoded (dap--parser-read parser (substring wire offset end))))
          (push (list :offset end
                      :reading-body (dap--parser-reading-body parser)
                      :received (dap--parser-body-received parser)
                      :leftovers (length (or (dap--parser-leftovers parser) "")))
                parser-states)
          (setq offset end)))))
  (when (< offset (length wire))
    (setq decoded (nconc decoded (dap--parser-read parser (substring wire offset)))))
  (list :wire-bytes (string-bytes wire)
        :messages (mapcar (lambda (json)
                            (neomacs-dap-test-normalize (dap--read-json json)))
                          decoded)
        :states (nreverse parser-states)
        :final (list :reading-body (dap--parser-reading-body parser)
                     :leftovers (dap--parser-leftovers parser))))
"####;
    let expected = expect![[
        r#"OK (:wire-bytes 378 :messages ((("body" ("category" . "stdout") ("output" . "building λ…\n")) ("event" . "output") ("seq" . 1) ("type" . "event")) (("body" ("reason" . "breakpoint") ("threadId" . 7)) ("event" . "stopped") ("seq" . 2) ("type" . "event")) (("body" ("threads" (("id" . 7) ("name" . "主线程")))) ("command" . "threads") ("request_seq" . 9) ("seq" . 3) ("success" . t) ("type" . "response"))) :states ((:offset 1 :reading-body nil :received 0 :leftovers 1) (:offset 9 :reading-body nil :received 0 :leftovers 9) (:offset 27 :reading-body t :received 5 :leftovers 0) (:offset 41 :reading-body t :received 19 :leftovers 0) (:offset 64 :reading-body t :received 42 :leftovers 0) (:offset 83 :reading-body t :received 61 :leftovers 0) (:offset 109 :reading-body t :received 87 :leftovers 0) (:offset 137 :reading-body nil :received nil :leftovers 20) (:offset 171 :reading-body t :received 32 :leftovers 0) (:offset 211 :reading-body t :received 72 :leftovers 0) (:offset 257 :reading-body t :received 9 :leftovers 0) (:offset 369 :reading-body nil :received nil :leftovers 0)) :final (:reading-body nil :leftovers ""))"#
    ]];
    ParityBatchCase::value(
        "protocol_parser_reassembles_fragmented_unicode_and_pipelined_events",
        elisp_form,
        expected,
    )
}

fn request_response_builders_round_trip_over_the_real_dap_wire_format() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((request (dap--make-request
                 "setBreakpoints"
                 '(:source (:path "/workspace/src/main.rs")
                   :breakpoints [(:line 7 :condition "attempts > 2")
                                 (:line 19 :logMessage "job={job_id}")])))
       (success (dap--make-success-response
                 41 "setBreakpoints"
                 '(:breakpoints [(:id 11 :verified t :line 7)
                                 (:id 12 :verified :json-false :line 19)])))
       (failure (dap--make-error-response
                 42 "evaluate" nil "variable is unavailable"))
       (initialize (dap--initialize-message "neomacs-test-adapter")))
  (mapcar
   (lambda (message)
     (let* ((wire (dap--make-message message))
            (separator (string-match "\r\n\r\n" wire))
            (header (substring wire 0 separator))
            (body (substring wire (+ separator 4))))
       (list :header header
             :declared (string-to-number (cadr (split-string header ": ")))
             :actual (string-bytes body)
             :decoded (neomacs-dap-test-normalize (dap--read-json body)))))
   (list request success failure initialize)))
"####;
    let expected = expect![[
        r#"OK ((:header "Content-Length: 196" :declared 196 :actual 196 :decoded (("arguments" ("breakpoints" (("condition" . "attempts > 2") ("line" . 7)) (("line" . 19) ("logMessage" . "job={job_id}"))) ("source" ("path" . "/workspace/src/main.rs"))) ("command" . "setBreakpoints") ("type" . "request"))) (:header "Content-Length: 175" :declared 175 :actual 175 :decoded (("body" ("breakpoints" (("id" . 11) ("line" . 7) ("verified" . t)) (("id" . 12) ("line" . 19) ("verified")))) ("command" . "setBreakpoints") ("request_seq" . 41) ("success" . t) ("type" . "response"))) (:header "Content-Length: 108" :declared 108 :actual 108 :decoded (("command" . "evaluate") ("message" . "variable is unavailable") ("request_seq" . 42) ("success") ("type" . "response"))) (:header "Content-Length: 319" :declared 319 :actual 319 :decoded (("arguments" ("adapterID" . "neomacs-test-adapter") ("clientID" . "vscode") ("clientName" . "Visual Studio Code") ("columnsStartAt1" . t) ("linesStartAt1" . t) ("locale" . "en-us") ("pathFormat" . "path") ("supportsRunInTerminalRequest" . t) ("supportsVariablePaging" . t) ("supportsVariableType" . t)) ("command" . "initialize") ("type" . "request"))))"#
    ]];
    ParityBatchCase::value(
        "request_response_builders_round_trip_over_the_real_dap_wire_format",
        elisp_form,
        expected,
    )
}

fn breakpoints_follow_live_edits_and_persist_conditions_without_markers() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (make-temp-file "dap-breakpoints-" t))
       (source (expand-file-name "worker.rs" root))
       (dap-breakpoints-file (expand-file-name "state/breakpoints.el" root))
       (metadata (make-hash-table :test 'equal))
       (hook-count 0)
       buffer)
  (unwind-protect
      (cl-letf (((symbol-function 'lsp-workspace-get-metadata)
                 (lambda (key) (gethash key metadata)))
                ((symbol-function 'lsp-workspace-set-metadata)
                 (lambda (key value) (puthash key value metadata))))
        (with-temp-file source
          (insert "fn run() {\n    prepare();\n    execute();\n    publish();\n}\n"))
        (setq buffer (find-file-noselect source))
        (with-current-buffer buffer
          (add-hook 'dap-breakpoints-changed-hook
                    (lambda () (setq hook-count (1+ hook-count))) nil t)
          (goto-char (point-min))
          (forward-line 1)
          (dap-breakpoint-toggle)
          (forward-line 2)
          (dap-breakpoint-add)
          (goto-char (point-min))
          (let ((inhibit-read-only t)) (insert "// generated header\n"))
          (let* ((breakpoints (gethash source (dap--get-breakpoints)))
                 (first (car (sort (copy-sequence breakpoints)
                                   (lambda (a b) (< (dap-breakpoint-get-point a)
                                                    (dap-breakpoint-get-point b))))))
                 (answers '("attempts > 2" "5" "publishing {job_id}")))
            (cl-letf (((symbol-function 'read-string)
                       (lambda (&rest _) (pop answers))))
              (dap-breakpoint-condition source first)
              (dap-breakpoint-hit-condition source first)
              (dap-breakpoint-log-message source first))
            (let ((live
                   (mapcar
                    (lambda (breakpoint)
                      (list :line (line-number-at-pos
                                   (dap-breakpoint-get-point breakpoint))
                            :point (dap-breakpoint-get-point breakpoint)
                            :condition (plist-get breakpoint :condition)
                            :hit (plist-get breakpoint :hit-condition)
                            :log (plist-get breakpoint :log-message)
                            :marker (markerp (plist-get breakpoint :marker))))
                    (sort (copy-sequence (gethash source (dap--get-breakpoints)))
                          (lambda (a b) (< (dap-breakpoint-get-point a)
                                           (dap-breakpoint-get-point b)))))))
              (dap--persist-breakpoints (dap--get-breakpoints))
              (list :text (buffer-substring-no-properties (point-min) (point-max))
                    :live live
                    :hooks hook-count
                    :persisted
                    (let ((table (with-temp-buffer
                                   (insert-file-contents dap-breakpoints-file)
                                   (read (current-buffer)))))
                      (mapcar
                       (lambda (entry)
                         (cons (file-name-nondirectory (car entry)) (cdr entry)))
                       (neomacs-dap-test-normalize table))))))))
    (when (buffer-live-p buffer) (kill-buffer buffer))
    (delete-directory root t)))
"####;
    let expected = expect![[
        r#"OK (:text "// generated header\nfn run() {\n    prepare();\n    execute();\n    publish();\n}\n" :live ((:line 3 :point 32 :condition "attempts > 2" :hit "5" :log "publishing {job_id}" :marker t) (:line 5 :point 62 :condition nil :hit nil :log nil :marker t)) :hooks 5 :persisted (("worker.rs" (:point 62) (:condition "attempts > 2" :hit-condition "5" :log-message "publishing {job_id}" :point 32))))"#
    ]];
    ParityBatchCase::value(
        "breakpoints_follow_live_edits_and_persist_conditions_without_markers",
        elisp_form,
        expected,
    )
}

fn templates_preserve_explicit_names_and_quote_argument_lists_when_edited() -> ParityBatchCase {
    let elisp_form = r####"
(let ((dap-debug-template-configurations nil)
      (dap--debug-providers (make-hash-table :test 'equal))
      (current-prefix-arg nil))
  (dap-register-debug-provider
   "rust-test"
   (lambda (config) (plist-put (copy-sequence config) :adapter "prepared")))
  (dap-register-debug-template
   "Release tests"
   '(:type "rust-test" :request "launch" :program "target/test-bin"
     :args ("--exact" "suite::ships λ") :env (("MODE" . "ci"))))
  (dap-register-debug-template
   "Attach worker"
   '(:name nil :type "rust-test" :request "attach" :processId 42))
  (let ((original (copy-tree dap-debug-template-configurations))
        selected edited)
    (cl-letf (((symbol-function 'completing-read)
               (lambda (&rest _) "Release tests")))
      (setq selected (dap--select-template nil)))
    (dap-debug-edit-template (cdr (assoc "Release tests" original)))
    (setq edited
          (with-current-buffer "*DAP Templates*"
            (prog1 (buffer-substring-no-properties (point-min) (point-max))
              (eval-buffer))))
    (list :original original
          :selected selected
          :edited edited
          :registered-after-eval dap-debug-template-configurations)))
"####;
    let expected = expect![[
        r#"OK (:original (("Attach worker" :name nil :type "rust-test" :request "attach" :processId 42) ("Release tests" :name "Release tests" :type "rust-test" :request "launch" :program "target/test-bin" :args ("--exact" "suite::ships λ") :env (("MODE" . "ci")))) :selected (:name "Release tests" :type "rust-test" :request "launch" :program "target/test-bin" :args ("--exact" "suite::ships λ") :env (("MODE" . "ci")) :adapter "prepared") :edited ";; Eval Buffer with `M-x eval-buffer' to register the newly created template.\n\n(dap-register-debug-template\n  \"Release tests\"\n  (list :name \"Release tests\"\n        :type \"rust-test\"\n        :request \"launch\"\n        :program \"target/test-bin\"\n        :args '(\"--exact\" \"suite::ships λ\")\n        :env '((\"MODE\" . \"ci\"))))" :registered-after-eval (("Release tests" :name "Release tests" :type "rust-test" :request "launch" :program "target/test-bin" :args ("--exact" "suite::ships λ") :env (("MODE" . "ci"))) ("Attach worker" :name nil :type "rust-test" :request "attach" :processId 42)))"#
    ]];
    ParityBatchCase::value(
        "templates_preserve_explicit_names_and_quote_argument_lists_when_edited",
        elisp_form,
        expected,
    )
}

fn vscode_variables_expand_a_real_project_file_selection_and_environment() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (make-temp-file "dap-project-" t))
       (source (expand-file-name "src/release worker.rs" root))
       (process-environment (cons "DEPLOY_ENV=staging" process-environment))
       buffer)
  (unwind-protect
      (progn
        (make-directory (file-name-directory source) t)
        (with-temp-file source (insert "fn ship() { deploy(); }\n"))
        (setq buffer (find-file-noselect source))
        (with-current-buffer buffer
          (goto-char (point-min))
          (search-forward "deploy")
          (set-mark (- (point) 6))
          (activate-mark)
          (let* ((dap-variables-project-root-function
                  (lambda () (directory-file-name root)))
                 (config
                  '(:program "${workspaceFolder}/target/release/app"
                    :cwd "${workspaceFolder}"
                    :file "${file}"
                    :relative "${relativeFile}"
                    :basename "${fileBasename}"
                    :stem "${fileBasenameNoExtension}"
                    :selected "${selectedText}"
                    :environment "${env:DEPLOY_ENV}"
                    :unknown "${extension:future}"))
                 (expanded (dap-variables-expand config)))
            (mapcar
             (lambda (item)
               (if (and (stringp item) (string-match-p (regexp-quote root) item))
                   (replace-regexp-in-string (regexp-quote root) "<PROJECT>" item t t)
                 item))
             expanded))))
    (when (buffer-live-p buffer) (kill-buffer buffer))
    (delete-directory root t)))
"####;
    let expected = expect![[
        r#"OK (:program "<PROJECT>/target/release/app" :cwd "<PROJECT>" :file "<PROJECT>/src/release worker.rs" :relative "src/release worker.rs" :basename "release worker.rs" :stem "release worker" :selected "deploy" :environment "staging" :unknown "${extension:future}")"#
    ]];
    ParityBatchCase::value(
        "vscode_variables_expand_a_real_project_file_selection_and_environment",
        elisp_form,
        expected,
    )
}

fn commented_launch_json_yields_runnable_configs_and_environment_pairs() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (make-temp-file "dap-launch-json-" t))
       (vscode (expand-file-name ".vscode" root))
       (launch-file (expand-file-name "launch.json" vscode)))
  (unwind-protect
      (progn
        (make-directory vscode t)
        (with-temp-file launch-file
          (insert
           "{\n"
           "  // Team launch profiles allow JSON5 comments.\n"
           "  \"version\": \"0.2.0\",\n"
           "  \"configurations\": [\n"
           "    {\"name\": \"Run worker\", \"type\": \"rust\",\n"
           "     \"request\": \"launch\", \"args\": [\"--jobs\", \"4\",],\n"
           "     \"environment\": [{\"name\": \"MODE\", \"value\": \"ci\"},],},\n"
           "    {\"name\": \"Attach worker\", \"type\": \"rust\",\n"
           "     \"request\": \"attach\", \"processId\": 42,},\n"
           "  ],\n"
           "}\n"))
        (cl-letf (((symbol-function 'lsp-workspace-root) (lambda () root)))
          (let ((raw (dap-launch-get-launch-json)))
            (list :raw (neomacs-dap-test-normalize raw)
                  :configs (neomacs-dap-test-normalize
                            (dap-launch-parse-launch-json raw))))))
    (delete-directory root t)))
"####;
    let expected = expect![[
        r#"OK (:raw (:version "0.2.0" :configurations ((:name "Run worker" :type "rust" :request "launch" :args ("--jobs" "4") :environment ((:name "MODE" :value "ci"))) (:name "Attach worker" :type "rust" :request "attach" :processId 42))) :configs (("Run worker" :name "Run worker" :type "rust" :request "launch" :args ("--jobs" "4") :environment ((:name "MODE" :value "ci")) :environment-variables (("MODE" . "ci"))) ("Attach worker" :name "Attach worker" :type "rust" :request "attach" :processId 42)))"#
    ]];
    ParityBatchCase::value(
        "commented_launch_json_yields_runnable_configs_and_environment_pairs",
        elisp_form,
        expected,
    )
}

fn concurrent_sessions_get_stable_names_and_category_labeled_ansi_output() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((first (make-dap--debug-session :name "worker"))
       (second (make-dap--debug-session :name "worker<1>"))
       (output-name "*dap-release out*")
       (session (make-dap--debug-session
                 :name "worker<2>" :output-buffer output-name
                 :output-displayed t))
       (dap-label-output-buffer-category t)
       (dap-auto-show-output nil))
  (unwind-protect
      (progn
        (dap--create-output-buffer "dap-release")
        (dap--print-to-output-buffer
         session
         (dap--output-buffer-format-with-category
          "stdout" "\x1b[32mbuild complete\x1b[0m\n"))
        (dap--print-to-output-buffer
         session
         (dap--output-buffer-format-with-category
          "stderr" "retry budget exhausted"))
        (list :names
              (list
               (list :requested "worker"
                     :resolved (dap--calculate-unique-name "worker" (list first second)))
               (list :requested "worker<9>"
                     :resolved (dap--calculate-unique-name "worker<9>"
                                                          (list first second))))
              :output
              (with-current-buffer output-name
                (buffer-substring-no-properties (point-min) (point-max)))
              :mode (with-current-buffer output-name major-mode)
              :displayed (dap--debug-session-output-displayed session)))
    (when (get-buffer output-name) (kill-buffer output-name))))
"####;
    let expected = expect![[
        r#"OK (:names ((:requested "worker" :resolved "worker<2>") (:requested "worker<9>" :resolved "worker<2>")) :output "stdout: build complete\nstderr: retry budget exhausted\n" :mode special-mode :displayed t)"#
    ]];
    ParityBatchCase::value(
        "concurrent_sessions_get_stable_names_and_category_labeled_ansi_output",
        elisp_form,
        expected,
    )
}

fn dap_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DAP_MODE_MELPA_PIN, "dap-mode.el")
        .expect("prepare pinned DAP Mode and its exact dependencies below ./tmp")
        .with_timeout(Duration::from_secs(420))
        .with_prelude(PRELUDE)
}

#[test]
fn dap_mode_practical_workflows_batch() {
    let cases = vec![
        protocol_parser_reassembles_fragmented_unicode_and_pipelined_events(),
        request_response_builders_round_trip_over_the_real_dap_wire_format(),
        breakpoints_follow_live_edits_and_persist_conditions_without_markers(),
        templates_preserve_explicit_names_and_quote_argument_lists_when_edited(),
        vscode_variables_expand_a_real_project_file_selection_and_environment(),
        commented_launch_json_yields_runnable_configs_and_environment_pairs(),
        concurrent_sessions_get_stable_names_and_category_labeled_ansi_output(),
    ];
    let thread = std::thread::current();
    let test_name = thread
        .name()
        .unwrap_or("dap-mode practical workflow parity batch");
    assert_oracle_batch_cases(dap_mode_oracle(), test_name, "dap-mode parity", &cases);
}

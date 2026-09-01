use expect_test::expect;

use super::ParityBatchCase;

fn mcp_initialize_and_builtin_tools_expose_stable_protocol_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "mcp_initialize_and_builtin_tools_expose_stable_protocol_contract",
        r##"
(let ((ai-code-mcp-server-tools nil)
      (ai-code-mcp-server-tool-setup-functions nil))
  (let* ((initialize (ai-code-mcp-dispatch "initialize"))
         (tools (ai-code-mcp-dispatch "tools/list"))
         (entries (alist-get 'tools tools)))
    (list
     (alist-get 'protocolVersion initialize)
     (alist-get 'serverInfo initialize)
     (sort (mapcar (lambda (tool) (alist-get 'name tool)) entries)
           #'string<)
     (mapcar
      (lambda (tool)
        (let ((schema (alist-get 'inputSchema tool)))
          (list (alist-get 'name tool)
                (alist-get 'type schema)
                (append (alist-get 'required schema) nil))))
      entries))))
"##,
        expect![[
            r#"OK ("2024-11-05" ((name . "ai-code-mcp-tools") (version . "0.1.0")) ("buffer_query" "diagnostics_baseline" "editor_state" "get_diagnostics" "get_project_buffers" "get_project_files" "imenu_list_symbols" "notify_user" "project_info" "treesit_info" "visible_buffers" "xref_find_definitions_at_point" "xref_find_references") (("project_info" "object" nil) ("editor_state" "object" nil) ("visible_buffers" "object" nil) ("buffer_query" "object" ("buffer_name")) ("get_diagnostics" "object" nil) ("diagnostics_baseline" "object" nil) ("get_project_files" "object" nil) ("get_project_buffers" "object" nil) ("notify_user" "object" ("message_text")) ("imenu_list_symbols" "object" ("file_path")) ("xref_find_references" "object" ("identifier" "file_path")) ("xref_find_definitions_at_point" "object" ("file_path" "line" "column")) ("treesit_info" "object" ("file_path"))))"#
        ]],
    )
}

fn mcp_custom_tool_schema_and_call_roundtrip_required_and_optional_arguments() -> ParityBatchCase {
    ParityBatchCase::value(
        "mcp_custom_tool_schema_and_call_roundtrip_required_and_optional_arguments",
        r##"
(let ((ai-code-mcp-server-tools nil)
      (ai-code-mcp-server-tool-setup-functions nil))
  (ai-code-mcp-make-tool
   :function
   (lambda (service retries dry-run)
     (format "deploy:%s retries=%s dry-run=%s" service retries dry-run))
   :name "prepare_deploy"
   :description "Prepare a deterministic deployment plan."
   :category "workflow"
   :args '((:name "service" :type string :description "Service name.")
           (:name "retries" :type number :description "Retry budget.")
           (:name "dry-run" :type boolean :optional t)))
  (let* ((tool
          (car (alist-get 'tools (ai-code-mcp-dispatch "tools/list"))))
         (schema (alist-get 'inputSchema tool))
         (result
          (ai-code-mcp-dispatch
           "tools/call"
           '((name . "prepare_deploy")
             (arguments . ((service . "billing")
                           (retries . 3)
                           (dry-run . t)))))))
    (list tool
          (append (alist-get 'required schema) nil)
          (alist-get 'text (car (alist-get 'content result))))))
"##,
        expect![[
            r#"OK (((name . "prepare_deploy") (description . "Prepare a deterministic deployment plan.") (inputSchema (type . "object") (properties (service (type . "string") (description . "Service name.")) (retries (type . "number") (description . "Retry budget.")) (dry-run (type . "boolean"))) (required . ["service" "retries"]))) ("service" "retries") "deploy:billing retries=3 dry-run=t")"#
        ]],
    )
}

fn mcp_validation_reports_missing_argument_and_unknown_method_precisely() -> ParityBatchCase {
    ParityBatchCase::value(
        "mcp_validation_reports_missing_argument_and_unknown_method_precisely",
        r##"
(let ((ai-code-mcp-server-tools nil))
  (ai-code-mcp-make-tool
   :function #'identity
   :name "echo"
   :description "Echo required text."
   :args '((:name "text" :type string)))
  (mapcar
   (lambda (thunk)
     (condition-case err
         (funcall thunk)
       (error (list (car err) (error-message-string err)))))
   (list
    (lambda ()
      (ai-code-mcp-dispatch
       "tools/call" '((name . "echo") (arguments . ()))))
    (lambda () (ai-code-mcp-dispatch "resources/list"))
    (lambda () (ai-code-mcp-make-tool :name "broken"
                                       :description "No function")))))
"##,
        expect![[
            r#"OK ((error "Missing required argument: text") (error "Unknown MCP method: resources/list") (error "Tool :function is required"))"#
        ]],
    )
}

fn mcp_session_context_drives_project_directory_and_active_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "mcp_session_context_drives_project_directory_and_active_buffer",
        r##"
(let ((ai-code-mcp--sessions (make-hash-table :test 'equal))
      (root (make-temp-file "ai-code-mcp-session-" t))
      (buffer (generate-new-buffer "*ai-code-mcp-work*")))
  (unwind-protect
      (progn
        (with-current-buffer buffer
          (insert "alpha\nbeta\ngamma\n"))
        (ai-code-mcp-register-session "session-7" root buffer)
        (let ((ai-code-mcp--current-session-id "session-7"))
          (ai-code-mcp-with-session-context nil
            (list
             (eq (current-buffer) buffer)
             (equal default-directory
                    (file-name-as-directory root))
             (buffer-substring-no-properties (point-min) (point-max))
             (equal
              (plist-get
               (ai-code-mcp-get-session-context "session-7")
               :project-dir)
              root)))))
    (when (buffer-live-p buffer) (kill-buffer buffer))
    (delete-directory root t)))
"##,
        expect![[r#"OK (t t "alpha\nbeta\ngamma\n" t)"#]],
    )
}

fn mcp_buffer_query_preserves_ranges_and_trailing_whitespace() -> ParityBatchCase {
    ParityBatchCase::value(
        "mcp_buffer_query_preserves_ranges_and_trailing_whitespace",
        r##"
(let ((buffer (generate-new-buffer "ai-code-mcp-query.txt")))
  (unwind-protect
      (progn
        (with-current-buffer buffer
          (insert "first  \nsecond\t\nthird\nfourth"))
        (list
         (ai-code-mcp-buffer-query (buffer-name buffer))
         (ai-code-mcp-buffer-query (buffer-name buffer) 2 2)
         (condition-case err
             (ai-code-mcp-buffer-query (buffer-name buffer) 0 2)
           (error (error-message-string err)))
         (condition-case err
             (ai-code-mcp-buffer-query (buffer-name buffer) 2 0)
           (error (error-message-string err)))))
    (kill-buffer buffer)))
"##,
        expect![[
            r#"OK ("first  \nsecond\11\nthird\nfourth" "second\11\nthird" "Arguments start_line and num_lines must be positive integers" "Arguments start_line and num_lines must be positive integers")"#
        ]],
    )
}

fn mcp_project_files_skip_hidden_metadata_and_return_relative_paths() -> ParityBatchCase {
    ParityBatchCase::value(
        "mcp_project_files_skip_hidden_metadata_and_return_relative_paths",
        r##"
(let ((root (make-temp-file "ai-code-mcp-files-" t))
      (ai-code-mcp--sessions (make-hash-table :test 'equal))
      (buffer (generate-new-buffer "*ai-code-mcp-project*")))
  (unwind-protect
      (progn
        (make-directory (expand-file-name "src/" root))
        (make-directory (expand-file-name ".git/" root))
        (make-directory (expand-file-name ".cache/" root))
        (dolist (file '("README.md" "src/lib.rs" "src/api.rs"
                        ".git/config" ".cache/index"))
          (with-temp-file (expand-file-name file root) (insert file)))
        (ai-code-mcp-register-session "project" root buffer)
        (let ((ai-code-mcp--current-session-id "project"))
          (sort (ai-code-mcp-get-project-files) #'string<)))
    (when (buffer-live-p buffer) (kill-buffer buffer))
    (delete-directory root t)))
"##,
        expect![[r#"OK ("README.md" "src/api.rs" "src/lib.rs")"#]],
    )
}

fn mcp_uri_helpers_canonicalize_spaces_localhost_and_external_paths() -> ParityBatchCase {
    ParityBatchCase::value(
        "mcp_uri_helpers_canonicalize_spaces_localhost_and_external_paths",
        r##"
(let* ((path "/workspace/payment service/src/api.el")
       (uri (ai-code-mcp--file-path-to-uri path)))
  (list
   uri
   (ai-code-mcp--uri-to-file-path uri)
   (ai-code-mcp--uri-to-file-path
    "file://localhost/workspace/payment%20service/src/api.el")
   (ai-code-mcp--local-file-uri-path
    "file:///workspace/payment%20service/src/api.el")
   (ai-code-mcp--display-path "/external/shared/types.el")))
"##,
        expect![[
            r#"OK ("file:///workspace/payment%20service/src/api.el" "/workspace/payment service/src/api.el" "/workspace/payment service/src/api.el" "/workspace/payment service/src/api.el" "/external/shared/types.el")"#
        ]],
    )
}

pub(super) fn mcp_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        mcp_initialize_and_builtin_tools_expose_stable_protocol_contract(),
        mcp_custom_tool_schema_and_call_roundtrip_required_and_optional_arguments(),
        mcp_validation_reports_missing_argument_and_unknown_method_precisely(),
        mcp_session_context_drives_project_directory_and_active_buffer(),
        mcp_buffer_query_preserves_ranges_and_trailing_whitespace(),
        mcp_project_files_skip_hidden_metadata_and_return_relative_paths(),
        mcp_uri_helpers_canonicalize_spaces_localhost_and_external_paths(),
    ]
}

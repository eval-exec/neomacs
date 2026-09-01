use expect_test::expect;

use super::ParityBatchCase;

fn viewport_request_protocol_decodes_versioned_regular_and_staging_payloads() -> ParityBatchCase {
    ParityBatchCase::value(
        "viewport_request_protocol_decodes_versioned_regular_and_staging_payloads",
        r##"
(progn
  (require 'ai-code-editor-viewport)
  (let ((encode
         (lambda (fields)
           (base64-encode-string
            (concat (mapconcat #'identity fields "\0") "\0")
            t))))
    (list
     (ai-code-editor-viewport--decode-request
      (funcall
       encode
       (list "/status/one" "/repo" "0"
             ai-code-editor-viewport--request-version
             "regular" "+42:7" "src/lib.rs")))
     (ai-code-editor-viewport--decode-request
      (funcall
       encode
       (list "/status/two" "/repo" "1"
             ai-code-editor-viewport--request-version
             "staging" "--" "-literal-name.txt"))))))
"##,
        expect![[
            r#"OK ((:status-file "/status/one" :directory "/repo" :submit-p nil :staging-request-p nil :arguments ("+42:7" "src/lib.rs")) (:status-file "/status/two" :directory "/repo" :submit-p t :staging-request-p t :arguments ("--" "-literal-name.txt")))"#
        ]],
    )
}

fn viewport_request_protocol_validates_malformed_intent_kind_and_field_count() -> ParityBatchCase {
    ParityBatchCase::value(
        "viewport_request_protocol_validates_malformed_intent_kind_and_field_count",
        r##"
(progn
  (require 'ai-code-editor-viewport)
  (let ((encode
         (lambda (fields)
           (base64-encode-string
            (concat (mapconcat #'identity fields "\0") "\0")
            t))))
    (mapcar
     (lambda (fields)
       (condition-case err
           (ai-code-editor-viewport--decode-request
            (funcall encode fields))
         (error (list (car err) (error-message-string err)))))
     (list
      '("/status" "/repo" "2" "legacy")
      (list "/status" "/repo" "0"
            ai-code-editor-viewport--request-version "unknown")
      '("/status" "/repo" "0")))))
"##,
        expect![[
            r#"OK ((error "Invalid AI Code editor submit intent") (error "Invalid AI Code editor request kind") (error "Malformed AI Code editor request"))"#
        ]],
    )
}

fn viewport_file_arguments_apply_positions_once_and_respect_double_dash() -> ParityBatchCase {
    ParityBatchCase::value(
        "viewport_file_arguments_apply_positions_once_and_respect_double_dash",
        r##"
(progn
  (require 'ai-code-editor-viewport)
  (mapcar
   (lambda (request)
     (list
      (file-name-nondirectory (plist-get request :file))
      (plist-get request :line)
      (plist-get request :column)))
   (ai-code-editor-viewport--parse-file-arguments
    "/workspace/project/"
    '("--wait" "+12:4" "src/api.el" "README.md"
      "+7" "src/model.el" "--" "+literal.el" "-draft.txt"))))
"##,
        expect![[
            r#"OK (("api.el" 12 4) ("README.md" nil nil) ("model.el" 7 nil) ("+literal.el" nil nil) ("-draft.txt" nil nil))"#
        ]],
    )
}

fn viewport_status_file_is_scoped_and_writes_submit_token_atomically() -> ParityBatchCase {
    ParityBatchCase::value(
        "viewport_status_file_is_scoped_and_writes_submit_token_atomically",
        r##"
(progn
  (require 'ai-code-editor-viewport)
  (let* ((root (make-temp-file "ai-code-viewport-status-" t))
         (status (expand-file-name "ai-code-editor-status-42" root))
         (outside (make-temp-file "ai-code-editor-status-outside-"))
         (ai-code-editor-viewport--helper-status-directory root))
    (unwind-protect
        (progn
          (with-temp-file status)
          (list
           (ai-code-editor-viewport--valid-status-file-p status)
           (ai-code-editor-viewport--valid-status-file-p outside)
           (progn
             (ai-code-editor-viewport--write-status
              status 0 "submit-7")
             (with-temp-buffer
               (insert-file-contents status)
               (buffer-string)))
           (condition-case err
               (ai-code-editor-viewport--write-status
                status 0 "invalid token")
             (error (error-message-string err)))))
      (delete-directory root t)
      (delete-file outside))))
"##,
        expect![[r#"OK (t nil "0 1 submit-7\n" "Invalid AI Code editor submit token")"#]],
    )
}

fn viewport_attachment_references_are_repo_relative_and_external_absolute() -> ParityBatchCase {
    ParityBatchCase::value(
        "viewport_attachment_references_are_repo_relative_and_external_absolute",
        r##"
(progn
  (require 'ai-code-editor-viewport)
  (require 'ai-code-editor-viewport-attachments)
  (let* ((root (make-temp-file "ai-code-viewport-files-" t))
         (inside (expand-file-name "notes/design.txt" root))
         (outside (make-temp-file "ai-code-viewport-external-" nil ".txt")))
    (unwind-protect
        (progn
          (make-directory (file-name-directory inside) t)
          (with-temp-file inside (insert "design"))
          (with-temp-buffer
            (setq-local ai-code-editor-viewport--source-directory root)
            (let ((arguments
                   (ai-code-editor-viewport--handler-arguments
                    '("--output" "%s" "--format=%s") inside)))
              (list
               (ai-code-editor-viewport--file-reference inside)
               (equal
                (ai-code-editor-viewport--file-reference outside)
                (concat "@" outside))
               (mapcar
                (lambda (argument)
                  (string-replace root "$ROOT/" argument))
                arguments)))))
      (delete-directory root t)
      (delete-file outside))))
"##,
        expect![[
            r#"OK ("@notes/design.txt" t ("--output" "$ROOT//notes/design.txt" "--format=$ROOT//notes/design.txt"))"#
        ]],
    )
}

fn viewport_attachment_serialization_spaces_adjacent_images_without_mutation() -> ParityBatchCase {
    ParityBatchCase::value(
        "viewport_attachment_serialization_spaces_adjacent_images_without_mutation",
        r##"
(progn
  (require 'ai-code-editor-viewport)
  (require 'ai-code-editor-viewport-attachments)
  (with-temp-buffer
    (insert "Compare")
    (let ((start (point)))
      (insert "@first.png")
      (add-text-properties
       start (point)
       '(ai-code-editor-viewport-image first
         ai-code-editor-viewport-file "/repo/first.png")))
    (insert "with")
    (let ((start (point)))
      (insert "@second.png")
      (add-text-properties
       start (point)
       '(ai-code-editor-viewport-image second
         ai-code-editor-viewport-file "/repo/second.png")))
    (insert "now")
    (let ((before (buffer-string)))
      (list
       (ai-code-editor-viewport-attachments-serialize-buffer
        (current-buffer))
       before
       (buffer-string)))))
"##,
        expect![[
            r#"OK ("Compare @first.png with @second.png now" #("Compare@first.pngwith@second.pngnow" 7 17 (ai-code-editor-viewport-file #1="/repo/first.png" ai-code-editor-viewport-image first) 21 32 (ai-code-editor-viewport-file #2="/repo/second.png" ai-code-editor-viewport-image second)) #("Compare@first.pngwith@second.pngnow" 7 17 (ai-code-editor-viewport-file #1# ai-code-editor-viewport-image first) 21 32 (ai-code-editor-viewport-file #2# ai-code-editor-viewport-image second)))"#
        ]],
    )
}

pub(super) fn viewport_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        viewport_request_protocol_decodes_versioned_regular_and_staging_payloads(),
        viewport_request_protocol_validates_malformed_intent_kind_and_field_count(),
        viewport_file_arguments_apply_positions_once_and_respect_double_dash(),
        viewport_status_file_is_scoped_and_writes_submit_token_atomically(),
        viewport_attachment_references_are_repo_relative_and_external_absolute(),
        viewport_attachment_serialization_spaces_adjacent_images_without_mutation(),
    ]
}

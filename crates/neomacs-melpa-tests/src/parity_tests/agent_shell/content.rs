use expect_test::expect;

use super::ParityBatchCase;

fn submitted_prompt_embeds_source_links_large_notes_and_encodes_an_image() -> ParityBatchCase {
    ParityBatchCase::value(
        "submitted_prompt_embeds_source_links_large_notes_and_encodes_an_image",
        r##"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "agent-shell-content-workflow"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "src/example.el" root))
       (notes (expand-file-name "docs/design notes.md" root))
       (image (expand-file-name "assets/pixel.png" root))
       (transcript (expand-file-name "conversation.md" root))
       (png
        (unibyte-string
         #x89 #x50 #x4e #x47 #x0d #x0a #x1a #x0a
         #x00 #x00 #x00 #x0d #x49 #x48 #x44 #x52
         #x00 #x00 #x00 #x01 #x00 #x00 #x00 #x01))
       (notifications
        '(((:direction . incoming)
           (:kind . notification)
           (:object
            (jsonrpc . "2.0")
            (method . "session/update")
            (params
             (sessionId . "parity-session")
             (update
              (sessionUpdate . "agent_message_chunk")
              (messageId . "content-answer")
              (content
               (type . "text")
               (text
                . "I compared the implementation, design notes, and image."))))))))
       (messages
        (neomacs-agent-shell-test-session-messages notifications))
       (agent-shell-cwd-function (lambda () root))
       (agent-shell-transcript-file-path-function (lambda () transcript))
       (agent-shell-show-welcome-message nil)
       (agent-shell-show-busy-indicator nil)
       (agent-shell-show-usage-at-turn-end nil)
       (agent-shell-embed-file-size-limit 64)
       (shell nil)
       (snapshot nil))
  (make-directory (file-name-directory source) t)
  (make-directory (file-name-directory notes) t)
  (make-directory (file-name-directory image) t)
  (make-directory (expand-file-name ".git" root) t)
  (with-temp-file source
    (insert "(defun answer () 42)\n"))
  (with-temp-file notes
    (insert
     "# Design notes\n\n"
     "The implementation must preserve GNU Emacs behavior across "
     "large prompts, quoted paths, and binary attachments.\n"))
  (let ((coding-system-for-write 'binary))
    (with-temp-file image
      (set-buffer-multibyte nil)
      (insert png)))
  (unwind-protect
      (progn
        (setq shell (neomacs-agent-shell-test-start messages))
        (with-current-buffer shell
          (shell-maker-submit
           :input
           "Compare @src/example.el with @\"docs/design notes.md\" and inspect @assets/pixel.png before recommending a fix.")
          (let* ((prompt-request
                  (seq-find
                   (lambda (request)
                     (equal (map-elt request :method)
                            "session/prompt"))
                   neomacs-agent-shell-test-sent-requests))
                 (blocks
                  (map-nested-elt prompt-request '(:params prompt)))
                 (image-block
                  (seq-find
                   (lambda (block)
                     (equal (map-elt block 'type) "image"))
                   blocks)))
            (setq
             snapshot
             (list
              blocks
              (and image-block
                   (equal
                    (base64-decode-string
                     (map-elt image-block 'data))
                    png))
              (mapcar
               (lambda (block)
                 (or
                  (map-elt block 'type)
                  (map-nested-elt block '(resource uri))))
               blocks)
              (neomacs-agent-shell-test-visible-buffer-string)
              (with-temp-buffer
                (insert-file-contents transcript)
                (neomacs-agent-shell-test-normalize-transcript
                 (buffer-string))))))))
    (neomacs-agent-shell-test-kill shell))
  snapshot)
"##,
        expect![[
            r##"OK ([(#1=(type . "text") (text . "Compare")) ((type . "resource") (resource (uri . "file://[ORACLE-SANDBOX]/agent-shell-content-workflow/src/example.el") (text . "(defun answer () 42)\n") (mimeType . "application/emacs-lisp"))) (#1# (text . " with")) ((type . "resource_link") (uri . "file://[ORACLE-SANDBOX]/agent-shell-content-workflow/docs/design notes.md") (name . "docs/design notes.md") (mimeType . "text/plain") (size . 128)) (#1# (text . " and inspect")) ((type . "image") (data . "iVBORw0KGgoAAAANSUhEUgAAAAEAAAAB") (mimeType . "image/png") (uri . "file://[ORACLE-SANDBOX]/agent-shell-content-workflow/assets/pixel.png")) ((type . "text") (text . " before recommending a fix."))] t ("text" "resource" "text" "resource_link" "text" "image" "text") "\n\n\n▶ [✓] Starting agent\n\n▶ Agent capabilities\n\n▶ Available config options\n\n▶ Available models\n\n  Available /commands\n\nParity> Compare @src/example.el with @\"docs/design notes.md\" and inspect @assets/pixel.png before recommending a fix.\n\n▶ 3 files attached\n\nI compared the implementation, design notes, and image.\n\nParity>" "# Agent Shell Transcript\n\n**Agent:** Parity\n**Started:** TIME\n**Working Directory:** [ORACLE-SANDBOX]/agent-shell-content-workflow/\n**Session ID:** parity-session\n\n---\n\n## User (TIME)\n\nCompare @src/example.el with @\"docs/design notes.md\" and inspect @assets/pixel.png before recommending a fix.\n\n\n## Agent (TIME)\n\nI compared the implementation, design notes, and image.\n\n")"##
        ]],
    )
    .fresh_process()
}

pub(super) fn content_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![submitted_prompt_embeds_source_links_large_notes_and_encodes_an_image()]
}

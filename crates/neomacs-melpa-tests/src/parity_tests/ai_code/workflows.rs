use expect_test::expect;

use super::ParityBatchCase;

fn the_editor_helper_the_package_installs_emits_frames_its_own_parser_decodes() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_editor_helper_the_package_installs_emits_frames_its_own_parser_decodes",
        r##"(progn
  (require 'ai-code-editor-viewport-transport)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (file-name-as-directory (expand-file-name "repo" sandbox)))
         (status-directory (file-name-as-directory (expand-file-name "status" sandbox)))
         (helper (expand-file-name "ai-code-editor-helper" sandbox))
         (default-directory root)
         ;; The helper wants the WHOLE frame prefix -- the OSC introducer,
         ;; the protocol name and the per-frame token.  Handing it the bare
         ;; token produces a frame that looks plausible and that the
         ;; package's parser silently finds nothing in.
         (frame-prefix (ai-code-editor-viewport--frame-prefix)))
    (make-directory root t)
    (make-directory status-directory t)
    (write-region "alpha\nbravo\ncharlie\n" nil
                  (expand-file-name "api.el" root) nil 'silent)
    (write-region (ai-code-editor-viewport--helper-content status-directory)
                  nil helper nil 'silent)
    (set-file-modes helper #o755)
    (cl-flet
        ((invoke (arguments environment)
           (let* ((collected "")
                  (process-environment (append environment process-environment))
                  (finished nil)
                  (process (make-process
                            :name "ai-code-editor-helper"
                            :command (cons helper arguments)
                            ;; A pty, because the helper writes to /dev/tty.
                            :connection-type 'pty
                            :noquery t
                            :filter (lambda (_process output)
                                      (setq collected (concat collected output)))
                            :sentinel (lambda (_process _event)
                                        (setq finished t))))
                  (rounds 0))
             ;; Two of the arms below are SUPPOSED to emit nothing, so
             ;; waiting for output would burn the whole budget on exactly
             ;; the cases that are working correctly.  Stop as soon as
             ;; either the frame has arrived or the helper has exited.
             (while (and (string-empty-p collected) (not finished) (< rounds 200))
               (accept-process-output nil 0.05)
               (setq rounds (1+ rounds)))
             (when (process-live-p process) (delete-process process))
             (let* ((parsed (ai-code-editor-viewport--parse-output "" collected))
                    (payloads (plist-get parsed :payloads)))
               (list :payloads (length payloads)
                     :fields
                     (when payloads
                       (mapcar
                        (lambda (field)
                          ;; The status file and the working directory are
                          ;; sandbox paths; keep their shape, drop the noise.
                          (cond
                           ((string-match-p "ai-code-editor-status-" field)
                            "[STATUS-FILE]")
                           ((string-prefix-p sandbox field)
                            (concat "[SANDBOX]/"
                                    (file-relative-name field sandbox)))
                           (t (copy-sequence field))))
                        (split-string
                         (base64-decode-string (car payloads)) "\0"))))))))
      (let ((prefix-entry
             (list (concat
                    ai-code-editor-viewport--frame-prefix-environment-variable
                    "=" frame-prefix))))
        (list
         ;; An ordinary open, carrying the +LINE:COL argument whose later
         ;; handling is divergence 38.
         :regular (invoke '("+2:3" "api.el") prefix-entry)
         ;; The helper's own two option flags, which it strips before
         ;; building the payload and reports as separate fields.
         :staging (invoke '("--ai-code-staging" "api.el") prefix-entry)
         :submit (invoke '("--ai-code-submit" "api.el") prefix-entry)
         ;; Without a frame prefix in the environment the helper must refuse
         ;; rather than emit an unaddressed frame.
         :without-frame-prefix (invoke '("api.el") nil)
         ;; And with no file arguments at all.
         :without-files (invoke '() prefix-entry))))))"##,
        expect![[
            r#"OK (:regular (:payloads 1 :fields ("[STATUS-FILE]" "[SANDBOX]/repo" "0" "ai-code-editor-viewport-v1" "regular" "+2:3" "api.el" "")) :staging (:payloads 1 :fields ("[STATUS-FILE]" "[SANDBOX]/repo" "0" "ai-code-editor-viewport-v1" "staging" "api.el" "")) :submit (:payloads 1 :fields ("[STATUS-FILE]" "[SANDBOX]/repo" "1" "ai-code-editor-viewport-v1" "regular" "api.el" "")) :without-frame-prefix (:payloads 0 :fields nil) :without-files (:payloads 0 :fields nil))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![the_editor_helper_the_package_installs_emits_frames_its_own_parser_decodes()]
}

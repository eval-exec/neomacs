use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, SLY_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const SLY_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const SLY_TEST_PRELUDE: &str = r#####"
(require 'cl-lib)
(require 'sly)
(sly-setup '(sly-mrepl))

(setq sly-log-events nil
      sly-mrepl-history-file-name
      (expand-file-name "sly-mrepl-history" temporary-file-directory))

(defun sly-parity-kill-buffers ()
  (dolist (buffer (buffer-list))
    (when (string-match-p "\\`\\*sly-" (buffer-name buffer))
      (with-current-buffer buffer
        (setq kill-buffer-query-functions nil
              sly-mrepl--dirty-history nil))
      (kill-buffer buffer))))

(sly-parity-kill-buffers)
"#####;

fn sly_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SLY_MELPA_PIN, "sly.el")
        .expect("prepare pinned SLY source below ./tmp")
        .with_prelude(SLY_TEST_PRELUDE)
        .with_timeout(SLY_TEST_TIMEOUT)
}

fn common_lisp_source_workflow_tracks_package_symbols_forms_and_editor_commands() -> ParityBatchCase
{
    let elisp_form = r#####"
(progn
  (with-temp-buffer
    (insert
     ";; A quoted example must not become the active package.\n"
     "(defparameter *example* \"(in-package :wrong)\")\n"
     "#| (in-package :also-wrong) |#\n"
     "(in-package #:release.pipeline)\n\n"
     "(defun deploy-release (artifact &key (environment :staging))\n"
     "  (labels ((announce (state)\n"
     "             (format nil \"~A:~A\" artifact state)))\n"
     "    (list :environment environment\n"
     "          :message (announce :ready))))\n")
    (lisp-mode)
    (sly-editing-mode 1)
    (goto-char (point-min))
    (search-forward "release.pipeline")
    (let ((package-in-declaration (sly-search-buffer-package)))
      (search-forward "deploy-release")
      (let ((function-symbol (sly-symbol-at-point))
            (package-in-definition (sly-current-package)))
        (search-forward "announce :ready")
        (let* ((nested-symbol (sly-symbol-at-point))
               (sexp (sly-sexp-at-point))
               (region (sly-region-for-defun-at-point)))
          (list
           :mode (list major-mode sly-mode sly-editing-mode)
           :package-in-declaration package-in-declaration
           :package-in-definition package-in-definition
           :symbols (list function-symbol nested-symbol)
           :sexp sexp
           :region region
           :definition (apply #'buffer-substring-no-properties region)
           :complete
           (list
            (sly-input-complete-p (car region) (cadr region))
            (sly-input-complete-p (car region) (- (cadr region) 2)))
           :keys
           (list
            (lookup-key sly-mode-map (kbd "C-M-x"))
            (lookup-key sly-editing-mode-map (kbd "C-c C-k"))
            (lookup-key sly-mode-map (kbd "M-.")))))))))
"#####;
    let expect = expect![[
        r####"OK (:mode (lisp-mode t t) :package-in-declaration nil :package-in-definition "#:release.pipeline" :symbols ("deploy-release" ":ready") :sexp ":ready" :region (168 384) :definition "(defun deploy-release (artifact &key (environment :staging))\n  (labels ((announce (state)\n             (format nil \"~A:~A\" artifact state)))\n    (list :environment environment\n          :message (announce :ready))))\n" :complete (t nil) :keys (sly-eval-defun sly-compile-and-load-file sly-edit-definition))"####
    ]];
    ParityBatchCase::value(
        "common_lisp_source_workflow_tracks_package_symbols_forms_and_editor_commands",
        elisp_form,
        expect,
    )
}

fn compile_defun_sends_complete_source_and_precise_coordinates_to_slynk() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (with-temp-buffer
    (rename-buffer "release-pipeline.lisp" t)
    (insert
     "(in-package :release.pipeline)\n\n"
     "(defun publish-artifact (artifact channel)\n"
     "  (check-type artifact pathname)\n"
     "  (format nil \"Published ~A to ~A\" artifact channel))\n")
    (lisp-mode)
    (goto-char (point-min))
    (search-forward "check-type")
    (let (rpc callback flashes)
      (cl-letf (((symbol-function 'sly-connection)
                 (lambda () 'release-connection))
                ((symbol-function 'sly-eval-async)
                 (lambda (form &optional continuation _package)
                   (setq rpc form callback continuation)
                   :queued))
                ((symbol-function 'sly-flash-region)
                 (lambda (start end &rest options)
                   (push (list start end options
                               (buffer-substring-no-properties start end))
                         flashes))))
        (sly-compile-defun)
        (list
         :rpc rpc
         :callback (functionp callback)
         :flashes (nreverse flashes)
         :policy sly-compilation-policy
         :point (point)
         :modified (buffer-modified-p))))))
"#####;
    let expect = expect![[
        r####"OK (:rpc (slynk:compile-string-for-emacs "(defun publish-artifact (artifact channel)\n  (check-type artifact pathname)\n  (format nil \"Published ~A to ~A\" artifact channel))\n" "release-pipeline.lisp" '((:position 33) (:line 3 1)) nil 'nil) :callback t :flashes ((33 163 nil "(defun publish-artifact (artifact channel)\n  (check-type artifact pathname)\n  (format nil \"Published ~A to ~A\" artifact channel))\n")) :policy nil :point 89 :modified t)"####
    ]];
    ParityBatchCase::value(
        "compile_defun_sends_complete_source_and_precise_coordinates_to_slynk",
        elisp_form,
        expect,
    )
}

fn slynk_transport_sanitizes_frames_unicode_and_dispatches_fragmented_replies() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (let* ((network-buffer (generate-new-buffer " *sly-parity-network*"))
         (connection
          (make-pipe-process
           :name "sly-parity-transport"
           :buffer network-buffer
           :command '("cat")
           :coding 'binary
           :noquery t))
         (expression (propertize "(+ 20 22)" 'face 'bold 'origin 'editor))
         sent continuation-value dispatches)
    (unwind-protect
        (cl-letf (((symbol-function 'process-send-string)
                   (lambda (_process wire) (setq sent wire)))
                  ((symbol-function 'sly--refresh-mode-line) #'ignore))
          (setf (sly-rex-continuations connection)
                (list (cons 73
                            (lambda (value)
                              (setq continuation-value value)))))
          (setf (sly-continuation-counter connection) 73)
          (sly-net-send
           `(:emacs-rex (slynk:interactive-eval ,expression)
                        "RELEASE.PIPELINE" t 74)
           connection)
          (let* ((unicode-result
                  (concat "d" (string #x00e9) "ploy" (string #x00e9)
                          " " (string #x2713)))
                 (reply `(:return (:ok ,unicode-result) 73))
                 (payload (encode-coding-string
                           (concat (sly-prin1-to-string reply) "\n")
                           'utf-8-unix))
                 (wire (concat (sly-net-encode-length (length payload)) payload)))
            (sly-net-filter connection (substring wire 0 4))
            (push (list :after-header-fragment
                        (with-current-buffer network-buffer (buffer-size))
                        (mapcar #'car (sly-rex-continuations connection)))
                  dispatches)
            (sly-net-filter connection (substring wire 4 13))
            (push (list :after-payload-fragment
                        (with-current-buffer network-buffer (buffer-size))
                        (mapcar #'car (sly-rex-continuations connection)))
                  dispatches)
            (sly-net-filter connection (substring wire 13)))
          (list
           :sanitized-properties (text-properties-at 0 expression)
           :outbound-header (substring sent 0 6)
           :outbound-length (length (substring sent 6))
           :outbound-form
           (decode-coding-string (substring sent 6) 'utf-8-unix)
           :fragment-states (nreverse dispatches)
           :continuation-value
           (list (car continuation-value)
                 (string-to-list (cadr continuation-value)))
           :remaining
           (with-current-buffer network-buffer
             (list (buffer-string)
                   (mapcar #'car (sly-rex-continuations connection))))))
      (when (process-live-p connection)
        (set-process-sentinel connection #'ignore)
        (delete-process connection))
      (kill-buffer network-buffer))))
"#####;
    let expect = expect![[
        r####"OK (:sanitized-properties nil :outbound-header "00004a" :outbound-length 74 :outbound-form "(:emacs-rex (slynk:interactive-eval \"(+ 20 22)\") \"RELEASE.PIPELINE\" t 74)\n" :fragment-states ((:after-header-fragment 4 (73)) (:after-payload-fragment 13 (73))) :continuation-value (:ok (100 233 112 108 111 121 233 32 10003)) :remaining ("" nil))"####
    ]];
    ParityBatchCase::value(
        "slynk_transport_sanitizes_frames_unicode_and_dispatches_fragmented_replies",
        elisp_form,
        expect,
    )
}

fn mrepl_session_preserves_transcript_results_buttons_and_deduplicated_history() -> ParityBatchCase
{
    let elisp_form = r#####"
(progn
  (let* ((buffer (generate-new-buffer "*sly-mrepl parity*"))
         (saved-kill-emacs-hook kill-emacs-hook)
         connection sent inspections)
    (unwind-protect
        (progn
          (make-comint-in-buffer "sly-parity" buffer "cat")
          (setq connection (get-buffer-process buffer))
          (set-process-query-on-exit-flag connection nil)
          (setf (sly-connection-name connection) "release")
          (cl-letf (((symbol-function 'sly-mrepl--send)
                     (lambda (message) (push message sent)))
                    ((symbol-function 'sly-eval-for-inspector)
                     (lambda (&rest arguments) (push arguments inspections))))
            (with-current-buffer buffer
              (sly-mrepl-mode)
              (setq sly-buffer-connection connection
                    sly-mrepl--remote-channel 41)
              (sly-mrepl--insert-prompt
               "RELEASE.PIPELINE" "REL" 0 91 nil)
              (insert "(publish-artifact #P\"build/app\" :production)")
              (sly-mrepl-return)
              (sly-mrepl--insert-output "uploading app…\n")
              (sly-mrepl--insert-results
               '(("42" 91) ("#P\"releases/app\"" 91)))
              (sly-mrepl--insert-prompt
               "RELEASE.PIPELINE" "REL" 0 92 nil)
              (insert "(status :production)")
              (sly-mrepl-return)
              (sly-mrepl--insert-results '(("READY" 92)))
              (sly-mrepl--insert-prompt
               "RELEASE.PIPELINE" "REL" 0 93 nil)
              (insert "(status :production)")
              (sly-mrepl-return)
              (sly-mrepl--insert-prompt
               "RELEASE.PIPELINE" "REL" 0 94 nil)
              (insert "pending input")
              (let* ((buttons (sly-button-buttons-in (point-min) (point-max)))
                     (first-result (car buttons))
                     (pending (buffer-substring-no-properties
                               (sly-mrepl--mark) (point-max))))
                (sly-button-inspect first-result)
                (setq last-command nil this-command nil)
                (sly-mrepl-previous-input-or-button 1)
                (list
                 :transcript
                 (buffer-substring-no-properties (point-min) (point-max))
                 :sent (nreverse sent)
                 :history (ring-elements comint-input-ring)
                 :pending-before-history pending
                 :current-input
                 (buffer-substring-no-properties
                  (sly-mrepl--mark) (point-max))
                 :buttons
                 (mapcar (lambda (button)
                           (list (button-label button)
                                 (button-type button)
                                 (button-get button 'part-args)))
                         buttons)
                 :inspections (nreverse inspections)
                 :markers
                 (list
                  (marker-position (process-mark connection))
                  (marker-position sly-mrepl--output-mark)
                  (overlay-start sly-mrepl--last-prompt-overlay)
                  (overlay-end sly-mrepl--last-prompt-overlay))
                 :fields
                 (list
                  (get-text-property (point-min) 'field)
                  (get-text-property
                   (button-start first-result) 'field)))))))
      (setq kill-emacs-hook saved-kill-emacs-hook)
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (setq sly-mrepl--dirty-history nil
                kill-buffer-query-functions nil)))
      (when (and connection (process-live-p connection))
        (set-process-sentinel connection #'ignore)
        (delete-process connection))
      (when (buffer-live-p buffer) (kill-buffer buffer)))))
"#####;
    let expect = expect![[
        r####"OK (:transcript "REL> (publish-artifact #P\"build/app\" :production)\nuploading app…\n42\n#P\"releases/app\"\nREL> (status :production)\nREADY\nREL> (status :production)\nREL> (status :production)" :sent ((:process "(publish-artifact #P\"build/app\" :production)") (:process "(status :production)") (:process "(status :production)")) :history ("(status :production)" "(publish-artifact #P\"build/app\" :production)") :pending-before-history "pending input" :current-input "(status :production)" :buttons (("42" sly-mrepl-part (91 0)) ("#P\"releases/app\"" sly-mrepl-part (91 1)) ("READY" sly-mrepl-part (92 0))) :inspections (((slynk-mrepl:inspect-entry 41 91 0) :inspector-name nil)) :markers (149 144 144 149) :fields (output output))"####
    ]];
    ParityBatchCase::value(
        "mrepl_session_preserves_transcript_results_buttons_and_deduplicated_history",
        elisp_form,
        expect,
    )
}

fn apropos_results_render_symbol_buttons_documentation_and_namespace_actions() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (sly-parity-kill-buffers)
  (let (requests)
    (cl-letf (((symbol-function 'sly-current-connection)
               (lambda () 'release-connection))
              ((symbol-function 'sly-connection-name)
               (lambda (&optional _connection) "release"))
              ((symbol-function 'display-buffer)
               (lambda (&rest _arguments) nil))
              ((symbol-function 'sly-eval-describe)
               (lambda (form &rest _arguments) (push form requests))))
      (sly-show-apropos
       '((:designator ("PUBLISH-ARTIFACT" "RELEASE.PIPELINE" t)
          :bounds ((0 7))
          :function "(artifact channel)"
          :setf :not-documented)
         (:designator ("*DEPLOYMENT-STATE*" "RELEASE.PIPELINE" t)
          :bounds ((1 11))
          :variable "Current immutable deployment state."))
       "release" "RELEASE.PIPELINE"
       "Apropos for release in RELEASE.PIPELINE")
      (let ((buffer (get-buffer (sly-buffer-name :apropos :connection t))))
        (with-current-buffer buffer
          (let* ((buttons (sly-button-buttons-in (point-min) (point-max)))
                 (symbol-buttons
                  (cl-remove-if-not
                   (lambda (button)
                     (button-type-subtype-p
                      (button-type button) 'sly-apropos-symbol))
                   buttons)))
            (sly-button-describe (car symbol-buttons))
            (prog1
                (list
                 :mode (list major-mode sly-mode buffer-read-only)
                 :header header-line-format
                 :body (buffer-substring-no-properties (point-min) (point-max))
                 :buttons
                 (mapcar
                  (lambda (button)
                    (list (button-label button)
                          (button-type button)
                          (button-get button 'part-args)
                          (button-get button 'apropos-label)))
                  buttons)
                 :requests (nreverse requests)
                 :package sly-buffer-package
                 :connection sly-buffer-connection)
              (kill-buffer buffer))))))))
"#####;
    let expect = expect![[
        r####"OK (:mode (sly-apropos-mode t t) :header "Apropos for release in RELEASE.PIPELINE" :body "RELEASE.PIPELINE:PUBLISH-ARTIFACT\n  Function: (artifact channel)\n  Setf: (not documented)\nRELEASE.PIPELINE:*DEPLOYMENT-STATE*\n  Variable: Current immutable deployment state.\n" :buttons (("RELEASE.PIPELINE:PUBLISH-ARTIFACT" sly-apropos-symbol ("RELEASE.PIPELINE:PUBLISH-ARTIFACT" nil) nil) ("RELEASE.PIPELINE:*DEPLOYMENT-STATE*" sly-apropos-symbol ("RELEASE.PIPELINE:*DEPLOYMENT-STATE*" nil) nil)) :requests ((slynk:describe-symbol "RELEASE.PIPELINE:PUBLISH-ARTIFACT")) :package "RELEASE.PIPELINE" :connection release-connection)"####
    ]];
    ParityBatchCase::value(
        "apropos_results_render_symbol_buttons_documentation_and_namespace_actions",
        elisp_form,
        expect,
    )
}

fn named_inspector_preserves_value_action_and_pagination_button_workflows() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (sly-parity-kill-buffers)
  (let (requests)
    (cl-letf (((symbol-function 'sly-current-connection)
               (lambda () 'release-connection))
              ((symbol-function 'sly-connection-name)
               (lambda (&optional _connection) "release"))
              ((symbol-function 'display-buffer)
               (lambda (&rest _arguments) nil))
              ((symbol-function 'sly-eval-for-inspector)
               (lambda (&rest arguments) (push arguments requests))))
      (sly--open-inspector
       '(:id 900
         :title "#<DEPLOYMENT release-2026.08>\n"
         :content
         (("Environment: " (:value ":PRODUCTION" 11) "\n"
           (:label "Artifact: ") (:value "#P\"releases/app\"" 12) "\n"
           (:action "Rollback deployment" 4) "\n")
          9 2 9))
       :inspector-name "release-audit"
       :switch nil)
      (let ((buffer
             (get-buffer
              (sly-buffer-name :inspector :connection t
                               :suffix "release-audit"))))
        (with-current-buffer buffer
          (let* ((buttons (sly-button-buttons-in (point-min) (point-max)))
                 (value-button
                  (cl-find-if
                   (lambda (button)
                     (equal (button-get button 'part-args) '(11)))
                   buttons))
                 (action-button
                  (cl-find-if
                   (lambda (button)
                     (string= (button-label button) "Rollback deployment"))
                   buttons)))
            (sly-button-inspect value-button)
            (button-activate action-button)
            (prog1
                (list
                 :mode (list major-mode sly-mode buffer-read-only)
                 :name sly--this-inspector-name
                 :body (buffer-substring-no-properties (point-min) (point-max))
                 :buttons
                 (mapcar
                  (lambda (button)
                    (list (button-label button)
                          (button-type button)
                          (button-get button 'part-args)
                          (button-get button 'range-args)))
                  buttons)
                 :requests
                 (mapcar
                  (lambda (request)
                    (mapcar (lambda (item)
                              (if (functionp item) :function item))
                            request))
                  (nreverse requests))
                 :keys
                 (list
                  (lookup-key sly-inspector-mode-map (kbd "n"))
                  (lookup-key sly-inspector-mode-map (kbd "l"))
                  (lookup-key sly-inspector-mode-map (kbd ">"))))
              (kill-buffer buffer))))))))
"#####;
    let expect = expect![[
        r####"OK (:mode (sly-inspector-mode t t) :name "release-audit" :body "#<DEPLOYMENT release-2026.08>\n--------------------\n [--more--]\nEnvironment: :PRODUCTION\nArtifact: #P\"releases/app\"\nRollback deployment\n" :buttons (("#<DEPLOYMENT release-2026.08>" sly-inspector-part (900) nil) (" [--more--]\n" sly-action nil (2 t)) (":PRODUCTION" sly-inspector-part (11) nil) ("#P\"releases/app\"" sly-inspector-part (12) nil) ("Rollback deployment" sly-action nil nil)) :requests (((slynk:inspect-nth-part 11) :inspector-name "release-audit") ((slynk::inspector-call-nth-action 4) :restore-point t)) :keys (sly-inspector-next sly-inspector-pop sly-inspector-fetch-all))"####
    ]];
    ParityBatchCase::value(
        "named_inspector_preserves_value_action_and_pagination_button_workflows",
        elisp_form,
        expect,
    )
}

fn debugger_presents_condition_restart_and_frame_button_workflows() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (sly-parity-kill-buffers)
  (let* ((network-buffer (generate-new-buffer " *sly-parity-debug-connection*"))
         (connection
          (make-pipe-process
           :name "sly-parity-debug-connection"
           :buffer network-buffer
           :command '("cat")
           :coding 'binary
           :noquery t))
         (sly-default-connection connection)
         event)
    (setf (sly-connection-name connection) "release")
    (unwind-protect
        (save-window-excursion
          (cl-letf (((symbol-function 'sly-db--display-debugger)
                     (lambda (_thread)
                       (set-window-buffer (selected-window) (current-buffer))
                       (selected-window)))
                    ((symbol-function 'sly-dispatch-event)
                     (lambda (rpc &optional process)
                       (setq event
                             (list
                              (list (nth 0 rpc)
                                    (nth 1 rpc)
                                    (nth 2 rpc)
                                    (nth 3 rpc)
                                    (functionp (nth 4 rpc)))
                              process)))))
            (sly-db-setup
             88 2
             '("Checksum mismatch for release-2026.08" "SIMPLE-ERROR" nil)
             '(("RETRY" "Re-download the immutable artifact")
               ("USE-CACHED" "Deploy the previously verified artifact")
               ("ABORT" "Abort this deployment"))
             '((0 "(VERIFY-ARTIFACT #P\"releases/app\")" (:restartable t))
               (1 "(PUBLISH-ARTIFACT #P\"releases/app\" :PRODUCTION)" nil)
               (2 "(SLYNK::EVAL-FOR-EMACS ...)" nil))
             '(73 74))
            (let ((buffer (sly-db-find-buffer 88 connection)))
              (with-current-buffer buffer
                (let* ((buttons (sly-button-buttons-in (point-min) (point-max)))
                       (restart-buttons
                        (cl-remove-if-not
                         (lambda (button)
                           (button-get button 'restart-number))
                         buttons))
                       (frame-buttons
                        (cl-remove-if-not
                         (lambda (button)
                           (button-type-subtype-p
                            (button-type button) 'sly-db-frame))
                         buttons)))
                  (button-activate (car restart-buttons))
                  (prog1
                      (list
                       :mode (list major-mode mode-name buffer-read-only)
                       :body
                       (buffer-substring-no-properties (point-min) (point-max))
                       :restarts
                       (mapcar
                        (lambda (button)
                          (list (button-label button)
                                (button-get button 'restart-number)))
                        restart-buttons)
                       :frames
                       (mapcar
                        (lambda (button)
                          (list (button-label button)
                                (button-get button 'frame-number)
                                (button-get button 'frame-string)
                                (button-get button 'part-args)))
                        frame-buttons)
                       :event event
                       :thread sly-current-thread
                       :level sly-db-level
                       :continuations sly-db-continuations
                       :keys
                       (list
                        (lookup-key sly-db-mode-map (kbd "c"))
                        (lookup-key sly-db-frame-map (kbd "RET")))
                       :markers
                       (list
                        (marker-position sly-db-restart-list-start-marker)
                        (marker-position sly-db-backtrace-start-marker)))
                    (setq kill-buffer-query-functions nil)
                    (kill-buffer buffer)))))))
      (when (process-live-p connection)
        (set-process-sentinel connection #'ignore)
        (delete-process connection))
      (when (buffer-live-p network-buffer) (kill-buffer network-buffer)))))
"#####;
    let expect = expect![[
        r####"OK (:mode (sly-db-mode "sly-db[2]" t) :body "Checksum mismatch for release-2026.08\nSIMPLE-ERROR\n\nRestarts:\n 0: [RETRY] Re-download the immutable artifact\n 1: [USE-CACHED] Deploy the previously verified artifact\n 2: [ABORT] Abort this deployment\n\nBacktrace:\n 0: (VERIFY-ARTIFACT #P\"releases/app\")\n 1: (PUBLISH-ARTIFACT #P\"releases/app\" :PRODUCTION)\n --more--\n" :restarts (("[RETRY]" 0) ("[USE-CACHED]" 1) ("[ABORT]" 2)) :frames (("(VERIFY-ARTIFACT #P\"releases/app\")" 0 "(VERIFY-ARTIFACT #P\"releases/app\")" (0 VERIFY-ARTIFACT)) ("(PUBLISH-ARTIFACT #P\"releases/app\" :PRODUCTION)" 1 "(PUBLISH-ARTIFACT #P\"releases/app\" :PRODUCTION)" (1 PUBLISH-ARTIFACT))) :event ((:emacs-rex (slynk:invoke-nth-restart-for-emacs 2 0) nil 88 t) nil) :thread 88 :level 2 :continuations (73 74) :keys (sly-db-continue sly-db-toggle-details) :markers (63 213))"####
    ]];
    ParityBatchCase::value(
        "debugger_presents_condition_restart_and_frame_button_workflows",
        elisp_form,
        expect,
    )
}

#[test]
fn sly_package_batch() {
    let cases = vec![
        common_lisp_source_workflow_tracks_package_symbols_forms_and_editor_commands(),
        compile_defun_sends_complete_source_and_precise_coordinates_to_slynk(),
        slynk_transport_sanitizes_frames_unicode_and_dispatches_fragmented_replies(),
        mrepl_session_preserves_transcript_results_buttons_and_deduplicated_history(),
        apropos_results_render_symbol_buttons_documentation_and_namespace_actions(),
        named_inspector_preserves_value_action_and_pagination_button_workflows(),
        debugger_presents_condition_restart_and_frame_button_workflows(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed SLY parity test");
    assert_oracle_batch_cases(sly_oracle(), test_name, "sly_parity", &cases);
}

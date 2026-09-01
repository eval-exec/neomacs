use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, MACROSTEP_MELPA_PIN, SLIME_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const SLIME_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const SLIME_TEST_PRELUDE: &str = r#####"
(require 'cl-lib)
(require 'slime)
(require 'slime-repl)

(setq slime-net-coding-system 'utf-8-unix
      slime-log-events nil
      slime-repl-history-file nil)

(defun slime-parity-kill-buffers ()
  (dolist (buffer (buffer-list))
    (when (string-match-p
           "\\`\\*\\(?:slime-\\|sldb \\|SLIME \\|slime-repl \\)"
           (buffer-name buffer))
      (kill-buffer buffer))))

(slime-parity-kill-buffers)
"#####;

fn slime_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SLIME_MELPA_PIN, "slime.el")
        .expect("prepare pinned SLIME source below ./tmp")
        .with_melpa_dependency(MACROSTEP_MELPA_PIN)
        .expect("prepare pinned macrostep dependency below ./tmp")
        .with_prelude(SLIME_TEST_PRELUDE)
        .with_timeout(SLIME_TEST_TIMEOUT)
}

fn common_lisp_source_workflow_tracks_package_symbols_forms_and_compile_region() -> ParityBatchCase
{
    let elisp_form = r#####"
(progn
  (with-temp-buffer
    (insert
     ";; The string below is deliberately not a package declaration.\n"
     "(defparameter *banner* \"(in-package :wrong)\")\n"
     "#| (in-package :also-wrong) |#\n"
     "(in-package #:release.pipeline)\n\n"
     "(defun deploy-release (artifact &key (environment :staging))\n"
     "  (labels ((announce (state)\n"
     "             (format nil \"~A:~A\" artifact state)))\n"
     "    (list :environment environment\n"
     "          :message (announce :ready))))\n")
    (lisp-mode)
    (slime-mode 1)
    (goto-char (point-min))
    (search-forward "release.pipeline")
    (let ((package-in-declaration (slime-search-buffer-package)))
      (search-forward "deploy-release")
      (let ((function-symbol (slime-symbol-at-point))
            (package-in-definition (slime-current-package)))
        (search-forward "announce :ready")
        (let* ((nested-symbol (slime-symbol-at-point))
               (sexp (slime-sexp-at-point))
               (region (slime-region-for-defun-at-point))
               (definition (slime-defun-at-point)))
          (list
           :mode (list major-mode slime-mode slime-editing-mode)
           :package-in-declaration package-in-declaration
           :package-in-definition package-in-definition
           :symbols (list function-symbol nested-symbol)
           :sexp sexp
           :region region
           :definition definition
           :complete
           (list
            (slime-input-complete-p (car region) (cadr region))
            (slime-input-complete-p (car region) (- (cadr region) 2)))
           :keys
           (list
            (lookup-key slime-mode-map (kbd "C-M-x"))
            (lookup-key slime-mode-map (kbd "C-c C-k"))
            (lookup-key slime-mode-map (kbd "M-.")))))))))
"#####;
    let expect = expect![[
        r####"OK (:mode (lisp-mode t nil) :package-in-declaration nil :package-in-definition "#:release.pipeline" :symbols ("deploy-release" ":ready") :sexp ":ready" :region (174 390) :definition "(defun deploy-release (artifact &key (environment :staging))\n  (labels ((announce (state)\n             (format nil \"~A:~A\" artifact state)))\n    (list :environment environment\n          :message (announce :ready))))\n" :complete (t nil) :keys (slime-eval-defun slime-compile-and-load-file slime-edit-definition))"####
    ]];
    ParityBatchCase::value(
        "common_lisp_source_workflow_tracks_package_symbols_forms_and_compile_region",
        elisp_form,
        expect,
    )
}

fn compile_defun_sends_complete_source_and_precise_coordinates_to_swank() -> ParityBatchCase {
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
    (let (rpc callback flashes hooks)
      (setq slime-before-compile-functions
            (list (lambda (start end)
                    (push (list start end
                                (buffer-substring-no-properties start end))
                          hooks))))
      (cl-letf (((symbol-function 'slime-connection)
                 (lambda (&optional _connection) 'release-connection))
                ((symbol-function 'slime-eval-async)
                 (lambda (form &optional continuation _package)
                   (setq rpc form callback continuation)
                   :queued))
                ((symbol-function 'slime-flash-region)
                 (lambda (start end &optional timeout)
                   (push (list start end timeout
                               (buffer-substring-no-properties start end))
                         flashes))))
        (slime-compile-defun)
        (list
         :rpc rpc
         :callback callback
         :flashes (nreverse flashes)
         :hooks (nreverse hooks)
         :policy slime-compilation-policy
         :point (point)
         :modified (buffer-modified-p))))))
"#####;
    let expect = expect![[
        r####"OK (:rpc (swank:compile-string-for-emacs "(defun publish-artifact (artifact channel)\n  (check-type artifact pathname)\n  (format nil \"Published ~A to ~A\" artifact channel))\n" "release-pipeline.lisp" '((:position 33) (:line 3 1)) nil 'nil) :callback slime-compilation-finished :flashes ((33 163 nil "(defun publish-artifact (artifact channel)\n  (check-type artifact pathname)\n  (format nil \"Published ~A to ~A\" artifact channel))\n")) :hooks ((33 163 "(defun publish-artifact (artifact channel)\n  (check-type artifact pathname)\n  (format nil \"Published ~A to ~A\" artifact channel))\n")) :policy nil :point 89 :modified t)"####
    ]];
    ParityBatchCase::value(
        "compile_defun_sends_complete_source_and_precise_coordinates_to_swank",
        elisp_form,
        expect,
    )
}

fn swank_transport_frames_unicode_and_dispatches_fragmented_replies() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (let* ((network-buffer (generate-new-buffer " *slime-parity-network*"))
         (connection
          (make-pipe-process
           :name "slime-parity-transport"
           :buffer network-buffer
           :command '("cat")
           :coding 'binary
           :noquery t))
         sent continuation-value dispatches)
    (unwind-protect
        (cl-letf (((symbol-function 'slime--recompute-modelines) #'ignore))
          (process-put connection 'slime-net-send-function
                       (lambda (_process wire) (setq sent wire)))
          (with-current-buffer network-buffer
            (setq-local slime-buffer-connection connection)
            (setq-local slime-rex-continuations:connlocal
                        (list (cons 73
                                    (lambda (value)
                                      (setq continuation-value value)))))
            (setq-local slime-continuation-counter:connlocal 73))
          (slime-net-send
           '(:emacs-rex (swank:interactive-eval "(+ 20 22)")
                        "RELEASE.PIPELINE" t 74)
           connection)
          (let* ((unicode-result
                  (concat "d" (string #x00e9) "ploy" (string #x00e9)
                          " " (string #x2713)))
                 (reply `(:return (:ok ,unicode-result) 73))
                 (payload (encode-coding-string
                           (concat (slime-prin1-to-string reply) "\n")
                           'utf-8-unix))
                 (wire (concat (slime-net-encode-length (length payload)) payload)))
            (slime-net-filter connection (substring wire 0 4))
            (push (with-current-buffer network-buffer
                    (list :after-header-fragment
                          (buffer-size)
                          (mapcar #'car slime-rex-continuations:connlocal)))
                  dispatches)
            (slime-net-filter connection (substring wire 4 13))
            (push (with-current-buffer network-buffer
                    (list :after-payload-fragment
                          (buffer-size)
                          (mapcar #'car slime-rex-continuations:connlocal)))
                  dispatches)
            (slime-net-filter connection (substring wire 13)))
          (list
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
                   (mapcar #'car slime-rex-continuations:connlocal)))))
      (when (process-live-p connection)
        (set-process-sentinel connection #'ignore)
        (delete-process connection))
      (kill-buffer network-buffer))))
"#####;
    let expect = expect![[
        r####"OK (:outbound-header "00004a" :outbound-length 74 :outbound-form "(:emacs-rex (swank:interactive-eval \"(+ 20 22)\") \"RELEASE.PIPELINE\" t 74)\n" :fragment-states ((:after-header-fragment 4 (73)) (:after-payload-fragment 13 (73))) :continuation-value (:ok (100 233 112 108 111 121 233 32 10003)) :remaining ("" nil))"####
    ]];
    ParityBatchCase::value(
        "swank_transport_frames_unicode_and_dispatches_fragmented_replies",
        elisp_form,
        expect,
    )
}

fn repl_session_preserves_transcript_results_prompts_and_deduplicated_history() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (let ((buffer (generate-new-buffer "*slime-repl parity*")) sent)
    (unwind-protect
        (cl-letf (((symbol-function 'slime-output-buffer)
                   (lambda (&optional _noprompt) buffer))
                  ((symbol-function 'slime-lisp-package-prompt-string)
                   (lambda (&optional _connection) "RELEASE"))
                  ((symbol-function 'slime-repl-send-string)
                   (lambda (string &optional command-string)
                     (push (list string command-string) sent))))
          (with-current-buffer buffer
            (slime-repl-mode)
            (setq slime-repl-input-history nil)
            (slime-reset-repl-markers)
            (slime-repl-insert-prompt)
            (insert "(publish-artifact #P\"build/app\" :production)")
            (slime-repl-send-input t)
            (slime-repl-emit "uploading app…\n")
            (slime-repl-insert-result '(:values "42\n" "#P\"releases/app\"\n"))
            (insert "pending input")
            (slime-repl-add-to-input-history
             "(publish-artifact #P\"build/app\" :production)")
            (slime-repl-add-to-input-history "  (status :production)  ")
            (slime-repl-add-to-input-history "(status :production)")
            (let ((pending (slime-repl-current-input)))
              (slime-repl-replace-input "")
              (setq last-command nil this-command nil)
              (slime-repl-backward-input)
              (list
               :transcript (buffer-substring-no-properties (point-min) (point-max))
               :sent (nreverse sent)
               :history slime-repl-input-history
               :pending-before-history pending
               :current-input (slime-repl-current-input)
               :markers
               (list
                (marker-position slime-output-start)
                (marker-position slime-output-end)
                (marker-position slime-repl-prompt-start-mark)
                (marker-position slime-repl-input-start-mark))
               :properties
               (list
                (get-text-property (point-min) 'slime-repl-prompt)
                (get-text-property
                 (or (text-property-any (point-min) (point-max)
                                        'slime-repl-output t)
                     (point-min))
                 'slime-repl-output)
                (get-text-property
                 (or (text-property-any (point-min) (point-max)
                                        'slime-repl-old-input 1)
                     (point-min))
                 'slime-repl-old-input))))))
      (kill-buffer buffer))))
"#####;
    let expect = expect![[
        r####"OK (:transcript "RELEASE> (publish-artifact #P\"build/app\" :production)\nuploading app…\n42\n#P\"releases/app\"\nRELEASE> (status :production)" :sent (("(publish-artifact #P\"build/app\" :production)\n" nil)) :history ("(status :production)" "(publish-artifact #P\"build/app\" :production)") :pending-before-history "pending input" :current-input "(status :production)" :markers (55 90 90 99) :properties (t t 1))"####
    ]];
    ParityBatchCase::value(
        "repl_session_preserves_transcript_results_prompts_and_deduplicated_history",
        elisp_form,
        expect,
    )
}

fn apropos_results_render_documentation_and_actionable_namespace_buttons() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (slime-parity-kill-buffers)
  (cl-letf (((symbol-function 'slime-current-connection)
             (lambda () 'release-connection))
            ((symbol-function 'display-buffer)
             (lambda (buffer &rest _arguments) buffer)))
    (slime-show-apropos
     '((:designator "RELEASE.PIPELINE:PUBLISH-ARTIFACT"
        :function "(artifact channel)"
        :setf :not-documented)
       (:designator "RELEASE.PIPELINE:*DEPLOYMENT-STATE*"
        :variable "Current immutable deployment state."))
     "release" "RELEASE.PIPELINE"
     "Apropos for release in RELEASE.PIPELINE")
    (with-current-buffer (slime-buffer-name :apropos)
      (let (buttons)
        (goto-char (point-min))
        (while-let ((button (next-button (point))))
          (push (list
                 (button-label button)
                 (button-get button 'item-type)
                 (button-get button 'item)
                 (button-get button 'follow-link))
                buttons)
          (goto-char (button-end button)))
        (prog1
            (list
             :mode (list major-mode slime-popup-buffer-mode buffer-read-only)
             :header header-line-format
             :body (buffer-substring-no-properties (point-min) (point-max))
             :buttons (nreverse buttons)
             :package slime-buffer-package
             :connection slime-buffer-connection
             :keys
             (list
              (lookup-key slime-apropos-mode-map (kbd "n"))
              (lookup-key slime-apropos-mode-map (kbd "p"))))
          (kill-buffer (current-buffer)))))))
"#####;
    let expect = expect![[
        r####"OK (:mode (slime-apropos-mode t t) :header "Apropos for release in RELEASE.PIPELINE" :body "RELEASE.PIPELINE:PUBLISH-ARTIFACT\n  Function: (artifact channel)\n  Setf: (not documented)\nRELEASE.PIPELINE:*DEPLOYMENT-STATE*\n  Variable: Current immutable deployment state.\n" :buttons (("Function" :function "RELEASE.PIPELINE:PUBLISH-ARTIFACT" t) ("Setf" :setf "RELEASE.PIPELINE:PUBLISH-ARTIFACT" t) ("Variable" :variable "RELEASE.PIPELINE:*DEPLOYMENT-STATE*" t)) :package "RELEASE.PIPELINE" :connection release-connection :keys (slime-apropos-next-symbol slime-apropos-previous-symbol))"####
    ]];
    ParityBatchCase::value(
        "apropos_results_render_documentation_and_actionable_namespace_buttons",
        elisp_form,
        expect,
    )
}

fn inspector_page_preserves_value_action_and_pagination_navigation() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (slime-parity-kill-buffers)
  (let (requests)
    (cl-letf (((symbol-function 'slime-current-connection)
               (lambda () 'release-connection))
              ((symbol-function 'pop-to-buffer)
               (lambda (buffer &rest _arguments) (set-buffer buffer) buffer))
              ((symbol-function 'display-buffer)
               (lambda (buffer &rest _arguments) buffer))
              ((symbol-function 'slime-eval-async)
               (lambda (form &optional continuation _package)
                 (push (list form (functionp continuation)) requests)
                 :queued)))
      (slime-open-inspector
       '(:id 900
         :title "#<DEPLOYMENT release-2026.08>\n"
         :content
         (("Environment: " (:value ":PRODUCTION" 11) "\n"
           (:label "Artifact: ") (:value "#P\"releases/app\"" 12) "\n"
           (:action "Rollback deployment" 4) "\n")
          9 2 9)))
      (with-current-buffer (slime-buffer-name :inspector)
        (let (targets)
          (goto-char (point-min))
          (while (< (point) (point-max))
            (when-let ((property (slime-inspector-property-at-point)))
              (unless (member property targets)
                (push property targets)))
            (forward-char 1))
          (goto-char (point-min))
          (search-forward ":PRODUCTION")
          (slime-inspector-operate-on-point)
          (goto-char (point-min))
          (search-forward "Rollback")
          (slime-inspector-operate-on-point)
          (prog1
              (list
               :mode (list major-mode buffer-read-only)
               :body (buffer-substring-no-properties (point-min) (point-max))
               :targets (nreverse targets)
               :requests (nreverse requests)
               :mark-stack slime-inspector-mark-stack
               :title-properties
               (list
                (get-text-property (point-min) 'slime-part-number)
                (get-text-property (point-min) 'face)))
            (kill-buffer (current-buffer))))))))
"#####;
    let expect = expect![[
        r####"OK (:mode (slime-inspector-mode t) :body "#<DEPLOYMENT release-2026.08>\n--------------------\n [--more--]\nEnvironment: :PRODUCTION\nArtifact: #P\"releases/app\"\nRollback deployment\n" :targets ((slime-part-number 900) (slime-range-button (2 t)) (slime-part-number 11) (slime-part-number 12) (slime-action-number 4)) :requests (((swank:inspect-nth-part 11) t) ((swank:inspector-call-nth-action 4) t)) :mark-stack ((4 . 24)) :title-properties (900 slime-inspector-value-face))"####
    ]];
    ParityBatchCase::value(
        "inspector_page_preserves_value_action_and_pagination_navigation",
        elisp_form,
        expect,
    )
}

fn debugger_presents_condition_restarts_and_navigable_restartable_frames() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (slime-parity-kill-buffers)
  (let (rpc)
    (cl-letf (((symbol-function 'slime-connection)
               (lambda (&optional _connection) 'release-connection))
              ((symbol-function 'slime-connection-name)
               (lambda (&optional _connection) "SBCL release"))
              ((symbol-function 'sldb-display-buffer)
               (lambda (buffer) buffer))
              ((symbol-function 'slime-dispatch-event)
               (lambda (event &optional process)
                 (setq rpc
                       (list
                        (list (nth 0 event)
                              (nth 1 event)
                              (nth 2 event)
                              (nth 3 event)
                              (functionp (nth 4 event)))
                        process)))))
      (sldb-setup
       88 2
       '("Checksum mismatch for release-2026.08" "SIMPLE-ERROR" nil)
       '(("RETRY" "Re-download the immutable artifact")
         ("USE-CACHED" "Deploy the previously verified artifact")
         ("ABORT" "Abort this deployment"))
       '((0 "(VERIFY-ARTIFACT #P\"releases/app\")" (:restartable t))
         (1 "(PUBLISH-ARTIFACT #P\"releases/app\" :PRODUCTION)" nil)
         (2 "(SWANK::EVAL-FOR-EMACS ...)" nil))
       '(73 74))
      (with-current-buffer (sldb-find-buffer 88 'release-connection)
        (goto-char sldb-restart-list-start-marker)
        (forward-line 1)
        (let ((restart (sldb-restart-at-point)))
          (sldb-invoke-restart restart)
          (goto-char sldb-backtrace-start-marker)
          (let ((frame (sldb-frame-number-at-point)))
            (prog1
                (list
                 :mode (list major-mode mode-name buffer-read-only)
                 :body (buffer-substring-no-properties (point-min) (point-max))
                 :restart restart
                 :frame frame
                 :frame-data
                 (let ((data (get-text-property (point) 'frame)))
                   (list
                    (car data)
                    (substring-no-properties (cadr data))
                    (plist-get (caddr data) :restartable)))
                 :rpc rpc
                 :thread slime-current-thread
                 :level sldb-level
                 :continuations sldb-continuations
                 :keys
                 (list
                  (lookup-key sldb-mode-map (kbd "RET"))
                  (lookup-key sldb-mode-map (kbd "c"))
                  (lookup-key sldb-mode-map (kbd "0")))
                 :markers
                 (list
                  (marker-position sldb-restart-list-start-marker)
                  (marker-position sldb-backtrace-start-marker)))
              (kill-buffer (current-buffer)))))))))
"#####;
    let expect = expect![[
        r####"OK (:mode (sldb-mode "sldb[2]" t) :body "Checksum mismatch for release-2026.08\nSIMPLE-ERROR\n\nRestarts:\n 0: [RETRY] Re-download the immutable artifact\n 1: [USE-CACHED] Deploy the previously verified artifact\n 2: [ABORT] Abort this deployment\n\nBacktrace:\n  0: (VERIFY-ARTIFACT #P\"releases/app\")\n  1: (PUBLISH-ARTIFACT #P\"releases/app\" :PRODUCTION)\n --more--\n" :restart 1 :frame 0 :frame-data (0 "(VERIFY-ARTIFACT #P\"releases/app\")" t) :rpc ((:emacs-rex (swank:invoke-nth-restart-for-emacs 2 1) nil 88 t) nil) :thread 88 :level 2 :continuations (73 74) :keys (sldb-default-action sldb-continue sldb-invoke-restart-0) :markers (63 213))"####
    ]];
    ParityBatchCase::value(
        "debugger_presents_condition_restarts_and_navigable_restartable_frames",
        elisp_form,
        expect,
    )
}

#[test]
fn slime_package_batch() {
    let cases = vec![
        common_lisp_source_workflow_tracks_package_symbols_forms_and_compile_region(),
        compile_defun_sends_complete_source_and_precise_coordinates_to_swank(),
        swank_transport_frames_unicode_and_dispatches_fragmented_replies(),
        repl_session_preserves_transcript_results_prompts_and_deduplicated_history(),
        apropos_results_render_documentation_and_actionable_namespace_buttons(),
        inspector_page_preserves_value_action_and_pagination_navigation(),
        debugger_presents_condition_restarts_and_navigable_restartable_frames(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed SLIME parity test");
    assert_oracle_batch_cases(slime_oracle(), test_name, "slime_parity", &cases);
}

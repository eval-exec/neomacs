use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, SHELL_MAKER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const SHELL_MAKER_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const SHELL_MAKER_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'shell-maker)

(defvar shell-maker-test-root (make-temp-file "shell-maker-parity-" t))
(setq shell-maker-root-path shell-maker-test-root)

(defun shell-maker-test-config (name executor &optional validator observer)
  (make-shell-maker-config
   :name name
   :prompt (concat name "> ")
   :prompt-regexp (concat "^" (regexp-quote name) "> ")
   :validate-command validator
   :execute-command executor
   :on-command-finished observer))

(defun shell-maker-test-start (config)
  (shell-maker-start-v2
   :config config
   :no-focus t
   :buffer-name (format " *shell-maker-test-%s*"
                        (downcase (shell-maker-config-name config)))
   :alias-commands nil))

(defun shell-maker-test-cleanup (buffer)
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (when-let ((process (get-buffer-process buffer)))
        (set-process-query-on-exit-flag process nil)
        (set-process-sentinel process #'ignore)
        (delete-process process))
      (set-buffer-modified-p nil))
    (kill-buffer buffer)))

(defun shell-maker-test-plain-history ()
  (mapcar
   (lambda (entry)
     (cons (and (car entry) (substring-no-properties (car entry)))
           (and (cdr entry) (substring-no-properties (cdr entry)))))
   (shell-maker-history)))

(defun shell-maker-test-submit (input)
  (cl-letf (((symbol-function 'shell-maker--curl-version-supported)
             (lambda () t)))
    (shell-maker-submit :input input)))
"####;

fn shell_maker_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SHELL_MAKER_MELPA_PIN, "shell-maker.el")
        .expect("prepare pinned shell-maker source below ./tmp")
        .with_prelude(SHELL_MAKER_TEST_PRELUDE)
        .with_timeout(SHELL_MAKER_TEST_TIMEOUT)
}

fn starting_a_named_shell_builds_a_live_comint_mode_with_a_protected_prompt() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((config
        (shell-maker-test-config
         "ResearchInit"
         (lambda (_input _shell) (error "executor should not run"))))
       (buffer (shell-maker-test-start config)))
  (unwind-protect
      (with-current-buffer buffer
        (let ((prompt-start (point-min)))
          (list
           :buffer-name (buffer-name)
           :mode major-mode
           :parent (get major-mode 'derived-mode-parent)
           :config-copied (and (not (eq config shell-maker--config)) t)
           :prompt (buffer-substring-no-properties (point-min) (point-max))
           :prompt-face (get-text-property prompt-start 'font-lock-face)
           :prompt-read-only (get-text-property prompt-start 'read-only)
           :prompt-field (get-text-property prompt-start 'field)
           :last-prompt
           (list (marker-position (car comint-last-prompt))
                 (marker-position (cdr comint-last-prompt)))
           :process
           (let ((process (get-buffer-process buffer)))
             (list (process-name process)
                   (and (process-live-p process) t)
                   (marker-position (process-mark process))))
           :settings
           (list comint-prompt-regexp comint-prompt-read-only
                 comint-process-echoes comint-completion-addsuffix
                 comint-get-old-input paragraph-start)
           :history-empty (ring-empty-p comint-input-ring)
           :history-file
           (file-relative-name
            (shell-maker-history-file-path shell-maker--config)
            shell-maker-test-root)
           :point-at-prompt (shell-maker-point-at-last-prompt-p))))
    (shell-maker-test-cleanup buffer)))
"####;
    let expect = expect![[
        r####"OK (:buffer-name " *shell-maker-test-researchinit*" :mode researchinit-shell-mode :parent comint-mode :config-copied t :prompt "ResearchInit> " :prompt-face (comint-highlight-prompt) :prompt-read-only t :prompt-field output :last-prompt (1 15) :process ("researchinit" t 15) :settings ("^ResearchInit> " t nil nil shell-maker--get-old-input "^ResearchInit> ") :history-empty t :history-file "researchinit/history" :point-at-prompt t)"####
    ]];
    ParityBatchCase::value(
        "starting_a_named_shell_builds_a_live_comint_mode_with_a_protected_prompt",
        elisp_form,
        expect,
    )
}

fn submitting_a_command_streams_fragments_freezes_input_and_finishes_with_history()
-> ParityBatchCase {
    let elisp_form = r####"
(let (executor-events output-events finished-events observer-events buffer)
  (let* ((config
          (shell-maker-test-config
           "StreamWork"
           (lambda (input shell)
             (push (list (substring-no-properties input)
                         (mapcar #'car shell))
                   executor-events)
             (funcall (map-elt shell :write-output) "Planning 2 + 2...\n")
             (funcall (map-elt shell :write-output) "Result: 4")
             (funcall (map-elt shell :finish-output) t))
           nil
           (lambda (input output success)
             (push (list (substring-no-properties input) output success)
                   observer-events)))))
    (setq buffer (shell-maker-test-start config))
    (unwind-protect
        (with-current-buffer buffer
          (cl-letf (((symbol-function 'shell-maker--curl-version-supported)
                     (lambda () t)))
            (shell-maker-submit
             :input "calculate 2 + 2"
             :on-output (lambda (fragment) (push fragment output-events))
             :on-finished
             (lambda (input output success)
               (push (list (substring-no-properties input) output success)
                     finished-events))))
          (goto-char (point-min))
          (search-forward "calculate 2 + 2")
          (let* ((input-start (- (point) (length "calculate 2 + 2")))
                 (marker-start
                  (progn
                    (search-forward "<shell-maker-end-of-prompt>")
                    (- (point) (length "<shell-maker-end-of-prompt>"))))
                 (final-prompt
                  (progn
                    (goto-char (point-max))
                    (re-search-backward "^StreamWork> ")
                    (point))))
            (list
             :buffer (buffer-substring-no-properties (point-min) (point-max))
             :executor (nreverse executor-events)
             :output-callbacks (nreverse output-events)
             :finished (nreverse finished-events)
             :observer (nreverse observer-events)
             :history (shell-maker-test-plain-history)
             :ring (mapcar #'substring-no-properties
                           (ring-elements comint-input-ring))
             :busy (shell-maker-busy)
             :input-properties
             (list (get-text-property input-start 'read-only)
                   (get-text-property input-start 'front-sticky)
                   (get-text-property input-start 'font-lock-face))
             :marker-properties
             (list (get-text-property marker-start 'shell-maker--marker)
                   (get-text-property marker-start 'invisible)
                   (get-text-property marker-start 'field)
                   (get-text-property marker-start 'read-only))
             :final-prompt-properties
             (list (get-text-property final-prompt 'font-lock-face)
                   (get-text-property final-prompt 'read-only)))))
      (shell-maker-test-cleanup buffer))))
"####;
    let expect = expect![[
        r####"OK (:buffer "StreamWork> calculate 2 + 2\n<shell-maker-end-of-prompt>\nPlanning 2 + 2...\nResult: 4\n\nStreamWork> " :executor (("calculate 2 + 2" (:history :log :buffer :write-output :finish-output))) :output-callbacks ("Planning 2 + 2...\n" "Result: 4" "\n\n") :finished (("calculate 2 + 2" "Planning 2 + 2...\nResult: 4" t)) :observer (("calculate 2 + 2" "Planning 2 + 2...\nResult: 4" t)) :history (("calculate 2 + 2" . "Planning 2 + 2...\nResult: 4")) :ring ("calculate 2 + 2") :busy nil :input-properties (t (read-only) comint-highlight-input) :marker-properties (t t output t) :final-prompt-properties ((comint-highlight-prompt) t))"####
    ]];
    ParityBatchCase::value(
        "submitting_a_command_streams_fragments_freezes_input_and_finishes_with_history",
        elisp_form,
        expect,
    )
}

fn validation_failure_records_diagnostics_but_excludes_the_exchange_from_history() -> ParityBatchCase
{
    let elisp_form = r####"
(let (executed output-events observer-events buffer)
  (let* ((config
          (shell-maker-test-config
           "GuardedWork"
           (lambda (_input _shell) (setq executed t))
           (lambda (input)
             (and (string-match-p "drop" input)
                  "Destructive requests are disabled"))
           (lambda (input output success)
             (push (list (substring-no-properties input) output success)
                   observer-events)))))
    (setq buffer (shell-maker-test-start config))
    (unwind-protect
        (with-current-buffer buffer
          (cl-letf (((symbol-function 'shell-maker--curl-version-supported)
                     (lambda () t)))
            (shell-maker-submit
             :input "drop production table"
             :on-output (lambda (fragment) (push fragment output-events))))
          (goto-char (point-min))
          (search-forward "<shell-maker-failed-command>")
          (let ((marker-start
                 (- (point) (length "<shell-maker-failed-command>"))))
            (list
             :executed executed
             :buffer (buffer-substring-no-properties (point-min) (point-max))
             :callbacks (nreverse output-events)
             :observer (nreverse observer-events)
             :history (shell-maker-test-plain-history)
             :ring (mapcar #'substring-no-properties
                           (ring-elements comint-input-ring))
             :busy (shell-maker-busy)
             :failure-marker
             (list (get-text-property marker-start 'shell-maker--marker)
                   (get-text-property marker-start 'invisible)))))
      (shell-maker-test-cleanup buffer))))
"####;
    let expect = expect![[
        r####"OK (:executed nil :buffer "GuardedWork> drop production table\n\nDestructive requests are disabled\n\n\n<shell-maker-failed-command>\nGuardedWork> " :callbacks ("\nDestructive requests are disabled\n\n") :observer (("drop production table" "\nDestructive requests are disabled\n\n" nil)) :history nil :ring ("drop production table") :busy nil :failure-marker (nil t))"####
    ]];
    ParityBatchCase::value(
        "validation_failure_records_diagnostics_but_excludes_the_exchange_from_history",
        elisp_form,
        expect,
    )
}

fn interrupting_a_stream_exposes_the_protected_output_failure_without_losing_partial_history()
-> ParityBatchCase {
    let elisp_form = r####"
(let (buffer)
  (let* ((config
          (shell-maker-test-config
           "InterruptWork"
           (lambda (_input shell)
             (funcall (map-elt shell :write-output)
                      "Completed phase one\nWaiting for remote phase")))))
    (setq buffer (shell-maker-test-start config))
    (unwind-protect
        (with-current-buffer buffer
          (shell-maker-test-submit "run staged analysis")
          (let ((busy-before (shell-maker-busy))
                (request-before (shell-maker--current-request-id))
                (interrupt-outcome
                 (condition-case error-data
                     (progn (shell-maker-interrupt nil) :ok)
                   (error (list :signal (car error-data)
                                (cdr error-data))))))
            (goto-char (point-min))
            (search-forward "<shell-maker-interrupted-command>")
            (let ((marker-start
                   (- (point) (length "<shell-maker-interrupted-command>"))))
              (list
               :busy-before busy-before
               :busy-after (shell-maker-busy)
               :interrupt interrupt-outcome
               :request-ids (list request-before
                                  (shell-maker--current-request-id))
               :history (shell-maker-test-plain-history)
               :buffer (buffer-substring-no-properties (point-min) (point-max))
               :marker
               (list (get-text-property marker-start 'shell-maker--marker)
                     (get-text-property marker-start 'invisible))
               :last-prompt (save-excursion
                              (goto-char (point-max))
                              (re-search-backward "^InterruptWork> ")
                              (line-number-at-pos))))))
      (shell-maker-test-cleanup buffer))))
"####;
    let expect = expect![[
        r####"OK (:busy-before t :busy-after t :interrupt (:signal text-read-only nil) :request-ids (1 2) :history (("run staged analysis" . "Completed phase one\nWaiting for remote phase\n<shell-maker-interrupted-command>")) :buffer "InterruptWork> run staged analysis\n<shell-maker-end-of-prompt>\nCompleted phase one\nWaiting for remote phase\n<shell-maker-interrupted-command>\n" :marker (t t) :last-prompt 1)"####
    ]];
    ParityBatchCase::value(
        "interrupting_a_stream_exposes_the_protected_output_failure_without_losing_partial_history",
        elisp_form,
        expect,
    )
}

fn history_navigation_marks_and_deletes_a_real_middle_interaction() -> ParityBatchCase {
    let elisp_form = r####"
(let (buffer)
  (let* ((config
          (shell-maker-test-config
           "HistoryWork"
           (lambda (input shell)
             (funcall (map-elt shell :write-output)
                      (format "report for [%s]" input))
             (funcall (map-elt shell :finish-output) t)))))
    (setq buffer (shell-maker-test-start config))
    (unwind-protect
        (with-current-buffer buffer
          (dolist (input '("inspect alpha" "inspect beta" "publish gamma"))
            (shell-maker-test-submit input))
          (goto-char (point-min))
          (search-forward "report for [inspect beta]")
          (let ((position (shell-maker-history-position))
                (interaction (shell-maker--command-and-response-at-point)))
            (shell-maker-mark-output)
            (let ((marked (buffer-substring-no-properties
                           (region-beginning) (region-end))))
              (shell-maker-delete-interaction-at-point)
              (list
               :position position
               :interaction
               (cons (substring-no-properties (car interaction))
                     (substring-no-properties (cdr interaction)))
               :marked marked
               :history-after-delete (shell-maker-test-plain-history)
               :buffer-after-delete
               (buffer-substring-no-properties (point-min) (point-max))
               :point (list (line-number-at-pos) (current-column))))))
      (shell-maker-test-cleanup buffer))))
"####;
    let expect = expect![[
        r####"OK (:position ((:current . 2) (:total . 3)) :interaction ("inspect beta" . "report for [inspect beta]") :marked "report for [inspect beta]\n" :history-after-delete (("inspect alpha" . "report for [inspect alpha]") ("publish gamma" . "report for [publish gamma]")) :buffer-after-delete "HistoryWork> inspect alpha\n<shell-maker-end-of-prompt>\nreport for [inspect alpha]\n\nHistoryWork> publish gamma\n<shell-maker-end-of-prompt>\nreport for [publish gamma]\n\nHistoryWork> " :point (5 13))"####
    ]];
    ParityBatchCase::value(
        "history_navigation_marks_and_deletes_a_real_middle_interaction",
        elisp_form,
        expect,
    )
}

fn transcript_restore_save_and_resume_round_trips_a_practical_session() -> ParityBatchCase {
    let elisp_form = r####"
(let (buffer executor-events)
  (let* ((config
          (shell-maker-test-config
           "TranscriptWork"
           (lambda (input shell)
             (push (substring-no-properties input) executor-events)
             (funcall (map-elt shell :write-output) (concat "LIVE: " input))
             (funcall (map-elt shell :finish-output) t))))
         (transcript (expand-file-name "saved-session.txt" shell-maker-test-root)))
    (setq buffer (shell-maker-test-start config))
    (unwind-protect
        (with-current-buffer buffer
          (cl-letf (((symbol-function 'shell-maker--curl-version-supported)
                     (lambda () t)))
            (shell-maker-restore-session-from-transcript
             '(("load dataset" . "dataset: 42 rows")
               ("summarize revenue" . "revenue: 1250")))
            (setq shell-maker--file transcript)
            (shell-maker-save-session-transcript)
            (let ((saved
                   (with-temp-buffer
                     (insert-file-contents transcript)
                     (buffer-string))))
              (shell-maker-submit :input "forecast next quarter")
              (list
               :saved saved
               :saved-history
               (with-temp-buffer
                 (insert saved)
                 (shell-maker--extract-history "^TranscriptWork> "
                                               :propertized nil))
               :live-history (shell-maker-test-plain-history)
               :executor-events (nreverse executor-events)
               :file (file-relative-name shell-maker--file shell-maker-test-root)
               :modified (buffer-modified-p)
               :busy (shell-maker-busy)))))
      (shell-maker-test-cleanup buffer))))
"####;
    let expect = expect![[
        r####"OK (:saved "TranscriptWork> load dataset\n<shell-maker-end-of-prompt>\ndataset: 42 rows\n\nTranscriptWork> summarize revenue\n<shell-maker-end-of-prompt>\nrevenue: 1250\n\nTranscriptWork> \n" :saved-history (("load dataset" . "dataset: 42 rows") ("summarize revenue" . "revenue: 1250")) :live-history (("load dataset" . "dataset: 42 rows") ("summarize revenue" . "revenue: 1250") ("forecast next quarter" . "LIVE: forecast next quarter")) :executor-events ("forecast next quarter") :file "saved-session.txt" :modified t :busy nil)"####
    ]];
    ParityBatchCase::value(
        "transcript_restore_save_and_resume_round_trips_a_practical_session",
        elisp_form,
        expect,
    )
}

fn local_command_json_and_http_building_cover_a_stream_transport_pipeline() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((split
        (shell-maker--split-text
         "event: completion\ndata: {\"delta\":\"hello\"}\nplain tail\n"))
       (preparsed
        (shell-maker--preparse-json
         "data: {\"index\":1}{\"index\":2}partial"))
       (command-result
        (shell-maker-execute-command
         :command '("sh" "-c" "printf 'alpha\\nbeta\\n'")
         :filter
         (lambda (state)
           `((:filtered . ,(upcase (map-elt state :pending)))))))
       (curl-command
        (shell-maker-make--curl-command
         :url "https://api.example.test/v1/chat"
         :timeout 45
         :proxy "http://proxy.example.test:8080"
         :headers '("Authorization: Bearer redacted" "Content-Type: application/json")
         :fields '("model=local" "stream=true"))))
  (list
   :split split
   :preparsed preparsed
   :json (shell-maker--json-encode
          '((prompt . "hello") (stream . t) (limit . 3)))
   :command command-result
   :curl curl-command
   :aligned
   (shell-maker-align-columns
    '(("command" "status" "latency")
      ("summarize" "ok" "12ms")
      ("long-analysis" "failed" "250ms")))))
"####;
    let expect = expect![[
        r####"OK (:split (((:key . "event:") (:value . "completion")) ((:key . "data:") (:value . "{\"delta\":\"hello\"}")) ((:key) (:value . "plain tail"))) :preparsed ((((index . 1)) ((index . 2))) . "partial") :json "{\"prompt\":\"hello\",\"stream\":true,\"limit\":3}" :command ((:exit-status . 0) (:output . "ALPHA\nBETA\n")) :curl ("curl" "https://api.example.test/v1/chat" "--fail-with-body" "--no-progress-meter" "-m" "45" "--proxy" "http://proxy.example.test:8080" "-H" "Authorization: Bearer redacted" "-H" "Content-Type: application/json" "-F" "model=local" "-F" "stream=true") :aligned "command         status   latency\nsummarize       ok       12ms   \nlong-analysis   failed   250ms  ")"####
    ]];
    ParityBatchCase::value(
        "local_command_json_and_http_building_cover_a_stream_transport_pipeline",
        elisp_form,
        expect,
    )
}

fn actionable_help_text_exposes_keyboard_and_mouse_activation() -> ParityBatchCase {
    let elisp_form = r####"
(let (actions)
  (with-temp-buffer
    (let ((text
           (shell-maker--actionable-text
            "open transcript"
            (lambda () (push (list :opened (current-buffer)) actions)))))
      (insert text)
      (goto-char (point-min))
      (let* ((map (get-text-property (point) 'keymap))
             (return-command (lookup-key map (kbd "RET")))
             (mouse-command (lookup-key map [mouse-1])))
        (call-interactively return-command)
        (list
         :text (buffer-substring-no-properties (point-min) (point-max))
         :face (get-text-property (point) 'font-lock-face)
         :return-command (commandp return-command)
         :same-command (eq return-command mouse-command)
         :self-insert (lookup-key map [remap self-insert-command])
         :actions (mapcar
                   (lambda (action)
                     (list (car action)
                           (eq (cadr action) (current-buffer))))
                   actions))))))
"####;
    let expect = expect![[
        r####"OK (:text "open transcript" :face link :return-command t :same-command t :self-insert ignore :actions ((:opened t)))"####
    ]];
    ParityBatchCase::value(
        "actionable_help_text_exposes_keyboard_and_mouse_activation",
        elisp_form,
        expect,
    )
}

#[test]
fn shell_maker_package_batch() {
    let cases = vec![
        starting_a_named_shell_builds_a_live_comint_mode_with_a_protected_prompt(),
        submitting_a_command_streams_fragments_freezes_input_and_finishes_with_history(),
        validation_failure_records_diagnostics_but_excludes_the_exchange_from_history(),
        interrupting_a_stream_exposes_the_protected_output_failure_without_losing_partial_history(),
        history_navigation_marks_and_deletes_a_real_middle_interaction(),
        transcript_restore_save_and_resume_round_trips_a_practical_session(),
        local_command_json_and_http_building_cover_a_stream_transport_pipeline(),
        actionable_help_text_exposes_keyboard_and_mouse_activation(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed shell-maker parity test");
    assert_oracle_batch_cases(
        shell_maker_oracle(),
        test_name,
        "shell_maker_parity",
        &cases,
    );
}

//! Practical parity for Command Log Mode's documented logging workflows.
//!
//! The cases drive real keyboard commands through the package's installed
//! hooks, exercise repetition and text policies, public window and global-mode
//! lifecycles, and the timestamp-preserving save/recovery path.
//!
//! Two package-free reductions are recorded as live core reds in
//! `DIVERGENCES.md` entries 86 and 87: Neomacs does not extend the timestamp property across
//! `move-to-column` padding, and its write path ignores
//! `write-region-annotate-functions`.  The corpus keeps GNU's complete
//! behavior as the expectation rather than hiding either difference.

use std::time::Duration;

use expect_test::expect;

use crate::{COMMAND_LOG_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'command-log-mode)

(defconst clm396-test-source-sha256
  "f512e7cdc757b1776945bce74fca60f8805d4a50ceee76178b45e0cfa3597ff7")

(defun clm396-test-file-sha256 (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(let ((file (symbol-file 'command-log-mode 'defun)))
  (unless (and (file-regular-p file)
               (equal (file-name-nondirectory file) "command-log-mode.el")
               (equal (clm396-test-file-sha256 file)
                      clm396-test-source-sha256))
    (error "Unexpected installed Command Log Mode source: %S" file)))

(defun clm396-test-condition (condition root)
  (list :type (car condition)
        :data (clm396-test-normalize (copy-tree (cdr condition)) root)
        :message (clm396-test-normalize (error-message-string condition) root)))

(defun clm396-test-normalize (value root)
  (cond ((stringp value)
         (let ((normalized
                (if root
                    (replace-regexp-in-string
                     (regexp-quote root) "[ROOT]/" value t t)
                  (copy-sequence value))))
           (replace-regexp-in-string
            "log-[0-9]\\{4\\}-[0-9]\\{2\\}-[0-9]\\{2\\}"
            "log-[DATE]" normalized t t)))
        ((consp value)
         (cons (clm396-test-normalize (car value) root)
               (clm396-test-normalize (cdr value) root)))
        ((vectorp value)
         (apply #'vector
                (mapcar (lambda (entry)
                          (clm396-test-normalize entry root))
                        value)))
        (t value)))

(defun clm396-test-window-state ()
  (list :selected (selected-window)
        :windows
        (mapcar
         (lambda (window)
           (list :window window :buffer (window-buffer window)
                 :point (window-point window) :start (window-start window)
                 :hscroll (window-hscroll window)
                 :vscroll (window-vscroll window t)
                 :prev (copy-tree (window-prev-buffers window))
                 :next (copy-tree (window-next-buffers window))))
         (window-list nil 'no-minibuf))))

(defun clm396-test-restore-windows (configuration state)
  (set-window-configuration configuration)
  (dolist (entry (plist-get state :windows))
    (let ((window (plist-get entry :window)))
      (unless (window-live-p window)
        (error "Baseline Command Log Mode window died: %S" window))
      (set-window-prev-buffers window (copy-tree (plist-get entry :prev)))
      (set-window-next-buffers window (copy-tree (plist-get entry :next)))
      (set-window-point window (plist-get entry :point))
      (set-window-start window (plist-get entry :start) 'noforce)
      (set-window-hscroll window (plist-get entry :hscroll))
      (set-window-vscroll window (plist-get entry :vscroll) t))))

(defun clm396-test-park-buffer (name)
  (when-let* ((buffer (get-buffer name)))
    (with-current-buffer buffer
      (let ((old-name (buffer-name)))
        (rename-buffer
         (format " *clm396-parked-%s*" (sxhash-eq buffer)) t)
        (cons buffer old-name)))))

(defun clm396-test-properties ()
  (let ((position (point-min)) runs)
    (while (< position (point-max))
      (let ((next (next-property-change position nil (point-max))))
        (push (list :range (list position next)
                    :text (buffer-substring-no-properties position next)
                    :time (get-text-property position :time))
              runs)
        (setq position next)))
    (nreverse runs)))

(defun clm396-test-log-state ()
  (when (buffer-live-p clm/command-log-buffer)
    (with-current-buffer clm/command-log-buffer
      (list :text (buffer-substring-no-properties (point-min) (point-max))
            :properties (clm396-test-properties)
            :point (point)
            :windows
            (mapcar
             (lambda (window)
               (list :dedicated (window-dedicated-p window)
                     :point (window-point window)))
             (get-buffer-window-list (current-buffer) nil t))))))

(defun clm396-test-command ()
  (interactive)
  (insert "<deployed>"))

(defun clm396-test-run-keys (keys)
  (let ((old-binding (lookup-key global-map (kbd "C-c !"))))
    (unwind-protect
        (progn
          (global-set-key (kbd "C-c !") #'clm396-test-command)
          (execute-kbd-macro (kbd keys)))
      (define-key global-map (kbd "C-c !") old-binding))))

(defun clm396-test-run (body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name "command-log-mode/" sandbox))))
         (window-before (current-window-configuration))
         (window-state-before (clm396-test-window-state))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (ambient-modes
          (mapcar (lambda (buffer)
                    (with-current-buffer buffer
                      (list :buffer buffer
                            :local (local-variable-p 'command-log-mode)
                            :value command-log-mode)))
                  (buffer-list)))
         (ambient-global-mode global-command-log-mode)
         (ambient-global-hook (copy-sequence after-change-major-mode-hook))
         (parked nil)
         (after-change-major-mode-hook
          (copy-sequence after-change-major-mode-hook))
         (global-command-log-mode nil)
         (clm/log-text t)
         (clm/log-repeat nil)
         (clm/recent-history-string "")
         (clm/time-string "STAMP")
         (clm/logging-dir (and root (expand-file-name "log-" root)))
         (clm/log-command-exceptions* '(nil))
         (clm/command-log-buffer nil)
         (clm/command-repetitions 0)
         (clm/last-keyboard-command nil)
         (clm/log-command-indentation 11)
         (command-log-mode-auto-show nil)
         (command-log-mode-window-size 30)
         (command-log-mode-window-font-size 2)
         (command-log-mode-open-log-turns-on-mode nil)
         (command-log-mode-is-global nil)
         (print-circle nil)
         root-owned result body-error cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute Command Log Mode sandbox root"))
              (when (file-exists-p root)
                (error "Command Log Mode sandbox root already exists: %s" root))
              (setq root-owned t)
              (setq parked (clm396-test-park-buffer " *command-log*"))
              (when ambient-global-mode (global-command-log-mode -1))
              (cl-letf (((symbol-function 'make-process)
                         (lambda (&rest arguments)
                           (error "Unexpected process: %S" arguments)))
                        ((symbol-function 'start-process)
                         (lambda (&rest arguments)
                           (error "Unexpected process start: %S" arguments)))
                        ((symbol-function 'call-process)
                         (lambda (&rest arguments)
                           (error "Unexpected synchronous process: %S"
                                  arguments)))
                        ((symbol-function 'process-file)
                         (lambda (&rest arguments)
                           (error "Unexpected file process: %S" arguments)))
                        ((symbol-function 'make-network-process)
                         (lambda (&rest arguments)
                           (error "Unexpected network process: %S" arguments)))
                        ((symbol-function 'url-retrieve)
                         (lambda (&rest arguments)
                           (error "Unexpected URL retrieval: %S" arguments))))
                (save-window-excursion
                  (save-current-buffer
                    (setq result (funcall body root))))))
          (t (setq body-error (clm396-test-condition condition root))))
      (condition-case condition
          (when global-command-log-mode (global-command-log-mode -1))
        (t (push (clm396-test-condition condition root) cleanup-errors)))
      (condition-case condition
          (when ambient-global-mode (global-command-log-mode 1))
        (t (push (clm396-test-condition condition root) cleanup-errors)))
      (dolist (entry ambient-modes)
        (condition-case condition
            (when (buffer-live-p (plist-get entry :buffer))
              (with-current-buffer (plist-get entry :buffer)
                (unless (eq command-log-mode (plist-get entry :value))
                  (command-log-mode
                   (if (plist-get entry :value) 1 -1)))
                (unless (plist-get entry :local)
                  (kill-local-variable 'command-log-mode))))
          (t (push (clm396-test-condition condition root) cleanup-errors))))
      (condition-case condition
          (clm396-test-restore-windows window-before window-state-before)
        (t (push (clm396-test-condition condition root) cleanup-errors)))
      (dolist (process (seq-difference (process-list) processes-before #'eq))
        (condition-case condition (delete-process process)
          (t (push (clm396-test-condition condition root) cleanup-errors))))
      (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
        (condition-case condition
            (when (buffer-live-p buffer)
              (with-current-buffer buffer
                (let ((kill-buffer-hook nil)
                      (kill-buffer-query-functions nil))
                  (set-buffer-modified-p nil)
                  (kill-buffer buffer))))
          (t (push (clm396-test-condition condition root) cleanup-errors))))
      (dolist (timer (seq-difference timer-list timers-before #'eq))
        (condition-case condition (cancel-timer timer)
          (t (push (clm396-test-condition condition root) cleanup-errors))))
      (dolist (frame (seq-difference (frame-list) frames-before #'eq))
        (condition-case condition (delete-frame frame t)
          (t (push (clm396-test-condition condition root) cleanup-errors))))
      (when parked
        (condition-case condition
            (if (buffer-live-p (car parked))
                (with-current-buffer (car parked)
                  (rename-buffer (cdr parked) t))
              (error "Parked Command Log Mode buffer died"))
          (t (push (clm396-test-condition condition root) cleanup-errors))))
      (condition-case condition
          (clm396-test-restore-windows window-before window-state-before)
        (t (push (clm396-test-condition condition root) cleanup-errors)))
      (condition-case condition
          (when (buffer-live-p buffer-before) (set-buffer buffer-before))
        (t (push (clm396-test-condition condition root) cleanup-errors)))
      (condition-case condition
          (when (and root-owned root (file-exists-p root))
            (delete-directory root t))
        (t (push (clm396-test-condition condition root) cleanup-errors))))
    (let ((cleanup
           (list :new-buffers
                 (mapcar #'buffer-name
                         (seq-filter #'buffer-live-p
                                     (seq-difference (buffer-list)
                                                     buffers-before #'eq)))
                 :new-processes
                 (length (seq-difference (process-list) processes-before #'eq))
                 :new-timers
                 (length (seq-difference timer-list timers-before #'eq))
                 :new-frames
                 (length (seq-difference (frame-list) frames-before #'eq))
                 :root-exists (and root (file-exists-p root))
                 :ambient-global-restored
                 (eq global-command-log-mode ambient-global-mode)
                 :ambient-global-hook-restored
                 (equal after-change-major-mode-hook ambient-global-hook)
                 :ambient-modes-restored
                 (seq-every-p
                  (lambda (entry)
                    (or (not (buffer-live-p (plist-get entry :buffer)))
                        (with-current-buffer (plist-get entry :buffer)
                          (and (eq command-log-mode
                                   (plist-get entry :value))
                               (eq (local-variable-p 'command-log-mode)
                                   (plist-get entry :local))))))
                  ambient-modes)
                 :window-restored
                 (equal (clm396-test-window-state) window-state-before)
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Command Log Mode workflow failed: %S" (list result cleanup))
        (clm396-test-normalize
         (list :source clm396-test-source-sha256
               :result result :cleanup cleanup)
         root)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(COMMAND_LOG_MODE_MELPA_PIN, "command-log-mode.el")
        .expect("prepare exact shallow Command Log Mode source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn real_keyboard_commands_log_text_repetition_and_timestamp_properties() -> ParityBatchCase {
    ParityBatchCase::value(
        "real_keyboard_commands_log_text_repetition_and_timestamp_properties",
        r####"
(clm396-test-run
 (lambda (_root)
   (let ((buffer (generate-new-buffer "*clm396-commands*")))
     (set-window-buffer (selected-window) buffer)
     (set-buffer buffer)
     (clm/open-command-log-buffer t)
     (command-log-mode 1)
     (clm396-test-run-keys "a b C-c ! C-c !")
     (list :source-text (buffer-string)
           :mode command-log-mode
           :log (clm396-test-log-state)))))
"####,
        expect![[
            r#"OK (:source "f512e7cdc757b1776945bce74fca60f8805d4a50ceee76178b45e0cfa3597ff7" :result (:source-text "ab<deployed><deployed>" :mode t :log (:text "a\11   self-insert-command [2 times]\n[text: ab]\nC-c !\11   clm396-test-command [2 times]\n" :properties ((:range (1 6) :text "a\11   " :time "STAMP") (:range (6 47) :text "self-insert-command [2 times]\n[text: ab]\n" :time nil) (:range (47 56) :text "C-c !\11   " :time "STAMP") (:range (56 86) :text "clm396-test-command [2 times]\n" :time nil)) :point 86 :windows ((:dedicated t :point 86)))) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :ambient-global-restored t :ambient-global-hook-restored t :ambient-modes-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn repeat_policy_switches_between_merged_and_individual_command_lines() -> ParityBatchCase {
    ParityBatchCase::value(
        "repeat_policy_switches_between_merged_and_individual_command_lines",
        r####"
(clm396-test-run
 (lambda (_root)
   (let ((buffer (generate-new-buffer "*clm396-repeat*")))
     (set-window-buffer (selected-window) buffer)
     (set-buffer buffer)
     (clm/open-command-log-buffer t)
     (command-log-mode 1)
     (let ((clm/log-text nil)
           (clm/log-repeat nil))
       (clm396-test-run-keys "C-c ! C-c !")
       (let ((merged (clm396-test-log-state)))
         (clm/command-log-clear)
         (setq clm/last-keyboard-command nil
               clm/command-repetitions 0)
         (let ((clm/log-repeat t))
           (clm396-test-run-keys "C-c ! C-c !")
           (list :merged merged
                 :individual (clm396-test-log-state))))))))
"####,
        expect![[
            r#"OK (:source "f512e7cdc757b1776945bce74fca60f8805d4a50ceee76178b45e0cfa3597ff7" :result (:merged (:text "C-c !\11   clm396-test-command [2 times]\n" :properties ((:range (1 10) :text "C-c !\11   " :time "STAMP") (:range (10 40) :text "clm396-test-command [2 times]\n" :time nil)) :point 40 :windows ((:dedicated t :point 40))) :individual (:text "C-c !\11   clm396-test-command\nC-c !\11   clm396-test-command\n" :properties ((:range (1 10) :text "C-c !\11   " :time "STAMP") (:range (10 30) :text "clm396-test-command\n" :time nil) (:range (30 39) :text "C-c !\11   " :time "STAMP") (:range (39 59) :text "clm396-test-command\n" :time nil)) :point 59 :windows ((:dedicated t :point 59)))) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :ambient-global-restored t :ambient-global-hook-restored t :ambient-modes-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_toggle_opens_clears_and_closes_a_dedicated_log_window() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_toggle_opens_clears_and_closes_a_dedicated_log_window",
        r####"
(clm396-test-run
 (lambda (_root)
   (let ((buffer (generate-new-buffer "*clm396-toggle*"))
         (command-log-mode-open-log-turns-on-mode t)
         (command-log-mode-is-global nil))
     (set-window-buffer (selected-window) buffer)
     (set-buffer buffer)
     (setq clm/command-log-buffer (get-buffer-create " *command-log*"))
     (with-current-buffer clm/command-log-buffer (insert "stale\n"))
     (clm/toggle-command-log-buffer '(4))
     (let ((opened (list :mode command-log-mode
                         :log (clm396-test-log-state)
                         :windows (length (window-list)))))
       (clm/toggle-command-log-buffer)
       (list :opened opened
             :closed (list :mode command-log-mode
                           :visible (and (get-buffer-window
                                          clm/command-log-buffer) t)
                           :windows (length (window-list))))))))
"####,
        expect![[
            r#"OK (:source "f512e7cdc757b1776945bce74fca60f8805d4a50ceee76178b45e0cfa3597ff7" :result (:opened (:mode t :log (:text "" :properties nil :point 1 :windows ((:dedicated t :point 1))) :windows 2) :closed (:mode t :visible nil :windows 1)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :ambient-global-restored t :ambient-global-hook-restored t :ambient-modes-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn global_mode_enables_owned_buffers_then_restores_all_baseline_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "global_mode_enables_owned_buffers_then_restores_all_baseline_buffers",
        r####"
(clm396-test-run
 (lambda (_root)
   (let ((one (generate-new-buffer "*clm396-global-one*"))
         (two (generate-new-buffer "*clm396-global-two*")))
     (global-command-log-mode 1)
     (let ((enabled
            (list :global global-command-log-mode
                  :one (buffer-local-value 'command-log-mode one)
                  :two (buffer-local-value 'command-log-mode two))))
       (global-command-log-mode -1)
       (list :enabled enabled
             :disabled
             (list :global global-command-log-mode
                   :one (buffer-local-value 'command-log-mode one)
                   :two (buffer-local-value 'command-log-mode two)))))))
"####,
        expect![[
            r#"OK (:source "f512e7cdc757b1776945bce74fca60f8805d4a50ceee76178b45e0cfa3597ff7" :result (:enabled (:global t :one t :two t) :disabled (:global nil :one nil :two nil)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :ambient-global-restored t :ambient-global-hook-restored t :ambient-modes-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn save_failure_preserves_log_then_owned_directory_recovery_writes_and_clears() -> ParityBatchCase {
    ParityBatchCase::value(
        "save_failure_preserves_log_then_owned_directory_recovery_writes_and_clears",
        r####"
(clm396-test-run
 (lambda (root)
   (setq clm/command-log-buffer (get-buffer-create " *command-log*"))
   (with-current-buffer clm/command-log-buffer
     (insert (propertize "C-c !      clm396-test-command\n" :time "STAMP"))
     (insert "plain note\n"))
   (let ((failure
          (condition-case condition
              (progn (clm/save-command-log) :unexpected-success)
            (error (clm396-test-condition condition root)))))
     (let ((retained (clm396-test-log-state)))
       (make-directory root t)
       (clm/save-command-log)
       (let* ((files (directory-files root t "\\`log-"))
              (file (car files))
              (name (and file (file-name-nondirectory file))))
         (list :failure failure
               :retained retained
               :file-count (length files)
               :file-name-is-dated
               (and name
                    (string-match-p
                     "\\`log-[0-9]\\{4\\}-[0-9]\\{2\\}-[0-9]\\{2\\}\\'"
                     name)
                    t)
               :saved (and file
                           (with-temp-buffer
                             (insert-file-contents file)
                             (buffer-string)))
               :after (clm396-test-log-state)))))))
"####,
        expect![[
            r#"OK (:source "f512e7cdc757b1776945bce74fca60f8805d4a50ceee76178b45e0cfa3597ff7" :result (:failure (:type file-missing :data ("Opening output file" "No such file or directory" "[ROOT]/log-[DATE]") :message "Opening output file: No such file or directory, [ROOT]/log-[DATE]") :retained (:text "C-c !      clm396-test-command\nplain note\n" :properties ((:range (1 32) :text "C-c !      clm396-test-command\n" :time "STAMP") (:range (32 43) :text "plain note\n" :time nil)) :point 31 :windows nil) :file-count 1 :file-name-is-dated t :saved "[STAMP] C-c !      clm396-test-command\nplain note\n" :after (:text "" :properties nil :point 1 :windows nil)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :ambient-global-restored t :ambient-global-hook-restored t :ambient-modes-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn public_command_log_mode_workflows_match() {
    let cases: Vec<ParityBatchCase> = vec![
        real_keyboard_commands_log_text_repetition_and_timestamp_properties(),
        repeat_policy_switches_between_merged_and_individual_command_lines(),
        public_toggle_opens_clears_and_closes_a_dedicated_log_window(),
        global_mode_enables_owned_buffers_then_restores_all_baseline_buffers(),
        save_failure_preserves_log_then_owned_directory_recovery_writes_and_clears(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        std::thread::current()
            .name()
            .unwrap_or("unnamed Command Log Mode parity test"),
        "command-log-mode-rank396",
        &cases,
    );
}

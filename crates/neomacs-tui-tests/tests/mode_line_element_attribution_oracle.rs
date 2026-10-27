#![cfg(unix)]
//! GNU authority after the original D29 W100 TTY scene; raw outputs are retained.
//! Only bytecode documentation reached through property values is projected.
//! Run the owning test invocation through the campaign sandbox-run.sh.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use neomacs_tui_tests::{TuiLaunch, TuiProcessOutcome, TuiSession, TuiTerminalConfig};

// Copied verbatim from the D29 micro-prep/base-fixture.el that owns the
// initialized scene and original 100 x/DEL command pairs. No timing is used.
const ORIGINAL_W100: &str = r###";;; -*- lexical-binding: t; -*-
;; Input-driven adaptation of 09-24 diag-se / 10-01 latency.el.
;; Python injects actual x/DEL or C-n/C-p. ACK follows forced redisplay.
;; Suppress ordinary startup redisplay until the initial command-loop hook
;; has armed setup.  Recursive edit binds inhibit-redisplay, so the initial
;; hook below must also set its active binding before the first read.
(setq inhibit-startup-screen t inhibit-startup-message t inhibit-redisplay t)
(require 'json)
(require 'timer)
(defvar gnugap-kind (or (getenv "GNUGAP_KEY") "self-insert"))
(defvar gnugap-n (string-to-number (or (getenv "GNUGAP_N") "500")))
(defvar gnugap-warmup (string-to-number (or (getenv "GNUGAP_WARMUP") "50")))
(defvar gnugap-target-command nil)
(defvar gnugap-reset-command nil)
(defvar gnugap-phase 'sample)
(defvar gnugap-pairs 0)
(defvar gnugap-seq 0)
(defvar gnugap-start nil)
(defvar gnugap-size nil)
(defvar gnugap-point nil)
(defvar gnugap-checksum nil)
(defvar gnugap-scene nil)
(defvar gnugap-timers-ready nil)
(defvar gnugap-gc0 nil)
(defvar gnugap-gct0 nil)
(defvar gnugap-startup-barrier-count 0)
(defvar gnugap-startup-arm-count 0)
(defvar gnugap-startup-state 'loaded)
(defun gnugap--assert-disabled-native-modes ()
  (unless (and (boundp 'show-paren-mode) (not show-paren-mode)
               (boundp 'blink-cursor-mode) (not blink-cursor-mode))
    (error "Native fixture modes re-enabled: show-paren=%S blink-cursor=%S"
           (and (boundp 'show-paren-mode) show-paren-mode)
           (and (boundp 'blink-cursor-mode) blink-cursor-mode))))
(defun gnugap--timer-list-inventory (timers)
  ;; Preserve every listed timer, including currently triggered entries, and
  ;; expose armed status instead of silently dropping functions or duplicates.
  (let ((index 0) rows
        (print-circle t) (print-level nil) (print-length nil))
    (dolist (timer timers)
      (unless (timerp timer) (error "Malformed timer-list entry at %d" index))
      (let ((function (timer--function timer))
            (triggered (timer--triggered timer)))
        (push `((index . ,index)
                (function . ,(prin1-to-string function))
                (symbol . ,(if (symbolp function) (symbol-name function) :null))
                (triggered . ,(if triggered t :false))
                (armed . ,(if triggered :false t))
                (repeat . ,(prin1-to-string (timer--repeat-delay timer))))
              rows))
      (setq index (1+ index)))
    (vconcat (nreverse rows))))
(defun gnugap--timer-inventory ()
  (gnugap--assert-disabled-native-modes)
  `((schema . "ttykey-cont23-timers-v1")
    (show_paren_mode . :false)
    (blink_cursor_mode . :false)
    (ordinary . ,(gnugap--timer-list-inventory timer-list))
    (idle . ,(gnugap--timer-list-inventory timer-idle-list))))

;; CONT13 SOURCE DRAFT. Inventory-first: no guessed b121 or GNU timer names.
;; Copied from ttykey CONT26; this fixture/controller pair is not yet activated.
(defvar gnugap-policy-json nil)
(defvar gnugap-policy
  (with-temp-buffer
    (insert-file-contents (getenv "GNUGAP_SUPPRESSION_POLICY"))
    (setq gnugap-policy-json (buffer-string))
    (json-parse-buffer :object-type 'alist :array-type 'list
                       :null-object nil :false-object nil)))
(unless (and (equal (alist-get 'schema gnugap-policy) "editpath-cont13-declared-suppression-v1")
             (member (alist-get 'phase gnugap-policy) '("inventory" "cohort")))
  (error "Wrong explicit suppression plan"))
(when (and (equal (alist-get 'phase gnugap-policy) "inventory")
           (or (alist-get 'cancel_symbols gnugap-policy)
               (alist-get 'cancel_function_prints gnugap-policy)))
  (error "Inventory phase must not guess cancellation identities"))
(defvar gnugap-policy-snapshots nil)
(defvar gnugap-cancellations nil)
(defvar gnugap-handler-events nil)
(defvar gnugap-measurement-armed nil)
(defvar gnugap-command-active nil)
(defvar gnugap-handler-context nil)
(defvar gnugap-final-ack-inventory nil)
(defvar gnugap-release-observed nil)
(defvar gnugap-gc-ready nil)
(defvar gnugap-gc-final-ack nil)
(defun gnugap--policy-snapshot (label)
  (let ((snapshot
         `((schema . "ttykey-cont26-unfiltered-timers-v1")
           (show_paren_mode . ,(if (and (boundp 'show-paren-mode) show-paren-mode) t :false))
           (blink_cursor_mode . ,(if (and (boundp 'blink-cursor-mode) blink-cursor-mode) t :false))
           (eldoc_mode . ,(if (and (boundp 'eldoc-mode) eldoc-mode) t :false))
           (jit_lock_defer_time . ,(prin1-to-string jit-lock-defer-time))
           (jit_lock_stealth_time . ,(prin1-to-string jit-lock-stealth-time))
           (jit_lock_contextually . ,(prin1-to-string jit-lock-contextually))
           (jit_lock_antiblink_grace . ,(prin1-to-string jit-lock-antiblink-grace))
           (ordinary . ,(gnugap--timer-list-inventory timer-list))
           (idle . ,(gnugap--timer-list-inventory timer-idle-list)))))
    (push `((label . ,label) (timers . ,snapshot)) gnugap-policy-snapshots)
    snapshot))
(defun gnugap--cancel-declared-timers ()
  (let ((symbols (alist-get 'cancel_symbols gnugap-policy))
        (prints (alist-get 'cancel_function_prints gnugap-policy)) seen)
    (dolist (timer (append timer-list timer-idle-list))
      (unless (memq timer seen)
        (push timer seen)
        (let* ((fn (timer--function timer)) (printed (prin1-to-string fn))
               (symbol (and (symbolp fn) (symbol-name fn))))
          (when (or (and symbol (member symbol symbols)) (member printed prints))
            (push `((function . ,printed) (symbol . ,(or symbol :null))
                    (was_triggered . ,(if (timer--triggered timer) t :false)))
                  gnugap-cancellations)
            (cancel-timer timer)))))))
(defun gnugap--event-boundary ()
  (cond (gnugap-command-active "interior-command")
        ((and (> gnugap-seq 0) (< gnugap-seq (* 2 (+ gnugap-n gnugap-warmup))))
         "interior-between-commands")
        ((= gnugap-seq 0) "ambiguous-leading-control-gap")
        (t "ambiguous-trailing-control-gap")))
(defun gnugap--timer-handler-observer (original timer)
  ;; Record handler execution, never assert that timer.el applied its function.
  (let ((gnugap-handler-context t))
    (when gnugap-measurement-armed
      (let ((fn (timer--function timer)))
        (push `((kind . "timer-handler")
                (scheduled_function . ,(prin1-to-string fn))
                (scheduled_symbol . ,(if (symbolp fn) (symbol-name fn) :null))
                (sequence . ,gnugap-seq)
                (command_active . ,(if gnugap-command-active t :false))
                (boundary . ,(gnugap--event-boundary))) gnugap-handler-events)))
    (funcall original timer)))
(defun gnugap--async-entry (symbol)
  ;; Fixed asynchronous seams only.  Foreground fontification stays counted.
  (when gnugap-measurement-armed
    (push `((kind . "async-fontification-entry")
            (symbol . ,(symbol-name symbol))
            (sequence . ,gnugap-seq)
            (command_active . ,(if gnugap-command-active t :false))
            (boundary . ,(gnugap--event-boundary))) gnugap-handler-events)))
(defun gnugap--deferred-entry (&rest _args)
  (gnugap--async-entry 'jit-lock-deferred-fontify))
(defun gnugap--stealth-entry (&rest _args)
  (gnugap--async-entry 'jit-lock-stealth-fontify))
(defun gnugap--context-entry (&rest _args)
  (gnugap--async-entry 'jit-lock-context--update))
(defun gnugap--antiblink-entry (&rest _args)
  (gnugap--async-entry 'jit-lock--antiblink-update))
(advice-add 'timer-event-handler :around #'gnugap--timer-handler-observer)
(advice-add 'jit-lock-deferred-fontify :before #'gnugap--deferred-entry)
(advice-add 'jit-lock-stealth-fontify :before #'gnugap--stealth-entry)
(advice-add 'jit-lock-context--update :before #'gnugap--context-entry)
(advice-add 'jit-lock--antiblink-update :before #'gnugap--antiblink-entry)
(defun gnugap--gc-policy-snapshot ()
  ;; Observe startup's existing first-pre-command release; never move its hook
  ;; or assign collector thresholds.  The canceled backstop is redundant.
  `((gc_cons_threshold . ,gc-cons-threshold)
    (gc_cons_percentage . ,gc-cons-percentage)
    (startup_ceiling_bound . ,(if (boundp 'neomacs--startup-gc-ceiling-active) t :false))
    (startup_ceiling_active . ,(if (and (boundp 'neomacs--startup-gc-ceiling-active)
                                      neomacs--startup-gc-ceiling-active) t :false))))
(defun gnugap--fontification-settings ()
  ;; Preserve normal font-lock/jit-lock synchronous work and initial ensure.
  (setq jit-lock-defer-time nil jit-lock-stealth-time nil)
  (when (alist-get 'disable_context gnugap-policy)
    (setq-local jit-lock-contextually nil))
  (when (alist-get 'disable_antiblink gnugap-policy)
    (setq-local jit-lock-antiblink-grace nil)))
(defun gnugap--finish-after-release ()
  (interactive)
  (unless (and (getenv "GNUGAP_PERF_RELEASE")
               (file-exists-p (getenv "GNUGAP_PERF_RELEASE"))
               (= gnugap-pairs (+ gnugap-n gnugap-warmup)))
    (error "Exit key before profiler release/final reset"))
  (setq gnugap-release-observed t gnugap-measurement-armed nil)
  (gnugap--finish))

(defun gnugap--marker (s)
  (send-string-to-terminal (concat "\e]777;GNUGAP;" s "\a")))
(defun gnugap--fail (err)
  (gnugap--marker (format "ERROR;%S" err))
  (kill-emacs 2))

;; CONT14 explicit after-warmup scene fence, outside collection.
(defun gnugap--cont14-effective-hooks (symbol)
  (let ((value (symbol-value symbol)))
    (if (local-variable-p symbol)
        (let (result)
          (dolist (entry value)
            (setq result (append result (if (eq entry t)
                                            (default-value symbol)
                                          (list entry)))))
          result)
      value)))
(defun gnugap--cont14-scene-state ()
  (let ((print-circle t) (print-level nil) (print-length nil))
    `((buffer . ,(buffer-name)) (bytes . ,(buffer-size))
      (point . ,(point)) (window_start . ,(window-start))
      (window_end . ,(window-end nil nil))
      (point_min . ,(point-min)) (point_max . ,(point-max))
      (buffer_modified_tick . ,(buffer-modified-tick))
      (buffer_chars_modified_tick . ,(buffer-chars-modified-tick))
      (modified . ,(if (buffer-modified-p) t :false))
      (major_mode . ,(symbol-name major-mode))
      (width . ,(window-width)) (height . ,(window-height))
      (frame_width . ,(frame-width)) (frame_height . ,(frame-height))
      (font_lock_fontified . ,(prin1-to-string font-lock-fontified))
      (text_with_properties . ,(prin1-to-string (buffer-substring (point-min) (point-max))))
      (undo . ,(prin1-to-string buffer-undo-list))
      (pre_command_hook . ,(prin1-to-string pre-command-hook))
      (post_command_hook . ,(prin1-to-string post-command-hook))
      (effective_pre_command_hook . ,(prin1-to-string (gnugap--cont14-effective-hooks 'pre-command-hook)))
      (effective_post_command_hook . ,(prin1-to-string (gnugap--cont14-effective-hooks 'post-command-hook)))
      (jit_lock_defer_time . ,(prin1-to-string jit-lock-defer-time))
      (jit_lock_stealth_time . ,(prin1-to-string jit-lock-stealth-time)))))
(defun gnugap--cont14-measure-fence ()
  (unless (and (eq gnugap-phase 'reset)
               (= (1+ gnugap-seq) (* 2 gnugap-warmup))
               (= (point) gnugap-point) (= (buffer-size) gnugap-size))
    (error "Invalid CONT14 common warmup fence"))
  (unless (and (eq (car (gnugap--cont14-effective-hooks 'pre-command-hook)) #'gnugap--pre)
               (eq (car (last (gnugap--cont14-effective-hooks 'post-command-hook))) #'gnugap--post))
    (error "CONT14 effective hook order differs"))
  (let ((state (gnugap--cont14-scene-state))
        (timers (gnugap--timer-inventory)))
    (with-temp-file (expand-file-name "measurement-start.json" (getenv "CONT17_ARM"))
      (insert (json-serialize
               `((schema . "editpath-cont14-measurement-start-v1")
                 (warmup_pairs . ,gnugap-warmup) (scene . ,state)
                 (timers . ,timers) (collection . "starts after warmup ACK"))))))
  ;; Match the original startup GC cleanup, now outside the measured warm region.
  (garbage-collect)
  (setq gnugap-gc0 gcs-done gnugap-gct0 gc-elapsed
        gnugap-gc-ready (gnugap--gc-policy-snapshot)))

(defun gnugap--pre ()
  (setq inhibit-redisplay nil gnugap-command-active t gnugap-start (float-time)))
(defun gnugap--post ()
  (condition-case err
      (let* ((post (float-time))
             (command-us (round (* 1e6 (- post gnugap-start))))
             (expected (if (eq gnugap-phase 'sample)
                           gnugap-target-command gnugap-reset-command)))
        (if (equal gnugap-kind "open-line-kill-line")
            ;; GNU31.1 kill-region changes this-command during kill-line;
            ;; real-this-command retains the invoked C-k binding.
            (unless (and (eq real-this-command expected)
                         (eq this-command (if (eq gnugap-phase 'sample)
                                              'open-line 'kill-region)))
              (error "Unexpected multiline command %S/%S, invoked %S"
                     this-command real-this-command expected))
          (unless (eq this-command expected)
            (error "Unexpected command %S, expected %S" this-command expected)))
        (unless (eq (window-buffer (selected-window)) (current-buffer))
          (error "Edited buffer is not displayed"))
        (if (eq gnugap-phase 'reset)
            (unless (and (= (buffer-size) gnugap-size) (= (point) gnugap-point))
              (error "Reset did not restore point and buffer size"))
          (if (equal gnugap-kind "open-line-kill-line")
              (unless (and (= (buffer-size) (1+ gnugap-size))
                           (= (point) gnugap-point) (= (char-after) ?\n))
                (error "Open-line did not insert one newline while keeping point"))
            (if (equal gnugap-kind "self-insert")
                (unless (and (= (buffer-size) (1+ gnugap-size))
                             (= (point) (1+ gnugap-point)) (= (char-before) ?x))
                  (error "Insertion was not the requested x"))
              (unless (and (= (buffer-size) gnugap-size)
                           (if (equal gnugap-kind "c-p")
                               (< (point) gnugap-point)
                             (> (point) gnugap-point)))
                (error "%s did not move in its target direction" gnugap-kind)))))
        (let ((rd-start (float-time)))
          (redisplay t)
          (let ((redisplay-us (round (* 1e6 (- (float-time) rd-start)))))
            (when (and (eq gnugap-phase 'reset)
                       (= (1+ gnugap-seq) (* 2 gnugap-warmup)))
              (gnugap--cont14-measure-fence))
            (when (and (eq gnugap-phase 'reset)
                       (= (1+ gnugap-pairs) (+ gnugap-warmup gnugap-n)))
              (setq gnugap-final-ack-inventory (gnugap--policy-snapshot "final-ack")
                    gnugap-gc-final-ack (gnugap--gc-policy-snapshot)))
            (setq inhibit-redisplay t
                  gnugap-seq (1+ gnugap-seq) gnugap-command-active nil)
            (gnugap--marker (format "ACK;%d;%s;%d;%d" gnugap-seq
                                    gnugap-phase command-us redisplay-us))))
        (if (eq gnugap-phase 'sample)
            (setq gnugap-phase 'reset)
          (setq gnugap-pairs (1+ gnugap-pairs) gnugap-phase 'sample)
          (when (= gnugap-pairs (+ gnugap-warmup gnugap-n))
            (remove-hook 'pre-command-hook #'gnugap--pre)
            (remove-hook 'post-command-hook #'gnugap--post)
            ;; No finish timer: controller sends the bound exit key ONLY
            ;; after --instr=off has returned and release file is written.
            nil)))
    (error (gnugap--fail err))))
(defun gnugap--finish ()
  (condition-case err
      (if (not gnugap-release-observed)
          (error "Finish requires explicit post-profiler exit command")
        (unless (equal gnugap-checksum
                       (secure-hash 'sha256 (get-buffer "gnugap-visible-1000")))
          (error "Final checksum differs"))
        (gnugap--assert-disabled-native-modes)
        (with-temp-file (getenv "GNUGAP_SCENE_OUT")
          (insert (json-serialize
                   `((status . "valid") (kind . ,gnugap-kind)
                     (scene . ,gnugap-scene)
                     (cohort_policy .
                       ((schema . "ttykey-cont26-timer-evidence-v1")
                        (plan_json . ,gnugap-policy-json)
                        (snapshots . ,(vconcat (nreverse gnugap-policy-snapshots)))
                        (cancellations . ,(vconcat (nreverse gnugap-cancellations)))
                        (handler_events . ,(vconcat (nreverse gnugap-handler-events)))
                        (final_ack . ,gnugap-final-ack-inventory)
                        (release_observed . ,(if gnugap-release-observed t :false))
                        (gc_policy . ((ready . ,gnugap-gc-ready)
                                      (final_ack . ,gnugap-gc-final-ack)
                                      (finish . ,(gnugap--gc-policy-snapshot))))))
                     (timer_policy .
                       ((disabled_modes . ["show-paren-mode" "blink-cursor-mode"])
                        (ready . ,gnugap-timers-ready)
                        (finish . ,(gnugap--timer-inventory))))
                     (buffer_restored . t)
                     (gcs . ,(- gcs-done gnugap-gc0))
                     (gc_us . ,(round (* 1e6 (- gc-elapsed gnugap-gct0))))))))
        (write-region "done\n" nil (getenv "SENTINEL") nil 'silent)
        (setq inhibit-redisplay nil)
        (kill-emacs 0))
    (error (gnugap--fail err))))
(defun gnugap--setup ()
  (condition-case err
      (progn
        (unless (and (eq gnugap-startup-state 'armed)
                     (= gnugap-startup-barrier-count 1)
                     (= gnugap-startup-arm-count 1)
                     inhibit-redisplay
                     (not (memq #'gnugap--startup-redisplay-barrier post-command-hook))
                     (not (memq #'gnugap--arm-startup-setup post-command-hook)))
          (error "Startup hooks did not establish the one-shot barrier"))
        (setq gnugap-startup-state 'setting-up)
        (when (fboundp 'menu-bar-mode) (menu-bar-mode -1))
        (setq inhibit-startup-screen t inhibit-startup-message t)
        (switch-to-buffer (get-buffer-create "gnugap-visible-1000"))
        (dotimes (i 1000) (insert (format "(message \"line %04d\")\n" i)))
        (gnugap--fontification-settings)
        (emacs-lisp-mode)
        (gnugap--fontification-settings)
        (local-set-key (kbd "C-]") #'gnugap--finish-after-release)
        (unless (and (commandp 'gnugap--finish-after-release)
                     (eq (key-binding (kbd "C-]")) 'gnugap--finish-after-release))
          (error "Post-profiler exit command is not interactive/bound"))
        (gnugap--policy-snapshot "after-major-mode-hooks")

        ;; Both artifact arms load this same fixture. Disable only the two
        ;; explicitly requested modes after major-mode hooks have completed.
        (show-paren-mode -1)
        (blink-cursor-mode -1)
        (when (alist-get 'disable_eldoc gnugap-policy)
          (eldoc-mode -1))
        (gnugap--policy-snapshot "after-mode-off")
        (font-lock-ensure (point-min) (point-max))
        (gnugap--policy-snapshot "after-font-lock-ensure")
        (gnugap--cancel-declared-timers)
        (gnugap--policy-snapshot "after-declared-cancellation")
        (goto-char (point-min)) (forward-line 499)
        (set-window-start (selected-window) (point))
        (forward-line 10) (end-of-line)
        (when (equal gnugap-kind "open-line-kill-line")
          (move-to-column 9))
        (setq gnugap-size (buffer-size) gnugap-point (point)
              gnugap-checksum (secure-hash 'sha256 (current-buffer))
              gnugap-target-command (key-binding (if (equal gnugap-kind "open-line-kill-line") "\C-o"
                                     (if (equal gnugap-kind "self-insert") "x"
                                       (if (equal gnugap-kind "c-p") "\C-p" "\C-n"))))
              gnugap-reset-command (key-binding (if (equal gnugap-kind "open-line-kill-line") "\C-k"
                                    (if (equal gnugap-kind "self-insert") "\177"
                                      (if (equal gnugap-kind "c-p") "\C-n" "\C-p")))))
        (unless (eq gnugap-target-command
                    (if (equal gnugap-kind "open-line-kill-line") 'open-line
                      (if (equal gnugap-kind "self-insert") 'self-insert-command
                        (if (equal gnugap-kind "c-p") 'previous-line 'next-line))))
          (error "Unexpected target binding: %S" gnugap-target-command))
        (unless (if (equal gnugap-kind "open-line-kill-line")
                    (eq gnugap-reset-command 'kill-line)
                  (or (equal gnugap-kind "self-insert")
                      (eq gnugap-reset-command
                          (if (equal gnugap-kind "c-p") 'next-line 'previous-line))))
          (error "Unexpected reset binding: %S" gnugap-reset-command))
        ;; Release only after the original scene/binding preparation completed.
        (setq inhibit-redisplay nil)
        (message nil) (redisplay t)
        (setq gnugap-scene `((lines . ,(count-lines (point-min) (point-max)))
                              (bytes . ,gnugap-size) (point . ,gnugap-point)
                              (window_start . ,(window-start))
                              (window_width . ,(window-width))
                              (window_height . ,(window-height))
                              (major_mode . ,(symbol-name major-mode))
                              (target . ,(symbol-name gnugap-target-command))
                              (reset . ,(symbol-name gnugap-reset-command))))
        (garbage-collect)
        (setq gnugap-gc0 gcs-done gnugap-gct0 gc-elapsed)
        (add-hook 'pre-command-hook #'gnugap--pre)
        (add-hook 'post-command-hook #'gnugap--post t)
        (unless (and (null inhibit-redisplay)
                     (= gnugap-startup-barrier-count 1)
                     (= gnugap-startup-arm-count 1))
          (error "Startup barrier remains active at READY"))
        (gnugap--assert-disabled-native-modes)
        (setq gnugap-gc-ready (gnugap--gc-policy-snapshot))
        (setq gnugap-timers-ready (gnugap--policy-snapshot "ready")
              gnugap-measurement-armed t)
        (setq inhibit-redisplay t gnugap-startup-state 'ready)
        (gnugap--marker "READY"))
    (error (gnugap--fail err))))
(defun gnugap--startup-redisplay-barrier ()
  (condition-case err
      (progn
        (unless (and (eq gnugap-startup-state 'loaded)
                     (= gnugap-startup-barrier-count 0)
                     (= gnugap-startup-arm-count 0))
          (error "Unexpected startup barrier invocation"))
        (remove-hook 'post-command-hook #'gnugap--startup-redisplay-barrier)
        (setq gnugap-startup-barrier-count 1
              gnugap-startup-state 'barrier
              inhibit-redisplay t))
    (error (gnugap--fail err))))
(defun gnugap--arm-startup-setup ()
  (condition-case err
      (progn
        (unless (and (eq gnugap-startup-state 'barrier)
                     (= gnugap-startup-barrier-count 1)
                     (= gnugap-startup-arm-count 0))
          (error "Startup setup was not armed after its barrier"))
        (remove-hook 'post-command-hook #'gnugap--arm-startup-setup)
        (setq gnugap-startup-arm-count 1
              gnugap-startup-state 'armed
              inhibit-redisplay t)
        ;; Arm only at the end of the initial hook chain, after loop entry.
        ;; READY still comes from setup's timer, never directly from this hook.
        (run-at-time 0 nil #'gnugap--setup))
    (error (gnugap--fail err))))
(add-hook 'post-command-hook #'gnugap--startup-redisplay-barrier)
(add-hook 'post-command-hook #'gnugap--arm-startup-setup t)
"###;

const POLICY: &str = r#"{
  "schema":"editpath-cont13-declared-suppression-v1","phase":"cohort",
  "cancel_symbols":["eldoc--update","jit-lock-context--update",
    "neomacs--release-startup-gc-ceiling","show-paren-function","undo-auto--boundary-timer"],
  "cancel_function_prints":[],"disable_context":true,
  "disable_antiblink":false,"disable_eldoc":true
}"#;

const STRICT_PROBE: &str = r###"
(defvar ep30-strict-effects 0)
(defun ep30-strict-counter ()
  (setq ep30-strict-effects (1+ ep30-strict-effects)) "EV")
(defun ep30-strict-print (value)
  (let ((print-circle t) (print-level nil) (print-length nil))
    (prin1-to-string value)))
(defun ep30-strict-marker (marker)
  (cond ((equal marker "READY")
         (with-temp-file (getenv "EP30_READY") (insert "ready\n")))
        ((string-prefix-p "ACK;" marker)
         (with-temp-file (getenv "EP30_ACK")
           (insert (number-to-string gnugap-seq) "\n")))))
(advice-add 'gnugap--marker :after #'ep30-strict-marker)
(defun ep30-strict-collect (&rest ignored)
  (unless (and (null noninteractive) (null window-system)
               (= gnugap-pairs 100) (= gnugap-seq 200)
               (= (window-width) 80) (= (window-height) 23)
               (= (point) gnugap-point) (= (buffer-size) gnugap-size))
    (error "Strict comparison requires the actual initialized TTY/W100 scene"))
  (let* ((window (selected-window)) (buffer (current-buffer))
         (targets (list (list "full" mode-line-format)))
         (index 0) (reference-file (getenv "EP30_REFERENCE"))
         (reference (when reference-file
                      (with-temp-buffer
                        (insert-file-contents reference-file)
                        (read (current-buffer)))))
         (observations nil) (reference-output nil)
         (raw-strict-passes 0))
    (dolist (element mode-line-format)
      (setq targets (append targets (list (list (format "element-%02d" index) element)))
            index (1+ index)))
    (setq targets
          (append targets
                  (list (list "percent-specs" "[%l|%c|%p|%I]")
                        (list "eval-constant" '(:eval (ep30-strict-counter)))
                        (list "empty-string" "") (list "loop-control" ""))))
    (unless (= (length targets) 22) (error "Default target count changed"))
    (when reference-file
      (unless (= (length reference) 22) (error "GNU must supply all 22 outputs")))
    (dolist (target targets)
      (let* ((name (car target)) (form (cadr target))
             (effects-before ep30-strict-effects)
             (value (format-mode-line form nil window buffer))
             (printed (ep30-strict-print value)) (roundtrip (read printed))
             (expected (assoc name reference)))
        (unless (and (stringp value) (stringp roundtrip)
                     (equal-including-properties value roundtrip))
          (error "Unfiltered property-bearing print/read failed: %s" name))
        (when (equal name "full")
          (unless (> (length value) 0) (error "Full formatter was a batch no-op")))
        (when (equal name "eval-constant")
          (unless (= ep30-strict-effects (1+ effects-before))
            (error "Counting eval did not execute exactly once")))
        ;; Collect every strict outcome before failing the native test, so a
        ;; failure identifies all original targets without filtering output.
        (let ((source-equal (or (not reference-file)
                                (and expected
                                     (equal-including-properties
                                      form (read (cadr expected))))))
              (raw-output-equal (or (not reference-file)
                                (and expected
                                     (equal-including-properties
                                      value (nth 2 expected))))))
          (when (and source-equal raw-output-equal)
            (setq raw-strict-passes (1+ raw-strict-passes)))
        (push (list name (ep30-strict-print form) value) reference-output)
        (push `((target . ,name) (source . ,(ep30-strict-print form))
                (output . ,printed) (chars . ,(length value))
                (bytes . ,(string-bytes value)) (strict_roundtrip . t)
                (source_equal_including_properties . ,(if source-equal t :false))
                (equal_including_properties . ,(if raw-output-equal t :false))) observations))))
    (with-temp-file (getenv "EP30_VALUES")
      (insert (ep30-strict-print (nreverse reference-output))))
    (with-temp-file (getenv "EP30_RESULT")
      (insert (json-serialize
               `((schema . "mode-line-element-attribution-w100-v1")
                 (status . ,(if (= raw-strict-passes 22)
                                "PASS_RAW_PROPERTIES"
                              "FAIL_RAW_PROPERTIES"))
                 (raw_strict_passes . ,raw-strict-passes)
                 (emacs_version . ,emacs-version)
                 (warmup_pairs . ,gnugap-pairs) (commands . ,gnugap-seq)
                 (outputs . 22) (point . ,(point)) (window_start . ,(window-start))
                 (width . ,(window-width)) (height . ,(window-height))
                 (entries . ,(vconcat (nreverse observations)))))))))
(advice-add 'gnugap--finish :before #'ep30-strict-collect)
"###;

struct Observation {
    values: String,
    result: serde_json::Value,
}

fn wait_for_file(session: &mut TuiSession, path: &Path) {
    session.read_until(Duration::from_secs(60), |_| path.exists());
    assert!(
        path.exists(),
        "owned fixture did not reach {}: {:?}",
        path.display(),
        session.text_grid()
    );
}

fn run_probe(name: &str, program: PathBuf, reference: Option<&str>) -> Observation {
    let home = neomacs_tui_tests::TuiTempDirectory::new("neomacs-element-attribution-");
    let script = home.path().join("probe.el");
    let policy = home.path().join("policy.json");
    let ready = home.path().join("ready");
    let ack = home.path().join("ack");
    let release = home.path().join("release");
    let values = home.path().join("values.el");
    let result = home.path().join("result.json");
    std::fs::write(&script, format!("{ORIGINAL_W100}\n{STRICT_PROBE}"))
        .expect("write owned fixture");
    std::fs::write(&policy, POLICY).expect("write original suppression policy");
    let mut launch = TuiLaunch::new(program.as_os_str()).arg("-nw").arg("-Q");
    if reference.is_none() {
        launch = launch.arg("-no-comp-spawn")
            .arg("--eval=(progn(set'native-comp-jit-compilation())(set'native-comp-async-report-warnings-errors'silent))");
    }
    launch = launch
        .arg("-l")
        .arg(&script)
        .env("GNUGAP_KEY", "self-insert")
        .env("GNUGAP_N", "0")
        .env("GNUGAP_WARMUP", "100")
        .env("GNUGAP_SUPPRESSION_POLICY", &policy)
        .env("GNUGAP_PERF_RELEASE", &release)
        .env("GNUGAP_SCENE_OUT", home.path().join("scene.json"))
        .env("SENTINEL", home.path().join("done"))
        .env("CONT17_ARM", home.path())
        .env("EP30_READY", &ready)
        .env("EP30_ACK", &ack)
        .env("EP30_VALUES", &values)
        .env("EP30_RESULT", &result);
    if let Some(reference) = reference {
        let path = home.path().join("gnu-values.el");
        std::fs::write(&path, reference).expect("persist exact GNU strings with all properties");
        launch = launch.env("EP30_REFERENCE", path);
    }
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        name,
        TuiTerminalConfig::new("xterm-256color", 24, 80),
    );
    wait_for_file(&mut session, &ready);
    let deadline = Instant::now() + Duration::from_secs(180);
    for sequence in 1..=200 {
        assert!(Instant::now() < deadline, "bounded W100 deadline expired");
        session.send(if sequence % 2 == 1 { b"x" } else { b"\x7f" });
        let expected = format!("{sequence}\n");
        session.read_until(Duration::from_secs(5), |_| {
            std::fs::read_to_string(&ack).is_ok_and(|actual| actual == expected)
        });
        assert_eq!(
            std::fs::read_to_string(&ack).ok().as_deref(),
            Some(expected.as_str()),
            "{name} command {sequence} did not acknowledge"
        );
    }
    std::fs::write(&release, "release\n").expect("release original finish command");
    session.send(b"\x1d");
    assert_eq!(
        session.run_to_completion(Duration::from_secs(30)),
        TuiProcessOutcome::Exited,
        "{name} did not finish the strict fixture"
    );
    let values = std::fs::read_to_string(values).expect("full readable property-bearing outputs");
    let result: serde_json::Value =
        serde_json::from_slice(&std::fs::read(result).expect("strict GNU/Neo comparison result"))
            .expect("result JSON");
    assert_eq!(
        result["status"],
        "PASS_RAW_PROPERTIES",
        "{name} unfiltered property outcomes: {}",
        serde_json::to_string_pretty(&result).expect("readable strict outcome")
    );
    assert_eq!(result["warmup_pairs"], 100);
    assert_eq!(result["commands"], 200);
    assert_eq!(result["outputs"], 22);
    assert_eq!(result["raw_strict_passes"], 22);
    let entries = result["entries"].as_array().expect("all target records");
    assert_eq!(entries.len(), 22);
    for entry in entries {
        assert_eq!(entry["strict_roundtrip"], true);
        assert_eq!(entry["source_equal_including_properties"], true);
        assert_eq!(entry["equal_including_properties"], true);
    }
    Observation { values, result }
}

#[test]
fn mode_line_element_attribution_all_22_default_w100_outputs_equal_gnu_properties() {
    let gnu_program = std::env::var_os("ORACLE_EMACS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("emacs"));
    let gnu = run_probe("GNU", gnu_program, None);
    assert_eq!(gnu.result["emacs_version"], "31.1", "pinned GNU authority");
    let program = neomacs_tui_tests::neomacs_binary();
    assert!(
        program.exists(),
        "Neomacs binary missing: {}",
        program.display()
    );
    let neo = run_probe("Neomacs", program, Some(&gnu.values));
    for key in [
        "warmup_pairs",
        "commands",
        "outputs",
        "point",
        "window_start",
        "width",
        "height",
    ] {
        assert_eq!(neo.result[key], gnu.result[key], "native scene field {key}");
    }
    let gnu_targets: Vec<_> = gnu.result["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["target"].as_str().unwrap())
        .collect();
    let neo_targets: Vec<_> = neo.result["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["target"].as_str().unwrap())
        .collect();
    assert_eq!(neo_targets, gnu_targets, "all targets in original order");
}

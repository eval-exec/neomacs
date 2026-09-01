use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, FANCY_BATTERY_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'fancy-battery)

(defun neomacs-fancy-battery-test-display (value)
  "Describe VALUE's visible text and face without losing either."
  (when value
    (list :text (substring-no-properties value)
          :face (get-text-property 0 'face value))))
"####;

fn charging_discharging_and_critical_states_assemble_face_annotated_segments() -> ParityBatchCase {
    let elisp_form = r####"
(mapcar
 (lambda (scenario)
   (let ((name (car scenario))
         (fancy-battery-last-status (cadr scenario))
         (fancy-battery-show-percentage (caddr scenario)))
     (list name
           :direct
           (neomacs-fancy-battery-test-display
            (fancy-battery-default-mode-line)))))
 '((charging ((?b . "+") (?t . "1:20") (?p . "72")) nil)
   (charging-percentage ((?b . "+") (?t . "1:20") (?p . "72")) t)
   (discharging ((?b . "-") (?t . "3:10") (?p . "58")) nil)
   (critical ((?b . "!") (?t . "0:09") (?p . "6")) nil)))
"####;
    let expected = expect![[
        r#"OK ((charging :direct (:text "1:20" :face fancy-battery-charging)) (charging-percentage :direct (:text "72%%" :face fancy-battery-charging)) (discharging :direct (:text "3:10" :face fancy-battery-discharging)) (critical :direct (:text "0:09" :face fancy-battery-critical)))"#
    ]];
    ParityBatchCase::value(
        "charging_discharging_and_critical_states_assemble_face_annotated_segments",
        elisp_form,
        expected,
    )
}

fn unavailable_time_and_partial_backend_data_fall_back_predictably() -> ParityBatchCase {
    let elisp_form = r####"
(mapcar
 (lambda (scenario)
   (let ((name (car scenario))
         (fancy-battery-last-status (cadr scenario))
         (fancy-battery-show-percentage (caddr scenario)))
     (list name
           :direct
           (neomacs-fancy-battery-test-display
            (fancy-battery-default-mode-line)))))
 '((time-unavailable ((?b . "+") (?t . "N/A") (?p . "43")) nil)
   (missing-percentage ((?b . "-") (?t . "N/A")) nil)
   (missing-time ((?b . "-") (?p . "81")) nil)
   (no-status nil nil)))
"####;
    let expected = expect![[
        r#"OK ((time-unavailable :direct (:text "43%%" :face fancy-battery-charging)) (missing-percentage :direct (:text "N/A" :face error)) (missing-time :direct (:text "N/A" :face error)) (no-status :direct nil))"#
    ]];
    ParityBatchCase::value(
        "unavailable_time_and_partial_backend_data_fall_back_predictably",
        elisp_form,
        expected,
    )
}

fn status_updates_query_once_cache_and_notify_every_registered_consumer() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((statuses
        (list '((?b . "+") (?t . "0:45") (?p . "88"))
              '((?b . "!") (?t . "0:05") (?p . "4"))))
       (backend-calls 0)
       (hook-events nil)
       (repaints nil)
       (battery-status-function
        (lambda ()
          (setq backend-calls (1+ backend-calls))
          (copy-tree (pop statuses))))
       (fancy-battery-last-status 'stale)
       (fancy-battery-status-update-functions
        (list (lambda (status)
                (push (list :percent (cdr (assq ?p status))
                            :state (cdr (assq ?b status))
                            :cached-during-hook
                            (equal status fancy-battery-last-status))
                      hook-events)))))
  (cl-letf (((symbol-function 'force-mode-line-update)
             (lambda (&optional all)
               (push all repaints))))
    (fancy-battery-update)
    (let ((first-render
           (neomacs-fancy-battery-test-display
            (fancy-battery-default-mode-line))))
      (fancy-battery-update)
      (list :backend-calls backend-calls
            :first-render first-render
            :second-render
            (neomacs-fancy-battery-test-display
             (fancy-battery-default-mode-line))
            :cached-state (cdr (assq ?b fancy-battery-last-status))
            :cached-percent (cdr (assq ?p fancy-battery-last-status))
            :hook-events (nreverse hook-events)
            :repaints (nreverse repaints)))))
"####;
    let expected = expect![[
        r#"OK (:backend-calls 2 :first-render (:text "0:45" :face fancy-battery-charging) :second-render (:text "0:05" :face fancy-battery-critical) :cached-state "!" :cached-percent "4" :hook-events ((:percent "88" :state "+" :cached-during-hook t) (:percent "4" :state "!" :cached-during-hook t)) :repaints (all all))"#
    ]];
    ParityBatchCase::value(
        "status_updates_query_once_cache_and_notify_every_registered_consumer",
        elisp_form,
        expected,
    )
}

fn global_mode_enable_reenable_and_disable_manage_one_mode_line_entry_and_timer() -> ParityBatchCase
{
    let elisp_form = r####"
(let ((saved-mode (default-value 'fancy-battery-mode)))
  (unwind-protect
      (let* ((global-mode-string '("clock"))
             (battery-status-function
              (lambda () '((?b . "-") (?t . "2:30") (?p . "64"))))
             (battery-update-interval 45)
             (fancy-battery-timer 'preexisting-timer)
             (scheduled nil)
             (cancelled nil)
             first-state
             second-state)
        (setq-default fancy-battery-mode nil)
        (cl-letf (((symbol-function 'run-at-time)
                   (lambda (time repeat function &rest arguments)
                     (let ((timer (intern
                                   (format "timer-%d" (1+ (length scheduled))))))
                       (push (list time repeat function arguments timer)
                             scheduled)
                       timer)))
                  ((symbol-function 'cancel-timer)
                   (lambda (timer) (push timer cancelled))))
          (fancy-battery-mode 1)
          (setq first-state
                (list :enabled (default-value 'fancy-battery-mode)
                      :global-mode-string (copy-tree global-mode-string)
                      :timer fancy-battery-timer))
          (fancy-battery-mode 1)
          (setq second-state
                (list :enabled (default-value 'fancy-battery-mode)
                      :global-mode-string (copy-tree global-mode-string)
                      :timer fancy-battery-timer))
          (fancy-battery-mode -1)
          (list :first first-state
                :second second-state
                :disabled (default-value 'fancy-battery-mode)
                :final-global-mode-string global-mode-string
                :scheduled (nreverse scheduled)
                :cancelled (nreverse cancelled)
                :timer-after-disable fancy-battery-timer)))
    (setq fancy-battery-timer nil)
    (setq-default fancy-battery-mode saved-mode)))
"####;
    let expected = expect![[
        r#"OK (:first (:enabled t :global-mode-string ("clock" fancy-battery-mode-line) :timer timer-1) :second (:enabled t :global-mode-string ("clock" fancy-battery-mode-line) :timer timer-2) :disabled nil :final-global-mode-string ("clock") :scheduled ((nil 45 fancy-battery-update nil timer-1) (nil 45 fancy-battery-update nil timer-2)) :cancelled (preexisting-timer timer-1 timer-2) :timer-after-disable timer-2)"#
    ]];
    ParityBatchCase::value(
        "global_mode_enable_reenable_and_disable_manage_one_mode_line_entry_and_timer",
        elisp_form,
        expected,
    )
}

fn unavailable_backend_refuses_to_enable_without_leaving_mode_line_state() -> ParityBatchCase {
    let elisp_form = r####"
(let ((saved-mode (default-value 'fancy-battery-mode)))
  (unwind-protect
      (let ((global-mode-string nil)
            (battery-status-function nil)
            (fancy-battery-timer nil))
        (setq-default fancy-battery-mode nil)
        (fancy-battery-mode 1)
        (list :enabled (default-value 'fancy-battery-mode)
              :global-mode-string global-mode-string
              :battery-entry-present
              (memq 'fancy-battery-mode-line global-mode-string)
              :timer fancy-battery-timer))
    (setq fancy-battery-timer nil)
    (setq-default fancy-battery-mode saved-mode)))
"####;
    let expected = expect![[
        r#"OK (:enabled nil :global-mode-string ("") :battery-entry-present nil :timer nil)"#
    ]];
    ParityBatchCase::value(
        "unavailable_backend_refuses_to_enable_without_leaving_mode_line_state",
        elisp_form,
        expected,
    )
}

#[test]
fn fancy_battery_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(FANCY_BATTERY_MELPA_PIN, "fancy-battery.el")
            .expect("prepare revision-pinned Fancy Battery source below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "fancy-battery-package-batch",
        "Fancy Battery",
        &[
            charging_discharging_and_critical_states_assemble_face_annotated_segments(),
            unavailable_time_and_partial_backend_data_fall_back_predictably(),
            status_updates_query_once_cache_and_notify_every_registered_consumer(),
            global_mode_enable_reenable_and_disable_manage_one_mode_line_entry_and_timer(),
            unavailable_backend_refuses_to_enable_without_leaving_mode_line_state(),
        ],
    );
}

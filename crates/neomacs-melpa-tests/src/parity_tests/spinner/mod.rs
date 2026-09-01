use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, SPINNER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const SPINNER_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const SPINNER_TEST_PRELUDE: &str = r#####"
(require 'cl-lib)
(require 'spinner)

(defun spinner-parity-cancel-package-timers ()
  (dolist (timer (copy-sequence timer-list))
    (when (eq (timer--function timer) #'spinner--timer-function)
      (cancel-timer timer))))

(spinner-parity-cancel-package-timers)
"#####;

fn spinner_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SPINNER_MELPA_PIN, "spinner.el")
        .expect("prepare pinned Spinner source below ./tmp")
        .with_prelude(SPINNER_TEST_PRELUDE)
        .with_timeout(SPINNER_TEST_TIMEOUT)
}

fn configuration_builds_builtin_custom_and_generated_animation_frames() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (spinner-parity-cancel-package-timers)
  (let* ((spinner-frames-per-second 12)
         (default (spinner-create))
         (moon (spinner-create 'moon nil 5 2))
         (custom (spinner-create ["queued" "uploading" "published"] nil 3))
         (bar (spinner-make-progress-bar 4 ?#))
         (invalid (spinner-create 'triangle nil 0))
         invalid-result)
    (setq invalid-result
          (condition-case error
              (progn (spinner-start invalid) :unexpected-success)
            (error (list (car error) (cadr error)))))
    (unwind-protect
        (list
         :default
         (list (spinner--frames default)
               (spinner--fps default)
               (spinner--delay default)
               (spinner--counter default)
               (spinner--active-p default)
               (spinner-print default))
         :moon
         (list (spinner--frames moon)
               (spinner--fps moon)
               (spinner--delay moon))
         :custom (spinner--frames custom)
         :progress-bar bar
         :timers
         (list (timerp (spinner--timer default))
               (memq (spinner--timer default) timer-list)
               (memq (spinner--timer moon) timer-list))
         :invalid
         (list invalid-result
               (spinner--active-p invalid)
               (memq (spinner--timer invalid) timer-list)))
      (spinner-stop invalid))))
"#####;
    let expect = expect![[
        r######"OK (:default (["┤" "┘" "┴" "└" "├" "┌" "┬" "┐"] 12 0 0 nil nil) :moon (["🌑" "🌘" "🌗" "🌖" "🌕" "🌔" "🌓" "🌒"] 5 2) :custom ["queued" "uploading" "published"] :progress-bar ["    " "#   " "##  " "### " "####" " ###" "  ##" "   #"] :timers (t nil nil) :invalid ((error "A spinner’s FPS must be a positive number") t nil))"######
    ]];
    ParityBatchCase::value(
        "configuration_builds_builtin_custom_and_generated_animation_frames",
        elisp_form,
        expect,
    )
}

fn major_mode_spinner_reuses_mode_line_slot_timer_and_stopper() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (spinner-parity-cancel-package-timers)
  (with-temp-buffer
    (let ((target (current-buffer))
          (mode-line-process '(" jobs:1"))
          updates first-spinner first-timer first-stopper second-stopper result)
      (cl-letf (((symbol-function 'force-mode-line-update)
                 (lambda (&optional all)
                   (push (list (eq (current-buffer) target) all) updates)
                   all)))
        (unwind-protect
            (progn
              (setq first-stopper (spinner-start 'rotating-line 8 0)
                    first-spinner spinner-current
                    first-timer (spinner--timer spinner-current))
              (setq second-stopper (spinner-start 'triangle 4 0))
              (spinner--timer-function spinner-current)
              (setq result
                    (list
                     :same-object (eq first-spinner spinner-current)
                     :same-timer (eq first-timer (spinner--timer spinner-current))
                     :stoppers
                     (list (functionp first-stopper) (functionp second-stopper))
                     :mode-line mode-line-process
                     :construct-count
                     (cl-count 'spinner--mode-line-construct
                               mode-line-process :test #'eq)
                     :frames (spinner--frames spinner-current)
                     :state
                     (list (spinner--active-p spinner-current)
                           (spinner--counter spinner-current)
                           (spinner-print spinner-current)
                           (timer--repeat-delay (spinner--timer spinner-current))
                           (eq (timer--function (spinner--timer spinner-current))
                               #'spinner--timer-function)
                           (eq (car (timer--args (spinner--timer spinner-current)))
                               spinner-current)
                           (and (memq (spinner--timer spinner-current) timer-list) t))
                     :updates-before-stop (nreverse updates)))
              (setq updates nil)
              (funcall first-stopper)
              (append
               result
               (list
                :after-stop
                (list (spinner--active-p spinner-current)
                      (spinner-print spinner-current)
                      (memq (spinner--timer spinner-current) timer-list)
                      (nreverse updates)))))
          (spinner-stop spinner-current))))))
"#####;
    let expect = expect![[
        r####"OK (:same-object t :same-timer t :stoppers (t t) :mode-line ((" jobs:1") spinner--mode-line-construct) :construct-count 1 :frames ["◢" "◣" "◤" "◥"] :state (t 1 "◣" 0.25 t t t) :updates-before-stop ((t nil)) :after-stop (nil nil nil ((t nil))))"####
    ]];
    ParityBatchCase::value(
        "major_mode_spinner_reuses_mode_line_slot_timer_and_stopper",
        elisp_form,
        expect,
    )
}

fn delayed_spinner_ticks_from_hidden_countdown_through_wrapped_frames() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (spinner-parity-cancel-package-timers)
  (let* ((buffer (generate-new-buffer "*spinner release upload*"))
         (spinner (with-current-buffer buffer
                    (spinner-create ["queued" "uploading" "published"] t 2 1.5)))
         updates states stopper)
    (unwind-protect
        (cl-letf (((symbol-function 'force-mode-line-update)
                   (lambda (&optional all)
                     (push (list (eq (current-buffer) buffer) all) updates)
                     all)))
          (setq stopper (spinner-start spinner))
          (let ((start-state
                 (list (functionp stopper)
                       (spinner--counter spinner)
                       (spinner-print spinner))))
          (dotimes (_ 6)
            (spinner--timer-function spinner)
            (push (list (spinner--counter spinner)
                        (spinner-print spinner))
                  states))
          (let ((repeat (timer--repeat-delay (spinner--timer spinner))))
            (spinner-stop spinner)
            (list
             :start start-state
             :ticks (nreverse states)
             :timer
             (list repeat
                   (eq (timer--function (spinner--timer spinner))
                       #'spinner--timer-function)
                   (eq (car (timer--args (spinner--timer spinner))) spinner))
             :updates (nreverse updates)
             :stopped
             (list (spinner--active-p spinner)
                   (spinner-print spinner)
                   (memq (spinner--timer spinner) timer-list))))))
      (spinner-stop spinner)
      (when (buffer-live-p buffer) (kill-buffer buffer)))))
"#####;
    let expect = expect![[
        r####"OK (:start (t -3 nil) :ticks ((-2 nil) (-1 nil) (0 "queued") (1 "uploading") (2 "published") (0 "queued")) :timer (0.5 t t) :updates ((t nil) (t nil) (t nil) (t nil) (t nil) (t nil) (nil nil)) :stopped (nil nil nil))"####
    ]];
    ParityBatchCase::value(
        "delayed_spinner_ticks_from_hidden_countdown_through_wrapped_frames",
        elisp_form,
        expect,
    )
}

fn concurrent_buffer_operations_keep_animation_and_updates_isolated() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (spinner-parity-cancel-package-timers)
  (let* ((release-buffer (generate-new-buffer "*spinner release*"))
         (index-buffer (generate-new-buffer "*spinner index*"))
         (release (spinner-create ["R0" "R1"] release-buffer 2))
         (index (spinner-create ["I0" "I1" "I2"] index-buffer 5))
         updates result)
    (unwind-protect
        (cl-letf (((symbol-function 'force-mode-line-update)
                   (lambda (&optional _all)
                     (push
                      (cond ((eq (current-buffer) release-buffer) :release)
                            ((eq (current-buffer) index-buffer) :index)
                            (t :caller))
                      updates))))
          (spinner-start release)
          (spinner-start index)
          (spinner--timer-function release)
          (spinner--timer-function index)
          (spinner--timer-function index)
          (setq result
                (list
                 :running
                 (list (spinner-print release)
                       (spinner--counter release)
                       (spinner-print index)
                       (spinner--counter index)
                       (and (memq (spinner--timer release) timer-list) t)
                       (and (memq (spinner--timer index) timer-list) t))
                 :updates (nreverse updates)))
          (setq updates nil)
          (spinner-stop release)
          (spinner--timer-function index)
          (append
           result
           (list
            :after-release-stop
            (list (spinner--active-p release)
                  (spinner-print release)
                  (spinner--active-p index)
                  (spinner-print index)
                  (memq (spinner--timer release) timer-list)
                  (and (memq (spinner--timer index) timer-list) t)
                  (nreverse updates)))))
      (spinner-stop release)
      (spinner-stop index)
      (when (buffer-live-p release-buffer) (kill-buffer release-buffer))
      (when (buffer-live-p index-buffer) (kill-buffer index-buffer)))))
"#####;
    let expect = expect![[
        r####"OK (:running ("R1" 1 "I2" 2 t t) :updates (:release :index :index) :after-release-stop (nil nil t "I0" nil t (:caller :index)))"####
    ]];
    ParityBatchCase::value(
        "concurrent_buffer_operations_keep_animation_and_updates_isolated",
        elisp_form,
        expect,
    )
}

fn killed_buffer_is_detected_and_its_timer_is_cancelled_on_next_tick() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (spinner-parity-cancel-package-timers)
  (let* ((buffer (generate-new-buffer "*spinner disposable operation*"))
         (spinner (spinner-create 'moon buffer 10))
         updates)
    (unwind-protect
        (cl-letf (((symbol-function 'force-mode-line-update)
                   (lambda (&optional all)
                     (push (list (buffer-live-p buffer) all) updates)
                     all)))
          (spinner-start spinner)
          (kill-buffer buffer)
          (spinner--timer-function spinner)
          (list
           :buffer-live (buffer-live-p buffer)
           :active (spinner--active-p spinner)
           :printed (spinner-print spinner)
           :timer
           (list (timerp (spinner--timer spinner))
                 (memq (spinner--timer spinner) timer-list))
           :updates (nreverse updates)))
      (spinner-stop spinner)
      (when (buffer-live-p buffer) (kill-buffer buffer)))))
"#####;
    let expect = expect![[
        r####"OK (:buffer-live nil :active nil :printed nil :timer (t nil) :updates ((nil nil)))"####
    ]];
    ParityBatchCase::value(
        "killed_buffer_is_detected_and_its_timer_is_cancelled_on_next_tick",
        elisp_form,
        expect,
    )
}

fn minor_mode_lighter_lazily_starts_without_claiming_mode_line_process() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (spinner-parity-cancel-package-timers)
  (with-temp-buffer
    (let* ((mode-line-process '(" worker:active"))
           (spinner (spinner-create ["idle" "working"] nil 4))
           (before mode-line-process)
           updates first second)
      (unwind-protect
          (cl-letf (((symbol-function 'force-mode-line-update)
                     (lambda (&optional all) (push all updates) all)))
            (setq first (spinner-start-print spinner))
            (spinner--timer-function spinner)
            (setq second (spinner-start-print spinner))
            (let ((repeat (timer--repeat-delay (spinner--timer spinner))))
              (spinner-stop spinner)
              (list
               :frames (list first second)
               :mode-line (list mode-line-process
                                (equal before mode-line-process)
                                (eq before mode-line-process))
               :timer-repeat repeat
               :updates (nreverse updates)
               :stopped
               (list (spinner--active-p spinner)
                     (spinner-print spinner)
                     (memq (spinner--timer spinner) timer-list)))))
        (spinner-stop spinner)))))
"#####;
    let expect = expect![[
        r####"OK (:frames ("idle" "working") :mode-line ((" worker:active") t t) :timer-repeat 0.25 :updates (nil nil) :stopped (nil nil nil))"####
    ]];
    ParityBatchCase::value(
        "minor_mode_lighter_lazily_starts_without_claiming_mode_line_process",
        elisp_form,
        expect,
    )
}

#[test]
fn spinner_package_batch() {
    let cases = vec![
        configuration_builds_builtin_custom_and_generated_animation_frames(),
        major_mode_spinner_reuses_mode_line_slot_timer_and_stopper(),
        delayed_spinner_ticks_from_hidden_countdown_through_wrapped_frames(),
        concurrent_buffer_operations_keep_animation_and_updates_isolated(),
        killed_buffer_is_detected_and_its_timer_is_cancelled_on_next_tick(),
        minor_mode_lighter_lazily_starts_without_claiming_mode_line_process(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Spinner parity test");
    assert_oracle_batch_cases(spinner_oracle(), test_name, "spinner_parity", &cases);
}

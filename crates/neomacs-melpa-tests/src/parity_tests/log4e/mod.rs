use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, LOG4E_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const LOG4E_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const LOG4E_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'log4e)

(log4e:deflogger "parity" "%t|%l|%m" "STAMP")

(defvar log4e-test-pristine-buffers (buffer-list))

(defun log4e-test-reset ()
  (dolist (buffer (buffer-list))
    (unless (memq buffer log4e-test-pristine-buffers)
      (kill-buffer buffer)))
  (when-let ((buffer (get-buffer log4e--log-buffer-parity)))
    (kill-buffer buffer))
  (setq log4e--log-buffer-parity " *log4e-parity*"
        log4e--log-template-parity "%t|%l|%m"
        log4e--time-template-parity "STAMP"
        log4e--min-level-parity 'info
        log4e--max-level-parity 'fatal
        log4e--toggle-logging-parity nil
        log4e--msg-buffer-parity nil
        log4e--toggle-debugging-parity nil
        log4e--buffer-coding-system-parity nil))

(defun log4e-test-content (&optional buffer)
  (let ((buffer (or buffer (get-buffer log4e--log-buffer-parity))))
    (and buffer
         (with-current-buffer buffer
           (buffer-substring-no-properties (point-min) (point-max))))))

(defun log4e-test-records ()
  (with-current-buffer log4e--log-buffer-parity
    (save-excursion
      (goto-char (point-min))
      (let (records)
        (while (< (point) (point-max))
          (when-let ((level (get-text-property (point) 'log4e--level)))
            (push (list level
                        (buffer-substring-no-properties
                         (line-beginning-position) (line-end-position)))
                  records))
          (forward-line 1))
        (nreverse records)))))

(defun log4e-test-face-at (text)
  (with-current-buffer log4e--log-buffer-parity
    (save-excursion
      (goto-char (point-min))
      (search-forward text)
      (get-text-property (- (point) (length text)) 'face))))
"##;

fn log4e_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LOG4E_MELPA_PIN, "log4e.el")
        .expect("prepare pinned log4e source below ./tmp")
        .with_prelude(LOG4E_TEST_PRELUDE)
        .with_timeout(LOG4E_TEST_TIMEOUT)
}

fn production_logger_lifecycle_filters_two_level_ranges_and_stops_cleanly() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (log4e-test-reset)
  (let ((before (list log4e--toggle-logging-parity
                      log4e--min-level-parity
                      log4e--max-level-parity)))
    (parity--log-enable-logging)
    (parity--log-trace "bootstrap trace")
    (parity--log-debug "debug request")
    (parity--log-info "accepted %s" "REL-417")
    (parity--log-warn "retry %d" 2)
    (parity--log-error "backend %s" "payments")
    (parity--log-fatal "deployment stopped")
    (let ((default-range (log4e-test-content)))
      (parity--log-set-level 'debug 'error)
      (parity--log-trace "filtered trace")
      (parity--log-debug "worker detail")
      (parity--log-info "worker ready")
      (parity--log-warn "worker delayed")
      (parity--log-error "worker failed")
      (parity--log-fatal "filtered fatal")
      (parity--log-disable-logging)
      (parity--log-error "filtered after disable")
      (list :before before
            :after (list log4e--toggle-logging-parity
                         log4e--min-level-parity
                         log4e--max-level-parity)
            :default default-range
            :complete (log4e-test-content)
            :records (log4e-test-records)))))
"##;
    let expect = expect![[
        r##"OK (:before (nil info fatal) :after (nil debug error) :default "STAMP|INFO |accepted REL-417\nSTAMP|WARN |retry 2\nSTAMP|ERROR|backend payments\nSTAMP|FATAL|deployment stopped\n" :complete "STAMP|INFO |accepted REL-417\nSTAMP|WARN |retry 2\nSTAMP|ERROR|backend payments\nSTAMP|FATAL|deployment stopped\nSTAMP|DEBUG|worker detail\nSTAMP|INFO |worker ready\nSTAMP|WARN |worker delayed\nSTAMP|ERROR|worker failed\n" :records ((info "STAMP|INFO |accepted REL-417") (warn "STAMP|WARN |retry 2") (error "STAMP|ERROR|backend payments") (fatal "STAMP|FATAL|deployment stopped") (debug "STAMP|DEBUG|worker detail") (info "STAMP|INFO |worker ready") (warn "STAMP|WARN |worker delayed") (error "STAMP|ERROR|worker failed")))"##
    ]];
    ParityBatchCase::value(
        "production_logger_lifecycle_filters_two_level_ranges_and_stops_cleanly",
        elisp_form,
        expect,
    )
}

fn logging_macros_skip_expensive_arguments_while_functions_remain_eager() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (log4e-test-reset)
  (let ((evaluations 0) checkpoints)
    (parity--log-debug "disabled function %d" (cl-incf evaluations))
    (push evaluations checkpoints)
    (parity--log-debug* "disabled macro %d" (cl-incf evaluations))
    (push evaluations checkpoints)
    (parity--log-enable-logging)
    (parity--log-debug "filtered function %d" (cl-incf evaluations))
    (push evaluations checkpoints)
    (parity--log-debug* "filtered macro %d" (cl-incf evaluations))
    (push evaluations checkpoints)
    (parity--log-set-level 'trace 'fatal)
    (parity--log-debug* "recorded macro %d" (cl-incf evaluations))
    (push evaluations checkpoints)
    (parity--log* 'info "generic macro %s" (prog1 "ran" (cl-incf evaluations)))
    (push evaluations checkpoints)
    (list :checkpoints (nreverse checkpoints)
          :evaluations evaluations
          :buffer-created (and (get-buffer log4e--log-buffer-parity) t)
          :content (log4e-test-content))))
"##;
    let expect = expect![[
        r##"OK (:checkpoints (1 1 2 2 3 3) :evaluations 3 :buffer-created t :content "STAMP|DEBUG|recorded macro 3\n")"##
    ]];
    ParityBatchCase::value(
        "logging_macros_skip_expensive_arguments_while_functions_remain_eager",
        elisp_form,
        expect,
    )
}

fn structured_formatting_marks_values_and_failures_and_preserves_unicode_coding() -> ParityBatchCase
{
    let elisp_form = r##"
(progn
  (log4e-test-reset)
  (parity--log-set-level 'trace 'fatal)
  (parity--log-set-coding-system 'utf-8-unix)
  (parity--log-enable-logging)
  (parity--log-info
   "release=%s attempts=%03d ratio=%.2f payload=%S"
   "REL-417" 7 0.875 '(:region "東京" :ready t))
  (parity--log-warn "bad-count=%d next=%s" "three" "continue")
  (parity--log-error "配送先=%s status=%s" "東京" "失敗")
  (with-current-buffer log4e--log-buffer-parity
    (list :content (log4e-test-content)
          :coding buffer-file-coding-system
          :mode major-mode
          :read-only buffer-read-only
          :records (log4e-test-records)
          :faces (list :time (log4e-test-face-at "STAMP")
                       :level (log4e-test-face-at "INFO ")
                       :value (log4e-test-face-at "REL-417")
                       :failure (save-excursion
                                  (goto-char (point-min))
                                  (search-forward "bad-count=")
                                  (get-text-property (point) 'face))
                       :unicode (log4e-test-face-at "東京")))))
"##;
    let expect = expect![[
        r##"OK (:content "STAMP|INFO |release=REL-417 attempts=007 ratio=0.88 payload=(:region \"東京\" :ready t)\nSTAMP|WARN |bad-count==Format specifier doesn’t match argument type= next=continue\nSTAMP|ERROR|配送先=東京 status=失敗\n" :coding utf-8-unix :mode log4e-mode :read-only t :records ((info "STAMP|INFO |release=REL-417 attempts=007 ratio=0.88 payload=(:region \"東京\" :ready t)") (warn "STAMP|WARN |bad-count==Format specifier doesn’t match argument type= next=continue") (error "STAMP|ERROR|配送先=東京 status=失敗")) :faces (:time font-lock-doc-face :level font-lock-keyword-face :value font-lock-string-face :failure font-lock-warning-face :unicode font-lock-string-face))"##
    ]];
    ParityBatchCase::value(
        "structured_formatting_marks_values_and_failures_and_preserves_unicode_coding",
        elisp_form,
        expect,
    )
}

fn custom_message_sink_receives_exact_rendered_records_until_messaging_is_disabled()
-> ParityBatchCase {
    let elisp_form = r##"
(progn
  (log4e-test-reset)
  (let ((sink (generate-new-buffer "*log4e-deployment-stream*")))
    (unwind-protect
        (progn
          (parity--log-set-level 'trace 'fatal)
          (parity--log-enable-logging)
          (parity--log-enable-messaging sink)
          (parity--log-info "queued %s" "REL-417")
          (parity--log-warn "retrying %s\nattempt=%d" "REL-418" 3)
          (let ((enabled (log4e-test-content sink)))
            (parity--log-disable-messaging)
            (parity--log-error "failed %s" "REL-419")
            (list :enabled enabled
                  :after-disable (log4e-test-content sink)
                  :sink-mode (with-current-buffer sink major-mode)
                  :message-target log4e--msg-buffer-parity
                  :log (log4e-test-content))))
      (when (buffer-live-p sink)
        (kill-buffer sink)))))
"##;
    let expect = expect![[
        r##"OK (:enabled "STAMP|INFO |queued REL-417\nSTAMP|WARN |retrying REL-418\nattempt=3\n" :after-disable "STAMP|INFO |queued REL-417\nSTAMP|WARN |retrying REL-418\nattempt=3\n" :sink-mode fundamental-mode :message-target nil :log "STAMP|INFO |queued REL-417\nSTAMP|WARN |retrying REL-418\nattempt=3\nSTAMP|ERROR|failed REL-419\n")"##
    ]];
    ParityBatchCase::value(
        "custom_message_sink_receives_exact_rendered_records_until_messaging_is_disabled",
        elisp_form,
        expect,
    )
}

fn log_view_navigation_skips_continuations_and_open_and_clear_manage_the_buffer() -> ParityBatchCase
{
    let elisp_form = r##"
(progn
  (log4e-test-reset)
  (setq log4e--log-template-parity "%m")
  (parity--log-set-level 'trace 'fatal)
  (parity--log-enable-logging)
  (parity--log-fatal "fatal\ncontext-a\ncontext-b")
  (parity--log-debug "debug\nworker=api")
  (parity--log-error "error\nbackend=payments")
  (parity--log-trace "trace")
  (let (forward backward opened cleared)
    (with-current-buffer log4e--log-buffer-parity
      (goto-char (point-min))
      (while (log4e:next-log)
        (push (list (log4e--get-current-log-line-level)
                    (buffer-substring-no-properties
                     (line-beginning-position) (line-end-position)))
              forward))
      (goto-char (point-max))
      (while (log4e:previous-log)
        (push (list (log4e--get-current-log-line-level)
                    (buffer-substring-no-properties
                     (line-beginning-position) (line-end-position)))
              backward)))
    (setq forward (nreverse forward)
          backward (nreverse backward))
    (save-window-excursion
      (parity--log-open-log)
      (setq opened
            (list :selected (buffer-name (window-buffer (selected-window)))
                  :mode major-mode
                  :read-only buffer-read-only
                  :next (key-binding (kbd "J"))
                  :previous (key-binding (kbd "K")))))
    (parity--log-clear-log)
    (setq cleared
          (with-current-buffer log4e--log-buffer-parity
            (list :content (buffer-string)
                  :mode major-mode
                  :read-only buffer-read-only)))
    (list :forward forward :backward backward
          :opened opened :cleared cleared)))
"##;
    let expect = expect![[
        r##"OK (:forward ((debug "debug") (error "error") (trace "trace")) :backward ((trace "trace") (error "error") (debug "debug") (fatal "fatal")) :opened (:selected " *log4e-parity*" :mode log4e-mode :read-only t :next log4e:next-log :previous log4e:previous-log) :cleared (:content "" :mode log4e-mode :read-only nil))"##
    ]];
    ParityBatchCase::value(
        "log_view_navigation_skips_continuations_and_open_and_clear_manage_the_buffer",
        elisp_form,
        expect,
    )
}

fn custom_names_and_generic_dispatch_create_a_domain_specific_logger_surface() -> ParityBatchCase {
    let elisp_form = r##"
(progn
  (log4e-test-reset)
  (log4e:deflogger
   "deploy-parity" "%l|%m" "STAMP"
   '((fatal . "panic") (error . "fail") (warn . "caution")
     (info . "notice") (debug . "inspect") (trace . "verbose")))
  (deploy-parity--log-set-level 'trace 'fatal)
  (deploy-parity--log-enable-logging)
  (deploy-parity--notice "release %s queued" "REL-417")
  (deploy-parity--log 'warn "release %s delayed %d seconds" "REL-418" 30)
  (deploy-parity--inspect* "worker=%s attempt=%d" "api" 2)
  (deploy-parity--panic "release %s stopped" "REL-419")
  (let ((buffer (get-buffer log4e--log-buffer-deploy-parity)))
    (list
     :surface
     (mapcar (lambda (symbol)
               (list symbol (fboundp symbol) (macrop symbol)
                     (and (fboundp symbol)
                          (help-function-arglist symbol t))))
             '(deploy-parity--notice deploy-parity--notice*
               deploy-parity--inspect deploy-parity--inspect*
               deploy-parity--log deploy-parity--log*))
     :config (list log4e--min-level-deploy-parity
                   log4e--max-level-deploy-parity
                   log4e--toggle-logging-deploy-parity)
     :content (log4e-test-content buffer))))
"##;
    let expect = expect![[
        r##"OK (:surface ((deploy-parity--notice t nil #1=(msg &rest msgargs)) (deploy-parity--notice* t t #1#) (deploy-parity--inspect t nil #1#) (deploy-parity--inspect* t t #1#) (deploy-parity--log t nil #2=(level msg &rest msgargs)) (deploy-parity--log* t t #2#)) :config (trace fatal t) :content "INFO |release REL-417 queued\nWARN |release REL-418 delayed 30 seconds\nDEBUG|worker=api attempt=2\nFATAL|release REL-419 stopped\n")"##
    ]];
    ParityBatchCase::value(
        "custom_names_and_generic_dispatch_create_a_domain_specific_logger_surface",
        elisp_form,
        expect,
    )
}

fn source_instrumentation_inserts_a_trace_record_after_the_real_function_docstring()
-> ParityBatchCase {
    let elisp_form = r##"
(progn
  (log4e-test-reset)
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert
     ";; (log4e:deflogger \"ignored\" \"%m\" \"%S\")\n"
     "(log4e:deflogger \"service\" \"%m\" \"%S\")\n\n"
     "(defun service-deploy (release &optional region &rest labels)\n"
     "  \"Deploy RELEASE to REGION with LABELS.\"\n"
     "  (list release region labels))\n")
    (font-lock-ensure)
    (goto-char (point-max))
    (search-backward "list release")
    (log4e:insert-start-log-quickly)
    (list :source (buffer-substring-no-properties (point-min) (point-max))
          :point-line (line-number-at-pos)
          :mode major-mode
          :parse (condition-case problem
                     (progn
                       (check-parens)
                       :balanced)
                   (error (list :signal (car problem)
                                (error-message-string problem)))))))
"##;
    let expect = expect![[
        r##"OK (:source ";; (log4e:deflogger \"ignored\" \"%m\" \"%S\")\n(log4e:deflogger \"service\" \"%m\" \"%S\")\n\n(defun service-deploy (release &optional region &rest labels)\n  \"Deploy RELEASE to REGION with LABELS.\"\n  (service--log 'trace \"start deploy. release[%s] region[%s] labels[%s]\" release region labels)\n  (list release region labels))\n" :point-line 6 :mode emacs-lisp-mode :parse :balanced)"##
    ]];
    ParityBatchCase::value(
        "source_instrumentation_inserts_a_trace_record_after_the_real_function_docstring",
        elisp_form,
        expect,
    )
}

#[test]
fn log4e_package_batch() {
    let cases = vec![
        production_logger_lifecycle_filters_two_level_ranges_and_stops_cleanly(),
        logging_macros_skip_expensive_arguments_while_functions_remain_eager(),
        structured_formatting_marks_values_and_failures_and_preserves_unicode_coding(),
        custom_message_sink_receives_exact_rendered_records_until_messaging_is_disabled(),
        log_view_navigation_skips_continuations_and_open_and_clear_manage_the_buffer(),
        custom_names_and_generic_dispatch_create_a_domain_specific_logger_surface(),
        source_instrumentation_inserts_a_trace_record_after_the_real_function_docstring(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed log4e parity test");
    assert_oracle_batch_cases(log4e_oracle(), test_name, "log4e_parity", &cases);
}

use expect_test::expect;

use super::ParityBatchCase;

/// The default path: three alerts of different severity through the `message'
/// style.  Each reaches the echo area with its `%' left alone, each is written
/// to the log with the level its severity maps to (normal to INFO, high to
/// ERROR, trivial to TRACE) because `alert-log-messages' logs independently of
/// the style, and each stays on `alert-active-alerts' with a fade timer holding
/// its remover.
fn alerting_shows_the_message_and_logs_every_severity() -> ParityBatchCase {
    ParityBatchCase::value(
        "alerting_shows_the_message_and_logs_every_severity",
        r##"(let ((alert-default-style 'message)
      (alert-user-configuration nil)
      (alert-internal-configuration nil)
      (alert-active-alerts nil)
      (alert-hide-all-notifications nil)
      (alert-log-messages t)
      (alert-fade-time 300)
      (buffer (generate-new-buffer "*alert-origin*"))
      (mark (al-test-messages-mark)))
  (unwind-protect
      (with-current-buffer buffer
        (set-window-buffer (selected-window) buffer)
        (text-mode)
        (alert "Build finished")
        (alert "Disk almost full" :title "System" :severity 'high)
        (alert "100% done" :severity 'trivial)
        (list (al-test-messages-since mark)
              (al-test-buffer-text " *log4e-alert*")
              (al-test-active-alerts)
              (al-test-pending-fades)
              alert-log-level))
    (kill-buffer buffer)))"##,
        expect![[
            r#"OK (("Build finished" "Disk almost full" "100% done") "<TIME> [INFO ] Build finished\n<TIME> [ERROR] Disk almost full\n<TIME> [TRACE] 100% done\n" (("*alert-origin*" "100% done" alert-message-remove) ("*alert-origin*" "Disk almost full" alert-message-remove) ("*alert-origin*" "Build finished" alert-message-remove)) ("100% done" "Build finished" "Disk almost full") normal)"#
        ]],
    )
}

fn a_custom_style_receives_the_complete_alert_plist() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_custom_style_receives_the_complete_alert_plist",
        r##"(let ((alert-default-style 'al-test-recorder)
      (alert-user-configuration nil)
      (alert-internal-configuration nil)
      (alert-active-alerts nil)
      (alert-hide-all-notifications nil)
      (alert-log-messages nil)
      (buffer (generate-new-buffer "*alert-origin*")))
  (al-test-define-recorder 'al-test-recorder)
  (unwind-protect
      (with-current-buffer buffer
        (set-window-buffer (selected-window) buffer)
        (text-mode)
        (alert "Nightly build failed"
               :title "CI"
               :severity 'urgent
               :category 'build
               :data '(job 42)
               :id 'nightly
               :icon "mail-message-new"
               :persistent t)
        (let ((notified (al-test-captured-infos :notify)))
          (execute-kbd-macro (kbd "x"))
          (list (al-test-style-summary 'al-test-recorder)
                (mapcar #'al-test-info (mapcar #'cdr notified))
                (mapcar (lambda (entry) (al-test-info (cdr entry) :message :severity))
                        (al-test-captured-infos :remove))
                (length alert-active-alerts)
                (and (memq #'alert-remove-on-command post-command-hook) t)
                (buffer-string))))
    (kill-buffer buffer)))"##,
        expect![[
            r#"OK ((al-test-recorder "Recorder al-test-recorder" t t) (((:message . "Nightly build failed") (:title . "CI") (:severity . urgent) (:category . build) (:mode . text-mode) (:buffer . "*alert-origin*") (:data job 42) (:id . nightly) (:persistent . t) (:never-persist) (:style))) (((:message . "Nightly build failed") (:severity . urgent))) 0 t "x")"#
        ]],
    )
}

fn user_configuration_rules_route_each_alert_to_a_style() -> ParityBatchCase {
    ParityBatchCase::value(
        "user_configuration_rules_route_each_alert_to_a_style",
        r##"(let ((alert-default-style 'al-test-fallback)
      (alert-internal-configuration nil)
      (alert-active-alerts nil)
      (alert-hide-all-notifications nil)
      (alert-log-messages nil)
      (al-test-captured nil))
  (al-test-define-recorder 'al-test-urgent)
  (al-test-define-recorder 'al-test-chat)
  (al-test-define-recorder 'al-test-fallback)
  (let ((alert-user-configuration
         '((((:severity urgent high) (:mode . "\\`text-mode\\'"))
            al-test-urgent nil)
           (((:category . "\\`chat\\'"))
            al-test-chat ((:continue . t)))
           (((:message . "audit"))
            al-test-fallback nil))))
    (with-temp-buffer
      (rename-buffer "*alert-origin*" t)
      (text-mode)
      (alert "server on fire" :severity 'urgent)
      (alert "ping from audit team" :category 'chat)
      (alert "nothing matches me" :severity 'low :category 'misc))
    (mapcar (lambda (entry)
              (cons (car entry)
                    (al-test-info (cdr entry) :message :severity :category)))
            (al-test-captured-infos :notify))))"##,
        expect![[
            r#"OK ((al-test-urgent (:message . "server on fire") (:severity . urgent) (:category)) (al-test-chat (:message . "ping from audit team") (:severity . normal) (:category . chat)) (al-test-fallback (:message . "ping from audit team") (:severity . normal) (:category . chat)) (al-test-fallback (:message . "nothing matches me") (:severity . low) (:category . misc)))"#
        ]],
    )
}

fn hiding_all_notifications_still_delivers_the_default_style() -> ParityBatchCase {
    ParityBatchCase::value(
        "hiding_all_notifications_still_delivers_the_default_style",
        r##"(let ((alert-internal-configuration nil)
      (alert-active-alerts nil)
      (alert-log-messages t)
      (al-test-captured nil))
  (al-test-define-recorder 'al-test-default)
  (al-test-define-recorder 'al-test-rule)
  (al-test-define-recorder 'al-test-forced)
  (let ((alert-user-configuration
         '((((:severity urgent)) al-test-rule nil))))
    (with-temp-buffer
      (rename-buffer "*alert-origin*" t)
      (text-mode)
      (let ((alert-default-style 'al-test-default)
            (alert-hide-all-notifications t))
        (alert "hidden with a default style" :severity 'urgent))
      (let ((hidden-with-default (al-test-captured-infos :notify)))
        (setq al-test-captured nil)
        (let ((alert-default-style nil)
              (alert-hide-all-notifications t))
          (alert "hidden with no default style" :severity 'urgent))
        (let ((hidden-without-default (al-test-captured-infos :notify)))
          (setq al-test-captured nil)
          (let ((alert-default-style 'al-test-default))
            (alert "shown through the rule" :severity 'urgent)
            (alert "forced past the rules" :severity 'low :style 'al-test-forced))
          (list (mapcar (lambda (entry)
                          (cons (car entry)
                                (al-test-info (cdr entry) :message :persistent :never-persist)))
                        hidden-with-default)
                hidden-without-default
                (mapcar (lambda (entry)
                          (cons (car entry)
                                (al-test-info (cdr entry) :message :severity :never-persist)))
                        (al-test-captured-infos :notify))
                (al-test-buffer-text " *log4e-alert*")))))))"##,
        expect![[
            r#"OK (((al-test-default (:message . "hidden with a default style") (:persistent) (:never-persist))) nil ((al-test-rule (:message . "shown through the rule") (:severity . urgent) (:never-persist)) (al-test-forced (:message . "forced past the rules") (:severity . low) (:never-persist))) "<TIME> [FATAL] hidden with a default style\n<TIME> [FATAL] hidden with no default style\n<TIME> [FATAL] shown through the rule\n<TIME> [DEBUG] forced past the rules\n")"#
        ]],
    )
    .fresh_process()
}

fn programmatic_rules_control_persistence_and_ordering() -> ParityBatchCase {
    ParityBatchCase::value(
        "programmatic_rules_control_persistence_and_ordering",
        r##"(let ((alert-default-style 'al-test-default)
      (alert-user-configuration nil)
      (alert-internal-configuration nil)
      (alert-active-alerts nil)
      (alert-log-messages nil)
      (alert-fade-time 300)
      (al-test-captured nil))
  (al-test-define-recorder 'al-test-default)
  (al-test-define-recorder 'al-test-chat)
  (al-test-define-recorder 'al-test-audit)
  (let ((first (alert-add-rule :severity '(urgent high)
                               :mode 'text-mode
                               :style 'al-test-chat
                               :persistent (lambda (info)
                                             (eq (plist-get info :severity) 'urgent))
                               :continue t))
        (second (alert-add-rule :category "audit"
                                :style 'al-test-audit
                                :never-persist t
                                :append t)))
    (with-temp-buffer
      (rename-buffer "*alert-origin*" t)
      (text-mode)
      (alert "urgent audit finding" :severity 'urgent :category 'audit)
      (alert "routine note" :severity 'high :category 'chores))
    (list (mapcar #'al-test-rule-summary (list first second))
          (mapcar #'al-test-rule-summary alert-internal-configuration)
          (mapcar (lambda (entry)
                    (cons (car entry)
                          (al-test-info (cdr entry) :message :severity :persistent :never-persist)))
                  (al-test-captured-infos :notify))
          (length alert-active-alerts)
          (al-test-pending-fades))))"##,
        expect![[
            r#"OK (((:selectors ((:severity urgent high) (:mode . "\\`text-mode\\'")) :style al-test-chat :options ((:persistent . :function) (:continue . t))) (:selectors ((:category . "audit")) :style al-test-audit :options ((:never-persist . t)))) ((:selectors ((:severity urgent high) (:mode . "\\`text-mode\\'")) :style al-test-chat :options ((:persistent . :function) (:continue . t))) (:selectors ((:category . "audit")) :style al-test-audit :options ((:never-persist . t)))) ((al-test-chat (:message . "urgent audit finding") (:severity . urgent) (:persistent) (:never-persist)) (al-test-audit (:message . "urgent audit finding") (:severity . urgent) (:persistent) (:never-persist . t)) (al-test-chat (:message . "routine note") (:severity . high) (:persistent) (:never-persist))) 3 ("routine note" "urgent audit finding"))"#
        ]],
    )
    .fresh_process()
}

fn an_unavailable_backend_falls_back_to_the_message_style() -> ParityBatchCase {
    ParityBatchCase::value(
        "an_unavailable_backend_falls_back_to_the_message_style",
        r##"(let ((alert-user-configuration nil)
      (alert-internal-configuration nil)
      (alert-log-messages nil)
      (alert-active-alerts nil)
      (mark (al-test-messages-mark)))
  (list
   (list (executable-find "growlnotify") alert-growl-command)
   (let ((alert-growl-command nil)
         (alert-default-style 'growl))
     (with-temp-buffer
       (rename-buffer "*alert-origin*" t)
       (alert "Growl is not installed" :severity 'high))
     (list (al-test-messages-since mark) (al-test-commands)))
   (let* ((command (al-test-install-growlnotify))
          (alert-growl-command command)
          (alert-default-style 'growl)
          (system-type 'gnu/linux))
     (with-temp-buffer
       (rename-buffer "*alert-origin*" t)
       (alert "Build finished" :title "CI" :severity 'high :persistent t)
       (alert "Coffee ready" :severity 'trivial))
     (list (al-test-commands)
           (al-test-messages-since mark)))))"##,
        expect![[
            r#"OK ((nil nil) (("Growl is not installed") no-command-ran) (("growlnotify --appIcon Emacs --name Emacs --title CI --priority 2 --sticky --message Build finished" "growlnotify --appIcon Emacs --name Emacs --title *alert-origin* --priority -2 --message Coffee ready") ("Growl is not installed")))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alerting_shows_the_message_and_logs_every_severity(),
        a_custom_style_receives_the_complete_alert_plist(),
        user_configuration_rules_route_each_alert_to_a_style(),
        hiding_all_notifications_still_delivers_the_default_style(),
        programmatic_rules_control_persistence_and_ordering(),
        an_unavailable_backend_falls_back_to_the_message_style(),
    ]
}

use expect_test::expect;

use super::ParityBatchCase;

fn auto_dark_start_timer_stops_previous_then_schedules_immediate_repeating_check() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dark_start_timer_stops_previous_then_schedules_immediate_repeating_check",
        r##"(let ((auto-dark--timer
                                'old-timer)
                               events)
          (cl-letf
              (((symbol-function
                 'auto-dark-stop-timer)
                (lambda ()
                  (push
                   (list
                    :stop
                    auto-dark--timer)
                   events)
                  :stopped))
               ((symbol-function 'run-with-timer)
                (lambda (&rest arguments)
                  (push
                   (cons :run arguments)
                   events)
                  'new-timer)))
            (let ((auto-dark-polling-interval-seconds
                   17))
              (list
               (auto-dark-start-timer)
               auto-dark--timer
               (nreverse events)))))"##,
        expect![
            "OK (new-timer new-timer ((:stop old-timer) (:run 0 17 auto-dark--check-and-set-dark-mode)))"
        ],
    )
}

fn auto_dark_stop_timer_cancels_only_timer_objects_and_preserves_stale_slot() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_stop_timer_cancels_only_timer_objects_and_preserves_stale_slot",
        r##"(let (calls)
          (cl-letf
              (((symbol-function 'timerp)
                (lambda (value)
                  (memq value
                        '(valid-timer
                          second-timer))))
               ((symbol-function 'cancel-timer)
                (lambda (timer)
                  (push timer calls)
                  (list
                   :cancelled
                   timer))))
            (list
             (mapcar
              (lambda (value)
                (let ((auto-dark--timer
                       value))
                  (list
                   value
                   (auto-dark-stop-timer)
                   auto-dark--timer)))
              '(nil
                not-a-timer
                valid-timer
                second-timer))
             (nreverse calls))))"##,
        expect![
            "OK (((nil nil nil) (not-a-timer nil not-a-timer) (valid-timer (:cancelled valid-timer) valid-timer) (second-timer (:cancelled second-timer) second-timer)) (valid-timer second-timer))"
        ],
    )
}

fn auto_dark_real_timer_contains_exact_callback_repeat_and_cancel_lifecycle() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_real_timer_contains_exact_callback_repeat_and_cancel_lifecycle",
        r##"(let ((auto-dark--timer nil)
                               (auto-dark-polling-interval-seconds
                                29))
          (cl-letf
              (((symbol-function
                 'auto-dark--check-and-set-dark-mode)
                #'ignore))
            (unwind-protect
                (progn
                  (auto-dark-start-timer)
                  (let ((timer
                         auto-dark--timer))
                    (list
                     (timerp timer)
                     (timer--function timer)
                     (timer--repeat-delay timer)
                     (auto-dark-stop-timer)
                     (timerp auto-dark--timer)
                     (memq timer timer-list)
                     (eq timer
                         auto-dark--timer))))
              (auto-dark-stop-timer))))"##,
        expect!["OK (t auto-dark--check-and-set-dark-mode 29 nil t nil t)"],
    )
}

fn auto_dark_register_dbus_listener_forwards_exact_signal_and_callback_mapping() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dark_register_dbus_listener_forwards_exact_signal_and_callback_mapping",
        r##"(let (register-calls
                               callback
                               theme-calls)
          (cl-letf
              (((symbol-function
                 'dbus-register-signal)
                (lambda (&rest arguments)
                  (setq callback
                        (car
                         (last arguments)))
                  (push arguments
                        register-calls)
                  'fixture-dbus-object))
               ((symbol-function
                 'auto-dark--set-theme)
                (lambda (appearance)
                  (push appearance
                        theme-calls)
                  (list
                   :theme
                   appearance))))
            (list
             (auto-dark--register-dbus-listener)
             auto-dark--dbus-listener-object
             (butlast
              (car register-calls))
             (functionp callback)
             (mapcar
              (lambda (arguments)
                (list
                 arguments
                 (apply callback arguments)
                 (nreverse
                  (prog1
                      theme-calls
                    (setq theme-calls
                          nil)))))
              '(("org.freedesktop.appearance"
                 "color-scheme"
                 (1))
                ("org.freedesktop.appearance"
                 "color-scheme"
                 (0))
                ("org.freedesktop.appearance"
                 "color-scheme"
                 (2))
                ("org.freedesktop.appearance"
                 "color-scheme"
                 (3))
                ("other.path"
                 "color-scheme"
                 (1))
                ("org.freedesktop.appearance"
                 "other-setting"
                 (1)))))))"##,
        expect![[
            r#"OK (fixture-dbus-object fixture-dbus-object (:session "org.freedesktop.portal.Desktop" "/org/freedesktop/portal/desktop" "org.freedesktop.portal.Settings" "SettingChanged") t ((("org.freedesktop.appearance" "color-scheme" (1)) (:theme dark) (dark)) (("org.freedesktop.appearance" "color-scheme" (0)) (:theme light) (light)) (("org.freedesktop.appearance" "color-scheme" (2)) (:theme light) (light)) (("org.freedesktop.appearance" "color-scheme" (3)) nil nil) (("other.path" "color-scheme" (1)) nil nil) (("org.freedesktop.appearance" "other-setting" (1)) nil nil)))"#
        ]],
    )
}

fn auto_dark_unregister_dbus_listener_forwards_current_object_and_return_value() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dark_unregister_dbus_listener_forwards_current_object_and_return_value",
        r##"(let ((auto-dark--dbus-listener-object
                                'fixture-listener)
                               calls)
          (cl-letf
              (((symbol-function
                 'dbus-unregister-object)
                (lambda (object)
                  (push object calls)
                  (list
                   :unregistered
                   object))))
            (list
             (auto-dark--unregister-dbus-listener)
             auto-dark--dbus-listener-object
             calls)))"##,
        expect!["OK ((:unregistered fixture-listener) fixture-listener (fixture-listener))"],
    )
}

fn auto_dark_register_change_listener_selects_ns_mac_dbus_then_timer_priority() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_register_change_listener_selects_ns_mac_dbus_then_timer_priority",
        r##"(let (events)
          (cl-letf
              (((symbol-function 'add-hook)
                (lambda (&rest arguments)
                  (push
                   (cons :add-hook arguments)
                   events)
                  :hook-added))
               ((symbol-function
                 'auto-dark--register-dbus-listener)
                (lambda ()
                  (push :dbus events)
                  :dbus-registered))
               ((symbol-function
                 'auto-dark-start-timer)
                (lambda ()
                  (push :timer events)
                  :timer-started))
               ((symbol-function
                 'auto-dark--use-ns-system-appearance)
                (lambda ()
                  auto-dark-test-use-ns))
               ((symbol-function
                 'auto-dark--use-mac-system-appearance)
                (lambda ()
                  auto-dark-test-use-mac))
               ((symbol-function
                 'auto-dark--use-dbus)
                (lambda ()
                  auto-dark-test-use-dbus)))
            (mapcar
             (lambda (flags)
               (let ((auto-dark-test-use-ns
                      (nth 0 flags))
                     (auto-dark-test-use-mac
                      (nth 1 flags))
                     (auto-dark-test-use-dbus
                      (nth 2 flags)))
                 (setq events nil)
                 (list
                  flags
                  (auto-dark--register-change-listener)
                  (nreverse events))))
             '((t t t)
               (nil t t)
               (nil nil t)
               (nil nil nil)))))"##,
        expect![
            "OK (((t t t) :hook-added ((:add-hook ns-system-appearance-change-functions auto-dark--set-theme))) ((nil t t) :hook-added ((:add-hook mac-effective-appearance-change-hook auto-dark--check-and-set-dark-mode))) ((nil nil t) :dbus-registered (:dbus)) ((nil nil nil) :timer-started (:timer)))"
        ],
    )
}

fn auto_dark_unregister_change_listener_selects_matching_ns_mac_dbus_then_timer_path()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_unregister_change_listener_selects_matching_ns_mac_dbus_then_timer_path",
        r##"(let (events)
          (cl-letf
              (((symbol-function 'remove-hook)
                (lambda (&rest arguments)
                  (push
                   (cons :remove-hook arguments)
                   events)
                  :hook-removed))
               ((symbol-function
                 'auto-dark--unregister-dbus-listener)
                (lambda ()
                  (push :dbus events)
                  :dbus-unregistered))
               ((symbol-function
                 'auto-dark-stop-timer)
                (lambda ()
                  (push :timer events)
                  :timer-stopped))
               ((symbol-function
                 'auto-dark--use-ns-system-appearance)
                (lambda ()
                  auto-dark-test-use-ns))
               ((symbol-function
                 'auto-dark--use-mac-system-appearance)
                (lambda ()
                  auto-dark-test-use-mac))
               ((symbol-function
                 'auto-dark--use-dbus)
                (lambda ()
                  auto-dark-test-use-dbus)))
            (mapcar
             (lambda (flags)
               (let ((auto-dark-test-use-ns
                      (nth 0 flags))
                     (auto-dark-test-use-mac
                      (nth 1 flags))
                     (auto-dark-test-use-dbus
                      (nth 2 flags)))
                 (setq events nil)
                 (list
                  flags
                  (auto-dark--unregister-change-listener)
                  (nreverse events))))
             '((t t t)
               (nil t t)
               (nil nil t)
               (nil nil nil)))))"##,
        expect![
            "OK (((t t t) :hook-removed ((:remove-hook ns-system-appearance-change-functions auto-dark--set-theme))) ((nil t t) :hook-removed ((:remove-hook mac-effective-appearance-change-hook auto-dark--check-and-set-dark-mode))) ((nil nil t) :dbus-unregistered (:dbus)) ((nil nil nil) :timer-stopped (:timer)))"
        ],
    )
}

fn auto_dark_listener_feature_predicates_follow_binding_and_configured_method_exactly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_listener_feature_predicates_follow_binding_and_configured_method_exactly",
        r##"(list
          (let ((auto-dark-detection-method
                 'dbus))
            (list
             (auto-dark--use-ns-system-appearance)
             (auto-dark--use-mac-system-appearance)
             (auto-dark--use-dbus)))
          (progn
            (set
             'ns-system-appearance-change-functions
             nil)
            (let ((auto-dark-detection-method
                   'manual))
              (list
               (auto-dark--use-ns-system-appearance)
               (auto-dark--use-mac-system-appearance)
               (auto-dark--use-dbus))))
          (progn
            (set
             'mac-effective-appearance-change-hook
             nil)
            (let ((auto-dark-detection-method
                   'dbus))
              (list
               (auto-dark--use-ns-system-appearance)
               (auto-dark--use-mac-system-appearance)
               (auto-dark--use-dbus)))))"##,
        expect!["OK ((nil nil t) (t nil nil) (t t t))"],
    )
}

pub(super) fn listeners_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_dark_start_timer_stops_previous_then_schedules_immediate_repeating_check(),
        auto_dark_stop_timer_cancels_only_timer_objects_and_preserves_stale_slot(),
        auto_dark_real_timer_contains_exact_callback_repeat_and_cancel_lifecycle(),
        auto_dark_register_dbus_listener_forwards_exact_signal_and_callback_mapping(),
        auto_dark_unregister_dbus_listener_forwards_current_object_and_return_value(),
        auto_dark_register_change_listener_selects_ns_mac_dbus_then_timer_priority(),
        auto_dark_unregister_change_listener_selects_matching_ns_mac_dbus_then_timer_path(),
        auto_dark_listener_feature_predicates_follow_binding_and_configured_method_exactly(),
    ]
}

use expect_test::expect;

use super::ParityBatchCase;

fn auto_package_update_file_helpers_distinguish_missing_overwrite_and_unwritable_files()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_package_update_file_helpers_distinguish_missing_overwrite_and_unwritable_files",
        r##"(let*
                             ((root
                               (auto-package-update-test-root
                                "file-helpers"))
                              (file
                               (auto-package-update-test-path
                                root
                                "state/day"))
                              (missing
                               (auto-package-update-test-path
                                root
                                "missing")))
                           (auto-package-update-test-write
                            file
                            "old\n")
                           (let
                               ((before
                                 (list
                                  (apu--read-file-as-string
                                   file)
                                  (apu--read-file-as-string
                                   missing))))
                             (apu--write-string-to-file
                              file
                              "new-value")
                             (cl-letf
                                 (((symbol-function
                                    'file-writable-p)
                                   (lambda (_file) nil)))
                               (apu--write-string-to-file
                                file
                                "must-not-change"))
                             (list
                              before
                              (apu--read-file-as-string file)
                              (file-attribute-size
                               (file-attributes file))
                              (file-exists-p missing))))"##,
        expect![[r#"OK (("old\n" nil) "new-value" 9 nil)"#]],
    )
}

fn auto_package_update_current_day_round_trips_through_configured_sandbox_file() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_package_update_current_day_round_trips_through_configured_sandbox_file",
        r##"(let*
                             ((root
                               (auto-package-update-test-root
                                "current-day"))
                              (auto-package-update-last-update-day-path
                               (auto-package-update-test-path
                                root
                                "state/last-day")))
                           (make-directory
                            (file-name-directory
                             auto-package-update-last-update-day-path)
                            t)
                           (cl-letf
                               (((symbol-function
                                  'apu--today-day)
                                 (lambda () 24680)))
                             (let ((write-result
                                    (apu--write-current-day)))
                               (list
                                write-result
                                (apu--read-last-update-day)
                                (auto-package-update-test-read
                                 auto-package-update-last-update-day-path)
                                (file-exists-p
                                 auto-package-update-last-update-day-path)))))"##,
        expect![[r#"OK (nil 24680 "24680" t)"#]],
    )
}

fn auto_package_update_daily_timer_uses_exact_period_callback_and_return_value() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_package_update_daily_timer_uses_exact_period_callback_and_return_value",
        r##"(let (calls)
                           (cl-letf
                               (((symbol-function
                                  'run-at-time)
                                 (lambda
                                     (time repeat function
                                           &rest arguments)
                                   (push
                                    (list
                                     time
                                     repeat
                                     function
                                     arguments)
                                    calls)
                                   'fixture-timer)))
                             (list
                              (auto-package-update-at-time
                               "03:15")
                              (nreverse calls))))"##,
        expect![[r#"OK (fixture-timer (("03:15" 86400 auto-package-update-maybe nil)))"#]],
    )
}

pub(super) fn schedule_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_package_update_file_helpers_distinguish_missing_overwrite_and_unwritable_files(),
        auto_package_update_current_day_round_trips_through_configured_sandbox_file(),
        auto_package_update_daily_timer_uses_exact_period_callback_and_return_value(),
    ]
}

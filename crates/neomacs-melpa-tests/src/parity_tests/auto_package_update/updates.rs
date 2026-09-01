use expect_test::expect;

use super::ParityBatchCase;

fn auto_package_update_async_creates_named_thread_and_stores_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_package_update_async_creates_named_thread_and_stores_it",
        r##"(let
                             ((apu--update-thread nil)
                              captured-function
                              captured-name)
                           (cl-letf
                               (((symbol-function
                                  'make-thread)
                                 (lambda (function name)
                                   (setq
                                    captured-function
                                    function
                                    captured-name
                                    name)
                                   'fixture-thread)))
                             (let ((result
                                    (auto-package-update-now-async)))
                               (list
                                result
                                apu--update-thread
                                captured-name
                                (functionp
                                 captured-function)))))"##,
        expect![[r#"OK (fixture-thread fixture-thread "auto-package-update-now-async" t)"#]],
    )
}

fn auto_package_update_async_rejects_second_live_thread_without_force() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_package_update_async_rejects_second_live_thread_without_force",
        r##"(let
                             ((apu--update-thread
                               'existing-thread)
                              calls)
                           (cl-letf
                               (((symbol-function
                                  'thread-live-p)
                                 (lambda (thread)
                                   (push
                                    (list :live thread)
                                    calls)
                                   t))
                                ((symbol-function
                                  'make-thread)
                                 (lambda (&rest arguments)
                                   (push
                                    (list :make arguments)
                                    calls)
                                   'unexpected-thread))
                                ((symbol-function
                                  'thread-signal)
                                 (lambda (&rest arguments)
                                   (push
                                    (list :signal arguments)
                                    calls))))
                             (list
                              (auto-package-update-test-error
                               #'auto-package-update-now-async)
                              apu--update-thread
                              (nreverse calls))))"##,
        expect![[
            r#"OK ((:signal error ("auto-package-update thread is still running.")) existing-thread ((:live existing-thread)))"#
        ]],
    )
}

fn auto_package_update_async_force_signals_live_thread_clears_and_replaces_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_package_update_async_force_signals_live_thread_clears_and_replaces_it",
        r##"(let
                             ((apu--update-thread
                               'existing-thread)
                              calls)
                           (cl-letf
                               (((symbol-function
                                  'thread-live-p)
                                 (lambda (thread)
                                   (push
                                    (list :live thread)
                                    calls)
                                   (eq
                                    thread
                                    'existing-thread)))
                                ((symbol-function
                                  'thread-signal)
                                 (lambda
                                     (thread error-symbol data)
                                   (push
                                    (list
                                     :signal
                                     thread
                                     error-symbol
                                     data)
                                    calls)
                                   :signaled))
                                ((symbol-function
                                  'make-thread)
                                 (lambda (function name)
                                   (push
                                    (list
                                     :make
                                     (functionp function)
                                     name)
                                    calls)
                                   'replacement-thread)))
                             (list
                              (auto-package-update-now-async
                               t)
                              apu--update-thread
                              (nreverse calls))))"##,
        expect![[
            r#"OK (replacement-thread replacement-thread ((:live existing-thread) (:signal existing-thread nil nil) (:make t "auto-package-update-now-async")))"#
        ]],
    )
}

pub(super) fn updates_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_package_update_async_creates_named_thread_and_stores_it(),
        auto_package_update_async_rejects_second_live_thread_without_force(),
        auto_package_update_async_force_signals_live_thread_clears_and_replaces_it(),
    ]
}

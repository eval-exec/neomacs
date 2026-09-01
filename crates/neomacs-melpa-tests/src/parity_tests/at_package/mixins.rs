use expect_test::expect;

use super::ParityBatchCase;

fn at_soft_get_returns_configured_fallback_while_explicit_default_still_wins() -> ParityBatchCase {
    ParityBatchCase::value(
        "at_soft_get_returns_configured_fallback_while_explicit_default_still_wins",
        r##"(let ((first
                    (@extend @soft-get))
                   (second
                    (@extend
                     @soft-get
                     :default-get 'soft)))
               (list
                (@ first :missing)
                (@ second :missing)
                (@ second :missing
                   :default 'explicit)
                (@ second :default-get)))"##,
        expect!["OK (nil soft explicit soft)"],
    )
}

fn at_immutable_rejects_assignment_with_exact_property_error() -> ParityBatchCase {
    ParityBatchCase::signal(
        "at_immutable_rejects_assignment_with_exact_property_error",
        r##"(let ((object
                    (@extend @immutable)))
               (setf
                (@ object :blocked)
                10))"##,
        expect![[r#"ERR (error "Object is immutable, cannot set :blocked")"#]],
    )
}

fn at_immutable_disabled_setter_returns_nil_without_assigning() -> ParityBatchCase {
    ParityBatchCase::value(
        "at_immutable_disabled_setter_returns_nil_without_assigning",
        r##"(let ((object
                    (@extend
                     @immutable
                     :immutable-error nil)))
               (list
                (setf
                 (@ object :blocked)
                 10)
                (@ object :blocked
                   :default 'absent)
                (@ object
                   :immutable-error)))"##,
        expect!["OK (nil absent nil)"],
    )
}

fn at_watchable_notifies_in_order_assigns_after_callbacks_and_unwatches() -> ParityBatchCase {
    ParityBatchCase::value(
        "at_watchable_notifies_in_order_assigns_after_callbacks_and_unwatches",
        r##"(let (events)
               (let* ((first
                       (lambda (object
                                property new)
                         (push
                          (list
                           'first property
                           (if (eq property
                                   :watchers)
                               (length new)
                             new)
                           (let ((current
                                  (@ object property
                                     :default
                                     'absent)))
                             (if (eq property
                                     :watchers)
                                 (length current)
                               current)))
                          events)))
                      (second
                       (lambda (object
                                property new)
                         (push
                          (list
                           'second property
                           (if (eq property
                                   :watchers)
                               (length new)
                             new)
                           (let ((current
                                  (@ object property
                                     :default
                                     'absent)))
                             (if (eq property
                                     :watchers)
                                 (length current)
                               current)))
                          events)))
                      (object
                       (@extend
                        @watchable
                        :watchers
                        (list first second))))
                 (setf (@ object :foo) 1)
                 (@! object :unwatch second)
                 (setf (@ object :bar) 2)
                 (list
                  (@ object :foo)
                  (@ object :bar)
                  (length
                   (@ object :watchers))
                  (nreverse events))))"##,
        expect![[
            r#"OK (1 2 1 ((first :foo 1 absent) (second :foo 1 absent) (first :watchers 1 2) (second :watchers 1 2) (first :bar 2 absent)))"#
        ]],
    )
}

pub(super) fn mixins_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        at_soft_get_returns_configured_fallback_while_explicit_default_still_wins(),
        at_immutable_rejects_assignment_with_exact_property_error(),
        at_immutable_disabled_setter_returns_nil_without_assigning(),
        at_watchable_notifies_in_order_assigns_after_callbacks_and_unwatches(),
    ]
}

//! Lisp-visible dbusbind surface: feature, subrs, DEFVARs, special events.

use crate::test_utils::runtime_startup_eval_one;

#[test]
fn dbusbind_surface_matches_have_dbus() {
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one(
        "(list
           (featurep 'dbusbind)
           (mapcar #'fboundp '(dbus--init-bus dbus-get-unique-name
                               dbus-message-internal dbus--fd-open
                               dbus--fd-close dbus--registered-fds))
           (mapcar #'boundp '(dbus-compiled-version dbus-runtime-version
                              dbus-message-type-invalid
                              dbus-message-type-method-call
                              dbus-message-type-method-return
                              dbus-message-type-error
                              dbus-message-type-signal
                              dbus-registered-objects-table
                              dbus-debug))
           (get 'dbus-error 'error-conditions)
           (lookup-key special-event-map [dbus-event]))",
    );
    std::cfg_select! {
        neomacs_have_dbus => {
            assert!(
                result.starts_with(
                    "OK (t (t t t t t t) (t t t t t t t t t) (dbus-error error)"
                ),
                "expected dbusbind surface, got {result}"
            );
            assert!(
                result.contains("dbus-handle-event"),
                "special-event-map [dbus-event] should bind dbus-handle-event: {result}"
            );
        }
        _ => {
            assert_eq!(
                result,
                "OK (nil (nil nil nil nil nil nil) (nil nil nil nil nil nil nil nil nil) nil nil)"
            );
        }
    }
}

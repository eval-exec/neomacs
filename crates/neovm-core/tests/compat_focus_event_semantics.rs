mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

fn run_case(name: &str, form: &str) {
    if !oracle_enabled() {
        eprintln!(
            "skipping focus-event audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "focus-event semantics mismatch for {}:\nGNU: {}\nNeoVM: {}",
        name, gnu, neovm
    );
}

#[test]
fn compat_special_event_map_bootstraps_focus_handlers_matches_gnu_emacs() {
    run_case(
        "special_event_map_bootstraps_focus_handlers",
        r#"
(list
 (lookup-key special-event-map [focus-in])
 (lookup-key special-event-map [focus-out]))
"#,
    );
}

#[test]
fn compat_special_event_map_bootstraps_delete_frame_handler_matches_gnu_emacs() {
    run_case(
        "special_event_map_bootstraps_delete_frame_handler",
        r#"
(lookup-key special-event-map [delete-frame])
"#,
    );
}

#[test]
fn compat_handle_focus_events_match_gnu_emacs() {
    run_case(
        "handle_focus_events",
        r#"
(progn
  (defvar compat--focus-log nil)
  (defun compat--focus-in-hook ()
    (push 'focus-in-hook compat--focus-log))
  (defun compat--focus-out-hook ()
    (push 'focus-out-hook compat--focus-log))
  (defun compat--after-focus-change ()
    (push 'after-focus-change compat--focus-log))
  (let ((frame (selected-frame))
        (saved-focus-in-hook focus-in-hook)
        (saved-focus-out-hook focus-out-hook)
        (saved-after-focus-change-function after-focus-change-function))
    (setq focus-in-hook nil
          focus-out-hook nil
          after-focus-change-function #'ignore)
    (add-hook 'focus-in-hook #'compat--focus-in-hook)
    (add-hook 'focus-out-hook #'compat--focus-out-hook)
    (add-function :after after-focus-change-function
                  #'compat--after-focus-change)
    (unwind-protect
        (list
         (progn
           (setq compat--focus-log nil)
           (handle-focus-out (list 'focus-out frame))
           (list (nreverse compat--focus-log)
                 (frame-parameter frame 'last-focus-update)))
         (progn
           (setq compat--focus-log nil)
           (handle-focus-in (list 'focus-in frame))
           (list (nreverse compat--focus-log)
                 (frame-parameter frame 'last-focus-update))))
      (setq focus-in-hook saved-focus-in-hook
            focus-out-hook saved-focus-out-hook
            after-focus-change-function saved-after-focus-change-function)
      (fmakunbound 'compat--focus-in-hook)
      (fmakunbound 'compat--focus-out-hook)
      (fmakunbound 'compat--after-focus-change)
      (makunbound 'compat--focus-log))))
"#,
    );
}

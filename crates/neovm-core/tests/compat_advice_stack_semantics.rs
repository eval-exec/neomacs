mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

fn run_case(name: &str, form: &str) {
    if !oracle_enabled() {
        eprintln!(
            "skipping advice stack audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "advice stack semantics mismatch for {}:\nGNU: {}\nNeoVM: {}",
        name, gnu, neovm
    );
}

#[test]
fn compat_backtrace_frame_marks_funcall_interactively_matches_gnu_emacs() {
    run_case(
        "backtrace_frame_marks_funcall_interactively",
        r#"
(progn
  (defun compat--bt-marker-target ()
    (interactive)
    (nth 1 (backtrace-frame 1 'compat--bt-marker-target)))
  (unwind-protect
      (list
       (funcall-interactively 'compat--bt-marker-target)
       (call-interactively 'compat--bt-marker-target))
    (fmakunbound 'compat--bt-marker-target)))
"#,
    );
}

#[test]
fn compat_advised_called_interactively_p_matches_gnu_emacs() {
    run_case(
        "advised_called_interactively_p",
        r#"
(progn
  (defun compat--advice-ci-target ()
    (interactive)
    (list (called-interactively-p 'any)
          (called-interactively-p 'interactive)))
  (defun compat--advice-ci-around (orig &rest args)
    (apply orig args))
  (advice-add 'compat--advice-ci-target :around 'compat--advice-ci-around)
  (unwind-protect
      (list
       (funcall-interactively 'compat--advice-ci-target)
       (call-interactively 'compat--advice-ci-target))
    (advice-remove 'compat--advice-ci-target 'compat--advice-ci-around)
    (fmakunbound 'compat--advice-ci-around)
    (fmakunbound 'compat--advice-ci-target)))
"#,
    );
}

#[test]
fn compat_around_advice_stack_shape_matches_gnu_emacs() {
    run_case(
        "around_advice_stack_shape",
        r#"
(progn
  (defun compat--advice-stack-target ()
    (interactive)
    (list 'target
          (called-interactively-p 'any)
          (called-interactively-p 'interactive)
          (nth 1 (backtrace-frame 1 'compat--advice-stack-target))))
  (defun compat--advice-stack-around (orig &rest args)
    (list 'around-enter
          (called-interactively-p 'any)
          (called-interactively-p 'interactive)
          (nth 1 (backtrace-frame 1 'compat--advice-stack-around))
          (apply orig args)))
  (advice-add 'compat--advice-stack-target :around 'compat--advice-stack-around)
  (unwind-protect
      (list
       (funcall-interactively 'compat--advice-stack-target)
       (call-interactively 'compat--advice-stack-target))
    (advice-remove 'compat--advice-stack-target 'compat--advice-stack-around)
    (fmakunbound 'compat--advice-stack-around)
    (fmakunbound 'compat--advice-stack-target)))
"#,
    );
}

#[test]
fn compat_before_advice_stack_shape_matches_gnu_emacs() {
    run_case(
        "before_advice_stack_shape",
        r#"
(progn
  (defvar compat--advice-stack-before-result nil)
  (defun compat--advice-stack-target ()
    (interactive)
    (list 'target
          (called-interactively-p 'any)
          (called-interactively-p 'interactive)
          (nth 1 (backtrace-frame 1 'compat--advice-stack-target))))
  (defun compat--advice-stack-before (&rest _args)
    (setq compat--advice-stack-before-result
          (list 'before
                (called-interactively-p 'any)
                (called-interactively-p 'interactive)
                (nth 1 (backtrace-frame 1 'compat--advice-stack-before)))))
  (advice-add 'compat--advice-stack-target :before 'compat--advice-stack-before)
  (unwind-protect
      (list
       (list
        (funcall-interactively 'compat--advice-stack-target)
        compat--advice-stack-before-result)
       (progn
         (setq compat--advice-stack-before-result nil)
         (list
          (call-interactively 'compat--advice-stack-target)
          compat--advice-stack-before-result)))
    (advice-remove 'compat--advice-stack-target 'compat--advice-stack-before)
    (fmakunbound 'compat--advice-stack-before)
    (fmakunbound 'compat--advice-stack-target)
    (makunbound 'compat--advice-stack-before-result)))
"#,
    );
}

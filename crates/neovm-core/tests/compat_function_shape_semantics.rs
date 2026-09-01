mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

#[test]
fn compat_function_shape_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping function shape audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(let ((symbols '(if
                        throw
                        load
                        set-window-buffer
                        select-window
                        window-buffer
                        face-attributes-as-vector
                        switch-to-buffer
                        switch-to-buffer-other-window)))
  (mapcar
   (lambda (sym)
     (let ((fn (symbol-function sym)))
       (list sym
             (functionp fn)
             (if (subrp fn) 'subr 'elisp)
             (autoloadp fn))))
   symbols))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "function shape mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}

#[test]
fn compat_public_evaluator_subr_masking_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping public evaluator subr masking audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(let ((before
         (list
          (list 'if (fboundp 'if) (special-form-p 'if)
                (let ((fn (symbol-function 'if)))
                  (list (subrp fn) (autoloadp fn))))
          (list 'throw (fboundp 'throw) (special-form-p 'throw)
                (let ((fn (symbol-function 'throw)))
                  (list (subrp fn) (autoloadp fn))))))
      (saved-if (symbol-function 'if))
      (saved-throw (symbol-function 'throw)))
  (unwind-protect
      (progn
        (fmakunbound 'if)
        (fmakunbound 'throw)
        (list before
              (list 'if (fboundp 'if) (symbol-function 'if)
                    (condition-case err (if t 1 2) (error (car err))))
              (list 'throw (fboundp 'throw) (symbol-function 'throw)
                    (condition-case err (throw 'tag 1) (error (car err))))))
    (fset 'if saved-if)
    (fset 'throw saved-throw)))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "public evaluator subr masking mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}

#[test]
fn compat_gnu_lisp_macro_surface_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping GNU Lisp macro surface audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(let ((symbols '(eval-when-compile
                        declare
                        eval-and-compile
                        defvar-local
                        with-current-buffer
                        with-temp-buffer
                        with-output-to-string
                        track-mouse
                        with-syntax-table
                        with-mutex)))
  (mapcar
   (lambda (sym)
     (let ((fn (symbol-function sym)))
       (list sym
             (fboundp sym)
             (macrop sym)
             (special-form-p sym)
             (subrp fn)
             (car-safe fn)
             (car-safe (cdr-safe fn)))))
   symbols))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "GNU Lisp macro surface mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}

#[test]
fn compat_gnu_owned_callables_masking_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping GNU-owned callable masking audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(let ((cases
         '((setq-default . (setq-default vm-mask-a 1))
           (define-error . (define-error 'vm-mask-error "Mask error"))
           (defcustom . (defcustom vm-mask-opt 1 "Mask opt"))
           (defgroup . (defgroup vm-mask-group nil "Mask group"))
           (autoload . (autoload 'vm-mask-fn "vm-mask-file")))))
  (mapcar
   (lambda (entry)
     (let* ((sym (car entry))
            (call-form (cdr entry))
            (saved (symbol-function sym))
            (before (list (fboundp sym)
                          (macrop sym)
                          (special-form-p sym))))
       (unwind-protect
           (progn
             (fmakunbound sym)
             (list sym
                   before
                   (list (fboundp sym)
                         (condition-case err
                             (eval call-form t)
                           (error err)))))
         (fset sym saved))))
   cases))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "GNU-owned callable masking mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}

#[test]
fn compat_loaddefs_runtime_helper_surface_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping loaddefs runtime helper audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(let ((symbols '(function-put
                        register-definition-prefixes
                        custom-autoload
                        make-obsolete
                        make-obsolete-variable
                        define-obsolete-function-alias
                        define-obsolete-variable-alias)))
  (mapcar
   (lambda (sym)
     (let ((fn (symbol-function sym)))
       (list sym
             (fboundp sym)
             (macrop sym)
             (special-form-p sym)
             (functionp fn)
             (subrp fn)
             (autoloadp fn)
             (car-safe fn)
             (car-safe (cdr-safe fn)))))
   symbols))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "loaddefs runtime helper surface mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}

mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

fn run_case(name: &str, form: &str) {
    if !oracle_enabled() {
        eprintln!(
            "skipping advice place audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "advice place semantics mismatch for {}:\nGNU: {}\nNeoVM: {}",
        name, gnu, neovm
    );
}

#[test]
fn compat_add_function_and_advice_mapc_on_symbol_function_matches_gnu_emacs() {
    run_case(
        "add_function_and_advice_mapc_on_symbol_function",
        r#"
(progn
  (defun compat--place-target (x)
    (list 'target x))
  (defun compat--place-around (orig x)
    (list 'around (funcall orig x)))
  (unwind-protect
      (progn
        (add-function :around (symbol-function 'compat--place-target)
                      #'compat--place-around
                      '((name . compat-place-around) (depth . -50)))
        (list
         (compat--place-target 1)
         (let (seen)
           (advice-mapc
            (lambda (f props)
              (push (list f
                          (cdr (assq 'name props))
                          (cdr (assq 'depth props)))
                    seen))
            'compat--place-target)
           (nreverse seen))
         (progn
           (remove-function (symbol-function 'compat--place-target)
                            'compat-place-around)
           (compat--place-target 2))))
    (ignore-errors
      (remove-function (symbol-function 'compat--place-target)
                       'compat-place-around))
    (fmakunbound 'compat--place-around)
    (fmakunbound 'compat--place-target)))
"#,
    );
}

#[test]
fn compat_add_function_on_local_place_matches_gnu_emacs() {
    run_case(
        "add_function_on_local_place",
        r#"
(progn
  (defvar compat--local-place-fn nil)
  (setq-default compat--local-place-fn
                (lambda (x) (list 'global x)))
  (defun compat--local-place-around (orig x)
    (list 'local-around (funcall orig x)))
  (let ((other (get-buffer-create " *compat-advice-other*")))
    (unwind-protect
        (with-temp-buffer
          (setq-local compat--local-place-fn
                      (lambda (x) (list 'local x)))
          (add-function :around (local 'compat--local-place-fn)
                        #'compat--local-place-around)
          (list
           (funcall compat--local-place-fn 1)
           (with-current-buffer other
             (funcall compat--local-place-fn 2))
           (progn
             (remove-function (local 'compat--local-place-fn)
                              #'compat--local-place-around)
             (funcall compat--local-place-fn 3))))
      (when (buffer-live-p other)
        (kill-buffer other))
      (makunbound 'compat--local-place-fn)
      (fmakunbound 'compat--local-place-around))))
"#,
    );
}

#[test]
fn compat_add_function_on_process_filter_place_matches_gnu_emacs() {
    run_case(
        "add_function_on_process_filter_place",
        r#"
(progn
  (defun compat--proc-filter-around (orig proc string)
    (list 'filter string (null (funcall orig proc string))))
  (let ((p (make-pipe-process :name "compat-adv-filter")))
    (unwind-protect
        (progn
          (add-function :around (process-filter p)
                        #'compat--proc-filter-around)
          (list
           (funcall (process-filter p) p "chunk")
           (progn
             (remove-function (process-filter p)
                              #'compat--proc-filter-around)
             (funcall (process-filter p) p "chunk"))))
      (ignore-errors (delete-process p))
      (fmakunbound 'compat--proc-filter-around))))
"#,
    );
}

#[test]
fn compat_add_function_on_process_sentinel_place_matches_gnu_emacs() {
    run_case(
        "add_function_on_process_sentinel_place",
        r#"
(progn
  (defun compat--proc-sentinel-around (orig proc string)
    (list 'sentinel string (null (funcall orig proc string))))
  (let ((p (make-pipe-process :name "compat-adv-sentinel")))
    (unwind-protect
        (progn
          (add-function :around (process-sentinel p)
                        #'compat--proc-sentinel-around)
          (list
           (funcall (process-sentinel p) p "done")
           (progn
             (remove-function (process-sentinel p)
                              #'compat--proc-sentinel-around)
             (funcall (process-sentinel p) p "done"))))
      (ignore-errors (delete-process p))
      (fmakunbound 'compat--proc-sentinel-around))))
"#,
    );
}

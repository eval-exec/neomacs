use expect_test::expect;

use super::ParityBatchCase;

fn current_package_modeline_mode_counts_only_install_jobs_and_dings_on_disable() -> ParityBatchCase
{
    ParityBatchCase::value(
        "current_package_modeline_mode_counts_only_install_jobs_and_dings_on_disable",
        r##"
(let (dinged observed)
  (cl-letf (((symbol-function 'dired-async-processes)
             (lambda (&optional property)
               (setq observed property)
               '(one two three)))
            ((symbol-function 'ding)
             (lambda (&rest _) (setq dinged visible-bell))))
    (unwind-protect
        (progn
          (async-package--modeline-mode 1)
          (let ((lighter
                 (eval
                  (cadr
                   (assq :eval
                         (cdr
                          (assq 'async-package--modeline-mode
                                minor-mode-alist)))))))
            (async-package--modeline-mode -1)
            (list (substring-no-properties lighter)
                  (get-text-property 0 'face lighter)
                  observed dinged
                  async-package--modeline-mode)))
      (async-package--modeline-mode -1))))
"##,
        expect![[
            r#"OK (" [3 async job Installing package(s)]" async-package-message async-pkg-install t nil)"#
        ]],
    )
}

fn current_package_install_builds_child_executes_packages_and_finishes_lifecycle() -> ParityBatchCase
{
    ParityBatchCase::value(
        "current_package_install_builds_child_executes_packages_and_finishes_lifecycle",
        r##"
(let* ((root (file-name-as-directory
              (async-melpa-test-path "package/install/")))
       (temporary-file-directory root)
       (async-byte-compile-log-file "async-bytecomp.log")
       (error-file (expand-file-name "install-errors.log" root))
       (package-archives '(("fixture" . "https://invalid.example/")))
       (package-pinned-packages '((one . "fixture")))
       (package-archive-contents nil)
       (package-user-dir (expand-file-name "elpa/" root))
       (package-alist nil)
       (package-selected-packages '(existing))
       hook-runs
       (after-hook
        (lambda ()
          (setq hook-runs
                (1+ (or hook-runs 0)))))
       child callback process-properties installed activated saved
       modeline messages timer-notices descriptors)
  (make-directory root t)
  (add-hook 'async-pkg-install-after-hook after-hook)
  (unwind-protect
      (cl-letf (((symbol-function 'async-start)
                 (lambda (start finish)
                   (setq child start callback finish)
                   'fixture-install-process))
                ((symbol-function 'process-put)
                 (lambda (&rest args) (push args process-properties)))
                ((symbol-function 'package-install)
                 (lambda (package) (push package installed) package))
                ((symbol-function 'async-package--modeline-mode)
                 (lambda (arg) (push arg modeline)))
                ((symbol-function 'message)
                 (lambda (format-string &rest args)
                   (push (apply #'format format-string args) messages)))
                ((symbol-function 'customize-save-variable)
                 (lambda (symbol value) (setq saved (list symbol value))))
                ((symbol-function 'package-load-all-descriptors)
                 (lambda () (setq descriptors t)))
                ((symbol-function 'package-activate)
                 (lambda (package) (push package activated)))
                ((symbol-function 'run-with-timer)
                 (lambda (_ _ function &rest args)
                   (apply function args)))
                ((symbol-function 'dired-async-mode-line-message)
                 (lambda (&rest args) (push args timer-notices)))
                ((symbol-function 'async-bytecomp--file-to-comp-buffer-1)
                 (lambda (&rest args) (error "unexpected nonempty log: %S" args))))
        (async-package-do-action 'install '(one two) error-file)
        (let ((child-result (funcall child)))
          (funcall callback child-result)
          (list
           (nreverse installed)
           child-result
           (nreverse process-properties)
           (nreverse activated)
           saved descriptors
           (nreverse modeline)
           (nreverse messages)
           (nreverse timer-notices)
           hook-runs
           (file-exists-p error-file))))
    (remove-hook 'async-pkg-install-after-hook after-hook)))
"##,
        expect![[
            r#"OK ((one two) (one two) ((fixture-install-process async-pkg-install t)) (one two) (package-selected-packages (one two existing)) t (1 -1) ("Installing 2 package(s)..." "Installing 2 packages done") (("%s %d package(s) done" async-package-message "Installing" 2)) 1 nil)"#
        ]],
    )
}

fn current_package_upgrade_and_reinstall_select_exact_functions_and_messages() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_package_upgrade_and_reinstall_select_exact_functions_and_messages",
        r##"
(let* ((root (file-name-as-directory
              (async-melpa-test-path "package/actions/")))
       (temporary-file-directory root)
       (async-byte-compile-log-file "async-bytecomp.log")
       captures messages modeline process-properties)
  (make-directory root t)
  (cl-letf (((symbol-function 'async-start)
             (lambda (start finish)
               (push (list start finish) captures)
               'fixture-process))
            ((symbol-function 'process-put)
             (lambda (&rest args) (push args process-properties)))
            ((symbol-function 'async-package--modeline-mode)
             (lambda (arg) (push arg modeline)))
            ((symbol-function 'message)
             (lambda (format-string &rest args)
               (push (apply #'format format-string args) messages))))
    (async-package-do-action
     'upgrade '(alpha beta gamma)
     (expand-file-name "upgrade-errors.log" root))
    (async-package-do-action
     'reinstall '(delta)
     (expand-file-name "reinstall-errors.log" root))
    (list
     (mapcar
      (lambda (capture)
        (let ((printed (prin1-to-string (car capture))))
          (list
           (and (string-match-p "(mapc 'package-upgrade" printed) 'upgrade)
           (and (string-match-p "(mapc 'package-reinstall" printed) 'reinstall)
           (functionp (cadr capture)))))
      (nreverse captures))
     (nreverse messages)
     (nreverse modeline)
     (nreverse process-properties))))
"##,
        expect![[
            r#"OK (((upgrade nil t) (nil reinstall t)) ("Upgrading 3 package(s)..." "Reinstalling 1 package(s)...") (1 1) ((fixture-process async-pkg-install t) (fixture-process async-pkg-install t)))"#
        ]],
    )
}

fn current_package_error_callback_opens_special_buffer_deletes_error_and_runs_hook()
-> ParityBatchCase {
    ParityBatchCase::value(
        "current_package_error_callback_opens_special_buffer_deletes_error_and_runs_hook",
        r##"
(let* ((root (file-name-as-directory
              (async-melpa-test-path "package/error/")))
       (temporary-file-directory root)
       (async-byte-compile-log-file "async-bytecomp.log")
       (error-file (expand-file-name "errors.log" root))
       child callback popped modeline hook-runs)
  (make-directory root t)
  (add-hook 'async-pkg-install-after-hook #'ignore)
  (unwind-protect
      (cl-letf (((symbol-function 'async-start)
                 (lambda (start finish)
                   (setq child start callback finish)
                   'fixture-process))
                ((symbol-function 'process-put) #'ignore)
                ((symbol-function 'message) #'ignore)
                ((symbol-function 'pop-to-buffer)
                 (lambda (buffer action)
                   (setq popped (list (buffer-name buffer) action))
                   (set-buffer buffer)))
                ((symbol-function 'async-package--modeline-mode)
                 (lambda (arg) (push arg modeline)))
                ((symbol-function 'run-hooks)
                 (lambda (&rest hooks) (setq hook-runs hooks))))
        (async-package-do-action 'install '(broken) error-file)
        (async-melpa-test-write-file error-file "fixture package failure")
        (funcall callback nil)
        (let ((buffer (get-buffer (file-name-nondirectory error-file))))
          (prog1
              (list
               (car child)
               popped
               (and buffer
                    (with-current-buffer buffer
                      (list (buffer-string) (derived-mode-p 'special-mode))))
               (file-exists-p error-file)
               modeline hook-runs)
            (when buffer (kill-buffer buffer)))))
    (remove-hook 'async-pkg-install-after-hook #'ignore)))
"##,
        expect![[
            r#"OK (lambda ("errors.log" (nil (window-height . fit-window-to-buffer))) ("fixture package failure" special-mode) nil (-1 1) (async-pkg-install-after-hook))"#
        ]],
    )
}

pub(super) fn package_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        current_package_modeline_mode_counts_only_install_jobs_and_dings_on_disable(),
        current_package_install_builds_child_executes_packages_and_finishes_lifecycle(),
        current_package_upgrade_and_reinstall_select_exact_functions_and_messages(),
        current_package_error_callback_opens_special_buffer_deletes_error_and_runs_hook(),
    ]
}

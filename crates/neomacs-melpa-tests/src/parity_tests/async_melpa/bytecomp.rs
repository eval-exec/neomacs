use expect_test::expect;

use super::ParityBatchCase;

fn current_bytecomp_defaults_and_customization_metadata_match_gnu_emacs() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_bytecomp_defaults_and_customization_metadata_match_gnu_emacs",
        r##"
(list
 async-bytecomp-allowed-packages
 async-byte-compile-log-file
 async-bytecomp-load-variable-regexp
 (get 'async-bytecomp-allowed-packages 'custom-type)
 (get 'async-bytecomp-package-mode 'custom-type)
 (get 'async-bytecomp-package-mode 'globalized-minor-mode)
 (documentation 'async-byte-compile-file)
 (help-function-arglist 'async-bytecomp--file-to-comp-buffer t))
"##,
        expect![[
            r#"OK (all "async-bytecomp.log" "\\`load-path\\'" (choice (const :tag "All packages" all) (repeat symbol)) boolean nil "Byte compile Lisp code FILE asynchronously.\n\nSame as ‘byte-compile-file’ but asynchronous." (file-or-dir &optional quiet type log-file))"#
        ]],
    )
}

fn current_bytecomp_log_file_is_imported_into_compilation_buffer_and_removed() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_bytecomp_log_file_is_imported_into_compilation_buffer_and_removed",
        r##"
(let* ((log-file (async-melpa-test-path "bytecomp/import.log"))
       (byte-compile-log-buffer "*async-bytecomp-import*")
       displayed
       postprocessed)
  (async-melpa-test-write-file
   log-file
   "fixture.el:3:2: Warning: first\nfixture.el:8:1: Error: broken\n")
  (unwind-protect
      (cl-letf (((symbol-function 'display-buffer)
                 (lambda (buffer &rest _)
                   (setq displayed (buffer-name buffer))
                   buffer)))
        (async-bytecomp--file-to-comp-buffer-1
         log-file
         (lambda () (setq postprocessed t)))
        (with-current-buffer byte-compile-log-buffer
          (list displayed
                postprocessed
                (derived-mode-p 'compilation-mode)
                (buffer-string)
                (file-exists-p log-file))))
    (async-melpa-test-kill-buffers byte-compile-log-buffer)))
"##,
        expect![[
            r#"OK ("*async-bytecomp-import*" t compilation-mode "fixture.el:3:2: Warning: first\nfixture.el:8:1: Error: broken\n" nil)"#
        ]],
    )
}

fn current_bytecomp_completion_reports_success_warnings_and_error_counts() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_bytecomp_completion_reports_success_warnings_and_error_counts",
        r##"
(let* ((root (async-melpa-test-path "bytecomp/completion/"))
       (warning-log (expand-file-name "warnings.log" root))
       (error-log (expand-file-name "errors.log" root))
       (byte-compile-log-buffer "*async-bytecomp-completion*")
       messages)
  (make-directory root t)
  (async-melpa-test-write-file warning-log "a.el:1: Warning: caution\n")
  (async-melpa-test-write-file
   error-log
   "a.el:1: Error: first\nb.el:2: Error: second\n")
  (unwind-protect
      (cl-letf (((symbol-function 'display-buffer) #'ignore)
                ((symbol-function 'message)
                 (lambda (format-string &rest args)
                   (push (apply #'format format-string args) messages))))
        (async-bytecomp--file-to-comp-buffer root nil 'directory nil)
        (async-bytecomp--file-to-comp-buffer "one.el" nil 'file warning-log)
        (with-current-buffer byte-compile-log-buffer
          (let ((inhibit-read-only t))
            (erase-buffer)))
        (async-bytecomp--file-to-comp-buffer root nil 'directory error-log)
        (nreverse messages))
    (async-melpa-test-kill-buffers byte-compile-log-buffer)))
"##,
        expect![[
            r#"OK ("Directory `completion' compiled asynchronously with success" "File `one.el' compiled asynchronously with warnings" "Directory `completion' compiled asynchronously with warnings")"#
        ]],
    )
}

fn current_comp_buffer_to_file_uses_sandbox_prefix_and_preserves_diagnostics() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_comp_buffer_to_file_uses_sandbox_prefix_and_preserves_diagnostics",
        r##"
(let* ((root (file-name-as-directory
              (async-melpa-test-path "bytecomp/export/")))
       (temporary-file-directory root)
       (async-byte-compile-log-file
        (expand-file-name "reports/compile." root))
       (byte-compile-log-buffer "*async-bytecomp-export*"))
  (make-directory (file-name-directory async-byte-compile-log-file) t)
  (unwind-protect
      (progn
        (get-buffer-create byte-compile-log-buffer)
        (let ((empty (async-bytecomp--comp-buffer-to-file)))
          (with-current-buffer byte-compile-log-buffer
            (insert "fixture.el:9: Error: deterministic\n"))
          (let ((log-file (async-bytecomp--comp-buffer-to-file)))
            (prog1
                (list empty
                      (file-name-directory log-file)
                      (string-prefix-p "compile." (file-name-nondirectory log-file))
                      (async-melpa-test-read-file log-file))
              (delete-file log-file)))))
    (async-melpa-test-kill-buffers byte-compile-log-buffer)))
"##,
        expect![[
            r#"OK (nil "[ORACLE-SANDBOX]/bytecomp/export/" t "fixture.el:9: Error: deterministic\n")"#
        ]],
    )
}

fn current_package_dependency_walk_is_transitive_deduplicated_and_cycle_safe() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_package_dependency_walk_is_transitive_deduplicated_and_cycle_safe",
        r##"
(let* ((desc (lambda (name reqs)
               (package-desc-create
                :name name :version '(1 0) :summary "fixture"
                :reqs reqs :kind 'tar :archive "fixture"
                :dir (async-melpa-test-path (format "bytecomp/%s/" name)))))
       (package-archive-contents
        `((app ,(funcall desc 'app '((left (1 0)) (right (1 0)))))
          (left ,(funcall desc 'left '((shared (1 0)))))
          (right ,(funcall desc 'right '((shared (1 0)) (missing (1 0)))))
          (shared ,(funcall desc 'shared '((app (1 0)))))))
       (package-alist
        `((fallback ,(funcall desc 'fallback '((shared (1 0))))))))
  (list
   (async-bytecomp--get-package-deps '(app))
   (async-bytecomp--get-package-deps '(fallback))
   (async-bytecomp--get-package-deps '(absent app right))))
"##,
        expect![
            "OK ((right shared left app) (right left app shared fallback) (right shared left app))"
        ],
    )
}

fn current_directory_recompile_removes_stale_elc_and_constructs_real_job() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_directory_recompile_removes_stale_elc_and_constructs_real_job",
        r##"
(let* ((root (file-name-as-directory
              (async-melpa-test-path "bytecomp/recompile/")))
       (source (expand-file-name "fixture.el" root))
       (stale (expand-file-name "fixture.elc" root))
       child callback messages loaded)
  (async-melpa-test-write-file source "(defun fixture-value () 42)\n")
  (async-melpa-test-write-file stale "stale")
  (cl-letf (((symbol-function 'load)
             (lambda (file &rest _) (push file loaded) t))
            ((symbol-function 'async-start)
             (lambda (start finish)
               (setq child start callback finish)
               'fixture-process))
            ((symbol-function 'message)
             (lambda (format-string &rest args)
               (push (apply #'format format-string args) messages))))
    (let ((result (async-byte-recompile-directory root)))
      (list result
            (file-exists-p stale)
            loaded
            (car child)
            (and
             (string-match-p "byte-recompile-directory"
                             (prin1-to-string child))
             t)
            (functionp callback)
            (nreverse messages)))))
"##,
        expect![[
            r#"OK (#1=("Started compiling asynchronously directory [ORACLE-SANDBOX]/bytecomp/recompile/") nil ("async") lambda t t #1#)"#
        ]],
    )
}

fn current_single_file_compile_constructs_child_and_callback_protocol() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_single_file_compile_constructs_child_and_callback_protocol",
        r##"
(let* ((file (async-melpa-test-path "bytecomp/single/fixture.el"))
       child callback imported)
  (async-melpa-test-write-file file "(defun fixture-single () :ok)\n")
  (cl-letf (((symbol-function 'async-start)
             (lambda (start finish)
               (setq child start callback finish)
               'fixture-process))
            ((symbol-function 'async-bytecomp--file-to-comp-buffer)
             (lambda (&rest args) (setq imported args))))
    (let ((result (async-byte-compile-file file)))
      (funcall callback "fixture.log")
      (list result
            (car child)
            (and
             (string-match-p (regexp-quote file) (prin1-to-string child))
             t)
            (and
             (string-match-p "byte-compile-file" (prin1-to-string child))
             t)
            imported))))
"##,
        expect![[
            r#"OK (fixture-process lambda t t ("[ORACLE-SANDBOX]/bytecomp/single/fixture.el" nil file "fixture.log"))"#
        ]],
    )
}

fn current_package_compile_routes_allowed_dependency_and_sync_packages() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_package_compile_routes_allowed_dependency_and_sync_packages",
        r##"
(let* ((root (async-melpa-test-path "bytecomp/package/"))
       (make-desc
        (lambda (name reqs)
          (package-desc-create
           :name name :version '(1 0) :summary "fixture"
           :reqs reqs :kind 'tar :archive "fixture"
           :dir (expand-file-name (format "%s/" name) root))))
       (app (funcall make-desc 'app '((dep (1 0)))))
       (dep (funcall make-desc 'dep nil))
       (other (funcall make-desc 'other nil))
       (package-archive-contents `((app ,app) (dep ,dep) (other ,other)))
       compiled synchronous)
  (cl-letf (((symbol-function 'async-byte-recompile-directory)
             (lambda (directory &optional quiet)
               (push (list directory quiet) compiled)
               :async))
            ((symbol-function 'fixture-original)
             (lambda (desc &rest args)
               (push (list (package-desc-name desc) args) synchronous)
               :sync)))
    (let ((async-bytecomp-allowed-packages 'all))
      (async--package-compile #'fixture-original other :all))
    (let ((async-bytecomp-allowed-packages '(app)))
      (async--package-compile #'fixture-original dep :dependency)
      (async--package-compile #'fixture-original other :other))
    (list (nreverse compiled) (nreverse synchronous))))
"##,
        expect![[
            r#"OK ((("[ORACLE-SANDBOX]/bytecomp/package/other/" t) ("[ORACLE-SANDBOX]/bytecomp/package/dep/" t)) ((other (:other))))"#
        ]],
    )
}

fn current_bytecomp_package_mode_adds_and_removes_exact_advice() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_bytecomp_package_mode_adds_and_removes_exact_advice",
        r##"
(unwind-protect
    (progn
      (async-bytecomp-package-mode -1)
      (let ((before (advice-member-p #'async--package-compile 'package--compile)))
        (async-bytecomp-package-mode 1)
        (let ((enabled (advice-member-p #'async--package-compile 'package--compile)))
          (async-bytecomp-package-mode -1)
          (list (and before t)
                (and enabled t)
                (and
                 (advice-member-p #'async--package-compile 'package--compile)
                 t)
                async-bytecomp-package-mode))))
  (async-bytecomp-package-mode -1))
"##,
        expect!["OK (nil t nil nil)"],
    )
}

pub(super) fn bytecomp_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        current_bytecomp_defaults_and_customization_metadata_match_gnu_emacs(),
        current_bytecomp_log_file_is_imported_into_compilation_buffer_and_removed(),
        current_bytecomp_completion_reports_success_warnings_and_error_counts(),
        current_comp_buffer_to_file_uses_sandbox_prefix_and_preserves_diagnostics(),
        current_package_dependency_walk_is_transitive_deduplicated_and_cycle_safe(),
        current_directory_recompile_removes_stale_elc_and_constructs_real_job(),
        current_single_file_compile_constructs_child_and_callback_protocol(),
        current_package_compile_routes_allowed_dependency_and_sync_packages(),
        current_bytecomp_package_mode_adds_and_removes_exact_advice(),
    ]
}

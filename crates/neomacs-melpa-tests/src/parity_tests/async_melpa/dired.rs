use expect_test::expect;

use super::ParityBatchCase;

fn current_dired_defaults_and_customization_metadata_match_gnu_emacs() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_dired_defaults_and_customization_metadata_match_gnu_emacs",
        r##"
(list
 dired-async-env-variables-regexp
 dired-async-message-function
 dired-async-mode-lighter
 dired-async-skip-fast
 dired-async-small-file-max
 dired-async-large-file-warning-threshold
 (file-name-nondirectory
  dired-async-log-file)
 (file-name-absolute-p
  dired-async-log-file)
 (get 'dired-async-log-file 'custom-type)
 (get 'dired-async-mode 'custom-type)
 (get 'dired-async-mode 'globalized-minor-mode)
 (get 'dired-async-small-file-max 'risky-local-variable)
 (mapcar
  (lambda (face)
    (list
     face
     (facep face)
     (get face 'face-defface-spec)))
  '(dired-async-message
    dired-async-failures
    dired-async-mode-message))
 (help-function-arglist 'dired-async-create-files t))
"##,
        expect![[
            r#"OK ("\\`\\(tramp-\\(default\\|connection\\|remote\\)\\|ange-ftp\\)-.*" dired-async-mode-line-message (:eval (when (eq major-mode 'dired-mode) " Async")) nil 5000000 10000000 "dired-async.log" t string boolean nil t ((dired-async-message [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] ((t (:foreground "yellow")))) (dired-async-failures [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] ((t (:foreground "red")))) (dired-async-mode-message [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] ((t (:foreground "Gold"))))) (file-creator operation fn-list name-constructor &optional _marker-char))"#
        ]],
    )
}

fn current_dired_file_classification_uses_real_sizes_directories_and_devices() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_dired_file_classification_uses_real_sizes_directories_and_devices",
        r##"
(let* ((root (file-name-as-directory
              (async-melpa-test-path "dired/classify/")))
       (small (expand-file-name "small.txt" root))
       (large (expand-file-name "large.txt" root))
       (nested (expand-file-name "nested/" root))
       (dired-async-small-file-max 8))
  (async-melpa-test-write-file small "1234567")
  (async-melpa-test-write-file large "12345678")
  (make-directory nested t)
  (list
   (list
    'directory
    (async-melpa-test-outcome
     (lambda ()
       (dired-async--directory-p
        (file-attributes nested)))))
   (list
    'small
    (async-melpa-test-outcome
     (lambda ()
       (dired-async--small-file-p small))))
   (list
    'threshold
    (async-melpa-test-outcome
     (lambda ()
       (dired-async--small-file-p large))))
   (list
    'nested
    (async-melpa-test-outcome
     (lambda ()
       (dired-async--small-file-p nested))))
   (list
    'device
    (async-melpa-test-outcome
     (lambda ()
       (dired-async--same-device-p small root))))
   (list
    'copy
    (async-melpa-test-outcome
     (lambda ()
       (dired-async--skip-async-p
        'dired-copy-file small (lambda (_) large)))))
   (list
    'rename
    (async-melpa-test-outcome
     (lambda ()
       (dired-async--skip-async-p
        'dired-rename-file large
        (lambda (_) (expand-file-name "renamed" root))))))))
"##,
        expect![
            "OK ((directory (:value t)) (small (:value t)) (threshold (:value nil)) (nested (:value nil)) (device (:value t)) (copy (:value t)) (rename (:value t)))"
        ],
    )
}

fn current_smart_create_files_splits_fast_work_and_promotes_large_aggregate() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_smart_create_files_splits_fast_work_and_promotes_large_aggregate",
        r##"
(let* ((root (file-name-as-directory
              (async-melpa-test-path "dired/smart/")))
       (small-a (expand-file-name "small-a" root))
       (small-b (expand-file-name "small-b" root))
       (large (expand-file-name "large" root))
       (dired-async-skip-fast t)
       (dired-async-small-file-max 10)
       async-calls
       sync-calls)
  (async-melpa-test-write-file small-a "1234")
  (async-melpa-test-write-file small-b "12345")
  (async-melpa-test-write-file large "12345678901234567890")
  (cl-letf (((symbol-function 'dired-async-create-files)
             (lambda (&rest args) (push args async-calls))))
    (let ((old (lambda (&rest args) (push args sync-calls))))
      (dired-async--smart-create-files
       old 'dired-copy-file "Copy" (list small-a large small-b)
       (lambda (file)
         (expand-file-name (file-name-nondirectory file) "dest/"))
       ?*)
      (let ((split (list (nreverse async-calls) (nreverse sync-calls))))
        (setq async-calls nil sync-calls nil dired-async-small-file-max 8)
        (dired-async--smart-create-files
         old 'dired-copy-file "Copy" (list small-a small-b)
         (lambda (file)
           (expand-file-name (file-name-nondirectory file) "dest/"))
         ?+)
        (list split (nreverse async-calls) (nreverse sync-calls))))))
"##,
        expect![[
            r#"OK ((((dired-copy-file "Copy" ("[ORACLE-SANDBOX]/dired/smart/large") #1=#[(file) ((expand-file-name (file-name-nondirectory file) "dest/")) #2=(t)] 42)) ((dired-copy-file "Copy" ("[ORACLE-SANDBOX]/dired/smart/small-a" "[ORACLE-SANDBOX]/dired/smart/small-b") #1# 42))) ((dired-copy-file "Copy" ("[ORACLE-SANDBOX]/dired/smart/small-a" "[ORACLE-SANDBOX]/dired/smart/small-b") #[(file) ((expand-file-name (file-name-nondirectory file) "dest/")) #2#] 43)) nil)"#
        ]],
    )
}

fn current_large_file_guard_has_exact_threshold_and_abort_semantics() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_large_file_guard_has_exact_threshold_and_abort_semantics",
        r##"
(let ((dired-async-large-file-warning-threshold 100)
      asked)
  (cl-letf (((symbol-function 'files--ask-user-about-large-file)
             (lambda (&rest args)
               (push args asked)
               'abort)))
    (list
     (dired-async--abort-if-file-too-large 100 "copy" "equal")
     (dired-async--abort-if-file-too-large 101 "copy" "over")
     (progn
       (setq dired-async-large-file-warning-threshold nil)
       (dired-async--abort-if-file-too-large 1000 "rename" "disabled"))
     (nreverse asked))))
"##,
        expect![[r#"OK (nil abort nil ((101 "copy" "over" nil)))"#]],
    )
}

fn current_process_registry_filters_properties_and_kills_latest_job() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_process_registry_filters_properties_and_kills_latest_job",
        r##"
(let ((p1 (make-process :name "async-dired-one"
                        :command '("sh" "-c" "sleep 20")
                        :noquery t))
      (p2 (make-process :name "async-dired-two"
                        :command '("sh" "-c" "sleep 20")
                        :noquery t))
      modeline)
  (unwind-protect
      (progn
        (process-put p1 'dired-async-process t)
        (process-put p2 'async-pkg-install t)
        (cl-letf (((symbol-function 'dired-async--modeline-mode)
                   (lambda (arg) (push arg modeline))))
          (let ((all (mapcar #'process-name (dired-async-processes)))
                (packages
                 (mapcar #'process-name
                         (dired-async-processes 'async-pkg-install))))
            (dired-async-kill-process)
            (list all packages
                  (process-live-p p1)
                  (process-live-p p2)
                  modeline))))
    (when (process-live-p p1) (delete-process p1))
    (when (process-live-p p2) (delete-process p2))))
"##,
        expect![[
            r#"OK (("async-dired-one") ("async-dired-two") nil (run open listen connect stop) (-1))"#
        ]],
    )
}

fn current_mode_line_message_formats_face_and_restores_outer_mode_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_mode_line_message_formats_face_and_restores_outer_mode_line",
        r##"
(let ((mode-line-format '("outer"))
      updates
      observed)
  (cl-letf (((symbol-function 'message) #'ignore)
            ((symbol-function 'force-mode-line-update)
             (lambda (&rest _) (push mode-line-format updates)))
            ((symbol-function 'sit-for)
             (lambda (_)
               (setq observed mode-line-format)
               t)))
    (dired-async-mode-line-message "Copied %d %s" 'success 3 "files")
    (list (substring-no-properties observed)
          (get-text-property 1 'face observed)
          mode-line-format
          (length updates))))
"##,
        expect![[r#"OK (" Copied 3 files" success ("outer") 2)"#]],
    )
}

fn current_after_file_create_imports_error_log_and_reports_success_and_failures() -> ParityBatchCase
{
    ParityBatchCase::value(
        "current_after_file_create_imports_error_log_and_reports_success_and_failures",
        r##"
(let* ((dired-async-log-file
        (async-melpa-test-path "dired/callback/errors.log"))
       (dired-log-buffer "*async-dired-log*")
       (dired-buffers nil)
       notices
       modeline)
  (async-melpa-test-write-file
   dired-async-log-file
   "Copy failed: permission denied\n")
  (unwind-protect
      (cl-letf (((symbol-function 'dired-async-processes) (lambda (&optional _) nil))
                ((symbol-function 'dired-async--modeline-mode)
                 (lambda (arg) (push arg modeline)))
                ((symbol-function 'pop-to-buffer)
                 (lambda (buffer &rest _) (set-buffer buffer)))
                ((symbol-function 'shrink-window-if-larger-than-buffer) #'ignore)
                ((symbol-function 'run-with-timer)
                 (lambda (_ _ function &rest args)
                   (apply function args)))
                ((symbol-function 'fixture-notice)
                 (lambda (&rest args) (push args notices))))
        (let ((dired-async-message-function #'fixture-notice))
          (dired-async-after-file-create
           3 '("Copy" 2) '("failed.txt") '("skipped.txt"))
          (let ((error-result
                 (with-current-buffer dired-log-buffer
                   (list (buffer-string)
                         (derived-mode-p 'special-mode)
                         (file-exists-p dired-async-log-file)))))
            (with-current-buffer dired-log-buffer (erase-buffer))
            (dired-async-after-file-create
             3 '("Copy" 2) '("failed.txt") '("skipped.txt"))
            (list error-result (nreverse notices) modeline))))
    (async-melpa-test-kill-buffers dired-log-buffer)))
"##,
        expect![[
            r#"OK (("Error: Copy failed: permission denied\n" special-mode nil) (("%s failed for %d of %d file%s -- See *Dired log* buffer" dired-async-failures "Copy" 1 3 "s") ("Asynchronous %s of %s on %s file%s done" dired-async-message "Copy" 2 3 "s")) (-1 -1))"#
        ]],
    )
}

fn current_maybe_kill_ftp_form_kills_only_first_matching_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_maybe_kill_ftp_form_kills_only_first_matching_buffer",
        r##"
(let ((ftp-one (get-buffer-create "*ftp fixture one*"))
      (ftp-two (get-buffer-create "*ftp fixture two*"))
      (ordinary (get-buffer-create "*fixture ordinary*")))
  (unwind-protect
      (progn
        (eval (dired-async-maybe-kill-ftp))
        (list
         (buffer-live-p ftp-one)
         (buffer-live-p ftp-two)
         (buffer-live-p ordinary)
         (car (dired-async-maybe-kill-ftp))))
    (mapc (lambda (buffer)
            (when (buffer-live-p buffer) (kill-buffer buffer)))
          (list ftp-one ftp-two ordinary))))
"##,
        expect!["OK (nil t t progn)"],
    )
}

fn current_create_files_same_destination_reports_skip_without_starting_process() -> ParityBatchCase
{
    ParityBatchCase::value(
        "current_create_files_same_destination_reports_skip_without_starting_process",
        r##"
(let* ((root (file-name-as-directory
              (async-melpa-test-path "dired/no-job/")))
       (file (expand-file-name "same.txt" root))
       (dired-async-large-file-warning-threshold nil)
       logs notices started)
  (async-melpa-test-write-file file "fixture")
  (cl-letf (((symbol-function 'dired-log)
             (lambda (&rest args) (push args logs)))
            ((symbol-function 'fixture-notice)
             (lambda (&rest args) (push args notices)))
            ((symbol-function 'async-start)
             (lambda (&rest _) (setq started t))))
    (let ((dired-async-message-function #'fixture-notice))
      (let ((outcome
             (async-melpa-test-outcome
              (lambda ()
                (dired-async-create-files
                 'dired-copy-file "Copy" (list file) #'identity)))))
        (list outcome started (nreverse logs)
              (nreverse notices) overwrite-query)))))
"##,
        expect![[
            r#"OK ((:signal (wrong-type-argument stringp nil)) nil (("Cannot %s to same file: %s\n" "copy" "[ORACLE-SANDBOX]/dired/no-job/same.txt") (t)) (("%s: %d of %d file%s skipped -- See *Dired log* buffer" dired-async-failures "Copy" 1 1 "")) nil)"#
        ]],
    )
}

fn current_create_files_async_branch_constructs_job_callback_and_process_metadata()
-> ParityBatchCase {
    ParityBatchCase::value(
        "current_create_files_async_branch_constructs_job_callback_and_process_metadata",
        r##"
(let* ((root (file-name-as-directory
              (async-melpa-test-path "dired/job/")))
       (source (expand-file-name "source.txt" root))
       (destination (expand-file-name "dest.txt" root))
       (dired-async-large-file-warning-threshold nil)
       (dired-create-destination-dirs nil)
       child callback process-properties modeline messages)
  (async-melpa-test-write-file source "payload")
  (cl-letf (((symbol-function 'async-start)
             (lambda (start finish)
               (setq child start callback finish)
               'fixture-process))
            ((symbol-function 'process-put)
             (lambda (&rest args) (push args process-properties)))
            ((symbol-function 'dired-async--modeline-mode)
             (lambda (arg) (push arg modeline)))
            ((symbol-function 'message)
             (lambda (format-string &rest args)
               (push (apply #'format format-string args) messages))))
    (dired-async-create-files
     'dired-copy-file "Copy" (list source)
     (lambda (_) destination))
    (list
     (car child)
     (and
      (string-match-p (regexp-quote source) (prin1-to-string child))
      t)
     (and
      (string-match-p (regexp-quote destination) (prin1-to-string child))
      t)
     (functionp callback)
     process-properties modeline (nreverse messages))))
"##,
        expect![[
            r#"OK (lambda t t t ((fixture-process dired-async-process t)) (1) ("Copy proceeding asynchronously..."))"#
        ]],
    )
}

fn current_wdired_advice_modes_and_four_command_wrappers_preserve_arguments() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_wdired_advice_modes_and_four_command_wrappers_preserve_arguments",
        r##"
(let (wdired-observation command-observations)
  (cl-letf (((symbol-function 'fixture-wdired)
             (lambda (&rest args)
               (setq wdired-observation
                     (list wdired-use-interactive-rename args))))
            ((symbol-function 'dired-do-copy)
             (lambda (arg)
               (push (list 'copy arg
                           (eq (symbol-function 'dired-create-files)
                               #'dired-async-create-files))
                     command-observations)))
            ((symbol-function 'dired-do-symlink)
             (lambda (arg)
               (push (list 'symlink arg
                           (eq (symbol-function 'dired-create-files)
                               #'dired-async-create-files))
                     command-observations)))
            ((symbol-function 'dired-do-hardlink)
             (lambda (arg)
               (push (list 'hardlink arg
                           (eq (symbol-function 'dired-create-files)
                               #'dired-async-create-files))
                     command-observations)))
            ((symbol-function 'dired-do-rename)
             (lambda (arg)
               (push (list 'rename arg
                           (eq (symbol-function 'dired-create-files)
                               #'dired-async-create-files))
                     command-observations))))
    (let ((wdired-use-interactive-rename t))
      (dired-async-wdired-do-renames #'fixture-wdired :one :two))
    (dired-async-do-copy '(4))
    (dired-async-do-symlink '-)
    (dired-async-do-hardlink nil)
    (dired-async-do-rename 7)
    (unwind-protect
        (progn
          (dired-async-mode -1)
          (let ((before
                 (list
                  (and
                   (advice-member-p #'dired-async--smart-create-files
                                    'dired-create-files)
                   t)
                  (and
                   (advice-member-p #'dired-async-wdired-do-renames
                                    'wdired-do-renames)
                   t))))
            (dired-async-mode 1)
            (let ((enabled
                   (list
                    (and
                     (advice-member-p #'dired-async--smart-create-files
                                      'dired-create-files)
                     t)
                    (and
                     (advice-member-p #'dired-async-wdired-do-renames
                                      'wdired-do-renames)
                     t))))
              (dired-async-mode -1)
              (list wdired-observation
                    (nreverse command-observations)
                    before enabled
                    dired-async-mode))))
      (dired-async-mode -1))))
"##,
        expect![
            "OK ((nil (:one :two)) ((copy (4) t) (symlink - t) (hardlink nil t) (rename 7 t)) (nil nil) (t t) nil)"
        ],
    )
}

pub(super) fn dired_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        current_dired_defaults_and_customization_metadata_match_gnu_emacs(),
        current_dired_file_classification_uses_real_sizes_directories_and_devices(),
        current_smart_create_files_splits_fast_work_and_promotes_large_aggregate(),
        current_large_file_guard_has_exact_threshold_and_abort_semantics(),
        current_process_registry_filters_properties_and_kills_latest_job(),
        current_mode_line_message_formats_face_and_restores_outer_mode_line(),
        current_after_file_create_imports_error_log_and_reports_success_and_failures(),
        current_maybe_kill_ftp_form_kills_only_first_matching_buffer(),
        current_create_files_same_destination_reports_skip_without_starting_process(),
        current_create_files_async_branch_constructs_job_callback_and_process_metadata(),
        current_wdired_advice_modes_and_four_command_wrappers_preserve_arguments(),
    ]
}

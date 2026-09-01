//! Practical parity for Format All's native and external formatter workflows.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, FORMAT_ALL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'format-all)

(defconst format399-test-source
  '("format-all.el"
    "552416d1dc85953fc2646e7b75892c914e57c27237b9a69a56f7e75ff0020a41"))

(defun format399-test-file-sha256 (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(let* ((loaded (symbol-file 'format-all-buffer 'defun))
       (source (and loaded
                    (if (string-suffix-p ".elc" loaded)
                        (concat (file-name-sans-extension loaded) ".el")
                      loaded))))
  (unless (and (featurep 'inheritenv)
               (featurep 'language-id)
               (file-regular-p source)
               (not (file-symlink-p source))
               (equal (file-name-nondirectory source)
                      (car format399-test-source))
               (equal (format399-test-file-sha256 source)
                      (cadr format399-test-source)))
    (error "Unexpected installed Format All sources: %S" source)))

(defvar format399-test-root nil)
(defvar format399-test-program nil)
(defvar format399-test-plan nil)
(defvar format399-test-trace nil)

(defun format399-test-normalize (value root)
  (cond ((stringp value)
         (replace-regexp-in-string (regexp-quote root) "[ROOT]/" value t t))
        ((consp value)
         (cons (format399-test-normalize (car value) root)
               (format399-test-normalize (cdr value) root)))
        ((vectorp value)
         (apply #'vector
                (mapcar (lambda (item) (format399-test-normalize item root))
                        value)))
        (t value)))

(defun format399-test-condition (condition root)
  (list :type (car condition)
        :data (format399-test-normalize (copy-tree (cdr condition)) root)
        :message (format399-test-normalize
                  (error-message-string condition) root)))

(defun format399-test-window-state ()
  (mapcar (lambda (window)
            (list (window-buffer window)
                  (window-point window)
                  (window-start window)))
          (window-list nil 'nomini)))

(defun format399-test-write-file (root relative contents)
  (let ((path (expand-file-name relative root)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert contents)
      (let ((coding-system-for-write 'utf-8-unix))
        (write-region (point-min) (point-max) path nil 'silent)))
    path))

(defun format399-test-manifest (root)
  (mapcar
   (lambda (path)
     (unless (and (file-regular-p path) (not (file-symlink-p path)))
       (error "Non-regular Format All fixture entry: %s" path))
     (list (file-relative-name path root)
           (format399-test-file-sha256 path)))
   (sort (directory-files-recursively root "." nil nil nil) #'string-lessp)))

(defun format399-test-park-buffer (name)
  (when-let* ((buffer (get-buffer name)))
    (let ((old-name (buffer-name buffer)))
      (with-current-buffer buffer
        (rename-buffer (format " *format399-parked-%s*" (sxhash-eq buffer)) t))
      (cons buffer old-name))))

(defun format399-test-forbid-external (name &rest arguments)
  (error "Unexpected Format All external boundary: %S %S" name arguments))

(defun format399-test-call-process-region
    (start end program delete destination display &rest args)
  (unless format399-test-plan
    (error "Unexpected extra formatter invocation: %S %S" program args))
  (let* ((step (pop format399-test-plan))
         (input (and (stringp start) start))
         (stdout (plist-get step :stdout))
         (stderr (plist-get step :stderr))
         (status (plist-get step :status))
         (expected-input (plist-get step :input))
         (expected-args (plist-get step :args))
         (error-file (and (listp destination) (cadr destination))))
    (unless (and input
                 (null end)
                 (equal program format399-test-program)
                 (null delete)
                 (equal (car destination) t)
                 (stringp error-file)
                 (file-in-directory-p error-file temporary-file-directory)
                 (equal (file-name-directory error-file)
                        temporary-file-directory)
                 (file-regular-p error-file)
                 (not (file-symlink-p error-file))
                 (null display)
                 (equal input expected-input)
                 (equal (format399-test-normalize
                         (copy-tree args) format399-test-root)
                        expected-args)
                 (file-in-directory-p default-directory format399-test-root))
      (error "Unexpected formatter call: %S"
             (list start end program delete destination display args step
                   default-directory)))
    (push (list :input input
                :args (copy-tree args)
                :directory default-directory
                :status status
                :stdout stdout
                :stderr stderr)
          format399-test-trace)
    (insert stdout)
    (with-temp-file error-file (insert stderr))
    (unless (and (file-regular-p error-file) (not (file-symlink-p error-file)))
      (error "Unsafe formatter stderr file: %s" error-file))
    status))

(defun format399-test-hook-state (formatter status)
  (push (list formatter status
              (buffer-substring-no-properties (point-min) (point-max)))
        format399-test-hook-log))

(defun format399-test-run (files plan body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name "format-all/" sandbox))))
         (window-before (current-window-configuration))
         (window-state-before (format399-test-window-state))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (format-all-show-errors 'never)
         (format-all-debug nil)
         (format-all-after-format-functions nil)
         (transient-mark-mode transient-mark-mode)
         (auto-save-default nil)
         (create-lockfiles nil)
         (make-backup-files nil)
         (message-log-max nil)
         (print-circle nil)
         (parked nil)
         (root-owned nil)
         fixture-before fixture-after result body-error cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute Format All sandbox root"))
              (when (file-exists-p root)
                (error "Format All sandbox root already exists: %s" root))
              (dolist (name '("*format-all-errors*" "*Help*"))
                (when-let* ((entry (format399-test-park-buffer name)))
                  (push entry parked)))
              (make-directory root t)
              (setq root-owned t)
              (dolist (file files)
                (format399-test-write-file root (car file) (cdr file)))
              (let ((program (format399-test-write-file root "bin/shfmt" "fixture\n")))
                (set-file-modes program #o700)
                (let ((format399-test-root root)
                      (format399-test-program program)
                      (format399-test-plan (copy-tree plan))
                      (format399-test-trace nil)
                      (temporary-file-directory
                       (file-name-as-directory (expand-file-name "tmp" root))))
                  (make-directory temporary-file-directory t)
                  (setq fixture-before (format399-test-manifest root))
                  (setq result
                        (cl-letf (((symbol-function 'call-process-region)
                                   #'format399-test-call-process-region)
                                  ((symbol-function 'call-process)
                                   (lambda (&rest arguments)
                                     (apply #'format399-test-forbid-external
                                            'call-process arguments)))
                                  ((symbol-function 'process-file)
                                   (lambda (&rest arguments)
                                     (apply #'format399-test-forbid-external
                                            'process-file arguments)))
                                  ((symbol-function 'start-process)
                                   (lambda (&rest arguments)
                                     (apply #'format399-test-forbid-external
                                            'start-process arguments)))
                                  ((symbol-function 'start-file-process)
                                   (lambda (&rest arguments)
                                     (apply #'format399-test-forbid-external
                                            'start-file-process arguments)))
                                  ((symbol-function 'make-process)
                                   (lambda (&rest arguments)
                                     (apply #'format399-test-forbid-external
                                            'make-process arguments)))
                                  ((symbol-function 'url-retrieve-synchronously)
                                   (lambda (&rest arguments)
                                     (apply #'format399-test-forbid-external
                                            'url-retrieve-synchronously arguments))))
                          (funcall body root program)))
                  (unless (null format399-test-plan)
                    (error "Unused formatter plan: %S" format399-test-plan))
                  (setq result
                        (list :body result
                              :trace (nreverse format399-test-trace)))))
              (setq fixture-after (format399-test-manifest root))
              (unless (equal fixture-before fixture-after)
                (error "Format All fixture changed: %S -> %S"
                       fixture-before fixture-after)))
          (error (setq body-error (format399-test-condition condition root))))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case condition (delete-process process)
            (error (push (format399-test-condition condition root)
                         cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case condition (kill-buffer buffer)
            (error (push (format399-test-condition condition root)
                         cleanup-errors)))))
      (dolist (timer (copy-sequence timer-list))
        (unless (memq timer timers-before)
          (condition-case condition (cancel-timer timer)
            (error (push (format399-test-condition condition root)
                         cleanup-errors)))))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (condition-case condition (delete-frame frame t)
            (error (push (format399-test-condition condition root)
                         cleanup-errors)))))
      (condition-case condition (set-window-configuration window-before)
        (error (push (format399-test-condition condition root) cleanup-errors)))
      (dolist (entry parked)
        (condition-case condition
            (if (buffer-live-p (car entry))
                (with-current-buffer (car entry) (rename-buffer (cdr entry) t))
              (error "Parked Format All buffer died: %S" entry))
          (error (push (format399-test-condition condition root) cleanup-errors))))
      (when (buffer-live-p buffer-before) (set-buffer buffer-before))
      (when root-owned
        (condition-case condition (delete-directory root t)
          (error (push (format399-test-condition condition root) cleanup-errors)))))
    (let ((cleanup
           (list :new-buffers
                 (mapcar #'buffer-name
                         (seq-filter (lambda (buffer)
                                       (and (buffer-live-p buffer)
                                            (not (memq buffer buffers-before))))
                                     (buffer-list)))
                 :new-processes
                 (length (seq-remove (lambda (process)
                                       (memq process processes-before))
                                     (process-list)))
                 :new-timers
                 (length (seq-remove (lambda (timer) (memq timer timers-before))
                                     timer-list))
                 :new-frames
                 (length (seq-remove (lambda (frame) (memq frame frames-before))
                                     (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :fixture-restored (equal fixture-before fixture-after)
                 :window-restored
                 (equal window-state-before (format399-test-window-state))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Format All workflow failed: %S" (list result cleanup))
        (format399-test-normalize
         (list :source (cadr format399-test-source)
               :result result
               :cleanup cleanup)
         root)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(FORMAT_ALL_MELPA_PIN, "format-all.el")
        .expect("prepare exact Format All source and dependencies below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_native_buffer_format_preserves_location_and_supports_undo() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_native_buffer_format_preserves_location_and_supports_undo",
        r####"
(format399-test-run
 nil nil
 (lambda (_root _program)
   (with-temp-buffer
     (emacs-lisp-mode)
     (buffer-enable-undo)
     (insert "(progn\n(message \"界\")\n(list 1\n2))\n")
     (setq-local format-all-formatters '(("Emacs Lisp" emacs-lisp)))
     (setq-local format399-test-hook-log nil)
     (add-hook 'format-all-after-format-functions
               #'format399-test-hook-state nil t)
     (goto-char (point-min))
     (forward-line 2)
     (move-to-column 7)
     (let ((before (buffer-substring-no-properties (point-min) (point-max)))
           (line-before (line-number-at-pos))
           (column-before (current-column)))
       (undo-boundary)
       (call-interactively #'format-all-buffer)
       (undo-boundary)
       (let ((formatted (buffer-substring-no-properties (point-min) (point-max)))
             (line-after (line-number-at-pos))
             (column-after (current-column))
             (hooks (reverse (copy-tree format399-test-hook-log))))
         (undo-only 1)
         (list :before before
               :formatted formatted
               :location (list line-before column-before line-after column-after)
               :hooks hooks
               :undo (buffer-substring-no-properties (point-min) (point-max))))))))
"####,
        expect![[
            r#"OK (:source "552416d1dc85953fc2646e7b75892c914e57c27237b9a69a56f7e75ff0020a41" :result (:body (:before "(progn\n(message \"界\")\n(list 1\n2))\n" :formatted "(progn\n  (message \"界\")\n  (list 1\n\0112))\n" :location (3 7 3 7) :hooks ((emacs-lisp :reformatted "(progn\n  (message \"界\")\n  (list 1\n\0112))\n")) :undo "(progn\n(message \"界\")\n(list 1\n2))\n") :trace nil) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :fixture-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_region_or_buffer_formats_only_active_region() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_region_or_buffer_formats_only_active_region",
        r####"
(format399-test-run
 nil nil
 (lambda (_root _program)
   (with-temp-buffer
     (emacs-lisp-mode)
     (insert "(list 1\n2)\n\n(list 3\n4)\n")
     (setq-local format-all-formatters '(("Emacs Lisp" emacs-lisp)))
     (setq-local format399-test-hook-log nil)
     (add-hook 'format-all-after-format-functions
               #'format399-test-hook-state nil t)
     (goto-char (point-min))
     (search-forward "(list 3")
     (beginning-of-line)
     (push-mark (point-max) t t)
     (setq transient-mark-mode t mark-active t)
     (call-interactively #'format-all-region-or-buffer)
     (list :text (buffer-substring-no-properties (point-min) (point-max))
           :region-active (use-region-p)
           :hooks (reverse (copy-tree format399-test-hook-log))))))
"####,
        expect![[
            r#"OK (:source "552416d1dc85953fc2646e7b75892c914e57c27237b9a69a56f7e75ff0020a41" :result (:body (:text "(list 1\n2)\n\n(list 3\n      4)\n" :region-active t :hooks ((emacs-lisp :reformatted "(list 1\n2)\n\n(list 3\n      4)\n"))) :trace nil) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :fixture-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn external_formatter_reformats_then_reports_already_formatted() -> ParityBatchCase {
    ParityBatchCase::value(
        "external_formatter_reformats_then_reports_already_formatted",
        r####"
(format399-test-run
 '(("src/demo.sh" . "echo    hello\n"))
 (list (list :input "echo    hello\n"
             :args '("-filename" "[ROOT]/src/demo.sh.sh")
             :stdout "echo hello\n" :stderr "" :status 0)
       (list :input "echo hello\n"
             :args '("-filename" "[ROOT]/src/demo.sh.sh")
             :stdout "echo hello\n" :stderr "" :status 0))
 (lambda (root program)
   (let* ((file (expand-file-name "src/demo.sh" root))
          (buffer (find-file-noselect file)))
     (with-current-buffer buffer
       (sh-mode)
       (setq-local format-all-formatters
                   `(("Shell" (shfmt :executable ,program))))
       (setq-local format399-test-hook-log nil)
       (add-hook 'format-all-after-format-functions
                 #'format399-test-hook-state nil t)
       (call-interactively #'format-all-buffer)
       (let ((first (buffer-substring-no-properties (point-min) (point-max))))
         (call-interactively #'format-all-buffer)
         (list :first first
               :second (buffer-substring-no-properties (point-min) (point-max))
               :modified (buffer-modified-p)
               :hooks (reverse (copy-tree format399-test-hook-log))))))))
"####,
        expect![[
            r#"OK (:source "552416d1dc85953fc2646e7b75892c914e57c27237b9a69a56f7e75ff0020a41" :result (:body (:first "echo hello\n" :second "echo hello\n" :modified t :hooks ((shfmt :reformatted "echo hello\n") (shfmt :already-formatted "echo hello\n"))) :trace ((:input "echo    hello\n" :args ("-filename" "[ROOT]/src/demo.sh.sh") :directory "[ROOT]/src/" :status 0 :stdout "echo hello\n" :stderr "") (:input "echo hello\n" :args ("-filename" "[ROOT]/src/demo.sh.sh") :directory "[ROOT]/src/" :status 0 :stdout "echo hello\n" :stderr ""))) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :fixture-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn formatter_error_keeps_source_and_recovers_on_next_public_run() -> ParityBatchCase {
    ParityBatchCase::value(
        "formatter_error_keeps_source_and_recovers_on_next_public_run",
        r####"
(format399-test-run
 nil
 (list (list :input "BAD syntax\n" :args '("-ln" "bash")
             :stdout "" :stderr "line 1: bad syntax\n" :status 7)
       (list :input "echo    recovered\n" :args '("-ln" "bash")
             :stdout "echo recovered\n" :stderr "" :status 0))
 (lambda (root program)
   (with-temp-buffer
     (setq default-directory root)
     (sh-mode)
     (insert "BAD syntax\n")
     (setq-local format-all-formatters
                 `(("Shell" (shfmt :executable ,program))))
     (setq-local format-all-show-errors 'always)
     (setq-local format399-test-hook-log nil)
     (add-hook 'format-all-after-format-functions
               #'format399-test-hook-state nil t)
     (call-interactively #'format-all-buffer)
     (let ((failed-source (buffer-substring-no-properties (point-min) (point-max)))
           (failed-errors
            (with-current-buffer "*format-all-errors*"
              (buffer-substring-no-properties (point-min) (point-max)))))
       (erase-buffer)
       (insert "echo    recovered\n")
       (call-interactively #'format-all-buffer)
       (list :failed-source failed-source
             :failed-errors failed-errors
             :recovered (buffer-substring-no-properties (point-min) (point-max))
             :hooks (reverse (copy-tree format399-test-hook-log)))))))
"####,
        expect![[
            r#"OK (:source "552416d1dc85953fc2646e7b75892c914e57c27237b9a69a56f7e75ff0020a41" :result (:body (:failed-source "BAD syntax\n" :failed-errors "line 1: bad syntax\n" :recovered "echo recovered\n" :hooks ((shfmt :error "BAD syntax\n") (shfmt :reformatted "echo recovered\n"))) :trace ((:input "BAD syntax\n" :args ("-ln" "bash") :directory "[ROOT]/" :status 7 :stdout "" :stderr "line 1: bad syntax\n") (:input "echo    recovered\n" :args ("-ln" "bash") :directory "[ROOT]/" :status 0 :stdout "echo recovered\n" :stderr ""))) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :fixture-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn format_all_mode_formats_real_file_before_save_and_disables_cleanly() -> ParityBatchCase {
    ParityBatchCase::value(
        "format_all_mode_formats_real_file_before_save_and_disables_cleanly",
        r####"
(format399-test-run
 '(("src/save.sh" . "echo saved\n"))
 (list (list :input "echo    saved\n"
             :args '("-filename" "[ROOT]/src/save.sh.sh")
             :stdout "echo saved\n" :stderr "" :status 0))
 (lambda (root program)
   (let* ((file (expand-file-name "src/save.sh" root))
          (buffer (find-file-noselect file)))
     (with-current-buffer buffer
       (sh-mode)
       (setq-local format-all-formatters
                   `(("Shell" (shfmt :executable ,program))))
       (setq-local format399-test-hook-log nil)
       (add-hook 'format-all-after-format-functions
                 #'format399-test-hook-state nil t)
       (format-all-mode 1)
       (let ((enabled (list format-all-mode
                            (and (memq #'format-all--buffer-from-hook
                                       before-save-hook)
                                 t))))
         (erase-buffer)
         (insert "echo    saved\n")
         (save-buffer)
         (let ((buffer-text
                (buffer-substring-no-properties (point-min) (point-max)))
               (disk-text
                (with-temp-buffer
                  (insert-file-contents file)
                  (buffer-string)))
               (hooks (reverse (copy-tree format399-test-hook-log))))
           (format-all-mode -1)
           (let ((observation
                  (list :enabled enabled
                        :buffer buffer-text
                        :disk disk-text
                        :modified (buffer-modified-p)
                        :hooks hooks
                        :disabled
                        (list format-all-mode
                              (and (memq #'format-all--buffer-from-hook
                                         before-save-hook)
                                   t)))))
             (with-temp-file file (insert "echo saved\n"))
             observation)))))))
"####,
        expect![[
            r#"OK (:source "552416d1dc85953fc2646e7b75892c914e57c27237b9a69a56f7e75ff0020a41" :result (:body (:enabled (t t) :buffer "echo saved\n" :disk "echo saved\n" :modified nil :hooks ((shfmt :reformatted "echo saved\n")) :disabled (nil nil)) :trace ((:input "echo    saved\n" :args ("-filename" "[ROOT]/src/save.sh.sh") :directory "[ROOT]/src/" :status 0 :stdout "echo saved\n" :stderr ""))) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :fixture-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn format_all_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_native_buffer_format_preserves_location_and_supports_undo(),
        public_region_or_buffer_formats_only_active_region(),
        external_formatter_reformats_then_reports_already_formatted(),
        formatter_error_keeps_source_and_recovers_on_next_public_run(),
        format_all_mode_formats_real_file_before_save_and_disables_cleanly(),
    ];
    assert_oracle_batch_cases(oracle(), "format-all-rank399", "Format All", &cases);
}

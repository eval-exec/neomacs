//! Practical parity for dired-hacks-utils public listing helpers.
//!
//! These cases open a real Dired tree, skip non-file lines, query file
//! info, match names by regexp/extension, format the information line,
//! and compare files through a planted md5sum boundary.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DASH_MELPA_PIN, DIRED_HACKS_UTILS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'dired)
(require 'dash)
(require 'dired-hacks-utils)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")

(defconst dhu448-test-tree
  "09923383a9e2a8c796d3209c2cb503de53199e07")
(defconst dhu448-test-manifest
  '(("dired-hacks-utils-pkg.el" . "71947423e81fad4b190fb26b219083ce0f08f231ee6fadb070ec0dc1c120353e")
    ("dired-hacks-utils.el" . "9af0b8600a98c2faa004b328d2ef6a43cd44a4d939b6a80d6798570260a640c3")))
(defconst dhu448-test-switches "-l")

(defvar dhu448-test-case-index 0)
(defvar dhu448-test-root nil)
(defvar dhu448-test-root-owned nil)
(defvar dhu448-test-ledger nil)

(defun dhu448-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun dhu448-test-source-state ()
  (let* ((located (locate-library "dired-hacks-utils.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main)))
         (files
          (and directory
               (sort
                (mapcar (lambda (file) (file-relative-name file directory))
                        (seq-filter
                         (lambda (file)
                           (and (string-suffix-p ".el" file)
                                (not (string-suffix-p "-autoloads.el" file))))
                         (directory-files-recursively directory "\\.el\\'")))
                #'string<)))
         (manifest
          (and files
               (mapcar (lambda (file)
                         (cons file (dhu448-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/dired-hacks-utils.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car dhu448-test-manifest)))
      (error "Unexpected installed dired-hacks-utils payload: %S"
             (or manifest files)))
    (dolist (entry dhu448-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (dhu448-test-sha file) expected))
          (error "Unexpected installed dired-hacks-utils source: %S"
                 (cons entry manifest)))))
    (list :tree dhu448-test-tree
          :manifest manifest
          :feature (featurep 'dired-hacks-utils)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'dired-hacks-utils package-alist)))))))

(defun dhu448-test-forbid-external (operation &rest arguments)
  (error "Unexpected dired-hacks-utils external boundary: %S %S"
         operation arguments))

(defconst dhu448-test-real-call-process (symbol-function 'call-process))
(defconst dhu448-test-real-process-file (symbol-function 'process-file))

(defun dhu448-test-ls-p (program &rest arguments)
  (or (and program (stringp program)
           (or (equal program "ls")
               (equal program (executable-find "ls"))))
      ;; Wildcard Dired listings shell out: insert-directory runs
      ;; `bash -c "\\ls -d --dired ..."' rather than calling ls directly.
      (and program (stringp program)
           (member (file-name-nondirectory program) '("bash" "sh"))
           (member "-c" arguments)
           (cl-some (lambda (argument)
                      (and (stringp argument)
                           (string-match-p "\\`\\\\*ls \\|-c +\\\\*ls" argument)))
                    arguments))))

(defun dhu448-test-call-process (program &rest arguments)
  (unless (apply #'dhu448-test-ls-p program arguments)
    (apply #'dhu448-test-forbid-external 'call-process program arguments))
  (apply dhu448-test-real-call-process program arguments))

(defun dhu448-test-process-file (program &rest arguments)
  (unless (apply #'dhu448-test-ls-p program arguments)
    (apply #'dhu448-test-forbid-external 'process-file program arguments))
  (apply dhu448-test-real-process-file program arguments))

(defun dhu448-test-mask (value)
  (cond
   ((and (stringp value) dhu448-test-root)
    (replace-regexp-in-string (regexp-quote dhu448-test-root)
                              "[SANDBOX]/" value t t))
   ((stringp value) (copy-sequence value))
   (t value)))

(defun dhu448-test-write (root name code)
  (let ((file (expand-file-name name root)))
    (make-directory (file-name-directory file) t)
    (write-region code nil file nil 'silent)
    file))

(defun dhu448-test-names ()
  (mapcar (lambda (file)
            (dhu448-test-mask
             (file-relative-name file dhu448-test-root)))
          (dired-utils-get-all-files)))

(defun dhu448-test-run (body)
  (let* ((index (cl-incf dhu448-test-case-index))
         (sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (format "dired-hacks-utils-%d" index)
                                       sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (selected-window-before (selected-window))
         (window-before (current-window-configuration))
         (source-before (dhu448-test-source-state))
         (directory-before default-directory)
         (enable-local-before enable-local-variables)
         (debug-before debug-on-error)
         (print-circle-before print-circle)
         (hook-before (copy-sequence dired-after-readin-hook))
         (switches-before dired-listing-switches)
         (mode-before (and (boundp 'dired-utils-format-information-line-mode)
                           dired-utils-format-information-line-mode))
         (dhu448-test-root root)
         (dhu448-test-root-owned nil)
         (dhu448-test-ledger nil)
         result body-error source-after cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute dired-hacks-utils sandbox root"))
              (when (file-exists-p root)
                (error "dired-hacks-utils sandbox root exists: %S" root))
              (make-directory root)
              (setq dhu448-test-root-owned t
                    enable-local-variables nil
                    debug-on-error nil
                    print-circle nil
                    default-directory root
                    dired-listing-switches dhu448-test-switches)
              (cl-letf (((symbol-function 'call-process)
                         #'dhu448-test-call-process)
                        ((symbol-function 'process-file)
                         #'dhu448-test-process-file)
                        ((symbol-function 'call-process-region)
                         (lambda (&rest args)
                           (apply #'dhu448-test-forbid-external
                                  'call-process-region args)))
                        ((symbol-function 'start-process)
                         (lambda (&rest args)
                           (apply #'dhu448-test-forbid-external
                                  'start-process args)))
                        ((symbol-function 'start-file-process)
                         (lambda (&rest args)
                           (apply #'dhu448-test-forbid-external
                                  'start-file-process args)))
                        ((symbol-function 'make-process)
                         (lambda (&rest args)
                           (apply #'dhu448-test-forbid-external
                                  'make-process args)))
                        ((symbol-function 'make-network-process)
                         (lambda (&rest args)
                           (apply #'dhu448-test-forbid-external
                                  'make-network-process args)))
                        ((symbol-function 'shell-command)
                         (lambda (&rest args)
                           (apply #'dhu448-test-forbid-external
                                  'shell-command args)))
                        ((symbol-function 'url-retrieve)
                         (lambda (&rest args)
                           (apply #'dhu448-test-forbid-external
                                  'url-retrieve args)))
                        ((symbol-function 'url-retrieve-synchronously)
                         (lambda (&rest args)
                           (apply #'dhu448-test-forbid-external
                                  'url-retrieve-synchronously args)))
                        ((symbol-function 'kill-emacs)
                         (lambda (&rest args)
                           (apply #'dhu448-test-forbid-external
                                  'kill-emacs args))))
                (setq result (funcall body root)))
              (setq source-after (dhu448-test-source-state))
              (unless (equal source-before source-after)
                (error "dired-hacks-utils source changed")))
          (error (setq body-error
                       (list (car condition)
                             (copy-tree (cdr condition))))))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error (push (list label (car condition)
                                  (copy-tree (cdr condition)))
                            cleanup-errors)))))
        (when (boundp 'dired-utils-format-information-line-mode)
          (setq dired-utils-format-information-line-mode mode-before))
        (setq dired-after-readin-hook hook-before
              dired-listing-switches switches-before
              enable-local-variables enable-local-before
              debug-on-error debug-before
              print-circle print-circle-before
              default-directory directory-before)
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer (buffer-list))
          (unless (memq buffer buffers-before)
            (attempt (list 'buffer (buffer-name buffer))
                     (lambda ()
                       (when (buffer-live-p buffer)
                         (with-current-buffer buffer
                           (set-buffer-modified-p nil))
                         (kill-buffer buffer))))))
        (dolist (timer (append timer-list timer-idle-list))
          (unless (memq timer timers-before)
            (attempt 'timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'frame (lambda () (delete-frame frame t)))))
        (attempt 'window
                 (lambda () (set-window-configuration window-before)))
        (when (window-live-p selected-window-before)
          (attempt 'selected
                   (lambda () (select-window selected-window-before))))
        (when (buffer-live-p buffer-before)
          (attempt 'current-buffer
                   (lambda () (set-buffer buffer-before))))
        (when dhu448-test-root-owned
          (attempt 'root (lambda () (delete-directory root t))))))
    (when body-error
      (error "dired-hacks-utils body failed: %S" body-error))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :new-buffers (mapcar #'buffer-name
                                      (seq-remove
                                       (lambda (buffer)
                                         (memq buffer buffers-before))
                                       (buffer-list)))
                 :new-processes (length
                                 (seq-remove
                                  (lambda (process)
                                    (memq process processes-before))
                                  (process-list)))
                 :new-timers (length
                              (seq-remove
                               (lambda (timer)
                                 (memq timer timers-before))
                               (append timer-list timer-idle-list)))
                 :new-frames (length
                              (seq-remove
                               (lambda (frame)
                                 (memq frame frames-before))
                               (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :window-restored (eq (selected-window)
                                      selected-window-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if cleanup-errors
          (error "dired-hacks-utils cleanup failed: %S" (list result cleanup))
        (list :source source-before
              :result result
              :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DIRED_HACKS_UTILS_MELPA_PIN, "dired-hacks-utils.el")
        .expect("prepare pinned dired-hacks-utils source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned dash dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn lists_files_skips_headers_and_queries_info() -> ParityBatchCase {
    ParityBatchCase::value(
        "lists_files_skips_headers_and_queries_info",
        r####"
(dhu448-test-run
 (lambda (root)
   (dhu448-test-write root "café.txt" "hello")
   (dhu448-test-write root "notes.org" "* todo")
   (dhu448-test-write root "sub/inner.txt" "nested")
   (make-symbolic-link (expand-file-name "café.txt" root)
                       (expand-file-name "alias.txt" root))
   (dired root)
   (goto-char (point-min))
   (let ((header (dired-utils-get-filename))
         (next (progn (dired-hacks-next-file) (dired-utils-get-filename 'no-dir)))
         (all (dhu448-test-names)))
     (dired-utils-goto-line (expand-file-name "sub" root))
     (let ((dirp (and (dired-utils-is-dir-p) t))
           (info (dired-utils-get-info :name :isdir :issym)))
       (dired-utils-goto-line (expand-file-name "alias.txt" root))
       (list :header header
             :first next
             :all all
             :dir (list :is-dir dirp
                        :name (dhu448-test-mask
                               (file-relative-name (nth 0 info) root))
                        :isdir (nth 1 info)
                        :issym (nth 2 info))
             :link (list :name (dired-utils-get-filename 'no-dir)
                         :issym (dired-utils-get-info :issym)
                         :target (dhu448-test-mask
                                  (file-relative-name
                                   (file-truename (dired-utils-get-info :target))
                                   root))))))))
"####,
        expect![[
            r#"OK (:source (:tree "09923383a9e2a8c796d3209c2cb503de53199e07" :manifest (("dired-hacks-utils-pkg.el" . "71947423e81fad4b190fb26b219083ce0f08f231ee6fadb070ec0dc1c120353e") ("dired-hacks-utils.el" . "9af0b8600a98c2faa004b328d2ef6a43cd44a4d939b6a80d6798570260a640c3")) :feature t :version "20240629.1906") :result (:header nil :first "alias.txt" :all ("alias.txt" "café.txt" "notes.org" "sub") :dir (:is-dir t :name "sub" :isdir t :issym nil) :link (:name "alias.txt" :issym t :target "café.txt")) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn matches_names_and_formats_information_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "matches_names_and_formats_information_line",
        r####"
(dhu448-test-run
 (lambda (root)
   (let* ((ext (dired-utils-match-filename-extension
                "notes.org" '(("el" . elisp) ("org" . org))))
          (re (dired-utils-match-filename-regexp
               "café.txt" '(("\\.org\\'" . org) ("café" . unicode))))
          (miss (dired-utils-match-filename-extension "café.txt" '(("org" . org))))
          formatted)
     (with-temp-buffer
       (insert "  /tmp/listing:\n  used 2048 available 1048576\n")
       (dired-utils-format-information-line)
       (setq formatted (buffer-substring-no-properties (point-min) (point-max))))
     (list :ext (list :key (car ext) :val (cdr ext))
           :re (list :key (car re) :val (cdr re))
           :miss miss
           :formatted formatted))))
"####,
        expect![[
            r#"OK (:source (:tree "09923383a9e2a8c796d3209c2cb503de53199e07" :manifest (("dired-hacks-utils-pkg.el" . "71947423e81fad4b190fb26b219083ce0f08f231ee6fadb070ec0dc1c120353e") ("dired-hacks-utils.el" . "9af0b8600a98c2faa004b328d2ef6a43cd44a4d939b6a80d6798570260a640c3")) :feature t :version "20240629.1906") :result (:ext (:key "org" :val org) :re (:key "café" :val unicode) :miss nil :formatted "  /tmp/listing:\n  used 2048 available 1048576\n") :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn next_file_wraps_and_non_file_line_is_silent() -> ParityBatchCase {
    ParityBatchCase::value(
        "next_file_wraps_and_non_file_line_is_silent",
        r####"
(dhu448-test-run
 (lambda (root)
   (dhu448-test-write root "a.txt" "a")
   (dhu448-test-write root "b.txt" "b")
   (dired root)
   (goto-char (point-min))
   (let ((header-name (dired-utils-get-filename 'no-dir))
         (header-filep (and (dired-utils-is-file-p) t)))
     (dired-hacks-next-file)
     (let ((first (dired-utils-get-filename 'no-dir)))
       (dired-hacks-next-file 50)
       (list :header-name header-name
             :header-filep header-filep
             :first first
             :last (dired-utils-get-filename 'no-dir)
             :prev (progn (dired-hacks-previous-file)
                          (dired-utils-get-filename 'no-dir)))))))
"####,
        expect![[
            r#"OK (:source (:tree "09923383a9e2a8c796d3209c2cb503de53199e07" :manifest (("dired-hacks-utils-pkg.el" . "71947423e81fad4b190fb26b219083ce0f08f231ee6fadb070ec0dc1c120353e") ("dired-hacks-utils.el" . "9af0b8600a98c2faa004b328d2ef6a43cd44a4d939b6a80d6798570260a640c3")) :feature t :version "20240629.1906") :result (:header-name nil :header-filep nil :first "a.txt" :last "b.txt" :prev "a.txt") :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn compare_files_uses_md5sum_and_reports_sameness() -> ParityBatchCase {
    ParityBatchCase::value(
        "compare_files_uses_md5sum_and_reports_sameness",
        r####"
(dhu448-test-run
 (lambda (root)
   (let ((same-a (dhu448-test-write root "same-a.txt" "payload"))
         (same-b (dhu448-test-write root "same-b.txt" "payload"))
         (other (dhu448-test-write root "other.txt" "other"))
         messages)
     (cl-letf (((symbol-function 'shell-command)
                (lambda (command &optional output-buffer &rest _)
                  (push (dhu448-test-mask command) dhu448-test-ledger)
                  (let ((file (car (last (split-string command)))))
                    (with-current-buffer (or output-buffer (current-buffer))
                      (insert
                       (format "%s  %s\n"
                               (if (string-match-p "other" file)
                                   "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                                 "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
                               file))))))
               ((symbol-function 'message)
                (lambda (fmt &rest args)
                  (let ((text (apply #'format fmt args)))
                    (push (dhu448-test-mask text) messages)
                    text))))
       (dired-hacks-compare-files same-a same-b)
       (let ((same (car messages)))
         (dired-hacks-compare-files same-a other)
         (list :commands (nreverse dhu448-test-ledger)
               :same same
               :different (car messages)))))))
"####,
        expect![[
            r#"OK (:source (:tree "09923383a9e2a8c796d3209c2cb503de53199e07" :manifest (("dired-hacks-utils-pkg.el" . "71947423e81fad4b190fb26b219083ce0f08f231ee6fadb070ec0dc1c120353e") ("dired-hacks-utils.el" . "9af0b8600a98c2faa004b328d2ef6a43cd44a4d939b6a80d6798570260a640c3")) :feature t :version "20240629.1906") :result (:commands ("md5sum [SANDBOX]/same-a.txt" "md5sum [SANDBOX]/same-b.txt" "md5sum [SANDBOX]/same-a.txt" "md5sum [SANDBOX]/other.txt") :same "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  [SANDBOX]/same-a.txt\naaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  [SANDBOX]/same-b.txt\nFiles are probably the same." :different "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  [SANDBOX]/same-a.txt\nbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  [SANDBOX]/other.txt\nFiles are different.") :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn dired_hacks_utils_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        lists_files_skips_headers_and_queries_info(),
        matches_names_and_formats_information_line(),
        next_file_wraps_and_non_file_line_is_silent(),
        compare_files_uses_md5sum_and_reports_sameness(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "dired-hacks-utils-rank448",
        "dired_hacks_utils_parity",
        &cases,
    );
}

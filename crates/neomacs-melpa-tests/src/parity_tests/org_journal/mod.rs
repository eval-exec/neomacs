//! Practical parity for org-journal daily files, search, and navigation.
//!
//! These cases write a dated journal under a sandbox directory, open
//! today's file, search planted café notes, and walk next/previous
//! days without encrypting or carrying TODOs over.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, ORG_JOURNAL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'org-journal)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")

(defconst oj490-test-tree
  "ace116319cabb66aea4fb1188e74c5e678496a94")
(defconst oj490-test-manifest
  '(("org-journal-pkg.el" . "9d170b173a079da983f3af10030a853fb6edc02b547ea0e284e8e561e03644ce")
    ("org-journal.el" . "8ad8d1be667371cfb64e4f037f0948eb5dd79345a498e26cc08189896f3f8491")))

(defun oj490-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun oj490-test-source-state ()
  (let* ((located (locate-library "org-journal.el"))
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
                         (cons file (oj490-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/org-journal.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car oj490-test-manifest)))
      (error "Unexpected installed org-journal payload: %S"
             (or manifest files)))
    (dolist (entry oj490-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (oj490-test-sha file) expected))
          (error "Unexpected installed org-journal source: %S"
                 (cons entry manifest)))))
    (list :tree oj490-test-tree
          :manifest manifest
          :feature (featurep 'org-journal)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'org-journal package-alist)))))))

(defun oj490-test-root (name)
  (file-name-as-directory
   (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defun oj490-test-write (path text)
  (make-directory (file-name-directory path) t)
  (write-region text nil path nil 'silent)
  path)

(defun oj490-test-rel (root path)
  (and path (file-relative-name (file-truename path) (file-truename root))))

(defun oj490-test-cleanup (root)
  (dolist (buf (buffer-list))
    (let ((name (buffer-file-name buf)))
      (when (and name
                 (string-prefix-p (file-truename root)
                                  (file-truename name)))
        (with-current-buffer buf
          (set-buffer-modified-p nil)
          (kill-buffer buf)))))
  (when (get-buffer org-journal--search-buffer)
    (kill-buffer org-journal--search-buffer))
  (org-journal-invalidate-cache)
  (when (file-exists-p root)
    (delete-directory root t)))

(defun oj490-test-time (year month day hour)
  (encode-time 0 0 hour day month year))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ORG_JOURNAL_MELPA_PIN, "org-journal.el")
        .expect("prepare pinned org-journal source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn new_entry_writes_header_and_cafe_time_heading() -> ParityBatchCase {
    ParityBatchCase::value(
        "new_entry_writes_header_and_cafe_time_heading",
        r####"
(let* ((root (oj490-test-root "oj-café-new"))
       (identity (current-buffer))
       (windows (current-window-configuration))
       (org-journal-dir root)
       (org-journal-file-type 'daily)
       (org-journal-file-format "%Y%m%d")
       (org-journal-date-format "%Y-%m-%d")
       (org-journal-date-prefix "* ")
       (org-journal-time-format "%H:%M ")
       (org-journal-time-prefix "** ")
       (org-journal-file-header "#+TITLE: Café journal\n")
       (org-journal-hide-entries-p nil)
       (org-journal-find-file-fn #'find-file)
       (org-journal-carryover-items "")
       (org-journal-encrypt-journal nil)
       (org-journal-enable-encryption nil)
       (org-journal-enable-cache nil)
       (org-journal-enable-agenda-integration nil)
       (org-agenda-inhibit-startup t)
       (inhibit-message t)
       (time (oj490-test-time 2026 1 15 9)))
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (make-directory root t)
        (org-journal-invalidate-cache)
        (org-journal-new-entry nil time)
        (insert "café notes")
        (save-buffer)
        (let ((file (buffer-file-name)))
          (list :source (oj490-test-source-state)
                :relative (oj490-test-rel root file)
                :mode major-mode
                :visual (and visual-line-mode t)
                :new (lookup-key org-journal-mode-map (kbd "C-c C-j"))
                :next (lookup-key org-journal-mode-map (kbd "C-c C-f"))
                :prev (lookup-key org-journal-mode-map (kbd "C-c C-b"))
                :search (lookup-key org-journal-mode-map (kbd "C-c C-s"))
                :buffer (buffer-substring-no-properties (point-min) (point-max))
                :file (with-temp-buffer
                        (insert-file-contents file)
                        (buffer-string)))))
    (oj490-test-cleanup root)
    (set-window-configuration windows)
    (when (buffer-live-p identity)
      (set-buffer identity))))
"####,
        expect![[
            r##"OK (:source (:tree "ace116319cabb66aea4fb1188e74c5e678496a94" :manifest (("org-journal-pkg.el" . "9d170b173a079da983f3af10030a853fb6edc02b547ea0e284e8e561e03644ce") ("org-journal.el" . "8ad8d1be667371cfb64e4f037f0948eb5dd79345a498e26cc08189896f3f8491")) :feature t :version "20260413.1401") :relative "20260115" :mode org-journal-mode :visual t :new org-journal-new-entry :next org-journal-next-entry :prev org-journal-previous-entry :search org-journal-search :buffer "#+TITLE: Café journal\n* 2026-01-15\n** café notes\n" :file "#+TITLE: Café journal\n* 2026-01-15\n** café notes\n")"##
        ]],
    )
}

fn prefix_new_entry_opens_date_without_time_heading() -> ParityBatchCase {
    ParityBatchCase::value(
        "prefix_new_entry_opens_date_without_time_heading",
        r####"
(let* ((root (oj490-test-root "oj-café-prefix"))
       (identity (current-buffer))
       (windows (current-window-configuration))
       (org-journal-dir root)
       (org-journal-file-type 'daily)
       (org-journal-file-format "%Y%m%d")
       (org-journal-date-format "%Y-%m-%d")
       (org-journal-date-prefix "* ")
       (org-journal-time-prefix "** ")
       (org-journal-file-header "#+TITLE: Café journal\n")
       (org-journal-hide-entries-p nil)
       (org-journal-find-file-fn #'find-file)
       (org-journal-carryover-items "")
       (org-journal-encrypt-journal nil)
       (org-journal-enable-encryption nil)
       (org-journal-enable-cache nil)
       (org-journal-enable-agenda-integration nil)
       (org-agenda-inhibit-startup t)
       (inhibit-message t)
       (time (oj490-test-time 2026 1 15 9)))
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (make-directory root t)
        (org-journal-invalidate-cache)
        (org-journal-new-entry t time)
        (save-buffer)
        (list :source (oj490-test-source-state)
              :relative (oj490-test-rel root (buffer-file-name))
              :mode major-mode
              :buffer (buffer-substring-no-properties (point-min) (point-max))))
    (oj490-test-cleanup root)
    (set-window-configuration windows)
    (when (buffer-live-p identity)
      (set-buffer identity))))
"####,
        expect![[
            r##"OK (:source (:tree "ace116319cabb66aea4fb1188e74c5e678496a94" :manifest (("org-journal-pkg.el" . "9d170b173a079da983f3af10030a853fb6edc02b547ea0e284e8e561e03644ce") ("org-journal.el" . "8ad8d1be667371cfb64e4f037f0948eb5dd79345a498e26cc08189896f3f8491")) :feature t :version "20260413.1401") :relative "20260115" :mode org-journal-mode :buffer "#+TITLE: Café journal\n* 2026-01-15\n")"##
        ]],
    )
}

fn open_current_visits_planted_today_or_reports_missing() -> ParityBatchCase {
    ParityBatchCase::value(
        "open_current_visits_planted_today_or_reports_missing",
        r####"
(let* ((root (oj490-test-root "oj-café-today"))
       (identity (current-buffer))
       (windows (current-window-configuration))
       (org-journal-dir root)
       (org-journal-file-type 'daily)
       (org-journal-file-format "%Y%m%d")
       (org-journal-find-file-fn #'find-file)
       (org-journal-encrypt-journal nil)
       (org-journal-enable-encryption nil)
       (org-journal-enable-cache nil)
       (org-journal-enable-agenda-integration nil)
       (org-agenda-inhibit-startup t)
       (inhibit-message t)
       (today (org-journal--get-entry-path))
       (body "* today\n** café planted\n"))
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (make-directory root t)
        (org-journal-invalidate-cache)
        (oj490-test-write today body)
        (org-journal-open-current-journal-file)
        (let ((opened
               (list :same (equal (file-truename (buffer-file-name))
                                  (file-truename today))
                     :mode major-mode
                     :buffer (buffer-substring-no-properties
                              (point-min) (point-max)))))
          (oj490-test-cleanup root)
          (make-directory root t)
          (org-journal-invalidate-cache)
          (let* ((missing-path (org-journal--get-entry-path))
                 (msg (org-journal-open-current-journal-file)))
            (list :source (oj490-test-source-state)
                  :opened opened
                  :missing-exists (file-exists-p missing-path)
                  :not-found
                  (and (stringp msg)
                       (string-match-p (regexp-quote "not found") msg)
                       (string-match-p (regexp-quote missing-path) msg)
                       t)))))
    (oj490-test-cleanup root)
    (set-window-configuration windows)
    (when (buffer-live-p identity)
      (set-buffer identity))))
"####,
        expect![[
            r#"OK (:source (:tree "ace116319cabb66aea4fb1188e74c5e678496a94" :manifest (("org-journal-pkg.el" . "9d170b173a079da983f3af10030a853fb6edc02b547ea0e284e8e561e03644ce") ("org-journal.el" . "8ad8d1be667371cfb64e4f037f0948eb5dd79345a498e26cc08189896f3f8491")) :feature t :version "20260413.1401") :opened (:same t :mode org-journal-mode :buffer "* today\n** café planted\n") :missing-exists nil :not-found t)"#
        ]],
    )
}

fn search_forever_lists_cafe_hits() -> ParityBatchCase {
    ParityBatchCase::value(
        "search_forever_lists_cafe_hits",
        r####"
(let* ((root (oj490-test-root "oj-café-search"))
       (identity (current-buffer))
       (windows (current-window-configuration))
       (org-journal-dir root)
       (org-journal-file-type 'daily)
       (org-journal-file-format "%Y%m%d")
       (org-journal-search-result-date-format "%Y-%m-%d")
       (org-journal-date-format "%Y-%m-%d")
       (org-journal-date-prefix "* ")
       (org-journal-find-file-fn #'find-file)
       (org-journal-encrypt-journal nil)
       (org-journal-enable-encryption nil)
       (org-journal-enable-cache nil)
       (org-journal-enable-agenda-integration nil)
       (org-agenda-inhibit-startup t)
       (inhibit-message t)
       (display-buffer-overriding-action '(display-buffer-same-window)))
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (make-directory root t)
        (org-journal-invalidate-cache)
        (oj490-test-write (expand-file-name "20260114" root)
                          "* 2026-01-14\n** café morning\n")
        (oj490-test-write (expand-file-name "20260116" root)
                          "* 2026-01-16\n** café evening\n")
        (oj490-test-write (expand-file-name "20260115" root)
                          "* 2026-01-15\n** other notes\n")
        (org-journal-search-forever "café")
        (list :source (oj490-test-source-state)
              :search-mode (with-current-buffer org-journal--search-buffer
                             major-mode)
              :search (with-current-buffer org-journal--search-buffer
                        (buffer-substring-no-properties (point-min) (point-max)))))
    (oj490-test-cleanup root)
    (set-window-configuration windows)
    (when (buffer-live-p identity)
      (set-buffer identity))))
"####,
        expect![[
            r#"OK (:source (:tree "ace116319cabb66aea4fb1188e74c5e678496a94" :manifest (("org-journal-pkg.el" . "9d170b173a079da983f3af10030a853fb6edc02b547ea0e284e8e561e03644ce") ("org-journal.el" . "8ad8d1be667371cfb64e4f037f0948eb5dd79345a498e26cc08189896f3f8491")) :feature t :version "20260413.1401") :search-mode org-journal-search-mode :search "Search results for \"café\" between 1970-12-31 and 2030-12-31: \n\n2026-01-14\11** café morning\n2026-01-16\11** café evening\n")"#
        ]],
    )
}

fn next_previous_walk_planted_days() -> ParityBatchCase {
    ParityBatchCase::value(
        "next_previous_walk_planted_days",
        r####"
(let* ((root (oj490-test-root "oj-café-walk"))
       (identity (current-buffer))
       (windows (current-window-configuration))
       (org-journal-dir root)
       (org-journal-file-type 'daily)
       (org-journal-file-format "%Y%m%d")
       (org-journal-find-file-fn #'find-file)
       (org-journal-encrypt-journal nil)
       (org-journal-enable-encryption nil)
       (org-journal-enable-cache nil)
       (org-journal-enable-agenda-integration nil)
       (org-agenda-inhibit-startup t)
       (inhibit-message t))
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (make-directory root t)
        (org-journal-invalidate-cache)
        (oj490-test-write (expand-file-name "20260113" root)
                          "* 2026-01-13\n** café a\n")
        (oj490-test-write (expand-file-name "20260114" root)
                          "* 2026-01-14\n** café b\n")
        (oj490-test-write (expand-file-name "20260115" root)
                          "* 2026-01-15\n** café c\n")
        (find-file (expand-file-name "20260114" root))
        (let ((start (list :relative (oj490-test-rel root (buffer-file-name))
                           :mode major-mode)))
          (org-journal-next-entry)
          (let ((after (oj490-test-rel root (buffer-file-name)))
                (after-msg (org-journal-next-entry)))
            (org-journal-previous-entry)
            (org-journal-previous-entry)
            (let ((before (oj490-test-rel root (buffer-file-name)))
                  (before-msg (org-journal-previous-entry)))
              (list :source (oj490-test-source-state)
                    :start start
                    :after after
                    :after-end (and (stringp after-msg)
                                    (string-match-p "No journal entry after"
                                                    after-msg)
                                    t)
                    :before before
                    :before-end (and (stringp before-msg)
                                     (string-match-p "No journal entry before"
                                                     before-msg)
                                     t))))))
    (oj490-test-cleanup root)
    (set-window-configuration windows)
    (when (buffer-live-p identity)
      (set-buffer identity))))
"####,
        expect![[
            r#"OK (:source (:tree "ace116319cabb66aea4fb1188e74c5e678496a94" :manifest (("org-journal-pkg.el" . "9d170b173a079da983f3af10030a853fb6edc02b547ea0e284e8e561e03644ce") ("org-journal.el" . "8ad8d1be667371cfb64e4f037f0948eb5dd79345a498e26cc08189896f3f8491")) :feature t :version "20260413.1401") :start (:relative "20260114" :mode org-journal-mode) :after "20260115" :after-end t :before "20260113" :before-end t)"#
        ]],
    )
}

fn missing_journal_dir_signals_when_creation_is_declined() -> ParityBatchCase {
    ParityBatchCase::value(
        "missing_journal_dir_signals_when_creation_is_declined",
        r####"
(let* ((root (oj490-test-root "oj-café-missing"))
       (identity (current-buffer))
       (windows (current-window-configuration))
       (org-journal-dir root)
       (org-journal-file-type 'daily)
       (org-journal-file-format "%Y%m%d")
       (org-journal-find-file-fn #'find-file)
       (org-journal-encrypt-journal nil)
       (org-journal-enable-encryption nil)
       (org-journal-enable-cache nil)
       (org-journal-carryover-items "")
       (org-agenda-inhibit-startup t)
       (inhibit-message t)
       (time (oj490-test-time 2026 1 15 9)))
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (org-journal-invalidate-cache)
        (list :source (oj490-test-source-state)
              :exists (file-exists-p root)
              :declined
              (cl-letf (((symbol-function 'yes-or-no-p) (lambda (&rest _) nil)))
                (condition-case err
                    (org-journal-new-entry t time)
                  (error (list (car err)
                               (error-message-string err)))))))
    (oj490-test-cleanup root)
    (set-window-configuration windows)
    (when (buffer-live-p identity)
      (set-buffer identity))))
"####,
        expect![[
            r#"OK (:source (:tree "ace116319cabb66aea4fb1188e74c5e678496a94" :manifest (("org-journal-pkg.el" . "9d170b173a079da983f3af10030a853fb6edc02b547ea0e284e8e561e03644ce") ("org-journal.el" . "8ad8d1be667371cfb64e4f037f0948eb5dd79345a498e26cc08189896f3f8491")) :feature t :version "20260413.1401") :exists nil :declined (user-error "A journal directory is necessary to use org-journal"))"#
        ]],
    )
}

#[test]
fn org_journal_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        new_entry_writes_header_and_cafe_time_heading(),
        prefix_new_entry_opens_date_without_time_heading(),
        open_current_visits_planted_today_or_reports_missing(),
        search_forever_lists_cafe_hits(),
        next_previous_walk_planted_days(),
        missing_journal_dir_signals_when_creation_is_declined(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "org-journal-rank490",
        "org_journal_parity",
        &cases,
    );
}

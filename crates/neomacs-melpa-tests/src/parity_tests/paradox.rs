use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HYDRA_MELPA_PIN, PARADOX_MELPA_PIN, SPINNER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PARADOX_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const PARADOX_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'package)
(require 'seq)
(require 'subr-x)
(require 'paradox)

(defun neomacs-paradox-test-desc
    (name version status summary &optional url archive)
  "Create a package descriptor and pair it with STATUS."
  (cons
   (package-desc-create
    :name name
    :version (version-to-list version)
    :summary summary
    :reqs nil
    :kind 'single
    :archive (or archive "melpa")
    :extras (and url (list (cons :url url))))
   status))

(defun neomacs-paradox-test-entry-snapshot (entry)
  "Return externally visible data and stable properties from ENTRY."
  (let ((cells (append (cadr entry) nil)))
    (list
     :id (package-desc-name (car entry))
     :cells (mapcar #'substring-no-properties cells)
     :faces
     (mapcar
      (lambda (cell)
        (let (runs)
          (dolist (range (object-intervals cell))
            (when-let ((face (plist-get (nth 2 range) 'font-lock-face)))
              (push (list (car range) (cadr range) face) runs)))
          (nreverse runs)))
      cells)
     :stars
     (mapcar
      #'substring-no-properties
      (cdr (assq :stars (package-desc-extras (car entry))))))))

(defun neomacs-paradox-test-button-snapshot (cell)
  "Insert CELL and return its package and homepage button contract."
  (with-temp-buffer
    (insert cell)
    (let (buttons)
      (goto-char (point-min))
      (while-let ((button (next-button (point) t)))
        (push
         (list :text (button-label button)
               :type (button-type button)
               :help (button-get button 'help-echo)
               :package
               (when-let ((desc (button-get button 'package-desc)))
                 (package-desc-name desc)))
         buttons)
        (goto-char (button-end button)))
      (nreverse buttons))))

(defun neomacs-paradox-test-capture-signal (function)
  "Return stable signal data from FUNCTION, or its value."
  (condition-case error-data
      (list :value (funcall function))
    (error
     (list :signal (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))

(defun neomacs-paradox-test-visible-menu ()
  "Return the visible package rows and stable menu presentation state."
  (with-current-buffer "*Packages*"
    (let (rows)
      (goto-char (point-min))
      (while (not (eobp))
        (when-let ((desc (tabulated-list-get-id)))
          (push
           (list (package-desc-name desc)
                 (package-desc-status desc)
                 (char-after (line-beginning-position)))
           rows))
        (forward-line 1))
      (list :rows (nreverse rows)
            :filter paradox--current-filter
            :sort tabulated-list-sort-key
            :header
            (mapcar #'car (append tabulated-list-format nil))
            :buffer
            (buffer-substring-no-properties (point-min) (point-max))))))

(defvar neomacs-paradox-test-last-transaction nil)

(defun neomacs-paradox-test-record-transaction (alist)
  "Record the public Paradox post-transaction hook payload ALIST."
  (setq neomacs-paradox-test-last-transaction alist))

(defun neomacs-paradox-test-write (file contents)
  "Write CONTENTS to FILE inside the test sandbox."
  (make-directory (file-name-directory file) t)
  (with-temp-file file
    (insert contents))
  file)

(defun neomacs-paradox-test-package-source (name version body)
  "Return a complete single-file package NAME at VERSION with BODY."
  (format
   ";;; %s.el --- Local Paradox transaction fixture -*- lexical-binding: t; -*-\n\n;; Package-Version: %s\n;; Package-Requires: ((emacs \"24.4\"))\n\n;;; Code:\n%s\n(provide '%s)\n;;; %s.el ends here\n"
   name version body name name))

(defun neomacs-paradox-test-configure-local-archive ()
  "Create and select a deterministic local package world."
  (let* ((root
          (file-name-as-directory
           (expand-file-name
            "paradox-transaction"
            (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (archive (expand-file-name "archive/" root)))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory archive t)
    (setq package-user-dir (expand-file-name "elpa/" root)
          package-archives `(("fixture" . ,archive))
          package-check-signature nil
          package-unsigned-archives '("fixture")
          package-alist nil
          package-archive-contents nil
          package-activated-list nil
          package-selected-packages nil
          neomacs-paradox-test-last-transaction nil)
    (neomacs-paradox-test-write
     (expand-file-name "release-console-2.0.0.el" archive)
     (neomacs-paradox-test-package-source
      "release-console" "2.0.0"
      "(defconst release-console-generation 'current)"))
    (with-temp-file (expand-file-name "archive-contents" archive)
      (prin1
       '(1
         (release-console
          . [(2 0 0) ((emacs (24 4)))
             "Local fixture release-console" single nil]))
       (current-buffer)))
    (package-refresh-contents)
    (let ((legacy-source (expand-file-name "legacy-deployer.el" root)))
      (neomacs-paradox-test-write
       legacy-source
       (neomacs-paradox-test-package-source
        "legacy-deployer" "1.0.0"
        "(defconst legacy-deployer-generation 'retired)"))
      (package-install-file legacy-source))
    root))

(defun neomacs-paradox-test-goto-package (name)
  "Move point to package NAME in the active package menu."
  (goto-char (point-min))
  (while (and (not (eobp))
              (let ((desc (tabulated-list-get-id)))
                (not (and desc (eq (package-desc-name desc) name)))))
    (forward-line 1))
  (unless (and (not (eobp)) (tabulated-list-get-id))
    (error "Package %s is absent from the menu" name)))

(defun neomacs-paradox-test-transaction-summary (alist)
  "Return stable package identities from transaction hook ALIST."
  (mapcar
   (lambda (key)
     (cons
     key
      (if (eq key 'error)
          (mapcar #'error-message-string (alist-get key alist))
        (mapcar
         (lambda (desc)
           (list (package-desc-name desc)
                 (package-version-join (package-desc-version desc))))
         (alist-get key alist)))))
   '(installed deleted activated error)))

(defun neomacs-paradox-test-reset ()
  "Restore editor state changed by a Paradox parity case."
  (ignore-errors (paradox-disable))
  (dolist (name '("*Packages*" "*Paradox Report*"
                  "*Package Commit List*" "*Paradox Github*"))
    (when-let ((buffer (get-buffer name)))
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (setq paradox--backups nil
        paradox--current-filter nil
        paradox--upgradeable-packages nil
        paradox--upgradeable-packages-number nil
        paradox--upgradeable-packages-any? nil
        paradox--star-count (make-hash-table)
        paradox--download-count (make-hash-table)
        paradox--package-repo-list (make-hash-table)
        paradox--user-starred-repos (make-hash-table)
        package-alist nil
        package-archive-contents nil
        package-selected-packages nil))

(defun neomacs-paradox-test-with-reset (function)
  "Run FUNCTION without leaking Paradox state to another workflow."
  (neomacs-paradox-test-reset)
  (unwind-protect
      (funcall function)
    (neomacs-paradox-test-reset)))
"###;

fn paradox_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PARADOX_MELPA_PIN, "paradox.el")
        .expect("prepare revision-pinned Paradox source below ./tmp")
        .with_melpa_dependency(HYDRA_MELPA_PIN)
        .expect("prepare revision-pinned Hydra dependency below ./tmp")
        .with_melpa_dependency(SPINNER_MELPA_PIN)
        .expect("prepare revision-pinned Spinner dependency below ./tmp")
        .with_prelude(PARADOX_TEST_PRELUDE)
        .with_timeout(PARADOX_TEST_TIMEOUT)
}

fn menu_entries_render_real_catalog_metadata_buttons_and_counts() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-paradox-test-with-reset
 (lambda ()
   (let* ((paradox-display-star-count t)
          (paradox-display-download-count t)
          (paradox-use-homepage-buttons t)
          (package-archives '(("gnu" . "gnu/") ("melpa" . "melpa/")))
          (alpha
           (neomacs-paradox-test-desc
            'release-console "2.4.1" "available"
            "Operate staged releases"
            "https://example.test/release-console" "melpa"))
          (beta
           (neomacs-paradox-test-desc
            'audit-log "1.9.0" "installed"
            "Review immutable deployment evidence" nil "gnu")))
     (puthash 'release-console 1287 paradox--star-count)
     (puthash 'release-console 4821 paradox--download-count)
     (puthash 'release-console "ops/release-console"
              paradox--package-repo-list)
     (puthash "ops/release-console" t paradox--user-starred-repos)
     (puthash 'audit-log 37 paradox--star-count)
     (puthash 'audit-log 912 paradox--download-count)
     (let* ((alpha-entry (paradox--print-info alpha))
            (beta-entry (paradox--print-info beta)))
       (list
        :alpha (neomacs-paradox-test-entry-snapshot alpha-entry)
        :alpha-buttons
        (neomacs-paradox-test-button-snapshot
         (aref (cadr alpha-entry) 0))
        :beta (neomacs-paradox-test-entry-snapshot beta-entry)
        :counts
        (mapcar
         (lambda (status)
           (cons status (cdr (assoc-string status paradox--package-count))))
         '("total" "available" "installed")))))))
"###;
    let expected = expect![[
        r###"OK (:alpha (:id release-console :cells ("release-console  h" "2.4.1" "available" "melpa" "1287" "4K" " Operate staged releases ") :faces (((0 15 paradox-name-face) (17 18 paradox-homepage-button-face)) ((0 5 default)) ((0 9 default)) ((0 5 paradox-archive-face)) ((0 4 paradox-star-face)) ((0 2 paradox-download-face)) ((0 1 paradox-description-face) (1 24 paradox-description-face) (24 25 paradox-description-face))) :stars ("1287" "4K")) :alpha-buttons ((:text "release-console" :type paradox-name :help "Package: release-console" :package release-console) (:text "h" :type paradox-homepage :help "Visit https://example.test/release-console" :package nil)) :beta (:id audit-log :cells ("audit-log" "1.9.0" "installed" "gnu" "37" "912" " Review immutable deployment evidence ") :faces (((0 9 paradox-name-face)) ((0 5 font-lock-comment-face)) ((0 9 font-lock-comment-face)) ((0 3 paradox-archive-face)) ((0 2 paradox-star-face)) ((0 3 paradox-download-face)) ((0 1 paradox-description-face) (1 37 paradox-description-face) (37 38 paradox-description-face))) :stars ("37" "912")) :counts (("total" . 2) ("available" . 1) ("installed" . 1)))"###
    ]];
    ParityBatchCase::value(
        "menu_entries_render_real_catalog_metadata_buttons_and_counts",
        elisp_form,
        expected,
    )
}

fn enable_disable_lifecycle_is_idempotent_and_restores_gnu_package_menu() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-paradox-test-with-reset
 (lambda ()
   (let ((original-generate (symbol-function 'package-menu--generate))
         (original-mode (symbol-function 'package-menu-mode))
         first-backups)
     (paradox-enable)
     (setq first-backups (copy-sequence paradox--backups))
     (paradox-enable)
     (let ((enabled
            (list
             :generate-overridden
             (and (advice-member-p :paradox-override 'package-menu--generate) t)
             :mode-overridden
             (and (advice-member-p :paradox-override 'package-menu-mode) t)
             :truncate-overridden
             (and (advice-member-p :paradox-override
                                   'truncate-string-to-width)
                  t)
             :backup-count (length paradox--backups)
             :same-backups (equal first-backups paradox--backups))))
       (with-temp-buffer
         (package-menu-mode)
         (setq enabled
               (append enabled
                       (list :actual-mode major-mode
                             :derived-package-menu
                             (derived-mode-p 'package-menu-mode)
                             :keys
                             (mapcar
                              (lambda (key) (cons key (key-binding (kbd key))))
                              '("n" "p" "s" "v" "w" "x"))))))
       (paradox-disable)
       (list
        :enabled enabled
        :restored
        (list :generate (eq original-generate
                            (symbol-function 'package-menu--generate))
              :mode (eq original-mode (symbol-function 'package-menu-mode))
              :truncate
              (not (advice-member-p :paradox-override
                                    'truncate-string-to-width))
              :backups paradox--backups))))))
"###;
    let expected = expect![[
        r###"OK (:enabled (:generate-overridden t :mode-overridden t :truncate-overridden t :backup-count 5 :same-backups t :actual-mode paradox-menu-mode :derived-package-menu package-menu-mode :keys (("n" . paradox-next-entry) ("p" . paradox-previous-entry) ("s" . paradox-menu-mark-star-unstar) ("v" . paradox-menu-visit-homepage) ("w" . paradox-menu-copy-homepage-as-kill) ("x" . paradox-menu-execute))) :restored (:generate t :mode t :truncate t :backups nil))"###
    ]];
    ParityBatchCase::value(
        "enable_disable_lifecycle_is_idempotent_and_restores_gnu_package_menu",
        elisp_form,
        expected,
    )
}

fn homepage_workflow_visits_copies_and_reports_missing_catalog_links() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-paradox-test-with-reset
 (lambda ()
   (let* ((release-console
           (car
            (neomacs-paradox-test-desc
             'release-console "2.4.1" "available"
             "Operate staged releases"
             "https://example.test/release-console")))
          (repo-fallback
           (car
            (neomacs-paradox-test-desc
             'repo-fallback "1.0" "available"
             "Use repository metadata")))
          (missing-release
           (car
            (neomacs-paradox-test-desc
             'missing-release "0.1" "available"
             "Package without a homepage")))
          (package-archive-contents
           `((release-console ,release-console)
             (repo-fallback ,repo-fallback)
             (missing-release ,missing-release)))
          (kill-ring nil)
          (kill-ring-yank-pointer nil)
          (visited nil)
          missing-message)
     (puthash 'repo-fallback "ops/repo-fallback"
              paradox--package-repo-list)
     (cl-letf (((symbol-function 'browse-url)
                (lambda (url &rest _)
                  (push url visited)
                  :visited)))
       (paradox-menu-visit-homepage 'release-console)
       (paradox-menu-visit-homepage 'repo-fallback)
       (paradox-menu-copy-homepage-as-kill 'release-console)
       (cl-letf (((symbol-function 'message)
                  (lambda (format &rest args)
                    (setq missing-message
                          (substring-no-properties
                           (apply #'format format args))))))
         (paradox-menu-copy-homepage-as-kill 'missing-release)))
     (list :visited (nreverse visited)
           :kill-ring kill-ring
           :missing missing-message
           :direct (paradox--package-homepage 'release-console)
           :fallback (paradox--package-homepage 'repo-fallback)))))
"###;
    let expected = expect![[
        r###"OK (:visited ("https://example.test/release-console" "https://github.com/ops/repo-fallback") :kill-ring ("https://example.test/release-console") :missing "Package missing-release has no homepage." :direct "https://example.test/release-console" :fallback "https://github.com/ops/repo-fallback")"###
    ]];
    ParityBatchCase::value(
        "homepage_workflow_visits_copies_and_reports_missing_catalog_links",
        elisp_form,
        expected,
    )
}

fn catalog_sort_filter_and_multiline_navigation_drive_a_real_package_menu() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-paradox-test-with-reset
 (lambda ()
   (let* ((paradox-display-star-count t)
          (paradox-display-download-count nil)
          (paradox-lines-per-entry 2)
          (package-archives '(("melpa" . "melpa/")))
          (release
           (car
            (neomacs-paradox-test-desc
             'release-console "2.4.1" "available"
             "Operate staged releases"
             "https://example.test/release-console")))
          (audit
           (car
            (neomacs-paradox-test-desc
             'audit-log "1.9.0" "available"
             "Review immutable deployment evidence")))
          (traces
           (car
            (neomacs-paradox-test-desc
             'trace-viewer "3.0.0" "available"
             "Explore service traces")))
          (package-archive-contents
           `((release-console ,release)
             (audit-log ,audit)
             (trace-viewer ,traces)))
          (repo "ops/release-console"))
     (puthash 'release-console 90 paradox--star-count)
     (puthash 'audit-log 240 paradox--star-count)
     (puthash 'trace-viewer 12 paradox--star-count)
     (puthash 'release-console repo paradox--package-repo-list)
     (puthash repo t paradox--user-starred-repos)
     (paradox-enable)
     (package-show-package-list
      '(release-console audit-log trace-viewer))
     (let ((initial (neomacs-paradox-test-visible-menu)))
       (with-current-buffer "*Packages*"
         (paradox-sort-by-package nil))
       (let ((sorted (neomacs-paradox-test-visible-menu)))
         (with-current-buffer "*Packages*"
           (goto-char (point-min))
           (let ((first (package-desc-name (tabulated-list-get-id))))
             (paradox-next-entry 2)
             (setq first
                   (list first
                         (package-desc-name (tabulated-list-get-id))
                         (line-number-at-pos)))
             (paradox-filter-regexp "release\\|audit")
             (let ((regexp-filter (neomacs-paradox-test-visible-menu)))
               (paradox-filter-stars)
               (list :initial initial
                     :sorted sorted
                     :navigation first
                     :regexp regexp-filter
                     :starred (neomacs-paradox-test-visible-menu))))))))))
"###;
    let expected = expect![[
        r###"OK (:initial (:rows ((audit-log "available" 32) (release-console "available" 32) (trace-viewer "available" 32)) :filter nil :sort ("Status") :header ("Package" "Version" "Status" "★" "Description") :buffer "  audit-log          1.9.0     available   240  Review immutable deployment evidence \n  release-console  h 2.4.1     available    90  Operate staged releases \n  trace-viewer       3.0.0     available    12  Explore service traces \n") :sorted (:rows ((audit-log "available" 32) (release-console "available" 32) (trace-viewer "available" 32)) :filter nil :sort ("Package") :header ("Package" "Version" "Status" "★" "Description") :buffer "  audit-log          1.9.0     available   240  Review immutable deployment evidence \n  release-console  h 2.4.1     available    90  Operate staged releases \n  trace-viewer       3.0.0     available    12  Explore service traces \n") :navigation (audit-log trace-viewer 3) :regexp (:rows ((audit-log "available" 32) (release-console "available" 32)) :filter "Regexp:release\\|audit" :sort ("Status") :header ("Package" "Version" "Status" "★" "Description") :buffer "  audit-log          1.9.0     available   240  Review immutable deployment evidence \n  release-console  h 2.4.1     available    90  Operate staged releases \n") :starred (:rows ((release-console "available" 32)) :filter "Starred" :sort ("Status") :header ("Package" "Version" "Status" "★" "Description") :buffer "  release-console  h 2.4.1     available    90  Operate staged releases \n"))"###
    ]];
    ParityBatchCase::value(
        "catalog_sort_filter_and_multiline_navigation_drive_a_real_package_menu",
        elisp_form,
        expected,
    )
}

fn marked_local_install_and_delete_execute_as_one_real_package_transaction() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-paradox-test-with-reset
 (lambda ()
   (let* ((root (neomacs-paradox-test-configure-local-archive))
          (paradox-display-star-count nil)
          (paradox-display-download-count nil)
          (paradox-execute-asynchronously nil)
          (paradox-github-token t)
          (paradox-automatically-star nil)
          (paradox-after-execute-functions
           '(neomacs-paradox-test-record-transaction
             paradox--report-buffer-print)))
     (unwind-protect
         (progn
           (paradox-enable)
           (package-show-package-list
            '(legacy-deployer release-console))
           (with-current-buffer "*Packages*"
             (neomacs-paradox-test-goto-package 'release-console)
             (package-menu-mark-install)
             (neomacs-paradox-test-goto-package 'legacy-deployer)
             (package-menu-mark-delete)
             (let ((marked
                    (buffer-substring-no-properties (point-min) (point-max))))
               (cl-letf (((symbol-function 'format-time-string)
                          (lambda (&rest _)
                            "Package transaction finished. 2026-07-04 09:30:00\n")))
                 (paradox-menu-execute 'noquery))
               (let* ((release-desc
                       (cadr (assq 'release-console package-alist)))
                      (release-file
                       (and release-desc
                            (expand-file-name
                             "release-console.el"
                             (package-desc-dir release-desc))))
                      (report
                       (with-current-buffer "*Paradox Report*"
                         (buffer-substring-no-properties
                          (point-min) (point-max)))))
                 (list
                  :marked marked
                  :transaction
                  (neomacs-paradox-test-transaction-summary
                   neomacs-paradox-test-last-transaction)
                  :flags
                  (list
                   (alist-get 'async neomacs-paradox-test-last-transaction)
                   (alist-get 'noquery neomacs-paradox-test-last-transaction))
                  :installed
                  (and release-desc
                       (package-version-join
                        (package-desc-version release-desc)))
                  :installed-source
                  (and release-file
                       (file-readable-p release-file)
                       (with-temp-buffer
                         (insert-file-contents release-file)
                         (and (search-forward
                               "release-console-generation 'current"
                               nil t)
                              t)))
                  :legacy-present (and (assq 'legacy-deployer package-alist) t)
                  :selected
                  (list
                   :release-console
                   (and (memq 'release-console package-selected-packages) t)
                   :legacy-deployer
                   (and (memq 'legacy-deployer package-selected-packages) t))
                  :report report)))))
       (when (file-exists-p root)
         (delete-directory root t))))))
"###;
    let expected = expect![[
        r###"OK (:marked "I release-console    2.0.0     available   Local fixture release-console \nD legacy-deployer    1.0.0     installed   Local Paradox transaction fixture \n" :transaction ((installed (release-console "2.0.0")) (deleted (legacy-deployer "1.0.0")) (activated (release-console "2.0.0")) (error)) :flags (nil noquery) :installed "2.0.0" :installed-source t :legacy-present nil :selected (:release-console t :legacy-deployer nil) :report "\n\f\nPackage transaction finished. 2026-07-04 09:30:00\nInstalled:\n  release-console  2.0.0\n\nDeleted:\n  legacy-deployer  1.0.0\n\n")"###
    ]];
    ParityBatchCase::value(
        "marked_local_install_and_delete_execute_as_one_real_package_transaction",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn github_star_workflow_updates_remote_intent_local_state_and_menu_count() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-paradox-test-with-reset
 (lambda ()
   (let ((paradox-github-token "fixture-token")
         (repo "ops/release-console")
         (requests nil)
         (messages nil))
     (puthash 'release-console repo
              paradox--package-repo-list)
     (puthash 'release-console 41 paradox--star-count)
     (cl-letf (((symbol-function 'paradox--github-action-star)
                (lambda (repo &optional delete)
                  (push (list repo delete) requests)))
               ((symbol-function 'message)
                (lambda (format &rest args)
                  (push (apply #'format format args) messages))))
       (paradox--star-repo repo)
       (let ((starred
              (list :state
                    (and (paradox--starred-repo-p repo)
                         t)
                    :display
                    (substring-no-properties
                     (paradox--package-star-count 'release-console))
                    :face
                    (get-text-property
                     0 'font-lock-face
                     (paradox--package-star-count 'release-console)))))
         (paradox--star-repo repo t)
         (list :starred starred
               :unstarred
               (paradox--starred-repo-p repo)
               :requests (nreverse requests)
               :messages (nreverse messages)
               :safe-missing
               (paradox--star-package-safe 'not-on-github)))))))
"###;
    let expected = expect![[
        r###"OK (:starred (:state t :display "41" :face paradox-starred-face) :unstarred nil :requests (("ops/release-console" nil) ("ops/release-console" t)) :messages ("Starred ops/release-console." "Unstarred ops/release-console.") :safe-missing nil)"###
    ]];
    ParityBatchCase::value(
        "github_star_workflow_updates_remote_intent_local_state_and_menu_count",
        elisp_form,
        expected,
    )
}

fn github_response_parser_handles_success_missing_auth_and_malformed_responses() -> ParityBatchCase
{
    let elisp_form = r###"
(neomacs-paradox-test-with-reset
 (lambda ()
   (let ((paradox-github-token "expired-token"))
     (mapcar
      (lambda (response)
        (with-temp-buffer
          (insert response)
          (list
           :response (car (split-string response "\n"))
           :outcome
           (neomacs-paradox-test-capture-signal
            (lambda () (paradox--github-parse-response-code)))
           :point (point)
           :report
           (when-let ((report (get-buffer "*Paradox Report*")))
             (prog1
                 (with-current-buffer report
                   (buffer-substring-no-properties (point-min) (point-max)))
               (kill-buffer report))))))
      '("HTTP/1.1 200 OK\n\n[]"
        "HTTP/1.1 204 No Content\n\n"
        "HTTP/1.1 404 Not Found\n\nmissing"
        "HTTP/1.1 401 Unauthorized\n\ndenied"
        "not-http")))))
"###;
    let expected = expect![[
        r###"OK ((:response "HTTP/1.1 200 OK" :outcome (:value t) :point 10 :report nil) (:response "HTTP/1.1 204 No Content" :outcome (:value nil) :point 10 :report nil) (:response "HTTP/1.1 404 Not Found" :outcome (:value nil) :point 10 :report "HTTP/1.1 404 Not Found\n\nmissing") (:response "HTTP/1.1 401 Unauthorized" :outcome (:signal error :data ("Github says you’re not authenticated, try creating a new Github token  See *Paradox Github* buffer for the full result") :message "Github says you’re not authenticated, try creating a new Github token  See *Paradox Github* buffer for the full result") :point 10 :report "HTTP/1.1 401 Unauthorized\n\ndenied") (:response "not-http" :outcome (:signal error :data ("Tried contacting Github, but I can’t understand the result.  See *Paradox Github* buffer for the full result") :message "Tried contacting Github, but I can’t understand the result.  See *Paradox Github* buffer for the full result") :point 1 :report "not-http"))"###
    ]];
    ParityBatchCase::value(
        "github_response_parser_handles_success_missing_auth_and_malformed_responses",
        elisp_form,
        expected,
    )
}

fn commit_history_renders_tags_multiline_messages_navigation_and_visit() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-paradox-test-with-reset
 (lambda ()
   (let* ((paradox-date-format "%Y-%m-%d")
          (paradox--package-version "2.0.0")
          (paradox--package-tag-commit-alist
           '(("new-sha" . "v2.1.0") ("installed-sha" . "v2.0.0")))
          (feed
           '(((sha . "new-sha")
              (html_url . "https://example.test/commit/new")
              (commit
               (comment_count . 2)
               (committer (date . "2026-07-03T12:00:00Z"))
               (message . "Prepare release\nValidate checksums\nNotify operators")))
             ((sha . "installed-sha")
              (html_url . "https://example.test/commit/installed")
              (commit
               (comment_count . 0)
               (committer (date . "2026-06-02T12:00:00Z"))
               (message . "Release 2.0.0")))
             ((sha . "old-sha")
              (html_url . "https://example.test/commit/old")
              (commit
               (comment_count . 1)
               (committer (date . "2026-05-01T12:00:00Z"))
               (message . "Legacy migration\nKeep rollback notes")))))
          (paradox--commit-message-face nil)
          (entries (apply #'append
                          (mapcar #'paradox--commit-print-info feed)))
          visited)
     (with-temp-buffer
       (paradox-commit-list-mode)
       (setq tabulated-list-entries entries)
       (tabulated-list-print)
       (goto-char (point-min))
       (forward-line 1)
       (let ((first (cdr (assoc 'sha (tabulated-list-get-id)))))
         (paradox-next-commit 1)
         (let ((second (cdr (assoc 'sha (tabulated-list-get-id)))))
           (paradox-previous-commit 1)
           (cl-letf (((symbol-function 'browse-url)
                      (lambda (url &rest _) (setq visited url))))
             (paradox-commit-list-visit-commit))
           (list
            :entries
            (mapcar
             (lambda (entry)
               (list
                :sha (cdr (assoc 'sha (car entry)))
                :old (cdr (assoc 'is-old (car entry)))
                :cells
                (mapcar #'substring-no-properties
                        (append (cadr entry) nil))))
             entries)
            :buffer (buffer-substring-no-properties (point-min) (point-max))
            :navigation (list first second
                              (cdr (assoc 'sha (tabulated-list-get-id))))
            :visited visited)))))))
"###;
    let expected = expect![[
        r###"OK (:entries ((:sha "new-sha" :old t :cells ("2026-07-03" "(2 comments) v2.1.0 Prepare release")) (:sha "new-sha" :old nil :cells ("" "Validate checksums")) (:sha "new-sha" :old nil :cells ("" "Notify operators")) (:sha "installed-sha" :old t :cells ("2026-06-02" "v2.0.0 Release 2.0.0")) (:sha "old-sha" :old t :cells ("2026-05-01" "(1 comments) Legacy migration")) (:sha "old-sha" :old nil :cells ("" "Keep rollback notes"))) :buffer " 2026-07-03 (2 comments) v2.1.0 Prepare release\n            Validate checksums\n            Notify operators\n 2026-06-02 v2.0.0 Release 2.0.0\n 2026-05-01 (1 comments) Legacy migration\n            Keep rollback notes\n" :navigation ("new-sha" "installed-sha" "new-sha") :visited "https://example.test/commit/new")"###
    ]];
    ParityBatchCase::value(
        "commit_history_renders_tags_multiline_messages_navigation_and_visit",
        elisp_form,
        expected,
    )
}

fn transaction_report_records_installs_deletes_errors_and_read_only_output() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-paradox-test-with-reset
 (lambda ()
   (let* ((installed
           (car (neomacs-paradox-test-desc
                 'release-console "2.4.1" "installed"
                 "Operate staged releases")))
          (deleted
           (car (neomacs-paradox-test-desc
                 'legacy-deployer "0.8.0" "deleted"
                 "Retired deployment client")))
          (alist
           `((installed ,installed)
             (deleted ,deleted)
             (activated ,installed)
             (error (file-error "Signature rejected" "legacy.sig"))
             (async . nil)
             (noquery . t))))
     (cl-letf (((symbol-function 'format-time-string)
                (lambda (&rest _)
                  "Package transaction finished. 2026-07-03 12:00:00\n")))
       (paradox--report-buffer-print alist))
     (with-current-buffer "*Paradox Report*"
       (list :text (buffer-substring-no-properties (point-min) (point-max))
             :mode major-mode
             :read-only buffer-read-only
             :point (point)
             :format tabulated-list-format)))))
"###;
    let expected = expect![[
        r###"OK (:text "\n\f\nPackage transaction finished. 2026-07-03 12:00:00\nErrors:\n  (file-error Signature rejected legacy.sig)\n\n\nInstalled:\n  release-console  2.4.1\n\nDeleted:\n  legacy-deployer  0.8.0\n\n" :mode special-mode :read-only t :point 4 :format nil)"###
    ]];
    ParityBatchCase::value(
        "transaction_report_records_installs_deletes_errors_and_read_only_output",
        elisp_form,
        expected,
    )
}

#[test]
fn paradox_package_batch() {
    let cases = vec![
        menu_entries_render_real_catalog_metadata_buttons_and_counts(),
        enable_disable_lifecycle_is_idempotent_and_restores_gnu_package_menu(),
        homepage_workflow_visits_copies_and_reports_missing_catalog_links(),
        catalog_sort_filter_and_multiline_navigation_drive_a_real_package_menu(),
        marked_local_install_and_delete_execute_as_one_real_package_transaction(),
        github_star_workflow_updates_remote_intent_local_state_and_menu_count(),
        github_response_parser_handles_success_missing_auth_and_malformed_responses(),
        commit_history_renders_tags_multiline_messages_navigation_and_visit(),
        transaction_report_records_installs_deletes_errors_and_read_only_output(),
    ];
    assert_oracle_batch_cases(paradox_oracle(), "paradox-package-batch", "Paradox", &cases);
}

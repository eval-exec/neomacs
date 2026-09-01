use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, IVY_MELPA_PIN, IVY_RICH_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const IVY_RICH_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const IVY_RICH_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'ivy)
(require 'ivy-rich)

(setq make-backup-files nil create-lockfiles nil)

(defvar ivy-rich-test-root
  (file-name-as-directory
   (expand-file-name "ivy-rich" (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
(defvar ivy-rich-test-project-root nil)
(defvar ivy-rich-test-project-lookups 0)
(defvar ivy-rich-test-orders
  '(("INC-417" :owner "alice" :state "deploying-to-production")
    ("INC-418" :owner "bob" :state "queued")))

(defun ivy-rich-test-write (relative contents)
  (let ((path (expand-file-name relative ivy-rich-test-root)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert contents)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun ivy-rich-test-project-finder (directory)
  (setq ivy-rich-test-project-lookups (1+ ivy-rich-test-project-lookups))
  (when (and ivy-rich-test-project-root
             (file-in-directory-p directory ivy-rich-test-project-root))
    (cons 'transient ivy-rich-test-project-root)))

(defun ivy-rich-test-order-field (candidate field)
  (plist-get (cdr (assoc-string candidate ivy-rich-test-orders)) field))

(defun ivy-rich-test-order-name (candidate)
  candidate)

(defun ivy-rich-test-order-owner (candidate)
  (ivy-rich-test-order-field candidate :owner))

(defun ivy-rich-test-order-state (candidate)
  (ivy-rich-test-order-field candidate :state))

(defun ivy-rich-test-original-transformer (candidate)
  (concat "plain:" candidate))

(defun ivy-rich-test-describe-string (string)
  (let ((length (length string))
        (position 0)
        runs)
    (while (< position length)
      (let ((next (or (next-single-property-change position 'face string)
                      length)))
        (push (list position next (get-text-property position 'face string)) runs)
        (setq position next)))
    (list :text (substring-no-properties string)
          :faces (nreverse runs))))

(defun ivy-rich-test-bookmark-description (candidate)
  (let ((type (ivy-rich-bookmark-type candidate)))
    (list (substring-no-properties type)
          (get-text-property 0 'face type))))
"##;

fn ivy_rich_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(IVY_RICH_MELPA_PIN, "ivy-rich.el")
        .expect("prepare pinned Ivy Rich source below ./tmp")
        .with_melpa_dependency(IVY_MELPA_PIN)
        .expect("prepare pinned Ivy dependency below ./tmp")
        .with_prelude(IVY_RICH_TEST_PRELUDE)
        .with_timeout(IVY_RICH_TEST_TIMEOUT)
}

fn configured_order_dashboard_installs_updates_and_restores_ivy_transformer() -> ParityBatchCase {
    let elisp_form = r##"
(let ((saved-transformers ivy-rich-display-transformers-list)
      (saved-originals ivy-rich--original-display-transformers-list)
      (saved-registry ivy--display-transformers-alist)
      (ivy-rich-display-transformers-list
       '(ivy-rich-test-orders
         (:columns
          ((ivy-rich-test-order-name
            (:width 12 :face font-lock-function-name-face))
           (ivy-rich-test-order-owner (:width 8 :align right))
           (ivy-rich-test-order-state (:width 10 :face warning)))
          :delimiter " | "
          :predicate
          (lambda (candidate)
            (assoc-string candidate ivy-rich-test-orders)))))
      before changed passthrough installed restored)
  (unwind-protect
      (progn
        (when ivy-rich-mode (ivy-rich-mode -1))
        (setq ivy-rich--original-display-transformers-list nil)
        (ivy-set-display-transformer
         'ivy-rich-test-orders #'ivy-rich-test-original-transformer)
        (ivy-rich-mode 1)
        (let ((transformer
               (cdr (assq 'ivy-rich-test-orders
                          ivy--display-transformers-alist))))
          (setq installed (functionp transformer)
                before
                (ivy-rich-test-describe-string
                 (funcall transformer "INC-417"))
                passthrough (funcall transformer "INC-999")))
        (ivy-rich-modify-columns
         'ivy-rich-test-orders
         '((ivy-rich-test-order-name (:width 9))
           (ivy-rich-test-order-owner (:align left))))
        (setq changed
              (ivy-rich-test-describe-string
               (funcall
                (cdr (assq 'ivy-rich-test-orders
                           ivy--display-transformers-alist))
                "INC-417")))
        (ivy-rich-mode -1)
        (let ((restored-transformer
               (cdr (assq 'ivy-rich-test-orders
                          ivy--display-transformers-alist))))
          (setq restored
                (list :registered (functionp restored-transformer)
                      :result
                      (and restored-transformer
                           (funcall restored-transformer "INC-418"))))))
    (when ivy-rich-mode (ivy-rich-mode -1))
    (setq ivy-rich-display-transformers-list saved-transformers
          ivy-rich--original-display-transformers-list saved-originals
          ivy--display-transformers-alist saved-registry))
  (list :installed installed
        :before before
        :changed changed
        :passthrough passthrough
        :restored restored))
"##;
    let expect = expect![[
        r##"OK (:installed t :before (:text "INC-417      |    alice | deploying…" :faces ((0 12 font-lock-function-name-face) (12 26 nil) (26 36 warning))) :changed (:text "INC-417   | alice    | deploying…" :faces ((0 9 font-lock-function-name-face) (9 23 nil) (23 33 warning))) :passthrough "INC-999" :restored (:registered nil :result nil))"##
    ]];
    ParityBatchCase::value(
        "configured_order_dashboard_installs_updates_and_restores_ivy_transformer",
        elisp_form,
        expect,
    )
}

fn buffer_dashboard_combines_unsaved_file_metadata_and_live_process_state() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((project-root (file-name-as-directory
                      (expand-file-name "buffer-project" ivy-rich-test-root)))
       (source (expand-file-name "services/api/orders.el" project-root))
       (file-buffer (generate-new-buffer "orders.el<ivy-rich-test>"))
       (process-buffer (generate-new-buffer "deploy-worker<ivy-rich-test>"))
       process result)
  (unwind-protect
      (progn
        (ivy-rich-test-write "buffer-project/services/api/orders.el"
                             "(message \"deployed\")\n")
        (with-current-buffer file-buffer
          (setq buffer-file-name source
                default-directory (file-name-directory source))
          (emacs-lisp-mode)
          (insert (make-string 1501 ?x))
          (setq buffer-read-only t))
        (with-current-buffer process-buffer
          (setq default-directory project-root)
          (fundamental-mode)
          (insert "deployment queued\n")
          (setq buffer-read-only t))
        (setq process
              (make-pipe-process
               :name "ivy-rich-deploy-worker"
               :buffer process-buffer
               :noquery t))
        (let ((ivy-rich-test-project-root project-root)
              (project-find-functions '(ivy-rich-test-project-finder))
              (ivy-rich-path-style 'relative))
          (setq result
                (list
                 :file
                 (list
                  :indicators
                  (ivy-rich-switch-buffer-indicators
                   (buffer-name file-buffer))
                  :size
                  (ivy-rich-switch-buffer-size (buffer-name file-buffer))
                  :mode
                  (ivy-rich-switch-buffer-major-mode
                   (buffer-name file-buffer))
                  :project
                  (ivy-rich-switch-buffer-project (buffer-name file-buffer))
                  :path
                  (ivy-rich-switch-buffer-path (buffer-name file-buffer)))
                 :process
                 (list
                  :indicators
                  (ivy-rich-switch-buffer-indicators
                   (buffer-name process-buffer))
                  :size
                  (ivy-rich-switch-buffer-size (buffer-name process-buffer))
                  :mode
                  (ivy-rich-switch-buffer-major-mode
                   (buffer-name process-buffer)))))))
    (when (and process (process-live-p process))
      (delete-process process))
    (dolist (buffer (list file-buffer process-buffer))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer (setq buffer-read-only nil))
        (kill-buffer buffer)))
    (when (file-exists-p project-root)
      (delete-directory project-root t)))
  result)
"##;
    let expect = expect![[
        r##"OK (:file (:indicators "!*" :size "1.5k " :mode "Emacs Lisp" :project "buffer-project" :path "services/api/") :process (:indicators "!&" :size "18 " :mode "Fundamental"))"##
    ]];
    ParityBatchCase::value(
        "buffer_dashboard_combines_unsaved_file_metadata_and_live_process_state",
        elisp_form,
        expect,
    )
}

fn project_cache_reuses_roots_tracks_killed_buffers_and_formats_paths() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((project-root (file-name-as-directory
                      (expand-file-name "cache-project" ivy-rich-test-root)))
       (source (expand-file-name "services/payments/worker.el" project-root))
       (project-find-functions '(ivy-rich-test-project-finder))
       (ivy-rich-test-project-root project-root)
       (ivy-rich-test-project-lookups 0)
       (first (generate-new-buffer "payments-worker<ivy-rich-test>"))
       second result)
  (unwind-protect
      (progn
        (ivy-rich-test-write "cache-project/services/payments/worker.el"
                             "(provide 'payments-worker)\n")
        (with-current-buffer first
          (setq buffer-file-name source
                default-directory (file-name-directory source)))
        (ivy-rich-project-root-cache-mode 1)
        (let* ((root-one (ivy-rich-switch-buffer-root (buffer-name first)))
               (root-two (ivy-rich-switch-buffer-root (buffer-name first)))
               (relative (let ((ivy-rich-path-style 'relative))
                           (ivy-rich-switch-buffer-path (buffer-name first))))
               (absolute (let ((ivy-rich-path-style 'absolute))
                           (ivy-rich-switch-buffer-path (buffer-name first))))
               (after-two
                (list :same-root (equal root-one root-two)
                      :lookups ivy-rich-test-project-lookups
                      :cache-size (hash-table-count ivy-rich--project-root-cache)
                      :project (ivy-rich-switch-buffer-project
                                (buffer-name first))
                      :relative relative
                      :absolute
                      (file-relative-name absolute project-root))))
          (kill-buffer first)
          (setq second
                (generate-new-buffer "payments-worker-2<ivy-rich-test>"))
          (with-current-buffer second
            (setq buffer-file-name source
                  default-directory (file-name-directory source)))
          (ivy-rich-switch-buffer-root (buffer-name second))
          (setq result
                (list
                 :cached after-two
                 :after-kill-and-reopen
                 (list :lookups ivy-rich-test-project-lookups
                       :cache-size
                       (hash-table-count ivy-rich--project-root-cache))
                 :shortened
                 (ivy-rich-switch-buffer-shorten-path
                  "/srv/platform/services/payments/internal/worker.el" 27))))
        (ivy-rich-project-root-cache-mode -1)
        (setq result
              (append result
                      (list :disabled
                            (list
                             :cache-size
                             (hash-table-count ivy-rich--project-root-cache)
                             :hook-installed
                             (and (memq
                                   'ivy-rich-project-root-cache-kill-buffer-hook
                                   kill-buffer-hook)
                                  t))))))
    (when ivy-rich-project-root-cache-mode
      (ivy-rich-project-root-cache-mode -1))
    (dolist (buffer (list first second))
      (when (buffer-live-p buffer) (kill-buffer buffer)))
    (when (file-exists-p project-root)
      (delete-directory project-root t)))
  result)
"##;
    let expect = expect![[
        r##"OK (:cached (:same-root t :lookups 1 :cache-size 1 :project "cache-project" :relative "services/payments/" :absolute "services/payments/") :after-kill-and-reopen (:lookups 2 :cache-size 1) :shortened "/srv/…/internal/worker.el" :disabled (:cache-size 0 :hook-installed nil))"##
    ]];
    ParityBatchCase::value(
        "project_cache_reuses_roots_tracks_killed_buffers_and_formats_paths",
        elisp_form,
        expect,
    )
}

fn documentation_files_and_bookmarks_render_operational_context() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (file-name-as-directory
              (expand-file-name "metadata" ivy-rich-test-root)))
       (target (expand-file-name "runbooks/deploy.org" root))
       (link (expand-file-name "current-runbook" root))
       (missing (expand-file-name "runbooks/missing.org" root))
       (directory (file-name-directory target))
       (ivy--directory root)
       (process-environment (copy-sequence process-environment))
       result)
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (setq target
              (let ((ivy-rich-test-root root))
                (ivy-rich-test-write "runbooks/deploy.org"
                                     "* Production deployment\n")))
        (make-symbolic-link "runbooks/deploy.org" link)
        (set-file-times target (encode-time 0 34 12 2 1 2020 t))
        (setenv "TZ" "UTC")
        (set-time-zone-rule t)
        (defun ivy-rich-test-deploy-service (service)
          "Deploy SERVICE after validating its release manifest.\nReturns the deployment identifier."
          service)
        (defun ivy-rich-test-audit-deploy (&rest _arguments))
        (defvar ivy-rich-test-deploy-timeout 45
          "Seconds to wait before a deployment is rolled back.\nUsed by operators.")
        (advice-add 'ivy-rich-test-deploy-service
                    :before #'ivy-rich-test-audit-deploy)
        (let ((bookmark-alist
               `(("Production runbook" (filename . ,target))
                 ("Runbook directory" (filename . ,directory))
                 ("Retired runbook" (filename . ,missing))
                 ("Manual page" (handler . bookmark-jump-man)))))
          (setq result
                (list
                 :docs
                 (list
                  (ivy-rich-counsel-function-docstring
                   "ivy-rich-test-deploy-service")
                  (ivy-rich-counsel-variable-docstring
                   "ivy-rich-test-deploy-timeout"))
                 :file
                 (list
                  :symlink
                  (file-relative-name
                   (substring
                    (ivy-rich-counsel-find-file-truename
                     "current-runbook")
                    3)
                   root)
                  :modified
                  (ivy-rich-file-last-modified-time
                   "runbooks/deploy.org"))
                 :bookmarks
                 (mapcar
                  (lambda (name)
                    (list name
                          (ivy-rich-test-bookmark-description name)
                          (let ((info (ivy-rich-bookmark-info name)))
                            (cond
                             ((and info (file-name-absolute-p info))
                              (file-relative-name info root))
                             (t info)))))
                  '("Production runbook" "Runbook directory"
                    "Retired runbook" "Manual page"))))))
    (advice-remove 'ivy-rich-test-deploy-service
                   #'ivy-rich-test-audit-deploy)
    (when (file-exists-p root) (delete-directory root t)))
  result)
"##;
    let expect = expect![[
        r##"OK (:docs ("Deploy SERVICE after validating its release manifest." "Seconds to wait before a deployment is rolled back.") :file (:symlink "runbooks/deploy.org" :modified "2020-01-02 12:34:00") :bookmarks (("Production runbook" ("FILE    " success) "runbooks/deploy.org") ("Runbook directory" ("DIRED   " warning) "runbooks/") ("Retired runbook" ("NOTFOUND" error) "runbooks/missing.org") ("Manual page" ("MAN     " font-lock-keyword-face) nil)))"##
    ]];
    ParityBatchCase::value(
        "documentation_files_and_bookmarks_render_operational_context",
        elisp_form,
        expect,
    )
}

fn package_catalog_columns_describe_installable_release_and_missing_candidate() -> ParityBatchCase {
    let elisp_form = r##"
(require 'package)
(let* ((descriptor
        (package-desc-create
         :name 'deploy-tools
         :version '(2 4 1)
         :summary "Deploy and roll back production services"
         :kind 'single
         :archive "melpa"))
       (package-archive-contents `((deploy-tools ,descriptor))))
  (mapcar
   (lambda (candidate)
     (list candidate
           :version (ivy-rich-package-version candidate)
           :archive (ivy-rich-package-archive-summary candidate)
           :summary (ivy-rich-package-install-summary candidate)))
   '("deploy-tools" "missing-tools")))
"##;
    let expect = expect![[
        r##"OK (("deploy-tools" :version "2.4.1" :archive "melpa" :summary "Deploy and roll back production services") ("missing-tools" :version "" :archive "" :summary ""))"##
    ]];
    ParityBatchCase::value(
        "package_catalog_columns_describe_installable_release_and_missing_candidate",
        elisp_form,
        expect,
    )
}

#[test]
fn ivy_rich_package_batch() {
    let cases = vec![
        configured_order_dashboard_installs_updates_and_restores_ivy_transformer(),
        buffer_dashboard_combines_unsaved_file_metadata_and_live_process_state(),
        project_cache_reuses_roots_tracks_killed_buffers_and_formats_paths(),
        documentation_files_and_bookmarks_render_operational_context(),
        package_catalog_columns_describe_installable_release_and_missing_candidate(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Ivy Rich parity test");
    assert_oracle_batch_cases(ivy_rich_oracle(), test_name, "ivy_rich_parity", &cases);
}

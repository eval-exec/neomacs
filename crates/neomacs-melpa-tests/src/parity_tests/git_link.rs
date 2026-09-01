use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, GIT_LINK_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'git-link)

(defun neomacs-git-link-test-process (directory &rest arguments)
  "Run Git with ARGUMENTS in DIRECTORY and return trimmed stdout."
  (with-temp-buffer
    (let ((default-directory directory))
      (let ((status (apply #'process-file
                           "git" nil (current-buffer) nil arguments)))
        (unless (zerop status)
          (error "git %S failed (%s): %s"
                 arguments status (buffer-string)))
        (goto-char (point-max))
        (skip-chars-backward "\n\r")
        (buffer-substring-no-properties (point-min) (point))))))

(defun neomacs-git-link-test-write (filename content)
  "Write CONTENT to FILENAME, creating its parent directory."
  (make-directory (file-name-directory filename) t)
  (with-temp-file filename
    (insert content)))

(defun neomacs-git-link-test-repository (remote-url)
  "Create a deterministic repository whose origin is REMOTE-URL."
  (let* ((root (file-name-as-directory
                (make-temp-file "git-link-repository-" t)))
         (process-environment
          (append '("GIT_AUTHOR_NAME=Parity Author"
                    "GIT_AUTHOR_EMAIL=parity@example.test"
                    "GIT_AUTHOR_DATE=2001-02-03T04:05:06+0000"
                    "GIT_COMMITTER_NAME=Parity Committer"
                    "GIT_COMMITTER_EMAIL=committer@example.test"
                    "GIT_COMMITTER_DATE=2001-02-03T04:05:06+0000")
                  process-environment)))
    (neomacs-git-link-test-write
     (expand-file-name "docs/Release Notes.md" root)
     "# Release\nAlpha details\nBeta details\nGamma details\n")
    (neomacs-git-link-test-write
     (expand-file-name "src/λ parser.el" root)
     ";;; parser\n(defun parse-input (value)\n  (message \"%s\" value))\n")
    (neomacs-git-link-test-process root "init" "--quiet")
    (neomacs-git-link-test-process root "symbolic-ref" "HEAD" "refs/heads/main")
    (neomacs-git-link-test-process root "add" "docs" "src")
    (neomacs-git-link-test-process root "commit" "--quiet" "-m" "Initial fixture")
    (neomacs-git-link-test-process root "remote" "add" "origin" remote-url)
    root))

(defun neomacs-git-link-test-error (function)
  "Return FUNCTION's value or stable error details."
  (condition-case error-data
      (list :ok (funcall function))
    (error
     (list :error (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))

(defvar neomacs-git-link-test-opened-url nil)

(defun neomacs-git-link-test-open-url (url)
  "Record URL as the browser target."
  (setq neomacs-git-link-test-opened-url url))
"###;

fn package_defaults_and_provider_routing_cover_the_supported_hosting_surface() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'git-link package-alist))))
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :feature (and (featurep 'git-link) t))
   :defaults
   (list :remote git-link-default-remote
         :branch git-link-default-branch
         :browser git-link-open-in-browser
         :kill-ring git-link-add-to-kill-ring
         :commit git-link-use-commit
         :single-line git-link-use-single-line-number
         :ssh-config git-link-consider-ssh-config)
   :routing
   (mapcar
    (lambda (host)
      (list host
            (git-link--handler git-link-remote-alist host)
            (git-link--handler git-link-commit-remote-alist host)
            (git-link--handler git-link-homepage-remote-alist host)))
    '("github.com" "gitlab.example.test" "bitbucket.org"
      "codeberg.org" "forge.fedoraproject.org" "git.sr.ht"
      "git.savannah.gnu.org" "go.googlesource.com"
      "dev.azure.com" "sourcegraph.example.test"
      "git-codecommit.us-east-1.amazonaws.com"))))
"###;
    let expected = expect![[
        r#"OK (:package (:name git-link :version "20260723.2213" :requirements ((emacs (24 3))) :feature t) :defaults (:remote nil :branch nil :browser nil :kill-ring t :commit nil :single-line t :ssh-config nil) :routing (("github.com" git-link-github git-link-commit-github git-link-homepage-github) ("gitlab.example.test" git-link-gitlab git-link-commit-gitlab git-link-homepage-github) ("bitbucket.org" git-link-bitbucket git-link-commit-bitbucket git-link-homepage-github) ("codeberg.org" git-link-codeberg git-link-commit-codeberg git-link-homepage-codeberg) ("forge.fedoraproject.org" git-link-codeberg git-link-commit-codeberg git-link-homepage-codeberg) ("git.sr.ht" git-link-sourcehut git-link-commit-github git-link-homepage-github) ("git.savannah.gnu.org" git-link-savannah git-link-commit-savannah git-link-homepage-savannah) ("go.googlesource.com" git-link-googlesource git-link-commit-googlesource git-link-homepage-github) ("dev.azure.com" git-link-azure git-link-commit-azure git-link-homepage-github) ("sourcegraph.example.test" git-link-sourcegraph git-link-commit-sourcegraph git-link-homepage-github) ("git-codecommit.us-east-1.amazonaws.com" git-link-codecommit git-link-commit-codecommit git-link-homepage-codecommit)))"#
    ]];
    ParityBatchCase::value(
        "package_defaults_and_provider_routing_cover_the_supported_hosting_surface",
        elisp_form,
        expected,
    )
}

fn remote_parsing_normalizes_real_https_scp_azure_savannah_and_codecommit_forms() -> ParityBatchCase
{
    let elisp_form = r###"
(mapcar
 (lambda (remote)
   (cons remote (git-link--parse-remote remote)))
 '("https://github.com/acme/widgets.git"
   "git@github.com:CaseSensitive/Widget-Kit.git"
   "git+ssh://git@gitlab.example.test:2222/platform/service.git"
   "git@ssh.dev.azure.com:v3/acme/platform/widget-api"
   "acme@vs-ssh.visualstudio.com:v3/acme/platform/widget-api"
   "ssh://git.savannah.gnu.org/srv/git/emacs.git"
   "git://git.sv.gnu.org/emacs.git"
   "ssh://git-codecommit.us-east-1.amazonaws.com/v1/repos/WidgetApi"
   "https://go.googlesource.com/go"))
"###;
    let expected = expect![[
        r#"OK (("https://github.com/acme/widgets.git" "github.com" "acme/widgets") ("git@github.com:CaseSensitive/Widget-Kit.git" "github.com" "CaseSensitive/Widget-Kit") ("git+ssh://git@gitlab.example.test:2222/platform/service.git" "gitlab.example.test" "platform/service") ("git@ssh.dev.azure.com:v3/acme/platform/widget-api" "dev.azure.com" "acme/platform/_git/widget-api") ("acme@vs-ssh.visualstudio.com:v3/acme/platform/widget-api" "acme.visualstudio.com" "platform/_git/widget-api") ("ssh://git.savannah.gnu.org/srv/git/emacs.git" "git.savannah.gnu.org" "emacs") ("git://git.sv.gnu.org/emacs.git" "git.sv.gnu.org" "emacs") ("ssh://git-codecommit.us-east-1.amazonaws.com/v1/repos/WidgetApi" "us-east-1.console.aws.amazon.com" "codesuite/codecommit/repositories/WidgetApi") ("https://go.googlesource.com/go" "go.googlesource.com" "go"))"#
    ]];
    ParityBatchCase::value(
        "remote_parsing_normalizes_real_https_scp_azure_savannah_and_codecommit_forms",
        elisp_form,
        expected,
    )
}

fn github_markdown_region_link_runs_the_full_git_file_and_kill_ring_workflow() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (neomacs-git-link-test-repository
              "git@github.com:acme/widgets.git"))
       (filename (expand-file-name "docs/Release Notes.md" root))
       (buffer (find-file-noselect filename))
       (kill-ring nil)
       (kill-ring-yank-pointer nil)
       (interprogram-cut-function nil)
       (git-link-open-in-browser nil))
  (unwind-protect
      (with-current-buffer buffer
        (goto-char (point-min))
        (forward-line 1)
        (set-mark (point))
        (forward-line 2)
        (activate-mark)
        (let* ((region (git-link--get-region))
               (url (apply #'git-link (cons "origin" region))))
          (list :region region
                :url url
                :kill (car kill-ring)
                :deactivate-mark deactivate-mark
                :branch (git-link--branch)
                :relative (git-link--relative-filename))))
    (when (buffer-live-p buffer)
      (kill-buffer buffer))))
"###;
    let expected = expect![[
        r#"OK (:region (2 3) :url "https://github.com/acme/widgets/blob/main/docs/Release%20Notes.md?plain=1#L2-L3" :kill "https://github.com/acme/widgets/blob/main/docs/Release%20Notes.md?plain=1#L2-L3" :deactivate-mark t :branch "main" :relative "docs/Release Notes.md")"#
    ]];
    ParityBatchCase::value(
        "github_markdown_region_link_runs_the_full_git_file_and_kill_ring_workflow",
        elisp_form,
        expected,
    )
}

fn reverse_selection_in_a_narrowed_unicode_file_generates_a_commit_permalink() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (neomacs-git-link-test-repository
              "https://codeberg.org/acme/parser.git"))
       (filename (expand-file-name "src/λ parser.el" root))
       (buffer (find-file-noselect filename))
       (kill-ring nil)
       (interprogram-cut-function nil)
       (git-link-use-commit t))
  (unwind-protect
      (with-current-buffer buffer
        (goto-char (point-min))
        (forward-line 1)
        (let ((narrow-start (point)))
          (goto-char (point-max))
          (narrow-to-region narrow-start (point))
          (goto-char (point-max))
          (set-mark (point))
          (goto-char (point-min))
          (activate-mark)
          (let* ((region (git-link--get-region))
                 (commit (git-link--commit))
                 (url (apply #'git-link (cons "origin" region))))
            (list :region region
                  :commit commit
                  :url url
                  :kill (car kill-ring)
                  :restriction (list (point-min) (point-max))))))
    (when (buffer-live-p buffer)
      (kill-buffer buffer))))
"###;
    let expected = expect![[
        r#"OK (:region (2 3) :commit "786cfc0929a5e4a193ab22df31406032fd4f3763" :url "https://codeberg.org/acme/parser/src/commit/786cfc0929a5e4a193ab22df31406032fd4f3763/src/%CE%BB%20parser.el#L2-L3" :kill "https://codeberg.org/acme/parser/src/commit/786cfc0929a5e4a193ab22df31406032fd4f3763/src/%CE%BB%20parser.el#L2-L3" :restriction (12 63))"#
    ]];
    ParityBatchCase::value(
        "reverse_selection_in_a_narrowed_unicode_file_generates_a_commit_permalink",
        elisp_form,
        expected,
    )
}

fn enterprise_web_host_branch_override_and_browser_policy_are_honored_end_to_end() -> ParityBatchCase
{
    let elisp_form = r###"
(let* ((root (neomacs-git-link-test-repository
              "git@ssh.gitlab.corp.test:platform/widgets.git"))
       (filename (expand-file-name "src/λ parser.el" root))
       (buffer (find-file-noselect filename))
       (git-link-web-host-alist
        '(("ssh\\.gitlab\\.corp\\.test" . "http://code.corp.test")))
       (git-link-default-branch "release/v2 beta")
       (git-link-add-to-kill-ring nil)
       (kill-ring '("existing"))
       (neomacs-git-link-test-opened-url nil))
  (unwind-protect
      (with-current-buffer buffer
        (let ((git-link-open-in-browser 'neomacs-git-link-test-open-url))
          (list :url (git-link "origin" 2 nil)
                :opened neomacs-git-link-test-opened-url
                :kill kill-ring
                :web-host (git-link--web-host "ssh.gitlab.corp.test"))))
    (when (buffer-live-p buffer)
      (kill-buffer buffer))))
"###;
    let expected = expect![[
        r#"OK (:url "http://code.corp.test/platform/widgets/-/blob/release%2Fv2%20beta/src/%CE%BB%20parser.el#L2" :opened "http://code.corp.test/platform/widgets/-/blob/release%2Fv2%20beta/src/%CE%BB%20parser.el#L2" :kill ("existing") :web-host "http://code.corp.test")"#
    ]];
    ParityBatchCase::value(
        "enterprise_web_host_branch_override_and_browser_policy_are_honored_end_to_end",
        elisp_form,
        expected,
    )
}

fn one_review_selection_maps_to_each_supported_provider_url_convention() -> ParityBatchCase {
    let elisp_form = r###"
(let ((host-data
       '((github git-link-github "https://github.com" "acme/widgets")
         (gitlab git-link-gitlab "https://gitlab.com" "acme/widgets")
         (codeberg git-link-codeberg "https://codeberg.org" "acme/widgets")
         (bitbucket git-link-bitbucket "https://bitbucket.org" "acme/widgets")
         (sourcehut git-link-sourcehut "https://git.sr.ht" "~acme/widgets")
         (sourcegraph git-link-sourcegraph "https://sourcegraph.com" "acme/widgets")
         (savannah git-link-savannah "https://git.savannah.gnu.org" "widgets")
         (azure git-link-azure "https://dev.azure.com" "acme/platform/_git/widgets")
         (codecommit git-link-codecommit "https://us-east-1.console.aws.amazon.com"
                     "codesuite/codecommit/repositories/widgets"))))
  (mapcar
   (lambda (entry)
     (list (nth 0 entry)
           (funcall (nth 1 entry)
                    (nth 2 entry) (nth 3 entry)
                    "docs/Release Notes.md" "feature/review" "abc1234" 12 18)))
   host-data))
"###;
    let expected = expect![[
        r#"OK ((github "https://github.com/acme/widgets/blob/feature/review/docs/Release Notes.md?plain=1#L12-L18") (gitlab "https://gitlab.com/acme/widgets/-/blob/feature/review/docs/Release Notes.md#L12-18") (codeberg "https://codeberg.org/acme/widgets/src/feature/review/docs/Release Notes.md#L12-L18") (bitbucket "https://bitbucket.org/acme/widgets/annotate/abc1234/docs/Release Notes.md#Release Notes.md-12:18") (sourcehut "https://git.sr.ht/~acme/widgets/tree/feature/review/docs/Release Notes.md#L12-18") (sourcegraph "https://sourcegraph.com/acme/widgets@feature/review/-/blob/docs/Release Notes.md#L12-18") (savannah "https://git.savannah.gnu.org/cgit/widgets.git/tree/docs/Release Notes.md?h=feature/review#n12") (azure "https://dev.azure.com/acme/platform/_git/widgets?path=%2Fdocs/Release Notes.md&version=GBfeature/review&line=12&lineEnd=18&lineStartColumn=1&lineEndColumn=9999&lineStyle=plain") (codecommit "https://us-east-1.console.aws.amazon.com/codesuite/codecommit/repositories/widgets/browse/refs/heads/feature/review/--/docs/Release Notes.md?lines=12-18"))"#
    ]];
    ParityBatchCase::value(
        "one_review_selection_maps_to_each_supported_provider_url_convention",
        elisp_form,
        expected,
    )
}

fn commit_and_homepage_commands_use_the_repository_remote_and_copy_each_result() -> ParityBatchCase
{
    let elisp_form = r###"
(let* ((root (neomacs-git-link-test-repository
              "https://gitlab.com/acme/widgets.git"))
       (commit (neomacs-git-link-test-process root "rev-parse" "HEAD"))
       (kill-ring nil)
       (kill-ring-yank-pointer nil)
       (interprogram-cut-function nil)
       (default-directory root))
  (with-temp-buffer
    (setq default-directory root)
    (insert (concat "Reviewed commit " commit " for deployment"))
    (goto-char (+ (point-min) (length "Reviewed commit ") 8))
    (let ((commit-url (git-link-commit "origin"))
          (homepage-url (git-link-homepage "origin")))
      (list :commit commit
            :commit-url commit-url
            :homepage-url homepage-url
            :kill-ring kill-ring))))
"###;
    let expected = expect![[
        r#"OK (:commit "786cfc0929a5e4a193ab22df31406032fd4f3763" :commit-url "https://gitlab.com/acme/widgets/-/commit/786cfc0929a5e4a193ab22df31406032fd4f3763" :homepage-url "https://gitlab.com/acme/widgets" :kill-ring ("https://gitlab.com/acme/widgets" "https://gitlab.com/acme/widgets/-/commit/786cfc0929a5e4a193ab22df31406032fd4f3763"))"#
    ]];
    ParityBatchCase::value(
        "commit_and_homepage_commands_use_the_repository_remote_and_copy_each_result",
        elisp_form,
        expected,
    )
}

fn remote_precedence_and_failures_match_git_configuration_and_user_error_contracts()
-> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (neomacs-git-link-test-repository
              "https://github.com/acme/widgets.git"))
       (default-directory root))
  (neomacs-git-link-test-process
   root "remote" "add" "upstream" "https://github.com/platform/widgets.git")
  (neomacs-git-link-test-process root "config" "branch.main.remote" "upstream")
  (let ((tracking (git-link--remote)))
    (neomacs-git-link-test-process root "config" "git-link.remote" "origin")
    (let ((configured (git-link--remote))
          (missing
           (neomacs-git-link-test-error
            (lambda () (git-link-homepage "does-not-exist")))))
      (neomacs-git-link-test-process
       root "remote" "add" "unknown" "ssh://git@example.invalid/acme/widgets.git")
      (list
       :tracking tracking
       :configured configured
       :missing missing
       :unsupported
       (neomacs-git-link-test-error
        (lambda () (git-link-homepage "unknown")))))))
"###;
    let expected = expect![[
        r#"OK (:tracking "upstream" :configured "origin" :missing (:error user-error :data ("Remote ‘does-not-exist’ not found") :message "Remote ‘does-not-exist’ not found") :unsupported (:error user-error :data ("No handler for example.invalid") :message "No handler for example.invalid"))"#
    ]];
    ParityBatchCase::value(
        "remote_precedence_and_failures_match_git_configuration_and_user_error_contracts",
        elisp_form,
        expected,
    )
}

#[test]
fn git_link_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(GIT_LINK_MELPA_PIN, "git-link.el")
            .expect("prepare revision-pinned Git Link below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "git-link-package-batch",
        "Git Link",
        &[
            package_defaults_and_provider_routing_cover_the_supported_hosting_surface(),
            remote_parsing_normalizes_real_https_scp_azure_savannah_and_codecommit_forms(),
            github_markdown_region_link_runs_the_full_git_file_and_kill_ring_workflow(),
            reverse_selection_in_a_narrowed_unicode_file_generates_a_commit_permalink(),
            enterprise_web_host_branch_override_and_browser_policy_are_honored_end_to_end(),
            one_review_selection_maps_to_each_supported_provider_url_convention(),
            commit_and_homepage_commands_use_the_repository_remote_and_copy_each_result(),
            remote_precedence_and_failures_match_git_configuration_and_user_error_contracts(),
        ],
    );
}

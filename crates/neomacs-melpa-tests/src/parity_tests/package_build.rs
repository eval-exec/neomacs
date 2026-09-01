use std::time::Duration;

use expect_test::expect;

use crate::{COMPAT_GNU_ELPA_PIN, CachedMelpaOracle, PACKAGE_BUILD_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PACKAGE_BUILD_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const PACKAGE_BUILD_TEST_PRELUDE: &str = r##"
(require 'package-build)
(require 'package-recipe-mode)

(defun neomacs-package-build-test-write (file text)
  "Write TEXT to FILE, creating its parent directories."
  (make-directory (file-name-directory file) t)
  (with-temp-file file
    (insert text)))

(defun neomacs-package-build-test-slurp (file)
  "Return FILE's contents without text properties."
  (with-temp-buffer
    (insert-file-contents file)
    (buffer-substring-no-properties (point-min) (point-max))))

(defun neomacs-package-build-test-git (directory &rest arguments)
  "Run Git with ARGUMENTS in DIRECTORY and return trimmed stdout."
  (let ((default-directory (file-name-as-directory directory)))
    (with-temp-buffer
      (let ((status (apply #'process-file "git" nil t nil arguments)))
        (unless (zerop status)
          (error "git %S failed (%s): %s"
                 arguments status (buffer-string)))
        (string-trim-right (buffer-string))))))

(defun neomacs-package-build-test-commit (directory message timestamp)
  "Commit DIRECTORY at deterministic TIMESTAMP and return its object name."
  (apply #'neomacs-package-build-test-git directory
         '("add" "--all"))
  (let ((process-environment (copy-sequence process-environment)))
    (setenv "GIT_AUTHOR_NAME" "Neomacs Parity")
    (setenv "GIT_AUTHOR_EMAIL" "parity@example.test")
    (setenv "GIT_AUTHOR_DATE" timestamp)
    (setenv "GIT_COMMITTER_NAME" "Neomacs Parity")
    (setenv "GIT_COMMITTER_EMAIL" "parity@example.test")
    (setenv "GIT_COMMITTER_DATE" timestamp)
    (neomacs-package-build-test-git
     directory "commit" "--no-gpg-sign" "-m" message))
  (neomacs-package-build-test-git directory "rev-parse" "HEAD"))

(defun neomacs-package-build-test-error (function)
  "Call FUNCTION and return its exact error identity and message."
  (condition-case err
      (progn (funcall function) :unexpected-success)
    (package-recipe-invalid
     (list (car err) (error-message-string err)))
    (error (list (car err) (error-message-string err)))))

(defun neomacs-package-build-test-run (body)
  "Run BODY in an isolated package archive rooted below workspace TMPDIR."
  (let* ((root (file-name-as-directory
                (make-temp-file "package-build-parity-" t)))
         (package-build-directory root)
         (package-build-working-dir (expand-file-name "working/" root))
         (package-build-archive-dir (expand-file-name "packages/" root))
         (package-build-recipes-dir (expand-file-name "recipes/" root))
         (package-build-verbose nil)
         (package-build--use-sandbox nil)
         (package-build--tar-type nil)
         (package-build-badge-data nil)
         (package-build-run-recipe-org-exports nil)
         (package-build-run-recipe-shell-command nil)
         (package-build-run-recipe-make-targets nil))
    (make-directory package-build-working-dir t)
    (make-directory package-build-archive-dir t)
    (make-directory package-build-recipes-dir t)
    (unwind-protect
        (funcall body root)
      (dolist (buffer (buffer-list))
        (when-let* ((file (buffer-file-name buffer))
                    ((file-in-directory-p file root)))
          (kill-buffer buffer)))
      (delete-directory root t))))
"##;

fn package_build_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PACKAGE_BUILD_MELPA_PIN, "package-build.el")
        .expect("prepare revision-pinned Package-Build source below ./tmp")
        .with_gnu_elpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare revision-pinned Compat dependency below ./tmp")
        .with_prelude(PACKAGE_BUILD_TEST_PRELUDE)
        .with_timeout(PACKAGE_BUILD_TEST_TIMEOUT)
}

fn curator_recipes_resolve_fetchers_urls_slots_and_exact_validation_errors() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-package-build-test-run
 (lambda (_root)
   (neomacs-package-build-test-write
    (expand-file-name "ship" package-build-recipes-dir)
    "(ship :fetcher github :repo \"team/ship.el\" :branch \"main\" :files (:defaults \"assets/*.json\"))\n")
   (neomacs-package-build-test-write
    (expand-file-name "mirror" package-build-recipes-dir)
    "(mirror :fetcher git :url \"ssh://git@example.test/mirror.git\" :commit \"deadbeef\")\n")
   (neomacs-package-build-test-write
    (expand-file-name "mismatch" package-build-recipes-dir)
    "(other :fetcher github :repo \"team/other\")\n")
   (neomacs-package-build-test-write
    (expand-file-name "unknown" package-build-recipes-dir)
    "(unknown :fetcher github :repo \"team/unknown\" :surprise t)\n")
   (neomacs-package-build-test-write
    (expand-file-name "redundant" package-build-recipes-dir)
    "(redundant :fetcher github :repo \"team/redundant\" :url \"https://example.test/redundant\")\n")
   (neomacs-package-build-test-write
    (expand-file-name "unsafe" package-build-recipes-dir)
    "(unsafe :fetcher git :url \"https://example.test/unsafe\" :files (\"*\"))\n")
   (let ((ship (package-recipe-lookup "ship"))
         (mirror (package-recipe-lookup "mirror")))
     (list
      :recipes (package-recipe-recipes)
      :ship
      (list :class (eieio-object-class ship)
            :fetcher (package-recipe--fetcher ship)
            :url (oref ship url)
            :protocol (package-recipe--upstream-protocol ship)
            :branch (oref ship branch)
            :files (oref ship files)
            :working-directory
            (file-name-nondirectory
             (directory-file-name (package-recipe--working-tree ship))))
      :mirror
      (list :class (eieio-object-class mirror)
            :fetcher (package-recipe--fetcher mirror)
            :url (oref mirror url)
            :protocol (package-recipe--upstream-protocol mirror)
            :commit (oref mirror commit))
      :errors
      (mapcar
       (lambda (name)
         (cons name
               (neomacs-package-build-test-error
                (lambda () (package-recipe-lookup name)))))
       '("mismatch" "unknown" "redundant" "unsafe"))))))
"##;
    let expected = expect![[
        r####"OK (:recipes ("mirror" "mismatch" "redundant" "ship" "unknown" "unsafe") :ship (:class package-github-recipe :fetcher "github" :url "https://github.com/team/ship.el" :protocol "https" :branch "main" :files (:defaults "assets/*.json") :working-directory "ship") :mirror (:class package-git-recipe :fetcher "git" :url "ssh://git@example.test/mirror.git" :protocol "ssh" :commit "deadbeef") :errors (("mismatch" package-recipe-invalid "Invalid package recipe: nil, \"mismatched package name mismatch vs. other\"") ("unknown" package-recipe-invalid "Invalid package recipe: unknown, \"unknown keyword :surprise\"") ("redundant" package-recipe-invalid "Invalid package recipe: redundant, \":url is redundant\"") ("unsafe" package-recipe-invalid "Invalid package recipe: unsafe, \"invalid files spec entry \\\"*\\\"\"")))"####
    ]];
    ParityBatchCase::value(
        "curator_recipes_resolve_fetchers_urls_slots_and_exact_validation_errors",
        elisp_form,
        expected,
    )
}

fn file_spec_builds_the_exact_publishable_tree_and_change_trigger_plan() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-package-build-test-run
 (lambda (root)
   (let* ((repository (expand-file-name "source/" root))
          (target (expand-file-name "staged/" root))
          (spec '(:defaults
                  "assets/*.json"
                  ("lisp" "extensions/*.el")
                  (:rename "LICENSE.md" "LICENSE")
                  (:exclude "lisp/private.el")
                  (:inputs "schema/*.json")))
          (recipe (package-git-recipe
                   :name "ship" :url "https://example.test/ship"
                   :files spec)))
     (dolist (entry '(("ship.el" . "ship")
                      ("helper.el" . "helper")
                      ("ship-tests.el" . "excluded test")
                      ("lisp/private.el" . "excluded private")
                      ("extensions/github.el" . "github adapter")
                      ("assets/schema.json" . "{\"version\":2}")
                      ("schema/input.json" . "{\"input\":true}")
                      ("LICENSE.md" . "GPL-3.0")))
       (neomacs-package-build-test-write
        (expand-file-name (car entry) repository) (cdr entry)))
     (let* ((files (sort (package-build-expand-files-spec
                          recipe t repository spec)
                         (lambda (left right)
                           (string< (car left) (car right)))))
            (default-directory repository))
       (package-build--copy-package-files files target)
       (list
        :files files
        :change-triggers (package-build--spec-globs recipe)
        :staged
        (mapcar
         (lambda (file)
           (cons (file-relative-name file target)
                 (neomacs-package-build-test-slurp file)))
         (sort (directory-files-recursively target "." nil)
               #'string<)))))))
"##;
    let expected = expect![[
        r####"OK (:files (("LICENSE.md" . "LICENSE") ("assets/schema.json" . "schema.json") ("extensions/github.el" . "lisp/github.el") ("helper.el" . "helper.el") ("ship.el" . "ship.el")) :change-triggers (":(glob)*.el" ":(glob)lisp/*.el" ":(glob)dir" ":(glob)*.info" ":(glob)*.texi" ":(glob)*.texinfo" ":(glob)doc/dir" ":(glob)doc/*.info" ":(glob)doc/*.texi" ":(glob)doc/*.texinfo" ":(glob)docs/dir" ":(glob)docs/*.info" ":(glob)docs/*.texi" ":(glob)docs/*.texinfo" ":(glob,exclude).*.el" ":(glob,exclude)lisp/.*.el" ":(glob,exclude)test.el" ":(glob,exclude)tests.el" ":(glob,exclude)*-test.el" ":(glob,exclude)*-tests.el" ":(glob,exclude)lisp/test.el" ":(glob,exclude)lisp/tests.el" ":(glob,exclude)lisp/*-test.el" ":(glob,exclude)lisp/*-tests.el" ":(glob)assets/*.json" ":(glob)extensions/*.el" ":(glob)LICENSE.md" ":(glob,exclude)lisp/private.el" ":(glob)schema/*.json") :staged (("LICENSE" . "GPL-3.0") ("helper.el" . "helper") ("lisp/github.el" . "github adapter") ("schema.json" . "{\"version\":2}") ("ship.el" . "ship")))"####
    ]];
    ParityBatchCase::value(
        "file_spec_builds_the_exact_publishable_tree_and_change_trigger_plan",
        elisp_form,
        expected,
    )
}

fn local_git_history_builds_a_reproducible_installable_archive() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-package-build-test-run
 (lambda (_root)
   (let* ((repository (expand-file-name "demo/" package-build-working-dir))
          (recipe-file (expand-file-name "demo" package-build-recipes-dir))
          (library (expand-file-name "demo.el" repository))
          (readme (expand-file-name "README.md" repository))
          (initial-source
           ";;; demo.el --- publish deterministic release artifacts -*- lexical-binding: t -*-\n\n;; Author: Ada Lovelace <ada@example.test>\n;; Maintainer: Grace Hopper <grace@example.test>\n;; Package-Requires: ((emacs \"26.1\") (compat \"31.0\"))\n;; Version: 1.2.0\n;; URL: http://github.com/team/demo\n;; Keywords: maint, tools\n\n;;; Commentary:\n;; Build deterministic artifacts for release automation.\n\n;;; Code:\n(defun demo-channel () \"stable\")\n(provide 'demo)\n;;; demo.el ends here\n")
          (updated-source
           (replace-regexp-in-string "stable" "candidate" initial-source t t)))
     (make-directory repository t)
     (neomacs-package-build-test-git repository "init" "-b" "main")
     (neomacs-package-build-test-write library initial-source)
     (neomacs-package-build-test-commit
      repository "Release 1.2.0" "2024-01-01T00:00:00Z")
     (neomacs-package-build-test-git repository "tag" "v1.2.0")
     (neomacs-package-build-test-write library updated-source)
     (let ((source-commit
            (neomacs-package-build-test-commit
             repository "Prepare candidate channel" "2024-01-02T03:04:00Z")))
       (neomacs-package-build-test-write readme "Release operator notes.\n")
       (let ((head-commit
              (neomacs-package-build-test-commit
               repository "Document release operation" "2024-01-03T04:05:00Z")))
         (neomacs-package-build-test-git
          repository "update-ref" "refs/remotes/origin/main" "HEAD")
         (neomacs-package-build-test-git
          repository "symbolic-ref" "refs/remotes/origin/HEAD"
          "refs/remotes/origin/main")
         (neomacs-package-build-test-write
          recipe-file
          "(demo :fetcher git :url \"https://example.test/team/demo\")\n")
         (let ((package-build--inhibit-fetch 'strict)
               (package-build--inhibit-checkout nil)
               (package-build-releases nil)
               (package-build-release-version-functions
                (list #'package-build-tag-version))
               (package-build-snapshot-version-functions
                (list #'package-build-release+count-version)))
           (package-build-archive "demo" t t)
           (let* ((entry (assq 'demo (package-build-archive-alist)))
                  (description (cdr entry))
                  (extras (aref description 4))
                  (selected (alist-get :commit extras))
                  (revdesc (alist-get :revdesc extras))
                  (tar-file (package-build--artifact-file entry))
                  (first-digest (secure-hash 'sha256 tar-file))
                  (tar-list (process-lines "tar" "--list" "--file" tar-file))
                  (pkg-file
                   (with-temp-buffer
                     (process-file
                      "tar" nil t nil "-xOf" tar-file
                      "demo-1.2.0.0.1/demo-pkg.el")
                     (buffer-string)))
                  (pkg-file
                   (replace-regexp-in-string
                    "[0-9a-f]\\{12\\}" "<revision>"
                    (replace-regexp-in-string
                     "[0-9a-f]\\{40\\}" "<commit>" pkg-file))))
             (package-build-archive "demo" t t)
             (let ((second-digest (secure-hash 'sha256 tar-file)))
               (list
                :selection
                (list :head-is-docs-only (not (equal head-commit source-commit))
                      :selected-source (equal selected source-commit)
                      :revdesc-valid
                      (and (string-match-p
                            "\\`[0-9a-f]\\{12\\}\\'" revdesc)
                           t))
                :entry
                (list (car entry)
                      (aref description 0)
                      (aref description 1)
                      (aref description 2)
                      (aref description 3)
                      (assq-delete-all
                       :revdesc (assq-delete-all :commit (copy-tree extras))))
                :archive-files
                (sort (directory-files package-build-archive-dir nil
                                       "^[^.]") #'string<)
                :tar-list tar-list
                :descriptor pkg-file
                :readme
                (neomacs-package-build-test-slurp
                 (expand-file-name "demo-readme.txt"
                                   package-build-archive-dir))
                :reproducible (equal first-digest second-digest))))))))))
"##;
    let expected = expect![[
        r####"OK (:selection (:head-is-docs-only t :selected-source t :revdesc-valid t) :entry (demo (1 2 0 0 1) ((emacs (26 1)) (compat (31 0))) "Publish deterministic release artifacts" tar ((:url . "https://github.com/team/demo") (:keywords "maint" "tools") (:authors ("Ada Lovelace" . "ada@example.test")) (:maintainers ("Grace Hopper" . "grace@example.test")) (:maintainer "Grace Hopper" . "grace@example.test"))) :archive-files ("archive-contents" "demo-1.2.0.0.1.entry" "demo-1.2.0.0.1.tar" "demo-readme.txt" "elpa-packages.eld") :tar-list ("demo-1.2.0.0.1/" "demo-1.2.0.0.1/demo-pkg.el" "demo-1.2.0.0.1/demo.el") :descriptor ";; -*- no-byte-compile: t; lexical-binding: nil -*-\n(define-package \"demo\" \"1.2.0.0.1\"\n  \"Publish deterministic release artifacts.\"\n  '((emacs  \"26.1\")\n    (compat \"31.0\"))\n  :url \"https://github.com/team/demo\"\n  :commit \"<commit>\"\n  :revdesc \"<revision>\"\n  :keywords '(\"maint\" \"tools\")\n  :authors '((\"Ada Lovelace\" . \"ada@example.test\"))\n  :maintainers '((\"Grace Hopper\" . \"grace@example.test\")))\n" :readme "Build deterministic artifacts for release automation.\n" :reproducible t)"####
    ]];
    ParityBatchCase::value(
        "local_git_history_builds_a_reproducible_installable_archive",
        elisp_form,
        expected,
    )
}

fn archive_refresh_keeps_the_newest_recipe_and_removes_orphans() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-package-build-test-run
 (lambda (_root)
   (neomacs-package-build-test-write
    (expand-file-name "alpha" package-build-recipes-dir)
    "(alpha :fetcher github :repo \"team/alpha\")\n")
   (dolist
       (entry
        '(("alpha-1.0.entry"
           . (alpha . [(1 0) nil "Old alpha" tar
                       ((:url . "https://example.test/alpha/old"))]))
          ("alpha-2.0.entry"
           . (alpha . [(2 0) ((emacs (26 1))) "Current alpha" tar
                       ((:url . "https://example.test/alpha/current"))]))
          ("orphan-1.0.entry"
           . (orphan . [(1 0) nil "Removed package" tar
                        ((:url . "https://example.test/orphan"))]))))
     (let ((file (expand-file-name (car entry) package-build-archive-dir)))
       (with-temp-file file (prin1 (cdr entry) (current-buffer)))
       (with-temp-file
           (expand-file-name
            (replace-regexp-in-string "\\.entry\\'" ".tar" (car entry))
            package-build-archive-dir)
         (insert "artifact"))))
   (set-file-times (expand-file-name "alpha-1.0.entry" package-build-archive-dir)
                   (seconds-to-time 1000))
   (set-file-times (expand-file-name "orphan-1.0.entry" package-build-archive-dir)
                   (seconds-to-time 1500))
   (set-file-times (expand-file-name "alpha-2.0.entry" package-build-archive-dir)
                   (seconds-to-time 2000))
   (let ((entries (package-build-dump-archive-contents nil t)))
     (list
      :entries entries
      :remaining
      (sort (directory-files package-build-archive-dir nil "^[^.]") #'string<)
      :archive-contents
      (neomacs-package-build-test-slurp
       (expand-file-name "archive-contents" package-build-archive-dir))
      :vc-spec
      (neomacs-package-build-test-slurp
       (expand-file-name "elpa-packages.eld" package-build-archive-dir))))))
"##;
    let expected = expect![[
        r####"OK (:entries ((alpha . [(2 0) ((emacs (26 1))) "Current alpha" tar ((:url . "https://example.test/alpha/current"))])) :remaining ("alpha-2.0.entry" "alpha-2.0.tar" "archive-contents" "elpa-packages.eld") :archive-contents "(1\n (alpha\n  . [(2 0) ((emacs (26 1))) \"Current alpha\" tar\n     ((:url . \"https://example.test/alpha/current\"))]))\n" :vc-spec "(((alpha :url \"https://github.com/team/alpha\"))\n :version 1 :default-vc Git)\n")"####
    ]];
    ParityBatchCase::value(
        "archive_refresh_keeps_the_newest_recipe_and_removes_orphans",
        elisp_form,
        expected,
    )
}

fn recipe_authoring_mode_creates_saves_and_loads_a_new_recipe() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-package-build-test-run
 (lambda (_root)
   (let* ((file (expand-file-name "deploy-tool" package-build-recipes-dir))
          buffer
          created)
     (package-build-create-recipe "deploy-tool" 'github)
     (setq buffer (find-buffer-visiting file))
     (unwind-protect
         (with-current-buffer buffer
           (package-recipe-mode)
           (setq created
                 (list
                  :text (buffer-string)
                  :mode major-mode
                  :indent-tabs indent-tabs-mode
                  :final-newline require-final-newline
                  :save-hook (memq #'whitespace-cleanup before-save-hook)
                  :build-key (lookup-key package-recipe-mode-map (kbd "C-c C-c"))
                  :new-key (lookup-key package-recipe-mode-map (kbd "C-c C-n"))
                  :recipes-dir
                  (file-name-nondirectory
                   (directory-file-name package-build-recipes-dir))
                  :working-dir
                  (file-name-nondirectory
                   (directory-file-name package-build-working-dir))
                  :archive-dir
                  (file-name-nondirectory
                   (directory-file-name package-build-archive-dir))))
           (goto-char (point-min))
           (search-forward "USER/REPO")
           (replace-match "team/deploy-tool" t t)
           (save-buffer)
           (let ((recipe (package-recipe-lookup "deploy-tool")))
             (list :created created
                   :saved (neomacs-package-build-test-slurp buffer-file-name)
                   :class (eieio-object-class recipe)
                   :url (oref recipe url)
                   :duplicate
                   (neomacs-package-build-test-error
                    (lambda ()
                      (package-build-create-recipe "deploy-tool" 'github))))))
       (when (buffer-live-p buffer)
         (kill-buffer buffer))))))
"##;
    let expected = expect![[
        r####"OK (:created (:text "(deploy-tool\n :fetcher github\n :repo \"USER/REPO\")\n" :mode package-recipe-mode :indent-tabs nil :final-newline t :save-hook (whitespace-cleanup t) :build-key package-build-current-recipe :new-key package-build-create-recipe :recipes-dir "recipes" :working-dir "working" :archive-dir "packages") :saved "(deploy-tool\n :fetcher github\n :repo \"team/deploy-tool\")\n" :class package-github-recipe :url "https://github.com/team/deploy-tool" :duplicate (error "Recipe already exists"))"####
    ]];
    ParityBatchCase::value(
        "recipe_authoring_mode_creates_saves_and_loads_a_new_recipe",
        elisp_form,
        expected,
    )
}

#[test]
fn package_build_package_batch() {
    assert_oracle_batch_cases(
        package_build_oracle(),
        "package-build-package-batch",
        "Package-Build",
        &[
            curator_recipes_resolve_fetchers_urls_slots_and_exact_validation_errors(),
            file_spec_builds_the_exact_publishable_tree_and_change_trigger_plan(),
            local_git_history_builds_a_reproducible_installable_archive(),
            archive_refresh_keeps_the_newest_recipe_and_removes_orphans(),
            recipe_authoring_mode_creates_saves_and_loads_a_new_recipe(),
        ],
    );
}

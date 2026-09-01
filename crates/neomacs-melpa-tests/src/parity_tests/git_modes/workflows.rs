use expect_test::expect;

use super::ParityBatchCase;

fn installed_autoloads_progressively_activate_real_repository_files() -> ParityBatchCase {
    let elisp_form = r####"
(gm353-test-run
 "autoload-progression"
 (lambda (root)
   (let* ((initial-features (gm353-test-feature-state))
          (initial-autoloads (gm353-test-autoload-state))
          (initial-registrations (gm353-test-registration-state)))
     (gm353-test-write-file root ".git/info/attributes"
                            "*.wasm binary\n*.txt text eol=lf\n")
     (gm353-test-write-file root ".git/config"
                            "[core]\n\teditor = emacs\n")
     (gm353-test-write-file root ".git/info/exclude"
                            "dist/\n!dist/manifest.json\n")
     (let* ((attributes (gm353-test-visit root ".git/info/attributes"))
            (after-attributes (gm353-test-feature-state))
            (config (gm353-test-visit root ".git/config"))
            (after-config (gm353-test-feature-state))
            (exclude (gm353-test-visit root ".git/info/exclude"))
            (after-exclude (gm353-test-feature-state))
            (before-root-buffers
             (mapcar (lambda (buffer) (gm353-test-buffer-state buffer root))
                     (list attributes config exclude)))
            (registrations-after-components (gm353-test-registration-state))
            (first-require (require 'git-modes))
            (after-root (gm353-test-feature-state))
            (second-require (require 'git-modes))
            (after-second (gm353-test-feature-state)))
       (list
        :initial-features initial-features
        :initial-autoloads initial-autoloads
        :initial-registrations initial-registrations
        :after-attributes after-attributes
        :after-config after-config
        :after-exclude after-exclude
        :before-root-buffers before-root-buffers
        :root-still-unloaded-before-require
        (not (cdr (assq 'git-modes after-exclude)))
        :registrations-after-components registrations-after-components
        :registrations-unchanged-after-components
        (equal initial-registrations registrations-after-components)
        :first-require first-require
        :after-root after-root
        :second-require second-require
        :after-second after-second
        :registrations-unchanged-after-root
        (equal initial-registrations (gm353-test-registration-state))
        :buffers-after-root
        (mapcar (lambda (buffer) (gm353-test-buffer-state buffer root))
                (list attributes config exclude)))))))
"####;
    let expect = expect![[
        r#"OK (:result (:initial-features ((git-modes) (gitattributes-mode) (gitconfig-mode) (gitignore-mode) (compat)) :initial-autoloads ((gitattributes-mode . t) (gitconfig-mode . t) (gitignore-mode . t)) :initial-registrations (:total 12 :unique t :by-mode ((gitattributes-mode . 3) (gitconfig-mode . 6) (gitignore-mode . 3))) :after-attributes ((git-modes) (gitattributes-mode . t) (gitconfig-mode) (gitignore-mode) (compat . t)) :after-config ((git-modes) (gitattributes-mode . t) (gitconfig-mode . t) (gitignore-mode) (compat . t)) :after-exclude ((git-modes) (gitattributes-mode . t) (gitconfig-mode . t) (gitignore-mode . t) (compat . t)) :before-root-buffers ((:file ".git/info/attributes" :mode gitattributes-mode :name "Gitattributes" :parent text-mode :point 1 :modified nil :text "*.wasm binary\n*.txt text eol=lf\n") (:file ".git/config" :mode gitconfig-mode :name "Gitconfig" :parent conf-unix-mode :point 1 :modified nil :text "[core]\n\11editor = emacs\n") (:file ".git/info/exclude" :mode gitignore-mode :name "Gitignore" :parent conf-unix-mode :point 1 :modified nil :text "dist/\n!dist/manifest.json\n")) :root-still-unloaded-before-require t :registrations-after-components (:total 12 :unique t :by-mode ((gitattributes-mode . 3) (gitconfig-mode . 6) (gitignore-mode . 3))) :registrations-unchanged-after-components t :first-require git-modes :after-root ((git-modes . t) (gitattributes-mode . t) (gitconfig-mode . t) (gitignore-mode . t) (compat . t)) :second-require git-modes :after-second ((git-modes . t) (gitattributes-mode . t) (gitconfig-mode . t) (gitignore-mode . t) (compat . t)) :registrations-unchanged-after-root t :buffers-after-root ((:file ".git/info/attributes" :mode gitattributes-mode :name "Gitattributes" :parent text-mode :point 1 :modified nil :text "*.wasm binary\n*.txt text eol=lf\n") (:file ".git/config" :mode gitconfig-mode :name "Gitconfig" :parent conf-unix-mode :point 1 :modified nil :text "[core]\n\11editor = emacs\n") (:file ".git/info/exclude" :mode gitignore-mode :name "Gitignore" :parent conf-unix-mode :point 1 :modified nil :text "dist/\n!dist/manifest.json\n"))) :cleanup (:new-buffers nil :owned-live nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :window-restored t :auto-mode-before-restore t :auto-mode-restored t :body-error nil :cleanup-errors nil))"#
    ]];
    ParityBatchCase::value("autoload-progressive-root-activation", elisp_form, expect)
}

fn repository_policy_edits_save_kill_and_reopen_across_all_modes() -> ParityBatchCase {
    let elisp_form = r####"
(gm353-test-run
 "repository-policy-roundtrip"
 (lambda (root)
   (require 'git-modes)
   (gm353-test-write-file root ".git/config"
                          "[core]\nhooksPath = .githooks\n")
   (gm353-test-write-file root ".gitattributes"
                          "*.txt text eol=lf\n")
   (gm353-test-write-file root ".gitignore" "dist/\n")
   (let ((config (gm353-test-visit root ".git/config"))
         (attributes (gm353-test-visit root ".gitattributes"))
         (ignore (gm353-test-visit root ".gitignore")))
     (with-current-buffer config
       (goto-char (point-min))
       (forward-line 1)
       (call-interactively #'indent-for-tab-command)
       (search-forward ".githooks")
       (replace-match ".config/git/hooks" t t)
       (save-buffer))
     (with-current-buffer attributes
       (goto-char (point-max))
       (insert "*.wasm binary\nrelease/** export-ignore\n")
       (save-buffer))
     (with-current-buffer ignore
       (goto-char (point-max))
       (insert "!dist/manifest.json\n.cache/\n")
       (save-buffer))
     (let ((saved
            (mapcar (lambda (buffer) (gm353-test-buffer-state buffer root))
                    (list config attributes ignore))))
       (dolist (buffer (list config attributes ignore))
         (setq gm353-test-owned-buffers
               (delq buffer gm353-test-owned-buffers))
         (kill-buffer buffer))
       (let ((reopened
              (mapcar (lambda (relative)
                        (gm353-test-buffer-state
                         (gm353-test-visit root relative) root))
                      '(".git/config" ".gitattributes" ".gitignore"))))
         (list :saved saved
               :reopened reopened
               :disk
               (mapcar
                (lambda (relative)
                  (with-temp-buffer
                    (insert-file-contents (expand-file-name relative root))
                    (list relative
                          (buffer-string)
                          (secure-hash 'sha256 (current-buffer)))))
                '(".git/config" ".gitattributes" ".gitignore"))))))))
"####;
    let expect = expect![[
        r#"OK (:result (:saved ((:file ".git/config" :mode gitconfig-mode :name "Gitconfig" :parent conf-unix-mode :point 38 :modified nil :text "[core]\n\11hooksPath = .config/git/hooks\n") (:file ".gitattributes" :mode gitattributes-mode :name "Gitattributes" :parent text-mode :point 58 :modified nil :text "*.txt text eol=lf\n*.wasm binary\nrelease/** export-ignore\n") (:file ".gitignore" :mode gitignore-mode :name "Gitignore" :parent conf-unix-mode :point 35 :modified nil :text "dist/\n!dist/manifest.json\n.cache/\n")) :reopened ((:file ".git/config" :mode gitconfig-mode :name "Gitconfig" :parent conf-unix-mode :point 1 :modified nil :text "[core]\n\11hooksPath = .config/git/hooks\n") (:file ".gitattributes" :mode gitattributes-mode :name "Gitattributes" :parent text-mode :point 1 :modified nil :text "*.txt text eol=lf\n*.wasm binary\nrelease/** export-ignore\n") (:file ".gitignore" :mode gitignore-mode :name "Gitignore" :parent conf-unix-mode :point 1 :modified nil :text "dist/\n!dist/manifest.json\n.cache/\n")) :disk ((".git/config" "[core]\n\11hooksPath = .config/git/hooks\n" "e6fc655d57faae712171a17c173dc3abf9d113f6434fd74c052d22f3b1ff911c") (".gitattributes" "*.txt text eol=lf\n*.wasm binary\nrelease/** export-ignore\n" "2f8c6072e211e95af4660dd58f670e78cf545651e5e29de8ebb3cba421626c82") (".gitignore" "dist/\n!dist/manifest.json\n.cache/\n" "472482961ea5af53d1206a29b6fc3527151b26f730b2cd49e43c6f6328b3e278"))) :cleanup (:new-buffers nil :owned-live nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :window-restored t :auto-mode-before-restore t :auto-mode-restored t :body-error nil :cleanup-errors nil))"#
    ]];
    ParityBatchCase::value(
        "repository-policy-save-reopen-roundtrip",
        elisp_form,
        expect,
    )
}

fn documented_paths_case_folding_and_near_misses_use_real_auto_mode_routing() -> ParityBatchCase {
    let elisp_form = r####"
(gm353-test-run
 "filename-boundaries"
 (lambda (root)
   (require 'git-modes)
   (let ((auto-mode-alist (copy-tree auto-mode-alist))
         (fixtures
          '(("services/api/.dockerignore"
             "# Local Docker context\nnode_modules/\ndist/\n")
            (".gitmodules" "[submodule \"vendor\"]\n\tpath = vendor\n")
            (".config/git/attributes" "*.zip binary\n")
            (".config/git/ignore" "*.swp\n")
            (".GITIGNORE" "UPPER/\n")
            (".git/info/attributes.d/policy" "*.pdf diff\n")
            (".git/info/exclude.bak" "cache/\n")
            (".gitignore.sample" "sample/\n"))))
     ;; This is the README's documented way to reuse Gitignore Mode for a
     ;; non-Git policy file.  Keep it dynamically scoped to this user story.
     (add-to-list 'auto-mode-alist
                  (cons "/.dockerignore\\'" 'gitignore-mode))
     (let ((buffers
            (mapcar
             (lambda (fixture)
               (gm353-test-write-file root (car fixture) (cadr fixture))
               (gm353-test-visit root (car fixture)))
             fixtures)))
       (with-current-buffer (car buffers)
         (goto-char (point-max))
         (insert "!dist/manifest.json\n")
         (save-buffer))
       (list
        :custom-registration
        (cl-count '("/.dockerignore\\'" . gitignore-mode)
                  auto-mode-alist :test #'equal)
        :files
        (mapcar (lambda (buffer) (gm353-test-buffer-state buffer root))
                buffers))))))
"####;
    let expect = expect![[
        r##"OK (:result (:custom-registration 1 :files ((:file "services/api/.dockerignore" :mode gitignore-mode :name "Gitignore" :parent conf-unix-mode :point 64 :modified nil :text "# Local Docker context\nnode_modules/\ndist/\n!dist/manifest.json\n") (:file ".gitmodules" :mode gitconfig-mode :name "Gitconfig" :parent conf-unix-mode :point 1 :modified nil :text "[submodule \"vendor\"]\n\11path = vendor\n") (:file ".config/git/attributes" :mode gitattributes-mode :name "Gitattributes" :parent text-mode :point 1 :modified nil :text "*.zip binary\n") (:file ".config/git/ignore" :mode gitignore-mode :name "Gitignore" :parent conf-unix-mode :point 1 :modified nil :text "*.swp\n") (:file ".GITIGNORE" :mode gitignore-mode :name "Gitignore" :parent conf-unix-mode :point 1 :modified nil :text "UPPER/\n") (:file ".git/info/attributes.d/policy" :mode fundamental-mode :name "Fundamental" :parent nil :point 1 :modified nil :text "*.pdf diff\n") (:file ".git/info/exclude.bak" :mode gitignore-mode :name "Gitignore" :parent conf-unix-mode :point 1 :modified nil :text "cache/\n") (:file ".gitignore.sample" :mode fundamental-mode :name "Fundamental" :parent nil :point 1 :modified nil :text "sample/\n"))) :cleanup (:new-buffers nil :owned-live nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :window-restored t :auto-mode-before-restore t :auto-mode-restored t :body-error nil :cleanup-errors nil))"##
    ]];
    ParityBatchCase::value(
        "documented-custom-mode-and-routing-boundaries",
        elisp_form,
        expect,
    )
}

pub(super) fn activation_workflow_cases() -> Vec<ParityBatchCase> {
    vec![installed_autoloads_progressively_activate_real_repository_files()]
}

pub(super) fn loaded_workflow_cases() -> Vec<ParityBatchCase> {
    vec![
        repository_policy_edits_save_kill_and_reopen_across_all_modes(),
        documented_paths_case_folding_and_near_misses_use_real_auto_mode_routing(),
    ]
}

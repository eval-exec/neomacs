use expect_test::expect;

use super::ParityBatchCase;

fn file_region_browser_and_clipboard_preserve_real_state() -> ParityBatchCase {
    let elisp_form = r####"
(bar355-test-run
 "file-region-browser"
 (lambda (root)
   (let* ((fixture
           (bar355-test-make-repo
            root "git@github.com:acme/Widget.Kit.git"))
          (repo (plist-get fixture :repo))
          (notes (plist-get fixture :notes))
          (buffer (bar355-test-visit notes))
          (browse-at-remote-prefer-symbolic t)
          (browse-at-remote-preferred-remote-name "origin")
          (browse-at-remote-add-line-number-if-no-region-selected t))
     (switch-to-buffer buffer)
     (goto-char (point-min))
     (forward-line 2)
     (setq bar355-test-browser-plan
           (list (list :url
                       "https://github.com/acme/Widget.Kit/blob/main/docs/Release Notes.md#L3"
                       :args '(nil) :return 'browser-result)))
     (let* ((before
             (list :bytes (buffer-string) :point (point)
                   :line (line-number-at-pos) :mark-active mark-active))
            (line-browse
             (bar355-test-traced
              root (lambda () (call-interactively #'bar-browse))))
            (line-event (copy-tree (car bar355-test-browser-events)))
            forward-region reverse-region region-url hash-ref tree-kill)
       (setq bar355-test-browser-events nil)
       ;; Forward selection: MARK starts line two and point starts line six.
       (goto-char (point-min))
       (forward-line 1)
       (push-mark (point) t t)
       (goto-char (point-min))
       (forward-line 5)
       (setq transient-mark-mode t mark-active t)
       (setq forward-region
             (list
              :state (list :point (point) :mark (mark)
                           :point-after-mark (> (point) (mark))
                           :active (use-region-p))
              :call
              (bar355-test-traced
               root (lambda () (browse-at-remote-get-url)))))
       ;; Reverse selection: point starts line two and MARK starts line six.
       ;; The public command must exclude line six's preceding newline.
       (goto-char (point-min))
       (forward-line 5)
       (push-mark (point) t t)
       (goto-char (point-min))
       (forward-line 1)
       (setq transient-mark-mode t mark-active t)
       (setq reverse-region
             (list
              :state (list :point (point) :mark (mark)
                           :point-before-mark (< (point) (mark))
                           :active (use-region-p))
              :call
              (bar355-test-traced
               root (lambda ()
                      (call-interactively #'bar-to-clipboard)))))
       (setq region-url (copy-sequence (car kill-ring)))
       (deactivate-mark)
       (goto-char (point-min))
       (forward-line 2)
       (let ((browse-at-remote-prefer-symbolic nil))
         (setq hash-ref
               (bar355-test-traced
                root (lambda () (browse-at-remote-get-url))))
         (setf (plist-get (plist-get hash-ref :outcome) :value)
               (bar355-test-normalize-head
                (plist-get (plist-get hash-ref :outcome) :value)
                (plist-get fixture :head))))
       (setq browse-at-remote-add-line-number-if-no-region-selected nil)
       (setq tree-kill
             (bar355-test-traced
              root (lambda ()
                     (call-interactively #'browse-at-remote-kill))))
       (let ((disk-bytes
              (with-temp-buffer
                (insert-file-contents notes)
                (buffer-string))))
         (list
          :repo (bar355-test-relative repo root)
          :head-valid
          (and (string-match-p "\\`[0-9a-f]\\{40\\}\\'"
                               (plist-get fixture :head))
               t)
          :before before
          :line-browse line-browse
          :line-browser-event line-event
          :forward-region forward-region
          :reverse-region reverse-region
          :region-url region-url
          :hash-ref hash-ref
          :tree-kill tree-kill
          :kill-heads (mapcar #'copy-sequence (seq-take kill-ring 2))
          :kill-yank-is-head (eq kill-ring-yank-pointer kill-ring)
          :browser-events-after-kills bar355-test-browser-events
          :after
          (list :bytes (buffer-string) :disk-bytes disk-bytes
                :point (point) :line (line-number-at-pos)
                :modified (buffer-modified-p) :mark-active mark-active
                :git-status
                (bar355-test-git-stdout repo "status" "--porcelain"))))))))
"####;
    ParityBatchCase::value(
        "file-region-browser-and-clipboard-preserve-real-state",
        elisp_form,
        expect![[
            r#"OK (:result (:repo "repo/" :head-valid t :before (:bytes "alpha\nbeta界\ngamma\ndelta\nepsilon\n" :point 13 :line 3 :mark-active nil) :line-browse (:outcome (:value browser-result) :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "config" "branch.main.pushRemote") :worktree "repo" :exit 1) (:argv ("git" "--no-pager" "rev-parse" "--symbolic-full-name" "--abbrev-ref" "main@{upstream}") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "origin") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "repo" :exit 1) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "repo" :exit 1))) :line-browser-event (:url "https://github.com/acme/Widget.Kit/blob/main/docs/Release Notes.md#L3" :args (nil) :mode fundamental-mode :buffer "Release Notes.md" :file "repo/docs/Release Notes.md" :directory "repo/docs/") :forward-region (:state (:point 33 :mark 7 :point-after-mark t :active t) :call (:outcome (:value "https://github.com/acme/Widget.Kit/blob/main/docs/Release Notes.md#L2-L5") :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "config" "branch.main.pushRemote") :worktree "repo" :exit 1) (:argv ("git" "--no-pager" "rev-parse" "--symbolic-full-name" "--abbrev-ref" "main@{upstream}") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "origin") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "repo" :exit 1) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "repo" :exit 1)))) :reverse-region (:state (:point 7 :mark 33 :point-before-mark t :active t) :call (:outcome (:value nil) :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "config" "branch.main.pushRemote") :worktree "repo" :exit 1) (:argv ("git" "--no-pager" "rev-parse" "--symbolic-full-name" "--abbrev-ref" "main@{upstream}") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "origin") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "repo" :exit 1) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "repo" :exit 1)))) :region-url "https://github.com/acme/Widget.Kit/blob/main/docs/Release Notes.md#L2-L5" :hash-ref (:outcome (:value "https://github.com/acme/Widget.Kit/blob/[HEAD]/docs/Release Notes.md#L3") :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "rev-parse" "HEAD") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "config" "branch.main.pushRemote") :worktree "repo" :exit 1) (:argv ("git" "--no-pager" "rev-parse" "--symbolic-full-name" "--abbrev-ref" "main@{upstream}") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "origin") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "repo" :exit 1) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "repo" :exit 1))) :tree-kill (:outcome (:value nil) :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "config" "branch.main.pushRemote") :worktree "repo" :exit 1) (:argv ("git" "--no-pager" "rev-parse" "--symbolic-full-name" "--abbrev-ref" "main@{upstream}") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "origin") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "repo" :exit 1) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "repo" :exit 1))) :kill-heads ("https://github.com/acme/Widget.Kit/tree/main/docs/Release Notes.md" "https://github.com/acme/Widget.Kit/blob/main/docs/Release Notes.md#L2-L5") :kill-yank-is-head t :browser-events-after-kills nil :after (:bytes "alpha\nbeta界\ngamma\ndelta\nepsilon\n" :disk-bytes "alpha\nbeta界\ngamma\ndelta\nepsilon\n" :point 13 :line 3 :modified nil :mark-active nil :git-status "")) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :owned-buffer-live nil :owned-timer-live nil :root-exists nil :root-owned nil :current-buffer-restored t :window-restored t :dired-buffers-restored t :vc-annotate-display-restored t :kill-ring-restored t :kill-yank-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn real_remote_and_ref_precedence_uses_git_configuration() -> ParityBatchCase {
    let elisp_form = r####"
(bar355-test-run
 "remote-ref-precedence"
 (lambda (root)
   (let* ((tracked-root
           (file-name-as-directory
            (bar355-test-owned-path root "tracked")))
          (tracked
           (bar355-test-make-repo
            tracked-root "git@github.com:acme/Widget.Kit.git"
            "feature/review"))
          (tracked-repo (plist-get tracked :repo))
          (tracked-file (plist-get tracked :notes))
          (browse-at-remote-preferred-remote-name "origin")
          (browse-at-remote-prefer-symbolic t))
     (bar355-test-git tracked-repo "update-ref"
                      "refs/remotes/origin/main" "HEAD")
     (bar355-test-config tracked-repo "branch.feature/review.merge"
                         "refs/heads/main")
     (bar355-test-git tracked-repo "remote" "add" "backup"
                      "https://gitlab.com/acme/widget-kit.git")
     (let ((buffer (bar355-test-visit tracked-file)))
       (switch-to-buffer buffer)
       (goto-char (point-min))
       (forward-line 2)
       (let ((upstream
              (bar355-test-traced
               root (lambda () (browse-at-remote-get-url)))))
         (bar355-test-config tracked-repo
                             "branch.feature/review.pushRemote" "backup")
         (let ((push-remote
                (bar355-test-traced
                 root (lambda () (browse-at-remote-get-url)))))
           (bar355-test-git tracked-repo "config" "--unset-all"
                            "branch.feature/review.pushRemote")
           (let* ((browse-at-remote-prefer-symbolic nil)
                  (hash-ref
                   (bar355-test-traced
                    root (lambda () (browse-at-remote-get-url)))))
             (setf (plist-get (plist-get hash-ref :outcome) :value)
                   (bar355-test-normalize-head
                    (plist-get (plist-get hash-ref :outcome) :value)
                    (plist-get tracked :head)))
             (let* ((detached-root
                     (file-name-as-directory
                      (bar355-test-owned-path root "detached")))
                    (detached
                     (bar355-test-make-repo
                      detached-root "git@github.com:acme/project.git"))
                    (detached-repo (plist-get detached :repo)))
               (bar355-test-git detached-repo "checkout" "--quiet" "--detach")
               ;; Visit only after detaching so GNU's symbolic-ref cache cannot
               ;; preserve the old branch.
               (let ((detached-buffer
                      (bar355-test-visit (plist-get detached :demo))))
                 (switch-to-buffer detached-buffer)
                 (goto-char (point-min))
                 (forward-line 1)
                 (let* ((browse-at-remote-prefer-symbolic t)
                        (detached-symbolic
                         (bar355-test-traced
                          root (lambda () (browse-at-remote-get-url))))
                        (browse-at-remote-prefer-symbolic nil)
                        (detached-hash
                         (bar355-test-traced
                          root (lambda () (browse-at-remote-get-url)))))
                   (setf (plist-get (plist-get detached-hash :outcome) :value)
                         (bar355-test-normalize-head
                          (plist-get (plist-get detached-hash :outcome) :value)
                          (plist-get detached :head)))
                   (let* ((preferred-root
                           (file-name-as-directory
                            (bar355-test-owned-path root "preferred")))
                          (preferred
                           (bar355-test-make-repo
                            preferred-root
                            "git@gitlab.com:wrong/project.git"
                            "topic/deep"))
                          (preferred-repo (plist-get preferred :repo)))
                     (bar355-test-git preferred-repo "config" "--unset-all"
                                      "branch.topic/deep.remote")
                     (bar355-test-git preferred-repo "config" "--unset-all"
                                      "branch.topic/deep.merge")
                     (bar355-test-git preferred-repo "remote" "rename"
                                      "origin" "zzz")
                     (bar355-test-git preferred-repo "remote" "add" "aaa"
                                      "git@github.com:first/project.git")
                     (let ((preferred-buffer
                            (bar355-test-visit (plist-get preferred :demo))))
                       (switch-to-buffer preferred-buffer)
                       (goto-char (point-min))
                       (forward-line 1)
                       (let* ((browse-at-remote-preferred-remote-name "missing")
                              (browse-at-remote-prefer-symbolic t)
                              (alphabetical
                               (bar355-test-traced
                                root
                                (lambda () (browse-at-remote-get-url)))))
                         (list
                          :tracked-state
                          (list
                           :branch
                           (bar355-test-git-stdout
                            tracked-repo "branch" "--show-current")
                           :upstream
                           (bar355-test-git-stdout
                            tracked-repo "rev-parse" "--abbrev-ref"
                            "feature/review@{upstream}")
                           :remotes
                           (split-string
                            (bar355-test-git-stdout tracked-repo "remote")
                            "\n" t))
                          :upstream upstream
                          :push-remote push-remote
                          :hash-ref hash-ref
                          :detached-state
                          (list
                           :branch
                           (bar355-test-git-stdout
                            detached-repo "branch" "--show-current")
                           :head-valid
                           (and
                            (string-match-p "\\`[0-9a-f]\\{40\\}\\'"
                                            (plist-get detached :head))
                            t))
                          :detached-symbolic detached-symbolic
                          :detached-hash detached-hash
                          :preferred-remotes
                          (split-string
                           (bar355-test-git-stdout preferred-repo "remote")
                           "\n" t)
                          :alphabetical-fallback alphabetical))))))))))))))
"####;
    ParityBatchCase::value(
        "real-remote-and-ref-precedence-uses-git-configuration",
        elisp_form,
        expect![[
            r#"OK (:result (:tracked-state (:branch "feature/review" :upstream "origin/main" :remotes ("backup" "origin")) :upstream (:outcome (:value "https://github.com/acme/Widget.Kit/blob/main/docs/Release Notes.md#L3") :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "tracked/repo" :exit 0) (:argv ("git" "--no-pager" "config" "branch.feature/review.pushRemote") :worktree "tracked/repo" :exit 1) (:argv ("git" "--no-pager" "rev-parse" "--symbolic-full-name" "--abbrev-ref" "feature/review@{upstream}") :worktree "tracked/repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "origin") :worktree "tracked/repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "tracked/repo" :exit 1) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "tracked/repo" :exit 1))) :push-remote (:outcome (:value "https://gitlab.com/acme/widget-kit/blob/feature/review/docs/Release Notes.md#L3") :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "tracked/repo" :exit 0) (:argv ("git" "--no-pager" "config" "branch.feature/review.pushRemote") :worktree "tracked/repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "backup") :worktree "tracked/repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "tracked/repo" :exit 1) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "tracked/repo" :exit 1))) :hash-ref (:outcome (:value "https://github.com/acme/Widget.Kit/blob/[HEAD]/docs/Release Notes.md#L3") :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "tracked/repo" :exit 0) (:argv ("git" "--no-pager" "rev-parse" "HEAD") :worktree "tracked/repo" :exit 0) (:argv ("git" "--no-pager" "config" "branch.feature/review.pushRemote") :worktree "tracked/repo" :exit 1) (:argv ("git" "--no-pager" "rev-parse" "--symbolic-full-name" "--abbrev-ref" "feature/review@{upstream}") :worktree "tracked/repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "origin") :worktree "tracked/repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "tracked/repo" :exit 1) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "tracked/repo" :exit 1))) :detached-state (:branch "" :head-valid t) :detached-symbolic (:outcome (:value "https://github.com/acme/project/blob/nil/src/demo.el#L2") :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "detached/repo" :exit 128) (:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "detached/repo" :exit 128) (:argv ("git" "--no-pager" "remote") :worktree "detached/repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "origin") :worktree "detached/repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "detached/repo" :exit 1) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "detached/repo" :exit 1))) :detached-hash (:outcome (:value "https://github.com/acme/project/blob/[HEAD]/src/demo.el#L2") :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "detached/repo" :exit 128) (:argv ("git" "--no-pager" "rev-parse" "HEAD") :worktree "detached/repo" :exit 0) (:argv ("git" "--no-pager" "remote") :worktree "detached/repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "origin") :worktree "detached/repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "detached/repo" :exit 1) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "detached/repo" :exit 1))) :preferred-remotes ("aaa" "zzz") :alphabetical-fallback (:outcome (:value "https://github.com/first/project/blob/topic/deep/src/demo.el#L2") :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "preferred/repo" :exit 0) (:argv ("git" "--no-pager" "config" "branch.topic/deep.pushRemote") :worktree "preferred/repo" :exit 1) (:argv ("git" "--no-pager" "rev-parse" "--symbolic-full-name" "--abbrev-ref" "topic/deep@{upstream}") :worktree "preferred/repo" :exit 128) (:argv ("git" "--no-pager" "remote") :worktree "preferred/repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "aaa") :worktree "preferred/repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "preferred/repo" :exit 1) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "preferred/repo" :exit 1)))) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :owned-buffer-live nil :owned-timer-live nil :root-exists nil :root-owned nil :current-buffer-restored t :window-restored t :dired-buffers-restored t :vc-annotate-display-restored t :kill-ring-restored t :kill-yank-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_provider_and_enterprise_routes_preserve_exact_url_syntax() -> ParityBatchCase {
    let elisp_form = r####"
(bar355-test-run
 "provider-enterprise-routes"
 (lambda (root)
   (let* ((fixture
           (bar355-test-make-repo
            root "git@github.com:acme/project.git" "release/v2"))
          (repo (plist-get fixture :repo))
          (buffer (bar355-test-visit (plist-get fixture :demo)))
          (readme-buffer (bar355-test-visit (plist-get fixture :readme)))
          (browse-at-remote-preferred-remote-name "origin")
          (browse-at-remote-prefer-symbolic t))
     (switch-to-buffer buffer)
     (goto-char (point-min))
     (forward-line 1)
     (push-mark (point) t t)
     (forward-line 2)
     (setq transient-mark-mode t mark-active t)
     (let ((route-contracts nil) (route-call-count 0))
       (cl-labels
           ((route
             (name remote &optional type actual-host target-buffer)
             (bar355-test-git repo "remote" "set-url" "origin" remote)
             (bar355-test-unset-config repo "browseAtRemote.type")
             (bar355-test-unset-config repo "browseAtRemote.actualHost")
             (when type
               (bar355-test-config repo "browseAtRemote.type" type))
             (when actual-host
               (bar355-test-config repo "browseAtRemote.actualHost" actual-host))
             (let* ((traced
                     (with-current-buffer (or target-buffer (current-buffer))
                       (bar355-test-traced
                        root (lambda () (browse-at-remote-get-url)))))
                    (actual-host-status (if actual-host 0 1))
                    (type-status (if type 0 1))
                    (url
                     (bar355-test-validate-provider-trace
                      traced "repo" "release/v2"
                      actual-host-status type-status)))
               (setq route-call-count (+ route-call-count 6))
               (push (list name actual-host-status type-status)
                     route-contracts)
               (list name url))))
         (let ((matrix
                (list
                  (route 'github "git@github.com:acme/project.git")
                  (route 'gitlab "git@gitlab.com:acme/project.git")
                  (route 'bitbucket "git@bitbucket.org:acme/project.git")
                  (route 'gnu "git://git.savannah.gnu.org/emacs.git")
                  (route 'ado
                         "https://vs-ssh.visualstudio.com/v3/GreatBanana/Forest/Gorillas")
                  (route 'gist "git@gist.github.com:abc123.git")
                  (route 'sourcehut "git@git.sr.ht:~acme/project.git")
                  (route 'pagure "https://pagure.io/acme/project.git")
                  (with-current-buffer readme-buffer
                    (goto-char (point-min))
                    (forward-line 1)
                    (push-mark (point) t t)
                    (forward-line 3)
                    (setq transient-mark-mode t mark-active t)
                    (route 'pagure-markup
                           "https://pagure.io/acme/project.git"
                           nil nil readme-buffer))
                  (route 'gitiles
                         "https://chromium-review.googlesource.com/chromiumos/platform/ec.git")
                  (route 'gitea "git@gitea.com:acme/project.git")
                  (route 'stash "https://stash.example.invalid/ACME/project.git"
                         "stash")
                  (route 'phabricator
                         "https://phab.example.invalid/source/project.git"
                         "phabricator")
                  (route 'enterprise
                         "git@work.github.invalid:acme/project.git"
                         "github" "github.com"))))
           (list
            :selection
          (list :point (point) :mark (mark) :active (use-region-p)
                :lines (list (line-number-at-pos (region-beginning))
                             (line-number-at-pos (1- (region-end))))
                :file (bar355-test-relative buffer-file-name root))
          :pagure-markup-selection
          (with-current-buffer readme-buffer
            (list :point (point) :mark (mark) :active (use-region-p)
                  :lines (list (line-number-at-pos (region-beginning))
                               (line-number-at-pos (1- (region-end))))
                  :char-before (char-before)
                  :bytes (buffer-string)
                  :file (bar355-test-relative buffer-file-name root)))
            :matrix matrix
            :git-contract
            (list :routes (nreverse route-contracts)
                  :calls route-call-count :argv-validated t)
            :repo-state
            (list
             :origin (bar355-test-git-stdout
                      repo "remote" "get-url" "origin")
             :type (bar355-test-git-stdout
                    repo "config" "--get" "browseAtRemote.type")
             :actual-host (bar355-test-git-stdout
                           repo "config" "--get"
                           "browseAtRemote.actualHost")
             :status (bar355-test-git-stdout repo "status" "--porcelain")))))))))
"####;
    ParityBatchCase::value(
        "public-provider-and-enterprise-routes-preserve-exact-url-syntax",
        elisp_form,
        expect![[
            r##"OK (:result (:selection (:point 39 :mark 13 :active t :lines (2 3) :file "repo/src/demo.el") :pagure-markup-selection (:point 38 :mark 14 :active t :lines (2 4) :char-before 10 :bytes "# Widget Kit\n\nRead me.\nMore details.\n" :file "repo/README.md") :matrix ((github "https://github.com/acme/project/blob/release/v2/src/demo.el#L2-L3") (gitlab "https://gitlab.com/acme/project/blob/release/v2/src/demo.el#L2-3") (bitbucket "https://bitbucket.org/acme/project/src/release/v2/src/demo.el#cl-2:3") (gnu "https:/git.savannah.gnu.org/cgit/emacs.git/tree/src/demo.el?h=release/v2#n2") (ado "https://GreatBanana.visualstudio.com/Forest/_git/Gorillas?version=GBrelease/v2&path=/src/demo.el&line=2&lineEnd=4&lineStartColumn=1&lineEndColumn=1") (gist "https://gist.github.com/abc123#file-src-demo-el-L2-L3") (sourcehut "https://git.sr.ht/~acme/project/tree/release/v2/src/demo.el#L2-3") (pagure "https://pagure.io/acme/project/blob/release/v2/f/src/demo.el#_2-3") (pagure-markup "https://pagure.io/acme/project/blob/release/v2/f/README.md?text=True#_2-4") (gitiles "https://chromium.googlesource.com/chromiumos/platform/ec/+/release/v2/src/demo.el#2") (gitea "https://gitea.com/acme/project/src/release/v2/src/demo.el#L2-L3") (stash "https://stash.example.invalid/projects/ACME/repos/project/browse/src/demo.el?at=release/v2#2-3") (phabricator "https://phab.example.invalid/source/project/browse/release/v2/src/demo.el$2-3") (enterprise "https://github.com/acme/project/blob/release/v2/src/demo.el#L2-L3")) :git-contract (:routes ((github 1 1) (gitlab 1 1) (bitbucket 1 1) (gnu 1 1) (ado 1 1) (gist 1 1) (sourcehut 1 1) (pagure 1 1) (pagure-markup 1 1) (gitiles 1 1) (gitea 1 1) (stash 1 0) (phabricator 1 0) (enterprise 0 0)) :calls 84 :argv-validated t) :repo-state (:origin "git@work.github.invalid:acme/project.git" :type "github" :actual-host "github.com" :status "")) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :owned-buffer-live nil :owned-timer-live nil :root-exists nil :root-owned nil :current-buffer-restored t :window-restored t :dired-buffers-restored t :vc-annotate-display-restored t :kill-ring-restored t :kill-yank-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn real_dired_tree_and_vc_annotation_open_public_urls() -> ParityBatchCase {
    let elisp_form = r####"
(bar355-test-run
 "dired-vc-annotation"
 (lambda (root)
   (let* ((fixture
           (bar355-test-make-repo
            root "git@github.com:acme/Widget.Kit.git"))
          (repo (plist-get fixture :repo))
          (notes (plist-get fixture :notes))
          (first-head (plist-get fixture :head))
          (second-head (bar355-test-second-commit root repo))
          (docs (file-name-as-directory (expand-file-name "docs" repo)))
          (browse-at-remote-preferred-remote-name "origin")
          (browse-at-remote-prefer-symbolic t))
     (bar355-test-git repo "update-ref" "refs/remotes/origin/main" "HEAD")
     (let ((dired-buffer (bar355-test-own-buffer (dired-noselect docs))))
       (switch-to-buffer dired-buffer)
       (setq bar355-test-browser-plan
             (list (list :url
                         "https://github.com/acme/Widget.Kit/tree/main/docs/"
                         :args '(nil) :return 'dired-browser-result)))
       (let* ((dired-call
               (bar355-test-traced
                root (lambda () (call-interactively #'browse-at-remote))))
              (dired-event (copy-tree (car bar355-test-browser-events))))
         (setq bar355-test-browser-events nil)
         (let ((source-buffer (bar355-test-visit notes))
               annotate-buffer annotate-terminal)
           (switch-to-buffer source-buffer)
           (goto-char (point-min))
           (forward-line 1)
           (let ((annotation-call
                  (bar355-test-traced
                   root
                   (lambda ()
                     (vc-annotate notes second-head nil nil 2 'Git)
                     (setq annotate-buffer (current-buffer))
                     (bar355-test-own-buffer annotate-buffer)
                     (setq annotate-terminal
                           (bar355-test-wait-annotate annotate-buffer))
                     (unless (eq major-mode 'vc-annotate-mode)
                       (error "Browse At Remote did not enter real VC annotate mode: %S"
                              major-mode))
                     (goto-char (point-min))
                     (forward-line 1)
                     (let* ((line
                             (buffer-substring-no-properties
                              (line-beginning-position) (line-end-position)))
                            (revision-file
                             (vc-annotate-extract-revision-at-line))
                            (revision
                             (if (consp revision-file)
                                 (car revision-file)
                               revision-file)))
                       (unless (and (stringp revision)
                                    (string-prefix-p revision second-head))
                         (error "Browse At Remote selected wrong annotation revision: %S"
                                revision-file))
                       (setq bar355-test-browser-plan
                             (list
                              (list
                               :url
                               (format
                                "https://github.com/acme/Widget.Kit/commit/%s"
                                revision)
                               :args '(nil)
                               :return 'annotate-browser-result)))
                       (let ((command-return
                              (call-interactively #'browse-at-remote))
                             (event
                              (copy-tree (car bar355-test-browser-events))))
                         (setf (plist-get event :url)
                               (bar355-test-normalize-abbrev
                                (plist-get event :url) revision second-head))
                         (setf (plist-get event :buffer)
                               (bar355-test-normalize-head
                                (plist-get event :buffer) second-head))
                         (list
                          :mode major-mode
                          :line
                          (bar355-test-normalize-abbrev
                           line revision second-head)
                          :revision "[ABBREV-HEAD]"
                          :revision-file
                          (bar355-test-relative
                           (and (consp revision-file) (cdr revision-file)) root)
                          :command-return command-return
                          :browser-event event
                          :process annotate-terminal)))))))
             (setf (plist-get annotation-call :git)
                   (bar355-test-normalize-trace-head
                    (plist-get annotation-call :git) second-head))
             (list
              :heads
              (list :first-valid
                    (and (string-match-p "\\`[0-9a-f]\\{40\\}\\'" first-head)
                         t)
                    :second-valid
                    (and (string-match-p "\\`[0-9a-f]\\{40\\}\\'" second-head)
                         t)
                    :changed (not (equal first-head second-head)))
              :dired
              (list :mode (buffer-local-value 'major-mode dired-buffer)
                    :directory (bar355-test-relative
                                (buffer-local-value 'default-directory
                                                    dired-buffer)
                                root)
                    :call dired-call :browser-event dired-event)
              :annotation annotation-call
              :disk-bytes
              (with-temp-buffer
                (insert-file-contents notes)
                (buffer-string))
              :git-status
              (bar355-test-git-stdout repo "status" "--porcelain")))))))))
"####;
    ParityBatchCase::value(
        "real-dired-tree-and-vc-annotation-open-public-urls",
        elisp_form,
        expect![[
            r#"OK (:result (:heads (:first-valid t :second-valid t :changed t) :dired (:mode dired-mode :directory "repo/docs/" :call (:outcome (:value dired-browser-result) :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "config" "branch.main.pushRemote") :worktree "repo" :exit 1) (:argv ("git" "--no-pager" "rev-parse" "--symbolic-full-name" "--abbrev-ref" "main@{upstream}") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "origin") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "repo" :exit 1) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "repo" :exit 1))) :browser-event (:url "https://github.com/acme/Widget.Kit/tree/main/docs/" :args (nil) :mode dired-mode :buffer "docs" :file nil :directory "repo/docs/")) :annotation (:outcome (:value (:mode vc-annotate-mode :line "[ABBREV-HEAD] (Parity Author 2002-03-04 2) beta界 updated" :revision "[ABBREV-HEAD]" :revision-file "repo/docs/Release Notes.md" :command-return annotate-browser-result :browser-event (:url "https://github.com/acme/Widget.Kit/commit/[ABBREV-HEAD]" :args (nil) :mode vc-annotate-mode :buffer "*Annotate Release Notes.md (rev [HEAD])*" :file nil :directory "repo/docs/") :process (:status exit :exit 0 :live nil :attached-live nil :stable-rounds 3 :buffer-live t :trace-terminal t))) :git ((:argv ("git" "--no-pager" "blame" "--date=short" "[HEAD]" "--" "Release Notes.md") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "config" "branch.main.pushRemote") :worktree "repo" :exit 1) (:argv ("git" "--no-pager" "rev-parse" "--symbolic-full-name" "--abbrev-ref" "main@{upstream}") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "origin") :worktree "repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "repo" :exit 1) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "repo" :exit 1))) :disk-bytes "alpha\nbeta界 updated\ngamma\ndelta\nepsilon\n" :git-status "") :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :owned-buffer-live nil :owned-timer-live nil :root-exists nil :root-owned nil :current-buffer-restored t :window-restored t :dired-buffers-restored t :vc-annotate-display-restored t :kill-ring-restored t :kill-yank-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_failures_preserve_state_and_recover_in_process() -> ParityBatchCase {
    let elisp_form = r####"
(bar355-test-run
 "public-failures-recovery"
 (lambda (root)
   (let* ((unsupported-buffer
           (with-temp-buffer
             (insert "ordinary scratch bytes")
             (goto-char 7)
             (let ((before (list :bytes (buffer-string) :point (point))))
               (list
                :call
                (bar355-test-traced
                 root (lambda () (browse-at-remote-get-url)) t)
                :before before
                :after (list :bytes (buffer-string) :point (point))))))
          (no-remote-root
           (file-name-as-directory
            (bar355-test-owned-path root "no-remote")))
          (no-remote
           (bar355-test-make-repo
            no-remote-root "git@github.com:acme/no-remote.git"))
          (no-remote-repo (plist-get no-remote :repo)))
     (bar355-test-git no-remote-repo "remote" "remove" "origin")
     (let ((buffer (bar355-test-visit (plist-get no-remote :demo))))
       (switch-to-buffer buffer)
       (goto-char (point-min))
       (forward-line 1)
       (let ((no-remote-call
              (bar355-test-traced
               root (lambda () (browse-at-remote-get-url)))))
         (let* ((enterprise-root
                 (file-name-as-directory
                  (bar355-test-owned-path root "enterprise")))
                (enterprise
                 (bar355-test-make-repo
                  enterprise-root
                  "git@unknown.example.invalid:acme/project.git"
                  "topic/deep"))
                (repo (plist-get enterprise :repo))
                (file (plist-get enterprise :demo))
                (enterprise-buffer (bar355-test-visit file)))
           (switch-to-buffer enterprise-buffer)
           (goto-char (point-min))
           (forward-line 1)
           (let* ((before
                   (list :bytes (buffer-string) :point (point)
                         :mark-active mark-active
                         :kill-ring (copy-tree kill-ring)))
                  (unknown
                   (bar355-test-traced
                    root (lambda () (browse-at-remote-get-url)))))
             (bar355-test-config repo "browseAtRemote.type" "github")
             (bar355-test-config repo "browseAtRemote.actualHost" "github.com")
             (let ((enterprise-recovery
                    (bar355-test-traced
                     root (lambda () (browse-at-remote-get-url)))))
               (bar355-test-config repo "browseAtRemote.type" "unsupported")
               (bar355-test-unset-config repo "browseAtRemote.actualHost")
               (let ((unsupported-type
                      (bar355-test-traced
                       root (lambda () (browse-at-remote-get-url)))))
                 (bar355-test-git repo "remote" "set-url" "origin"
                                  "https://gitlab.com:8443/acme/project.git")
                 (bar355-test-unset-config repo "browseAtRemote.type")
                 (let ((ported-host
                        (bar355-test-traced
                         root (lambda () (browse-at-remote-get-url)))))
                   (bar355-test-config repo "browseAtRemote.type" "gitlab")
                   (let ((ported-recovery
                          (bar355-test-traced
                           root (lambda () (browse-at-remote-get-url)))))
                     (bar355-test-git repo "remote" "set-url" "origin"
                                      "git@plain.example.invalid:acme/project.git")
                     (bar355-test-config repo "browseAtRemote.type" "github")
                     (let* ((browse-at-remote-use-http
                             '("plain.example.invalid"))
                            (forced-http
                             (bar355-test-traced
                              root (lambda () (browse-at-remote-get-url)))))
                       (bar355-test-git repo "remote" "set-url" "origin"
                                        "git@github.com:acme/project.git")
                       (bar355-test-unset-config repo "browseAtRemote.type")
                       (setq bar355-test-browser-plan
                             (list
                              (list
                               :url
                               "https://github.com/acme/project/blob/topic/deep/src/demo.el#L2"
                               :args '(nil)
                               :signal '(error "owned browser unavailable"))))
                       (let* ((browser-failure
                               (bar355-test-traced
                                root
                                (lambda ()
                                  (call-interactively #'browse-at-remote))))
                              (failure-event
                               (copy-tree (car bar355-test-browser-events))))
                         (setq bar355-test-browser-events nil)
                         (setq bar355-test-browser-plan
                               (list
                                (list
                                 :url
                                 "https://github.com/acme/project/blob/topic/deep/src/demo.el#L2"
                                 :args '(nil) :return 'browser-recovered)))
                         (let* ((browser-recovery
                                 (bar355-test-traced
                                  root
                                  (lambda ()
                                    (call-interactively #'browse-at-remote))))
                                (recovery-event
                                 (copy-tree (car bar355-test-browser-events)))
                                (after
                                 (list
                                  :bytes (buffer-string) :point (point)
                                  :mark-active mark-active
                                  :modified (buffer-modified-p)
                                  :kill-ring (copy-tree kill-ring)
                                  :git-status
                                  (bar355-test-git-stdout
                                   repo "status" "--porcelain"))))
                           (list
                            :unsupported-buffer unsupported-buffer
                            :no-remote no-remote-call
                            :before before
                            :unknown-host unknown
                            :enterprise-recovery enterprise-recovery
                            :unsupported-type unsupported-type
                            :ported-host ported-host
                            :ported-recovery ported-recovery
                            :forced-http forced-http
                            :browser-failure browser-failure
                            :failure-event failure-event
                            :browser-recovery browser-recovery
                            :recovery-event recovery-event
                            :after after)))))))))))))))
"####;
    ParityBatchCase::value(
        "public-failures-preserve-state-and-recover-in-process",
        elisp_form,
        expect![[
            r#"OK (:result (:unsupported-buffer (:call (:outcome (:signal error :data ("Sorry, I’m not sure what to do with this.") :message "Sorry, I’m not sure what to do with this.") :git nil) :before (:bytes "ordinary scratch bytes" :point 7) :after (:bytes "ordinary scratch bytes" :point 7)) :no-remote (:outcome (:signal wrong-type-argument :data (stringp nil) :message "Wrong type argument: stringp, nil") :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "no-remote/repo" :exit 0) (:argv ("git" "--no-pager" "config" "branch.main.pushRemote") :worktree "no-remote/repo" :exit 1) (:argv ("git" "--no-pager" "rev-parse" "--symbolic-full-name" "--abbrev-ref" "main@{upstream}") :worktree "no-remote/repo" :exit 128) (:argv ("git" "--no-pager" "remote") :worktree "no-remote/repo" :exit 0))) :before (:bytes ";;; demo.el\n(defun demo ()\n  \"hello\")\n" :point 13 :mark-active nil :kill-ring nil) :unknown-host (:outcome (:signal error :data ("Sorry, not sure what to do with host ‘unknown.example.invalid’ (consider adding it to ‘browse-at-remote-remote-type-regexps’)") :message "Sorry, not sure what to do with host ‘unknown.example.invalid’ (consider adding it to ‘browse-at-remote-remote-type-regexps’)") :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "config" "branch.topic/deep.pushRemote") :worktree "enterprise/repo" :exit 1) (:argv ("git" "--no-pager" "rev-parse" "--symbolic-full-name" "--abbrev-ref" "topic/deep@{upstream}") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "origin") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "enterprise/repo" :exit 1) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "enterprise/repo" :exit 1))) :enterprise-recovery (:outcome (:value "https://github.com/acme/project/blob/topic/deep/src/demo.el#L2") :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "config" "branch.topic/deep.pushRemote") :worktree "enterprise/repo" :exit 1) (:argv ("git" "--no-pager" "rev-parse" "--symbolic-full-name" "--abbrev-ref" "topic/deep@{upstream}") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "origin") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "enterprise/repo" :exit 0))) :unsupported-type (:outcome (:signal error :data ("Origin repo parsing failed: https://unknown.example.invalid/acme/project") :message "Origin repo parsing failed: https://unknown.example.invalid/acme/project") :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "config" "branch.topic/deep.pushRemote") :worktree "enterprise/repo" :exit 1) (:argv ("git" "--no-pager" "rev-parse" "--symbolic-full-name" "--abbrev-ref" "topic/deep@{upstream}") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "origin") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "enterprise/repo" :exit 1) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "enterprise/repo" :exit 0))) :ported-host (:outcome (:signal error :data ("Sorry, not sure what to do with host ‘gitlab.com:8443’ (consider adding it to ‘browse-at-remote-remote-type-regexps’)") :message "Sorry, not sure what to do with host ‘gitlab.com:8443’ (consider adding it to ‘browse-at-remote-remote-type-regexps’)") :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "config" "branch.topic/deep.pushRemote") :worktree "enterprise/repo" :exit 1) (:argv ("git" "--no-pager" "rev-parse" "--symbolic-full-name" "--abbrev-ref" "topic/deep@{upstream}") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "origin") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "enterprise/repo" :exit 1) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "enterprise/repo" :exit 1))) :ported-recovery (:outcome (:value "https://gitlab.com:8443/acme/project/blob/topic/deep/src/demo.el#L2") :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "config" "branch.topic/deep.pushRemote") :worktree "enterprise/repo" :exit 1) (:argv ("git" "--no-pager" "rev-parse" "--symbolic-full-name" "--abbrev-ref" "topic/deep@{upstream}") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "origin") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "enterprise/repo" :exit 1) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "enterprise/repo" :exit 0))) :forced-http (:outcome (:value "http://plain.example.invalid/acme/project/blob/topic/deep/src/demo.el#L2") :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "config" "branch.topic/deep.pushRemote") :worktree "enterprise/repo" :exit 1) (:argv ("git" "--no-pager" "rev-parse" "--symbolic-full-name" "--abbrev-ref" "topic/deep@{upstream}") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "origin") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "enterprise/repo" :exit 1) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "enterprise/repo" :exit 0))) :browser-failure (:outcome (:signal error :data ("owned browser unavailable") :message "owned browser unavailable") :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "config" "branch.topic/deep.pushRemote") :worktree "enterprise/repo" :exit 1) (:argv ("git" "--no-pager" "rev-parse" "--symbolic-full-name" "--abbrev-ref" "topic/deep@{upstream}") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "origin") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "enterprise/repo" :exit 1) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "enterprise/repo" :exit 1))) :failure-event (:url "https://github.com/acme/project/blob/topic/deep/src/demo.el#L2" :args (nil) :mode emacs-lisp-mode :buffer "demo.el<enterprise>" :file "enterprise/repo/src/demo.el" :directory "enterprise/repo/src/") :browser-recovery (:outcome (:value browser-recovered) :git ((:argv ("git" "--no-pager" "symbolic-ref" "HEAD") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "config" "branch.topic/deep.pushRemote") :worktree "enterprise/repo" :exit 1) (:argv ("git" "--no-pager" "rev-parse" "--symbolic-full-name" "--abbrev-ref" "topic/deep@{upstream}") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "ls-remote" "--get-url" "origin") :worktree "enterprise/repo" :exit 0) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.actualHost") :worktree "enterprise/repo" :exit 1) (:argv ("git" "--no-pager" "config" "--get" "browseAtRemote.type") :worktree "enterprise/repo" :exit 1))) :recovery-event (:url "https://github.com/acme/project/blob/topic/deep/src/demo.el#L2" :args (nil) :mode emacs-lisp-mode :buffer "demo.el<enterprise>" :file "enterprise/repo/src/demo.el" :directory "enterprise/repo/src/") :after (:bytes ";;; demo.el\n(defun demo ()\n  \"hello\")\n" :point 13 :mark-active nil :modified nil :kill-ring nil :git-status "")) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :owned-buffer-live nil :owned-timer-live nil :root-exists nil :root-owned nil :current-buffer-restored t :window-restored t :dired-buffers-restored t :vc-annotate-display-restored t :kill-ring-restored t :kill-yank-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        file_region_browser_and_clipboard_preserve_real_state(),
        real_remote_and_ref_precedence_uses_git_configuration(),
        public_provider_and_enterprise_routes_preserve_exact_url_syntax(),
        real_dired_tree_and_vc_annotation_open_public_urls(),
        public_failures_preserve_state_and_recover_in_process(),
    ]
}

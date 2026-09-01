use expect_test::expect;

use super::ParityBatchCase;

fn dependency_graph_builds_installs_and_runs_composed_packages() -> ParityBatchCase {
    ParityBatchCase::value(
        "dependency_graph_builds_installs_and_runs_composed_packages",
        r####"
(neomacs-quelpa-test-in-sandbox
 "dependency-graph"
 (lambda (root)
   (let* ((helper-source
           ";;; qpt-helper.el --- Helper service -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-helper-render (project item)\n  (format \"%s :: %s\" (upcase project) item))\n(provide 'qpt-helper)\n;;; qpt-helper.el ends here\n")
          (app-source
           ";;; qpt-app.el --- Composed release report -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Requires: ((emacs \"25.1\") (qpt-helper \"1.0\"))\n;;; Code:\n(require 'qpt-helper)\n(require 'subr-x)\n(defun qpt-app-report (project items)\n  (let ((asset (expand-file-name \"assets/release-template.txt\"\n                                 (file-name-directory (locate-library \"qpt-app\")))))\n    (with-temp-buffer\n      (insert-file-contents asset)\n      (format \"%s => %s\"\n              (string-trim (buffer-string))\n              (mapconcat (lambda (item) (qpt-helper-render project item))\n                         items \" | \")))))\n(provide 'qpt-app)\n;;; qpt-app.el ends here\n")
          (helper (neomacs-quelpa-test-repository
                   root "repositories/helper" `(("qpt-helper.el" . ,helper-source))
                   "2024-02-03T12:34:56+0000"))
          (app (neomacs-quelpa-test-repository
                root "repositories/app"
                `(("qpt-app.el" . ,app-source)
                  ("assets/release-template.txt" . "Release bundle Ω\n"))
                "2024-02-04T13:45:01+0000"))
          (helper-recipe
           (neomacs-quelpa-test-recipe 'qpt-helper (car helper)))
          (app-recipe
           (neomacs-quelpa-test-recipe
            'qpt-app (car app)
            :files '("qpt-app.el" ("assets" "assets/*.txt"))))
          (recipe-store (neomacs-quelpa-test-path root "recipes")))
     (make-directory recipe-store t)
     (neomacs-quelpa-test-write
      recipe-store "qpt-helper" (concat (prin1-to-string helper-recipe) "\n"))
     (neomacs-quelpa-test-write
      recipe-store "qpt-app" (concat (prin1-to-string app-recipe) "\n"))
     (setq quelpa-melpa-recipe-stores (list recipe-store))
     (let ((package-install-file-original
            (symbol-function 'package-install-file))
           artifact-descriptions
           result)
       (cl-letf (((symbol-function 'package-install-file)
                  (lambda (file &rest arguments)
                    (let ((buffer (find-file-noselect file t)))
                      (unwind-protect
                          (with-current-buffer buffer
                            (push
                             (neomacs-quelpa-test-normalize-description
                              (if (derived-mode-p 'tar-mode)
                                  (package-tar-file-info)
                                (package-buffer-info)))
                             artifact-descriptions))
                        (kill-buffer buffer)))
                    (apply package-install-file-original file arguments))))
         (setq result (quelpa 'qpt-app)))
       (require 'qpt-app)
       (list
          :result result
          :runtime (qpt-app-report "release α" '("compile" "ship Ω"))
          :versions (list (neomacs-quelpa-test-version 'qpt-helper)
                          (neomacs-quelpa-test-version 'qpt-app))
          :descriptors (list
                        (neomacs-quelpa-test-descriptor 'qpt-helper)
                        (neomacs-quelpa-test-descriptor 'qpt-app))
          :artifact-descriptors (nreverse artifact-descriptions)
          :installed (list (package-installed-p 'qpt-helper '(1 0))
                           (package-installed-p 'qpt-app))
          :helper-files (neomacs-quelpa-test-installed-tree 'qpt-helper)
          :app-files (neomacs-quelpa-test-installed-tree 'qpt-app)
          :payloads
          (list
           :helper-installed
           (neomacs-quelpa-test-installed-source 'qpt-helper "qpt-helper.el")
           :helper-build
           (neomacs-quelpa-test-read-file
            (expand-file-name "qpt-helper.el"
                              (expand-file-name "qpt-helper" quelpa-build-dir)))
           :app-installed
           (neomacs-quelpa-test-installed-source 'qpt-app "qpt-app.el")
           :asset-installed
           (neomacs-quelpa-test-installed-source
            'qpt-app "assets/release-template.txt")
           :app-build
           (neomacs-quelpa-test-read-file
            (expand-file-name "qpt-app.el"
                              (expand-file-name "qpt-app" quelpa-build-dir)))
           :asset-build
           (neomacs-quelpa-test-read-file
            (expand-file-name
             "assets/release-template.txt"
             (expand-file-name "qpt-app" quelpa-build-dir))))
          :selected (neomacs-quelpa-test-selected-fixtures)
          :build-heads (list
                        (neomacs-quelpa-test-build-head-matches-p
                         'qpt-helper (cdr helper))
                        (neomacs-quelpa-test-build-head-matches-p
                         'qpt-app (cdr app)))
          :recipe-store
          (mapcar
           (lambda (name)
             (with-temp-buffer
               (insert-file-contents-literally
                (expand-file-name name recipe-store))
               (read (current-buffer))))
           '("qpt-app" "qpt-helper"))
          :cache (neomacs-quelpa-test-cache-summary)
          :archives-cleaned (not (file-exists-p quelpa-packages-dir))
          :live-processes (neomacs-quelpa-test-live-processes))))))
"####,
        expect![[
            r#"OK (:result nil :runtime "Release bundle Ω => RELEASE Α :: compile | RELEASE Α :: ship Ω" :versions ("20240203.123456" "20240204.134501") :descriptors ((:name qpt-helper :version "20240203.123456" :summary "Helper service" :requirements ((emacs (25 1))) :kind nil) (:name qpt-app :version "20240204.134501" :summary "Composed release report" :requirements ((emacs (25 1)) (qpt-helper (1 0))) :kind nil)) :artifact-descriptors ((:name qpt-helper :version "20240203.123456" :summary "Helper service" :requirements ((emacs (25 1))) :kind single) (:name qpt-app :version "20240204.134501" :summary "Composed release report" :requirements ((emacs (25 1)) (qpt-helper (1 0))) :kind tar)) :installed (t t) :helper-files ("qpt-helper-autoloads.el" "qpt-helper-pkg.el" "qpt-helper.el" "qpt-helper.elc") :app-files ("assets/release-template.txt" "qpt-app-autoloads.el" "qpt-app-pkg.el" "qpt-app.el" "qpt-app.elc") :payloads (:helper-installed ";;; qpt-helper.el --- Helper service -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Version: 20240203.123456\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-helper-render (project item)\n  (format \"%s :: %s\" (upcase project) item))\n(provide 'qpt-helper)\n;;; qpt-helper.el ends here\n" :helper-build ";;; qpt-helper.el --- Helper service -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-helper-render (project item)\n  (format \"%s :: %s\" (upcase project) item))\n(provide 'qpt-helper)\n;;; qpt-helper.el ends here\n" :app-installed ";;; qpt-app.el --- Composed release report -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Requires: ((emacs \"25.1\") (qpt-helper \"1.0\"))\n;;; Code:\n(require 'qpt-helper)\n(require 'subr-x)\n(defun qpt-app-report (project items)\n  (let ((asset (expand-file-name \"assets/release-template.txt\"\n                                 (file-name-directory (locate-library \"qpt-app\")))))\n    (with-temp-buffer\n      (insert-file-contents asset)\n      (format \"%s => %s\"\n              (string-trim (buffer-string))\n              (mapconcat (lambda (item) (qpt-helper-render project item))\n                         items \" | \")))))\n(provide 'qpt-app)\n;;; qpt-app.el ends here\n" :asset-installed "Release bundle Ω\n" :app-build ";;; qpt-app.el --- Composed release report -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Requires: ((emacs \"25.1\") (qpt-helper \"1.0\"))\n;;; Code:\n(require 'qpt-helper)\n(require 'subr-x)\n(defun qpt-app-report (project items)\n  (let ((asset (expand-file-name \"assets/release-template.txt\"\n                                 (file-name-directory (locate-library \"qpt-app\")))))\n    (with-temp-buffer\n      (insert-file-contents asset)\n      (format \"%s => %s\"\n              (string-trim (buffer-string))\n              (mapconcat (lambda (item) (qpt-helper-render project item))\n                         items \" | \")))))\n(provide 'qpt-app)\n;;; qpt-app.el ends here\n" :asset-build "Release bundle Ω\n") :selected (qpt-app qpt-helper) :build-heads (t t) :recipe-store ((qpt-app :fetcher git :url "file://[ORACLE-SANDBOX]/quelpa-workflows/dependency-graph/repositories/app" :branch "main" :depth 1 :files ("qpt-app.el" ("assets" "assets/*.txt"))) (qpt-helper :fetcher git :url "file://[ORACLE-SANDBOX]/quelpa-workflows/dependency-graph/repositories/helper" :branch "main" :depth 1)) :cache (:live ((qpt-app :fetcher git :url "file://[ORACLE-SANDBOX]/quelpa-workflows/dependency-graph/repositories/app" :branch "main" :depth 1 :files ("qpt-app.el" ("assets" "assets/*.txt")))) :disk ((qpt-app :fetcher git :url "file://[ORACLE-SANDBOX]/quelpa-workflows/dependency-graph/repositories/app" :branch "main" :depth 1 :files ("qpt-app.el" ("assets" "assets/*.txt")))) :raw "((qpt-app :fetcher git :url \"file://[ORACLE-SANDBOX]/quelpa-workflows/dependency-graph/repositories/app\" :branch \"main\" :depth 1 :files (\"qpt-app.el\" (\"assets\" \"assets/*.txt\"))))" :same t) :archives-cleaned t :live-processes nil)"#
        ]],
    )
}

fn direct_upgrade_reloads_behavior_removes_old_release_and_persists_recipe() -> ParityBatchCase {
    ParityBatchCase::value(
        "direct_upgrade_reloads_behavior_removes_old_release_and_persists_recipe",
        r####"
(neomacs-quelpa-test-in-sandbox
 "direct-upgrade"
 (lambda (root)
   (let* ((v1
           ";;; qpt-upgrade.el --- Upgrade fixture -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-upgrade-release () '(:channel stable :revision 1 :message \"ready α\"))\n(provide 'qpt-upgrade)\n;;; qpt-upgrade.el ends here\n")
          (v2
           ";;; qpt-upgrade.el --- Upgrade fixture -*- lexical-binding: t; -*-\n;; Version: 2.0\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-upgrade-release () '(:channel stable :revision 2 :message \"ready Ω\"))\n(provide 'qpt-upgrade)\n;;; qpt-upgrade.el ends here\n")
          (repository
           (neomacs-quelpa-test-repository
            root "repositories/upgrade" `(("qpt-upgrade.el" . ,v1))
            "2024-03-01T12:00:01+0000"))
          (upgrade-recipe
           (neomacs-quelpa-test-recipe 'qpt-upgrade (car repository)))
          (before-count 0)
          (after-count 0)
          (quelpa-before-hook (list (lambda () (setq before-count (1+ before-count)))))
          (quelpa-after-hook (list (lambda () (setq after-count (1+ after-count))))))
     (quelpa upgrade-recipe)
     (require 'qpt-upgrade)
     (let* ((v1-value (qpt-upgrade-release))
            (v1-descriptor (neomacs-quelpa-test-descriptor 'qpt-upgrade))
            (v1-installed-source
             (neomacs-quelpa-test-installed-source
              'qpt-upgrade "qpt-upgrade.el"))
            (v1-directory
             (package-desc-dir (neomacs-quelpa-test-description 'qpt-upgrade)))
            (v1-head
             (neomacs-quelpa-test-build-head-matches-p
              'qpt-upgrade (cdr repository)))
            (v2-sha
             (neomacs-quelpa-test-advance
              (car repository) `(("qpt-upgrade.el" . ,v2))
              "2024-03-02T13:00:02+0000" "release v2")))
       (quelpa upgrade-recipe)
       (let* ((ordinary (list :descriptor
                              (neomacs-quelpa-test-descriptor 'qpt-upgrade)
                              :runtime (qpt-upgrade-release)
                              :installed-source
                              (neomacs-quelpa-test-installed-source
                               'qpt-upgrade "qpt-upgrade.el")
                              :build-source
                              (neomacs-quelpa-test-read-file
                               (expand-file-name
                                "qpt-upgrade.el"
                                (expand-file-name
                                 "qpt-upgrade" quelpa-build-dir)))
                              :head (neomacs-quelpa-test-build-head-matches-p
                                     'qpt-upgrade (cdr repository))))
              (checkout (expand-file-name "qpt-upgrade" quelpa-build-dir)))
         (neomacs-quelpa-test-write
          checkout "qpt-upgrade.el"
          ";;; locally modified checkout; force upgrade must discard this\n")
         (let ((dirty-before-force
                (neomacs-quelpa-test-git checkout "status" "--short")))
           (quelpa-upgrade upgrade-recipe 'force)
           (let ((live-before-reload (qpt-upgrade-release))
                 (installed-v2
                  (neomacs-quelpa-test-installed-source
                   'qpt-upgrade "qpt-upgrade.el"))
                 (build-v2
                  (neomacs-quelpa-test-read-file
                   (expand-file-name "qpt-upgrade.el" checkout))))
             (unload-feature 'qpt-upgrade t)
             (require 'qpt-upgrade)
             (list
              :v1 (list :descriptor v1-descriptor
                        :runtime v1-value
                        :installed-source v1-installed-source
                        :head v1-head)
              :ordinary ordinary
              :dirty-before-force dirty-before-force
              :forced (list :descriptor
                            (neomacs-quelpa-test-descriptor 'qpt-upgrade)
                            :installed-source installed-v2
                            :build-source build-v2
                            :live-before-reload live-before-reload
                            :live-after-reload (qpt-upgrade-release)
                            :head (neomacs-quelpa-test-build-head-matches-p
                                   'qpt-upgrade v2-sha)
                            :clean (string-empty-p
                                    (neomacs-quelpa-test-git
                                     checkout "status" "--short")))
              :old-removed (not (file-exists-p v1-directory))
              :installed-files (neomacs-quelpa-test-installed-tree 'qpt-upgrade)
              :selected (neomacs-quelpa-test-selected-fixtures)
              :hooks (list before-count after-count)
              :cache (neomacs-quelpa-test-cache-summary)
              :archives-cleaned (not (file-exists-p quelpa-packages-dir))
              :live-processes (neomacs-quelpa-test-live-processes)))))))))
"####,
        expect![[
            r#"OK (:v1 (:descriptor (:name qpt-upgrade :version "20240301.120001" :summary "Upgrade fixture" :requirements ((emacs (25 1))) :kind nil) :runtime (:channel stable :revision 1 :message "ready α") :installed-source ";;; qpt-upgrade.el --- Upgrade fixture -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Version: 20240301.120001\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-upgrade-release () '(:channel stable :revision 1 :message \"ready α\"))\n(provide 'qpt-upgrade)\n;;; qpt-upgrade.el ends here\n" :head t) :ordinary (:descriptor (:name qpt-upgrade :version "20240301.120001" :summary "Upgrade fixture" :requirements ((emacs (25 1))) :kind nil) :runtime (:channel stable :revision 1 :message "ready α") :installed-source ";;; qpt-upgrade.el --- Upgrade fixture -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Version: 20240301.120001\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-upgrade-release () '(:channel stable :revision 1 :message \"ready α\"))\n(provide 'qpt-upgrade)\n;;; qpt-upgrade.el ends here\n" :build-source ";;; qpt-upgrade.el --- Upgrade fixture -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-upgrade-release () '(:channel stable :revision 1 :message \"ready α\"))\n(provide 'qpt-upgrade)\n;;; qpt-upgrade.el ends here\n" :head t) :dirty-before-force " M qpt-upgrade.el" :forced (:descriptor (:name qpt-upgrade :version "20240302.130002" :summary "Upgrade fixture" :requirements ((emacs (25 1))) :kind nil) :installed-source ";;; qpt-upgrade.el --- Upgrade fixture -*- lexical-binding: t; -*-\n;; Version: 2.0\n;; Package-Version: 20240302.130002\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-upgrade-release () '(:channel stable :revision 2 :message \"ready Ω\"))\n(provide 'qpt-upgrade)\n;;; qpt-upgrade.el ends here\n" :build-source ";;; qpt-upgrade.el --- Upgrade fixture -*- lexical-binding: t; -*-\n;; Version: 2.0\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-upgrade-release () '(:channel stable :revision 2 :message \"ready Ω\"))\n(provide 'qpt-upgrade)\n;;; qpt-upgrade.el ends here\n" :live-before-reload (:channel stable :revision 2 :message "ready Ω") :live-after-reload (:channel stable :revision 2 :message "ready Ω") :head t :clean t) :old-removed t :installed-files ("qpt-upgrade-autoloads.el" "qpt-upgrade-pkg.el" "qpt-upgrade.el" "qpt-upgrade.elc") :selected (qpt-upgrade) :hooks (3 3) :cache (:live ((qpt-upgrade :fetcher git :url "file://[ORACLE-SANDBOX]/quelpa-workflows/direct-upgrade/repositories/upgrade" :branch "main" :depth 1)) :disk ((qpt-upgrade :fetcher git :url "file://[ORACLE-SANDBOX]/quelpa-workflows/direct-upgrade/repositories/upgrade" :branch "main" :depth 1)) :raw "((qpt-upgrade :fetcher git :url \"file://[ORACLE-SANDBOX]/quelpa-workflows/direct-upgrade/repositories/upgrade\" :branch \"main\" :depth 1))" :same t) :archives-cleaned t :live-processes nil)"#
        ]],
    )
}

fn persisted_cache_drives_upgrade_all_for_every_recorded_repository() -> ParityBatchCase {
    ParityBatchCase::value(
        "persisted_cache_drives_upgrade_all_for_every_recorded_repository",
        r####"
(neomacs-quelpa-test-in-sandbox
 "upgrade-all"
 (lambda (root)
   (let* ((a1 ";;; qpt-fleet-a.el --- Fleet A -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-fleet-a-status () '(a 1 \"alpha\"))\n(provide 'qpt-fleet-a)\n;;; qpt-fleet-a.el ends here\n")
          (a2 ";;; qpt-fleet-a.el --- Fleet A -*- lexical-binding: t; -*-\n;; Version: 2.0\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-fleet-a-status () '(a 2 \"omega\"))\n(provide 'qpt-fleet-a)\n;;; qpt-fleet-a.el ends here\n")
          (b1 ";;; qpt-fleet-b.el --- Fleet B -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-fleet-b-status () '(b 1 \"queued\"))\n(provide 'qpt-fleet-b)\n;;; qpt-fleet-b.el ends here\n")
          (b2 ";;; qpt-fleet-b.el --- Fleet B -*- lexical-binding: t; -*-\n;; Version: 2.0\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-fleet-b-status () '(b 2 \"deployed\"))\n(provide 'qpt-fleet-b)\n;;; qpt-fleet-b.el ends here\n")
          (repo-a (neomacs-quelpa-test-repository
                   root "repositories/fleet-a" `(("qpt-fleet-a.el" . ,a1))
                   "2024-04-01T12:10:01+0000"))
          (repo-b (neomacs-quelpa-test-repository
                   root "repositories/fleet-b" `(("qpt-fleet-b.el" . ,b1))
                   "2024-04-01T13:20:02+0000"))
          (recipe-a (neomacs-quelpa-test-recipe 'qpt-fleet-a (car repo-a)))
          (recipe-b (neomacs-quelpa-test-recipe 'qpt-fleet-b (car repo-b))))
     (quelpa recipe-a)
     (quelpa recipe-b)
     (let* ((old-a (package-desc-dir
                    (neomacs-quelpa-test-description 'qpt-fleet-a)))
            (old-b (package-desc-dir
                    (neomacs-quelpa-test-description 'qpt-fleet-b)))
            (disk-before (neomacs-quelpa-test-cache-on-disk))
            (sha-a (neomacs-quelpa-test-advance
                    (car repo-a) `(("qpt-fleet-a.el" . ,a2))
                    "2024-04-02T14:30:03+0000" "fleet a v2"))
            (sha-b (neomacs-quelpa-test-advance
                    (car repo-b) `(("qpt-fleet-b.el" . ,b2))
                    "2024-04-02T15:40:04+0000" "fleet b v2")))
       (setq quelpa-cache nil
             quelpa-initialized-p nil)
       (quelpa-upgrade-all)
       (require 'qpt-fleet-a)
       (require 'qpt-fleet-b)
       (list
        :disk-before disk-before
        :descriptors
        (list (neomacs-quelpa-test-descriptor 'qpt-fleet-a)
              (neomacs-quelpa-test-descriptor 'qpt-fleet-b))
        :runtime (list (qpt-fleet-a-status) (qpt-fleet-b-status))
        :installed-trees
        (list (neomacs-quelpa-test-installed-tree 'qpt-fleet-a)
              (neomacs-quelpa-test-installed-tree 'qpt-fleet-b))
        :payloads
        (list
         :a-installed
         (neomacs-quelpa-test-installed-source 'qpt-fleet-a "qpt-fleet-a.el")
         :a-build
         (neomacs-quelpa-test-read-file
          (expand-file-name
           "qpt-fleet-a.el" (expand-file-name "qpt-fleet-a" quelpa-build-dir)))
         :b-installed
         (neomacs-quelpa-test-installed-source 'qpt-fleet-b "qpt-fleet-b.el")
         :b-build
         (neomacs-quelpa-test-read-file
          (expand-file-name
           "qpt-fleet-b.el" (expand-file-name "qpt-fleet-b" quelpa-build-dir))))
        :selected (neomacs-quelpa-test-selected-fixtures)
        :old-removed (list (not (file-exists-p old-a))
                           (not (file-exists-p old-b)))
        :build-heads (list
                      (neomacs-quelpa-test-build-head-matches-p
                       'qpt-fleet-a sha-a)
                      (neomacs-quelpa-test-build-head-matches-p
                       'qpt-fleet-b sha-b))
        :cache (neomacs-quelpa-test-cache-summary)
        :self-upgrade quelpa-self-upgrade-p
        :archives-cleaned (not (file-exists-p quelpa-packages-dir))
        :live-processes (neomacs-quelpa-test-live-processes))))))
"####,
        expect![[
            r#"OK (:disk-before ((qpt-fleet-a :fetcher git :url "file://[ORACLE-SANDBOX]/quelpa-workflows/upgrade-all/repositories/fleet-a" :branch "main" :depth 1) (qpt-fleet-b :fetcher git :url "file://[ORACLE-SANDBOX]/quelpa-workflows/upgrade-all/repositories/fleet-b" :branch "main" :depth 1)) :descriptors ((:name qpt-fleet-a :version "20240402.143003" :summary "Fleet A" :requirements ((emacs (25 1))) :kind nil) (:name qpt-fleet-b :version "20240402.154004" :summary "Fleet B" :requirements ((emacs (25 1))) :kind nil)) :runtime ((a 2 "omega") (b 2 "deployed")) :installed-trees (("qpt-fleet-a-autoloads.el" "qpt-fleet-a-pkg.el" "qpt-fleet-a.el" "qpt-fleet-a.elc") ("qpt-fleet-b-autoloads.el" "qpt-fleet-b-pkg.el" "qpt-fleet-b.el" "qpt-fleet-b.elc")) :payloads (:a-installed ";;; qpt-fleet-a.el --- Fleet A -*- lexical-binding: t; -*-\n;; Version: 2.0\n;; Package-Version: 20240402.143003\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-fleet-a-status () '(a 2 \"omega\"))\n(provide 'qpt-fleet-a)\n;;; qpt-fleet-a.el ends here\n" :a-build ";;; qpt-fleet-a.el --- Fleet A -*- lexical-binding: t; -*-\n;; Version: 2.0\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-fleet-a-status () '(a 2 \"omega\"))\n(provide 'qpt-fleet-a)\n;;; qpt-fleet-a.el ends here\n" :b-installed ";;; qpt-fleet-b.el --- Fleet B -*- lexical-binding: t; -*-\n;; Version: 2.0\n;; Package-Version: 20240402.154004\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-fleet-b-status () '(b 2 \"deployed\"))\n(provide 'qpt-fleet-b)\n;;; qpt-fleet-b.el ends here\n" :b-build ";;; qpt-fleet-b.el --- Fleet B -*- lexical-binding: t; -*-\n;; Version: 2.0\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-fleet-b-status () '(b 2 \"deployed\"))\n(provide 'qpt-fleet-b)\n;;; qpt-fleet-b.el ends here\n") :selected (qpt-fleet-a qpt-fleet-b) :old-removed (t t) :build-heads (t t) :cache (:live ((qpt-fleet-a :fetcher git :url "file://[ORACLE-SANDBOX]/quelpa-workflows/upgrade-all/repositories/fleet-a" :branch "main" :depth 1) (qpt-fleet-b :fetcher git :url "file://[ORACLE-SANDBOX]/quelpa-workflows/upgrade-all/repositories/fleet-b" :branch "main" :depth 1)) :disk ((qpt-fleet-a :fetcher git :url "file://[ORACLE-SANDBOX]/quelpa-workflows/upgrade-all/repositories/fleet-a" :branch "main" :depth 1) (qpt-fleet-b :fetcher git :url "file://[ORACLE-SANDBOX]/quelpa-workflows/upgrade-all/repositories/fleet-b" :branch "main" :depth 1)) :raw "((qpt-fleet-a :fetcher git :url \"file://[ORACLE-SANDBOX]/quelpa-workflows/upgrade-all/repositories/fleet-a\" :branch \"main\" :depth 1) (qpt-fleet-b :fetcher git :url \"file://[ORACLE-SANDBOX]/quelpa-workflows/upgrade-all/repositories/fleet-b\" :branch \"main\" :depth 1))" :same t) :self-upgrade nil :archives-cleaned t :live-processes nil)"#
        ]],
    )
}

fn asynchronous_git_install_completes_package_and_process_lifecycle() -> ParityBatchCase {
    ParityBatchCase::value(
        "asynchronous_git_install_completes_package_and_process_lifecycle",
        r####"
(neomacs-quelpa-test-in-sandbox
 "async-command-loop"
 (lambda (root)
   (let* ((source
           ";;; qpt-async.el --- Async deployment -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-async-deploy (environment artifacts)\n  (list :environment environment :artifacts artifacts :count (length artifacts)))\n(provide 'qpt-async)\n;;; qpt-async.el ends here\n")
          (repository
           (neomacs-quelpa-test-repository
            root "repositories/async"
            (list (cons "qpt-async.el" source))
            "2024-05-03T16:50:05+0000"))
          (recipe (neomacs-quelpa-test-recipe 'qpt-async (car repository)))
          (real-git (executable-find "git"))
          (wrapper-directory (neomacs-quelpa-test-path root "git-gate/bin"))
          (wrapper (neomacs-quelpa-test-path wrapper-directory "git"))
          (trace (neomacs-quelpa-test-path root "git-gate/trace"))
          (miss (neomacs-quelpa-test-path root "git-gate/miss"))
          (plan-state (neomacs-quelpa-test-path root "git-gate/plan-state"))
          (started (neomacs-quelpa-test-path root "git-gate/started"))
          (release (neomacs-quelpa-test-path root "git-gate/release"))
          (checkout (expand-file-name "qpt-async" quelpa-build-dir))
          (checkout-slash (file-name-as-directory checkout))
          (workspace
           (directory-file-name (getenv "NEOMACS_TEST_WORKSPACE_ROOT")))
          (artifact
           (expand-file-name
            "qpt-async-20240503.165005.el" quelpa-packages-dir))
          (artifact-argument (file-relative-name artifact workspace))
          (base-recursion-depth (recursion-depth))
          (timer-fired nil)
          (live-process-witnessed nil)
          (during-recursion-depth nil)
          (observed-process nil)
          (completed nil)
          timer)
     (make-directory wrapper-directory t)
     (neomacs-quelpa-test-write
      wrapper-directory "git"
      (format
       (concat
        "#!/bin/sh\nset -eu\n"
        "real_git=%s\n"
        "trace=%s\n"
        "miss=%s\n"
        "state=%s\n"
        "started=%s\n"
        "release=%s\n"
        "build=%s\n"
        "checkout=%s\n"
        "checkout_slash=%s\n"
        "repo=%s\n"
        "workspace=%s\n"
        "artifact=%s\n"
        "step=0\n"
        "if [ -f \"$state\" ]; then IFS= read -r step < \"$state\"; fi\n"
        "record() { { printf 'step=%%s|cwd=%%s|lc=%%s|terminal=%%s|protocol=%%s|argc=%%s' \"$step\" \"$PWD\" \"${LC_ALL-}\" \"${GIT_TERMINAL_PROMPT-}\" \"${GIT_ALLOW_PROTOCOL-}\" \"$#\"; for argument in \"$@\"; do printf '|arg=%%s' \"$argument\"; done; printf '\\n'; }; }\n"
        "fail() { record \"$@\" >> \"$miss\"; exit 97; }\n"
        "[ \"${LC_ALL-}\" = C ] && [ \"${GIT_TERMINAL_PROMPT-}\" = 0 ] && [ \"${GIT_ALLOW_PROTOCOL-}\" = file ] || fail \"$@\"\n"
        "case \"$step\" in\n"
        "  0) [ \"$PWD\" = \"$build\" ] && [ \"$#\" -eq 1 ] && [ \"$1\" = version ] || fail \"$@\" ;;\n"
        "  1) [ \"$PWD\" = \"$build\" ] && [ \"$#\" -eq 10 ] && [ \"$1\" = clone ] && [ \"$2\" = \"$repo\" ] && [ \"$3\" = \"$checkout_slash\" ] && [ \"$4\" = --origin ] && [ \"$5\" = origin ] && [ \"$6\" = --depth ] && [ \"$7\" = 1 ] && [ \"$8\" = --no-single-branch ] && [ \"$9\" = --branch ] && [ \"${10}\" = main ] || fail \"$@\" ;;\n"
        "  2) [ \"$PWD\" = \"$checkout\" ] && [ \"$#\" -eq 3 ] && [ \"$1\" = cat-file ] && [ \"$2\" = -e ] && [ \"$3\" = origin/main ] || fail \"$@\" ;;\n"
        "  3) [ \"$PWD\" = \"$checkout\" ] && [ \"$#\" -eq 2 ] && [ \"$1\" = checkout ] && [ \"$2\" = origin/main ] || fail \"$@\" ;;\n"
        "  4) [ \"$PWD\" = \"$checkout\" ] && [ \"$#\" -eq 3 ] && [ \"$1\" = submodule ] && [ \"$2\" = sync ] && [ \"$3\" = --recursive ] || fail \"$@\" ;;\n"
        "  5) [ \"$PWD\" = \"$checkout\" ] && [ \"$#\" -eq 4 ] && [ \"$1\" = submodule ] && [ \"$2\" = update ] && [ \"$3\" = --init ] && [ \"$4\" = --recursive ] || fail \"$@\" ;;\n"
        "  6) [ \"$PWD\" = \"$checkout\" ] && [ \"$#\" -eq 6 ] && [ \"$1\" = --no-pager ] && [ \"$2\" = log ] && [ \"$3\" = --first-parent ] && [ \"$4\" = -n1 ] && [ \"$5\" = \"--pretty=format:'%%ci'\" ] && [ \"$6\" = qpt-async.el ] || fail \"$@\" ;;\n"
        "  7|9) [ \"$PWD\" = \"$workspace\" ] && [ \"$#\" -eq 6 ] && [ \"$1\" = --no-pager ] && [ \"$2\" = ls-files ] && [ \"$3\" = -c ] && [ \"$4\" = -z ] && [ \"$5\" = -- ] && [ \"$6\" = \"$artifact\" ] || fail \"$@\" ;;\n"
        "  8|10) [ \"$PWD\" = \"$workspace\" ] && [ \"$#\" -eq 7 ] && [ \"$1\" = --no-pager ] && [ \"$2\" = ls-tree ] && [ \"$3\" = --name-only ] && [ \"$4\" = -z ] && [ \"$5\" = HEAD ] && [ \"$6\" = -- ] && [ \"$7\" = \"$artifact\" ] || fail \"$@\" ;;\n"
        "  *) fail \"$@\" ;;\n"
        "esac\n"
        "record \"$@\" >> \"$trace\"\n"
        "next=$((step + 1))\n"
        "printf '%%s\\n' \"$next\" > \"$state.next.$$\"\n"
        "mv -- \"$state.next.$$\" \"$state\"\n"
        "if [ \"$step\" -eq 1 ]; then\n"
        "  printf 'started\\n' > \"$started\"\n"
        "  count=0\n"
        "  while [ ! -f \"$release\" ]; do\n"
        "    count=$((count + 1))\n"
        "    [ \"$count\" -lt 500 ] || exit 98\n"
        "    sleep 0.01\n"
        "  done\n"
        "fi\n"
        "exec \"$real_git\" \"$@\"\n")
       (shell-quote-argument real-git)
       (shell-quote-argument trace)
       (shell-quote-argument miss)
       (shell-quote-argument plan-state)
       (shell-quote-argument started)
       (shell-quote-argument release)
       (shell-quote-argument
        (directory-file-name quelpa-build-dir))
       (shell-quote-argument checkout)
       (shell-quote-argument checkout-slash)
       (shell-quote-argument (car repository))
       (shell-quote-argument workspace)
       (shell-quote-argument artifact-argument)))
     (set-file-modes wrapper #o755)
     (let ((exec-path (cons wrapper-directory exec-path))
           (process-environment (copy-sequence process-environment))
           (quelpa-async-p t))
       (setenv "PATH"
               (concat wrapper-directory
                       (if (characterp path-separator)
                           (char-to-string path-separator)
                         path-separator)
                       (getenv "PATH")))
       (setq timer
             (run-at-time
              0.01 0.01
              (lambda ()
                (when (and
                       (file-exists-p started)
                       (setq observed-process
                             (cl-find-if
                              (lambda (process)
                                (and (process-live-p process)
                                     (string-match-p
                                      "quelpa" (process-name process))))
                              (process-list))))
                  (setq timer-fired t
                        live-process-witnessed (and observed-process t)
                        during-recursion-depth (recursion-depth))
                  (with-temp-file release
                    (insert "release\n"))
                  (cancel-timer timer)))))
       (unwind-protect
           (progn
             (quelpa recipe)
             (setq completed t))
         (when (timerp timer)
           (cancel-timer timer))
         (unless completed
           (unless (file-exists-p release)
             (with-temp-file release
               (insert "abort\n")))
           (when (and observed-process (process-live-p observed-process))
             (delete-process observed-process)))))
     (require 'qpt-async)
     (list
      :descriptor (neomacs-quelpa-test-descriptor 'qpt-async)
      :runtime (qpt-async-deploy "staging Ω" '("core" "docs" "assets"))
      :installed-tree (neomacs-quelpa-test-installed-tree 'qpt-async)
      :installed-source
      (neomacs-quelpa-test-installed-source 'qpt-async "qpt-async.el")
      :build-source
      (neomacs-quelpa-test-read-file
       (expand-file-name "qpt-async.el" checkout))
      :event-loop
      (list :timer-fired timer-fired
            :live-process live-process-witnessed
            :base-recursion-depth base-recursion-depth
            :during-recursion-depth during-recursion-depth
            :after-recursion-depth (recursion-depth)
            :public-call-completed completed
            :started (neomacs-quelpa-test-read-file started)
            :release (neomacs-quelpa-test-read-file release)
            :timer-live (memq timer timer-list)
            :process-status (process-status observed-process)
            :process-exit-status (process-exit-status observed-process))
      :git-trace
      (mapconcat
       #'identity
       (split-string
        (neomacs-quelpa-test-normalize-sandbox-paths
         (neomacs-quelpa-test-read-file trace))
       "\n" t)
       "\n")
      :git-plan
      (let ((steps
             (string-to-number
              (string-trim
               (neomacs-quelpa-test-read-file plan-state)))))
        (list
         :steps steps
         :exhausted (= steps 11)
         :miss
         (if (file-exists-p miss)
             (neomacs-quelpa-test-normalize-sandbox-paths
              (neomacs-quelpa-test-read-file miss))
           "")))
      :git-state
      (list
       :clone-depth-option-passed
       (and (string-match-p
             "arg=--depth|arg=1" (neomacs-quelpa-test-read-file trace))
            t)
       :head
       (string=
        (neomacs-quelpa-test-git-program
         real-git checkout '("rev-parse" "HEAD"))
        (cdr repository))
       :ref
       (neomacs-quelpa-test-git-program
        real-git checkout '("rev-parse" "--abbrev-ref" "HEAD"))
       :remote
       (neomacs-quelpa-test-git-program
        real-git checkout '("remote" "get-url" "origin"))
       :origin-main
       (string=
        (neomacs-quelpa-test-git-program
         real-git checkout '("rev-parse" "refs/remotes/origin/main"))
        (cdr repository))
       :shallow-effective
       (neomacs-quelpa-test-git-program
        real-git checkout '("rev-parse" "--is-shallow-repository")))
      :cache (neomacs-quelpa-test-cache-summary)
      :archives-cleaned (not (file-exists-p quelpa-packages-dir))
      :live-processes (neomacs-quelpa-test-live-processes)))))
"####,
        expect![[r#"OK (:value (:descriptor (:name qpt-async :version "20240503.165005" :summary "Async deployment" :requirements ((emacs (25 1))) :kind nil) :runtime (:environment "staging Ω" :artifacts ("core" "docs" "assets") :count 3) :installed-tree ("qpt-async-autoloads.el" "qpt-async-pkg.el" "qpt-async.el" "qpt-async.elc") :installed-source ";;; qpt-async.el --- Async deployment -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Version: 20240503.165005\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-async-deploy (environment artifacts)\n  (list :environment environment :artifacts artifacts :count (length artifacts)))\n(provide 'qpt-async)\n;;; qpt-async.el ends here\n" :build-source ";;; qpt-async.el --- Async deployment -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-async-deploy (environment artifacts)\n  (list :environment environment :artifacts artifacts :count (length artifacts)))\n(provide 'qpt-async)\n;;; qpt-async.el ends here\n" :event-loop (:timer-fired t :live-process t :base-recursion-depth 0 :during-recursion-depth 1 :after-recursion-depth 0 :public-call-completed t :started "started\n" :release "release\n" :timer-live nil :process-status exit :process-exit-status 0) :git-trace "step=0|cwd=[ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/quelpa/build|lc=C|terminal=0|protocol=file|argc=1|arg=version\nstep=1|cwd=[ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/quelpa/build|lc=C|terminal=0|protocol=file|argc=10|arg=clone|arg=[ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/repositories/async|arg=[ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/quelpa/build/qpt-async/|arg=--origin|arg=origin|arg=--depth|arg=1|arg=--no-single-branch|arg=--branch|arg=main\nstep=2|cwd=[ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/quelpa/build/qpt-async|lc=C|terminal=0|protocol=file|argc=3|arg=cat-file|arg=-e|arg=origin/main\nstep=3|cwd=[ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/quelpa/build/qpt-async|lc=C|terminal=0|protocol=file|argc=2|arg=checkout|arg=origin/main\nstep=4|cwd=[ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/quelpa/build/qpt-async|lc=C|terminal=0|protocol=file|argc=3|arg=submodule|arg=sync|arg=--recursive\nstep=5|cwd=[ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/quelpa/build/qpt-async|lc=C|terminal=0|protocol=file|argc=4|arg=submodule|arg=update|arg=--init|arg=--recursive\nstep=6|cwd=[ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/quelpa/build/qpt-async|lc=C|terminal=0|protocol=file|argc=6|arg=--no-pager|arg=log|arg=--first-parent|arg=-n1|arg=--pretty=format:'%ci'|arg=qpt-async.el\nstep=7|cwd=[ORACLE-WORKSPACE]|lc=C|terminal=0|protocol=file|argc=6|arg=--no-pager|arg=ls-files|arg=-c|arg=-z|arg=--|arg=[ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/quelpa/packages/qpt-async-20240503.165005.el\nstep=8|cwd=[ORACLE-WORKSPACE]|lc=C|terminal=0|protocol=file|argc=7|arg=--no-pager|arg=ls-tree|arg=--name-only|arg=-z|arg=HEAD|arg=--|arg=[ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/quelpa/packages/qpt-async-20240503.165005.el\nstep=9|cwd=[ORACLE-WORKSPACE]|lc=C|terminal=0|protocol=file|argc=6|arg=--no-pager|arg=ls-files|arg=-c|arg=-z|arg=--|arg=[ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/quelpa/packages/qpt-async-20240503.165005.el\nstep=10|cwd=[ORACLE-WORKSPACE]|lc=C|terminal=0|protocol=file|argc=7|arg=--no-pager|arg=ls-tree|arg=--name-only|arg=-z|arg=HEAD|arg=--|arg=[ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/quelpa/packages/qpt-async-20240503.165005.el" :git-plan (:steps 11 :exhausted t :miss "") :git-state (:clone-depth-option-passed t :head t :ref "HEAD" :remote "[ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/repositories/async" :origin-main t :shallow-effective "false") :cache (:live ((qpt-async :fetcher git :url "file://[ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/repositories/async" :branch "main" :depth 1)) :disk ((qpt-async :fetcher git :url "file://[ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/repositories/async" :branch "main" :depth 1)) :raw "((qpt-async :fetcher git :url \"file://[ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/repositories/async\" :branch \"main\" :depth 1))" :same t) :archives-cleaned t :live-processes nil) :stdout "" :stderr "  INFO     Scraping 1 files for loaddefs...\n  INFO     Scraping 1 files for loaddefs...done\n  GEN      qpt-async-autoloads.el\nChecking [ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/elpa/qpt-async-20240503.165005...\nCompiling [ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/elpa/qpt-async-20240503.165005/qpt-async-autoloads.el...\nCompiling [ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/elpa/qpt-async-20240503.165005/qpt-async-pkg.el...\nCompiling [ORACLE-SANDBOX]/quelpa-workflows/async-command-loop/elpa/qpt-async-20240503.165005/qpt-async.el...\nDone (Total of 1 file compiled, 2 skipped)\nSetting ‘package-selected-packages’ temporarily since \"emacs -q\" would overwrite customizations\n")"#]],
    )
    .direct_command_loop()
}

fn stable_recipe_exposes_exact_multi_tag_failure_and_shallow_checkout_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "stable_recipe_exposes_exact_multi_tag_failure_and_shallow_checkout_state",
        r####"
(neomacs-quelpa-test-in-sandbox
 "stable-tags"
 (lambda (root)
   (let* ((source-1
           ";;; qpt-stable.el --- Stable deployment -*- lexical-binding: t; -*-\n;; Version: 1.9.0\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-stable-release () '(:tag \"v1.9.0\" :schema 1))\n(provide 'qpt-stable)\n;;; qpt-stable.el ends here\n")
          (source-2
           ";;; qpt-stable.el --- Stable deployment -*- lexical-binding: t; -*-\n;; Version: 1.10.0\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-stable-release () '(:tag \"v1.10.0\" :schema 2))\n(provide 'qpt-stable)\n;;; qpt-stable.el ends here\n")
          (source-3
           ";;; qpt-stable.el --- Stable deployment -*- lexical-binding: t; -*-\n;; Version: 1.2.99\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-stable-release () '(:tag \"v1.2.99\" :schema 3))\n(provide 'qpt-stable)\n;;; qpt-stable.el ends here\n")
          (source-4
           ";;; qpt-stable.el --- Stable deployment nightly -*- lexical-binding: t; -*-\n;; Version: 9.0-dev\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-stable-release () '(:tag nightly :schema 4))\n(provide 'qpt-stable)\n;;; qpt-stable.el ends here\n")
          (repository
           (neomacs-quelpa-test-repository
            root "repositories/stable" `(("qpt-stable.el" . ,source-1))
            "2024-05-10T09:00:01+0000"))
          (sha-1 (cdr repository))
          (transport-url "https://quelpa-parity.invalid/qpt-stable")
          (git-config (neomacs-quelpa-test-path root "gitconfig"))
          (recipe (list 'qpt-stable
                        :fetcher 'git
                        :url transport-url
                        :branch "main"
                        :depth 1
                        :stable t
                        :version-regexp "^v\\(.*\\)$"))
          (before-count 0)
          (after-count 0)
          (quelpa-before-hook
           (list (lambda () (setq before-count (1+ before-count)))))
          (quelpa-after-hook
           (list (lambda () (setq after-count (1+ after-count))))))
     (neomacs-quelpa-test-git (car repository) "tag" "v1.9.0" sha-1)
     (let ((sha-2
            (neomacs-quelpa-test-advance
             (car repository) `(("qpt-stable.el" . ,source-2))
             "2024-05-11T09:00:02+0000" "stable 1.10")))
       (neomacs-quelpa-test-git (car repository) "tag" "v1.10.0" sha-2))
     (let ((sha-3
            (neomacs-quelpa-test-advance
             (car repository) `(("qpt-stable.el" . ,source-3))
             "2024-05-12T09:00:03+0000" "stable 1.2.99")))
       (neomacs-quelpa-test-git (car repository) "tag" "v1.2.99" sha-3))
     (let ((nightly-sha
            (neomacs-quelpa-test-advance
             (car repository) `(("qpt-stable.el" . ,source-4))
             "2024-05-13T09:00:04+0000" "nightly after stable tags")))
       (neomacs-quelpa-test-write
        root "gitconfig"
        (format "[url \"file://%s\"]\n\tinsteadOf = %s\n"
                (car repository) transport-url))
       (setenv "GIT_CONFIG_GLOBAL" git-config)
       (let* ((outcome
               (condition-case error-data
                   (list :value (quelpa recipe))
                 (error
                  (list :signal (car error-data)
                        :message (error-message-string error-data)))))
              (checkout (expand-file-name "qpt-stable" quelpa-build-dir)))
         (list
          :outcome outcome
          :checkout
          (list
           :exists (file-directory-p checkout)
           :shallow
           (neomacs-quelpa-test-git checkout
                                    "rev-parse" "--is-shallow-repository")
           :head (string= (neomacs-quelpa-test-git checkout "rev-parse" "HEAD")
                          nightly-sha)
           :ref (neomacs-quelpa-test-git
                 checkout "rev-parse" "--abbrev-ref" "HEAD")
           :tags (neomacs-quelpa-test-git checkout "tag" "--list")
           :remote (neomacs-quelpa-test-git checkout "remote" "get-url" "origin")
           :source
           (neomacs-quelpa-test-read-file
            (expand-file-name "qpt-stable.el" checkout)))
          :installed (package-installed-p 'qpt-stable)
          :archives
          (and (file-directory-p quelpa-packages-dir)
               (directory-files quelpa-packages-dir nil "^[^.].*"))
          :cache (neomacs-quelpa-test-cache-summary)
          :hooks (list before-count after-count)
          :initialized quelpa-initialized-p
          :live-processes (neomacs-quelpa-test-live-processes)))))))
"####,
        expect![[
            r#"OK (:outcome (:signal error :message "Failed to checkout ‘qpt-stable’: ‘Symbol’s function definition is void: nil’") :checkout (:exists t :shallow "true" :head t :ref "main" :tags "v1.10.0\nv1.2.99\nv1.9.0" :remote "file://[ORACLE-SANDBOX]/quelpa-workflows/stable-tags/repositories/stable" :source ";;; qpt-stable.el --- Stable deployment nightly -*- lexical-binding: t; -*-\n;; Version: 9.0-dev\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-stable-release () '(:tag nightly :schema 4))\n(provide 'qpt-stable)\n;;; qpt-stable.el ends here\n") :installed nil :archives nil :cache (:live nil :disk nil :raw nil :same t) :hooks (1 0) :initialized t :live-processes nil)"#
        ]],
    )
}

fn missing_git_remote_reports_real_subprocess_failure_without_package_side_effects()
-> ParityBatchCase {
    ParityBatchCase::value(
        "missing_git_remote_reports_real_subprocess_failure_without_package_side_effects",
        r####"
(neomacs-quelpa-test-in-sandbox
 "missing-git-remote"
 (lambda (root)
   (let* ((missing (neomacs-quelpa-test-path
                    root "remotes/does-not-exist.git"))
          (recipe (neomacs-quelpa-test-recipe 'qpt-missing-remote missing))
          (before-count 0)
          (after-count 0)
          (quelpa-before-hook (list (lambda () (setq before-count (1+ before-count)))))
          (quelpa-after-hook (list (lambda () (setq after-count (1+ after-count)))))
          (outcome
           (condition-case error-data
               (list :value (quelpa recipe))
             (error
              (list :signal (car error-data)
                    :message (error-message-string error-data))))))
     (list
      :outcome outcome
      :installed (package-installed-p 'qpt-missing-remote)
      :build-exists (file-exists-p
                     (expand-file-name "qpt-missing-remote" quelpa-build-dir))
      :archives (and (file-directory-p quelpa-packages-dir)
                     (directory-files quelpa-packages-dir nil "^[^.].*"))
      :cache (neomacs-quelpa-test-cache-summary)
      :hooks (list before-count after-count)
      :initialized quelpa-initialized-p
      :live-processes (neomacs-quelpa-test-live-processes)))))
"####,
        expect![[
            r#"OK (:outcome (:signal error :message "Failed to checkout ‘qpt-missing-remote’: ‘Command ’(env LC_ALL=C timeout -k 60 600 git clone [ORACLE-SANDBOX]/quelpa-workflows/missing-git-remote/remotes/does-not-exist.git [ORACLE-SANDBOX]/quelpa-workflows/missing-git-remote/quelpa/build/qpt-missing-remote/ --origin origin --depth 1 --no-single-branch --branch main)’ exited with non-zero status 128: fatal: repository '[ORACLE-SANDBOX]/quelpa-workflows/missing-git-remote/remotes/does-not-exist.git' does not exist\n’") :installed nil :build-exists nil :archives nil :cache (:live nil :disk nil :raw nil :same t) :hooks (1 0) :initialized t :live-processes nil)"#
        ]],
    )
}

fn dependency_failure_preserves_completed_dependency_and_uncommitted_build_state() -> ParityBatchCase
{
    ParityBatchCase::value(
        "dependency_failure_preserves_completed_dependency_and_uncommitted_build_state",
        r####"
(neomacs-quelpa-test-in-sandbox
 "dependency-failure"
 (lambda (root)
   (let* ((good-source
           ";;; qpt-good-dep.el --- Working dependency -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-good-dep-value () \"available α\")\n(provide 'qpt-good-dep)\n;;; qpt-good-dep.el ends here\n")
          (broken-source
           ";;; qpt-broken-dep.el --- Broken recipe target -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(provide 'qpt-broken-dep)\n;;; qpt-broken-dep.el ends here\n")
          (root-source
           ";;; qpt-failure-root.el --- Dependency failure fixture -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Requires: ((emacs \"25.1\") (qpt-good-dep \"1.0\") (qpt-broken-dep \"1.0\"))\n;;; Code:\n(require 'qpt-good-dep)\n(require 'qpt-broken-dep)\n(provide 'qpt-failure-root)\n;;; qpt-failure-root.el ends here\n")
          (good (neomacs-quelpa-test-repository
                 root "repositories/good" `(("qpt-good-dep.el" . ,good-source))
                 "2024-06-01T12:01:01+0000"))
          (broken (neomacs-quelpa-test-repository
                   root "repositories/broken" `(("qpt-broken-dep.el" . ,broken-source))
                   "2024-06-01T13:02:02+0000"))
          (root-repository
           (neomacs-quelpa-test-repository
            root "repositories/root" `(("qpt-failure-root.el" . ,root-source))
            "2024-06-01T14:03:03+0000"))
          (good-recipe (neomacs-quelpa-test-recipe 'qpt-good-dep (car good)))
          (broken-recipe
           (neomacs-quelpa-test-recipe
            'qpt-broken-dep (car broken) :files '("missing-release.el")))
          (root-recipe
           (neomacs-quelpa-test-recipe 'qpt-failure-root (car root-repository)))
          (before-count 0)
          (after-count 0)
          (quelpa-before-hook (list (lambda () (setq before-count (1+ before-count)))))
          (quelpa-after-hook (list (lambda () (setq after-count (1+ after-count))))))
     (setq quelpa-melpa-recipe-stores
           (list (list good-recipe broken-recipe)))
     (let ((outcome
            (condition-case error-data
                (list :value (quelpa root-recipe))
              (error
               (list :signal (car error-data)
                     :message (error-message-string error-data))))))
       (require 'qpt-good-dep)
       (list
        :outcome outcome
        :installed (list (package-installed-p 'qpt-good-dep)
                         (package-installed-p 'qpt-broken-dep)
                         (package-installed-p 'qpt-failure-root))
        :versions (list (neomacs-quelpa-test-version 'qpt-good-dep)
                        (neomacs-quelpa-test-version 'qpt-broken-dep)
                        (neomacs-quelpa-test-version 'qpt-failure-root))
        :completed-runtime (qpt-good-dep-value)
        :completed-descriptor (neomacs-quelpa-test-descriptor 'qpt-good-dep)
        :completed-files (neomacs-quelpa-test-installed-tree 'qpt-good-dep)
        :completed-payloads
        (list
         :installed
         (neomacs-quelpa-test-installed-source 'qpt-good-dep "qpt-good-dep.el")
         :build
         (neomacs-quelpa-test-read-file
          (expand-file-name
           "qpt-good-dep.el"
           (expand-file-name "qpt-good-dep" quelpa-build-dir))))
        :partial-payloads
        (list
         :broken-build
         (neomacs-quelpa-test-read-file
          (expand-file-name
           "qpt-broken-dep.el"
           (expand-file-name "qpt-broken-dep" quelpa-build-dir)))
         :root-build
         (neomacs-quelpa-test-read-file
          (expand-file-name
           "qpt-failure-root.el"
           (expand-file-name "qpt-failure-root" quelpa-build-dir)))
         :root-archive
         (neomacs-quelpa-test-read-file
          (expand-file-name
           "qpt-failure-root-20240601.140303.el"
           quelpa-packages-dir)))
        :selected (neomacs-quelpa-test-selected-fixtures)
        :archives
        (and (file-directory-p quelpa-packages-dir)
             (directory-files quelpa-packages-dir nil "^[^.].*"))
        :build-heads (list
                      (neomacs-quelpa-test-build-head-matches-p
                       'qpt-good-dep (cdr good))
                      (neomacs-quelpa-test-build-head-matches-p
                       'qpt-broken-dep (cdr broken))
                      (neomacs-quelpa-test-build-head-matches-p
                       'qpt-failure-root (cdr root-repository)))
        :cache (neomacs-quelpa-test-cache-summary)
        :hooks (list before-count after-count)
        :initialized quelpa-initialized-p
        :live-processes (neomacs-quelpa-test-live-processes))))))
"####,
        expect![[
            r#"OK (:outcome (:signal error :message "Failed to checkout ‘qpt-broken-dep’: ‘No matching file(s) found in [ORACLE-SANDBOX]/quelpa-workflows/dependency-failure/quelpa/build/qpt-broken-dep/: (missing-release.el)’") :installed (t nil nil) :versions ("20240601.120101" nil nil) :completed-runtime "available α" :completed-descriptor (:name qpt-good-dep :version "20240601.120101" :summary "Working dependency" :requirements ((emacs (25 1))) :kind nil) :completed-files ("qpt-good-dep-autoloads.el" "qpt-good-dep-pkg.el" "qpt-good-dep.el" "qpt-good-dep.elc") :completed-payloads (:installed ";;; qpt-good-dep.el --- Working dependency -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Version: 20240601.120101\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-good-dep-value () \"available α\")\n(provide 'qpt-good-dep)\n;;; qpt-good-dep.el ends here\n" :build ";;; qpt-good-dep.el --- Working dependency -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-good-dep-value () \"available α\")\n(provide 'qpt-good-dep)\n;;; qpt-good-dep.el ends here\n") :partial-payloads (:broken-build ";;; qpt-broken-dep.el --- Broken recipe target -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(provide 'qpt-broken-dep)\n;;; qpt-broken-dep.el ends here\n" :root-build ";;; qpt-failure-root.el --- Dependency failure fixture -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Requires: ((emacs \"25.1\") (qpt-good-dep \"1.0\") (qpt-broken-dep \"1.0\"))\n;;; Code:\n(require 'qpt-good-dep)\n(require 'qpt-broken-dep)\n(provide 'qpt-failure-root)\n;;; qpt-failure-root.el ends here\n" :root-archive ";;; qpt-failure-root.el --- Dependency failure fixture -*- lexical-binding: t; -*-\n;; Version: 1.0\n;; Package-Version: 20240601.140303\n;; Package-Requires: ((emacs \"25.1\") (qpt-good-dep \"1.0\") (qpt-broken-dep \"1.0\"))\n;;; Code:\n(require 'qpt-good-dep)\n(require 'qpt-broken-dep)\n(provide 'qpt-failure-root)\n;;; qpt-failure-root.el ends here\n") :selected (qpt-good-dep) :archives ("qpt-failure-root-20240601.140303.el" "qpt-good-dep-20240601.120101.el") :build-heads (t t t) :cache (:live nil :disk nil :raw nil :same t) :hooks (1 0) :initialized t :live-processes nil)"#
        ]],
    )
}

fn local_file_recipe_preserves_original_version_across_unicode_and_spaces() -> ParityBatchCase {
    ParityBatchCase::value(
        "local_file_recipe_preserves_original_version_across_unicode_and_spaces",
        r####"
(neomacs-quelpa-test-in-sandbox
 "local-file"
 (lambda (root)
   (let* ((source
           ";;; qpt-local.el --- Local developer tool -*- lexical-binding: t; -*-\n;; Version: 3.4.5\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-local-release-note (owner tasks)\n  (format \"%s: %s\" owner (mapconcat #'identity tasks \", \")))\n(provide 'qpt-local)\n;;; qpt-local.el ends here\n")
          (path (neomacs-quelpa-test-write
                 root "developer sources/naive package/qpt-local.el" source))
          (recipe (list 'qpt-local :fetcher 'file :path path :version 'original)))
     (quelpa recipe)
     (require 'qpt-local)
     (let ((live-cache quelpa-cache))
       (setq quelpa-cache nil)
       (quelpa-read-cache)
       (let* ((record (neomacs-quelpa-test-cache-record-on-disk))
              (cached-path (plist-get (cdar quelpa-cache) :path)))
         (list
          :descriptor (neomacs-quelpa-test-descriptor 'qpt-local)
          :runtime (qpt-local-release-note "Zoë Ω" '("review" "ship"))
          :installed-tree (neomacs-quelpa-test-installed-tree 'qpt-local)
          :payloads
          (list
           :original (neomacs-quelpa-test-read-file path)
           :installed
           (neomacs-quelpa-test-installed-source 'qpt-local "qpt-local.el")
           :build
           (neomacs-quelpa-test-read-file
            (expand-file-name
             "qpt-local.el"
             (expand-file-name "qpt-local" quelpa-build-dir))))
          :source-retained (file-exists-p path)
          :cache (list :live live-cache
                       :disk quelpa-cache
                       :raw (plist-get record :raw)
                       :same (equal live-cache quelpa-cache)
                       :cached-source-exists (file-exists-p cached-path)
                       :cached-source-path cached-path)
          :archives-cleaned (not (file-exists-p quelpa-packages-dir))
          :live-processes (neomacs-quelpa-test-live-processes)))))))
"####,
        expect![[
            r#"OK (:descriptor (:name qpt-local :version "3.4.5" :summary "Local developer tool" :requirements ((emacs (25 1))) :kind nil) :runtime "Zoë Ω: review, ship" :installed-tree ("qpt-local-autoloads.el" "qpt-local-pkg.el" "qpt-local.el" "qpt-local.elc") :payloads (:original ";;; qpt-local.el --- Local developer tool -*- lexical-binding: t; -*-\n;; Version: 3.4.5\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-local-release-note (owner tasks)\n  (format \"%s: %s\" owner (mapconcat #'identity tasks \", \")))\n(provide 'qpt-local)\n;;; qpt-local.el ends here\n" :installed ";;; qpt-local.el --- Local developer tool -*- lexical-binding: t; -*-\n;; Version: 3.4.5\n;; Package-Version: 3.4.5\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-local-release-note (owner tasks)\n  (format \"%s: %s\" owner (mapconcat #'identity tasks \", \")))\n(provide 'qpt-local)\n;;; qpt-local.el ends here\n" :build ";;; qpt-local.el --- Local developer tool -*- lexical-binding: t; -*-\n;; Version: 3.4.5\n;; Package-Requires: ((emacs \"25.1\"))\n;;; Code:\n(defun qpt-local-release-note (owner tasks)\n  (format \"%s: %s\" owner (mapconcat #'identity tasks \", \")))\n(provide 'qpt-local)\n;;; qpt-local.el ends here\n") :source-retained t :cache (:live ((qpt-local :fetcher file :path "[ORACLE-SANDBOX]/quelpa-workflows/local-file/developer sources/naive package/qpt-local.el" :version original)) :disk ((qpt-local :fetcher file :path "[ORACLE-SANDBOX]/quelpa-workflows/local-file/developer sources/naive package/qpt-local.el" :version original)) :raw "((qpt-local :fetcher file :path \"[ORACLE-SANDBOX]/quelpa-workflows/local-file/developer sources/naive package/qpt-local.el\" :version original))" :same t :cached-source-exists t :cached-source-path "[ORACLE-SANDBOX]/quelpa-workflows/local-file/developer sources/naive package/qpt-local.el") :archives-cleaned t :live-processes nil)"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        dependency_graph_builds_installs_and_runs_composed_packages(),
        direct_upgrade_reloads_behavior_removes_old_release_and_persists_recipe(),
        persisted_cache_drives_upgrade_all_for_every_recorded_repository(),
        asynchronous_git_install_completes_package_and_process_lifecycle(),
        stable_recipe_exposes_exact_multi_tag_failure_and_shallow_checkout_state(),
        missing_git_remote_reports_real_subprocess_failure_without_package_side_effects(),
        dependency_failure_preserves_completed_dependency_and_uncommitted_build_state(),
        local_file_recipe_preserves_original_version_across_unicode_and_spaces(),
    ]
}

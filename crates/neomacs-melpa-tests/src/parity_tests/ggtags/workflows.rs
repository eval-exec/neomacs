use expect_test::expect;

use super::ParityBatchCase;

fn mode_activation_highlight_and_restoration() -> ParityBatchCase {
    let elisp_form = r#"(ggt-test-run
 "ggtags-mode-lifecycle"
 (lambda (case-root project)
   (let* ((case "ggtags-mode-lifecycle")
          (raw-env (ggt-test-env project))
          (plan
           (list
            (ggt-test-record "global" case "indexed" "app" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-p") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-crs") raw-env
                             "WIDGET_H\ncount\nwidget_total\nwidget_use\n")
            (ggt-test-record "global" case "indexed" "."
                             '("--path-style" "shorter" "--help") raw-env)
            (ggt-test-record "global" case "indexed" "."
                             '("--color" "--help") raw-env)
            (ggt-test-record "global" case "indexed" "app"
                             '("-c" "widget_total") raw-env
                             "widget_total\n")))
          (fixture (progn
                     (ggt-test-seed-database project)
                     (ggt-test-install-plan case-root project case plan "indexed")))
          (ggtags-executable-directory (plist-get fixture :bin))
          (exec-path (cons (directory-file-name (plist-get fixture :bin)) exec-path))
          (source (find-file-noselect (expand-file-name "app/main.c" project)))
          (sentinel-after-save #'ggt-test-after-save-sentinel)
          (sentinel-xref #'ggt-test-xref-sentinel)
          (sentinel-capf #'ggt-test-capf-sentinel)
          (sentinel-eldoc #'ggt-test-eldoc-sentinel)
          before enabled highlight disabled)
     (with-current-buffer source
       (c-mode)
       (set-window-buffer (selected-window) source)
       (setq-local after-save-hook (list sentinel-after-save)
                   xref-backend-functions (list sentinel-xref)
                   completion-at-point-functions (list sentinel-capf)
                   eldoc-documentation-function sentinel-eldoc
                   mode-line-buffer-identification '("sentinel"))
       (setq before
             (list (copy-tree after-save-hook)
                   (copy-tree xref-backend-functions)
                   (copy-tree completion-at-point-functions)
                   eldoc-documentation-function
                   (copy-tree mode-line-buffer-identification)))
       (ggtags-find-project)
       (ggtags-mode 1)
       (goto-char (point-min))
       (forward-line 1)
       (setq enabled
             (list
              :mode ggtags-mode
              :after-save-sentinel (and (memq sentinel-after-save after-save-hook) t)
              :after-save-package (and (memq #'ggtags-after-save-function after-save-hook) t)
              :xref-sentinel (and (memq sentinel-xref xref-backend-functions) t)
              :xref-package (and (memq #'ggtags--xref-backend xref-backend-functions) t)
              :capf-sentinel (and (memq sentinel-capf completion-at-point-functions) t)
              :capf-package (and (memq #'ggtags-completion-at-point
                                       completion-at-point-functions) t)
              :eldoc-result (funcall eldoc-documentation-function)
              :eldoc-package (and (advice-function-member-p
                                    #'ggtags-eldoc-function
                                    eldoc-documentation-function) t)
              :mode-line-member
              (and (memq 'ggtags-mode-line-project-name
                         mode-line-buffer-identification) t)
              :mode-line-eval
              (substring-no-properties
               (eval (cadr (cadr ggtags-mode-line-project-name)) t))
              :mode-line-rendered-batch
              (format-mode-line ggtags-mode-line-project-name)
              :keys (list (lookup-key ggtags-mode-map (kbd "M-."))
                          (lookup-key ggtags-mode-map (kbd "M-]"))
                          (lookup-key ggtags-mode-map (kbd "C-M-.")))))
       (goto-char (point-min))
       (search-forward "widget_total")
       (backward-word)
       (ggtags-highlight-tag-at-point)
       (let ((overlay ggtags-highlight-tag-overlay))
         (push overlay ggt-test-owned-overlays)
         (setq highlight
               (list :start (overlay-start overlay)
                     :end (overlay-end overlay)
                     :length (- (overlay-end overlay) (overlay-start overlay))
                     :text (buffer-substring-no-properties
                            (overlay-start overlay) (overlay-end overlay))
                     :category (overlay-get overlay 'category)
                     :face (get 'ggtags-active-tag 'face)
                     :mouse (lookup-key (get 'ggtags-active-tag 'keymap)
                                        [S-mouse-1]))))
       (ggtags-mode -1)
       (setq disabled
             (list :mode ggtags-mode
                   :after-save-restored (equal after-save-hook (nth 0 before))
                   :xref-restored (equal xref-backend-functions (nth 1 before))
                   :capf-restored (equal completion-at-point-functions (nth 2 before))
                   :eldoc-restored (eq eldoc-documentation-function (nth 3 before))
                   :mode-line-restored
                   (equal mode-line-buffer-identification (nth 4 before))
                   :overlay ggtags-highlight-tag-overlay
                   :timer ggtags-highlight-tag-timer)))
     (list :enabled enabled :highlight highlight :disabled disabled
           :fixture (ggt-test-fixture-state fixture project)))))"#;
    let expected = expect![[
        r#"OK (:result (:enabled (:mode t :after-save-sentinel t :after-save-package t :xref-sentinel t :xref-package t :capf-sentinel t :capf-package t :eldoc-result "sentinel" :eldoc-package t :mode-line-member t :mode-line-eval "project Ω space" :mode-line-rendered-batch "" :keys (ggtags-find-tag-dwim ggtags-find-reference ggtags-find-tag-regexp)) :highlight (:start 57 :end 69 :length 12 :text "widget_total" :category ggtags-active-tag :face ggtags-highlight :mouse ggtags-find-tag-dwim) :disabled (:mode nil :after-save-restored t :xref-restored t :capf-restored t :eldoc-restored t :mode-line-restored t :overlay nil :timer nil) :fixture (:index 6 :planned 6 :generation "indexed" :misses nil :help-stdout-contracts (:count 2 :values ("status-only:process-file-destination-nil:global-6.7:8590:3f269245f1c7abedd402629112a843e238947aed79f6556b4228f369e1b7af39")) :recording-stream-contracts (:count 6 :values ("b0c4324e7fc48346b1b32a11a76cff33b4593176b7f6a5ac57f6a01bf6e84853" "8fd149ac622eda6d0dac6eca9906d7b5ed14cd375a8f5c4532fe758e2f69ac2e" "eadb6c511e574df8ca19d1b205a972ea31770dc39c5056fb00c939557bdb72e1" "0ef548d0c6ad2408baa68a6a53b0c67fff20535841266fb48069c8014958db73")) :trace ("CALL" "0" "global" "ggtags-mode-lifecycle" "indexed" "app" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "1" "global" "ggtags-mode-lifecycle" "indexed" "." "1" "-p" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "2" "global" "ggtags-mode-lifecycle" "indexed" "." "1" "-crs" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "3" "global" "ggtags-mode-lifecycle" "indexed" "." "3" "--path-style" "shorter" "--help" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "4" "global" "ggtags-mode-lifecycle" "indexed" "." "2" "--color" "--help" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "5" "global" "ggtags-mode-lifecycle" "indexed" "app" "2" "-c" "widget_total" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END"))) :cleanup (:new-buffers nil :new-processes nil :compilation-last-buffer nil :compilation-processes nil :new-timers 0 :root-exists nil :root-owned nil :window-restored t :navigation nil :xref-history (0 0) :start-marker nil :start-file nil :line-overlay nil :highlight-overlay nil :project-count 0 :prompts-remaining nil :prompt-calls nil :body-error nil :cleanup-errors nil))"#
    ]];
    ParityBatchCase::value(
        "mode_activation_highlight_and_restoration",
        elisp_form,
        expected,
    )
}

fn creates_updates_saves_and_deletes_a_real_project() -> ParityBatchCase {
    let elisp_form = r#"(ggt-test-run
 "ggtags-database-lifecycle"
 (lambda (case-root project)
   (let* ((case "ggtags-database-lifecycle")
          (raw-env (ggt-test-env project))
          (create-env (ggt-test-env project t "ctags"))
          (project-env (ggt-test-env project t))
          (updated-widget
           (concat ggt-test-widget-source
                   "\nint widget_flush(int count) {\n"
                   "    return widget_total(count);\n}\n"))
          (plan
           (list
            (ggt-test-record "global" case "empty" "." '("-pr") project-env
                             "" "global: GTAGS not found.\n" 3)
            (ggt-test-record "global" case "empty" "." '("-pr") project-env
                             "" "global: GTAGS not found.\n" 3)
            (ggt-test-record "gtags" case "empty" "." '("--idutils") create-env
                             "" "gtags: mkid not found.\n" 1)
            ;; Pinned ggtags retries with `cl-remove' without an `equal' test,
            ;; so current GNU Emacs leaves the distinct string in ARGS.
            (ggt-test-record "gtags" case "empty" "." '("--idutils") create-env
                             "" "gtags: mkid not found.\n" 1)
            (ggt-test-record "global" case "empty" "." '("-pr") project-env
                             "" "global: GTAGS not found.\n" 3)
            (ggt-test-record "global" case "empty" "." '("-pr") project-env
                             "" "global: GTAGS not found.\n" 3)
            ;; GNU Global 6.7 documents --compact as a safe create-time
            ;; database-format option.  Supply it only through the package's
            ;; public ggtags-extra-args customization.
            (ggt-test-record
             "gtags" case "empty" "." '("--compact") create-env "" "" 0
             '((kind . "create-database")) "indexed")
            (ggt-test-record "global" case "indexed" "src" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-p") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-crs") raw-env
                             "WIDGET_H\ncount\nwidget_total\nwidget_use\n")
            (ggt-test-record "global" case "indexed" "."
                             '("--path-style" "shorter" "--help") raw-env)
            (ggt-test-record "global" case "indexed" "."
                             '("--color" "--help") raw-env)
            (ggt-test-record "global" case "indexed" "src" '("-u") raw-env
                             "" "" 0 nil "updated")
            (append
             (ggt-test-record "global" case "updated" "." '("-pr") raw-env
                              (concat (directory-file-name project) "\n"))
             '((fixture_state . "saved")))
            (ggt-test-record
             "global" case "updated" "."
             '("--single-update" "src/widget.c") raw-env "" "" 0
             '((kind . "validate-file")
               (path . "src/widget.c")
               (sha256 . "bc2ea5b463ab073e3f24968a0a31cdcdc04f313265bcdb003273897529b47e23"))
             "saved")
            (ggt-test-record "global" case "saved" "src" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "saved" "src"
                             '("-c" "widget_fl") raw-env
                             "widget_flush\n")
            (ggt-test-record "global" case "saved" "src"
                             '("-c" "widget_flush") raw-env
                             "widget_flush\n")
            (ggt-test-record
             "global" case "saved" "src"
             '("--result=grep" "--path-style=absolute" "widget_flush")
             raw-env
             (concat (directory-file-name project)
                     "/src/widget.c:11:int widget_flush(int count) {\n"))))
          (fixture (ggt-test-install-plan case-root project case plan "empty"))
          (ggtags-executable-directory (plist-get fixture :bin))
          (exec-path (cons (directory-file-name (plist-get fixture :bin)) exec-path))
          (ggtags-use-idutils t)
          (create-extra-args '("--compact"))
          (source-file (expand-file-name "src/widget.c" project))
          (message-start (ggt-test-messages-point))
          source idutils-error idutils-files created files project-state force-state save-state
          post-update before-delete after-delete messages)
     (setq ggt-test-prompts
           '(("Use `ctags' backend? " . t)
             ("Use `ctags' backend? " . t)
             ("Remove GNU Global tag files? " . t)))
     (cl-letf (((symbol-function 'yes-or-no-p)
                #'ggt-test-answer-yes-or-no))
         (setq idutils-error
               (condition-case condition
                   (let ((default-directory project))
                     (ggtags-create-tags project)
                     :unexpected-success)
                 (error condition)))
         (setq idutils-files
               (mapcar (lambda (name)
                         (list name
                               (file-exists-p (expand-file-name name project))))
                       '("GPATH" "GRTAGS" "GTAGS" "ID")))
         (let ((ggtags-use-idutils nil)
               (ggtags-extra-args create-extra-args))
           (let ((default-directory project))
             (setq created (ggtags-create-tags project))))
       (setq source (find-file-noselect source-file))
       (set-window-buffer (selected-window) source)
       (with-current-buffer source
         (c-mode)
         (ggtags-mode 1)
         (let ((found (ggtags-find-project)))
           (setq files
                 (mapcar
                  (lambda (name)
                    (let ((file (expand-file-name name project)))
                      (list name (file-exists-p file)
                            (and (file-exists-p file)
                                 (file-attribute-size (file-attributes file))))))
                  '("GPATH" "GRTAGS" "GTAGS" "ID")))
           (setq project-state
                 (list (ggtags-project-root found)
                       (ggtags-project-dirty-p found)
                       (ggtags-project-has-refs found)
                       (ggtags-project-has-path-style found)
                       (ggtags-project-has-color found)))
           (setf (ggtags-project-dirty-p found) t)
           (ggtags-update-tags t)
           (setq force-state
                 (list (ggtags-project-dirty-p found)
                       (alist-get 'generation (ggt-test-read-state fixture))))
           (goto-char (point-max))
           (insert "\nint widget_flush(int count) {\n"
                   "    return widget_total(count);\n}\n")
           (save-buffer)
           (ggt-test-wait-index fixture 15)
           (setq save-state
                 (list :disk (ggt-test-read-file source-file)
                       :expected (equal (ggt-test-read-file source-file)
                                        updated-widget)
                       :modified (buffer-modified-p)
                       :generation
                       (alist-get 'generation (ggt-test-read-state fixture))))
           (goto-char (point-max))
           (insert "\nwidget_fl")
           (let ((start (- (point) (length "widget_fl"))))
             (setq post-update
                   (list :completion-return (completion-at-point)
                         :completion-text
                         (buffer-substring-no-properties start (point))
                         :completion-cache (copy-tree ggtags-completion-cache))))
           (delete-region (line-beginning-position) (point-max))
           (set-buffer-modified-p nil)
           (let ((original-message (symbol-function 'message)))
             (cl-letf (((symbol-function 'message)
                        (lambda (format-string &rest arguments)
                          (apply #'ggt-test-observe-message
                                 original-message format-string arguments))))
               (ggtags-show-definition "widget_flush")
               (ggt-test-wait-index fixture 19)
               (ggt-test-wait-until
                "post-update definition callback"
                (lambda ()
                  (and ggt-test-message-ledger
                       (not (get-buffer " *ggtags-definition*")))))))
           (setq post-update
                 (append post-update
                         (list :definition
                               (car ggt-test-message-ledger)
                               :disk-unchanged
                               (equal (ggt-test-read-file source-file)
                                      updated-widget))))
           (setq before-delete (ggt-test-read-file source-file))
           (let ((temp-buffer-show-hook '(ggt-test-capture-temp-buffer)))
             (ggtags-delete-tags))
           (setq after-delete
                 (list :files
                       (mapcar (lambda (name)
                                 (list name
                                       (file-exists-p
                                        (expand-file-name name project))))
                               '("GPATH" "GRTAGS" "GTAGS" "ID"))
                       :cache (gethash project ggtags-projects)
                       :source-unchanged
                       (equal before-delete (ggt-test-read-file source-file)))))))
     (setq messages (ggt-test-messages-since message-start))
     (list :idutils-error idutils-error :idutils-files idutils-files
           :create-extra-args create-extra-args
           :created created :files files :project project-state
           :force-update force-state :saved save-state
           :post-update post-update
           :delete-list ggt-test-temp-buffer-text
           :delete after-delete :prompts (ggt-test-prompt-calls)
           :messages messages
           :fixture (ggt-test-fixture-state fixture project)))))"#;
    let expected = expect![[
        r##"OK (:result (:idutils-error (error "‘gtags’ non-zero exit: gtags: mkid not found.") :idutils-files (("GPATH" nil) ("GRTAGS" nil) ("GTAGS" nil) ("ID" nil)) :create-extra-args ("--compact") :created "[ROOT]/" :files (("GPATH" t 16384) ("GRTAGS" t 16384) ("GTAGS" t 16384) ("ID" nil nil)) :project ("[ROOT]/" nil has-refs has-path-style has-color) :force-update (nil "updated") :saved (:disk "#include \"widget.h\"\n\nint widget_total(int count) {\n    return count + 1;\n}\n\nint widget_use(void) {\n    return widget_total(41);\n}\n\nint widget_flush(int count) {\n    return widget_total(count);\n}\n" :expected t :modified nil :generation "saved") :post-update (:completion-return t :completion-text "widget_flush" :completion-cache ("widget_flush$" "widget_flush") :definition "int widget_flush(int count) {" :disk-unchanged t) :delete-list "[ROOT]/GPATH\n[ROOT]/GRTAGS\n[ROOT]/GTAGS" :delete (:files (("GPATH" nil) ("GRTAGS" nil) ("GTAGS" nil) ("ID" nil)) :cache nil :source-unchanged t) :prompts (("Use `ctags' backend? " . t) ("Use `ctags' backend? " . t) ("Remove GNU Global tag files? " . t)) :messages ("`gtags' in progress...done (<TIME>)" "GTAGS generated in ‘[ROOT]/’" "`global -u' in progress...done (<TIME>)") :fixture (:index 19 :planned 19 :generation "saved" :misses nil :help-stdout-contracts (:count 2 :values ("status-only:process-file-destination-nil:global-6.7:8590:3f269245f1c7abedd402629112a843e238947aed79f6556b4228f369e1b7af39")) :recording-stream-contracts (:count 19 :values ("396316a0b541a487816382800512d33018e810489683027b476fbc604c3a26dd" "7d3e41bfff584699cfe96a1636c15c2224898a52c01542673c862fec1d7e761d" "eadb6c511e574df8ca19d1b205a972ea31770dc39c5056fb00c939557bdb72e1" "b0c4324e7fc48346b1b32a11a76cff33b4593176b7f6a5ac57f6a01bf6e84853" "8fd149ac622eda6d0dac6eca9906d7b5ed14cd375a8f5c4532fe758e2f69ac2e" "f2e368a0537de0e2b4361e3d639fa079565869efc759190d03caf119ca250128" "5da0104944628f0721b91bd18ecd1fa677a7c3093cfd45d55c605516f9a1d41e")) :trace ("CALL" "0" "global" "ggtags-database-lifecycle" "empty" "." "1" "-pr" "GTAGSROOT=[ROOT]" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "1" "global" "ggtags-database-lifecycle" "empty" "." "1" "-pr" "GTAGSROOT=[ROOT]" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "2" "gtags" "ggtags-database-lifecycle" "empty" "." "1" "--idutils" "GTAGSROOT=[ROOT]" "GTAGSDBPATH=None" "GTAGSLABEL=ctags" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "3" "gtags" "ggtags-database-lifecycle" "empty" "." "1" "--idutils" "GTAGSROOT=[ROOT]" "GTAGSDBPATH=None" "GTAGSLABEL=ctags" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "4" "global" "ggtags-database-lifecycle" "empty" "." "1" "-pr" "GTAGSROOT=[ROOT]" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "5" "global" "ggtags-database-lifecycle" "empty" "." "1" "-pr" "GTAGSROOT=[ROOT]" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "6" "gtags" "ggtags-database-lifecycle" "empty" "." "1" "--compact" "GTAGSROOT=[ROOT]" "GTAGSDBPATH=None" "GTAGSLABEL=ctags" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "7" "global" "ggtags-database-lifecycle" "indexed" "src" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "8" "global" "ggtags-database-lifecycle" "indexed" "." "1" "-p" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "9" "global" "ggtags-database-lifecycle" "indexed" "." "1" "-crs" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "10" "global" "ggtags-database-lifecycle" "indexed" "." "3" "--path-style" "shorter" "--help" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "11" "global" "ggtags-database-lifecycle" "indexed" "." "2" "--color" "--help" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "12" "global" "ggtags-database-lifecycle" "indexed" "src" "1" "-u" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "13" "global" "ggtags-database-lifecycle" "updated" "." "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "14" "global" "ggtags-database-lifecycle" "updated" "." "2" "--single-update" "src/widget.c" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "15" "global" "ggtags-database-lifecycle" "saved" "src" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "16" "global" "ggtags-database-lifecycle" "saved" "src" "2" "-c" "widget_fl" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "17" "global" "ggtags-database-lifecycle" "saved" "src" "2" "-c" "widget_flush" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "18" "global" "ggtags-database-lifecycle" "saved" "src" "3" "--result=grep" "--path-style=absolute" "widget_flush" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END"))) :cleanup (:new-buffers nil :new-processes nil :compilation-last-buffer nil :compilation-processes nil :new-timers 0 :root-exists nil :root-owned nil :window-restored t :navigation nil :xref-history (0 0) :start-marker nil :start-file nil :line-overlay nil :highlight-overlay nil :project-count 0 :prompts-remaining nil :prompt-calls (("Use `ctags' backend? " . t) ("Use `ctags' backend? " . t) ("Remove GNU Global tag files? " . t)) :body-error nil :cleanup-errors nil))"##
    ]];
    ParityBatchCase::value(
        "creates_updates_saves_and_deletes_a_real_project",
        elisp_form,
        expected,
    )
}

fn compilation_search_navigation_and_history() -> ParityBatchCase {
    let elisp_form = r#"(ggt-test-run
 "ggtags-compilation-navigation"
 (lambda (case-root project)
   (let* ((case "ggtags-compilation-navigation")
          (raw-env (ggt-test-env project))
          (output
           (concat
            "src/widget.c:3:int \e[01;31mwidget_total\e[m(int count) {\n"
            "src/widget_alt.c:3:int \e[01;31mwidget_total\e[m(int count) {\n"
            "2 objects located (using '" (directory-file-name project)
            "/GTAGS').\n"))
          (plan
           (list
            (ggt-test-record "global" case "indexed" "app" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-p") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-crs") raw-env
                             "WIDGET_H\ncount\nwidget_total\nwidget_use\n")
            (ggt-test-record "global" case "indexed" "."
                             '("--path-style" "shorter" "--help") raw-env)
            (ggt-test-record "global" case "indexed" "."
                             '("--color" "--help") raw-env)
            (ggt-test-record "global" case "indexed" "app" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "."
                             '("-vP" "^app/main.c$") raw-env "")
            (ggt-test-record "global" case "indexed" "." '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record
             "global" case "indexed" "."
             '("-v" "--result=grep" "--color=always" "--path-style=shorter"
               "--from-here=4:app/main.c" "--" "widget_total")
             raw-env output)
            (ggt-test-record "global" case "indexed" "src" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "src" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))))
          (fixture
           (progn (ggt-test-seed-database project)
                  (ggt-test-install-plan case-root project case plan "indexed")))
          (ggtags-executable-directory (plist-get fixture :bin))
          (exec-path (cons (directory-file-name (plist-get fixture :bin)) exec-path))
          (ggtags-auto-jump-to-match nil)
          (source (find-file-noselect (expand-file-name "app/main.c" project)))
          results command origin result-state first second history returned)
     (with-current-buffer source
       (c-mode)
       (set-window-buffer (selected-window) source)
       (ggtags-mode 1)
       (ggtags-find-project)
       (goto-char (point-min))
       (forward-line 3)
       (move-to-column 18)
       (setq origin (ggt-test-location))
       (setq command (lookup-key ggtags-mode-map (kbd "M-.")))
       (call-interactively command))
     (setq results (ggt-test-wait-global fixture 10))
     (with-current-buffer results
       (goto-char (point-min))
       (search-forward "src/widget.c:3:")
       (let ((row-start (line-beginning-position)))
         (search-forward "widget_total")
         (setq result-state
               (list
                :mode major-mode :navigation ggtags-navigation-mode
                :exit (mapcar (lambda (value)
                                (if (stringp value)
                                    (substring-no-properties value)
                                  value))
                              ggtags-global-exit-info)
                :output-lines ggtags-global-output-lines
                :text (ggt-test-global-text results)
                :first-tag-properties
                (list :column (- (match-beginning 0) row-start)
                      :global-color
                      (get-text-property (match-beginning 0) 'global-color)
                      :face (get-text-property (match-beginning 0) 'face)
                      :font-lock-face
                      (get-text-property (match-beginning 0) 'font-lock-face)
                      :compilation-message
                      (and (get-text-property row-start 'compilation-message) t)))))
       (setq next-error-last-buffer results)
       (goto-char (point-min)))
     (next-error 1 t)
     (setq first (with-current-buffer (window-buffer) (ggt-test-location)))
     (next-error 1)
     (setq second (with-current-buffer (window-buffer) (ggt-test-location)))
     (with-current-buffer results
       (setq history (ggtags-global-current-search)))
     (ggtags-navigation-mode-abort)
     (setq returned
           (list (ggt-test-location)
                 :same-buffer (eq (current-buffer) source)
                 :same-point (= (point) (with-current-buffer source (point)))
                 :navigation ggtags-navigation-mode
                 :result-live (buffer-live-p results)))
     (list :command command :origin origin
           :result result-state :first first :second second
           :history history :returned returned
           :fixture (ggt-test-fixture-state fixture project)))))"#;
    let expected = expect![[
        r#"OK (:result (:command ggtags-find-tag-dwim :origin ("main.c" 4 18 "      int value = widget_total(2);" t) :result (:mode ggtags-global-mode :navigation t :exit (0 2 "GTAGS") :output-lines 3 :text "-*- mode: ggtags-global; default-directory: \"[ROOT]/\" -*-\nGlobal <STATUS>\n\n[CASE]/bin/global -v --result=grep --color=always --path-style=shorter --from-here=4:app/main.c -- widget_total\nsrc/widget.c:3:int widget_total(int count) {\nsrc/widget_alt.c:3:int widget_total(int count) {\n2 objects located (using '[ROOT]/GTAGS').\n\nGlobal <STATUS>" :first-tag-properties (:column 19 :global-color t :face nil :font-lock-face nil :compilation-message t)) :first ("widget.c" 3 4 "int widget_total(int count) {" t) :second ("widget_alt.c" 3 4 "int widget_total(int count) {" t) :history ("--from-here=4:app/main.c -- widget_total" "[ROOT]/" nil 6 "src/widget_alt.c:3:int widget_total(int count) {") :returned (("main.c" 4 18 "      int value = widget_total(2);" t) :same-buffer t :same-point t :navigation nil :result-live nil) :fixture (:index 13 :planned 13 :generation "indexed" :misses nil :help-stdout-contracts (:count 2 :values ("status-only:process-file-destination-nil:global-6.7:8590:3f269245f1c7abedd402629112a843e238947aed79f6556b4228f369e1b7af39")) :recording-stream-contracts (:count 13 :values ("b0c4324e7fc48346b1b32a11a76cff33b4593176b7f6a5ac57f6a01bf6e84853" "8fd149ac622eda6d0dac6eca9906d7b5ed14cd375a8f5c4532fe758e2f69ac2e" "eadb6c511e574df8ca19d1b205a972ea31770dc39c5056fb00c939557bdb72e1" "bb8eab3a176a2494c3af6a0630d1eafae2077f55e208e939a8da194a1f4fb5b1")) :trace ("CALL" "0" "global" "ggtags-compilation-navigation" "indexed" "app" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "1" "global" "ggtags-compilation-navigation" "indexed" "." "1" "-p" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "2" "global" "ggtags-compilation-navigation" "indexed" "." "1" "-crs" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "3" "global" "ggtags-compilation-navigation" "indexed" "." "3" "--path-style" "shorter" "--help" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "4" "global" "ggtags-compilation-navigation" "indexed" "." "2" "--color" "--help" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "5" "global" "ggtags-compilation-navigation" "indexed" "app" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "6" "global" "ggtags-compilation-navigation" "indexed" "." "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "7" "global" "ggtags-compilation-navigation" "indexed" "." "2" "-vP" "^app/main.c$" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "8" "global" "ggtags-compilation-navigation" "indexed" "." "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "9" "global" "ggtags-compilation-navigation" "indexed" "." "7" "-v" "--result=grep" "--color=always" "--path-style=shorter" "--from-here=4:app/main.c" "--" "widget_total" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "10" "global" "ggtags-compilation-navigation" "indexed" "src" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "11" "global" "ggtags-compilation-navigation" "indexed" "." "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "12" "global" "ggtags-compilation-navigation" "indexed" "src" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END"))) :cleanup (:new-buffers nil :new-processes nil :compilation-last-buffer nil :compilation-processes nil :new-timers 0 :root-exists nil :root-owned nil :window-restored t :navigation nil :xref-history (0 0) :start-marker nil :start-file nil :line-overlay nil :highlight-overlay nil :project-count 0 :prompts-remaining nil :prompt-calls nil :body-error nil :cleanup-errors nil))"#
    ]];
    ParityBatchCase::value(
        "compilation_search_navigation_and_history",
        elisp_form,
        expected,
    )
}

fn include_dwim_uses_real_key_and_auto_jumps() -> ParityBatchCase {
    let elisp_form = r#"(ggt-test-run
 "ggtags-include-dwim"
 (lambda (case-root project)
   (let* ((case "ggtags-include-dwim")
          (raw-env (ggt-test-env project))
          (output
           (concat "src/widget.h\n1 object located (using '"
                   (directory-file-name project) "/GPATH').\n"))
          (plan
           (list
            (ggt-test-record "global" case "indexed" "app" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-p") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-crs") raw-env
                             "WIDGET_H\ncount\nwidget_total\nwidget_use\n")
            (ggt-test-record "global" case "indexed" "."
                             '("--path-style" "shorter" "--help") raw-env)
            (ggt-test-record "global" case "indexed" "."
                             '("--color" "--help") raw-env)
            (ggt-test-record "global" case "indexed" "." '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record
             "global" case "indexed" "."
             '("-v" "--result=path" "--color=always" "--path-style=shorter"
               "--path" "--" "widget.h")
             raw-env output)
            (ggt-test-record "global" case "indexed" "src" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))))
          (fixture
           (progn (ggt-test-seed-database project)
                  (ggt-test-install-plan case-root project case plan "indexed")))
          (ggtags-executable-directory (plist-get fixture :bin))
          (exec-path (cons (directory-file-name (plist-get fixture :bin)) exec-path))
          (source (find-file-noselect (expand-file-name "app/main.c" project)))
          command visited)
     (with-current-buffer source
       (c-mode)
       (set-window-buffer (selected-window) source)
       (ggtags-mode 1)
       (ggtags-find-project)
       (goto-char (point-min))
       (setq command (lookup-key ggtags-mode-map (kbd "M-.")))
       (call-interactively command))
     (ggt-test-wait-index fixture 8)
     (ggt-test-wait-until
      "include auto-jump and result cleanup"
      (lambda ()
        (and (not ggtags-navigation-mode)
             (not (buffer-live-p ggtags-global-last-buffer))
             (with-current-buffer (window-buffer)
               (and buffer-file-name
                    (string-suffix-p "/src/widget.h" buffer-file-name))))))
     (setq visited (with-current-buffer (window-buffer) (ggt-test-location)))
     (list :command command :visited visited
           :navigation ggtags-navigation-mode
           :global-buffer-live (buffer-live-p ggtags-global-last-buffer)
           :fixture (ggt-test-fixture-state fixture project)))))"#;
    let expected = expect![[
        r##"OK (:result (:command ggtags-find-tag-dwim :visited ("widget.h" 1 0 "#ifndef WIDGET_H" t) :navigation nil :global-buffer-live nil :fixture (:index 9 :planned 9 :generation "indexed" :misses nil :help-stdout-contracts (:count 2 :values ("status-only:process-file-destination-nil:global-6.7:8590:3f269245f1c7abedd402629112a843e238947aed79f6556b4228f369e1b7af39")) :recording-stream-contracts (:count 9 :values ("b0c4324e7fc48346b1b32a11a76cff33b4593176b7f6a5ac57f6a01bf6e84853" "8fd149ac622eda6d0dac6eca9906d7b5ed14cd375a8f5c4532fe758e2f69ac2e" "eadb6c511e574df8ca19d1b205a972ea31770dc39c5056fb00c939557bdb72e1" "719759c4cda7d1d2829f6035765c47f5cffe6f4aca761a641222fb0da06213bb")) :trace ("CALL" "0" "global" "ggtags-include-dwim" "indexed" "app" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "1" "global" "ggtags-include-dwim" "indexed" "." "1" "-p" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "2" "global" "ggtags-include-dwim" "indexed" "." "1" "-crs" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "3" "global" "ggtags-include-dwim" "indexed" "." "3" "--path-style" "shorter" "--help" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "4" "global" "ggtags-include-dwim" "indexed" "." "2" "--color" "--help" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "5" "global" "ggtags-include-dwim" "indexed" "." "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "6" "global" "ggtags-include-dwim" "indexed" "." "7" "-v" "--result=path" "--color=always" "--path-style=shorter" "--path" "--" "widget.h" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "7" "global" "ggtags-include-dwim" "indexed" "src" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "8" "global" "ggtags-include-dwim" "indexed" "." "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END"))) :cleanup (:new-buffers nil :new-processes nil :compilation-last-buffer nil :compilation-processes nil :new-timers 0 :root-exists nil :root-owned nil :window-restored t :navigation nil :xref-history (0 0) :start-marker nil :start-file nil :line-overlay nil :highlight-overlay nil :project-count 0 :prompts-remaining nil :prompt-calls nil :body-error nil :cleanup-errors nil))"##
    ]];
    ParityBatchCase::value(
        "include_dwim_uses_real_key_and_auto_jumps",
        elisp_form,
        expected,
    )
}

fn integrations_complete_describe_index_and_navigate_real_xref_ui() -> ParityBatchCase {
    let elisp_form = r#"(ggt-test-run
 "ggtags-ide-integrations"
 (lambda (case-root project)
   (let* ((case "ggtags-ide-integrations")
          (raw-env (ggt-test-env project))
          (xref-output
           (concat
            "src/widget.c:3:int \e[01;31mwidget_total\e[m(int count) {\n"
            "src/widget_alt.c:3:int \e[01;31mwidget_total\e[m(int count) {\n"
            "2 objects located (using '" (directory-file-name project)
            "/GTAGS').\n"))
          (plan
           (list
            (ggt-test-record "global" case "indexed" "app" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-p") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-crs") raw-env
                             "WIDGET_H\ncount\nwidget_total\nwidget_use\n")
            (ggt-test-record "global" case "indexed" "."
                             '("--path-style" "shorter" "--help") raw-env)
            (ggt-test-record "global" case "indexed" "."
                             '("--color" "--help") raw-env)
            (ggt-test-record "global" case "indexed" "app"
                             '("-c" "widget_tot") raw-env
                             "widget_total\n")
            (ggt-test-record "global" case "indexed" "app"
                             '("-c" "widget_total") raw-env
                             "widget_total\n")
            (ggt-test-record
             "global" case "indexed" "app"
             '("--result=grep" "--path-style=absolute" "widget_total")
             raw-env
             (concat (directory-file-name project)
                     "/src/widget.c:3:int widget_total(int count) {\n"
                     (directory-file-name project)
                     "/src/widget_alt.c:3:int widget_total(int count) {\n"))
            (ggt-test-record "global" case "indexed" "src" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "src"
                             '("-x" "-f" "widget.c") raw-env
                             (concat
                              "widget_total 3 src/widget.c int widget_total(int count) {\n"
                              "widget_use 7 src/widget.c int widget_use(void) {\n"))
            (ggt-test-record "global" case "indexed" "." '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record
             "global" case "indexed" "."
             '("-v" "--result=grep" "--color=always" "--path-style=shorter"
               "--" "widget_total")
             raw-env xref-output)))
          (fixture
           (progn (ggt-test-seed-database project)
                  (ggt-test-install-plan case-root project case plan "indexed")))
          (ggtags-executable-directory (plist-get fixture :bin))
          (exec-path (cons (directory-file-name (plist-get fixture :bin)) exec-path))
          (source (find-file-noselect (expand-file-name "app/main.c" project)))
          (widget (find-file-noselect (expand-file-name "src/widget.c" project)))
          completion definition imenu xref-state visited returned)
     (with-current-buffer source
       (c-mode)
       (set-window-buffer (selected-window) source)
       (ggtags-mode 1)
       (ggtags-find-project)
       (goto-char (point-max))
       (insert "\nwidget_tot")
       (let ((start (- (point) (length "widget_tot"))))
         (setq completion
               (list :return (completion-at-point)
                     :text (buffer-substring-no-properties start (point))
                     :column (current-column)
                     :cache (copy-tree ggtags-completion-cache))))
       (delete-region (line-beginning-position) (point-max))
       (set-buffer-modified-p nil)
       (goto-char (point-min))
       (forward-line 3)
       (move-to-column 18)
       (let ((original-message (symbol-function 'message)))
         (cl-letf (((symbol-function 'message)
                    (lambda (format-string &rest arguments)
                      (apply #'ggt-test-observe-message
                             original-message format-string arguments))))
           (ggtags-show-definition "widget_total")
           (ggt-test-wait-index fixture 8)
           (ggt-test-wait-until
            "definition callback and message"
            (lambda ()
              (and ggt-test-message-ledger
                   (not (get-buffer " *ggtags-definition*")))))))
       (setq definition (reverse (copy-sequence ggt-test-message-ledger))))
     (with-current-buffer widget
       (c-mode)
       (ggtags-mode 1)
       (setq imenu (ggtags-build-imenu-index)))
     (with-current-buffer source
       (set-window-buffer (selected-window) source)
       (goto-char (point-min))
       (forward-line 3)
       (move-to-column 18)
       (condition-case condition
           (xref-find-definitions "widget_total")
         (error
          (error "GGTAGS xref failed: %S trace=%S"
                 condition (ggt-test-trace fixture project)))))
     (let ((xref-buffer (get-buffer "*xref*")))
       (unless (buffer-live-p xref-buffer)
         (error "GGTAGS public xref UI was not displayed"))
       (with-current-buffer xref-buffer
         (let ((position (point-min))
               previous items)
           (while (< position (point-max))
             (let ((item (get-text-property position 'xref-item)))
               (when (and item (not (eq item previous)))
                 (let* ((location (xref-item-location item))
                        (marker (xref-location-marker location)))
                   (push (list (substring-no-properties (xref-item-summary item))
                               (file-relative-name (buffer-file-name
                                                    (marker-buffer marker))
                                                   project)
                               (xref-location-line location))
                         items))
                 (setq previous item)))
             (setq position (next-single-property-change
                             position 'xref-item nil (point-max))))
           (setq xref-state
                 (list :mode major-mode
                       :text (buffer-substring-no-properties
                              (point-min) (point-max))
                       :font-lock-runs
                       (ggt-test-property-runs 'font-lock-face)
                       :face-runs (ggt-test-property-runs 'face)
                       :items (nreverse items)
                       :selected (buffer-name (window-buffer))))
           (goto-char (point-min))
           (xref-next-line-no-show)
           (call-interactively (key-binding (kbd "RET")))))
       (setq visited
             (with-current-buffer (window-buffer) (ggt-test-location)))
       (xref-go-back)
       (setq returned
             (with-current-buffer (window-buffer) (ggt-test-location))))
     (list :completion completion :definition definition :imenu imenu
           :xref xref-state :visited visited :returned returned
           :fixture (ggt-test-fixture-state fixture project)))))"#;
    let expected = expect![[
        r#"OK (:result (:completion (:return t :text "widget_total" :column 12 :cache ("widget_total$" "widget_total")) :definition ("int widget_total(int count) { [guess]") :imenu (("widget_total" 3 ggtags-goto-imenu-index) ("widget_use" 7 ggtags-goto-imenu-index)) :xref (:mode xref--xref-buffer-mode :text "src/widget.c\n3:int widget_total(int count) {\nsrc/widget_alt.c\n3:int widget_total(int count) {\n" :font-lock-runs (("widget_total" (ansi-color-bold (:foreground "red3"))) ("widget_total" (ansi-color-bold (:foreground "red3")))) :face-runs (("src/widget.c\n" xref-file-header) ("3" xref-line-number) (":" shadow) ("src/widget_alt.c\n" xref-file-header) ("3" xref-line-number) (":" shadow)) :items (("int widget_total(int count) {" "src/widget.c" 3) ("int widget_total(int count) {" "src/widget_alt.c" 3)) :selected "*xref*") :visited ("widget.c" 3 11 "int widget_total(int count) {" t) :returned ("main.c" 4 18 "      int value = widget_total(2);" t) :fixture (:index 12 :planned 12 :generation "indexed" :misses nil :help-stdout-contracts (:count 2 :values ("status-only:process-file-destination-nil:global-6.7:8590:3f269245f1c7abedd402629112a843e238947aed79f6556b4228f369e1b7af39")) :recording-stream-contracts (:count 12 :values ("b0c4324e7fc48346b1b32a11a76cff33b4593176b7f6a5ac57f6a01bf6e84853" "8fd149ac622eda6d0dac6eca9906d7b5ed14cd375a8f5c4532fe758e2f69ac2e" "eadb6c511e574df8ca19d1b205a972ea31770dc39c5056fb00c939557bdb72e1" "0ef548d0c6ad2408baa68a6a53b0c67fff20535841266fb48069c8014958db73" "6880774a3c5fcf902fd3d3aa1e2e5930a065ade303758227f8597273486a0f7e" "fcd673b554223c39ab98c616ec053063740282f528b4d81b1c4ab3e34cc6e9fa" "bb8eab3a176a2494c3af6a0630d1eafae2077f55e208e939a8da194a1f4fb5b1")) :trace ("CALL" "0" "global" "ggtags-ide-integrations" "indexed" "app" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "1" "global" "ggtags-ide-integrations" "indexed" "." "1" "-p" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "2" "global" "ggtags-ide-integrations" "indexed" "." "1" "-crs" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "3" "global" "ggtags-ide-integrations" "indexed" "." "3" "--path-style" "shorter" "--help" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "4" "global" "ggtags-ide-integrations" "indexed" "." "2" "--color" "--help" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "5" "global" "ggtags-ide-integrations" "indexed" "app" "2" "-c" "widget_tot" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "6" "global" "ggtags-ide-integrations" "indexed" "app" "2" "-c" "widget_total" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "7" "global" "ggtags-ide-integrations" "indexed" "app" "3" "--result=grep" "--path-style=absolute" "widget_total" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "8" "global" "ggtags-ide-integrations" "indexed" "src" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "9" "global" "ggtags-ide-integrations" "indexed" "src" "3" "-x" "-f" "widget.c" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "10" "global" "ggtags-ide-integrations" "indexed" "." "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "11" "global" "ggtags-ide-integrations" "indexed" "." "6" "-v" "--result=grep" "--color=always" "--path-style=shorter" "--" "widget_total" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END"))) :cleanup (:new-buffers nil :new-processes nil :compilation-last-buffer nil :compilation-processes nil :new-timers 0 :root-exists nil :root-owned nil :window-restored t :navigation nil :xref-history (0 0) :start-marker nil :start-file nil :line-overlay nil :highlight-overlay nil :project-count 0 :prompts-remaining nil :prompt-calls nil :body-error nil :cleanup-errors nil))"#
    ]];
    ParityBatchCase::value(
        "integrations_complete_describe_index_and_navigate_real_xref_ui",
        elisp_form,
        expected,
    )
}

fn search_toolbox_routes_public_options_across_real_results() -> ParityBatchCase {
    let elisp_form = r##"(ggt-test-run
 "ggtags-search-toolbox"
 (lambda (case-root project)
   (let* ((case "ggtags-search-toolbox")
          (raw-env (ggt-test-env project))
          (library (file-name-as-directory
                    (expand-file-name "vendor-lib" project)))
          (library-env (ggt-test-env project nil nil nil nil
                                     (directory-file-name library)))
          (plan
           (list
            (ggt-test-record "global" case "indexed" "app" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-p") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-crs") raw-env
                             "WIDGET_H\ncount\nwidget_total\nwidget_use\n")
            (ggt-test-record "global" case "indexed" "."
                             '("--path-style" "shorter" "--help") raw-env)
            (ggt-test-record "global" case "indexed" "."
                             '("--color" "--help") raw-env)
            (ggt-test-record "global" case "indexed" "app"
                             '("--nearness=." "--help") raw-env)
            (ggt-test-record "global" case "indexed" "." '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record
             "global" case "indexed" "."
             '("-v" "--result=grep" "--color=always" "--path-style=shorter"
               "--reference" "--nearness=app/main.c" "--" "widget_total")
             library-env
             (concat "object not found (using '" (directory-file-name project)
                     "/GRTAGS').\n"))
            (ggt-test-record "global" case "indexed" "vendor-lib" '("-pr") raw-env
                             (concat (directory-file-name library) "\n"))
            (ggt-test-record "global" case "indexed" "vendor-lib" '("-p") raw-env
                             (concat (directory-file-name library) "\n"))
            (ggt-test-record "global" case "indexed" "vendor-lib" '("-crs") raw-env
                             "widget_total\n")
            (ggt-test-record "global" case "indexed" "vendor-lib"
                             '("--path-style" "shorter" "--help") raw-env)
            (ggt-test-record "global" case "indexed" "vendor-lib"
                             '("--color" "--help") raw-env)
            (ggt-test-record
             "global" case "indexed" "vendor-lib"
             '("-v" "--result=grep" "--color=always" "--path-style=shorter"
               "--reference" "widget_total")
             raw-env
             (concat
              "libwidget.c:2:int library_use(void) { return widget_total(7); }\n"
              "1 object located (using '" (directory-file-name library)
              "/GRTAGS').\n"))
            ;; Public navigation abort resolves the project of the visited file
            ;; before restoring the originating marker.
            (ggt-test-record "global" case "indexed" "vendor-lib" '("-pr") raw-env
                             (concat (directory-file-name library) "\n"))
            (ggt-test-record "global" case "indexed" "vendor-lib" '("-pr") raw-env
                             (concat (directory-file-name library) "\n"))
            (ggt-test-record "global" case "indexed" "app" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record
             "global" case "indexed" "."
             '("-v" "--result=ctags-x" "--ignore-case" "--color=always"
               "--path-style=shorter" "--symbol" "--" "Widget_Use")
             raw-env
             "widget_use 7 src/widget.c int widget_use(void) {\n")
            (ggt-test-record "global" case "indexed" "." '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "src" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "app" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record
             "global" case "indexed" "."
             '("-v" "--result=grep" "--color=always" "--path-style=shorter"
               "--other" "--grep" "--invert-match" "--" "TODO")
             raw-env
             (concat
              "docs/design Ω notes.txt:1:Widget totals in a path with spaces and Unicode Ω.\n"
              "1 object located (using '" (directory-file-name project)
              "/GPATH').\n"))
            (ggt-test-record "global" case "indexed" "." '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "docs" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "app" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record
             "global" case "indexed" "."
             '("-v" "--result=path" "--color=always" "--path-style=shorter"
               "--other" "--path" "--" "widget")
             raw-env
             (concat "src/widget.c\nsrc/widget_alt.c\n2 objects located (using '"
                     (directory-file-name project) "/GPATH').\n"))
            (ggt-test-record "global" case "indexed" "." '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "app" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "src" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record
             "global" case "indexed" "src"
             '("-v" "--result=grep" "--color=always" "--path-style=shorter"
               "-l" "--" "widget_.*")
             raw-env
             (concat
              "widget.c:3:int widget_total(int count) {\n"
              "widget.c:7:int widget_use(void) {\n"
              "widget_alt.c:3:int widget_total(int count) {\n"
              "3 objects located (using '" (directory-file-name project)
              "/GTAGS').\n"))
            (ggt-test-record "global" case "indexed" "src" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))))
          (fixture
           (progn
             (ggt-test-seed-database project)
             (make-directory library)
             (ggt-test-write-file
              project "vendor-lib/libwidget.c"
              "#include \"../src/widget.h\"\nint library_use(void) { return widget_total(7); }\n")
             (ggt-test-seed-database library)
             (ggt-test-install-plan case-root project case plan "indexed")))
          (ggtags-executable-directory (plist-get fixture :bin))
          (exec-path (cons (directory-file-name (plist-get fixture :bin)) exec-path))
          (ggtags-auto-jump-to-match nil)
          (source (find-file-noselect (expand-file-name "app/main.c" project)))
          observations)
     (with-current-buffer source
       (c-mode)
       (set-window-buffer (selected-window) source)
       (ggtags-mode 1)
       (ggtags-find-project)
       (goto-char (point-min))
       (forward-line 3)
       (move-to-column 18))
     (cl-labels
         ((observe
           (label expected-index)
           (let ((buffer (ggt-test-wait-global fixture expected-index))
                 state visited)
             (with-current-buffer buffer
               (setq state
                     (list :label label :mode major-mode
                           :exit (copy-tree ggtags-global-exit-info)
                           :rows ggtags-global-output-lines
                           :text (ggt-test-global-text buffer)
                           :history (ggtags-global-current-search)))
               (setq next-error-last-buffer buffer)
               (goto-char (point-min)))
             (next-error 1 t)
             (setq visited
                   (with-current-buffer (window-buffer) (ggt-test-location)))
             (ggtags-navigation-mode-abort)
             (list :result state :visited visited
                   :returned
                   (with-current-buffer (window-buffer) (ggt-test-location))))))
       (with-current-buffer source
         (set-window-buffer (selected-window) source)
         (let ((ggtags-sort-by-nearness t)
               (ggtags-global-search-libpath-for-reference t)
               (ggtags-process-environment
                (list (concat "GTAGSLIBPATH=" (directory-file-name library)))))
           (ggtags-find-reference "widget_total")))
       (push (observe 'reference-library 14) observations)
       (with-current-buffer source
         (set-window-buffer (selected-window) source)
         (let ((ggtags-global-output-format 'ctags-x)
               (ggtags-global-ignore-case t))
           (ggtags-find-other-symbol "Widget_Use")))
       (push (observe 'other-symbol 19) observations)
       (with-current-buffer source
         (set-window-buffer (selected-window) source)
         (let ((ggtags-global-treat-text t))
           (ggtags-grep "TODO" t)))
       (push (observe 'inverted-text-grep 24) observations)
       (with-current-buffer source
         (set-window-buffer (selected-window) source)
         (let ((ggtags-global-treat-text t))
           (ggtags-find-file "widget")))
       (push (observe 'path-search 29) observations)
       (with-current-buffer source
         (set-window-buffer (selected-window) source)
         (ggtags-find-tag-regexp "widget_.*"
                                 (expand-file-name "src" project)))
       (push (observe 'local-regexp 33) observations))
     (list :searches (nreverse observations)
           :history-count (length ggtags-global-search-history)
           :fixture (ggt-test-fixture-state fixture project)))))"##;
    let expected = expect![[
        r##"OK (:result (:searches ((:result (:label reference-library :mode ggtags-global-mode :exit (0 1 #("GRTAGS" 0 6 (fontified nil))) :rows 2 :text "-*- mode: ggtags-global; default-directory: \"[ROOT]/vendor-lib/\" -*-\nGlobal <STATUS>\n\n[CASE]/bin/global -v --result=grep --color=always --path-style=shorter --reference widget_total\nlibwidget.c:2:int library_use(void) { return widget_total(7); }\n1 object located (using '[ROOT]/vendor-lib/GRTAGS').\n\nGlobal <STATUS>" :history ("--reference widget_total" "[ROOT]/vendor-lib/" nil 1 "-*- mode: ggtags-global; default-directory: \"[ROOT]/vendor-lib/\" -*-")) :visited ("libwidget.c" 2 0 "int library_use(void) { return widget_total(7); }" t) :returned ("main.c" 4 18 "      int value = widget_total(2);" t)) (:result (:label other-symbol :mode ggtags-global-mode :exit (0 0 nil) :rows 1 :text "-*- mode: ggtags-global; default-directory: \"[ROOT]/\" -*-\nGlobal <STATUS>\n\n[CASE]/bin/global -v --result=ctags-x --ignore-case --color=always --path-style=shorter --symbol -- Widget_Use\nwidget_use 7 src/widget.c int widget_use(void) {\n\nGlobal <STATUS>" :history ("[CASE]/bin/global -v --result=ctags-x --ignore-case --color=always --path-style=shorter --symbol -- Widget_Use" "[ROOT]/" nil 1 "-*- mode: ggtags-global; default-directory: \"[ROOT]/\" -*-")) :visited ("widget.c" 7 0 "int widget_use(void) {" t) :returned ("widget.c" 7 0 "int widget_use(void) {" t)) (:result (:label inverted-text-grep :mode ggtags-global-mode :exit (0 1 #("GPATH" 0 5 (fontified nil))) :rows 2 :text "-*- mode: ggtags-global; default-directory: \"[ROOT]/\" -*-\nGlobal <STATUS>\n\n[CASE]/bin/global -v --result=grep --color=always --path-style=shorter --other --grep --invert-match -- \"TODO\"\ndocs/design Ω notes.txt:1:Widget totals in a path with spaces and Unicode Ω.\n1 object located (using '[ROOT]/GPATH').\n\nGlobal <STATUS>" :history ("--other --grep --invert-match -- \"TODO\"" "[ROOT]/" nil 1 "-*- mode: ggtags-global; default-directory: \"[ROOT]/\" -*-")) :visited ("design Ω notes.txt" 1 0 "Widget totals in a path with spaces and Unicode Ω." t) :returned ("main.c" 4 18 "      int value = widget_total(2);" t)) (:result (:label path-search :mode ggtags-global-mode :exit (0 2 #("GPATH" 0 5 (fontified nil))) :rows 3 :text "-*- mode: ggtags-global; default-directory: \"[ROOT]/\" -*-\nGlobal <STATUS>\n\n[CASE]/bin/global -v --result=path --color=always --path-style=shorter --other --path -- \"widget\"\nsrc/widget.c\nsrc/widget_alt.c\n2 objects located (using '[ROOT]/GPATH').\n\nGlobal <STATUS>" :history ("[CASE]/bin/global -v --result=path --color=always --path-style=shorter --other --path -- \"widget\"" "[ROOT]/" nil 1 "-*- mode: ggtags-global; default-directory: \"[ROOT]/\" -*-")) :visited ("widget.c" 1 0 "#include \"widget.h\"" t) :returned ("main.c" 4 18 "      int value = widget_total(2);" t)) (:result (:label local-regexp :mode ggtags-global-mode :exit (0 3 #("GTAGS" 0 5 (fontified nil))) :rows 4 :text "-*- mode: ggtags-global; default-directory: \"[ROOT]/src/\" -*-\nGlobal <STATUS>\n\n[CASE]/bin/global -v --result=grep --color=always --path-style=shorter -l -- \"widget_.*\"\nwidget.c:3:int widget_total(int count) {\nwidget.c:7:int widget_use(void) {\nwidget_alt.c:3:int widget_total(int count) {\n3 objects located (using '[ROOT]/GTAGS').\n\nGlobal <STATUS>" :history ("-l -- \"widget_.*\"" "[ROOT]/src/" nil 1 "-*- mode: ggtags-global; default-directory: \"[ROOT]/src/\" -*-")) :visited ("widget.c" 3 0 "int widget_total(int count) {" t) :returned ("main.c" 4 18 "      int value = widget_total(2);" t))) :history-count 5 :fixture (:index 34 :planned 34 :generation "indexed" :misses nil :help-stdout-contracts (:count 5 :values ("status-only:process-file-destination-nil:global-6.7:8590:3f269245f1c7abedd402629112a843e238947aed79f6556b4228f369e1b7af39")) :recording-stream-contracts (:count 34 :values ("b0c4324e7fc48346b1b32a11a76cff33b4593176b7f6a5ac57f6a01bf6e84853" "8fd149ac622eda6d0dac6eca9906d7b5ed14cd375a8f5c4532fe758e2f69ac2e" "eadb6c511e574df8ca19d1b205a972ea31770dc39c5056fb00c939557bdb72e1" "715cff98026f812f6397a6fa9ae8c368903ba8d7bf460dff19039ff6fda12cbf" "b28bb37d7f39373f0df865d930effdd7540a242ca2d8ebf7cbf29c80260a9426" "0ef548d0c6ad2408baa68a6a53b0c67fff20535841266fb48069c8014958db73" "6cd18a38b5f3b92cf32563b3a01f6519c64f71f0424497562ee0c04758945a30" "e764edf4de71389be469cbd77d9d3b555990c7b298ceaf916395edd6d75c3bfd" "dd5dabcd6f8246006cc545ad66268ca10b90699ab1a85a6dfc7e4d7e97ccd270" "d7e11de43cb5d5f1f59a618fce06d5f32b772b5c76a648aff3dd6904eb5da5c4" "dd7f3293b503b995b06e45f0f7ba4073ccddae69f119a99bb09331841b61d6c3")) :trace ("CALL" "0" "global" "ggtags-search-toolbox" "indexed" "app" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "1" "global" "ggtags-search-toolbox" "indexed" "." "1" "-p" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "2" "global" "ggtags-search-toolbox" "indexed" "." "1" "-crs" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "3" "global" "ggtags-search-toolbox" "indexed" "." "3" "--path-style" "shorter" "--help" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "4" "global" "ggtags-search-toolbox" "indexed" "." "2" "--color" "--help" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "5" "global" "ggtags-search-toolbox" "indexed" "app" "2" "--nearness=." "--help" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "6" "global" "ggtags-search-toolbox" "indexed" "." "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "7" "global" "ggtags-search-toolbox" "indexed" "." "8" "-v" "--result=grep" "--color=always" "--path-style=shorter" "--reference" "--nearness=app/main.c" "--" "widget_total" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=[ROOT]/vendor-lib" "LC_ALL=C.UTF-8" "END" "CALL" "8" "global" "ggtags-search-toolbox" "indexed" "vendor-lib" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "9" "global" "ggtags-search-toolbox" "indexed" "vendor-lib" "1" "-p" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "10" "global" "ggtags-search-toolbox" "indexed" "vendor-lib" "1" "-crs" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "11" "global" "ggtags-search-toolbox" "indexed" "vendor-lib" "3" "--path-style" "shorter" "--help" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "12" "global" "ggtags-search-toolbox" "indexed" "vendor-lib" "2" "--color" "--help" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "13" "global" "ggtags-search-toolbox" "indexed" "vendor-lib" "6" "-v" "--result=grep" "--color=always" "--path-style=shorter" "--reference" "widget_total" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "14" "global" "ggtags-search-toolbox" "indexed" "vendor-lib" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "15" "global" "ggtags-search-toolbox" "indexed" "vendor-lib" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "16" "global" "ggtags-search-toolbox" "indexed" "app" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "17" "global" "ggtags-search-toolbox" "indexed" "." "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "18" "global" "ggtags-search-toolbox" "indexed" "." "8" "-v" "--result=ctags-x" "--ignore-case" "--color=always" "--path-style=shorter" "--symbol" "--" "Widget_Use" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "19" "global" "ggtags-search-toolbox" "indexed" "." "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "20" "global" "ggtags-search-toolbox" "indexed" "src" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "21" "global" "ggtags-search-toolbox" "indexed" "app" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "22" "global" "ggtags-search-toolbox" "indexed" "." "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "23" "global" "ggtags-search-toolbox" "indexed" "." "9" "-v" "--result=grep" "--color=always" "--path-style=shorter" "--other" "--grep" "--invert-match" "--" "TODO" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "24" "global" "ggtags-search-toolbox" "indexed" "." "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "25" "global" "ggtags-search-toolbox" "indexed" "docs" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "26" "global" "ggtags-search-toolbox" "indexed" "app" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "27" "global" "ggtags-search-toolbox" "indexed" "." "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "28" "global" "ggtags-search-toolbox" "indexed" "." "8" "-v" "--result=path" "--color=always" "--path-style=shorter" "--other" "--path" "--" "widget" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "29" "global" "ggtags-search-toolbox" "indexed" "." "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "30" "global" "ggtags-search-toolbox" "indexed" "app" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "31" "global" "ggtags-search-toolbox" "indexed" "src" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "32" "global" "ggtags-search-toolbox" "indexed" "src" "7" "-v" "--result=grep" "--color=always" "--path-style=shorter" "-l" "--" "widget_.*" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "33" "global" "ggtags-search-toolbox" "indexed" "src" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END"))) :cleanup (:new-buffers nil :new-processes nil :compilation-last-buffer nil :compilation-processes nil :new-timers 0 :root-exists nil :root-owned nil :window-restored t :navigation nil :xref-history (0 0) :start-marker nil :start-file nil :line-overlay nil :highlight-overlay nil :project-count 0 :prompts-remaining nil :prompt-calls nil :body-error nil :cleanup-errors nil))"##
    ]];
    ParityBatchCase::value(
        "search_toolbox_routes_public_options_across_real_results",
        elisp_form,
        expected,
    )
}

fn failures_distinguish_missing_database_corruption_and_recover() -> ParityBatchCase {
    let elisp_form = r#"(ggt-test-run
 "ggtags-failure-recovery"
 (lambda (case-root project)
   (let* ((case "ggtags-failure-recovery")
          (raw-env (ggt-test-env project))
          (plan
           (list
            (ggt-test-record "global" case "indexed" "app" '("-pr") raw-env
                             "" "global: GTAGS not found.\n" 3)
            (ggt-test-record "global" case "indexed" "app" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-p") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-crs") raw-env
                             "WIDGET_H\ncount\nwidget_total\nwidget_use\n")
            (ggt-test-record "global" case "indexed" "."
                             '("--path-style" "shorter" "--help") raw-env)
            (ggt-test-record "global" case "indexed" "."
                             '("--color" "--help") raw-env)
            (ggt-test-record "global" case "indexed" "." '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record
             "global" case "indexed" "."
             '("-v" "--result=grep" "--color=always" "--path-style=shorter"
               "--" "MISSING_DB")
             raw-env "" "global: GTAGS not found.\n" 3)
            (ggt-test-record "global" case "indexed" "app" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-p") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-crs") raw-env
                             "WIDGET_H\ncount\nwidget_total\nwidget_use\n")
            (ggt-test-record "global" case "indexed" "."
                             '("--path-style" "shorter" "--help") raw-env)
            (ggt-test-record "global" case "indexed" "."
                             '("--color" "--help") raw-env)
            (ggt-test-record "global" case "indexed" "." '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record
             "global" case "indexed" "."
             '("-v" "--result=grep" "--color=always" "--path-style=shorter"
               "--" "FAIL")
             raw-env "" "global: simulated database corruption\n" 2)
            (ggt-test-record "global" case "indexed" "app" '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record "global" case "indexed" "." '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))
            (ggt-test-record
             "global" case "indexed" "."
             '("-v" "--result=grep" "--color=always" "--path-style=shorter"
               "--" "widget_total")
             raw-env
             (concat
              "src/widget.c:3:int widget_total(int count) {\n"
              "src/widget_alt.c:3:int widget_total(int count) {\n"
              "2 objects located (using '" (directory-file-name project)
              "/GTAGS').\n"))
            (ggt-test-record "global" case "indexed" "." '("-pr") raw-env
                             (concat (directory-file-name project) "\n"))))
          (fixture (ggt-test-install-plan case-root project case plan "indexed"))
          (ggtags-executable-directory (plist-get fixture :bin))
          (exec-path (cons (directory-file-name (plist-get fixture :bin)) exec-path))
          (ggtags-auto-jump-to-match nil)
          (source (find-file-noselect (expand-file-name "app/main.c" project)))
          (original-message (symbol-function 'message))
          boundaries missing-state corruption-state recovery-state)
     (cl-labels
         ((capture-signal
           (thunk)
           (condition-case condition
               (progn (funcall thunk) :unexpected-success)
             (t (list :signal (car condition) :data (cdr condition)))))
          (result-state
           (buffer)
           (with-current-buffer buffer
             (list :mode major-mode
                   :exit-info (copy-tree ggtags-global-exit-info)
                   :navigation ggtags-navigation-mode
                   :displayed (and (get-buffer-window buffer) t)
                   :text (ggt-test-global-text buffer)))))
       (with-current-buffer source
         (c-mode)
         (set-window-buffer (selected-window) source)
         (setq boundaries
               (list
                :definition
                (capture-signal
                 (lambda () (ggtags-find-definition "widget_total")))
                :empty-root
                (capture-signal (lambda () (ggtags-create-tags "")))
                :history
                (capture-signal (lambda () (ggtags-view-search-history))))))
       (ggt-test-seed-database project)
       (cl-letf (((symbol-function 'message)
                  (lambda (format-string &rest arguments)
                    (apply #'ggt-test-observe-message
                           original-message format-string arguments))))
         (with-current-buffer source
           (ggtags-mode 1)
           (ggtags-find-project)
           (ggtags-find-definition "MISSING_DB"))
         (setq missing-state
               (let ((buffer (ggt-test-wait-global fixture 8)))
                 (append
                  (result-state buffer)
                  (list :project-cached
                        (and (gethash (file-name-as-directory project)
                                      ggtags-projects)
                             t)
                        :messages (reverse (copy-sequence ggt-test-message-ledger))))))
         (ggtags-navigation-mode-abort)
         (setq ggt-test-message-ledger nil)
         (with-current-buffer source
           (set-window-buffer (selected-window) source)
           (ggtags-find-definition "FAIL"))
         (setq corruption-state
               (let ((buffer (ggt-test-wait-global fixture 15)))
                 (append
                  (result-state buffer)
                  (list :project-cached
                        (and (gethash (file-name-as-directory project)
                                      ggtags-projects)
                             t)
                        :messages (reverse (copy-sequence ggt-test-message-ledger))))))
         (ggtags-navigation-mode-abort)
         (setq ggt-test-message-ledger nil)
         (with-current-buffer source
           (set-window-buffer (selected-window) source)
           (ggtags-find-definition "widget_total"))
         (setq recovery-state
               (let ((buffer (ggt-test-wait-global fixture 18)))
                 (append
                  (result-state buffer)
                  (list :history (ggtags-global-current-search)
                        :project-cached
                        (and (gethash (file-name-as-directory project)
                                      ggtags-projects)
                             t)
                        :messages (reverse (copy-sequence ggt-test-message-ledger))))))
         (ggtags-navigation-mode-abort))
       (list :boundaries boundaries
             :missing-database missing-state
             :corruption corruption-state
             :recovery recovery-state
             :source (with-current-buffer source (ggt-test-location))
             :fixture (ggt-test-fixture-state fixture project))))))"#;
    let expected = expect![[
        r##"OK (:result (:boundaries (:definition (:signal error :data ("File GTAGS not found")) :empty-root (:signal error :data ("No root directory provided")) :history (:signal user-error :data ("No search history"))) :missing-database (:mode ggtags-global-mode :exit-info (3 0 nil) :navigation t :displayed t :text "-*- mode: ggtags-global; default-directory: \"[ROOT]/\" -*-\nGlobal <STATUS>\n\n[CASE]/bin/global -v --result=grep --color=always --path-style=shorter -- MISSING_DB\nglobal: GTAGS not found.\n\nGlobal <STATUS>\n" :project-cached nil :messages ("Global exited abnormally with code 3" "WARNING: Global tag files missing in ‘[ROOT]/’")) :corruption (:mode ggtags-global-mode :exit-info (2 0 nil) :navigation t :displayed t :text "-*- mode: ggtags-global; default-directory: \"[ROOT]/\" -*-\nGlobal <STATUS>\n\n[CASE]/bin/global -v --result=grep --color=always --path-style=shorter -- FAIL\nglobal: simulated database corruption\n\nGlobal <STATUS>\n" :project-cached t :messages ("Global exited abnormally with code 2")) :recovery (:mode ggtags-global-mode :exit-info (0 2 #("GTAGS" 0 5 (fontified nil))) :navigation t :displayed t :text "-*- mode: ggtags-global; default-directory: \"[ROOT]/\" -*-\nGlobal <STATUS>\n\n[CASE]/bin/global -v --result=grep --color=always --path-style=shorter -- widget_total\nsrc/widget.c:3:int widget_total(int count) {\nsrc/widget_alt.c:3:int widget_total(int count) {\n2 objects located (using '[ROOT]/GTAGS').\n\nGlobal <STATUS>" :history ("-- widget_total" "[ROOT]/" nil 1 "-*- mode: ggtags-global; default-directory: \"[ROOT]/\" -*-") :project-cached t :messages ("Global found 2 definitions")) :source ("main.c" 1 0 "#include \"widget.h\"" t) :fixture (:index 19 :planned 19 :generation "indexed" :misses nil :help-stdout-contracts (:count 4 :values ("status-only:process-file-destination-nil:global-6.7:8590:3f269245f1c7abedd402629112a843e238947aed79f6556b4228f369e1b7af39")) :recording-stream-contracts (:count 19 :values ("396316a0b541a487816382800512d33018e810489683027b476fbc604c3a26dd" "b0c4324e7fc48346b1b32a11a76cff33b4593176b7f6a5ac57f6a01bf6e84853" "8fd149ac622eda6d0dac6eca9906d7b5ed14cd375a8f5c4532fe758e2f69ac2e" "eadb6c511e574df8ca19d1b205a972ea31770dc39c5056fb00c939557bdb72e1" "2fd4fa25336c5229d0e193d9dd26b155018a42b2c9011fbf32d48825df02c910" "73d4fd3a9781d99bb96c68a1d896656e22349196bb1609ec63dbfc6e5c2cc2dc")) :trace ("CALL" "0" "global" "ggtags-failure-recovery" "indexed" "app" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "1" "global" "ggtags-failure-recovery" "indexed" "app" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "2" "global" "ggtags-failure-recovery" "indexed" "." "1" "-p" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "3" "global" "ggtags-failure-recovery" "indexed" "." "1" "-crs" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "4" "global" "ggtags-failure-recovery" "indexed" "." "3" "--path-style" "shorter" "--help" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "5" "global" "ggtags-failure-recovery" "indexed" "." "2" "--color" "--help" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "6" "global" "ggtags-failure-recovery" "indexed" "." "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "7" "global" "ggtags-failure-recovery" "indexed" "." "6" "-v" "--result=grep" "--color=always" "--path-style=shorter" "--" "MISSING_DB" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "8" "global" "ggtags-failure-recovery" "indexed" "app" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "9" "global" "ggtags-failure-recovery" "indexed" "." "1" "-p" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "10" "global" "ggtags-failure-recovery" "indexed" "." "1" "-crs" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "11" "global" "ggtags-failure-recovery" "indexed" "." "3" "--path-style" "shorter" "--help" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "12" "global" "ggtags-failure-recovery" "indexed" "." "2" "--color" "--help" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "13" "global" "ggtags-failure-recovery" "indexed" "." "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "14" "global" "ggtags-failure-recovery" "indexed" "." "6" "-v" "--result=grep" "--color=always" "--path-style=shorter" "--" "FAIL" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "15" "global" "ggtags-failure-recovery" "indexed" "app" "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "16" "global" "ggtags-failure-recovery" "indexed" "." "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "17" "global" "ggtags-failure-recovery" "indexed" "." "6" "-v" "--result=grep" "--color=always" "--path-style=shorter" "--" "widget_total" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END" "CALL" "18" "global" "ggtags-failure-recovery" "indexed" "." "1" "-pr" "GTAGSROOT=None" "GTAGSDBPATH=None" "GTAGSLABEL=None" "GTAGSCONF=None" "GTAGSLIBPATH=None" "LC_ALL=C.UTF-8" "END"))) :cleanup (:new-buffers nil :new-processes nil :compilation-last-buffer nil :compilation-processes nil :new-timers 0 :root-exists nil :root-owned nil :window-restored t :navigation nil :xref-history (0 0) :start-marker nil :start-file nil :line-overlay nil :highlight-overlay nil :project-count 0 :prompts-remaining nil :prompt-calls nil :body-error nil :cleanup-errors nil))"##
    ]];
    ParityBatchCase::value(
        "failures_distinguish_missing_database_corruption_and_recover",
        elisp_form,
        expected,
    )
}

pub(super) fn public_workflow_cases() -> Vec<ParityBatchCase> {
    vec![
        mode_activation_highlight_and_restoration(),
        creates_updates_saves_and_deletes_a_real_project(),
        compilation_search_navigation_and_history(),
        include_dwim_uses_real_key_and_auto_jumps(),
        integrations_complete_describe_index_and_navigate_real_xref_ui(),
        search_toolbox_routes_public_options_across_real_results(),
        failures_distinguish_missing_database_corruption_and_recover(),
    ]
}

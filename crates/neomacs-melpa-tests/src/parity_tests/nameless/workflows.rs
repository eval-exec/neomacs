use expect_test::expect;

use super::ParityBatchCase;

fn package_header_discovery_hides_code_but_preserves_copied_source() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((nameless-current-name nil)
        (nameless-discover-current-name t)
        (nameless-prefix ":")
        (nameless-private-prefix t)
        (nameless-global-aliases '(("fl" . "font-lock"))))
    (neomacs-nameless-test-setup
     (concat
      ";;; deploy-kit-mode-tests.el --- release helpers\n"
      "(defun deploy-kit-build (artifact)\n"
      "  (deploy-kit--sign artifact)\n"
      "  (font-lock-add-keywords nil deploy-kit-keywords)\n"
      "  (message \"deploy-kit-ready\"))\n"))
    (list
     :current-name nameless-current-name
     :mode nameless-mode
     :insert-binding (lookup-key nameless-mode-map (kbd "C-c C--"))
     :spans (neomacs-nameless-test-spans)
     :copy (neomacs-nameless-test-filtered-copy)
     :source (buffer-substring-no-properties (point-min) (point-max)))))
"##;
    let expect = expect![[
        r#"OK (:current-name "deploy-kit" :mode t :insert-binding nameless-insert-name :spans ((:range (4 15) :source "deploy-kit-" :composition ((11 58)) :display nil :face (nameless-face font-lock-comment-face)) (:range (56 67) :source "deploy-kit-" :composition ((11 58)) :display nil :face (nameless-face font-lock-function-name-face)) (:range (87 99) :source "deploy-kit--" :composition ((12 58 63 58)) :display nil :face (nameless-face)) (:range (117 127) :source "font-lock-" :composition ((10 102 63 108 63 58)) :display nil :face (nameless-face)) (:range (144 155) :source "deploy-kit-" :composition ((11 58)) :display nil :face (nameless-face)) (:range (177 188) :source "deploy-kit-" :composition nil :display ":" :face (nameless-face font-lock-string-face))) :copy (:text ";;; deploy-kit-mode-tests.el --- release helpers\n(defun deploy-kit-build (artifact)\n  (deploy-kit--sign artifact)\n  (font-lock-add-keywords nil deploy-kit-keywords)\n  (message \"deploy-kit-ready\"))\n" :composition nil :display nil :face t) :source ";;; deploy-kit-mode-tests.el --- release helpers\n(defun deploy-kit-build (artifact)\n  (deploy-kit--sign artifact)\n  (font-lock-add-keywords nil deploy-kit-keywords)\n  (message \"deploy-kit-ready\"))\n")"#
    ]];
    ParityBatchCase::value(
        "package_header_discovery_hides_code_but_preserves_copied_source",
        elisp_form,
        expect,
    )
}

fn insertion_binding_expands_current_and_file_local_alias_names() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (emacs-lisp-mode)
    (let ((nameless-current-name "deploy-kit")
          (nameless-discover-current-name nil)
          (nameless-separator "-")
          (nameless-global-aliases '(("fl" . "font-lock")))
          (nameless-aliases '(("fl" . "fancy-lock"))))
      (nameless-mode 1)
      (insert "(defun ")
      (execute-kbd-macro (kbd "C-c C--"))
      (insert "publish ()\n  (fl")
      (execute-kbd-macro (kbd "C-c C--"))
      (insert "add-keywords nil rules)\n  ;; unknown alias: unknown")
      (let ((unknown
             (condition-case error
                 (progn
                   (execute-kbd-macro (kbd "C-c C--"))
                   nil)
               (user-error (error-message-string error)))))
        (insert "\n  (")
        (let ((nameless-separator nil))
          (execute-kbd-macro (kbd "C-c C--")))
        (insert "))\n")
        (list
         :buffer (buffer-string)
         :unknown-alias unknown
         :point (point)
         :mode nameless-mode)))))
"##;
    let expect = expect![[
        r#"OK (:buffer "(defun deploy-kit-publish ()\n  (fancy-lock-add-keywords nil rules)\n  ;; unknown alias: unknown\n  (deploy-kit))\n" :unknown-alias "No name for alias ‘unknown’, see ‘nameless-aliases’" :point 112 :mode t)"#
    ]];
    ParityBatchCase::value(
        "insertion_binding_expands_current_and_file_local_alias_names",
        elisp_form,
        expect,
    )
}

fn underscore_binding_distinguishes_arguments_aliases_and_escaped_characters() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (emacs-lisp-mode)
    (let ((nameless-current-name "deploy-kit")
          (nameless-discover-current-name nil)
          (nameless-global-aliases '(("fl" . "font-lock"))))
      (nameless-mode 1)
      (use-local-map (copy-keymap (current-local-map)))
      (local-set-key (kbd "_") #'nameless-insert-name-or-self-insert)
      (insert "(defun deploy-kit-run (artifact")
      (execute-kbd-macro (kbd "_"))
      (insert ")\n  (")
      (execute-kbd-macro (kbd "_"))
      (insert "publish artifact)\n  (fl")
      (execute-kbd-macro (kbd "_"))
      (insert "add-keywords nil rules)\n  ?\\")
      (execute-kbd-macro (kbd "_"))
      (insert ")\n")
      (list
       :buffer (buffer-string)
       :point (point)
       :binding (lookup-key (current-local-map) (kbd "_"))
       :shared-binding (lookup-key emacs-lisp-mode-map (kbd "_"))))))
"##;
    let expect = expect![[
        r#"OK (:buffer "(defun deploy-kit-run (artifact_)\n  (deploy-kit-publish artifact)\n  (font-lock-add-keywords nil rules)\n  ?\\_)\n" :point 111 :binding nameless-insert-name-or-self-insert :shared-binding nil)"#
    ]];
    ParityBatchCase::value(
        "underscore_binding_distinguishes_arguments_aliases_and_escaped_characters",
        elisp_form,
        expect,
    )
}

fn private_and_nonhyphen_namespaces_follow_visible_prefix_configuration() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((nameless-current-name "deploy")
        (nameless-discover-current-name nil)
        (nameless-prefix "§")
        (nameless-separator "/")
        (nameless-private-prefix t)
        (nameless-global-aliases nil))
    (neomacs-nameless-test-setup
     (concat
      "(defun deploy/run ()\n"
      "  (deploy//internal \"deploy/message\"))\n"))
    (let ((double-prefix (neomacs-nameless-test-spans)))
      (nameless-mode -1)
      (setq nameless-private-prefix "!")
      (nameless-mode 1)
      (neomacs-nameless-test-fontify)
      (let ((custom-private-prefix (neomacs-nameless-test-spans)))
        (nameless-mode -1)
        (setq nameless-separator nil)
        (nameless-mode 1)
        (neomacs-nameless-test-fontify)
        (list
         :source (buffer-substring-no-properties (point-min) (point-max))
         :double-prefix double-prefix
         :custom-private-prefix custom-private-prefix
         :separator-preserved (neomacs-nameless-test-spans)
         :copy (neomacs-nameless-test-filtered-copy))))))
"##;
    let expect = expect![[
        r#"OK (:source "(defun deploy/run ()\n  (deploy//internal \"deploy/message\"))\n" :double-prefix ((:range (7 14) :source "deploy/" :composition ((7 167)) :display nil :face (nameless-face font-lock-function-name-face)) (:range (24 32) :source "deploy//" :composition ((8 167 63 167)) :display nil :face (nameless-face)) (:range (42 49) :source "deploy/" :composition nil :display "§" :face (nameless-face font-lock-string-face))) :custom-private-prefix ((:range (7 14) :source "deploy/" :composition ((7 167)) :display nil :face (nameless-face font-lock-function-name-face)) (:range (24 32) :source "deploy//" :composition ((8 33)) :display nil :face (nameless-face)) (:range (42 49) :source "deploy/" :composition nil :display "§" :face (nameless-face font-lock-string-face))) :separator-preserved ((:range (7 13) :source "deploy" :composition ((6 167)) :display nil :face (nameless-face font-lock-function-name-face)) (:range (24 30) :source "deploy" :composition ((6 167)) :display nil :face (nameless-face)) (:range (42 48) :source "deploy" :composition nil :display "§" :face (nameless-face font-lock-string-face))) :copy (:text "(defun deploy/run ()\n  (deploy//internal \"deploy/message\"))\n" :composition nil :display nil :face t))"#
    ]];
    ParityBatchCase::value(
        "private_and_nonhyphen_namespaces_follow_visible_prefix_configuration",
        elisp_form,
        expect,
    )
}

fn real_file_local_alias_refreshes_presentation_and_persisted_configuration() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root (expand-file-name "nameless-file-locals"
                               (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (file (expand-file-name "pipeline.el" root))
       buffer)
  (make-directory root t)
  (with-temp-file file
    (insert
     "(setq result (seq-map #'identity items))\n"
     "(font-lock-add-keywords nil rules)\n"
     "\n"
     ";; Local Variables:\n"
     ";; nameless-aliases: ((\"sq\" . \"seq\"))\n"
     ";; End:\n"))
  (unwind-protect
      (progn
        (let ((enable-local-variables :safe))
          (setq buffer (find-file-noselect file)))
        (with-current-buffer buffer
          (let ((nameless-current-name nil)
                (nameless-discover-current-name nil)
                (nameless-global-aliases '(("fl" . "font-lock"))))
            (nameless-mode 1)
            (neomacs-nameless-test-fontify)
            (let ((initial-value nameless-aliases)
                  (initial (neomacs-nameless-test-spans)))
              (require 'files-x)
              (add-file-local-variable
               'nameless-aliases '(("sequence" . "seq")))
              (save-buffer)
              (let ((enable-local-variables :safe))
                (hack-local-variables))
              (neomacs-nameless-test-fontify)
              (list
               :initial-value initial-value
               :initial initial
               :refreshed-value nameless-aliases
               :refreshed (neomacs-nameless-test-spans)
               :source (buffer-substring-no-properties
                        (point-min) (point-max))
               :copy (neomacs-nameless-test-filtered-copy)
               :disk-bytes
               (with-temp-buffer
                 (insert-file-contents-literally file)
                 (buffer-string)))))))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer))
    (delete-directory root t)))
"##;
    let expect = expect![[
        r#"OK (:initial-value (("sq" . "seq")) :initial ((:range (14 18) :source "seq-" :composition ((4 115 63 113 63 58)) :display nil :face (nameless-face)) (:range (42 52) :source "font-lock-" :composition ((10 102 63 108 63 58)) :display nil :face (nameless-face))) :refreshed-value (("sequence" . "seq")) :refreshed ((:range (14 18) :source "seq-" :composition ((4 115 63 101 63 113 63 117 63 101 63 110 63 99 63 101 63 58)) :display nil :face (nameless-face)) (:range (42 52) :source "font-lock-" :composition ((10 102 63 108 63 58)) :display nil :face (nameless-face))) :source "(setq result (seq-map #'identity items))\n(font-lock-add-keywords nil rules)\n\n;; Local Variables:\n;; nameless-aliases: ((\"sequence\" . \"seq\"))\n;; End:\n" :copy (:text "(setq result (seq-map #'identity items))\n(font-lock-add-keywords nil rules)\n\n;; Local Variables:\n;; nameless-aliases: ((\"sequence\" . \"seq\"))\n;; End:\n" :composition nil :display nil :face t) :disk-bytes "(setq result (seq-map #'identity items))\n(font-lock-add-keywords nil rules)\n\n;; Local Variables:\n;; nameless-aliases: ((\"sequence\" . \"seq\"))\n;; End:\n")"#
    ]];
    ParityBatchCase::value(
        "real_file_local_alias_refreshes_presentation_and_persisted_configuration",
        elisp_form,
        expect,
    )
}

fn disabling_mode_removes_presentation_and_copy_filter_then_reenables_cleanly() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((nameless-current-name "release-tools")
        (nameless-discover-current-name nil)
        (nameless-global-aliases nil))
    (neomacs-nameless-test-setup
     (concat
      "(defun release-tools-publish ()\n"
      "  (message \"release-tools-ready\"))\n"))
    (let ((active-spans (neomacs-nameless-test-spans))
          (active-copy (neomacs-nameless-test-filtered-copy))
          (filter-installed
           (and (advice-function-member-p
                 #'nameless--filter-string
                 filter-buffer-substring-function)
                t)))
      (nameless-mode -1)
      (neomacs-nameless-test-fontify)
      (let ((disabled
             (list
              :mode nameless-mode
              :spans (neomacs-nameless-test-spans)
              :keywords nameless--font-lock-keywords
              :filter-installed
              (and (advice-function-member-p
                    #'nameless--filter-string
                    filter-buffer-substring-function)
                   t)
              :hook-installed
              (and (memq #'nameless--after-hack-local-variables
                         hack-local-variables-hook)
                   t))))
        (goto-char (point-max))
        (insert "(release-tools-verify)\n")
        (nameless-mode 1)
        (neomacs-nameless-test-fontify)
        (list
         :active-spans active-spans
         :active-copy active-copy
         :filter-installed filter-installed
         :disabled disabled
         :reenabled
         (list
          :mode nameless-mode
          :spans (neomacs-nameless-test-spans)
          :copy (neomacs-nameless-test-filtered-copy)))))))
"##;
    let expect = expect![[
        r#"OK (:active-spans ((:range (7 21) :source "release-tools-" :composition ((14 58)) :display nil :face (nameless-face font-lock-function-name-face)) (:range (44 58) :source "release-tools-" :composition nil :display ":" :face (nameless-face font-lock-string-face))) :active-copy (:text "(defun release-tools-publish ()\n  (message \"release-tools-ready\"))\n" :composition nil :display nil :face t) :filter-installed t :disabled (:mode nil :spans nil :keywords nil :filter-installed nil :hook-installed nil) :reenabled (:mode t :spans ((:range (7 21) :source "release-tools-" :composition ((14 58)) :display nil :face (nameless-face font-lock-function-name-face)) (:range (44 58) :source "release-tools-" :composition nil :display ":" :face (nameless-face font-lock-string-face)) (:range (68 82) :source "release-tools-" :composition ((14 58)) :display nil :face (nameless-face))) :copy (:text "(defun release-tools-publish ()\n  (message \"release-tools-ready\"))\n(release-tools-verify)\n" :composition nil :display nil :face t)))"#
    ]];
    ParityBatchCase::value(
        "disabling_mode_removes_presentation_and_copy_filter_then_reenables_cleanly",
        elisp_form,
        expect,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        package_header_discovery_hides_code_but_preserves_copied_source(),
        insertion_binding_expands_current_and_file_local_alias_names(),
        underscore_binding_distinguishes_arguments_aliases_and_escaped_characters(),
        private_and_nonhyphen_namespaces_follow_visible_prefix_configuration(),
        real_file_local_alias_refreshes_presentation_and_persisted_configuration(),
        disabling_mode_removes_presentation_and_copy_filter_then_reenables_cleanly(),
    ]
}

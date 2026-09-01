use expect_test::expect;

use super::ParityBatchCase;

fn a_project_session_classifies_buffers_and_restores_scoped_configuration() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-window-purpose-test-with-configuration
    nil
    '((prog-mode . source))
    '(("*project-shell*" . terminal))
    '(("^build-" . diagnostics))
  (let* ((root (expand-file-name "window-purpose-config/"
                                 (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
         (readme (expand-file-name "project/README.md" root))
         (source (get-buffer-create "project-main.el"))
         (build (get-buffer-create "build-project"))
         (shell (get-buffer-create "*project-shell*"))
         (review (get-buffer-create "*review-request*"))
         file-buffer)
    (unwind-protect
        (progn
          (make-directory (file-name-directory readme) t)
          (with-temp-file readme
            (insert "Project notes\n"))
          (setq file-buffer (find-file-noselect readme))
          (with-current-buffer source
            (emacs-lisp-mode))
          (purpose-set-extension-configuration
           :review-tool
           (purpose-conf :name-purposes '(("*review-request*" . review))))
          (let ((initial
                 (mapcar #'neomacs-window-purpose-test-buffer-purpose
                         (list shell build source file-buffer review)))
                (known-before
                 (sort (mapcar #'symbol-name (purpose-get-all-purposes))
                       #'string<)))
            (purpose-add-user-purposes
             :names '(("*review-request*" . urgent-review)))
            (let ((user-override (purpose-buffer-purpose review)))
              (purpose-remove-user-purposes :names '("*review-request*"))
              (let* ((extension-revealed (purpose-buffer-purpose review))
                     (temporary
                      (purpose-with-additional-purposes
                          '(("*scratch*" . notes)) nil nil
                        (list (purpose-buffer-purpose (get-buffer "*scratch*"))
                              (sort (mapcar #'symbol-name
                                            (purpose-get-all-purposes))
                                    #'string<))))
                     (rollback-signal
                      (condition-case error-data
                          (purpose-with-temp-purposes
                              '(("*project-shell*" . throwaway)) nil nil
                            (error "abort temporary workspace"))
                        (error (list (car error-data) (cadr error-data)))))
                     (after
                      (list (purpose-buffer-purpose shell)
                            (purpose-buffer-purpose (get-buffer "*scratch*"))
                            (sort (mapcar #'symbol-name
                                          (purpose-get-all-purposes))
                                  #'string<))))
                (list :initial initial
                      :known-before known-before
                      :user-override user-override
                      :extension-revealed extension-revealed
                      :temporary temporary
                      :rollback-signal rollback-signal
                      :after after)))))
      (neomacs-window-purpose-test-kill-buffers
       source build shell review file-buffer))))
"##;
    let expect = expect![[
        r#"OK (:initial (("*project-shell*" terminal) ("build-project" diagnostics) ("project-main.el" source) ("README.md" edit) ("*review-request*" review)) :known-before ("diagnostics" "general" "review" "source" "terminal") :user-override urgent-review :extension-revealed review :temporary (notes ("diagnostics" "general" "notes" "review" "source" "terminal")) :rollback-signal (error "abort temporary workspace") :after (terminal source ("diagnostics" "general" "review" "source" "terminal")))"#
    ]];
    ParityBatchCase::value(
        "a_project_session_classifies_buffers_and_restores_scoped_configuration",
        elisp_form,
        expect,
    )
}

fn default_configuration_supports_real_work_buffers_and_live_user_overrides() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-window-purpose-test-with-configuration
    t nil nil nil
  (let* ((root (expand-file-name "window-purpose-defaults/project/"
                                 (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
         (ignore-file (expand-file-name ".gitignore" root))
         (source (get-buffer-create "wp-default-source.el"))
         (build (get-buffer-create "*wp-default-build*"))
         (shell (get-buffer-create "*shell*"))
         (plain (get-buffer-create "wp-default-dashboard"))
         directory-buffer ignore-buffer result)
    (unwind-protect
        (progn
          (make-directory root t)
          (with-temp-file ignore-file
            (insert "target/\n"))
          (setq directory-buffer (dired-noselect root)
                ignore-buffer (find-file-noselect ignore-file))
          (with-current-buffer source
            (emacs-lisp-mode))
          (with-current-buffer build
            (compilation-mode))
          (with-current-buffer shell
            (fundamental-mode))
          (with-current-buffer plain
            (fundamental-mode))
          (let ((default-file-purpose 'file-fallback)
                (buffers
                 `((source . ,source)
                   (build . ,build)
                   (shell . ,shell)
                   (directory . ,directory-buffer)
                   (ignore-file . ,ignore-buffer)
                   (plain . ,plain))))
            (cl-labels
                ((snapshot ()
                   (mapcar
                    (lambda (entry)
                      (list (car entry)
                            (purpose-buffer-purpose (cdr entry))))
                    buffers)))
              (let ((defaults (snapshot)))
                (purpose-add-user-purposes
                 :modes '((compilation-mode . diagnostics))
                 :names '(("*shell*" . project-terminal)))
                (let ((overridden (snapshot))
                      (defaults-disabled
                       (let ((purpose-use-default-configuration nil))
                         (snapshot))))
                  (let ((defaults-reenabled (snapshot)))
                    (purpose-remove-user-purposes
                     :modes '(compilation-mode)
                     :names '("*shell*"))
                    (setq result
                          (list :defaults defaults
                                :overridden overridden
                                :defaults-disabled defaults-disabled
                                :defaults-reenabled defaults-reenabled
                                :overrides-removed (snapshot))))))))
          result)
      (neomacs-window-purpose-test-kill-buffers
       source build shell plain directory-buffer ignore-buffer))))
"##;
    let expect = expect![[
        r#"OK (:defaults ((source edit) (build search) (shell terminal) (directory dired) (ignore-file edit) (plain general)) :overridden ((source edit) (build diagnostics) (shell project-terminal) (directory dired) (ignore-file edit) (plain general)) :defaults-disabled ((source general) (build diagnostics) (shell project-terminal) (directory general) (ignore-file file-fallback) (plain general)) :defaults-reenabled ((source edit) (build diagnostics) (shell project-terminal) (directory dired) (ignore-file edit) (plain general)) :overrides-removed ((source edit) (build search) (shell terminal) (directory dired) (ignore-file edit) (plain general)))"#
    ]];
    ParityBatchCase::value(
        "default_configuration_supports_real_work_buffers_and_live_user_overrides",
        elisp_form,
        expect,
    )
}

pub(crate) fn configuration_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        a_project_session_classifies_buffers_and_restores_scoped_configuration(),
        default_configuration_supports_real_work_buffers_and_live_user_overrides(),
    ]
}

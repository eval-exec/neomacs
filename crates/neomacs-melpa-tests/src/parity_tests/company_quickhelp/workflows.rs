use expect_test::expect;

use super::ParityBatchCase;

fn docstring_from_buffer_truncates_and_skips_footers() -> ParityBatchCase {
    ParityBatchCase::value(
        "docstring_from_buffer_truncates_and_skips_footers",
        r####"
(with-temp-buffer
  (insert "Line one docs\nLine two docs\nLine three docs\n\n[back]\n")
  (let* ((company-quickhelp-max-lines 2)
         (result (company-quickhelp--docstring-from-buffer (point-min))))
    (list :doc (plist-get result :doc)
          :truncated (plist-get result :truncated)
          :full
          (let ((company-quickhelp-max-lines nil))
            (goto-char (point-min))
            (plist-get (company-quickhelp--docstring-from-buffer (point-min))
                       :doc)))))
"####,
        expect![[
            r#"OK (:doc "Line one docs\nLine two docs\nLine three docs" :truncated t :full "Line one docs\nLine two docs\nLine three docs")"#
        ]],
    )
}

fn fetch_docstring_prefers_quickhelp_string_backend() -> ParityBatchCase {
    ParityBatchCase::value(
        "fetch_docstring_prefers_quickhelp_string_backend",
        r####"
(cl-letf (((symbol-function 'company-call-backend)
           (lambda (cmd &optional arg)
             (pcase cmd
               (`quickhelp-string
                (format "quickhelp for %s" arg))
               (`doc-buffer
                (error "should not use doc-buffer when quickhelp-string exists"))
               (_ nil)))))
  (company-quickhelp--fetch-docstring "candidate"))
"####,
        expect![[r#"OK (:doc "quickhelp for candidate" :truncated nil)"#]],
    )
}

fn doc_appends_ellipsis_when_truncated() -> ParityBatchCase {
    ParityBatchCase::value(
        "doc_appends_ellipsis_when_truncated",
        r####"
(cl-letf (((symbol-function 'company-quickhelp--fetch-docstring)
           (lambda (_selected)
             (list :doc "partial docs" :truncated t))))
  (list :truncated (company-quickhelp--doc "x")
        :full
        (cl-letf (((symbol-function 'company-quickhelp--fetch-docstring)
                   (lambda (_selected)
                     (list :doc "full docs" :truncated nil))))
          (company-quickhelp--doc "x"))
        :empty
        (cl-letf (((symbol-function 'company-quickhelp--fetch-docstring)
                   (lambda (_selected)
                     (list :doc "" :truncated nil))))
          (company-quickhelp--doc "x"))))
"####,
        expect![[r#"OK (:truncated "partial docs\n\n[...]" :full "full docs" :empty nil)"#]],
    )
}

fn local_mode_installs_frontend_and_timer_lifecycle() -> ParityBatchCase {
    ParityBatchCase::value(
        "local_mode_installs_frontend_and_timer_lifecycle",
        r####"
(with-temp-buffer
  (let ((company-frontends nil)
        (company-tooltip-minimum-width 10)
        (company-quickhelp-delay 0.5)
        (company-quickhelp--timer nil)
        (company-quickhelp-local-mode nil))
    (company-quickhelp-local-mode 1)
    (let ((enabled
           (list :mode company-quickhelp-local-mode
                 :frontend (car (last company-frontends))
                 :min-width company-tooltip-minimum-width
                 :focus-hook
                 (and (memq 'company-quickhelp-hide focus-out-hook) t))))
      (company-quickhelp--set-timer)
      (let ((armed (timerp company-quickhelp--timer)))
        (company-quickhelp--cancel-timer)
        (company-quickhelp-local-mode -1)
        (list :enabled enabled
              :armed armed
              :cancelled (null company-quickhelp--timer)
              :disabled-mode company-quickhelp-local-mode
              :frontend-removed
              (not (memq 'company-quickhelp-frontend company-frontends))
              :width-restored company-tooltip-minimum-width)))))
"####,
        expect![[
            r#"OK (:enabled (:mode t :frontend company-quickhelp-frontend :min-width 40 :focus-hook t) :armed t :cancelled t :disabled-mode nil :frontend-removed t :width-restored 10)"#
        ]],
    )
}

fn frontend_post_command_arms_timer_and_hide_cancels() -> ParityBatchCase {
    ParityBatchCase::value(
        "frontend_post_command_arms_timer_and_hide_cancels",
        r####"
(let ((company-quickhelp-delay 0.2)
      (company-quickhelp--timer nil)
      (hidden nil))
  (cl-letf (((symbol-function 'company-quickhelp--hide)
             (lambda () (setq hidden t))))
    (company-quickhelp-frontend 'post-command)
    (let ((after-post (timerp company-quickhelp--timer)))
      (company-quickhelp-frontend 'hide)
      (list :after-post after-post
            :after-hide-timer (null company-quickhelp--timer)
            :hidden hidden))))
"####,
        expect!["OK (:after-post t :after-hide-timer t :hidden t)"],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        docstring_from_buffer_truncates_and_skips_footers(),
        fetch_docstring_prefers_quickhelp_string_backend(),
        doc_appends_ellipsis_when_truncated(),
        local_mode_installs_frontend_and_timer_lifecycle(),
        frontend_post_command_arms_timer_and_hide_cancels(),
    ]
}

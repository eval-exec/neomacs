use expect_test::expect;

use super::ParityBatchCase;

fn find_settings_path_prefers_isort_cfg_dominating_dir() -> ParityBatchCase {
    ParityBatchCase::value(
        "find_settings_path_prefers_isort_cfg_dominating_dir",
        r####"
(let* ((root (make-temp-file "py-isort-root" t))
       (nested (expand-file-name "pkg" root))
       (py (expand-file-name "mod.py" nested)))
  (make-directory nested t)
  (with-temp-file (expand-file-name ".isort.cfg" root)
    (insert "[settings]\n"))
  (with-temp-file py (insert "import os\n"))
  (with-temp-buffer
    (setq buffer-file-name py)
    (let ((settings (file-name-as-directory
                     (file-truename (py-isort--find-settings-path))))
          (want (file-name-as-directory (file-truename root))))
      (list :matches (and (equal settings want) t)
            :has-cfg
            (file-exists-p (expand-file-name ".isort.cfg" settings))))))
"####,
        expect![[r#"OK (:matches t :has-cfg t)"#]],
    )
}

fn before_save_only_runs_in_python_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "before_save_only_runs_in_python_mode",
        r####"
(let ((calls 0))
  (cl-letf (((symbol-function 'py-isort-buffer)
             (lambda () (setq calls (1+ calls)))))
    (with-temp-buffer
      (fundamental-mode)
      (py-isort-before-save)
      (let ((after-fundamental calls))
        (python-mode)
        (py-isort-before-save)
        (list :after-fundamental after-fundamental
              :after-python calls)))))
"####,
        expect![[r#"OK (:after-fundamental 0 :after-python 1)"#]],
    )
}

fn options_custom_are_appended_to_isort_invocation() -> ParityBatchCase {
    ParityBatchCase::value(
        "options_custom_are_appended_to_isort_invocation",
        r####"
(let ((calls nil)
      (py-isort-options '("--lines=80" "-m=3")))
  (cl-letf (((symbol-function 'call-process)
             (lambda (program &rest args)
               (push (cons program args) calls)
               0)))
    (with-temp-buffer
      (setq buffer-file-name (expand-file-name "x.py" temporary-file-directory))
      (let ((err (get-buffer-create " *py-isort-err*")))
        (unwind-protect
            (progn
              (py-isort--call-executable err "x.py")
              (let* ((call (car (reverse calls)))
                     (args (cdr call))
                     (settings (cl-find-if
                                (lambda (a)
                                  (and (stringp a)
                                       (string-prefix-p "--settings-path=" a)))
                                args)))
                (list :program (car call)
                      :has-settings (and settings t)
                      :options-tail (last args 2)
                      :options py-isort-options)))
          (kill-buffer err))))))
"####,
        expect![[
            r#"OK (:program "isort" :has-settings t :options-tail ("--lines=80" "-m=3") :options ("--lines=80" "-m=3"))"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        find_settings_path_prefers_isort_cfg_dominating_dir(),
        before_save_only_runs_in_python_mode(),
        options_custom_are_appended_to_isort_invocation(),
    ]
}

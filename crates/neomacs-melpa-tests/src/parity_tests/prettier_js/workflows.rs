use expect_test::expect;

use super::ParityBatchCase;

fn width_args_follow_width_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "width_args_follow_width_mode",
        r####"
(list :none
      (let ((prettier-js-width-mode nil))
        (prettier-js--width-args))
      :fill
      (let ((prettier-js-width-mode 'fill)
            (fill-column 88))
        (prettier-js--width-args))
      :window
      (let ((prettier-js-width-mode 'window))
        (cl-letf (((symbol-function 'window-body-width) (lambda () 100)))
          (prettier-js--width-args))))
"####,
        expect![[r#"OK (:none nil :fill ("--print-width" "88") :window ("--print-width" "100"))"#]],
    )
}

fn get_command_uses_configured_executable_when_on_path() -> ParityBatchCase {
    ParityBatchCase::value(
        "get_command_uses_configured_executable_when_on_path",
        r####"
(let ((prettier-js-command "prettier")
      (prettier-js-use-modules-bin nil)
      (prettier-js-error-state 'stale))
  (cl-letf (((symbol-function 'executable-find)
             (lambda (cmd) (concat "/usr/bin/" cmd))))
    (list :cmd (prettier-js--get-command)
          :error-state prettier-js-error-state)))
"####,
        expect![[r#"OK (:cmd "prettier" :error-state nil)"#]],
    )
}

fn get_command_errors_when_missing_and_file_path_override() -> ParityBatchCase {
    ParityBatchCase::value(
        "get_command_errors_when_missing_and_file_path_override",
        r####"
(list :missing
      (let ((prettier-js-command "prettier-not-installed-xyz")
            (prettier-js-use-modules-bin nil))
        (cl-letf (((symbol-function 'executable-find) (lambda (_) nil)))
          (condition-case err
              (progn (prettier-js--get-command) :ok)
            (error (error-message-string err)))))
      :file-path
      (let ((prettier-js-file-path "/tmp/demo.js"))
        (prettier-js--file-path)))
"####,
        expect![[
            r#"OK (:missing "Could not find prettier executable; is it installed and on Emacs’ exec-path?" :file-path "/tmp/demo.js")"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        width_args_follow_width_mode(),
        get_command_uses_configured_executable_when_on_path(),
        get_command_errors_when_missing_and_file_path_override(),
    ]
}

use expect_test::expect;

use super::ParityBatchCase;

fn mode_binds_indent_font_lock_comments_and_file_associations() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_binds_indent_font_lock_comments_and_file_associations",
        r####"
(with-temp-buffer
  (cmake-mode)
  (list :mode major-mode
        :indent indent-line-function
        :comment comment-start
        :beginning beginning-of-defun-function
        :end end-of-defun-function
        :font-lock (car font-lock-defaults)
        :auto-mode
        (list (assoc-default "CMakeLists.txt" auto-mode-alist #'string-match-p)
              (assoc-default "release.cmake" auto-mode-alist #'string-match-p)
              (assoc-default "notes.txt" auto-mode-alist #'string-match-p))))
"####,
        expect![[
            r##"OK (:mode cmake-mode :indent cmake-indent :comment "#" :beginning cmake-beginning-of-defun :end cmake-end-of-defun :font-lock cmake-font-lock-keywords :auto-mode (cmake-mode cmake-mode text-mode))"##
        ]],
    )
}

fn indentation_tracks_blocks_parens_and_closing_keywords() -> ParityBatchCase {
    ParityBatchCase::value(
        "indentation_tracks_blocks_parens_and_closing_keywords",
        r####"
(neomacs-cmake-mode-test-with-buffer
 "if(ENABLE_RELEASE)
set(APP_NAME \"release\")
function(deploy target)
add_executable(${target} main.cpp)
endfunction()
endif()
"
 (lambda ()
   (let ((cmake-tab-width 2))
     (indent-region (point-min) (point-max))
     (neomacs-cmake-mode-test-indent-snapshot))))
"####,
        expect![[
            r#"OK ((0 "if(ENABLE_RELEASE)") (2 "  set(APP_NAME \"release\")") (2 "  function(deploy target)") (4 "    add_executable(${target} main.cpp)") (2 "  endfunction()") (0 "endif()"))"#
        ]],
    )
}

fn font_lock_marks_keywords_commands_and_variables() -> ParityBatchCase {
    ParityBatchCase::value(
        "font_lock_marks_keywords_commands_and_variables",
        r####"
(neomacs-cmake-mode-test-with-buffer
 "if(ENABLE)
  set(APP_NAME \"release\")
  message(STATUS \"${APP_NAME}\")
endif()
"
 (lambda ()
   (mapcar #'neomacs-cmake-mode-test-face-at
           '("if" "set" "message" "APP_NAME" "STATUS"))))
"####,
        expect![[
            r#"OK (("if" font-lock-keyword-face) ("set" font-lock-function-name-face) ("message" font-lock-function-name-face) ("APP_NAME" nil) ("STATUS" nil))"#
        ]],
    )
}

fn unscreamify_lowercases_commands_without_touching_arguments() -> ParityBatchCase {
    ParityBatchCase::value(
        "unscreamify_lowercases_commands_without_touching_arguments",
        r####"
(neomacs-cmake-mode-test-with-buffer
 "SET(APP_NAME \"Release\")
IF(ENABLE)
  MESSAGE(STATUS \"${APP_NAME}\")
ENDIF()
"
 (lambda ()
   (cmake-unscreamify-buffer)
   (buffer-substring-no-properties (point-min) (point-max))))
"####,
        expect![[
            r#"OK "set(APP_NAME \"Release\")\nif(ENABLE)\n  message(STATUS \"${APP_NAME}\")\nendif()\n""#
        ]],
    )
}

fn defun_navigation_moves_between_function_and_macro_bounds() -> ParityBatchCase {
    ParityBatchCase::value(
        "defun_navigation_moves_between_function_and_macro_bounds",
        r####"
(neomacs-cmake-mode-test-with-buffer
 "set(PREFIX ready)
function(deploy target)
  add_executable(${target} main.cpp)
endfunction()
macro(announce)
  message(STATUS hi)
endmacro()
"
 (lambda ()
   (let (states)
     (goto-char (point-min))
     (search-forward "add_executable")
     (push (list :start (line-number-at-pos)
                 :beginning (progn (cmake-beginning-of-defun) (line-number-at-pos))
                 :end (progn (cmake-end-of-defun) (line-number-at-pos)))
           states)
     (goto-char (point-min))
     (search-forward "message")
     (push (list :start (line-number-at-pos)
                 :beginning (progn (cmake-beginning-of-defun) (line-number-at-pos))
                 :end (progn (cmake-end-of-defun) (line-number-at-pos)))
           states)
     (nreverse states))))
"####,
        expect!["OK ((:start 3 :beginning 2 :end 5) (:start 6 :beginning 5 :end 8))"],
    )
}

fn help_command_runs_cmake_and_opens_read_only_rst_help() -> ParityBatchCase {
    ParityBatchCase::value(
        "help_command_runs_cmake_and_opens_read_only_rst_help",
        r####"
(let (commands help-state
      (cmake-commands '("add_executable" "set"))
      (cmake-help-command-history nil))
  (when (get-buffer "*CMake Help*") (kill-buffer "*CMake Help*"))
  (cl-letf (((symbol-function 'shell-command)
             (lambda (command buffer &optional _error-buffer)
               (push command commands)
               (with-current-buffer buffer
                 (erase-buffer)
                 (insert "add_executable\n-------------\n\nAdd an executable.\n"))
               0))
            ((symbol-function 'display-buffer)
             (lambda (buffer &rest _)
               (set-window-buffer (selected-window) buffer)
               (selected-window)))
            ((symbol-function 'completing-read)
             (lambda (prompt collection &rest _)
               (list :prompt prompt :choices collection)
               "add_executable")))
    (save-window-excursion
      (with-temp-buffer
        (cmake-mode)
        (insert "add_executable(app main.cpp)\n")
        (goto-char (point-min))
        (search-forward "add_executable")
        (cmake-help-command)))
    (with-current-buffer "*CMake Help*"
      (setq help-state
            (list :mode major-mode
                  :read-only buffer-read-only
                  :view (and (bound-and-true-p view-mode) t)
                  :text (string-trim
                         (buffer-substring-no-properties
                          (point-min) (point-max)))))))
  (list :commands (nreverse commands)
        :help help-state
        :executable cmake-mode-cmake-executable))
"####,
        expect![[
            r#"OK (:commands ("cmake --help-command add_executable") :help (:mode rst-mode :read-only t :view t :text "add_executable\n-------------\n\nAdd an executable.") :executable "cmake")"#
        ]],
    )
}

fn empty_help_topic_signals_an_actionable_error() -> ParityBatchCase {
    ParityBatchCase::value(
        "empty_help_topic_signals_an_actionable_error",
        r####"
(cl-letf (((symbol-function 'cmake-get-list)
           (lambda (_type) '("add_executable" "set")))
          ((symbol-function 'completing-read)
           (lambda (&rest _) "")))
  (condition-case err
      (list :value (cmake-help-type "command"))
    (error (list :signal (car err)
                 :message (error-message-string err)))))
"####,
        expect![[r#"OK (:signal error :message "No argument given")"#]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        mode_binds_indent_font_lock_comments_and_file_associations(),
        indentation_tracks_blocks_parens_and_closing_keywords(),
        font_lock_marks_keywords_commands_and_variables(),
        unscreamify_lowercases_commands_without_touching_arguments(),
        defun_navigation_moves_between_function_and_macro_bounds(),
        help_command_runs_cmake_and_opens_read_only_rst_help(),
        empty_help_topic_signals_an_actionable_error(),
    ]
}

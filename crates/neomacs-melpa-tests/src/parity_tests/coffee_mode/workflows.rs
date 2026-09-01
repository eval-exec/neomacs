use expect_test::expect;

use super::ParityBatchCase;

fn mode_binds_indent_font_lock_and_file_associations() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_binds_indent_font_lock_and_file_associations",
        r####"
(with-temp-buffer
  (coffee-mode)
  (list :mode major-mode
        :indent indent-line-function
        :comment comment-start
        :font-lock (car font-lock-defaults)
        :auto-mode
        (list (assoc-default "app.coffee" auto-mode-alist #'string-match-p)
              (assoc-default "Cakefile" auto-mode-alist #'string-match-p)
              (assoc-default "notes.txt" auto-mode-alist #'string-match-p))))
"####,
        expect![[
            r##"OK (:mode coffee-mode :indent coffee-indent-line :comment "#" :font-lock (coffee-font-lock-keywords) :auto-mode (coffee-mode coffee-mode text-mode))"##
        ]],
    )
}

fn indentation_follows_block_openers_and_shift_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "indentation_follows_block_openers_and_shift_commands",
        r####"
(neomacs-coffee-mode-test-with-buffer
 "if ready
doWork()
class Release
deploy: ->
console.log 'ok'
"
 (lambda ()
   (indent-region (point-min) (point-max))
   (let ((after-indent (neomacs-coffee-mode-test-indent-snapshot)))
     (goto-char (point-min))
     (search-forward "doWork")
     (beginning-of-line)
     (coffee-indent-shift-right (line-beginning-position) (line-end-position))
     (list :indent after-indent
           :shifted (neomacs-coffee-mode-test-indent-snapshot)))))
"####,
        expect![[
            r#"OK (:indent ((0 "if ready") (2 "  doWork()") (0 "class Release") (2 "  deploy: ->") (4 "    console.log 'ok'")) :shifted ((0 "if ready") (4 "    doWork()") (0 "class Release") (2 "  deploy: ->") (4 "    console.log 'ok'")))"#
        ]],
    )
}

fn font_lock_marks_keywords_assignments_and_lambdas() -> ParityBatchCase {
    ParityBatchCase::value(
        "font_lock_marks_keywords_assignments_and_lambdas",
        r####"
(neomacs-coffee-mode-test-with-buffer
 "class Release
  if ready
    name = 'widget'
    deploy = -> console.log name
"
 (lambda ()
   (mapcar #'neomacs-coffee-mode-test-face-at
           '("class" "if" "name" "deploy" "->"))))
"####,
        expect![[
            r#"OK (("class" font-lock-keyword-face) ("if" font-lock-keyword-face) ("name" font-lock-variable-name-face) ("deploy" font-lock-variable-name-face) ("->" font-lock-function-name-face))"#
        ]],
    )
}

fn fat_arrow_toggle_and_comment_dwim_edit_the_current_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "fat_arrow_toggle_and_comment_dwim_edit_the_current_line",
        r####"
(neomacs-coffee-mode-test-with-buffer
 "handler = -> console.log 'ok'
work()
"
 (lambda ()
   (goto-char (point-min))
   (search-forward "->")
   (coffee-toggle-fatness)
   (let ((after-fat
          (buffer-substring-no-properties (point-min) (point-max))))
     (goto-char (point-min))
     (search-forward "work")
     (beginning-of-line)
     (coffee-comment-dwim nil)
     (list :fat after-fat
           :commented
           (buffer-substring-no-properties (point-min) (point-max))))))
"####,
        expect![[
            r#"OK (:fat "handler = => console.log 'ok'\nwork()\n" :commented "handler = => console.log 'ok'\nwork()                          #\n")"#
        ]],
    )
}

fn compile_region_invokes_coffee_and_fills_the_compiled_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "compile_region_invokes_coffee_and_fills_the_compiled_buffer",
        r####"
(let (calls compiled)
  (when (get-buffer coffee-compiled-buffer-name)
    (kill-buffer coffee-compiled-buffer-name))
  (neomacs-coffee-mode-test-with-buffer
   "square = (x) -> x * x\n"
   (lambda ()
     (cl-letf (((symbol-function 'coffee-generate-sourcemap-p)
                (lambda () nil))
               ((symbol-function 'coffee-start-compile-process)
                (lambda (curbuf line column)
                  (lambda (start end)
                    (push (list :buffer (buffer-name curbuf)
                                :line line
                                :column column
                                :input (buffer-substring-no-properties start end)
                                :command coffee-command
                                :args coffee-args-compile)
                          calls)
                    (with-current-buffer
                        (get-buffer-create coffee-compiled-buffer-name)
                      (setq buffer-read-only nil)
                      (erase-buffer)
                      (insert
                       "var square;\nsquare = function(x) {\n  return x * x;\n};\n")
                      (funcall coffee-show-mode))))))
       (coffee-compile-region (point-min) (point-max))
       (with-current-buffer coffee-compiled-buffer-name
         (setq compiled
               (list :mode major-mode
                     :text (string-trim
                            (buffer-substring-no-properties
                             (point-min) (point-max)))))))))
  (list :calls (nreverse calls)
        :compiled compiled))
"####,
        expect![[
            r#"OK (:calls ((:buffer " *temp*" :line 2 :column 0 :input "square = (x) -> x * x\n" :command "coffee" :args ("-c" "--no-header"))) :compiled (:mode js-mode :text "var square;\nsquare = function(x) {\n  return x * x;\n};"))"#
        ]],
    )
}

fn imenu_indexes_classes_and_methods() -> ParityBatchCase {
    ParityBatchCase::value(
        "imenu_indexes_classes_and_methods",
        r####"
(neomacs-coffee-mode-test-with-buffer
 "class Release
  deploy: ->
    true
  rollback: (n) ->
    n
"
 (lambda ()
   (let ((index (coffee-imenu-create-index)))
     (list :names (mapcar #'car index)
           :count (length index)))))
"####,
        expect![[r#"OK (:names ("Release::rollback" "Release::deploy") :count 2)"#]],
    )
}

fn under_indent_shift_left_reports_an_actionable_error() -> ParityBatchCase {
    ParityBatchCase::value(
        "under_indent_shift_left_reports_an_actionable_error",
        r####"
(neomacs-coffee-mode-test-with-buffer
 "x = 1\n"
 (lambda ()
   (condition-case err
       (list :value
             (coffee-indent-shift-left
              (point-min) (point-max) 1))
     (error (list :signal (car err)
                  :message (error-message-string err))))))
"####,
        expect![[r#"OK (:signal error :message "Can’t shift all lines enough")"#]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        mode_binds_indent_font_lock_and_file_associations(),
        indentation_follows_block_openers_and_shift_commands(),
        font_lock_marks_keywords_assignments_and_lambdas(),
        fat_arrow_toggle_and_comment_dwim_edit_the_current_line(),
        compile_region_invokes_coffee_and_fills_the_compiled_buffer(),
        imenu_indexes_classes_and_methods(),
        under_indent_shift_left_reports_an_actionable_error(),
    ]
}

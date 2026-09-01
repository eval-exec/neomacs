use expect_test::expect;

use super::ParityBatchCase;

fn affe_grep_builds_real_consult_pipeline_with_paths_options_history_and_protocol_actions()
-> ParityBatchCase {
    ParityBatchCase::value(
        "affe_grep_builds_real_consult_pipeline_with_paths_options_history_and_protocol_actions",
        r##"(let ((affe-grep-command
                     "rg --null .")
                    (affe-count 6)
                    (affe-regexp-compiler
                     (lambda (input _type _case)
                       (cons
                        (list
                         (concat "re:" input))
                        #'ignore)))
                    callback writes
                    directory-calls read-report)
               (cl-letf
                   (((symbol-function
                      'consult--directory-prompt)
                     (lambda (prompt dir)
                       (push
                        (list prompt dir)
                        directory-calls)
                       (list
                        "Grep fixture: "
                        '("one" "two words")
                        "/")))
                    ((symbol-function 'make-temp-name)
                     (lambda (_) "affe-grep"))
                    ((symbol-function 'call-process)
                     (lambda (&rest _) 0))
                    ((symbol-function 'affe--connect)
                     (lambda (_name function)
                       (setq callback function)
                       'backend))
                    ((symbol-function
                      'process-send-string)
                     (lambda (_process string)
                       (push string writes)))
                    ((symbol-function
                      'minibuffer-prompt-end)
                     (lambda () 3))
                    ((symbol-function 'make-overlay)
                     (lambda (&rest _) 'indicator))
                    ((symbol-function 'delete-overlay)
                     (lambda (&rest _) nil))
                    ((symbol-function 'thing-at-point)
                     (lambda (thing)
                       (list 'thing thing)))
                    ((symbol-function 'consult--read)
                     (lambda (table &rest options)
                       (let (candidates sink-events)
                         (with-temp-buffer
                           (insert "P: ")
                           (let* ((sink
                                   (lambda (action)
                                     (push action sink-events)
                                     (cond
                                      ((eq action 'flush)
                                       (setq candidates nil))
                                      ((consp action)
                                       (setq candidates
                                             (append
                                              candidates
                                              action)))
                                      ((null action)
                                       candidates))))
                                  (runner
                                   (funcall table sink)))
                             (setq read-report
                                   (list
                                    (functionp table)
                                    default-directory
                                    (plist-get options
                                               :prompt)
                                    (plist-get options
                                               :sort)
                                    (plist-get options
                                               :require-match)
                                    (plist-get options
                                               :initial)
                                    (plist-get options
                                               :history)
                                    (plist-get options
                                               :category)
                                    (plist-get options
                                               :add-history)
                                    (plist-get options
                                               :async-wrap)
                                    (plist-get options
                                               :lookup)
                                    (plist-get options
                                               :group)
                                    (functionp
                                     (plist-get options
                                                :state))
                                    (funcall runner 'setup)
                                    (funcall runner "needle")
                                    (funcall runner nil)))
                             (funcall runner 'destroy)
                             (setq read-report
                                   (append
                                    read-report
                                    (list
                                     (nreverse
                                      sink-events)))))))
                       'selected)))
                 (list
                  (affe-grep 'fixture "initial")
                  (functionp callback)
                  (nreverse directory-calls)
                  read-report
                  (nreverse writes))))"##,
        expect![[
            r#"OK (selected t (("Fuzzy grep" fixture)) (t "/" "Grep fixture: " nil t "initial" (:input affe--grep-history) consult-grep (thing symbol) identity consult--lookup-member consult--prefix-group t nil nil nil (setup "needle" nil destroy)) ("(start \"\\\\`[^\\0]+\\0[^\\0:]+[\\0:]\\\\(.*\\\\)\\\\'\" \"rg\" \"--null\" \"one\" \"two words\")\n" "(search 6)\n" "(search 6 \"re:needle\")\n" "exit\n"))"#
        ]],
    )
}

fn affe_find_pipeline_strips_dot_prefix_and_return_state_opens_only_selected_candidate()
-> ParityBatchCase {
    ParityBatchCase::value(
        "affe_find_pipeline_strips_dot_prefix_and_return_state_opens_only_selected_candidate",
        r##"(let ((affe-find-command
                     "find . -type f")
                    (affe-count 4)
                    (affe-regexp-compiler
                     (lambda (input _type _case)
                       (cons (list input)
                             #'ignore)))
                    callback writes opened
                    read-report)
               (cl-letf
                   (((symbol-function
                      'consult--directory-prompt)
                     (lambda (_prompt _dir)
                       (list
                        "Find fixture: "
                        '("src")
                        "/")))
                    ((symbol-function 'make-temp-name)
                     (lambda (_) "affe-find"))
                    ((symbol-function 'call-process)
                     (lambda (&rest _) 0))
                    ((symbol-function 'affe--connect)
                     (lambda (_name function)
                       (setq callback function)
                       'backend))
                    ((symbol-function
                      'process-send-string)
                     (lambda (_process string)
                       (push string writes)))
                    ((symbol-function
                      'minibuffer-prompt-end)
                     (lambda () 3))
                    ((symbol-function 'make-overlay)
                     (lambda (&rest _) 'indicator))
                    ((symbol-function 'overlay-put)
                     (lambda (&rest _) nil))
                    ((symbol-function 'delete-overlay)
                     (lambda (&rest _) nil))
                    ((symbol-function 'thing-at-point)
                     (lambda (thing)
                       (list 'thing thing)))
                    ((symbol-function 'find-file)
                     (lambda (file)
                       (push file opened)
                       (list 'opened file)))
                    ((symbol-function 'consult--read)
                     (lambda (table &rest options)
                       (let (candidates sink-events)
                         (with-temp-buffer
                           (insert "P: ")
                           (let* ((sink
                                   (lambda (action)
                                     (push action sink-events)
                                     (cond
                                      ((eq action 'flush)
                                       (setq candidates nil))
                                      ((consp action)
                                       (setq candidates
                                             (append
                                              candidates
                                              action)))
                                      ((null action)
                                       candidates))))
                                  (runner
                                   (funcall table sink))
                                  (state
                                   (plist-get options
                                              :state)))
                             (funcall runner 'setup)
                             (funcall runner "alpha")
                             (funcall
                              callback
                              '("(match \"\" \"./alpha.txt\" \"\")"))
                             (setq read-report
                                   (list
                                    (plist-get options
                                               :prompt)
                                    (plist-get options
                                               :sort)
                                    (plist-get options
                                               :require-match)
                                    (plist-get options
                                               :history)
                                    (plist-get options
                                               :initial)
                                    (plist-get options
                                               :category)
                                    (plist-get options
                                               :add-history)
                                    (plist-get options
                                               :async-wrap)
                                    (funcall runner nil)
                                    (funcall
                                     state
                                     'preview
                                     "preview.txt")
                                    (funcall
                                     state
                                     'return
                                     "chosen.txt")
                                    (funcall
                                     state
                                     'return nil)))
                             (funcall runner 'destroy)
                             (setq read-report
                                   (append
                                    read-report
                                    (list
                                     (nreverse
                                      sink-events)))))))
                       'chosen)))
                 (list
                  (affe-find 'fixture "seed")
                  read-report
                  (nreverse opened)
                  (nreverse writes))))"##,
        expect![[
            r#"OK (chosen ("Find fixture: " nil t (:input affe--find-history) "seed" file (thing filename) identity #1=("alpha.txt") nil (opened "chosen.txt") nil (setup "alpha" #1# nil destroy)) ("chosen.txt") ("(start nil \"find\" \"src\" \"-type\" \"f\")\n" "(search 4)\n" "(search 4 \"alpha\")\n" "exit\n"))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        affe_grep_builds_real_consult_pipeline_with_paths_options_history_and_protocol_actions(),
        affe_find_pipeline_strips_dot_prefix_and_return_state_opens_only_selected_candidate(),
    ]
}

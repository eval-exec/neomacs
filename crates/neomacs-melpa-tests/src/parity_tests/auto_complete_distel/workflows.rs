use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_distel_real_module_menu_navigates_and_completes_erlang_module() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_distel_real_module_menu_navigates_and_completes_erlang_module",
        r##"(save-window-excursion
          (with-temp-buffer
            (switch-to-buffer
             (current-buffer))
            (let ((ac-use-comphist nil)
                  (ac-use-quick-help nil)
                  (ac-auto-show-menu t)
                  (ac-expand-on-auto-complete nil)
                  (ac-ignore-case nil)
                  (ac-sources
                   '(auto-complete-distel))
                  events)
              (cl-letf
                  (((symbol-function
                     'distel-completion-complete-module)
                    (lambda (module)
                      (push
                       (list
                        :module
                        module
                        (buffer-name))
                       events)
                      (setq
                       distel-completion-try-erl-complete-cache
                       '("lists"
                         "lib"
                         "linux"))))
                   ((symbol-function 'sleep-for)
                    (lambda (&rest duration)
                      (push
                       (cons :sleep duration)
                       events))))
                (unwind-protect
                    (progn
                      (auto-complete-mode 1)
                      (insert
                       "handle(Items) -> li")
                      (auto-complete)
                      (let ((initial
                             (list
                              ac-prefix
                              (mapcar
                               (lambda (candidate)
                                 (list
                                  (substring-no-properties
                                   candidate)
                                  (popup-item-symbol
                                   candidate)))
                               ac-candidates)
                              (popup-live-p ac-menu)
                              (substring-no-properties
                               (ac-selected-candidate)))))
                        (ac-next)
                        (let ((selected
                               (substring-no-properties
                                (ac-selected-candidate))))
                          (ac-complete)
                          (list
                           initial
                           selected
                           (buffer-substring-no-properties
                            (line-beginning-position)
                            (line-end-position))
                           (nreverse events)
                           ac-menu
                           ac-completing
                           ac-prefix))))
                  (auto-complete-mode -1))))))"##,
        expect![[
            r#"OK (("li" (("lists" "m") ("lib" "m") ("linux" "m")) t "lists") "lib" "handle(Items) -> lib" ((:module "li" " *temp*") (:sleep 0.1)) nil nil nil)"#
        ]],
    )
}

fn auto_complete_distel_real_function_menu_prefixes_distel_results_with_module() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_distel_real_function_menu_prefixes_distel_results_with_module",
        r##"(save-window-excursion
          (with-temp-buffer
            (switch-to-buffer
             (current-buffer))
            (let ((ac-use-comphist nil)
                  (ac-use-quick-help nil)
                  (ac-auto-show-menu t)
                  (ac-expand-on-auto-complete nil)
                  (ac-ignore-case nil)
                  (ac-sources
                   '(auto-complete-distel))
                  events)
              (cl-letf
                  (((symbol-function
                     'distel-completion-complete-function)
                    (lambda (module function)
                      (push
                       (list
                        :function
                        module
                        function)
                       events)
                      (setq
                       distel-completion-try-erl-complete-cache
                       '("map"
                         "mapfoldl"
                         "mapfoldr"))))
                   ((symbol-function 'sleep-for)
                    (lambda (&rest duration)
                      (push
                       (cons :sleep duration)
                       events))))
                (unwind-protect
                    (progn
                      (auto-complete-mode 1)
                      (insert
                       "Result = lists:ma")
                      (auto-complete)
                      (let ((session
                             (list
                              ac-prefix
                              (mapcar
                               #'substring-no-properties
                               ac-candidates)
                              (substring-no-properties
                               (ac-selected-candidate)))))
                        (ac-next)
                        (ac-next)
                        (let ((selected
                               (substring-no-properties
                                (ac-selected-candidate))))
                          (ac-complete)
                          (list
                           session
                           selected
                           (buffer-substring-no-properties
                            (line-beginning-position)
                            (line-end-position))
                           (nreverse events)
                           ac-menu
                           ac-completing))))
                  (auto-complete-mode -1))))))"##,
        expect![[
            r#"OK (("lists:ma" ("lists:map" "lists:mapfoldl" "lists:mapfoldr") "lists:map") "lists:mapfoldr" "Result = lists:mapfoldr" ((:function "lists" "ma") (:sleep 0.1)) nil nil)"#
        ]],
    )
}

fn auto_complete_distel_incremental_erlang_function_typing_requeries_and_narrows_menu()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_incremental_erlang_function_typing_requeries_and_narrows_menu",
        r##"(save-window-excursion
          (with-temp-buffer
            (switch-to-buffer
             (current-buffer))
            (let ((ac-use-comphist nil)
                  (ac-use-quick-help nil)
                  (ac-auto-show-menu t)
                  (ac-expand-on-auto-complete nil)
                  (ac-ignore-case nil)
                  (ac-sources
                   '(auto-complete-distel))
                  events)
              (cl-letf
                  (((symbol-function
                     'distel-completion-complete-function)
                    (lambda (module function)
                      (push
                       (list module function)
                       events)
                      (setq
                       distel-completion-try-erl-complete-cache
                       (cond
                        ((equal function "m")
                         '("map" "mapfoldl"
                           "mapfoldr" "member"))
                        ((equal function "ma")
                         '("map" "mapfoldl"
                           "mapfoldr"))
                        (t
                         '("mapfoldl"
                           "mapfoldr"))))))
                   ((symbol-function 'sleep-for)
                    (lambda (&rest _duration)
                      nil)))
                (unwind-protect
                    (progn
                      (auto-complete-mode 1)
                      (insert "lists:m")
                      (auto-complete)
                      (let ((first
                             (list
                              ac-prefix
                              (mapcar
                               #'substring-no-properties
                               ac-candidates))))
                        (insert "a")
                        (setq ac-prefix
                              (buffer-substring-no-properties
                               ac-point
                               (point)))
                        (ac-update t)
                        (let ((second
                               (list
                                ac-prefix
                                (mapcar
                                 #'substring-no-properties
                                 ac-candidates))))
                          (insert "pf")
                          (setq ac-prefix
                                (buffer-substring-no-properties
                                 ac-point
                                 (point)))
                          (ac-update t)
                          (let ((third
                                 (list
                                  ac-prefix
                                  (mapcar
                                   #'substring-no-properties
                                   ac-candidates))))
                            (ac-next)
                            (ac-complete)
                            (list
                             first
                             second
                             third
                             (buffer-substring-no-properties
                              (line-beginning-position)
                              (line-end-position))
                             (nreverse events))))))
                  (auto-complete-mode -1))))))"##,
        expect![[
            r#"OK (("lists:m" ("lists:map" "lists:mapfoldl" "lists:mapfoldr" "lists:member")) ("lists:ma" ("lists:map" "lists:mapfoldl" "lists:mapfoldr")) ("lists:mapf" ("lists:mapfoldl" "lists:mapfoldr")) "lists:mapfoldr" (("lists" "m") ("lists" "ma") ("lists" "mapf")))"#
        ]],
    )
}

fn auto_complete_distel_real_candidates_retain_document_function_for_selected_item()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_real_candidates_retain_document_function_for_selected_item",
        r##"(save-window-excursion
          (with-temp-buffer
            (switch-to-buffer
             (current-buffer))
            (let ((ac-use-comphist nil)
                  (ac-use-quick-help nil)
                  (ac-auto-show-menu t)
                  (ac-expand-on-auto-complete nil)
                  (ac-ignore-case nil)
                  (ac-sources
                   '(auto-complete-distel))
                  document-calls)
              (cl-letf
                  (((symbol-function
                     'distel-completion-complete-function)
                    (lambda (_module _function)
                      (setq
                       distel-completion-try-erl-complete-cache
                       '("map" "mapfoldl"))))
                   ((symbol-function
                     'distel-completion-get-doc-string)
                    (lambda (candidate)
                      (push candidate
                            document-calls)
                      (format
                       "DOC[%s]"
                       candidate)))
                   ((symbol-function 'sleep-for)
                    (lambda (&rest _duration)
                      nil)))
                (unwind-protect
                    (progn
                      (auto-complete-mode 1)
                      (insert "lists:ma")
                      (auto-complete)
                      (let* ((selected
                              (ac-selected-candidate))
                             (document
                              (popup-item-property
                               selected
                               'document)))
                        (list
                         (substring-no-properties
                          selected)
                         document
                         (funcall
                          document
                          (substring-no-properties
                           selected))
                         document-calls)))
                  (auto-complete-mode -1))))))"##,
        expect![[
            r#"OK ("lists:map" distel-completion-get-doc-string "DOC[lists:map]" ("lists:map"))"#
        ]],
    )
}

fn auto_complete_distel_punctuation_or_digit_suffix_does_not_start_remote_completion()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_punctuation_or_digit_suffix_does_not_start_remote_completion",
        r##"(save-window-excursion
          (mapcar
           (lambda (text)
             (with-temp-buffer
               (switch-to-buffer
                (current-buffer))
               (let ((ac-use-comphist nil)
                     (ac-use-quick-help nil)
                     (ac-auto-show-menu t)
                     (ac-sources
                      '(auto-complete-distel))
                     calls)
                 (cl-letf
                     (((symbol-function
                        'distel-completion-complete)
                       (lambda (&rest arguments)
                         (push arguments calls)
                         '("unexpected"))))
                   (unwind-protect
                       (progn
                         (auto-complete-mode 1)
                         (insert text)
                         (let ((started
                                (auto-complete)))
                           (list
                            text
                            started
                            calls
                            ac-prefix
                            ac-candidates
                            ac-menu
                            ac-completing)))
                     (auto-complete-mode -1))))))
           '("." "module2" "lists:map(")))"##,
        expect![[
            r#"OK (("." nil nil nil nil nil nil) ("module2" nil nil nil nil nil nil) ("lists:map(" nil nil nil nil nil nil))"#
        ]],
    )
}

fn auto_complete_distel_two_erlang_buffers_keep_prefix_remote_query_and_result_isolated()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_two_erlang_buffers_keep_prefix_remote_query_and_result_isolated",
        r##"(save-window-excursion
          (let ((first
                 (generate-new-buffer
                  " *distel-orders*"))
                (second
                 (generate-new-buffer
                  " *distel-users*"))
                events
                results)
            (unwind-protect
                (setq
                 results
                 (cl-letf
                     (((symbol-function
                        'distel-completion-complete-function)
                       (lambda (module function)
                         (push
                          (list
                           (buffer-name)
                           module
                           function)
                          events)
                         (setq
                          distel-completion-try-erl-complete-cache
                          (if
                              (equal module "orders")
                              '("fetch" "find")
                            '("fetch" "filter")))))
                      ((symbol-function 'sleep-for)
                       (lambda (&rest _duration)
                         nil)))
                   (mapcar
                    (lambda (fixture)
                      (with-current-buffer
                          (car fixture)
                        (switch-to-buffer
                         (current-buffer))
                        (let ((ac-use-comphist nil)
                              (ac-use-quick-help nil)
                              (ac-auto-show-menu t)
                              (ac-expand-on-auto-complete nil)
                              (ac-ignore-case nil)
                              (ac-sources
                               '(auto-complete-distel)))
                          (auto-complete-mode 1)
                          (insert
                           (cdr fixture))
                          (auto-complete)
                          (let ((result
                                 (list
                                  (buffer-name)
                                  ac-prefix
                                  (mapcar
                                   #'substring-no-properties
                                   ac-candidates))))
                            (ac-complete)
                            (auto-complete-mode -1)
                            (append
                             result
                             (list
                              (buffer-string)))))))
                    (list
                     (cons first
                           "orders:f")
                     (cons second
                           "users:f")))))
              (kill-buffer first)
              (kill-buffer second))
            (list
             results
             (nreverse events))))"##,
        expect![[
            r#"OK (((" *distel-orders*" "orders:f" ("orders:fetch" "orders:find") "orders:fetch") (" *distel-users*" "users:f" ("users:fetch" "users:filter") "users:fetch")) ((" *distel-orders*" "orders" "f") (" *distel-users*" "users" "f")))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_distel_real_module_menu_navigates_and_completes_erlang_module(),
        auto_complete_distel_real_function_menu_prefixes_distel_results_with_module(),
        auto_complete_distel_incremental_erlang_function_typing_requeries_and_narrows_menu(),
        auto_complete_distel_real_candidates_retain_document_function_for_selected_item(),
        auto_complete_distel_punctuation_or_digit_suffix_does_not_start_remote_completion(),
        auto_complete_distel_two_erlang_buffers_keep_prefix_remote_query_and_result_isolated(),
    ]
}

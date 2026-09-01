use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_clang_async_sources_resolve_to_callable_prefix_candidate_document_and_action_contracts()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_sources_resolve_to_callable_prefix_candidate_document_and_action_contracts",
        r##"(mapcar
                           (lambda (source)
                             (let ((definition
                                    (symbol-value source)))
                               (list
                                source
                                definition
                                (mapcar
                                 (lambda (property)
                                   (let ((value
                                          (cdr
                                           (assq
                                            property
                                            definition))))
                                     (list
                                      property
                                      value
                                      (and
                                       (symbolp value)
                                       (fboundp value)))))
                                 '(candidates
                                   prefix
                                   action
                                   document))
                                (cdr
                                 (assq
                                  'requires
                                  definition))
                                (cdr
                                 (assq
                                  'symbol
                                  definition))
                                (assq
                                 'cache
                                 definition))))
                           '(ac-source-clang-template
                             ac-source-clang-async))"##,
        expect![[
            r#"OK ((ac-source-clang-template ((candidates . ac-clang-template-candidate) (prefix . ac-clang-template-prefix) (requires . 0) (action . ac-clang-template-action) (document . ac-clang-document) #1=(cache) (symbol . "t")) ((candidates ac-clang-template-candidate t) (prefix ac-clang-template-prefix t) (action ac-clang-template-action t) (document ac-clang-document t)) 0 "t" #1#) (ac-source-clang-async ((candidates . ac-clang-candidate) (candidate-face . ac-clang-candidate-face) (selection-face . ac-clang-selection-face) (prefix . ac-clang-prefix) (requires . 0) (document . ac-clang-document) (action . ac-clang-action) #2=(cache) (symbol . "c")) ((candidates ac-clang-candidate t) (prefix ac-clang-prefix t) (action ac-clang-action t) (document ac-clang-document t)) 0 "c" #2#))"#
        ]],
    )
}

fn auto_complete_clang_async_launch_wrapper_distinguishes_nil_filename_from_empty_and_real_paths()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_launch_wrapper_distinguishes_nil_filename_from_empty_and_real_paths",
        r##"(mapcar
                           (lambda (filename)
                             (with-temp-buffer
                               (setq
                                buffer-file-name
                                filename)
                               (let (calls)
                                 (cl-letf
                                     (((symbol-function
                                        'ac-clang-launch-completion-process-with-file)
                                       (lambda (file)
                                         (push file calls)
                                         :launched)))
                                   (list
                                    filename
                                    (ac-clang-launch-completion-process)
                                    (nreverse calls))))))
                           (list
                            nil
                            ""
                            (expand-file-name
                             "./tmp/auto-complete-clang-async/project/main.cpp")))"##,
        expect![[
            r#"OK ((nil nil nil) ("" :launched ("")) ("[ORACLE-TMPDIR]/auto-complete-clang-async/project/main.cpp" :launched ("[ORACLE-TMPDIR]/auto-complete-clang-async/project/main.cpp")))"#
        ]],
    )
}

fn auto_complete_clang_async_real_cat_callback_delivers_completion_candidates_through_os_process_filter()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_real_cat_callback_delivers_completion_candidates_through_os_process_filter",
        r##"(let* ((pair
                                 (acclang-test-start-cat
                                  "acclang-real-callback"))
                                (process
                                 (car pair))
                                (buffer
                                 (cdr pair))
                                (ac-clang-status
                                 'wait)
                                (ac-clang-saved-prefix
                                 "fo")
                                calls
                                (attempts 0))
                           (unwind-protect
                               (cl-letf
                                   (((symbol-function
                                      'ac-start)
                                     (lambda (&rest arguments)
                                       (push
                                        (cons :start arguments)
                                        calls)
                                       :started))
                                    ((symbol-function
                                      'ac-update)
                                     (lambda (&rest arguments)
                                       (push
                                        (cons :update arguments)
                                        calls)
                                       :updated)))
                                 (set-process-filter
                                  process
                                  #'ac-clang-filter-output)
                                 (process-send-string
                                  process
                                  (concat
                                   "COMPLETION: format : [#int#]format(<#const char *fmt#>)\n"
                                   "COMPLETION: fork : [#void#]fork()\n"
                                   "COMPLETION: false : [#bool#]false\n"
                                   "$"))
                                 (while
                                     (and
                                      (not
                                       (eq
                                        (with-current-buffer
                                            buffer
                                          ac-clang-status)
                                        'idle))
                                      (< attempts 20))
                                   (setq attempts
                                         (1+ attempts))
                                   (accept-process-output
                                    process
                                    0.05))
                                 (list
                                  attempts
                                  (list
                                   :source-buffer
                                   ac-clang-status
                                   ac-clang-current-candidate)
                                  (with-current-buffer buffer
                                    (list
                                     :process-buffer
                                     ac-clang-status
                                     (mapcar
                                      #'acclang-test-candidate-summary
                                      ac-clang-current-candidate)
                                     (buffer-substring-no-properties
                                      (point-min)
                                      (point-max))))
                                  (nreverse calls)
                                  (process-live-p process)))
                             (acclang-test-finish-process
                              process
                              buffer)))"##,
        expect![[
            r#"OK (1 (:source-buffer idle (#("fork" 0 4 (ac-clang-help "[#void#]fork()")) #("format" 0 6 (ac-clang-help "[#int#]format(<#const char *fmt#>)")))) (:process-buffer idle nil "COMPLETION: format : [#int#]format(<#const char *fmt#>)\nCOMPLETION: fork : [#void#]fork()\nCOMPLETION: false : [#bool#]false\n$") ((:start :force-init t) (:update)) (run open listen connect stop))"#
        ]],
    )
}

fn auto_complete_clang_async_practical_cpp_request_response_document_and_template_workflow_matches()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_practical_cpp_request_response_document_and_template_workflow_matches",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (c++-mode)
                             (insert
                              "struct Formatter { int format(const char *); void fork(); };\n"
                              "void use(Formatter object) {\n"
                              "  object.fo")
                             (let* ((pair
                                     (acclang-test-start-cat
                                      "acclang-cpp-workflow"))
                                    (process
                                     (car pair))
                                    (process-buffer
                                     (cdr pair))
                                    (ac-clang-completion-process
                                     process)
                                    (ac-clang-status
                                     'idle)
                                    (ac-prefix
                                     "fo")
                                    calls
                                    (attempts 0))
                               (unwind-protect
                                   (cl-letf
                                       (((symbol-function
                                          'ac-start)
                                         (lambda (&rest arguments)
                                           (push
                                            (cons :start arguments)
                                            calls)
                                           :started))
                                        ((symbol-function
                                          'ac-update)
                                         (lambda (&rest arguments)
                                           (push
                                            (cons :update arguments)
                                            calls)
                                           :updated))
                                        ((symbol-function
                                          'ac-complete-clang-template)
                                         (lambda ()
                                           (push
                                            (list
                                             :template
                                             ac-clang-template-start-point
                                             (mapcar
                                              #'acclang-test-candidate-summary
                                              ac-clang-template-candidates))
                                            calls)
                                           :template-started)))
                                     (set-process-filter
                                      process
                                      #'ac-clang-filter-output)
                                     (let ((request-result
                                            (ac-clang-candidate)))
                                       (accept-process-output
                                        process
                                        0.05)
                                       (let ((request
                                              (acclang-test-process-buffer-string
                                               process)))
                                         (with-current-buffer
                                             process-buffer
                                           (erase-buffer)
                                           (set-marker
                                            (process-mark process)
                                            (point-min)))
                                         (process-send-string
                                          process
                                          (concat
                                           "COMPLETION: format : [#int#]format(<#const char *text#>)\n"
                                           "COMPLETION: fork : [#void#]fork()\n"
                                           "$"))
                                         (while
                                             (and
                                              (not
                                               (eq
                                                ac-clang-status
                                                'idle))
                                              (< attempts 20))
                                           (setq attempts
                                                 (1+ attempts))
                                           (accept-process-output
                                            process
                                            0.05))
                                         (let* ((candidates
                                                ac-clang-current-candidate)
                                                (selected
                                                 (cadr candidates))
                                                (document
                                                 (ac-clang-document
                                                  selected))
                                                (ac-last-completion
                                                 (cons nil selected))
                                                (action
                                                 (ac-clang-action)))
                                           (list
                                            request-result
                                            request
                                            attempts
                                            (list
                                             :source-status
                                             ac-clang-status
                                             :process-buffer-status
                                             (with-current-buffer
                                                 process-buffer
                                               ac-clang-status))
                                            (mapcar
                                             #'acclang-test-candidate-summary
                                             candidates)
                                            document
                                            action
                                            (nreverse calls)
                                            (buffer-string))))))
                                 (acclang-test-finish-process
                                  process
                                  process-buffer)))))"##,
        expect![[
            r#"OK (nil "COMPLETION\nrow:3\ncolumn:10\nsource_length:101\nstruct Formatter { int format(const char *); void fork(); };\nvoid use(Formatter object) {\n  object.fo\n\n" 1 (:source-status idle :process-buffer-status idle) (("fork" "[#void#]fork()" nil) ("format" "[#int#]format(<#const char *text#>)" nil)) "int format(const char *text)" "int format(const char *text)" ((:start :force-init t) (:update) (:template 102 (("(const char *text)" "int" "(<#const char *text#>)")))) "struct Formatter { int format(const char *); void fork(); };\nvoid use(Formatter object) {\n  object.fo")"#
        ]],
    )
}

fn auto_complete_clang_async_two_live_buffers_keep_candidates_local_while_saved_prefix_remains_global()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_async_two_live_buffers_keep_candidates_local_while_saved_prefix_remains_global",
        r##"(let ((first
                                (generate-new-buffer
                                 " *acclang-workflow-first*"))
                               (second
                                (generate-new-buffer
                                 " *acclang-workflow-second*"))
                               processes)
                           (unwind-protect
                               (progn
                                 (dolist
                                     (fixture
                                      (list
                                       (list
                                        first
                                        "first.me"
                                        "me"
                                        "COMPLETION: member : [#int#]member$")
                                       (list
                                        second
                                        "second.ru"
                                        "ru"
                                        "COMPLETION: run : [#void#]run()$")))
                                   (with-current-buffer
                                       (nth 0 fixture)
                                     (insert
                                      (nth 1 fixture))
                                     (let* ((pair
                                             (acclang-test-start-cat
                                              (format
                                               "acclang-%s"
                                               (buffer-name))))
                                            (process
                                             (car pair))
                                            (process-buffer
                                             (cdr pair))
                                            (attempts 0))
                                       (push
                                        (list process process-buffer)
                                        processes)
                                       (setq
                                        ac-clang-completion-process
                                        process
                                        ac-clang-status
                                        'wait
                                        ac-clang-saved-prefix
                                        (nth 2 fixture))
                                       (set-process-filter
                                        process
                                        #'ac-clang-filter-output)
                                       (cl-letf
                                           (((symbol-function
                                              'ac-start)
                                             (lambda (&rest _arguments)
                                               :started))
                                            ((symbol-function
                                              'ac-update)
                                             (lambda (&rest _arguments)
                                               :updated)))
                                         (process-send-string
                                          process
                                          (nth 3 fixture))
                                         (while
                                             (and
                                              (not
                                               (eq
                                                ac-clang-status
                                                'idle))
                                              (< attempts 20))
                                           (setq attempts
                                                 (1+ attempts))
                                           (accept-process-output
                                            process
                                            0.05))))))
                                 (mapcar
                                  (lambda (buffer)
                                    (with-current-buffer buffer
                                      (list
                                       (buffer-name)
                                       (buffer-string)
                                       ac-clang-status
                                       ac-clang-saved-prefix
                                       (mapcar
                                        #'acclang-test-candidate-summary
                                        ac-clang-current-candidate)
                                       (with-current-buffer
                                           (process-buffer
                                            ac-clang-completion-process)
                                         (list
                                          ac-clang-status
                                          ac-clang-saved-prefix
                                          (mapcar
                                           #'acclang-test-candidate-summary
                                           ac-clang-current-candidate)
                                          (buffer-substring-no-properties
                                           (point-min)
                                           (point-max))))
                                       (process-live-p
                                        ac-clang-completion-process)
                                       (local-variable-p
                                        'ac-clang-completion-process))))
                                  (list first second)))
                             (dolist (pair processes)
                               (acclang-test-finish-process
                                (car pair)
                                (cadr pair)))
                             (kill-buffer first)
                             (kill-buffer second)))"##,
        expect![[
            r#"OK ((" *acclang-workflow-first*" "first.me" idle "ru" (("member" "[#int#]member$" nil)) (idle "ru" nil "COMPLETION: member : [#int#]member$") #1=(run open listen connect stop) t) (" *acclang-workflow-second*" "second.ru" idle "ru" (("run" "[#void#]run()$" nil)) (idle "ru" nil "COMPLETION: run : [#void#]run()$") #1# t))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_clang_async_sources_resolve_to_callable_prefix_candidate_document_and_action_contracts(),
        auto_complete_clang_async_launch_wrapper_distinguishes_nil_filename_from_empty_and_real_paths(),
        auto_complete_clang_async_real_cat_callback_delivers_completion_candidates_through_os_process_filter(),
        auto_complete_clang_async_practical_cpp_request_response_document_and_template_workflow_matches(),
        auto_complete_clang_async_two_live_buffers_keep_candidates_local_while_saved_prefix_remains_global(),
    ]
}

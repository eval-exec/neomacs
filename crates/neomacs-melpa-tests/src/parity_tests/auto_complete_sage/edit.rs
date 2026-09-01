use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_sage_edit_generated_prefixes_use_cached_and_fresh_completion_states()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_edit_generated_prefixes_use_cached_and_fresh_completion_states",
        r##"(with-temp-buffer
                           (insert "module.na")
                           (goto-char (point-max))
                           (let ((states
                                  '(((interface . "sage")
                                     (types . ("interface")))
                                    ((interface . "sage")
                                     (types . ("modules")))
                                    ((interface . "sage")
                                     (types . ("vars-in-module")))))
                                 parse-calls)
                             (cl-letf
                                 (((symbol-function
                                    'sage-shell-edit:parse-current-state)
                                   (lambda ()
                                     (push :parse parse-calls)
                                     (cadr states))))
                               (mapcar
                                (lambda (fixture)
                                  (let ((ac-sage-edit:-state-cached
                                         (cadr fixture)))
                                    (list
                                     (car fixture)
                                     (funcall (car fixture))
                                     ac-sage-edit:-state-cached
                                     (length parse-calls))))
                                `((ac-sage-edit--sage-commands-prefix
                                    ,(car states))
                                  (ac-sage-edit--modules-prefix
                                    ,(car states))
                                  (ac-sage-edit--vars-in-module-prefix
                                    ,(nth 2 states))
                                  (ac-sage-edit--sage-commands-prefix
                                    ,(nth 1 states)))))))"##,
        expect![[
            r#"OK ((ac-sage-edit--sage-commands-prefix 8 ((interface . "sage") (types "interface")) 0) (ac-sage-edit--modules-prefix 8 #1=((interface . "sage") (types "modules")) 1) (ac-sage-edit--vars-in-module-prefix 8 ((interface . "sage") (types "vars-in-module")) 1) (ac-sage-edit--sage-commands-prefix nil #1# 1))"#
        ]],
    )
}

fn auto_complete_sage_edit_source_initializers_select_process_buffer_once_and_forward_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_edit_source_initializers_select_process_buffer_once_and_forward_state",
        r##"(let ((ac-sage-edit:-state-cached
                                '((interface . "sage")
                                  (prefix . 1)
                                  (types . ("interface"))))
                               (sage-shell:process-buffer nil)
                               calls)
                           (cl-letf
                               (((symbol-function
                                  'sage-shell-edit:set-sage-proc-buf-internal)
                                 (lambda (&rest arguments)
                                   (push
                                    (cons :select arguments)
                                    calls)
                                   (setq sage-shell:process-buffer
                                         " *chosen-sage*")))
                                ((symbol-function
                                  'sage-shell-cpl:completion-init)
                                 (lambda (sync &rest arguments)
                                   (push
                                    (list
                                     :init
                                     sync
                                     (plist-get
                                      arguments
                                      :compl-state)
                                     sage-shell:process-buffer)
                                    calls)
                                   :initialized)))
                             (dolist
                                 (command
                                  '(auto-complete
                                    self-insert-command))
                               (let ((this-command command))
                                 (funcall
                                  (cdr
                                   (assq
                                    'init
                                    ac-source-sage-commands)))))
                             (nreverse calls)))"##,
        expect![[
            r#"OK ((:select nil nil) (:init t #1=((interface . "sage") (prefix . 1) (types "interface")) " *chosen-sage*") (:init nil #1# " *chosen-sage*"))"#
        ]],
    )
}

fn auto_complete_sage_edit_candidates_gate_missing_dead_and_unfinished_process_buffers()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_edit_candidates_gate_missing_dead_and_unfinished_process_buffers",
        r##"(let ((live-buffer
                                (generate-new-buffer
                                 " *acsage-edit-live*"))
                               calls)
                           (unwind-protect
                               (cl-letf
                                   (((symbol-function
                                      'sage-shell:redirect-finished-p)
                                     (lambda ()
                                       (push :redirect calls)
                                       (with-current-buffer
                                           live-buffer
                                         comint-redirect-completed)))
                                    ((symbol-function
                                      'sage-shell:output-finished-p)
                                     (lambda (&optional _buffer)
                                       (push :output calls)
                                       (with-current-buffer
                                           live-buffer
                                         sage-shell:output-finished-p)))
                                    ((symbol-function
                                      'sage-shell-cpl:candidates)
                                     (lambda (&rest arguments)
                                       (push
                                        (cons :candidate arguments)
                                        calls)
                                       '("factor"
                                         "find_root"))))
                                 (let (results)
                                   (dolist
                                       (fixture
                                        (list
                                         (list nil nil nil)
                                         (list
                                          " *acsage-missing*"
                                          nil
                                          nil)
                                         (list live-buffer nil t)
                                         (list live-buffer t nil)
                                         (list live-buffer t t)))
                                     (with-current-buffer
                                         live-buffer
                                       (setq
                                        comint-redirect-completed
                                        (cadr fixture)
                                        sage-shell:output-finished-p
                                        (nth 2 fixture)))
                                     (let ((sage-shell:process-buffer
                                            (car fixture))
                                           (ac-sage-edit:-state-cached
                                            '((interface . "sage")
                                              (types . ("interface")))))
                                       (push
                                        (list
                                         (cdr fixture)
                                         (ac-sage-edit:candidates)
                                         (nreverse calls))
                                        results)
                                       (setq calls nil)))
                                   (nreverse results)))
                             (when
                                 (buffer-live-p live-buffer)
                               (kill-buffer live-buffer))))"##,
        expect![[
            r#"OK (((nil nil) nil nil) ((nil nil) nil nil) ((nil t) nil (:redirect)) ((t nil) nil (:redirect :output)) ((t t) ("factor" "find_root") (:redirect :output (:candidate :state ((interface . "sage") (types "interface"))))))"#
        ]],
    )
}

fn auto_complete_sage_edit_candidates_forward_exact_cached_state_on_success() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_edit_candidates_forward_exact_cached_state_on_success",
        r##"(let ((process-buffer
                                (generate-new-buffer
                                 " *acsage-edit-success*"))
                               (state
                                '((interface . "sage")
                                  (prefix . 7)
                                  (module-name . "sage.matrix")
                                  (types . ("modules"
                                            "vars-in-module"))))
                               calls)
                           (unwind-protect
                               (with-current-buffer
                                   process-buffer
                                 (setq comint-redirect-completed t
                                       sage-shell:output-finished-p t)
                                 (let ((sage-shell:process-buffer
                                        process-buffer)
                                       (ac-sage-edit:-state-cached
                                        state))
                                   (cl-letf
                                       (((symbol-function
                                          'sage-shell-cpl:candidates)
                                         (lambda (&rest arguments)
                                           (push arguments calls)
                                           '("matrix"
                                             "matrix_space"
                                             "matrix_modn_dense"))))
                                     (list
                                      (ac-sage-edit:candidates)
                                      (nreverse calls)
                                      (eq
                                       (plist-get
                                        (car calls)
                                        :state)
                                       state)))))
                             (kill-buffer process-buffer)))"##,
        expect![[
            r#"OK (("matrix" "matrix_space" "matrix_modn_dense") ((:state ((interface . "sage") (prefix . 7) (module-name . "sage.matrix") (types "modules" "vars-in-module")))) t)"#
        ]],
    )
}

fn auto_complete_sage_complete_on_dot_prefix_covers_words_whitespace_dots_and_bob()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_complete_on_dot_prefix_covers_words_whitespace_dots_and_bob",
        r##"(mapcar
                           (lambda (fixture)
                             (with-temp-buffer
                               (insert (car fixture))
                               (goto-char
                                (or
                                 (nth 2 fixture)
                                 (point-max)))
                               (let ((ac-sage-complete-on-dot
                                      (cadr fixture)))
                                 (list
                                  fixture
                                  (point)
                                  (acsage-test-error
                                   #'ac-sage:complete-on-dot-prefix)))))
                           '(("matrix.rank" nil)
                             ("matrix." nil)
                             ("matrix." t)
                             ("matrix. " t)
                             ("alpha beta" t)
                             ("" nil)
                             ("" t)))"##,
        expect![[
            r#"OK ((("matrix.rank" nil) 12 (:value 8)) (("matrix." nil) 8 (:value nil)) (("matrix." t) 8 (:value 8)) (("matrix. " t) 9 (:value nil)) (("alpha beta" t) 11 (:value 7)) (("" nil) 1 (:value nil)) (("" t) 1 (:signal wrong-type-argument (number-or-marker-p nil))))"#
        ]],
    )
}

fn auto_complete_sage_edit_add_sources_appends_exact_order_and_preserves_duplicates()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_edit_add_sources_appends_exact_order_and_preserves_duplicates",
        r##"(let ((ac-sources
                                '(ac-source-filename
                                  ac-source-sage-modules)))
                           (ac-sage:add-sources)
                           (let ((first ac-sources))
                             (ac-sage:add-sources)
                             (list
                              first
                              ac-sources
                              (mapcar
                               (lambda (source)
                                 (list
                                  source
                                  (cl-count
                                   source
                                   ac-sources)))
                               '(ac-source-filename
                                 ac-source-sage-modules
                                 ac-source-sage-commands
                                 ac-source-sage-words-in-buffers)))))"##,
        expect![
            "OK ((ac-source-filename ac-source-sage-modules . #1=(ac-source-sage-modules ac-source-sage-vars-in-modules ac-source-sage-commands ac-source-sage-words-in-buffers)) (ac-source-filename ac-source-sage-modules ac-source-sage-modules ac-source-sage-vars-in-modules ac-source-sage-commands ac-source-sage-words-in-buffers . #1#) ((ac-source-filename 1) (ac-source-sage-modules 3) (ac-source-sage-commands 2) (ac-source-sage-words-in-buffers 2)))"
        ],
    )
}

fn auto_complete_sage_words_source_filters_real_buffers_by_sage_major_modes() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_words_source_filters_real_buffers_by_sage_major_modes",
        r##"(let ((sage-buffer
                                (generate-new-buffer
                                 " *acsage-words-sage*"))
                               (edit-buffer
                                (generate-new-buffer
                                 " *acsage-words-edit*"))
                               (text-buffer
                                (generate-new-buffer
                                 " *acsage-words-text*"))
                               inspected)
                           (unwind-protect
                               (progn
                                 (with-current-buffer
                                     sage-buffer
                                   (setq major-mode
                                         'sage-shell-mode))
                                 (with-current-buffer
                                     edit-buffer
                                   (setq major-mode
                                         'sage-shell:sage-mode))
                                 (with-current-buffer
                                     text-buffer
                                   (setq major-mode
                                         'text-mode))
                                 (cl-letf
                                     (((symbol-function
                                        'ac-word-candidates)
                                       (lambda (predicate)
                                         (mapcar
                                          (lambda (buffer)
                                            (let ((accepted
                                                   (funcall
                                                    predicate
                                                    buffer)))
                                              (push
                                               (list
                                                (buffer-name
                                                 buffer)
                                                (buffer-local-value
                                                 'major-mode
                                                 buffer)
                                                accepted)
                                               inspected)
                                              (and
                                               accepted
                                               (buffer-name
                                                buffer))))
                                          (list
                                           sage-buffer
                                           edit-buffer
                                           text-buffer)))))
                                   (list
                                    (ac-sage:words-in-sage-buffers)
                                    (nreverse inspected))))
                             (mapc
                              (lambda (buffer)
                                (when
                                    (buffer-live-p buffer)
                                  (kill-buffer buffer)))
                              (list
                               sage-buffer
                               edit-buffer
                               text-buffer))))"##,
        expect![[
            r#"OK ((" *acsage-words-sage*" " *acsage-words-edit*" nil) ((" *acsage-words-sage*" sage-shell-mode sage-shell-mode) (" *acsage-words-edit*" sage-shell:sage-mode sage-shell:sage-mode) (" *acsage-words-text*" text-mode nil)))"#
        ]],
    )
}

fn auto_complete_sage_real_edit_menu_completes_a_sage_command_through_target_source()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_sage_real_edit_menu_completes_a_sage_command_through_target_source",
        r##"(let ((process-buffer
                                (generate-new-buffer
                                 " *acsage-edit-menu-process*")))
                           (unwind-protect
                               (save-window-excursion
                                 (with-temp-buffer
                                   (switch-to-buffer
                                    (current-buffer))
                                   (let ((ac-use-comphist nil)
                                         (ac-use-quick-help nil)
                                         (ac-auto-show-menu t)
                                         (ac-expand-on-auto-complete nil)
                                         (ac-ignore-case nil)
                                         (ac-sources
                                          '(ac-source-sage-commands))
                                         (sage-shell:process-buffer
                                          process-buffer)
                                         (ac-sage-edit:-state-cached
                                          '((interface . "sage")
                                            (prefix . 1)
                                            (types . ("interface"))))
                                         events)
                                     (with-current-buffer
                                         process-buffer
                                       (setq
                                        comint-redirect-completed
                                        t
                                        sage-shell:output-finished-p
                                        t))
                                     (cl-letf
                                         (((symbol-function
                                            'sage-shell-cpl:completion-init)
                                           (lambda (sync &rest arguments)
                                             (push
                                              (list
                                               :init
                                               sync
                                               (plist-get
                                                arguments
                                                :compl-state))
                                              events)))
                                          ((symbol-function
                                            'sage-shell-cpl:candidates)
                                           (lambda (&rest arguments)
                                             (push
                                              (cons
                                               :candidates
                                               arguments)
                                              events)
                                             '("factor"
                                               "factorial"
                                               "factor_integer"))))
                                       (unwind-protect
                                           (progn
                                             (auto-complete-mode
                                              1)
                                             (insert
                                              "result = fac")
                                             (auto-complete)
                                             (let ((initial
                                                    (list
                                                     ac-prefix
                                                     (mapcar
                                                      #'substring-no-properties
                                                      ac-candidates)
                                                     (popup-live-p
                                                      ac-menu)
                                                     (substring-no-properties
                                                      (ac-selected-candidate)))))
                                               (ac-next)
                                               (ac-complete)
                                               (list
                                                initial
                                                (buffer-string)
                                                (nreverse events)
                                                ac-menu
                                                ac-completing
                                                ac-prefix)))
                                         (auto-complete-mode
                                          -1))))))
                             (kill-buffer process-buffer)))"##,
        expect![[
            r#"OK (("fac" ("factor" "factorial" "factor_integer") t "factor") "result = factorial" ((:init nil #1=((interface . "sage") (prefix . 1) (types "interface"))) (:candidates :state #1#)) nil nil nil)"#
        ]],
    )
}

pub(super) fn edit_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_sage_edit_generated_prefixes_use_cached_and_fresh_completion_states(),
        auto_complete_sage_edit_source_initializers_select_process_buffer_once_and_forward_state(),
        auto_complete_sage_edit_candidates_gate_missing_dead_and_unfinished_process_buffers(),
        auto_complete_sage_edit_candidates_forward_exact_cached_state_on_success(),
        auto_complete_sage_complete_on_dot_prefix_covers_words_whitespace_dots_and_bob(),
        auto_complete_sage_edit_add_sources_appends_exact_order_and_preserves_duplicates(),
        auto_complete_sage_words_source_filters_real_buffers_by_sage_major_modes(),
        auto_complete_sage_real_edit_menu_completes_a_sage_command_through_target_source(),
    ]
}

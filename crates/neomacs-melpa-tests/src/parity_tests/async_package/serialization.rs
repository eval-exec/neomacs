use expect_test::expect;

use super::ParityBatchCase;

fn async_public_defaults_and_environment_alias_match_the_pinned_release() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_public_defaults_and_environment_alias_match_the_pinned_release",
        r##"(list
               async-prompt-for-password
               async-process-noquery-on-exit
               async-debug
               async-send-over-pipe
               async-in-child-emacs
               async-quiet-switch
               (eq async-variables-noprops-function
                   #'async--purecopy)
               (eq (indirect-function 'async-inject-environment)
                   (indirect-function 'async-inject-variables)))"##,
        expect![[r#"OK (t nil nil t nil "-Q" t t)"#]],
    )
}

fn async_purecopy_strips_nested_string_properties_without_mutating_the_input() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_purecopy_strips_nested_string_properties_without_mutating_the_input",
        r##"(let* ((top (propertize "top" 'face 'bold))
                    (nested (propertize "nested" 'help-echo "tip"))
                    (key (propertize "key" 'category 'key))
                    (value (propertize "value" 'category 'value))
                    (original
                     (list top
                           (list 'inside nested)
                           (cons key value)
                           17
                           [vector]))
                    (copy (async--purecopy original)))
               (list
                copy
                (mapcar
                 (lambda (string)
                   (text-properties-at 0 string))
                 (list
                  (nth 0 copy)
                  (cadr (nth 1 copy))
                  (car (nth 2 copy))
                  (cdr (nth 2 copy))))
                (list
                 (text-properties-at 0 top)
                 (text-properties-at 0 nested)
                 (text-properties-at 0 key)
                 (text-properties-at 0 value))
                (eq (nth 4 copy) (nth 4 original))))"##,
        expect![[
            r#"OK (("top" (inside "nested") ("key" . "value") 17 [vector]) (nil nil nil nil) ((face bold) (help-echo "tip") (category key) (category value)) t)"#
        ]],
    )
}

fn async_inject_variables_honors_include_predicate_exclusion_and_noprops() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_inject_variables_honors_include_predicate_exclusion_and_noprops",
        r##"(progn
               (defvar neomacs-async-alpha nil)
               (defvar neomacs-async-beta nil)
               (defvar neomacs-async-excluded nil)
               (defvar neomacs-async-rejected nil)
               (defvar neomacs-async-syntax-table nil)
               (setq
                neomacs-async-alpha
                (propertize "alpha" 'face 'bold)
                neomacs-async-beta
                '(one (two . three))
                neomacs-async-excluded 7
                neomacs-async-rejected 8
                neomacs-async-syntax-table 9)
               (let ((form
                      (async-inject-variables
                       "\\`neomacs-async-"
                       (lambda (symbol)
                         (not
                          (eq symbol
                              'neomacs-async-rejected)))
                       "-excluded\\'"
                       t)))
                 (mapc
                  #'makunbound
                  '(neomacs-async-alpha
                    neomacs-async-beta
                    neomacs-async-excluded
                    neomacs-async-rejected
                    neomacs-async-syntax-table))
                 (eval form)
                 (list
                  neomacs-async-alpha
                  (text-properties-at
                   0 neomacs-async-alpha)
                  neomacs-async-beta
                  (boundp 'neomacs-async-excluded)
                  (boundp 'neomacs-async-rejected)
                  (boundp
                   'neomacs-async-syntax-table))))"##,
        expect![[r#"OK ("alpha" nil (one (two . three)) nil nil nil)"#]],
    )
}

fn async_message_packet_recognition_preserves_the_plist_marker_value() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_message_packet_recognition_preserves_the_plist_marker_value",
        r##"(mapcar
               #'async-message-p
               '(nil
                 t
                 symbol
                 ()
                 (:async-message nil)
                 (:async-message t)
                 (:payload 1 :async-message marker)
                 ((:async-message t))
                 (:async-message 0 :payload "value")))"##,
        expect![[r#"OK (nil nil nil nil nil t marker nil 0)"#]],
    )
}

fn async_wire_encoding_round_trips_unicode_vectors_and_dotted_pairs() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_wire_encoding_round_trips_unicode_vectors_and_dotted_pairs",
        r##"(let ((value
                     '("λ雪"
                       [alpha 17 "β"]
                       (left . right)
                       (:nested (1 2 3)))))
               (with-temp-buffer
                 (async--insert-sexp
                  (list 'quote value))
                 (let ((wire (buffer-string)))
                   (list
                    (string-prefix-p "\"" wire)
                    (string-suffix-p "\"\n" wire)
                    (async--receive-sexp wire)))))"##,
        expect![[r#"OK (t t ("λ雪" [alpha 17 "β"] (left . right) (:nested (1 2 3))))"#]],
    )
}

fn async_wire_encoding_preserves_shared_and_circular_structure() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_wire_encoding_preserves_shared_and_circular_structure",
        r##"(let* ((shared (list 'shared))
                    (cycle (list 'cycle))
                    (value (list shared shared cycle)))
               (setcdr cycle cycle)
               (with-temp-buffer
                 (async--insert-sexp
                  (list 'quote value))
                 (let* ((decoded
                         (async--receive-sexp
                          (buffer-string)))
                        (decoded-cycle
                         (nth 2 decoded)))
                   (list
                    (equal (car decoded) '(shared))
                    (eq (car decoded)
                        (cadr decoded))
                    (eq decoded-cycle
                        (cdr decoded-cycle))
                    (car decoded-cycle)))))"##,
        expect![[r#"OK (t t t cycle)"#]],
    )
}

fn async_handle_result_without_callback_stores_the_future_value_in_its_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_handle_result_without_callback_stores_the_future_value_in_its_buffer",
        r##"(let ((buffer
                     (generate-new-buffer
                      " *neomacs-async-result*")))
               (unwind-protect
                   (progn
                     (with-current-buffer buffer
                       (async-handle-result
                        nil
                        '(:answer 42)
                        buffer))
                     (with-current-buffer buffer
                       (list
                        async-callback-value-set
                        async-callback-value
                        (buffer-live-p buffer))))
                 (when (buffer-live-p buffer)
                   (kill-buffer buffer))))"##,
        expect![[r#"OK (t (:answer 42) t)"#]],
    )
}

fn async_handle_result_callback_receives_value_and_disposes_of_the_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_handle_result_callback_receives_value_and_disposes_of_the_buffer",
        r##"(let ((buffer
                     (generate-new-buffer
                      " *neomacs-async-callback*"))
                    received)
               (let ((async-debug nil))
                 (async-handle-result
                  (lambda (value)
                    (setq received
                          (list value
                                (buffer-live-p buffer))))
                  '(:done t)
                  buffer))
               (list received
                     (buffer-live-p buffer)))"##,
        expect![[r#"OK (((:done t) t) nil)"#]],
    )
}

fn async_handle_result_resignals_the_exact_child_error_and_cleans_up() -> ParityBatchCase {
    ParityBatchCase::signal(
        "async_handle_result_resignals_the_exact_child_error_and_cleans_up",
        r##"(let ((buffer
                     (generate-new-buffer
                      " *neomacs-async-signal*"))
                    (async-debug nil))
               (async-handle-result
                #'identity
                '(async-signal
                  (wrong-type-argument
                   integerp not-an-integer))
                buffer))"##,
        expect![[r#"ERR (wrong-type-argument integerp not-an-integer)"#]],
    )
}

fn async_child_program_arguments_preserve_quiet_init_batch_and_payload_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_child_program_arguments_preserve_quiet_init_batch_and_payload_order",
        r##"(let* ((async-quiet-switch "-q")
                    (async-child-init
                     (expand-file-name
                      "child-init.el"
                      temporary-file-directory))
                    (args
                     (async--emacs-program-args
                      '(lambda ()
                         (list "λ" 42))))
                    (payload (car (last args))))
               (list
                (nth 0 args)
                (nth 1 args)
                (file-name-nondirectory
                 (nth 2 args))
                (nth 3 args)
                (equal (nth 4 args)
                       async-child-init)
                (nth 5 args)
                (nth 6 args)
                (nth 7 args)
                (async--receive-sexp payload)))"##,
        expect![[
            r#"OK ("-q" "-l" "async.el" "-l" t "-batch" "-f" "async-batch-invoke" (lambda nil (list "λ" 42)))"#
        ]],
    )
}

fn async_sandbox_and_async_let_expand_to_the_upstream_callback_shape() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_sandbox_and_async_let_expand_to_the_upstream_callback_shape",
        r##"(list
               (macroexpand
                '(async-sandbox
                  (lambda () 42)))
               (macroexpand
                '(async-let
                     ((x (+ 1 2))
                      (y (lambda () (+ x 4))))
                   (list x y))))"##,
        expect![[
            r#"OK ((async-get (async-start (lambda nil 42))) (async-start (lambda nil (+ 1 2)) (lambda (x) (async-start (lambda nil (+ x 4)) (lambda (y) (progn (list x y)))))))"#
        ]],
    )
}

pub(super) fn serialization_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        async_public_defaults_and_environment_alias_match_the_pinned_release(),
        async_purecopy_strips_nested_string_properties_without_mutating_the_input(),
        async_inject_variables_honors_include_predicate_exclusion_and_noprops(),
        async_message_packet_recognition_preserves_the_plist_marker_value(),
        async_wire_encoding_round_trips_unicode_vectors_and_dotted_pairs(),
        async_wire_encoding_preserves_shared_and_circular_structure(),
        async_handle_result_without_callback_stores_the_future_value_in_its_buffer(),
        async_handle_result_callback_receives_value_and_disposes_of_the_buffer(),
        async_handle_result_resignals_the_exact_child_error_and_cleans_up(),
        async_child_program_arguments_preserve_quiet_init_batch_and_payload_order(),
        async_sandbox_and_async_let_expand_to_the_upstream_callback_shape(),
    ]
}

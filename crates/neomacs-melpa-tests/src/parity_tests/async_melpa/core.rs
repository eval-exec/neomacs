use super::ParityBatchCase;
use expect_test::expect;

fn current_public_defaults_aliases_and_custom_metadata_match_the_melpa_release() -> ParityBatchCase
{
    ParityBatchCase::value(
        "current_public_defaults_aliases_and_custom_metadata_match_the_melpa_release",
        r##"
(list
 async-prompt-for-password
 async-process-noquery-on-exit
 async-debug
 async-send-over-pipe
 async-in-child-emacs
 async-callback
 async-callback-for-process
 async-callback-value
 async-callback-value-set
 async-current-process
 async-child-init
 async-quiet-switch
 async-library
 async-inject-variables-exclude-regexps
 (eq
  async-variables-noprops-function
  #'async--purecopy)
 (eq
  (indirect-function
   'async-inject-environment)
  (indirect-function
   'async-inject-variables))
 (list
  (get 'async-prompt-for-password
       'custom-type)
  (get 'async-variables-noprops-function
       'custom-type)))
"##,
        expect![[
            r#"OK (t nil nil t nil nil nil nil nil nil nil "-Q" nil ("-syntax-table\\'" "-abbrev-table\\'") t t (boolean function))"#
        ]],
    )
}

fn purecopy_strips_nested_string_properties_and_preserves_nonstrings_and_input() -> ParityBatchCase
{
    ParityBatchCase::value(
        "purecopy_strips_nested_string_properties_and_preserves_nonstrings_and_input",
        r##"
(let* ((top
        (propertize
         "top" 'face 'bold))
       (nested
        (propertize
         "nested" 'help-echo "tip"))
       (key
        (propertize
         "key" 'category 'key))
       (value
        (propertize
         "value" 'category 'value))
       (vector [vector])
       (original
        (list
         top
         (list 'inside nested)
         (cons key value)
         17 vector))
       (copy
        (async--purecopy original)))
  (list
   copy
   (mapcar
    (lambda (string)
      (text-properties-at
       0 string))
    (list
     (nth 0 copy)
     (cadr (nth 1 copy))
     (car (nth 2 copy))
     (cdr (nth 2 copy))))
   (mapcar
    (lambda (string)
      (text-properties-at
       0 string))
    (list top nested key value))
   (eq
    (nth 4 copy)
    vector)
   (async--purecopy 42)
   (async--purecopy nil)))
"##,
        expect![[
            r#"OK (("top" (inside "nested") ("key" . "value") 17 [vector]) (nil nil nil nil) ((face bold) (help-echo "tip") (category key) (category value)) t 42 nil)"#
        ]],
    )
}

fn inject_variables_honors_include_predicate_exclusions_quoting_vectors_and_noprops()
-> ParityBatchCase {
    ParityBatchCase::value(
        "inject_variables_honors_include_predicate_exclusions_quoting_vectors_and_noprops",
        r##"
(progn
  (defvar async-melpa-alpha nil)
  (defvar async-melpa-beta nil)
  (defvar async-melpa-vector nil)
  (defvar async-melpa-excluded nil)
  (defvar async-melpa-rejected nil)
  (defvar async-melpa-syntax-table nil)
  (setq
   async-melpa-alpha
   (propertize
    "alpha" 'face 'bold)
   async-melpa-beta
   '(one (two . three))
   async-melpa-vector
   [1 two "three"]
   async-melpa-excluded 7
   async-melpa-rejected 8
   async-melpa-syntax-table 9)
  (let ((form
         (async-inject-variables
          "\\`async-melpa-"
          (lambda (symbol)
            (not
             (eq symbol
                 'async-melpa-rejected)))
          "-excluded\\'"
          t)))
    (mapc
     #'makunbound
     '(async-melpa-alpha
       async-melpa-beta
       async-melpa-vector
       async-melpa-excluded
       async-melpa-rejected
       async-melpa-syntax-table))
    (eval form)
    (list
     form
     async-melpa-alpha
     (text-properties-at
      0 async-melpa-alpha)
     async-melpa-beta
     async-melpa-vector
     (boundp
      'async-melpa-excluded)
     (boundp
      'async-melpa-rejected)
     (boundp
      'async-melpa-syntax-table))))
"##,
        expect![[
            r#"OK ((setq async-melpa-vector #2=[1 two "three"] async-melpa-alpha "alpha" async-melpa-beta '#1=(one (two . three))) "alpha" nil #1# #2# nil nil nil)"#
        ]],
    )
}

fn message_packet_recognition_preserves_marker_values_and_rejects_nonplists() -> ParityBatchCase {
    ParityBatchCase::value(
        "message_packet_recognition_preserves_marker_values_and_rejects_nonplists",
        r##"
(mapcar
 #'async-message-p
 '(nil
   t
   symbol
   ()
   (:async-message nil)
   (:async-message t)
   (:payload 1
    :async-message marker)
   ((:async-message t))
   (:async-message 0
    :payload "value")))
"##,
        expect!["OK (nil nil nil nil nil t marker nil 0)"],
    )
}

fn wire_encoding_round_trips_unicode_vectors_dotted_pairs_and_embedded_eof() -> ParityBatchCase {
    ParityBatchCase::value(
        "wire_encoding_round_trips_unicode_vectors_dotted_pairs_and_embedded_eof",
        r##"
(let ((value
       '("λ雪"
         [alpha 17 "β"]
         (left . right)
         (:nested
          (1 2 3))
         "before\C-dafter")))
  (with-temp-buffer
    (async--insert-sexp
     (list 'quote value))
    (let ((wire
           (buffer-string)))
      (list
       (string-prefix-p
        "\"" wire)
       (string-suffix-p
        "\"\n" wire)
       (string-match-p
        "\n" wire)
       (async--receive-sexp
        wire)))))
"##,
        expect![[
            r#"OK (t t 102 ("λ雪" [alpha 17 "β"] (left . right) (:nested (1 2 3)) "before\4after"))"#
        ]],
    )
}

fn wire_encoding_preserves_shared_and_circular_structure() -> ParityBatchCase {
    ParityBatchCase::value(
        "wire_encoding_preserves_shared_and_circular_structure",
        r##"
(let* ((shared
        (list 'shared))
       (cycle
        (list 'cycle))
       (value
        (list shared shared cycle)))
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
       (equal
        (car decoded)
        '(shared))
       (eq
        (car decoded)
        (cadr decoded))
       (eq
        decoded-cycle
        (cdr decoded-cycle))
       (car decoded-cycle)))))
"##,
        expect!["OK (t t t cycle)"],
    )
}

fn handle_result_stores_future_values_and_callback_values_with_expected_cleanup() -> ParityBatchCase
{
    ParityBatchCase::value(
        "handle_result_stores_future_values_and_callback_values_with_expected_cleanup",
        r##"
(let ((future-buffer
       (generate-new-buffer
        " *async-melpa-future*"))
      (callback-buffer
       (generate-new-buffer
        " *async-melpa-callback*"))
      received)
  (unwind-protect
      (progn
        (with-current-buffer
            future-buffer
          (async-handle-result
           nil
           '(:answer 42)
           future-buffer))
        (let ((async-debug nil))
          (async-handle-result
           (lambda (value)
             (setq received
                   (list
                    value
                    (buffer-live-p
                     callback-buffer))))
           '(:done t)
           callback-buffer))
        (list
         (with-current-buffer
             future-buffer
           (list
            async-callback-value-set
            async-callback-value))
         received
         (buffer-live-p
          future-buffer)
         (buffer-live-p
          callback-buffer)))
    (when
        (buffer-live-p
         future-buffer)
      (kill-buffer
       future-buffer))
    (when
        (buffer-live-p
         callback-buffer)
      (kill-buffer
       callback-buffer))))
"##,
        expect!["OK ((t (:answer 42)) ((:done t) t) t nil)"],
    )
}

fn handle_result_resignals_exact_child_error_and_cleans_buffer() -> ParityBatchCase {
    ParityBatchCase::signal(
        "handle_result_resignals_exact_child_error_and_cleans_buffer",
        r##"
(let ((buffer
       (generate-new-buffer
        " *async-melpa-signal*"))
      (async-debug nil))
  (async-handle-result
   #'identity
   '(async-signal
     (wrong-type-argument
      integerp not-an-integer))
   buffer))
"##,
        expect!["ERR (wrong-type-argument integerp not-an-integer)"],
    )
}

fn child_program_arguments_cover_pipe_argument_child_init_cached_library_and_quiet_switch()
-> ParityBatchCase {
    ParityBatchCase::value(
        "child_program_arguments_cover_pipe_argument_child_init_cached_library_and_quiet_switch",
        r##"
(let* ((async-quiet-switch
        "-q")
       (async-library
        (locate-library
         "async"))
       (async-child-init
        (async-melpa-test-path
         "child/init.el"))
       (args
        (async--emacs-program-args
         '(lambda ()
            (list "λ" 42))))
       (payload
        (car
         (last args))))
  (list
   (nth 0 args)
   (nth 1 args)
   (file-name-nondirectory
    (nth 2 args))
   (nth 3 args)
   (equal
    (nth 4 args)
    async-child-init)
   (nth 5 args)
   (nth 6 args)
   (nth 7 args)
   (async--receive-sexp
    payload)
   (car
    (last
     (let ((async-child-init nil))
       (async--emacs-program-args))))))
"##,
        expect![[
            r#"OK ("-q" "-l" "async.el" "-l" t "-batch" "-f" "async-batch-invoke" (lambda nil (list "λ" 42)) "<none>")"#
        ]],
    )
}

fn sandbox_let_and_fold_left_expand_to_current_callback_shapes() -> ParityBatchCase {
    ParityBatchCase::value(
        "sandbox_let_and_fold_left_expand_to_current_callback_shapes",
        r##"
(list
 (macroexpand
  '(async-sandbox
    (lambda () 42)))
 (macroexpand
  '(async-let
       ((x (+ 1 2))
        (y
         (lambda ()
           (+ x 4))))
     (list x y)))
 (async--fold-left
  (lambda (acc binding)
    (list
     :binding binding
     :inside acc))
  '(done)
  '(alpha
    (beta 2)
    gamma)))
"##,
        expect![
            "OK ((async-get (async-start (lambda nil 42))) (async-start (lambda nil (+ 1 2)) (lambda (x) (async-start (lambda nil (+ x 4)) (lambda (y) (progn (list x y)))))) (:binding (gamma) :inside (:binding (beta 2) :inside (:binding (alpha) :inside (done)))))"
        ],
    )
}

fn send_parent_branch_transmits_quoted_message_and_child_branch_prints_wire_packet()
-> ParityBatchCase {
    ParityBatchCase::value(
        "send_parent_branch_transmits_quoted_message_and_child_branch_prints_wire_packet",
        r##"
(let (transmissions
      child-output)
  (cl-letf
      (((symbol-function
         'async--transmit-sexp)
        (lambda (process sexp)
          (push
           (list process sexp)
           transmissions)
          :sent)))
    (let ((async-in-child-emacs nil))
      (async-send
       'fixture-process
       :operation 'sum
       :values '(2 3 5))))
  (with-temp-buffer
    (let ((standard-output
           (current-buffer))
          (async-in-child-emacs t))
      (async-send
       :phase 'complete
       :payload "λ")
      (setq child-output
            (buffer-string))))
  (list
   transmissions
   (string-prefix-p
    "\n\"" child-output)
   (car
    (read-from-string
     (decode-coding-string
      (base64-decode-string
       (read
        (substring child-output 1)))
      'utf-8-emacs-unix)))))
"##,
        expect![[
            r#"OK (((fixture-process '(:operation sum :values (2 3 5) :async-message t))) t (:phase complete :payload "λ" :async-message t))"#
        ]],
    )
}

fn receive_delegates_to_wire_receiver_and_batch_invoke_prints_value_and_signal_protocols()
-> ParityBatchCase {
    ParityBatchCase::value(
        "receive_delegates_to_wire_receiver_and_batch_invoke_prints_value_and_signal_protocols",
        r##"
(let (receive-calls
      value-output
      signal-output)
  (cl-letf
      (((symbol-function
         'async--receive-sexp)
        (lambda (&optional stream)
          (push stream receive-calls)
          :received)))
    (async-receive))
  (with-temp-buffer
    (let ((standard-output
           (current-buffer)))
      (cl-letf
          (((symbol-function
             'async--receive-sexp)
            (lambda (&optional _)
              (lambda ()
                '(:value "λ")))))
        (async-batch-invoke))
      (setq value-output
            (buffer-string))))
  (with-temp-buffer
    (let ((standard-output
           (current-buffer)))
      (cl-letf
          (((symbol-function
             'async--receive-sexp)
            (lambda (&optional _)
              (lambda ()
                (error
                 "child failure")))))
        (async-batch-invoke))
      (setq signal-output
            (buffer-string))))
  (list
   receive-calls
   value-output
   signal-output
   async-in-child-emacs
   command-line-args-left))
"##,
        expect![[
            r#"OK ((nil) "\n(:value \"λ\")\n" "\n(async-signal (error \"child failure\"))\n" t nil)"#
        ]],
    )
}

fn read_from_client_reassembles_fragmented_multiple_wire_messages() -> ParityBatchCase {
    ParityBatchCase::value(
        "read_from_client_reassembles_fragmented_multiple_wire_messages",
        r##"
(let* ((buffer
        (generate-new-buffer
         " *async-melpa-client*"))
       (process
        (make-process
         :name
         "async-melpa-client"
         :buffer buffer
         :command
         '("sh" "-c" "sleep 2")
         :noquery t))
       first second combined events)
  (unwind-protect
      (progn
        (setq first
              (with-temp-buffer
                (async--insert-sexp
                 '(:phase first
                   :async-message t))
                (buffer-string))
              second
              (with-temp-buffer
                (async--insert-sexp
                 '(:phase second
                   :payload "λ"
                   :async-message t))
                (buffer-string))
              combined
              (concat first second))
        (with-current-buffer buffer
          (setq-local
           async-read-marker
           (set-marker
            (make-marker)
            (point-min)
            buffer))
          (setq-local
           async-callback
           (lambda (value)
             (push value events))))
        (let ((split
               (/ (length combined) 2)))
          (async-read-from-client
           process
           (substring
            combined 0 split))
          (async-read-from-client
           process
           (substring
            combined split)))
        (with-current-buffer buffer
          (list
           (nreverse events)
           (marker-position
            async-read-marker)
           (buffer-string))))
    (when
        (process-live-p process)
      (delete-process process))
    (when
        (buffer-live-p buffer)
      (kill-buffer buffer))))
"##,
        expect![[
            r#"OK (((:phase first :async-message t) (:phase second :payload "λ" :async-message t)) 115 "\"KDpwaGFzZSBmaXJzdCA6YXN5bmMtbWVzc2FnZSB0KQ==\"\n\"KDpwaGFzZSBzZWNvbmQgOnBheWxvYWQgIs67IiA6YXN5bmMtbWVzc2FnZSB0KQ==\"\n")"#
        ]],
    )
}

fn start_future_returns_structured_unicode_and_transitions_to_ready() -> ParityBatchCase {
    ParityBatchCase::value(
        "start_future_returns_structured_unicode_and_transitions_to_ready",
        r##"
(let* ((future
        (async-start
         (lambda ()
           (sleep-for 0.05)
           (list
            "λ雪"
            [1 two 3]
            '(:nested
              ((left . right)))))))
       (ready-before
        (async-ready future))
       (value
        (async-get future)))
  (list
   ready-before
   value
   (async-ready future)
   (buffer-live-p
    (process-buffer future))))
"##,
        expect![[r#"OK (nil ("λ雪" [1 two 3] (:nested ((left . right)))) t nil)"#]],
    )
}

fn start_future_resignals_exact_child_error() -> ParityBatchCase {
    ParityBatchCase::signal(
        "start_future_resignals_exact_child_error",
        r##"
(async-get
 (async-start
  (lambda ()
    (signal
     'wrong-type-argument
     '(integerp child-value)))))
"##,
        expect!["ERR (wrong-type-argument integerp child-value)"],
    )
}

fn callback_receives_messages_before_final_result_and_future_then_yields_nil() -> ParityBatchCase {
    ParityBatchCase::value(
        "callback_receives_messages_before_final_result_and_future_then_yields_nil",
        r##"
(let (events)
  (let ((future
         (async-start
          (lambda ()
            (async-send
             :phase 'first
             :payload "λ")
            (async-send
             :phase 'second
             :payload '(1 2 3))
            'finished)
          (lambda (value)
            (push value events)))))
    (async-wait future)
    (list
     (nreverse events)
     (async-get future)
     (async-ready future)
     (buffer-live-p
      (process-buffer future)))))
"##,
        expect![[
            r#"OK (((:phase first :payload "λ" :async-message t) (:phase second :payload (1 2 3) :async-message t) finished) nil t nil)"#
        ]],
    )
}

fn parent_to_child_message_roundtrip_supports_real_request_response_workflow() -> ParityBatchCase {
    ParityBatchCase::value(
        "parent_to_child_message_roundtrip_supports_real_request_response_workflow",
        r##"
(let (received)
  (let ((future
         (async-start
          (lambda ()
            (let ((message
                   (async-receive)))
              (list
               (plist-get
                message :operation)
               (apply
                #'+
                (plist-get
                 message :values))
               (async-message-p
                message))))
          (lambda (value)
            (setq received value)))))
    (async-send
     future
     :operation 'sum
     :values '(2 3 5 7))
    (async-wait future)
    (list
     received
     (async-get future)
     (async-ready future))))
"##,
        expect!["OK ((sum 17 t) nil t)"],
    )
    .fresh_process()
}

fn callback_reassembles_message_larger_than_process_chunk_with_unicode_edges() -> ParityBatchCase {
    ParityBatchCase::value(
        "callback_reassembles_message_larger_than_process_chunk_with_unicode_edges",
        r##"
(let (events)
  (let ((future
         (async-start
          (lambda ()
            (async-send
             :payload
             (concat
              "λ"
              (make-string
               65536 ?x)
              "雪"))
            'finished)
          (lambda (value)
            (push
             (if
                 (async-message-p
                  value)
                 (let ((payload
                        (plist-get
                         value :payload)))
                   (list
                    'message
                    (length payload)
                    (substring
                     payload 0 2)
                    (substring
                     payload -2)))
               value)
             events)))))
    (async-wait future)
    (nreverse events)))
"##,
        expect![[r#"OK ((message 65541 "λ" "��") finished)"#]],
    )
}

fn sandbox_and_async_let_execute_real_child_workflows() -> ParityBatchCase {
    ParityBatchCase::value(
        "sandbox_and_async_let_execute_real_child_workflows",
        r##"
(let ((sandbox-value
       (async-sandbox
        (lambda ()
          (let ((values
                 '(1 2 3 4 5)))
            (list
             (apply #'+ values)
             (mapcar
              (lambda (value)
                (* value value))
              values)
             "λ雪")))))
      received)
  (let ((outer
         (async-let
             ((x (+ 1 2))
              (y (+ 3 4)))
           (setq received
                 (list x y)))))
    (async-wait outer)
    (async-melpa-test-wait-until
     (lambda ()
       received))
    (list
     sandbox-value
     received)))
"##,
        expect![[r#"OK ((15 (1 4 9 16 25) "λ雪") (3 7))"#]],
    )
}

fn start_process_future_callback_failure_and_noquery_cover_real_process_lifecycle()
-> ParityBatchCase {
    ParityBatchCase::value(
        "start_process_future_callback_failure_and_noquery_cover_real_process_lifecycle",
        r##"
(let (callback-result
      query noquery)
  (let* ((success
          (async-start-process
           "async-melpa-success"
           "sh" nil "-c"
           "printf 'alpha\\nbeta\\n'"))
         (success-result
          (async-get success))
         (callback
          (async-start-process
           "async-melpa-callback"
           "sh"
           (lambda (finished)
             (setq callback-result
                   (list
                    (process-exit-status
                     finished)
                    (with-current-buffer
                        (process-buffer
                         finished)
                      (buffer-string)))))
           "-c"
           "printf callback-output"))
         (failure
          (async-start-process
           "async-melpa-failure"
           "sh" nil "-c"
           "printf partial; exit 7"))
         (failure-result
          (async-get failure)))
    (async-wait callback)
    (let ((async-process-noquery-on-exit
           nil))
      (setq query
            (async-start-process
             "async-melpa-query"
             "sh" nil "-c"
             "sleep 0.05")))
    (let ((async-process-noquery-on-exit
           t))
      (setq noquery
            (async-start-process
             "async-melpa-noquery"
             "sh" nil "-c"
             "sleep 0.05")))
    (let ((query-flags
           (list
            (process-query-on-exit-flag
             query)
            (process-query-on-exit-flag
             noquery))))
      (async-get query)
      (async-get noquery)
      (list
       (eq success-result success)
       (process-exit-status
        success)
       callback-result
       (async-get callback)
       failure-result
       (process-exit-status
        failure)
       query-flags
       (mapcar
        (lambda (process)
          (buffer-live-p
           (process-buffer process)))
        (list
         success callback failure
         query noquery))))))
"##,
        expect![[
            r#"OK (t 0 (0 "callback-output") nil (error "Async process 'async-melpa-failure' failed with exit code 7") 7 (t nil) (nil nil nil nil nil))"#
        ]],
    )
}

pub(super) fn core_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        current_public_defaults_aliases_and_custom_metadata_match_the_melpa_release(),
        purecopy_strips_nested_string_properties_and_preserves_nonstrings_and_input(),
        inject_variables_honors_include_predicate_exclusions_quoting_vectors_and_noprops(),
        message_packet_recognition_preserves_marker_values_and_rejects_nonplists(),
        wire_encoding_round_trips_unicode_vectors_dotted_pairs_and_embedded_eof(),
        wire_encoding_preserves_shared_and_circular_structure(),
        handle_result_stores_future_values_and_callback_values_with_expected_cleanup(),
        handle_result_resignals_exact_child_error_and_cleans_buffer(),
        child_program_arguments_cover_pipe_argument_child_init_cached_library_and_quiet_switch(),
        sandbox_let_and_fold_left_expand_to_current_callback_shapes(),
        send_parent_branch_transmits_quoted_message_and_child_branch_prints_wire_packet(),
        receive_delegates_to_wire_receiver_and_batch_invoke_prints_value_and_signal_protocols(),
        read_from_client_reassembles_fragmented_multiple_wire_messages(),
        start_future_returns_structured_unicode_and_transitions_to_ready(),
        start_future_resignals_exact_child_error(),
        callback_receives_messages_before_final_result_and_future_then_yields_nil(),
        parent_to_child_message_roundtrip_supports_real_request_response_workflow(),
        callback_reassembles_message_larger_than_process_chunk_with_unicode_edges(),
        sandbox_and_async_let_execute_real_child_workflows(),
        start_process_future_callback_failure_and_noquery_cover_real_process_lifecycle(),
    ]
}

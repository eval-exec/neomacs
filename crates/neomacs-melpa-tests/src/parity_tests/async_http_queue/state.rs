use expect_test::expect;

use super::ParityBatchCase;

fn async_http_queue_state_constructor_predicate_accessors_and_no_copier_contract() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async_http_queue_state_constructor_predicate_accessors_and_no_copier_contract",
        r##"(let* ((completion
                (lambda (_) :complete))
               (failure
                (lambda (_) :failed))
               (parser
                (lambda () :parsed))
               (queue
                '(((url . "https://api.test/one")
                   (status . pending)
                   (data . nil))))
               (state
                (async-http-queue--state-create
                 :queue queue
                 :active-workers 2
                 :max-concurrent 7
                 :timeout 19
                 :parser parser
                 :completion-callback completion
                 :error-callback failure)))
          (list
           (async-http-queue--state-p state)
           (async-http-queue--state-p queue)
           (equal
            (async-http-queue--state-queue state)
            queue)
           (async-http-queue--state-active-workers
            state)
           (async-http-queue--state-max-concurrent
            state)
           (async-http-queue--state-timeout state)
           (eq
            (async-http-queue--state-parser state)
            parser)
           (eq
            (async-http-queue--state-completion-callback
             state)
            completion)
           (eq
            (async-http-queue--state-error-callback
             state)
            failure)
           (fboundp
            'copy-async-http-queue--state)
           (type-of state)))"##,
        expect!["OK (t nil t 2 7 19 t t t nil async-http-queue--state)"],
    )
}

fn async_http_queue_every_generated_accessor_supports_setf_mutation() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_every_generated_accessor_supports_setf_mutation",
        r##"(let* ((state
                (async-http-queue-test-state
                 '("https://api.test/old")
                 1
                 2
                 :default))
               (completion
                (lambda (_) :new-completion))
               (failure
                (lambda (_) :new-failure))
               (parser
                (lambda () :new-parser)))
          (setf
           (async-http-queue--state-queue state)
           '(((url . "https://api.test/new")
              (status . processing)
              (data . "seed")))
           (async-http-queue--state-active-workers
            state)
           4
           (async-http-queue--state-max-concurrent
            state)
           9
           (async-http-queue--state-timeout state)
           31
           (async-http-queue--state-parser state)
           parser
           (async-http-queue--state-completion-callback
            state)
           completion
           (async-http-queue--state-error-callback
            state)
           failure)
          (list
           (async-http-queue-test-state-snapshot
            state)
           (eq
            (async-http-queue--state-parser state)
            parser)
           (eq
            (async-http-queue--state-completion-callback
             state)
            completion)
           (eq
            (async-http-queue--state-error-callback
             state)
            failure)))"##,
        expect![[
            r#"OK ((:queue (("https://api.test/new" processing "seed")) :active 4 :limit 9 :timeout 31 :parser :custom :completion t :error t) t t t)"#
        ]],
    )
}

fn async_http_queue_update_status_copy_on_write_preserves_input_and_other_fields() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async_http_queue_update_status_copy_on_write_preserves_input_and_other_fields",
        r##"(let* ((state
                (async-http-queue-test-state
                 '("https://api.test/a"
                   "https://api.test/b")))
               (queue-before
                (async-http-queue--state-queue state))
               (first-before
                (car queue-before))
               (second-before
                (cadr queue-before)))
          (setcdr
           (assoc
            'data
            second-before)
           '(:existing payload))
          (async-http-queue--update-status
           state
           "https://api.test/b"
           'processing)
          (let* ((queue-after
                  (async-http-queue--state-queue
                   state))
                 (first-after
                  (car queue-after))
                 (second-after
                  (cadr queue-after)))
            (list
             (async-http-queue-test-queue-snapshot
              state)
             (eq queue-before queue-after)
             (eq first-before first-after)
             (eq second-before second-after)
             (alist-get 'status second-before)
             (alist-get 'data second-after))))"##,
        expect![[
            r#"OK ((("https://api.test/a" pending (:existing payload)) ("https://api.test/b" processing #1=(:existing payload))) nil t nil pending #1#)"#
        ]],
    )
}

fn async_http_queue_update_data_copies_matching_item_and_preserves_status_and_neighbors()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_update_data_copies_matching_item_and_preserves_status_and_neighbors",
        r##"(let* ((payload
                '((id . 42)
                  (nested . [a b c])))
               (state
                (async-http-queue-test-state
                 '("https://api.test/a"
                   "https://api.test/b"
                   "https://api.test/c")))
               (middle-before
                (cadr
                 (async-http-queue--state-queue
                  state))))
          (async-http-queue--update-status
           state
           "https://api.test/b"
           'processing)
          (async-http-queue--update-data
           state
           "https://api.test/b"
           payload)
          (let ((middle-after
                 (cadr
                  (async-http-queue--state-queue
                   state))))
            (list
             (async-http-queue-test-queue-snapshot
              state)
             (eq middle-before middle-after)
             (equal
              (alist-get 'data middle-after)
              payload)
             (eq
              (alist-get 'data middle-after)
              payload))))"##,
        expect![[
            r#"OK ((("https://api.test/a" pending nil) ("https://api.test/b" processing ((id . 42) (nested . [a b c]))) ("https://api.test/c" pending nil)) nil t t)"#
        ]],
    )
}

fn async_http_queue_updates_for_unknown_url_rebuild_queue_without_changing_values()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_updates_for_unknown_url_rebuild_queue_without_changing_values",
        r##"(let* ((state
                (async-http-queue-test-state
                 '("https://api.test/a"
                   "https://api.test/b")))
               (queue-before
                (async-http-queue--state-queue state))
               (items-before
                (copy-sequence queue-before)))
          (async-http-queue--update-status
           state
           "https://api.test/missing"
           'done)
          (async-http-queue--update-data
           state
           "https://api.test/missing"
           :ghost)
          (let ((queue-after
                 (async-http-queue--state-queue
                  state)))
            (list
             (async-http-queue-test-queue-snapshot
              state)
             (eq queue-before queue-after)
             (cl-mapcar
              #'eq
              items-before
              queue-after))))"##,
        expect![[
            r#"OK ((("https://api.test/a" pending nil) ("https://api.test/b" pending nil)) nil (t t))"#
        ]],
    )
}

fn async_http_queue_duplicate_urls_are_all_updated_by_each_url_keyed_mutation() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_duplicate_urls_are_all_updated_by_each_url_keyed_mutation",
        r##"(let ((state
               (async-http-queue-test-state
                '("https://api.test/same"
                  "https://api.test/other"
                  "https://api.test/same"))))
          (async-http-queue--update-status
           state
           "https://api.test/same"
           'processing)
          (async-http-queue--update-data
           state
           "https://api.test/same"
           'shared-result)
          (async-http-queue-test-queue-snapshot
           state))"##,
        expect![[
            r#"OK (("https://api.test/same" processing shared-result) ("https://api.test/other" pending nil) ("https://api.test/same" processing shared-result))"#
        ]],
    )
}

fn async_http_queue_malformed_matching_item_signals_during_missing_slot_mutation() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async_http_queue_malformed_matching_item_signals_during_missing_slot_mutation",
        r##"(let ((status-state
               (async-http-queue--state-create
                :queue
                '(((url . "https://api.test/a")
                   (data . nil)))))
              (data-state
               (async-http-queue--state-create
                :queue
                '(((url . "https://api.test/a")
                   (status . pending))))))
          (list
           (async-http-queue-test-error-data
            (lambda ()
              (async-http-queue--update-status
               status-state
               "https://api.test/a"
               'done)))
           (async-http-queue-test-error-data
            (lambda ()
              (async-http-queue--update-data
               data-state
               "https://api.test/a"
               :value)))
           (async-http-queue--state-queue
            status-state)
           (async-http-queue--state-queue
            data-state)))"##,
        expect![[
            r#"OK ((:error wrong-type-argument (consp nil)) (:error wrong-type-argument (consp nil)) (((url . "https://api.test/a") (data))) (((url . "https://api.test/a") (status . pending))))"#
        ]],
    )
}

fn async_http_queue_completion_preserves_input_order_across_mixed_terminal_states()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_completion_preserves_input_order_across_mixed_terminal_states",
        r##"(let* ((results)
               (messages)
               (state
                (async-http-queue-test-state
                 '("https://api.test/first"
                   "https://api.test/second"
                   "https://api.test/third")
                 2
                 10
                 nil
                 (lambda (value)
                   (push value results)))))
          (async-http-queue--update-status
           state
           "https://api.test/third"
           'done)
          (async-http-queue--update-data
           state
           "https://api.test/third"
           '(:id 3))
          (async-http-queue--update-status
           state
           "https://api.test/first"
           'done)
          (async-http-queue--update-data
           state
           "https://api.test/first"
           '(:id 1))
          (async-http-queue--update-status
           state
           "https://api.test/second"
           'error)
          (cl-letf
              (((symbol-function 'message)
                (lambda (format-string &rest arguments)
                  (push
                   (apply
                    #'format
                    format-string
                    arguments)
                   messages))))
            (async-http-queue--check-completion
             state))
          (list
           (mapcar
            (lambda (value)
              (list
               (vectorp value)
               (append value nil)))
            (nreverse results))
           (nreverse messages)
           (async-http-queue-test-queue-snapshot
            state)))"##,
        expect![[
            r#"OK (((t (#1=(:id 1) nil #2=(:id 3)))) ("Loaded 2 URLs (1 failed)") (("https://api.test/first" done #1#) ("https://api.test/second" error nil) ("https://api.test/third" done #2#)))"#
        ]],
    )
}

fn async_http_queue_completion_waits_for_pending_and_processing_items_with_progress()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_completion_waits_for_pending_and_processing_items_with_progress",
        r##"(let* ((urls
                (cl-loop
                 for index from 1 to 12
                 collect
                 (format
                  "https://api.test/%02d"
                  index)))
               (callbacks 0)
               messages
               (state
                (async-http-queue-test-state
                 urls
                 3
                 10
                 nil
                 (lambda (_)
                   (cl-incf callbacks)))))
          (async-http-queue--update-status
           state
           "https://api.test/01"
           'done)
          (async-http-queue--update-data
           state
           "https://api.test/01"
           :one)
          (async-http-queue--update-status
           state
           "https://api.test/02"
           'error)
          (async-http-queue--update-status
           state
           "https://api.test/03"
           'processing)
          (cl-letf
              (((symbol-function 'message)
                (lambda (format-string &rest arguments)
                  (push
                   (apply
                    #'format
                    format-string
                    arguments)
                   messages))))
            (async-http-queue--check-completion
             state))
          (list
           callbacks
           (nreverse messages)
           (seq-count
            (lambda (item)
              (eq
               (alist-get 'status item)
               'pending))
            (async-http-queue--state-queue
             state))
           (async-http-queue--state-active-workers
            state)))"##,
        expect![[r#"OK (0 ("Loading URLs... 1/12 completed (1 failed)") 9 0)"#]],
    )
}

fn async_http_queue_empty_state_completes_with_empty_vector_and_nil_callback_is_safe()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_empty_state_completes_with_empty_vector_and_nil_callback_is_safe",
        r##"(let* (results
               messages
               (state
                (async-http-queue-test-state
                 nil
                 5
                 10
                 nil
                 (lambda (value)
                   (push value results)))))
          (cl-letf
              (((symbol-function 'message)
                (lambda (format-string &rest arguments)
                  (push
                   (apply
                    #'format
                    format-string
                    arguments)
                   messages))))
            (async-http-queue--check-completion
             state)
            (setf
             (async-http-queue--state-completion-callback
              state)
             nil)
            (async-http-queue--check-completion
             state))
          (list
           (mapcar
            (lambda (value)
              (list
               (vectorp value)
               (length value)))
            results)
           (nreverse messages)))"##,
        expect![[r#"OK (((t 0)) ("Loaded 0 URLs" "Loaded 0 URLs"))"#]],
    )
}

fn async_http_queue_check_completion_is_not_idempotent_after_terminal_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_check_completion_is_not_idempotent_after_terminal_state",
        r##"(let* (results
               messages
               (state
                (async-http-queue-test-state
                 '("https://api.test/a")
                 1
                 10
                 nil
                 (lambda (value)
                   (push
                    (append value nil)
                    results)))))
          (async-http-queue--update-status
           state
           "https://api.test/a"
           'done)
          (async-http-queue--update-data
           state
           "https://api.test/a"
           :payload)
          (cl-letf
              (((symbol-function 'message)
                (lambda (format-string &rest arguments)
                  (push
                   (apply
                    #'format
                    format-string
                    arguments)
                   messages))))
            (async-http-queue--check-completion
             state)
            (async-http-queue--check-completion
             state))
          (list
           (nreverse results)
           (nreverse messages)))"##,
        expect![[r#"OK (((:payload) (:payload)) ("Loaded 1 URLs" "Loaded 1 URLs"))"#]],
    )
}

pub(super) fn state_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        async_http_queue_state_constructor_predicate_accessors_and_no_copier_contract(),
        async_http_queue_every_generated_accessor_supports_setf_mutation(),
        async_http_queue_update_status_copy_on_write_preserves_input_and_other_fields(),
        async_http_queue_update_data_copies_matching_item_and_preserves_status_and_neighbors(),
        async_http_queue_updates_for_unknown_url_rebuild_queue_without_changing_values(),
        async_http_queue_duplicate_urls_are_all_updated_by_each_url_keyed_mutation(),
        async_http_queue_malformed_matching_item_signals_during_missing_slot_mutation(),
        async_http_queue_completion_preserves_input_order_across_mixed_terminal_states(),
        async_http_queue_completion_waits_for_pending_and_processing_items_with_progress(),
        async_http_queue_empty_state_completes_with_empty_vector_and_nil_callback_is_safe(),
        async_http_queue_check_completion_is_not_idempotent_after_terminal_state(),
    ]
}

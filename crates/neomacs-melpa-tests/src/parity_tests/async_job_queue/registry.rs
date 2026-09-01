use expect_test::expect;

use super::ParityBatchCase;

fn exact_archive_metadata_dependencies_and_source_identity_match_the_pin() -> ParityBatchCase {
    ParityBatchCase::value(
        "exact_archive_metadata_dependencies_and_source_identity_match_the_pin",
        r##"
(let* ((description
        (cadr
         (assq 'async-job-queue package-alist)))
       (directory
       (package-desc-dir description))
       (source
        (expand-file-name
         "async-job-queue.el"
         directory))
       (source-hash
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally source)
          (secure-hash
           'sha256
           (current-buffer)))))
  (list
   (package-version-join
    (package-desc-version description))
   (mapcar
    (lambda (dependency)
      (list
       (car dependency)
       (package-version-join
        (cadr dependency))))
    (package-desc-reqs description))
   source-hash
   (featurep 'async-job-queue)))
"##,
        expect![[
            r#"OK ("20230427.2122" ((async "1.4") (emacs "25.1") (queue "0.2")) "d5afa48bcdb8fffa7cf60d6d4719705af78ae58681d3e63502075d5c4137e8b7" t)"#
        ]],
    )
}

fn every_declared_queue_callable_has_the_exact_kind_and_argument_list() -> ParityBatchCase {
    ParityBatchCase::value(
        "every_declared_queue_callable_has_the_exact_kind_and_argument_list",
        r##"
(mapcar
 (lambda (symbol)
   (list
    symbol
    (if (macrop symbol)
        'macro
      (if (commandp symbol)
          'command
        'function))
    (copy-tree
     (help-function-arglist symbol t))))
 '(async-job-queue--call-with-warn
   async-job-queue--job-slot
   async-job-queue-displayable-table
   async-job-queue-displayable-slot
   async-job-queue-displayable-job
   async-job-queue--expr-to-async
   async-job-queue--slots-in-use-list
   async-job-queue--slots-free-list
   async-job-queue-make-job-queue
   async-job-queue--alloc-slot
   async-job-queue--reclaim-slot
   async-job-queue--dispatch
   async-job-queue-schedule-job
   async-job-queue--dispatch-queued
   async-job-queue--terminate-job-process
   async-job-queue--cleanup-job
   async-job-queue--handle-finished-job
   async-job-queue--handle-terminated-job
   async-job-queue-cancel-job
   async-job-queue-cancel-job-queue
   async-job-queue--timer-info
   async-job-queue--make-timer
   async-job-queue--ensure-queue-running
   async-job-queue--process-queue
   async-job-queue-add-deactivation
   async-job-queue-add-activation
   async-job-queue-deactivate-queue
   async-job-queue-activate-queue))
"##,
        expect![
            "OK ((async-job-queue--call-with-warn macro (&rest app-form)) (async-job-queue--job-slot function (job)) (async-job-queue-displayable-table function (tbl)) (async-job-queue-displayable-slot function (slot)) (async-job-queue-displayable-job function (job)) (async-job-queue--expr-to-async function (e)) (async-job-queue--slots-in-use-list function (tbl)) (async-job-queue--slots-free-list function (tbl)) (async-job-queue-make-job-queue function (freq &optional N on-empty inactive on-activate on-deactivate id)) (async-job-queue--alloc-slot function (tbl)) (async-job-queue--reclaim-slot function (s)) (async-job-queue--dispatch function (tbl &optional job)) (async-job-queue-schedule-job function (tbl prog &optional id on-dispatch on-finish max-time on-timeout on-quit)) (async-job-queue--dispatch-queued function (tbl)) (async-job-queue--terminate-job-process function (job fut)) (async-job-queue--cleanup-job function (job slot)) (async-job-queue--handle-finished-job function (slot job v)) (async-job-queue--handle-terminated-job function (slot job fut)) (async-job-queue-cancel-job function (job)) (async-job-queue-cancel-job-queue function (tbl)) (async-job-queue--timer-info function (tmr)) (async-job-queue--make-timer function (freq rpt fxn &rest args)) (async-job-queue--ensure-queue-running function (tbl)) (async-job-queue--process-queue function (tbl)) (async-job-queue-add-deactivation function (tbl callback)) (async-job-queue-add-activation function (tbl callback)) (async-job-queue-deactivate-queue function (tbl &optional key)) (async-job-queue-activate-queue function (tbl &optional key)))"
        ],
    )
}

fn table_struct_registry_covers_constructor_predicate_copy_and_every_accessor() -> ParityBatchCase {
    ParityBatchCase::value(
        "table_struct_registry_covers_constructor_predicate_copy_and_every_accessor",
        r##"
(mapcar
 (lambda (symbol)
   (list
    symbol
    (fboundp symbol)
    (copy-tree
     (help-function-arglist symbol t))))
 '(async-job-queue--table-create
   async-job-queue--table-p
   async-job-queue--table-copy
   async-job-queue--table-id
   async-job-queue--table-slots
   async-job-queue--table-active
   async-job-queue--table-in-use
   async-job-queue--table-free
   async-job-queue--table-queue
   async-job-queue--table-on-empty
   async-job-queue--table-on-activate
   async-job-queue--table-on-deactivate
   async-job-queue--table-first-used
   async-job-queue--table-last-used
   async-job-queue--table-first-free
   async-job-queue--table-last-free
   async-job-queue--table-freq
   async-job-queue--table-timer))
"##,
        expect![
            "OK ((async-job-queue--table-create t (&rest --cl-rest--)) (async-job-queue--table-p t (x)) (async-job-queue--table-copy t (arg)) (async-job-queue--table-id t (x)) (async-job-queue--table-slots t (x)) (async-job-queue--table-active t (x)) (async-job-queue--table-in-use t (x)) (async-job-queue--table-free t (x)) (async-job-queue--table-queue t (x)) (async-job-queue--table-on-empty t (x)) (async-job-queue--table-on-activate t (x)) (async-job-queue--table-on-deactivate t (x)) (async-job-queue--table-first-used t (x)) (async-job-queue--table-last-used t (x)) (async-job-queue--table-first-free t (x)) (async-job-queue--table-last-free t (x)) (async-job-queue--table-freq t (x)) (async-job-queue--table-timer t (x)))"
        ],
    )
}

fn queue_slot_and_job_struct_registries_cover_every_generated_callable() -> ParityBatchCase {
    ParityBatchCase::value(
        "queue_slot_and_job_struct_registries_cover_every_generated_callable",
        r##"
(mapcar
 (lambda (group)
   (mapcar
    (lambda (symbol)
      (list
       symbol
       (fboundp symbol)
       (copy-tree
        (help-function-arglist symbol t))))
    group))
 '((async-job-queue--queue-create
    async-job-queue--queue-p
    async-job-queue--queue-copy
    async-job-queue--queue-head
    async-job-queue--queue-last)
   (async-job-queue--slot-create
    async-job-queue--slot-p
    async-job-queue--slot-copy
    async-job-queue--slot-table
    async-job-queue--slot-index
    async-job-queue--slot-next
    async-job-queue--slot-prev
    async-job-queue--slot-job)
   (async-job-queue--job-create
    async-job-queue--job-p
    async-job-queue--job-copy
    async-job-queue--job-id
    async-job-queue--job-table
    async-job-queue--job-run-slot
    async-job-queue--job-program
    async-job-queue--job-started
    async-job-queue--job-max-time
    async-job-queue--job-future
    async-job-queue--job-ended
    async-job-queue--job-returned
    async-job-queue--job-result
    async-job-queue--job-dispatched
    async-job-queue--job-succeed
    async-job-queue--job-timeout
    async-job-queue--job-quit)))
"##,
        expect![
            "OK (((async-job-queue--queue-create t (&rest --cl-rest--)) (async-job-queue--queue-p t (x)) (async-job-queue--queue-copy t (arg)) (async-job-queue--queue-head t (x)) (async-job-queue--queue-last t (x))) ((async-job-queue--slot-create t (&rest --cl-rest--)) (async-job-queue--slot-p t (x)) (async-job-queue--slot-copy t (arg)) (async-job-queue--slot-table t (x)) (async-job-queue--slot-index t (x)) (async-job-queue--slot-next t (x)) (async-job-queue--slot-prev t (x)) (async-job-queue--slot-job t (x))) ((async-job-queue--job-create t (&rest --cl-rest--)) (async-job-queue--job-p t (x)) (async-job-queue--job-copy t (arg)) (async-job-queue--job-id t (x)) (async-job-queue--job-table t (x)) (async-job-queue--job-run-slot t (x)) (async-job-queue--job-program t (x)) (async-job-queue--job-started t (x)) (async-job-queue--job-max-time t (x)) (async-job-queue--job-future t (x)) (async-job-queue--job-ended t (x)) (async-job-queue--job-returned t (x)) (async-job-queue--job-result t (x)) (async-job-queue--job-dispatched t (x)) (async-job-queue--job-succeed t (x)) (async-job-queue--job-timeout t (x)) (async-job-queue--job-quit t (x))))"
        ],
    )
}

fn customization_conditions_group_and_generated_autoload_state_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "customization_conditions_group_and_generated_autoload_state_are_exact",
        r##"
(let* ((standard-values
        (get
         'async-job-queue-default-size
         'standard-value))
       (standard-form
        (car standard-values))
       (standard-value
        (eval standard-form t)))
  (list
   (integerp async-job-queue-default-size)
   (> async-job-queue-default-size 0)
   (get 'async-job-queue-default-size 'custom-type)
   (and
    (= (length standard-values) 1)
    (consp standard-form)
    (eq (car standard-form) 'funcall)
    t)
   (integerp standard-value)
   (> standard-value 0)
   (= standard-value
      async-job-queue-default-size)
   (get 'async-job-queue 'group-documentation)
   (get 'async-job-queue-slot-already-free 'error-conditions)
   (get 'async-job-queue-slot-already-free 'error-message)
   (get 'async-job-queue--table-no-free-slot 'error-conditions)
   (get 'async-job-queue--table-no-free-slot 'error-message)
   async-job-queue--num-tables-created))
"##,
        expect![[
            r#"OK (t t natnum t t t t "Customization group for async-job-queue package." (async-job-queue-slot-already-free error) "Slot in job queue is already free" (async-job-queue--table-no-free-slot error) "No free slots in job queue" 0)"#
        ]],
    )
    .fresh_process()
}

fn generated_autoload_file_registers_prefix_without_eagerly_loading_runtime() -> ParityBatchCase {
    ParityBatchCase::value(
        "generated_autoload_file_registers_prefix_without_eagerly_loading_runtime",
        r##"
(let* ((source (getenv "NEOMACS_PACKAGE_SOURCE"))
       ;; Mask the installed package's own directory.  Spelling it out
       ;; pinned the harness's acquisition layout, so this expectation
       ;; broke when the cache moved from package-cache/ to the
       ;; revision-pinned source-install-cache/ -- a harness change
       ;; wearing the shape of a package regression.  What the assertion
       ;; is about is that the autoload file, and only it, is on
       ;; `load-history'.
       (directory
        (directory-file-name
         (file-name-directory source))))
  (list
   (featurep 'async-job-queue-autoloads)
   (featurep 'async-job-queue)
   (fboundp 'async-job-queue-make-job-queue)
   (fboundp 'async-job-queue-schedule-job)
   (mapcar
    (lambda (value)
      (if (stringp value)
          (replace-regexp-in-string
           (regexp-quote directory)
           "[PACKAGE]"
           value t t)
        value))
    (assoc source load-history))))
"##,
        expect![[
            r#"OK (t nil nil nil ("[PACKAGE]/async-job-queue-autoloads.el" (provide . async-job-queue-autoloads)))"#
        ]],
    )
}

pub(super) fn registry_async_job_queue_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        exact_archive_metadata_dependencies_and_source_identity_match_the_pin(),
        every_declared_queue_callable_has_the_exact_kind_and_argument_list(),
        table_struct_registry_covers_constructor_predicate_copy_and_every_accessor(),
        queue_slot_and_job_struct_registries_cover_every_generated_callable(),
        customization_conditions_group_and_generated_autoload_state_are_exact(),
    ]
}

pub(super) fn registry_async_job_queue_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![generated_autoload_file_registers_prefix_without_eagerly_loading_runtime()]
}

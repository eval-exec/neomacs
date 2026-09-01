use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DEFERRED_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const DEFERRED_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn deferred_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DEFERRED_MELPA_PIN, "deferred.el")
        .expect("prepare pinned Deferred source below ./tmp")
        .with_prelude(r##"(setq deferred:tick-time 0.001)"##)
        .with_timeout(DEFERRED_TEST_TIMEOUT)
}

fn order_pipeline_carries_values_and_audit_events_between_stages() -> ParityBatchCase {
    ParityBatchCase::value(
        "order_pipeline_carries_values_and_audit_events_between_stages",
        r##"
(let (events)
  (deferred:clear-queue)
  (let ((result
         (deferred:sync!
          (deferred:$
            (deferred:succeed '(:sku "A-17" :quantity 3 :unit-price 1250))
            (deferred:nextc
             it
             (lambda (order)
               (push 'priced events)
               (plist-put order :subtotal
                          (* (plist-get order :quantity)
                             (plist-get order :unit-price)))))
            (deferred:nextc
             it
             (lambda (order)
               (push 'labelled events)
               (plist-put order :label
                          (format "%s × %d = %d"
                                  (plist-get order :sku)
                                  (plist-get order :quantity)
                                  (plist-get order :subtotal)))))))))
    (list :result result :events (nreverse events))))
"##,
        expect![[
            r##"OK (:result (:sku "A-17" :quantity 3 :unit-price 1250 :subtotal 3750 :label "A-17 × 3 = 3750") :events (priced labelled))"##
        ]],
    )
}

fn rejected_invoice_is_recovered_and_cleanup_still_runs() -> ParityBatchCase {
    ParityBatchCase::value(
        "rejected_invoice_is_recovered_and_cleanup_still_runs",
        r##"
(let (events)
  (deferred:clear-queue)
  (let ((result
         (deferred:sync!
          (deferred:try
            (deferred:$
              (deferred:succeed '(:invoice "INV-9" :total -20))
              (deferred:nextc
               it
               (lambda (invoice)
                 (push 'validating events)
                 (when (<= (plist-get invoice :total) 0)
                   (error "invoice total %d must be positive"
                          (plist-get invoice :total)))
                 invoice)))
            :catch
            (lambda (err)
              (push 'caught events)
              (list :status 'rejected :reason (error-message-string err)))
            :finally
            (lambda (_outcome)
              (push 'cleanup events))))))
    (list :result result :events (nreverse events))))
"##,
        expect![[
            r##"OK (:result (:status rejected :reason "invoice total -20 must be positive") :events (validating caught cleanup))"##
        ]],
    )
}

fn ledger_rows_are_applied_sequentially_with_running_balances() -> ParityBatchCase {
    ParityBatchCase::value(
        "ledger_rows_are_applied_sequentially_with_running_balances",
        r##"
(let ((rows '((:id "opening" :delta 1000)
              (:id "invoice" :delta 3750)
              (:id "refund" :delta -500)
              (:id "fee" :delta -75)))
      (balance 0)
      journal)
  (deferred:clear-queue)
  (deferred:sync!
   (deferred:$
     (deferred:loop
      rows
      (lambda (row)
        (setq balance (+ balance (plist-get row :delta)))
        (push (list (plist-get row :id)
                    (plist-get row :delta)
                    balance)
              journal)
        (deferred:succeed balance)))
     (deferred:nextc
      it
      (lambda (last-balance)
        (list :last-balance last-balance
              :journal (nreverse journal)))))))
"##,
        expect![[
            r##"OK (:last-balance 4175 :journal (("opening" 1000 1000) ("invoice" 3750 4750) ("refund" -500 4250) ("fee" -75 4175)))"##
        ]],
    )
}

fn parallel_report_keeps_request_order_when_tasks_finish_out_of_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "parallel_report_keeps_request_order_when_tasks_finish_out_of_order",
        r##"
(progn
  (deferred:clear-queue)
  (deferred:sync!
   (deferred:parallel
    (deferred:nextc (deferred:wait 15)
                    (lambda (_elapsed) '(inventory . 7)))
    (deferred:nextc (deferred:wait 1)
                    (lambda (_elapsed) '(price . 1250)))
    (deferred:call (lambda () '(shipping . "ready"))))))
"##,
        expect![[r##"OK ((inventory . 7) (price . 1250) (shipping . "ready"))"##]],
    )
}

fn subprocess_jobs_parse_success_and_recover_structured_failure() -> ParityBatchCase {
    ParityBatchCase::value(
        "subprocess_jobs_parse_success_and_recover_structured_failure",
        r##"
(progn
  (deferred:clear-queue)
  (let ((success
         (deferred:sync!
          (deferred:$
            (deferred:process
             "sh" "-c" "printf 'A-17\t3\nB-04\t5\n'")
            (deferred:nextc
             it
             (lambda (output)
               (let ((rows
                      (mapcar
                       (lambda (line)
                         (let ((fields (split-string line "\t")))
                           (cons (car fields)
                                 (string-to-number (cadr fields)))))
                       (split-string output "\n" t))))
                 (list :rows rows
                       :units (apply #'+ (mapcar #'cdr rows)))))))))
        (failure
         (deferred:sync!
          (deferred:$
            (deferred:process
             "sh" "-c" "printf 'bad inventory row\n' >&2; exit 7")
            (deferred:error
             it
             (lambda (err)
               (let* ((message (error-message-string err))
                      (exit
                       (and (string-match
                             "exit status: [^ ]+ \\([0-9]+\\)" message)
                            (string-to-number (match-string 1 message))))
                     (stderr
                      (if (string-match-p "bad inventory row" message)
                          "bad inventory row"
                        message)))
                 (list :status 'failed :exit exit :stderr stderr))))))))
    (list :success success :failure failure)))
"##,
        expect![[
            r##"OK (:success (:rows (("A-17" . 3) ("B-04" . 5)) :units 8) :failure (:status failed :exit 7 :stderr "bad inventory row"))"##
        ]],
    )
}

#[test]
fn deferred_package_batch() {
    let cases = vec![
        order_pipeline_carries_values_and_audit_events_between_stages(),
        rejected_invoice_is_recovered_and_cleanup_still_runs(),
        ledger_rows_are_applied_sequentially_with_running_balances(),
        parallel_report_keeps_request_order_when_tasks_finish_out_of_order(),
        subprocess_jobs_parse_success_and_recover_structured_failure(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Deferred parity test");
    assert_oracle_batch_cases(deferred_oracle(), test_name, "deferred_parity", &cases);
}

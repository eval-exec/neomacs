use expect_test::expect;

use super::ParityBatchCase;

fn wire_framing_encodes_lengths_and_preserves_unicode_prin1() -> ParityBatchCase {
    ParityBatchCase::value(
        "wire_framing_encodes_lengths_and_preserves_unicode_prin1",
        r####"
(list :zero (epc:net-encode-length 0)
      :byte (epc:net-encode-length 255)
      :large (epc:net-encode-length 65536)
      :prin1 (epc:prin1-to-string '("café" 日本語 (a . 1)))
      :decode
      (with-temp-buffer
        (insert "00000a")
        (goto-char (point-min))
        (epc:net-decode-length)))
"####,
        expect![[
            r#"OK (:zero "000000" :byte "0000ff" :large "010000" :prin1 "(\"café\" 日本語 (a . 1))" :decode 10)"#
        ]],
    )
}

fn loopback_echo_and_add_methods_round_trip_synchronously() -> ParityBatchCase {
    ParityBatchCase::value(
        "loopback_echo_and_add_methods_round_trip_synchronously",
        r####"
(neomacs-epc-test-with-loopback
 (lambda (mngr)
   (epc:define-method mngr 'echo (lambda (&rest xs) xs) "XS" "Echo arguments")
   (epc:define-method mngr 'add (lambda (&rest xs) (apply #'+ xs)) "XS.." "Sum"))
 (lambda (client _server)
   (list :echo (neomacs-epc-test-call client 'echo '("release" 41))
         :add (neomacs-epc-test-call client 'add '(2 3 5))
         :live (and (epc:live-p client) t)
         :port (numberp (epc:manager-port client)))))
"####,
        expect![[r#"OK (:echo (:ok ("release" 41)) :add (:ok 10) :live t :port t)"#]],
    )
}

fn method_discovery_returns_registered_specs_and_docstrings() -> ParityBatchCase {
    ParityBatchCase::value(
        "method_discovery_returns_registered_specs_and_docstrings",
        r####"
(neomacs-epc-test-with-loopback
 (lambda (mngr)
   (epc:define-method mngr 'echo (lambda (x) x) "X" "Return X")
   (epc:define-method mngr 'add (lambda (&rest xs) (apply #'+ xs)) "XS.." "Sum XS"))
 (lambda (client _server)
   (let ((methods
          (sort (copy-sequence
                 (neomacs-epc-test-sync
                  client (epc:query-methods-deferred client)))
                (lambda (a b)
                  (string< (format "%s" (car a)) (format "%s" (car b)))))))
     (list :methods methods
           :live (and (epc:live-p client) t)))))
"####,
        expect![[r#"OK (:methods ((add "XS.." "Sum XS") (echo "X" "Return X")) :live t)"#]],
    )
}

fn application_and_missing_method_errors_propagate_to_the_client() -> ParityBatchCase {
    ParityBatchCase::value(
        "application_and_missing_method_errors_propagate_to_the_client",
        r####"
(neomacs-epc-test-with-loopback
 (lambda (mngr)
   (epc:define-method mngr 'boom (lambda (_x) (/ 1 0))))
 (lambda (client _server)
   (list :app (neomacs-epc-test-call client 'boom '(0))
         :missing (neomacs-epc-test-call client 'no-such-method '(1)))))
"####,
        expect![[
            r#"OK (:app (:error "(error (arith-error))") :missing (:error "(epc-error EPC-ERROR: No such method : no-such-method)"))"#
        ]],
    )
}

fn unicode_payloads_round_trip_through_the_binary_connection() -> ParityBatchCase {
    ParityBatchCase::value(
        "unicode_payloads_round_trip_through_the_binary_connection",
        r####"
(let ((payload "日本語能力!!ソﾊﾝｶｸ café"))
  (neomacs-epc-test-with-loopback
   (lambda (mngr)
     (epc:define-method
      mngr 'echo
      (lambda (x)
        (if (equal x payload)
            payload
          (error "Different content: %S" x)))))
   (lambda (client _server)
     (neomacs-epc-test-call client 'echo (list payload)))))
"####,
        expect![[r#"OK (:ok "日本語能力!!ソﾊﾝｶｸ café")"#]],
    )
}

fn stop_disconnects_the_client_and_runs_exit_hooks() -> ParityBatchCase {
    ParityBatchCase::value(
        "stop_disconnects_the_client_and_runs_exit_hooks",
        r####"
(let (events)
  (neomacs-epc-test-with-loopback
   (lambda (mngr)
     (epc:define-method mngr 'echo (lambda (x) x)))
   (lambda (client server)
     (epc:manager-add-exit-hook
      client
      (lambda () (push 'client-exit events)))
     (let* ((before (and (epc:live-p client) t))
            (echo (neomacs-epc-test-call client 'echo '("ok")))
            (stopped (progn (epc:stop-epc client) t))
            (after (and (epc:live-p client) t)))
       (list :before before
             :echo echo
             :stopped stopped
             :after after
             :events (nreverse events)
             :server-alive (and (process-live-p server) t))))))
"####,
        expect![[
            r#"OK (:before t :echo (:ok "ok") :stopped t :after nil :events nil :server-alive t)"#
        ]],
    )
}

fn server_stop_rejects_unknown_process_objects() -> ParityBatchCase {
    ParityBatchCase::value(
        "server_stop_rejects_unknown_process_objects",
        r####"
(condition-case err
    (list :value (epcs:server-stop 'not-a-server-process))
  (error (list :signal (car err)
               :message (error-message-string err))))
"####,
        expect![[
            r#"OK (:signal error :message "Not found in the server process list. [not-a-server-process]")"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        wire_framing_encodes_lengths_and_preserves_unicode_prin1(),
        loopback_echo_and_add_methods_round_trip_synchronously(),
        method_discovery_returns_registered_specs_and_docstrings(),
        application_and_missing_method_errors_propagate_to_the_client(),
        unicode_payloads_round_trip_through_the_binary_connection(),
        stop_disconnects_the_client_and_runs_exit_hooks(),
        server_stop_rejects_unknown_process_objects(),
    ]
}

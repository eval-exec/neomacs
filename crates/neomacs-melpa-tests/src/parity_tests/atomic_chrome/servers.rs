use expect_test::expect;

use super::ParityBatchCase;

fn atomic_chrome_start_websocket_server_forwards_exact_local_callbacks_and_port() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atomic_chrome_start_websocket_server_forwards_exact_local_callbacks_and_port",
        r##"(let (calls)
          (cl-letf
              (((symbol-function 'websocket-server)
                (lambda (&rest arguments)
                  (push arguments calls)
                  :server-handle)))
            (list
             (atomic-chrome-start-websocket-server
              64292)
             (atomic-chrome-start-websocket-server
              4001)
             (nreverse calls))))"##,
        expect![
            "OK (:server-handle :server-handle ((64292 :host local :on-message atomic-chrome-on-message :on-open nil :on-close atomic-chrome-on-close) (4001 :host local :on-message atomic-chrome-on-message :on-open nil :on-close atomic-chrome-on-close)))"
        ],
    )
}

fn atomic_chrome_start_httpd_forwards_custom_port_and_exact_network_process_contract()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_start_httpd_forwards_custom_port_and_exact_network_process_contract",
        r##"(let ((atomic-chrome-server-ghost-text-port
                4777)
               calls)
          (cl-letf
              (((symbol-function 'make-network-process)
                (lambda (&rest arguments)
                  (push arguments calls)
                  :httpd-process)))
            (list
             (atomic-chrome-start-httpd)
             (nreverse calls)
             (commandp
              'atomic-chrome-start-httpd)
             (interactive-form
              'atomic-chrome-start-httpd))))"##,
        expect![[
            r#"OK (:httpd-process ((:name "atomic-chrome-httpd" :family ipv4 :host local :service 4777 :filter atomic-chrome-httpd-process-filter :filter-multibyte nil :server t :noquery t)) t (interactive nil))"#
        ]],
    )
}

fn atomic_chrome_start_server_obeys_extension_selection_existing_state_and_process_status()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_start_server_obeys_extension_selection_existing_state_and_process_status",
        r##"(let (snapshots)
          (dolist
              (scenario
               '((:both
                  (atomic-chrome ghost-text)
                  nil
                  nil)
                 (:atomic-existing
                  (atomic-chrome)
                  :existing-atomic
                  nil)
                 (:ghost-running
                  (ghost-text)
                  nil
                  run)
                 (:none
                  nil
                  nil
                  nil)))
            (let ((name
                   (nth 0 scenario))
                  (atomic-chrome-extension-type-list
                   (nth 1 scenario))
                  (atomic-chrome-server-atomic-chrome
                   (nth 2 scenario))
                  (atomic-chrome-server-ghost-text
                   nil)
                  (status
                   (nth 3 scenario))
                  events)
              (cl-letf
                  (((symbol-function
                     'atomic-chrome-start-websocket-server)
                    (lambda (port)
                      (push
                       (list 'websocket port)
                       events)
                      :started-atomic))
                   ((symbol-function 'process-status)
                    (lambda (process)
                      (push
                       (list 'status process)
                       events)
                      status))
                   ((symbol-function
                     'atomic-chrome-start-httpd)
                    (lambda ()
                      (push
                       '(httpd)
                       events)
                      :started-httpd))
                   ((symbol-function
                     'global-atomic-chrome-edit-mode)
                    (lambda (argument)
                      (push
                       (list 'global argument)
                       events)
                      :global-enabled)))
                (push
                 (list
                  name
                  (atomic-chrome-start-server)
                  atomic-chrome-server-atomic-chrome
                  (nreverse events))
                 snapshots))))
          (nreverse snapshots))"##,
        expect![[
            r#"OK ((:both :global-enabled :started-atomic ((websocket 64292) (status "atomic-chrome-httpd") (httpd) (global 1))) (:atomic-existing :global-enabled :existing-atomic ((status "atomic-chrome-httpd") (global 1))) (:ghost-running :global-enabled nil ((status "atomic-chrome-httpd") (global 1))) (:none :global-enabled nil ((status "atomic-chrome-httpd") (global 1))))"#
        ]],
    )
}

fn atomic_chrome_start_server_swallows_failures_and_stops_at_exact_failed_stage() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atomic_chrome_start_server_swallows_failures_and_stops_at_exact_failed_stage",
        r##"(let (snapshots)
          (dolist
              (failure
               '(:websocket
                 :status
                 :httpd
                 :global
                 :none))
            (let ((atomic-chrome-extension-type-list
                   '(atomic-chrome ghost-text))
                  (atomic-chrome-server-atomic-chrome
                   nil)
                  events)
              (cl-letf
                  (((symbol-function
                     'atomic-chrome-start-websocket-server)
                    (lambda (port)
                      (push
                       (list 'websocket port)
                       events)
                      (if
                          (eq failure :websocket)
                          (error "websocket failed")
                        :atomic-server)))
                   ((symbol-function 'process-status)
                    (lambda (process)
                      (push
                       (list 'status process)
                       events)
                      (if
                          (eq failure :status)
                          (error "status failed")
                        nil)))
                   ((symbol-function
                     'atomic-chrome-start-httpd)
                    (lambda ()
                      (push '(httpd) events)
                      (if
                          (eq failure :httpd)
                          (error "httpd failed")
                        :httpd)))
                   ((symbol-function
                     'global-atomic-chrome-edit-mode)
                    (lambda (argument)
                      (push
                       (list 'global argument)
                       events)
                      (if
                          (eq failure :global)
                          (error "global failed")
                        :enabled))))
                (push
                 (list
                  failure
                  (atomic-chrome-start-server)
                  atomic-chrome-server-atomic-chrome
                  (nreverse events))
                 snapshots))))
          (nreverse snapshots))"##,
        expect![[
            r#"OK ((:websocket nil nil ((websocket 64292))) (:status nil :atomic-server ((websocket 64292) (status "atomic-chrome-httpd"))) (:httpd nil :atomic-server ((websocket 64292) (status "atomic-chrome-httpd") #1=(httpd))) (:global nil :atomic-server ((websocket 64292) (status "atomic-chrome-httpd") #1# (global 1))) (:none :enabled :atomic-server ((websocket 64292) (status "atomic-chrome-httpd") #1# (global 1))))"#
        ]],
    )
}

fn atomic_chrome_stop_server_closes_both_websockets_httpd_and_global_mode_in_order()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_stop_server_closes_both_websockets_httpd_and_global_mode_in_order",
        r##"(let ((atomic-chrome-server-atomic-chrome
                :atomic-server)
               (atomic-chrome-server-ghost-text
                :ghost-server)
               events)
          (cl-letf
              (((symbol-function
                 'websocket-server-close)
                (lambda (server)
                  (push
                   (list 'close server)
                   events)
                  :closed))
               ((symbol-function 'process-status)
                (lambda (process)
                  (push
                   (list 'status process)
                   events)
                  'listen))
               ((symbol-function 'delete-process)
                (lambda (process)
                  (push
                   (list 'delete process)
                   events)
                  :deleted))
               ((symbol-function
                 'global-atomic-chrome-edit-mode)
                (lambda (argument)
                  (push
                   (list 'global argument)
                   events)
                  :disabled)))
            (list
             (atomic-chrome-stop-server)
             atomic-chrome-server-atomic-chrome
             atomic-chrome-server-ghost-text
             (nreverse events))))"##,
        expect![[
            r#"OK (:disabled nil nil ((close :atomic-server) (close :ghost-server) (status "atomic-chrome-httpd") (delete "atomic-chrome-httpd") (global 0)))"#
        ]],
    )
}

fn atomic_chrome_stop_server_skips_absent_resources_but_always_disables_global_mode()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_stop_server_skips_absent_resources_but_always_disables_global_mode",
        r##"(let ((atomic-chrome-server-atomic-chrome
                nil)
               (atomic-chrome-server-ghost-text
                nil)
               events)
          (cl-letf
              (((symbol-function
                 'websocket-server-close)
                (lambda (server)
                  (push
                   (list 'unexpected-close server)
                   events)))
               ((symbol-function 'process-status)
                (lambda (process)
                  (push
                   (list 'status process)
                   events)
                  nil))
               ((symbol-function 'delete-process)
                (lambda (process)
                  (push
                   (list 'unexpected-delete process)
                   events)))
               ((symbol-function
                 'global-atomic-chrome-edit-mode)
                (lambda (argument)
                  (push
                   (list 'global argument)
                   events)
                  :disabled)))
            (list
             (atomic-chrome-stop-server)
             (nreverse events))))"##,
        expect![[r#"OK (:disabled ((status "atomic-chrome-httpd") (global 0)))"#]],
    )
}

fn atomic_chrome_stop_server_propagates_failures_with_exact_partial_cleanup_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_stop_server_propagates_failures_with_exact_partial_cleanup_state",
        r##"(let (snapshots)
          (dolist
              (failure
               '(:atomic-close
                 :ghost-close
                 :status
                 :delete
                 :global))
            (let ((atomic-chrome-server-atomic-chrome
                   :atomic-server)
                  (atomic-chrome-server-ghost-text
                   :ghost-server)
                  events)
              (cl-letf
                  (((symbol-function
                     'websocket-server-close)
                    (lambda (server)
                      (push
                       (list 'close server)
                       events)
                      (when
                          (or
                           (and
                            (eq failure :atomic-close)
                            (eq server :atomic-server))
                           (and
                            (eq failure :ghost-close)
                            (eq server :ghost-server)))
                        (error
                         "close failed %S"
                         server))
                      :closed))
                   ((symbol-function 'process-status)
                    (lambda (process)
                      (push
                       (list 'status process)
                       events)
                      (if
                          (eq failure :status)
                          (error "status failed")
                        'listen)))
                   ((symbol-function 'delete-process)
                    (lambda (process)
                      (push
                       (list 'delete process)
                       events)
                      (if
                          (eq failure :delete)
                          (error "delete failed")
                        :deleted)))
                   ((symbol-function
                     'global-atomic-chrome-edit-mode)
                    (lambda (argument)
                      (push
                       (list 'global argument)
                       events)
                      (if
                          (eq failure :global)
                          (error "global failed")
                        :disabled))))
                (push
                 (list
                  failure
                  (atomic-chrome-test-error-data
                   #'atomic-chrome-stop-server)
                  atomic-chrome-server-atomic-chrome
                  atomic-chrome-server-ghost-text
                  (nreverse events))
                 snapshots))))
          (nreverse snapshots))"##,
        expect![[
            r#"OK ((:atomic-close (:error error ("close failed :atomic-server")) :atomic-server :ghost-server ((close :atomic-server))) (:ghost-close (:error error ("close failed :ghost-server")) nil :ghost-server ((close :atomic-server) (close :ghost-server))) (:status (:error error ("status failed")) nil nil ((close :atomic-server) (close :ghost-server) (status "atomic-chrome-httpd"))) (:delete (:error error ("delete failed")) nil nil ((close :atomic-server) (close :ghost-server) (status "atomic-chrome-httpd") (delete "atomic-chrome-httpd"))) (:global (:error error ("global failed")) nil nil ((close :atomic-server) (close :ghost-server) (status "atomic-chrome-httpd") (delete "atomic-chrome-httpd") (global 0))))"#
        ]],
    )
}

pub(super) fn servers_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atomic_chrome_start_websocket_server_forwards_exact_local_callbacks_and_port(),
        atomic_chrome_start_httpd_forwards_custom_port_and_exact_network_process_contract(),
        atomic_chrome_start_server_obeys_extension_selection_existing_state_and_process_status(),
        atomic_chrome_start_server_swallows_failures_and_stops_at_exact_failed_stage(),
        atomic_chrome_stop_server_closes_both_websockets_httpd_and_global_mode_in_order(),
        atomic_chrome_stop_server_skips_absent_resources_but_always_disables_global_mode(),
        atomic_chrome_stop_server_propagates_failures_with_exact_partial_cleanup_state(),
    ]
}

use expect_test::expect;

use super::ParityBatchCase;

fn atomic_chrome_buffer_table_lookup_round_trips_live_buffers_sockets_and_frames() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atomic_chrome_buffer_table_lookup_round_trips_live_buffers_sockets_and_frames",
        r##"(let ((first
                (generate-new-buffer
                 " *atomic-table-first*"))
               (second
                (generate-new-buffer
                 " *atomic-table-second*"))
               (atomic-chrome-buffer-table
                (make-hash-table
                 :test 'equal)))
          (unwind-protect
              (progn
                (puthash
                 first
                 '((socket 1) frame-a)
                 atomic-chrome-buffer-table)
                (puthash
                 second
                 '((socket 2) frame-b)
                 atomic-chrome-buffer-table)
                (list
                 (atomic-chrome-get-websocket first)
                 (atomic-chrome-get-frame first)
                 (buffer-name
                  (atomic-chrome-get-buffer-by-socket
                   '(socket 2)))
                 (buffer-name
                  (atomic-chrome-get-buffer-by-socket
                   '(socket 1)))
                 (hash-table-count
                  atomic-chrome-buffer-table)
                 (atomic-chrome-test-buffer-table-snapshot)))
            (atomic-chrome-test-kill-buffer first)
            (atomic-chrome-test-kill-buffer second)))"##,
        expect![[
            r#"OK (#1=(socket 1) frame-a " *atomic-table-second*" " *atomic-table-first*" 2 ((" *atomic-table-first*" #1# frame-a) (" *atomic-table-second*" (socket 2) frame-b)))"#
        ]],
    )
}

fn atomic_chrome_buffer_table_lookup_handles_missing_short_and_non_list_entries_exactly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_buffer_table_lookup_handles_missing_short_and_non_list_entries_exactly",
        r##"(let ((atomic-chrome-buffer-table
                (make-hash-table
                 :test 'equal)))
          (puthash
           'short
           '(:socket-only)
           atomic-chrome-buffer-table)
          (puthash
           'empty
           nil
           atomic-chrome-buffer-table)
          (puthash
           'invalid
           42
           atomic-chrome-buffer-table)
          (list
           (atomic-chrome-get-websocket 'missing)
           (atomic-chrome-get-frame 'missing)
           (atomic-chrome-get-websocket 'short)
           (atomic-chrome-get-frame 'short)
           (atomic-chrome-get-websocket 'empty)
           (atomic-chrome-get-frame 'empty)
           (atomic-chrome-test-error-data
            (lambda ()
              (atomic-chrome-get-websocket
               'invalid)))
           (atomic-chrome-test-error-data
            (lambda ()
              (atomic-chrome-get-frame
               'invalid)))
           (atomic-chrome-test-error-data
            (lambda ()
              (atomic-chrome-get-buffer-by-socket
               :absent)))
           (hash-table-count
            atomic-chrome-buffer-table)))"##,
        expect![
            "OK (nil nil :socket-only nil nil nil (:error wrong-type-argument (listp 42)) (:error wrong-type-argument (listp 42)) (:error wrong-type-argument (listp 42)) 3)"
        ],
    )
}

fn atomic_chrome_socket_lookup_uses_equal_and_returns_the_last_matching_hash_iteration_entry()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_socket_lookup_uses_equal_and_returns_the_last_matching_hash_iteration_entry",
        r##"(let ((atomic-chrome-buffer-table
                (make-hash-table
                 :test 'equal)))
          (puthash
           "alpha"
           '((shared socket) frame-a)
           atomic-chrome-buffer-table)
          (puthash
           "beta"
           '((shared socket) frame-b)
           atomic-chrome-buffer-table)
          (puthash
           "gamma"
           '((other socket) frame-c)
           atomic-chrome-buffer-table)
          (let ((iteration-order nil))
            (maphash
             (lambda (key _value)
               (push key iteration-order))
             atomic-chrome-buffer-table)
            (list
             (nreverse iteration-order)
             (atomic-chrome-get-buffer-by-socket
              (list 'shared 'socket))
             (atomic-chrome-get-buffer-by-socket
              (list 'other 'socket)))))"##,
        expect![[r#"OK (("alpha" "beta" "gamma") "beta" "gamma")"#]],
    )
}

fn atomic_chrome_close_connection_removes_current_buffer_before_closing_socket() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atomic_chrome_close_connection_removes_current_buffer_before_closing_socket",
        r##"(let ((atomic-chrome-buffer-table
                (make-hash-table
                 :test 'equal))
               events)
          (with-temp-buffer
            (puthash
             (current-buffer)
             '(:socket-a :frame-a)
             atomic-chrome-buffer-table)
            (cl-letf
                (((symbol-function 'websocket-close)
                  (lambda (socket)
                    (push
                     (list
                      socket
                      (gethash
                       (current-buffer)
                       atomic-chrome-buffer-table)
                      (hash-table-count
                       atomic-chrome-buffer-table))
                     events)
                    :closed)))
              (list
               (atomic-chrome-close-connection)
               (nreverse events)
               (gethash
                (current-buffer)
                atomic-chrome-buffer-table)
               (hash-table-count
                atomic-chrome-buffer-table)))))"##,
        expect!["OK (:closed ((:socket-a nil 0)) nil 0)"],
    )
}

fn atomic_chrome_close_connection_no_socket_is_noop_and_close_errors_leave_entry_removed()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_close_connection_no_socket_is_noop_and_close_errors_leave_entry_removed",
        r##"(let ((atomic-chrome-buffer-table
                (make-hash-table
                 :test 'equal))
               close-calls)
          (with-temp-buffer
            (let ((buffer
                   (current-buffer)))
              (cl-letf
                  (((symbol-function 'websocket-close)
                    (lambda (socket)
                      (push socket close-calls)
                      (error
                       "close failed %S"
                       socket))))
                (let ((missing-result
                       (atomic-chrome-close-connection)))
                  (puthash
                   buffer
                   '(:socket-failing nil)
                   atomic-chrome-buffer-table)
                  (list
                   missing-result
                   (atomic-chrome-test-error-data
                    #'atomic-chrome-close-connection)
                   (nreverse close-calls)
                   (gethash
                    buffer
                    atomic-chrome-buffer-table)
                   (hash-table-count
                    atomic-chrome-buffer-table)))))))"##,
        expect![[
            r#"OK (nil (:error error ("close failed :socket-failing")) (:socket-failing) nil 0)"#
        ]],
    )
}

pub(super) fn tables_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atomic_chrome_buffer_table_lookup_round_trips_live_buffers_sockets_and_frames(),
        atomic_chrome_buffer_table_lookup_handles_missing_short_and_non_list_entries_exactly(),
        atomic_chrome_socket_lookup_uses_equal_and_returns_the_last_matching_hash_iteration_entry(),
        atomic_chrome_close_connection_removes_current_buffer_before_closing_socket(),
        atomic_chrome_close_connection_no_socket_is_noop_and_close_errors_leave_entry_removed(),
    ]
}

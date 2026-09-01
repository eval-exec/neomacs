use expect_test::expect;

use super::ParityBatchCase;

fn atomic_chrome_send_buffer_text_emits_exact_atomic_chrome_update_json_and_clears_modified_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_send_buffer_text_emits_exact_atomic_chrome_update_json_and_clears_modified_state",
        r##"(let ((atomic-chrome-buffer-table
                (make-hash-table
                 :test 'equal))
               (atomic-chrome-server-ghost-text
                :ghost-server)
               (socket
                (atomic-chrome-test-socket
                 'atomic-socket
                 :atomic-server))
               events)
          (with-temp-buffer
            (insert
             "Hello \"browser\"\n"
             "λ and emoji 😀")
            (add-text-properties
             (point-min)
             (+ (point-min) 5)
             '(face bold invisible nil))
            (puthash
             (current-buffer)
             (list socket nil)
             atomic-chrome-buffer-table)
            (set-buffer-modified-p t)
            (cl-letf
                (((symbol-function
                   'websocket-send-text)
                  (lambda (target text)
                    (push
                     (list
                      'send
                      (atomic-chrome-test-socket-name
                       target)
                      text)
                     events)
                    :sent)))
              (list
               (atomic-chrome-send-buffer-text)
               (nreverse events)
               (buffer-string)
               (buffer-modified-p)
               (text-properties-at
                (point-min))))))"##,
        expect![[
            r#"OK (nil ((send atomic-socket "{\"type\":\"updateText\",\"payload\":{\"text\":\"Hello \\\"browser\\\"\\nλ and emoji 😀\"}}")) #("Hello \"browser\"\nλ and emoji 😀" 0 5 (invisible nil face bold)) nil (invisible nil face bold))"#
        ]],
    )
}

fn atomic_chrome_send_buffer_text_emits_exact_ghost_text_json_shape() -> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_send_buffer_text_emits_exact_ghost_text_json_shape",
        r##"(let ((atomic-chrome-buffer-table
                (make-hash-table
                 :test 'equal))
               (atomic-chrome-server-ghost-text
                :ghost-server)
               (socket
                (atomic-chrome-test-socket
                 'ghost-socket
                 :ghost-server))
               events)
          (with-temp-buffer
            (insert
             "line one\nline two")
            (puthash
             (current-buffer)
             (list socket nil)
             atomic-chrome-buffer-table)
            (set-buffer-modified-p t)
            (cl-letf
                (((symbol-function
                   'websocket-send-text)
                  (lambda (target text)
                    (push
                     (list
                      'send
                      (atomic-chrome-test-socket-name
                       target)
                      text)
                     events)
                    :sent)))
              (list
               (atomic-chrome-send-buffer-text)
               (nreverse events)
               (buffer-modified-p)))))"##,
        expect![[r#"OK (nil ((send ghost-socket "{\"text\":\"line one\\nline two\"}")) nil)"#]],
    )
}

fn atomic_chrome_send_buffer_text_uses_accessible_narrowed_plain_text() -> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_send_buffer_text_uses_accessible_narrowed_plain_text",
        r##"(let ((atomic-chrome-buffer-table
                (make-hash-table
                 :test 'equal))
               (socket
                (atomic-chrome-test-socket
                 'narrowed-socket
                 :atomic-server))
               sent)
          (with-temp-buffer
            (insert
             "prefix|editable|suffix")
            (goto-char
             (point-min))
            (search-forward "|")
            (let ((start
                   (point)))
              (search-forward "|")
              (narrow-to-region
               start
               (1- (point))))
            (add-text-properties
             (point-min)
             (point-max)
             '(face italic category test))
            (puthash
             (current-buffer)
             (list socket nil)
             atomic-chrome-buffer-table)
            (cl-letf
                (((symbol-function
                   'websocket-send-text)
                  (lambda (target text)
                    (setq sent
                          (list
                           (atomic-chrome-test-socket-name
                            target)
                           text))
                    :sent)))
              (list
               (atomic-chrome-send-buffer-text)
               sent
               (buffer-substring
                (point-min)
                (point-max))
               (buffer-substring-no-properties
                (point-min)
                (point-max))
               (point-min)
               (point-max)))))"##,
        expect![[
            r#"OK (nil (narrowed-socket "{\"type\":\"updateText\",\"payload\":{\"text\":\"editable\"}}") #("editable" 0 8 (category test face italic)) "editable" 8 16)"#
        ]],
    )
}

fn atomic_chrome_send_buffer_text_sends_empty_content_but_without_socket_only_clears_modified()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_send_buffer_text_sends_empty_content_but_without_socket_only_clears_modified",
        r##"(let ((atomic-chrome-buffer-table
                (make-hash-table
                 :test 'equal))
               (socket
                (atomic-chrome-test-socket
                 'empty-socket
                 :atomic-server))
               events)
          (cl-letf
              (((symbol-function
                 'websocket-send-text)
                (lambda (target text)
                  (push
                   (list
                    (atomic-chrome-test-socket-name
                     target)
                    text)
                   events)
                  :sent)))
            (let ((empty-result
                   (with-temp-buffer
                     (puthash
                      (current-buffer)
                      (list socket nil)
                      atomic-chrome-buffer-table)
                     (set-buffer-modified-p t)
                     (list
                      (atomic-chrome-send-buffer-text)
                      (buffer-modified-p)))))
              (let ((missing-result
                     (with-temp-buffer
                       (insert "unsent")
                       (set-buffer-modified-p t)
                       (list
                        (atomic-chrome-send-buffer-text)
                        (buffer-modified-p)))))
                (list
                 empty-result
                 missing-result
                 (nreverse events))))))"##,
        expect![[
            r#"OK ((nil nil) (nil nil) ((empty-socket "{\"type\":\"updateText\",\"payload\":{\"text\":\"\"}}")))"#
        ]],
    )
}

fn atomic_chrome_send_buffer_text_propagates_transport_error_before_clearing_modified_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_send_buffer_text_propagates_transport_error_before_clearing_modified_state",
        r##"(let ((atomic-chrome-buffer-table
                (make-hash-table
                 :test 'equal))
               (atomic-chrome-server-ghost-text
                :ghost-server)
               (socket
                (atomic-chrome-test-socket
                 'failing-socket
                 :atomic-server))
               events)
          (with-temp-buffer
            (insert
             "must remain modified")
            (puthash
             (current-buffer)
             (list socket nil)
             atomic-chrome-buffer-table)
            (set-buffer-modified-p t)
            (cl-letf
                (((symbol-function
                   'websocket-send-text)
                  (lambda (target text)
                    (push
                     (list
                      'send
                      (atomic-chrome-test-socket-name
                       target)
                      text)
                     events)
                    (error
                     "transport failed %S"
                     (atomic-chrome-test-socket-name
                      target)))))
              (list
               (atomic-chrome-test-error-data
                #'atomic-chrome-send-buffer-text)
               (nreverse events)
               (buffer-modified-p)
               (buffer-string)
               (gethash
                (current-buffer)
                atomic-chrome-buffer-table)))))"##,
        expect![[
            r#"OK ((:error error ("transport failed failing-socket")) ((send failing-socket "{\"type\":\"updateText\",\"payload\":{\"text\":\"must remain modified\"}}")) t "must remain modified" (#s(websocket connecting failing-socket nil nil nil nil nil nil nil "ws://failing-socket.test" nil nil failing-socket :atomic-server nil nil nil) nil))"#
        ]],
    )
}

pub(super) fn messaging_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atomic_chrome_send_buffer_text_emits_exact_atomic_chrome_update_json_and_clears_modified_state(),
        atomic_chrome_send_buffer_text_emits_exact_ghost_text_json_shape(),
        atomic_chrome_send_buffer_text_uses_accessible_narrowed_plain_text(),
        atomic_chrome_send_buffer_text_sends_empty_content_but_without_socket_only_clears_modified(),
        atomic_chrome_send_buffer_text_propagates_transport_error_before_clearing_modified_state(),
    ]
}

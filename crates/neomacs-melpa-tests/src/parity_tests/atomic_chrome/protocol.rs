use expect_test::expect;

use super::ParityBatchCase;

fn atomic_chrome_on_message_decodes_atomic_register_payload_into_exact_create_call()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_on_message_decodes_atomic_register_payload_into_exact_create_call",
        r##"(let ((atomic-chrome-server-ghost-text
                :ghost-server)
               events
               (payload
                "{\"type\":\"register\",\"payload\":{\"url\":\"https://example.test/edit\",\"title\":\"Résumé 😀\",\"text\":\"first\\nsecond λ\"}}")
               (socket
                (atomic-chrome-test-socket
                 'atomic-socket
                 :atomic-server)))
          (cl-letf
              (((symbol-function
                 'atomic-chrome-create-buffer)
                (lambda (target url title text)
                  (push
                   (list
                    'create
                    (atomic-chrome-test-socket-name
                     target)
                    url
                    title
                    text)
                   events)
                  :created))
               ((symbol-function
                 'atomic-chrome-update-buffer)
                (lambda (&rest arguments)
                  (push
                   (cons 'unexpected-update
                         arguments)
                   events))))
            (list
             (atomic-chrome-on-message
              socket
              (atomic-chrome-test-frame
               payload))
             (nreverse events))))"##,
        expect![[
            r#"OK (:created ((create atomic-socket "https://example.test/edit" "Résumé 😀" "first\nsecond λ")))"#
        ]],
    )
}

fn atomic_chrome_on_message_applies_atomic_updates_only_when_bidirectional_edit_is_enabled()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_on_message_applies_atomic_updates_only_when_bidirectional_edit_is_enabled",
        r##"(let ((atomic-chrome-server-ghost-text
                :ghost-server)
               (socket
                (atomic-chrome-test-socket
                 'atomic-socket
                 :atomic-server))
               events)
          (cl-letf
              (((symbol-function
                 'atomic-chrome-update-buffer)
                (lambda (target text)
                  (push
                   (list
                    'update
                    (atomic-chrome-test-socket-name
                     target)
                    text)
                   events)
                  :updated))
               ((symbol-function
                 'atomic-chrome-create-buffer)
                (lambda (&rest arguments)
                  (push
                   (cons 'unexpected-create
                         arguments)
                   events))))
            (let ((atomic-chrome-enable-bidirectional-edit
                   t))
              (push
               (list
                :enabled
                (atomic-chrome-on-message
                 socket
                 (atomic-chrome-test-frame
                  "{\"type\":\"updateText\",\"payload\":{\"text\":\"from browser\"}}")))
               events))
            (let ((atomic-chrome-enable-bidirectional-edit
                   nil))
              (push
               (list
                :disabled
                (atomic-chrome-on-message
                 socket
                 (atomic-chrome-test-frame
                  "{\"type\":\"updateText\",\"payload\":{\"text\":\"from browser\"}}")))
               events))
            (push
             (list
              :unknown
              (atomic-chrome-on-message
               socket
               (atomic-chrome-test-frame
                "{\"type\":\"cursorMoved\",\"payload\":{\"text\":\"ignored\"}}")))
             events)
            (nreverse events)))"##,
        expect![[
            r#"OK ((update atomic-socket "from browser") (:enabled :updated) (:disabled nil) (:unknown nil))"#
        ]],
    )
}

fn atomic_chrome_on_message_ghost_text_creates_first_buffer_then_updates_existing_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_on_message_ghost_text_creates_first_buffer_then_updates_existing_buffer",
        r##"(let ((atomic-chrome-server-ghost-text
                :ghost-server)
               (existing-buffer
                (generate-new-buffer
                 " *atomic-ghost-existing*"))
               (socket
                (atomic-chrome-test-socket
                 'ghost-socket
                 :ghost-server))
               events
               existing)
          (unwind-protect
              (cl-letf
                  (((symbol-function
                     'atomic-chrome-get-buffer-by-socket)
                    (lambda (target)
                      (push
                       (list
                        'lookup
                        (atomic-chrome-test-socket-name
                         target)
                        (and existing t))
                       events)
                      existing))
                   ((symbol-function
                     'atomic-chrome-create-buffer)
                    (lambda (target url title text)
                      (push
                       (list
                        'create
                        (atomic-chrome-test-socket-name
                         target)
                        url
                        title
                        text)
                       events)
                      (setq existing
                            existing-buffer)
                      :created))
                   ((symbol-function
                     'atomic-chrome-update-buffer)
                    (lambda (target text)
                      (push
                       (list
                        'update
                        (atomic-chrome-test-socket-name
                         target)
                        text)
                       events)
                      :updated)))
                (list
                 (atomic-chrome-on-message
                  socket
                  (atomic-chrome-test-frame
                   "{\"url\":\"ghost.example\",\"title\":\"Ghost field\",\"text\":\"initial\"}"))
                 (atomic-chrome-on-message
                  socket
                  (atomic-chrome-test-frame
                   "{\"url\":\"ghost.example\",\"title\":\"Ghost field\",\"text\":\"replacement\"}"))
                 (nreverse events)))
            (atomic-chrome-test-kill-buffer
             existing-buffer)))"##,
        expect![[
            r#"OK (:created :updated ((lookup ghost-socket nil) (create ghost-socket "ghost.example" "Ghost field" "initial") (lookup ghost-socket t) (update ghost-socket "replacement")))"#
        ]],
    )
}

fn atomic_chrome_on_message_records_malformed_missing_and_wrong_type_json_failures()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_on_message_records_malformed_missing_and_wrong_type_json_failures",
        r##"(let ((atomic-chrome-server-ghost-text
                :ghost-server)
               (socket
                (atomic-chrome-test-socket
                 'atomic-socket
                 :atomic-server)))
          (cl-letf
              (((symbol-function
                 'atomic-chrome-create-buffer)
                (lambda (&rest _arguments)
                  :created))
               ((symbol-function
                 'atomic-chrome-update-buffer)
                (lambda (&rest _arguments)
                  :updated)))
            (mapcar
             (lambda (payload)
               (list
                payload
                (atomic-chrome-test-error-data
                 (lambda ()
                   (atomic-chrome-on-message
                    socket
                    (atomic-chrome-test-frame
                     payload))))))
             '("{"
               "null"
               "{}"
               "{\"type\":17}"
               "{\"type\":\"register\"}"
               "{\"type\":\"updateText\",\"payload\":null}"))))"##,
        expect![[
            r#"OK (("{" (:error end-of-buffer nil)) ("null" (:ok nil)) ("{}" (:ok nil)) ("{\"type\":17}" (:error wrong-type-argument (stringp 17))) ("{\"type\":\"register\"}" (:ok :created)) ("{\"type\":\"updateText\",\"payload\":null}" (:ok :updated)))"#
        ]],
    )
}

fn atomic_chrome_on_message_utf8_round_trip_preserves_multibyte_payload_content() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atomic_chrome_on_message_utf8_round_trip_preserves_multibyte_payload_content",
        r##"(let ((atomic-chrome-server-ghost-text
                :ghost-server)
               (socket
                (atomic-chrome-test-socket
                 'atomic-socket
                 :atomic-server))
               (payload
                (encode-coding-string
                 "{\"type\":\"register\",\"payload\":{\"url\":\"https://例.example/λ\",\"title\":\"日本語 😀\",\"text\":\"café\\nκαλημέρα\"}}"
                 'utf-8))
               observed)
          (cl-letf
              (((symbol-function
                 'atomic-chrome-create-buffer)
                (lambda (_socket url title text)
                  (setq observed
                        (list
                         url
                         title
                         text
                         (multibyte-string-p url)
                         (multibyte-string-p title)
                         (multibyte-string-p text)))
                  :created)))
            (list
             (atomic-chrome-on-message
              socket
              (atomic-chrome-test-frame
               payload))
             observed)))"##,
        expect![[r#"OK (:created ("https://例.example/λ" "日本語 😀" "café\nκαλημέρα" t t t))"#]],
    )
}

fn atomic_chrome_on_close_delegates_only_for_associated_buffer_and_propagates_close_error()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atomic_chrome_on_close_delegates_only_for_associated_buffer_and_propagates_close_error",
        r##"(let ((buffer
                (generate-new-buffer
                 " *atomic-on-close*"))
               events
               found)
          (unwind-protect
              (cl-letf
                  (((symbol-function
                     'atomic-chrome-get-buffer-by-socket)
                    (lambda (socket)
                      (push
                       (list
                        'lookup
                        socket
                        (and found t))
                       events)
                      found))
                   ((symbol-function
                     'atomic-chrome-close-edit-buffer)
                    (lambda (target)
                      (push
                       (list
                        'close
                        (buffer-name target))
                       events)
                      (error
                       "close callback failed"))))
                (let ((missing
                       (atomic-chrome-on-close
                        :missing)))
                  (setq found buffer)
                  (list
                   missing
                   (atomic-chrome-test-error-data
                    (lambda ()
                      (atomic-chrome-on-close
                       :present)))
                   (nreverse events))))
            (atomic-chrome-test-kill-buffer
             buffer)))"##,
        expect![[
            r#"OK (nil (:error error ("close callback failed")) ((lookup :missing nil) (lookup :present t) (close " *atomic-on-close*")))"#
        ]],
    )
}

pub(super) fn protocol_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atomic_chrome_on_message_decodes_atomic_register_payload_into_exact_create_call(),
        atomic_chrome_on_message_applies_atomic_updates_only_when_bidirectional_edit_is_enabled(),
        atomic_chrome_on_message_ghost_text_creates_first_buffer_then_updates_existing_buffer(),
        atomic_chrome_on_message_records_malformed_missing_and_wrong_type_json_failures(),
        atomic_chrome_on_message_utf8_round_trip_preserves_multibyte_payload_content(),
        atomic_chrome_on_close_delegates_only_for_associated_buffer_and_propagates_close_error(),
    ]
}

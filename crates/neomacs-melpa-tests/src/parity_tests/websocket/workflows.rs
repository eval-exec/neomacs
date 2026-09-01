use expect_test::expect;

use super::ParityBatchCase;

fn negotiated_unicode_request_response_completes_the_client_and_server_lifecycle() -> ParityBatchCase
{
    let elisp_form = r##"
(let ((saved-websockets websocket-server-websockets)
      server client server-socket response server-events client-events)
  (unwind-protect
      (progn
        (setq server
              (websocket-server
               t
               :host 'local
               :protocol '("neomacs.json.v2")
               :on-open
               (lambda (socket)
                 (setq server-socket socket)
                 (push
                  (list :open
                        :protocols (copy-sequence (websocket-protocols socket))
                        :origin (websocket-origin socket))
                  server-events))
               :on-message
               (lambda (socket frame)
                 (let ((message (websocket-frame-text frame)))
                   (push
                    (list :message
                          :opcode (websocket-frame-opcode frame)
                          :complete (websocket-frame-completep frame)
                          :bytes (string-bytes (websocket-frame-payload frame))
                          :text (copy-sequence message))
                    server-events)
                   (websocket-send-text socket (concat "accepted:" message))))
               :on-close
               (lambda (socket)
                 (push
                  (list :close :process (process-status (websocket-conn socket)))
                  server-events))))
        (let ((port (process-contact server :service)))
          (setq client
                (websocket-open
                 (format "ws://127.0.0.1:%d/releases?channel=stable" port)
                 :protocols '("neomacs.json.v2")
                 :on-open
                 (lambda (socket)
                   (push
                    (list :open
                          :protocols
                          (copy-sequence
                           (websocket-negotiated-protocols socket)))
                    client-events))
                 :on-message
                 (lambda (_socket frame)
                   (setq response (websocket-frame-text frame))
                   (push
                    (list :message
                          :opcode (websocket-frame-opcode frame)
                          :complete (websocket-frame-completep frame)
                          :text (copy-sequence response))
                    client-events))
                 :on-close
                 (lambda (_socket)
                   (push '(:close) client-events))))
          (unless (websocket-parity-wait-until
                   (lambda ()
                     (and server-socket
                          (eq (websocket-ready-state client) 'open))))
            (error "websocket handshake did not complete"))
          (websocket-send-text
           client
           "{\"action\":\"publish\",\"artifact\":\"neomacs-λ\",\"revision\":41}")
          (unless (websocket-parity-wait-until (lambda () response))
            (error "websocket response did not arrive"))
          (let ((open-state
                 (list
                  :client-open (and (websocket-openp client) t)
                  :server-open (and (websocket-openp server-socket) t)
                  :response (copy-sequence response))))
            (websocket-close client)
            (unless (websocket-parity-wait-until
                     (lambda ()
                       (and (not (websocket-openp server-socket))
                            (null websocket-server-websockets))))
              (error "websocket close handshake did not complete"))
            (list
             :ephemeral-port (and (integerp port) (> port 0))
             :open-state open-state
             :server-events (nreverse server-events)
             :client-events (nreverse client-events)
             :client-after-close (and (websocket-openp client) t)
             :server-after-close (and (websocket-openp server-socket) t)
             :registered-after-close
             (length websocket-server-websockets)))))
    (websocket-parity-close-client client)
    (websocket-parity-close-server server)
    (setq websocket-server-websockets saved-websockets)))
"##;
    let expect = expect![[
        r####"OK (:ephemeral-port t :open-state (:client-open t :server-open t :response "accepted:{\"action\":\"publish\",\"artifact\":\"neomacs-λ\",\"revision\":41}") :server-events ((:open :protocols ("neomacs.json.v2") :origin nil) (:message :opcode text :complete t :bytes 58 :text "{\"action\":\"publish\",\"artifact\":\"neomacs-λ\",\"revision\":41}") (:close :process closed)) :client-events ((:open :protocols ("neomacs.json.v2")) (:message :opcode text :complete t :text "accepted:{\"action\":\"publish\",\"artifact\":\"neomacs-λ\",\"revision\":41}") (:close)) :client-after-close nil :server-after-close nil :registered-after-close 0)"####
    ]];
    ParityBatchCase::value(
        "negotiated_unicode_request_response_completes_the_client_and_server_lifecycle",
        elisp_form,
        expect,
    )
}

fn fragmented_text_and_a_large_binary_artifact_cross_real_frame_boundaries() -> ParityBatchCase {
    let elisp_form = r##"
(let ((saved-websockets websocket-server-websockets)
      server client server-socket fragments acknowledgements server-frames)
  (unwind-protect
      (progn
        (setq server
              (websocket-server
               t
               :host 'local
               :on-open
               (lambda (socket)
                 (setq server-socket socket))
               :on-message
               (lambda (socket frame)
                 (let* ((opcode (websocket-frame-opcode frame))
                        (payload (websocket-frame-payload frame)))
                   (push
                    (list
                     :opcode opcode
                     :complete (websocket-frame-completep frame)
                     :bytes (string-bytes payload)
                     :digest (secure-hash 'sha256 payload)
                     :text
                     (when (memq opcode '(text continuation))
                       (copy-sequence (websocket-frame-text frame))))
                    server-frames)
                   (cond
                    ((memq opcode '(text continuation))
                     (setq fragments
                           (concat fragments (websocket-frame-text frame)))
                     (when (websocket-frame-completep frame)
                       (websocket-send-text
                        socket
                        (format "document:%d:%s"
                                (length fragments)
                                (secure-hash 'sha256 fragments)))))
                    ((eq opcode 'binary)
                     (websocket-send-text
                      socket
                      (format "artifact:%d:%s"
                              (string-bytes payload)
                              (secure-hash 'sha256 payload)))))))))
        (let ((port (process-contact server :service)))
          (setq client
                (websocket-open
                 (format "ws://127.0.0.1:%d/stream" port)
                 :on-message
                 (lambda (_socket frame)
                   (push (copy-sequence (websocket-frame-text frame))
                         acknowledgements))))
          (unless (websocket-parity-wait-until
                   (lambda ()
                     (and server-socket
                          (eq (websocket-ready-state client) 'open))))
            (error "streaming websocket handshake did not complete"))
          (dolist
              (frame-spec
               '((text nil "Release 41: ")
                 (continuation nil "publish neomacs-λ ")
                 (continuation t "after all checks pass.")))
            (pcase-let ((`(,opcode ,completep ,payload) frame-spec))
              (websocket-send
               client
               (make-websocket-frame
                :opcode opcode
                :payload (encode-coding-string payload 'utf-8)
                :completep completep))))
          (let ((artifact (string-make-unibyte (make-string 70000 0))))
            (dotimes (index (length artifact))
              (aset artifact index (mod (+ 3 (* index 17)) 256)))
            (websocket-send
             client
             (make-websocket-frame
              :opcode 'binary
              :payload artifact
              :completep t)))
          (unless (websocket-parity-wait-until
                   (lambda () (= (length acknowledgements) 2)))
            (error
             (concat
              "streaming websocket acknowledgements did not arrive: "
              "acknowledgements=%S server-frames=%S "
              "client=(%S %S %d) server=(%S %S %d)")
             (nreverse (copy-tree acknowledgements))
             (nreverse (copy-tree server-frames))
             (websocket-ready-state client)
             (process-status (websocket-conn client))
             (length (or (websocket-inflight-input client) ""))
             (websocket-ready-state server-socket)
             (process-status (websocket-conn server-socket))
             (length (or (websocket-inflight-input server-socket) ""))))
          (let ((result
                 (list
                  :ephemeral-port (and (integerp port) (> port 0))
                  :client-open (and (websocket-openp client) t)
                  :server-open (and (websocket-openp server-socket) t)
                  :server-frames (nreverse server-frames)
                  :acknowledgements (nreverse acknowledgements))))
            (websocket-close client)
            (websocket-parity-wait-until
             (lambda () (not (websocket-openp server-socket))))
            result)))
    (websocket-parity-close-client client)
    (websocket-parity-close-server server)
    (setq websocket-server-websockets saved-websockets)))
"##;
    let expect = expect![[
        r####"OK (:ephemeral-port t :client-open t :server-open t :server-frames ((:opcode text :complete nil :bytes 12 :digest "fdac51f1ed86cf61ffe51860d3ef0f767fcf7237cab59f9e4bb89621dd52853c" :text "Release 41: ") (:opcode continuation :complete nil :bytes 19 :digest "8821271fb94b48b92e6b9e844c895e87fe59038af1e3449e19b39d72edce10f6" :text "publish neomacs-λ ") (:opcode continuation :complete t :bytes 22 :digest "6f40c456b5ef02a134a04ea7440e6c65a421f45c84cc59fe47808de549f1c99b" :text "after all checks pass.") (:opcode binary :complete t :bytes 70000 :digest "410bc4e8cff855051870b66993a633fce06c9a56108fcde63e0a299f5f404ea5" :text nil)) :acknowledgements ("document:52:65378995d1d5aea11ef790ec14a67106504110ce00eef303da4eedd199a5beef" "artifact:70000:410bc4e8cff855051870b66993a633fce06c9a56108fcde63e0a299f5f404ea5"))"####
    ]];
    ParityBatchCase::value(
        "fragmented_text_and_a_large_binary_artifact_cross_real_frame_boundaries",
        elisp_form,
        expect,
    )
}

fn a_callback_failure_is_reported_and_the_connection_can_serve_the_next_message() -> ParityBatchCase
{
    let elisp_form = r##"
(let ((saved-websockets websocket-server-websockets)
      server client server-socket errors responses closed-send)
  (unwind-protect
      (progn
        (setq server
              (websocket-server
               t
               :host 'local
               :on-open (lambda (socket) (setq server-socket socket))
               :on-message
               (lambda (socket frame)
                 (let ((message (websocket-frame-text frame)))
                   (if (equal message "publish-invalid-release")
                       (error "release 41 failed policy validation")
                     (websocket-send-text
                      socket
                      (concat "healthy:" message)))))
               :on-error
               (lambda (_socket callback-type error-data)
                 (push
                  (list callback-type
                        (car error-data)
                        (copy-tree (cdr error-data)))
                  errors))))
        (let ((port (process-contact server :service)))
          (setq client
                (websocket-open
                 (format "ws://127.0.0.1:%d/recovery" port)
                 :on-message
                 (lambda (_socket frame)
                   (push (copy-sequence (websocket-frame-text frame)) responses))))
          (unless (websocket-parity-wait-until
                   (lambda ()
                     (and server-socket
                          (eq (websocket-ready-state client) 'open))))
            (error "recovery websocket handshake did not complete"))
          (websocket-send-text client "publish-invalid-release")
          (unless (websocket-parity-wait-until (lambda () errors))
            (error "callback failure was not reported"))
          (let ((open-after-error
                 (list
                  (and (websocket-openp client) t)
                  (and (websocket-openp server-socket) t))))
            (websocket-send-text client "health-check")
            (unless (websocket-parity-wait-until (lambda () responses))
              (error "connection did not recover after callback failure"))
            (websocket-close client)
            (condition-case error-data
                (websocket-send-text client "late-release")
              (websocket-closed
               (let ((frame (cadr error-data)))
                 (setq closed-send
                       (list
                        (car error-data)
                        (websocket-frame-opcode frame)
                        (websocket-frame-text frame)
                        (websocket-frame-completep frame))))))
            (list
             :ephemeral-port (and (integerp port) (> port 0))
             :errors (nreverse errors)
             :open-after-error open-after-error
             :responses (nreverse responses)
             :closed-send closed-send
             :client-open-after-close (and (websocket-openp client) t)))))
    (websocket-parity-close-client client)
    (websocket-parity-close-server server)
    (setq websocket-server-websockets saved-websockets)))
"##;
    let expect = expect![[
        r####"OK (:ephemeral-port t :errors ((on-message error ("release 41 failed policy validation"))) :open-after-error (t t) :responses ("healthy:health-check") :closed-send (websocket-closed text "late-release" t) :client-open-after-close nil)"####
    ]];
    ParityBatchCase::value(
        "a_callback_failure_is_reported_and_the_connection_can_serve_the_next_message",
        elisp_form,
        expect,
    )
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        negotiated_unicode_request_response_completes_the_client_and_server_lifecycle(),
        fragmented_text_and_a_large_binary_artifact_cross_real_frame_boundaries(),
        a_callback_failure_is_reported_and_the_connection_can_serve_the_next_message(),
    ]
}

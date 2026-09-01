use expect_test::expect;

use super::ParityBatchCase;

fn fragmented_unicode_json_rpc_stream_dispatches_every_message_in_order() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((received nil)
       (filter (lsp--create-filter-function 'deployment-workspace))
       (request
        '(:jsonrpc "2.0"
          :id 41
          :method "workspace/executeCommand"
          :params (:command "deploy.preview"
                   :arguments ["orders" "😀"])))
       (notification
        '(:jsonrpc "2.0"
          :method "window/logMessage"
          :params (:type 3
                   :message "deployed café"
                   :enabled :json-false)))
       (wire
        (string-as-unibyte
         (concat
          "language server ready\n"
          (lsp--make-message request)
          (lsp--make-message notification))))
       (chunk-sizes '(1 2 7 3 11 5 4 13))
       (offset 0)
       (chunk-index 0))
  (cl-letf (((symbol-function 'lsp--parser-on-message)
             (lambda (message workspace)
               (let ((params
                      (neomacs-lsp-test-json-get message "params")))
                 (push
                  (list
                   :workspace workspace
                   :id (neomacs-lsp-test-json-get message "id")
                   :method (neomacs-lsp-test-json-get message "method")
                   :command (and params
                                 (neomacs-lsp-test-json-get params "command"))
                   :arguments (and params
                                   (neomacs-lsp-test-json-get params "arguments"))
                   :type (and params
                              (neomacs-lsp-test-json-get params "type"))
                   :message (and params
                                 (neomacs-lsp-test-json-get params "message"))
                   :enabled (and params
                                 (neomacs-lsp-test-json-get params "enabled")))
                  received)))))
    (while (< offset (length wire))
      (let* ((size (nth (mod chunk-index (length chunk-sizes)) chunk-sizes))
             (end (min (length wire) (+ offset size))))
        (funcall filter nil (substring wire offset end))
        (setq offset end
              chunk-index (1+ chunk-index))))
    (list :chunk-count chunk-index
          :messages (nreverse received))))
"##;
    let expected = expect![[
        r##"OK (:chunk-count 55 :messages ((:workspace deployment-workspace :id 41 :method "workspace/executeCommand" :command "deploy.preview" :arguments ["orders" "😀"] :type nil :message nil :enabled nil) (:workspace deployment-workspace :id nil :method "window/logMessage" :command nil :arguments nil :type 3 :message "deployed café" :enabled nil)))"##
    ]];
    ParityBatchCase::value(
        "fragmented_unicode_json_rpc_stream_dispatches_every_message_in_order",
        elisp_form,
        expected,
    )
}

pub(super) fn transport_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![fragmented_unicode_json_rpc_stream_dispatches_every_message_in_order()]
}

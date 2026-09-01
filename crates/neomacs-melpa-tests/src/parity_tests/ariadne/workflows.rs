use expect_test::expect;

use super::ParityBatchCase;

fn documented_key_binding_drives_a_multi_reply_definition_session_over_a_real_byte_stream()
-> ParityBatchCase {
    ParityBatchCase::value(
        "documented_key_binding_drives_a_multi_reply_definition_session_over_a_real_byte_stream",
        r##"(let* ((sandbox
                 (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                (project
                 (expand-file-name "ariadne-project" sandbox))
                (source-file
                 (expand-file-name "src/Main.hs" project))
                (target-file
                 (expand-file-name "src/Lib.hs" project))
                (server-script
                 (expand-file-name "fake-ariadne-server.py"
                                   sandbox))
                (request-log
                 (expand-file-name "requests.bert" sandbox))
                (process-buffer
                 (get-buffer-create "*ariadne*"))
                (source-buffer nil)
                (target-buffer nil)
                (process nil)
                (network-arguments nil)
                (dial-count 0)
                (binding nil)
                (known-target nil)
                (known-mark nil)
                (unknown-message nil)
                (rpc-error-message nil)
                (no-name-before nil)
                (no-name-point nil)
                (messages nil)
                (real-message (symbol-function 'message))
                (requests nil))
         (make-directory
          (file-name-directory source-file) t)
         (with-temp-file source-file
           (insert
            "module Main where\n"
            "answer = helper 41\n"
            "missing = externalName\n"
            "broken = serverFailure\n"
            "nowhere = absent\n"))
         (with-temp-file target-file
           (insert
            "module Lib where\n"
            "helper x =\n"
            "  x + 1\n"
            "other = helper 0\n"))
         (with-temp-file server-script
           (insert
            "import pathlib, struct, sys, time\n"
            "log_path = pathlib.Path(sys.argv[1])\n"
            "target = sys.argv[2].encode('utf-8')\n"
            "def atom(value):\n"
            "    data = value.encode('ascii')\n"
            "    return b'd' + struct.pack('>H', len(data)) + data\n"
            "def binary(value):\n"
            "    return b'm' + struct.pack('>I', len(value)) + value\n"
            "def integer(value):\n"
            "    if 0 <= value < 256:\n"
            "        return b'a' + bytes((value,))\n"
            "    return b'b' + struct.pack('>I', value)\n"
            "def tuple_of(*values):\n"
            "    return b'h' + bytes((len(values),)) + b''.join(values)\n"
            "def frame(value):\n"
            "    body = bytes((131,)) + value\n"
            "    return struct.pack('>I', len(body)) + body\n"
            "responses = [\n"
            "    frame(tuple_of(atom('reply'),\n"
            "                   tuple_of(atom('loc_known'),\n"
            "                            binary(target), integer(3),\n"
            "                            integer(7)))),\n"
            "    frame(tuple_of(atom('reply'),\n"
            "                   tuple_of(atom('loc_unknown'),\n"
            "                            binary(b'Data.Map.Internal')))),\n"
            "    frame(tuple_of(atom('error'),\n"
            "                   tuple_of(atom('request'), integer(500),\n"
            "                            atom('server'),\n"
            "                            binary(b'database unavailable'),\n"
            "                            b'j'))),\n"
            "    frame(tuple_of(atom('reply'),\n"
            "                   tuple_of(atom('no_name')))),\n"
            "]\n"
            "with log_path.open('wb') as log:\n"
            "    for response in responses:\n"
            "        header = sys.stdin.buffer.read(4)\n"
            "        if len(header) != 4:\n"
            "            raise SystemExit('incomplete request header')\n"
            "        size = struct.unpack('>I', header)[0]\n"
            "        body = sys.stdin.buffer.read(size)\n"
            "        if len(body) != size:\n"
            "            raise SystemExit('incomplete request body')\n"
            "        log.write(header + body)\n"
            "        log.flush()\n"
            "        for chunk in (response[:2], response[2:7], response[7:]):\n"
            "            sys.stdout.buffer.write(chunk)\n"
            "            sys.stdout.buffer.flush()\n"
            "            time.sleep(0.01)\n"))
         (setq source-buffer
               (find-file-noselect source-file))
         (setq target-buffer
               (find-file-noselect target-file))
         (with-current-buffer source-buffer
           (goto-char (point-min))
           (forward-line 1)
           (narrow-to-region (point) (point-max)))
         (with-current-buffer target-buffer
           (goto-char (point-min))
           (forward-line 1)
           (narrow-to-region (point) (point-max)))
         (unwind-protect
             (progn
               (with-current-buffer process-buffer
                 (set-buffer-multibyte nil)
                 (erase-buffer))
               (setq process
                     (make-process
                      :name "ariadne-parity-server"
                      :buffer process-buffer
                      :command
                      (list
                       (or (executable-find "python3")
                           (error "python3 is required"))
                       "-u" server-script request-log target-file)
                      :connection-type 'pipe
                      :coding 'binary
                      :filter #'ariadne-filter
                      :noquery t))
               (cl-letf
                   (((symbol-function 'make-network-process)
                     (lambda (&rest arguments)
                       (setq network-arguments arguments
                             dial-count (1+ dial-count))
                       process))
                    ((symbol-function 'message)
                     (lambda (format-string &rest arguments)
                       (when format-string
                         (push
                          (apply #'format format-string arguments)
                          messages))
                       (apply real-message format-string arguments))))
                 (with-current-buffer source-buffer
                   (use-local-map
                    (copy-keymap
                     (or
                      (current-local-map)
                      (make-sparse-keymap))))
                   (local-set-key
                    (kbd "C-c d") #'ariadne-goto-definition)
                   (setq binding (key-binding (kbd "C-c d")))
                   (goto-char (point-min))
                   (search-forward "helper")
                   (goto-char (match-beginning 0))
                   (call-interactively binding))
                 (let ((deadline (+ (float-time) 5.0)))
                   (while
                       (and
                        (< (float-time) deadline)
                        (with-current-buffer target-buffer
                          (not
                           (and (= (line-number-at-pos) 3)
                                (= (current-column) 6)))))
                     (accept-process-output process 0.05)))
                 (setq known-target
                       (with-current-buffer target-buffer
                         (list
                          (file-name-nondirectory
                           (buffer-file-name))
                          (line-number-at-pos)
                          (current-column)
                          (char-after)
                          (point-min)
                          (point-max)
                          (buffer-size))))
                 (setq known-mark
                       (with-current-buffer source-buffer
                         (save-restriction
                           (widen)
                           (list
                            (line-number-at-pos (mark t))
                            (save-excursion
                              (goto-char (mark t))
                              (current-column))))))
                 (with-current-buffer source-buffer
                   (goto-char (point-min))
                   (search-forward "externalName")
                   (goto-char (match-beginning 0))
                   (call-interactively binding))
                 (let ((deadline (+ (float-time) 5.0)))
                   (while
                       (and
                        (< (float-time) deadline)
                        (not
                         (member
                          "The name at point is defined in Data.Map.Internal"
                          messages)))
                     (accept-process-output process 0.05)))
                 (setq unknown-message
                       (car
                        (member
                         "The name at point is defined in Data.Map.Internal"
                         messages)))
                 (with-current-buffer source-buffer
                   (goto-char (point-min))
                   (search-forward "serverFailure")
                   (goto-char (match-beginning 0))
                   (call-interactively binding))
                 (let ((deadline (+ (float-time) 5.0)))
                   (while
                       (and
                        (< (float-time) deadline)
                        (not
                         (member
                          "BERT-RPC error: database unavailable"
                          messages)))
                     (accept-process-output process 0.05)))
                 (setq rpc-error-message
                       (car
                        (member
                         "BERT-RPC error: database unavailable"
                         messages)))
                 (with-current-buffer source-buffer
                   (goto-char (point-min))
                   (search-forward "absent")
                   (goto-char (match-beginning 0))
                   (setq no-name-before (point))
                   (call-interactively binding)
                   (let ((deadline (+ (float-time) 5.0)))
                     (while
                         (and
                          (< (float-time) deadline)
                          (process-live-p process))
                       (accept-process-output process 0.05)))
                     (accept-process-output process 0.05))
                   (setq no-name-point
                         (with-current-buffer source-buffer
                           (list
                            (= no-name-before (point))
                            (save-restriction
                              (widen)
                              (line-number-at-pos))
                            (current-column)
                            (point-min)
                            (save-restriction
                              (widen)
                              (point-min))
                            (car messages)))))
               (with-temp-buffer
                 (set-buffer-multibyte nil)
                 (insert-file-contents-literally request-log)
                 (goto-char (point-min))
                 (while (< (point) (point-max))
                   (let* ((header-start (point))
                          (header-end (+ header-start 4))
                          (header
                           (buffer-substring-no-properties
                            header-start header-end))
                          (size
                           (bindat-get-field
                            (bindat-unpack
                             '((length u32)) header)
                            'length))
                          (body-start header-end)
                          (body-end (+ body-start size)))
                     (push
                      (bert-unpack
                       (buffer-substring-no-properties
                        body-start body-end))
                      requests)
                     (goto-char body-end))))
               (list
                binding
                (list
                 dial-count
                 (plist-get network-arguments :name)
                 (plist-get network-arguments :host)
                 (plist-get network-arguments :service)
                 (plist-get network-arguments :buffer)
                 (plist-get network-arguments :filter)
                 (plist-get network-arguments :sentinel)
                 (with-current-buffer process-buffer
                   enable-multibyte-characters)
                 (process-query-on-exit-flag process))
                (nreverse requests)
                known-target
                known-mark
                unknown-message
                rpc-error-message
                no-name-point))
           (when (and process (process-live-p process))
             (delete-process process))
           (setq ariadne-process nil)
           (dolist
               (buffer
                (list source-buffer target-buffer process-buffer))
             (when (buffer-live-p buffer)
               (with-current-buffer buffer
                 (set-buffer-modified-p nil))
               (kill-buffer buffer)))))"##,
        expect![[
            r#"OK (ariadne-goto-definition (1 "ariadne" "localhost" 39014 "*ariadne*" ariadne-filter ariadne-sentinel nil nil) ([call ariadne find ("[ORACLE-SANDBOX]/ariadne-project/src/Main.hs" 2 9)] [call ariadne find ("[ORACLE-SANDBOX]/ariadne-project/src/Main.hs" 3 10)] [call ariadne find ("[ORACLE-SANDBOX]/ariadne-project/src/Main.hs" 4 9)] [call ariadne find ("[ORACLE-SANDBOX]/ariadne-project/src/Main.hs" 5 10)]) ("Lib.hs" 3 6 49 1 54 53) (2 9) "The name at point is defined in Data.Map.Internal" "BERT-RPC error: database unavailable" (t 5 10 19 1 "BERT-RPC error: database unavailable"))"#
        ]],
    )
}

fn documented_command_ignores_an_unsaved_draft_then_reports_an_offline_server_after_save()
-> ParityBatchCase {
    ParityBatchCase::value(
        "documented_command_ignores_an_unsaved_draft_then_reports_an_offline_server_after_save",
        r##"(let* ((sandbox
                 (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
                (file
                 (expand-file-name
                  "offline-project/src/Offline.hs" sandbox))
                (buffer
                 (generate-new-buffer
                  "ariadne-offline-draft"))
                (dial-count 0)
                (binding nil)
                (draft nil)
                (saved nil)
                (messages nil)
                (real-message (symbol-function 'message)))
         (make-directory (file-name-directory file) t)
         (unwind-protect
             (cl-letf
                 (((symbol-function 'make-network-process)
                   (lambda (&rest _arguments)
                     (setq dial-count (1+ dial-count))
                     (signal
                      'file-error
                      '("connection refused" "localhost" 39014))))
                  ((symbol-function 'message)
                   (lambda (format-string &rest arguments)
                     (when format-string
                       (push
                        (apply #'format format-string arguments)
                        messages))
                     (apply real-message format-string arguments))))
               (with-current-buffer buffer
                 (insert
                  "module Offline where\n"
                  "answer = missingDefinition\n")
                 (goto-char (point-min))
                 (forward-line 1)
                 (search-forward "missingDefinition")
                 (goto-char (match-beginning 0))
                 (use-local-map
                  (copy-keymap
                   (or
                    (current-local-map)
                    (make-sparse-keymap))))
                 (local-set-key
                  (kbd "C-c d") #'ariadne-goto-definition)
                 (setq binding (key-binding (kbd "C-c d")))
                 (let ((before (point)))
                   (call-interactively binding)
                   (setq draft
                         (list
                          (buffer-file-name)
                          (= before (point))
                          (mark t)
                          dial-count
                          (buffer-modified-p))))
                 (set-visited-file-name file t t)
                 (save-buffer)
                 (let ((before (point)))
                   (call-interactively binding)
                   (setq saved
                         (list
                          (file-name-nondirectory
                           (buffer-file-name))
                          (file-exists-p file)
                          (= before (point))
                          (line-number-at-pos)
                          (current-column)
                          (mark t)
                          dial-count
                          ariadne-process
                          (car messages)
                          (buffer-modified-p)))))
               (list binding draft saved))
           (setq ariadne-process nil)
           (when (buffer-live-p buffer)
             (with-current-buffer buffer
               (set-buffer-modified-p nil))
             (kill-buffer buffer))))"##,
        expect![[
            r#"OK (ariadne-goto-definition (nil t nil 0 t) ("Offline.hs" t t 2 9 nil 1 nil "Failed to connect to Ariadne.  Is ariadne-server running?" nil))"#
        ]],
    )
}

pub(super) fn workflows_ariadne_with_legacy_cl_batch_cases() -> Vec<ParityBatchCase> {
    vec![documented_key_binding_drives_a_multi_reply_definition_session_over_a_real_byte_stream()]
}

pub(super) fn workflows_ariadne_batch_cases() -> Vec<ParityBatchCase> {
    vec![documented_command_ignores_an_unsaved_draft_then_reports_an_offline_server_after_save()]
}

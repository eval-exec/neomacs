;;; neomacs-mcp-test.el --- Tests for neomacs-mcp.el -*- lexical-binding: t; -*-

;; Copyright (C) 2026 Free Software Foundation, Inc.

;; This file is part of GNU Emacs.

;; GNU Emacs is free software: you can redistribute it and/or modify
;; it under the terms of the GNU General Public License as published by
;; the Free Software Foundation, either version 3 of the License, or
;; (at your option) any later version.

;; GNU Emacs is distributed in the hope that it will be useful,
;; but WITHOUT ANY WARRANTY; without even the implied warranty of
;; MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
;; GNU General Public License for more details.

;; You should have received a copy of the GNU General Public License
;; along with GNU Emacs.  If not, see <https://www.gnu.org/licenses/>.

;;; Commentary:

;; Run with Neomacs or GNU Emacs from the repository root:
;;
;;   neomacs -Q --batch -L lisp -l test/neomacs/neomacs-mcp-test.el \
;;     -f ert-run-tests-batch-and-exit
;;
;; Set NEOMACS_MCP_RELAY to a built `neomacs-mcp' relay to also run the
;; stdio relay round trip.

;;; Code:

(require 'ert)
(require 'cl-lib)
(require 'neomacs-mcp)

(defvar neomacs-mcp-test-effect nil)

;;;; Helpers

(defun neomacs-mcp-test--instance ()
  (gethash "instance" (neomacs-mcp-identity)))

(defun neomacs-mcp-test--args (&rest pairs)
  "Return tool arguments with the current instance and PAIRS."
  (apply #'neomacs-mcp--object "instance" (neomacs-mcp-test--instance) pairs))

(defun neomacs-mcp-test--request (peer id &optional generation)
  "Return a queued request for PEER and ID, built by the real constructor."
  (let ((neomacs-mcp--queue nil)
        (neomacs-mcp--generation (or generation neomacs-mcp--generation)))
    (cl-letf (((symbol-function 'neomacs-mcp--schedule) #'ignore))
      (neomacs-mcp--enqueue peer (neomacs-mcp--object "id" id "method" "fixture")))
    (car neomacs-mcp--queue)))

(defmacro neomacs-mcp-test--with-peer (&rest body)
  "Run BODY with `peer' bound to a live process for process properties."
  (declare (indent 0) (debug t))
  `(let ((peer (make-pipe-process :name "neomacs-mcp-test-peer" :noquery t)))
     (unwind-protect (progn ,@body)
       (delete-process peer))))

(defun neomacs-mcp-test--initialize (peer version)
  (neomacs-mcp--dispatch
   peer (neomacs-mcp--object
         "method" "initialize" "params"
         (neomacs-mcp--object "protocolVersion" version
                              "capabilities" (neomacs-mcp--object)
                              "clientInfo" (neomacs-mcp--object "name" "test"
                                                                "version" "1")))))

(defun neomacs-mcp-test--modern-params (&rest pairs)
  (apply #'neomacs-mcp--object
         "_meta" (neomacs-mcp--object
                  "io.modelcontextprotocol/protocolVersion" neomacs-mcp--modern
                  "io.modelcontextprotocol/clientCapabilities" (neomacs-mcp--object))
         pairs))

(defun neomacs-mcp-test--tool-names (result)
  (mapcar (lambda (tool) (gethash "name" tool)) (gethash "tools" result)))

;;;; Tools and registry

(ert-deftest neomacs-mcp-test-default-tools ()
  (let ((neomacs-mcp-full-access t))
    (should (equal '("neomacs_buffer_list" "neomacs_buffer_read"
                     "neomacs_eval" "neomacs_identity")
                   (neomacs-mcp-test--tool-names
                    (neomacs-mcp--object "tools" (neomacs-mcp--tool-list)))))
    (should (gethash "readOnlyHint"
                     (plist-get (alist-get "neomacs_buffer_read" neomacs-mcp-tools
                                           nil nil #'equal)
                                :annotations)))
    (should-not (plist-get (alist-get "neomacs_eval" neomacs-mcp-tools nil nil #'equal)
                           :annotations))))

(ert-deftest neomacs-mcp-test-full-access-default-and-off ()
  (should (eq t (default-value 'neomacs-mcp-full-access)))
  (should (custom-variable-p 'neomacs-mcp-full-access))
  (neomacs-mcp-test--with-peer
    (process-put peer 'legacy 'ready)
    (let ((neomacs-mcp-full-access nil)
          (neomacs-mcp-test-effect nil))
      (let ((names (neomacs-mcp-test--tool-names
                    (neomacs-mcp--dispatch
                     peer (neomacs-mcp--object "method" "tools/list")))))
        (should-not (member "neomacs_eval" names))
        (should (member "neomacs_identity" names))
        (should (member "neomacs_buffer_read" names)))
      (let ((failure
             (should-error
              (neomacs-mcp--dispatch
               peer (neomacs-mcp--object
                     "method" "tools/call" "params"
                     (neomacs-mcp--object
                      "name" "neomacs_eval" "arguments"
                      (neomacs-mcp-test--args "code" "(setq neomacs-mcp-test-effect t)"))))
              :type 'neomacs-mcp-protocol-error)))
        (should (= -32602 (nth 1 failure))))
      (should-not neomacs-mcp-test-effect))
    (let ((neomacs-mcp-full-access t))
      (should (member "neomacs_eval"
                      (neomacs-mcp-test--tool-names
                       (neomacs-mcp--dispatch
                        peer (neomacs-mcp--object "method" "tools/list"))))))))

(ert-deftest neomacs-mcp-test-instance-checked-before-eval ()
  (let ((neomacs-mcp-test-effect nil))
    (should-error
     (neomacs-mcp--eval
      (neomacs-mcp--object "instance" "wrong"
                           "code" "(setq neomacs-mcp-test-effect t)")))
    (should-not neomacs-mcp-test-effect)))

(ert-deftest neomacs-mcp-test-eval-all-forms-and-unicode ()
  (should (equal "\"Ελληνικά\\n42\""
                 (neomacs-mcp--eval
                  (neomacs-mcp-test--args
                   "code" "(setq neomacs-mcp-test-effect 42) (format \"Ελληνικά\\n%s\" neomacs-mcp-test-effect)")))))

(ert-deftest neomacs-mcp-test-eval-rejects-unbalanced-code ()
  (let ((neomacs-mcp-test-effect nil))
    (should-error
     (neomacs-mcp--eval
      (neomacs-mcp-test--args "code" ") (setq neomacs-mcp-test-effect t)")))
    (should-not neomacs-mcp-test-effect)))

(ert-deftest neomacs-mcp-test-registry-validation ()
  (let ((neomacs-mcp-tools (copy-sequence neomacs-mcp-tools)))
    (should-error (neomacs-mcp-register-tool "" "Bad" (neomacs-mcp--schema nil nil) #'ignore))
    (should-error (neomacs-mcp-register-tool "bad" "Bad" (neomacs-mcp--object "type" "object") nil))
    (neomacs-mcp-register-tool "x" "One" (neomacs-mcp--schema nil nil) #'ignore)
    (neomacs-mcp-register-tool "x" "Two" (neomacs-mcp--schema nil nil) #'ignore)
    (should (= 1 (cl-count "x" neomacs-mcp-tools :key #'car :test #'equal)))
    (should (equal "Two" (plist-get (alist-get "x" neomacs-mcp-tools nil nil #'equal)
                                    :description)))))

(ert-deftest neomacs-mcp-test-argument-validation ()
  (let ((schema (neomacs-mcp--schema '(("code" . "string")) '("code"))))
    (should-error (neomacs-mcp--arguments schema (neomacs-mcp--object "code" 7)))
    (should-error (neomacs-mcp--arguments schema (neomacs-mcp--object)))
    (should-error (neomacs-mcp--arguments schema (neomacs-mcp--object "code" "ok" "extra" 1)))))

(ert-deftest neomacs-mcp-test-nested-json-unicode ()
  (let ((neomacs-mcp-tools nil))
    (neomacs-mcp-register-tool
     "unicode" "Fixture" (neomacs-mcp--schema nil nil)
     (lambda (_) (neomacs-mcp--object "text" "Ελλάδα\n")))
    (let ((result (neomacs-mcp--call (neomacs-mcp--object "name" "unicode") t)))
      (should (equal "Ελλάδα\n"
                     (gethash "text" (json-parse-string
                                      (gethash "text" (aref (gethash "content" result) 0)))))))))

(ert-deftest neomacs-mcp-test-tool-error-is-result ()
  (let ((neomacs-mcp-tools nil))
    (neomacs-mcp-register-tool "boom" "Fixture" (neomacs-mcp--schema nil nil)
                               (lambda (_) (error "Boom")))
    (neomacs-mcp-register-tool "typed" "Fixture"
                               (neomacs-mcp--schema '(("n" . "integer")) '("n"))
                               (lambda (_) (ert-fail "Ran with invalid arguments")))
    (let ((result (neomacs-mcp--call (neomacs-mcp--object "name" "boom") nil)))
      (should (eq t (gethash "isError" result)))
      (should (equal "Boom" (gethash "text" (aref (gethash "content" result) 0)))))
    ;; Invalid arguments are a tool execution error, not a protocol error.
    (let ((result (neomacs-mcp--call
                   (neomacs-mcp--object "name" "typed" "arguments"
                                        (neomacs-mcp--object "n" "x"))
                   nil)))
      (should (eq t (gethash "isError" result)))
      (should (equal "Wrong argument type: n"
                     (gethash "text" (aref (gethash "content" result) 0)))))
    (should-error (neomacs-mcp--call (neomacs-mcp--object "name" "absent") nil)
                  :type 'neomacs-mcp-protocol-error)))

;;;; Protocol versions

(ert-deftest neomacs-mcp-test-supported-handshake-versions ()
  (dolist (version '("2025-06-18" "2025-11-25"))
    (neomacs-mcp-test--with-peer
      (let ((result (neomacs-mcp-test--initialize peer version)))
        (should (equal version (gethash "protocolVersion" result)))
        (should (hash-table-p (gethash "tools" (gethash "capabilities" result))))
        (should-not (gethash "resultType" result))
        (should (eq 'initializing (process-get peer 'legacy)))))))

(ert-deftest neomacs-mcp-test-unsupported-handshake-counterproposal ()
  (dolist (version '("2024-11-05" "1900-01-01" "2026-07-28"))
    (neomacs-mcp-test--with-peer
      (should (equal "2025-11-25"
                     (gethash "protocolVersion"
                              (neomacs-mcp-test--initialize peer version)))))))

(ert-deftest neomacs-mcp-test-malformed-initialize ()
  (dolist (params
           (list (neomacs-mcp--object "protocolVersion" ""
                                      "capabilities" (neomacs-mcp--object)
                                      "clientInfo" (neomacs-mcp--object))
                 (neomacs-mcp--object "protocolVersion" 20250618
                                      "capabilities" (neomacs-mcp--object)
                                      "clientInfo" (neomacs-mcp--object))
                 (neomacs-mcp--object "protocolVersion" "2025-06-18"
                                      "capabilities" :null
                                      "clientInfo" (neomacs-mcp--object))
                 (neomacs-mcp--object)))
    (neomacs-mcp-test--with-peer
      (let ((failure
             (should-error
              (neomacs-mcp--dispatch
               peer (neomacs-mcp--object "method" "initialize" "params" params))
              :type 'neomacs-mcp-protocol-error)))
        (should (= -32602 (nth 1 failure)))
        (should-not (process-get peer 'legacy))
        ;; A malformed offer does not prevent a later valid handshake.
        (should (equal "2025-06-18"
                       (gethash "protocolVersion"
                                (neomacs-mcp-test--initialize peer "2025-06-18"))))))))

(ert-deftest neomacs-mcp-test-handshake-readiness ()
  (neomacs-mcp-test--with-peer
    (should-error
     (neomacs-mcp--dispatch peer (neomacs-mcp--object "method" "notifications/initialized"))
     :type 'neomacs-mcp-protocol-error)
    (neomacs-mcp-test--initialize peer "2025-06-18")
    (should (= -32600 (nth 1 (should-error
                              (neomacs-mcp--dispatch
                               peer (neomacs-mcp--object "method" "tools/list"))
                              :type 'neomacs-mcp-protocol-error))))
    ;; Ping is allowed while initialization is in progress.
    (should (= 0 (hash-table-count
                  (neomacs-mcp--dispatch peer (neomacs-mcp--object "method" "ping")))))
    (should-error (neomacs-mcp-test--initialize peer "2025-06-18")
                  :type 'neomacs-mcp-protocol-error)
    (neomacs-mcp--dispatch peer (neomacs-mcp--object "method" "notifications/initialized"))
    (should (eq 'ready (process-get peer 'legacy)))
    (should (= 0 (hash-table-count
                  (neomacs-mcp--dispatch peer (neomacs-mcp--object "method" "ping")))))))

(ert-deftest neomacs-mcp-test-legacy-metadata-keeps-era ()
  (neomacs-mcp-test--with-peer
    (process-put peer 'legacy 'ready)
    (dolist (meta (list (neomacs-mcp--object)
                        (neomacs-mcp--object "progressToken" "token")
                        (neomacs-mcp--object "example.com/context" "fixture")))
      (should (= 0 (hash-table-count
                    (neomacs-mcp--dispatch
                     peer (neomacs-mcp--object "method" "ping" "params"
                                               (neomacs-mcp--object "_meta" meta)))))))
    (dolist (key '("io.modelcontextprotocol/protocolVersion"
                   "io.modelcontextprotocol/clientCapabilities"
                   "io.modelcontextprotocol/clientInfo"))
      (should-error
       (neomacs-mcp--dispatch
        peer (neomacs-mcp--object "method" "ping" "params"
                                  (neomacs-mcp--object
                                   "_meta" (neomacs-mcp--object key :null))))
       :type 'neomacs-mcp-protocol-error))))

(ert-deftest neomacs-mcp-test-modern-requests ()
  (neomacs-mcp-test--with-peer
    (let ((discover (neomacs-mcp--dispatch
                     peer (neomacs-mcp--object "method" "server/discover"
                                               "params" (neomacs-mcp-test--modern-params))))
          (ping (neomacs-mcp--dispatch
                 peer (neomacs-mcp--object "method" "ping"
                                           "params" (neomacs-mcp-test--modern-params)))))
      (should (equal ["2026-07-28" "2025-11-25" "2025-06-18"]
                     (gethash "supportedVersions" discover)))
      (should (equal "complete" (gethash "resultType" discover)))
      (should (equal "complete" (gethash "resultType" ping)))
      (should-not (process-get peer 'legacy)))
    (let* ((failure
            (should-error
             (neomacs-mcp--modern-p
              (neomacs-mcp--object
               "_meta" (neomacs-mcp--object
                        "io.modelcontextprotocol/protocolVersion" "1900-01-01"
                        "io.modelcontextprotocol/clientCapabilities" (neomacs-mcp--object))))
             :type 'neomacs-mcp-protocol-error))
           (data (nth 3 failure)))
      (should (= -32022 (nth 1 failure)))
      (should (equal "1900-01-01" (gethash "requested" data))))))

;;;; Framing and scheduling

(ert-deftest neomacs-mcp-test-filter-never-runs-tools ()
  (let ((calls nil))
    (cl-letf (((symbol-function 'neomacs-mcp--enqueue)
               (lambda (_peer message &optional failure) (push (or failure message) calls)))
              ((symbol-function 'neomacs-mcp--dispatch)
               (lambda (&rest _) (ert-fail "Filter ran a tool"))))
      (neomacs-mcp--frame nil (encode-coding-string
                               "{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"tools/list\"}" 'utf-8))
      (neomacs-mcp--frame nil "{bad}")
      ;; An invalid request keeps a readable ID for its error reply.
      (neomacs-mcp--frame nil "{\"jsonrpc\":\"2.0\",\"id\":5}")
      ;; Notifications other than initialized are ignored.
      (neomacs-mcp--frame nil "{\"jsonrpc\":\"2.0\",\"method\":\"tools/call\",\"params\":{}}")
      (should (= 3 (length calls)))
      (should (equal '(-32600 "Invalid request") (car calls)))
      (should (= -32700 (car (nth 1 calls)))))))

(ert-deftest neomacs-mcp-test-drain-one-request-and-reentry-guard ()
  (let* ((neomacs-mcp--active nil) (neomacs-mcp--timer nil)
         (neomacs-mcp--queue (mapcar (lambda (id) (neomacs-mcp-test--request 'peer id))
                                     '(1 2 3)))
         (calls 0) (scheduled 0))
    (cl-letf (((symbol-function 'neomacs-mcp--live-p) (lambda (&rest _) t))
              ((symbol-function 'input-pending-p) (lambda (&optional _) nil))
              ((symbol-function 'neomacs-mcp--send) #'ignore)
              ((symbol-function 'neomacs-mcp--schedule) (lambda () (cl-incf scheduled)))
              ((symbol-function 'neomacs-mcp--dispatch)
               (lambda (&rest _)
                 (cl-incf calls)
                 (neomacs-mcp--drain)   ; Reentry must not run another request.
                 (neomacs-mcp--object))))
      (neomacs-mcp--drain)
      (should (= 1 calls))
      (should (= 2 (length neomacs-mcp--queue)))
      (should (= 1 scheduled))
      (should-not neomacs-mcp--active))))

(ert-deftest neomacs-mcp-test-pending-input-defers-requests ()
  (let* ((neomacs-mcp--active nil) (neomacs-mcp--timer nil)
         (neomacs-mcp--queue (list (neomacs-mcp-test--request 'a 1)))
         (unread-command-events '(?x)) (calls 0) (timers nil))
    (cl-letf (((symbol-function 'run-at-time)
               (lambda (&rest args) (push args timers) 'fixture-timer))
              ((symbol-function 'neomacs-mcp--live-p) (lambda (&rest _) t))
              ((symbol-function 'neomacs-mcp--send) #'ignore)
              ((symbol-function 'neomacs-mcp--dispatch)
               (lambda (&rest _) (cl-incf calls) (neomacs-mcp--object))))
      (neomacs-mcp--drain)
      (should (= calls 0))
      (should (equal unread-command-events '(?x)))
      (should (equal timers '((0.01 nil neomacs-mcp--drain))))
      (setq unread-command-events nil neomacs-mcp--timer nil)
      (neomacs-mcp--drain)
      (should (= calls 1))
      (should-not neomacs-mcp--queue))))

(ert-deftest neomacs-mcp-test-cancelled-and-stale-requests-skipped ()
  (let* ((neomacs-mcp--active nil) (neomacs-mcp--timer nil)
         (neomacs-mcp--generation 7) (neomacs-mcp--peers '(a b))
         (neomacs-mcp--queue (list (neomacs-mcp-test--request 'a 1)
                                   (neomacs-mcp-test--request 'a 2 6)
                                   (neomacs-mcp-test--request 'dead 3)
                                   (neomacs-mcp-test--request 'b 1)))
         (calls nil))
    (cl-letf (((symbol-function 'process-live-p) (lambda (_) t))
              ((symbol-function 'input-pending-p)
               (lambda (&optional _)
                 (neomacs-mcp--frame
                  'a "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/cancelled\",\"params\":{\"requestId\":1}}")
                 nil))
              ((symbol-function 'neomacs-mcp--schedule) #'ignore)
              ((symbol-function 'neomacs-mcp--send) #'ignore)
              ((symbol-function 'neomacs-mcp--dispatch)
               (lambda (peer _) (push peer calls) (neomacs-mcp--object))))
      (neomacs-mcp--drain)
      (should (equal calls '(b)))
      (should-not neomacs-mcp--queue))))

(ert-deftest neomacs-mcp-test-queue-limits ()
  (let ((neomacs-mcp--queue (make-list neomacs-mcp--peer-queue-limit (list :peer 'peer)))
        (closed nil))
    (cl-letf (((symbol-function 'neomacs-mcp--close) (lambda (peer) (setq closed peer))))
      (neomacs-mcp--enqueue 'peer (neomacs-mcp--object))
      (should (eq closed 'peer))
      (should (= neomacs-mcp--peer-queue-limit (length neomacs-mcp--queue))))))

;;;; Endpoint lifecycle

(defmacro neomacs-mcp-test--with-root (&rest body)
  "Run BODY with `root' bound to a fresh private directory."
  (declare (indent 0) (debug t))
  `(let ((root (make-temp-file "neomacs-mcp-test-" t)))
     (unwind-protect (progn (set-file-modes root #o700) ,@body)
       (neomacs-mcp-stop)
       (delete-directory root t))))

(ert-deftest neomacs-mcp-test-start-stop-and-owned-socket ()
  (neomacs-mcp-test--with-root
    (let ((socket (expand-file-name "mcp" root))
          (boot (neomacs-mcp-test--instance)))
      (neomacs-mcp-start socket)
      (should (file-exists-p socket))
      (should-error (neomacs-mcp-start socket))
      (neomacs-mcp-stop)
      (should-not (file-exists-p socket))
      (neomacs-mcp-start socket)
      (should (equal boot (neomacs-mcp-test--instance)))
      ;; A file that replaced the socket is not removed on stop.
      (delete-file socket)
      (with-temp-file socket (insert "successor"))
      (neomacs-mcp-stop)
      (should (file-exists-p socket)))))

(ert-deftest neomacs-mcp-test-refuse-existing-node-and-unsafe-dir ()
  (neomacs-mcp-test--with-root
    (let ((socket (expand-file-name "mcp" root)))
      (with-temp-file socket (insert "not a socket"))
      (should-error (neomacs-mcp-start socket))
      (delete-file socket)
      (make-symbolic-link (expand-file-name "absent" root) socket)
      (should-error (neomacs-mcp-start socket))
      (delete-file socket)
      (should-error (neomacs-mcp-start "relative/mcp"))
      (set-file-modes root #o777)
      (should-error (neomacs-mcp-start socket))
      (should-not neomacs-mcp--listener))))

(defun neomacs-mcp-test--exchange (process output messages)
  "Send MESSAGES to PROCESS and return the responses parsed from OUTPUT.
OUTPUT is a cons whose car accumulates received text.  Notifications
receive no response."
  (dolist (message messages)
    (process-send-string process (concat (json-serialize message) "\n")))
  (let ((expected (cl-count-if (lambda (m) (gethash "id" m)) messages))
        (deadline (+ (float-time) 10)))
    (while (and (< (cl-count ?\n (car output)) expected)
                (< (float-time) deadline))
      (accept-process-output nil 0.05))
    (prog1 (mapcar (lambda (line)
                     (json-parse-string line :false-object :false :null-object :null))
                   (split-string (car output) "\n" t))
      (setcar output ""))))

(defun neomacs-mcp-test--session (process output)
  "Exercise a full legacy MCP session over PROCESS reading OUTPUT."
  (let* ((instance (neomacs-mcp-test--instance))
         (init (neomacs-mcp-test--exchange
                process output
                (list (neomacs-mcp--object
                       "jsonrpc" "2.0" "id" 1 "method" "initialize" "params"
                       (neomacs-mcp--object "protocolVersion" "2025-06-18"
                                            "capabilities" (neomacs-mcp--object)
                                            "clientInfo" (neomacs-mcp--object
                                                          "name" "test" "version" "1"))))))
         (rest (neomacs-mcp-test--exchange
                process output
                (list (neomacs-mcp--object "jsonrpc" "2.0"
                                           "method" "notifications/initialized")
                      (neomacs-mcp--object "jsonrpc" "2.0" "id" 2 "method" "tools/list")
                      (neomacs-mcp--object
                       "jsonrpc" "2.0" "id" "three" "method" "tools/call" "params"
                       (neomacs-mcp--object
                        "name" "neomacs_eval" "arguments"
                        (neomacs-mcp--object "instance" instance
                                             "code" "(setq neomacs-mcp-test-effect 'ok) (* 6 7)")))))))
    (should (equal "2025-06-18"
                   (gethash "protocolVersion" (gethash "result" (car init)))))
    (should (= 2 (length rest)))
    (should (equal 2 (gethash "id" (nth 0 rest))))
    (should (member "neomacs_eval"
                    (neomacs-mcp-test--tool-names (gethash "result" (nth 0 rest)))))
    (should (equal "three" (gethash "id" (nth 1 rest))))
    (let ((result (gethash "result" (nth 1 rest))))
      (should (eq :false (gethash "isError" result)))
      (should (equal "42" (gethash "text" (aref (gethash "content" result) 0)))))
    (should (eq neomacs-mcp-test-effect 'ok))))

(ert-deftest neomacs-mcp-test-socket-round-trip ()
  (neomacs-mcp-test--with-root
    (let* ((socket (expand-file-name "mcp" root))
           (neomacs-mcp-test-effect nil)
           (output (list ""))
           client)
      (neomacs-mcp-start socket)
      (should (= #o700 (logand #o777 (file-modes root))))
      (setq client (make-network-process
                    :name "neomacs-mcp-test-client" :family 'local :service socket
                    :coding 'utf-8 :noquery t
                    :filter (lambda (_ chunk) (setcar output (concat (car output) chunk)))))
      (unwind-protect
          (neomacs-mcp-test--session client output)
        (delete-process client)))))

(ert-deftest neomacs-mcp-test-unsendable-response-is-error-reply ()
  ;; ESC prints raw but JSON-escapes to 6 bytes: 30000 of them print to
  ;; about 30 KB but encode to about 180 KB, over the response limit.  A
  ;; raw byte, as in undecodable process output, cannot be encoded at all.
  (neomacs-mcp-test--with-root
    (let* ((socket (expand-file-name "mcp" root))
           (output (list ""))
           (instance (neomacs-mcp-test--instance))
           (neomacs-mcp-tools (copy-sequence neomacs-mcp-tools))
           client)
      (neomacs-mcp-register-tool
       "raw" "Fixture" (neomacs-mcp--schema '(("instance" . "string")) nil)
       (lambda (_) (string ?a (unibyte-char-to-multibyte 200) ?b)))
      (neomacs-mcp-start socket)
      (setq client (make-network-process
                    :name "neomacs-mcp-test-client" :family 'local :service socket
                    :coding 'utf-8 :noquery t
                    :filter (lambda (_ chunk) (setcar output (concat (car output) chunk)))))
      (unwind-protect
          (let* ((call (lambda (id name arguments)
                         (puthash "instance" instance arguments)
                         (neomacs-mcp--object
                          "jsonrpc" "2.0" "id" id "method" "tools/call" "params"
                          (neomacs-mcp-test--modern-params
                           "name" name "arguments" arguments))))
                 (eval (lambda (id code)
                         (funcall call id "neomacs_eval" (neomacs-mcp--object "code" code)))))
            (dolist (case (list (funcall eval 1 "(make-string 30000 27)")
                                (funcall call 2 "raw" (neomacs-mcp--object))))
              (let ((replies (neomacs-mcp-test--exchange client output (list case))))
                (should (= 1 (length replies)))
                (should (equal (gethash "id" case) (gethash "id" (car replies))))
                (should (= -32603 (gethash "code" (gethash "error" (car replies)))))))
            ;; The connection stays usable.
            (let ((next (neomacs-mcp-test--exchange
                         client output (list (funcall eval 3 "(* 6 7)")))))
              (should (= 1 (length next)))
              (should (equal "42" (gethash "text" (aref (gethash "content"
                                                                 (gethash "result" (car next)))
                                                        0))))))
        (delete-process client)))))

(defun neomacs-mcp-test--raw-replies (process output line barrier)
  "Send LINE to PROCESS, then after its first reply a ping with ID BARRIER.
Return every reply line in OUTPUT up to and including the ping's reply,
without newlines, or nil if that reply does not arrive.  Requests run
in order, so the lines before the last are the replies to LINE.  The
ping is sent separately because LINE alone may fill the input limit.
Clear OUTPUT afterwards."
  (let ((ping (neomacs-mcp--object "jsonrpc" "2.0" "id" barrier "method" "ping"))
        (done (concat "\"id\":" (json-serialize barrier)))
        (deadline (+ (float-time) 10)))
    (process-send-string process line)
    (while (and (not (string-search "\n" (car output)))
                (process-live-p process)
                (< (float-time) deadline))
      (accept-process-output nil 0.05))
    (when (string-search "\n" (car output))
      (process-send-string process (concat (json-serialize ping) "\n"))
      (while (and (not (and (string-search done (car output))
                            (string-suffix-p "\n" (car output))))
                  (process-live-p process)
                  (< (float-time) deadline))
        (accept-process-output nil 0.05)))
    (prog1 (and (string-search done (car output))
                (string-suffix-p "\n" (car output))
                (split-string (car output) "\n" t))
      (setcar output ""))))

(ert-deftest neomacs-mcp-test-near-limit-id-reply-stays-bounded ()
  ;; A request just under the input limit whose ID alone is near that
  ;; limit must not produce a reply over the output limit.  The ID is
  ;; not echoed, and the connection stays usable.
  (neomacs-mcp-test--with-root
    (let* ((socket (expand-file-name "mcp" root))
           (output (list ""))
           client)
      (neomacs-mcp-start socket)
      (setq client (make-network-process
                    :name "neomacs-mcp-test-client" :family 'local :service socket
                    :coding 'utf-8 :noquery t
                    :filter (lambda (_ chunk) (setcar output (concat (car output) chunk)))))
      (unwind-protect
          (progn
            (neomacs-mcp-test--exchange
             client output
             (list (neomacs-mcp--object
                    "jsonrpc" "2.0" "id" 1 "method" "initialize" "params"
                    (neomacs-mcp--object "protocolVersion" "2025-06-18"
                                         "capabilities" (neomacs-mcp--object)
                                         "clientInfo" (neomacs-mcp--object
                                                       "name" "test" "version" "1")))
                   (neomacs-mcp--object "jsonrpc" "2.0"
                                        "method" "notifications/initialized")))
            (let ((barrier 0))
              (dolist (id (list (make-string 131000 ?x)
                                (json-parse-string (make-string 131000 ?9))))
                ;; A valid request, and one rejected as invalid (no method).
                (dolist (message (list (neomacs-mcp--object
                                        "jsonrpc" "2.0" "id" id "method" "tools/list")
                                       (neomacs-mcp--object "jsonrpc" "2.0" "id" id)))
                  (let* ((line (concat (json-serialize message) "\n"))
                         (lines (progn
                                  (should (<= (string-bytes line) neomacs-mcp--frame-limit))
                                  (neomacs-mcp-test--raw-replies
                                   client output line
                                   (format "barrier-%d" (cl-incf barrier)))))
                         (raw (car lines))
                         (reply (and raw (json-parse-string raw :null-object :null))))
                    ;; Exactly one reply, then the barrier's.
                    (should (= 2 (length lines)))
                    (should (<= (1+ (string-bytes raw)) neomacs-mcp--output-limit))
                    (should (eq :null (gethash "id" reply)))
                    (should (= -32600 (gethash "code" (gethash "error" reply))))))))
            (let ((next (neomacs-mcp-test--exchange
                         client output
                         (list (neomacs-mcp--object
                                "jsonrpc" "2.0" "id" 2 "method" "tools/list")))))
              (should (= 1 (length next)))
              (should (equal 2 (gethash "id" (car next))))
              (should (gethash "tools" (gethash "result" (car next))))))
        (delete-process client)))))

(ert-deftest neomacs-mcp-test-id-limit-boundary ()
  ;; The quotes count: a 1022-character string ID encodes to 1024 bytes.
  (should (neomacs-mcp--id-fits-p (make-string 1022 ?x)))
  (should-not (neomacs-mcp--id-fits-p (make-string 1023 ?x)))
  (should (neomacs-mcp--id-fits-p (json-parse-string (make-string 1024 ?9))))
  (should-not (neomacs-mcp--id-fits-p (json-parse-string (make-string 1025 ?9)))))

(ert-deftest neomacs-mcp-test-wire-never-exceeds-the-output-limit ()
  ;; Admission bounds IDs, but the encoder must hold the limit on its
  ;; own: an oversized or unencodable response with an oversized ID
  ;; still yields one bounded error line.
  (let ((id (make-string neomacs-mcp--output-limit ?x)))
    (dolist (result (list (make-string neomacs-mcp--output-limit ?y)
                          (string ?a (unibyte-char-to-multibyte 200))))
      (let* ((wire (neomacs-mcp--wire
                    (neomacs-mcp--object "jsonrpc" "2.0" "id" id "result" result)))
             (reply (json-parse-string wire :null-object :null)))
        (should (<= (string-bytes wire) neomacs-mcp--output-limit))
        (should (string-suffix-p "\n" wire))
        (should (eq :null (gethash "id" reply)))
        (should (= -32603 (gethash "code" (gethash "error" reply))))))
    ;; A short ID is still echoed in the replacement error.
    (let ((reply (json-parse-string
                  (neomacs-mcp--wire
                   (neomacs-mcp--object
                    "jsonrpc" "2.0" "id" 7
                    "result" (make-string neomacs-mcp--output-limit ?y))))))
      (should (equal 7 (gethash "id" reply)))
      (should (= -32603 (gethash "code" (gethash "error" reply)))))))

(ert-deftest neomacs-mcp-test-relay-round-trip ()
  (let ((relay (getenv "NEOMACS_MCP_RELAY")))
    (skip-unless (and relay (file-executable-p relay)))
    (neomacs-mcp-test--with-root
      (let* ((socket (expand-file-name "mcp" root))
             (neomacs-mcp-test-effect nil)
             (output (list ""))
             process)
        (neomacs-mcp-start socket)
        (setq process (make-process
                       :name "neomacs-mcp-test-relay" :command (list relay "--socket" socket)
                       :connection-type 'pipe :coding 'utf-8 :noquery t
                       :stderr (get-buffer-create " *neomacs-mcp-test-relay*")
                       :filter (lambda (_ chunk) (setcar output (concat (car output) chunk)))))
        (unwind-protect
            (progn
              (neomacs-mcp-test--session process output)
              ;; Closing the relay's stdin ends the relay, not the editor.
              (process-send-eof process)
              (let ((deadline (+ (float-time) 10)))
                (while (and (process-live-p process) (< (float-time) deadline))
                  (accept-process-output nil 0.05)))
              (should-not (process-live-p process))
              (should (file-exists-p socket)))
          (when (process-live-p process) (delete-process process)))))))

;;;; Buffer tools

(ert-deftest neomacs-mcp-test-buffer-read-range-and-preserved-state ()
  (let ((buffer (generate-new-buffer "mcp-read-fixture"))
        (current (current-buffer)))
    (unwind-protect
        (with-current-buffer buffer
          (insert (propertize "α\n\"\\β終🙂text" 'face 'bold))
          (buffer-enable-undo)
          (goto-char 5)
          (narrow-to-region 3 9)
          (let* ((point (point)) (minimum (point-min)) (maximum (point-max))
                 (undo buffer-undo-list) (tick (buffer-modified-tick))
                 (value (neomacs-mcp--buffer-read
                         (neomacs-mcp-test--args "name" (buffer-name) "start" 1
                                                 "maxChars" 5 "expectedTick" tick))))
            (should (equal (gethash "text" value) "α\n\"\\β"))
            (should-not (text-properties-at 0 (gethash "text" value)))
            (should (= (gethash "end" value) 6))
            (should (= (gethash "nextStart" value) 6))
            (should (eq (gethash "truncated" value) t))
            (should (= point (point)))
            (should (= minimum (point-min)))
            (should (= maximum (point-max)))
            (should (eq undo buffer-undo-list))
            (should (= tick (buffer-modified-tick)))))
      (kill-buffer buffer))
    (should (eq current (current-buffer)))))

(ert-deftest neomacs-mcp-test-buffer-read-validation-tick-and-eob ()
  (with-temp-buffer
    (rename-buffer "mcp-validation-fixture" t)
    (insert "abc")
    (let ((args (neomacs-mcp-test--args "name" (buffer-name) "start" 1 "maxChars" 4)))
      (dolist (pair '(("instance" . "wrong") ("name" . "absent-mcp-fixture")
                      ("start" . 0) ("start" . 5) ("maxChars" . 0)
                      ("maxChars" . 4097) ("expectedTick" . -1)))
        (let ((bad (copy-hash-table args)))
          (puthash (car pair) (cdr pair) bad)
          (should-error (neomacs-mcp--buffer-read bad))))
      (puthash "expectedTick" (buffer-modified-tick) args)
      (insert "d")
      (should-error (neomacs-mcp--buffer-read args))
      (remhash "expectedTick" args)
      (puthash "start" (point-max) args)
      (let ((value (neomacs-mcp--buffer-read args)))
        (should (equal "" (gethash "text" value)))
        (should (eq :false (gethash "truncated" value)))
        (should (eq :null (gethash "nextStart" value)))))))

(ert-deftest neomacs-mcp-test-buffer-read-output-limit ()
  (with-temp-buffer
    (rename-buffer "mcp-output-fixture" t)
    ;; Control characters expand to six bytes each when JSON-escaped twice.
    (insert (make-string 100000 ?x) (make-string 4096 1) "🙂")
    (let* ((neomacs-mcp-buffer-output-limit 2048)
           (value (neomacs-mcp--buffer-read
                   (neomacs-mcp-test--args "name" (buffer-name) "start" 100001
                                           "maxChars" 4096)))
           (text (gethash "text" value)))
      (should (< 0 (length text) 4096))
      (should (<= (neomacs-mcp--result-bytes value) neomacs-mcp-buffer-output-limit))
      (should (= (gethash "end" value) (+ 100001 (length text))))
      (should (equal text (make-string (length text) 1))))))

(ert-deftest neomacs-mcp-test-buffer-list-pages ()
  (let ((buffers (cl-loop for n below 40
                          collect (generate-new-buffer (format "mcp-list-%s" n)))))
    (unwind-protect
        (cl-letf (((symbol-function 'buffer-list) (lambda (&rest _) buffers)))
          (let* ((value (neomacs-mcp--buffer-list
                         (neomacs-mcp-test--args "offset" 0 "limit" 2)))
                 (entries (gethash "buffers" value)))
            (should (= 2 (length entries)))
            (should (= 2 (gethash "nextOffset" value)))
            (should (eq t (gethash "truncated" value)))
            (should (equal (gethash "name" (aref entries 0)) (buffer-name (car buffers)))))
          (let ((value (neomacs-mcp--buffer-list
                        (neomacs-mcp-test--args "offset" 38 "limit" 32))))
            (should (= 2 (length (gethash "buffers" value))))
            (should (eq :null (gethash "nextOffset" value))))
          (dolist (pair '(("offset" . -1) ("limit" . 0) ("limit" . 33)))
            (let ((args (neomacs-mcp-test--args "offset" 0 "limit" 1)))
              (puthash (car pair) (cdr pair) args)
              (should-error (neomacs-mcp--buffer-list args)))))
      (mapc #'kill-buffer buffers))))

(ert-deftest neomacs-mcp-test-buffer-tools-hide-credentials-and-minibuffers ()
  (let* ((secret (generate-new-buffer "mcp-secret-fixture"))
         (alias (make-indirect-buffer secret "mcp-secret-alias" nil)))
    (unwind-protect
        (progn
          (with-current-buffer secret (setq buffer-file-name "/fixture/.authinfo.gpg"))
          (dolist (buffer (list secret alias (window-buffer (minibuffer-window))))
            (should (neomacs-mcp--buffer-hidden-p buffer))
            (should-error (neomacs-mcp--buffer-read
                           (neomacs-mcp-test--args "name" (buffer-name buffer)
                                                   "start" 1 "maxChars" 1))))
          (cl-letf (((symbol-function 'buffer-list) (lambda (&rest _) (make-list 200 secret))))
            (let ((value (neomacs-mcp--buffer-list
                          (neomacs-mcp-test--args "offset" 0 "limit" 1))))
              (should (= 0 (length (gethash "buffers" value))))
              (should (= neomacs-mcp-buffer-scan-limit (gethash "scanned" value))))))
      (kill-buffer alias)
      (with-current-buffer secret (setq buffer-file-name nil))
      (kill-buffer secret))))

(provide 'neomacs-mcp-test)
;;; neomacs-mcp-test.el ends here

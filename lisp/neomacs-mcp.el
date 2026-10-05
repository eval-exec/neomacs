;;; neomacs-mcp.el --- Local MCP endpoint for agent access -*- lexical-binding: t; -*-

;; Copyright (C) 2026 Free Software Foundation, Inc.

;; Author: Neomacs Contributors
;; Keywords: tools, processes

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

;; A Model Context Protocol (MCP) endpoint that gives a local agent
;; access to the running editor.  It listens on an explicitly named,
;; owner-private Unix socket, separate from server.el; loading this
;; library starts nothing.  Messages are UTF-8 JSON-RPC 2.0, one per
;; line.  Standard stdio MCP clients reach the socket through the
;; protocol-blind `neomacs-mcp' relay program.
;;
;;   (require 'neomacs-mcp)
;;   (neomacs-mcp-start "/run/user/1000/neomacs/mcp")
;;
;; Built-in tools: `neomacs_identity', `neomacs_buffer_list',
;; `neomacs_buffer_read' and, while `neomacs-mcp-full-access' is
;; non-nil (the default), `neomacs_eval', which evaluates arbitrary Lisp
;; with the user's privileges.
;;
;; Process filters only frame and enqueue requests.  A timer admits at
;; most one request at a time, and only while no user input is pending.
;; This library is plain Emacs Lisp and also runs on GNU Emacs.  See
;; docs/neomacs-mcp.md.

;;; Code:

(require 'cl-lib)
(require 'json)
(require 'rx)
(require 'server)

(defgroup neomacs-mcp nil
  "Local Model Context Protocol endpoint."
  :group 'external)

(defcustom neomacs-mcp-full-access t
  "Non-nil means MCP clients may evaluate arbitrary Lisp in this editor.
When non-nil, the `neomacs_eval' tool is listed and callable.  It runs
code with the same privileges as the user running the editor, without
a sandbox or confirmation prompt.  When nil, that tool is neither
listed nor callable; the other built-in tools only read buffer text and
metadata.  Tools registered by other libraries are not affected."
  :type 'boolean
  :group 'neomacs-mcp)

(defcustom neomacs-mcp-send-timeout 0.25
  "Seconds before closing a client whose response send has not returned."
  :type 'number
  :group 'neomacs-mcp)

(defconst neomacs-mcp--frame-limit 131072)
(defconst neomacs-mcp--output-limit 131072)
(defconst neomacs-mcp--queue-limit 64)
(defconst neomacs-mcp--peer-queue-limit 16)
(defconst neomacs-mcp--peer-limit 8)
(defconst neomacs-mcp--scan-limit 8)
(defconst neomacs-mcp--modern "2026-07-28")
(defconst neomacs-mcp--legacy "2025-11-25")
(defconst neomacs-mcp--legacy-versions '("2025-11-25" "2025-06-18")
  "Supported handshake versions, newest first; both use legacy envelopes.")
(defconst neomacs-mcp--full-access-tools '("neomacs_eval")
  "Tools available only while `neomacs-mcp-full-access' is non-nil.")

(defvar neomacs-mcp--boot nil)
(defvar neomacs-mcp--generation 0)
(defvar neomacs-mcp--listener nil)
(defvar neomacs-mcp--socket nil)
(defvar neomacs-mcp--socket-identity nil)
(defvar neomacs-mcp--peers nil)
(defvar neomacs-mcp--queue nil)
(defvar neomacs-mcp--active nil)
(defvar neomacs-mcp--timer nil)
(defvar neomacs-mcp-tools nil
  "Alist of tool NAME and tool plist.
Each plist has :description, :schema, :handler and optional
:annotations.  Use `neomacs-mcp-register-tool' to add or replace an
entry.  Changes are visible on the next list request.")

(define-error 'neomacs-mcp-protocol-error "MCP protocol error")

(defun neomacs-mcp--object (&rest pairs)
  "Return a JSON object populated by alternating key/value PAIRS."
  (let ((object (make-hash-table :test #'equal)))
    (while pairs (puthash (pop pairs) (pop pairs) object))
    object))

(defun neomacs-mcp--fail (code message &optional data)
  "Signal protocol CODE with MESSAGE and optional DATA."
  (signal 'neomacs-mcp-protocol-error (list code message data)))

(defun neomacs-mcp-identity ()
  "Return this editor process's identity as a JSON object.
The `instance' string stays the same across endpoint restarts and
differs for every editor process.  Tools that act on the editor require
it, so a client cannot silently act on a different editor."
  (unless neomacs-mcp--boot
    (setq neomacs-mcp--boot
          (format "%s:%s:%s" (emacs-pid) (float-time) (random))))
  (neomacs-mcp--object "instance" neomacs-mcp--boot "pid" (emacs-pid)
                       "runtime" emacs-version "serverName" server-name
                       "endpointGeneration" neomacs-mcp--generation))

(defun neomacs-mcp--instance (arguments)
  "Refuse unless ARGUMENTS name this editor process's instance."
  (unless (equal (gethash "instance" arguments)
                 (gethash "instance" (neomacs-mcp-identity)))
    (user-error "Editor instance mismatch")))

(defun neomacs-mcp-register-tool (name description schema handler &optional annotations)
  "Register tool NAME with DESCRIPTION, input SCHEMA and HANDLER.
SCHEMA is a JSON object schema as a hash table.  HANDLER receives the
arguments as a hash table and returns a JSON value; a string is used
directly as text content.  Optional ANNOTATIONS are descriptive hints,
not permissions.  Registering an existing NAME replaces it."
  (unless (and (stringp name) (string-match-p "\\`[A-Za-z0-9_.-]+\\'" name)
               (<= (length name) 128) (stringp description)
               (hash-table-p schema) (equal (gethash "type" schema) "object")
               (functionp handler))
    (error "Invalid MCP tool registration"))
  (setf (alist-get name neomacs-mcp-tools nil nil #'equal)
        (list :description description :schema schema :handler handler
              :annotations annotations)))

(defun neomacs-mcp--available-tools ()
  "Return the registered tools that clients may currently list and call."
  (if neomacs-mcp-full-access
      neomacs-mcp-tools
    (cl-remove-if (lambda (entry) (member (car entry) neomacs-mcp--full-access-tools))
                  neomacs-mcp-tools)))

(defun neomacs-mcp--schema (properties required)
  "Return a closed object schema from PROPERTIES and REQUIRED field names."
  (neomacs-mcp--object
   "type" "object" "properties"
   (apply #'neomacs-mcp--object
          (cl-loop for (name . type) in properties
                   append (list name (neomacs-mcp--object "type" type))))
   "required" (vconcat required) "additionalProperties" :false))

(defun neomacs-mcp--arguments (schema arguments)
  "Validate ARGUMENTS against the supported subset of SCHEMA.
Check required fields, primitive types and closed properties.  Handlers
own any further constraints."
  (unless (hash-table-p arguments)
    (neomacs-mcp--fail -32602 "Arguments must be an object"))
  (mapc (lambda (key)
          (when (eq (gethash key arguments :absent) :absent)
            (neomacs-mcp--fail -32602 (concat "Missing argument: " key))))
        (gethash "required" schema))
  (maphash
   (lambda (key value)
     (let* ((property (gethash key (gethash "properties" schema)))
            (type (and property (gethash "type" property))))
       (when (and (not property) (eq (gethash "additionalProperties" schema) :false))
         (neomacs-mcp--fail -32602 (concat "Unknown argument: " key)))
       (unless (pcase type
                 ("string" (stringp value)) ("integer" (integerp value))
                 ("number" (numberp value)) ("object" (hash-table-p value))
                 ("array" (vectorp value))
                 ("boolean" (memq value '(t :false))) (_ t))
         (neomacs-mcp--fail -32602 (concat "Wrong argument type: " key)))))
   arguments)
  arguments)

(defun neomacs-mcp--eval (arguments)
  "Evaluate the Lisp forms in ARGUMENTS after checking the instance.
Return the printed value.  Effects are not rolled back on failure."
  (neomacs-mcp--instance arguments)
  (let* ((code (gethash "code" arguments))
         (wrapped (concat "(progn\n" code "\n)"))
         (parsed (read-from-string wrapped))
         (_complete
          (unless (= (cdr parsed) (length wrapped))
            (error "Malformed Lisp input: unbalanced forms")))
         (value (eval (car parsed) t))
         (print-length 64) (print-level 16) (print-circle t)
         (print-escape-newlines t)
         (text (prin1-to-string value)))
    (if (> (string-bytes text) 65536)
        (error "Eval result exceeds the output limit; effects may have occurred")
      text)))

;;;; Read-only buffer tools

(defconst neomacs-mcp-buffer-output-limit 32768
  "Maximum encoded tool-result bytes for the buffer tools.")
(defconst neomacs-mcp-buffer-scan-limit 128
  "Maximum `buffer-list' entries examined per `neomacs_buffer_list' call.")
(defconst neomacs-mcp--buffer-name-limit 256)

(defun neomacs-mcp--credential-name-p (name)
  "Return non-nil if NAME looks like an authinfo, netrc or password-store file."
  (and (stringp name)
       (let ((case-fold-search t))
         (string-match-p
          (rx (or string-start "/")
              (or (seq (optional (any "._")) (or "authinfo" "netrc")
                       (or string-end "." "~" "<"))
                  (seq (optional ".") "password-store" (or string-end "/"))))
          name))))

(defun neomacs-mcp--buffer-hidden-p (buffer)
  "Return non-nil if the buffer tools must not expose BUFFER.
Hide minibuffers and buffers whose name or file name looks like a
credential store, including indirect buffers of those.  This is a
lexical filter for the read-only tools, not a sandbox."
  (with-current-buffer buffer
    (or (minibufferp buffer)
        (cl-some #'neomacs-mcp--credential-name-p
                 (list (buffer-name) buffer-file-name buffer-file-truename))
        (when-let* ((base (buffer-base-buffer)))
          (neomacs-mcp--buffer-hidden-p base)))))

(defun neomacs-mcp--integer (arguments key minimum maximum)
  "Return ARGUMENTS' integer KEY within MINIMUM and MAXIMUM, or refuse."
  (let ((value (gethash key arguments)))
    (unless (and (integerp value) (<= minimum value maximum))
      (user-error "Invalid bounded integer: %s" key))
    value))

(defun neomacs-mcp--result-bytes (value)
  "Return the encoded size in bytes of a tool result carrying VALUE."
  (string-bytes
   (json-serialize
    (neomacs-mcp--object
     "resultType" "complete" "isError" :false
     "content" (vector
                (neomacs-mcp--object
                 "type" "text" "text"
                 (decode-coding-string
                  (json-serialize value :false-object :false :null-object :null) 'utf-8))))
    :false-object :false :null-object :null)))

(defun neomacs-mcp--buffer-entry (buffer)
  "Return metadata for BUFFER without scanning lines or running hooks."
  (with-current-buffer buffer
    (let ((mode (symbol-name major-mode)))
      (neomacs-mcp--object
       "name" (buffer-name) "tick" (buffer-modified-tick)
       "sizeChars" (buffer-size) "point" (point)
       "mode" (substring mode 0 (min 128 (length mode)))
       "modeTruncated" (if (> (length mode) 128) t :false)
       "modified" (if (buffer-modified-p) t :false)
       "readOnly" (if buffer-read-only t :false)))))

(defun neomacs-mcp--buffer-list (arguments)
  "Return a page of buffer metadata for ARGUMENTS.
OFFSET counts `buffer-list' entries, including skipped ones.  Internal,
hidden and overlong names are skipped, so a page can be empty."
  (neomacs-mcp--instance arguments)
  (let* ((offset (neomacs-mcp--integer arguments "offset" 0 most-positive-fixnum))
         (limit (neomacs-mcp--integer arguments "limit" 1 32))
         (remaining (nthcdr offset (buffer-list)))
         (scanned 0) (entries nil) (full nil)
         (result (neomacs-mcp--object
                  "instance" (gethash "instance" arguments) "offset" offset
                  "buffers" [] "scanned" 0 "truncated" :false "nextOffset" :null
                  "encodedOutputLimit" neomacs-mcp-buffer-output-limit)))
    (while (and remaining (not full) (< (length entries) limit)
                (< scanned neomacs-mcp-buffer-scan-limit))
      (let* ((buffer (car remaining)) (name (buffer-name buffer))
             (entry (and (buffer-live-p buffer) name
                         (> (length name) 0) (not (eq (aref name 0) ?\s))
                         (<= (length name) neomacs-mcp--buffer-name-limit)
                         (not (neomacs-mcp--buffer-hidden-p buffer))
                         (neomacs-mcp--buffer-entry buffer))))
        (when entry
          ;; Measure with the final pagination fields before accepting it.
          (puthash "buffers" (vconcat (reverse (cons entry entries))) result)
          (puthash "scanned" (1+ scanned) result)
          (puthash "nextOffset" (+ offset scanned 1) result)
          (puthash "truncated" t result)
          (if (> (neomacs-mcp--result-bytes result) neomacs-mcp-buffer-output-limit)
              (setq full t)
            (push entry entries)))
        (unless full
          (setq remaining (cdr remaining))
          (cl-incf scanned))))
    (puthash "buffers" (vconcat (reverse entries)) result)
    (puthash "scanned" scanned result)
    (puthash "truncated" (if remaining t :false) result)
    (puthash "nextOffset" (if remaining (+ offset scanned) :null) result)
    (when (or (and full (= scanned 0))
              (> (neomacs-mcp--result-bytes result) neomacs-mcp-buffer-output-limit))
      (user-error "Buffer metadata exceeds the output limit"))
    result))

(defun neomacs-mcp--buffer-read (arguments)
  "Return a range of text from the buffer named in ARGUMENTS.
Positions are widened, 1-based and end-exclusive.  Preserve the
current buffer, point and narrowing.  When EXPECTEDTICK is supplied,
refuse unless it equals the buffer's modification tick."
  (neomacs-mcp--instance arguments)
  (let* ((name (gethash "name" arguments))
         (count (neomacs-mcp--integer arguments "maxChars" 1 4096))
         (expected (gethash "expectedTick" arguments :absent))
         (buffer (and (stringp name) (> (length name) 0)
                      (<= (length name) neomacs-mcp--buffer-name-limit)
                      (get-buffer name))))
    (unless (and (buffer-live-p buffer) (not (neomacs-mcp--buffer-hidden-p buffer)))
      (user-error "Named buffer is unavailable"))
    (unless (or (eq expected :absent) (and (integerp expected) (>= expected 0)))
      (user-error "Invalid expected modification tick"))
    (with-current-buffer buffer
      (save-excursion
        (save-restriction
          (widen)
          (let* ((tick (buffer-modified-tick))
                 (start (neomacs-mcp--integer arguments "start" 1 (point-max)))
                 (end (min (point-max) (+ start count)))
                 (result (neomacs-mcp--object
                          "instance" (gethash "instance" arguments) "name" (buffer-name)
                          "tick" tick "start" start "end" end "text" ""
                          "truncated" :false "nextStart" :null
                          "encodedOutputLimit" neomacs-mcp-buffer-output-limit)))
            (unless (or (eq expected :absent) (= expected tick))
              (user-error "Buffer modification tick is stale"))
            ;; Halve the range until the encoded result fits.
            (while
                (progn
                  (puthash "text" (buffer-substring-no-properties start end) result)
                  (puthash "end" end result)
                  (puthash "truncated" (if (< end (point-max)) t :false) result)
                  (puthash "nextStart" (if (< end (point-max)) end :null) result)
                  (> (neomacs-mcp--result-bytes result) neomacs-mcp-buffer-output-limit))
              (when (= end start)
                (user-error "Read metadata exceeds the output limit"))
              (setq end (+ start (/ (- end start) 2))))
            (when (and (= end start) (< start (point-max)))
              (user-error "No character fits in the output limit"))
            result))))))

;;;; Protocol

(defun neomacs-mcp--tool-list ()
  "Return sorted descriptors for the available tools."
  (vconcat
   (mapcar (lambda (entry)
             (let* ((tool (cdr entry))
                    (object (neomacs-mcp--object
                             "name" (car entry) "description" (plist-get tool :description)
                             "inputSchema" (plist-get tool :schema))))
               (when (plist-get tool :annotations)
                 (puthash "annotations" (plist-get tool :annotations) object))
               object))
           (sort (copy-sequence (neomacs-mcp--available-tools))
                 (lambda (a b) (string-lessp (car a) (car b)))))))

(defun neomacs-mcp--call (params modern)
  "Call the tool named by PARAMS and return the MODERN or legacy envelope."
  (let* ((tool (alist-get (gethash "name" params) (neomacs-mcp--available-tools)
                          nil nil #'equal))
         (arguments (gethash "arguments" params (neomacs-mcp--object))))
    (unless tool (neomacs-mcp--fail -32602 "Unknown tool"))
    (let* ((failed nil)
           (value (condition-case failure
                      (progn
                        (neomacs-mcp--arguments (plist-get tool :schema) arguments)
                        (with-local-quit (funcall (plist-get tool :handler) arguments)))
                    (neomacs-mcp-protocol-error
                     (setq failed t)
                     (nth 2 failure))
                    ((error quit)
                     (setq failed t)
                     (error-message-string failure))))
           (text (if (stringp value) value
                   (decode-coding-string
                    (json-serialize value :false-object :false :null-object :null)
                    'utf-8)))
           (result (neomacs-mcp--object
                    "content" (vector (neomacs-mcp--object "type" "text" "text" text))
                    "isError" (if failed t :false))))
      (when modern (puthash "resultType" "complete" result))
      result)))

(defun neomacs-mcp--modern-p (params)
  "Validate per-request protocol metadata in PARAMS and return non-nil."
  (let* ((meta (gethash "_meta" params))
         (version (and (hash-table-p meta)
                       (gethash "io.modelcontextprotocol/protocolVersion" meta))))
    (unless (and (stringp version)
                 (hash-table-p (gethash "io.modelcontextprotocol/clientCapabilities" meta)))
      (neomacs-mcp--fail -32602 "Required protocol metadata is missing"))
    (unless (equal version neomacs-mcp--modern)
      (neomacs-mcp--fail -32022 "Unsupported protocol version"
                         (neomacs-mcp--object
                          "supported" (vconcat (list neomacs-mcp--modern)
                                               neomacs-mcp--legacy-versions)
                          "requested" version)))
    t))

(defun neomacs-mcp--dispatch (peer message)
  "Dispatch MESSAGE from PEER outside process filters."
  (let* ((method (gethash "method" message))
         (params (gethash "params" message (neomacs-mcp--object)))
         (meta (gethash "_meta" params))
         ;; Progress tokens and extension metadata do not select an era.
         (modern (and (not (equal method "initialize"))
                      (or (and (hash-table-p meta)
                               (cl-some
                                (lambda (key) (not (eq (gethash key meta :absent) :absent)))
                                '("io.modelcontextprotocol/protocolVersion"
                                  "io.modelcontextprotocol/clientCapabilities"
                                  "io.modelcontextprotocol/clientInfo")))
                          (not (process-get peer 'legacy)))
                      (neomacs-mcp--modern-p params))))
    (pcase method
      ("initialize"
       (let ((version (gethash "protocolVersion" params)))
         (unless (and (stringp version) (> (length version) 0)
                      (hash-table-p (gethash "capabilities" params))
                      (hash-table-p (gethash "clientInfo" params))
                      (not (process-get peer 'legacy)))
           (neomacs-mcp--fail -32602 "Expected fresh initialization"))
         (process-put peer 'legacy 'initializing)
         ;; Echo a supported offer; otherwise propose the newest one.
         (neomacs-mcp--object
          "protocolVersion" (if (member version neomacs-mcp--legacy-versions)
                                version neomacs-mcp--legacy)
          "capabilities" (neomacs-mcp--object "tools" (neomacs-mcp--object))
          "serverInfo" (neomacs-mcp--object "name" "Neomacs" "version" "1"))))
      ("notifications/initialized"
       (unless (eq (process-get peer 'legacy) 'initializing)
         (neomacs-mcp--fail -32600 "Unexpected initialized notification"))
       (process-put peer 'legacy 'ready) nil)
      (_
       (unless (or modern (eq (process-get peer 'legacy) 'ready)
                   (and (equal method "ping") (process-get peer 'legacy)))
         (neomacs-mcp--fail -32600 "Initialization is incomplete"))
       (pcase method
         ("ping" (if modern (neomacs-mcp--object "resultType" "complete")
                   (neomacs-mcp--object)))
         ("server/discover"
          (unless modern (neomacs-mcp--fail -32601 "Method not found"))
          (neomacs-mcp--object
           "resultType" "complete" "supportedVersions"
           (vconcat (list neomacs-mcp--modern) neomacs-mcp--legacy-versions)
           "capabilities" (neomacs-mcp--object "tools" (neomacs-mcp--object))
           "_meta" (neomacs-mcp--object "io.modelcontextprotocol/serverInfo"
                                        (neomacs-mcp--object "name" "Neomacs" "version" "1"))
           "ttlMs" 0 "cacheScope" "private"))
         ("tools/list"
          (when (gethash "cursor" params)
            (neomacs-mcp--fail -32602 "No pagination cursor is supported"))
          (let ((result (neomacs-mcp--object "tools" (neomacs-mcp--tool-list))))
            (when modern
              (puthash "resultType" "complete" result)
              (puthash "ttlMs" 0 result) (puthash "cacheScope" "private" result))
            result))
         ("tools/call" (neomacs-mcp--call params modern))
         (_ (neomacs-mcp--fail -32601 "Method not found")))))))

;;;; Transport and scheduling

(defun neomacs-mcp--live-p (peer generation)
  "Return non-nil if PEER still belongs to endpoint GENERATION."
  (and (= generation neomacs-mcp--generation)
       (memq peer neomacs-mcp--peers) (process-live-p peer)))

(defun neomacs-mcp--close (peer)
  "Close PEER and drop its queued requests and send timer."
  (setq neomacs-mcp--peers (delq peer neomacs-mcp--peers)
        neomacs-mcp--queue
        (cl-remove peer neomacs-mcp--queue :key (lambda (request) (plist-get request :peer))))
  (when-let* ((timer (process-get peer 'send-timer)))
    (cancel-timer timer) (process-put peer 'send-timer nil))
  (when (and neomacs-mcp--active (eq peer (plist-get neomacs-mcp--active :peer)))
    (setf (plist-get neomacs-mcp--active :cancelled) t))
  (when (process-live-p peer) (delete-process peer)))

(defun neomacs-mcp--sentinel (peer _event)
  "Close PEER once its connection is gone."
  (unless (process-live-p peer) (neomacs-mcp--close peer)))

(defun neomacs-mcp--wire (response)
  "Return RESPONSE as one line of JSON.
If RESPONSE cannot be encoded, for example because it contains raw
bytes, or exceeds the output limit, return an error for its ID instead."
  (let* ((json (condition-case nil
                   (json-serialize response :false-object :false :null-object :null)
                 (error nil)))
         (message (cond ((null json) "Response cannot be encoded as JSON")
                        ((>= (string-bytes json) neomacs-mcp--output-limit)
                         "Response exceeds the output limit"))))
    (concat (if message
                (json-serialize
                 (neomacs-mcp--object
                  "jsonrpc" "2.0" "id" (gethash "id" response :null)
                  "error" (neomacs-mcp--object "code" -32603 "message" message)))
              json)
            "\n")))

(defun neomacs-mcp--send (request response)
  "Send RESPONSE to the client of REQUEST unless it was abandoned.
A response over the output limit is replaced by an error.  Close the
client if the send does not return within `neomacs-mcp-send-timeout'
seconds."
  (let* ((peer (plist-get request :peer))
         (generation (plist-get request :generation))
         (wire (neomacs-mcp--wire response)))
    (when (and (not (plist-get request :cancelled)) (neomacs-mcp--live-p peer generation))
      (let ((timer (run-at-time
                    neomacs-mcp-send-timeout nil
                    (lambda ()
                      (when (neomacs-mcp--live-p peer generation)
                        (neomacs-mcp--close peer))))))
        (process-put peer 'send-timer timer)
        (unwind-protect
            (condition-case nil
                (process-send-string peer wire)
              (error (neomacs-mcp--close peer)))
          (cancel-timer timer)
          (when (eq timer (process-get peer 'send-timer))
            (process-put peer 'send-timer nil)))))))

(defun neomacs-mcp--schedule ()
  "Schedule a queue drain unless one is already pending or running."
  (when (and neomacs-mcp--queue (not neomacs-mcp--active) (not neomacs-mcp--timer))
    (setq neomacs-mcp--timer (run-at-time 0.01 nil #'neomacs-mcp--drain))))

(defun neomacs-mcp--drain ()
  "Run at most one queued request, unless user input is pending.
Before that, drop at most `neomacs-mcp--scan-limit' abandoned requests."
  (setq neomacs-mcp--timer nil)
  (unless neomacs-mcp--active
    ;; Guard against reentry from input polling and from the handler.
    (setq neomacs-mcp--active (list :admission t))
    (unwind-protect
        (unless (input-pending-p nil)
          (let ((scanned 0))
            (while (and neomacs-mcp--queue (< scanned neomacs-mcp--scan-limit)
                        (let ((request (car neomacs-mcp--queue)))
                          (or (plist-get request :cancelled)
                              (not (neomacs-mcp--live-p
                                    (plist-get request :peer)
                                    (plist-get request :generation))))))
              (pop neomacs-mcp--queue)
              (cl-incf scanned))
            (when (and neomacs-mcp--queue (< scanned neomacs-mcp--scan-limit))
              (let* ((request (pop neomacs-mcp--queue))
                     (peer (plist-get request :peer))
                     (message (plist-get request :message))
                     (id (and message (gethash "id" message :notification)))
                     (response nil))
                (setq neomacs-mcp--active request)
                (condition-case failure
                    (if (plist-get request :error)
                        (signal 'neomacs-mcp-protocol-error (plist-get request :error))
                      (setq response (neomacs-mcp--object
                                      "jsonrpc" "2.0" "id" id "result"
                                      (neomacs-mcp--dispatch peer message))))
                  (neomacs-mcp-protocol-error
                   (setq response
                         (neomacs-mcp--object
                          "jsonrpc" "2.0" "id" (if (eq id :notification) :null (or id :null))
                          "error" (neomacs-mcp--object "code" (nth 1 failure)
                                                       "message" (nth 2 failure))))
                   (when (nth 3 failure)
                     (puthash "data" (nth 3 failure) (gethash "error" response))))
                  ((error quit)
                   (setq response (neomacs-mcp--object
                                   "jsonrpc" "2.0" "id" (or id :null) "error"
                                   (neomacs-mcp--object "code" -32603
                                                        "message" "Internal error")))))
                (unless (eq id :notification) (neomacs-mcp--send request response))))))
      (setq neomacs-mcp--active nil)
      (neomacs-mcp--schedule))))

(defun neomacs-mcp--enqueue (peer message &optional failure)
  "Queue MESSAGE or framing FAILURE from PEER, closing PEER on overflow."
  (if (or (>= (length neomacs-mcp--queue) neomacs-mcp--queue-limit)
          (>= (cl-count peer neomacs-mcp--queue :key (lambda (r) (plist-get r :peer)))
              neomacs-mcp--peer-queue-limit))
      (neomacs-mcp--close peer)
    (setq neomacs-mcp--queue
          (nconc neomacs-mcp--queue
                 (list (list :peer peer :generation neomacs-mcp--generation
                             :message message :error failure :cancelled nil))))
    (neomacs-mcp--schedule)))

(defun neomacs-mcp--cancel (peer id)
  "Mark the queued or running request ID from PEER as abandoned."
  (dolist (request (cons neomacs-mcp--active neomacs-mcp--queue))
    (when (and request (eq peer (plist-get request :peer))
               (hash-table-p (plist-get request :message))
               (equal id (gethash "id" (plist-get request :message) :absent)))
      (setf (plist-get request :cancelled) t))))

(defun neomacs-mcp--frame (peer line)
  "Parse LINE from PEER, then queue it or record a cancellation."
  (condition-case nil
      (let* ((decoded (decode-coding-string line 'utf-8))
             (_valid-utf8
              (unless (equal line (encode-coding-string decoded 'utf-8))
                (error "Invalid UTF-8")))
             (message (json-parse-string decoded :object-type 'hash-table :array-type 'array
                                         :null-object :null :false-object :false))
             (id (and (hash-table-p message) (gethash "id" message :absent)))
             (method (and (hash-table-p message) (gethash "method" message)))
             (params (and (hash-table-p message)
                          (gethash "params" message (neomacs-mcp--object)))))
        (cond
         ((not (and (hash-table-p message) (equal (gethash "jsonrpc" message) "2.0")
                    (stringp method) (hash-table-p params)
                    (or (eq id :absent) (stringp id) (integerp id))))
          (neomacs-mcp--enqueue
           peer (and (or (stringp id) (integerp id)) (neomacs-mcp--object "id" id))
           '(-32600 "Invalid request")))
         ((and (eq id :absent) (equal method "notifications/cancelled"))
          (neomacs-mcp--cancel peer (gethash "requestId" params :absent)))
         ((and (eq id :absent) (not (equal method "notifications/initialized")))
          nil)
         ((and (not (eq id :absent)) (string-prefix-p "notifications/" method))
          (neomacs-mcp--enqueue peer message '(-32600 "Notification must not have an ID")))
         ((and (not (eq id :absent))
               (cl-some (lambda (r)
                          (and r (eq peer (plist-get r :peer))
                               (hash-table-p (plist-get r :message))
                               (equal id (gethash "id" (plist-get r :message) :absent))))
                        (cons neomacs-mcp--active neomacs-mcp--queue)))
          ;; Reusing an outstanding request ID closes the connection.
          (neomacs-mcp--close peer))
         (t (neomacs-mcp--enqueue peer message))))
    (error (neomacs-mcp--enqueue peer nil '(-32700 "Parse error")))))

(defun neomacs-mcp--filter (peer chunk)
  "Split CHUNK from PEER into lines and queue them; never run a tool here."
  (when (neomacs-mcp--live-p peer (process-get peer 'generation))
    (let ((text (concat (process-get peer 'input) chunk)) (frames 0))
      (if (> (string-bytes text) neomacs-mcp--frame-limit)
          (neomacs-mcp--close peer)
        (while (and (process-live-p peer) (string-match "\n" text)
                    (< frames neomacs-mcp--peer-queue-limit))
          (let ((end (match-beginning 0)))
            (neomacs-mcp--frame peer (substring text 0 end))
            (setq text (substring text (1+ end)) frames (1+ frames))))
        (if (string-match-p "\n" text) (neomacs-mcp--close peer)
          (process-put peer 'input text))))))

(defun neomacs-mcp--accept (listener peer _message)
  "Accept PEER on LISTENER unless the connection limit is reached."
  (if (or (not (eq listener neomacs-mcp--listener))
          (>= (length neomacs-mcp--peers) neomacs-mcp--peer-limit))
      (delete-process peer)
    (push peer neomacs-mcp--peers)
    (process-put peer 'generation neomacs-mcp--generation)
    (process-put peer 'input (encode-coding-string "" 'no-conversion))
    (set-process-query-on-exit-flag peer nil)
    (set-process-coding-system peer 'no-conversion 'no-conversion)
    (set-process-filter peer #'neomacs-mcp--filter)
    (set-process-sentinel peer #'neomacs-mcp--sentinel)))

(defun neomacs-mcp--socket-id (socket)
  "Return the inode and device of SOCKET, or nil if it does not exist."
  (when-let* ((attributes (file-attributes socket 'integer)))
    (list (file-attribute-inode-number attributes)
          (file-attribute-device-number attributes))))

;;;###autoload
(defun neomacs-mcp-start (socket)
  "Start the MCP endpoint on the Unix socket SOCKET.
SOCKET must be an absolute local file name in a directory that is owned
by the user and not accessible to others, as for `server-start'.  An
existing file at SOCKET is never replaced."
  (interactive "FMCP socket: ")
  (unless (and (stringp socket) (file-name-absolute-p socket)
               (not (file-remote-p socket)))
    (user-error "MCP requires an absolute local socket file name"))
  (when neomacs-mcp--listener (user-error "MCP endpoint is already started"))
  (server-ensure-safe-dir (file-name-directory socket))
  (when (or (file-exists-p socket) (file-symlink-p socket))
    (user-error "MCP socket already exists: %s" socket))
  (cl-incf neomacs-mcp--generation)
  (let ((listener (make-network-process
                   :name "neomacs-mcp" :family 'local :service socket
                   :server t :noquery t :coding 'no-conversion
                   :log #'neomacs-mcp--accept)))
    (setq neomacs-mcp--listener listener neomacs-mcp--socket socket
          neomacs-mcp--socket-identity (neomacs-mcp--socket-id socket))
    (add-hook 'kill-emacs-hook #'neomacs-mcp-stop)
    (neomacs-mcp-identity)))

;;;###autoload
(defun neomacs-mcp-stop ()
  "Stop the MCP endpoint, close its connections and remove its socket.
A file that replaced the socket is left alone."
  (interactive)
  (cl-incf neomacs-mcp--generation)
  (when neomacs-mcp--timer (cancel-timer neomacs-mcp--timer))
  (setq neomacs-mcp--timer nil neomacs-mcp--queue nil)
  (mapc #'neomacs-mcp--close (copy-sequence neomacs-mcp--peers))
  (when (process-live-p neomacs-mcp--listener) (delete-process neomacs-mcp--listener))
  (when (and neomacs-mcp--socket
             (equal (neomacs-mcp--socket-id neomacs-mcp--socket) neomacs-mcp--socket-identity)
             (file-exists-p neomacs-mcp--socket))
    (delete-file neomacs-mcp--socket))
  (setq neomacs-mcp--listener nil neomacs-mcp--socket nil neomacs-mcp--socket-identity nil)
  (remove-hook 'kill-emacs-hook #'neomacs-mcp-stop))

;;;; Built-in tools

(neomacs-mcp-register-tool
 "neomacs_identity" "Return this editor's instance, PID and runtime. Pass instance to other tools."
 (neomacs-mcp--schema nil nil) (lambda (_) (neomacs-mcp-identity))
 (neomacs-mcp--object "readOnlyHint" t))

(neomacs-mcp-register-tool
 "neomacs_eval"
 "Evaluate Emacs Lisp forms in the running editor and return the printed value. Full access: no sandbox, effects are not rolled back."
 (neomacs-mcp--schema '(("instance" . "string") ("code" . "string")) '("instance" "code"))
 #'neomacs-mcp--eval)

(let* ((instance '("instance" . "string"))
       (list-schema (neomacs-mcp--schema
                     (list instance '("offset" . "integer") '("limit" . "integer"))
                     '("instance" "offset" "limit")))
       (read-schema (neomacs-mcp--schema
                     (list instance '("name" . "string") '("start" . "integer")
                           '("maxChars" . "integer") '("expectedTick" . "integer"))
                     '("instance" "name" "start" "maxChars"))))
  (dolist (spec (list (list list-schema "offset" 0 most-positive-fixnum)
                      (list list-schema "limit" 1 32)
                      (list read-schema "start" 1 most-positive-fixnum)
                      (list read-schema "maxChars" 1 4096)
                      (list read-schema "expectedTick" 0 most-positive-fixnum)))
    (let ((property (gethash (nth 1 spec) (gethash "properties" (car spec)))))
      (puthash "minimum" (nth 2 spec) property)
      (when (< (nth 3 spec) most-positive-fixnum)
        (puthash "maximum" (nth 3 spec) property))))
  (puthash "maxLength" neomacs-mcp--buffer-name-limit
           (gethash "name" (gethash "properties" read-schema)))
  (neomacs-mcp-register-tool
   "neomacs_buffer_list"
   "List buffer names and metadata, a page at a time. Names are not stable handles."
   list-schema #'neomacs-mcp--buffer-list (neomacs-mcp--object "readOnlyHint" t))
  (neomacs-mcp-register-tool
   "neomacs_buffer_read"
   "Read text from a named buffer in widened 1-based character positions; optional expectedTick."
   read-schema #'neomacs-mcp--buffer-read (neomacs-mcp--object "readOnlyHint" t)))

(provide 'neomacs-mcp)
;;; neomacs-mcp.el ends here

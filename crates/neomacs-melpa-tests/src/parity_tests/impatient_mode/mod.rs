use std::time::Duration;

use crate::{
    COMPAT_GNU_ELPA_PIN, CachedMelpaOracle, HTMLIZE_MELPA_PIN, IMPATIENT_MODE_MELPA_PIN,
    SIMPLE_HTTPD_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const IMPATIENT_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const IMPATIENT_MODE_TEST_PRELUDE: &str = r####"
(require 'browse-url)
(require 'cl-lib)
(require 'seq)

(defvar imp-test-owned-buffers nil)
(defvar imp-test-clients nil)
(defvar imp-test-browser-events nil)

(defun imp-test-buffer (name)
  "Create and own a deterministic buffer named NAME."
  (when (get-buffer name)
    (error "Impatient test buffer already exists: %s" name))
  (let ((buffer (generate-new-buffer name)))
    (push buffer imp-test-owned-buffers)
    buffer))

(defun imp-test-own-buffer (buffer)
  "Record BUFFER for unconditional teardown."
  (cl-pushnew buffer imp-test-owned-buffers)
  buffer)

(defun imp-test-browser (url &rest arguments)
  "Observe the external browser launch boundary."
  (push (list url arguments) imp-test-browser-events)
  url)

(defun imp-test-wait (predicate label)
  "Drive real process output until PREDICATE or fail closed at LABEL."
  (let ((deadline (+ (float-time) 10.0)))
    (while (and (< (float-time) deadline)
                (not (funcall predicate)))
      (accept-process-output nil 0.01))
    (unless (funcall predicate)
      (error
       "Timed out waiting for %s; server=%S clients=%S owned=%S"
       label
       (and (boundp 'httpd--server) httpd--server
            (process-status httpd--server))
       (and (boundp 'httpd--clients)
            (mapcar #'process-status httpd--clients))
       (mapcar
        (lambda (client)
          (list (plist-get client :path)
                (and (process-live-p (plist-get client :process))
                     (process-status (plist-get client :process)))
                (with-current-buffer (plist-get client :buffer)
                  (buffer-size))))
        imp-test-clients)))))

(defun imp-test-port ()
  "Return the real ephemeral simple-httpd listener port."
  (unless (and (boundp 'httpd--server)
               (process-live-p httpd--server))
    (error "simple-httpd is not listening"))
  (let ((port (process-contact httpd--server :service)))
    (unless (and (integerp port) (> port 0))
      (error "Invalid simple-httpd port: %S" port))
    port))

(defun imp-test-open-client (name path)
  "Open a real binary loopback client NAME and send GET PATH."
  (let* ((buffer (imp-test-buffer (format " *%s response*" name)))
         (port (imp-test-port))
         process client)
    (with-current-buffer buffer
      (set-buffer-multibyte nil))
    (setq process
          (make-network-process
           :name name
           :buffer buffer
           :host "127.0.0.1"
           :service port
           :family 'ipv4
           :coding 'binary
           :nowait nil
           :sentinel #'ignore
           :noquery t))
    (setq client (list :name name :path path
                       :process process :buffer buffer))
    (push client imp-test-clients)
    (process-send-string
     process
     (format
      "GET %s HTTP/1.1\r\nHost: 127.0.0.1:%d\r\nConnection: close\r\n\r\n"
      path port))
    client))

(defun imp-test-wire-parts (client)
  "Return raw header/body boundaries for CLIENT, or nil while incomplete."
  (with-current-buffer (plist-get client :buffer)
    (let ((case-fold-search t))
      (save-excursion
        (goto-char (point-min))
        (when (search-forward "\r\n\r\n" nil t)
          (let* ((header-end (point))
                 (header
                  (buffer-substring-no-properties (point-min) header-end))
                 (length
                  (and
                   (string-match
                    "\r\nContent-Length: \\([0-9]+\\)\r\n" header)
                   (string-to-number (match-string 1 header))))
                 (actual (- (point-max) header-end)))
            (unless length
              (error "Response has no numeric Content-Length: %S" header))
            (when (> actual length)
              (error "Response exceeded Content-Length: %d > %d"
                     actual length))
            (and (= actual length)
                 (list header-end header length))))))))

(defun imp-test-response-complete-p (client)
  "Return non-nil when CLIENT has complete wire bytes and is closed."
  (and (imp-test-wire-parts client)
       (not (process-live-p (plist-get client :process)))))

(defun imp-test-await-response (client)
  "Wait for and parse one complete connection-close CLIENT response."
  (imp-test-wait
   (lambda () (imp-test-response-complete-p client))
   (format "complete HTTP response for %s" (plist-get client :path)))
  (imp-test-parse-response client))

(defun imp-test-parse-response (client)
  "Parse CLIENT's complete raw response and validate byte framing."
  (let* ((parts (or (imp-test-wire-parts client)
                    (error "Incomplete response for %s"
                           (plist-get client :path))))
         (header-end (nth 0 parts))
         (header-wire (nth 1 parts))
         (declared-length (nth 2 parts))
         ;; Do not let GNU's generic US-ASCII decoder translate CRLF to LF;
         ;; the parser validates the actual HTTP/1.1 framing below.
         (header-text (decode-coding-string header-wire 'utf-8-unix))
         (lines (split-string header-text "\r\n" t))
         (status (car lines))
         (headers
          (mapcar
           (lambda (line)
             (unless (string-match "^\\([^:]+\\): \\(.*\\)$" line)
               (error "Malformed HTTP header: %S" line))
             (cons (match-string 1 line) (match-string 2 line)))
           (cdr lines)))
         (date (cdr (assoc "Date" headers)))
         (type (cdr (assoc "Content-Type" headers)))
         (body-bytes
          (with-current-buffer (plist-get client :buffer)
            (buffer-substring-no-properties header-end (point-max))))
         (textual
          (and type
               (or (string-prefix-p "text/" type)
                   (string-match-p
                    "^application/\\(?:javascript\\|json\\|xml\\)" type)))))
    (unless (string-prefix-p "HTTP/1.1 " status)
      (error "Unexpected HTTP status line: %S" status))
    (unless (= declared-length (string-bytes body-bytes))
      (error "Body framing changed: declared=%d bytes=%d"
             declared-length (string-bytes body-bytes)))
    (unless (equal (cdr (assoc "Connection" headers)) "close")
      (error "Test response did not close: %S" headers))
    (unless
        (and date
             (condition-case nil
                 (equal date
                        (httpd-date-string (date-to-time date)))
               (error nil)))
      (error "Invalid RFC 1123 Date header: %S" date))
    (list
     :path (plist-get client :path)
     :status status
     :headers (assoc-delete-all "Date" headers)
     :date-valid t
     :body-bytes declared-length
     :body-sha256 (secure-hash 'sha256 body-bytes)
     :body
     (if textual
         (decode-coding-string body-bytes 'utf-8)
       body-bytes))))

(defun imp-test-header (response name)
  "Return response header NAME."
  (cdr (assoc name (plist-get response :headers))))

(defun imp-test-response-summary (response)
  "Return stable strict protocol facts from RESPONSE."
  (list
   :status (plist-get response :status)
   :type (imp-test-header response "Content-Type")
   :length (plist-get response :body-bytes)
   :connection (imp-test-header response "Connection")
   :server (imp-test-header response "Server")
   :cache (imp-test-header response "Cache-Control")
   :count (imp-test-header response "X-Imp-Count")
   :location (imp-test-header response "Location")
   :date-valid (plist-get response :date-valid)
   :sha256 (plist-get response :body-sha256)))

(defun imp-test-normalize-url (url)
  "Replace only the real ephemeral listener port in URL."
  (replace-regexp-in-string
   (format ":%d" (imp-test-port)) ":PORT" url t t))

(defun imp-test-type (character)
  "Insert CHARACTER through the real self-insert command."
  (let ((last-command-event character))
    (call-interactively #'self-insert-command)))

(defun imp-test-word-count-filter (buffer)
  "Render BUFFER through a documented user filter with practical output."
  (princ
   (format
    "<output data-kind=\"word-count\">%d</output>"
    (with-current-buffer buffer
      (count-words (point-min) (point-max))))))

(defun imp-test-owned-live-processes ()
  "Return live processes owned by the test server or loopback clients."
  (seq-filter
   (lambda (process)
     (or (string= (process-name process) "httpd")
         (string-prefix-p "imp-test-" (process-name process))))
   (process-list)))

(defun imp-test-cleanup ()
  "Unconditionally stop modes, timers, network peers, buffers, and server."
  (let (first-error)
    (cl-labels
        ((attempt
          (function)
          (condition-case error-data
              (funcall function)
            (error
             (unless first-error
               (setq first-error error-data))))))
      (dolist (buffer imp-test-owned-buffers)
        (when (buffer-live-p buffer)
          (attempt
           (lambda ()
             (with-current-buffer buffer
               (when (and (boundp 'impatient-mode) impatient-mode)
                 (impatient-mode -1)))))
          (attempt
           (lambda ()
             (with-current-buffer buffer
               (when (and (boundp 'imp--idle-timer) imp--idle-timer)
                 (cancel-timer (cdr imp--idle-timer))
                 (setq imp--idle-timer nil)))))
          (attempt
           (lambda ()
             (with-current-buffer buffer
               (set-buffer-modified-p nil))))))
      (dolist (client imp-test-clients)
        (attempt
         (lambda ()
           (let ((process (plist-get client :process)))
             (when (process-live-p process)
               (set-process-sentinel process #'ignore)
               (delete-process process))))))
      (attempt
       (lambda ()
         (when (and (boundp 'httpd--server) (httpd-running-p))
           (httpd-stop))))
      (attempt
       (lambda ()
         (imp-test-wait
          (lambda ()
            (and (null (imp-test-owned-live-processes))
                 (or (not (boundp 'httpd--clients))
                     (null httpd--clients))))
          "network teardown")))
      (dolist (buffer imp-test-owned-buffers)
        (attempt
         (lambda ()
           (when (buffer-live-p buffer)
             (kill-buffer buffer)))))
      (when first-error
        (signal (car first-error) (cdr first-error))))))

(defun imp-test-clean-state (root)
  "Return exact final ownership state for ROOT."
  (list
   :server (and (boundp 'httpd--server) (httpd-running-p))
   :httpd-clients (and (boundp 'httpd--clients) httpd--clients)
   :network-processes
   (mapcar #'process-name (imp-test-owned-live-processes))
   :owned-buffers
   (seq-filter
    (lambda (name)
      (or (string-prefix-p " *imp-test-" name)
          (string-prefix-p " *httpd-" name)))
    (mapcar #'buffer-name (buffer-list)))
   :owned-reference-live
   (and (seq-some #'buffer-live-p imp-test-owned-buffers) t)
   :published
   (mapcar #'buffer-name (imp--buffer-list))
   :root-exists (file-exists-p root)))

(defun imp-test-run (name function)
  "Run FUNCTION in a real isolated server sandbox named NAME."
  (let ((sandbox-root (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
    (unless (and (stringp sandbox-root)
                 (not (string-empty-p sandbox-root))
                 (file-name-absolute-p sandbox-root))
      (error "NEOMACS_TEST_SANDBOX_ROOT must be a nonempty absolute path"))
    (let* ((root
          (file-name-as-directory
           (expand-file-name name sandbox-root)))
         (httpd-host "127.0.0.1")
         (httpd-port t)
         (httpd-ip-family 'ipv4)
         (httpd-root root)
         (httpd-log-buffer nil)
         (httpd-server-name "impatient-mode parity")
         (httpd--server nil)
         (httpd--clients nil)
         (browse-url-browser-function #'imp-test-browser)
         (browse-url-handlers nil)
         (browse-url-default-handlers nil)
         (imp-test-owned-buffers nil)
         (imp-test-clients nil)
         (imp-test-browser-events nil)
         result first-error clean-state)
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    (condition-case error-data
        (setq result (funcall function root))
      (error (setq first-error error-data)))
    (condition-case error-data
        (imp-test-cleanup)
      (error
       (unless first-error
         (setq first-error error-data))))
    (condition-case error-data
        (when (file-exists-p root)
          (delete-directory root t))
      (error
       (unless first-error
         (setq first-error error-data))))
    (condition-case error-data
        (setq clean-state (imp-test-clean-state root))
      (error
       (unless first-error
         (setq first-error error-data))))
    (setq imp-test-owned-buffers nil
          imp-test-clients nil
          imp-test-browser-events nil)
    (when first-error
      (signal (car first-error) (cdr first-error)))
    (list :result result :cleanup clean-state))))
"####;

fn impatient_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(IMPATIENT_MODE_MELPA_PIN, "impatient-mode.el")
        .expect("prepare exact impatient-mode source below ./tmp")
        .with_melpa_dependency(HTMLIZE_MELPA_PIN)
        .expect("prepare exact htmlize dependency below ./tmp")
        .with_melpa_dependency(SIMPLE_HTTPD_MELPA_PIN)
        .expect("prepare exact simple-httpd dependency below ./tmp")
        .with_gnu_elpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare exact Compat dependency below ./tmp")
        .with_prelude(IMPATIENT_MODE_TEST_PRELUDE)
        .with_timeout(IMPATIENT_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed impatient-mode parity test")
        .into()
}

fn assert_impatient_mode_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        impatient_mode_oracle(),
        &current_test_name(),
        "impatient_mode_parity",
        cases,
    );
}

#[test]
fn impatient_mode_package_batch() {
    assert_impatient_mode_batch(&workflows::public_workflow_cases());
}

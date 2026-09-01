use std::time::Duration;

use crate::{CachedMelpaOracle, ORG_CLIPLINK_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Org Cliplink's public commands cross GNU's kill-ring, url.el, TLS, and
/// subprocess boundaries.  The fixture keeps them intact.  Loopback HTTP and
/// TLS origins supply deterministic external pages; the curl wrapper records
/// exact argv before delegating to the real executable.  No package command,
/// title parser, transformer, or transport function is replaced.
const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'org)
(require 'seq)
(require 'url-http)
(require 'org-cliplink)

(defvar neomacs-org-cliplink-test-server nil)
(defvar neomacs-org-cliplink-test-connections nil)
(defvar neomacs-org-cliplink-test-requests nil)
(defvar neomacs-org-cliplink-test-routes nil)
(defvar neomacs-org-cliplink-test-tls-process nil)
(defvar neomacs-org-cliplink-test-tls-buffer nil)

(defun neomacs-org-cliplink-test-header (name headers)
  (let ((case-fold-search t))
    (seq-some
     (lambda (line)
       (when (string-match "\\`\\([^:]+\\):[ \t]*\\(.*\\)\\'" line)
         (when (string-equal-ignore-case (match-string 1 line) name)
           (match-string 2 line))))
     (split-string headers "\r\n" t))))

(defun neomacs-org-cliplink-test-answer (connection headers)
  (let* ((request-line (car (split-string headers "\r\n")))
         (parts (split-string request-line " "))
         (path (nth 1 parts))
         (route (cdr (assoc path neomacs-org-cliplink-test-routes))))
    (unless route
      (error "Unexpected Org Cliplink request: %s" request-line))
    (setq neomacs-org-cliplink-test-requests
          (append
           neomacs-org-cliplink-test-requests
           (list
            (list :request-line request-line
                  :authorization
                  (neomacs-org-cliplink-test-header "Authorization" headers)
                  :accept-encoding
                  (neomacs-org-cliplink-test-header "Accept-encoding" headers)))))
    (let* ((status (or (plist-get route :status) "200 OK"))
           (body (or (plist-get route :body) ""))
           (wire (if (multibyte-string-p body)
                     (encode-coding-string body 'utf-8)
                   body))
           (extra-headers (or (plist-get route :headers) nil))
           (head
            (encode-coding-string
             (concat
              "HTTP/1.1 " status "\r\n"
              "Content-Type: text/html; charset=utf-8\r\n"
              (mapconcat #'identity extra-headers "\r\n")
              (if extra-headers "\r\n" "")
              (format "Content-Length: %d\r\n" (string-bytes wire))
              "Connection: close\r\n\r\n")
             'us-ascii)))
      (process-send-string connection (concat head wire))
      (process-send-eof connection))))

(defun neomacs-org-cliplink-test-server-filter (connection chunk)
  (process-put
   connection 'neomacs-inbox
   (concat (or (process-get connection 'neomacs-inbox) "") chunk))
  (let* ((text (process-get connection 'neomacs-inbox))
         (header-end (string-match "\r\n\r\n" text)))
    (when header-end
      (process-put connection 'neomacs-inbox "")
      (neomacs-org-cliplink-test-answer
       connection (substring text 0 header-end)))))

(defun neomacs-org-cliplink-test-start-server ()
  (setq neomacs-org-cliplink-test-connections nil
        neomacs-org-cliplink-test-requests nil
        neomacs-org-cliplink-test-server
        (make-network-process
         :name "neomacs-org-cliplink-test-server"
         :server t :host "127.0.0.1" :service t :family 'ipv4
         :coding 'binary :noquery t
         :filter #'neomacs-org-cliplink-test-server-filter
         :log (lambda (_server connection _message)
                (push connection neomacs-org-cliplink-test-connections)
                (set-process-query-on-exit-flag connection nil))))
  (process-contact neomacs-org-cliplink-test-server :service))

(defun neomacs-org-cliplink-test-stop-server ()
  (dolist (connection neomacs-org-cliplink-test-connections)
    (when (process-live-p connection)
      (delete-process connection)))
  (setq neomacs-org-cliplink-test-connections nil)
  (when (and neomacs-org-cliplink-test-server
             (process-live-p neomacs-org-cliplink-test-server))
    (delete-process neomacs-org-cliplink-test-server))
  (setq neomacs-org-cliplink-test-server nil)
  (when (and neomacs-org-cliplink-test-tls-process
             (process-live-p neomacs-org-cliplink-test-tls-process))
    (delete-process neomacs-org-cliplink-test-tls-process))
  (setq neomacs-org-cliplink-test-tls-process nil)
  (when (buffer-live-p neomacs-org-cliplink-test-tls-buffer)
    (kill-buffer neomacs-org-cliplink-test-tls-buffer))
  (setq neomacs-org-cliplink-test-tls-buffer nil))

(defun neomacs-org-cliplink-test-wait-for (predicate description)
  (let ((round 0))
    (while (and (< round 800) (not (funcall predicate)))
      (accept-process-output nil 0.01)
      (setq round (1+ round)))
    (unless (funcall predicate)
      (error "Timed out waiting for %s" description))))

(defun neomacs-org-cliplink-test-normalize-origin (text base-url)
  (let* ((url (url-generic-parse-url base-url))
         (endpoint (format "%s:%d" (url-host url) (url-port url)))
         (normalized
          (replace-regexp-in-string
           (regexp-quote base-url) "<ORIGIN>" text t t)))
    (replace-regexp-in-string
     (regexp-quote endpoint) "<ORIGIN-HOST>" normalized t t)))

(defun neomacs-org-cliplink-test-write-file (relative contents)
  (let ((path (expand-file-name relative
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert contents)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun neomacs-org-cliplink-test-read-file (path)
  (with-temp-buffer
    (insert-file-contents path)
    (buffer-string)))

(defun neomacs-org-cliplink-test-write-executable (relative contents)
  (let ((path (neomacs-org-cliplink-test-write-file relative contents)))
    (set-file-modes path #o755)
    path))

(defun neomacs-org-cliplink-test-recorded-request (path)
  (let* ((headers
          (with-temp-buffer
            (insert-file-contents-literally path)
            (buffer-string)))
         (request-line (car (split-string headers "\r\n"))))
    (list :request-line request-line
          :authorization
          (neomacs-org-cliplink-test-header "Authorization" headers)
          :accept-encoding
          (neomacs-org-cliplink-test-header "Accept-encoding" headers))))

(defun neomacs-org-cliplink-test-start-tls-server (status body)
  (let* ((root (expand-file-name "org-cliplink/tls/"
                                 (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
         (certificate (expand-file-name "certificate.pem" root))
         (private-key (expand-file-name "private-key.pem" root))
         (port-file (expand-file-name "port" root))
         (request-file (expand-file-name "request" root))
         (response-file (expand-file-name "response" root))
         (server-script
          (neomacs-org-cliplink-test-write-executable
           "org-cliplink/tls/server.py"
           "#!/usr/bin/env python3\nimport pathlib\nimport socket\nimport ssl\nimport sys\n\ncert, key, port_path, request_path, response_path = map(pathlib.Path, sys.argv[1:])\nlistener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)\nlistener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)\nlistener.bind(('127.0.0.1', 0))\nlistener.listen(1)\nport_path.write_text(str(listener.getsockname()[1]), encoding='ascii')\ncontext = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)\ncontext.load_cert_chain(cert, key)\nraw, _ = listener.accept()\nwith context.wrap_socket(raw, server_side=True) as connection:\n    request = b''\n    while b'\\r\\n\\r\\n' not in request:\n        chunk = connection.recv(65536)\n        if not chunk:\n            break\n        request += chunk\n    request_path.write_bytes(request)\n    connection.sendall(response_path.read_bytes())\nlistener.close()\n"))
         (wire (encode-coding-string body 'utf-8)))
    (make-directory root t)
    (when (file-exists-p port-file) (delete-file port-file))
    (when (file-exists-p request-file) (delete-file request-file))
    (with-temp-buffer
      (insert
       (encode-coding-string
        (concat "HTTP/1.1 " status "\r\n"
                "Content-Type: text/html; charset=utf-8\r\n"
                (format "Content-Length: %d\r\n" (string-bytes wire))
                "Connection: close\r\n\r\n")
        'us-ascii))
      (insert wire)
      (set-buffer-multibyte nil)
      (write-region (point-min) (point-max) response-file nil 'silent))
    (let ((openssl-buffer (generate-new-buffer " *org-cliplink-openssl*")))
      (unwind-protect
          (unless
              (zerop
               (call-process
                (or (executable-find "openssl")
                    (error "openssl is required for Org Cliplink TLS parity"))
                nil openssl-buffer nil
                "req" "-x509" "-newkey" "rsa:2048" "-nodes"
                "-keyout" private-key "-out" certificate "-days" "1"
                "-subj" "/CN=127.0.0.1"
                "-addext" "subjectAltName=IP:127.0.0.1"
                "-addext" "basicConstraints=critical,CA:FALSE"
                "-addext" "keyUsage=critical,digitalSignature,keyEncipherment"
                "-addext" "extendedKeyUsage=serverAuth"))
            (error "TLS certificate generation failed: %s"
                   (with-current-buffer openssl-buffer (buffer-string))))
        (kill-buffer openssl-buffer)))
    (setq neomacs-org-cliplink-test-tls-buffer
          (generate-new-buffer " *org-cliplink-tls-server*"))
    (setq neomacs-org-cliplink-test-tls-process
          (start-process
           "org-cliplink-tls-server"
           neomacs-org-cliplink-test-tls-buffer
           (or (executable-find "python3")
               (error "python3 is required for Org Cliplink TLS parity"))
           server-script certificate private-key port-file request-file response-file))
    (set-process-query-on-exit-flag neomacs-org-cliplink-test-tls-process nil)
    (neomacs-org-cliplink-test-wait-for
     (lambda ()
       (and (file-exists-p port-file)
            (> (string-to-number
                (neomacs-org-cliplink-test-read-file port-file))
               0)))
     "the complete TLS origin port")
    (list
     :base-url
     (format "https://127.0.0.1:%d"
             (string-to-number (neomacs-org-cliplink-test-read-file port-file)))
     :request-file request-file
     :certificate certificate)))

(defmacro neomacs-org-cliplink-test-with-curl-wrapper (&rest body)
  (declare (indent 0) (debug t))
  `(let* ((real-curl
           (or (executable-find "curl")
               (error "curl is required for Org Cliplink parity")))
          (wrapper-directory
           (expand-file-name "org-cliplink/curl/bin/"
                             (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
          (argv-file
           (expand-file-name "org-cliplink/curl/argv"
                             (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
          (output-file
           (expand-file-name "org-cliplink/curl/output"
                             (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
          (wrapper
           (neomacs-org-cliplink-test-write-executable
            "org-cliplink/curl/bin/curl"
            (concat
             "#!/bin/sh\n"
             "printf '%s\\n' \"$@\" > \"$NEOMACS_ORG_CLIPLINK_CURL_ARGV\"\n"
             (format "REAL_CURL=%s\n" (shell-quote-argument real-curl))
             "\"$REAL_CURL\" \"$@\" > \"$NEOMACS_ORG_CLIPLINK_CURL_OUTPUT\" 2>/dev/null\n"
             "status=$?\n"
             "if [ \"$status\" -eq 0 ]; then\n"
             "  cat \"$NEOMACS_ORG_CLIPLINK_CURL_OUTPUT\"\n"
             "else\n"
             "  printf '%s\\n' 'curl: deterministic request failure' >&2\n"
             "fi\n"
             "exit \"$status\"\n")))
          (process-environment (copy-sequence process-environment))
          (exec-path (cons wrapper-directory exec-path)))
     (ignore wrapper)
     (setenv "NEOMACS_ORG_CLIPLINK_CURL_ARGV" argv-file)
     (setenv "NEOMACS_ORG_CLIPLINK_CURL_OUTPUT" output-file)
     ,@body))

(defun neomacs-org-cliplink-test-last-message-matching (regexp)
  (when-let ((buffer (get-buffer "*Messages*")))
    (with-current-buffer buffer
      (save-excursion
        (goto-char (point-max))
        (when (re-search-backward regexp nil t)
          (match-string-no-properties 0))))))

(defun neomacs-org-cliplink-test-link-state (base-url)
  (let ((link (org-element-context)))
    (list :type (org-element-type link)
          :raw-link
          (neomacs-org-cliplink-test-normalize-origin
           (org-element-property :raw-link link) base-url)
          :contents
          (when-let* ((begin (org-element-property :contents-begin link))
                      (end (org-element-property :contents-end link)))
            (buffer-substring-no-properties begin end)))))

(defmacro neomacs-org-cliplink-test-with-environment (&rest body)
  (declare (indent 1) (debug t))
  `(let* ((buffers-before (buffer-list))
          (kill-ring nil)
          (kill-ring-yank-pointer nil)
          (interprogram-cut-function nil)
          (interprogram-paste-function nil)
          (url-proxy-services nil)
          (url-gateway-method 'native)
          (url-http-attempt-keepalives nil)
          (org-cliplink-transport-implementation 'url-el)
          (org-cliplink-simpleclip-source nil)
          (org-cliplink-secrets-path
           (expand-file-name "missing-secrets.el"
                             (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
     (unwind-protect
         (progn ,@body)
       (neomacs-org-cliplink-test-stop-server)
       (dolist (buffer (buffer-list))
         (unless (memq buffer buffers-before)
           (let ((process (get-buffer-process buffer)))
             (when (process-live-p process)
               (delete-process process)))
           (with-current-buffer buffer
             (set-buffer-modified-p nil))
           (kill-buffer buffer))))))

(defmacro neomacs-org-cliplink-test-with-site (routes &rest body)
  (declare (indent 1) (debug t))
  `(neomacs-org-cliplink-test-with-environment
     (let* ((neomacs-org-cliplink-test-routes ,routes)
            (port (neomacs-org-cliplink-test-start-server))
            (base-url (format "http://127.0.0.1:%d" port)))
       ,@body)))

(defmacro neomacs-org-cliplink-test-with-tls-site (status response-body &rest body)
  (declare (indent 2) (debug t))
  `(neomacs-org-cliplink-test-with-environment
     (let* ((tls-site
             (neomacs-org-cliplink-test-start-tls-server ,status ,response-body))
            (base-url (plist-get tls-site :base-url))
            (request-file (plist-get tls-site :request-file))
            (gnutls-trustfiles (list (plist-get tls-site :certificate)))
            (network-security-level 'low))
       ,@body)))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ORG_CLIPLINK_MELPA_PIN, "org-cliplink.el")
        .expect("prepare exact shallow Org Cliplink source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn org_cliplink_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "org-cliplink-package-batch",
        "org_cliplink_parity",
        &workflows::workflow_batch_cases(),
    );
}

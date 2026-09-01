use std::time::Duration;

use crate::{CachedMelpaOracle, TERN_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Tern's editor integration talks JSON over real HTTP to an external analyzer.
/// The fixture preserves that boundary: url.el opens a loopback connection,
/// writes the package's actual POST body, parses an HTTP response, and invokes
/// Tern's real asynchronous callbacks.  Only the analyzer answers are
/// deterministic test data.  No package command, query builder, transport,
/// completion, refactoring, navigation, or display function is replaced.
const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'js)
(require 'json)
(require 'seq)
(require 'tern)

(defvar neomacs-tern-test-server nil)
(defvar neomacs-tern-test-connections nil)
(defvar neomacs-tern-test-requests nil)
(defvar neomacs-tern-test-responses nil)

(defun neomacs-tern-test-write-file (root relative contents)
  (let ((path (expand-file-name relative root)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert contents)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun neomacs-tern-test-query-type (document)
  (let ((query (cdr (assq 'query document))))
    (if query (intern (cdr (assq 'type query))) 'sync)))

(defun neomacs-tern-test-response (document)
  (let* ((type (neomacs-tern-test-query-type document))
         (configured (cdr (assq type neomacs-tern-test-responses)))
         (value (if (functionp configured)
                    (funcall configured document)
                  configured)))
    (unless configured
      (error "Unexpected Tern analyzer request: %S" document))
    (if (eq (car-safe value) 'raw)
        (list (nth 1 value) (nth 2 value))
      (list "200 OK" (json-encode (or value '((ok . t))))))))

(defun neomacs-tern-test-request-summary (document request-line)
  (let* ((query (cdr (assq 'query document)))
         (files (cdr (assq 'files document))))
    (list :request-line request-line
          :type (neomacs-tern-test-query-type document)
          :file (cdr (assq 'file query))
          :end (cdr (assq 'end query))
          :new-name (cdr (assq 'newName query))
          :variable (cdr (assq 'variable query))
          :include-keywords (cdr (assq 'includeKeywords query))
          :prefer-function (cdr (assq 'preferFunction query))
          :files
          (when files
            (mapcar
             (lambda (file)
               (list :type (cdr (assq 'type file))
                     :name (cdr (assq 'name file))
                     :offset (cdr (assq 'offset file))
                     :text
                     (let ((text (cdr (assq 'text file))))
                       (if (> (length text) 240)
                           (list :length (length text)
                                 :sha256 (secure-hash 'sha256 text)
                                 :prefix (substring text 0 40)
                                 :suffix (substring text (- (length text) 60)))
                         text))))
             (append files nil))))))

(defun neomacs-tern-test-answer (connection headers body)
  (let* ((json-object-type 'alist)
         (json-array-type 'vector)
         (document (json-read-from-string
                    (decode-coding-string body 'utf-8)))
         (request-line (car (split-string headers "\r\n")))
         (answer (neomacs-tern-test-response document))
         (status (car answer))
         (response-body (cadr answer)))
    (setq neomacs-tern-test-requests
          (append neomacs-tern-test-requests
                  (list (neomacs-tern-test-request-summary
                         document request-line))))
    (let ((wire (encode-coding-string response-body 'utf-8)))
      (process-send-string
       connection
       (concat "HTTP/1.1 " status "\r\n"
               "Content-Type: application/json; charset=utf-8\r\n"
               (format "Content-Length: %d\r\n" (string-bytes wire))
               "Connection: close\r\n\r\n"
               wire))
      (process-send-eof connection))))

(defun neomacs-tern-test-server-filter (connection chunk)
  (process-put connection 'neomacs-inbox
               (concat (or (process-get connection 'neomacs-inbox) "")
                       chunk))
  (let* ((text (process-get connection 'neomacs-inbox))
         (header-end (string-match "\r\n\r\n" text)))
    (when header-end
      (let* ((headers (substring text 0 header-end))
             (body-start (+ header-end 4))
             (length
              (if (string-match
                   "[Cc]ontent-[Ll]ength: *\\([0-9]+\\)" headers)
                  (string-to-number (match-string 1 headers))
                0)))
        (when (>= (- (string-bytes text) body-start) length)
          (process-put connection 'neomacs-inbox "")
          (neomacs-tern-test-answer
           connection headers (substring text body-start (+ body-start length))))))))

(defun neomacs-tern-test-start-server ()
  (setq neomacs-tern-test-connections nil
        neomacs-tern-test-requests nil
        neomacs-tern-test-server
        (make-network-process
         :name "neomacs-tern-test-server" :server t
         :host "127.0.0.1" :service t :family 'ipv4
         :coding 'binary :noquery t
         :filter #'neomacs-tern-test-server-filter
         :log (lambda (_server connection _message)
                (push connection neomacs-tern-test-connections)
                (set-process-query-on-exit-flag connection nil))))
  (process-contact neomacs-tern-test-server :service))

(defun neomacs-tern-test-stop-server ()
  (dolist (connection neomacs-tern-test-connections)
    (when (process-live-p connection) (delete-process connection)))
  (setq neomacs-tern-test-connections nil)
  (when (and neomacs-tern-test-server
             (process-live-p neomacs-tern-test-server))
    (delete-process neomacs-tern-test-server))
  (setq neomacs-tern-test-server nil))

(defun neomacs-tern-test-wait-for (predicate description)
  (let ((round 0))
    (while (and (< round 800) (not (funcall predicate)))
      (accept-process-output nil 0.01)
      (setq round (1+ round)))
    (unless (funcall predicate)
      (error "Timed out waiting for %s" description))))

(defun neomacs-tern-test-http-idle-p (buffers-before)
  (not
   (seq-some
    (lambda (buffer)
      (and (not (memq buffer buffers-before))
           (process-live-p (get-buffer-process buffer))))
    (buffer-list))))

(defun neomacs-tern-test-buffer-state ()
  (list :file (file-name-nondirectory (buffer-file-name))
        :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :line (line-number-at-pos)
        :column (current-column)
        :modified (buffer-modified-p)))

(defvar neomacs-tern-test-messages nil)

(defun neomacs-tern-test-observe-message (original format-string &rest arguments)
  (setq neomacs-tern-test-messages
        (append neomacs-tern-test-messages
                (list (apply #'format format-string arguments))))
  (apply original format-string arguments))

(defmacro neomacs-tern-test-with-message-observer (&rest body)
  (declare (indent 0) (debug t))
  `(let ((neomacs-tern-test-messages nil))
     (advice-add #'tern-message :around #'neomacs-tern-test-observe-message)
     (unwind-protect (progn ,@body)
       (advice-remove #'tern-message #'neomacs-tern-test-observe-message))))

(defmacro neomacs-tern-test-with-project
    (name files responses &rest body)
  (declare (indent 3) (debug t))
  `(let* ((root (file-name-as-directory
                 (expand-file-name ,name
                                   (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
          (buffers-before (buffer-list))
          (tern-idle-timer nil)
          ;; Keep incidental post-command scheduling outside every practical
          ;; workflow, while cases that exercise hints bind a short delay.
          (tern-update-argument-hints-timer 60000)
          (tern-update-argument-hints-async nil)
          (tern-find-definition-stack nil)
          (tern-command-generation 0)
          (tern-activity-since-command -1)
          (tern-last-docs-url nil)
          (js-mode-hook (cons #'tern-mode js-mode-hook))
          (tern-test-responses ,responses)
          (neomacs-tern-test-responses tern-test-responses))
     (unwind-protect
         (progn
           (dolist (file ,files)
             (neomacs-tern-test-write-file root (car file) (cdr file)))
           (let ((port (neomacs-tern-test-start-server)))
             (neomacs-tern-test-write-file root ".tern-project" "{}\n")
             (neomacs-tern-test-write-file root ".tern-port"
                                           (number-to-string port)))
           (find-file (expand-file-name "src/main.js" root))
           (neomacs-tern-test-wait-for
            (lambda () (= (length neomacs-tern-test-requests) 1))
            "Tern's initial full-file synchronization")
           (let ((result (progn ,@body)))
             (neomacs-tern-test-wait-for
              (lambda ()
                (neomacs-tern-test-http-idle-p buffers-before))
              "all HTTP callbacks to finish")
             result))
       (when tern-update-argument-hints-async
         (cancel-timer tern-update-argument-hints-async)
         (setq tern-update-argument-hints-async nil))
       (neomacs-tern-test-stop-server)
       (dolist (buffer (buffer-list))
         (unless (memq buffer buffers-before)
           (with-current-buffer buffer
             (when tern-mode (tern-mode -1))
             (let ((process (get-buffer-process buffer)))
               (when (process-live-p process) (delete-process process)))
             (set-buffer-modified-p nil))
           (kill-buffer buffer))))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(TERN_MELPA_PIN, "tern.el")
        .expect("prepare exact shallow Tern source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn tern_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "tern-package-batch",
        "tern_parity",
        &workflows::workflow_batch_cases(),
    );
}

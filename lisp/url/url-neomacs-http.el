;;; url-neomacs-http.el --- Browser host HTTP loader -*- lexical-binding: t; -*-

;; HTTP request/response adaptation, not emulated network processes.
;; Browser Fetch owns TLS, redirects and decompression.  Native URL loaders
;; are untouched unless this module is explicitly enabled by the WASM host.

(require 'url)
(require 'url-http)

(declare-function neomacs-http-start nil (url method headers body))
(declare-function neomacs-http-take nil (request))
(declare-function neomacs-http-cancel nil (request))

(defvar-local url-neomacs-http--request nil)
(defvar-local url-neomacs-http--timer nil)

(defun url-neomacs-http--cancel ()
  (when url-neomacs-http--timer
    (cancel-timer url-neomacs-http--timer)
    (setq url-neomacs-http--timer nil))
  (when url-neomacs-http--request
    (neomacs-http-cancel url-neomacs-http--request)
    (setq url-neomacs-http--request nil)))

(defun url-neomacs-http--poll (buffer original-url callback cbargs)
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (let ((result (condition-case err
                        (neomacs-http-take url-neomacs-http--request)
                      (error (vector 'error (error-message-string err))))))
        (when result
          ;; Retire the request before invoking arbitrary Lisp callbacks.
          (url-neomacs-http--cancel)
          (let (status)
            (if (eq (aref result 0) 'error)
                (setq status (list :error (list 'error (aref result 1))))
              (setq-local url-http-response-status (aref result 0))
              (setq-local url-current-object (url-generic-parse-url (aref result 1)))
              (unless (equal original-url (aref result 1))
                (setq status (list :redirect (aref result 1))))
              (when (>= url-http-response-status 400)
                (setq status (append status (list :error (list 'error 'http url-http-response-status)))))
              (insert (format "HTTP/1.1 %d\n" url-http-response-status))
              (dolist (header (aref result 2))
                ;; Fetch has already decoded content and transfer encodings.
                (unless (member (downcase (car header))
                                '("content-encoding" "transfer-encoding" "content-length"))
                  (insert (car header) ": " (cdr header) "\n")))
              (insert "\n")
              (setq-local url-http-end-of-headers (copy-marker (1- (point))))
              (insert (aref result 3)))
            (goto-char (point-min))
            (apply callback (append status (car cbargs)) (cdr cbargs))))))))

(defun url-neomacs-http (url callback cbargs)
  "Retrieve HTTP URL asynchronously through the browser host.
CALLBACK and CBARGS follow `url-retrieve'.  Kill the returned buffer to cancel.
Browser CORS restrictions apply; browser login cookies are not sent."
  (let* ((address (url-recreate-url url))
         (request (neomacs-http-start address (or url-request-method "GET")
                                      url-request-extra-headers url-request-data))
         (buffer (generate-new-buffer " *neomacs-http*")))
    (with-current-buffer buffer
      (set-buffer-multibyte nil)
      (setq-local url-current-object url)
      (setq url-neomacs-http--request request)
      (add-hook 'kill-buffer-hook #'url-neomacs-http--cancel nil t)
      (setq url-neomacs-http--timer
            (run-at-time 0.01 0.01 #'url-neomacs-http--poll
                         buffer address callback cbargs)))
    buffer))

(defun url-neomacs-http-enable ()
  "Install browser HTTP loaders in the current VM's URL scheme registry."
  (dolist (scheme '("http" "https"))
    (puthash scheme
             (list 'name scheme 'loader #'url-neomacs-http 'asynchronous-p t
                   'default-port (if (equal scheme "https") 443 80)
                   'expand-file-name #'url-default-expander
                   'parse-url #'url-generic-parse-url)
             url-scheme-registry)))

(provide 'url-neomacs-http)
;;; url-neomacs-http.el ends here

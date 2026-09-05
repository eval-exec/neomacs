;;; url-neomacs-http-tests.el --- Host HTTP integration tests -*- lexical-binding: t; -*-
(require 'ert)
(require 'cl-lib)
(require 'url)
(require 'url-neomacs-http)

(ert-deftest url-neomacs-http-asynchronous-response ()
  (let ((url-scheme-registry (copy-hash-table url-scheme-registry))
        (url-proxy-services nil)
        (ready nil) (completed nil) (response-text nil))
    (cl-letf (((symbol-function 'neomacs-http-start) (lambda (&rest _) 1))
              ((symbol-function 'neomacs-http-take)
               (lambda (_id)
                 (when ready
                   (vector 404 "https://example.invalid/final"
                           '(("content-type" . "text/plain"))
                           (unibyte-string 0 128 255)))))
              ((symbol-function 'neomacs-http-cancel) #'ignore))
      (url-neomacs-http-enable)
      (let ((buffer (url-retrieve
                     "https://example.invalid/start"
                     (lambda (status)
                       (setq completed status response-text (buffer-string))))))
        (unwind-protect
            (progn
              (should (buffer-live-p buffer))
              (should-not response-text)
              (setq ready t)
              (let ((deadline (+ (float-time) 2)))
                (while (and (not response-text) (< (float-time) deadline))
                  (accept-process-output nil 0.01)))
              (should (equal (plist-get completed :redirect) "https://example.invalid/final"))
              (should (equal response-text
                             (concat "HTTP/1.1 404\ncontent-type: text/plain\n\n"
                                     (unibyte-string 0 128 255)))))
          (kill-buffer buffer))))))

(ert-deftest url-neomacs-http-killed-buffer-cancels ()
  (let ((url-scheme-registry (copy-hash-table url-scheme-registry))
        (cancelled nil) (called nil))
    (cl-letf (((symbol-function 'neomacs-http-start) (lambda (&rest _) 7))
              ((symbol-function 'neomacs-http-take) (lambda (_) nil))
              ((symbol-function 'neomacs-http-cancel) (lambda (id) (setq cancelled id))))
      (url-neomacs-http-enable)
      (kill-buffer (url-retrieve "https://example.invalid/"
                                 (lambda (&rest _) (setq called t))))
      (accept-process-output nil 0.03)
      (should (equal cancelled 7))
      (should-not called))))

(ert-deftest url-neomacs-http-failure-preserves-callback-arguments ()
  (let ((url-scheme-registry (copy-hash-table url-scheme-registry))
        (result nil))
    (cl-letf (((symbol-function 'neomacs-http-start) (lambda (&rest _) 8))
              ((symbol-function 'neomacs-http-take) (lambda (_) (error "Fetch failed")))
              ((symbol-function 'neomacs-http-cancel) #'ignore))
      (url-neomacs-http-enable)
      (let ((buffer (url-retrieve "https://example.invalid/"
                                 (lambda (status token) (setq result (list status token)))
                                 '(token))))
        (unwind-protect
            (progn
              (let ((deadline (+ (float-time) 2)))
                (while (and (not result) (< (float-time) deadline))
                  (accept-process-output nil 0.01)))
              (should (equal result '((:error (error "Fetch failed")) token))))
          (kill-buffer buffer))))))

(provide 'url-neomacs-http-tests)

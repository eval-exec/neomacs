;;; clatter-process-filter-notification.el --- Account-free Clatter notification -*- lexical-binding: t -*-
(require 'clatter-handlers)
(require 'clatter-notify)
(require 'json)
(setq clatter-notify-current-buffer t
      clatter-notify-cooldown 0
      clatter-notify-echo-area nil
      clatter-flyspell-enable nil)
(clatter-notify-enable)
(defvar neomacs-clatter-control (getenv "NEOMACS_CLATTER_CONTROL"))

(defun neomacs-clatter-notification-run ()
  "Deliver a real IRC line through Clatter and inspect its private notification."
  (let* ((process (make-process
                   :name "clatter-private-replay" :connection-type 'pipe :noquery t
                   :command '("printf" ":alice!alice@localhost PRIVMSG neo :hello from local fixture\r\n")
                   :filter #'clatter--process-filter))
         (connection (clatter-connection--create
                      :network-id "fixture" :process process :state :connected
                      :nick "neo" :recv-buffer ""
                      :isupport (make-hash-table :test 'equal)))
         (deadline (+ (float-time) 5)))
    (unwind-protect
        (progn
          (process-put process :clatter-network-id "fixture")
          (puthash "fixture" connection clatter-connections)
          (while (and (process-live-p process) (< (float-time) deadline))
            (accept-process-output process 0.05))
          (when (process-live-p process) (error "Clatter replay process timed out"))
          (accept-process-output process 0.05)
          (let* ((received (equal (clatter-connection-recv-buffer connection) ""))
                 (released (not (process-get process :clatter-filter-busy)))
                 (notification (expand-file-name "notify.json" neomacs-clatter-control))
                 (payload (and (file-exists-p notification) (json-read-file notification)))
                 (passed (and received released (= (process-exit-status process) 0)
                              (= (or (alist-get 'count payload) 0) 1)
                              (equal (alist-get 'app-name payload) "CLatter")
                              (equal (alist-get 'summary payload) "DM from alice")
                              (equal (alist-get 'body payload) "hello from local fixture")
                              (= (or (alist-get 'urgency payload) -1) 1)
                              (equal (alist-get 'category payload) "im.received")
                              (= (or (alist-get 'timeout payload) -1) 5000))))
            (with-temp-file (expand-file-name "client.json" neomacs-clatter-control)
              (insert (json-encode
                       `((passed . ,(if passed t :json-false))
                         (received-line . ,(if received t :json-false))
                         (filter-released . ,(if released t :json-false))
                         (exit-status . ,(process-exit-status process))))))
            passed))
      (when (process-live-p process) (delete-process process))
      (remhash "fixture" clatter-connections))))

(defun neomacs-clatter-await-paint ()
  (if (file-exists-p (expand-file-name "painted" neomacs-clatter-control))
      (kill-emacs 0)
    (run-at-time 0.05 nil #'neomacs-clatter-await-paint)))

(defun neomacs-clatter-present-notification ()
  (let ((passed (neomacs-clatter-notification-run)))
    (switch-to-buffer (get-buffer-create "*clatter-notification*"))
    (erase-buffer)
    (setq-local mode-line-format '(" Clatter notifications "))
    (insert (if passed "CLATTER-NOTIFICATION-PASSED\n" "CLATTER-NOTIFICATION-FAILED\n"))
    (insert "DM from alice\nhello from local fixture\n")
    (goto-char (point-min))
    (when (display-graphic-p)
      (set-face-attribute 'default nil :background (if passed "#00ff00" "#ff0000"))
      (neomacs--write-frame-snapshot
       (expand-file-name "final.json" neomacs-clatter-control) nil 'json)
      (neomacs-clatter-await-paint))))

(if noninteractive
    (neomacs-clatter-notification-run)
  (setq inhibit-startup-screen t)
  (blink-cursor-mode -1)
  (when (display-graphic-p)
    (menu-bar-mode -1)
    (tool-bar-mode -1)
    (set-frame-size nil 500 400 t)
    (set-face-attribute 'default nil :foreground "#000000" :background "#ffffff"))
  (run-at-time 0.5 nil #'neomacs-clatter-present-notification)
  (run-at-time 15 nil (lambda () (kill-emacs 2))))

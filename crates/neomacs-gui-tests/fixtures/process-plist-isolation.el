;;; process-plist-isolation.el --- Accepted TCP process properties -*- lexical-binding: t -*-

(defun neomacs-process-plist-spines-overlap-p (left right)
  "Return non-nil if LEFT and RIGHT share any cons in their list spines."
  (let (right-cells)
    (while (consp right)
      (push right right-cells)
      (setq right (cdr right)))
    (catch 'shared
      (while (consp left)
        (when (memq left right-cells) (throw 'shared t))
        (setq left (cdr left)))
      nil)))

(defun neomacs-process-plist-await-accepted (accepted expected)
  "Wait boundedly for ACCEPTED to report EXPECTED TCP connections."
  (let ((deadline (+ (float-time) 2)))
    (while (and (< (length (funcall accepted)) expected)
                (< (float-time) deadline))
      (accept-process-output nil 0.05))
    (unless (= (length (funcall accepted)) expected)
      (error "Expected %s accepted connections, got %s"
             expected (length (funcall accepted))))))

(defun neomacs-process-plist-isolation-check ()
  "Check real TCP children: independent list spines, shared nested values.
Return ten boolean checks followed by the accepted connection count."
  (let* ((nested (list 'shared))
         (data (vector 'shared))
         (accepted nil)
         (clients nil)
         (server nil))
    (unwind-protect
        (progn
          (setq server
                (make-network-process
                 :name "plist-isolation-server" :server t :host "127.0.0.1"
                 :service t :family 'ipv4 :noquery t :filter #'ignore
                 :plist (list :same 'old :nested nested :data data)
                 :log (lambda (_server child _message) (push child accepted))))
          (let ((port (process-contact server :service)))
            ;; Both clients connect before the first acceptance wait.
            (dotimes (_ 2)
              (push (make-network-process
                     :name "plist-isolation-client" :host "127.0.0.1"
                     :service port :family 'ipv4 :noquery t :filter #'ignore)
                    clients))
            (neomacs-process-plist-await-accepted (lambda () accepted) 2)
            (let* ((first (car (last accepted)))
                   (second (car accepted))
                   (initial-old (and (eq (process-get server :same) 'old)
                                     (eq (process-get first :same) 'old)
                                     (eq (process-get second :same) 'old))))
              (process-put first :same 'changed)
              (process-put first :response-sent t)
              ;; A later child must inherit listener properties, not FIRST's.
              (push (make-network-process
                     :name "plist-isolation-future" :host "127.0.0.1"
                     :service port :family 'ipv4 :noquery t :filter #'ignore)
                    clients)
              (neomacs-process-plist-await-accepted (lambda () accepted) 3)
              (let* ((future (car accepted))
                     (processes (list server first second future))
                     (distinct t)
                     (nested-shared t)
                     (vector-shared t)
                     (nested-visible t)
                     (vector-visible t)
                     (remaining processes))
                (while remaining
                  (dolist (other (cdr remaining))
                    (when (neomacs-process-plist-spines-overlap-p
                           (process-plist (car remaining)) (process-plist other))
                      (setq distinct nil)))
                  (setq remaining (cdr remaining)))
                (dolist (process processes)
                  (unless (eq (process-get process :nested) nested)
                    (setq nested-shared nil))
                  (unless (eq (process-get process :data) data)
                    (setq vector-shared nil)))
                ;; GNU's copy is shallow: nested mutable objects remain shared.
                (setcar nested 'updated)
                (aset data 0 'updated)
                (dolist (process processes)
                  (unless (eq (car (process-get process :nested)) 'updated)
                    (setq nested-visible nil))
                  (unless (eq (aref (process-get process :data) 0) 'updated)
                    (setq vector-visible nil)))
                (list distinct initial-old
                      (and (eq (process-get first :same) 'changed)
                           (eq (process-get first :response-sent) t))
                      (and (eq (process-get server :same) 'old)
                           (null (process-get server :response-sent)))
                      (and (eq (process-get second :same) 'old)
                           (null (process-get second :response-sent)))
                      (and (eq (process-get future :same) 'old)
                           (null (process-get future :response-sent)))
                      nested-shared vector-shared nested-visible vector-visible
                      (length accepted))))))
      (dolist (process (append clients accepted (list server)))
        (when (and process (process-live-p process)) (delete-process process))))))

(defvar neomacs-process-plist-control (getenv "NEOMACS_PROCESS_PLIST_CONTROL"))

(defun neomacs-process-plist-await-paint ()
  (if (file-exists-p (expand-file-name "painted" neomacs-process-plist-control))
      (kill-emacs 0)
    (run-at-time 0.05 nil #'neomacs-process-plist-await-paint)))

(defun neomacs-process-plist-present ()
  (switch-to-buffer (get-buffer-create "*process-plist-isolation*"))
  (erase-buffer)
  (setq-local mode-line-format '(" TCP process properties "))
  (let* ((result (condition-case failure
                     (neomacs-process-plist-isolation-check)
                   (error (list (error-message-string failure)))))
         (passed (equal result '(t t t t t t t t t t 3))))
    (insert (if passed "PROCESS-PLIST-PASSED\n" "PROCESS-PLIST-FAILED\n"))
    (insert "Accepted TCP connections keep independent properties.\n")
    (goto-char (point-min))
    (with-temp-file (expand-file-name "result.json" neomacs-process-plist-control)
      (insert (json-encode `((passed . ,(if passed t :json-false))
                            (checks . ,(vconcat result))))))
    (when (display-graphic-p)
      (set-face-attribute 'default nil :background (if passed "#00ff00" "#ff0000"))
      (neomacs--write-frame-snapshot
       (expand-file-name "final.json" neomacs-process-plist-control) nil 'json)
      (neomacs-process-plist-await-paint))))

;; Unit and oracle tests load these definitions without launching UI timers.
(when neomacs-process-plist-control
  (require 'json)
  (setq inhibit-startup-screen t)
  (blink-cursor-mode -1)
  (when (display-graphic-p)
    (menu-bar-mode -1)
    (tool-bar-mode -1)
    (set-frame-size nil 500 400 t)
    (set-face-attribute 'default nil :foreground "#000000" :background "#ffffff"))
  (run-at-time 0.5 nil #'neomacs-process-plist-present)
  (run-at-time 15 nil (lambda () (kill-emacs 2))))

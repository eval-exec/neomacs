;;; clatter-notification-service.el --- Private notification counterparty -*- lexical-binding: t -*-
(require 'dbus)
(require 'json)
(defvar neomacs-clatter-service-control (getenv "NEOMACS_CLATTER_CONTROL"))
(defvar neomacs-clatter-service-count 0)

(dbus-register-method
 :session "org.freedesktop.Notifications" "/org/freedesktop/Notifications"
 "org.freedesktop.Notifications" "GetCapabilities"
 (lambda () '((:array :string "body"))))

(dbus-register-method
 :session "org.freedesktop.Notifications" "/org/freedesktop/Notifications"
 "org.freedesktop.Notifications" "Notify"
 (lambda (&rest parameters)
   (setq neomacs-clatter-service-count (1+ neomacs-clatter-service-count))
   (let* ((hints (nth 6 parameters))
          (payload `((count . ,neomacs-clatter-service-count)
                     (app-name . ,(nth 0 parameters))
                     (summary . ,(nth 3 parameters))
                     (body . ,(nth 4 parameters))
                     (actions . ,(vconcat (nth 5 parameters)))
                     (urgency . ,(car (cadr (assoc "urgency" hints))))
                     (category . ,(car (cadr (assoc "category" hints))))
                     (timeout . ,(nth 7 parameters)))))
     (with-temp-file (expand-file-name "notify.json" neomacs-clatter-service-control)
       (insert (json-encode payload))))
   '(:uint32 41)))

(with-temp-file (expand-file-name "service-ready" neomacs-clatter-service-control)
  (insert "ready"))
;; This process is a real D-Bus server. read-event dispatches its method calls.
(let ((deadline (+ (float-time) 180)))
  (while (< (float-time) deadline) (read-event nil nil 0.05)))
(kill-emacs 0)

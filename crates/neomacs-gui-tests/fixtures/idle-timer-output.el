;;; idle-timer-output.el --- Activity must preserve idleness -*- lexical-binding: t -*-

(require 'json)
(setq inhibit-startup-screen t)
(blink-cursor-mode -1)
(when (display-graphic-p)
  (menu-bar-mode -1)
  (tool-bar-mode -1)
  (set-frame-size nil 500 400 t)
  (set-face-attribute 'default nil :foreground "#000000" :background "#ffffff")
  (set-face-attribute 'mode-line nil :foreground "#000000" :background "#ff0000"))

(defvar neomacs-idle-control (getenv "NEOMACS_IDLE_TEST_CONTROL"))
(defvar neomacs-idle-ticks 0)
(defvar neomacs-idle-output 0)
(defvar neomacs-idle-tick-timer nil)
(defvar neomacs-idle-worker nil)

(defun neomacs-idle-await-ack ()
  ;; This observer is installed only after the tested idle callback fired.
  (if (file-exists-p (expand-file-name "painted" neomacs-idle-control))
      (kill-emacs 0)
    (run-at-time 0.05 nil #'neomacs-idle-await-ack)))

(defun neomacs-idle-fired ()
  (let* ((before (current-idle-time))
         (worker-live (process-live-p neomacs-idle-worker))
         (read-result (read-char nil nil 0.01))
         (after (current-idle-time))
         (preserved (and before after
                         (not (time-less-p after before)))))
    (cancel-timer neomacs-idle-tick-timer)
    (delete-process neomacs-idle-worker)
    (with-current-buffer "*idle-timer-output*"
      (goto-char (point-max))
      (insert (if preserved "IDLE-FIRED READ-TIMEOUT-PRESERVES-IDLE\n"
                "IDLE-FIRED READ-TIMEOUT-LOST-IDLE\n")))
    (with-temp-file (expand-file-name "result.json" neomacs-idle-control)
      (insert (json-encode
               `((ticks . ,neomacs-idle-ticks) (output . ,neomacs-idle-output)
                 (idle-before . ,(and before (float-time before)))
                 (idle-after . ,(and after (float-time after)))
                 (read-timeout . ,(null read-result))
                 (worker-live . ,(if worker-live t :json-false))
                 (preserved . ,(if preserved t :json-false))))))
    (when (display-graphic-p)
      (set-face-attribute 'mode-line nil :background "#00ff00")
      (neomacs--write-frame-snapshot
       (expand-file-name "final.json" neomacs-idle-control) nil 'json)
      (run-at-time 0.05 nil #'neomacs-idle-await-ack))))

(defun neomacs-idle-start ()
  (switch-to-buffer (get-buffer-create "*idle-timer-output*"))
  (erase-buffer)
  (insert "IDLE-WAITING\n")
  (setq-local mode-line-format '(" IDLE TIMER OUTPUT "))
  (setq neomacs-idle-tick-timer
        (run-at-time 0.05 0.05 (lambda () (setq neomacs-idle-ticks (1+ neomacs-idle-ticks)))))
  ;; Every output pass and ordinary timer invokes the real command wait.
  ;; The worker is bounded independently of the editor's timer scheduler.
  (setq neomacs-idle-worker
        (make-process :name "idle-pulses" :connection-type 'pipe :noquery t
                      :command '("sh" "-c" "i=0; while [ $i -lt 120 ]; do printf pulse; i=$((i+1)); sleep 0.05; done")
                      :filter (lambda (_process _text)
                                (setq neomacs-idle-output (1+ neomacs-idle-output)))))
  (run-with-idle-timer 1 nil #'neomacs-idle-fired)
  (when (display-graphic-p)
    (neomacs--write-frame-snapshot
     (expand-file-name "initial.json" neomacs-idle-control) nil 'json)))

(run-at-time 0.5 nil #'neomacs-idle-start)
(run-at-time 12 nil (lambda () (kill-emacs 2)))

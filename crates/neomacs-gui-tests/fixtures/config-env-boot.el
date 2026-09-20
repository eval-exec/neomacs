;;; config-env-boot.el --- Config-environment GUI boot probe -*- lexical-binding: t; -*-
;;
;; Shared by the Doom and Spacemacs GUI comparisons: after the config
;; framework settles, dump the selected window's visible text and frame
;; geometry to the artifacts the harness compares.  Which string to wait
;; for is passed in NEOMACS_GUI_CONFIG_HOME_NEEDLE.

(require 'cl-lib)

;; Config frameworks confirm quit (spacemacs prompts, doom advises); a
;; harness probe must exit unconditionally.
(setq confirm-kill-emacs nil)

(defun neomacs-config-boot--json-escape (value)
  "Escape VALUE for embedding inside a JSON string literal."
  ;; The spacemacs home buffer carries control characters (form-feed
  ;; separators among them); raw %S would emit them unescaped and the
  ;; harness's JSON parser rejects them.  Escape per RFC 8259.
  (let ((start 0)
        (out ""))
    (while (string-match "[\"\\\001-\037\\\\]" value start)
      (let ((ch (match-string 0 value)))
        (setq out
              (concat out
                      (substring value start (match-beginning 0))
                      (cond
                       ((string= ch "\\") "\\\\")
                       ((string= ch "\"") "\\\"")
                       ((string= ch "\n") "\\n")
                       ((string= ch "\r") "\\r")
                       ((string= ch "\t") "\\t")
                       (t (format "\\u%04x" (string-to-char ch)))))))
      (setq start (match-end 0)))
    (concat out (substring value start))))

(defun neomacs-config-boot--write-state ()
  (let* ((path (getenv "NEOMACS_GUI_STATE_JSON"))
         (buf (window-buffer (selected-window)))
         (visible (buffer-substring-no-properties
                   (window-start) (window-end nil t)))
         (payload
          (format "{\"frame\":{\"cols\":%d,\"rows\":%d,\"pixel\":\"%dx%d\"},\"buffer\":\"%s\",\"text\":\"%s\"}"
                  (frame-width) (frame-height)
                  (frame-pixel-width) (frame-pixel-height)
                  (buffer-name buf)
                  (neomacs-config-boot--json-escape visible))))
    (when path
      (let ((coding-system-for-write 'utf-8))
        (with-temp-file path (insert payload))))))

(defun neomacs-config-boot--finished-p (needle)
  (or
   ;; Some visible window names or shows the needle.
   (cl-some
    (lambda (window)
      (or (string-match-p needle (buffer-name (window-buffer window)))
          (cl-some (lambda (row)
                     (and row (string-match-p needle row)))
                   (split-string
                    (buffer-substring-no-properties
                     (window-start window) (window-end window t))
                    "\n"))))
    (window-list))
   ;; A config framework may legitimately leave every window empty (the
   ;; minimal doom fixture sets initial-scratch-message nil and ships no
   ;; dashboard without a DOOMDIR); its finish line in *Messages* is
   ;; then the only observable startup signal.  Verified on both editors:
   ;; ISM=nil, scratch empty, "Doom loaded ..." in *Messages*.
   (and (get-buffer "*Messages*")
        (string-match-p
         needle
         (with-current-buffer "*Messages*"
           (buffer-substring-no-properties (point-min) (point-max)))))))

(defvar neomacs-config-boot--deadline nil)

(defun neomacs-config-boot--tick ()
  (let ((needle (or (getenv "NEOMACS_GUI_CONFIG_HOME_NEEDLE") "SPC")))
    (cond
     ((neomacs-config-boot--finished-p needle)
      ;; Settle one more idle slice so deferred repaints land, then dump.
      (run-at-time
       2 nil
       (lambda ()
         (condition-case err
             (progn
               (neomacs-config-boot--write-state)
               (when (fboundp 'neomacs--write-frame-snapshot)
                 (neomacs--write-frame-snapshot
                  (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") t 'json))
               (let (kill-emacs-hook) (kill-emacs 0)))
           (error (message "config boot dump failed: %S" err) (kill-emacs 1))))))
     ((time-less-p nil neomacs-config-boot--deadline)
      (run-at-time 1 nil #'neomacs-config-boot--tick))
     (t
      ;; Diagnose from the artifact: dump whatever IS visible at the
      ;; deadline so a missed needle is one glance, not a mystery.
      (condition-case nil (neomacs-config-boot--write-state) (error nil))
      (message "config boot timed out waiting for %S" needle)
      (let (kill-emacs-hook) (kill-emacs 1))))))

(setq neomacs-config-boot--deadline (time-add nil 240))
(run-at-time 3 nil #'neomacs-config-boot--tick)

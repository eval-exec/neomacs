;;; font-open.el --- GNU `open-font' oracle -*- lexical-binding: t -*-

;; Both editors run against the same pinned Fontconfig catalog and the same
;; Xvfb frame. The fixture publishes the observable contract of
;; (open-font ENTITY [SIZE [FRAME]]) so the harness can require the two
;; engines to agree: opened pixel sizes for nil/0/20/float requests, the
;; font-object's public properties and metrics, the per-entity same-size
;; identity, and the error data for bad entity/size/frame arguments.

(require 'json)
(setq inhibit-startup-screen t initial-scratch-message nil create-lockfiles nil)
(menu-bar-mode -1)
(tool-bar-mode -1)
(blink-cursor-mode -1)

(defun font-open-write (value)
  (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
    (insert (json-encode value))))

(defun font-open-symbol-name (symbol)
  (and symbol (format "%s" symbol)))

(defun font-open-error (thunk)
  ;; Error data may embed a font object/spec whose printed form is not a
  ;; stable cross-engine comparison. The contract under test is the error
  ;; symbol and the predicate named by GNU's CHECK_* macro.
  (condition-case err
      (progn (funcall thunk) nil)
    (error
     (list (font-open-symbol-name (car err))
           (font-open-symbol-name (cadr err))))))

(defun font-open-probe ()
  (condition-case err
      (let* ((entity (find-font (font-spec :family "DejaVu Sans Mono")))
             (opened (and entity (open-font entity)))
             (opened-20 (and entity (open-font entity 20)))
             (opened-0 (and entity (open-font entity 0)))
             (opened-float (and entity (open-font entity 12.0)))
             (opened-float0 (and entity (open-font entity 0.0)))
             (opened-float-tiny (and entity (open-font entity 0.1)))
             (opened-float-neg5 (and entity (open-font entity -5.0)))
             (opened-neg5 (and entity (open-font entity -5)))
             (opened-neg16 (and entity (open-font entity -16)))
             (opened-20-again (and entity (open-font entity 20)))
             (opened-close (and entity (open-font entity 21)))
             (query (and opened-20 (query-font opened-20)))
             (query-nil (and opened (query-font opened)))
             (query-0 (and opened-0 (query-font opened-0)))
             (query-float (and opened-float (query-font opened-float)))
             (query-float0 (and opened-float0 (query-font opened-float0)))
             (query-float-tiny (and opened-float-tiny (query-font opened-float-tiny)))
             (query-float-neg5 (and opened-float-neg5 (query-font opened-float-neg5)))
             (query-neg5 (and opened-neg5 (query-font opened-neg5)))
             (info (and opened-20 (font-info opened-20)))
             (err-size (font-open-error
                        (lambda () (open-font entity "20"))))
             (err-spec (font-open-error
                        (lambda () (open-font (font-spec :family "DejaVu Sans Mono")))))
             (err-frame (font-open-error
                         (lambda () (open-font entity 20 5)))))
        (unless (and (display-graphic-p)
                     entity
                     opened opened-20 opened-0 opened-float
                     opened-float0 opened-float-tiny opened-float-neg5
                     opened-neg5 (null opened-neg16)
                     (equal (type-of opened-20) 'font-object)
                     (equal (aref query 2) 20)
                     (equal (aref query-0 2) 120)
                     (equal (aref query-float0 2) 120)
                     (equal (aref query-float-tiny 2) 120)
                     (equal (aref query-float-neg5 2) 1)
                     (equal (aref query-neg5 2) 1)
                     (eq opened-20 opened-20-again)
                     (null (close-font opened-close)))
          (error "open-font contract failed: %S"
                 (list :graphic (display-graphic-p)
                       :entity entity
                       :opened opened
                       :opened-20 opened-20
                       :opened-0 opened-0
                       :opened-float opened-float
                       :type (and opened-20 (type-of opened-20))
                       :query query
                       :query-0 query-0
                       :eq (and opened-20 (eq opened-20 opened-20-again))
                       :close (and opened-close (close-font opened-close)))))
        (font-open-write
         `((passed . t)
           (graphic . t)
           (entity-family . ,(font-open-symbol-name (font-get entity :family)))
           (entity-weight . ,(font-open-symbol-name (font-get entity :weight)))
           (open-nil-type . ,(font-open-symbol-name (type-of opened)))
           (open-nil-size . ,(aref query-nil 2))
           (open-nil-get-size . ,(font-get opened :size))
           (open-20-size . ,(aref query 2))
           (open-20-max-width . ,(aref query 3))
           (open-20-ascent . ,(aref query 4))
           (open-20-descent . ,(aref query 5))
           (open-20-space-width . ,(aref query 6))
           (open-20-average-width . ,(aref query 7))
           (open-20-family . ,(font-open-symbol-name (font-get opened-20 :family)))
           (open-20-weight . ,(font-open-symbol-name (font-get opened-20 :weight)))
           (open-20-get-size . ,(font-get opened-20 :size))
           (open-20-info-pixel . ,(aref info 2))
           (open-20-info-space . ,(aref info 10))
           (open-20-xlfd . ,(font-xlfd-name opened-20))
           (open-20-full-name . ,(font-get opened-20 :full-name))
           (open-20-get-name . ,(font-get opened-20 :name))
           (open-20-get-file . ,(font-get opened-20 :file))
           (open-20-info-full . ,(aref info 1))
           (open-0-size . ,(aref query-0 2))
           (open-0-get-size . ,(font-get opened-0 :size))
           (open-float-size . ,(aref query-float 2))
           (open-float-get-size . ,(font-get opened-float :size))
           (open-float0-size . ,(aref query-float0 2))
           (open-float0-get-size . ,(font-get opened-float0 :size))
           (open-float-tiny-size . ,(aref query-float-tiny 2))
           (open-float-tiny-get-size . ,(font-get opened-float-tiny :size))
           (open-float-neg5-size . ,(aref query-float-neg5 2))
           (open-float-neg5-get-size . ,(font-get opened-float-neg5 :size))
           (open-neg5-size . ,(aref query-neg5 2))
           (open-neg5-get-size . ,(font-get opened-neg5 :size))
           (open-neg16-nil . ,(if (null opened-neg16) t :json-false))
           (eq-same-size . ,(if (eq opened-20 opened-20-again) t :json-false))
           (close-returns-nil . ,(if (null (close-font opened-close)) t :json-false))
           (err-size . ,err-size)
           (err-spec . ,err-spec)
           (err-frame . ,err-frame)))
        (when (fboundp 'neomacs--write-frame-snapshot)
          (neomacs--write-frame-snapshot
           (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") nil 'json))
        (run-at-time 0.2 nil (lambda () (kill-emacs 0))))
    (error
     (font-open-write `((passed . :json-false)
                        (error . ,(format "%S" err))))
     (princ (format "open-font GUI failure: %S\n" err) 'external-debugging-output)
     (kill-emacs 1))))

(run-at-time 0.7 nil #'font-open-probe)

;;; image-size-oracle.el --- image-size / metadata GUI parity oracle -*- lexical-binding: t -*-

;; Runs identically under GNU Emacs and Neomacs on a window-system frame
;; (the GUI test harness starts Xvfb and sets DISPLAY). Computes a battery
;; of image probes and prin1's them to the file named by
;; NEOMACS_GUI_IMAGE_RESULT, so the Rust test can diff the two editors'
;; results. Raster probes are pixel-only; SVG text probes also check the
;; opened-font metrics used by packages such as svg-lib (issue #360).

(require 'svg)

;; A missing font must fail setup, not turn the metrics oracle into matching
;; nils (or matching errors caught by neomacs-image-oracle-cell).
(let ((info (font-info "Noto Sans-7")))
  (unless (and (vectorp info) (> (aref info 2) 0)
               (> (aref info 8) 0) (> (aref info 11) 0))
    (error "The SVG font oracle requires an openable Noto Sans font")))

(defconst neomacs-image-oracle-png-b64
  ;; A 5x3 solid-red PNG; the decoded bytes are the ground truth both
  ;; editors must size identically.
  "iVBORw0KGgoAAAANSUhEUgAAAAUAAAADCAIAAADUVFKvAAAAEElEQVR4nGP4z8CAjBgI8AF1yA7yBqYM2wAAAABJRU5ErkJggg==")

(defun neomacs-image-oracle-cell (name thunk)
  "Call THUNK with no args; return (NAME VALUE) or (NAME error MSG)."
  (let ((value (condition-case err
                  (funcall thunk)
                (error (list 'error (error-message-string err))))))
    (list name value)))

(defun neomacs-image-oracle-result ()
  (let ((d (base64-decode-string neomacs-image-oracle-png-b64)))
    (list
     (neomacs-image-oracle-cell
      :pixels
      (lambda () (image-size (create-image d 'png t) t)))
     (neomacs-image-oracle-cell
      :margin-2
      (lambda () (image-size (create-image d 'png t :margin 2) t)))
     (neomacs-image-oracle-cell
      :relief-3
      (lambda () (image-size (create-image d 'png t :relief 3) t)))
     ;; image-metadata now returns nil like GNU (the dual-extent geometry
     ;; moved to the neomacs-image-extent companion), so this is a real
     ;; parity probe rather than an intentional split.
     (neomacs-image-oracle-cell
      :metadata
      (lambda () (image-metadata (create-image d 'png t))))
     (neomacs-image-oracle-cell
      :flush-reread
      (lambda ()
        (let ((img (create-image d 'png t)))
          (image-size img t)
          (image-flush img)
          (image-size img t))))
     (neomacs-image-oracle-cell
      :svg-named-font-metrics
      (lambda ()
        ;; svg-lib obtains the SVG's font-size, baseline, and character
        ;; width from font-info, not from the selected window's font.
        ;; Include point and explicit-pixel names, and query twice with
        ;; different default faces to expose accidental frame-font reuse.
        (let ((height (face-attribute 'default :height)))
          (unwind-protect
              (mapcar
               (lambda (default-height)
                 (set-face-attribute 'default nil :height default-height)
                 (mapcar
                  (lambda (name)
                    (let ((info (font-info name)))
                      (seq-subseq info 2 12)))
                  '("Noto Sans-7" "Noto Sans-12" "Noto Sans-20"
                    "Noto Sans:pixelsize=9" "Noto Sans-7:dpi=144"
                    "Noto Sans")))
               '(120 200))
            (set-face-attribute 'default nil :height height)))))
     (neomacs-image-oracle-cell
      :svg-window-font-metrics
      (lambda ()
        ;; svg-lib sizes the tag background from window-font-*, which in
        ;; turn reopen the realized name returned by face-font.
        (let ((height (face-attribute 'default :height))
              (family (face-attribute 'default :family)))
          (unwind-protect
              (mapcar
               (lambda (default-height)
                 (set-face-attribute 'default nil :family "Noto Sans"
                                     :height default-height)
                 (let ((info (font-info (face-font 'default))))
                   (list (aref info 2) (aref info 3) (aref info 11)
                         (window-font-width) (window-font-height))))
               '(120 200))
            (set-face-attribute 'default nil :family family :height height)))))
     (neomacs-image-oracle-cell
      :svg-font-spec-and-entity
      (lambda ()
        (let ((spec (font-spec :family "Noto Sans" :size 20
                               :weight 'normal :slant 'normal :width 'normal)))
          (mapcar (lambda (font) (seq-subseq (font-info font) 2 12))
                  (list spec (find-font spec))))))
     (neomacs-image-oracle-cell
      :font-spec-frame-styles
      (lambda ()
        (let ((frame (selected-frame))
              (weight (face-attribute 'default :weight))
              (other (make-frame '((visibility . nil)
                                   (font . "Noto Sans-12:weight=bold")))))
          (unwind-protect
              (progn
                (set-face-attribute 'default frame :weight 'normal)
                (set-face-attribute 'default other :weight 'bold)
                (mapcar
                 (lambda (target)
                   ;; Realize the modified default before font-info reads
                   ;; GNU's cached DEFAULT_FACE_ID for this frame.
                   (face-font 'default target)
                   (mapcar
                    (lambda (spec-weight)
                      (aref (font-info (font-spec :family "Noto Sans" :weight spec-weight)
                                       target)
                            0))
                    '(normal bold)))
                 (list frame other)))
            (delete-frame other t)
            (set-face-attribute 'default frame :weight weight)))))
     (neomacs-image-oracle-cell
      :missing-font
      (lambda ()
        (list (font-info "NeomacsIssue360NonexistentFamily-7")
              (font-info (font-spec :family "NeomacsIssue360NonexistentFamily")))))
     (neomacs-image-oracle-cell
      :unspecified-font-family
      (lambda () (seq-subseq (font-info (font-spec)) 0 12)))
     (neomacs-image-oracle-cell
      :svg-tag
      (lambda ()
        ;; Exercise svg-lib's public font-info -> SVG text -> image path
        ;; without a network package dependency. Explicit 1:1 scaling keeps
        ;; this probe about fonts, independently of default image scaling.
        (let* ((info (font-info "Noto Sans-7"))
               (svg (svg-create 80 24)))
          (svg-rectangle svg 0 0 80 24 :fill "purple" :rx 8)
          (svg-text svg "TODO" :font-family "Noto Sans"
                    :font-size (aref info 2) :fill "white"
                    :x (- 40 (* 2 (aref info 11))) :y (aref info 8))
          (let ((image (svg-image svg :scale 1 :ascent 79)))
            (with-current-buffer (get-buffer-create "*SVG tag oracle*")
              (erase-buffer)
              (insert-image image)
              (switch-to-buffer (current-buffer)))
            (list (plist-get (cdr image) :data) (image-size image t)))))))))

(defun neomacs-image-oracle-write ()
  (let ((path (getenv "NEOMACS_GUI_IMAGE_RESULT")))
    (when path
      (make-directory (file-name-directory path) t)
      (with-temp-file path
        (prin1 (neomacs-image-oracle-result) (current-buffer))
        (insert "\n")))))

;; Compute and write during load: a window-system frame already exists at
;; this point under the GUI harness (the font selection oracle relies on
;; the same top-level timing). Defer kill-emacs to the event loop: calling
;; it at top-level mid-init hangs GNU's GTK teardown.
(neomacs-image-oracle-write)
(run-at-time 2 nil (lambda () (kill-emacs 0)))

;;; image-size-oracle.el ends here

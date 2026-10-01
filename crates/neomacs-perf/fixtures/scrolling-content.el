;;; scrolling-content.el --- Shared rich scrolling content -*- lexical-binding: t; -*-

(defvar-local neomacs-scroll-content-summary nil)

(defun neomacs-scroll-content-insert (lines)
  "Insert LINES of deterministic mixed-font text and overlapping overlays.
Construction is outside timing. GUI font families must actually be installed;
Unicode fallback within those families remains the editor's responsibility."
  (let* ((graphical (display-graphic-p))
         (available (if graphical (font-family-list)
                      '("DejaVu Sans Mono" "DejaVu Serif" "DejaVu Sans")))
         (families nil)
         (overlay-count 0))
    (dolist (family '("DejaVu Sans Mono" "DejaVu Serif" "DejaVu Sans"
                      "Liberation Mono" "Liberation Serif" "Liberation Sans"
                      "Consolas" "Arial" "Times New Roman"
                      "Menlo" "Helvetica" "Times" "Georgia"))
      (when (member family available)
        (setq families (append families (list family)))))
    (unless (>= (length families) 3)
      (error "Rich scrolling requires three installed font families: %S" families))
    (let* ((faces
            (vector
             `(:family ,(nth 0 families) :height 100 :weight normal :foreground "#b0c4de")
             `(:family ,(nth 1 families) :height 180 :weight bold :foreground "gold")
             `(:family ,(nth 2 families) :height 130 :slant italic :underline (:style wave :color "red"))
             `(:family ,(nth 0 families) :height 85 :weight light :background "#303050" :extend t)
             `(:family ,(nth 1 families) :height 150 :overline t :strike-through t :foreground "cyan")
             `(:family ,(nth 2 families) :height 120 :box (:line-width 2 :color "orange") :inverse-video t)))
           ;; Repeated property-bearing blocks avoid timing fixture construction
           ;; or making setup cost proportional to Lisp calls per character.
           (block
            (with-temp-buffer
              (dotimes (i 1000)
                (let ((start (point)))
                  (insert (format "Line %03d -- buffer window layout office affinity " i))
                  (insert (if (= (% i 16) 0)
                              (make-string 180 ?W)
                            "café á 好好 שלום سلام"))
                  (insert "\tredisplay overlay glyph cache\n")
                  (put-text-property start (point) 'face (aref faces (% i 6)))
                  (put-text-property (+ start 12) (+ start 26) 'face
                                     (list (aref faces (% (+ i 1) 6)) '(:weight bold)))
                  (put-text-property (+ start 27) (+ start 43) 'face (aref faces (% (+ i 2) 6)))
                  (when (= (% i 7) 0)
                    (put-text-property (1- (point)) (point) 'line-height 1.3))
                  (when (= (% i 11) 0)
                    (put-text-property (+ start 5) (+ start 8) 'display '(raise 0.2)))))
              (buffer-string))))
      (dotimes (_ (/ lines 1000)) (insert block))
      (when (> (% lines 1000) 0)
        (insert (with-temp-buffer
                  (insert block)
                  (goto-char (point-min))
                  (forward-line (% lines 1000))
                  (buffer-substring (point-min) (point)))))
      (setq-local truncate-lines nil)
      (setq-local word-wrap t)
      (setq-local buffer-invisibility-spec '((neomacs-scroll-fold . t)))
      (save-excursion
        (goto-char (point-min))
        (dotimes (i (/ (+ lines 31) 32))
          (let* ((start (point))
                 (end (save-excursion (forward-line 3) (point)))
                 (outer (make-overlay start end))
                 (inner (make-overlay (+ start 12) (+ start 38)))
                 (insertion (make-overlay (+ start 20) (+ start 20)))
                 (special (make-overlay (+ start 40) (+ start 46))))
            (overlay-put outer 'priority 10)
            (overlay-put outer 'face '(:background "#243040" :extend t))
            (overlay-put inner 'priority 30)
            (overlay-put inner 'face (aref faces (% (+ i 3) 6)))
            (overlay-put inner 'mouse-face 'highlight)
            (overlay-put inner 'help-echo "deterministic nested scrolling overlay")
            (overlay-put insertion 'before-string
                         (propertize "[before]" 'face (aref faces (% (+ i 1) 6))))
            (overlay-put insertion 'after-string
                         (propertize "[after]" 'face (aref faces (% (+ i 2) 6))))
            (overlay-put special 'priority 40)
            (if (= (% i 2) 0)
                (overlay-put special 'display
                             (propertize "replacement" 'face (aref faces (% (+ i 4) 6))))
              (overlay-put special 'invisible 'neomacs-scroll-fold))
            (setq overlay-count (+ overlay-count 4)))
          (forward-line 32)))
      (setq neomacs-scroll-content-summary
            `((profile . "rich-v1") (lines . ,lines)
              (font-families . ,(vector (nth 0 families) (nth 1 families) (nth 2 families)))
              (font-selection . ,(if graphical "installed" "terminal-face-attributes-only"))
              (face-variants . ,(length faces))
              (overlays . ,overlay-count))))))

(provide 'scrolling-content)

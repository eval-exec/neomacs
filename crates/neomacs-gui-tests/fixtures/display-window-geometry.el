;;; display-window-geometry.el --- Real GUI geometry contracts -*- lexical-binding: t; -*-
(run-at-time
 1 nil
 (lambda ()
   (condition-case err
       (progn
         (unless (display-graphic-p) (error "Expected a graphical frame"))
         (if (equal (getenv "NEOMACS_GEOMETRY_PROBE") "desktop")
             (let* ((monitors (display-monitor-attributes-list))
                    (geometry (cdr (assq 'geometry (car monitors))))
                    (width (display-pixel-width))
                    (height (display-pixel-height)))
               (unless (and geometry (> width 80) (> height 25))
                 (error "Invalid graphical desktop: %S %S %S" width height geometry)))
           (setq window-resize-pixelwise t)
           (split-window-below)
           (let* ((window (selected-window))
                  (before (window-pixel-height window)))
             (window-resize window 1 nil nil t)
             (unless (= (window-pixel-height window) (1+ before))
               (error "One-pixel resize lost: before=%S after=%S"
                      before (window-pixel-height window)))
             (window-resize window 0 nil nil t)
             (unless (= (window-pixel-height window) (1+ before))
               (error "Zero-delta resize changed pixel geometry"))))
         (princ "GEOMETRY-PASS\n" 'external-debugging-output)
         (kill-emacs 0))
     (error
      (princ (format "GEOMETRY-FAIL %S\n" err) 'external-debugging-output)
      (kill-emacs 1)))))

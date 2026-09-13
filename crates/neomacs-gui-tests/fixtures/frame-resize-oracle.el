;;; frame-resize-oracle.el --- Shared GNU/Neomacs resize oracle -*- lexical-binding: t -*-
(run-at-time
 1 nil
 (lambda ()
   (let ((before (frame-pixel-height)))
     (set-frame-width nil 91)
     (run-at-time
      1 nil
      (lambda ()
        (princ (format "RESIZE-ORACLE columns=%S height-before=%S height-after=%S\n"
                       (frame-width) before (frame-pixel-height))
               'external-debugging-output)
        (kill-emacs (if (and (= (frame-width) 91)
                            (= before (frame-pixel-height))) 0 1)))))))

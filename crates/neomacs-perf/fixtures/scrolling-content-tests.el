;;; scrolling-content-tests.el --- Rich workload contract -*- lexical-binding: t; -*-
(require 'ert)
(require 'cl-lib)
(load (expand-file-name "scrolling-content.el" (file-name-directory load-file-name)) nil nil t)

(ert-deftest neomacs-scroll-content-keeps-faces-and-overlays-across-blocks ()
  ;; Test construction independently of the machine's font installation.
  ;; Native GUI tests separately require installed families.
  (cl-letf (((symbol-function 'display-graphic-p) (lambda (&rest _) t))
            ((symbol-function 'font-family-list)
             (lambda (&rest _) '("DejaVu Sans Mono" "DejaVu Serif" "DejaVu Sans"))))
    (with-temp-buffer
      (neomacs-scroll-content-insert 1033)
      (should (= (count-lines (point-min) (point-max)) 1033))
      (should (= (length (overlays-in (point-min) (point-max))) 132))
      (let ((initial-face (get-text-property (point-min) 'face))
            (mixed-face (get-text-property (+ (point-min) 12) 'face)))
        (should-not (equal initial-face mixed-face))
        (goto-char (point-min))
        (forward-line 1000)
        (should (equal initial-face (get-text-property (point) 'face)))
        (should (equal mixed-face (get-text-property (+ (point) 12) 'face))))
      (let ((overlays (overlays-in (point-min) (point-max))))
        (dolist (property '(before-string after-string display invisible priority mouse-face))
          (should (cl-some (lambda (overlay) (overlay-get overlay property)) overlays))))
      (should (= (length (alist-get 'font-families neomacs-scroll-content-summary)) 3))
      (should (= (alist-get 'overlays neomacs-scroll-content-summary) 132)))))

(ert-deftest neomacs-scroll-content-refuses-silent-font-substitution ()
  (cl-letf (((symbol-function 'display-graphic-p) (lambda (&rest _) t))
            ((symbol-function 'font-family-list) (lambda (&rest _) '("Only One Font"))))
    (with-temp-buffer
      (should-error (neomacs-scroll-content-insert 10))
      (should (= (buffer-size) 0)))))

(ert-deftest neomacs-scroll-content-labels-terminal-font-limitations ()
  (cl-letf (((symbol-function 'display-graphic-p) (lambda (&rest _) nil))
            ((symbol-function 'font-family-list) (lambda (&rest _) (error "no GUI fonts"))))
    (with-temp-buffer
      (neomacs-scroll-content-insert 33)
      (should (= (count-lines (point-min) (point-max)) 33))
      (should (equal (alist-get 'font-selection neomacs-scroll-content-summary)
                     "terminal-face-attributes-only")))))

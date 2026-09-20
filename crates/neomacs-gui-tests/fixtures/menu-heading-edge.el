;;; menu-heading-edge.el --- Menu-bar dropdown constrained by right edge -*- lexical-binding: t -*-
(load (expand-file-name "menu-interaction.el" (file-name-directory load-file-name)))
(let ((bar (make-sparse-keymap)) (spacer (make-sparse-keymap)))
  (define-key bar [mouse-1] #'menu-bar-open-mouse)
  (define-key spacer [noop] '(menu-item "Spacer action" ignore))
  (define-key bar [edge] (list 'menu-item "中中中中" neomacs-menu-test-map))
  ;; Push the CJK heading near the right edge with a long ASCII label.
  (define-key bar [spacer]
    (list 'menu-item (make-string (/ 864 (frame-char-width)) ?x) spacer))
  (define-key global-map [menu-bar] bar))

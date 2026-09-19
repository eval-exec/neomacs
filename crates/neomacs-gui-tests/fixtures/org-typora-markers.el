;;; org-typora-markers.el --- Reporter configuration from issue #379 -*- lexical-binding: nil -*-
;; https://github.com/eval-exec/neomacs/issues/379
;; Function bodies unchanged; explanatory comments omitted.
(defvar-local my/org-typora-mode-enabled nil)
(defvar-local my/org-typora-mode--current-line-beg nil)
(defvar-local my/org-typora-mode--user-point nil)
(defun my/org-typora-mode--marker-p (pos)
    "Return non-nil when POS is an actual Org emphasis marker.
In the current Org implementation, emphasis content and
emphasis markers both have `org-emphasis' = t.
Markers additionally have no `face'."
    (and (eq (get-char-property pos 'org-emphasis) t)
        (null (get-char-property pos 'face))))
(defun my/org-typora-mode--set-line-visible (beg end visible)
    "Show or hide Org emphasis markers in BEG..END.
Only characters satisfying `my/org-typora-mode--marker-p'
are modified."
    (when (and beg
              end
              (< beg end))
        (with-silent-modifications
            (let ((pos beg))
                (while (< pos end)
                    (when (my/org-typora-mode--marker-p pos)
                        (put-text-property
                            pos
                            (1+ pos)
                            'invisible
                            (unless visible t)))
                    (setq pos (1+ pos)))))))
(defun my/org-typora-mode--show-current-line ()
    "Show emphasis markers on the current line."
    (my/org-typora-mode--set-line-visible
        (line-beginning-position)
        (line-end-position)
        t))
(defun my/org-typora-mode--hide-line (beg)
    "Hide emphasis markers on the line beginning at BEG."
    (when (and beg
              (marker-position beg))
        (save-excursion
            (goto-char (marker-position beg))
            (my/org-typora-mode--set-line-visible
                (line-beginning-position)
                (line-end-position)
                nil))))
(defun my/org-typora-mode--remember-point ()
    "Remember the actual user point."
    (when my/org-typora-mode--user-point
        (set-marker
            my/org-typora-mode--user-point
            (point))))
(defun my/org-typora-mode--font-lock-show-current-line (limit)
    "Reveal emphasis markers on the user's current line."
    (when (and my/org-typora-mode-enabled
              my/org-typora-mode--user-point)
        (let ((user-pos
                  (marker-position my/org-typora-mode--user-point)))
            (when user-pos
                (save-excursion
                    (goto-char user-pos)
                    (let ((beg (line-beginning-position))
                             (end (min (line-end-position) limit)))
                        (when (< beg end)
                            (my/org-typora-mode--set-line-visible
                                beg end t)))))))
    nil)
(defun my/org-typora-mode--post-command ()
    "Show markers on the new current line and hide the old line."
    (when my/org-typora-mode-enabled
        (my/org-typora-mode--remember-point)
        (let ((new-line
                  (line-beginning-position)))
            (unless (and my/org-typora-mode--current-line-beg
                        (marker-position
                            my/org-typora-mode--current-line-beg)
                        (= new-line
                            (marker-position
                                my/org-typora-mode--current-line-beg)))
                (when (and my/org-typora-mode--current-line-beg
                          (marker-position
                              my/org-typora-mode--current-line-beg))
                    (my/org-typora-mode--hide-line
                        my/org-typora-mode--current-line-beg))
                (when my/org-typora-mode--current-line-beg
                    (set-marker
                        my/org-typora-mode--current-line-beg
                        nil))
                (setq-local
                    my/org-typora-mode--current-line-beg
                    (copy-marker new-line))
                (my/org-typora-mode--show-current-line)))))
(defun my/org-typora-mode-enable ()
    "Enable the experimental Typora-style Org marker behavior."
    (interactive)
    (when (derived-mode-p 'org-mode)
        (setq-local org-hide-emphasis-markers t)
        (setq-local my/org-typora-mode-enabled t)
        (setq-local
            my/org-typora-mode--user-point
            (copy-marker (point)))
        (setq-local
            my/org-typora-mode--current-line-beg
            (copy-marker
                (line-beginning-position)))
        (font-lock-add-keywords
            nil
            '((my/org-typora-mode--font-lock-show-current-line))
            'append)
        (add-hook 'post-command-hook
            #'my/org-typora-mode--post-command
            nil
            t)
        (font-lock-flush)
        (font-lock-ensure)
        (my/org-typora-mode--show-current-line)
        (message "Typora TEST enabled")))
(defun my/org-typora-mode-disable ()
    "Disable the experimental Typora-style Org marker behavior."
    (interactive)
    (when (derived-mode-p 'org-mode)
        (remove-hook 'post-command-hook
            #'my/org-typora-mode--post-command
            t)
        (font-lock-remove-keywords
            nil
            '((my/org-typora-mode--font-lock-show-current-line)))
        (when my/org-typora-mode--user-point
            (set-marker
                my/org-typora-mode--user-point
                nil))
        (when my/org-typora-mode--current-line-beg
            (set-marker
                my/org-typora-mode--current-line-beg
                nil))
        (setq-local my/org-typora-mode--user-point nil)
        (setq-local my/org-typora-mode--current-line-beg nil)
        (setq-local my/org-typora-mode-enabled nil)
        (setq-local org-hide-emphasis-markers nil)
        (font-lock-flush)
        (font-lock-ensure)
        (message "Typora TEST disabled")))
(defun my/org-typora-mode-toggle ()
    "Toggle the experimental Typora-style marker behavior."
    (interactive)
    (if my/org-typora-mode-enabled
        (my/org-typora-mode-disable)
        (my/org-typora-mode-enable)))
(with-eval-after-load 'org
    (define-key org-mode-map
        (kbd "C-c t")
        #'my/org-typora-mode-toggle)
    (add-hook 'org-mode-hook
        #'my/org-typora-mode-enable))

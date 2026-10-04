;;; issue-458-ime.el --- Native XIM regression -*- lexical-binding: t -*-
(load (expand-file-name "native-frame-focus.el"
                        (file-name-directory load-file-name)) nil t)
(global-set-key [f13]
                (lambda () (interactive)
                  (with-current-buffer "*focus-secondary*" (insert "F13"))))
(advice-add 'neomacs-focus-tick :before
            (lambda ()
              (when (and (file-exists-p
                          (expand-file-name "stop" neomacs-focus-directory))
                         (fboundp 'neomacs--write-frame-snapshot))
                (neomacs--write-frame-snapshot
                 (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_TXT") t 'text))))

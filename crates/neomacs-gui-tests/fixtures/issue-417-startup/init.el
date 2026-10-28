;;; init.el --- Observe the regular init boundary -*- lexical-binding: t; -*-
(issue-417-record "init")
(unless (and (> (display-pixel-width) 0) (> (display-pixel-height) 0))
  (error "GUI display has no pixel dimensions during init"))

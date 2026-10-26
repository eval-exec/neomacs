;;; Issue #600 startup frame defaults. -*- lexical-binding: t; -*-
(setq issue-600-early-init-count (1+ (if (boundp 'issue-600-early-init-count)
                                      issue-600-early-init-count 0)))
(push '(menu-bar-lines . 0) default-frame-alist)
(push '(tool-bar-lines . 0) default-frame-alist)
(push '(vertical-scroll-bars) default-frame-alist)

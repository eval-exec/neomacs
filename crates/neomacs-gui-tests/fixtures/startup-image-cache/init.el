;;; init.el --- Continue normal startup after the early cache query -*- lexical-binding: t; -*-

(unless (fboundp 'startup-image-cache-record)
  (error "Startup image-cache fixture did not load early-init.el"))

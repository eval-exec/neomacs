;;; native-resize-decorations.el --- Decorated fullscreen control -*- lexical-binding: t -*-
(setq resize-increments-decorated t)
(load (expand-file-name "native-resize-increments.el"
                        (file-name-directory load-file-name)) nil t)

pub(super) const HELM_PYDOC_TUI_PRELUDE: &str = r####"
(defun neomacs-helm-pydoc-tui-write (path contents)
  (make-directory (file-name-directory path) t)
  (let ((dbg (getenv "NEOMACS_HELM_PYDOC_OVERLAY_STATE")))
    (when (and dbg (string-match-p "helm-overlay" path))
      (with-temp-file (concat (file-name-directory dbg) "write-calls.log")
        (let ((prior (condition-case e (with-temp-buffer (insert-file-contents (concat (file-name-directory dbg) "write-calls.log")) (buffer-string)) (error ""))))
          (erase-buffer) (insert prior) (goto-char (point-max)) (insert (format "WRITE %s\n" path))))))
  (with-temp-file path
    (insert contents))
  path)

(defun neomacs-helm-pydoc-tui-setup ()
  (require 'helm-pydoc)
  (let* ((root (file-name-as-directory (getenv "HOME")))
         (project (expand-file-name "release workspace/" root))
         (python (expand-file-name "venv/bin/python" project))
         (python-fixture (expand-file-name "python-fixture/" project))
         (log (expand-file-name "python-invocations.log" root))
         (source (expand-file-name "release_console.py" project))
         (module-source (expand-file-name "deploymentkit.py" project)))
    (neomacs-helm-pydoc-tui-write
     python
     (mapconcat
      #'identity
      '("#!/bin/sh"
        "case \"$1\" in"
        "  */helm-pydoc.py)"
        "    printf 'collect|%s\\n' \"${1##*/}\" >> \"$NEOMACS_HELM_PYDOC_LOG\""
        "    if [ \"${NEOMACS_HELM_PYDOC_FAIL_COLLECT:-0}\" = 1 ]; then"
        "      printf '%s\\n' 'collector unavailable' >&2"
        "      exit 19"
        "    fi"
        "    PYTHONPATH=\"$NEOMACS_HELM_PYDOC_PYTHON_FIXTURE\" exec python3 -S \"$@\""
        "    ;;"
        "  -m)"
        "    printf 'pydoc|%s|%s|%s\\n' \"$1\" \"$2\" \"$3\" >> \"$NEOMACS_HELM_PYDOC_LOG\""
        "    if [ \"${NEOMACS_HELM_PYDOC_FAIL_DOCS:-}\" = \"$3\" ]; then"
        "      printf '%s\\n' \"No Python documentation found for $3\" >&2"
        "      exit 23"
        "    fi"
        "    printf 'Help on package %s:\\n\\nNAME\\n    %s - Release deployment helpers.\\n\\nFUNCTIONS\\n    promote(release, region=\"prod\")\\n        Promote one release after policy validation.\\n' \"$3\" \"$3\""
        "    ;;"
        "  -c)"
        "    printf 'source|%s|%s\\n' \"$1\" \"$2\" >> \"$NEOMACS_HELM_PYDOC_LOG\""
        "    PYTHONPATH=\"$NEOMACS_HELM_PYDOC_PROJECT\" exec python3 -S \"$@\""
        "    ;;"
        "  *)"
        "    printf 'unexpected' >&2"
        "    exit 97"
        "    ;;"
        "esac"
        "")
      "\n"))
    (set-file-modes python #o755)
    (neomacs-helm-pydoc-tui-write
     (expand-file-name "pkgutil.py" python-fixture)
     "def iter_modules():\n    return [(None, 'deploymentkit', False), (None, 'json', False), (None, 'analytics', False)]\n")
    (neomacs-helm-pydoc-tui-write log "")
    (neomacs-helm-pydoc-tui-write
     module-source
     "\"\"\"Release deployment helpers.\"\"\"\n\ndef promote(release, region=\"prod\"):\n    \"\"\"Promote one release after policy validation.\"\"\"\n    return release, region\n")
    (neomacs-helm-pydoc-tui-write
     source
     "# Release operations console\nimport json\nfrom os import path\n\nrelease = {\"id\": \"candidate-42\"}\n")
    (setenv "NEOMACS_HELM_PYDOC_LOG" log)
    (setenv "NEOMACS_HELM_PYDOC_PROJECT" project)
    (setenv "NEOMACS_HELM_PYDOC_PYTHON_FIXTURE" python-fixture)
    (setq helm-pydoc-virtualenv "venv"
          helm-input-idle-delay 0
          helm-candidate-number-limit 20)
    ;; Overlay introspection: helm hooks write the helm buffer's overlay
    ;; state (start/end/face/priority per overlay, plus the selection
    ;; point) to a per-engine file on every selection move and buffer
    ;; update.  The harness compares the reports after the action menu
    ;; opens; identical state with differing rendering means the RENDERER
    ;; dropped the face, while differing state points at helm's elisp
    ;; machinery.
    (let ((dump (expand-file-name "helm-overlay-state.txt" root)))
      (setenv "NEOMACS_HELM_PYDOC_OVERLAY_STATE" (expand-file-name "helm-overlay-state.txt" root))
      (condition-case setup-err
          (progn
            (defun neomacs-helm-pydoc-overlay-dump ()
              (condition-case dump-err
                  (let ((helm-buf (and (boundp 'helm-buffer)
                                       (get-buffer helm-buffer)))
                        (fired-log (expand-file-name "helm-observer-fired.log"
                                                     (getenv "HOME"))))
                    (let ((prior (condition-case e
                                    (with-temp-buffer
                                      (insert-file-contents fired-log)
                                      (buffer-string))
                                  (error ""))))
                      (neomacs-helm-pydoc-tui-write
                       fired-log
                       (concat prior
                               (format "FIRED at point-max=%S\n"
                                       (buffer-live-p helm-buf)))))
                    (when (buffer-live-p helm-buf)
                      (let* ((windows
                              (mapcar
                               (lambda (w)
                                 (format
                                  "win buf=%S start=%S point=%S hscroll=%S\n"
                                  (buffer-name (window-buffer w))
                                  (window-start w)
                                  (window-point w)
                                  (window-hscroll w)))
                               (window-list nil 'no-mini)))
                             (report
                              (with-current-buffer helm-buf
                                (format
                                 "point=%S point-min=%S point-max=%S selection=%S mode=%S read-only=%S\n"
                                 (point) (point-min) (point-max)
                                 (and (boundp 'helm-selection-point) helm-selection-point)
                                 major-mode buffer-read-only))))
                        (neomacs-helm-pydoc-tui-write
                         (getenv "NEOMACS_HELM_PYDOC_OVERLAY_STATE")
                         (concat
                          (mapconcat #'identity windows "")
                          report
                          (mapconcat
                           (lambda (ov)
                             (format
                              "ov start=%S end=%S face=%S priority=%S window=%S\n"
                              (overlay-start ov) (overlay-end ov)
                              (overlay-get ov 'face) (overlay-get ov 'priority)
                              (overlay-get ov 'window)))
                           (with-current-buffer helm-buf
                             (overlays-in (point-min) (point-max)))
                           "")))
                        ;; Every window's buffer, not just helm's: the action
                        ;; selection screen highlights a row in the ACTIONS
                        ;; window, and the helm-buffer introspection above
                        ;; would not see that state at all.
                        (neomacs-helm-pydoc-tui-write
                         (expand-file-name "helm-window-overlays.txt" (getenv "HOME"))
                         (mapconcat
                          (lambda (w)
                            (with-current-buffer (window-buffer w)
                              (concat
                               (format
                                "WIN buf=%S mode=%S point=%S\n"
                                (buffer-name) major-mode (point))
                               (mapconcat
                                (lambda (ov)
                                  (format
                                   "  ov start=%S end=%S face=%S priority=%S window=%S text=%S\n"
                                   (overlay-start ov) (overlay-end ov)
                                   (overlay-get ov 'face) (overlay-get ov 'priority)
                                   (overlay-get ov 'window)
                                   (and (overlay-start ov) (overlay-end ov)
                                        (buffer-substring-no-properties
                                         (overlay-start ov) (overlay-end ov)))))
                                (overlays-in (point-min) (point-max))
                                ""))))
                          (window-list nil 'no-mini)
                          "")))))
                (error (neomacs-helm-pydoc-tui-write
                        (expand-file-name "helm-overlay-dump-error.txt" (getenv "HOME"))
                        (format "DUMP-ERR: %S" dump-err)))))
            (add-hook 'helm-move-selection-after-hook #'neomacs-helm-pydoc-overlay-dump)
            (add-hook 'helm-after-update-hook #'neomacs-helm-pydoc-overlay-dump)
            ;; Converge: a fast idle dump so the file reflects the CURRENT
            ;; state (the hooks alone can lag the screen by one update).
            (run-with-idle-timer
             0.25 0.25 #'neomacs-helm-pydoc-overlay-dump))
        (error (neomacs-helm-pydoc-tui-write
                (expand-file-name "helm-overlay-setup-error.txt" root)
                (format "SETUP-ERR: %S" setup-err))))
      (neomacs-helm-pydoc-tui-write dump "OVERLAY-OBSERVER-INSTALLED"))
    (find-file source)
    (goto-char (point-max))))

(add-hook 'emacs-startup-hook #'neomacs-helm-pydoc-tui-setup 100)
"####;

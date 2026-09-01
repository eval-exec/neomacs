use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_keys_speed_babel_keymap_dispatch_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((([S-left] \"C-c <left>\") ([S-right] \"C-c <right>\") (\"\t\" \"TAB\")) ((\"C-c <left>\" translated-left) (\"C-c <right>\" translated-right) (\"TAB\" local-tab) (\"S-<left>\" nil)) (org-forward-element org-backward-element) ((\"TAB\" org-cycle) (\"C-c C-x\" (keymap (73 . org-info-find-node) (91 . org-reftex-citation) (64 . org-cite-insert) (71 . org-feed-goto-inbox) (103 . org-feed-update-all) (33 . org-reload) (3 . org-columns) (44 . org-timer-pause-or-continue) (59 . org-timer-set-timer) (95 . org-timer-stop) (48 . org-timer-start) (45 . org-timer-item) (46 . org-timer) (111 . org-toggle-ordered-property) (69 . org-inc-effort) (101 . org-set-effort) (80 . org-set-property-and-value) (112 . org-set-property) (18 . org-toggle-radio-button) (2 . org-toggle-checkbox) (92 . org-toggle-pretty-entities) (22 . org-link-preview) (12 . org-latex-preview) (21 . org-dblock-update) (120 . org-dynamic-block-insert-dblock) (4 . org-clock-display) (17 . org-clock-cancel) (10 . org-clock-goto) (15 . org-clock-out) (26 . org-resolve-clocks) (24 . org-clock-in-last) (9 . org-clock-in) (20 . org-toggle-timestamp-overlays) (25 . org-paste-special) (27 keymap (22 . org-link-preview-refresh) (119 . org-copy-special)) (23 . org-cut-special) (102 . org-footnote-action) (6 . org-emphasize) (62 . org-agenda-remove-restriction-lock) (60 . org-agenda-set-restriction-lock) (16 . org-previous-link) (14 . org-next-link) (118 . org-copy-visible) (113 . org-toggle-tags-groups) (98 . org-tree-to-indirect-buffer) (65 . org-archive-to-archive-sibling) (97 . org-toggle-archive-tag) (1 . org-archive-subtree-default) (19 . org-archive-subtree) (left . org-shiftcontrolleft) (right . org-shiftcontrolright) (68 . org-shiftmetadown) (85 . org-shiftmetaup) (82 . org-shiftmetaright) (76 . org-shiftmetaleft) (100 . org-insert-drawer) (117 . org-metaup) (114 . org-metaright) (108 . org-metaleft) (13 . org-meta-return) (115 . org-insert-structure-template) (77 . org-insert-todo-heading) (109 . org-meta-return) (99 . org-clone-subtree-with-time-shift))) (\"C-c C-x C-b\" org-toggle-checkbox) (\"C-c C-v n\" org-babel-next-src-block) (\"C-c C-v e\" org-babel-execute-maybe) (\"C-c C-v v\" org-babel-expand-src-block)) ((\"n\" org-speed-move-safe) (\"p\" org-speed-move-safe) (\"?\" org-speed-command-help) (\"x\" nil)) (nil nil) ((\"n\" org-babel-next-src-block) (\"e\" org-babel-execute-maybe) (\"v\" org-babel-expand-src-block) (\"x\" org-babel-do-key-sequence-in-edit-buffer) (\"z\" org-babel-switch-to-session-with-code)) (nil nil) (ok \"** Beta\") \"Speed commands\\n==============\\n\\nGroup\\n-----\\nn   org-next-visible-heading\\ne   (org-entry-put (point) \\\"X\\\" \\\"Y\\\")\\n?   org-speed-command-help\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-keys)
  (require 'ob-core)
  (with-temp-buffer
    (let ((org-replace-disputed-keys t)
          (org-disputed-keys
           `((,(kbd "S-<left>") . ,(kbd "C-c <left>"))
             (,(kbd "S-<right>") . ,(kbd "C-c <right>"))))
          (org-use-speed-commands t))
      (org-mode)
      (insert "* Alpha\n")
      (insert "#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n")
      (insert "** Beta\nBody\n")
      (let* ((local-map (make-sparse-keymap))
             (remap-map (make-sparse-keymap))
             (speed-head nil)
             (speed-body nil)
             (babel-head nil)
             (babel-body nil)
             (safe-forward nil)
             help)
        (org-defkey local-map (kbd "S-<left>") 'translated-left)
        (org-defkey local-map (kbd "S-<right>") 'translated-right)
        (org-defkey local-map (kbd "TAB") 'local-tab)
        (org-remap remap-map
                   'forward-word 'org-forward-element
                   'backward-word 'org-backward-element)
        (goto-char (point-min))
        (setq speed-head
              (mapcar (lambda (key)
                        (let ((handler
                               (org-speed-command-activate key)))
                          (list key
                                (cond ((symbolp handler) handler)
                                      ((consp handler) (car handler))
                                      ((functionp handler) 'function)
                                      (t handler)))))
                      '("n" "p" "?" "x")))
        (forward-char 2)
        (setq speed-body
              (mapcar #'org-speed-command-activate '("n" "?")))
        (goto-char (point-min))
        (search-forward "#+begin_src")
        (beginning-of-line)
        (setq babel-head
              (mapcar (lambda (key)
                        (let ((handler
                               (org-babel-speed-command-activate key)))
                          (list key
                                (cond ((symbolp handler) handler)
                                      ((consp handler) (car handler))
                                      ((functionp handler) 'function)
                                      (t handler)))))
                      '("n" "e" "v" "x" "z")))
        (forward-char 2)
        (setq babel-body
              (mapcar #'org-babel-speed-command-activate '("n" "e")))
        (goto-char (point-min))
        (setq safe-forward
              (condition-case err
                  (progn
                    (org-speed-move-safe 'org-next-visible-heading)
                    (list 'ok
                          (buffer-substring-no-properties
                           (line-beginning-position)
                           (line-end-position))))
                (error (cons (car err) (cdr err)))))
        (setq help
              (let ((org-speed-commands
                     '(("Group")
                       ("n" . org-next-visible-heading)
                       ("e" . (org-entry-put (point) "X" "Y"))
                       ("?" . org-speed-command-help))))
                (org-speed-command-help)
                (prog1
                    (with-current-buffer "*Help*"
                      (buffer-substring-no-properties
                       (point-min) (point-max)))
                  (when (get-buffer "*Help*")
                    (kill-buffer "*Help*")))))
        (list (mapcar (lambda (key)
                        (list key
                              (key-description (org-key key))))
                      (list (kbd "S-<left>")
                            (kbd "S-<right>")
                            (kbd "TAB")))
              (mapcar (lambda (key)
                        (list (key-description key)
                              (lookup-key local-map key)))
                      (list (kbd "C-c <left>")
                            (kbd "C-c <right>")
                            (kbd "TAB")
                            (kbd "S-<left>")))
              (list (lookup-key remap-map [remap forward-word])
                    (lookup-key remap-map [remap backward-word]))
              (mapcar (lambda (key)
                        (list key (lookup-key org-mode-map (kbd key))))
                      '("TAB" "C-c C-x" "C-c C-x C-b"
                        "C-c C-v n" "C-c C-v e" "C-c C-v v"))
              speed-head
              speed-body
              babel-head
              babel-body
              safe-forward
              help)))))"##,
        expect,
    );
}

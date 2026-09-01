use expect_test::expect;

use super::ParityBatchCase;

/// The surface: the autoloaded command, the mode-mapping alist, the
/// creation hook, and the payload.
fn the_surface_and_mode_mappings() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_surface_and_mode_mappings",
        r####"(list
 :source (scf00-test-source-state)
 :command (commandp 'scratch)
 :mode-alist (eval (car (get 'scratch-mode-alist 'standard-value)))
 :hook (boundp 'scratch-create-buffer-hook)
 :mode-list-samples
 (let ((modes (scratch--list-modes)))
   (list :count (length modes)
         :has-elisp (and (member "emacs-lisp" modes) t)
         :has-fundamental (and (member "fundamental" modes) t)
         :no-dashes (not (cl-some (lambda (m) (string-match-p "--" m))
                                   modes)))))"####,
        expect![[
            r#"OK (:source (:upstream-tree "944053221a06cb4ac8c46692e80db3375e025988" :feature t :version "20220319.1705") :command t :mode-alist ((erc-mode . fundamental-mode) (sql-interactive-mode . sql-mode) (shell-mode . sh-mode) (inferior-python-mode . python-mode) (inferior-emacs-lisp-mode . emacs-lisp-mode) (cider-repl-mode . clojure-mode) (inferior-tcl-mode . tcl-mode) (inferior-octave-mode . octave-mode)) :hook t :mode-list-samples (:count 429 :has-elisp t :has-fundamental t :no-dashes t))"#
        ]],
    )
}

/// Creating a scratch buffer from a mode: the buffer is named after the
/// mode, carries that mode, is flagged `scratch-buffer', and links back
/// to the parent.
fn creating_a_scratch_buffer_inherits_the_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "creating_a_scratch_buffer_inherits_the_mode",
        r####"(unwind-protect
    (progn
      (scf00-test-reset)
      (with-temp-buffer
        (rename-buffer "scf00-parent")
        (emacs-lisp-mode)
        (let ((buffer (scratch--create 'emacs-lisp-mode "*scf00-elisp*")))
          (with-current-buffer buffer
            (list :name (buffer-name)
                  :mode major-mode
                  :scratch-flag (and scratch-buffer t)
                  :parent (buffer-name scratch-parent)
                  :content (buffer-substring-no-properties
                            (point-min) (point-max)))))))
  (scf00-test-reset))"####,
        expect![[
            r#"OK (:name "*scf00-elisp*" :mode emacs-lisp-mode :scratch-flag t :parent "scf00-parent" :content "")"#
        ]],
    )
}

/// The mode alist maps inferior modes to their editing modes through
/// \`scratch--buffer-querymode'.
fn inferior_modes_map_through_the_mode_alist() -> ParityBatchCase {
    ParityBatchCase::value(
        "inferior_modes_map_through_the_mode_alist",
        r####"(unwind-protect
    (progn
      (scf00-test-reset)
      (with-temp-buffer
        (rename-buffer "scf00-shell")
        (shell-mode)
        (let ((mapped (scratch--buffer-querymode)))
          (fundamental-mode)
          (let ((plain (scratch--buffer-querymode)))
            (list :shell-mapped mapped
                  :fundamental-plain plain
                  :alist-entry (assoc 'shell-mode scratch-mode-alist))))))
  (scf00-test-reset))"####,
        expect![
            "OK (:shell-mapped sh-mode :fundamental-plain fundamental-mode :alist-entry (shell-mode . sh-mode))"
        ],
    )
}

/// The region copy and the pop-to reuse: an active region's text lands
/// in the new scratch buffer, and a second `scratch' invocation reuses
/// the existing buffer instead of recreating it.
fn the_region_copies_in_and_the_buffer_is_reused() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_region_copies_in_and_the_buffer_is_reused",
        r####"(unwind-protect
    (progn
      (scf00-test-reset)
      (with-temp-buffer
        (rename-buffer "scf00-source")
        (emacs-lisp-mode)
        (insert ";; first\n;; second\n")
        (goto-char (point-min))
        (forward-line 1)
        (set-mark (line-beginning-position))
        (end-of-line)
        (setq transient-mark-mode t)
        (let ((created (scratch 'emacs-lisp-mode)))
          (with-current-buffer created
            (let ((first-pass
                   (list :name (buffer-name)
                         :mode major-mode
                         :content (buffer-substring-no-properties
                                   (point-min) (point-max))
                         :scratch-flag (and scratch-buffer t))))
              (insert ";; appended")
              (let ((reused (scratch 'emacs-lisp-mode)))
                (list :first-pass first-pass
                      :reused-same (eq reused created)
                      :reused-content
                      (with-current-buffer reused
                        (buffer-substring-no-properties
                         (point-min) (point-max))))))))))
  (scf00-test-reset))"####,
        expect![[
            r#"OK (:first-pass (:name "*emacs-lisp*" :mode emacs-lisp-mode :content ";; second" :scratch-flag t) :reused-same t :reused-content ";; appended;; second")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_surface_and_mode_mappings(),
        creating_a_scratch_buffer_inherits_the_mode(),
        inferior_modes_map_through_the_mode_alist(),
        the_region_copies_in_and_the_buffer_is_reused(),
    ]
}

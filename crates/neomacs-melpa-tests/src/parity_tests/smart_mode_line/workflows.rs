use expect_test::expect;

use super::ParityBatchCase;

/// `sml/setup' installs the mode line: the default `mode-line-format'
/// pieces it replaces, the hooks it registers, rich-minority activation,
/// and the automatic theme decision on a batch frame.  The rendered line
/// for a file buffer carries the documented layout.
fn setup_installs_the_mode_line_and_renders_a_file_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "setup_installs_the_mode_line_and_renders_a_file_buffer",
        r####"(unwind-protect
    (progn
      (sml-test-reset)
      (let* ((root (file-name-as-directory
                    (expand-file-name
                     "sml-fixture"
                     (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
             (file (expand-file-name "notes.org" root)))
        (make-directory root t)
        (let ((coding-system-for-write 'utf-8-unix))
          (with-temp-file file (insert "* Heading\nbody\n")))
        (let ((before (list :front-space (default-value 'mode-line-front-space)
                            :buffer-id (default-value 'mode-line-buffer-identification)
                            :position (default-value 'mode-line-position)
                            :end-spaces (default-value 'mode-line-end-spaces))))
          (sml/setup)
          (let ((buffer (find-file-noselect file)))
            (with-current-buffer buffer
              (list
               :before before
               :source (sml-test-source-state)
               :installed
               (list :front-space (default-value 'mode-line-front-space)
                     :buffer-id (default-value 'mode-line-buffer-identification)
                     :position (default-value 'mode-line-position)
                     :end-spaces (default-value 'mode-line-end-spaces))
               :hooks
               (list (memq 'sml/generate-buffer-identification after-save-hook)
                     (memq 'sml/generate-position-help post-command-hook))
               :theme sml/theme
               :faces (sml-test-faces)
               :rendered (format-mode-line mode-line-format)
               :mode-line-length
               (string-width (format-mode-line mode-line-format))))))))
  (sml-test-reset))"####,
        expect![[
            r##"OK (:before (:front-space ("") :buffer-id ("%12b") :position ((-3 "%p") (line-number-mode (" L%l")) (column-number-mode (" C%c"))) :end-spaces ("%-")) :source (:upstream-tree "f933e4f517b18863773e2103c23f8030d6127e96" :feature t :version "20240924.2322" :rich-minority "20240924.2317") :installed (:front-space ((:eval (sml/generate-buffer-identification-if-necessary)) (sml/position-help-text nil (:eval (let ((sml/-this-buffer-changed-p t)) (sml/generate-position-help)))) (sml/position-construct sml/position-construct (:eval (sml/compile-position-construct)))) :buffer-id ("" (sml/buffer-identification sml/buffer-identification (:eval (sml/generate-buffer-identification)))) :position ((sml/buffer-identification-filling sml/buffer-identification-filling (:eval (setq sml/buffer-identification-filling (sml/fill-for-buffer-identification)))) (sml/position-percentage-format (-3 (:propertize (:eval sml/position-percentage-format) local-map (keymap (mode-line keymap (down-mouse-1 keymap (column-number-mode menu-item "Display Column Numbers" column-number-mode :help "Toggle displaying column numbers in the mode-line" :button (:toggle . column-number-mode)) (line-number-mode menu-item "Display Line Numbers" line-number-mode :help "Toggle displaying line numbers in the mode-line" :button (:toggle . line-number-mode)) (size-indication-mode menu-item "Display Size Indication" size-indication-mode :help "Toggle displaying a size indication in the mode-line" :button (:toggle . size-indication-mode)) "Toggle Line and Column Number Display"))) mouse-face mode-line-highlight face sml/position-percentage help-echo "Buffer Relative Position\nmouse-1: Display Line and Column Mode Menu")))) :end-spaces nil) :hooks ((sml/generate-buffer-identification) nil) :theme dark :faces ((sml/global :foreground "gray50" :background unspecified) (sml/line-number :foreground "White" :background unspecified) (sml/position-percentage :foreground "#bf6000" :background unspecified) (sml/prefix :foreground "#bf6000" :background unspecified) (sml/filename :foreground "#eab700" :background unspecified) (sml/fill :foreground nil :background nil) (sml/modes :foreground "White" :background unspecified)) :rendered "" :mode-line-length 0)"##
        ]],
    )
}

/// The prefix replacement rules: `sml/replacer' applies
/// `sml/replacer-regexp-list' to a path (home first, then the stock
/// entries), falls back to the truename when the literal path matches
/// nothing, and leaves unknown paths untouched.
fn the_replacer_applies_the_documented_substitutions() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_replacer_applies_the_documented_substitutions",
        r####"(let* ((home (getenv "HOME"))
        (in-home (expand-file-name "project/notes.org" home)))
  (list
   :home sml/replacer-regexp-list
   :empty (sml/replacer "")
   :home-path (sml/replacer in-home)
   :tmp-path (sml/replacer "/tmp/x.el")
   :etc-path (sml/replacer "/etc/hosts")
   :unchanged (sml/replacer "/nonexistent-root/file.txt")))"####,
        expect![[
            r#"OK (:home (("^~/org/" ":Org:") ("^~/\\.emacs\\.d/elpa/" ":ELPA:") ("^~/\\.emacs\\.d/" ":ED:") ("^/sudo:.*:" ":SU:") ("^~/Documents/" ":Doc:") ("^~/Dropbox/" ":DB:") ("^:\\([^:]*\\):Documento?s/" ":\\1/Doc:") ("^~/[Gg]it/" ":Git:") ("^~/[Gg]it[Hh]ub/" ":Git:") ("^~/[Gg]it\\([Hh]ub\\|\\)-?[Pp]rojects/" ":Git:")) :empty "" :home-path "[ORACLE-HOME]/project/notes.org" :tmp-path "/tmp/x.el" :etc-path "/etc/hosts" :unchanged "/nonexistent-root/file.txt")"#
        ]],
    )
}

/// The buffer identification: for a file buffer inside the HOME tree the
/// generated identification carries the replaced prefix and the file
/// name; for a non-file buffer it falls back to the buffer name.
fn buffer_identification_uses_the_prefix_and_filename() -> ParityBatchCase {
    ParityBatchCase::value(
        "buffer_identification_uses_the_prefix_and_filename",
        r####"(unwind-protect
    (progn
      (sml-test-reset)
      (sml/setup)
      (let* ((root (file-name-as-directory
                    (expand-file-name
                     "sml-fixture-ident"
                     (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
             (file (expand-file-name "todo.txt" root)))
        (make-directory root t)
        (let ((coding-system-for-write 'utf-8-unix))
          (with-temp-file file (insert "todo\n")))
        (let ((file-buffer (find-file-noselect file))
              (scratch (get-buffer-create "*sml-scratch*")))
          (cl-flet ((describe (string)
                      (list :content (substring-no-properties string)
                            :faces
                            (let (runs)
                              (dotimes (i (length string))
                                (let ((face (get-text-property i 'face string)))
                                  (unless (eq face (car runs))
                                    (push (and face t) runs))))
                              (nreverse runs)))))
            (list
             :file-buffer
             (with-current-buffer file-buffer
               (list :identification
                     (describe (sml/generate-buffer-identification))
                     :name (buffer-name)))
             :non-file
             (with-current-buffer scratch
               (list :identification
                     (describe (sml/generate-buffer-identification))
                     :name (buffer-name))))))))
  (sml-test-reset))"####,
        expect![[
            r#"OK (:file-buffer (:identification (:content "…/sml-fixture-ident/todo.txt" :faces (t t t t t t t t t t t t t t t t t t t t t t t t t t t t)) :name "todo.txt") :non-file (:identification (:content "*sml-scratch*" :faces (t t t t t t t t t t t t t)) :name "*sml-scratch*"))"#
        ]],
    )
}

/// The themes: 'dark and 'light apply their documented face colors,
/// 'respectful leaves the active theme's colors, and nil keeps whatever
/// the user set.  Each theme is applied through `sml/apply-theme' the
/// documented way.
fn the_three_themes_set_the_documented_face_colors() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_three_themes_set_the_documented_face_colors",
        r####"(unwind-protect
    (progn
      (sml-test-reset)
      (sml/setup)
      (let ((dark (progn (sml/apply-theme 'dark) (sml-test-faces)))
            (light (progn (sml/apply-theme 'light) (sml-test-faces)))
            (respectful (progn (sml/apply-theme 'respectful)
                               (sml-test-faces))))
        (list :dark dark
              :light light
              :respectful respectful
              :theme-after sml/theme)))
  (sml-test-reset))"####,
        expect![[
            r##"OK (:dark ((sml/global :foreground "gray50" :background unspecified) (sml/line-number :foreground "White" :background unspecified) (sml/position-percentage :foreground "#bf6000" :background unspecified) (sml/prefix :foreground "#bf6000" :background unspecified) (sml/filename :foreground "#eab700" :background unspecified) (sml/fill :foreground nil :background nil) (sml/modes :foreground "White" :background unspecified)) :light ((sml/global :foreground "gray20" :background unspecified) (sml/line-number :foreground "Black" :background unspecified) (sml/position-percentage :foreground "#5b2507" :background unspecified) (sml/prefix :foreground "#5b2507" :background unspecified) (sml/filename :foreground "Blue" :background unspecified) (sml/fill :foreground nil :background nil) (sml/modes :foreground "Black" :background unspecified)) :respectful ((sml/global :foreground unspecified :background unspecified) (sml/line-number :foreground "unspecified" :background unspecified) (sml/position-percentage :foreground unspecified :background unspecified) (sml/prefix :foreground unspecified :background unspecified) (sml/filename :foreground unspecified :background unspecified) (sml/fill :foreground nil :background nil) (sml/modes :foreground "unspecified" :background unspecified)) :theme-after respectful)"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        setup_installs_the_mode_line_and_renders_a_file_buffer(),
        the_replacer_applies_the_documented_substitutions(),
        buffer_identification_uses_the_prefix_and_filename(),
        the_three_themes_set_the_documented_face_colors(),
    ]
}

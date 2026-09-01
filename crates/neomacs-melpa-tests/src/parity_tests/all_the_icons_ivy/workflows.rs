use expect_test::expect;

use super::ParityBatchCase;

/// The documented one-liner installation, `(all-the-icons-ivy-setup)`.  Pins
/// both command lists and, for every command in them, what ivy's own display
/// transformer registry ends up holding -- which is the table ivy consults, so
/// this is the real contract rather than a proxy for it.
fn setup_registers_the_documented_transformers_for_every_command() -> ParityBatchCase {
    ParityBatchCase::value(
        "setup_registers_the_documented_transformers_for_every_command",
        r##"(progn
  (all-the-icons-ivy-setup)
  (list :buffer-commands all-the-icons-ivy-buffer-commands
        :file-commands all-the-icons-ivy-file-commands
        :registered-buffer (mapcar (lambda (c) (cons c (ativ-test-transformer-for c)))
                                   all-the-icons-ivy-buffer-commands)
        :registered-file (mapcar (lambda (c) (cons c (ativ-test-transformer-for c)))
                                 all-the-icons-ivy-file-commands)
        :spacer all-the-icons-spacer))"##,
        expect![[
            r#"OK (:buffer-commands (ivy-switch-buffer ivy-switch-buffer-other-window counsel-projectile-switch-to-buffer) :file-commands (counsel-find-file counsel-file-jump counsel-recentf counsel-projectile counsel-projectile-find-file counsel-projectile-find-dir counsel-git) :registered-buffer ((ivy-switch-buffer . all-the-icons-ivy-buffer-transformer) (ivy-switch-buffer-other-window . all-the-icons-ivy-buffer-transformer) (counsel-projectile-switch-to-buffer . all-the-icons-ivy-buffer-transformer)) :registered-file ((counsel-find-file . all-the-icons-ivy-file-transformer) (counsel-file-jump . all-the-icons-ivy-file-transformer) (counsel-recentf . all-the-icons-ivy-file-transformer) (counsel-projectile . all-the-icons-ivy-file-transformer) (counsel-projectile-find-file . all-the-icons-ivy-file-transformer) (counsel-projectile-find-dir . all-the-icons-ivy-file-transformer) (counsel-git . all-the-icons-ivy-file-transformer)) :spacer "\11")"#
        ]],
    )
}

fn the_buffer_transformer_prefixes_a_candidate_and_marks_modified_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_buffer_transformer_prefixes_a_candidate_and_marks_modified_buffers",
        r##"(let ((el (ativ-test-write "ativ-code.el" ";; Grüße\n")))
  (find-file-noselect el)
  (with-current-buffer (get-buffer-create "ativ-plain") (fundamental-mode))
  (with-current-buffer (get-buffer "ativ-code.el") (insert "geändert"))
  (list :modified-file-buffer (ativ-test-describe
                               (all-the-icons-ivy-buffer-transformer "ativ-code.el"))
        :plain-buffer (ativ-test-describe
                       (all-the-icons-ivy-buffer-transformer "ativ-plain"))))"##,
        expect![[
            r#"OK (:modified-file-buffer (:text "\11\11ativ-code.el" :length 14 :first-char 9 :prop-names-at-0 (display) :icon-one-char-string t :icon-prop-names (face font-lock-face display rear-nonsticky) :face-on-name ivy-modified-buffer) :plain-buffer (:text "\11\11ativ-plain" :length 12 :first-char 9 :prop-names-at-0 (display) :icon-one-char-string t :icon-prop-names (face font-lock-face display rear-nonsticky) :face-on-name nil))"#
        ]],
    )
}

fn a_candidate_that_names_no_buffer_falls_through_to_the_file_transformer() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_candidate_that_names_no_buffer_falls_through_to_the_file_transformer",
        r##"(list :missing-buffer (ativ-test-describe
                       (all-the-icons-ivy-buffer-transformer "gibt-es-nicht.py"))
      :same-as-file-transformer
      (equal (substring-no-properties
              (all-the-icons-ivy-buffer-transformer "gibt-es-nicht.py"))
             (substring-no-properties
              (all-the-icons-ivy-file-transformer "gibt-es-nicht.py"))))"##,
        expect![[
            r#"OK (:missing-buffer (:text "\11\11gibt-es-nicht.py" :length 18 :first-char 9 :prop-names-at-0 (display) :icon-one-char-string t :icon-prop-names (face font-lock-face display rear-nonsticky) :face-on-name nil) :same-as-file-transformer t)"#
        ]],
    )
}

fn the_file_transformer_gives_directories_the_packages_own_face() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_file_transformer_gives_directories_the_packages_own_face",
        r##"(let ((dir (all-the-icons-ivy-file-transformer "src/"))
      (file (all-the-icons-ivy-file-transformer "notes.org")))
  (list :dir-icon-inherits-dir-face
        (eq (plist-get (get-text-property 0 'face (get-text-property 0 'display dir))
                       :inherit)
            'all-the-icons-ivy-dir-face)
        :file-icon-inherits-dir-face
        (eq (plist-get (get-text-property 0 'face (get-text-property 0 'display file))
                       :inherit)
            'all-the-icons-ivy-dir-face)
        :dir-candidate (ativ-test-describe dir)))"##,
        expect![[
            r#"OK (:dir-icon-inherits-dir-face t :file-icon-inherits-dir-face nil :dir-candidate (:text "\11\11src/" :length 6 :first-char 9 :prop-names-at-0 (display) :icon-one-char-string t :icon-prop-names (face font-lock-face display rear-nonsticky) :face-on-name nil))"#
        ]],
    )
}

fn the_spacer_and_the_buffer_icon_fallback_are_customizable() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_spacer_and_the_buffer_icon_fallback_are_customizable",
        r##"(progn
  (define-derived-mode ativ-unknown-mode nil "AtivUnknown")
  (with-current-buffer (get-buffer-create "ativ-unknown") (ativ-unknown-mode))
  (let* ((icon-of (lambda (result)
                    (copy-sequence
                     (substring-no-properties (get-text-property 0 'display result)))))
         (default (all-the-icons-ivy-buffer-transformer "ativ-unknown"))
         (custom (let ((all-the-icons-ivy-family-fallback-for-buffer 'all-the-icons-octicon)
                       (all-the-icons-ivy-name-fallback-for-buffer "database"))
                   (all-the-icons-ivy-buffer-transformer "ativ-unknown"))))
    (list :mode-has-no-icon-of-its-own
          (symbolp (all-the-icons-icon-for-mode 'ativ-unknown-mode))
          :parent (get 'ativ-unknown-mode 'derived-mode-parent)
          :fallback-changes-the-icon
          (not (string= (funcall icon-of default) (funcall icon-of custom)))
          :default-shape (ativ-test-describe default)
          :custom-spacer
          (let ((all-the-icons-spacer " | "))
            (substring-no-properties
             (all-the-icons-ivy-buffer-transformer "ativ-unknown"))))))"##,
        expect![[
            r#"OK (:mode-has-no-icon-of-its-own t :parent nil :fallback-changes-the-icon t :default-shape (:text "\11\11ativ-unknown" :length 14 :first-char 9 :prop-names-at-0 (display) :icon-one-char-string t :icon-prop-names (face font-lock-face display rear-nonsticky) :face-on-name nil) :custom-spacer "\11 | ativ-unknown")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        setup_registers_the_documented_transformers_for_every_command(),
        the_buffer_transformer_prefixes_a_candidate_and_marks_modified_buffers(),
        a_candidate_that_names_no_buffer_falls_through_to_the_file_transformer(),
        the_file_transformer_gives_directories_the_packages_own_face(),
        the_spacer_and_the_buffer_icon_fallback_are_customizable(),
    ]
}

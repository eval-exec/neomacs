use expect_test::expect;

use super::ParityBatchCase;

fn apropospriate_dark_renders_and_refontifies_a_real_emacs_lisp_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "apropospriate_dark_renders_and_refontifies_a_real_emacs_lisp_buffer",
        r##"(unwind-protect
         (with-temp-buffer
           (emacs-lisp-mode)
           (insert
            ";; Build the release summary shown to operators.\n"
            "(defun release-summary (items)\n"
            "  \"Summarize ready release ITEMS.\"\n"
            "  (let ((ready (seq-filter #'identity items)))\n"
            "    (message \"Ready: %d\" (length ready))))\n")
           (font-lock-ensure)
           (apropospriate-test-load-color-theme
            'apropospriate-dark)
           (let ((before
                  (list
                   (apropospriate-test-face-view
                    "Build the release")
                   (apropospriate-test-face-view
                    "defun")
                   (apropospriate-test-face-view
                    "release-summary")
                   (apropospriate-test-face-view
                    "Ready: %d"))))
             (goto-char
              (point-min))
             (search-forward
              "defun")
             (replace-match
              "defmacro"
              t
              t)
             (font-lock-flush)
             (font-lock-ensure)
             (list
              custom-enabled-themes
              (list
               (face-attribute
                'default
                :foreground
                nil
                'default)
               (face-attribute
                'default
                :background
                nil
                'default))
              before
              (apropospriate-test-face-view
               "defmacro")
              (buffer-substring-no-properties
               (point-min)
               (point-max)))))
       (apropospriate-test-disable-themes))"##,
        expect![[
            r##"OK ((apropospriate-dark) ("#E0E0E0" "#424242") ((font-lock-comment-face "#757575" "#424242" normal) (font-lock-keyword-face "#E1BEE7" "#424242" normal) (font-lock-function-name-face "#64B5F6" "#424242" normal) (font-lock-string-face "#C5E1A5" "#424242" normal)) (font-lock-keyword-face "#E1BEE7" "#424242" normal) ";; Build the release summary shown to operators.\n(defmacro release-summary (items)\n  \"Summarize ready release ITEMS.\"\n  (let ((ready (seq-filter #'identity items)))\n    (message \"Ready: %d\" (length ready))))\n")"##
        ]],
    )
}

fn apropospriate_light_renders_added_removed_and_hunk_lines_in_a_real_diff() -> ParityBatchCase {
    ParityBatchCase::value(
        "apropospriate_light_renders_added_removed_and_hunk_lines_in_a_real_diff",
        r##"(unwind-protect
         (with-temp-buffer
           (require
            'diff-mode)
           (insert
            "diff --git a/src/release.rs b/src/release.rs\n"
            "index 1111111..2222222 100644\n"
            "--- a/src/release.rs\n"
            "+++ b/src/release.rs\n"
            "@@ -1,3 +1,3 @@\n"
            "-let channel = \"nightly\";\n"
            "+let channel = \"stable\";\n"
            " deploy(channel);\n")
           (diff-mode)
           (font-lock-ensure)
           (apropospriate-test-load-color-theme
            'apropospriate-light)
           (list
            custom-enabled-themes
            (list
             (face-attribute
              'default
              :foreground
              nil
              'default)
             (face-attribute
              'default
              :background
              nil
              'default))
            (apropospriate-test-face-view
             "@@ -1,3")
            (apropospriate-test-face-view
             "let channel = \"nightly\"")
            (apropospriate-test-face-view
             "let channel = \"stable\"")
            (buffer-substring-no-properties
             (point-min)
             (point-max))))
       (apropospriate-test-disable-themes))"##,
        expect![[
            r##"OK ((apropospriate-light) ("#546E7A" "#F5F5F5") (diff-hunk-header "#90A4AE" "#F5F5F5" normal) (diff-removed "#D50000" "#F5F5F5" normal) (diff-added "#66BB6A" "#F5F5F5" normal) "diff --git a/src/release.rs b/src/release.rs\nindex 1111111..2222222 100644\n--- a/src/release.rs\n+++ b/src/release.rs\n@@ -1,3 +1,3 @@\n-let channel = \"nightly\";\n+let channel = \"stable\";\n deploy(channel);\n")"##
        ]],
    )
}

fn apropospriate_dark_applies_org_heading_resizing_and_custom_mode_line_height() -> ParityBatchCase
{
    ParityBatchCase::value(
        "apropospriate_dark_applies_org_heading_resizing_and_custom_mode_line_height",
        r##"(unwind-protect
         (progn
           (require
            'org)
           (let ((apropospriate-mode-line-height
                  1.15)
                 (apropospriate-org-level-resizing
                  t))
             (apropospriate-test-load-color-theme
              'apropospriate-dark))
           (with-temp-buffer
             (org-mode)
             (insert
              "#+title: Release Plan\n"
              "* TODO Ship version 2.0\n"
              "** DONE Validate migration\n"
              "*** Notes\n"
              "The rollout passed staging.\n")
             (font-lock-ensure)
             (list
              custom-enabled-themes
              (apropospriate-test-face-view
               "Release Plan")
              (apropospriate-test-face-at
               "Ship version 2.0")
              (apropospriate-test-face-at
               "Validate migration")
              (apropospriate-test-face-at
               "TODO")
              (apropospriate-test-face-at
               "DONE")
              (list
               (list
                :level-1
                (face-attribute
                 'org-level-1
                 :foreground
                 nil
                 'default)
                (face-attribute
                 'org-level-1
                 :height
                 nil
                 nil))
               (list
                :level-2
                (face-attribute
                 'org-level-2
                 :foreground
                 nil
                 'default)
                (face-attribute
                 'org-level-2
                 :height
                 nil
                 nil))
               (list
                :todo
                (face-attribute
                 'org-todo
                 :foreground
                 nil
                 'default)
                (face-attribute
                 'org-todo
                 :weight
                 nil
                 'default))
               (list
                :done
                (face-attribute
                 'org-done
                 :foreground
                 nil
                 'default)
                (face-attribute
                 'org-done
                 :weight
                 nil
                 'default))
               (list
                :mode-line
                (face-attribute
                 'mode-line
                 :background
                 nil
                 'default)
                (face-attribute
                 'mode-line
                 :height
                 nil
                 nil)))
              (buffer-substring-no-properties
               (point-min)
               (point-max)))))
       (apropospriate-test-disable-themes))"##,
        expect![[
            r##"OK ((apropospriate-dark) (org-document-title "#FFCC80" "#424242" bold) org-level-1 (org-headline-done org-level-2) (org-todo org-level-1) (org-done org-level-2) ((:level-1 "#E1BEE7" 1.3) (:level-2 "#E1BEE7" 1.2) (:todo "#E57373" normal) (:done "#C5E1A5" normal) (:mode-line "#323232" 1.15)) "#+title: Release Plan\n* TODO Ship version 2.0\n** DONE Validate migration\n*** Notes\nThe rollout passed staging.\n")"##
        ]],
    )
}

fn apropospriate_switches_a_live_code_buffer_from_dark_to_light_and_restores_defaults()
-> ParityBatchCase {
    ParityBatchCase::value(
        "apropospriate_switches_a_live_code_buffer_from_dark_to_light_and_restores_defaults",
        r##"(let ((before
                (list
                 custom-enabled-themes
                 (face-attribute
                  'default
                  :foreground
                  nil
                  'default)
                 (face-attribute
                  'default
                  :background
                  nil
                  'default))))
         (unwind-protect
             (with-temp-buffer
               (emacs-lisp-mode)
               (insert
                "(when release-ready\n"
                "  (message \"Deploy now\"))\n")
               (font-lock-ensure)
               (apropospriate-test-load-color-theme
                'apropospriate-dark)
               (let ((dark
                      (list
                       custom-enabled-themes
                       (face-attribute
                        'default
                        :foreground
                        nil
                        'default)
                       (face-attribute
                        'default
                        :background
                        nil
                        'default)
                       (apropospriate-test-face-view
                        "when")
                       (apropospriate-test-face-view
                        "Deploy now"))))
                 (disable-theme
                  'apropospriate-dark)
                 (apropospriate-test-load-color-theme
                  'apropospriate-light)
                 (let ((light
                        (list
                         custom-enabled-themes
                         (face-attribute
                          'default
                          :foreground
                          nil
                          'default)
                         (face-attribute
                          'default
                          :background
                          nil
                          'default)
                         (apropospriate-test-face-view
                          "when")
                         (apropospriate-test-face-view
                          "Deploy now"))))
                   (disable-theme
                    'apropospriate-light)
                   (list
                    before
                    dark
                    light
                    (list
                     custom-enabled-themes
                     (face-attribute
                      'default
                      :foreground
                      nil
                      'default)
                     (face-attribute
                      'default
                      :background
                      nil
                      'default))))))
           (apropospriate-test-disable-themes)))"##,
        expect![[
            r##"OK ((nil "unspecified-fg" "unspecified-bg") ((apropospriate-dark) "#E0E0E0" "#424242" (font-lock-keyword-face "#E1BEE7" "#424242" normal) (font-lock-string-face "#C5E1A5" "#424242" normal)) ((apropospriate-light) "#546E7A" "#F5F5F5" (font-lock-keyword-face "#7E57C2" "#F5F5F5" normal) (font-lock-string-face "#66BB6A" "#F5F5F5" normal)) (nil "unspecified-fg" "unspecified-bg"))"##
        ]],
    )
}

fn apropospriate_dark_colors_real_ansi_build_output() -> ParityBatchCase {
    ParityBatchCase::value(
        "apropospriate_dark_colors_real_ansi_build_output",
        r##"(unwind-protect
         (progn
           (require
            'ansi-color)
           (apropospriate-test-load-color-theme
            'apropospriate-dark)
           (let* ((rendered
                   (ansi-color-apply
                    (concat
                     "build "
                     "\x1b[31mERROR\x1b[0m"
                     " then "
                     "\x1b[32mSUCCESS\x1b[0m")))
                  (error-position
                   (string-match
                    "ERROR"
                    rendered))
                  (success-position
                   (string-match
                    "SUCCESS"
                    rendered)))
             (list
              custom-enabled-themes
              (substring-no-properties
               rendered)
              (text-properties-at
               error-position
               rendered)
              (text-properties-at
               success-position
               rendered))))
       (apropospriate-test-disable-themes))"##,
        expect![[
            r##"OK ((apropospriate-dark) "build ERROR then SUCCESS" (font-lock-face (:foreground "#E57373")) (font-lock-face (:foreground "#C5E1A5")))"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        apropospriate_dark_renders_and_refontifies_a_real_emacs_lisp_buffer(),
        apropospriate_light_renders_added_removed_and_hunk_lines_in_a_real_diff(),
        apropospriate_dark_applies_org_heading_resizing_and_custom_mode_line_height(),
        apropospriate_switches_a_live_code_buffer_from_dark_to_light_and_restores_defaults(),
        apropospriate_dark_colors_real_ansi_build_output(),
    ]
}

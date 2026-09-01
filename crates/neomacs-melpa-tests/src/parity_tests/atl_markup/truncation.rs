use expect_test::expect;

use super::ParityBatchCase;

fn atl_markup_practical_html_navigation_enables_inside_tags_and_disables_in_text() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atl_markup_practical_html_navigation_enables_inside_tags_and_disables_in_text",
        r##"(with-temp-buffer
          (insert
           "<article class=\"card\">\n"
           "  <h1>Hello world</h1>\n"
           "</article>\n"
           "tail")
          (html-mode)
          (set-buffer-modified-p nil)
          (let ((original-text
                 (buffer-string))
                snapshots)
            (dolist
                (needle
                 '("class"
                   "Hello"
                   "</art"
                   "tail"))
              (goto-char
               (point-min))
              (search-forward needle)
              (push
               (list
                needle
                (atl-markup--web-truncate-lines-by-face)
                truncate-lines
                (point))
               snapshots))
            (list
             (nreverse snapshots)
             (equal
              original-text
              (buffer-string))
             (buffer-modified-p))))"##,
        expect![[
            r#"OK ((("class" "Truncate long lines enabled" t 15) ("Hello" "Truncate long lines disabled" nil 35) ("</art" "Truncate long lines enabled" t 52) ("tail" nil t 62)) t nil)"#
        ]],
    )
}

fn atl_markup_truncation_guard_matrix_skips_boundaries_whitespace_comments_and_eol()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_truncation_guard_matrix_skips_boundaries_whitespace_comments_and_eol",
        r##"(mapcar
          (lambda (specification)
            (pcase-let
                ((`(,name ,contents ,mode)
                  specification))
              (with-temp-buffer
                (let (calls)
                  (atl-markup-test-place-marker
                   contents)
                  (when mode
                    (funcall mode))
                  (setq-local
                   truncate-lines
                   :unchanged)
                  (cl-letf
                      (((symbol-function
                         'toggle-truncate-lines)
                        (lambda (argument)
                          (push argument calls)
                          :toggled)))
                    (list
                     name
                     (atl-markup--web-truncate-lines-by-face)
                     (nreverse calls)
                     truncate-lines
                     (point)))))))
          '((bob "|<tag>value</tag>" nil)
            (eob "<tag>value</tag>|" nil)
            (space "<tag>value |inside</tag>" nil)
            (tab "<tag>value\t|inside</tag>" nil)
            (newline "<tag>value\n|inside</tag>" nil)
            (eol "<tag>value|\nnext" nil)
            (elisp-comment ";; comment bod|y\n(value)" emacs-lisp-mode)
            (ordinary-tag "<tag attrib|ute>value</tag>" nil)
            (ordinary-text "<tag>val|ue</tag>" nil)))"##,
        expect![
            "OK ((bob nil nil :unchanged 1) (eob nil nil :unchanged 17) (space nil nil :unchanged 12) (tab nil nil :unchanged 12) (newline nil nil :unchanged 12) (eol nil nil :unchanged 11) (elisp-comment nil nil :unchanged 15) (ordinary-tag :toggled (1) :unchanged 12) (ordinary-text :toggled (-1) :unchanged 9))"
        ],
    )
}

fn atl_markup_custom_ignore_regex_controls_exact_previous_character_gate_and_errors()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_custom_ignore_regex_controls_exact_previous_character_gate_and_errors",
        r##"(mapcar
          (lambda (specification)
            (pcase-let
                ((`(,regex ,contents)
                  specification))
              (with-temp-buffer
                (atl-markup-test-place-marker
                 contents)
                (let ((atl-markup-ignore-regex
                       regex)
                      calls)
                  (cl-letf
                      (((symbol-function
                         'atl-markup--comment-block-p)
                        (lambda () nil))
                       ((symbol-function
                         'atl-markup--inside-tag-p)
                        (lambda () t))
                       ((symbol-function
                         'toggle-truncate-lines)
                        (lambda (argument)
                          (push argument calls)
                          :toggled)))
                    (list
                     regex
                     contents
                     (atl-markup-test-error-data
                      #'atl-markup--web-truncate-lines-by-face)
                     (nreverse calls)
                     (point)))))))
          '(("[ \t\r\n]" "<tag>space |value</tag>")
            ("x" "<tag>x|value</tag>")
            ("x" "<tag> |value</tag>")
            ("" "<tag>a|value</tag>")
            ("β" "<tag>β|value</tag>")
            ("[[:digit:]]" "<tag>7|value</tag>")
            ("[[:digit:]]" "<tag>a|value</tag>")
            ("[" "<tag>a|value</tag>")
            (nil "<tag>a|value</tag>")))"##,
        expect![[
            r#"OK (("[ \11\15\n]" "<tag>space |value</tag>" (:ok nil) nil 12) ("x" "<tag>x|value</tag>" (:ok nil) nil 7) ("x" "<tag> |value</tag>" (:ok :toggled) (1) 7) ("" "<tag>a|value</tag>" (:ok nil) nil 7) ("β" "<tag>β|value</tag>" (:ok nil) nil 7) ("[[:digit:]]" "<tag>7|value</tag>" (:ok nil) nil 7) ("[[:digit:]]" "<tag>a|value</tag>" (:ok :toggled) (1) 7) ("[" "<tag>a|value</tag>" (:error invalid-regexp ("Unmatched [ or [^")) nil 7) (nil "<tag>a|value</tag>" (:error wrong-type-argument (stringp nil)) nil 7))"#
        ]],
    )
}

fn atl_markup_markup_comments_block_tag_shaped_text_but_real_tags_still_toggle() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atl_markup_markup_comments_block_tag_shaped_text_but_real_tags_still_toggle",
        r##"(mapcar
          (lambda (contents)
            (with-temp-buffer
              (atl-markup-test-place-marker
               contents)
              (html-mode)
              (let (calls)
                (cl-letf
                    (((symbol-function
                       'toggle-truncate-lines)
                      (lambda (argument)
                        (push argument calls)
                        :toggled)))
                  (list
                   contents
                   (atl-markup--comment-block-p)
                   (atl-markup--inside-tag-p)
                   (atl-markup--web-truncate-lines-by-face)
                   (nreverse calls)
                   (point))))))
          '("<!-- <article class=\"car|d\">hidden</article> -->"
            "<article class=\"car|d\">visible</article>"
            "<article><!-- comment bod|y --></article>"
            "<article>visible bod|y</article>"
            "<!-- multiline\n <nested attrib|ute>\n --><p>x</p>"))"##,
        expect![[
            r#"OK (("<!-- <article class=\"car|d\">hidden</article> -->" t t nil nil 25) ("<article class=\"car|d\">visible</article>" nil t :toggled (1) 20) ("<article><!-- comment bod|y --></article>" t t nil nil 26) ("<article>visible bod|y</article>" nil nil :toggled (-1) 21) ("<!-- multiline\n <nested attrib|ute>\n --><p>x</p>" t t nil nil 31))"#
        ]],
    )
}

fn atl_markup_web_truncation_is_independent_of_fontification_and_face_properties() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atl_markup_web_truncation_is_independent_of_fontification_and_face_properties",
        r##"(mapcar
          (lambda (face)
            (with-temp-buffer
              (insert
               "<strong class=\"hero\">body text</strong>")
              (add-text-properties
               (point-min)
               (point-max)
               (list 'face face))
              (let (calls)
                (cl-letf
                    (((symbol-function
                       'toggle-truncate-lines)
                      (lambda (argument)
                        (push argument calls)
                        argument)))
                  (goto-char
                   (point-min))
                  (search-forward "class")
                  (let ((tag-result
                         (atl-markup--web-truncate-lines-by-face)))
                    (goto-char
                     (point-min))
                    (search-forward "body")
                    (list
                     (copy-tree face)
                     tag-result
                     (atl-markup--web-truncate-lines-by-face)
                     (nreverse calls)
                     (copy-tree
                      (get-text-property
                       (point-min)
                       'face))
                     (eq
                      face
                      (get-text-property
                       (point-min)
                       'face))))))))
          '(nil
            font-lock-keyword-face
            (:foreground "red" :weight bold)
            (font-lock-comment-face underline)))"##,
        expect![[
            r#"OK ((nil 1 -1 (1 -1) nil t) (font-lock-keyword-face 1 -1 (1 -1) font-lock-keyword-face t) ((:foreground "red" :weight bold) 1 -1 (1 -1) (:foreground "red" :weight bold) t) ((font-lock-comment-face underline) 1 -1 (1 -1) (font-lock-comment-face underline) t))"#
        ]],
    )
}

fn atl_markup_web_truncation_propagates_guard_classifier_and_toggle_failures() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_web_truncation_propagates_guard_classifier_and_toggle_failures",
        r##"(mapcar
          (lambda (failure)
            (with-temp-buffer
              (insert
               "<tag attribute>value</tag>")
              (goto-char
               (point-min))
              (search-forward "attribute")
              (let ((message-log-max
                     21)
                    (inhibit-message nil))
                (cl-letf
                    (((symbol-function
                       'atl-markup--comment-block-p)
                      (lambda ()
                        (when
                            (eq failure 'comment)
                          (error "comment failed"))
                        nil))
                     ((symbol-function
                       'atl-markup--inside-tag-p)
                      (lambda ()
                        (when
                            (eq failure 'inside)
                          (error "inside failed"))
                        t))
                     ((symbol-function
                       'toggle-truncate-lines)
                      (lambda (_argument)
                        (when
                            (eq failure 'toggle)
                          (error "toggle failed"))
                        :toggled)))
                  (list
                   failure
                   (atl-markup-test-error-data
                    #'atl-markup--web-truncate-lines-by-face)
                   message-log-max
                   inhibit-message
                   (point))))))
          '(none comment inside toggle))"##,
        expect![[
            r#"OK ((none (:ok :toggled) 21 nil 15) (comment (:error error ("comment failed")) 21 nil 15) (inside (:error error ("inside failed")) 21 nil 15) (toggle (:error error ("toggle failed")) 21 nil 15))"#
        ]],
    )
}

fn atl_markup_web_truncation_preserves_text_point_and_modified_state_but_records_match_data_effect()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_markup_web_truncation_preserves_text_point_and_modified_state_but_records_match_data_effect",
        r##"(with-temp-buffer
          (insert
           "<article data-id=\"42\">payload</article>")
          (goto-char
           (point-min))
          (search-forward "data-id")
          (set-buffer-modified-p nil)
          (string-match
           "b\\(eta\\)"
           "beta")
          (let ((before-point
                 (point))
                (before-text
                 (buffer-string))
                (before-match
                 (match-data t))
                result
                after-match)
            (cl-letf
                (((symbol-function
                   'toggle-truncate-lines)
                  (lambda (argument)
                    (list
                     :toggle
                     argument))))
              (setq result
                    (atl-markup--web-truncate-lines-by-face))
              (setq after-match
                    (match-data t)))
            (list
             result
             (=
              before-point
              (point))
             (equal
              before-text
              (buffer-string))
             (buffer-modified-p)
             before-match
             after-match
             (equal
              before-match
              after-match))))"##,
        expect!["OK ((:toggle 1) t t nil (0 4 1 4) (22 23 (:buffer nil)) nil)"],
    )
}

pub(super) fn truncation_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atl_markup_practical_html_navigation_enables_inside_tags_and_disables_in_text(),
        atl_markup_truncation_guard_matrix_skips_boundaries_whitespace_comments_and_eol(),
        atl_markup_custom_ignore_regex_controls_exact_previous_character_gate_and_errors(),
        atl_markup_markup_comments_block_tag_shaped_text_but_real_tags_still_toggle(),
        atl_markup_web_truncation_is_independent_of_fontification_and_face_properties(),
        atl_markup_web_truncation_propagates_guard_classifier_and_toggle_failures(),
        atl_markup_web_truncation_preserves_text_point_and_modified_state_but_records_match_data_effect(),
    ]
}

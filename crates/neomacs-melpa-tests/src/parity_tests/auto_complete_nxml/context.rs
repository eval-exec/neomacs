use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_nxml_inside_tag_tracks_open_close_and_content_positions() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_inside_tag_tracks_open_close_and_content_positions",
        r##"(with-temp-buffer
         (insert "<root attr=\"value\">text<child x='1'/>tail</root>")
         (mapcar
          (lambda (needle)
            (goto-char (point-min))
            (search-forward needle)
            (list needle (point) (auto-complete-nxml-point-inside-tag-p)))
          '("<roo" "attr=" "\">" "text" "<child" "/>" "tail" "</roo")))"##,
        expect![[
            r#"OK (("<roo" 5 t) ("attr=" 12 t) ("\">" 20 nil) ("text" 24 nil) ("<child" 30 t) ("/>" 38 nil) ("tail" 42 nil) ("</roo" 47 t))"#
        ]],
    )
}

fn auto_complete_nxml_current_tag_uses_nearest_non_closing_start_tag() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_current_tag_uses_nearest_non_closing_start_tag",
        r##"(with-temp-buffer
         (insert "<catalog><book id=\"1\"><title>Neo</title></book><appendix")
         (mapcar
          (lambda (needle)
            (goto-char (point-min))
            (search-forward needle)
            (auto-complete-nxml-update-current-tag)
            (list needle auto-complete-nxml-buffer-current-tag))
          '("<cat" "<book id" "<title>" "Neo" "</title>" "</book>" "<appendix")))"##,
        expect![[
            r#"OK (("<cat" "catalog") ("<book id" "book") ("<title>" "title") ("Neo" "title") ("</title>" "title") ("</book>" "title") ("<appendix" "appendix"))"#
        ]],
    )
}

fn auto_complete_nxml_current_attribute_handles_quotes_hyphens_and_outside_text() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_nxml_current_attribute_handles_quotes_hyphens_and_outside_text",
        r##"(with-temp-buffer
         (insert "<node data-kind=\"alpha beta\" single='gamma' style=\"color: red\">body</node>")
         (mapcar
          (lambda (needle)
            (goto-char (point-min))
            (search-forward needle)
            (auto-complete-nxml-update-current-attr)
            (list needle auto-complete-nxml-buffer-current-attr))
          '("alpha" "beta" "gamma" "color" "red" ">body")))"##,
        expect![[
            r#"OK (("alpha" "data-kind") ("beta" "data-kind") ("gamma" "single") ("color" "style") ("red" "style") (">body" ""))"#
        ]],
    )
}

fn auto_complete_nxml_context_symbol_classifies_real_editing_positions() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_context_symbol_classifies_real_editing_positions",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (insert (car case))
             (goto-char (point-max))
             (list
              (cdr case)
              (auto-complete-nxml-get-current-context-symbol)
              auto-complete-nxml-buffer-current-tag
              auto-complete-nxml-buffer-current-attr)))
         '(("<ta" . tag)
           ("<table cel" . attr)
           ("<table class=\"wid" . attr-value)
           ("<table style=\"font-siz" . css-property)
           ("<table style=\"color: re" . css-value)
           ("<table>" . content-start)
           ("<table>hello" . content)
           ("plain text" . otherwise)))"##,
        expect![[
            r#"OK ((tag tag "<" nil) (attr attr "table" " ") (attr-value attrvalue "table" "class") (css-property cssprop "table" "style") (css-value csspropvalue "table" "style") (content-start content "table" nil) (content content "table" nil) (otherwise otherwise "" nil))"#
        ]],
    )
}

fn auto_complete_nxml_start_completion_respects_automatic_and_manual_trigger() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_start_completion_respects_automatic_and_manual_trigger",
        r##"(mapcar
         (lambda (state)
           (let ((auto-complete-nxml-automatic-p (nth 0 state))
                 (this-command (nth 1 state)))
             (list state (auto-complete-nxml-start-completion-p))))
         '((t self-insert-command)
           (nil self-insert-command)
           (nil ac-trigger-key-command)
           (t ac-trigger-key-command)))"##,
        expect![
            "OK (((t self-insert-command) t) ((nil self-insert-command) nil) ((nil ac-trigger-key-command) t) ((t ac-trigger-key-command) t))"
        ],
    )
}

fn auto_complete_nxml_context_state_is_buffer_local() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_context_state_is_buffer_local",
        r##"(let ((first (generate-new-buffer " *acnxml-first*"))
             (second (generate-new-buffer " *acnxml-second*")))
         (unwind-protect
             (progn
               (with-current-buffer first
                 (insert "<alpha one=\"x")
                 (auto-complete-nxml-update-current-tag)
                 (auto-complete-nxml-update-current-attr))
               (with-current-buffer second
                 (insert "<beta two=\"y")
                 (auto-complete-nxml-update-current-tag)
                 (auto-complete-nxml-update-current-attr))
               (list
                (with-current-buffer first
                  (list auto-complete-nxml-buffer-current-tag
                        auto-complete-nxml-buffer-current-attr))
                (with-current-buffer second
                  (list auto-complete-nxml-buffer-current-tag
                        auto-complete-nxml-buffer-current-attr))))
           (kill-buffer first)
           (kill-buffer second)))"##,
        expect![[r#"OK (("alpha" "one") ("beta" "two"))"#]],
    )
}

fn auto_complete_nxml_context_symbol_mutates_state_for_popup_help_consumers() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_context_symbol_mutates_state_for_popup_help_consumers",
        r##"(with-temp-buffer
         (insert "<section data-role=\"main\"><child")
         (let ((tag-context (auto-complete-nxml-get-current-context-symbol))
               (tag-state auto-complete-nxml-buffer-current-tag))
           (erase-buffer)
           (insert "<section data-role=\"main")
           (let ((attr-context (auto-complete-nxml-get-current-context-symbol)))
             (list tag-context
                   tag-state
                   attr-context
                   auto-complete-nxml-buffer-current-tag
                   auto-complete-nxml-buffer-current-attr))))"##,
        expect![[r#"OK (tag "<" attrvalue "section" "data-role")"#]],
    )
}

fn auto_complete_nxml_point_inside_tag_survives_angle_brackets_in_sequence() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_point_inside_tag_survives_angle_brackets_in_sequence",
        r##"(with-temp-buffer
         (insert "<a><b key=\"v\">x</b><c")
         (let (states)
           (dotimes (offset (1+ (buffer-size)))
             (goto-char (1+ offset))
             (push (if (auto-complete-nxml-point-inside-tag-p) 1 0) states))
           (apply #'string (mapcar (lambda (state) (+ ?0 state)) (nreverse states)))))"##,
        expect![[r#"OK "0110111111111100111011""#]],
    )
}

fn auto_complete_nxml_context_detection_preserves_buffer_point_and_text() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_context_detection_preserves_buffer_point_and_text",
        r##"(with-temp-buffer
         (insert "<root><item class=\"one two\">payload")
         (goto-char (- (point-max) 3))
         (let ((before-point (point))
               (before-text (buffer-string))
               (context (auto-complete-nxml-get-current-context-symbol)))
           (list context
                 (= before-point (point))
                 (equal before-text (buffer-string))
                 auto-complete-nxml-buffer-current-tag
                 auto-complete-nxml-buffer-current-attr)))"##,
        expect![[r#"OK (content t t "item" nil)"#]],
    )
}

pub(super) fn context_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_nxml_inside_tag_tracks_open_close_and_content_positions(),
        auto_complete_nxml_current_tag_uses_nearest_non_closing_start_tag(),
        auto_complete_nxml_current_attribute_handles_quotes_hyphens_and_outside_text(),
        auto_complete_nxml_context_symbol_classifies_real_editing_positions(),
        auto_complete_nxml_start_completion_respects_automatic_and_manual_trigger(),
        auto_complete_nxml_context_state_is_buffer_local(),
        auto_complete_nxml_context_symbol_mutates_state_for_popup_help_consumers(),
        auto_complete_nxml_point_inside_tag_survives_angle_brackets_in_sequence(),
        auto_complete_nxml_context_detection_preserves_buffer_point_and_text(),
    ]
}

use expect_test::expect;

use super::ParityBatchCase;

fn opening_formatting_extending_and_saving_a_real_arscript_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "opening_formatting_extending_and_saving_a_real_arscript_file",
        r##"(let* ((directory (make-temp-file "arscript-project-" t))
       (script (expand-file-name "willow.arscript" directory))
       buffer)
  (with-temp-file script
    (insert
     "<Version>\n"
     "ArtRage Version: ArtRage 3 4\n"
     "ArtRage Build: 4.5.3\n"
     "</Version>\n"
     "<Header>\n"
     "Painting Name: \"Willow\"\n"
     "Painting Width: 2456\n"
     "</Header>\n"))
  (setq buffer (find-file-noselect script))
  (prog1
      (with-current-buffer buffer
        (let ((opened-state
               (list
                (file-name-nondirectory buffer-file-name)
                major-mode
                mode-name
                (eq indent-line-function #'arscript-indent-line)
                indent-tabs-mode
                comment-start
                (buffer-modified-p))))
          (setq-local tab-width 2)
          (indent-region (point-min) (point-max))
          (goto-char (point-min))
          (search-forward "Painting Width: 2456")
          (end-of-line)
          (newline-and-indent)
          (insert "Painting Height: 2206")
          (let ((edited-state
                 (list
                  (buffer-modified-p)
                  (line-number-at-pos)
                  (current-indentation)
                  (buffer-string))))
            (save-buffer)
            (list
             opened-state
             edited-state
             (buffer-modified-p)
             (with-temp-buffer
               (insert-file-contents script)
               (buffer-string))))))
    (kill-buffer buffer)))"##,
        expect![[
            r#"OK (("willow.arscript" fundamental-mode "Fundamental" t nil "//" nil) (t 8 2 "<Version>\n  ArtRage Version: ArtRage 3 4\n  ArtRage Build: 4.5.3\n</Version>\n<Header>\n  Painting Name: \"Willow\"\n  Painting Width: 2456\n  Painting Height: 2206\n</Header>\n") nil "<Version>\n  ArtRage Version: ArtRage 3 4\n  ArtRage Build: 4.5.3\n</Version>\n<Header>\n  Painting Name: \"Willow\"\n  Painting Width: 2456\n  Painting Height: 2206\n</Header>\n")"#
        ]],
    )
}

fn formatting_a_pasted_recording_produces_stable_nested_art_script_structure() -> ParityBatchCase {
    ParityBatchCase::value(
        "formatting_a_pasted_recording_produces_stable_nested_art_script_structure",
        r##"(with-temp-buffer
  (insert
   "<Events>\n"
   " <StrokeEvent>\n"
   "<StrokeHeader>\n"
   "<EventPt>\n"
   "Wait: 0.018s Loc: (1086.56, 559.258) Pr: 0.156599 Rv: NO Iv: NO\n"
   "</EventPt>\n"
   "<Recorded> Yes </Recorded>\n"
   "</StrokeHeader>\n"
   "</StrokeEvent>\n"
   "EvType: Command CommandID: Undo\n"
   "<StrokeEvent>\n"
   "<StrokeHeader>\n"
   "Loc: (-679.912, 774.652) Dr: (-0.99682, -0.0796825)\n"
   "</StrokeHeader>\n"
   "</StrokeEvent>\n"
   "</Events>\n")
  (arscript-mode)
  (setq-local tab-width 2)
  (indent-region (point-min) (point-max))
  (let ((first-pass (buffer-string))
        (first-indentations
         (let (columns)
           (goto-char (point-min))
           (while (not (eobp))
             (push (current-indentation) columns)
             (forward-line))
           (nreverse columns))))
    (indent-region (point-min) (point-max))
    (list
     first-pass
     first-indentations
     (equal first-pass (buffer-string)))))"##,
        expect![[
            r#"OK ("<Events>\n  <StrokeEvent>\n    <StrokeHeader>\n      <EventPt>\n        Wait: 0.018s Loc: (1086.56, 559.258) Pr: 0.156599 Rv: NO Iv: NO\n      </EventPt>\n      <Recorded> Yes </Recorded>\n    </StrokeHeader>\n  </StrokeEvent>\n  EvType: Command CommandID: Undo\n  <StrokeEvent>\n    <StrokeHeader>\n      Loc: (-679.912, 774.652) Dr: (-0.99682, -0.0796825)\n    </StrokeHeader>\n  </StrokeEvent>\n</Events>\n" (0 2 4 6 8 6 6 4 2 2 2 4 6 4 2 0) t)"#
        ]],
    )
}

fn disabling_reenabling_and_rewriting_recorded_actions_preserves_the_document() -> ParityBatchCase {
    ParityBatchCase::value(
        "disabling_reenabling_and_rewriting_recorded_actions_preserves_the_document",
        r##"(with-temp-buffer
  (insert
   "<Events>\n"
   "EvType: Command CommandID: CID_SetClearCanvas ParamType: flag Value: { true }\n"
   "EvType: Command CommandID: Undo\n"
   "EvType: Command CommandID: ExportLayer Path: \"willow.png\"\n"
   "</Events>\n")
  (arscript-mode)
  (setq-local tab-width 2)
  (indent-region (point-min) (point-max))
  (buffer-enable-undo)
  (undo-boundary)
  (goto-char (point-min))
  (forward-line)
  (let ((start (line-beginning-position)))
    (forward-line 2)
    (comment-region start (point))
    (let ((disabled (buffer-string)))
      (undo-boundary)
      (undo-only 1)
      (goto-char (point-min))
      (search-forward "Undo")
      (replace-match
       "SetForeColour ParamType: Pixel Value: { 0x0FFCCA38F }")
      (indent-region (point-min) (point-max))
      (list
       disabled
       (buffer-string)
       (line-number-at-pos)
       (current-indentation)
       (buffer-modified-p)))))"##,
        expect![[
            r#"OK ("<Events>\n  // EvType: Command CommandID: CID_SetClearCanvas ParamType: flag Value: { true }\n  // EvType: Command CommandID: Undo\n  EvType: Command CommandID: ExportLayer Path: \"willow.png\"\n</Events>\n" "<Events>\n  EvType: Command CommandID: CID_SetClearCanvas ParamType: flag Value: { true }\n  EvType: Command CommandID: SetForeColour ParamType: Pixel Value: { 0x0FFCCA38F }\n  EvType: Command CommandID: ExportLayer Path: \"willow.png\"\n</Events>\n" 3 2 t)"#
        ]],
    )
}

fn editing_and_refontifying_a_header_and_paint_event_updates_visible_semantics() -> ParityBatchCase
{
    ParityBatchCase::value(
        "editing_and_refontifying_a_header_and_paint_event_updates_visible_semantics",
        r##"(with-temp-buffer
  (insert
   "// Willow export workflow\n"
   "<Header>\n"
   "Painting Name: \"Willow\"\n"
   "Painting Width: 2456\n"
   "Script Feature Flags: 0x000000034\n"
   "</Header>\n"
   "<Events>\n"
   "EvType: Command CommandID: SetForeColour ParamType: Pixel Value: { 0x0FF7386A0 }\n"
   "<EventPt> Wait: 0.018s Loc: (1086.56, 559.258) Pr: 0.156599 </EventPt>\n"
   "</Events>\n")
  (arscript-mode)
  (font-lock-ensure)
  (goto-char (point-min))
  (search-forward "SetForeColour")
  (replace-match "SetForeColor")
  (goto-char (point-min))
  (search-forward "Painting Width: 2456")
  (replace-match "Painting Height: 2206")
  (font-lock-flush)
  (font-lock-ensure)
  (let ((position (point-min))
        visible-runs)
    (while (< position (point-max))
      (let ((face (get-text-property position 'face))
            (next
             (or
              (next-single-property-change
               position 'face nil (point-max))
              (point-max))))
        (when face
          (push
           (list
            (buffer-substring-no-properties position next)
            face)
           visible-runs))
        (setq position next)))
    (list
     (buffer-substring-no-properties (point-min) (point-max))
     (nreverse visible-runs))))"##,
        expect![[
            r#"OK ("// Willow export workflow\n<Header>\nPainting Name: \"Willow\"\nPainting Height: 2206\nScript Feature Flags: 0x000000034\n</Header>\n<Events>\nEvType: Command CommandID: SetForeColor ParamType: Pixel Value: { 0x0FF7386A0 }\n<EventPt> Wait: 0.018s Loc: (1086.56, 559.258) Pr: 0.156599 </EventPt>\n</Events>\n" (("// Willow export workflow" font-lock-comment-face) ("<Header>" font-lock-type-face) ("Painting Name" font-lock-keyword-face) ("\"Willow\"" font-lock-string-face) ("Painting Height" font-lock-keyword-face) ("2206" font-lock-constant-face) ("Script Feature Flags" font-lock-keyword-face) ("0x000000034" font-lock-string-face) ("</Header>" font-lock-type-face) ("<Events>" font-lock-type-face) ("EvType" font-lock-keyword-face) ("Command" font-lock-constant-face) ("CommandID" font-lock-keyword-face) ("SetForeColor" font-lock-constant-face) ("ParamType" font-lock-keyword-face) ("Pixel" font-lock-constant-face) ("Value" font-lock-keyword-face) ("0x0FF7386A0" font-lock-string-face) ("<EventPt>" font-lock-type-face) ("Wait:" font-lock-string-face) ("0.018s" font-lock-constant-face) ("Loc:" font-lock-keyword-face) ("1086.56" font-lock-constant-face) ("559.258" font-lock-constant-face) ("Pr:" font-lock-keyword-face) ("0.156599" font-lock-constant-face) ("</EventPt>" font-lock-type-face) ("</Events>" font-lock-type-face)))"#
        ]],
    )
}

fn composing_a_header_and_stroke_with_editor_commands_tracks_point_and_indentation()
-> ParityBatchCase {
    ParityBatchCase::value(
        "composing_a_header_and_stroke_with_editor_commands_tracks_point_and_indentation",
        r##"(with-temp-buffer
  (arscript-mode)
  (setq-local tab-width 3)
  (insert "<Header>")
  (newline-and-indent)
  (insert "Painting Name: \"Willow\"")
  (newline-and-indent)
  (insert "Painting DPI: 200")
  (newline-and-indent)
  (insert "</Header>")
  (arscript-indent-line)
  (end-of-line)
  (newline-and-indent)
  (insert "<Events>")
  (newline-and-indent)
  (insert "<StrokeEvent>")
  (newline-and-indent)
  (insert "<StrokeHeader>")
  (newline-and-indent)
  (insert "Loc: (1054.6, 527.3) Dr: (-0.919609, -0.392834)")
  (newline-and-indent)
  (insert "</StrokeHeader>")
  (arscript-indent-line)
  (end-of-line)
  (newline-and-indent)
  (insert "</StrokeEvent>")
  (arscript-indent-line)
  (end-of-line)
  (newline-and-indent)
  (insert "</Events>")
  (arscript-indent-line)
  (list
   (buffer-string)
   (point)
   (line-number-at-pos)
   (current-indentation)
   (mapcar
    (lambda (line)
      (goto-char (point-min))
      (forward-line (1- line))
      (current-indentation))
    (number-sequence 1 11))))"##,
        expect![[
            r#"OK ("<Header>\n   Painting Name: \"Willow\"\n   Painting DPI: 200\n</Header>\n<Events>\n   <StrokeEvent>\n      <StrokeHeader>\n         Loc: (1054.6, 527.3) Dr: (-0.919609, -0.392834)\n      </StrokeHeader>\n   </StrokeEvent>\n</Events>" 212 11 0 (0 3 3 0 0 3 6 9 6 3 0))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        opening_formatting_extending_and_saving_a_real_arscript_file(),
        formatting_a_pasted_recording_produces_stable_nested_art_script_structure(),
        disabling_reenabling_and_rewriting_recorded_actions_preserves_the_document(),
        editing_and_refontifying_a_header_and_paint_event_updates_visible_semantics(),
        composing_a_header_and_stroke_with_editor_commands_tracks_point_and_indentation(),
    ]
}

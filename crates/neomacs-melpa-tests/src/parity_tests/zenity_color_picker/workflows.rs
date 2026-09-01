use expect_test::expect;

use super::ParityBatchCase;

fn inserts_a_selected_accent_color_into_a_real_stylesheet() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-zenity-cp-test--with-runner
    "zenity-color-picker-insert" "rgb(17,34,255)\n" 0
  (with-temp-buffer
    (css-mode)
    (insert
     ":root {\n"
     "  --accent: ;\n"
     "  --label: \"Δ release\";\n"
     "}\n")
    (set-mark (point-max))
    (setq mark-active t)
    (goto-char (point-min))
    (search-forward "--accent: ")
    (set-buffer-modified-p nil)
    (string-match "\\(release\\)" "pre-release-post")
    (let ((before-match-data (match-data))
          (before-point (point))
          (returned (call-interactively #'zenity-cp-insert-color-at-point)))
      (list
       :return returned
       :before-point before-point
       :point (point)
       :line (line-number-at-pos)
       :column (current-column)
       :mode major-mode
       :mark (mark t)
       :mark-active mark-active
       :modified (buffer-modified-p)
       :text (buffer-substring-no-properties (point-min) (point-max))
       :match-data-before before-match-data
       :match-data-after (match-data)
       :argv (neomacs-zenity-cp-test--read-file log)))))
"####;
    let expect = expect![[
        r####"OK (:return nil :before-point 21 :point 28 :line 2 :column 19 :mode css-mode :mark 56 :mark-active t :modified t :text ":root {\n  --accent: #1122ff;\n  --label: \"Δ release\";\n}\n" :match-data-before (4 11 4 11) :match-data-after (4 11 4 11) :argv "program=[ORACLE-SANDBOX]/zenity-color-picker-insert/bin/zenity\nargc=1\narg[0]=--color-selection\n")"####
    ]];
    ParityBatchCase::value(
        "inserts_a_selected_accent_color_into_a_real_stylesheet",
        elisp_form,
        expect,
    )
}

fn adjusts_the_color_under_point_and_sends_the_exact_initial_value() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-zenity-cp-test--with-runner
    "zenity-color-picker-adjust" "#a0b1c2\n" 0
  (with-temp-buffer
    (css-mode)
    (insert
     ".banner {\n"
     "  color: #1122ff;\n"
     "  border-color: #778899;\n"
     "  content: \"Ω release\";\n"
     "}\n")
    (goto-char (point-min))
    (search-forward "#1122ff")
    (let ((before-point (point))
          (before-bounds (bounds-of-thing-at-point 'color))
          (before-color (zenity-cp-color-at-point))
          (returned (call-interactively #'zenity-cp-adjust-color-at-point)))
      (list
       :return returned
       :before (list
                :point before-point
                :bounds before-bounds
                :color before-color)
       :after (list
               :point (point)
               :line (line-number-at-pos)
               :column (current-column)
               :bounds (bounds-of-thing-at-point 'color)
               :color (zenity-cp-color-at-point))
       :text (buffer-substring-no-properties (point-min) (point-max))
       :argv (neomacs-zenity-cp-test--read-file log)))))
"####;
    let expect = expect![[
        r####"OK (:return nil :before (:point 27 :bounds (20 . 27) :color (17 34 255)) :after (:point 20 :line 2 :column 9 :bounds (20 . 27) :color (160 177 194)) :text ".banner {\n  color: #a0b1c2;\n  border-color: #778899;\n  content: \"Ω release\";\n}\n" :argv "program=[ORACLE-SANDBOX]/zenity-color-picker-adjust/bin/zenity\nargc=2\narg[0]=--color-selection\narg[1]=--color=#1122ff\n")"####
    ]];
    ParityBatchCase::value(
        "adjusts_the_color_under_point_and_sends_the_exact_initial_value",
        elisp_form,
        expect,
    )
}

fn dwim_adjusts_existing_colors_and_inserts_at_empty_properties() -> ParityBatchCase {
    let elisp_form = r####"
(let (results)
  (dolist (fixture
           '(("adjust"
              "  color: #336699;\n"
              "#336699"
              "rgb(204,85,0)"
              color)
             ("insert"
              "  border-color: ;\n"
              "border-color: "
              "rgb(1,2,3)"
              empty)))
    (push
     (neomacs-zenity-cp-test--with-runner
         (concat "zenity-color-picker-dwim-" (car fixture))
         (nth 3 fixture) 0
       (with-temp-buffer
         (css-mode)
         (insert
          ".notice {\n"
          (nth 1 fixture)
          "  content: \"release λ\";\n"
          "}\n")
         (goto-char (point-min))
         (search-forward (nth 2 fixture))
         (when (eq (nth 4 fixture) 'color)
           (backward-char 3))
         (let ((before
                (list
                 :point (point)
                 :color (zenity-cp-color-at-point)
                 :bounds (bounds-of-thing-at-point 'color)))
               (returned
                (call-interactively #'zenity-cp-color-at-point-dwim)))
           (list
            :case (car fixture)
            :return returned
            :before before
            :point (point)
            :line (line-number-at-pos)
            :column (current-column)
            :color (zenity-cp-color-at-point)
            :bounds (bounds-of-thing-at-point 'color)
            :text (buffer-substring-no-properties (point-min) (point-max))
            :argv (neomacs-zenity-cp-test--read-file log)))))
     results))
  (nreverse results))
"####;
    let expect = expect![[
        r####"OK ((:case "adjust" :return nil :before (:point 24 :color (51 102 153) :bounds (20 . 27)) :point 20 :line 2 :column 9 :color (204 85 0) :bounds (20 . 27) :text ".notice {\n  color: #cc5500;\n  content: \"release λ\";\n}\n" :argv "program=[ORACLE-SANDBOX]/zenity-color-picker-dwim-adjust/bin/zenity\nargc=2\narg[0]=--color-selection\narg[1]=--color=#336699\n") (:case "insert" :return nil :before (:point 27 :color nil :bounds nil) :point 34 :line 2 :column 23 :color (1 2 3) :bounds (27 . 34) :text ".notice {\n  border-color: #010203;\n  content: \"release λ\";\n}\n" :argv "program=[ORACLE-SANDBOX]/zenity-color-picker-dwim-insert/bin/zenity\nargc=1\narg[0]=--color-selection\n"))"####
    ]];
    ParityBatchCase::value(
        "dwim_adjusts_existing_colors_and_inserts_at_empty_properties",
        elisp_form,
        expect,
    )
}

fn preserves_user_text_across_cancel_malformed_output_and_missing_zenity() -> ParityBatchCase {
    let elisp_form = r####"
(let (results)
  (push
   (neomacs-zenity-cp-test--with-runner
       "zenity-color-picker-cancel-adjust" "" 1
     (with-temp-buffer
       (css-mode)
       (insert "body { color: #123456; }\n")
       (goto-char (point-min))
       (search-forward "#123456")
       (list
        :case 'cancel-adjust
        :outcome
        (neomacs-zenity-cp-test--outcome
         (lambda () (call-interactively #'zenity-cp-adjust-color-at-point)))
        :point (point)
        :text (buffer-substring-no-properties (point-min) (point-max))
        :argv (neomacs-zenity-cp-test--read-file log))))
   results)
  (push
   (neomacs-zenity-cp-test--with-runner
       "zenity-color-picker-cancel-insert" "" 1
     (with-temp-buffer
       (css-mode)
       (insert "body { color: ; }\n")
       (goto-char (point-min))
       (search-forward "color: ")
       (list
        :case 'cancel-insert
        :outcome
        (neomacs-zenity-cp-test--outcome
         (lambda () (call-interactively #'zenity-cp-insert-color-at-point)))
        :point (point)
        :text (buffer-substring-no-properties (point-min) (point-max))
        :argv (neomacs-zenity-cp-test--read-file log))))
   results)
  (push
   (neomacs-zenity-cp-test--with-runner
       "zenity-color-picker-malformed" "not-a-color\n" 0
     (with-temp-buffer
       (css-mode)
       (insert "body { color: ; }\n")
       (goto-char (point-min))
       (search-forward "color: ")
       (list
        :case 'malformed-output
        :outcome
        (neomacs-zenity-cp-test--outcome
         (lambda () (call-interactively #'zenity-cp-insert-color-at-point)))
        :point (point)
        :text (buffer-substring-no-properties (point-min) (point-max))
        :argv (neomacs-zenity-cp-test--read-file log))))
   results)
  (with-temp-buffer
    (css-mode)
    (insert "body { color: ; }\n")
    (goto-char (point-min))
    (search-forward "color: ")
    (let ((zenity-cp-zenity-bin
           (expand-file-name
            "missing-zenity"
            (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
      (push
       (list
        :case 'missing-executable
        :outcome
        (neomacs-zenity-cp-test--outcome
         (lambda () (call-interactively #'zenity-cp-insert-color-at-point)))
        :point (point)
        :text (buffer-substring-no-properties (point-min) (point-max)))
       results)))
  (nreverse results))
"####;
    let expect = expect![[
        r####"OK ((:case cancel-adjust :outcome (:value nil) :point 22 :text "body { color: #123456; }\n" :argv "program=[ORACLE-SANDBOX]/zenity-color-picker-cancel-adjust/bin/zenity\nargc=2\narg[0]=--color-selection\narg[1]=--color=#123456\n") (:case cancel-insert :outcome (:signal wrong-number-of-arguments :data (#1=(r g b) 0)) :point 15 :text "body { color: ; }\n" :argv "program=[ORACLE-SANDBOX]/zenity-color-picker-cancel-insert/bin/zenity\nargc=1\narg[0]=--color-selection\n") (:case malformed-output :outcome (:signal wrong-number-of-arguments :data (#1# 0)) :point 15 :text "body { color: ; }\n" :argv "program=[ORACLE-SANDBOX]/zenity-color-picker-malformed/bin/zenity\nargc=1\narg[0]=--color-selection\n") (:case missing-executable :outcome (:signal error :data ("Could not find [ORACLE-SANDBOX]/missing-zenity")) :point 15 :text "body { color: ; }\n"))"####
    ]];
    ParityBatchCase::value(
        "preserves_user_text_across_cancel_malformed_output_and_missing_zenity",
        elisp_form,
        expect,
    )
}

fn exposes_the_documented_three_digit_css_color_limitation() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-zenity-cp-test--with-runner
    "zenity-color-picker-short-css" "rgb(17,34,51)\n" 0
  (with-temp-buffer
    (css-mode)
    (insert "a { color: #abc; border: 1px solid; }\n")
    (goto-char (point-min))
    (search-forward "#abc")
    (let ((before
           (list
            :point (point)
            :color (zenity-cp-color-at-point)
            :bounds (bounds-of-thing-at-point 'color)))
          (returned (call-interactively #'zenity-cp-color-at-point-dwim)))
      (list
       :return returned
       :before before
       :point (point)
       :line (line-number-at-pos)
       :column (current-column)
       :color (zenity-cp-color-at-point)
       :bounds (bounds-of-thing-at-point 'color)
       :text (buffer-substring-no-properties (point-min) (point-max))
       :argv (neomacs-zenity-cp-test--read-file log)))))
"####;
    let expect = expect![[
        r####"OK (:return nil :before (:point 16 :color nil :bounds nil) :point 23 :line 1 :column 22 :color nil :bounds nil :text "a { color: #abc#112233; border: 1px solid; }\n" :argv "program=[ORACLE-SANDBOX]/zenity-color-picker-short-css/bin/zenity\nargc=1\narg[0]=--color-selection\n")"####
    ]];
    ParityBatchCase::value(
        "exposes_the_documented_three_digit_css_color_limitation",
        elisp_form,
        expect,
    )
}

pub(crate) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        inserts_a_selected_accent_color_into_a_real_stylesheet(),
        adjusts_the_color_under_point_and_sends_the_exact_initial_value(),
        dwim_adjusts_existing_colors_and_inserts_at_empty_properties(),
        preserves_user_text_across_cancel_malformed_output_and_missing_zenity(),
        exposes_the_documented_three_digit_css_color_limitation(),
    ]
}

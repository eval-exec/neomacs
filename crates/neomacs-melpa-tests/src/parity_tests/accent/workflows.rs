use expect_test::expect;

use super::ParityBatchCase;

fn accent_menu_accents_the_character_before_point_through_the_real_popup() -> ParityBatchCase {
    ParityBatchCase::value(
        "accent_menu_accents_the_character_before_point_through_the_real_popup",
        r##"(accent-test-with-live-buffer
 (insert "Le cafe est pret")
 (goto-char 8)
 (execute-kbd-macro (vconcat (kbd "C-x C-a") (kbd "<down> RET")))
 (let ((after-first (buffer-string)))
   (goto-char (point-max))
   (backward-char 1)
   (execute-kbd-macro (vconcat (kbd "C-x C-a") (kbd "<down> <down> <down> RET")))
   (list after-first
         (buffer-string)
         (point)
         (buffer-size)
         accent-position
         (buffer-modified-p))))"##,
        expect![[r#"OK ("Le café est pret" "Le café est prët" 16 16 before t)"#]],
    )
}

fn accent_position_after_replaces_the_character_under_the_cursor() -> ParityBatchCase {
    ParityBatchCase::value(
        "accent_position_after_replaces_the_character_under_the_cursor",
        r##"(accent-test-with-live-buffer
 (insert "Le cafe pres")
 (goto-char 11)
 (let ((accent-position 'after))
   (execute-kbd-macro (vconcat (kbd "C-x C-a") (kbd "<down> <down> RET"))))
 (list (buffer-string)
       (point)
       (char-after)
       (char-before)
       (buffer-size)))"##,
        expect![[r#"OK ("Le cafe prês" 12 115 234 12)"#]],
    )
}

fn accent_custom_characters_are_appended_and_reachable_from_the_popup() -> ParityBatchCase {
    ParityBatchCase::value(
        "accent_custom_characters_are_appended_and_reachable_from_the_popup",
        r##"(accent-test-with-live-buffer
 (insert "Xin chao ban a")
 (let ((accent-custom '((a (ằ ắ)) (z (ẑ)))))
   (let ((merged (accent-lst)))
     (execute-kbd-macro
      (vconcat (kbd "C-x C-a")
               (kbd "<down> <down> <down> <down> <down> <down> <down> <down> RET")))
     (list (buffer-string)
           (point)
           (length merged)
           (assoc 'a merged)
           (assoc 'z merged)
           (assoc 'e merged)
           (assoc 'a accent-diacritics)))))"##,
        expect![[
            r#"OK ("Xin chao ban ằ" 15 22 (a (à á â ä æ ã å ā ằ ắ)) (z (ž ź ż ẑ)) (e (è é ê ë ē ė ę)) (a (à á â ä æ ã å ā)))"#
        ]],
    )
}

fn accent_menu_reports_an_unaccentable_character_and_leaves_the_buffer_alone() -> ParityBatchCase {
    ParityBatchCase::value(
        "accent_menu_reports_an_unaccentable_character_and_leaves_the_buffer_alone",
        r##"(accent-test-with-live-buffer
 (insert "Bonjour!")
 (let ((before (buffer-string))
       (position (point)))
   (execute-kbd-macro (kbd "C-x C-a"))
   (let ((unaccentable (list (buffer-string) (point) (accent-test-last-message))))
     (goto-char (point-min))
     (let ((at-buffer-start
            (condition-case error
                (progn (accent-menu) 'no-error)
              (error (list 'signal (car error) (cdr error))))))
       (list before
             position
             unaccentable
             at-buffer-start
             (buffer-string)
             (point)
             (buffer-modified-p))))))"##,
        expect![[
            r#"OK ("Bonjour!" 9 ("Bonjour!" 9 "No accented characters available") (signal wrong-type-argument (characterp nil)) "Bonjour!" 1 t)"#
        ]],
    )
}

fn accent_popup_can_be_cancelled_without_touching_the_word() -> ParityBatchCase {
    ParityBatchCase::value(
        "accent_popup_can_be_cancelled_without_touching_the_word",
        r##"(accent-test-with-live-buffer
 (insert "cafe")
 (let ((outcome
        (condition-case nil
            (progn
              (execute-kbd-macro (vconcat (kbd "C-x C-a") (kbd "C-g")))
              'completed)
          (quit 'quit))))
   (list (buffer-string) (point) outcome (buffer-modified-p))))"##,
        expect![[r#"OK ("cafe" 5 quit t)"#]],
    )
}

fn accent_company_backend_answers_the_documented_command_protocol() -> ParityBatchCase {
    ParityBatchCase::value(
        "accent_company_backend_answers_the_documented_command_protocol",
        r##"(accent-test-with-live-buffer
 (insert "cafe")
 (let ((before (list (accent-company 'prefix)
                     (accent-company 'candidates)))
       after)
   (goto-char (1- (point)))
   (let ((accent-position 'after))
     (setq after (list (accent-company 'prefix)
                       (accent-company 'candidates))))
   (goto-char (point-max))
   (insert "!")
   (list before
         after
         (list (accent-company 'prefix)
               (accent-company 'candidates))
         (buffer-string)
         (point))))"##,
        expect![[
            r#"OK (("e" ("è" "é" "ê" "ë" "ē" "ė" "ę")) ("e" ("fè" "fé" "fê" "fë" "fē" "fė" "fę")) (nil nil) "cafe!" 6)"#
        ]],
    )
}

fn accent_corfu_offers_the_diacritics_through_completion_at_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "accent_corfu_offers_the_diacritics_through_completion_at_point",
        r##"(accent-test-with-live-buffer
 (insert "cafe")
 (accent-corfu)
 (let ((completions
        (and (get-buffer "*Completions*")
             (with-current-buffer "*Completions*"
               (buffer-substring-no-properties (point-min) (point-max))))))
   (let ((capf (car completion-at-point-functions)))
     (list (buffer-string)
           (point)
           (local-variable-p 'completion-at-point-functions)
           (length completion-at-point-functions)
           (funcall capf)
           completions))))"##,
        expect![[
            r#"OK ("caf" 4 t 1 (4 4 ("è" "é" "ê" "ë" "ē" "ė" "ę") :exclusive no) "Type M-RET on a completion to select it.\nType M-<down> or M-<up> to move point between completions.\n\n7 possible completions:\nè \11é \11ê\në \11ē \11ė\nę")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        accent_menu_accents_the_character_before_point_through_the_real_popup(),
        accent_position_after_replaces_the_character_under_the_cursor(),
        accent_custom_characters_are_appended_and_reachable_from_the_popup(),
        accent_menu_reports_an_unaccentable_character_and_leaves_the_buffer_alone(),
        accent_popup_can_be_cancelled_without_touching_the_word(),
        accent_company_backend_answers_the_documented_command_protocol(),
        accent_corfu_offers_the_diacritics_through_completion_at_point(),
    ]
}

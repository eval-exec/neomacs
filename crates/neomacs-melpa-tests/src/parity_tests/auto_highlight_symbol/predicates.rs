use expect_test::expect;

use super::ParityBatchCase;

fn auto_highlight_symbol_symbol_predicate_handles_default_regexp_case_and_nodefs() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_highlight_symbol_symbol_predicate_handles_default_regexp_case_and_nodefs",
        r##"(mapcar
                           (lambda (case)
                             (let ((ahs-case-fold-search
                                    (car case)))
                               (list
                                case
                                (ahs-symbol-p
                                 nil
                                 (cdr case))
                                (ahs-symbol-p
                                 nil
                                 (cdr case)
                                 t))))
                           '((t . "Alpha_42")
                             (nil . "Alpha_42")
                             (t . "two words")
                             (nil . "")
                             (t . "λ-value")
                             (t . "path/to:file")))"##,
        expect![[
            r#"OK (((t . "Alpha_42") 0 nil) ((nil . "Alpha_42") 0 nil) ((t . "two words") nil nil) ((nil . "") nil nil) ((t . "λ-value") nil nil) ((t . "path/to:file") 0 nil))"#
        ]],
    )
}

fn auto_highlight_symbol_symbol_predicate_supports_regex_function_and_mode_alist() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_highlight_symbol_symbol_predicate_supports_regex_function_and_mode_alist",
        r##"(let ((starts-with-user
                                (lambda (symbol)
                                  (string-prefix-p
                                   "user-"
                                   symbol))))
                           (mapcar
                            (lambda (case)
                              (let ((major-mode
                                     (car case))
                                    (predicate
                                     (cadr case))
                                    (symbol
                                     (caddr case)))
                                (list
                                 case
                                 (ahs-symbol-p
                                  predicate
                                  symbol))))
                            `((emacs-lisp-mode
                               "^user-"
                               "User-name")
                              (emacs-lisp-mode
                               ,starts-with-user
                               "user-name")
                              (text-mode
                               ((emacs-lisp-mode
                                  . "^elisp-")
                                (text-mode
                                  . "^text-"))
                               "text-value")
                              (python-mode
                               ((emacs-lisp-mode
                                  . "^elisp-"))
                               "anything")
                              (emacs-lisp-mode
                               ((emacs-lisp-mode
                                  . "^elisp-"))
                               "other"))))"##,
        expect![[
            r#"OK (((emacs-lisp-mode "^user-" "User-name") 0) ((emacs-lisp-mode #[(symbol) ((string-prefix-p "user-" symbol)) (t)] "user-name") t) ((text-mode ((emacs-lisp-mode . "^elisp-") (text-mode . "^text-")) "text-value") 0) ((python-mode ((emacs-lisp-mode . "^elisp-")) "anything") 0) ((emacs-lisp-mode ((emacs-lisp-mode . "^elisp-")) "other") nil))"#
        ]],
    )
}

fn auto_highlight_symbol_highlight_predicate_extracts_real_symbol_bounds_and_rules()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_highlight_predicate_extracts_real_symbol_bounds_and_rules",
        r##"(with-temp-buffer
                           (emacs-lisp-mode)
                           (insert
                            "(let ((alpha-value 1))\n  (+ alpha-value 2))")
                           (mapcar
                            (lambda (case)
                              (goto-char
                               (car case))
                              (let ((ahs-include
                                     (cadr case))
                                    (ahs-exclude
                                     (caddr case))
                                    (ahs-inhibit-face-list
                                     nil))
                                (list
                                 case
                                 (ahs-highlight-p))))
                            '((10 nil nil)
                              (10 "^alpha" nil)
                              (10 "^beta" nil)
                              (10 nil "^alpha")
                              (1 nil nil)
                              (24 nil nil)
                              (39 nil nil))))"##,
        expect![[
            r#"OK (((10 nil nil) ("alpha-value" 8 19)) ((10 "^alpha" nil) ("alpha-value" 8 19)) ((10 "^beta" nil) nil) ((10 nil "^alpha") nil) ((1 nil nil) nil) ((24 nil nil) nil) ((39 nil nil) ("alpha-value" 29 40)))"#
        ]],
    )
}

fn auto_highlight_symbol_highlight_predicate_rejects_inhibited_text_and_overlay_faces()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_highlight_predicate_rejects_inhibited_text_and_overlay_faces",
        r##"(with-temp-buffer
                           (insert
                            (propertize
                             "commented"
                             'face
                             'font-lock-comment-face)
                            " plain overlayed")
                           (let ((overlay
                                  (make-overlay
                                   17
                                   26)))
                             (overlay-put
                              overlay
                              'face
                              'font-lock-string-face)
                             (mapcar
                              (lambda (case)
                                (goto-char
                                 (car case))
                                (let ((ahs-face-check-include-overlay
                                       (cdr case))
                                      (ahs-inhibit-face-list
                                       '(font-lock-comment-face
                                         font-lock-string-face)))
                                  (list
                                   case
                                   (ahs-highlight-p)
                                   (ahs-get-overlay-face
                                    (point)))))
                              '((1)
                                (11)
                                (17)
                                (17 . t)))))"##,
        expect![[
            r#"OK (((1) nil nil) ((11) ("plain" 11 16) nil) ((17) ("overlayed" 17 26) (font-lock-string-face)) ((17 . t) nil (font-lock-string-face)))"#
        ]],
    )
}

fn auto_highlight_symbol_dropdown_expansion_suppresses_symbol_detection_only_when_active()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_dropdown_expansion_suppresses_symbol_detection_only_when_active",
        r##"(with-temp-buffer
                           (insert "candidate")
                           (goto-char 3)
                           (mapcar
                            (lambda (case)
                              (pcase-let
                                  ((`(,feature ,overlays)
                                    case))
                                (if feature
                                    (provide
                                     'dropdown-list)
                                  (setq features
                                        (delq
                                         'dropdown-list
                                         features)))
                                (setq
                                 dropdown-list-overlays
                                 overlays)
                                (list
                                 case
                                 (ahs-dropdown-list-p)
                                 (ahs-highlight-p))))
                            '((nil nil)
                              (t nil)
                              (t (fixture-overlay)))))"##,
        expect![[
            r#"OK (((nil nil) nil ("candidate" 1 10)) ((t nil) nil ("candidate" 1 10)) ((t #1=(fixture-overlay)) #1# nil))"#
        ]],
    )
}

fn auto_highlight_symbol_face_predicate_handles_symbols_lists_and_missing_matches()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_face_predicate_handles_symbols_lists_and_missing_matches",
        r##"(progn
                           (defvar fixture-faces nil)
                           (setq
                            fixture-faces
                            '(font-lock-comment-face
                              font-lock-string-face))
                           (mapcar
                            (lambda (face)
                              (list
                               face
                               (ahs-face-p
                                face
                                'fixture-faces)))
                            '(font-lock-comment-face
                              font-lock-keyword-face
                              (font-lock-keyword-face
                               font-lock-string-face)
                              (font-lock-comment-face
                               font-lock-string-face)
                              nil)))"##,
        expect![
            "OK ((font-lock-comment-face (font-lock-comment-face font-lock-string-face)) (font-lock-keyword-face nil) ((font-lock-keyword-face font-lock-string-face) font-lock-string-face) ((font-lock-comment-face font-lock-string-face) font-lock-comment-face) (nil nil))"
        ],
    )
}

fn auto_highlight_symbol_prepare_highlight_validates_plugin_boundaries_and_abort() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_highlight_symbol_prepare_highlight_validates_plugin_boundaries_and_abort",
        r##"(progn
                           (defvar auto-highlight-symbol-test-range-reversed
                             '((name . "reversed")
                               (start . point-max)
                               (end . point-min)))
                           (defvar auto-highlight-symbol-test-range-nonnumeric
                             '((name . "nonnumeric")
                               (start . "one")
                               (end . point-max)))
                           (defvar auto-highlight-symbol-test-range-abort
                             '((name . "abort")
                               (before-search . (lambda (_symbol) 'abort))
                               (start . point-min)
                               (end . point-max)))
                           (add-to-list
                            'ahs-range-plugin-list
                            'auto-highlight-symbol-test-range-reversed)
                           (add-to-list
                            'ahs-range-plugin-list
                            'auto-highlight-symbol-test-range-nonnumeric)
                           (add-to-list
                            'ahs-range-plugin-list
                            'auto-highlight-symbol-test-range-abort)
                           (with-temp-buffer
                             (insert "alpha beta")
                             (mapcar
                              (lambda (range)
                                (setq
                                 ahs-current-range
                                 (symbol-value range))
                                (list
                                 range
                                 (ahs-prepare-highlight
                                  "alpha")))
                              '(ahs-range-whole-buffer
                                auto-highlight-symbol-test-range-reversed
                                auto-highlight-symbol-test-range-nonnumeric
                                auto-highlight-symbol-test-range-abort))))"##,
        expect![
            "OK ((ahs-range-whole-buffer (1 . 11)) (auto-highlight-symbol-test-range-reversed nil) (auto-highlight-symbol-test-range-nonnumeric nil) (auto-highlight-symbol-test-range-abort nil))"
        ],
    )
}

fn auto_highlight_symbol_disabled_modes_commands_and_flags_gate_real_idle_highlighting()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_disabled_modes_commands_and_flags_gate_real_idle_highlighting",
        r##"(save-window-excursion
                           (with-temp-buffer
                             (switch-to-buffer
                              (current-buffer))
                             (emacs-lisp-mode)
                             (insert
                              "alpha alpha")
                             (goto-char 2)
                             (auto-highlight-symbol-mode 1)
                             (setq
                              ahs-current-range
                              ahs-range-whole-buffer)
                             (mapcar
                              (lambda (case)
                                (ahs-remove-all-overlay t)
                                (setq
                                 this-command
                                 (if
                                     (eq case 'command)
                                     'fixture-disabled
                                   'fixture-command)
                                 ahs-disabled-commands
                                 '(fixture-disabled)
                                 ahs-disabled-minor-modes
                                 '(fixture-minor-mode)
                                 ahs-disabled-flags
                                 '(fixture-flag)
                                 fixture-minor-mode
                                 (eq case 'minor-mode)
                                 fixture-flag
                                 (eq case 'flag))
                                (list
                                 case
                                 (ahs--do-hl)
                                 (auto-highlight-symbol-test-overlays)))
                              '(enabled
                                command
                                minor-mode
                                flag))))"##,
        expect![
            "OK ((enabled t ((1 6 current ahs-plugin-whole-buffer-face 1000 t t) (1 6 others ahs-face-unfocused nil t t) (7 12 others ahs-face-unfocused nil t t))) (command nil nil) (minor-mode nil nil) (flag nil nil))"
        ],
    )
    .fresh_process()
}

fn auto_highlight_symbol_case_policy_changes_search_matches_without_changing_bounds()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_highlight_symbol_case_policy_changes_search_matches_without_changing_bounds",
        r##"(with-temp-buffer
                           (insert
                            "Alpha alpha ALPHA alphabet")
                           (mapcar
                            (lambda (case-fold)
                              (let ((ahs-case-fold-search
                                     case-fold)
                                    (ahs-search-work nil))
                                (ahs-search-symbol
                                 "alpha"
                                 (cons
                                  (point-min)
                                  (point-max)))
                                (list
                                 case-fold
                                 (mapcar
                                  (lambda (match)
                                    (list
                                     (nth 0 match)
                                     (nth 1 match)))
                                  ahs-search-work))))
                            '(nil t)))"##,
        expect!["OK ((nil ((7 12))) (t ((1 6) (7 12) (13 18))))"],
    )
}

pub(super) fn predicates_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_highlight_symbol_symbol_predicate_handles_default_regexp_case_and_nodefs(),
        auto_highlight_symbol_symbol_predicate_supports_regex_function_and_mode_alist(),
        auto_highlight_symbol_highlight_predicate_extracts_real_symbol_bounds_and_rules(),
        auto_highlight_symbol_highlight_predicate_rejects_inhibited_text_and_overlay_faces(),
        auto_highlight_symbol_dropdown_expansion_suppresses_symbol_detection_only_when_active(),
        auto_highlight_symbol_face_predicate_handles_symbols_lists_and_missing_matches(),
        auto_highlight_symbol_prepare_highlight_validates_plugin_boundaries_and_abort(),
        auto_highlight_symbol_disabled_modes_commands_and_flags_gate_real_idle_highlighting(),
        auto_highlight_symbol_case_policy_changes_search_matches_without_changing_bounds(),
    ]
}

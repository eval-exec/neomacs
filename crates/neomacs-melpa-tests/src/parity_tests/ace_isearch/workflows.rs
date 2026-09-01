use expect_test::expect;

use super::ParityBatchCase;

/// The package's headline story: with `ace-isearch-mode' on, `C-s' plus a
/// single character hands the search over to avy after
/// `ace-isearch-jump-delay', and `ace-isearch-pop-mark' returns to where the
/// search started.
fn one_character_search_hands_the_jump_to_avy_and_pop_mark_returns_to_the_origin() -> ParityBatchCase
{
    ParityBatchCase::value(
        "one_character_search_hands_the_jump_to_avy_and_pop_mark_returns_to_the_origin",
        r##"(ace-isearch-test-with-live-buffer
 (ace-isearch-mode +1)
 (execute-kbd-macro (kbd "C-s p"))
 (let ((jumped (point))
       (jumped-line (line-number-at-pos)))
   (ace-isearch-pop-mark)
   (list jumped
         jumped-line
         (point)
         (buffer-size)
         isearch-string
         isearch-mode
         isearch--current-buffer
         (mark t)
         ace-isearch--ace-jump-or-avy
         ace-isearch-function
         ace-isearch-jump-delay
         ace-isearch-test-events
         isearch-update-post-hook
         (default-value 'isearch-update-post-hook)
         ace-isearch-mode
         (assq 'ace-isearch-mode minor-mode-alist)
         ace-isearch-lighter
         (ace-isearch-test-swoop-buffer))))"##,
        expect![[
            r#"OK (88 3 1 186 "" nil "*ace-isearch-workflow*" 1 avy avy-goto-word-1 0.3 ((avy-goto-word-1 (112) 1 "p" t (23 88 105)) (avy-pop-mark 88)) (ace-isearch--jumper-function t) nil t (ace-isearch-mode ace-isearch-lighter) " AceI" nil)"#
        ]],
    )
}

fn switch_commands_and_threshold_customizations_decide_when_and_how_a_jump_runs() -> ParityBatchCase
{
    ParityBatchCase::value(
        "switch_commands_and_threshold_customizations_decide_when_and_how_a_jump_runs",
        r##"(list
 (ace-isearch-test-with-live-buffer
  (let ((ace-isearch-jump-based-on-one-char nil))
    (ace-isearch-mode +1)
    (execute-kbd-macro (kbd "C-s t"))
    (list (point)
          (line-number-at-pos)
          isearch-string
          isearch-success
          ace-isearch-test-events
          ace-isearch--ace-jump-or-avy)))
 (ace-isearch-test-with-live-buffer
  (let ((ace-isearch-jump-based-on-one-char nil))
    (ace-isearch-mode +1)
    (execute-kbd-macro (kbd "C-s t h"))
    (list (point)
          (line-number-at-pos)
          isearch-string
          isearch-mode
          (mark t)
          (buffer-size)
          ace-isearch-test-events
          ace-isearch--ace-jump-or-avy
          ace-isearch-2-function)))
 (ace-isearch-test-with-live-buffer
  (let ((ace-isearch-use-jump 'printing-char))
    (kill-new "p")
    (ace-isearch-mode +1)
    (execute-kbd-macro (kbd "C-s C-y"))
    (list (point)
          (line-number-at-pos)
          isearch-string
          isearch-success
          ace-isearch-test-events)))
 (ace-isearch-test-with-live-buffer
  (let ((ace-isearch-use-jump t))
    (kill-new "p")
    (ace-isearch-mode +1)
    (execute-kbd-macro (kbd "C-s C-y"))
    (list (point)
          (line-number-at-pos)
          isearch-string
          isearch-success
          ace-isearch-test-events)))
 (ace-isearch-test-with-live-buffer
  (let* ((offered nil)
         (prompt nil)
         (ace-isearch-function ace-isearch-function)
         (completing-read-function
          (lambda (read-prompt collection &rest _)
            (setq prompt read-prompt
                  offered collection)
            "avy-goto-char")))
    (ace-isearch-mode +1)
    (ace-isearch-switch-function)
    (execute-kbd-macro (kbd "C-s p"))
    (list (point)
          (line-number-at-pos)
          prompt
          offered
          ace-isearch-function
          ace-isearch--ace-jump-or-avy
          ace-isearch-test-events
          (ace-isearch-test-last-message)))))"##,
        expect![[
            r#"OK ((12 1 "t" 12 nil avy) (38 2 "" nil 1 186 ((avy-goto-char-2 (116 104) 1 "th" t (19 38 84))) avy avy-goto-char-2) (24 1 "p" 24 nil) (88 3 "" 24 ((avy-goto-word-1 (112) 1 "p" t (23 88 105)))) (88 3 "Function for ace-isearch (current is avy-goto-word-1): " ("ace-jump-word-mode" "ace-jump-char-mode" "avy-goto-word-1" "avy-goto-subword-1" "avy-goto-word-or-subword-1" "avy-goto-char") avy-goto-char avy ((avy-goto-char (112) 1 "p" t (23 88 97 105))) "Function for ace-isearch is set to avy-goto-char."))"#
        ]],
    )
}

fn yanking_a_long_word_into_isearch_hands_the_query_to_helm_swoop() -> ParityBatchCase {
    ParityBatchCase::value(
        "yanking_a_long_word_into_isearch_hands_the_query_to_helm_swoop",
        r##"(ace-isearch-test-with-live-buffer
 (ace-isearch-mode +1)
 (execute-kbd-macro (kbd "C-s C-w"))
 (list (point)
       (line-number-at-pos)
       (buffer-size)
       isearch-string
       isearch-mode
       isearch--current-buffer
       ace-isearch-function-from-isearch
       ace-isearch-input-length
       ace-isearch-func-delay
       ace-isearch-test-events
       (ace-isearch-test-swoop-buffer)
       (car search-ring)
       (text-properties-at 0 (car search-ring))
       (length search-ring)))"##,
        expect![[
            r#"OK (8 1 186 "" nil "*ace-isearch-workflow*" ace-isearch-helm-swoop-from-isearch 6 0.0 ((helm-swoop "release" "*ace-isearch-workflow*" 8 nil)) "1: Release notes for the parser rewrite" #("release" 0 7 (isearch-case-fold-search t isearch-regexp-function nil)) (isearch-case-fold-search t isearch-regexp-function nil) 1)"#
        ]],
    )
    .fresh_process()
}

fn raising_input_length_keeps_six_characters_in_isearch_and_hands_longer_queries_to_swiper()
-> ParityBatchCase {
    ParityBatchCase::value(
        "raising_input_length_keeps_six_characters_in_isearch_and_hands_longer_queries_to_swiper",
        r##"(list
 (ace-isearch-test-with-live-buffer
  (search-forward "parser")
  (goto-char (match-beginning 0))
  (let ((ace-isearch-input-length 7)
        (ace-isearch-function-from-isearch 'ace-isearch-swiper-from-isearch))
    (ace-isearch-mode +1)
    (execute-kbd-macro (kbd "C-s C-w C-w"))
    (list (point)
          (line-number-at-pos)
          isearch-string
          isearch-mode
          (buffer-size)
          ace-isearch-test-events
          (ace-isearch-test-swoop-buffer)
          (car search-ring))))
 (ace-isearch-test-with-live-buffer
  (search-forward "parser")
  (goto-char (match-beginning 0))
  (let ((ace-isearch-input-length 7)
        (ace-isearch-function-from-isearch 'ace-isearch-swiper-from-isearch))
    (ace-isearch-mode +1)
    (define-key isearch-mode-map (kbd "C-'") 'ace-isearch-jump-during-isearch)
    (unwind-protect
        (progn
          (execute-kbd-macro (kbd "C-s C-w C-'"))
          (list (point)
                (line-number-at-pos)
                isearch-string
                isearch-mode
                (buffer-size)
                ace-isearch-test-events
                (ace-isearch-test-swoop-buffer)))
      (define-key isearch-mode-map (kbd "C-'") nil)))))"##,
        expect![[
            r#"OK ((37 1 "" nil 186 ((swiper "parser rewrite" "*ace-isearch-workflow*" 37 nil)) "1: Release notes for the parser rewrite" #("parser rewrite" 0 14 (isearch-case-fold-search t isearch-regexp-function nil))) (88 3 "parser" nil 186 ((avy-isearch nil 29 "parser" nil (23 88))) nil))"#
        ]],
    )
}

fn a_failing_search_invokes_the_fallback_function_with_a_regexp_quoted_query() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_failing_search_invokes_the_fallback_function_with_a_regexp_quoted_query",
        r##"(ace-isearch-test-with-live-buffer
 (let ((ace-isearch-use-jump nil)
       (ace-isearch-use-fallback-function t))
   (ace-isearch-mode +1)
   (execute-kbd-macro (kbd "C-s [ z"))
   (list (point)
         (line-number-at-pos)
         isearch-string
         isearch-success
         isearch-mode
         ace-isearch-fallback-function
         ace-isearch-test-events
         (ace-isearch-test-swoop-buffer)
         (car search-ring)
         (buffer-string))))"##,
        expect![[
            r#"OK (155 4 "[z" nil nil ace-isearch-helm-swoop-from-isearch ((helm-swoop "\\[z" "*ace-isearch-workflow*" 155 nil)) "" #("[z" 0 2 (isearch-case-fold-search t isearch-regexp-function nil)) "Release notes for the parser rewrite\nthe tokenizer now handles Unicode identifiers\nthe parser reports a precise column number\nfixture: naïve café resumé [see docs]\ntrailing summary line\n")"#
        ]],
    )
}

fn regexp_search_bypasses_ace_isearch_until_evil_mode_support_is_enabled() -> ParityBatchCase {
    ParityBatchCase::value(
        "regexp_search_bypasses_ace_isearch_until_evil_mode_support_is_enabled",
        r##"(list
 (ace-isearch-test-with-live-buffer
  (let ((ace-isearch-use-jump nil))
    (ace-isearch-mode +1)
    (execute-kbd-macro (kbd "C-M-s p a . s e r"))
    (list (point)
          (line-number-at-pos)
          isearch-string
          isearch-regexp
          isearch-success
          ace-isearch-on-evil-mode
          ace-isearch-test-events
          (ace-isearch-test-swoop-buffer)
          (car regexp-search-ring)
          search-ring)))
 (ace-isearch-test-with-live-buffer
  (let ((ace-isearch-use-jump nil)
        (ace-isearch-on-evil-mode t))
    (ace-isearch-mode +1)
    (execute-kbd-macro (kbd "C-M-s p a . s e r"))
    (list (point)
          (line-number-at-pos)
          isearch-string
          isearch-regexp
          ace-isearch-test-events
          (ace-isearch-test-swoop-buffer)
          (car regexp-search-ring)
          search-ring))))"##,
        expect![[
            r#"OK ((29 1 "pa.ser" t 29 nil nil nil #("pa.ser" 0 6 (isearch-case-fold-search t)) nil) (29 1 "" t ((helm-swoop "pa.ser" "*ace-isearch-workflow*" 29 nil)) "1: Release notes for the parser rewrite\n3: the parser reports a precise column number" #("pa.ser" 0 6 (isearch-case-fold-search t)) nil))"#
        ]],
    )
    .fresh_process()
}

fn global_ace_isearch_mode_skips_the_minibuffer_and_a_disabled_buffer_keeps_plain_isearch()
-> ParityBatchCase {
    ParityBatchCase::value(
        "global_ace_isearch_mode_skips_the_minibuffer_and_a_disabled_buffer_keeps_plain_isearch",
        r##"(let ((buffer (generate-new-buffer "*ace-isearch-workflow*")))
  (unwind-protect
      (progn
        (set-window-buffer (selected-window) buffer)
        (set-buffer buffer)
        (insert ace-isearch-test-text)
        (goto-char (point-min))
        (text-mode)
        (setq ace-isearch-test-events nil)
        (global-ace-isearch-mode +1)
        (list global-ace-isearch-mode
              ace-isearch-mode
              isearch-update-post-hook
              (with-current-buffer " *Minibuf-0*"
                (list (and (minibufferp) t) ace-isearch-mode))
              (with-temp-buffer
                (text-mode)
                (list ace-isearch-mode isearch-update-post-hook))
              (progn
                (ace-isearch-mode -1)
                (execute-kbd-macro (kbd "C-s p a r s e r"))
                (list (point)
                      (line-number-at-pos)
                      isearch-string
                      isearch-success
                      isearch-mode
                      (buffer-size)
                      ace-isearch-test-events
                      (ace-isearch-test-swoop-buffer)
                      isearch-update-post-hook))
              (progn
                (global-ace-isearch-mode -1)
                (list global-ace-isearch-mode
                      ace-isearch-mode
                      (with-temp-buffer (text-mode) ace-isearch-mode)))))
    (kill-buffer buffer)))"##,
        expect![[
            r#"OK (t t (ace-isearch--jumper-function t) (t nil) (t (ace-isearch--jumper-function t)) (29 1 "parser" 29 nil 186 nil nil nil) (nil nil nil))"#
        ]],
    )
    .fresh_process()
}

fn misconfigured_jump_and_swoop_backends_signal_and_leave_the_hook_installed() -> ParityBatchCase {
    ParityBatchCase::value(
        "misconfigured_jump_and_swoop_backends_signal_and_leave_the_hook_installed",
        r##"(list
 (ace-isearch-test-with-live-buffer
  (let ((ace-isearch-function 'avy-goto-line))
    (list (condition-case error (ace-isearch-mode +1) (error error))
          ace-isearch-mode
          isearch-update-post-hook
          (condition-case error (ace-isearch-mode -1) (error error))
          ace-isearch-mode
          isearch-update-post-hook)))
 (ace-isearch-test-with-live-buffer
  (let ((ace-isearch-function-from-isearch 'swoop-from-isearch))
    (ace-isearch-mode +1)
    (list (condition-case error
              (execute-kbd-macro (kbd "C-s C-w"))
            (error error))
          (point)
          (line-number-at-pos)
          isearch-string
          isearch-mode
          (buffer-size)
          ace-isearch-test-events
          (ace-isearch-test-swoop-buffer)))))"##,
        expect![[
            r#"OK (((error "Function name avy-goto-line for ace-isearch is invalid!") t (ace-isearch--jumper-function t) nil nil nil) ((error "function swoop-from-isearch is not bounded!") 8 1 "release" nil 186 nil nil))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        one_character_search_hands_the_jump_to_avy_and_pop_mark_returns_to_the_origin(),
        switch_commands_and_threshold_customizations_decide_when_and_how_a_jump_runs(),
        yanking_a_long_word_into_isearch_hands_the_query_to_helm_swoop(),
        raising_input_length_keeps_six_characters_in_isearch_and_hands_longer_queries_to_swiper(),
        a_failing_search_invokes_the_fallback_function_with_a_regexp_quoted_query(),
        regexp_search_bypasses_ace_isearch_until_evil_mode_support_is_enabled(),
        global_ace_isearch_mode_skips_the_minibuffer_and_a_disabled_buffer_keeps_plain_isearch(),
        misconfigured_jump_and_swoop_backends_signal_and_leave_the_hook_installed(),
    ]
}

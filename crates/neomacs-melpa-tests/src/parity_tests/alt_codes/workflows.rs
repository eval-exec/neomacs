use expect_test::expect;

use super::ParityBatchCase;

/// The package's whole purpose: hold Meta, type a Windows alt code on the
/// keypad, and get the character.  Four codes cover the ranges a user reaches
/// for -- an ASCII letter, a Latin-1 letter, a three digit code, and a
/// four digit code whose leading zero selects the Windows-1252 table -- and
/// each one leaves the pending code cleared for the next.  Code 32 is included
/// because the shipped table spells it "spc": alt-32 inserts those three
/// letters rather than a space, which is the data's own quirk and not a
/// rendering artefact.
fn typing_an_alt_code_on_the_keypad_inserts_the_character_it_names() -> ParityBatchCase {
    ParityBatchCase::value(
        "typing_an_alt_code_on_the_keypad_inserts_the_character_it_names",
        r##"(alt-codes-test-with-buffer
 (let ((max-lisp-eval-depth 12800))
   (list :ascii (alt-codes-test-enter "65")
         :latin1 (alt-codes-test-enter "225")
         :windows-1252 (alt-codes-test-enter "0128")
         :accented (alt-codes-test-enter "0193")
         :spelled-space (alt-codes-test-enter "32")
         :empty-entry (alt-codes-test-enter "189")
         :hook (alt-codes-test-hook))))"##,
        expect![[
            r#"OK (:ascii ("A" "") :latin1 ("ß" "") :windows-1252 ("€" "") :accented ("Á" "") :spelled-space ("spc" "") :empty-entry ("" "") :hook (t t t))"#
        ]],
    )
}

fn the_first_lookup_of_a_session_fails_at_the_default_eval_depth() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_first_lookup_of_a_session_fails_at_the_default_eval_depth",
        r##"(alt-codes-test-with-buffer
 (let ((mark (alt-codes-test-message-mark)))
   (apply #'alt-codes-test-type (alt-codes-test-code ?6 ?5))
   (let ((pending (copy-sequence alt-codes--code)))
     (alt-codes-test-type 'f5)
     (let ((after (list (copy-sequence (buffer-string))
                        (copy-sequence alt-codes--code)
                        (alt-codes-test-hook))))
       (alt-codes-test-type ?z)
       (list :depth max-lisp-eval-depth
             :table-entries (length alt-codes--list)
             :announced (alt-codes-test-messages-since mark "Alt Code")
             :pending pending
             :after-commit after
             :hook-error (alt-codes-test-messages-since mark "pre-command-hook")
             :typing-still-works (copy-sequence (buffer-string))
             :raising-the-limit-works
             (let ((max-lisp-eval-depth 12800))
               (alt-codes--get-symbol "65")))))))"##,
        expect![[
            r#"OK (:depth 1600 :table-entries 383 :announced ("[Alt Code]: 6" "[Alt Code]: 65") :pending "65" :after-commit ("" "65" (nil t t)) :hook-error ("Error in pre-command-hook (alt-codes--pre-command-hook): (excessive-lisp-nesting 1601)") :typing-still-works "z" :raising-the-limit-works "A")"#
        ]],
    )
}

fn the_keypad_digits_also_build_a_numeric_prefix_argument() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_keypad_digits_also_build_a_numeric_prefix_argument",
        r##"(alt-codes-test-with-buffer
 (setq prefix-arg nil current-prefix-arg nil)
 (execute-kbd-macro (vconcat (alt-codes-test-code ?1 ?2)))
 (list :pending (copy-sequence alt-codes--code)
       :prefix prefix-arg
       :keypad-translation (lookup-key function-key-map [kp-6])
       :meta-digit-command (key-binding (kbd "M-6"))
       :keypad-command (key-binding [M-kp-6])
       :commit-runs-that-many-times
       (let ((max-lisp-eval-depth 12800))
         (execute-kbd-macro (vconcat [?x]))
         (copy-sequence (buffer-string)))))"##,
        expect![[
            r#"OK (:pending "12" :prefix 12 :keypad-translation [54] :meta-digit-command digit-argument :keypad-command nil :commit-runs-that-many-times "x")"#
        ]],
    )
}

fn only_a_symbol_event_commits_the_pending_code() -> ParityBatchCase {
    ParityBatchCase::value(
        "only_a_symbol_event_commits_the_pending_code",
        r##"(alt-codes-test-with-buffer
 (let ((max-lisp-eval-depth 12800))
   (apply #'alt-codes-test-type (alt-codes-test-code ?6 ?5))
   (let ((pending (copy-sequence alt-codes--code)))
     (alt-codes-test-type ?x)
     (let ((after-letter (list (copy-sequence (buffer-string))
                               (copy-sequence alt-codes--code))))
       (alt-codes-test-type 'f5)
       (list :pending pending
             :after-a-letter after-letter
             :after-a-symbol (list (copy-sequence (buffer-string))
                                   (copy-sequence alt-codes--code)))))))"##,
        expect![[r#"OK (:pending "65" :after-a-letter ("x" "65") :after-a-symbol ("xA" ""))"#]],
    )
}

fn an_invalid_code_inserts_nothing_and_still_clears_the_pending_digits() -> ParityBatchCase {
    ParityBatchCase::value(
        "an_invalid_code_inserts_nothing_and_still_clears_the_pending_digits",
        r##"(alt-codes-test-with-buffer
 (let ((max-lisp-eval-depth 12800))
   (list :invalid (alt-codes-test-enter "9999")
         :next-code-is-unaffected (alt-codes-test-enter "65")
         :read-only
         (progn (erase-buffer)
                (setq buffer-read-only t)
                (apply #'alt-codes-test-type
                       (append (alt-codes-test-code ?6 ?5) (list 'f5)))
                (prog1 (list (copy-sequence (buffer-string))
                             (copy-sequence alt-codes--code))
                  (setq buffer-read-only nil))))))"##,
        expect![[r#"OK (:invalid ("" "") :next-code-is-unaffected ("A" "") :read-only ("" ""))"#]],
    )
}

fn the_mode_installs_and_removes_its_hook_and_leaves_plain_typing_alone() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_mode_installs_and_removes_its_hook_and_leaves_plain_typing_alone",
        r##"(list
 :lifecycle
 (alt-codes-test-with-buffer
  (let ((on (alt-codes-test-hook)))
    (alt-codes-mode -1)
    (list :on on :off (alt-codes-test-hook)
          :hook-value (copy-sequence pre-command-hook))))
 :mode-off
 (let ((buffer (generate-new-buffer "*alt-codes-off*")))
   (unwind-protect
       (progn
         (set-window-buffer (selected-window) buffer)
         (set-buffer buffer)
         (text-mode)
         (setq prefix-arg nil current-prefix-arg nil)
         (local-set-key [f5] #'ignore)
         (execute-kbd-macro "65")
         (let ((typed (copy-sequence (buffer-string))))
           (erase-buffer)
           (setq prefix-arg nil current-prefix-arg nil)
           (execute-kbd-macro (vconcat (append (alt-codes-test-code ?6 ?5) (list 'f5))))
           (list :digits typed
                 :keypad (copy-sequence (buffer-string))
                 :hook (and (memq #'alt-codes--pre-command-hook pre-command-hook) t))))
     (kill-buffer buffer)))
 :globalized
 (progn
   (global-alt-codes-mode 1)
   (let ((armed (with-temp-buffer (text-mode) (alt-codes-test-hook))))
     (global-alt-codes-mode -1)
     (list :armed armed
           :after (with-temp-buffer (text-mode) (alt-codes-test-hook))))))"##,
        expect![[
            r#"OK (:lifecycle (:on (t t t) :off (nil t nil) :hook-value (eldoc-pre-command-refresh-echo-area t)) :mode-off (:digits "65" :keypad "" :hook nil) :globalized (:armed (t t t) :after (nil t nil)))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        typing_an_alt_code_on_the_keypad_inserts_the_character_it_names(),
        the_first_lookup_of_a_session_fails_at_the_default_eval_depth(),
        the_keypad_digits_also_build_a_numeric_prefix_argument(),
        only_a_symbol_event_commits_the_pending_code(),
        an_invalid_code_inserts_nothing_and_still_clears_the_pending_digits(),
        the_mode_installs_and_removes_its_hook_and_leaves_plain_typing_alone(),
    ]
}

use expect_test::expect;

use super::ParityBatchCase;

/// The installation the README prescribes: `set-up-slime-ac' on the
/// `slime-mode' and `slime-repl-mode' hooks, optionally with a prefix argument
/// for the fuzzy source.  This pins which source each call installs, that a
/// second call does not install it twice, that `ac-sources' stays buffer local
/// so a Lisp buffer and the REPL are configured independently, that
/// auto-complete already knows both modes, and -- because a source that is
/// installed but cannot complete is worthless -- that completing at the REPL
/// prompt really reaches swank and inserts its answer.
fn set_up_slime_ac_installs_the_chosen_source_in_each_buffer_separately() -> ParityBatchCase {
    ParityBatchCase::value(
        "set_up_slime_ac_installs_the_chosen_source_in_each_buffer_separately",
        r##"(progn
  (require 'slime)
  (acs-test-connect)
  (let ((lisp (acs-test-lisp-buffer "(defun demo ()\n  (ca"))
        (fuzzy (generate-new-buffer "*acs-fuzzy*"))
        (repl (get-buffer "*slime-repl sbcl*"))
        (observed nil))
    (push (list :modes-known (list (and (memq 'lisp-mode ac-modes) t)
                                   (and (memq 'slime-repl-mode ac-modes) t)))
          observed)
    (with-current-buffer lisp
      (push (list :lisp-before ac-sources) observed)
      (set-up-slime-ac)
      (set-up-slime-ac)
      (push (list :lisp-after ac-sources
                  :buffer-local (local-variable-p 'ac-sources))
            observed))
    (with-current-buffer fuzzy
      (set-up-slime-ac t)
      (push (list :fuzzy-buffer ac-sources) observed))
    (with-current-buffer repl
      (set-window-buffer (selected-window) repl)
      (set-up-slime-ac)
      (push (list :repl-mode major-mode :repl-sources ac-sources) observed)
      (goto-char (point-max))
      (insert "(str")
      (acs-test-complete)
      (push (list :repl-prefix ac-prefix
                  :repl-candidates (acs-test-candidates))
            observed)
      (ac-complete)
      (push (list :repl-line (acs-test-line)) observed))
    (with-current-buffer lisp
      (push (list :lisp-unchanged ac-sources) observed))
    (nreverse observed)))"##,
        expect![[
            r#"OK ((:modes-known (t t)) (:lisp-before #1=(ac-source-words-in-same-mode-buffers)) (:lisp-after #2=(ac-source-slime-simple . #1#) :buffer-local t) (:fuzzy-buffer (ac-source-slime-fuzzy . #1#)) (:repl-mode slime-repl-mode :repl-sources (ac-source-slime-simple . #1#)) (:repl-prefix "str" :repl-candidates ("string" "string=" "stringp")) (:repl-line "(string") (:lisp-unchanged #2#))"#
        ]],
    )
}

fn completing_in_a_lisp_buffer_asks_swank_and_inserts_the_chosen_symbol() -> ParityBatchCase {
    ParityBatchCase::value(
        "completing_in_a_lisp_buffer_asks_swank_and_inserts_the_chosen_symbol",
        r##"(progn
  (require 'slime)
  (acs-test-connect)
  (acs-test-lisp-buffer "(defun demo ()\n  (ca")
  (set-up-slime-ac)
  (goto-char (point-max))
  (acs-test-complete)
  (let ((result (list :prefix ac-prefix
                      :prefix-start (slime-symbol-start-pos)
                      :candidates (acs-test-candidates)
                      :annotations (acs-test-summaries)
                      :requests (last (acs-test-swank-requests)))))
    (ac-complete)
    (append result (list :line (acs-test-line)
                         :point (point)
                         :mode major-mode))))"##,
        expect![[
            r#"OK (:prefix "ca" :prefix-start 19 :candidates ("car" "cadr" "case" "catch") :annotations (("car" nil "l") ("cadr" nil "l") ("case" nil "l") ("catch" nil "l")) :requests ("(:emacs-rex (swank:simple-completions \"ca\" '#1=\"COMMON-LISP-USER\") #1# t 4)") :line "  (car" :point 22 :mode lisp-mode)"#
        ]],
    )
    .fresh_process()
}

fn the_fuzzy_source_labels_each_candidate_with_the_flags_swank_returned() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_fuzzy_source_labels_each_candidate_with_the_flags_swank_returned",
        r##"(progn
  (require 'slime)
  (acs-test-connect)
  (acs-test-lisp-buffer "(defun demo ()\n  (ca")
  (set-up-slime-ac t)
  (goto-char (point-max))
  (acs-test-complete)
  (let ((observed (list (list :with-flags ac-slime-show-flags
                              :candidates (acs-test-candidates)
                              :annotations (acs-test-summaries)
                              :request (car (last (acs-test-swank-requests)))))))
    (setq ac-slime-show-flags nil)
    (acs-test-complete)
    (append observed
            (list (list :with-flags ac-slime-show-flags
                        :candidates (acs-test-candidates)
                        :annotations (acs-test-summaries))))))"##,
        expect![[
            r#"OK ((:with-flags t :candidates ("car" "cadr" "case" "catch") :annotations (("car" "-f--e-" "l") ("cadr" "-f--e-" "l") ("case" "-m----" "l") ("catch" "-m----" "l")) :request "(:emacs-rex (swank:fuzzy-completions \"ca\" #1=\"COMMON-LISP-USER\" :limit 50 :time-limit-in-msec 1500) #1# t 4)") (:with-flags nil :candidates ("car" "cadr" "case" "catch") :annotations (("car" nil "l") ("cadr" nil "l") ("case" nil "l") ("catch" nil "l"))))"#
        ]],
    )
    .fresh_process()
}

fn an_uppercase_prefix_is_carried_into_every_candidate_and_inserted() -> ParityBatchCase {
    ParityBatchCase::value(
        "an_uppercase_prefix_is_carried_into_every_candidate_and_inserted",
        r##"(progn
  (require 'slime)
  (acs-test-connect)
  (acs-test-lisp-buffer "(defun demo ()\n  (CA")
  (set-up-slime-ac)
  (goto-char (point-max))
  (acs-test-complete)
  (let ((result (list :prefix ac-prefix
                      :candidates (acs-test-candidates)
                      :request (car (last (acs-test-swank-requests))))))
    (ac-complete)
    (append result (list :line (acs-test-line) :point (point)))))"##,
        expect![[
            r#"OK (:prefix "CA" :candidates ("CAr" "CAdr" "CAse" "CAtch") :request "(:emacs-rex (swank:simple-completions \"CA\" '#1=\"COMMON-LISP-USER\") #1# t 4)" :line "  (CAr" :point 22)"#
        ]],
    )
    .fresh_process()
}

fn each_candidate_documents_itself_from_the_running_lisp() -> ParityBatchCase {
    ParityBatchCase::value(
        "each_candidate_documents_itself_from_the_running_lisp",
        r##"(progn
  (require 'slime)
  (acs-test-connect)
  (acs-test-lisp-buffer "(defun demo ()\n  (ca")
  (set-up-slime-ac)
  (goto-char (point-max))
  (acs-test-complete)
  (list :car (popup-item-documentation (nth 0 ac-candidates))
        :case (popup-item-documentation (nth 2 ac-candidates))
        :catch (popup-item-documentation (nth 3 ac-candidates))
        :requests (last (acs-test-swank-requests) 3)))"##,
        expect![[
            r#"OK (:car "Return the car of LIST.  Signals TYPE-ERROR otherwise." :case "CASE keyform {({key | (key*)} form*)}*" :catch "Not documented." :requests ("(:emacs-rex (swank:documentation-symbol \"car\") \"COMMON-LISP-USER\" t 5)" "(:emacs-rex (swank:documentation-symbol \"case\") \"COMMON-LISP-USER\" t 6)" "(:emacs-rex (swank:documentation-symbol \"catch\") \"COMMON-LISP-USER\" t 7)"))"#
        ]],
    )
    .fresh_process()
}

fn without_a_connection_neither_source_contacts_swank() -> ParityBatchCase {
    ParityBatchCase::value(
        "without_a_connection_neither_source_contacts_swank",
        r##"(progn
  (require 'slime)
  (acs-test-start-swank)
  (acs-test-lisp-buffer "(defun cabinet () nil)\n(caboose)\n(ca")
  (set-up-slime-ac)
  (goto-char (point-max))
  (acs-test-complete)
  (list :connected (slime-connected-p)
        :simple (ac-source-slime-simple-candidates)
        :fuzzy (ac-source-slime-fuzzy-candidates)
        :prefix ac-prefix
        :candidates (acs-test-candidates)
        :sources ac-sources
        :requests (acs-test-swank-requests)
        :line (acs-test-line)))"##,
        expect![[
            r#"OK (:connected nil :simple nil :fuzzy nil :prefix "ca" :candidates ("cabinet" "caboose") :sources (ac-source-slime-simple ac-source-words-in-same-mode-buffers) :requests nil :line "(ca")"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        set_up_slime_ac_installs_the_chosen_source_in_each_buffer_separately(),
        completing_in_a_lisp_buffer_asks_swank_and_inserts_the_chosen_symbol(),
        the_fuzzy_source_labels_each_candidate_with_the_flags_swank_returned(),
        an_uppercase_prefix_is_carried_into_every_candidate_and_inserted(),
        each_candidate_documents_itself_from_the_running_lisp(),
        without_a_connection_neither_source_contacts_swank(),
    ]
}

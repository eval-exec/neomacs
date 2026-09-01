use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, PAREDIT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PAREDIT_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const PAREDIT_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'paredit)

(defun neomacs-paredit-test-balanced-p ()
  "Return non-nil when the accessible buffer has balanced delimiters."
  (condition-case nil
      (progn (check-parens) t)
    (error nil)))

(defun neomacs-paredit-test-state (label)
  "Capture the structural editing checkpoint named LABEL."
  (let ((parse-state (syntax-ppss)))
    (list :label label
          :text (buffer-substring-no-properties (point-min) (point-max))
          :point (point)
          :mark (and mark-active (mark))
          :depth (nth 0 parse-state)
          :string (and (nth 3 parse-state) t)
          :comment (and (nth 4 parse-state) t)
          :balanced (neomacs-paredit-test-balanced-p))))

(defun neomacs-paredit-test-call (command &optional prefix)
  "Invoke COMMAND interactively with PREFIX as a user would."
  (let ((current-prefix-arg prefix))
    (call-interactively command)))

(defun neomacs-paredit-test-insert-marked (text)
  "Insert TEXT, remove its unique | marker, and leave point there."
  (insert text)
  (goto-char (point-min))
  (unless (search-forward "|" nil t)
    (error "Paredit test text lacks a point marker"))
  (delete-char -1))

(defun neomacs-paredit-test-capture-signal (function)
  "Run FUNCTION and return complete stable signal information."
  (condition-case error-data
      (progn (funcall function) 'no-signal)
    (error
     (list :symbol (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))
"###;

fn mode_lifecycle_and_balanced_insertion_support_a_real_definition() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (emacs-lisp-mode)
  (paredit-mode 1)
  (let ((bindings
         (mapcar
          (lambda (key)
            (list key (key-binding (kbd key))))
          '("(" ")" "\"" "DEL" "<delete>" "C-k"))))
    (neomacs-paredit-test-call 'paredit-open-round)
    (insert "defun release-label ")
    (neomacs-paredit-test-call 'paredit-open-round)
    (insert "version")
    (neomacs-paredit-test-call 'paredit-close-round)
    (insert " ")
    (neomacs-paredit-test-call 'paredit-doublequote)
    (insert "release-")
    (insert "version")
    (neomacs-paredit-test-call 'paredit-doublequote)
    (neomacs-paredit-test-call 'paredit-close-round)
    (let ((enabled (neomacs-paredit-test-state 'enabled)))
      (paredit-mode -1)
      (list :bindings bindings
            :enabled enabled
            :mode-after-disable paredit-mode
            :open-after-disable (key-binding (kbd "("))))))
"###;
    let expected = expect![[
        r#"OK (:bindings (("(" paredit-open-round) (")" paredit-close-round) ("\"" paredit-doublequote) ("DEL" paredit-backward-delete) ("<delete>" paredit-forward-delete) ("C-k" paredit-kill)) :enabled (:label enabled :text "(defun release-label (version) \"release-version\")" :point 50 :mark nil :depth 0 :string nil :comment nil :balanced t) :mode-after-disable nil :open-after-disable self-insert-command)"#
    ]];
    ParityBatchCase::value(
        "mode_lifecycle_and_balanced_insertion_support_a_real_definition",
        elisp_form,
        expected,
    )
}

fn prefix_slurp_and_barf_refactor_a_multistage_deployment_pipeline() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (emacs-lisp-mode)
  (setq indent-tabs-mode nil)
  (neomacs-paredit-test-insert-marked
   "(deploy\n  (build api|)\n  (verify unit integration)\n  (publish registry))")
  (paredit-mode 1)
  (let ((original (neomacs-paredit-test-state 'original)))
    (neomacs-paredit-test-call 'paredit-forward-slurp-sexp 2)
    (let ((slurped (neomacs-paredit-test-state 'slurped-two-stages)))
      (neomacs-paredit-test-call 'paredit-forward-barf-sexp 1)
      (let ((barfed-once (neomacs-paredit-test-state 'barfed-publish)))
        (neomacs-paredit-test-call 'paredit-forward-barf-sexp 1)
        (list :original original
              :slurped slurped
              :barfed-once barfed-once
              :round-trip (neomacs-paredit-test-state 'round-trip))))))
"###;
    let expected = expect![[
        r#"OK (:original (:label original :text "(deploy\n  (build api)\n  (verify unit integration)\n  (publish registry))" :point 21 :mark nil :depth 2 :string nil :comment nil :balanced t) :slurped (:label slurped-two-stages :text "(deploy\n (build api\n        (verify unit integration)\n        (publish registry)))" :point 20 :mark nil :depth 2 :string nil :comment nil :balanced t) :barfed-once (:label barfed-publish :text "(deploy\n (build api\n        (verify unit integration))\n (publish registry))" :point 20 :mark nil :depth 2 :string nil :comment nil :balanced t) :round-trip (:label round-trip :text "(deploy\n (build api)\n (verify unit integration)\n (publish registry))" :point 20 :mark nil :depth 2 :string nil :comment nil :balanced t))"#
    ]];
    ParityBatchCase::value(
        "prefix_slurp_and_barf_refactor_a_multistage_deployment_pipeline",
        elisp_form,
        expected,
    )
}

fn backward_slurp_and_barf_move_prerequisites_into_and_out_of_release() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (emacs-lisp-mode)
  (setq indent-tabs-mode nil)
  (neomacs-paredit-test-insert-marked
   "(setq deployment-manifest\n      '((:fetch source)\n        (:lint strict)\n        (:release production|)))")
  (paredit-mode 1)
  (neomacs-paredit-test-call 'paredit-backward-slurp-sexp 2)
  (let ((slurped (neomacs-paredit-test-state 'prerequisites-inside-release)))
    (neomacs-paredit-test-call 'paredit-backward-barf-sexp 1)
    (let ((barfed-once (neomacs-paredit-test-state 'fetch-outside-release)))
      (neomacs-paredit-test-call 'paredit-backward-barf-sexp 1)
      (list :slurped slurped
            :barfed-once barfed-once
            :round-trip (neomacs-paredit-test-state 'prerequisites-restored)))))
"###;
    let expected = expect![[
        r#"OK (:slurped (:label prerequisites-inside-release :text "(setq deployment-manifest\n      '(((:fetch source)\n         (:lint strict)\n         :release production)))" :point 104 :mark nil :depth 3 :string nil :comment nil :balanced t) :barfed-once (:label fetch-outside-release :text "(setq deployment-manifest\n      '((:fetch source)\n        ((:lint strict)\n         :release production)))" :point 103 :mark nil :depth 3 :string nil :comment nil :balanced t) :round-trip (:label prerequisites-restored :text "(setq deployment-manifest\n      '((:fetch source)\n        (:lint strict)\n        (:release production)))" :point 102 :mark nil :depth 3 :string nil :comment nil :balanced t))"#
    ]];
    ParityBatchCase::value(
        "backward_slurp_and_barf_move_prerequisites_into_and_out_of_release",
        elisp_form,
        expected,
    )
}

fn splice_raise_and_convolute_restructure_nested_release_logic() -> ParityBatchCase {
    let elisp_form = r###"
(list
 :splice
 (with-temp-buffer
   (emacs-lisp-mode)
   (setq indent-tabs-mode nil)
   (let ((kill-ring nil)
         (kill-ring-yank-pointer nil))
     (neomacs-paredit-test-insert-marked
      "(let ((artifact \"v2\"))\n  (progn|\n    (verify artifact)\n    (publish artifact)))")
     (paredit-mode 1)
     (neomacs-paredit-test-call 'paredit-splice-sexp-killing-backward)
     (list :state (neomacs-paredit-test-state 'removed-progn-wrapper)
           :kill-ring kill-ring)))
 :raise
 (with-temp-buffer
   (emacs-lisp-mode)
   (setq indent-tabs-mode nil)
   (neomacs-paredit-test-insert-marked
    "(when ready\n  (progn\n    (log \"start\")\n    |(publish artifact)\n    (notify team)))")
   (paredit-mode 1)
   (neomacs-paredit-test-call 'paredit-raise-sexp)
   (neomacs-paredit-test-state 'raised-publish-step))
 :convolute
 (with-temp-buffer
   (emacs-lisp-mode)
   (setq indent-tabs-mode nil)
   (neomacs-paredit-test-insert-marked
    "(let ((artifact \"v2\")) (when signed| (publish artifact)))")
   (paredit-mode 1)
   (neomacs-paredit-test-call 'paredit-convolute-sexp)
   (neomacs-paredit-test-state 'convoluted-guard)))
"###;
    let expected = expect![[
        r#"OK (:splice (:state (:label removed-progn-wrapper :text "(let ((artifact \"v2\"))\n  (verify artifact)\n  (publish artifact))" :point 26 :mark nil :depth 1 :string nil :comment nil :balanced t) :kill-ring ("progn")) :raise (:label raised-publish-step :text "(when ready\n  (publish artifact))" :point 15 :mark nil :depth 1 :string nil :comment nil :balanced t) :convolute (:label convoluted-guard :text "(when signed (let ((artifact \"v2\")) (publish artifact)))" :point 14 :mark nil :depth 1 :string nil :comment nil :balanced t))"#
    ]];
    ParityBatchCase::value(
        "splice_raise_and_convolute_restructure_nested_release_logic",
        elisp_form,
        expected,
    )
}

fn split_join_round_trips_lists_and_strings_and_rejects_mismatched_delimiters() -> ParityBatchCase {
    let elisp_form = r###"
(list
 :list
 (with-temp-buffer
   (emacs-lisp-mode)
   (neomacs-paredit-test-insert-marked "(pipeline (build api| worker))")
   (paredit-mode 1)
   (neomacs-paredit-test-call 'paredit-split-sexp)
   (let ((split (neomacs-paredit-test-state 'split-build-groups)))
     (neomacs-paredit-test-call 'paredit-join-sexps)
     (list :split split :joined (neomacs-paredit-test-state 'joined-build-groups))))
 :string
 (with-temp-buffer
   (emacs-lisp-mode)
   (neomacs-paredit-test-insert-marked "(label \"release|candidate\")")
   (paredit-mode 1)
   (neomacs-paredit-test-call 'paredit-split-sexp)
   (let ((split (neomacs-paredit-test-state 'split-label)))
     (neomacs-paredit-test-call 'paredit-join-sexps)
     (list :split split :joined (neomacs-paredit-test-state 'joined-label))))
 :mismatch
 (with-temp-buffer
   (emacs-lisp-mode)
   (neomacs-paredit-test-insert-marked
    "(deploy (build api)| [verify checksum])")
   (paredit-mode 1)
   (let ((before (neomacs-paredit-test-state 'before-mismatched-join))
         (signal
          (neomacs-paredit-test-capture-signal
           (lambda () (neomacs-paredit-test-call 'paredit-join-sexps)))))
     (list :before before
           :signal signal
           :after (neomacs-paredit-test-state 'after-mismatched-join)))))
"###;
    let expected = expect![[
        r#"OK (:list (:split (:label split-build-groups :text "(pipeline (build api) (worker))" :point 22 :mark nil :depth 1 :string nil :comment nil :balanced t) :joined (:label joined-build-groups :text "(pipeline (build api worker))" :point 21 :mark nil :depth 2 :string nil :comment nil :balanced t)) :string (:split (:label split-label :text "(label \"release\" \"candidate\")" :point 17 :mark nil :depth 1 :string nil :comment nil :balanced t) :joined (:label joined-label :text "(label \"releasecandidate\")" :point 16 :mark nil :depth 1 :string t :comment nil :balanced t)) :mismatch (:before (:label before-mismatched-join :text "(deploy (build api) [verify checksum])" :point 20 :mark nil :depth 1 :string nil :comment nil :balanced t) :signal (:symbol error :data ("Mismatched S-expressions to join.") :message "Mismatched S-expressions to join.") :after (:label after-mismatched-join :text "(deploy (build api) [verify checksum])" :point 20 :mark nil :depth 1 :string nil :comment nil :balanced t)))"#
    ]];
    ParityBatchCase::value(
        "split_join_round_trips_lists_and_strings_and_rejects_mismatched_delimiters",
        elisp_form,
        expected,
    )
}

fn active_regions_wrap_complete_steps_and_refuse_an_unbalanced_kill() -> ParityBatchCase {
    let elisp_form = r###"
(let ((transient-mark-mode t))
  (list
   :wrap
   (with-temp-buffer
     (emacs-lisp-mode)
     (insert "(deploy |(build api) (verify checksum)_ (publish registry))")
     (goto-char (point-min))
     (search-forward "|")
     (delete-char -1)
     (let ((start (point)))
       (search-forward "_")
       (delete-char -1)
       (set-mark (point))
       (goto-char start)
       (setq mark-active t)
       (paredit-mode 1)
       (neomacs-paredit-test-call 'paredit-wrap-round)
       (neomacs-paredit-test-state 'wrapped-build-and-verify)))
   :unsafe-kill
   (with-temp-buffer
     (emacs-lisp-mode)
     (let ((kill-ring nil)
           (kill-ring-yank-pointer nil))
       (insert "(deploy buil|d (verify checksum_) (publish registry))")
       (goto-char (point-min))
       (search-forward "|")
       (delete-char -1)
       (let ((start (point)))
         (search-forward "_")
         (delete-char -1)
         (set-mark (point))
         (goto-char start)
         (setq mark-active t)
         (paredit-mode 1)
         (let ((before (buffer-string))
               (signal
                (neomacs-paredit-test-capture-signal
                 (lambda ()
                   (neomacs-paredit-test-call 'paredit-kill-region)))))
           (list :before before
                 :signal signal
                 :after (buffer-string)
                 :point (point)
                 :mark (mark)
                 :kill-ring kill-ring
                 :balanced (neomacs-paredit-test-balanced-p))))))))
"###;
    let expected = expect![[
        r#"OK (:wrap (:label wrapped-build-and-verify :text "(deploy ((build api) (verify checksum)) (publish registry))" :point 10 :mark 39 :depth 2 :string nil :comment nil :balanced t) :unsafe-kill (:before "(deploy build (verify checksum) (publish registry))" :signal (:symbol error :data ("Mismatched parenthesis depth: 1 at start, 2 at end.") :message "Mismatched parenthesis depth: 1 at start, 2 at end.") :after "(deploy build (verify checksum) (publish registry))" :point 13 :mark 31 :kill-ring nil :balanced t))"#
    ]];
    ParityBatchCase::value(
        "active_regions_wrap_complete_steps_and_refuse_an_unbalanced_kill",
        elisp_form,
        expected,
    )
}

fn strings_and_comments_preserve_structure_during_real_text_editing() -> ParityBatchCase {
    let elisp_form = r###"
(list
 :escaped-quote
 (with-temp-buffer
   (emacs-lisp-mode)
   (neomacs-paredit-test-insert-marked
    "(message \"release |candidate\" artifact)")
   (paredit-mode 1)
   (neomacs-paredit-test-call 'paredit-doublequote)
   (neomacs-paredit-test-state 'escaped-quote))
 :string-continuation
 (with-temp-buffer
   (emacs-lisp-mode)
   (setq indent-tabs-mode nil)
   (neomacs-paredit-test-insert-marked
    "(message \"release |candidate\" artifact)")
   (paredit-mode 1)
   (neomacs-paredit-test-call 'paredit-meta-doublequote-and-newline)
   (neomacs-paredit-test-state 'continued-after-message))
 :comment
 (with-temp-buffer
   (emacs-lisp-mode)
   (setq indent-tabs-mode nil)
   (neomacs-paredit-test-insert-marked
    "(defun deploy ()\n  (build artifact) |(verify\n                     checksum)\n  (publish artifact))")
   (paredit-mode 1)
   (neomacs-paredit-test-call 'paredit-semicolon 2)
   (neomacs-paredit-test-state 'inserted-safe-inline-comment)))
"###;
    let expected = expect![[
        r#"OK (:escaped-quote (:label escaped-quote :text "(message \"release \\\"candidate\" artifact)" :point 21 :mark nil :depth 1 :string t :comment nil :balanced t) :string-continuation (:label continued-after-message :text "(message \"release candidate\"\n         artifact)" :point 39 :mark nil :depth 1 :string nil :comment nil :balanced t) :comment (:label inserted-safe-inline-comment :text "(defun deploy ()\n  (build artifact) ;;\n  (verify\n   checksum)\n  (publish artifact))" :point 39 :mark nil :depth 1 :string nil :comment t :balanced t))"#
    ]];
    ParityBatchCase::value(
        "strings_and_comments_preserve_structure_during_real_text_editing",
        elisp_form,
        expected,
    )
}

fn kill_commands_stop_at_list_string_and_comment_boundaries() -> ParityBatchCase {
    let elisp_form = r###"
(list
 :code
 (with-temp-buffer
   (emacs-lisp-mode)
   (let ((kill-ring nil)
         (kill-ring-yank-pointer nil))
     (neomacs-paredit-test-insert-marked
      "(deploy\n  |(build artifact) (verify checksum)\n  (publish artifact))")
     (paredit-mode 1)
     (neomacs-paredit-test-call 'paredit-kill)
     (list :state (neomacs-paredit-test-state 'killed-build-line)
           :kill-ring kill-ring)))
 :string
 (with-temp-buffer
   (emacs-lisp-mode)
   (let ((kill-ring nil)
         (kill-ring-yank-pointer nil))
     (neomacs-paredit-test-insert-marked
      "(message \"release-|candidate\" artifact)")
     (paredit-mode 1)
     (neomacs-paredit-test-call 'paredit-kill)
     (list :state (neomacs-paredit-test-state 'killed-string-tail)
           :kill-ring kill-ring)))
 :comment
 (with-temp-buffer
   (emacs-lisp-mode)
   (let ((kill-ring nil)
         (kill-ring-yank-pointer nil))
     (neomacs-paredit-test-insert-marked
      "(verify checksum) ;; release |candidate\n(publish artifact)")
     (paredit-mode 1)
     (neomacs-paredit-test-call 'paredit-kill)
     (list :state (neomacs-paredit-test-state 'killed-comment-tail)
           :kill-ring kill-ring))))
"###;
    let expected = expect![[
        r#"OK (:code (:state (:label killed-build-line :text "(deploy\n  \n  (publish artifact))" :point 11 :mark nil :depth 1 :string nil :comment nil :balanced t) :kill-ring ("(build artifact) (verify checksum)")) :string (:state (:label killed-string-tail :text "(message \"release-\" artifact)" :point 19 :mark nil :depth 1 :string t :comment nil :balanced t) :kill-ring ("candidate")) :comment (:state (:label killed-comment-tail :text "(verify checksum) ;; release \n(publish artifact)" :point 30 :mark nil :depth 0 :string nil :comment t :balanced t) :kill-ring ("candidate")))"#
    ]];
    ParityBatchCase::value(
        "kill_commands_stop_at_list_string_and_comment_boundaries",
        elisp_form,
        expected,
    )
}

fn mode_rejects_unbalanced_code_but_allows_explicit_recovery_mode() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(deploy (build artifact)")
  (let ((ordinary-signal
         (neomacs-paredit-test-capture-signal
          (lambda () (paredit-mode 1)))))
    (let ((ordinary-mode paredit-mode)
          (ordinary-text (buffer-string))
          (current-prefix-arg '(4)))
      (paredit-mode 1)
      (list :ordinary-signal ordinary-signal
            :ordinary-mode ordinary-mode
            :ordinary-text ordinary-text
            :forced-mode paredit-mode
            :forced-balance (neomacs-paredit-test-balanced-p)
            :forced-open-binding (key-binding (kbd "("))))))
"###;
    let expected = expect![[
        r#"OK (:ordinary-signal (:symbol user-error :data ("Unmatched bracket or quote") :message "Unmatched bracket or quote") :ordinary-mode nil :ordinary-text "(deploy (build artifact)" :forced-mode t :forced-balance nil :forced-open-binding paredit-open-round)"#
    ]];
    ParityBatchCase::value(
        "mode_rejects_unbalanced_code_but_allows_explicit_recovery_mode",
        elisp_form,
        expected,
    )
}

#[test]
fn paredit_package_batch() {
    let cases = vec![
        mode_lifecycle_and_balanced_insertion_support_a_real_definition(),
        prefix_slurp_and_barf_refactor_a_multistage_deployment_pipeline(),
        backward_slurp_and_barf_move_prerequisites_into_and_out_of_release(),
        splice_raise_and_convolute_restructure_nested_release_logic(),
        split_join_round_trips_lists_and_strings_and_rejects_mismatched_delimiters(),
        active_regions_wrap_complete_steps_and_refuse_an_unbalanced_kill(),
        strings_and_comments_preserve_structure_during_real_text_editing(),
        kill_commands_stop_at_list_string_and_comment_boundaries(),
        mode_rejects_unbalanced_code_but_allows_explicit_recovery_mode(),
    ];
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(PAREDIT_MELPA_PIN, "paredit.el")
            .expect("prepare revision-pinned Paredit source below ./tmp")
            .with_prelude(PAREDIT_TEST_PRELUDE)
            .with_timeout(PAREDIT_TEST_TIMEOUT),
        "paredit-package-batch",
        "Paredit",
        &cases,
    );
}

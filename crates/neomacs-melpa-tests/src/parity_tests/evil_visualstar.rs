use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EVIL_MELPA_PIN, EVIL_VISUALSTAR_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'evil-visualstar)

(defun neomacs-evil-visualstar-test-in-buffer (text function)
  (with-temp-buffer
    (insert text)
    (goto-char (point-min))
    (evil-local-mode 1)
    (unwind-protect
        (let ((evil-ex-search-interactive nil)
              (evil-ex-search-highlight-all nil)
              (evil-ex-search-pattern nil)
              (evil-ex-search-direction 'forward)
              (evil-search-wrap nil)
              (evil-search-wrap-ring-bell nil)
              (evil-ex-search-history nil)
              (evil-search-forward-history nil)
              (evil-search-backward-history nil)
              (isearch-string "")
              (regexp-search-ring nil)
              (search-ring nil))
          (evil-normal-state)
          (funcall function))
      (when (evil-visual-state-p) (evil-exit-visual-state))
      (evil-local-mode -1))))

(defun neomacs-evil-visualstar-test-bounds (text occurrence)
  (goto-char (point-min))
  (dotimes (_ occurrence) (search-forward text))
  (cons (match-beginning 0) (match-end 0)))

(defun neomacs-evil-visualstar-test-activate (bounds)
  (goto-char (cdr bounds))
  (set-mark (car bounds))
  (setq transient-mark-mode t
        mark-active t)
  (evil-visual-state))

(defun neomacs-evil-visualstar-test-state (selection-length)
  (list :point (point)
        :mark (mark t)
        :mark-active mark-active
        :visual (evil-visual-state-p)
        :match (and (<= (+ (point) selection-length) (point-max))
                    (buffer-substring-no-properties
                     (point) (+ (point) selection-length)))
        :ex-pattern (and evil-ex-search-pattern
                         (evil-ex-pattern-regex evil-ex-search-pattern))
        :ex-direction evil-ex-search-direction
        :isearch-string (and (boundp 'isearch-string) isearch-string)
        :ex-history (copy-tree evil-ex-search-history)
        :forward-history (copy-tree evil-search-forward-history)
        :backward-history (copy-tree evil-search-backward-history)))

(defun neomacs-evil-visualstar-test-capture (function)
  (condition-case error-data
      (list :ok (funcall function))
    (error
     (list :error (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))
"####;

fn ex_search_treats_selected_regexp_characters_literally_in_both_directions() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-visualstar-test-in-buffer
 "artifact[1].* ready | artifact[1].* shipped | artifact[1].* archived"
 (lambda ()
   (let* ((evil-search-module 'evil-search)
          (needle "artifact[1].*")
          (length (length needle))
          (first (neomacs-evil-visualstar-test-bounds needle 1)))
     (neomacs-evil-visualstar-test-activate first)
     (evil-visualstar/begin-search-forward (car first) (cdr first))
     (let ((forward (neomacs-evil-visualstar-test-state length))
           (third (neomacs-evil-visualstar-test-bounds needle 3)))
       (neomacs-evil-visualstar-test-activate third)
       (evil-visualstar/begin-search-backward (car third) (cdr third))
       (list :forward forward
             :backward (neomacs-evil-visualstar-test-state length))))))
"####;
    let expected = expect![[
        r#"OK (:forward (:point 23 :mark 1 :mark-active nil :visual nil :match "artifact[1].*" :ex-pattern "artifact\\[1]\\.\\*" :ex-direction forward :isearch-string "" :ex-history ("artifact\\[1]\\.\\*") :forward-history ("artifact\\[1]\\.\\*") :backward-history nil) :backward (:point 47 :mark 47 :mark-active nil :visual nil :match "artifact[1].*" :ex-pattern "artifact\\[1]\\.\\*" :ex-direction backward :isearch-string "" :ex-history ("artifact\\[1]\\.\\*") :forward-history ("artifact\\[1]\\.\\*") :backward-history ("artifact\\[1]\\.\\*")))"#
    ]];
    ParityBatchCase::value(
        "ex_search_treats_selected_regexp_characters_literally_in_both_directions",
        elisp_form,
        expected,
    )
}

fn isearch_backend_finds_the_next_exact_release_name_and_records_search_feedback() -> ParityBatchCase
{
    let elisp_form = r####"
(neomacs-evil-visualstar-test-in-buffer
 "Release Train draft | release train lowercase | Release Train approved"
 (lambda ()
   (let* ((evil-search-module 'isearch)
          (needle "Release Train")
          (bounds (neomacs-evil-visualstar-test-bounds needle 1))
          flashes)
     (cl-letf (((symbol-function 'evil-flash-search-pattern)
                (lambda (message &optional no-error)
                  (push (list message no-error) flashes)
                  t)))
       (neomacs-evil-visualstar-test-activate bounds)
       (evil-visualstar/begin-search-forward (car bounds) (cdr bounds))
       (list :state (neomacs-evil-visualstar-test-state (length needle))
             :flashes (nreverse flashes))))))
"####;
    let expected = expect![[
        r#"OK (:state (:point 49 :mark 1 :mark-active nil :visual nil :match "Release Train" :ex-pattern nil :ex-direction forward :isearch-string "Release Train" :ex-history nil :forward-history nil :backward-history nil) :flashes (("/Release Train" t)))"#
    ]];
    ParityBatchCase::value(
        "isearch_backend_finds_the_next_exact_release_name_and_records_search_feedback",
        elisp_form,
        expected,
    )
}

fn multiline_selection_searches_an_exact_deployment_block_instead_of_individual_lines()
-> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-visualstar-test-in-buffer
 "service=api\nchannel=stable\n---\nservice=web\nchannel=canary\n---\nservice=api\nchannel=stable\n"
 (lambda ()
   (let* ((evil-search-module 'evil-search)
          (needle "service=api\nchannel=stable")
          (bounds (neomacs-evil-visualstar-test-bounds needle 1)))
     (neomacs-evil-visualstar-test-activate bounds)
     (evil-visualstar/begin-search-forward (car bounds) (cdr bounds))
     (neomacs-evil-visualstar-test-state (length needle)))))
"####;
    let expected = expect![[
        r#"OK (:point 63 :mark 1 :mark-active nil :visual nil :match "service=api\nchannel=stable" :ex-pattern "service=api\nchannel=stable" :ex-direction forward :isearch-string "" :ex-history ("service=api\nchannel=stable") :forward-history ("service=api\nchannel=stable") :backward-history nil)"#
    ]];
    ParityBatchCase::value(
        "multiline_selection_searches_an_exact_deployment_block_instead_of_individual_lines",
        elisp_form,
        expected,
    )
}

fn persistent_mode_reselects_isearch_matches_and_exposes_the_current_ex_backend_contract()
-> ParityBatchCase {
    let elisp_form = r####"
(mapcar
 (lambda (backend)
   (neomacs-evil-visualstar-test-in-buffer
    "build verify deploy | build verify deploy | build verify deploy"
    (lambda ()
      (let* ((evil-search-module backend)
             (evil-visualstar/persistent t)
             (needle "build verify deploy")
             (bounds (neomacs-evil-visualstar-test-bounds needle 1)))
        (cl-letf (((symbol-function 'evil-flash-search-pattern)
                   (lambda (&rest _) t)))
          (neomacs-evil-visualstar-test-activate bounds)
          (evil-visualstar/begin-search-forward (car bounds) (cdr bounds))
          (list :backend backend
                :state (neomacs-evil-visualstar-test-state (length needle))
                :region (and (region-active-p)
                             (buffer-substring-no-properties
                              (region-beginning) (region-end)))
                :region-bounds (and (mark t)
                                    (list (region-beginning)
                                          (region-end)))))))))
 '(isearch evil-search))
"####;
    let expected = expect![[
        r#"OK ((:backend isearch :state (:point 23 :mark 42 :mark-active t :visual nil :match "build verify deploy" :ex-pattern nil :ex-direction forward :isearch-string "build verify deploy" :ex-history nil :forward-history nil :backward-history nil) :region "build verify deploy" :region-bounds (23 42)) (:backend evil-search :state (:point 23 :mark 1 :mark-active nil :visual nil :match "build verify deploy" :ex-pattern "build verify deploy" :ex-direction forward :isearch-string "" :ex-history ("build verify deploy") :forward-history ("build verify deploy") :backward-history nil) :region nil :region-bounds (1 23)))"#
    ]];
    ParityBatchCase::value(
        "persistent_mode_reselects_isearch_matches_and_exposes_the_current_ex_backend_contract",
        elisp_form,
        expected,
    )
}

fn repeated_searches_deduplicate_history_and_missing_matches_preserve_the_start() -> ParityBatchCase
{
    let elisp_form = r####"
(list
 (neomacs-evil-visualstar-test-in-buffer
  "alpha target omega target final target"
  (lambda ()
    (let* ((evil-search-module 'evil-search)
           (needle "target")
           (first (neomacs-evil-visualstar-test-bounds needle 1)))
      (neomacs-evil-visualstar-test-activate first)
      (evil-visualstar/begin-search-forward (car first) (cdr first))
      (let ((second (neomacs-evil-visualstar-test-bounds needle 2)))
        (neomacs-evil-visualstar-test-activate second)
        (evil-visualstar/begin-search-forward (car second) (cdr second)))
      (neomacs-evil-visualstar-test-state (length needle)))))
 (neomacs-evil-visualstar-test-in-buffer
  "only-once"
  (lambda ()
    (let* ((evil-search-module 'evil-search)
           (bounds (neomacs-evil-visualstar-test-bounds "only-once" 1))
           (start (point)))
      (list
       :outside-visual
       (progn
         (evil-normal-state)
         (evil-visualstar/begin-search (car bounds) (cdr bounds) t)
         (list (point) evil-ex-search-pattern))
       :missing
       (progn
         (neomacs-evil-visualstar-test-activate bounds)
         (let ((result
                (neomacs-evil-visualstar-test-capture
                 (lambda ()
                   (evil-visualstar/begin-search-forward
                    (car bounds) (cdr bounds))))))
           (list :result result :start start :point (point)
                 :visual (evil-visual-state-p)))))))))
"####;
    let expected = expect![[
        r#"OK ((:point 33 :mark 20 :mark-active nil :visual nil :match "target" :ex-pattern "target" :ex-direction forward :isearch-string "" :ex-history ("target") :forward-history ("target") :backward-history nil) (:outside-visual (10 nil) :missing (:result (:error search-failed :data ("only-once") :message "Search failed: \"only-once\"") :start 10 :point 10 :visual nil)))"#
    ]];
    ParityBatchCase::value(
        "repeated_searches_deduplicate_history_and_missing_matches_preserve_the_start",
        elisp_form,
        expected,
    )
}

fn minor_mode_lifecycle_exposes_visual_star_and_hash_bindings_only_while_enabled() -> ParityBatchCase
{
    let elisp_form = r####"
(with-temp-buffer
  (evil-local-mode 1)
  (unwind-protect
      (let* ((visual-map
              (evil-get-auxiliary-keymap evil-visualstar-mode-map 'visual))
             (bindings
              (list (evil-lookup-key visual-map (kbd "*"))
                    (evil-lookup-key visual-map (kbd "#")))))
        (turn-on-evil-visualstar-mode)
        (let ((enabled (list evil-visualstar-mode bindings)))
          (turn-off-evil-visualstar-mode)
          (list :enabled enabled
                :disabled evil-visualstar-mode
                :global-command (commandp 'global-evil-visualstar-mode)
                :turn-on-command (commandp 'turn-on-evil-visualstar-mode)
                :turn-off-command (commandp 'turn-off-evil-visualstar-mode))))
    (evil-visualstar-mode -1)
    (evil-local-mode -1)))
"####;
    let expected = expect![
        "OK (:enabled (t (evil-visualstar/begin-search-forward evil-visualstar/begin-search-backward)) :disabled nil :global-command t :turn-on-command t :turn-off-command t)"
    ];
    ParityBatchCase::value(
        "minor_mode_lifecycle_exposes_visual_star_and_hash_bindings_only_while_enabled",
        elisp_form,
        expected,
    )
}

#[test]
fn evil_visualstar_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(EVIL_VISUALSTAR_MELPA_PIN, "evil-visualstar.el")
            .expect("prepare revision-pinned Evil Visualstar source below ./tmp")
            .with_melpa_dependency(EVIL_MELPA_PIN)
            .expect("prepare revision-pinned Evil dependency below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "evil-visualstar-package-batch",
        "Evil Visualstar",
        &[
            ex_search_treats_selected_regexp_characters_literally_in_both_directions(),
            isearch_backend_finds_the_next_exact_release_name_and_records_search_feedback(),
            multiline_selection_searches_an_exact_deployment_block_instead_of_individual_lines(),
            persistent_mode_reselects_isearch_matches_and_exposes_the_current_ex_backend_contract(),
            repeated_searches_deduplicate_history_and_missing_matches_preserve_the_start(),
            minor_mode_lifecycle_exposes_visual_star_and_hash_bindings_only_while_enabled(),
        ],
    );
}

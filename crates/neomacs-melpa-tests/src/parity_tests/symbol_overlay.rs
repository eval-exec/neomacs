use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, SYMBOL_OVERLAY_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'symbol-overlay)

(defun neomacs-so-test-overlay-record (overlay)
  "Return the stable, user-visible state of OVERLAY."
  (list :range (list (overlay-start overlay) (overlay-end overlay))
        :text (buffer-substring-no-properties
               (overlay-start overlay) (overlay-end overlay))
        :symbol (overlay-get overlay 'symbol)
        :face (overlay-get overlay 'face)
        :keymap (and (eq (overlay-get overlay 'keymap)
                          symbol-overlay-map)
                     'symbol-overlay-map)
        :evaporate (overlay-get overlay 'evaporate)
        :priority (overlay-get overlay 'priority)
        :created (overlay-get overlay 'neomacs-so-test-created)))

(defun neomacs-so-test-overlays ()
  "Return Symbol Overlay overlays in deterministic buffer order."
  (mapcar
   #'neomacs-so-test-overlay-record
   (sort (copy-sequence (symbol-overlay-get-list 0))
         (lambda (left right)
           (let ((left-start (overlay-start left))
                 (right-start (overlay-start right)))
             (if (= left-start right-start)
                 (< (overlay-end left) (overlay-end right))
               (< left-start right-start)))))))

(defun neomacs-so-test-keywords ()
  "Return the highlighted-symbol registry without improper lists."
  (mapcar (lambda (keyword)
            (list (car keyword) (cadr keyword) (cddr keyword)))
          symbol-overlay-keywords-alist))

(defun neomacs-so-test-point-record ()
  "Return point, mark, and the symbol at point."
  (list :point (point)
        :mark (mark t)
        :symbol (thing-at-point 'symbol t)))

(defun neomacs-so-test-capture-error (function)
  "Return FUNCTION's value or exact error data."
  (condition-case error-data
      (list :ok (funcall function))
    (error
     (list :error (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))

(defvar neomacs-so-test-jump-count 0)
(defun neomacs-so-test-record-jump ()
  "Record one completed Symbol Overlay jump."
  (setq neomacs-so-test-jump-count (1+ neomacs-so-test-jump-count)))
"###;

fn package_registration_exposes_commands_options_keymaps_and_hook_lifecycle() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((descriptor (cadr (assq 'symbol-overlay package-alist)))
       (surface
        '(symbol-overlay-put symbol-overlay-count symbol-overlay-remove-all
          symbol-overlay-save-symbol symbol-overlay-toggle-in-scope
          symbol-overlay-jump-next symbol-overlay-jump-prev
          symbol-overlay-jump-first symbol-overlay-jump-last
          symbol-overlay-jump-to-definition symbol-overlay-switch-forward
          symbol-overlay-switch-backward symbol-overlay-isearch-literally
          symbol-overlay-query-replace symbol-overlay-rename)))
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :feature (and (featurep 'symbol-overlay) t))
   :surface (mapcar #'fboundp surface)
   :bindings
   (mapcar (lambda (key) (cons key (lookup-key symbol-overlay-map (kbd key))))
           '("i" "p" "n" "<" ">" "w" "t" "e" "d" "s" "q" "r"))
   :defaults
   (list :faces (length symbol-overlay-faces)
         :displayed-window symbol-overlay-displayed-window
         :single symbol-overlay-temp-highlight-single
         :idle symbol-overlay-idle-time
         :priority symbol-overlay-priority
         :inhibit-map-safe
         (get 'symbol-overlay-inhibit-map 'safe-local-variable))
   :global-hooks
   (list (and (memq #'symbol-overlay-refresh after-change-functions) t)
         (and (memq #'symbol-overlay-remove-all before-revert-hook) t)
         (and (memq #'symbol-overlay-after-revert after-revert-hook) t))
   :mode
   (with-temp-buffer
     (let ((symbol-overlay-idle-time 0))
       (symbol-overlay-mode 1)
       (let ((enabled
              (list symbol-overlay-mode
                    (local-variable-p 'post-command-hook)
                    (and (memq #'symbol-overlay-post-command post-command-hook)
                         t))))
         (symbol-overlay-mode -1)
         (list :enabled enabled
               :disabled
               (list symbol-overlay-mode
                     (and (memq #'symbol-overlay-post-command post-command-hook)
                          t))))))))
"###;
    let expected = expect![[
        r#"OK (:package (:name symbol-overlay :version "20260703.1437" :requirements ((emacs (24 3)) (seq (2 2))) :feature t) :surface (t t t t t t t t t t t t t t t) :bindings (("i" . symbol-overlay-put) ("p" . symbol-overlay-jump-prev) ("n" . symbol-overlay-jump-next) ("<" . symbol-overlay-jump-first) (">" . symbol-overlay-jump-last) ("w" . symbol-overlay-save-symbol) ("t" . symbol-overlay-toggle-in-scope) ("e" . symbol-overlay-echo-mark) ("d" . symbol-overlay-jump-to-definition) ("s" . symbol-overlay-isearch-literally) ("q" . symbol-overlay-query-replace) ("r" . symbol-overlay-rename)) :defaults (:faces 8 :displayed-window t :single nil :idle 0.5 :priority nil :inhibit-map-safe booleanp) :global-hooks (t t t) :mode (:enabled (t t t) :disabled (nil nil)))"#
    ]];
    ParityBatchCase::value(
        "package_registration_exposes_commands_options_keymaps_and_hook_lifecycle",
        elisp_form,
        expected,
    )
}

fn identifier_highlighting_respects_lisp_boundaries_case_and_overlay_properties() -> ParityBatchCase
{
    let elisp_form = r###"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun deploy (target)\n"
          "  (deploy target)\n"
          "  (list deploy-prod deployer Deploy 'deploy))\n")
  (goto-char (point-min))
  (search-forward "deploy")
  (backward-char 2)
  (let ((symbol-overlay-faces '(release-face fallback-face))
        (symbol-overlay-priority 37)
        (symbol-overlay-overlay-created-functions
         (list (lambda (overlay)
                 (overlay-put overlay 'neomacs-so-test-created 'audited)))))
    (symbol-overlay-put)
    (prog1
        (list :symbol (symbol-overlay-get-symbol)
              :regexp (symbol-overlay-regexp "deploy")
              :keywords (neomacs-so-test-keywords)
              :overlays (neomacs-so-test-overlays)
              :excluded
              (let ((case-fold-search nil))
                (mapcar
                 (lambda (name)
                   (goto-char (point-min))
                   (search-forward name)
                   (list name (symbol-overlay-get-symbol)
                         (length (symbol-overlay-get-list
                                  0 (symbol-overlay-get-symbol)))))
                 '("deploy-prod" "deployer" "Deploy"))))
      (symbol-overlay-remove-all))))
"###;
    let expected = expect![[
        r#"OK (:symbol "deploy" :regexp "\\_<deploy\\_>" :keywords (("deploy" nil release-face)) :overlays ((:range (8 14) :text "deploy" :symbol "deploy" :face release-face :keymap symbol-overlay-map :evaporate t :priority 37 :created audited) (:range (27 33) :text "deploy" :symbol "deploy" :face release-face :keymap symbol-overlay-map :evaporate t :priority 37 :created audited) (:range (79 85) :text "deploy" :symbol "deploy" :face release-face :keymap symbol-overlay-map :evaporate t :priority 37 :created audited)) :excluded (("deploy-prod" "deploy-prod" 0) ("deployer" "deployer" 0) ("Deploy" "Deploy" 0)))"#
    ]];
    ParityBatchCase::value(
        "identifier_highlighting_respects_lisp_boundaries_case_and_overlay_properties",
        elisp_form,
        expected,
    )
}

fn face_rotation_reuses_released_faces_and_exhaustion_uses_the_last_face() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "alpha beta gamma delta alpha beta gamma delta")
  (let ((symbol-overlay-faces '(primary-face secondary-face)))
    (symbol-overlay-put-all "alpha" nil)
    (symbol-overlay-put-all "beta" nil)
    (let ((initial (neomacs-so-test-keywords)))
      (symbol-overlay-maybe-remove (symbol-overlay-assoc "alpha"))
      (symbol-overlay-put-all "gamma" nil)
      (symbol-overlay-put-all "delta" nil)
      (let ((allocated (neomacs-so-test-keywords))
            (overlays (neomacs-so-test-overlays)))
        (symbol-overlay-remove-all)
        (list :initial initial
              :allocated allocated
              :overlays overlays
              :after-cleanup
              (list (neomacs-so-test-keywords)
                    (neomacs-so-test-overlays)))))))
"###;
    let expected = expect![[
        r#"OK (:initial (("beta" nil secondary-face) ("alpha" nil primary-face)) :allocated (("delta" nil secondary-face) ("gamma" nil primary-face) ("beta" nil secondary-face)) :overlays ((:range (7 11) :text "beta" :symbol "beta" :face secondary-face :keymap symbol-overlay-map :evaporate t :priority nil :created nil) (:range (12 17) :text "gamma" :symbol "gamma" :face primary-face :keymap symbol-overlay-map :evaporate t :priority nil :created nil) (:range (18 23) :text "delta" :symbol "delta" :face secondary-face :keymap symbol-overlay-map :evaporate t :priority nil :created nil) (:range (30 34) :text "beta" :symbol "beta" :face secondary-face :keymap symbol-overlay-map :evaporate t :priority nil :created nil) (:range (35 40) :text "gamma" :symbol "gamma" :face primary-face :keymap symbol-overlay-map :evaporate t :priority nil :created nil) (:range (41 46) :text "delta" :symbol "delta" :face secondary-face :keymap symbol-overlay-map :evaporate t :priority nil :created nil)) :after-cleanup (nil nil))"#
    ]];
    ParityBatchCase::value(
        "face_rotation_reuses_released_faces_and_exhaustion_uses_the_last_face",
        elisp_form,
        expected,
    )
}

fn cyclic_navigation_switching_and_mark_history_follow_nearby_identifiers() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "deploy alpha deploy beta alpha deploy")
  (symbol-overlay-put-all "deploy" nil)
  (symbol-overlay-put-all "alpha" nil)
  (setq neomacs-so-test-jump-count 0)
  (let ((symbol-overlay-jump-hook '(neomacs-so-test-record-jump))
        records)
    (goto-char (point-min))
    (forward-char 2)
    (push (cons :start (neomacs-so-test-point-record)) records)
    (symbol-overlay-jump-next)
    (push (cons :next (neomacs-so-test-point-record)) records)
    (symbol-overlay-jump-next)
    (push (cons :next-again (neomacs-so-test-point-record)) records)
    (symbol-overlay-jump-next)
    (push (cons :wrapped-next (neomacs-so-test-point-record)) records)
    (symbol-overlay-jump-prev)
    (push (cons :wrapped-prev (neomacs-so-test-point-record)) records)
    (symbol-overlay-jump-first)
    (push (cons :first (neomacs-so-test-point-record)) records)
    (symbol-overlay-jump-last)
    (push (cons :last (neomacs-so-test-point-record)) records)
    (goto-char (point-min))
    (forward-char 2)
    (symbol-overlay-switch-forward)
    (push (cons :switch-forward (neomacs-so-test-point-record)) records)
    (goto-char (point-max))
    (search-backward "deploy")
    (forward-char 2)
    (symbol-overlay-switch-backward)
    (push (cons :switch-backward (neomacs-so-test-point-record)) records)
    (symbol-overlay-remove-all)
    (list :records (nreverse records)
          :jump-hook-count neomacs-so-test-jump-count
          :mark-ring-size (length mark-ring))))
"###;
    let expected = expect![[
        r#"OK (:records ((:start :point 3 :mark nil :symbol "deploy") (:next :point 16 :mark 3 :symbol "deploy") (:next-again :point 34 :mark 16 :symbol "deploy") (:wrapped-next :point 3 :mark 34 :symbol "deploy") (:wrapped-prev :point 34 :mark 3 :symbol "deploy") (:first :point 3 :mark 34 :symbol "deploy") (:last :point 34 :mark 3 :symbol "deploy") (:switch-forward :point 8 :mark 3 :symbol "alpha") (:switch-backward :point 26 :mark 34 :symbol "alpha")) :jump-hook-count 6 :mark-ring-size 7)"#
    ]];
    ParityBatchCase::value(
        "cyclic_navigation_switching_and_mark_history_follow_nearby_identifiers",
        elisp_form,
        expected,
    )
}

fn scoped_rename_changes_one_defun_then_expands_the_highlight_to_the_buffer() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun compile-report (result)\n"
          "  (list result result-cache))\n\n"
          "(defun ship-report (result)\n"
          "  (list result result-cache))\n\n"
          "(setq value :global)\n")
  (goto-char (point-min))
  (search-forward "result")
  (backward-char 2)
  (symbol-overlay-put-all "result" t)
  (let ((scoped-before
         (list :keywords (neomacs-so-test-keywords)
               :overlays (neomacs-so-test-overlays))))
    (cl-letf (((symbol-function 'read-string)
               (lambda (&rest _arguments) "value")))
      (symbol-overlay-rename))
    (let ((renamed
           (list :buffer (buffer-string)
                 :keywords (neomacs-so-test-keywords)
                 :overlays (neomacs-so-test-overlays)
                 :scope-default symbol-overlay-scope)))
      (goto-char (point-min))
      (search-forward "value")
      (backward-char 2)
      (symbol-overlay-toggle-in-scope)
      (prog1
          (list :scoped-before scoped-before
                :renamed renamed
                :expanded
                (list :keywords (neomacs-so-test-keywords)
                      :overlays (neomacs-so-test-overlays)
                      :scope-default symbol-overlay-scope))
        (symbol-overlay-remove-all)))))
"###;
    let expected = expect![[
        r#"OK (:scoped-before (:keywords (("result" t symbol-overlay-face-1)) :overlays ((:range (24 30) :text "result" :symbol "result" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil) (:range (40 46) :text "result" :symbol "result" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil))) :renamed (:buffer "(defun compile-report (value)\n  (list value result-cache))\n\n(defun ship-report (result)\n  (list result result-cache))\n\n(setq value :global)\n" :keywords (("value" t symbol-overlay-face-1)) :overlays ((:range (24 29) :text "value" :symbol "value" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil) (:range (39 44) :text "value" :symbol "value" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil)) :scope-default nil) :expanded (:keywords (("value" nil symbol-overlay-face-1)) :overlays ((:range (24 29) :text "value" :symbol "value" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil) (:range (39 44) :text "value" :symbol "value" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil) (:range (126 131) :text "value" :symbol "value" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil)) :scope-default nil))"#
    ]];
    ParityBatchCase::value(
        "scoped_rename_changes_one_defun_then_expands_the_highlight_to_the_buffer",
        elisp_form,
        expected,
    )
}

fn incremental_edits_remove_and_recreate_only_exact_identifier_overlays() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(list incident incident incident-log)\n"
          "(setq incident incident)\n")
  (symbol-overlay-put-all "incident" nil)
  (let ((initial (neomacs-so-test-overlays)))
    (goto-char (point-min))
    (search-forward "incident")
    (search-forward "incident")
    (let ((bounds (bounds-of-thing-at-point 'symbol)))
      (delete-region (car bounds) (cdr bounds))
      (insert "case"))
    (let ((after-replace (neomacs-so-test-overlays)))
      (goto-char (point-min))
      (search-forward "incident-log")
      (let ((bounds (bounds-of-thing-at-point 'symbol)))
        (delete-region (car bounds) (cdr bounds))
        (insert "incident"))
      (let ((after-boundary-change (neomacs-so-test-overlays)))
        (goto-char (point-min))
        (search-forward "incident")
        (let ((bounds (bounds-of-thing-at-point 'symbol)))
          (delete-region (car bounds) (cdr bounds)))
        (prog1
            (list :initial initial
                  :after-replace after-replace
                  :after-boundary-change after-boundary-change
                  :after-delete (neomacs-so-test-overlays)
                  :buffer (buffer-string)
                  :keywords (neomacs-so-test-keywords))
          (symbol-overlay-remove-all))))))
"###;
    let expected = expect![[
        r#"OK (:initial ((:range (7 15) :text "incident" :symbol "incident" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil) (:range (16 24) :text "incident" :symbol "incident" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil) (:range (45 53) :text "incident" :symbol "incident" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil) (:range (54 62) :text "incident" :symbol "incident" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil)) :after-replace ((:range (7 15) :text "incident" :symbol "incident" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil) (:range (41 49) :text "incident" :symbol "incident" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil) (:range (50 58) :text "incident" :symbol "incident" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil)) :after-boundary-change ((:range (7 15) :text "incident" :symbol "incident" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil) (:range (21 29) :text "incident" :symbol "incident" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil) (:range (37 45) :text "incident" :symbol "incident" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil) (:range (46 54) :text "incident" :symbol "incident" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil)) :after-delete ((:range (13 21) :text "incident" :symbol "incident" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil) (:range (29 37) :text "incident" :symbol "incident" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil) (:range (38 46) :text "incident" :symbol "incident" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil)) :buffer "(list  case incident)\n(setq incident incident)\n" :keywords (("incident" nil symbol-overlay-face-1)))"#
    ]];
    ParityBatchCase::value(
        "incremental_edits_remove_and_recreate_only_exact_identifier_overlays",
        elisp_form,
        expected,
    )
}

fn temporary_highlighting_tracks_repeated_and_single_symbols_through_mode_lifecycle()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "draft draft release")
  (let ((symbol-overlay-idle-time 0)
        (symbol-overlay-displayed-window nil)
        (symbol-overlay-temp-highlight-single nil))
    (symbol-overlay-mode 1)
    (goto-char (point-min))
    (forward-char 2)
    (symbol-overlay-maybe-put-temp)
    (let ((repeated
           (list :temp symbol-overlay-temp-symbol
                 :overlays (neomacs-so-test-overlays)
                 :hook (and (memq #'symbol-overlay-post-command post-command-hook)
                            t))))
      (search-forward "release")
      (backward-char 2)
      (symbol-overlay-post-command)
      (symbol-overlay-maybe-put-temp)
      (let ((single-disabled
             (list :temp symbol-overlay-temp-symbol
                   :overlays (neomacs-so-test-overlays))))
        (setq symbol-overlay-temp-highlight-single t)
        (symbol-overlay-maybe-put-temp)
        (let ((single-enabled
               (list :temp symbol-overlay-temp-symbol
                     :overlays (neomacs-so-test-overlays))))
          (symbol-overlay-mode -1)
          (list :repeated repeated
                :single-disabled single-disabled
                :single-enabled single-enabled
                :disabled
                (list :mode symbol-overlay-mode
                      :temp symbol-overlay-temp-symbol
                      :overlays (neomacs-so-test-overlays)
                      :hook (and (memq #'symbol-overlay-post-command
                                       post-command-hook)
                                 t))))))))
"###;
    let expected = expect![[
        r#"OK (:repeated (:temp "draft" :overlays ((:range (1 6) :text "draft" :symbol "" :face symbol-overlay-default-face :keymap nil :evaporate nil :priority nil :created nil) (:range (7 12) :text "draft" :symbol "" :face symbol-overlay-default-face :keymap nil :evaporate nil :priority nil :created nil)) :hook t) :single-disabled (:temp nil :overlays nil) :single-enabled (:temp "release" :overlays ((:range (13 20) :text "release" :symbol "" :face symbol-overlay-default-face :keymap nil :evaporate nil :priority nil :created nil))) :disabled (:mode nil :temp nil :overlays nil :hook nil))"#
    ]];
    ParityBatchCase::value(
        "temporary_highlighting_tracks_repeated_and_single_symbols_through_mode_lifecycle",
        elisp_form,
        expected,
    )
}

fn c_workflow_ignores_language_keywords_but_highlights_values_and_copies_identifiers()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (c-mode)
  (insert "int calculate_total(int value) {\n"
          "  int total = value + value;\n"
          "  return total;\n"
          "}\n")
  (let ((symbol-overlay-idle-time 0)
        (symbol-overlay-displayed-window nil)
        (symbol-overlay-temp-highlight-single t)
        (kill-ring nil)
        (kill-ring-yank-pointer nil))
    (symbol-overlay-mode 1)
    (goto-char (point-min))
    (forward-char 1)
    (symbol-overlay-maybe-put-temp)
    (let ((keyword
           (list :symbol (symbol-overlay-get-symbol)
                 :ignored (symbol-overlay-ignored-p "int")
                 :overlays (neomacs-so-test-overlays))))
      (search-forward "value")
      (backward-char 2)
      (symbol-overlay-maybe-put-temp)
      (let ((business-value
             (list :symbol (symbol-overlay-get-symbol)
                   :ignored (symbol-overlay-ignored-p "value")
                   :overlays (neomacs-so-test-overlays))))
        (goto-char (point-min))
        (search-forward "calculate_total")
        (backward-char 3)
        (symbol-overlay-save-symbol)
        (let ((copied (current-kill 0 t)))
          (symbol-overlay-mode -1)
          (list :keyword keyword
                :business-value business-value
                :copied copied
                :extra-type
                (let ((c-font-lock-extra-types '("money_t")))
                  (symbol-overlay-ignore-function-c "money_t"))))))))
"###;
    let expected = expect![[
        r#"OK (:keyword (:symbol "int" :ignored "int" :overlays nil) :business-value (:symbol "value" :ignored nil :overlays ((:range (25 30) :text "value" :symbol "" :face symbol-overlay-default-face :keymap nil :evaporate nil :priority nil :created nil) (:range (48 53) :text "value" :symbol "" :face symbol-overlay-default-face :keymap nil :evaporate nil :priority nil :created nil) (:range (56 61) :text "value" :symbol "" :face symbol-overlay-default-face :keymap nil :evaporate nil :priority nil :created nil))) :copied "calculate_total" :extra-type "money_t")"#
    ]];
    ParityBatchCase::value(
        "c_workflow_ignores_language_keywords_but_highlights_values_and_copies_identifiers",
        elisp_form,
        expected,
    )
}

fn query_replace_orchestrates_a_real_exact_symbol_edit_and_rejects_scoped_runs() -> ParityBatchCase
{
    let elisp_form = r###"
(let (scope-error result)
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert "(deploy :preview)\n(deploy :production)\ndeploy-next\n")
    (goto-char (point-min))
    (search-forward "deploy")
    (backward-char 2)
    (symbol-overlay-put-all "deploy" t)
    (setq scope-error
          (neomacs-so-test-capture-error #'symbol-overlay-query-replace))
    (symbol-overlay-remove-all))
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert "(deploy :preview)\n(deploy :production)\ndeploy-next\n")
    (goto-char (point-min))
    (search-forward "deploy")
    (backward-char 2)
    (symbol-overlay-put-all "deploy" nil)
    (let (query-call)
      (cl-letf (((symbol-function 'read-string)
                 (lambda (&rest _arguments) "release"))
                ((symbol-function 'query-replace-regexp)
                 (lambda (regexp replacement &rest arguments)
                   (setq query-call (list regexp replacement arguments))
                   (save-excursion
                     (goto-char (point-min))
                     (while (re-search-forward regexp nil t)
                       (replace-match replacement t))))))
        (symbol-overlay-query-replace))
      (setq result
            (list :buffer (buffer-string)
                  :query-call query-call
                  :query-defaults query-replace-defaults
                  :keywords (neomacs-so-test-keywords)
                  :overlays (neomacs-so-test-overlays)
                  :point (point)
                  :mark (mark t)))
      (symbol-overlay-remove-all)))
  (list :scope-error scope-error :replacement result))
"###;
    let expected = expect![[
        r#"OK (:scope-error (:error user-error :data ("Query-replace invalid in scope") :message "Query-replace invalid in scope") :replacement (:buffer "(release :preview)\n(release :production)\ndeploy-next\n" :query-call ("\\_<deploy\\_>" "release" nil) :query-defaults (("deploy" . "release")) :keywords (("release" nil symbol-overlay-face-1)) :overlays ((:range (2 9) :text "release" :symbol "release" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil) (:range (21 28) :text "release" :symbol "release" :face symbol-overlay-face-1 :keymap symbol-overlay-map :evaporate t :priority nil :created nil)) :point 2 :mark 2))"#
    ]];
    ParityBatchCase::value(
        "query_replace_orchestrates_a_real_exact_symbol_edit_and_rejects_scoped_runs",
        elisp_form,
        expected,
    )
}

fn definition_jump_finds_the_real_defun_and_preserves_the_identifier_offset() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(deploy :preview)\n"
          "(defun deploy (stage)\n"
          "  (message \"deploy %s\" stage))\n"
          "(deploy :production)\n")
  (goto-char (point-min))
  (search-forward "deploy")
  (backward-char 3)
  (setq neomacs-so-test-jump-count 0)
  (let ((symbol-overlay-jump-hook '(neomacs-so-test-record-jump)))
    (let ((before (neomacs-so-test-point-record)))
      (symbol-overlay-jump-to-definition)
      (list :before before
            :after (neomacs-so-test-point-record)
            :line (buffer-substring-no-properties
                   (line-beginning-position) (line-end-position))
            :jump-hook-count neomacs-so-test-jump-count
            :definition-regexp
            (funcall symbol-overlay-definition-function "deploy")))))
"###;
    let expected = expect![[
        r#"OK (:before (:point 5 :mark nil :symbol "deploy") :after (:point 29 :mark 5 :symbol "deploy") :line "(defun deploy (stage)" :jump-hook-count 1 :definition-regexp "(?def[a-z-]* \\_<deploy\\_>")"#
    ]];
    ParityBatchCase::value(
        "definition_jump_finds_the_real_defun_and_preserves_the_identifier_offset",
        elisp_form,
        expected,
    )
}

#[test]
fn symbol_overlay_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(SYMBOL_OVERLAY_MELPA_PIN, "symbol-overlay.el")
            .expect("prepare revision-pinned Symbol Overlay source below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "symbol-overlay-package-batch",
        "Symbol Overlay",
        &[
            package_registration_exposes_commands_options_keymaps_and_hook_lifecycle(),
            identifier_highlighting_respects_lisp_boundaries_case_and_overlay_properties(),
            face_rotation_reuses_released_faces_and_exhaustion_uses_the_last_face(),
            cyclic_navigation_switching_and_mark_history_follow_nearby_identifiers(),
            scoped_rename_changes_one_defun_then_expands_the_highlight_to_the_buffer(),
            incremental_edits_remove_and_recreate_only_exact_identifier_overlays(),
            temporary_highlighting_tracks_repeated_and_single_symbols_through_mode_lifecycle(),
            c_workflow_ignores_language_keywords_but_highlights_values_and_copies_identifiers(),
            query_replace_orchestrates_a_real_exact_symbol_edit_and_rejects_scoped_runs(),
            definition_jump_finds_the_real_defun_and_preserves_the_identifier_offset(),
        ],
    );
}

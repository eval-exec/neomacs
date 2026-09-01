use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, IMENU_LIST_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'imenu-list)

(defvar neomacs-imenu-list-test-jumps nil)

(defun neomacs-imenu-list-test-record-jump ()
  "Record a stable description of an Imenu-List jump."
  (push (list (buffer-name) (line-number-at-pos) (current-column))
        neomacs-imenu-list-test-jumps))

(defun neomacs-imenu-list-test-reset ()
  "Restore global Imenu-List state between practical workflows."
  (imenu-list-stop-timer)
  (setq imenu-list-minor-mode nil
        imenu-list--imenu-entries nil
        imenu-list--line-entries nil
        imenu-list--displayed-buffer nil
        imenu-list--last-location nil
        neomacs-imenu-list-test-jumps nil)
  (when (get-buffer imenu-list-buffer-name)
    (kill-buffer imenu-list-buffer-name))
  (delete-other-windows)
  nil)

(defun neomacs-imenu-list-test-entry-position (entry)
  "Return ENTRY's raw position, including special Imenu entries."
  (if (listp (cdr entry)) (cadr entry) (cdr entry)))

(defun neomacs-imenu-list-test-normalize-index (entries buffer)
  "Normalize ENTRIES to names and source lines in BUFFER."
  (mapcar
   (lambda (entry)
     (if (imenu--subalist-p entry)
         (cons (car entry)
               (neomacs-imenu-list-test-normalize-index (cdr entry) buffer))
       (let ((position (neomacs-imenu-list-test-entry-position entry)))
         (list (car entry)
               (with-current-buffer buffer
                 (line-number-at-pos position))))))
   entries))

(defun neomacs-imenu-list-test-rendered-lines ()
  "Return rendered line, entry, and button behavior for the Ilist buffer."
  (save-excursion
    (goto-char (point-min))
    (let (result)
      (while (not (eobp))
        (let* ((line (buffer-substring-no-properties
                      (line-beginning-position) (line-end-position)))
               (entry (imenu-list--find-entry))
               (button-position
                (save-excursion
                  (back-to-indentation)
                  (point)))
               (button (button-at button-position)))
          (push (list :line line
                      :entry (car entry)
                      :subalist (and (imenu--subalist-p entry) t)
                      :face (and button (button-get button 'face))
                      :help (and button (button-get button 'help-echo))
                      :follow (and button (button-get button 'follow-link))
                      :action (and button (button-get button 'action)))
                result))
        (forward-line 1))
      (nreverse result))))

(defun neomacs-imenu-list-test-source (name)
  "Create NAME with a realistic nested service index."
  (let ((buffer (get-buffer-create name)))
    (with-current-buffer buffer
      (erase-buffer)
      (emacs-lisp-mode)
      (insert ";;; Orders\n"
              "(defun parse-order (payload)\n  payload)\n\n"
              "(defun checkout-total (order)\n  (+ 10 order))\n\n"
              ";;; Operations\n"
              "(defun deploy-canary ()\n  'ready)\n")
      (goto-char (point-min))
      (let ((orders (point-marker)) parse total operations deploy)
        (search-forward "(defun parse-order")
        (setq parse (copy-marker (match-beginning 0)))
        (search-forward "(defun checkout-total")
        (setq total (copy-marker (match-beginning 0)))
        (search-forward ";;; Operations")
        (setq operations (copy-marker (line-beginning-position)))
        (search-forward "(defun deploy-canary")
        (setq deploy (copy-marker (match-beginning 0)))
        (setq-local imenu-create-index-function
                    (lambda ()
                      `(("Orders" ("." . ,orders)
                         ("parse-order" . ,parse)
                         ("checkout-total" . ,total))
                        ("Operations" ("." . ,operations)
                         ("deploy-canary" . ,deploy)))))))
    buffer))
"###;

fn package_contract_exposes_sidebar_commands_keys_defaults_and_display_policy() -> ParityBatchCase {
    let elisp_form = r###"
(unwind-protect
    (progn
      (neomacs-imenu-list-test-reset)
      (let ((descriptor (cadr (assq 'imenu-list package-alist))))
        (with-current-buffer (imenu-list-get-buffer-create)
          (list
           :package
           (list :name (package-desc-name descriptor)
                 :version (package-version-join (package-desc-version descriptor))
                 :requirements (package-desc-reqs descriptor)
                 :feature (and (featurep 'imenu-list) t))
           :commands
           (mapcar #'commandp
                   '(imenu-list imenu-list-noselect imenu-list-refresh
                     imenu-list-smart-toggle imenu-list-minor-mode
                     imenu-list-goto-entry imenu-list-display-entry
                     imenu-list-ret-dwim imenu-list-display-dwim
                     imenu-list-quit-window))
           :keys
           (mapcar (lambda (key)
                     (lookup-key imenu-list-major-mode-map (kbd key)))
                   '("RET" "SPC" "TAB" "f" "g" "q" "n" "p"))
           :defaults
           (list imenu-list-buffer-name imenu-list-position imenu-list-size
                 imenu-list-auto-resize imenu-list-focus-after-activation
                 imenu-list-update-current-entry
                 imenu-list-persist-when-imenu-index-unavailable
                 imenu-list-auto-update imenu-list-idle-update-delay)
           :buffer
           (list major-mode mode-name buffer-read-only hs-minor-mode
                 comment-start comment-end
                 (equal mode-line-format imenu-list-mode-line-format))
           :display-policy
           (and (assoc (concat "^" (regexp-quote imenu-list-buffer-name) "$")
                       display-buffer-alist)
                t)))))
  (neomacs-imenu-list-test-reset))
"###;
    let expected = expect![[
        r#"OK (:package (:name imenu-list :version "20210420.1200" :requirements ((emacs (24 3))) :feature t) :commands (t t t t t t t t t t) :keys (imenu-list-ret-dwim imenu-list-display-dwim hs-toggle-hiding hs-toggle-hiding imenu-list-refresh imenu-list-quit-window next-line previous-line) :defaults ("*Ilist*" right 0.3 nil nil t t t 0.5) :buffer (imenu-list-major-mode "Ilist" t t "\\b\\B" "\\b\\B" t) :display-policy t)"#
    ]];
    ParityBatchCase::value(
        "package_contract_exposes_sidebar_commands_keys_defaults_and_display_policy",
        elisp_form,
        expected,
    )
}

fn real_emacs_lisp_source_builds_and_renders_a_nested_imenu_sidebar() -> ParityBatchCase {
    let elisp_form = r###"
(unwind-protect
    (progn
      (neomacs-imenu-list-test-reset)
      (let ((source (get-buffer-create "*imenu-list-real-source*")))
        (with-current-buffer source
          (erase-buffer)
          (emacs-lisp-mode)
          (insert "(defgroup checkout nil \"Checkout.\" :group 'tools)\n\n"
                  "(defcustom checkout-timeout 30 \"Timeout.\" :type 'integer)\n\n"
                  "(defun checkout-total (items)\n  (apply #'+ items))\n\n"
                  "(cl-defstruct checkout-order id items)\n")
          (goto-char (point-min))
          (imenu-list-update t))
        (with-current-buffer (imenu-list-get-buffer-create)
          (list
           :index (neomacs-imenu-list-test-normalize-index
                   imenu-list--imenu-entries source)
           :rendered (neomacs-imenu-list-test-rendered-lines)
           :line-entries
           (mapcar (lambda (entry)
                     (list (car entry) (and (imenu--subalist-p entry) t)))
                   imenu-list--line-entries)
           :displayed (buffer-name imenu-list--displayed-buffer)
           :read-only buffer-read-only
           :mode major-mode))))
  (when (get-buffer "*imenu-list-real-source*")
    (kill-buffer "*imenu-list-real-source*"))
  (neomacs-imenu-list-test-reset))
"###;
    let expected = expect![[
        r#"OK (:index (("Types" ("checkout" 1) ("checkout-order" 8)) ("Variables" ("checkout-timeout" 3)) ("checkout-total" 5)) :rendered ((:line "+ Types" :entry "Types" :subalist t :face imenu-list-entry-subalist-face-0 :help "Toggle: Types" :follow t :action imenu-list--action-toggle-hs) (:line "   checkout" :entry "checkout" :subalist nil :face imenu-list-entry-face-1 :help "Go to: checkout" :follow t :action imenu-list--action-goto-entry) (:line "   checkout-order" :entry "checkout-order" :subalist nil :face imenu-list-entry-face-1 :help "Go to: checkout-order" :follow t :action imenu-list--action-goto-entry) (:line "+ Variables" :entry "Variables" :subalist t :face imenu-list-entry-subalist-face-0 :help "Toggle: Variables" :follow t :action imenu-list--action-toggle-hs) (:line "   checkout-timeout" :entry "checkout-timeout" :subalist nil :face imenu-list-entry-face-1 :help "Go to: checkout-timeout" :follow t :action imenu-list--action-goto-entry) (:line "checkout-total" :entry "checkout-total" :subalist nil :face imenu-list-entry-face-0 :help "Go to: checkout-total" :follow t :action imenu-list--action-goto-entry)) :line-entries (("Types" t) ("checkout" nil) ("checkout-order" nil) ("Variables" t) ("checkout-timeout" nil) ("checkout-total" nil)) :displayed "*imenu-list-real-source*" :read-only t :mode imenu-list-major-mode)"#
    ]];
    ParityBatchCase::value(
        "real_emacs_lisp_source_builds_and_renders_a_nested_imenu_sidebar",
        elisp_form,
        expected,
    )
}

fn display_then_goto_leaf_preserves_preview_focus_and_runs_jump_hook() -> ParityBatchCase {
    let elisp_form = r###"
(unwind-protect
    (progn
      (neomacs-imenu-list-test-reset)
      (let* ((source (neomacs-imenu-list-test-source "*imenu-list-nav-source*"))
             (imenu-list-after-jump-hook
              '(neomacs-imenu-list-test-record-jump)))
        (switch-to-buffer source)
        (imenu-list-update t)
        (imenu-list-show-noselect)
        (let ((ilist-window (get-buffer-window imenu-list-buffer-name)))
          (select-window ilist-window)
          (goto-char (point-min))
          (search-forward "checkout-total")
          (beginning-of-line)
          (imenu-list-display-entry)
          (let ((preview
                 (list :selected (buffer-name (window-buffer (selected-window)))
                       :source-line
                       (with-current-buffer source (line-number-at-pos (point)))
                       :source-text
                       (with-current-buffer source
                         (buffer-substring-no-properties
                          (line-beginning-position) (line-end-position)))
                       :jumps (reverse neomacs-imenu-list-test-jumps))))
            (setq neomacs-imenu-list-test-jumps nil)
            (imenu-list-goto-entry)
            (list :preview preview
                  :goto
                  (list :selected (buffer-name (window-buffer (selected-window)))
                        :line (line-number-at-pos)
                        :text (buffer-substring-no-properties
                               (line-beginning-position) (line-end-position))
                        :jumps (reverse neomacs-imenu-list-test-jumps))
                  :sidebar
                  (with-current-buffer (imenu-list-get-buffer-create)
                    (list :point (point)
                          :line (line-number-at-pos)
                          :text (buffer-substring-no-properties
                                 (line-beginning-position) (line-end-position))
                          :highlight hl-line-mode)))))))
  (when (get-buffer "*imenu-list-nav-source*")
    (kill-buffer "*imenu-list-nav-source*"))
  (neomacs-imenu-list-test-reset))
"###;
    let expected = expect![[
        r#"OK (:preview (:selected "*Ilist*" :source-line 5 :source-text "(defun checkout-total (order)" :jumps (("*imenu-list-nav-source*" 5 0))) :goto (:selected "*imenu-list-nav-source*" :line 5 :text "(defun checkout-total (order)" :jumps (("*imenu-list-nav-source*" 5 0))) :sidebar (:point 30 :line 4 :text "   checkout-total" :highlight t))"#
    ]];
    ParityBatchCase::value(
        "display_then_goto_leaf_preserves_preview_focus_and_runs_jump_hook",
        elisp_form,
        expected,
    )
}

fn nested_groups_fold_and_expand_with_hideshow_without_losing_entry_mapping() -> ParityBatchCase {
    let elisp_form = r###"
(unwind-protect
    (progn
      (neomacs-imenu-list-test-reset)
      (let ((source (neomacs-imenu-list-test-source "*imenu-list-fold-source*")))
        (with-current-buffer source (imenu-list-update t))
        (with-current-buffer (imenu-list-get-buffer-create)
          (goto-char (point-min))
          (let ((before (neomacs-imenu-list-test-rendered-lines)))
            (hs-toggle-hiding)
            (let ((hidden
                   (mapcar (lambda (overlay)
                             (list (overlay-start overlay) (overlay-end overlay)
                                   (overlay-get overlay 'invisible)
                                   (overlay-get overlay 'hs)))
                           (overlays-in (point-min) (point-max))))
                  (mapping (mapcar #'car imenu-list--line-entries)))
              (hs-toggle-hiding)
              (list :before before :hidden hidden :mapping mapping
                    :shown-overlays (length (overlays-in (point-min) (point-max)))
                    :text (buffer-substring-no-properties
                           (point-min) (point-max))))))))
  (when (get-buffer "*imenu-list-fold-source*")
    (kill-buffer "*imenu-list-fold-source*"))
  (neomacs-imenu-list-test-reset))
"###;
    let expected = expect![[
        r#"OK (:before ((:line "+ Orders" :entry "Orders" :subalist t :face imenu-list-entry-subalist-face-0 :help "Toggle: Orders" :follow t :action imenu-list--action-toggle-hs) (:line "   ." :entry "." :subalist nil :face imenu-list-entry-face-1 :help "Go to: ." :follow t :action imenu-list--action-goto-entry) (:line "   parse-order" :entry "parse-order" :subalist nil :face imenu-list-entry-face-1 :help "Go to: parse-order" :follow t :action imenu-list--action-goto-entry) (:line "   checkout-total" :entry "checkout-total" :subalist nil :face imenu-list-entry-face-1 :help "Go to: checkout-total" :follow t :action imenu-list--action-goto-entry) (:line "+ Operations" :entry "Operations" :subalist t :face imenu-list-entry-subalist-face-0 :help "Toggle: Operations" :follow t :action imenu-list--action-toggle-hs) (:line "   ." :entry "." :subalist nil :face imenu-list-entry-face-1 :help "Go to: ." :follow t :action imenu-list--action-goto-entry) (:line "   deploy-canary" :entry "deploy-canary" :subalist nil :face imenu-list-entry-face-1 :help "Go to: deploy-canary" :follow t :action imenu-list--action-goto-entry)) :hidden ((1 9 nil nil) (9 47 hs code) (13 14 nil nil) (18 29 nil nil) (33 47 nil nil) (48 60 nil nil) (64 65 nil nil) (69 82 nil nil)) :mapping ("Orders" "." "parse-order" "checkout-total" "Operations" "." "deploy-canary") :shown-overlays 7 :text "+ Orders\n   .\n   parse-order\n   checkout-total\n+ Operations\n   .\n   deploy-canary\n")"#
    ]];
    ParityBatchCase::value(
        "nested_groups_fold_and_expand_with_hideshow_without_losing_entry_mapping",
        elisp_form,
        expected,
    )
}

fn point_motion_refreshes_current_entry_while_same_location_skips_duplicate_work() -> ParityBatchCase
{
    let elisp_form = r###"
(unwind-protect
    (progn
      (neomacs-imenu-list-test-reset)
      (let* ((source (neomacs-imenu-list-test-source "*imenu-list-update-source*"))
             (updates 0)
             (imenu-list-update-current-entry nil)
             (imenu-list-update-hook (list (lambda () (setq updates (1+ updates))))))
        (with-current-buffer source
          (goto-char (point-min))
          (imenu-list-update t)
          (let ((after-force updates)
                (first-text
                 (with-current-buffer (imenu-list-get-buffer-create)
                   (buffer-substring-no-properties (point-min) (point-max)))))
            (imenu-list-update)
            (let ((after-same-location updates))
              (search-forward "checkout-total")
              (imenu-list-update)
              (list :after-force after-force
                    :after-same-location after-same-location
                    :after-motion updates
                    :first-text first-text
                    :second-text
                    (with-current-buffer (imenu-list-get-buffer-create)
                      (buffer-substring-no-properties
                       (point-min) (point-max)))
                    :last-location
                    (list (buffer-name (marker-buffer imenu-list--last-location))
                          (marker-position imenu-list--last-location))))))))
  (when (get-buffer "*imenu-list-update-source*")
    (kill-buffer "*imenu-list-update-source*"))
  (neomacs-imenu-list-test-reset))
"###;
    let expected = expect![[
        r#"OK (:after-force 1 :after-same-location 1 :after-motion 2 :first-text "+ Orders\n   .\n   parse-order\n   checkout-total\n+ Operations\n   .\n   deploy-canary\n" :second-text "+ Orders\n   .\n   parse-order\n   checkout-total\n+ Operations\n   .\n   deploy-canary\n" :last-location ("*imenu-list-update-source*" 74))"#
    ]];
    ParityBatchCase::value(
        "point_motion_refreshes_current_entry_while_same_location_skips_duplicate_work",
        elisp_form,
        expected,
    )
}

fn unavailable_index_can_persist_previous_sidebar_or_clear_it_by_policy() -> ParityBatchCase {
    let elisp_form = r###"
(unwind-protect
    (progn
      (neomacs-imenu-list-test-reset)
      (let ((source (neomacs-imenu-list-test-source "*imenu-list-stable-source*"))
            (missing (get-buffer-create "*imenu-list-missing-source*")))
        (with-current-buffer source (imenu-list-update t))
        (let ((baseline
               (with-current-buffer (imenu-list-get-buffer-create)
                 (buffer-substring-no-properties (point-min) (point-max)))))
          (with-current-buffer missing
            (erase-buffer)
            (text-mode)
            (setq-local imenu-create-index-function
                        (lambda () (signal 'imenu-unavailable '("none"))))
            (let ((imenu-list-persist-when-imenu-index-unavailable t))
              (imenu-list-update t))
            (let ((persisted
                   (list :text
                         (with-current-buffer (imenu-list-get-buffer-create)
                           (buffer-substring-no-properties
                            (point-min) (point-max)))
                         :displayed (buffer-name imenu-list--displayed-buffer)
                         :entries (mapcar #'car imenu-list--line-entries))))
              (let ((imenu-list-persist-when-imenu-index-unavailable nil))
                (imenu-list-update t))
              (list :baseline baseline :persisted persisted
                    :cleared
                    (with-current-buffer (imenu-list-get-buffer-create)
                      (list :text (buffer-string)
                            :entries imenu-list--line-entries
                            :index imenu-list--imenu-entries))))))))
  (dolist (name '("*imenu-list-stable-source*" "*imenu-list-missing-source*"))
    (when (get-buffer name) (kill-buffer name)))
  (neomacs-imenu-list-test-reset))
"###;
    let expected = expect![[
        r#"OK (:baseline "+ Orders\n   .\n   parse-order\n   checkout-total\n+ Operations\n   .\n   deploy-canary\n" :persisted (:text "+ Orders\n   .\n   parse-order\n   checkout-total\n+ Operations\n   .\n   deploy-canary\n" :displayed "*imenu-list-stable-source*" :entries ("Orders" "." "parse-order" "checkout-total" "Operations" "." "deploy-canary")) :cleared (:text "" :entries nil :index nil))"#
    ]];
    ParityBatchCase::value(
        "unavailable_index_can_persist_previous_sidebar_or_clear_it_by_policy",
        elisp_form,
        expected,
    )
}

fn custom_position_translation_selects_nearest_regular_and_special_entries() -> ParityBatchCase {
    let elisp_form = r###"
(unwind-protect
    (progn
      (neomacs-imenu-list-test-reset)
      (let ((source (get-buffer-create "*imenu-list-translated-source*")))
        (with-current-buffer source
          (erase-buffer)
          (insert "alpha\nbody\nbeta\nbody\ngamma\nbody\n")
          (goto-char (point-min))
          (let ((alpha (vector (point))))
            (forward-line 2)
            (let ((beta (vector (point))))
              (forward-line 2)
              (let ((gamma (vector (point))))
                (setq-local imenu-list-custom-position-translator
                            (lambda () (lambda (position) (aref position 0))))
                (setq imenu-list--displayed-buffer source
                      imenu-list--line-entries
                      `(("alpha" . ,alpha)
                        ("beta" ,beta goto-char ,(aref beta 0))
                        ("gamma" . ,gamma))))))
          (goto-char (point-min))
          (let ((at-alpha (car (imenu-list--current-entry))))
            (forward-line 3)
            (let ((between (car (imenu-list--current-entry))))
              (goto-char (point-max))
              (list :at-alpha at-alpha
                    :between between
                    :at-end (car (imenu-list--current-entry))
                    :translator
                    (funcall (imenu-list-position-translator) (vector 7))))))))
  (when (get-buffer "*imenu-list-translated-source*")
    (kill-buffer "*imenu-list-translated-source*"))
  (neomacs-imenu-list-test-reset))
"###;
    let expected =
        expect![[r#"OK (:at-alpha "alpha" :between "beta" :at-end "gamma" :translator 7)"#]];
    ParityBatchCase::value(
        "custom_position_translation_selects_nearest_regular_and_special_entries",
        elisp_form,
        expected,
    )
}

fn global_mode_opens_dedicated_sidebar_starts_timer_and_cleans_up_on_disable() -> ParityBatchCase {
    let elisp_form = r###"
(unwind-protect
    (progn
      (neomacs-imenu-list-test-reset)
      (let ((source (neomacs-imenu-list-test-source "*imenu-list-mode-source*"))
            (imenu-list-auto-update t)
            (imenu-list-focus-after-activation nil)
            (imenu-list-position 'right)
            (imenu-list-size 20))
        (switch-to-buffer source)
        (imenu-list-minor-mode 1)
        (let* ((window (get-buffer-window imenu-list-buffer-name))
               (enabled
                (list :mode imenu-list-minor-mode
                      :selected (buffer-name (window-buffer (selected-window)))
                      :visible (and (window-live-p window) t)
                      :dedicated (and window (window-dedicated-p window))
                      :window-count (length (window-list))
                      :timer (and (timerp imenu-list--timer) t)
                      :timer-active (and (memq imenu-list--timer timer-idle-list) t)
                      :buffer-mode
                      (with-current-buffer (imenu-list-get-buffer-create)
                        major-mode))))
          (imenu-list-minor-mode -1)
          (list :enabled enabled
                :disabled
                (list :mode imenu-list-minor-mode
                      :visible (and (get-buffer-window imenu-list-buffer-name) t)
                      :window-count (length (window-list))
                      :timer imenu-list--timer
                      :buffer-live (and (get-buffer imenu-list-buffer-name) t))))))
  (when (get-buffer "*imenu-list-mode-source*")
    (kill-buffer "*imenu-list-mode-source*"))
  (neomacs-imenu-list-test-reset))
"###;
    let expected = expect![[
        r#"OK (:enabled (:mode t :selected "*imenu-list-mode-source*" :visible t :dedicated t :window-count 2 :timer t :timer-active t :buffer-mode imenu-list-major-mode) :disabled (:mode nil :visible nil :window-count 1 :timer nil :buffer-live t))"#
    ]];
    ParityBatchCase::value(
        "global_mode_opens_dedicated_sidebar_starts_timer_and_cleans_up_on_disable",
        elisp_form,
        expected,
    )
}

#[test]
fn imenu_list_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(IMENU_LIST_MELPA_PIN, "imenu-list.el")
            .expect("prepare revision-pinned Imenu-List below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "imenu-list-package-batch",
        "Imenu-List",
        &[
            package_contract_exposes_sidebar_commands_keys_defaults_and_display_policy(),
            real_emacs_lisp_source_builds_and_renders_a_nested_imenu_sidebar(),
            display_then_goto_leaf_preserves_preview_focus_and_runs_jump_hook(),
            nested_groups_fold_and_expand_with_hideshow_without_losing_entry_mapping(),
            point_motion_refreshes_current_entry_while_same_location_skips_duplicate_work(),
            unavailable_index_can_persist_previous_sidebar_or_clear_it_by_policy(),
            custom_position_translation_selects_nearest_regular_and_special_entries(),
            global_mode_opens_dedicated_sidebar_starts_timer_and_cleans_up_on_disable(),
        ],
    );
}

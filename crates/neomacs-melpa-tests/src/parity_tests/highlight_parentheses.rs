use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HIGHLIGHT_PARENTHESES_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const HIGHLIGHT_PARENTHESES_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const HIGHLIGHT_PARENTHESES_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'highlight-parentheses)

(defun neomacs-highlight-parentheses-test-in-buffer (text function)
  "Run FUNCTION in an Emacs Lisp work buffer containing TEXT."
  (let ((buffer (generate-new-buffer "*highlight-parentheses-parity*")))
    (unwind-protect
        (with-current-buffer buffer
          (insert text)
          (emacs-lisp-mode)
          (goto-char (point-min))
          (funcall function))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (when highlight-parentheses-mode
            (highlight-parentheses-mode -1))
          (set-buffer-modified-p nil))
        (kill-buffer buffer)))))

(defun neomacs-highlight-parentheses-test-face (face)
  "Return the configured attributes relevant to the parity corpus."
  (list :foreground (plist-get face :foreground)
        :background (plist-get face :background)
        :weight (plist-get face :weight)
        :underline (plist-get face :underline)
        :slant (plist-get face :slant)))

(defun neomacs-highlight-parentheses-test-overlays (&optional overlays)
  "Return stable summaries of Highlight Parentheses OVERLAYS."
  (mapcar
   (lambda (overlay)
     (let ((buffer (overlay-buffer overlay))
           (face (overlay-get
                  overlay highlight-parentheses--face-property)))
       (list :buffer (and buffer (buffer-name buffer))
             :range (and buffer
                         (list (overlay-start overlay)
                               (overlay-end overlay)))
             :text (and buffer
                        (buffer-substring-no-properties
                         (overlay-start overlay) (overlay-end overlay)))
             :marker (overlay-get overlay 'highlight-parentheses)
             :face (neomacs-highlight-parentheses-test-face face))))
   (or overlays highlight-parentheses--overlays)))

(defun neomacs-highlight-parentheses-test-force ()
  "Synchronously force the package to highlight around point."
  (setq highlight-parentheses--last-point -1)
  (highlight-parentheses--highlight (current-buffer)))

(defun neomacs-highlight-parentheses-test-hook-count (hook function)
  "Count registrations of FUNCTION in the effective value of HOOK."
  (cl-count function (symbol-value hook)))
"####;

fn highlight_parentheses_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HIGHLIGHT_PARENTHESES_MELPA_PIN, "highlight-parentheses.el")
        .expect("prepare revision-pinned Highlight Parentheses source below ./tmp")
        .with_prelude(HIGHLIGHT_PARENTHESES_TEST_PRELUDE)
        .with_timeout(HIGHLIGHT_PARENTHESES_TEST_TIMEOUT)
}

fn nested_program_context_maps_each_pair_to_its_configured_face() -> ParityBatchCase {
    let elisp_form = r####"
(let ((highlight-parentheses-colors
       '("red" "green" "blue" "purple"))
      (highlight-parentheses-background-colors
       '(nil "gray20"))
      (highlight-parentheses-attributes
       '((:weight bold) (:underline t) (:slant italic)))
      (highlight-parentheses-highlight-adjacent nil))
  (neomacs-highlight-parentheses-test-in-buffer
   "(defun deploy (payload)\n  (when (and payload (> (length payload) 2))\n    (message \"%s\" (car payload))))\n"
   (lambda ()
     (search-forward "payload))))")
     (backward-char 4)
     (highlight-parentheses-mode 1)
     (neomacs-highlight-parentheses-test-force)
     (list :mode highlight-parentheses-mode
           :lighter (assq 'highlight-parentheses-mode minor-mode-alist)
           :syntax-depth (car (syntax-ppss))
           :last-point highlight-parentheses--last-point
           :last-pair highlight-parentheses--last-pair
           :hooks
           (list
            (neomacs-highlight-parentheses-test-hook-count
             'post-command-hook 'highlight-parentheses--initiate-highlight)
            (neomacs-highlight-parentheses-test-hook-count
             'before-revert-hook 'highlight-parentheses--delete-overlays)
            (neomacs-highlight-parentheses-test-hook-count
             'change-major-mode-hook 'highlight-parentheses--delete-overlays))
           :overlays (neomacs-highlight-parentheses-test-overlays)))))
"####;
    let expected = expect![[
        r#"OK (:mode t :lighter (highlight-parentheses-mode " hl-p") :syntax-depth 4 :last-point 100 :last-pair (88 . 100) :hooks (1 1 1) :overlays ((:buffer "*highlight-parentheses-parity*" :range (88 89) :text "(" :marker t :face (:foreground "red" :background nil :weight bold :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (100 101) :text ")" :marker t :face (:foreground "red" :background nil :weight bold :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (74 75) :text "(" :marker t :face (:foreground "green" :background "gray20" :weight nil :underline t :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (101 102) :text ")" :marker t :face (:foreground "green" :background "gray20" :weight nil :underline t :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (27 28) :text "(" :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant italic)) (:buffer "*highlight-parentheses-parity*" :range (102 103) :text ")" :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant italic)) (:buffer "*highlight-parentheses-parity*" :range (1 2) :text "(" :marker t :face (:foreground "purple" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (103 104) :text ")" :marker t :face (:foreground "purple" :background nil :weight nil :underline nil :slant nil))))"#
    ]];
    ParityBatchCase::value(
        "nested_program_context_maps_each_pair_to_its_configured_face",
        elisp_form,
        expected,
    )
}

fn navigation_reuses_the_same_pair_then_reduces_to_the_outer_call() -> ParityBatchCase {
    let elisp_form = r####"
(let ((highlight-parentheses-colors '("red" "green" "blue"))
      (highlight-parentheses-background-colors nil)
      (highlight-parentheses-attributes nil)
      (highlight-parentheses-highlight-adjacent nil))
  (neomacs-highlight-parentheses-test-in-buffer
   "(mapcar (lambda (item) (list item (1+ item))) values)\n"
   (lambda ()
     (search-forward "1+ item")
     (highlight-parentheses-mode 1)
     (neomacs-highlight-parentheses-test-force)
     (let ((inside (neomacs-highlight-parentheses-test-overlays))
           (inside-pair highlight-parentheses--last-pair))
       (backward-char 1)
       (highlight-parentheses--highlight (current-buffer))
       (let ((same-pair (neomacs-highlight-parentheses-test-overlays))
             (same-last-point highlight-parentheses--last-point))
         (search-forward "values")
         (highlight-parentheses--highlight (current-buffer))
         (list :inside inside
               :same-pair same-pair
               :same-layout (equal inside same-pair)
               :inside-pair inside-pair
               :same-last-point same-last-point
               :outer-point (point)
               :outer-pair highlight-parentheses--last-pair
               :outer (neomacs-highlight-parentheses-test-overlays)))))))
"####;
    let expected = expect![[
        r#"OK (:inside ((:buffer "*highlight-parentheses-parity*" :range (35 36) :text "(" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (43 44) :text ")" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (24 25) :text "(" :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (44 45) :text ")" :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (9 10) :text "(" :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (45 46) :text ")" :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil))) :same-pair ((:buffer "*highlight-parentheses-parity*" :range (35 36) :text "(" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (43 44) :text ")" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (24 25) :text "(" :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (44 45) :text ")" :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (9 10) :text "(" :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (45 46) :text ")" :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil))) :same-layout t :inside-pair (35 . 43) :same-last-point 42 :outer-point 53 :outer-pair (1 . 53) :outer ((:buffer "*highlight-parentheses-parity*" :range (1 2) :text "(" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (53 54) :text ")" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil))))"#
    ]];
    ParityBatchCase::value(
        "navigation_reuses_the_same_pair_then_reduces_to_the_outer_call",
        elisp_form,
        expected,
    )
}

fn adjacent_delimiters_and_nonstructural_parens_follow_elisp_syntax() -> ParityBatchCase {
    let elisp_form = r####"
(let ((highlight-parentheses-colors '("red" "green" "blue"))
      (highlight-parentheses-background-colors nil)
      (highlight-parentheses-attributes nil)
      (highlight-parentheses-highlight-adjacent t))
  (neomacs-highlight-parentheses-test-in-buffer
   "(list \"(not structural)\" ; (ignored)\n      (deploy (plan)))\n"
   (lambda ()
     (highlight-parentheses-mode 1)
     (search-forward "not structural")
     (neomacs-highlight-parentheses-test-force)
     (let ((inside-string
            (list :context (syntax-ppss-context (syntax-ppss))
                  :overlays (neomacs-highlight-parentheses-test-overlays))))
       (goto-char (point-min))
       (search-forward "deploy ")
       (neomacs-highlight-parentheses-test-force)
       (let ((before-plan
              (list :following (char-after)
                    :overlays (neomacs-highlight-parentheses-test-overlays))))
         (goto-char (point-max))
         (skip-chars-backward "\n")
         (neomacs-highlight-parentheses-test-force)
         (list :inside-string inside-string
               :before-plan before-plan
               :after-expression
               (list :preceding (char-before)
                     :overlays
                     (neomacs-highlight-parentheses-test-overlays))))))))
"####;
    let expected = expect![[
        r#"OK (:inside-string (:context string :overlays ((:buffer "*highlight-parentheses-parity*" :range (1 2) :text "(" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (59 60) :text ")" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil)))) :before-plan (:following 40 :overlays ((:buffer "*highlight-parentheses-parity*" :range (52 53) :text "(" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (57 58) :text ")" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (44 45) :text "(" :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (58 59) :text ")" :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (1 2) :text "(" :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (59 60) :text ")" :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil)))) :after-expression (:preceding 41 :overlays ((:buffer "*highlight-parentheses-parity*" :range (1 2) :text "(" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (59 60) :text ")" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil)))))"#
    ]];
    ParityBatchCase::value(
        "adjacent_delimiters_and_nonstructural_parens_follow_elisp_syntax",
        elisp_form,
        expected,
    )
}

fn temporarily_unbalanced_editing_recovers_after_navigation() -> ParityBatchCase {
    let elisp_form = r####"
(let ((highlight-parentheses-colors '("red" "green" "blue" "purple"))
      (highlight-parentheses-background-colors nil)
      (highlight-parentheses-attributes nil)
      (highlight-parentheses-highlight-adjacent nil))
  (neomacs-highlight-parentheses-test-in-buffer
   "(defun calculate (x)\n  (let ((scaled (* x 2)))\n    (+ scaled 1)))\n"
   (lambda ()
     (highlight-parentheses-mode 1)
     (search-forward "+ scaled")
     (neomacs-highlight-parentheses-test-force)
     (let ((balanced (neomacs-highlight-parentheses-test-overlays)))
       (goto-char (point-max))
       (skip-chars-backward "\n")
       (delete-char -1)
       (search-backward "+ scaled")
       (forward-char 3)
       (neomacs-highlight-parentheses-test-force)
       (let ((unbalanced
              (list :text (buffer-string)
                    :pair highlight-parentheses--last-pair
                    :overlays
                    (neomacs-highlight-parentheses-test-overlays))))
         (goto-char (point-max))
         (skip-chars-backward "\n")
         (insert ")")
         (search-backward "+ scaled")
         (forward-char 3)
         (neomacs-highlight-parentheses-test-force)
         (let ((shortcut-restored
                (list :pair highlight-parentheses--last-pair
                      :overlays
                      (neomacs-highlight-parentheses-test-overlays))))
           (highlight-parentheses-mode -1)
           (highlight-parentheses-mode 1)
           (neomacs-highlight-parentheses-test-force)
           (let ((mode-refreshed
                  (list :pair highlight-parentheses--last-pair
                        :overlays
                        (neomacs-highlight-parentheses-test-overlays))))
             (goto-char (point-max))
             (neomacs-highlight-parentheses-test-force)
             (search-backward "+ scaled")
             (forward-char 3)
             (neomacs-highlight-parentheses-test-force)
             (list :balanced balanced
                   :unbalanced unbalanced
                   :restored-text (buffer-string)
                   :shortcut-restored shortcut-restored
                   :mode-refreshed mode-refreshed
                   :reentered-pair highlight-parentheses--last-pair
                   :reentered
                   (neomacs-highlight-parentheses-test-overlays)))))))))
"####;
    let expected = expect![[
        r#"OK (:balanced ((:buffer "*highlight-parentheses-parity*" :range (52 53) :text "(" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (63 64) :text ")" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (24 25) :text "(" :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (64 65) :text ")" :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (1 2) :text "(" :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (65 66) :text ")" :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "purple" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "purple" :background nil :weight nil :underline nil :slant nil))) :unbalanced (:text "(defun calculate (x)\n  (let ((scaled (* x 2)))\n    (+ scaled 1))\n" :pair (52 . 63) :overlays ((:buffer "*highlight-parentheses-parity*" :range (52 53) :text "(" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (63 64) :text ")" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (24 25) :text "(" :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (64 65) :text ")" :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (1 2) :text "(" :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (65 65) :text "" :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "purple" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "purple" :background nil :weight nil :underline nil :slant nil)))) :restored-text "(defun calculate (x)\n  (let ((scaled (* x 2)))\n    (+ scaled 1)))\n" :shortcut-restored (:pair (52 . 63) :overlays ((:buffer "*highlight-parentheses-parity*" :range (52 53) :text "(" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (63 64) :text ")" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (24 25) :text "(" :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (64 65) :text ")" :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (1 2) :text "(" :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (65 65) :text "" :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "purple" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "purple" :background nil :weight nil :underline nil :slant nil)))) :mode-refreshed (:pair (52 . 63) :overlays ((:buffer "*highlight-parentheses-parity*" :range (52 53) :text "(" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (63 64) :text ")" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (1 1) :text "" :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (1 1) :text "" :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (1 1) :text "" :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (1 1) :text "" :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (1 1) :text "" :marker t :face (:foreground "purple" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (1 1) :text "" :marker t :face (:foreground "purple" :background nil :weight nil :underline nil :slant nil)))) :reentered-pair (52 . 63) :reentered ((:buffer "*highlight-parentheses-parity*" :range (52 53) :text "(" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (63 64) :text ")" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (24 25) :text "(" :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (64 65) :text ")" :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (1 2) :text "(" :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (65 66) :text ")" :marker t :face (:foreground "blue" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "purple" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "purple" :background nil :weight nil :underline nil :slant nil))))"#
    ]];
    ParityBatchCase::value(
        "temporarily_unbalanced_editing_recovers_after_navigation",
        elisp_form,
        expected,
    )
}

fn live_customization_replaces_overlay_pool_and_applies_callable_settings() -> ParityBatchCase {
    let elisp_form = r####"
(let ((highlight-parentheses-colors '("red" "green"))
      (highlight-parentheses-background-colors nil)
      (highlight-parentheses-attributes nil)
      (highlight-parentheses-highlight-adjacent nil))
  (neomacs-highlight-parentheses-test-in-buffer
   "(outer (inner value))\n"
   (lambda ()
     (search-forward "value")
     (highlight-parentheses-mode 1)
     (neomacs-highlight-parentheses-test-force)
     (let ((old-pool highlight-parentheses--overlays)
           (initial (neomacs-highlight-parentheses-test-overlays)))
       (setq highlight-parentheses-colors
             (lambda () '("cyan" "magenta" "yellow"))
             highlight-parentheses-background-colors
             (lambda () '("black" nil "gray20"))
             highlight-parentheses-attributes
             (lambda () '((:weight bold) (:underline t))))
       (highlight-parentheses--color-update)
       (let ((shortcut-updated
              (neomacs-highlight-parentheses-test-overlays)))
         (goto-char (point-max))
         (neomacs-highlight-parentheses-test-force)
         (search-backward "value")
         (forward-char 3)
         (neomacs-highlight-parentheses-test-force)
         (list :initial initial
               :old-pool-after-update
               (neomacs-highlight-parentheses-test-overlays old-pool)
               :new-pool-length (length highlight-parentheses--overlays)
               :shortcut-updated shortcut-updated
               :reentered
               (neomacs-highlight-parentheses-test-overlays)))))))
"####;
    let expected = expect![[
        r#"OK (:initial ((:buffer "*highlight-parentheses-parity*" :range (8 9) :text "(" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (20 21) :text ")" :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (1 2) :text "(" :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (21 22) :text ")" :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil))) :old-pool-after-update ((:buffer nil :range nil :text nil :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil))) :new-pool-length 6 :shortcut-updated ((:buffer "*highlight-parentheses-parity*" :range (8 9) :text "(" :marker t :face (:foreground "cyan" :background "black" :weight bold :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (20 21) :text ")" :marker t :face (:foreground "cyan" :background "black" :weight bold :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (1 1) :text "" :marker t :face (:foreground "magenta" :background nil :weight nil :underline t :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (1 1) :text "" :marker t :face (:foreground "magenta" :background nil :weight nil :underline t :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (1 1) :text "" :marker t :face (:foreground "yellow" :background "gray20" :weight nil :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (1 1) :text "" :marker t :face (:foreground "yellow" :background "gray20" :weight nil :underline nil :slant nil))) :reentered ((:buffer "*highlight-parentheses-parity*" :range (8 9) :text "(" :marker t :face (:foreground "cyan" :background "black" :weight bold :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (20 21) :text ")" :marker t :face (:foreground "cyan" :background "black" :weight bold :underline nil :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (1 2) :text "(" :marker t :face (:foreground "magenta" :background nil :weight nil :underline t :slant nil)) (:buffer "*highlight-parentheses-parity*" :range (21 22) :text ")" :marker t :face (:foreground "magenta" :background nil :weight nil :underline t :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "yellow" :background "gray20" :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "yellow" :background "gray20" :weight nil :underline nil :slant nil))))"#
    ]];
    ParityBatchCase::value(
        "live_customization_replaces_overlay_pool_and_applies_callable_settings",
        elisp_form,
        expected,
    )
}

fn command_bursts_debounce_and_mode_shutdown_removes_editor_state() -> ParityBatchCase {
    let elisp_form = r####"
(let ((highlight-parentheses-colors '("red" "green"))
      (highlight-parentheses-background-colors nil)
      (highlight-parentheses-attributes nil)
      (highlight-parentheses-delay 0.2)
      next-id scheduled cancelled)
  (neomacs-highlight-parentheses-test-in-buffer
   "(outer (inner value))\n"
   (lambda ()
     (highlight-parentheses-mode 1)
     (let ((pool highlight-parentheses--overlays))
       (cl-letf (((symbol-function 'run-at-time)
                  (lambda (seconds repeat function &rest arguments)
                    (setq next-id (1+ (or next-id 0)))
                    (let ((timer
                           (intern (format "paren-timer-%d" next-id))))
                      (push (list timer seconds repeat function
                                  (mapcar (lambda (argument)
                                            (if (bufferp argument)
                                                (buffer-name argument)
                                              argument))
                                          arguments))
                            scheduled)
                      timer)))
                 ((symbol-function 'cancel-timer)
                  (lambda (timer) (push timer cancelled))))
         (highlight-parentheses--initiate-highlight)
         (highlight-parentheses--initiate-highlight)
         (highlight-parentheses--initiate-highlight)
         (let ((last-timer highlight-parentheses--timer))
           (highlight-parentheses-mode -1)
           (list :scheduled (nreverse scheduled)
                 :cancelled (nreverse cancelled)
                 :last-timer last-timer
                 :timer-after-disable highlight-parentheses--timer
                 :mode highlight-parentheses-mode
                 :hooks
                 (list
                  (neomacs-highlight-parentheses-test-hook-count
                   'post-command-hook 'highlight-parentheses--initiate-highlight)
                  (neomacs-highlight-parentheses-test-hook-count
                   'before-revert-hook 'highlight-parentheses--delete-overlays)
                  (neomacs-highlight-parentheses-test-hook-count
                   'change-major-mode-hook 'highlight-parentheses--delete-overlays))
                 :pool-after-disable
                 (neomacs-highlight-parentheses-test-overlays pool))))))))
"####;
    let expected = expect![[
        r#"OK (:scheduled ((paren-timer-1 0.2 nil highlight-parentheses--highlight ("*highlight-parentheses-parity*")) (paren-timer-2 0.2 nil highlight-parentheses--highlight ("*highlight-parentheses-parity*")) (paren-timer-3 0.2 nil highlight-parentheses--highlight ("*highlight-parentheses-parity*"))) :cancelled (paren-timer-1 paren-timer-2) :last-timer paren-timer-3 :timer-after-disable paren-timer-3 :mode nil :hooks (0 0 0) :pool-after-disable ((:buffer nil :range nil :text nil :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "red" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil)) (:buffer nil :range nil :text nil :marker t :face (:foreground "green" :background nil :weight nil :underline nil :slant nil))))"#
    ]];
    ParityBatchCase::value(
        "command_bursts_debounce_and_mode_shutdown_removes_editor_state",
        elisp_form,
        expected,
    )
}

#[test]
fn highlight_parentheses_package_batch() {
    assert_oracle_batch_cases(
        highlight_parentheses_oracle(),
        "highlight-parentheses-package-batch",
        "Highlight Parentheses",
        &[
            nested_program_context_maps_each_pair_to_its_configured_face(),
            navigation_reuses_the_same_pair_then_reduces_to_the_outer_call(),
            adjacent_delimiters_and_nonstructural_parens_follow_elisp_syntax(),
            temporarily_unbalanced_editing_recovers_after_navigation(),
            live_customization_replaces_overlay_pool_and_applies_callable_settings(),
            command_bursts_debounce_and_mode_shutdown_removes_editor_state(),
        ],
    );
}

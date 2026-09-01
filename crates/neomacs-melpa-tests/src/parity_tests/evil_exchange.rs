use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EVIL_EXCHANGE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const EVIL_EXCHANGE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const EVIL_EXCHANGE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'evil-exchange)

(defun neomacs-evil-exchange-test-in-buffer (name text function)
  "Run FUNCTION in a temporary buffer named for NAME containing TEXT."
  (evil-exchange--clean)
  (let ((buffer (generate-new-buffer (format "*evil-exchange-%s*" name))))
    (unwind-protect
        (with-current-buffer buffer
          (insert text)
          (goto-char (point-min))
          (funcall function))
      (evil-exchange--clean)
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (kill-buffer buffer)))))

(defun neomacs-evil-exchange-test-bounds (text &optional occurrence)
  "Return bounds of OCCURRENCE of TEXT from the buffer beginning."
  (goto-char (point-min))
  (let ((remaining (or occurrence 1)))
    (while (> remaining 0)
      (search-forward text)
      (setq remaining (1- remaining)))
    (cons (match-beginning 0) (match-end 0))))

(defun neomacs-evil-exchange-test-position (line column)
  "Return the position at zero-based COLUMN of one-based LINE."
  (save-excursion
    (goto-char (point-min))
    (forward-line (1- line))
    (move-to-column column)
    (point)))

(defun neomacs-evil-exchange-test-overlays ()
  "Return stable summaries of the pending exchange overlays."
  (mapcar
   (lambda (overlay)
     (let ((buffer (overlay-buffer overlay)))
       (list :buffer (and buffer (buffer-name buffer))
             :range (and buffer
                         (list (overlay-start overlay)
                               (overlay-end overlay)))
             :text (and buffer
                        (with-current-buffer buffer
                          (buffer-substring-no-properties
                           (overlay-start overlay) (overlay-end overlay))))
             :face (overlay-get overlay 'face))))
   (sort (copy-sequence evil-exchange--overlays)
         (lambda (left right)
           (let ((left-buffer (overlay-buffer left))
                 (right-buffer (overlay-buffer right)))
             (if (eq left-buffer right-buffer)
                 (< (overlay-start left) (overlay-start right))
               (string< (buffer-name left-buffer)
                        (buffer-name right-buffer))))))))

(defun neomacs-evil-exchange-test-state ()
  "Return stable marker and overlay state for a pending exchange."
  (list
   :position
   (when evil-exchange--position
     (cl-destructuring-bind (buffer beg end type)
         evil-exchange--position
       (list :buffer (buffer-name buffer)
             :beg (list (marker-position beg)
                        (marker-insertion-type beg))
             :end (list (marker-position end)
                        (marker-insertion-type end))
             :type type)))
   :overlays (neomacs-evil-exchange-test-overlays)))

(defun neomacs-evil-exchange-test-capture-signal (function)
  "Call FUNCTION and return complete stable signal data."
  (condition-case error-data
      (progn (funcall function) 'no-signal)
    (error
     (list :symbol (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))
"####;

fn evil_exchange_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EVIL_EXCHANGE_MELPA_PIN, "evil-exchange.el")
        .expect("prepare revision-pinned Evil Exchange source below ./tmp")
        .with_timeout(EVIL_EXCHANGE_TEST_TIMEOUT)
        .with_prelude(EVIL_EXCHANGE_TEST_PRELUDE)
}

fn edited_region_markers_keep_the_selected_words_and_exchange_in_place() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-exchange-test-in-buffer
 "edited-markers"
 "deploy alpha to staging, then beta to production.\n"
 (lambda ()
   (let ((alpha (neomacs-evil-exchange-test-bounds "alpha")))
     (evil-exchange (car alpha) (cdr alpha) 'exclusive))
   (let ((marked (neomacs-evil-exchange-test-state)))
     (goto-char (nth 1 evil-exchange--position))
     (insert "pre-")
     (let ((end-marker (nth 2 evil-exchange--position)))
       (goto-char end-marker)
       (insert "-post"))
     (let ((edited (list :text (buffer-string)
                         :state (neomacs-evil-exchange-test-state)))
           (beta (neomacs-evil-exchange-test-bounds "beta")))
       (evil-exchange (car beta) (cdr beta) 'exclusive)
       (list :marked marked
             :edited edited
             :final-text (buffer-string)
             :point (point)
             :final-state (neomacs-evil-exchange-test-state))))))
"####;
    let expected = expect![[
        r#"OK (:marked (:position (:buffer "*evil-exchange-edited-markers*" :beg (8 t) :end (13 nil) :type exclusive) :overlays ((:buffer "*evil-exchange-edited-markers*" :range (8 13) :text "alpha" :face highlight))) :edited (:text "deploy pre-alpha-post to staging, then beta to production.\n" :state (:position (:buffer "*evil-exchange-edited-markers*" :beg (12 t) :end (17 nil) :type exclusive) :overlays ((:buffer "*evil-exchange-edited-markers*" :range (12 17) :text "alpha" :face highlight)))) :final-text "deploy pre-beta-post to staging, then alpha to production.\n" :point 39 :final-state (:position nil :overlays nil))"#
    ]];
    ParityBatchCase::value(
        "edited_region_markers_keep_the_selected_words_and_exchange_in_place",
        elisp_form,
        expected,
    )
}

fn reverse_adjacent_and_punctuation_regions_swap_without_marker_drift() -> ParityBatchCase {
    let elisp_form = r####"
(list
 (neomacs-evil-exchange-test-in-buffer
  "reverse-adjacent"
  "leftRIGHT\n"
  (lambda ()
    (let ((right (neomacs-evil-exchange-test-bounds "RIGHT"))
          left)
      (evil-exchange (car right) (cdr right) 'exclusive)
      (setq left (neomacs-evil-exchange-test-bounds "left"))
      (evil-exchange (car left) (cdr left) 'exclusive)
      (list :text (buffer-string)
            :state (neomacs-evil-exchange-test-state)))))
 (neomacs-evil-exchange-test-in-buffer
  "punctuation"
  "alpha a, beta b\n"
  (lambda ()
    (let ((alpha (neomacs-evil-exchange-test-bounds "alpha"))
          beta)
      (evil-exchange (car alpha) (cdr alpha) 'exclusive)
      (setq beta (neomacs-evil-exchange-test-bounds "beta"))
      (evil-exchange (car beta) (cdr beta) 'exclusive)
      (list :text (buffer-string)
            :point (point)
            :state (neomacs-evil-exchange-test-state))))))
"####;
    let expected = expect![[
        r#"OK ((:text "RIGHTleft\n" :state (:position nil :overlays nil)) (:text "beta a, alpha b\n" :point 9 :state (:position nil :overlays nil)))"#
    ]];
    ParityBatchCase::value(
        "reverse_adjacent_and_punctuation_regions_swap_without_marker_drift",
        elisp_form,
        expected,
    )
}

fn linewise_exchange_reorders_complete_deployment_steps() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-exchange-test-in-buffer
 "linewise"
 "first: build\nsecond: test\nthird: deploy\n"
 (lambda ()
   (let ((first-beg (point-min))
         (first-end (save-excursion
                      (goto-char (point-min))
                      (forward-line 1)
                      (point)))
         third-beg third-end)
     (evil-exchange first-beg first-end 'line)
     (setq third-beg (save-excursion
                       (goto-char (point-min))
                       (forward-line 2)
                       (point))
           third-end (point-max))
     (let ((pending (neomacs-evil-exchange-test-state)))
       (evil-exchange third-beg third-end 'line)
       (list :pending pending
             :text (buffer-string)
             :lines (split-string (buffer-string) "\n" t)
             :point (point)
             :state (neomacs-evil-exchange-test-state))))))
"####;
    let expected = expect![[
        r#"OK (:pending (:position (:buffer "*evil-exchange-linewise*" :beg (1 t) :end (14 nil) :type line) :overlays ((:buffer "*evil-exchange-linewise*" :range (1 14) :text "first: build\n" :face highlight))) :text "third: deploy\nsecond: test\nfirst: build\n" :lines ("third: deploy" "second: test" "first: build") :point 1 :state (:position nil :overlays nil))"#
    ]];
    ParityBatchCase::value(
        "linewise_exchange_reorders_complete_deployment_steps",
        elisp_form,
        expected,
    )
}

fn cross_buffer_exchange_updates_both_open_configuration_files() -> ParityBatchCase {
    let elisp_form = r####"
(let ((source (generate-new-buffer "*evil-exchange-source-config*"))
      (target (generate-new-buffer "*evil-exchange-target-config*"))
      current-after-swap)
  (evil-exchange--clean)
  (unwind-protect
      (progn
        (with-current-buffer source
          (insert "service=api\nowner=alice\nregion=west\n")
          (let ((alice (neomacs-evil-exchange-test-bounds "alice")))
            (evil-exchange (car alice) (cdr alice) 'exclusive)))
        (let ((pending (neomacs-evil-exchange-test-state)))
          (with-current-buffer target
            (insert "service=worker\nowner=bob\nregion=east\n")
            (let ((bob (neomacs-evil-exchange-test-bounds "bob")))
              (evil-exchange (car bob) (cdr bob) 'exclusive))
            (setq current-after-swap (buffer-name)))
          (list
           :pending pending
           :source (with-current-buffer source (buffer-string))
           :target (with-current-buffer target (buffer-string))
           :current-buffer current-after-swap
           :state (neomacs-evil-exchange-test-state))))
    (evil-exchange--clean)
    (dolist (buffer (list source target))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer (set-buffer-modified-p nil))
        (kill-buffer buffer)))))
"####;
    let expected = expect![[
        r#"OK (:pending (:position (:buffer "*evil-exchange-source-config*" :beg (19 t) :end (24 nil) :type exclusive) :overlays ((:buffer "*evil-exchange-source-config*" :range (19 24) :text "alice" :face highlight))) :source "service=api\nowner=bob\nregion=west\n" :target "service=worker\nowner=alice\nregion=east\n" :current-buffer "*evil-exchange-target-config*" :state (:position nil :overlays nil))"#
    ]];
    ParityBatchCase::value(
        "cross_buffer_exchange_updates_both_open_configuration_files",
        elisp_form,
        expected,
    )
}

fn block_exchange_swaps_rectangular_table_columns() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-exchange-test-in-buffer
 "block"
 "aa11 xx\nbb22 yy\ncc33 zz\n"
 (lambda ()
   (let ((left-beg (neomacs-evil-exchange-test-position 1 0))
         (left-end (neomacs-evil-exchange-test-position 3 2))
         right-beg right-end)
     (evil-exchange left-beg left-end 'block)
     (setq right-beg (neomacs-evil-exchange-test-position 1 5)
           right-end (neomacs-evil-exchange-test-position 3 7))
     (let ((pending (neomacs-evil-exchange-test-state)))
       (evil-exchange right-beg right-end 'block)
       (list :pending pending
             :text (buffer-string)
             :rows (split-string (buffer-string) "\n" t)
             :mark (mark t)
             :point (point)
             :state (neomacs-evil-exchange-test-state))))))
"####;
    let expected = expect![[
        r#"OK (:pending (:position (:buffer "*evil-exchange-block*" :beg (1 t) :end (19 nil) :type block) :overlays ((:buffer "*evil-exchange-block*" :range (1 3) :text "aa" :face highlight) (:buffer "*evil-exchange-block*" :range (9 11) :text "bb" :face highlight) (:buffer "*evil-exchange-block*" :range (17 19) :text "cc" :face highlight))) :text "xx11 aa\nyy22 bb\nzz33 cc\n" :rows ("xx11 aa" "yy22 bb" "zz33 cc") :mark 6 :point 1 :state (:position nil :overlays nil))"#
    ]];
    ParityBatchCase::value(
        "block_exchange_swaps_rectangular_table_columns",
        elisp_form,
        expected,
    )
}

fn incompatible_exchange_can_be_cancelled_and_installers_bind_expected_keys() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-exchange-test-in-buffer
 "cancel-install"
 "aa11 xx\nbb22 yy\n"
 (lambda ()
   (let* ((block-beg (neomacs-evil-exchange-test-position 1 0))
          (block-end (neomacs-evil-exchange-test-position 2 2))
          (word (neomacs-evil-exchange-test-bounds "xx"))
          (old-normal-gx (lookup-key evil-normal-state-map (kbd "zx")))
          (old-visual-gx (lookup-key evil-visual-state-map (kbd "zx")))
          (old-normal-cancel (lookup-key evil-normal-state-map (kbd "zX")))
          (old-visual-cancel (lookup-key evil-visual-state-map (kbd "zX")))
          (old-operator-x (lookup-key evil-operator-state-map (kbd "x")))
          (old-visual-x (lookup-key evil-visual-state-map (kbd "X"))))
     (unwind-protect
         (progn
           (evil-exchange block-beg block-end 'block)
           (let ((incompatible
                  (neomacs-evil-exchange-test-capture-signal
                   (lambda ()
                     (evil-exchange (car word) (cdr word) 'exclusive))))
                 (pending (neomacs-evil-exchange-test-state)))
             (evil-exchange-cancel)
             (let ((cancel-message (current-message))
                   (cancelled (neomacs-evil-exchange-test-state)))
               (evil-exchange-cancel)
               (let ((empty-message (current-message))
                     (evil-exchange-key (kbd "zx"))
                     (evil-exchange-cancel-key (kbd "zX")))
                 (evil-exchange-install)
                 (evil-exchange-cx-install)
                 (list
                  :incompatible incompatible
                  :pending pending
                  :cancel-message cancel-message
                  :cancelled cancelled
                  :empty-message empty-message
                  :installed
                  (list
                   :normal-exchange
                   (lookup-key evil-normal-state-map (kbd "zx"))
                   :visual-exchange
                   (lookup-key evil-visual-state-map (kbd "zx"))
                   :normal-cancel
                   (lookup-key evil-normal-state-map (kbd "zX"))
                   :visual-cancel
                   (lookup-key evil-visual-state-map (kbd "zX"))
                   :operator-cx
                   (lookup-key evil-operator-state-map (kbd "x"))
                   :visual-x
                   (lookup-key evil-visual-state-map (kbd "X"))))))))
       (define-key evil-normal-state-map (kbd "zx") old-normal-gx)
       (define-key evil-visual-state-map (kbd "zx") old-visual-gx)
       (define-key evil-normal-state-map (kbd "zX") old-normal-cancel)
       (define-key evil-visual-state-map (kbd "zX") old-visual-cancel)
       (define-key evil-operator-state-map (kbd "x") old-operator-x)
       (define-key evil-visual-state-map (kbd "X") old-visual-x)))))
"####;
    let expected = expect![[
        r#"OK (:incompatible (:symbol user-error :data ("Can’t exchange block region with non-block region") :message "Can’t exchange block region with non-block region") :pending (:position (:buffer "*evil-exchange-cancel-install*" :beg (1 t) :end (11 nil) :type block) :overlays ((:buffer "*evil-exchange-cancel-install*" :range (1 3) :text "aa" :face highlight) (:buffer "*evil-exchange-cancel-install*" :range (9 11) :text "bb" :face highlight))) :cancel-message nil :cancelled (:position nil :overlays nil) :empty-message nil :installed (:normal-exchange evil-exchange :visual-exchange evil-exchange :normal-cancel evil-exchange-cancel :visual-cancel evil-exchange-cancel :operator-cx evil-exchange/cx :visual-x evil-exchange))"#
    ]];
    ParityBatchCase::value(
        "incompatible_exchange_can_be_cancelled_and_installers_bind_expected_keys",
        elisp_form,
        expected,
    )
}

#[test]
fn evil_exchange_package_batch() {
    assert_oracle_batch_cases(
        evil_exchange_oracle(),
        "evil-exchange-package-batch",
        "Evil Exchange",
        &[
            edited_region_markers_keep_the_selected_words_and_exchange_in_place(),
            reverse_adjacent_and_punctuation_regions_swap_without_marker_drift(),
            linewise_exchange_reorders_complete_deployment_steps(),
            cross_buffer_exchange_updates_both_open_configuration_files(),
            block_exchange_swaps_rectangular_table_columns(),
            incompatible_exchange_can_be_cancelled_and_installers_bind_expected_keys(),
        ],
    );
}

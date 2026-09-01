use std::time::Duration;

use crate::{CachedMelpaOracle, WINDOW_PURPOSE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod configuration;
mod display;
mod edges;
mod extensions;
mod layout;
mod lifecycle;
mod ownership;
mod routing;

const WINDOW_PURPOSE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const WINDOW_PURPOSE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'window-purpose)

;; GNU Emacs canonicalizes a fresh batch text frame's menu-bar offset on the
;; first window-configuration restoration.  Prime that transition once so a
;; case sees the same real frame-root coordinates alone or in a shared batch.
(save-window-excursion nil)

(defmacro neomacs-window-purpose-test-with-configuration
    (use-defaults mode-purposes name-purposes regexp-purposes &rest body)
  "Run BODY with an isolated, compiled Purpose configuration."
  (declare (indent 4) (debug (form form form form body)))
  `(let ((purpose-use-default-configuration ,use-defaults)
         (purpose-user-mode-purposes ,mode-purposes)
         (purpose-user-name-purposes ,name-purposes)
         (purpose-user-regexp-purposes ,regexp-purposes)
         (purpose-extended-configuration nil)
         (default-purpose 'general)
         (default-file-purpose 'edit)
         (purpose--user-mode-purposes (make-hash-table))
         (purpose--user-name-purposes (make-hash-table :test #'equal))
         (purpose--user-regexp-purposes (make-hash-table :test #'equal))
         (purpose--extended-mode-purposes (make-hash-table))
         (purpose--extended-name-purposes (make-hash-table :test #'equal))
         (purpose--extended-regexp-purposes (make-hash-table :test #'equal))
         (purpose--default-mode-purposes (make-hash-table))
         (purpose--default-name-purposes (make-hash-table :test #'equal))
         (purpose--default-regexp-purposes (make-hash-table :test #'equal)))
     (purpose-compile-default-configuration)
     (purpose-compile-extended-configuration)
     (purpose-compile-user-configuration)
     ,@body))

(defun neomacs-window-purpose-test-buffer-purpose (buffer)
  "Describe BUFFER through Window Purpose's public classification seam."
  (list (buffer-name buffer) (purpose-buffer-purpose buffer)))

(defun neomacs-window-purpose-test-kill-buffers (&rest buffers)
  "Kill test BUFFERS without allowing package hooks to retain them."
  (let ((kill-buffer-hook nil)
        (kill-buffer-query-functions nil))
    (dolist (buffer buffers)
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))

(defun neomacs-window-purpose-test-axis-edge
    (position start end start-name end-name)
  "Describe POSITION by its stable anchor between START and END."
  (cond
   ((= position start) start-name)
   ((= position end) end-name)
   ((<= (- position start) (- end position))
    (list start-name (- position start)))
   (t
    (list end-name (- end position)))))

(defun neomacs-window-purpose-test-root-edges (&optional frame)
  "Return the actual pixelwise frame-root window edges on FRAME.
Pixelwise edges avoid character-coordinate rounding around a text-frame
menu bar when a live root becomes an internal root after splitting."
  (window-edges (frame-root-window frame) nil nil t))

(defun neomacs-window-purpose-test-window-record (window)
  "Describe WINDOW relative to its usable root, without object identity."
  (let* ((edges (window-edges window nil nil t))
         (root-edges
          (neomacs-window-purpose-test-root-edges (window-frame window)))
         (left (nth 0 root-edges))
         (top (nth 1 root-edges))
         (right (nth 2 root-edges))
         (bottom (nth 3 root-edges)))
    (list :buffer (buffer-name (window-buffer window))
          :purpose (purpose-window-purpose window)
          :purpose-dedicated
          (and (purpose-window-purpose-dedicated-p window) t)
          :buffer-dedicated (window-dedicated-p window)
          :selected (eq window (selected-window))
          :edges
          (list
           (neomacs-window-purpose-test-axis-edge
            (nth 0 edges) left right 'left 'right)
           (neomacs-window-purpose-test-axis-edge
            (nth 1 edges) top bottom 'top 'bottom)
           (neomacs-window-purpose-test-axis-edge
            (nth 2 edges) left right 'left 'right)
           (neomacs-window-purpose-test-axis-edge
            (nth 3 edges) top bottom 'top 'bottom)))))

(defun neomacs-window-purpose-test-window-snapshot ()
  "Describe live non-minibuffer windows in stable geometric order."
  (let* ((windows (sort (copy-sequence (window-list nil 'nomini))
                        (lambda (left right)
                          (let ((left-edges (window-edges left nil nil t))
                                (right-edges (window-edges right nil nil t)))
                            (or (< (nth 1 left-edges) (nth 1 right-edges))
                                (and (= (nth 1 left-edges)
                                        (nth 1 right-edges))
                                     (< (car left-edges)
                                        (car right-edges))))))))
         (root-edges (neomacs-window-purpose-test-root-edges)))
    (list :frame-size (list (frame-width) (frame-height))
          :root-horizontal-span (list (nth 0 root-edges)
                                      (nth 2 root-edges))
          :root-vertical-span (list (nth 1 root-edges)
                                    (nth 3 root-edges))
          :windows
          (mapcar #'neomacs-window-purpose-test-window-record windows))))

(defun neomacs-window-purpose-test-layout-contract (layout)
  "Describe a Purpose LAYOUT without frame-dependent edge percentages."
  (if (purpose-window-params-p layout)
      (list :purpose (plist-get layout :purpose)
            :purpose-dedicated
            (and (plist-get layout :purpose-dedicated) t))
    (list :split (if (car layout) 'top-bottom 'left-right)
          :children
          (mapcar #'neomacs-window-purpose-test-layout-contract
                  (cddr layout)))))

(defvar neomacs-window-purpose-test-display-trace nil)

(defun neomacs-window-purpose-test-decline-display (buffer _alist)
  "Record and decline an explicit user display action for BUFFER."
  (push (list :user-declined (buffer-name buffer))
        neomacs-window-purpose-test-display-trace)
  nil)

(defun neomacs-window-purpose-test-search-p (purpose buffer _alist)
  "Record and recognize a search-purpose BUFFER."
  (push (list :predicate purpose (buffer-name buffer))
        neomacs-window-purpose-test-display-trace)
  (eq purpose 'search))

(defun neomacs-window-purpose-test-display-search-at-bottom (buffer alist)
  "Record and display search BUFFER in a five-line bottom pane."
  (push (list :special (buffer-name buffer))
        neomacs-window-purpose-test-display-trace)
  (purpose-display-at-bottom buffer alist 5))

(defun neomacs-window-purpose-test-return-fail (buffer _alist)
  "Record BUFFER and return GNU `display-buffer' sentinel `fail'."
  (push (list :user-failed (buffer-name buffer))
        neomacs-window-purpose-test-display-trace)
  'fail)

(defun neomacs-window-purpose-test-after-display (window)
  "Record Window Purpose's synchronous successful-display hook."
  (push (list :hook
              (buffer-name (window-buffer window))
              (eq window (selected-window)))
        neomacs-window-purpose-test-display-trace))

(defvar neomacs-window-purpose-test-mode-trace nil)

(defun neomacs-window-purpose-test-mode-state ()
  "Describe the global Purpose mode contract without function objects."
  (list
   :mode (and purpose-mode t)
   :active (and purpose--active-p t)
   :advices
   (cl-loop
    for (function . advice)
    in '((switch-to-buffer . purpose-switch-to-buffer-advice)
         (switch-to-buffer-other-window
          . purpose-switch-to-buffer-other-window-advice)
         (switch-to-buffer-other-frame
          . purpose-switch-to-buffer-other-frame-advice)
         (pop-to-buffer . purpose-pop-to-buffer-advice)
         (pop-to-buffer-same-window . purpose-pop-to-buffer-same-window-advice)
         (display-buffer . purpose-display-buffer-advice))
    when (advice-member-p advice function)
    collect function)
   :overriding-action (copy-tree display-buffer-overriding-action)
   :switch-key (lookup-key purpose-mode-map (kbd "C-x b"))
   :modeline (purpose--modeline-string)))

(defun neomacs-window-purpose-test-record-mode-hook ()
  "Record the public mode hook after Purpose has changed its state."
  (push (list (and purpose-mode t)
              (and purpose--active-p t)
              (and (advice-member-p #'purpose-display-buffer-advice
                                    'display-buffer)
                   t))
        neomacs-window-purpose-test-mode-trace))
"##;

fn window_purpose_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(WINDOW_PURPOSE_MELPA_PIN, "window-purpose.el")
        .expect("prepare exact Window Purpose source and dependencies below ./tmp")
        .with_prelude(WINDOW_PURPOSE_TEST_PRELUDE)
        .with_timeout(WINDOW_PURPOSE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Window Purpose parity test")
        .into()
}

pub(crate) fn assert_window_purpose_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        window_purpose_oracle(),
        &current_test_name(),
        "window_purpose_parity",
        cases,
    );
}

#[test]
fn window_purpose_package_batch() {
    let cases = [
        configuration::configuration_batch_cases(),
        display::display_batch_cases(),
        edges::edge_batch_cases(),
        extensions::extension_batch_cases(),
        layout::layout_batch_cases(),
        lifecycle::lifecycle_batch_cases(),
        ownership::ownership_batch_cases(),
        routing::routing_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    assert_window_purpose_batch(&cases);
}

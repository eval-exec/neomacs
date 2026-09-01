use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, SPLITTER_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const SPLITTER_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const SPLITTER_TEST_PRELUDE: &str = r#####"
(require 'cl-lib)
(require 'splitter)

(defun splitter-parity-window-shape (&optional windows)
  (mapcar
   (lambda (window)
     (list (buffer-name (window-buffer window))
           (window-total-width window)
           (window-total-height window)))
   (spl-sorted-window-list windows)))

(defun splitter-parity-kill-buffers ()
  (dolist (buffer (buffer-list))
    (when (string-match-p "\\`\\*splitter parity " (buffer-name buffer))
      (kill-buffer buffer))))

(splitter-parity-kill-buffers)
"#####;

fn splitter_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SPLITTER_MELPA_PIN, "splitter.el")
        .expect("prepare pinned Splitter source below ./tmp")
        .with_prelude(SPLITTER_TEST_PRELUDE)
        .with_timeout(SPLITTER_TEST_TIMEOUT)
}

fn documented_split_sizes_and_multi_split_validation_drive_real_windows() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (let (fractional absolute negative multiple validation)
    (setq fractional
          (save-window-excursion
            (delete-other-windows)
            (let* ((root (selected-window))
                   (before (list (window-total-width root)
                                 (window-total-height root)))
                   (windows (spl-split 'v .65 root)))
              (list before
                    (mapcar #'window-total-height windows)
                    (mapcar #'window-total-width windows)
                    (eq (selected-window) root)))))
    (setq absolute
          (save-window-excursion
            (delete-other-windows)
            (let ((windows (spl-split 'h 30 (selected-window))))
              (list (mapcar #'window-total-width windows)
                    (mapcar #'window-total-height windows)
                    :geometry
                    (list
                     (frame-width)
                     (frame-height)
                     (frame-char-width)
                     (frame-char-height)
                     (window-total-height (frame-root-window))
                     (window-total-height (minibuffer-window))
                     (window-pixel-edges (frame-root-window))
                     (mapcar #'window-pixel-edges windows)
                     (window-pixel-edges (minibuffer-window)))))))
    (setq negative
          (save-window-excursion
            (delete-other-windows)
            (condition-case error
                (let ((windows (spl-split 'h -20 (selected-window))))
                  (list :windows
                        (mapcar #'window-total-width windows)
                        (mapcar #'window-total-height windows)))
              (error (list :error (car error) (cadr error))))))
    (setq multiple
          (save-window-excursion
            (delete-other-windows)
            (let ((windows
                   (spl-split* 'h 3 '(.25 .5) (selected-window))))
              (list (mapcar #'window-total-width windows)
                    (mapcar #'window-total-height windows)
                    (length windows)))))
    (setq validation
          (mapcar
           (lambda (arguments)
             (condition-case error
                 (apply #'spl-verify-split*-sizes arguments)
               (error (list (car error) (cadr error)))))
           '((3 (.25 .5))
             (3 (.2))
             (2 (.7 .6))
             (2 (.2 .2))
             (2 (1.2)))))
    (list
     :fractional fractional
     :absolute absolute
     :documented-negative negative
     :multiple multiple
     :validation validation)))
"#####;
    let expect = expect![[
        r####"OK (:fractional ((80 24) (16 8) (80 80) t) :absolute ((31 49) (23 23) :geometry (80 25 1 1 23 1 (0 1 80 24) ((0 1 31 24) (31 1 80 24)) (0 24 80 25))) :documented-negative (:windows (70 10) (23 23)) :multiple ((21 40 19) (23 23 23) 3) :validation ((0.25 0.5 0.25) (error "wrong number of sizes given. num sizes = 1, num windows = 3.") (error "sum of sizes (1.300000) too high. should be 1.") (error "sum of sizes (0.400000) too low. should be 1.") (error "invalid size given: 1.200000. should be between 0 and 1.")))"####
    ]];
    ParityBatchCase::value(
        "documented_split_sizes_and_multi_split_validation_drive_real_windows",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn grid_assigns_buffers_in_visual_order_with_deterministic_fallbacks() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (splitter-parity-kill-buffers)
  (let* ((a (get-buffer-create "*splitter parity grid api*"))
         (b (get-buffer-create "*splitter parity grid logs*"))
         (c (get-buffer-create "*splitter parity grid queue*"))
         (default (get-buffer-create "*splitter parity grid fallback*"))
         result)
    (unwind-protect
        (setq result
              (save-window-excursion
                (delete-other-windows)
                (spl-grid
                 2 3
                 (list a "*splitter parity missing*" b c)
                 default)
                (let* ((windows (spl-sorted-window-list))
                       (box (spl-windows-bounding-box windows)))
                  (list
                   :shape (splitter-parity-window-shape windows)
                   :selected-first (eq (selected-window) (car windows))
                   :count (length windows)
                   :row-partition
                   (mapcar #'length
                           (spl-partition-windows-along-edge
                            windows 'v
                            (spl-bottom-edge (car windows))))
                   :box-size
                   (list (- (nth 2 box) (nth 0 box))
                         (- (nth 3 box) (nth 1 box)))))))
      (splitter-parity-kill-buffers))
    result))
"#####;
    let expect = expect![[
        r####"OK (:shape (("*splitter parity grid api*" 27 12) ("*splitter parity grid fallback*" 27 12) ("*splitter parity grid logs*" 26 12) ("*splitter parity grid queue*" 27 12) ("*splitter parity grid fallback*" 27 12) ("*splitter parity grid fallback*" 26 12)) :selected-first t :count 6 :row-partition (3 3) :box-size (80 24))"####
    ]];
    ParityBatchCase::value(
        "grid_assigns_buffers_in_visual_order_with_deterministic_fallbacks",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn nested_flex_layout_places_project_buffers_and_recovers_its_tree() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (splitter-parity-kill-buffers)
  (let* ((buffers
          (mapcar #'get-buffer-create
                  '("*splitter parity source*"
                    "*splitter parity tests*"
                    "*splitter parity repl*"
                    "*splitter parity diagnostics*")))
         (fallback (get-buffer-create "*splitter parity fallback*"))
         (layout '(h .4 (v .6 nil nil) (v .35 nil nil)))
         result)
    (unwind-protect
        (setq result
              (save-window-excursion
                (spl-flex-layout layout buffers fallback)
                (let ((windows (spl-sorted-window-list)))
                  (list
                   :shape (splitter-parity-window-shape windows)
                   :tree (spl-determine-window-layout-recursive windows)
                   :selected (buffer-name (window-buffer (selected-window)))
                   :bounding-box (spl-windows-bounding-box windows)))))
      (splitter-parity-kill-buffers))
    result))
"#####;
    let expect = expect![[
        r####"OK (:shape (("*splitter parity source*" 33 14) ("*splitter parity tests*" 47 8) ("*splitter parity repl*" 47 16) ("*splitter parity diagnostics*" 33 10)) :tree (h 0.4125 (v 0.5833333333333334 nil nil) (v 0.3333333333333333 nil nil)) :selected "*splitter parity source*" :bounding-box (0 0 80 24))"####
    ]];
    ParityBatchCase::value(
        "nested_flex_layout_places_project_buffers_and_recovers_its_tree",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn captured_layout_round_trip_restores_buffers_scroll_positions_and_geometry() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (splitter-parity-kill-buffers)
  (let* ((buffers
          (mapcar
           (lambda (name)
             (let ((buffer (get-buffer-create name)))
               (with-current-buffer buffer
                 (erase-buffer)
                 (dotimes (line 80)
                   (insert (format "line-%02d %s\n" line
                                   (make-string 120 (+ ?a (% line 20)))))))
               buffer))
           '("*splitter parity roundtrip source*"
             "*splitter parity roundtrip tests*"
             "*splitter parity roundtrip repl*")))
         (fallback (get-buffer-create "*splitter parity roundtrip fallback*"))
         result)
    (unwind-protect
        (setq result
              (save-window-excursion
                (spl-flex-layout '(v .55 (h .45 nil nil) nil)
                                 buffers fallback)
                (cl-loop
                 for window in (spl-sorted-window-list)
                 for line in '(4 17 39)
                 for hscroll in '(2 5 9)
                 do
                 (with-current-buffer (window-buffer window)
                   (goto-char (point-min))
                   (forward-line line)
                   (set-window-start window (point)))
                 (set-window-hscroll window hscroll))
                (let* ((captured (spl-determine-window-layout))
                       (captured-data
                        (mapcar
                         (lambda (entry)
                           (list (buffer-name (first entry))
                                 (with-current-buffer (first entry)
                                   (line-number-at-pos (second entry)))
                                 (third entry)))
                         (second captured))))
                  (delete-other-windows)
                  (set-window-buffer (selected-window) fallback)
                  (spl-apply-window-layout captured (selected-window))
                  (list
                   :tree (first captured)
                   :captured captured-data
                   :restored
                   (mapcar
                    (lambda (window)
                      (list
                       (buffer-name (window-buffer window))
                       (with-current-buffer (window-buffer window)
                         (line-number-at-pos (window-start window)))
                       (window-hscroll window)
                       (window-total-width window)
                       (window-total-height window)))
                    (spl-sorted-window-list))))))
      (splitter-parity-kill-buffers))
    result))
"#####;
    let expect = expect![[
        r####"OK (:tree (v 0.5416666666666666 (h 0.4625 nil nil) nil) :captured (("*splitter parity roundtrip source*" 5 2) ("*splitter parity roundtrip tests*" 18 5) ("*splitter parity roundtrip repl*" 40 9)) :restored (("*splitter parity roundtrip source*" 5 2 38 13) ("*splitter parity roundtrip tests*" 18 5 42 13) ("*splitter parity roundtrip repl*" 40 9 80 11)))"####
    ]];
    ParityBatchCase::value(
        "captured_layout_round_trip_restores_buffers_scroll_positions_and_geometry",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn shrinking_complex_layout_creates_room_without_losing_existing_panes() -> ParityBatchCase {
    let elisp_form = r#####"
(progn
  (splitter-parity-kill-buffers)
  (let* ((buffers
          (mapcar #'get-buffer-create
                  '("*splitter parity shrink source*"
                    "*splitter parity shrink tests*"
                    "*splitter parity shrink repl*"
                    "*splitter parity shrink logs*")))
         (room (get-buffer-create "*splitter parity shrink new terminal*"))
         (fallback (get-buffer-create "*splitter parity shrink fallback*"))
         result)
    (unwind-protect
        (setq result
              (save-window-excursion
                (spl-flex-layout '(h .5 (v .5 nil nil) (v .5 nil nil))
                                 buffers fallback)
                (let ((new-window (spl-shrink-window-layout 'h .7)))
                  (set-window-buffer new-window room)
                  (let* ((windows (spl-sorted-window-list))
                         (preserved (delq new-window (copy-sequence windows))))
                    (list
                     :count (length windows)
                     :shape (splitter-parity-window-shape windows)
                     :new-window
                     (list (buffer-name (window-buffer new-window))
                           (window-total-width new-window)
                           (window-total-height new-window))
                     :preserved-tree
                     (spl-determine-window-layout-recursive preserved)
                     :preserved-box
                     (spl-windows-bounding-box preserved))))))
      (splitter-parity-kill-buffers))
    result))
"#####;
    let expect = expect![[
        r####"OK (:count 5 :shape (("*splitter parity shrink source*" 29 12) ("*splitter parity shrink tests*" 27 12) ("*splitter parity shrink new terminal*" 24 24) ("*splitter parity shrink repl*" 29 12) ("*splitter parity shrink logs*" 27 12)) :new-window ("*splitter parity shrink new terminal*" 24 24) :preserved-tree (v 0.5 (h 0.5178571428571429 nil nil) (h 0.5178571428571429 nil nil)) :preserved-box (0 0 56 24))"####
    ]];
    ParityBatchCase::value(
        "shrinking_complex_layout_creates_room_without_losing_existing_panes",
        elisp_form,
        expect,
    )
    .fresh_process()
}

#[test]
fn splitter_package_batch() {
    let cases = vec![
        documented_split_sizes_and_multi_split_validation_drive_real_windows(),
        grid_assigns_buffers_in_visual_order_with_deterministic_fallbacks(),
        nested_flex_layout_places_project_buffers_and_recovers_its_tree(),
        captured_layout_round_trip_restores_buffers_scroll_positions_and_geometry(),
        shrinking_complex_layout_creates_room_without_losing_existing_panes(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Splitter parity test");
    assert_oracle_batch_cases(splitter_oracle(), test_name, "splitter_parity", &cases);
}

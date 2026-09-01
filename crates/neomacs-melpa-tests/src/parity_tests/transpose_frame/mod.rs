//! Practical parity for Transpose Frame's public window-arrangement commands.
//!
//! These cases build the documented four-window layout, transpose/flip/flop/
//! rotate it through the interactive commands, preserve point and dedicated
//! windows, and recover after a single-window identity and a dead-frame error.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, TRANSPOSE_FRAME_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'transpose-frame)
(set-window-configuration (current-window-configuration))

(defconst tf426-test-tree
  "1dea4af1ffd3cd208253b76d0787b7dfd7b58e4e")
(defconst tf426-test-manifest
  '(("transpose-frame-pkg.el" . "170f740b6c95c0e3da60e35d0cf27e0773db6da048088f19151536c1a0aaf7c7")
    ("transpose-frame.el" . "86cdda95cf897c64ccba5f25ff1357657969421315b18a2b33b1d99de21e7c70")))

(defvar tf426-test-case-index 0)
(defvar tf426-test-root nil)
(defvar tf426-test-root-owned nil)

(defun tf426-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun tf426-test-source-state ()
  (let* ((located (symbol-file 'transpose-frame 'defun))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main)))
         (files
          (and directory
               (sort
                (mapcar (lambda (file) (file-relative-name file directory))
                        (seq-filter
                         (lambda (file)
                           (and (string-suffix-p ".el" file)
                                (not (string-suffix-p "-autoloads.el" file))))
                         (directory-files-recursively directory "\\.el\\'")))
                #'string<)))
         (manifest
          (and files
               (mapcar (lambda (file)
                         (cons file (tf426-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/transpose-frame.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car tf426-test-manifest)))
      (error "Unexpected installed Transpose Frame payload: %S"
             (or manifest files)))
    (dolist (entry tf426-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (tf426-test-sha file) (cdr entry)))
          (error "Unexpected installed Transpose Frame source: %S"
                 (cons entry manifest)))))
    (list :tree tf426-test-tree
          :manifest tf426-test-manifest
          :feature (featurep 'transpose-frame)
          :version "20221109.2053")))

(defun tf426-test-window-state ()
  (mapcar
   (lambda (window)
     (list window
           (eq window (selected-window))
           (window-buffer window)
           (window-point window)
           (window-start window)
           (window-hscroll window)
           (window-dedicated-p window)
           (window-edges window)))
   (seq-mapcat (lambda (frame) (window-list frame 'nomini)) (frame-list))))

(defun tf426-test-condition (thunk)
  (condition-case condition
      (list :returned (funcall thunk))
    (error
     (list :error (car condition)
           :data (mapcar (lambda (item)
                           (if (stringp item)
                               (copy-sequence item)
                             (copy-tree item)))
                         (cdr condition))
           :message (copy-sequence (error-message-string condition))))))

(defun tf426-test-forbid-external (operation &rest arguments)
  (error "Unexpected Transpose Frame external boundary: %S %S"
         operation arguments))

(defun tf426-test-walk (node selected)
  (if (windowp node)
      (list :leaf (copy-sequence (buffer-name (window-buffer node)))
            :point (window-point node)
            :start (window-start node)
            :hscroll (window-hscroll node)
            :dedicated (and (window-dedicated-p node) t)
            :selected (eq node selected))
    (list :dir (if (car node) 'vertical 'horizontal)
          :kids (mapcar (lambda (child)
                          (tf426-test-walk child selected))
                        (cddr node)))))

(defun tf426-test-layout ()
  (tf426-test-walk (car (window-tree)) (frame-selected-window)))

(defun tf426-test-shape (node)
  (if (plist-get node :leaf)
      (list :leaf (plist-get node :leaf)
            :dedicated (plist-get node :dedicated)
            :selected (plist-get node :selected))
    (list :dir (plist-get node :dir)
          :kids (mapcar #'tf426-test-shape (plist-get node :kids)))))

(defun tf426-test-make-buffer (name text point)
  (let ((buffer (get-buffer-create name)))
    (with-current-buffer buffer
      (erase-buffer)
      (insert text)
      (goto-char point)
      (setq buffer-undo-list t))
    buffer))

(defun tf426-test-build-documented-layout ()
  "Build the commentary's A|(B/C) over D layout and select A."
  (let ((alpha (tf426-test-make-buffer "*tf426 A café*" "alpha café\nA-line-2\n" 7))
        (bravo (tf426-test-make-buffer "*tf426 B 界*" "bravo 界\nB-line-2\n" 7))
        (charlie (tf426-test-make-buffer "*tf426 C*" "charlie\nC-line-2\n" 1))
        (delta (tf426-test-make-buffer "*tf426 D ledger*" "delta ledger\nD-line-2\n" 13)))
    (delete-other-windows)
    (switch-to-buffer alpha)
    (set-window-point (selected-window) 7)
    (select-window (split-window (selected-window) nil 'below))
    (switch-to-buffer delta)
    (set-window-point (selected-window) 13)
    (select-window (get-buffer-window alpha))
    (select-window (split-window (selected-window) nil 'right))
    (switch-to-buffer bravo)
    (set-window-point (selected-window) 7)
    (select-window (split-window (selected-window) nil 'below))
    (switch-to-buffer charlie)
    (set-window-point (selected-window) 1)
    (select-window (get-buffer-window alpha))
    (tf426-test-layout)))

(defun tf426-test-run (body)
  (let* ((index (cl-incf tf426-test-case-index))
         (sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (format "transpose-frame-%d" index)
                                       sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (selected-window-before (selected-window))
         (window-before (current-window-configuration))
         (window-state-before (tf426-test-window-state))
         (source-before (tf426-test-source-state))
         (directory-before default-directory)
         (enable-local-before enable-local-variables)
         (debug-before debug-on-error)
         (print-circle-before print-circle)
         (tf426-test-root root)
         (tf426-test-root-owned nil)
         result body-error source-after cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
          (unless (and root (file-name-absolute-p root))
            (error "Missing absolute Transpose Frame sandbox root"))
          (when (file-exists-p root)
            (error "Transpose Frame sandbox root exists: %S" root))
          (make-directory root)
          (setq tf426-test-root-owned t
                enable-local-variables nil
                debug-on-error nil
                print-circle nil
                default-directory root)
          (cl-letf (((symbol-function 'call-process)
                     (lambda (&rest args)
                       (apply #'tf426-test-forbid-external 'call-process args)))
                    ((symbol-function 'call-process-region)
                     (lambda (&rest args)
                       (apply #'tf426-test-forbid-external
                              'call-process-region args)))
                    ((symbol-function 'process-file)
                     (lambda (&rest args)
                       (apply #'tf426-test-forbid-external 'process-file args)))
                    ((symbol-function 'start-process)
                     (lambda (&rest args)
                       (apply #'tf426-test-forbid-external 'start-process args)))
                    ((symbol-function 'start-file-process)
                     (lambda (&rest args)
                       (apply #'tf426-test-forbid-external
                              'start-file-process args)))
                    ((symbol-function 'make-process)
                     (lambda (&rest args)
                       (apply #'tf426-test-forbid-external 'make-process args)))
                    ((symbol-function 'make-network-process)
                     (lambda (&rest args)
                       (apply #'tf426-test-forbid-external
                              'make-network-process args)))
                    ((symbol-function 'url-retrieve)
                     (lambda (&rest args)
                       (apply #'tf426-test-forbid-external 'url-retrieve args)))
                    ((symbol-function 'url-retrieve-synchronously)
                     (lambda (&rest args)
                       (apply #'tf426-test-forbid-external
                              'url-retrieve-synchronously args)))
                    ((symbol-function 'kill-emacs)
                     (lambda (&rest args)
                       (apply #'tf426-test-forbid-external 'kill-emacs args))))
            (setq result (funcall body)))
          (setq source-after (tf426-test-source-state))
          (unless (equal source-before source-after)
            (error "Transpose Frame source changed")))
          (t (setq body-error
                   (list :error (car condition)
                         :data (copy-tree (cdr condition))
                         :message (error-message-string condition)))))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error
                (push (list label (car condition) (copy-tree (cdr condition)))
                      cleanup-errors)))))
        (setq default-directory directory-before
              enable-local-variables enable-local-before
              debug-on-error debug-before
              print-circle print-circle-before)
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer (buffer-list))
          (unless (memq buffer buffers-before)
            (attempt (list 'buffer (buffer-name buffer))
                     (lambda ()
                       (with-current-buffer buffer
                         (let ((kill-buffer-hook nil)
                               (kill-buffer-query-functions nil))
                           (set-buffer-modified-p nil)
                           (kill-buffer buffer)))))))
        (dolist (timer (append timer-list timer-idle-list))
          (unless (memq timer timers-before)
            (attempt 'timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'frame (lambda () (delete-frame frame t)))))
        (attempt 'window (lambda () (set-window-configuration window-before)))
        (when (buffer-live-p buffer-before)
          (attempt 'current-buffer (lambda () (set-buffer buffer-before))))
        (when tf426-test-root-owned
          (attempt 'sandbox (lambda () (delete-directory root t))))))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :new-buffers
                 (mapcar #'buffer-name
                         (seq-remove (lambda (buffer) (memq buffer buffers-before))
                                     (buffer-list)))
                 :new-processes
                 (length (seq-remove (lambda (process)
                                       (memq process processes-before))
                                     (process-list)))
                 :new-timers
                 (length (seq-remove (lambda (timer) (memq timer timers-before))
                                     (append timer-list timer-idle-list)))
                 :new-frames
                 (length (seq-remove (lambda (frame) (memq frame frames-before))
                                     (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :window-restored
                 (and (eq (selected-window) selected-window-before)
                      (equal (tf426-test-window-state) window-state-before))
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Transpose Frame workflow failed: %S" (list result cleanup))
        (list :source source-before :result result :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(TRANSPOSE_FRAME_MELPA_PIN, "transpose-frame.el")
        .expect("prepare exact Transpose Frame source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_transpose_swaps_split_axes_and_keeps_selection() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_transpose_swaps_split_axes_and_keeps_selection",
        r####"
(tf426-test-run
 (lambda ()
   (let ((before (tf426-test-build-documented-layout)))
     (transpose-frame)
     (list :before before
           :after (tf426-test-layout)
           :selected (copy-sequence (buffer-name))))))
"####,
        expect![[
            r#"OK (:source (:tree "1dea4af1ffd3cd208253b76d0787b7dfd7b58e4e" :manifest (("transpose-frame-pkg.el" . "170f740b6c95c0e3da60e35d0cf27e0773db6da048088f19151536c1a0aaf7c7") ("transpose-frame.el" . "86cdda95cf897c64ccba5f25ff1357657969421315b18a2b33b1d99de21e7c70")) :feature t :version "20221109.2053") :result (:before (:dir vertical :kids ((:dir horizontal :kids ((:leaf "*tf426 A café*" :point 7 :start 1 :hscroll 0 :dedicated nil :selected t) (:dir vertical :kids ((:leaf "*tf426 B 界*" :point 7 :start 1 :hscroll 0 :dedicated nil :selected nil) (:leaf "*tf426 C*" :point 1 :start 1 :hscroll 0 :dedicated nil :selected nil))))) (:leaf "*tf426 D ledger*" :point 13 :start 1 :hscroll 0 :dedicated nil :selected nil))) :after (:dir horizontal :kids ((:dir vertical :kids ((:leaf "*tf426 A café*" :point 7 :start 1 :hscroll 0 :dedicated nil :selected t) (:dir horizontal :kids ((:leaf "*tf426 B 界*" :point 7 :start 1 :hscroll 0 :dedicated nil :selected nil) (:leaf "*tf426 C*" :point 1 :start 1 :hscroll 0 :dedicated nil :selected nil))))) (:leaf "*tf426 D ledger*" :point 13 :start 1 :hscroll 0 :dedicated nil :selected nil))) :selected "*tf426 A café*") :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_flip_and_flop_mirror_the_documented_layout() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_flip_and_flop_mirror_the_documented_layout",
        r####"
(tf426-test-run
 (lambda ()
   (tf426-test-build-documented-layout)
   (flip-frame)
   (let ((flipped (tf426-test-layout)))
     (tf426-test-build-documented-layout)
     (flop-frame)
     (list :flipped flipped
           :flopped (tf426-test-shape (tf426-test-layout))))))
"####,
        expect![[
            r#"OK (:source (:tree "1dea4af1ffd3cd208253b76d0787b7dfd7b58e4e" :manifest (("transpose-frame-pkg.el" . "170f740b6c95c0e3da60e35d0cf27e0773db6da048088f19151536c1a0aaf7c7") ("transpose-frame.el" . "86cdda95cf897c64ccba5f25ff1357657969421315b18a2b33b1d99de21e7c70")) :feature t :version "20221109.2053") :result (:flipped (:dir vertical :kids ((:leaf "*tf426 D ledger*" :point 13 :start 1 :hscroll 0 :dedicated nil :selected nil) (:dir horizontal :kids ((:leaf "*tf426 A café*" :point 7 :start 1 :hscroll 0 :dedicated nil :selected t) (:dir vertical :kids ((:leaf "*tf426 C*" :point 1 :start 1 :hscroll 0 :dedicated nil :selected nil) (:leaf "*tf426 B 界*" :point 7 :start 1 :hscroll 0 :dedicated nil :selected nil))))))) :flopped (:dir vertical :kids ((:dir horizontal :kids ((:dir vertical :kids ((:leaf "*tf426 B 界*" :dedicated nil :selected nil) (:leaf "*tf426 C*" :dedicated nil :selected nil))) (:leaf "*tf426 A café*" :dedicated nil :selected t))) (:leaf "*tf426 D ledger*" :dedicated nil :selected nil)))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_rotate_180_clockwise_then_anticlockwise_recovers() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_rotate_180_clockwise_then_anticlockwise_recovers",
        r####"
(tf426-test-run
 (lambda ()
   (let ((original (tf426-test-build-documented-layout)))
     (rotate-frame)
     (let ((turned (tf426-test-layout)))
       (rotate-frame-clockwise)
       (let ((clockwise (tf426-test-layout)))
         (rotate-frame-anticlockwise)
         (list :original original
               :turned turned
               :clockwise clockwise
               :recovered (tf426-test-layout)
               :same-as-turned (equal (tf426-test-layout) turned)))))))
"####,
        expect![[
            r#"OK (:source (:tree "1dea4af1ffd3cd208253b76d0787b7dfd7b58e4e" :manifest (("transpose-frame-pkg.el" . "170f740b6c95c0e3da60e35d0cf27e0773db6da048088f19151536c1a0aaf7c7") ("transpose-frame.el" . "86cdda95cf897c64ccba5f25ff1357657969421315b18a2b33b1d99de21e7c70")) :feature t :version "20221109.2053") :result (:original (:dir vertical :kids ((:dir horizontal :kids ((:leaf "*tf426 A café*" :point 7 :start 1 :hscroll 0 :dedicated nil :selected t) (:dir vertical :kids ((:leaf "*tf426 B 界*" :point 7 :start 1 :hscroll 0 :dedicated nil :selected nil) (:leaf "*tf426 C*" :point 1 :start 1 :hscroll 0 :dedicated nil :selected nil))))) (:leaf "*tf426 D ledger*" :point 13 :start 1 :hscroll 0 :dedicated nil :selected nil))) :turned (:dir vertical :kids ((:leaf "*tf426 D ledger*" :point 13 :start 1 :hscroll 0 :dedicated nil :selected nil) (:dir horizontal :kids ((:dir vertical :kids ((:leaf "*tf426 C*" :point 1 :start 1 :hscroll 0 :dedicated nil :selected nil) (:leaf "*tf426 B 界*" :point 7 :start 1 :hscroll 0 :dedicated nil :selected nil))) (:leaf "*tf426 A café*" :point 7 :start 1 :hscroll 0 :dedicated nil :selected t))))) :clockwise (:dir horizontal :kids ((:dir vertical :kids ((:dir horizontal :kids ((:leaf "*tf426 B 界*" :point 7 :start 1 :hscroll 0 :dedicated nil :selected nil) (:leaf "*tf426 C*" :point 1 :start 1 :hscroll 0 :dedicated nil :selected nil))) (:leaf "*tf426 A café*" :point 7 :start 1 :hscroll 0 :dedicated nil :selected t))) (:leaf "*tf426 D ledger*" :point 13 :start 1 :hscroll 0 :dedicated nil :selected nil))) :recovered (:dir vertical :kids ((:leaf "*tf426 D ledger*" :point 13 :start 1 :hscroll 0 :dedicated nil :selected nil) (:dir horizontal :kids ((:dir vertical :kids ((:leaf "*tf426 C*" :point 1 :start 1 :hscroll 0 :dedicated nil :selected nil) (:leaf "*tf426 B 界*" :point 7 :start 1 :hscroll 0 :dedicated nil :selected nil))) (:leaf "*tf426 A café*" :point 7 :start 1 :hscroll 0 :dedicated nil :selected t))))) :same-as-turned t) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_dedicated_overlay_single_window_and_dead_frame() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_dedicated_overlay_single_window_and_dead_frame",
        r####"
(tf426-test-run
 (lambda ()
   (tf426-test-build-documented-layout)
   (let* ((delta (get-buffer-window "*tf426 D ledger*"))
          (overlay
           (with-current-buffer "*tf426 D ledger*"
             (let ((ol (make-overlay (point-min) (point-max))))
               (overlay-put ol 'window delta)
               (overlay-put ol 'tf426-mark 'ledger)
               ol))))
     (set-window-dedicated-p delta t)
     (transpose-frame)
     (let* ((after (tf426-test-layout))
            (host (get-buffer-window "*tf426 D ledger*"))
            (overlay-window (overlay-get overlay 'window))
            (dedicated (and host (window-live-p host)
                            (window-dedicated-p host) t))
            (overlay-follows (and host (eq overlay-window host)))
            (single
             (progn
               (delete-other-windows)
               (switch-to-buffer "*tf426 A café*")
               (let ((before (tf426-test-layout)))
                 (transpose-frame)
                 (list :before before :after (tf426-test-layout)))))
            (dead
             (tf426-test-condition
              (lambda () (transpose-frame 'not-a-frame)))))
       (list :after after
             :dedicated dedicated
             :overlay-follows overlay-follows
             :single single
             :dead dead)))))
"####,
        expect![[
            r#"OK (:source (:tree "1dea4af1ffd3cd208253b76d0787b7dfd7b58e4e" :manifest (("transpose-frame-pkg.el" . "170f740b6c95c0e3da60e35d0cf27e0773db6da048088f19151536c1a0aaf7c7") ("transpose-frame.el" . "86cdda95cf897c64ccba5f25ff1357657969421315b18a2b33b1d99de21e7c70")) :feature t :version "20221109.2053") :result (:after (:dir horizontal :kids ((:dir vertical :kids ((:leaf "*tf426 A café*" :point 7 :start 1 :hscroll 0 :dedicated nil :selected t) (:dir horizontal :kids ((:leaf "*tf426 B 界*" :point 7 :start 1 :hscroll 0 :dedicated nil :selected nil) (:leaf "*tf426 C*" :point 1 :start 1 :hscroll 0 :dedicated nil :selected nil))))) (:leaf "*tf426 D ledger*" :point 13 :start 1 :hscroll 0 :dedicated t :selected nil))) :dedicated t :overlay-follows t :single (:before (:leaf "*tf426 A café*" :point 7 :start 1 :hscroll 0 :dedicated nil :selected t) :after (:leaf "*tf426 A café*" :point 7 :start 1 :hscroll 0 :dedicated nil :selected t)) :dead (:error error :data ("not-a-frame is not a live frame") :message "not-a-frame is not a live frame")) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn transpose_frame_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_transpose_swaps_split_axes_and_keeps_selection(),
        public_flip_and_flop_mirror_the_documented_layout(),
        public_rotate_180_clockwise_then_anticlockwise_recovers(),
        public_dedicated_overlay_single_window_and_dead_frame(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "transpose-frame-rank426",
        "transpose_frame_parity",
        &cases,
    );
}

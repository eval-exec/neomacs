use std::time::Duration;

use expect_test::expect;

use super::support::PackageGuiPair;
use crate::{CachedMelpaOracle, GIT_GUTTER_FRINGE_MELPA_PIN};

const GUI_PROBE: &str = r####"
(progn
  (require 'cl-lib)
  (require 'seq)
  (require 'git-gutter-fringe)

  (defun ggf351-gui-write (file contents)
    (make-directory (file-name-directory file) t)
    (let ((coding-system-for-write 'utf-8-unix))
      (with-temp-file file (insert contents))))

  (defun ggf351-gui-git (git project &rest arguments)
    (let ((default-directory (file-name-as-directory project)))
      (with-temp-buffer
        (let ((status (apply #'process-file git nil (list t t) nil arguments)))
          (unless (zerop status)
            (error "GGF351-GUI: git %S failed (%s): %s"
                   arguments status (buffer-string)))))))

  (defun ggf351-gui-wait (expected-hunks owned-state)
    (let* ((process-buffer
            (git-gutter:diff-process-buffer (git-gutter:base-file)))
           (buffer (get-buffer process-buffer))
           (process (and buffer (get-buffer-process buffer)))
           (deadline (+ (float-time) 20)))
      (unless (and buffer process)
        (error "GGF351-GUI: public refresh exposed no exact Git process: %S"
               process-buffer))
      (aset owned-state 0 (cons process (aref owned-state 0)))
      (aset owned-state 1 (cons buffer (aref owned-state 1)))
      (while (and (< (float-time) deadline)
                  (or (get-buffer process-buffer)
                      (not git-gutter:enabled)
                      (/= (length git-gutter:diffinfos) expected-hunks)))
        (accept-process-output nil 0.02))
      (when (or (get-buffer process-buffer)
                (not git-gutter:enabled)
                (/= (length git-gutter:diffinfos) expected-hunks))
        (error "GGF351-GUI: refresh did not settle: %S"
               (list :process (get-buffer process-buffer)
                     :enabled git-gutter:enabled
                     :hunks (length git-gutter:diffinfos)
                     :expected expected-hunks)))))

  (defun ggf351-gui-row-display ()
    "Return the exact package display spec attached to the current row."
    (let ((position (line-beginning-position)))
      (seq-some
       (lambda (overlay)
         (let* ((before (overlay-get overlay 'before-string))
                (display (and before (get-text-property 0 'display before))))
           (and (consp display)
                (memq (car display) '(left-fringe right-fringe))
                display)))
       (delete-dups
        (append
         (overlays-at position)
         (overlays-in position (min (point-max) (1+ position))))))))

  (defun ggf351-gui-source-bitmaps ()
    "Read exact source bitmap forms and report their geometry and vectors."
    (let ((source (getenv "NEOMACS_PACKAGE_SOURCE")) forms)
      (with-temp-buffer
        (insert-file-contents-literally source)
        (goto-char (point-min))
        (condition-case nil
            (while t
              (let ((form (read (current-buffer))))
                (when (eq (car-safe form) 'fringe-helper-define)
                  (let* ((name (cadr (cadr form)))
                         (alignment (nth 2 form))
                         (rows (cdddr form))
                         (vector
                          (apply
                           #'vector
                           (mapcar
                            (lambda (row)
                              (let ((bits 0))
                                (dolist (character (string-to-list row) bits)
                                  (setq bits
                                        (+ (* bits 2)
                                           (if (eq character ?.) 0 1))))))
                            rows))))
                    (push (list name :alignment alignment
                                :width (length (car rows))
                                :height (length rows)
                                :vector vector)
                          forms)))))
          (end-of-file nil)))
      (list :installed-source-sha256
            (with-temp-buffer
              (insert-file-contents-literally source)
              (secure-hash 'sha256 (current-buffer)))
            :definitions (nreverse forms))))

  (defun ggf351-gui-rows ()
    (redisplay t)
    (cl-loop
     for line from 1 to (line-number-at-pos (point-max))
     collect
     (save-excursion
       (goto-char (point-min))
       (forward-line (1- line))
       (let* ((bitmaps (fringe-bitmaps-at-pos (point) (selected-window)))
              (display (ggf351-gui-row-display))
              (face (nth 2 display)))
         (list line :bitmaps bitmaps :display display
               (and face
                    (list :resolved-face face
                          :inherit (face-attribute face :inherit nil t)
                          :foreground
                          (face-attribute face :foreground nil t))))))))

  (defun ggf351-gui-owned-overlays ()
    (seq-filter
     (lambda (overlay)
       (or (overlay-get overlay 'git-gutter)
           (let ((parent (overlay-get overlay 'fringe-helper-parent)))
             (and parent (memq parent git-gutter-fr:bitmap-references)))))
     (apply #'append (overlay-lists))))

  (defun ggf351-gui-clean-processes (processes buffers)
    (dolist (process processes)
      (when (process-live-p process) (delete-process process)))
    (let ((deadline (+ (float-time) 5)))
      (while (and (< (float-time) deadline)
                  (seq-some #'process-live-p processes))
        (accept-process-output nil 0.02)))
    (dolist (buffer buffers)
      (when (buffer-live-p buffer) (kill-buffer buffer))))

  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (workspace (getenv "NEOMACS_TEST_WORKSPACE_ROOT"))
         (approved (and workspace
                        (file-name-as-directory
                         (expand-file-name "tmp" workspace))))
         (root (and sandbox
                    (expand-file-name "git-gutter-fringe-gui" sandbox)))
         (real-git (or (executable-find "git" t)
                       (error "GGF351-GUI: real Git is required")))
         (process-environment (copy-sequence process-environment))
         (baseline-processes (process-list))
         (baseline-buffers (buffer-list))
         (baseline-timers (copy-sequence timer-list))
         (baseline-window (selected-window))
         (baseline-window-config (current-window-configuration))
         (baseline-margins (window-margins baseline-window))
         (baseline-fringes (window-fringes baseline-window))
         (original-buffer (current-buffer))
         (root-owned nil)
         (buffer nil)
         (owned-state (vector nil nil))
         (body-result nil)
         (body-error nil)
         (cleanup-errors nil)
         (final-mode nil)
         (final-enabled nil)
         (final-refs nil)
         (final-owned-overlays nil))
    (unless (and (stringp sandbox) (> (length sandbox) 1)
                 (file-name-absolute-p sandbox)
                 approved
                 (file-in-directory-p (file-truename sandbox)
                                      (file-truename approved)))
      (error "GGF351-GUI: unsafe sandbox root %S below %S" sandbox approved))
    (when (file-exists-p root)
      (error "GGF351-GUI: owned root already exists: %s" root))
    (setenv "LC_ALL" "C")
    (setenv "LANG" "C")
    (setenv "TZ" "UTC")
    (setenv "GIT_CONFIG_GLOBAL" "/dev/null")
    (setenv "GIT_CONFIG_NOSYSTEM" "1")
    (setenv "GIT_AUTHOR_NAME" "Fringe GUI Parity")
    (setenv "GIT_AUTHOR_EMAIL" "fringe-gui@example.invalid")
    (setenv "GIT_COMMITTER_NAME" "Fringe GUI Parity")
    (setenv "GIT_COMMITTER_EMAIL" "fringe-gui@example.invalid")
    (setenv "GIT_AUTHOR_DATE" "2024-02-03T04:05:06+0000")
    (setenv "GIT_COMMITTER_DATE" "2024-02-03T04:05:06+0000")
    (unwind-protect
        (condition-case error-data
            (let* ((project (file-name-as-directory
                             (expand-file-name "project space Ω" root)))
                   (file (expand-file-name "src/sample.txt" project))
                   (baseline
                    "alpha\nbeta\ngamma\ndelta\nepsilon\nzeta\neta\ntheta\niota\nkappa\n")
                   (changed
                    "alpha\nBETA changed\ngamma\ndelta\nzeta\neta\ntheta\nadded one\nadded two\niota\nkappa\n"))
              (make-directory root)
              (setq root-owned t)
              (ggf351-gui-write file baseline)
              (ggf351-gui-git real-git project
                              "init" "--quiet" "--initial-branch=main")
              (ggf351-gui-git real-git project
                              "config" "core.hooksPath" "/dev/null")
              (ggf351-gui-git real-git project "config" "user.name"
                              "Fringe GUI Parity")
              (ggf351-gui-git real-git project "config" "user.email"
                              "fringe-gui@example.invalid")
              (ggf351-gui-git real-git project "add" "src/sample.txt")
              (ggf351-gui-git real-git project "commit" "--quiet"
                              "--no-gpg-sign" "-m" "Baseline GUI fringe fixture")
              (ggf351-gui-write file changed)
              (let ((enable-local-variables nil)
                    (enable-local-eval nil))
                (setq buffer (find-file-noselect file)))
              (set-window-buffer baseline-window buffer)
              (select-window baseline-window)
              (with-current-buffer buffer
                (let ((git-gutter:update-interval 0)
                      (git-gutter:verbosity 0)
                      (git-gutter:handled-backends '(git))
                      (git-gutter-fr:side 'left-fringe))
                  (git-gutter-mode 1)
                  (ggf351-gui-wait 3 owned-state)
                  (let ((left
                         (list
                          :display
                          (list :graphic (display-graphic-p)
                                :window-system-live (and window-system t)
                                :frame-visible
                                (frame-visible-p (selected-frame))
                                :selected-window-live
                                (window-live-p (selected-window))
                                :selected-window-frame
                                (eq (window-frame (selected-window))
                                    (selected-frame))
                                :selected-window-buffer
                                (eq (window-buffer (selected-window))
                                    (current-buffer)))
                          :frame-fringes
                          (list (frame-parameter nil 'left-fringe)
                                (frame-parameter nil 'right-fringe))
                          :window-fringes (window-fringes)
                          :margins (window-margins)
                          :bitmap-registrations
                          (mapcar
                           (lambda (bitmap)
                             (list bitmap (and (fringe-bitmap-p bitmap) t)))
                           '(git-gutter-fr:added git-gutter-fr:modified
                             git-gutter-fr:deleted))
                          :bitmap-source (ggf351-gui-source-bitmaps)
                          :rows (ggf351-gui-rows))))
                    (git-gutter:toggle)
                    (let ((cleared
                           (list :mode git-gutter-mode
                                 :enabled git-gutter:enabled
                                 :refs git-gutter-fr:bitmap-references
                                 :overlays (length (ggf351-gui-owned-overlays))
                                 :rows (ggf351-gui-rows))))
                      (setq git-gutter-fr:side 'right-fringe)
                      (git-gutter:toggle)
                      (ggf351-gui-wait 3 owned-state)
                      (let ((right
                             (list :side git-gutter-fr:side
                                   :window-fringes (window-fringes)
                                   :margins (window-margins)
                                   :rows (ggf351-gui-rows))))
                        (git-gutter:toggle)
                        (setq body-result
                              (list
                               :source (file-name-nondirectory
                                        (symbol-file 'git-gutter-fr:init))
                               :real-git-absolute
                               (file-name-absolute-p real-git)
                               :geometry-baseline
                               (list baseline-margins baseline-fringes)
                               :left left :cleared cleared :right right
                               :final
                               (list :mode git-gutter-mode
                                     :enabled git-gutter:enabled
                                     :refs git-gutter-fr:bitmap-references
                                     :overlays
                                     (length (ggf351-gui-owned-overlays))
                                     :rows (ggf351-gui-rows))))))))))
          (error (setq body-error error-data)))
      (dolist
          (phase
           (list
            (cons 'mode
                  (lambda ()
                    (when (buffer-live-p buffer)
                      (with-current-buffer buffer
                        (when git-gutter-mode (git-gutter-mode -1))))))
            (cons 'processes
                  (lambda ()
                    (ggf351-gui-clean-processes
                     (aref owned-state 0) (aref owned-state 1))))
            (cons 'clear
                  (lambda ()
                    (when (buffer-live-p buffer)
                      (with-current-buffer buffer
                        (git-gutter-fr:clear)
                        (setq final-mode git-gutter-mode
                              final-enabled git-gutter:enabled
                              final-refs git-gutter-fr:bitmap-references
                              final-owned-overlays
                              (ggf351-gui-owned-overlays))))))
            (cons 'buffers
                  (lambda ()
                    (when (buffer-live-p (get-buffer git-gutter:popup-buffer))
                      (kill-buffer git-gutter:popup-buffer))
                    (when (buffer-live-p buffer)
                      (with-current-buffer buffer (set-buffer-modified-p nil))
                      (kill-buffer buffer))))
            (cons 'timers
                  (lambda ()
                    (dolist (timer (seq-remove
                                    (lambda (candidate)
                                      (memq candidate baseline-timers))
                                    timer-list))
                      (cancel-timer timer))))
            (cons 'window
                  (lambda ()
                    (set-window-configuration baseline-window-config)))
            (cons 'final-process-sweep
                  (lambda ()
                    (ggf351-gui-clean-processes
                     (aref owned-state 0) (aref owned-state 1))))
            (cons 'root
                  (lambda ()
                    (when (and root-owned (file-exists-p root))
                      (delete-directory root t))
                    (setq root-owned nil)))
            (cons 'coding-cache
                  (lambda ()
                    (let ((conversion (get-buffer " *code-conversion-work*")))
                      (when (and (buffer-live-p conversion)
                                 (not (memq conversion baseline-buffers)))
                        (kill-buffer conversion)))))))
        (condition-case cleanup-error
            (funcall (cdr phase))
          (error (push (list (car phase) cleanup-error) cleanup-errors)))))
    (let* ((new-buffers
            (mapcar #'buffer-name
                    (seq-remove (lambda (candidate)
                                  (memq candidate baseline-buffers))
                                (buffer-list))))
           (new-processes
            (mapcar #'process-name
                    (seq-remove (lambda (candidate)
                                  (memq candidate baseline-processes))
                                (process-list))))
           (new-timers
            (length (seq-remove (lambda (candidate)
                                  (memq candidate baseline-timers))
                                timer-list)))
           (cleanup
            (list :new-buffers new-buffers
                  :new-processes new-processes
                  :new-timers new-timers
                  :root-exists (file-exists-p root)
                  :root-owned root-owned
                  :mode final-mode
                  :enabled final-enabled
                  :refs final-refs
                  :owned-overlays final-owned-overlays
                  :processes-live
                  (seq-some #'process-live-p (aref owned-state 0))
                  :process-buffers-live
                  (seq-some #'buffer-live-p (aref owned-state 1))
                  :window-restored (eq (selected-window) baseline-window)
                  :margins-restored
                  (equal (window-margins baseline-window) baseline-margins)
                  :fringes-restored
                  (equal (window-fringes baseline-window) baseline-fringes)
                  :buffer-restored (eq (current-buffer) original-buffer)
                  :body-error body-error
                  :cleanup-errors (nreverse cleanup-errors))))
      (when (or body-error cleanup-errors new-buffers new-processes
                (/= new-timers 0) (file-exists-p root) root-owned
                final-mode final-enabled final-refs final-owned-overlays
                (seq-some #'process-live-p (aref owned-state 0))
                (seq-some #'buffer-live-p (aref owned-state 1))
                (not (eq (selected-window) baseline-window))
                (not (equal (window-margins baseline-window)
                            baseline-margins))
                (not (equal (window-fringes baseline-window)
                            baseline-fringes))
                (not (eq (current-buffer) original-buffer)))
        (error "GGF351-GUI: body/cleanup failure: %S" cleanup))
      (list :result body-result :cleanup cleanup))))
"####;

#[test]
fn git_gutter_fringe_real_gui_rows_match_gnu() {
    let oracle = CachedMelpaOracle::new(GIT_GUTTER_FRINGE_MELPA_PIN, "git-gutter-fringe.el")
        .expect("prepare exact shallow Git Gutter Fringe source below ./tmp")
        .with_timeout(Duration::from_secs(180));
    let pair = PackageGuiPair::run(
        "git-gutter-fringe-real-gui-rows",
        oracle.prepared_packages(),
        GUI_PROBE,
    )
    .expect("run real graphical GNU Emacs and Neomacs sequentially");

    let expected_gnu = expect![[
        r#"OK (:result (:source "git-gutter-fringe.el" :real-git-absolute t :geometry-baseline ((nil) (8 8 nil nil)) :left (:display (:graphic t :window-system-live t :frame-visible t :selected-window-live t :selected-window-frame t :selected-window-buffer t) :frame-fringes (8 8) :window-fringes (8 8 nil nil) :margins (nil) :bitmap-registrations ((git-gutter-fr:added t) (git-gutter-fr:modified t) (git-gutter-fr:deleted t)) :bitmap-source (:installed-source-sha256 "0447e0b0a4b444d1fe00eac5686070b31971e2deffbfc8fa2c81423da4ea9685" :definitions ((git-gutter-fr:added :alignment nil :width 8 :height 8 :vector [24 24 24 255 255 24 24 24]) (git-gutter-fr:deleted :alignment nil :width 8 :height 8 :vector [0 0 0 255 255 0 0 0]) (git-gutter-fr:modified :alignment nil :width 8 :height 8 :vector [0 60 60 60 60 60 60 0]))) :rows ((1 :bitmaps (nil nil nil) :display nil nil) (2 :bitmaps (git-gutter-fr:modified nil nil) :display (left-fringe git-gutter-fr:modified git-gutter-fr:modified) (:resolved-face git-gutter-fr:modified :inherit #3=(git-gutter:modified fringe) :foreground "magenta")) (3 :bitmaps (nil nil nil) :display nil nil) (4 :bitmaps (git-gutter-fr:deleted nil nil) :display (left-fringe git-gutter-fr:deleted git-gutter-fr:deleted) (:resolved-face git-gutter-fr:deleted :inherit #4=(git-gutter:deleted fringe) :foreground "red")) (5 :bitmaps (nil nil nil) :display nil nil) (6 :bitmaps (nil nil nil) :display nil nil) (7 :bitmaps (nil nil nil) :display nil nil) (8 :bitmaps (git-gutter-fr:added nil nil) :display #1=(left-fringe git-gutter-fr:added git-gutter-fr:added) (:resolved-face git-gutter-fr:added :inherit #2=(git-gutter:added fringe) :foreground "green")) (9 :bitmaps (git-gutter-fr:added nil nil) :display #1# (:resolved-face git-gutter-fr:added :inherit #2# :foreground "green")) (10 :bitmaps (nil nil nil) :display nil nil) (11 :bitmaps (nil nil nil) :display nil nil) (12 :bitmaps (nil nil nil) :display nil nil))) :cleared (:mode nil :enabled nil :refs nil :overlays 0 :rows ((1 :bitmaps (nil nil nil) :display nil nil) (2 :bitmaps (nil nil nil) :display nil nil) (3 :bitmaps (nil nil nil) :display nil nil) (4 :bitmaps (nil nil nil) :display nil nil) (5 :bitmaps (nil nil nil) :display nil nil) (6 :bitmaps (nil nil nil) :display nil nil) (7 :bitmaps (nil nil nil) :display nil nil) (8 :bitmaps (nil nil nil) :display nil nil) (9 :bitmaps (nil nil nil) :display nil nil) (10 :bitmaps (nil nil nil) :display nil nil) (11 :bitmaps (nil nil nil) :display nil nil) (12 :bitmaps (nil nil nil) :display nil nil))) :right (:side right-fringe :window-fringes (8 8 nil nil) :margins (nil) :rows ((1 :bitmaps (nil nil nil) :display nil nil) (2 :bitmaps (nil git-gutter-fr:modified nil) :display (right-fringe git-gutter-fr:modified git-gutter-fr:modified) (:resolved-face git-gutter-fr:modified :inherit #3# :foreground "magenta")) (3 :bitmaps (nil nil nil) :display nil nil) (4 :bitmaps (nil git-gutter-fr:deleted nil) :display (right-fringe git-gutter-fr:deleted git-gutter-fr:deleted) (:resolved-face git-gutter-fr:deleted :inherit #4# :foreground "red")) (5 :bitmaps (nil nil nil) :display nil nil) (6 :bitmaps (nil nil nil) :display nil nil) (7 :bitmaps (nil nil nil) :display nil nil) (8 :bitmaps (nil git-gutter-fr:added nil) :display #5=(right-fringe git-gutter-fr:added git-gutter-fr:added) (:resolved-face git-gutter-fr:added :inherit #2# :foreground "green")) (9 :bitmaps (nil git-gutter-fr:added nil) :display #5# (:resolved-face git-gutter-fr:added :inherit #2# :foreground "green")) (10 :bitmaps (nil nil nil) :display nil nil) (11 :bitmaps (nil nil nil) :display nil nil) (12 :bitmaps (nil nil nil) :display nil nil))) :final (:mode nil :enabled nil :refs nil :overlays 0 :rows ((1 :bitmaps (nil nil nil) :display nil nil) (2 :bitmaps (nil nil nil) :display nil nil) (3 :bitmaps (nil nil nil) :display nil nil) (4 :bitmaps (nil nil nil) :display nil nil) (5 :bitmaps (nil nil nil) :display nil nil) (6 :bitmaps (nil nil nil) :display nil nil) (7 :bitmaps (nil nil nil) :display nil nil) (8 :bitmaps (nil nil nil) :display nil nil) (9 :bitmaps (nil nil nil) :display nil nil) (10 :bitmaps (nil nil nil) :display nil nil) (11 :bitmaps (nil nil nil) :display nil nil) (12 :bitmaps (nil nil nil) :display nil nil)))) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :mode nil :enabled nil :refs nil :owned-overlays nil :processes-live nil :process-buffers-live nil :window-restored t :margins-restored t :fringes-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
    ]];
    let expected_gnu_full = expect![[
        r#"OK (:value (:result (:source "git-gutter-fringe.el" :real-git-absolute t :geometry-baseline ((nil) (8 8 nil nil)) :left (:display (:graphic t :window-system-live t :frame-visible t :selected-window-live t :selected-window-frame t :selected-window-buffer t) :frame-fringes (8 8) :window-fringes (8 8 nil nil) :margins (nil) :bitmap-registrations ((git-gutter-fr:added t) (git-gutter-fr:modified t) (git-gutter-fr:deleted t)) :bitmap-source (:installed-source-sha256 "0447e0b0a4b444d1fe00eac5686070b31971e2deffbfc8fa2c81423da4ea9685" :definitions ((git-gutter-fr:added :alignment nil :width 8 :height 8 :vector [24 24 24 255 255 24 24 24]) (git-gutter-fr:deleted :alignment nil :width 8 :height 8 :vector [0 0 0 255 255 0 0 0]) (git-gutter-fr:modified :alignment nil :width 8 :height 8 :vector [0 60 60 60 60 60 60 0]))) :rows ((1 :bitmaps (nil nil nil) :display nil nil) (2 :bitmaps (git-gutter-fr:modified nil nil) :display (left-fringe git-gutter-fr:modified git-gutter-fr:modified) (:resolved-face git-gutter-fr:modified :inherit #3=(git-gutter:modified fringe) :foreground "magenta")) (3 :bitmaps (nil nil nil) :display nil nil) (4 :bitmaps (git-gutter-fr:deleted nil nil) :display (left-fringe git-gutter-fr:deleted git-gutter-fr:deleted) (:resolved-face git-gutter-fr:deleted :inherit #4=(git-gutter:deleted fringe) :foreground "red")) (5 :bitmaps (nil nil nil) :display nil nil) (6 :bitmaps (nil nil nil) :display nil nil) (7 :bitmaps (nil nil nil) :display nil nil) (8 :bitmaps (git-gutter-fr:added nil nil) :display #1=(left-fringe git-gutter-fr:added git-gutter-fr:added) (:resolved-face git-gutter-fr:added :inherit #2=(git-gutter:added fringe) :foreground "green")) (9 :bitmaps (git-gutter-fr:added nil nil) :display #1# (:resolved-face git-gutter-fr:added :inherit #2# :foreground "green")) (10 :bitmaps (nil nil nil) :display nil nil) (11 :bitmaps (nil nil nil) :display nil nil) (12 :bitmaps (nil nil nil) :display nil nil))) :cleared (:mode nil :enabled nil :refs nil :overlays 0 :rows ((1 :bitmaps (nil nil nil) :display nil nil) (2 :bitmaps (nil nil nil) :display nil nil) (3 :bitmaps (nil nil nil) :display nil nil) (4 :bitmaps (nil nil nil) :display nil nil) (5 :bitmaps (nil nil nil) :display nil nil) (6 :bitmaps (nil nil nil) :display nil nil) (7 :bitmaps (nil nil nil) :display nil nil) (8 :bitmaps (nil nil nil) :display nil nil) (9 :bitmaps (nil nil nil) :display nil nil) (10 :bitmaps (nil nil nil) :display nil nil) (11 :bitmaps (nil nil nil) :display nil nil) (12 :bitmaps (nil nil nil) :display nil nil))) :right (:side right-fringe :window-fringes (8 8 nil nil) :margins (nil) :rows ((1 :bitmaps (nil nil nil) :display nil nil) (2 :bitmaps (nil git-gutter-fr:modified nil) :display (right-fringe git-gutter-fr:modified git-gutter-fr:modified) (:resolved-face git-gutter-fr:modified :inherit #3# :foreground "magenta")) (3 :bitmaps (nil nil nil) :display nil nil) (4 :bitmaps (nil git-gutter-fr:deleted nil) :display (right-fringe git-gutter-fr:deleted git-gutter-fr:deleted) (:resolved-face git-gutter-fr:deleted :inherit #4# :foreground "red")) (5 :bitmaps (nil nil nil) :display nil nil) (6 :bitmaps (nil nil nil) :display nil nil) (7 :bitmaps (nil nil nil) :display nil nil) (8 :bitmaps (nil git-gutter-fr:added nil) :display #5=(right-fringe git-gutter-fr:added git-gutter-fr:added) (:resolved-face git-gutter-fr:added :inherit #2# :foreground "green")) (9 :bitmaps (nil git-gutter-fr:added nil) :display #5# (:resolved-face git-gutter-fr:added :inherit #2# :foreground "green")) (10 :bitmaps (nil nil nil) :display nil nil) (11 :bitmaps (nil nil nil) :display nil nil) (12 :bitmaps (nil nil nil) :display nil nil))) :final (:mode nil :enabled nil :refs nil :overlays 0 :rows ((1 :bitmaps (nil nil nil) :display nil nil) (2 :bitmaps (nil nil nil) :display nil nil) (3 :bitmaps (nil nil nil) :display nil nil) (4 :bitmaps (nil nil nil) :display nil nil) (5 :bitmaps (nil nil nil) :display nil nil) (6 :bitmaps (nil nil nil) :display nil nil) (7 :bitmaps (nil nil nil) :display nil nil) (8 :bitmaps (nil nil nil) :display nil nil) (9 :bitmaps (nil nil nil) :display nil nil) (10 :bitmaps (nil nil nil) :display nil nil) (11 :bitmaps (nil nil nil) :display nil nil) (12 :bitmaps (nil nil nil) :display nil nil)))) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :mode nil :enabled nil :refs nil :owned-overlays nil :processes-live nil :process-buffers-live nil :window-restored t :margins-restored t :fringes-restored t :buffer-restored t :body-error nil :cleanup-errors nil)) :stdout "" :stderr "\nGLib-GIO-CRITICAL: g_settings_schema_source_lookup: assertion 'source != NULL' failed\n")"#
    ]];
    let expected_neomacs_full = expect![[
        r#"OK (:value (:result (:source "git-gutter-fringe.el" :real-git-absolute t :geometry-baseline ((nil) (8 8 nil nil)) :left (:display (:graphic t :window-system-live t :frame-visible t :selected-window-live t :selected-window-frame t :selected-window-buffer t) :frame-fringes (8 8) :window-fringes (8 8 nil nil) :margins (nil) :bitmap-registrations ((git-gutter-fr:added t) (git-gutter-fr:modified t) (git-gutter-fr:deleted t)) :bitmap-source (:installed-source-sha256 "0447e0b0a4b444d1fe00eac5686070b31971e2deffbfc8fa2c81423da4ea9685" :definitions ((git-gutter-fr:added :alignment nil :width 8 :height 8 :vector [24 24 24 255 255 24 24 24]) (git-gutter-fr:deleted :alignment nil :width 8 :height 8 :vector [0 0 0 255 255 0 0 0]) (git-gutter-fr:modified :alignment nil :width 8 :height 8 :vector [0 60 60 60 60 60 60 0]))) :rows ((1 :bitmaps (nil nil nil) :display nil nil) (2 :bitmaps (git-gutter-fr:modified nil nil) :display (left-fringe git-gutter-fr:modified git-gutter-fr:modified) (:resolved-face git-gutter-fr:modified :inherit #3=(git-gutter:modified fringe) :foreground "magenta")) (3 :bitmaps (nil nil nil) :display nil nil) (4 :bitmaps (git-gutter-fr:deleted nil nil) :display (left-fringe git-gutter-fr:deleted git-gutter-fr:deleted) (:resolved-face git-gutter-fr:deleted :inherit #4=(git-gutter:deleted fringe) :foreground "red")) (5 :bitmaps (nil nil nil) :display nil nil) (6 :bitmaps (nil nil nil) :display nil nil) (7 :bitmaps (nil nil nil) :display nil nil) (8 :bitmaps (git-gutter-fr:added nil nil) :display #1=(left-fringe git-gutter-fr:added git-gutter-fr:added) (:resolved-face git-gutter-fr:added :inherit #2=(git-gutter:added fringe) :foreground "green")) (9 :bitmaps (git-gutter-fr:added nil nil) :display #1# (:resolved-face git-gutter-fr:added :inherit #2# :foreground "green")) (10 :bitmaps (nil nil nil) :display nil nil) (11 :bitmaps (nil nil nil) :display nil nil) (12 :bitmaps (nil nil nil) :display nil nil))) :cleared (:mode nil :enabled nil :refs nil :overlays 0 :rows ((1 :bitmaps (nil nil nil) :display nil nil) (2 :bitmaps (nil nil nil) :display nil nil) (3 :bitmaps (nil nil nil) :display nil nil) (4 :bitmaps (nil nil nil) :display nil nil) (5 :bitmaps (nil nil nil) :display nil nil) (6 :bitmaps (nil nil nil) :display nil nil) (7 :bitmaps (nil nil nil) :display nil nil) (8 :bitmaps (nil nil nil) :display nil nil) (9 :bitmaps (nil nil nil) :display nil nil) (10 :bitmaps (nil nil nil) :display nil nil) (11 :bitmaps (nil nil nil) :display nil nil) (12 :bitmaps (nil nil nil) :display nil nil))) :right (:side right-fringe :window-fringes (8 8 nil nil) :margins (nil) :rows ((1 :bitmaps (nil nil nil) :display nil nil) (2 :bitmaps (nil git-gutter-fr:modified nil) :display (right-fringe git-gutter-fr:modified git-gutter-fr:modified) (:resolved-face git-gutter-fr:modified :inherit #3# :foreground "magenta")) (3 :bitmaps (nil nil nil) :display nil nil) (4 :bitmaps (nil git-gutter-fr:deleted nil) :display (right-fringe git-gutter-fr:deleted git-gutter-fr:deleted) (:resolved-face git-gutter-fr:deleted :inherit #4# :foreground "red")) (5 :bitmaps (nil nil nil) :display nil nil) (6 :bitmaps (nil nil nil) :display nil nil) (7 :bitmaps (nil nil nil) :display nil nil) (8 :bitmaps (nil git-gutter-fr:added nil) :display #5=(right-fringe git-gutter-fr:added git-gutter-fr:added) (:resolved-face git-gutter-fr:added :inherit #2# :foreground "green")) (9 :bitmaps (nil git-gutter-fr:added nil) :display #5# (:resolved-face git-gutter-fr:added :inherit #2# :foreground "green")) (10 :bitmaps (nil nil nil) :display nil nil) (11 :bitmaps (nil nil nil) :display nil nil) (12 :bitmaps (nil nil nil) :display nil nil))) :final (:mode nil :enabled nil :refs nil :overlays 0 :rows ((1 :bitmaps (nil nil nil) :display nil nil) (2 :bitmaps (nil nil nil) :display nil nil) (3 :bitmaps (nil nil nil) :display nil nil) (4 :bitmaps (nil nil nil) :display nil nil) (5 :bitmaps (nil nil nil) :display nil nil) (6 :bitmaps (nil nil nil) :display nil nil) (7 :bitmaps (nil nil nil) :display nil nil) (8 :bitmaps (nil nil nil) :display nil nil) (9 :bitmaps (nil nil nil) :display nil nil) (10 :bitmaps (nil nil nil) :display nil nil) (11 :bitmaps (nil nil nil) :display nil nil) (12 :bitmaps (nil nil nil) :display nil nil)))) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :mode nil :enabled nil :refs nil :owned-overlays nil :processes-live nil :process-buffers-live nil :window-restored t :margins-restored t :fringes-restored t :buffer-restored t :body-error nil :cleanup-errors nil)) :stdout "" :stderr "libEGL warning: DRI3 error: Could not get DRI3 device\nlibEGL warning: Ensure your X server supports DRI3 to get accelerated rendering\nMESA: info: vulkan: No DRI3 support detected - required for presentation\nNote: you can probably enable DRI3 in your Xorg config\n")"#
    ]];
    expected_gnu.assert_eq(&pair.gnu_behavior.to_string());
    expected_gnu_full.assert_eq(&pair.gnu_emacs.to_string());
    expected_neomacs_full.assert_eq(&pair.neomacs.to_string());
    assert_eq!(
        pair.neomacs_behavior, pair.gnu_behavior,
        "real GUI fringe rows must match GNU exactly, including the \
         (LEFT RIGHT OVERLAY) triple `fringe-bitmaps-at-pos` reports for every \
         row; full outcomes with logs:\nNeomacs: {}\nGNU Emacs: {}",
        pair.neomacs, pair.gnu_emacs,
    );
}

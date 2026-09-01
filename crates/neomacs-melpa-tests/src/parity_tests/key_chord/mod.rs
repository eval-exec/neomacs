//! Practical parity for key-chord's public chord-definition and input method.
//!
//! These cases define global and local two-key and double-tap chords, drive
//! `key-chord-input-method` through an owned `read-event` clock, skip chords
//! during typing flow, and recover after invalid keys and a read-char context.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, KEY_CHORD_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'help-mode)
(require 'key-chord)
(set-window-configuration (current-window-configuration))

(defconst kc430-test-tree
  "ccc2ec4f0bc98a68bd72644c35a76fea2a2c906a")
(defconst kc430-test-manifest
  '(("key-chord-pkg.el" . "a7c8377bb43ea4ca1ac8f5c62c185ad4d51d159eba94daf96b8912d9cd01859f")
    ("key-chord.el" . "66d5f65f0666bbe9806ea84a73059b988640fde7c8a0aff79d4cf7e27256cb5c")))

(defvar kc430-test-case-index 0)
(defvar kc430-test-root nil)
(defvar kc430-test-root-owned nil)
(defvar kc430-test-clock 0.0)
(defvar kc430-test-events nil)

(defun kc430-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun kc430-test-source-state ()
  (let* ((located (locate-library "key-chord.el"))
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
                         (cons file (kc430-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/key-chord.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car kc430-test-manifest)))
      (error "Unexpected installed key-chord payload: %S" (or manifest files)))
    (dolist (entry kc430-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (kc430-test-sha file) expected))
          (error "Unexpected installed key-chord source: %S"
                 (cons entry manifest)))))
    (list :tree kc430-test-tree
          :manifest manifest
          :feature (featurep 'key-chord)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'key-chord package-alist)))))))

(defun kc430-test-condition (thunk)
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

(defun kc430-test-forbid-external (operation &rest arguments)
  (error "Unexpected key-chord external boundary: %S %S" operation arguments))

(defun kc430-test-current-time ()
  (seconds-to-time kc430-test-clock))

(defun kc430-test-read-event (&optional _prompt _inherit _seconds)
  (let ((next (pop kc430-test-events)))
    (cond
     ((null next) nil)
     ((eq next :timeout) nil)
     ((consp next)
      (setq kc430-test-clock (+ kc430-test-clock (or (plist-get next :dt) 0.0)))
      (plist-get next :char))
     (t next))))

(defun kc430-test-binding (first second)
  (key-binding (vector 'key-chord first second)))

(defun kc430-test-input (first &rest events)
  (setq kc430-test-events events)
  (let* ((unread-before unread-command-events)
         (event (key-chord-input-method first))
         (unread (mapcar #'identity
                         (seq-difference unread-command-events unread-before))))
    (setq unread-command-events unread-before)
    (list :event event
          :unread unread
          :binding (and (eq (car-safe event) 'key-chord)
                        (key-binding (apply #'vector event)))
          :last-unmatched key-chord-last-unmatched
          :typing key-chord-in-typing-flow)))

(defun kc430-test-run (body)
  (let* ((index (cl-incf kc430-test-case-index))
         (sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (format "key-chord-%d" index)
                                       sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (selected-window-before (selected-window))
         (window-before (current-window-configuration))
         (source-before (kc430-test-source-state))
         (directory-before default-directory)
         (enable-local-before enable-local-variables)
         (debug-before debug-on-error)
         (print-circle-before print-circle)
         (global-map-before (current-global-map))
         (input-method-before input-method-function)
         (mode-before key-chord-mode)
         (two-delay-before key-chord-two-keys-delay)
         (one-delay-before key-chord-one-key-delay)
         (min-delay-before key-chord-one-key-min-delay)
         (in-macros-before key-chord-in-macros)
         (typing-before key-chord-typing-detection)
         (speed-before key-chord-typing-speed-threshold)
         (reset-before key-chord-typing-reset-delay)
         (tracking-before key-chord-use-key-tracking)
         (keys-before (copy-sequence key-chord-keys-in-use))
         (unmatched-before key-chord-last-unmatched)
         (flow-before key-chord-in-typing-flow)
         (last-time-before key-chord-last-key-time)
         (reset-time-before key-chord-typing-reset-time)
         (macro-last-before key-chord-in-last-kbd-macro)
         (macro-def-before key-chord-defining-kbd-macro)
         (defining-macro-before defining-kbd-macro)
         (last-macro-before last-kbd-macro)
         (unread-before unread-command-events)
         (kc430-test-root root)
         (kc430-test-root-owned nil)
         (kc430-test-clock 0.0)
         (kc430-test-events nil)
         result body-error source-after cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute key-chord sandbox root"))
              (when (file-exists-p root)
                (error "key-chord sandbox root exists: %S" root))
              (make-directory root)
              (setq kc430-test-root-owned t
                    enable-local-variables nil
                    debug-on-error nil
                    print-circle nil
                    default-directory root)
              (use-global-map (copy-keymap global-map-before))
              (cl-letf (((symbol-function 'call-process)
                         (lambda (&rest args)
                           (apply #'kc430-test-forbid-external
                                  'call-process args)))
                        ((symbol-function 'call-process-region)
                         (lambda (&rest args)
                           (apply #'kc430-test-forbid-external
                                  'call-process-region args)))
                        ((symbol-function 'make-process)
                         (lambda (&rest args)
                           (apply #'kc430-test-forbid-external
                                  'make-process args)))
                        ((symbol-function 'start-process)
                         (lambda (&rest args)
                           (apply #'kc430-test-forbid-external
                                  'start-process args)))
                        ((symbol-function 'url-retrieve)
                         (lambda (&rest args)
                           (apply #'kc430-test-forbid-external
                                  'url-retrieve args)))
                        ((symbol-function 'url-retrieve-synchronously)
                         (lambda (&rest args)
                           (apply #'kc430-test-forbid-external
                                  'url-retrieve-synchronously args)))
                        ((symbol-function 'read-event)
                         #'kc430-test-read-event)
                        ((symbol-function 'current-time)
                         #'kc430-test-current-time))
                (setq result (funcall body root)))
              (setq source-after (kc430-test-source-state))
              (unless (equal source-before source-after)
                (error "key-chord source changed")))
          (error (setq body-error
                       (list (car condition)
                             (copy-tree (cdr condition))))))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error (push (list label (car condition)
                                  (copy-tree (cdr condition)))
                            cleanup-errors)))))
        (use-global-map global-map-before)
        (setq input-method-function input-method-before
              key-chord-mode mode-before
              key-chord-two-keys-delay two-delay-before
              key-chord-one-key-delay one-delay-before
              key-chord-one-key-min-delay min-delay-before
              key-chord-in-macros in-macros-before
              key-chord-typing-detection typing-before
              key-chord-typing-speed-threshold speed-before
              key-chord-typing-reset-delay reset-before
              key-chord-use-key-tracking tracking-before
              key-chord-last-unmatched unmatched-before
              key-chord-in-typing-flow flow-before
              key-chord-last-key-time last-time-before
              key-chord-typing-reset-time reset-time-before
              key-chord-in-last-kbd-macro macro-last-before
              key-chord-defining-kbd-macro macro-def-before
              defining-kbd-macro defining-macro-before
              last-kbd-macro last-macro-before
              unread-command-events unread-before
              enable-local-variables enable-local-before
              debug-on-error debug-before
              print-circle print-circle-before
              default-directory directory-before)
        (dotimes (i 256)
          (aset key-chord-keys-in-use i (aref keys-before i)))
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer (buffer-list))
          (unless (memq buffer buffers-before)
            (attempt (list 'buffer (buffer-name buffer))
                     (lambda ()
                       (when (buffer-live-p buffer)
                         (with-current-buffer buffer
                           (set-buffer-modified-p nil))
                         (kill-buffer buffer))))))
        (dolist (timer (append timer-list timer-idle-list))
          (unless (memq timer timers-before)
            (attempt 'timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'frame (lambda () (delete-frame frame t)))))
        (attempt 'window
                 (lambda () (set-window-configuration window-before)))
        (when (window-live-p selected-window-before)
          (attempt 'selected
                   (lambda () (select-window selected-window-before))))
        (when (buffer-live-p buffer-before)
          (attempt 'current-buffer
                   (lambda () (set-buffer buffer-before))))
        (when kc430-test-root-owned
          (attempt 'root (lambda () (delete-directory root t))))))
    (when body-error
      (error "key-chord body failed: %S" body-error))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :new-buffers (mapcar #'buffer-name
                                      (seq-remove
                                       (lambda (buffer)
                                         (memq buffer buffers-before))
                                       (buffer-list)))
                 :new-processes (length
                                 (seq-remove
                                  (lambda (process)
                                    (memq process processes-before))
                                  (process-list)))
                 :new-timers (length
                              (seq-remove
                               (lambda (timer)
                                 (memq timer timers-before))
                               (append timer-list timer-idle-list)))
                 :new-frames (length
                              (seq-remove
                               (lambda (frame)
                                 (memq frame frames-before))
                               (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :window-restored (eq (selected-window)
                                      selected-window-before)
                 :map-restored (eq (current-global-map) global-map-before)
                 :mode-restored (eq key-chord-mode mode-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if cleanup-errors
          (error "key-chord cleanup failed: %S" (list result cleanup))
        (list :source source-before
              :result result
              :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(KEY_CHORD_MELPA_PIN, "key-chord.el")
        .expect("prepare pinned key-chord source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn defines_global_and_local_chords_and_describes_them() -> ParityBatchCase {
    ParityBatchCase::value(
        "defines_global_and_local_chords_and_describes_them",
        r####"
(kc430-test-run
 (lambda (_root)
   (let (defined local-over help short non-byte)
     (key-chord-mode 1)
     (key-chord-define-global "hj" 'undo)
     (key-chord-define-global ",." "<>\C-b")
     (key-chord-define-global "''" "`'\C-b")
     (setq defined
           (list :mode key-chord-mode
                 :input input-method-function
                 :hj (kc430-test-binding ?h ?j)
                 :jh (kc430-test-binding ?j ?h)
                 :comma (kc430-test-binding ?, ?.)
                 :tick (kc430-test-binding ?' ?')
                 :tracked
                 (list (and (aref key-chord-keys-in-use ?h) t)
                       (and (aref key-chord-keys-in-use ?j) t)
                       (and (aref key-chord-keys-in-use ?') t))))
     (with-temp-buffer
       (let ((map (make-sparse-keymap)))
         (use-local-map map)
         (key-chord-define-local "hj" 'forward-char)
         (setq local-over
               (list :local (kc430-test-binding ?h ?j)
                     :global-still (lookup-key (current-global-map)
                                               (vector 'key-chord ?h ?j))))))
     (key-chord-describe)
     (setq help
           (and (get-buffer "*Help*")
                (with-current-buffer "*Help*"
                  (list :mode major-mode
                        :has-hj (and (save-excursion
                                       (goto-char (point-min))
                                       (search-forward "h j" nil t))
                                     t)
                        :has-undo
                        (and (save-excursion
                               (goto-char (point-min))
                               (search-forward "undo" nil t))
                             t)
                        :has-prefix
                        (and (save-excursion
                               (goto-char (point-min))
                               (search-forward "key-chord" nil t))
                             t)))))
     (key-chord-unset-global "hj")
     (let ((unset-hj (kc430-test-binding ?h ?j))
           (unset-jh (kc430-test-binding ?j ?h)))
       (setq short
             (kc430-test-condition
              (lambda () (key-chord-define-global "h" 'undo))))
       (setq non-byte
             (kc430-test-condition
              (lambda ()
                (key-chord-define-global (vector ?h #x754C) 'undo))))
       (key-chord-define-global "hj" 'undo)
       (list :defined defined
             :local-over local-over
             :help help
             :unset-hj unset-hj
             :unset-jh unset-jh
             :short short
             :non-byte non-byte
             :recovered (kc430-test-binding ?h ?j))))))
"####,
        expect![[
            r#"OK (:source (:tree "ccc2ec4f0bc98a68bd72644c35a76fea2a2c906a" :manifest (("key-chord-pkg.el" . "a7c8377bb43ea4ca1ac8f5c62c185ad4d51d159eba94daf96b8912d9cd01859f") ("key-chord.el" . "66d5f65f0666bbe9806ea84a73059b988640fde7c8a0aff79d4cf7e27256cb5c")) :feature t :version "20250330.2011") :result (:defined (:mode t :input key-chord-input-method :hj undo :jh undo :comma "<>\2" :tick "`'\2" :tracked (t t t)) :local-over (:local forward-char :global-still undo) :help (:mode help-mode :has-hj t :has-undo t :has-prefix t) :unset-hj nil :unset-jh nil :short (:error error :data ("Key-chord keys must have two elements") :message "Key-chord keys must have two elements") :non-byte (:error error :data ("Key-chord keys must both be bytes (characters with codes < 256)") :message "Key-chord keys must both be bytes (characters with codes < 256)") :recovered undo) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :map-restored t :mode-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn input_method_fires_two_key_and_double_tap_chords() -> ParityBatchCase {
    ParityBatchCase::value(
        "input_method_fires_two_key_and_double_tap_chords",
        r####"
(kc430-test-run
 (lambda (_root)
   (key-chord-mode 1)
   (setq key-chord-one-key-min-delay 0.0)
   (key-chord-define-global "hj" 'undo)
   (key-chord-define-global "qq" "the ")
   (list
    :two-key (kc430-test-input ?h (list :char ?j :dt 0.05))
    :mismatch (kc430-test-input ?h (list :char ?x :dt 0.05))
    :timeout (kc430-test-input ?h :timeout)
    :repeat-unmatched (kc430-test-input ?h (list :char ?j :dt 0.05))
    :double (kc430-test-input ?q (list :char ?q :dt 0.05))
    :held
    (let ((key-chord-one-key-min-delay 0.05))
      (kc430-test-input ?q (list :char ?q :dt 0.0)))
    :unused (kc430-test-input ?z (list :char ?x :dt 0.05)))))
"####,
        expect![[
            r#"OK (:source (:tree "ccc2ec4f0bc98a68bd72644c35a76fea2a2c906a" :manifest (("key-chord-pkg.el" . "a7c8377bb43ea4ca1ac8f5c62c185ad4d51d159eba94daf96b8912d9cd01859f") ("key-chord.el" . "66d5f65f0666bbe9806ea84a73059b988640fde7c8a0aff79d4cf7e27256cb5c")) :feature t :version "20250330.2011") :result (:two-key (:event (key-chord 104 106) :unread nil :binding undo :last-unmatched nil :typing nil) :mismatch (:event (104) :unread (120) :binding nil :last-unmatched 104 :typing nil) :timeout (:event (104) :unread nil :binding nil :last-unmatched 104 :typing nil) :repeat-unmatched (:event (104) :unread nil :binding nil :last-unmatched 104 :typing nil) :double (:event (key-chord 113 113) :unread nil :binding "the " :last-unmatched nil :typing nil) :held (:event (113) :unread (113) :binding nil :last-unmatched 113 :typing nil) :unused (:event (122) :unread nil :binding nil :last-unmatched 122 :typing nil)) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :map-restored t :mode-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn typing_detection_skips_chords_until_idle() -> ParityBatchCase {
    ParityBatchCase::value(
        "typing_detection_skips_chords_until_idle",
        r####"
(kc430-test-run
 (lambda (_root)
   (key-chord-mode 1)
   (setq key-chord-typing-detection t
         key-chord-typing-speed-threshold 0.1
         key-chord-typing-reset-delay 0.5)
   (key-chord-define-global "hj" 'undo)
   (let* ((first (progn
                   (setq kc430-test-clock 1.0)
                   (kc430-test-input ?a)))
          (typed (progn
                   (setq kc430-test-clock 1.05)
                   (kc430-test-input ?b)))
          (skipped (progn
                     (setq kc430-test-clock 1.08)
                     (kc430-test-input ?h (list :char ?j :dt 0.01))))
          (after-idle (progn
                        (setq kc430-test-clock 2.0)
                        (kc430-test-input ?h (list :char ?j :dt 0.05)))))
     (list :first first
           :typed typed
           :skipped skipped
           :after-idle after-idle))))
"####,
        expect![[
            r#"OK (:source (:tree "ccc2ec4f0bc98a68bd72644c35a76fea2a2c906a" :manifest (("key-chord-pkg.el" . "a7c8377bb43ea4ca1ac8f5c62c185ad4d51d159eba94daf96b8912d9cd01859f") ("key-chord.el" . "66d5f65f0666bbe9806ea84a73059b988640fde7c8a0aff79d4cf7e27256cb5c")) :feature t :version "20250330.2011") :result (:first (:event (97) :unread nil :binding nil :last-unmatched 97 :typing nil) :typed (:event (98) :unread nil :binding nil :last-unmatched 97 :typing t) :skipped (:event (104) :unread nil :binding nil :last-unmatched 97 :typing t) :after-idle (:event (key-chord 104 106) :unread nil :binding undo :last-unmatched nil :typing nil)) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :map-restored t :mode-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn macros_read_char_context_and_mode_restore() -> ParityBatchCase {
    ParityBatchCase::value(
        "macros_read_char_context_and_mode_restore",
        r####"
(kc430-test-run
 (lambda (_root)
   (key-chord-mode 1)
   (key-chord-define-global "hj" 'undo)
   (let* ((read-char
           (let ((overriding-local-map read-key-empty-map))
             (kc430-test-input ?h (list :char ?j :dt 0.05))))
          (macro-off
           (let ((executing-kbd-macro t)
                 (key-chord-in-macros nil))
             (kc430-test-input ?h (list :char ?j :dt 0.05))))
          (started (progn
                     (setq key-chord-defining-kbd-macro '(?x))
                     (start-kbd-macro nil)
                     key-chord-defining-kbd-macro))
          (recorded
           (progn
             (setq key-chord-last-unmatched nil)
             (kc430-test-input ?h (list :char ?j :dt 0.05))
             (end-kbd-macro)
             (copy-sequence key-chord-in-last-kbd-macro)))
          (mode-off
           (progn
             (key-chord-mode -1)
             (list :mode key-chord-mode
                   :input input-method-function)))
          (rearmed
           (progn
             (key-chord-mode 1)
             (setq key-chord-last-unmatched nil)
             (list :mode key-chord-mode
                   :input input-method-function
                   :chord (kc430-test-input ?h (list :char ?j :dt 0.05))))))
     (list :read-char read-char
           :macro-off macro-off
           :started started
           :recorded recorded
           :mode-off mode-off
           :rearmed rearmed))))
"####,
        expect![[
            r#"OK (:source (:tree "ccc2ec4f0bc98a68bd72644c35a76fea2a2c906a" :manifest (("key-chord-pkg.el" . "a7c8377bb43ea4ca1ac8f5c62c185ad4d51d159eba94daf96b8912d9cd01859f") ("key-chord.el" . "66d5f65f0666bbe9806ea84a73059b988640fde7c8a0aff79d4cf7e27256cb5c")) :feature t :version "20250330.2011") :result (:read-char (:event (104) :unread nil :binding nil :last-unmatched nil :typing nil) :macro-off (:event (104) :unread nil :binding nil :last-unmatched 104 :typing nil) :started nil :recorded (104) :mode-off (:mode nil :input nil) :rearmed (:mode t :input key-chord-input-method :chord (:event (key-chord 104 106) :unread nil :binding undo :last-unmatched nil :typing nil))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :map-restored t :mode-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn key_chord_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        defines_global_and_local_chords_and_describes_them(),
        input_method_fires_two_key_and_double_tap_chords(),
        typing_detection_skips_chords_until_idle(),
        macros_read_char_context_and_mode_restore(),
    ];
    assert_oracle_batch_cases(oracle(), "key-chord-rank430", "key_chord_parity", &cases);
}

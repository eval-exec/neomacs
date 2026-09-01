//! Practical Polymode parity against the exact locked framework source.
//!
//! The corpus defines a public host/inner polymode and exercises the real
//! span engine, indirect buffers, command map, mode-aware editing, saving,
//! and incremental delimiter recovery.  No package function is replaced.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, POLYMODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'polymode)

;; UTF-8 file I/O lazily creates this editor-owned scratch buffer.  Establish
;; it before case baselines so it is not misclassified as package residue.
(get-buffer-create " *code-conversion-work*")

(define-hostmode poly-pm377-text-hostmode
  :mode 'text-mode)

(define-innermode poly-pm377-elisp-innermode
  :mode 'emacs-lisp-mode
  :head-matcher "^<<elisp>>[ \t]*\n"
  :tail-matcher "^<<end>>[ \t]*\n?"
  :head-mode 'host
  :tail-mode 'host)

(define-polymode poly-pm377-mode
  :hostmode 'poly-pm377-text-hostmode
  :innermodes '(poly-pm377-elisp-innermode))

(defconst pm377-test-fixture
  (concat
   "Release notes café 界.\n\n"
   "<<elisp>>\n"
   ";; Compute λ exactly.\n"
   "(defun total-界 (items)\n"
   "(+ 1 (length items)))\n"
   "<<end>>\n\n"
   "Between chunks Ω.\n\n"
   "<<elisp>>\n"
   "(message \"second café\")\n"
   "<<end>>\n\n"
   "Closing prose.\n"))

(defconst pm377-test-format-fixture
  (concat
   "This deliberately long host paragraph contains café, lambda λ, and world 界 so Polymode must fill it using the host text mode rather than the inner Lisp mode.\n\n"
   "<<elisp>>\n"
   "(defun greeting-界 (name)\n"
   "(message \"hello %s λ\" name))\n"
   "<<end>>\n\n"
   "Trailing host prose Ω.\n"))

(defvar pm377-test-root nil)

(defun pm377-test-owned-path (name)
  (expand-file-name name pm377-test-root))

(defun pm377-test-write (path bytes)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert bytes)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun pm377-test-read (path)
  (let ((coding-system-for-read 'utf-8-unix))
    (with-temp-buffer
      (insert-file-contents path)
      (buffer-string))))

(defun pm377-test-open (name text)
  (let* ((path (pm377-test-write (pm377-test-owned-path name) text))
         (buffer (find-file-noselect path)))
    (switch-to-buffer buffer)
    (goto-char (point-min))
    (poly-pm377-mode)
    (buffer-enable-undo)
    (set-buffer-modified-p nil)
    buffer))

(defun pm377-test-base-buffer ()
  (or (buffer-base-buffer) (current-buffer)))

(defun pm377-test-relative-file (buffer)
  (when-let* ((file (buffer-file-name buffer)))
    (file-relative-name file pm377-test-root)))

(defun pm377-test-span-state ()
  (let ((base (pm377-test-base-buffer)) states)
    (with-current-buffer base
      (save-restriction
        (widen)
        (pm-map-over-spans
         (lambda (span)
           (let* ((raw-range (pm-span-to-range span))
                  (range
                   (cons (if (markerp (car raw-range))
                             (marker-position (car raw-range))
                           (car raw-range))
                         (if (markerp (cdr raw-range))
                             (marker-position (cdr raw-range))
                           (cdr raw-range)))))
             (push (list :type (or (car span) 'host)
                         :range range
                         :mode (pm-span-mode span)
                         :text (buffer-substring-no-properties
                                (car range) (cdr range)))
                   states)))
         (point-min) (point-max))))
    (nreverse states)))

(defun pm377-test-buffer-state ()
  (let* ((base (pm377-test-base-buffer))
         (polymode (buffer-local-value 'pm/polymode base))
         states)
    (dolist (buffer (oref polymode -buffers))
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (push (list :role (if (buffer-base-buffer) 'inner 'host)
                      :mode major-mode
                      :polymode polymode-mode
                      :specific poly-pm377-mode
                      :indirect (and (buffer-base-buffer) t)
                      :narrowed (buffer-narrowed-p)
                      :bounds (cons (point-min) (point-max))
                      :indent indent-line-function
                      :fill fill-forward-paragraph-function
                      :syntax syntax-propertize-function)
                states))))
    (sort states
          (lambda (left right)
            (string-lessp (symbol-name (plist-get left :role))
                          (symbol-name (plist-get right :role)))))))

(defun pm377-test-selected-state ()
  (let* ((window (selected-window))
         (buffer (window-buffer window)))
    (with-current-buffer buffer
      (list :role (if (buffer-base-buffer) 'inner 'host)
            :mode major-mode
            :file (pm377-test-relative-file buffer)
            :point (window-point window)
            :line (line-number-at-pos (window-point window))
            :column (save-excursion
                      (goto-char (window-point window))
                      (current-column))
            :narrowed (buffer-narrowed-p)
            :bounds (cons (point-min) (point-max))))))

(defun pm377-test-call-key (key)
  (switch-to-buffer (window-buffer (selected-window)))
  (let ((command (key-binding (kbd key))))
    (unless (commandp command)
      (error "No command bound to %S: %S" key command))
    (call-interactively command)
    ;; The editor command loop runs this package hook after every command.
    ;; Invoke that same installed hook because the oracle evaluates one form
    ;; below its transport rather than entering a top-level input loop.
    (run-hooks 'post-command-hook))
  (pm377-test-selected-state))

(defun pm377-test-select-position (base position)
  (switch-to-buffer base)
  (widen)
  (goto-char (max (point-min) (1- position)))
  (when (> position (point-min))
    (forward-char 1)
    (run-hooks 'post-command-hook))
  (unless (= (window-point (selected-window)) position)
    (error "Polymode did not select requested position %S: %S"
           position (pm377-test-selected-state)))
  (window-buffer (selected-window)))

(defun pm377-test-select-text (base text)
  (let ((position
         (with-current-buffer base
           (save-restriction
             (widen)
             (goto-char (point-min))
             (or (search-forward text nil t)
                 (error "Missing fixture text %S" text))
             (- (point) (length text))))))
    (pm377-test-select-position base position)))

(defun pm377-test-region-state ()
  (let ((beg (region-beginning))
        (end (region-end)))
    (list :point (point)
          :mark (mark t)
          :active mark-active
          :range (cons beg end)
          :text (buffer-substring-no-properties beg end))))

(defun pm377-test-base-text (base)
  (with-current-buffer base
    (save-restriction
      (widen)
      (buffer-substring-no-properties (point-min) (point-max)))))

(defun pm377-test-run (name thunk)
  (let* ((pm377-test-root
          (file-name-as-directory
           (expand-file-name (concat "polymode377/" name "/")
                             (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list nil))
         (idle-timers-before (append timer-idle-list nil))
         (kill-ring-before kill-ring)
         (kill-ring-yank-pointer-before kill-ring-yank-pointer)
         (overriding-terminal-local-map-before overriding-terminal-local-map)
         (overriding-local-map-before overriding-local-map)
         (emulation-mode-map-alists-before emulation-mode-map-alists)
         (copy-region-blink-delay 0)
         result body-error cleanup-errors)
    (when (file-exists-p pm377-test-root)
      (delete-directory pm377-test-root t))
    (make-directory pm377-test-root t)
    (unwind-protect
        (condition-case error
            (setq result
                  (save-window-excursion
                    (save-current-buffer
                      (funcall thunk))))
          (error (setq body-error error)))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (or (memq timer timers-before)
                    (memq timer idle-timers-before))
          (condition-case error
              (cancel-timer timer)
            (error (push (list :cancel-timer error) cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case error
              (when (buffer-live-p buffer)
                (with-current-buffer buffer
                  (set-buffer-modified-p nil))
                (kill-buffer buffer))
            (error
             (push (list :kill-buffer (buffer-name buffer) error)
                   cleanup-errors)))))
      (condition-case error
          (when (file-exists-p pm377-test-root)
            (delete-directory pm377-test-root t))
        (error (push (list :delete-root error) cleanup-errors)))
      (setq kill-ring kill-ring-before
            kill-ring-yank-pointer kill-ring-yank-pointer-before
            overriding-terminal-local-map overriding-terminal-local-map-before
            overriding-local-map overriding-local-map-before
            emulation-mode-map-alists emulation-mode-map-alists-before)
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (push (list :remaining-process (process-name process))
                cleanup-errors)))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (push (list :remaining-buffer (buffer-name buffer)) cleanup-errors)))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (or (memq timer timers-before)
                    (memq timer idle-timers-before))
          (push (list :remaining-timer t) cleanup-errors))))
    (cond
     ((and body-error cleanup-errors)
      (error "Polymode body failed %S; cleanup failed %S"
             body-error (nreverse cleanup-errors)))
     (body-error (signal (car body-error) (cdr body-error)))
     (cleanup-errors
      (error "Polymode cleanup failed: %S" (nreverse cleanup-errors)))
     (t result))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(POLYMODE_MELPA_PIN, "polymode.el")
        .expect("prepare pinned Polymode source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn activation_builds_exact_span_and_indirect_buffer_topology() -> ParityBatchCase {
    ParityBatchCase::value(
        "activation_builds_exact_span_and_indirect_buffer_topology",
        r####"(pm377-test-run
 "activation"
 (lambda ()
   (let ((base (pm377-test-open "document.pm" pm377-test-fixture)))
     (with-current-buffer base
       (list :mode major-mode
             :file (pm377-test-relative-file base)
             :text (buffer-substring-no-properties (point-min) (point-max))
             :spans (pm377-test-span-state)
             :buffers (pm377-test-buffer-state)
             :keys
             (mapcar (lambda (key) (list key (key-binding (kbd key))))
                     '("M-n C-n" "M-n C-p" "M-n C-M-n" "M-n C-M-p"
                       "M-n C-t" "M-n M-m" "M-n M-w" "M-n M-k")))))))"####,
        expect![[
            r#"OK (:mode text-mode :file "document.pm" :text "Release notes café 界.\n\n<<elisp>>\n;; Compute λ exactly.\n(defun total-界 (items)\n(+ 1 (length items)))\n<<end>>\n\nBetween chunks Ω.\n\n<<elisp>>\n(message \"second café\")\n<<end>>\n\nClosing prose.\n" :spans ((:type host :range (1 . 24) :mode text-mode :text "Release notes café 界.\n\n") (:type head :range (24 . 34) :mode text-mode :text "<<elisp>>\n") (:type body :range (34 . 101) :mode emacs-lisp-mode :text ";; Compute λ exactly.\n(defun total-界 (items)\n(+ 1 (length items)))\n") (:type tail :range (101 . 109) :mode text-mode :text "<<end>>\n") (:type host :range (109 . 129) :mode text-mode :text "\nBetween chunks Ω.\n\n") (:type head :range (129 . 139) :mode text-mode :text "<<elisp>>\n") (:type body :range (139 . 163) :mode emacs-lisp-mode :text "(message \"second café\")\n") (:type tail :range (163 . 171) :mode text-mode :text "<<end>>\n") (:type host :range (171 . 187) :mode text-mode :text "\nClosing prose.\n")) :buffers ((:role host :mode text-mode :polymode t :specific t :indirect nil :narrowed nil :bounds (1 . 187) :indent pm-indent-line-dispatcher :fill polymode-fill-forward-paragraph :syntax polymode-syntax-propertize) (:role inner :mode emacs-lisp-mode :polymode t :specific t :indirect t :narrowed nil :bounds (1 . 187) :indent pm-indent-line-dispatcher :fill polymode-fill-forward-paragraph :syntax polymode-syntax-propertize)) :keys (("M-n C-n" polymode-next-chunk) ("M-n C-p" polymode-previous-chunk) ("M-n C-M-n" polymode-next-chunk-same-type) ("M-n C-M-p" polymode-previous-chunk-same-type) ("M-n C-t" polymode-toggle-chunk-narrowing) ("M-n M-m" polymode-mark-or-extend-chunk) ("M-n M-w" polymode-kill-ring-save-chunk) ("M-n M-k" polymode-kill-chunk)))"#
        ]],
    )
}

fn public_navigation_and_narrowing_follow_chunk_modes() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_navigation_and_narrowing_follow_chunk_modes",
        r####"(pm377-test-run
 "navigation"
 (lambda ()
   (let ((base (pm377-test-open "navigation.pm" pm377-test-fixture)))
     (set-window-buffer (selected-window) base)
     (with-current-buffer base (goto-char (point-min)))
     (let ((start (pm377-test-selected-state))
           (next (pm377-test-call-key "M-n C-n"))
           (next-again (pm377-test-call-key "M-n C-n"))
           (previous (pm377-test-call-key "M-n C-p"))
           (same-type (pm377-test-call-key "M-n C-M-n"))
           (_ (pm377-test-select-text base "message \"second"))
           (body-before (pm377-test-selected-state))
           (narrowed (pm377-test-call-key "M-n C-t"))
           (widened (pm377-test-call-key "M-n C-t")))
       (list :start start :next next :next-again next-again
             :previous previous :same-type same-type
             :body-before body-before
             :narrowed narrowed :widened widened
             :text (pm377-test-base-text base))))))"####,
        expect![[
            r#"OK (:start (:role host :mode text-mode :file "navigation.pm" :point 1 :line 1 :column 0 :narrowed nil :bounds (1 . 187)) :next (:role host :mode text-mode :file "navigation.pm" :point 34 :line 4 :column 0 :narrowed nil :bounds (1 . 187)) :next-again (:role host :mode text-mode :file "navigation.pm" :point 110 :line 9 :column 0 :narrowed nil :bounds (1 . 187)) :previous (:role host :mode text-mode :file "navigation.pm" :point 34 :line 4 :column 0 :narrowed nil :bounds (1 . 187)) :same-type (:role host :mode text-mode :file "navigation.pm" :point 139 :line 12 :column 0 :narrowed nil :bounds (1 . 187)) :body-before (:role inner :mode emacs-lisp-mode :file "navigation.pm" :point 140 :line 12 :column 1 :narrowed nil :bounds (1 . 187)) :narrowed (:role inner :mode emacs-lisp-mode :file "navigation.pm" :point 140 :line 1 :column 1 :narrowed t :bounds (139 . 163)) :widened (:role inner :mode emacs-lisp-mode :file "navigation.pm" :point 140 :line 12 :column 1 :narrowed nil :bounds (1 . 187)) :text "Release notes café 界.\n\n<<elisp>>\n;; Compute λ exactly.\n(defun total-界 (items)\n(+ 1 (length items)))\n<<end>>\n\nBetween chunks Ω.\n\n<<elisp>>\n(message \"second café\")\n<<end>>\n\nClosing prose.\n")"#
        ]],
    )
}

fn public_mark_and_copy_ranges_then_kill_reparses_remaining_fence() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_mark_and_copy_ranges_then_kill_reparses_remaining_fence",
        r####"(pm377-test-run
 "chunk-editing"
 (lambda ()
   (let ((base (pm377-test-open "chunk editing.pm" pm377-test-fixture)))
     (setq kill-ring nil kill-ring-yank-pointer nil)
     (pm377-test-select-text base ";; Compute")
     (pm377-test-call-key "M-n M-m")
     (let ((marked (pm377-test-region-state)))
       (deactivate-mark)
       (pm377-test-call-key "M-n M-w")
       (let ((body-copy (substring-no-properties (car kill-ring))))
         (pm377-test-select-text base "<<elisp>>")
         (pm377-test-call-key "M-n M-w")
         (let ((whole-copy (substring-no-properties (car kill-ring))))
           (pm377-test-select-text base ";; Compute")
           (pm377-test-call-key "M-n M-k")
           (list :marked marked
                 :body-copy body-copy
                 :whole-copy whole-copy
                 :killed-text (pm377-test-base-text base)
                 :killed-spans (with-current-buffer base
                                 (pm377-test-span-state)))))))))"####,
        expect![[
            r#"OK (:marked (:point 34 :mark 101 :active t :range (34 . 101) :text ";; Compute λ exactly.\n(defun total-界 (items)\n(+ 1 (length items)))\n") :body-copy ";; Compute λ exactly.\n(defun total-界 (items)\n(+ 1 (length items)))\n" :whole-copy "<<elisp>>\n;; Compute λ exactly.\n(defun total-界 (items)\n(+ 1 (length items)))\n<<end>>\n" :killed-text "Release notes café 界.\n\n<<elisp>>\n\nBetween chunks Ω.\n\n<<elisp>>\n(message \"second café\")\n<<end>>\n\nClosing prose.\n" :killed-spans ((:type host :range (1 . 24) :mode text-mode :text "Release notes café 界.\n\n") (:type head :range (24 . 34) :mode text-mode :text "<<elisp>>\n") (:type body :range (34 . 88) :mode emacs-lisp-mode :text "\nBetween chunks Ω.\n\n<<elisp>>\n(message \"second café\")\n") (:type tail :range (88 . 96) :mode text-mode :text "<<end>>\n") (:type host :range (96 . 112) :mode text-mode :text "\nClosing prose.\n")))"#
        ]],
    )
}

fn mode_aware_indent_comment_fill_and_save_write_exact_bytes() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_aware_indent_comment_fill_and_save_write_exact_bytes",
        r####"(pm377-test-run
 "mode-aware-editing"
 (lambda ()
   (let* ((base (pm377-test-open "Café document 界.pm"
                                 pm377-test-format-fixture))
          (file (buffer-file-name base)))
     (with-current-buffer base
       (indent-region (point-min) (point-max)))
     (let ((indented (pm377-test-base-text base)))
       (pm377-test-select-text base "(message")
       (let ((line-beg (line-beginning-position))
             (line-end (line-end-position)))
         (comment-region line-beg line-end))
       (let ((commented (pm377-test-base-text base)))
         (pm377-test-select-text base "This deliberately")
         (let ((fill-column 44))
           (fill-paragraph nil))
         (let ((filled (pm377-test-base-text base)))
           (with-current-buffer base
             (save-buffer))
           (let ((saved (pm377-test-read file)))
             (list :indented indented
                   :commented commented
                   :filled filled
                   :saved saved
                   :saved-equal (equal saved filled)
                   :sha256 (secure-hash 'sha256 saved)
                   :modified (buffer-modified-p base)
                   :spans (with-current-buffer base
                            (pm377-test-span-state))))))))))"####,
        expect![[
            r#"OK (:indented "This deliberately long host paragraph contains café, lambda λ, and world 界 so Polymode must fill it using the host text mode rather than the inner Lisp mode.\n\n<<elisp>>\n(defun greeting-界 (name)\n  (message \"hello %s λ\" name))\n<<end>>\n\nTrailing host prose Ω.\n" :commented "This deliberately long host paragraph contains café, lambda λ, and world 界 so Polymode must fill it using the host text mode rather than the inner Lisp mode.\n\n<<elisp>>\n(defun greeting-界 (name)\n  ;; (message \"hello %s λ\" name))\n<<end>>\n\nTrailing host prose Ω.\n" :filled "This deliberately long host paragraph\ncontains café, lambda λ, and world 界 so\nPolymode must fill it using the host text\nmode rather than the inner Lisp mode.\n\n<<elisp>>\n(defun greeting-界 (name)\n  ;; (message \"hello %s λ\" name))\n<<end>>\n\nTrailing host prose Ω.\n" :saved "This deliberately long host paragraph\ncontains café, lambda λ, and world 界 so\nPolymode must fill it using the host text\nmode rather than the inner Lisp mode.\n\n<<elisp>>\n(defun greeting-界 (name)\n  ;; (message \"hello %s λ\" name))\n<<end>>\n\nTrailing host prose Ω.\n" :saved-equal t :sha256 "a630012f816aee9cd5345c6582cef7eaafc1bf9060d0f4986f68d3015fe0f786" :modified nil :spans ((:type host :range (1 . 160) :mode text-mode :text "This deliberately long host paragraph\ncontains café, lambda λ, and world 界 so\nPolymode must fill it using the host text\nmode rather than the inner Lisp mode.\n\n") (:type head :range (160 . 170) :mode text-mode :text "<<elisp>>\n") (:type body :range (170 . 229) :mode emacs-lisp-mode :text "(defun greeting-界 (name)\n  ;; (message \"hello %s λ\" name))\n") (:type tail :range (229 . 237) :mode text-mode :text "<<end>>\n") (:type host :range (237 . 261) :mode text-mode :text "\nTrailing host prose Ω.\n")))"#
        ]],
    )
}

fn incremental_fence_damage_and_repair_rebuilds_the_span_graph() -> ParityBatchCase {
    ParityBatchCase::value(
        "incremental_fence_damage_and_repair_rebuilds_the_span_graph",
        r####"(pm377-test-run
 "incremental-recovery"
 (lambda ()
   (let ((base (pm377-test-open "recovery.pm" pm377-test-fixture)))
     (with-current-buffer base
       (goto-char (point-min))
       (search-forward "<<end>>\n")
       (let* ((tail-end (point))
              (tail-beg (- tail-end (length "<<end>>\n")))
              (initial (pm377-test-span-state)))
         (delete-region tail-beg tail-end)
         (let ((missing-tail-text (pm377-test-base-text base))
               (missing-tail-spans (pm377-test-span-state)))
           (goto-char tail-beg)
           (insert "<<end>>\n")
           (let ((repaired (pm377-test-span-state)))
             (goto-char (point-min))
             (search-forward "<<elisp>>")
             (replace-match "<<broken>>" t t)
             (let ((missing-head (pm377-test-span-state)))
               (goto-char (point-min))
               (search-forward "<<broken>>")
               (replace-match "<<elisp>>" t t)
               (list :initial initial
                     :missing-tail-text missing-tail-text
                     :missing-tail-spans missing-tail-spans
                     :repaired repaired
                     :missing-head missing-head
                     :restored (pm377-test-span-state)
                     :buffers (pm377-test-buffer-state))))))))))"####,
        expect![[
            r#"OK (:initial ((:type host :range (1 . 24) :mode text-mode :text "Release notes café 界.\n\n") (:type head :range (24 . 34) :mode text-mode :text "<<elisp>>\n") (:type body :range (34 . 101) :mode emacs-lisp-mode :text ";; Compute λ exactly.\n(defun total-界 (items)\n(+ 1 (length items)))\n") (:type tail :range (101 . 109) :mode text-mode :text "<<end>>\n") (:type host :range (109 . 129) :mode text-mode :text "\nBetween chunks Ω.\n\n") (:type head :range (129 . 139) :mode text-mode :text "<<elisp>>\n") (:type body :range (139 . 163) :mode emacs-lisp-mode :text "(message \"second café\")\n") (:type tail :range (163 . 171) :mode text-mode :text "<<end>>\n") (:type host :range (171 . 187) :mode text-mode :text "\nClosing prose.\n")) :missing-tail-text "Release notes café 界.\n\n<<elisp>>\n;; Compute λ exactly.\n(defun total-界 (items)\n(+ 1 (length items)))\n\nBetween chunks Ω.\n\n<<elisp>>\n(message \"second café\")\n<<end>>\n\nClosing prose.\n" :missing-tail-spans ((:type host :range (1 . 24) :mode text-mode :text "Release notes café 界.\n\n") (:type head :range (24 . 34) :mode text-mode :text "<<elisp>>\n") (:type body :range (34 . 101) :mode emacs-lisp-mode :text ";; Compute λ exactly.\n(defun total-界 (items)\n(+ 1 (length items)))\n") (:type body :range (34 . 155) :mode emacs-lisp-mode :text ";; Compute λ exactly.\n(defun total-界 (items)\n(+ 1 (length items)))\n\nBetween chunks Ω.\n\n<<elisp>>\n(message \"second café\")\n") (:type tail :range (155 . 163) :mode text-mode :text "<<end>>\n") (:type host :range (163 . 179) :mode text-mode :text "\nClosing prose.\n")) :repaired ((:type host :range (1 . 24) :mode text-mode :text "Release notes café 界.\n\n") (:type head :range (24 . 34) :mode text-mode :text "<<elisp>>\n") (:type body :range (34 . 101) :mode emacs-lisp-mode :text ";; Compute λ exactly.\n(defun total-界 (items)\n(+ 1 (length items)))\n") (:type tail :range (101 . 109) :mode text-mode :text "<<end>>\n") (:type host :range (109 . 129) :mode text-mode :text "\nBetween chunks Ω.\n\n") (:type head :range (129 . 139) :mode text-mode :text "<<elisp>>\n") (:type body :range (139 . 163) :mode emacs-lisp-mode :text "(message \"second café\")\n") (:type tail :range (163 . 171) :mode text-mode :text "<<end>>\n") (:type host :range (171 . 187) :mode text-mode :text "\nClosing prose.\n")) :missing-head ((:type host :range (1 . 24) :mode text-mode :text "Release notes café 界.\n\n") (:type host :range (1 . 130) :mode text-mode :text "Release notes café 界.\n\n<<broken>>\n;; Compute λ exactly.\n(defun total-界 (items)\n(+ 1 (length items)))\n<<end>>\n\nBetween chunks Ω.\n\n") (:type head :range (130 . 140) :mode text-mode :text "<<elisp>>\n") (:type body :range (140 . 164) :mode emacs-lisp-mode :text "(message \"second café\")\n") (:type tail :range (164 . 172) :mode text-mode :text "<<end>>\n") (:type host :range (172 . 188) :mode text-mode :text "\nClosing prose.\n")) :restored ((:type host :range (1 . 24) :mode text-mode :text "Release notes café 界.\n\n") (:type head :range (24 . 34) :mode text-mode :text "<<elisp>>\n") (:type body :range (34 . 101) :mode emacs-lisp-mode :text ";; Compute λ exactly.\n(defun total-界 (items)\n(+ 1 (length items)))\n") (:type tail :range (101 . 109) :mode text-mode :text "<<end>>\n") (:type host :range (109 . 129) :mode text-mode :text "\nBetween chunks Ω.\n\n") (:type head :range (129 . 139) :mode text-mode :text "<<elisp>>\n") (:type body :range (139 . 163) :mode emacs-lisp-mode :text "(message \"second café\")\n") (:type tail :range (163 . 171) :mode text-mode :text "<<end>>\n") (:type host :range (171 . 187) :mode text-mode :text "\nClosing prose.\n")) :buffers ((:role host :mode text-mode :polymode t :specific t :indirect nil :narrowed nil :bounds (1 . 187) :indent pm-indent-line-dispatcher :fill polymode-fill-forward-paragraph :syntax polymode-syntax-propertize) (:role inner :mode emacs-lisp-mode :polymode t :specific t :indirect t :narrowed nil :bounds (1 . 187) :indent pm-indent-line-dispatcher :fill polymode-fill-forward-paragraph :syntax polymode-syntax-propertize)))"#
        ]],
    )
}

#[test]
fn polymode_practical_workflows_batch() {
    let cases = vec![
        activation_builds_exact_span_and_indirect_buffer_topology(),
        public_navigation_and_narrowing_follow_chunk_modes(),
        public_mark_and_copy_ranges_then_kill_reparses_remaining_fence(),
        mode_aware_indent_comment_fill_and_save_write_exact_bytes(),
        incremental_fence_damage_and_repair_rebuilds_the_span_graph(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "polymode_practical_workflows_batch",
        "polymode_parity",
        &cases,
    );
}

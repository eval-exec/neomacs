//! Practical parity for Fish Mode's public script-editing commands.
//!
//! These cases open a real `.fish` file, fontify and indent a production
//! script, run the documented `fish_indent` save hook through an owned
//! stand-in, and recover after unmatched `end`/`case` and a formatter
//! failure.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, FISH_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'fish-mode)
(set-window-configuration (current-window-configuration))

(defconst fm427-test-tree
  "149342ae80dcd15e9b88742c43cb12642c57077d")
(defconst fm427-test-manifest
  '(("fish-mode-pkg.el" . "5ac1c06357e4ff0c7345fc394a526196ec630101fc430fdb65ba3ada12c4e2e4")
    ("fish-mode.el" . "8f708b4c719c8800084551c079357508598ce0e4fc9f68930dd5cff5f9df5ceb")))

(defvar fm427-test-case-index 0)
(defvar fm427-test-root nil)
(defvar fm427-test-root-owned nil)
(defvar fm427-test-indent-plan nil)
(defvar fm427-test-indent-ledger nil)

(defun fm427-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun fm427-test-source-state ()
  (let* ((located (locate-library "fish-mode.el"))
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
                         (cons file (fm427-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/fish-mode.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car fm427-test-manifest)))
      (error "Unexpected installed Fish Mode payload: %S"
             (or manifest files)))
    (dolist (entry fm427-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (fm427-test-sha file) expected))
          (error "Unexpected installed Fish Mode source: %S"
                 (cons entry manifest)))))
    (list :tree fm427-test-tree
          :manifest manifest
          :feature (featurep 'fish-mode)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'fish-mode package-alist)))))))

(defun fm427-test-syntax-of (char)
  (let ((raw (char-to-string (char-syntax char))))
    (list char (copy-sequence raw))))

(defun fm427-test-face-runs ()
  (font-lock-ensure)
  (let ((position (point-min))
        runs)
    (while (< position (point-max))
      (let* ((face (get-text-property position 'face))
             (next (next-single-property-change
                    position 'face nil (point-max))))
        (when face
          (push (list position next
                      (buffer-substring-no-properties position next)
                      face)
                runs))
        (setq position next)))
    (nreverse runs)))

(defun fm427-test-faces-where (needle)
  (save-excursion
    (goto-char (point-min))
    (unless (search-forward needle nil t)
      (error "Missing Fish Mode needle: %s" needle))
    (let ((start (match-beginning 0))
          (end (match-end 0))
          (position (match-beginning 0))
          segments)
      (while (< position end)
        (let ((next (min end (next-single-property-change
                              position 'face nil end))))
          (push (list (- position start)
                      (- next start)
                      (buffer-substring-no-properties position next)
                      (get-text-property position 'face))
                segments)
          (setq position next)))
      (list (copy-sequence needle) start end (nreverse segments)))))

(defun fm427-test-syntax-at (needle &optional offset)
  (save-excursion
    (goto-char (point-min))
    (search-forward needle)
    (goto-char (+ (match-beginning 0) (or offset 0)))
    (let ((state (syntax-ppss)))
      (list (copy-sequence needle)
            (point)
            (nth 0 state)
            (nth 3 state)
            (nth 4 state)
            (nth 8 state)))))

(defun fm427-test-line-indents ()
  (save-excursion
    (goto-char (point-min))
    (let (indents)
      (while (not (eobp))
        (push (list (line-number-at-pos)
                    (current-indentation)
                    (buffer-substring-no-properties
                     (line-beginning-position) (line-end-position)))
              indents)
        (forward-line 1))
      (nreverse indents))))

(defun fm427-test-condition (thunk)
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

(defun fm427-test-forbid-external (operation &rest arguments)
  (error "Unexpected Fish Mode external boundary: %S %S"
         operation arguments))

(defun fm427-test-call-process-region
    (start end program &optional delete destination display &rest arguments)
  (unless fm427-test-indent-plan
    (error "Unexpected fish_indent invocation"))
  (unless (and (equal program "fish_indent")
               (eq delete t)
               (eq destination t)
               (null display)
               (null arguments))
    (error "Unexpected fish_indent call: %S"
           (list start end program delete destination display arguments)))
  (let* ((input (buffer-substring-no-properties start end))
         (plan (pop fm427-test-indent-plan))
         (status (plist-get plan :status))
         (output (plist-get plan :output)))
    (push (list :input (copy-sequence input)
                :argv (cons program (copy-sequence arguments))
                :status status
                :output (copy-sequence output))
          fm427-test-indent-ledger)
    (goto-char start)
    (delete-region start end)
    (insert output)
    status))

(defun fm427-test-write (relative contents)
  (let ((file (expand-file-name relative fm427-test-root)))
    (unless (and fm427-test-root-owned
                 (file-in-directory-p file fm427-test-root))
      (error "Refusing Fish Mode write outside owned root: %s" file))
    (make-directory (file-name-directory file) t)
    (let ((coding-system-for-write 'utf-8-unix))
      (with-temp-file file (insert contents)))
    file))

(defun fm427-test-file-bytes (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (buffer-string)))

(defun fm427-test-type (text)
  (dolist (char (string-to-list text))
    (setq last-command-event char)
    (self-insert-command 1)))

(defun fm427-test-run (body)
  (let* ((index (cl-incf fm427-test-case-index))
         (sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (format "fish-mode-%d" index)
                                       sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (selected-window-before (selected-window))
         (window-before (current-window-configuration))
         (source-before (fm427-test-source-state))
         (directory-before default-directory)
         (enable-local-before enable-local-variables)
         (debug-before debug-on-error)
         (print-circle-before print-circle)
         (offset-before fish-indent-offset)
         (auto-indent-before fish-enable-auto-indent)
         (mode-hook-before fish-mode-hook)
         (before-save-before before-save-hook)
         (fm427-test-root root)
         (fm427-test-root-owned nil)
         (fm427-test-indent-plan nil)
         (fm427-test-indent-ledger nil)
         result body-error source-after cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute Fish Mode sandbox root"))
              (when (file-exists-p root)
                (error "Fish Mode sandbox root exists: %S" root))
              (make-directory root)
              (setq fm427-test-root-owned t
                    enable-local-variables nil
                    debug-on-error nil
                    print-circle nil
                    default-directory root)
              (cl-letf (((symbol-function 'call-process)
                         (lambda (&rest args)
                           (apply #'fm427-test-forbid-external
                                  'call-process args)))
                        ((symbol-function 'call-process-region)
                         #'fm427-test-call-process-region)
                        ((symbol-function 'make-process)
                         (lambda (&rest args)
                           (apply #'fm427-test-forbid-external
                                  'make-process args)))
                        ((symbol-function 'process-file)
                         (lambda (&rest args)
                           (apply #'fm427-test-forbid-external
                                  'process-file args)))
                        ((symbol-function 'start-file-process)
                         (lambda (&rest args)
                           (apply #'fm427-test-forbid-external
                                  'start-file-process args)))
                        ((symbol-function 'start-process)
                         (lambda (&rest args)
                           (apply #'fm427-test-forbid-external
                                  'start-process args)))
                        ((symbol-function 'url-retrieve)
                         (lambda (&rest args)
                           (apply #'fm427-test-forbid-external
                                  'url-retrieve args)))
                        ((symbol-function 'url-retrieve-synchronously)
                         (lambda (&rest args)
                           (apply #'fm427-test-forbid-external
                                  'url-retrieve-synchronously args))))
                (setq result (funcall body root)))
              (when fm427-test-indent-plan
                (error "Unused fish_indent plan: %S" fm427-test-indent-plan))
              (setq source-after (fm427-test-source-state))
              (unless (equal source-before source-after)
                (error "Fish Mode source changed")))
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
        (setq fish-indent-offset offset-before
              fish-enable-auto-indent auto-indent-before
              fish-mode-hook mode-hook-before
              before-save-hook before-save-before
              enable-local-variables enable-local-before
              debug-on-error debug-before
              print-circle print-circle-before
              default-directory directory-before)
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
        (when fm427-test-root-owned
          (attempt 'root (lambda () (delete-directory root t))))))
    (when body-error
      (error "Fish Mode body failed: %S" body-error))
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
                 :offset-restored (eq fish-indent-offset offset-before)
                 :auto-indent-restored (eq fish-enable-auto-indent
                                           auto-indent-before)
                 :hooks-restored
                 (and (equal fish-mode-hook mode-hook-before)
                      (equal before-save-hook before-save-before))
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if cleanup-errors
          (error "Fish Mode cleanup failed: %S" (list result cleanup))
        (list :source source-before
              :result result
              :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(FISH_MODE_MELPA_PIN, "fish-mode.el")
        .expect("prepare pinned fish-mode source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn opens_a_real_fish_script_and_selects_the_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "opens_a_real_fish_script_and_selects_the_mode",
        r####"
(fm427-test-run
 (lambda (root)
   (let* ((script (fm427-test-write
                   "config.fish"
                   "#!/usr/bin/env fish\n# café 界 prompt\nfunction fish_prompt\n    echo $USER\nend\n"))
          (funced (fm427-test-write
                   "scratch/fish_funced.4242"
                   "function scratch_café\n    echo hi\nend\n"))
          (script-buffer nil)
          (funced-buffer nil)
          (shebang-buffer nil))
     (unwind-protect
         (progn
           (setq script-buffer (find-file-noselect script)
                 funced-buffer (find-file-noselect funced))
           (with-temp-buffer
             (insert "#!/usr/bin/fish\necho hi\n")
             (setq shebang-buffer (current-buffer))
             (set-auto-mode)
             (list
              :script
              (with-current-buffer script-buffer
                (list :file (file-relative-name buffer-file-name root)
                      :mode major-mode
                      :derived (derived-mode-p 'prog-mode)
                      :mode-name (copy-sequence mode-name)
                      :indent indent-line-function
                      :offset fish-indent-offset
                      :comments (list comment-start comment-start-skip)
                      :font-lock (copy-tree font-lock-defaults)
                      :syntax (mapcar #'fm427-test-syntax-of
                                      '(?# ?\n ?\" ?' ?\\ ?$))
                      :modified (buffer-modified-p)))
              :funced
              (with-current-buffer funced-buffer
                (list :file (file-relative-name buffer-file-name root)
                      :mode major-mode))
              :shebang (list :mode major-mode)
              :auto-mode
              (list (cdr (assoc "\\.fish\\'" auto-mode-alist))
                    (cdr (assoc "/fish_funced\\..*\\'" auto-mode-alist)))
              :interpreter (cdr (assoc "fish" interpreter-mode-alist)))))
       (when (buffer-live-p script-buffer)
         (with-current-buffer script-buffer
           (set-buffer-modified-p nil))
         (kill-buffer script-buffer))
       (when (buffer-live-p funced-buffer)
         (with-current-buffer funced-buffer
           (set-buffer-modified-p nil))
         (kill-buffer funced-buffer))))))
"####,
        expect![[
            r##"OK (:source (:tree "149342ae80dcd15e9b88742c43cb12642c57077d" :manifest (("fish-mode-pkg.el" . "5ac1c06357e4ff0c7345fc394a526196ec630101fc430fdb65ba3ada12c4e2e4") ("fish-mode.el" . "8f708b4c719c8800084551c079357508598ce0e4fc9f68930dd5cff5f9df5ceb")) :feature t :version "20240129.1213") :result (:script (:file "config.fish" :mode fish-mode :derived prog-mode :mode-name "Fish" :indent fish-indent-line :offset 4 :comments ("# " "#+[\11 ]*") :font-lock (fish-font-lock-keywords-1) :syntax ((35 "<") (10 ">") (34 "\"") (39 "\"") (92 "\\") (36 "'")) :modified nil) :funced (:file "scratch/fish_funced.4242" :mode fish-mode) :shebang (:mode fish-mode) :auto-mode (fish-mode fish-mode) :interpreter fish-mode) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :offset-restored t :auto-indent-restored t :hooks-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn fontifies_a_production_script_by_kind() -> ParityBatchCase {
    ParityBatchCase::value(
        "fontifies_a_production_script_by_kind",
        r####"
(fm427-test-run
 (lambda (_root)
   (with-temp-buffer
     (insert
      "#!/usr/bin/env fish\n"
      "# Deploy café widgets to 界\n"
      "function deploy_café --description \"Ship café 界\" --on-event fish_prompt -a dest\n"
      "    set -l dest /srv/café\n"
      "    set -gx PATH $dest/bin $PATH\n"
      "    if not test -d \"$dest\"\n"
      "        echo \"missing $dest\" >&2\n"
      "        return 1\n"
      "    end\n"
      "    for name in widget-α widget-β\n"
      "        echo $name | string collect\n"
      "    end\n"
      "    switch $argv[1]\n"
      "        case start\n"
      "            echo 42\n"
      "        case stop\n"
      "            echo -1\n"
      "    end\n"
      "    count (ls $dest)\n"
      "    math 3.14\n"
      "    echo hello \\\n"
      "        world\n"
      "end\n"
      "set setting leftover\n"
      "functional leftover\n")
     (fish-mode)
     (font-lock-ensure)
     (list
      :mode major-mode
      :comment (fm427-test-faces-where "Deploy café")
      :function-name (fm427-test-faces-where "deploy_café")
      :function-option (fm427-test-faces-where "--description")
      :set-option (fm427-test-faces-where "-l")
      :set-variable (fm427-test-faces-where "dest /srv")
      :dollar (fm427-test-faces-where "$dest/bin")
      :not (fm427-test-faces-where "not")
      :test (fm427-test-faces-where "test")
      :quoted-dest (fm427-test-faces-where "\"$dest\"")
      :redirect (fm427-test-faces-where ">&2")
      :return (fm427-test-faces-where "return")
      :for-in (fm427-test-faces-where "name in widget")
      :pipe-builtin (fm427-test-faces-where "string")
      :switch (fm427-test-faces-where "switch")
      :case (fm427-test-faces-where "case start")
      :number (fm427-test-faces-where "42")
      :negative (fm427-test-faces-where "-1")
      :float (fm427-test-faces-where "3.14")
      :process (fm427-test-faces-where "(ls $dest)")
      :backslash (fm427-test-faces-where "\\")
      :string-keyword (fm427-test-faces-where "Ship café")
      :comment-keyword (fm427-test-syntax-at "Deploy" 2)
      :setting (fm427-test-faces-where "setting")
      :functional (fm427-test-faces-where "functional")
      :faces (fm427-test-face-runs)))))
"####,
        expect![[
            r##"OK (:source (:tree "149342ae80dcd15e9b88742c43cb12642c57077d" :manifest (("fish-mode-pkg.el" . "5ac1c06357e4ff0c7345fc394a526196ec630101fc430fdb65ba3ada12c4e2e4") ("fish-mode.el" . "8f708b4c719c8800084551c079357508598ce0e4fc9f68930dd5cff5f9df5ceb")) :feature t :version "20240129.1213") :result (:mode fish-mode :comment ("Deploy café" 23 34 ((0 11 "Deploy café" font-lock-comment-face))) :function-name ("deploy_café" 57 68 ((0 11 "deploy_café" font-lock-function-name-face))) :function-option ("--description" 69 82 ((0 13 "--description" font-lock-negation-char-face))) :set-option ("-l" 136 138 ((0 2 "-l" font-lock-negation-char-face))) :set-variable ("dest /srv" 139 148 ((0 4 "dest" font-lock-variable-name-face) (4 9 " /srv" nil))) :dollar ("$dest/bin" 171 180 ((0 1 "$" font-lock-string-face) (1 9 "dest/bin" font-lock-variable-name-face))) :not ("not" 194 197 ((0 3 "not" font-lock-negation-char-face))) :test ("test" 198 202 ((0 4 "test" font-lock-keyword-face))) :quoted-dest ("\"$dest\"" 206 213 ((0 7 "\"$dest\"" font-lock-string-face))) :redirect (">&2" 243 246 ((0 3 ">&2" font-lock-negation-char-face))) :return ("return" 255 261 ((0 6 "return" font-lock-keyword-face))) :for-in ("name in widget" 280 294 ((0 4 "name" font-lock-variable-name-face) (4 5 " " nil) (5 7 "in" font-lock-keyword-face) (7 8 " " nil) (8 14 "widget" font-lock-string-face))) :pipe-builtin ("string" 327 333 ((0 6 "string" font-lock-builtin-face))) :switch ("switch" 354 360 ((0 6 "switch" font-lock-keyword-face))) :case ("case start" 378 388 ((0 4 "case" font-lock-keyword-face) (4 5 " " nil) (5 10 "start" font-lock-builtin-face))) :number ("42" 406 408 ((0 2 "42" font-lock-constant-face))) :negative ("-1" 444 446 ((0 2 "-1" font-lock-constant-face))) :float ("3.14" 485 489 ((0 4 "3.14" font-lock-constant-face))) :process ("(ls $dest)" 465 475 ((0 1 "(" nil) (1 3 "ls" font-lock-builtin-face) (3 4 " " nil) (4 5 "$" font-lock-string-face) (5 9 "dest" font-lock-variable-name-face) (9 10 ")" nil))) :backslash ("\\" 505 506 ((0 1 "\\" font-lock-negation-char-face))) :string-keyword ("Ship café" 84 93 ((0 9 "Ship café" font-lock-string-face))) :comment-keyword ("Deploy" 25 0 nil t 21) :setting ("setting" 529 536 ((0 7 "setting" font-lock-variable-name-face))) :functional ("functional" 546 556 ((0 10 "functional" font-lock-builtin-face))) :faces ((1 2 "#" font-lock-comment-delimiter-face) (2 21 "!/usr/bin/env fish\n" font-lock-comment-face) (21 23 "# " font-lock-comment-delimiter-face) (23 48 "Deploy café widgets to 界\n" font-lock-comment-face) (48 56 "function" font-lock-keyword-face) (57 68 "deploy_café" font-lock-function-name-face) (69 82 "--description" font-lock-negation-char-face) (83 96 "\"Ship café 界\"" font-lock-string-face) (97 107 "--on-event" font-lock-negation-char-face) (108 119 "fish_prompt" font-lock-builtin-face) (120 122 "-a" font-lock-negation-char-face) (132 135 "set" font-lock-keyword-face) (136 138 "-l" font-lock-negation-char-face) (139 143 "dest" font-lock-variable-name-face) (158 161 "set" font-lock-keyword-face) (162 165 "-gx" font-lock-negation-char-face) (166 170 "PATH" font-lock-variable-name-face) (171 172 "$" font-lock-string-face) (172 180 "dest/bin" font-lock-variable-name-face) (181 182 "$" font-lock-string-face) (182 186 "PATH" font-lock-variable-name-face) (191 193 "if" font-lock-keyword-face) (194 197 "not" font-lock-negation-char-face) (198 202 "test" font-lock-keyword-face) (206 213 "\"$dest\"" font-lock-string-face) (222 226 "echo" font-lock-builtin-face) (227 242 "\"missing $dest\"" font-lock-string-face) (242 247 " >&2\n" font-lock-negation-char-face) (255 261 "return" font-lock-keyword-face) (262 263 "1" font-lock-builtin-face) (268 271 "end" font-lock-keyword-face) (276 279 "for" font-lock-keyword-face) (280 284 "name" font-lock-variable-name-face) (285 287 "in" font-lock-keyword-face) (288 305 "widget-α widget-β" font-lock-string-face) (314 318 "echo" font-lock-builtin-face) (319 320 "$" font-lock-string-face) (320 324 "name" font-lock-variable-name-face) (325 326 "|" font-lock-negation-char-face) (327 333 "string" font-lock-builtin-face) (346 349 "end" font-lock-keyword-face) (354 360 "switch" font-lock-keyword-face) (361 362 "$" font-lock-string-face) (362 366 "argv" font-lock-variable-name-face) (367 368 "1" font-lock-constant-face) (378 382 "case" font-lock-keyword-face) (383 388 "start" font-lock-builtin-face) (401 405 "echo" font-lock-builtin-face) (406 408 "42" font-lock-constant-face) (417 421 "case" font-lock-keyword-face) (422 426 "stop" font-lock-builtin-face) (439 443 "echo" font-lock-builtin-face) (444 446 "-1" font-lock-constant-face) (451 454 "end" font-lock-keyword-face) (459 464 "count" font-lock-builtin-face) (466 468 "ls" font-lock-builtin-face) (469 470 "$" font-lock-string-face) (470 474 "dest" font-lock-variable-name-face) (480 484 "math" font-lock-builtin-face) (485 489 "3.14" font-lock-constant-face) (494 498 "echo" font-lock-builtin-face) (505 506 "\\" font-lock-negation-char-face) (515 520 "world" font-lock-builtin-face) (521 524 "end" font-lock-keyword-face) (525 528 "set" font-lock-keyword-face) (529 536 "setting" font-lock-variable-name-face) (546 556 "functional" font-lock-builtin-face))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :offset-restored t :auto-indent-restored t :hooks-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn indents_nested_control_flow_continued_lines_and_typed_end() -> ParityBatchCase {
    ParityBatchCase::value(
        "indents_nested_control_flow_continued_lines_and_typed_end",
        r####"
(fm427-test-run
 (lambda (_root)
   (let ((source
          (concat
           "function print_status\n"
           "if test $status -eq 0\n"
           "echo ok\n"
           "else if test $status -eq 1\n"
           "echo retry\n"
           "else\n"
           "echo fail\n"
           "end\n"
           "for name in a b\n"
           "echo $name\n"
           "end\n"
           "switch $argv[1]\n"
           "case start\n"
           "if true\n"
           "echo 1\n"
           "end\n"
           "case stop\n"
           "echo 2\n"
           "end\n"
           "echo hello \\\n"
           "world\n"
           "echo \"this end is a string\"\n"
           "# this end is a comment\n"
           "end\n"))
         default-pass custom-pass typed)
     (with-temp-buffer
       (insert source)
       (fish-mode)
       (setq-local indent-tabs-mode nil)
       (setq fish-indent-offset 4)
       (indent-region (point-min) (point-max))
       (let ((first (buffer-substring-no-properties (point-min) (point-max))))
         (indent-region (point-min) (point-max))
         (setq default-pass
               (list :text (buffer-substring-no-properties
                            (point-min) (point-max))
                     :idempotent (equal first (buffer-substring-no-properties
                                               (point-min) (point-max)))
                     :indents (fm427-test-line-indents)
                     :offset fish-indent-offset))))
     (with-temp-buffer
       (insert source)
       (fish-mode)
       (setq-local indent-tabs-mode nil)
       (setq fish-indent-offset 2)
       (indent-region (point-min) (point-max))
       (setq custom-pass
             (list :text (buffer-substring-no-properties
                          (point-min) (point-max))
                   :indents (fm427-test-line-indents)
                   :offset fish-indent-offset)))
     (let ((fish-enable-auto-indent t))
       (with-temp-buffer
         (fish-mode)
         (setq-local indent-tabs-mode nil)
         (setq fish-indent-offset 4)
         (insert "if test $x\n    echo hi\n    ")
         (fm427-test-type "end")
         (setq typed
               (list :text (buffer-substring-no-properties
                            (point-min) (point-max))
                     :point (point)
                     :hook (and (memq #'fish/auto-indent
                                      post-self-insert-hook)
                                t)))))
     (list :default default-pass
           :custom custom-pass
           :typed typed
           :same-shape
           (equal (mapcar #'cadr (plist-get default-pass :indents))
                  (mapcar (lambda (row)
                            (* 2 (cadr row)))
                          (plist-get custom-pass :indents)))))))
"####,
        expect![[
            r#"OK (:source (:tree "149342ae80dcd15e9b88742c43cb12642c57077d" :manifest (("fish-mode-pkg.el" . "5ac1c06357e4ff0c7345fc394a526196ec630101fc430fdb65ba3ada12c4e2e4") ("fish-mode.el" . "8f708b4c719c8800084551c079357508598ce0e4fc9f68930dd5cff5f9df5ceb")) :feature t :version "20240129.1213") :result (:default (:text "function print_status\n    if test $status -eq 0\n        echo ok\n    else if test $status -eq 1\n        echo retry\n    else\n        echo fail\n    end\n    for name in a b\n        echo $name\n    end\n    switch $argv[1]\n        case start\n            if true\n                echo 1\n            end\n        case stop\n            echo 2\n    end\n    echo hello \\\n        world\n    echo \"this end is a string\"\n    # this end is a comment\nend\n" :idempotent t :indents ((1 0 "function print_status") (2 4 "    if test $status -eq 0") (3 8 "        echo ok") (4 4 "    else if test $status -eq 1") (5 8 "        echo retry") (6 4 "    else") (7 8 "        echo fail") (8 4 "    end") (9 4 "    for name in a b") (10 8 "        echo $name") (11 4 "    end") (12 4 "    switch $argv[1]") (13 8 "        case start") (14 12 "            if true") (15 16 "                echo 1") (16 12 "            end") (17 8 "        case stop") (18 12 "            echo 2") (19 4 "    end") (20 4 "    echo hello \\") (21 8 "        world") (22 4 "    echo \"this end is a string\"") (23 4 "    # this end is a comment") (24 0 "end")) :offset 4) :custom (:text "function print_status\n  if test $status -eq 0\n    echo ok\n  else if test $status -eq 1\n    echo retry\n  else\n    echo fail\n  end\n  for name in a b\n    echo $name\n  end\n  switch $argv[1]\n    case start\n      if true\n        echo 1\n      end\n    case stop\n      echo 2\n  end\n  echo hello \\\n    world\n  echo \"this end is a string\"\n  # this end is a comment\nend\n" :indents ((1 0 "function print_status") (2 2 "  if test $status -eq 0") (3 4 "    echo ok") (4 2 "  else if test $status -eq 1") (5 4 "    echo retry") (6 2 "  else") (7 4 "    echo fail") (8 2 "  end") (9 2 "  for name in a b") (10 4 "    echo $name") (11 2 "  end") (12 2 "  switch $argv[1]") (13 4 "    case start") (14 6 "      if true") (15 8 "        echo 1") (16 6 "      end") (17 4 "    case stop") (18 6 "      echo 2") (19 2 "  end") (20 2 "  echo hello \\") (21 4 "    world") (22 2 "  echo \"this end is a string\"") (23 2 "  # this end is a comment") (24 0 "end")) :offset 2) :typed (:text "if test $x\n    echo hi\nend" :point 27 :hook t) :same-shape t) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :offset-restored t :auto-indent-restored t :hooks-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn fish_indent_formats_on_save_and_recovers_after_formatter_failure() -> ParityBatchCase {
    ParityBatchCase::value(
        "fish_indent_formats_on_save_and_recovers_after_formatter_failure",
        r####"
(fm427-test-run
 (lambda (root)
   (add-hook 'fish-mode-hook
             (lambda ()
               (add-hook 'before-save-hook #'fish_indent-before-save)))
   (let* ((messy "function deploy_café\necho hi\nend\n")
          (formatted "function deploy_café\n    echo hi\nend\n")
          (script (fm427-test-write "deploy.fish" messy))
          (notes (fm427-test-write "notes.txt" "leave this ledger alone\n"))
          (script-buffer nil)
          (notes-buffer nil)
          saved other failed recovered)
     (setq fm427-test-indent-plan
           (list (list :status 0 :output formatted)
                 (list :status 1 :output "parse error: unexpected end\n")
                 (list :status 0 :output formatted)))
     (unwind-protect
         (progn
           (setq script-buffer (find-file-noselect script))
           (with-current-buffer script-buffer
             (goto-char 12)
             (set-buffer-modified-p t)
             (save-buffer)
             (setq saved
                   (list :mode major-mode
                         :text (buffer-substring-no-properties
                                (point-min) (point-max))
                         :point (point)
                         :file (copy-sequence (fm427-test-file-bytes script))
                         :hook (and (memq #'fish_indent-before-save
                                          before-save-hook)
                                    t)
                         :ledger (copy-tree fm427-test-indent-ledger))))
           (setq notes-buffer (find-file-noselect notes))
           (with-current-buffer notes-buffer
             (text-mode)
             (save-buffer)
             (setq other
                   (list :mode major-mode
                         :file (copy-sequence (fm427-test-file-bytes notes))
                         :ledger-count (length fm427-test-indent-ledger))))
           (with-current-buffer script-buffer
             (erase-buffer)
             (insert messy)
             (goto-char 12)
             (fish_indent)
             (setq failed
                   (list :text (buffer-substring-no-properties
                                (point-min) (point-max))
                         :point (point)
                         :status-ignored t))
             (erase-buffer)
             (insert messy)
             (goto-char 12)
             (fish_indent)
             (setq recovered
                   (list :text (buffer-substring-no-properties
                                (point-min) (point-max))
                         :point (point))))
           (list :saved saved
                 :other other
                 :failed failed
                 :recovered recovered
                 :ledger (nreverse (copy-tree fm427-test-indent-ledger))))
       (setq fm427-test-indent-ledger nil)
       (when (buffer-live-p script-buffer)
         (with-current-buffer script-buffer
           (set-buffer-modified-p nil))
         (kill-buffer script-buffer))
       (when (buffer-live-p notes-buffer)
         (with-current-buffer notes-buffer
           (set-buffer-modified-p nil))
         (kill-buffer notes-buffer))))))
"####,
        expect![[
            r#"OK (:source (:tree "149342ae80dcd15e9b88742c43cb12642c57077d" :manifest (("fish-mode-pkg.el" . "5ac1c06357e4ff0c7345fc394a526196ec630101fc430fdb65ba3ada12c4e2e4") ("fish-mode.el" . "8f708b4c719c8800084551c079357508598ce0e4fc9f68930dd5cff5f9df5ceb")) :feature t :version "20240129.1213") :result (:saved (:mode fish-mode :text "function deploy_café\n    echo hi\nend\n" :point 12 :file "function deploy_café\n    echo hi\nend\n" :hook t :ledger ((:input "function deploy_café\necho hi\nend\n" :argv ("fish_indent") :status 0 :output "function deploy_café\n    echo hi\nend\n"))) :other (:mode text-mode :file "leave this ledger alone\n" :ledger-count 1) :failed (:text "parse error: unexpected end\n" :point 12 :status-ignored t) :recovered (:text "function deploy_café\n    echo hi\nend\n" :point 12) :ledger ((:input "function deploy_café\necho hi\nend\n" :argv ("fish_indent") :status 0 :output "function deploy_café\n    echo hi\nend\n") (:input "function deploy_café\necho hi\nend\n" :argv ("fish_indent") :status 1 :output "parse error: unexpected end\n") (:input "function deploy_café\necho hi\nend\n" :argv ("fish_indent") :status 0 :output "function deploy_café\n    echo hi\nend\n"))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :offset-restored t :auto-indent-restored t :hooks-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn unmatched_end_and_case_signal_then_recover() -> ParityBatchCase {
    ParityBatchCase::value(
        "unmatched_end_and_case_signal_then_recover",
        r####"
(fm427-test-run
 (lambda (_root)
   (let (end-fail case-fail case-then-switch recovered string-end)
     (with-temp-buffer
       (insert "echo café\nend\n")
       (fish-mode)
       (goto-char (point-min))
       (forward-line 1)
       (setq end-fail (fm427-test-condition #'fish-indent-line)))
     (with-temp-buffer
       (insert "echo café\ncase start\n")
       (fish-mode)
       (goto-char (point-min))
       (forward-line 1)
       (setq case-fail (fm427-test-condition #'fish-indent-line)))
     (with-temp-buffer
       (insert "function deploy_café\ncase start\nend\n")
       (fish-mode)
       (setq-local indent-tabs-mode nil)
       (setq case-then-switch
             (fm427-test-condition
              (lambda ()
                (goto-char (point-min))
                (forward-line 1)
                (fish-indent-line))))
       (erase-buffer)
       (insert
        "function deploy_café\n"
        "switch $argv[1]\n"
        "case start\n"
        "echo hi\n"
        "end\n"
        "end\n")
       (indent-region (point-min) (point-max))
       (setq recovered
             (list :text (buffer-substring-no-properties
                          (point-min) (point-max))
                   :indents (fm427-test-line-indents))))
     (with-temp-buffer
       (insert
        "function deploy_café\n"
        "echo \"this end is a string\"\n"
        "# this end is a comment\n"
        "end\n")
       (fish-mode)
       (setq-local indent-tabs-mode nil)
       (setq string-end
             (fm427-test-condition
              (lambda ()
                (indent-region (point-min) (point-max))
                (list :text (buffer-substring-no-properties
                             (point-min) (point-max))
                      :comment (fm427-test-syntax-at "this end is a comment" 7)
                      :string (fm427-test-syntax-at "this end is a string" 6))))))
     (list :end end-fail
           :case case-fail
           :case-without-switch case-then-switch
           :recovered recovered
           :string-and-comment-end string-end))))
"####,
        expect![[
            r#"OK (:source (:tree "149342ae80dcd15e9b88742c43cb12642c57077d" :manifest (("fish-mode-pkg.el" . "5ac1c06357e4ff0c7345fc394a526196ec630101fc430fdb65ba3ada12c4e2e4") ("fish-mode.el" . "8f708b4c719c8800084551c079357508598ce0e4fc9f68930dd5cff5f9df5ceb")) :feature t :version "20240129.1213") :result (:end (:error error :data ("Found unmatched ’end’ term.") :message "Found unmatched ’end’ term.") :case (:error error :data ("Found ’case’ term without matching ’switch’ term") :message "Found ’case’ term without matching ’switch’ term") :case-without-switch (:error error :data ("Found ’case’ term without matching ’switch’ term") :message "Found ’case’ term without matching ’switch’ term") :recovered (:text "function deploy_café\n    switch $argv[1]\n        case start\n            echo hi\n    end\nend\n" :indents ((1 0 "function deploy_café") (2 4 "    switch $argv[1]") (3 8 "        case start") (4 12 "            echo hi") (5 4 "    end") (6 0 "end"))) :string-and-comment-end (:returned (:text "function deploy_café\n    echo \"this end is a string\"\n    # this end is a comment\nend\n" :comment ("this end is a comment" 67 0 nil t 58) :string ("this end is a string" 38 0 34 nil 31)))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :offset-restored t :auto-indent-restored t :hooks-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn fish_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        opens_a_real_fish_script_and_selects_the_mode(),
        fontifies_a_production_script_by_kind(),
        indents_nested_control_flow_continued_lines_and_typed_end(),
        fish_indent_formats_on_save_and_recovers_after_formatter_failure(),
        unmatched_end_and_case_signal_then_recover(),
    ];
    assert_oracle_batch_cases(oracle(), "fish-mode-rank427", "fish_mode_parity", &cases);
}

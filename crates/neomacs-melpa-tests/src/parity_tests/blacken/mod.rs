use std::time::Duration;

use expect_test::expect;

use crate::{BLACKEN_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'python)
(require 'blacken)

(defvar blacken374-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defconst blacken374-test-fixed-buffer-names
  '("*blacken*" "*blacken-error*"))

(defun blacken374-test-case-root (name)
  (let ((root (file-name-as-directory
               (expand-file-name (concat "blacken374/" name "/")
                                 blacken374-test-root))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun blacken374-test-write (path bytes &optional executable)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert bytes)
      (write-region (point-min) (point-max) path nil 'silent)))
  (set-file-modes path (if executable #o755 #o644))
  path)

(defun blacken374-test-read-text (path)
  (when (file-exists-p path)
    (let ((coding-system-for-read 'utf-8-unix))
      (with-temp-buffer
        (insert-file-contents path)
        (buffer-string)))))

(defun blacken374-test-sha256 (bytes)
  (secure-hash 'sha256 bytes))

(defun blacken374-test-normalize-root (value root)
  (replace-regexp-in-string
   (regexp-quote (directory-file-name root))
   "[ROOT]" value t t))

(defun blacken374-test-install-adapter (root)
  (blacken374-test-write
   (expand-file-name "bin/owned-black" root)
   (concat
    "#!/bin/sh\n"
    "set -eu\n"
    "copy_file() {\n"
    "  while IFS= read -r line || [ -n \"$line\" ]; do\n"
    "    printf '%s\\n' \"$line\"\n"
    "  done < \"$1\"\n"
    "}\n"
    "printf '%s\\n' 'CALL' >> \"$BLACKEN374_TRANSCRIPT\"\n"
    "printf 'PWD\\t%s\\n' \"$PWD\" >> \"$BLACKEN374_TRANSCRIPT\"\n"
    "for arg in \"$@\"; do\n"
    "  printf 'ARG\\t%s\\n' \"$arg\" >> \"$BLACKEN374_TRANSCRIPT\"\n"
    "done\n"
    ": > \"$BLACKEN374_INPUT\"\n"
    "while IFS= read -r line || [ -n \"$line\" ]; do\n"
    "  printf '%s\\n' \"$line\" >> \"$BLACKEN374_INPUT\"\n"
    "done\n"
    "printf '%s\\n' 'INPUT-BEGIN' >> \"$BLACKEN374_TRANSCRIPT\"\n"
    "copy_file \"$BLACKEN374_INPUT\" >> \"$BLACKEN374_TRANSCRIPT\"\n"
    "printf '%s\\n' 'INPUT-END' >> \"$BLACKEN374_TRANSCRIPT\"\n"
    "case \"$BLACKEN374_MODE\" in\n"
    "  success) copy_file \"$BLACKEN374_OUTPUT\" ;;\n"
    "  failure)\n"
    "    printf '%s\\n' 'owned black failure: invalid syntax Ω' >&2\n"
    "    exit 23\n"
    "    ;;\n"
    "  *) printf '%s\\n' 'unknown owned black mode' >&2; exit 86 ;;\n"
    "esac\n")
   t))

(defun blacken374-test-with-adapter (root mode thunk)
  (let* ((adapter (blacken374-test-install-adapter root))
         (trace (expand-file-name "adapter/transcript" root))
         (input (expand-file-name "adapter/stdin.py" root))
         (output (expand-file-name "adapter/stdout.py" root))
         (process-environment (copy-sequence process-environment))
         (default-process-coding-system '(utf-8-unix . utf-8-unix)))
    (make-directory (file-name-directory trace) t)
    (setenv "BLACKEN374_TRANSCRIPT" trace)
    (setenv "BLACKEN374_INPUT" input)
    (setenv "BLACKEN374_OUTPUT" output)
    (setenv "BLACKEN374_MODE" mode)
    (funcall thunk adapter trace input output)))

(defun blacken374-test-owned-buffer-p (buffer root)
  (or (member (buffer-name buffer) blacken374-test-fixed-buffer-names)
      (string-prefix-p " *blacken374-" (buffer-name buffer))
      (let ((file (buffer-file-name buffer)))
        (and file (file-in-directory-p file root)))))

(defun blacken374-test-run-case (name thunk)
  (when (seq-some #'get-buffer blacken374-test-fixed-buffer-names)
    (error "blacken fixed output buffer leaked into case %s" name))
  (let ((root (blacken374-test-case-root name))
        result)
    (unwind-protect
        (setq result
              (save-excursion
                (save-window-excursion
                  (funcall thunk root))))
      (let* ((owned-processes
             (seq-filter
              (lambda (process)
                (string-prefix-p "blacken" (process-name process)))
              (process-list)))
             (live-processes
              (progn
                (dolist (process owned-processes)
                  (let ((remaining 20))
                    (while (and (process-live-p process) (> remaining 0))
                      (accept-process-output process 0.01)
                      (setq remaining (1- remaining)))))
                (seq-filter #'process-live-p owned-processes))))
        (dolist (process owned-processes)
          (ignore-errors (delete-process process)))
        (dolist (buffer (buffer-list))
          (when (blacken374-test-owned-buffer-p buffer root)
            (with-current-buffer buffer
              (set-buffer-modified-p nil))
            (kill-buffer buffer)))
        (when (file-exists-p root)
          (delete-directory root t))
        (when live-processes
          (error "live blacken process leaked from case %s: %S"
                 name (mapcar #'process-name live-processes)))))
    result))

(defun blacken374-test-mode-state ()
  (list :enabled blacken-mode
        :lighter (copy-tree (assq 'blacken-mode minor-mode-alist))
        :hook-count (cl-count 'blacken-buffer before-save-hook :test #'eq)
        :hook-local (local-variable-p 'before-save-hook)))

(defun blacken374-test-buffer-state ()
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :mark (mark t)
        :mark-active mark-active
        :modified (buffer-modified-p)
        :coding buffer-file-coding-system
        :narrowed (buffer-narrowed-p)
        :mode major-mode
        :blacken (blacken374-test-mode-state)))

(defun blacken374-test-window-state (buffer)
  (mapcar
   (lambda (window)
     (list :point (window-point window)
           :start (window-start window)
           :selected (eq window (selected-window))
           :buffer (eq (window-buffer window) buffer)))
   (sort (get-buffer-window-list buffer nil t)
         (lambda (left right)
           (let ((left-edges (window-edges left))
                 (right-edges (window-edges right)))
             (or (< (car left-edges) (car right-edges))
                 (and (= (car left-edges) (car right-edges))
                      (< (cadr left-edges) (cadr right-edges)))))))))

(defun blacken374-test-transcript (path root)
  (when-let ((text (blacken374-test-read-text path)))
    (blacken374-test-normalize-root text root)))

(defun blacken374-test-call-count (path)
  (cl-count "CALL"
            (split-string (or (blacken374-test-read-text path) "") "\n" t)
            :test #'equal))

(defun blacken374-test-last-message ()
  (with-current-buffer (messages-buffer)
    (save-excursion
      (goto-char (point-max))
      (skip-chars-backward "\n")
      (buffer-substring-no-properties
       (line-beginning-position) (line-end-position)))))

(defun blacken374-test-settle-error-pipe ()
  (when-let ((process (get-buffer-process "*blacken-error*")))
    (let ((remaining 100))
      (while (and (process-live-p process) (> remaining 0))
        (accept-process-output process 0.01)
        (setq remaining (1- remaining)))
      (accept-process-output process 0.01)
      (when (process-live-p process)
        (error "blacken stderr pipe did not settle")))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(BLACKEN_MELPA_PIN, "blacken.el")
        .expect("prepare pinned blacken source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_buffer_formatting_preserves_every_visible_window_view() -> ParityBatchCase {
    let form = r####"
(blacken374-test-run-case
 "visible-windows"
 (lambda (root)
   (let* ((input
           "def release( user ,items ):\n\tresult={\"user\":user,\"items\":items}\n\tfor item in items:\n\t\tresult[item[\"sku\"]]=item[\"qty\"]\n\treturn result\n\nsummary = release(\"界\", [{\"sku\":\"A-1\",\"qty\":2}])\nprint( summary )\n")
          (output
           "def release(user, items):\n    result = {\"user\": user, \"items\": items}\n    for item in items:\n        result[item[\"sku\"]] = item[\"qty\"]\n    return result\n\n\nsummary = release(\"界\", [{\"sku\": \"A-1\", \"qty\": 2}])\nprint(summary)\n")
          (source (generate-new-buffer " *blacken374-visible*"))
          (default-directory root))
     (blacken374-test-with-adapter
      root "success"
      (lambda (adapter trace _input-file output-file)
        (blacken374-test-write output-file output)
        (with-current-buffer source
          (setq default-directory root)
          (set-buffer-file-coding-system 'utf-8-unix)
          (insert input)
          (python-mode)
          (buffer-enable-undo)
          (goto-char 13)
          (set-mark 5)
          (setq mark-active t)
          (set-buffer-modified-p nil))
        (switch-to-buffer source)
        (delete-other-windows)
        (let* ((left (selected-window))
               (right (split-window-right))
               (right-point
                (with-current-buffer source
                  (save-excursion
                    (goto-char (point-min))
                    (search-forward "return result")
                    (point))))
               (right-start
                (with-current-buffer source
                  (save-excursion
                    (goto-char (point-min))
                    (forward-line 2)
                    (point)))))
          (set-window-buffer right source)
          (set-window-point left 13)
          (set-window-start left 1 t)
          (set-window-point right right-point)
          (set-window-start right right-start t)
          (select-window left)
          (with-current-buffer source (goto-char 13))
          (let ((before-state (with-current-buffer source
                                (blacken374-test-buffer-state)))
                (before-windows (blacken374-test-window-state source))
                (before-undo (with-current-buffer source buffer-undo-list)))
            (let ((blacken-executable adapter)
                  (blacken-line-length nil)
                  (blacken-allow-py36 nil)
                  (blacken-target-version nil)
                  (blacken-fast-unsafe nil)
                  (blacken-skip-string-normalization nil))
              (with-current-buffer source (blacken-buffer nil)))
            (list
             :before before-state
             :after (with-current-buffer source
                      (blacken374-test-buffer-state))
             :windows-before before-windows
             :windows-after (blacken374-test-window-state source)
             :undo-recorded
             (with-current-buffer source
               (not (eq before-undo buffer-undo-list)))
             :transcript (blacken374-test-transcript trace root)
             :output-buffers
             (mapcar (lambda (name) (and (get-buffer name) t))
                     blacken374-test-fixed-buffer-names)))))))))
"####;
    ParityBatchCase::value(
        "public_buffer_formatting_preserves_every_visible_window_view",
        form,
        expect![[
            r#"OK (:before (:text "def release( user ,items ):\n\11result={\"user\":user,\"items\":items}\n\11for item in items:\n\11\11result[item[\"sku\"]]=item[\"qty\"]\n\11return result\n\nsummary = release(\"界\", [{\"sku\":\"A-1\",\"qty\":2}])\nprint( summary )\n" :point 13 :mark 5 :mark-active t :modified nil :coding utf-8-unix :narrowed nil :mode python-mode :blacken (:enabled nil :lighter (blacken-mode " Black") :hook-count 0 :hook-local nil)) :after (:text "def release(user, items):\n    result = {\"user\": user, \"items\": items}\n    for item in items:\n        result[item[\"sku\"]] = item[\"qty\"]\n    return result\n\n\nsummary = release(\"界\", [{\"sku\": \"A-1\", \"qty\": 2}])\nprint(summary)\n" :point 13 :mark 1 :mark-active t :modified t :coding utf-8-unix :narrowed nil :mode python-mode :blacken (:enabled nil :lighter (blacken-mode " Black") :hook-count 0 :hook-local nil)) :windows-before ((:point 13 :start 1 :selected t :buffer t) (:point 133 :start 65 :selected nil :buffer t)) :windows-after ((:point 13 :start 1 :selected t :buffer t) (:point 133 :start 65 :selected nil :buffer t)) :undo-recorded t :transcript "CALL\nPWD\11[ROOT]\nARG\11-\nINPUT-BEGIN\ndef release( user ,items ):\n\11result={\"user\":user,\"items\":items}\n\11for item in items:\n\11\11result[item[\"sku\"]]=item[\"qty\"]\n\11return result\n\nsummary = release(\"界\", [{\"sku\":\"A-1\",\"qty\":2}])\nprint( summary )\nINPUT-END\n" :output-buffers (nil nil))"#
        ]],
    )
}

fn formatter_options_and_stub_files_produce_exact_public_argv() -> ParityBatchCase {
    let form = r####"
(blacken374-test-run-case
 "options-and-pyi"
 (lambda (root)
   (let* ((source-file (expand-file-name "project types/release.pyi" root))
          (source-text
           "class Release(TypedDict):\n    id: str\n    total: float\n")
          (default-directory root))
     (blacken374-test-write source-file source-text)
     (blacken374-test-with-adapter
      root "success"
      (lambda (adapter trace _input-file output-file)
        (blacken374-test-write output-file source-text)
        (let ((buffer (find-file-noselect source-file)))
          (with-current-buffer buffer
            (python-mode)
            (set-buffer-file-coding-system 'utf-8-unix)
            (buffer-enable-undo)
            (goto-char 9)
            (set-mark 2)
            (setq mark-active t)
            (set-buffer-modified-p nil)
            (let ((before (blacken374-test-buffer-state))
                  (before-undo (copy-tree buffer-undo-list)))
              (let ((blacken-executable adapter)
                    (blacken-line-length 'fill)
                    (fill-column 88)
                    (blacken-allow-py36 t)
                    (blacken-target-version "py312")
                    (blacken-fast-unsafe t)
                    (blacken-skip-string-normalization t))
                (blacken-buffer nil))
              (let ((blacken-executable adapter)
                    (blacken-line-length 100)
                    (blacken-allow-py36 nil)
                    (blacken-target-version "py311")
                    (blacken-fast-unsafe nil)
                    (blacken-skip-string-normalization nil))
                (blacken-buffer nil))
              (let ((blacken-executable adapter)
                    (blacken-line-length nil)
                    (blacken-allow-py36 nil)
                    (blacken-target-version nil)
                    (blacken-fast-unsafe nil)
                    (blacken-skip-string-normalization nil))
                (blacken-buffer nil))
              (list :before before
                    :after (blacken374-test-buffer-state)
                    :undo-unchanged (equal before-undo buffer-undo-list)
                    :calls (blacken374-test-call-count trace)
                    :transcript (blacken374-test-transcript trace root)
                    :output-buffers
                    (mapcar (lambda (name) (and (get-buffer name) t))
                            blacken374-test-fixed-buffer-names))))))))))
"####;
    ParityBatchCase::value(
        "formatter_options_and_stub_files_produce_exact_public_argv",
        form,
        expect![[
            r#"OK (:before (:text "class Release(TypedDict):\n    id: str\n    total: float\n" :point 9 :mark 2 :mark-active t :modified nil :coding utf-8-unix :narrowed nil :mode python-mode :blacken (:enabled nil :lighter (blacken-mode " Black") :hook-count 0 :hook-local nil)) :after (:text "class Release(TypedDict):\n    id: str\n    total: float\n" :point 9 :mark 2 :mark-active t :modified nil :coding utf-8-unix :narrowed nil :mode python-mode :blacken (:enabled nil :lighter (blacken-mode " Black") :hook-count 0 :hook-local nil)) :undo-unchanged t :calls 3 :transcript "CALL\nPWD\11[ROOT]/project types\nARG\11--line-length\nARG\1188\nARG\11--target-version\nARG\11py36\nARG\11--fast\nARG\11--skip-string-normalization\nARG\11--pyi\nARG\11-\nINPUT-BEGIN\nclass Release(TypedDict):\n    id: str\n    total: float\nINPUT-END\nCALL\nPWD\11[ROOT]/project types\nARG\11--line-length\nARG\011100\nARG\11--target-version\nARG\11py311\nARG\11--pyi\nARG\11-\nINPUT-BEGIN\nclass Release(TypedDict):\n    id: str\n    total: float\nINPUT-END\nCALL\nPWD\11[ROOT]/project types\nARG\11--pyi\nARG\11-\nINPUT-BEGIN\nclass Release(TypedDict):\n    id: str\n    total: float\nINPUT-END\n" :output-buffers (nil nil))"#
        ]],
    )
}

fn project_gate_controls_real_format_on_save_lifecycle() -> ParityBatchCase {
    let form = r####"
(blacken374-test-run-case
 "project-save-mode"
 (lambda (root)
   (let* ((project (file-name-as-directory
                    (expand-file-name "workspace Ω/" root)))
          (source-file (expand-file-name "src/deep/release.py" project))
          (config-file (expand-file-name "pyproject.toml" project))
          (first-input "release={\"id\":\"REL-417\",\"total\":49.95}\n")
          (second-input "release={\"id\":\"REL-418\",\"total\":51.25}\n")
          (formatted "release = {\"id\": \"REL-418\", \"total\": 51.25}\n")
          (default-directory project))
     (blacken374-test-write config-file
                            "[project]\nname = \"release-tools\"\n")
     (blacken374-test-write source-file first-input)
     (blacken374-test-with-adapter
      root "success"
      (lambda (adapter trace _input-file output-file)
        (blacken374-test-write output-file formatted)
        (let ((buffer (find-file-noselect source-file)))
          (with-current-buffer buffer
            (python-mode)
            (setq default-directory (file-name-directory source-file))
            (let ((blacken-executable adapter)
                  (blacken-only-if-project-is-blackened t))
              (blacken-mode 1)
              (set-buffer-modified-p t)
              (save-buffer)
              (let ((without-section
                     (list :project (and (blacken-project-is-blackened) t)
                           :mode (blacken374-test-mode-state)
                           :disk (blacken374-test-read-text source-file)
                           :calls (blacken374-test-call-count trace))))
                (blacken-mode -1)
                (blacken374-test-write
                 config-file
                 "[project]\nname = \"release-tools\"\n\n[tool.black]\nline-length = 88\n")
                (erase-buffer)
                (insert second-input)
                (set-buffer-modified-p t)
                (blacken-mode 1)
                (let ((with-section-before-save
                       (list :project (and (blacken-project-is-blackened) t)
                             :mode (blacken374-test-mode-state)
                             :text (buffer-string))))
                  (save-buffer)
                  (let ((after-save
                         (list :state (blacken374-test-buffer-state)
                               :disk (blacken374-test-read-text source-file)
                               :calls (blacken374-test-call-count trace))))
                    (blacken-mode -1)
                    (list
                     :without-section without-section
                     :with-section-before-save with-section-before-save
                     :after-save after-save
                     :disabled (blacken374-test-mode-state)
                     :transcript (blacken374-test-transcript trace root)))))))))))))
"####;
    ParityBatchCase::value(
        "project_gate_controls_real_format_on_save_lifecycle",
        form,
        expect![[
            r#"OK (:without-section (:project nil :mode (:enabled t :lighter (blacken-mode " Black") :hook-count 0 :hook-local nil) :disk "release={\"id\":\"REL-417\",\"total\":49.95}\n" :calls 0) :with-section-before-save (:project t :mode (:enabled t :lighter (blacken-mode " Black") :hook-count 1 :hook-local t) :text "release={\"id\":\"REL-418\",\"total\":51.25}\n") :after-save (:state (:text "release = {\"id\": \"REL-418\", \"total\": 51.25}\n" :point 1 :mark nil :mark-active nil :modified nil :coding undecided-unix :narrowed nil :mode python-mode :blacken (:enabled t :lighter (blacken-mode " Black") :hook-count 1 :hook-local t)) :disk "release = {\"id\": \"REL-418\", \"total\": 51.25}\n" :calls 1) :disabled (:enabled nil :lighter (blacken-mode " Black") :hook-count 0 :hook-local nil) :transcript "CALL\nPWD\11[ROOT]/workspace Ω/src/deep\nARG\11-\nINPUT-BEGIN\nrelease={\"id\":\"REL-418\",\"total\":51.25}\nINPUT-END\n")"#
        ]],
    )
}

fn visible_formatter_failure_is_atomic_and_a_second_call_recovers() -> ParityBatchCase {
    let form = r####"
(blacken374-test-run-case
 "failure-recovery"
 (lambda (root)
   (let* ((input "def broken( value ):\n\treturn {\"value\":value,\"label\":\"界\"}\n")
          (output "def broken(value):\n    return {\"value\": value, \"label\": \"界\"}\n")
          (source (generate-new-buffer " *blacken374-failure-source*"))
          (default-directory root))
     (blacken374-test-with-adapter
      root "failure"
      (lambda (adapter trace _input-file output-file)
        (blacken374-test-write output-file output)
        (with-current-buffer source
          (setq default-directory root)
          (set-buffer-file-coding-system 'utf-8-unix)
          (insert input)
          (python-mode)
          (buffer-enable-undo)
          (goto-char 8)
          (set-mark 2)
          (setq mark-active t)
          (set-buffer-modified-p nil))
        (switch-to-buffer source)
        (delete-other-windows)
        (let* ((left (selected-window))
               (right (split-window-right)))
          (set-window-buffer right source)
          (set-window-point left 8)
          (set-window-start left 1 t)
          (set-window-point right 35)
          (set-window-start right 24 t)
          (select-window left)
          (let ((before (with-current-buffer source
                          (blacken374-test-buffer-state)))
                (blacken-executable adapter))
            (with-current-buffer source (blacken-buffer t))
            (blacken374-test-settle-error-pipe)
            (let ((failure
                   (list
                    :source (with-current-buffer source
                              (blacken374-test-buffer-state))
                    :unchanged
                    (equal before
                           (with-current-buffer source
                             (blacken374-test-buffer-state)))
                    :selected-buffer (buffer-name (window-buffer (selected-window)))
                    :error-buffer
                    (when-let ((buffer (get-buffer "*blacken-error*")))
                      (with-current-buffer buffer
                        (list :text (buffer-string)
                              :mode major-mode
                              :scroll-conservatively scroll-conservatively
                              :local-scroll
                              (local-variable-p 'scroll-conservatively))))
                    :output-buffer
                    (when-let ((buffer (get-buffer "*blacken*")))
                      (with-current-buffer buffer (buffer-string)))
                    :message (blacken374-test-last-message))))
              (select-window (or (get-buffer-window source)
                                 (error "source buffer lost after failure")))
              (setenv "BLACKEN374_MODE" "success")
              (with-current-buffer source (blacken-buffer nil))
              (list
               :before before
               :failure failure
               :recovered (with-current-buffer source
                            (blacken374-test-buffer-state))
               :source-windows (blacken374-test-window-state source)
               :transcript (blacken374-test-transcript trace root)
               :output-buffers
               (mapcar (lambda (name) (and (get-buffer name) t))
                       blacken374-test-fixed-buffer-names))))))))))
"####;
    ParityBatchCase::value(
        "visible_formatter_failure_is_atomic_and_a_second_call_recovers",
        form,
        expect![[
            r#"OK (:before (:text "def broken( value ):\n\11return {\"value\":value,\"label\":\"界\"}\n" :point 8 :mark 2 :mark-active t :modified nil :coding utf-8-unix :narrowed nil :mode python-mode :blacken (:enabled nil :lighter (blacken-mode " Black") :hook-count 0 :hook-local nil)) :failure (:source (:text "def broken( value ):\n\11return {\"value\":value,\"label\":\"界\"}\n" :point 8 :mark 2 :mark-active t :modified nil :coding utf-8-unix :narrowed nil :mode python-mode :blacken (:enabled nil :lighter (blacken-mode " Black") :hook-count 0 :hook-local nil)) :unchanged t :selected-buffer "*blacken-error*" :error-buffer (:text "owned black failure: invalid syntax Ω\n" :mode fundamental-mode :scroll-conservatively 0 :local-scroll t) :output-buffer "" :message "Black failed, see *blacken-error* buffer for details") :recovered (:text "def broken(value):\n    return {\"value\": value, \"label\": \"界\"}\n" :point 8 :mark 1 :mark-active t :modified t :coding utf-8-unix :narrowed nil :mode python-mode :blacken (:enabled nil :lighter (blacken-mode " Black") :hook-count 0 :hook-local nil)) :source-windows ((:point 8 :start 1 :selected t :buffer t)) :transcript "CALL\nPWD\11[ROOT]\nARG\11-\nINPUT-BEGIN\ndef broken( value ):\n\11return {\"value\":value,\"label\":\"界\"}\nINPUT-END\nCALL\nPWD\11[ROOT]\nARG\11-\nINPUT-BEGIN\ndef broken( value ):\n\11return {\"value\":value,\"label\":\"界\"}\nINPUT-END\n" :output-buffers (nil nil))"#
        ]],
    )
}

fn formatter_handles_a_line_above_the_process_read_buffer_without_freezing() -> ParityBatchCase {
    let form = r####"
(blacken374-test-run-case
 "long-line-regression"
 (lambda (root)
   (let* ((long-value (make-string 1100 ?x))
          (input (concat "# release Ω\npayload = \"" long-value "\"\n# tail 界\n"))
          (output (concat "# formatted by owned black\n" input))
          (source (generate-new-buffer " *blacken374-long-line*"))
          (default-directory root))
     (blacken374-test-with-adapter
      root "success"
      (lambda (adapter trace input-file output-file)
        (blacken374-test-write output-file output)
        (with-current-buffer source
          (setq default-directory root)
          (set-buffer-file-coding-system 'utf-8-unix)
          (insert input)
          (python-mode)
          (buffer-enable-undo)
          (goto-char 540)
          (set-mark 10)
          (setq mark-active t)
          (set-buffer-modified-p nil)
          (let ((before-undo buffer-undo-list)
                (blacken-executable adapter))
            (blacken-buffer nil)
            (goto-char (point-min))
            (search-forward "payload = ")
            (list
             :input (list :length (length input)
                          :sha256 (blacken374-test-sha256 input))
             :adapter-input
             (let ((bytes (blacken374-test-read-text input-file)))
               (list :length (length bytes)
                     :sha256 (blacken374-test-sha256 bytes)
                     :full-input (equal bytes input)))
             :output
             (list :length (buffer-size)
                   :sha256 (blacken374-test-sha256 (buffer-string))
                   :prefix (buffer-substring-no-properties
                            (point-min) (+ (point-min) 30))
                   :suffix (buffer-substring-no-properties
                            (- (point-max) 30) (point-max))
                   :long-line-length
                   (- (line-end-position) (line-beginning-position)))
             :point (point)
             :mark (mark t)
             :modified (buffer-modified-p)
             :undo-recorded (not (eq before-undo buffer-undo-list))
             :calls (blacken374-test-call-count trace)
             :transcript-prefix
             (let ((transcript (blacken374-test-transcript trace root)))
               (substring transcript 0 (min 52 (length transcript))))
             :output-buffers
             (mapcar (lambda (name) (and (get-buffer name) t))
                     blacken374-test-fixed-buffer-names)))))))))
"####;
    ParityBatchCase::value(
        "formatter_handles_a_line_above_the_process_read_buffer_without_freezing",
        form,
        expect![[
            r##"OK (:input (:length 1134 :sha256 "0a77bfbe5ebdcf46dc411e1111156e62195177eaf6ae77528070656ab8961a78") :adapter-input (:length 1134 :sha256 "0a77bfbe5ebdcf46dc411e1111156e62195177eaf6ae77528070656ab8961a78" :full-input t) :output (:length 1161 :sha256 "b561f10fda555c018d93747ae4eb4f2032811d55eab63806f6b64a1d02349f57" :prefix "# formatted by owned black\n# r" :suffix "xxxxxxxxxxxxxxxxxxx\"\n# tail 界\n" :long-line-length 1112) :point 50 :mark 1 :modified t :undo-recorded t :calls 1 :transcript-prefix "CALL\nPWD\11[ROOT]\nARG\11-\nINPUT-BEGIN\n# release Ω\npayloa" :output-buffers (nil nil))"##
        ]],
    )
}

#[test]
fn blacken_practical_workflows_batch() {
    let cases = vec![
        public_buffer_formatting_preserves_every_visible_window_view(),
        formatter_options_and_stub_files_produce_exact_public_argv(),
        project_gate_controls_real_format_on_save_lifecycle(),
        visible_formatter_failure_is_atomic_and_a_second_call_recovers(),
        formatter_handles_a_line_above_the_process_read_buffer_without_freezing(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "blacken_practical_workflows_batch",
        "blacken_parity",
        &cases,
    );
}

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, PYTHON_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'python)
(require 'python-mode)

;; Establish Python Mode's process-wide keymap, font-lock, and hook additions
;; before shared-case baselines are captured.
(with-temp-buffer (python-mode))

(defvar python381-test-owned-roots nil)

(defun python381-test-line-state ()
  (save-excursion
    (goto-char (point-min))
    (let (state)
      (while (not (eobp))
        (push (list (line-number-at-pos) (current-indentation)
                    (buffer-substring-no-properties
                     (line-beginning-position) (line-end-position)))
              state)
        (forward-line 1))
      (nreverse state))))

(defun python381-test-face-runs ()
  (let ((position (point-min)) runs)
    (while (< position (point-max))
      (let* ((face (get-text-property position 'face))
             (next (or (next-single-property-change
                        position 'face nil (point-max))
                       (point-max))))
        (when face
          (push (list position next face
                      (buffer-substring-no-properties position next))
                runs))
        (setq position next)))
    (nreverse runs)))

(defun python381-test-locus ()
  (list :point (point) :line (line-number-at-pos)
        :column (current-column) :indent (current-indentation)
        :text (buffer-substring-no-properties
               (line-beginning-position) (line-end-position))))

(defun python381-test-region-state (bounds)
  (list :bounds bounds :point (point) :mark (mark t)
        :active mark-active
        :text (buffer-substring-no-properties
               (region-beginning) (region-end))))

(defun python381-test-buffer-state ()
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point) :line (line-number-at-pos) :column (current-column)
        :modified (buffer-modified-p)
        :lines (python381-test-line-state)))

(defun python381-test-wait-for-new-prompt (process start)
  (let ((attempt 0) complete)
    (while (and (< attempt 50) (process-live-p process) (not complete))
      (accept-process-output process 0.1)
      (setq attempt (1+ attempt)
            complete
            (with-current-buffer (process-buffer process)
              (save-excursion
                (goto-char start)
                (and (re-search-forward ">>> " nil t)
                     (= (match-end 0)
                        (marker-position (process-mark process))))))))
    (unless complete
      (error "Python Mode process did not produce a new complete prompt: %S"
             (with-current-buffer (process-buffer process)
               (buffer-string))))
    ;; The prompt is the fixture's definitive response boundary.  Two more
    ;; owned-process polls prove that its filter has no trailing chunk left.
    (let ((settled
           (with-current-buffer (process-buffer process)
             (list (buffer-size) (marker-position (process-mark process))))))
      (dotimes (_ 2)
        (accept-process-output process 0.05)
        (unless
            (equal settled
                   (with-current-buffer (process-buffer process)
                     (list (buffer-size)
                           (marker-position (process-mark process)))))
          (error "Python Mode process output changed after its prompt"))))
    (marker-position (process-mark process))))

(defun python381-test-make-shell-fixture (stem tool-name log-name script)
  (let* ((root (make-temp-file stem t))
         (tool (expand-file-name tool-name root))
         (log (expand-file-name log-name root)))
    (push root python381-test-owned-roots)
    (write-region script nil tool nil 'silent)
    (set-file-modes tool #o700)
    (list root tool log)))

(defun python381-test-boundary-log (log)
  (with-temp-buffer
    (insert-file-contents log)
    (buffer-string)))

(defun python381-test-stop-process (process)
  (delete-process process)
  (while (process-live-p process)
    (accept-process-output process 0.05))
  (list (process-status process) (process-live-p process)))

(defun python381-test-normalize-imenu (value)
  (cond
   ((markerp value) (marker-position value))
   ((consp value)
    (cons (python381-test-normalize-imenu (car value))
          (python381-test-normalize-imenu (cdr value))))
   ((stringp value) (copy-sequence value))
   (t value)))

(defun python381-test-run (body)
  (let ((buffers-before (buffer-list))
        (frames-before (frame-list))
        (processes-before (process-list))
        (timers-before (append timer-list timer-idle-list))
        (buffer-before (current-buffer))
        (windows-before (current-window-configuration))
        (python381-test-owned-roots nil)
        result body-error cleanup-errors)
    (unwind-protect
        (condition-case error
            (setq result (funcall body))
          (error (setq body-error error)))
      (condition-case error
          (progn
            (when (buffer-live-p buffer-before) (set-buffer buffer-before))
            (set-window-configuration windows-before))
        (error (push (list :restore-windows error) cleanup-errors)))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (condition-case error
              (delete-frame frame t)
            (error (push (list :delete-frame error) cleanup-errors)))))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case error
              (progn
                (set-process-query-on-exit-flag process nil)
                (delete-process process))
            (error
             (push (list :delete-process (process-name process) error)
                   cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case error
              (progn
                (with-current-buffer buffer (set-buffer-modified-p nil))
                (kill-buffer buffer))
            (error
             (push (list :kill-buffer (buffer-name buffer) error)
                   cleanup-errors)))))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (memq timer timers-before)
          (condition-case error
              (cancel-timer timer)
            (error (push (list :cancel-timer error) cleanup-errors)))))
      (dolist (root python381-test-owned-roots)
        (condition-case error
            (when (file-exists-p root) (delete-directory root t))
          (error (push (list :delete-root root error) cleanup-errors))))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (push (list :remaining-frame frame) cleanup-errors)))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (push (list :remaining-buffer (buffer-name buffer)) cleanup-errors)))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (push (list :remaining-process (process-name process))
                cleanup-errors)))
      (dolist (timer (append timer-list timer-idle-list))
        (unless (memq timer timers-before)
          (push (list :remaining-timer t) cleanup-errors)))
      (dolist (root python381-test-owned-roots)
        (when (file-exists-p root)
          (push (list :remaining-root root) cleanup-errors))))
    (cond
     ((and body-error cleanup-errors)
      (error "Python Mode body failed %S; cleanup failed %S"
             body-error (nreverse cleanup-errors)))
     (body-error (signal (car body-error) (cdr body-error)))
     (cleanup-errors
      (error "Python Mode cleanup failed: %S" (nreverse cleanup-errors)))
     (t result))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PYTHON_MODE_MELPA_PIN, "python-mode.el")
        .expect("prepare exact shallow Python Mode source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_return_indents_a_nested_unicode_definition() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_return_indents_a_nested_unicode_definition",
        r####"(python381-test-run
 (lambda ()
   (let ((buffer (generate-new-buffer " *python381-indent*"))
         (py-current-defun-show nil)
         (py-outline-minor-mode-p nil))
     (switch-to-buffer buffer)
     (python-mode)
     (insert "class Café:")
     (call-interactively #'py-newline-and-indent)
     (insert "def total(self, values):")
     (call-interactively #'py-newline-and-indent)
     (insert "if values:")
     (call-interactively #'py-newline-and-indent)
     (insert "return values[0] + values[1]")
     (font-lock-ensure)
     (list :mode major-mode
           :derived (derived-mode-p 'prog-mode)
           :text (buffer-substring-no-properties (point-min) (point-max))
           :point (point)
           :current-defun (py-current-defun)
           :lines (python381-test-line-state)
           :faces (python381-test-face-runs)))))"####,
        expect![[
            r#"OK (:mode python-mode :derived prog-mode :text "class Café:\n    def total(self, values):\n        if values:\n            return values[0] + values[1]" :point 101 :current-defun "total" :lines ((1 0 "class Café:") (2 4 "    def total(self, values):") (3 8 "        if values:") (4 12 "            return values[0] + values[1]")) :faces ((1 6 py-def-class-face "class") (7 11 py-class-name-face "Café") (17 20 py-def-class-face "def") (21 26 py-def-face "total") (27 31 py-object-reference-face "self") (50 52 font-lock-keyword-face "if") (73 79 font-lock-keyword-face "return") (87 88 py-number-face "0") (99 100 py-number-face "1")))"#
        ]],
    )
}

fn structural_navigation_and_marking_follow_nested_python_forms() -> ParityBatchCase {
    ParityBatchCase::value(
        "structural_navigation_and_marking_follow_nested_python_forms",
        r####"(python381-test-run
 (lambda ()
   (let ((buffer (generate-new-buffer " *python381-navigation*"))
         (transient-mark-mode t)
         (py-current-defun-show nil)
         (py-outline-minor-mode-p nil)
         defun-name defun-region if-region backward forward)
     (switch-to-buffer buffer)
     (insert "@logged\nclass Café:\n    def total(self, values):\n        if values:\n            first = values[0]\n            return first\n        else:\n            return 0\n\ndef peer():\n    return \"界\"\n")
     (python-mode)
     (goto-char (point-min))
     (search-forward "return first")
     (setq defun-name (py-which-def-or-class))
     (setq defun-region
           (python381-test-region-state (py-mark-def t)))
     (deactivate-mark)
     (goto-char (point-min))
     (search-forward "first =")
     (setq if-region
           (python381-test-region-state (py-mark-if-block)))
     (deactivate-mark)
     (goto-char (point-max))
     (py-backward-def-or-class)
     (push (python381-test-locus) backward)
     (py-backward-def-or-class)
     (push (python381-test-locus) backward)
     (setq backward (nreverse backward))
     (goto-char (point-min))
     (py-forward-def-or-class)
     (push (python381-test-locus) forward)
     (py-forward-def-or-class)
     (push (python381-test-locus) forward)
     (list :current-defun defun-name
           :def defun-region :if if-region
           :backward backward :forward (nreverse forward)))))"####,
        expect![[
            r#"OK (:current-defun "Café.total" :def (:bounds (21 . 159) :point 21 :mark 159 :active t :text "    def total(self, values):\n        if values:\n            first = values[0]\n            return first\n        else:\n            return 0\n") :if (:bounds (50 . 159) :point 50 :mark 159 :active t :text "        if values:\n            first = values[0]\n            return first\n        else:\n            return 0\n") :backward ((:point 160 :line 10 :column 0 :indent 0 :text "def peer():") (:point 9 :line 2 :column 0 :indent 0 :text "class Café:")) :forward ((:point 158 :line 8 :column 20 :indent 12 :text "            return 0") (:point 186 :line 11 :column 15 :indent 4 :text "    return \"界\"")))"#
        ]],
    )
}

fn public_sort_and_shift_commands_transform_complete_python_forms() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_sort_and_shift_commands_transform_complete_python_forms",
        r####"(python381-test-run
 (lambda ()
   (let ((buffer (generate-new-buffer " *python381-edit*"))
         (py-outline-minor-mode-p nil)
         sorted shifted restored)
     (switch-to-buffer buffer)
     (insert "from demo import (\n    zebra,\n    alpha,\n    café,\n    alpha,\n)\n\ndef compute():\n    if ready:\n        value = 1\n        return value\n")
     (python-mode)
     (goto-char (point-min))
     (search-forward "zebra")
     (call-interactively #'py-sort-imports)
     (setq sorted (python381-test-buffer-state))
     (goto-char (point-min))
     (search-forward "if ready")
     (call-interactively #'py-shift-block-right)
     (setq shifted (python381-test-buffer-state))
     (call-interactively #'py-shift-block-left)
     (setq restored (python381-test-buffer-state))
     (list :sorted sorted :shifted shifted :restored restored))))"####,
        expect![[
            r#"OK (:sorted (:text "from demo import (\n    alpha, café, zebra)\n\ndef compute():\n    if ready:\n        value = 1\n        return value\n" :point 18 :line 1 :column 17 :modified t :lines ((1 0 "from demo import (") (2 4 "    alpha, café, zebra)") (3 0 "") (4 0 "def compute():") (5 4 "    if ready:") (6 8 "        value = 1") (7 8 "        return value"))) :shifted (:text "from demo import (\n    alpha, café, zebra)\n\ndef compute():\n        if ready:\n            value = 1\n            return value\n" :point 76 :line 5 :column 16 :modified t :lines ((1 0 "from demo import (") (2 4 "    alpha, café, zebra)") (3 0 "") (4 0 "def compute():") (5 8 "        if ready:") (6 12 "            value = 1") (7 12 "            return value"))) :restored (:text "from demo import (\n    alpha, café, zebra)\n\ndef compute():\n    if ready:\n        value = 1\n        return value\n" :point 72 :line 5 :column 12 :modified t :lines ((1 0 "from demo import (") (2 4 "    alpha, café, zebra)") (3 0 "") (4 0 "def compute():") (5 4 "    if ready:") (6 8 "        value = 1") (7 8 "        return value"))))"#
        ]],
    )
}

fn documented_shell_and_execute_string_use_a_real_owned_comint_process() -> ParityBatchCase {
    ParityBatchCase::value(
        "documented_shell_and_execute_string_use_a_real_owned_comint_process",
        r####"(python381-test-run
 (lambda ()
   (let* ((fixture
           (python381-test-make-shell-fixture
            "python381-shell-" "python-fixture" "boundary.log"
            (concat
             "#!/bin/sh\n"
             "printf 'argv' >>\"$PY381_LOG\"\n"
             "for arg in \"$@\"; do printf '<%s>' \"$arg\" >>\"$PY381_LOG\"; done\n"
             "printf '\\n' >>\"$PY381_LOG\"\n"
             "printf 'Python 3.13.0 fixture\\n>>> '\n"
             "while IFS= read -r line; do\n"
             "  case \"$line\" in\n"
             "    *'__pyfile = codecs.open'*) printf 'completion:query\\n' >>\"$PY381_LOG\"; printf 'calculate;calculator\\n>>> ' ;;\n"
             "    *) printf 'stdin:%s\\n' \"$line\" >>\"$PY381_LOG\"\n"
             "       case \"$line\" in\n"
             "    \"print('café 界')\") printf 'café 界\\n>>> ' ;;\n"
             "    \"raise RuntimeError('boom')\") printf 'RuntimeError: boom\\n>>> ' ;;\n"
             "    *) printf 'ack:%s\\n>>> ' \"$line\" ;;\n"
             "       esac ;;\n"
             "  esac\n"
             "done\n")))
          (root (nth 0 fixture))
          (tool (nth 1 fixture))
          (log (nth 2 fixture))
          (temporary-file-directory (file-name-as-directory root))
          (process-environment
           (cons (concat "PY381_LOG=" log) process-environment))
          (py-register-shell-buffer-p t)
          (py-split-window-on-execute nil)
          (py-switch-buffers-on-execute-p nil)
          (py-shell-fontify-p nil)
          (py-python-send-delay 0)
          (register-alist (copy-tree register-alist))
          (py-output-buffer py-output-buffer)
          shell-buffer process prompt-start completion output boundary
          after-delete)
     (setq shell-buffer
           (py-shell nil "--isolated" nil tool "*Python381*"
                     nil nil nil nil nil)
           process (get-buffer-process shell-buffer))
     (unless (process-live-p process)
       (error "Python Mode did not start the owned interpreter"))
     (set-process-query-on-exit-flag process nil)
     (python381-test-wait-for-new-prompt process (point-min))
     (setq prompt-start (marker-position (process-mark process)))
     (py-execute-string "print('café 界')" process)
     (python381-test-wait-for-new-prompt process prompt-start)
     (setq prompt-start (marker-position (process-mark process)))
     (py-execute-string "raise RuntimeError('boom')" process)
     (python381-test-wait-for-new-prompt process prompt-start)
     (setq completion
           (with-temp-buffer
             (python-mode)
             (insert "cal")
             (let* ((capf (py-shell-completion-at-point process))
                    (beg (nth 0 capf))
                    (end (nth 1 capf))
                    (candidates
                     (all-completions
                      (buffer-substring-no-properties beg end)
                      (nth 2 capf))))
               (list :bounds (list beg end)
                     :input (buffer-substring-no-properties beg end)
                     :candidates candidates))))
     (setq output
           (with-current-buffer shell-buffer
             (list :mode major-mode
                   :process-live (and (process-live-p process) t)
                   :text (buffer-substring-no-properties
                          (point-min) (point-max))
                   :point (point)
                   :process-mark (marker-position (process-mark process)))))
     (setq boundary (python381-test-boundary-log log)
           after-delete (python381-test-stop-process process))
     (list :completion completion :shell output :boundary boundary
           :after-delete after-delete))))"####,
        expect![[
            r#"OK (:completion (:bounds (1 4) :input "cal" :candidates ("calculate" "calculator")) :shell (:mode py-shell-mode :process-live t :text "Python 3.13.0 fixture\n>>> café 界\n>>> RuntimeError: boom\n>>> " :point 61 :process-mark 61) :boundary "argv<--isolated>\nstdin:print('café 界')\nstdin:raise RuntimeError('boom')\ncompletion:query\n" :after-delete (signal nil))"#
        ]],
    )
}

fn file_routing_and_public_imenu_select_nested_definitions() -> ParityBatchCase {
    ParityBatchCase::value(
        "file_routing_and_public_imenu_select_nested_definitions",
        r####"(python381-test-run
 (lambda ()
   (let ((routes '(("module.py" . python-mode)
                   ("types.pyi" . python-mode)
                   ("accelerator.pyx" . python-mode)
                   ("notes.txt" . text-mode)))
         (py-outline-minor-mode-p nil)
         routed index class-item method-item destination)
     (dolist (route routes)
       (with-temp-buffer
         (setq buffer-file-name (concat "/virtual/" (car route)))
         (set-auto-mode)
         (push (list (car route) major-mode mode-name
                     (eq major-mode (cdr route)))
               routed)))
     (let ((buffer (generate-new-buffer " *python381-imenu*")))
       (switch-to-buffer buffer)
       (insert "class Café:\n    def total(self, values):\n        return sum(values)\n\ndef peer():\n    return 1\n")
       (python-mode)
       (setq index (funcall imenu-create-index-function)
             class-item (assoc "Café (class)" index)
             method-item (assoc "total (def)" (cdr class-item)))
       (goto-char (point-max))
       (imenu method-item)
       (setq destination (python381-test-locus))
       (list :routes (nreverse routed)
             :index (python381-test-normalize-imenu index)
             :selected (python381-test-normalize-imenu method-item)
             :destination destination)))))"####,
        expect![[
            r#"OK (:routes (("module.py" python-mode "Py" t) ("types.pyi" python-mode "Py" t) ("accelerator.pyx" python-mode "Py" t) ("notes.txt" text-mode "Text" t)) :index (("Café (class)" ("Café (class)" . 1) ("total (def)" . 17)) ("peer (def)" . 70)) :selected ("total (def)" . 17) :destination (:point 17 :line 2 :column 4 :indent 4 :text "    def total(self, values):"))"#
        ]],
    )
}

fn missing_interpreter_fails_atomically_then_public_shell_recovers() -> ParityBatchCase {
    ParityBatchCase::value(
        "missing_interpreter_fails_atomically_then_public_shell_recovers",
        r####"(python381-test-run
 (lambda ()
   (let* ((fixture
           (python381-test-make-shell-fixture
            "python381-recovery-" "late-python" "recovery.log" ""))
          (root (nth 0 fixture))
          (tool (nth 1 fixture))
          (log (nth 2 fixture))
          (exec-path (list root))
          (process-environment
           (list (concat "PATH=" root) (concat "PY381_LOG=" log)))
          (py-register-shell-buffer-p t)
          (py-split-window-on-execute nil)
          (py-switch-buffers-on-execute-p nil)
          (py-shell-fontify-p nil)
          (register-alist (copy-tree register-alist))
          (py-output-buffer py-output-buffer)
          failure failed-buffer shell-buffer process recovery boundary
          after-delete)
     ;; The fixture helper establishes ownership, but the executable must be
     ;; absent for the first public shell attempt.
     (delete-file tool)
     (setq failure
           (condition-case error
               (progn
                 (py-shell nil "--recover" nil tool
                           "*Python381-Recovery*" nil nil nil nil nil)
                 'unexpected-success)
             (error
              (list (car error)
                    (replace-regexp-in-string
                     (regexp-quote root) "[ROOT]"
                     (error-message-string error) t t)))))
     (setq failed-buffer (and (get-buffer "*Python381-Recovery*") t))
     (write-region
      (concat
       "#!/bin/sh\n"
       "printf 'argv' >>\"$PY381_LOG\"\n"
       "for arg in \"$@\"; do printf '<%s>' \"$arg\" >>\"$PY381_LOG\"; done\n"
       "printf '\\n' >>\"$PY381_LOG\"\n"
       "printf 'Python recovered\\n>>> '\n"
       "while IFS= read -r line; do printf 'stdin:%s\\n' \"$line\" >>\"$PY381_LOG\"; done\n")
      nil tool nil 'silent)
     (set-file-modes tool #o700)
     (setq shell-buffer
           (py-shell nil "--recover" nil tool "*Python381-Recovery*"
                     nil nil nil nil nil)
           process (get-buffer-process shell-buffer))
     (set-process-query-on-exit-flag process nil)
     (python381-test-wait-for-new-prompt process (point-min))
     (setq recovery
           (with-current-buffer shell-buffer
             (list :mode major-mode
                   :live (and (process-live-p process) t)
                   :text (buffer-substring-no-properties
                          (point-min) (point-max))
                   :mark (marker-position (process-mark process)))))
     (setq boundary (python381-test-boundary-log log)
           after-delete (python381-test-stop-process process))
     (list :failure failure :failed-buffer failed-buffer
           :recovery recovery :boundary boundary
           :after-delete after-delete))))"####,
        expect![[
            r#"OK (:failure (error "py-shell: Can not see an executable for ‘[ROOT]/late-python’ on your system. Maybe needs a link?") :failed-buffer nil :recovery (:mode py-shell-mode :live t :text "Python recovered\n>>> " :mark 22) :boundary "argv<--recover>\n" :after-delete (signal nil))"#
        ]],
    )
}

#[test]
fn python_mode_package_batch() {
    let cases = vec![
        public_return_indents_a_nested_unicode_definition(),
        structural_navigation_and_marking_follow_nested_python_forms(),
        public_sort_and_shift_commands_transform_complete_python_forms(),
        documented_shell_and_execute_string_use_a_real_owned_comint_process(),
        file_routing_and_public_imenu_select_nested_definitions(),
        missing_interpreter_fails_atomically_then_public_shell_recovers(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Python Mode parity test");
    assert_oracle_batch_cases(oracle(), test_name, "python_mode_parity", &cases);
}

//! Practical parity for the final standalone Csharp Mode release.
//!
//! The package is now maintained in Emacs core, but rank 398 selects its final
//! MELPA archive. These cases keep that exact source visible while exercising
//! automatic `.cs` activation, semantic fontification, CC Mode indentation and
//! brace editing, defun/statement navigation, compilation diagnostics, and repair
//! of an invalid multiline string.

use std::time::Duration;

use expect_test::expect;

use crate::{CSHARP_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'compile)
(require 'elec-pair)
(require 'csharp-mode)

(defconst csharp398-test-source
  '("csharp-mode.el"
    "0fa4030003726d8e8e05d25546d02e0eb0f07fc93cd16d3d74a0b54f000fdd40"
    3
    "52fef45d7c2934a6806f0861d10b26e41813d3d7f2e31ffea7e6edba19eb0265"))

(defun csharp398-test-file-sha256 (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(let* ((loaded (symbol-file 'csharp-mode 'defun))
       (source (and loaded
                    (if (string-suffix-p ".elc" loaded)
                        (concat (file-name-sans-extension loaded) ".el")
                      loaded)))
       (directory (and source (file-name-directory source)))
       (files (and directory
                   (seq-remove
                    (lambda (path)
                      (member (file-name-nondirectory path)
                              '("csharp-mode-autoloads.el"
                                "csharp-mode-pkg.el")))
                    (directory-files directory t "\\`csharp-.*\\.el\\'"))))
       (files (sort files #'string-lessp))
       (manifest
        (mapconcat
         (lambda (path)
           (format "%s\t%s\n"
                   (file-name-nondirectory path)
                   (csharp398-test-file-sha256 path)))
         files
         "")))
  (unless (and (file-regular-p source)
               (equal (file-name-nondirectory source)
                      (nth 0 csharp398-test-source))
               (equal (csharp398-test-file-sha256 source)
                      (nth 1 csharp398-test-source))
               (= (length files) (nth 2 csharp398-test-source))
               (equal (secure-hash 'sha256 manifest)
                      (nth 3 csharp398-test-source)))
    (error "Unexpected installed Csharp Mode sources: %S" source)))

(defun csharp398-test-normalize (value root)
  (cond ((stringp value)
         (replace-regexp-in-string (regexp-quote root) "[ROOT]/" value t t))
        ((consp value)
         (cons (csharp398-test-normalize (car value) root)
               (csharp398-test-normalize (cdr value) root)))
        ((vectorp value)
         (apply #'vector
                (mapcar (lambda (item)
                          (csharp398-test-normalize item root))
                        value)))
        (t value)))

(defun csharp398-test-condition (condition root)
  (list :type (car condition)
        :data (csharp398-test-normalize (copy-tree (cdr condition)) root)
        :message (csharp398-test-normalize
                  (error-message-string condition) root)))

(defun csharp398-test-window-state ()
  (mapcar (lambda (window)
            (list (window-buffer window)
                  (window-point window)
                  (window-start window)))
          (window-list nil 'nomini)))

(defun csharp398-test-write-file (root relative contents)
  (let ((path (expand-file-name relative root)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (set-buffer-multibyte t)
      (insert contents)
      (let ((coding-system-for-write 'utf-8-unix))
        (write-region (point-min) (point-max) path nil 'silent)))
    path))

(defun csharp398-test-manifest (root)
  (mapcar
   (lambda (path)
     (unless (and (file-regular-p path) (not (file-symlink-p path)))
       (error "Non-regular Csharp fixture entry: %s" path))
     (list (file-relative-name path root)
           (csharp398-test-file-sha256 path)))
   (sort (directory-files-recursively root "." nil nil nil)
         #'string-lessp)))

(defun csharp398-test-face-probe (label needle &optional occurrence)
  (goto-char (point-min))
  (dotimes (_ (or occurrence 1))
    (unless (search-forward needle nil t)
      (error "Missing Csharp face probe: %s" needle)))
  (let ((start (- (point) (length needle))))
    (list label
          (buffer-substring-no-properties start (point))
          (copy-tree (get-char-property start 'face))
          (cond ((nth 4 (syntax-ppss start)) 'comment)
                ((nth 3 (syntax-ppss start)) 'string)
                (t 'code)))))

(defun csharp398-test-compilation-face-probe (label needle)
  (goto-char (point-min))
  (unless (search-forward needle nil t)
    (error "Missing Csharp compilation face probe: %s" needle))
  (let ((start (- (point) (length needle))))
    (list label
          (buffer-substring-no-properties start (point))
          (copy-tree (get-text-property start 'font-lock-face)))))

(defun csharp398-test-line-state ()
  (list :file (and buffer-file-name (file-name-nondirectory buffer-file-name))
        :line (line-number-at-pos)
        :column (current-column)
        :text (buffer-substring-no-properties
               (line-beginning-position) (line-end-position))))

(defun csharp398-test-select-first-compilation-message ()
  (goto-char (point-min))
  (unless (get-text-property (point) 'compilation-message)
    (goto-char
     (or (next-single-property-change
          (point) 'compilation-message nil (point-max))
         (error "Missing parsed Csharp compilation message"))))
  (unless (get-text-property (point) 'compilation-message)
    (error "Csharp compilation message property is absent at %d" (point)))
  (list :line (line-number-at-pos)
        :column (current-column)
        :text (buffer-substring-no-properties
               (line-beginning-position) (line-end-position))))

(defun csharp398-test-park-buffer (name)
  (when-let* ((buffer (get-buffer name)))
    (let ((old-name (buffer-name buffer)))
      (with-current-buffer buffer
        (rename-buffer (format " *csharp398-parked-%s*" (sxhash-eq buffer)) t))
      (cons buffer old-name))))

(defun csharp398-test-forbid-external (name &rest arguments)
  (error "Unexpected Csharp external boundary: %S %S" name arguments))

(defun csharp398-test-run (files body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name "csharp-mode/" sandbox))))
         (window-before (current-window-configuration))
         (window-state-before (csharp398-test-window-state))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (next-error-last-buffer next-error-last-buffer)
         (next-error-highlight nil)
         (next-error-highlight-timer nil)
         (next-error-overlay-arrow-position nil)
         (compilation-highlight-overlay nil)
         (c-default-style "csharp")
         (c-offsets-alist nil)
         (font-lock-maximum-decoration t)
         (electric-pair-pairs nil)
         (electric-pair-text-pairs nil)
         (electric-pair-inhibit-predicate #'electric-pair-default-inhibit)
         (compilation-search-path '(nil))
         (compilation-search-all-directories nil)
         (vc-handled-backends nil)
         (auto-save-default nil)
         (create-lockfiles nil)
         (message-log-max nil)
         (print-circle nil)
         (parked nil)
         (root-owned nil)
         fixture-before fixture-after result body-error cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute Csharp Mode sandbox root"))
              (when (file-exists-p root)
                (error "Csharp Mode sandbox root already exists: %s" root))
              (dolist (name '("*compilation*" "*Csharp Build*"))
                (when-let* ((entry (csharp398-test-park-buffer name)))
                  (push entry parked)))
              (make-directory root t)
              (setq root-owned t)
              (dolist (file files)
                (csharp398-test-write-file root (car file) (cdr file)))
              (setq fixture-before (csharp398-test-manifest root))
              (setq result
                    (cl-letf (((symbol-function 'call-process)
                               (lambda (&rest arguments)
                                 (apply #'csharp398-test-forbid-external
                                        'call-process arguments)))
                              ((symbol-function 'process-file)
                               (lambda (&rest arguments)
                                 (apply #'csharp398-test-forbid-external
                                        'process-file arguments)))
                              ((symbol-function 'start-process)
                               (lambda (&rest arguments)
                                 (apply #'csharp398-test-forbid-external
                                        'start-process arguments)))
                              ((symbol-function 'start-file-process)
                               (lambda (&rest arguments)
                                 (apply #'csharp398-test-forbid-external
                                        'start-file-process arguments)))
                              ((symbol-function 'make-process)
                               (lambda (&rest arguments)
                                 (apply #'csharp398-test-forbid-external
                                        'make-process arguments)))
                              ((symbol-function 'url-retrieve-synchronously)
                               (lambda (&rest arguments)
                                 (apply #'csharp398-test-forbid-external
                                        'url-retrieve-synchronously arguments))))
                      (funcall body root)))
              (setq fixture-after (csharp398-test-manifest root))
              (unless (equal fixture-before fixture-after)
                (error "Csharp fixture changed: %S -> %S"
                       fixture-before fixture-after)))
          (error (setq body-error (csharp398-test-condition condition root))))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case condition (delete-process process)
            (error (push (csharp398-test-condition condition root)
                         cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case condition (kill-buffer buffer)
            (error (push (csharp398-test-condition condition root)
                         cleanup-errors)))))
      (dolist (timer (copy-sequence timer-list))
        (unless (memq timer timers-before)
          (condition-case condition (cancel-timer timer)
            (error (push (csharp398-test-condition condition root)
                         cleanup-errors)))))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (condition-case condition (delete-frame frame t)
            (error (push (csharp398-test-condition condition root)
                         cleanup-errors)))))
      (condition-case condition
          (set-window-configuration window-before)
        (error (push (csharp398-test-condition condition root) cleanup-errors)))
      (dolist (entry parked)
        (condition-case condition
            (if (buffer-live-p (car entry))
                (with-current-buffer (car entry)
                  (rename-buffer (cdr entry) t))
              (error "Parked Csharp Mode buffer died: %S" entry))
          (error (push (csharp398-test-condition condition root) cleanup-errors))))
      (when (buffer-live-p buffer-before) (set-buffer buffer-before))
      (when root-owned
        (condition-case condition (delete-directory root t)
          (error (push (csharp398-test-condition condition root) cleanup-errors)))))
    (let ((cleanup
           (list :new-buffers
                 (mapcar #'buffer-name
                         (seq-filter (lambda (buffer)
                                       (and (buffer-live-p buffer)
                                            (not (memq buffer buffers-before))))
                                     (buffer-list)))
                 :new-processes
                 (length (seq-remove (lambda (process)
                                       (memq process processes-before))
                                     (process-list)))
                 :new-timers
                 (length (seq-remove (lambda (timer) (memq timer timers-before))
                                     timer-list))
                 :new-frames
                 (length (seq-remove (lambda (frame) (memq frame frames-before))
                                     (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :fixture-restored (equal fixture-before fixture-after)
                 :window-restored
                 (equal window-state-before (csharp398-test-window-state))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Csharp Mode workflow failed: %S" (list result cleanup))
        (csharp398-test-normalize
         (list :source (nth 1 csharp398-test-source)
               :result result
               :cleanup cleanup)
         root)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CSHARP_MODE_MELPA_PIN, "csharp-mode.el")
        .expect("prepare exact shallow standalone Csharp Mode source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn automatic_cs_file_activation_and_semantic_fontification() -> ParityBatchCase {
    ParityBatchCase::value(
        "automatic_cs_file_activation_and_semantic_fontification",
        r####"
(csharp398-test-run
 '(("src/Program.cs" . "#if DEBUG\nusing System;\n#endif\nnamespace Café.App {\n    #region Greeting\n    /// <summary>Greets a user.</summary>\n    [Obsolete]\n    public sealed class Greeter {\n        public string Message(bool ready) {\n            var path = @\"C:\\\\界\\\\file.txt\";\n            return ready ? \"hello\" : null;\n        }\n    }\n    #endregion\n}\n"))
 (lambda (root)
   (let* ((file (expand-file-name "src/Program.cs" root))
          (buffer (find-file-noselect file)))
     (with-current-buffer buffer
       (font-lock-ensure)
       (list :mode major-mode
             :style c-indentation-style
             :indent indent-line-function
             :comment (list comment-start comment-end comment-padding)
             :doc-style (copy-tree c-doc-comment-style)
             :keys (mapcar (lambda (key) (key-binding (kbd key)))
                           '("{" "}" "C-c C-c"))
             :faces
             (mapcar (lambda (probe) (apply #'csharp398-test-face-probe probe))
                     '((using "using")
                       (if-directive "#if")
                       (region-directive "#region")
                       (namespace "namespace")
                       (namespace-name "Café.App")
                       (namespace-unicode-tail "é.App")
                       (doc-tag "<summary>")
                       (attribute "Obsolete")
                       (class-keyword "class")
                       (class-name "Greeter")
                       (method "Message")
                       (primitive "bool")
                       (verbatim-path "C:\\\\界\\\\file.txt")
                       (constant "null"))))))))
"####,
        expect![[
            r##"OK (:source "0fa4030003726d8e8e05d25546d02e0eb0f07fc93cd16d3d74a0b54f000fdd40" :result (:mode csharp-mode :style "csharp" :indent c-indent-line :comment ("// " "" " ") :doc-style ((csharp-mode . codedoc)) :keys (c-electric-brace c-electric-brace comment-region) :faces ((using "using" font-lock-keyword-face code) (if-directive "#if" font-lock-preprocessor-face code) (region-directive "#region" font-lock-preprocessor-face code) (namespace "namespace" font-lock-keyword-face code) (namespace-name "Café.App" font-lock-variable-name-face code) (namespace-unicode-tail "é.App" font-lock-variable-name-face code) (doc-tag "<summary>" (font-lock-doc-markup-face font-lock-doc-face) comment) (attribute "Obsolete" font-lock-variable-name-face code) (class-keyword "class" font-lock-keyword-face code) (class-name "Greeter" nil code) (method "Message" font-lock-function-name-face code) (primitive "bool" font-lock-type-face code) (verbatim-path "C:\\\\界\\\\file.txt" font-lock-string-face string) (constant "null" font-lock-constant-face code))) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :fixture-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn public_indentation_and_electric_brace_edit_real_csharp() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_indentation_and_electric_brace_edit_real_csharp",
        r####"
(csharp398-test-run
 nil
 (lambda (_root)
   (with-temp-buffer
     (insert "namespace Demo\n{\nclass Calculator\n{\npublic int Sum(int left, int right)\n{\nvar values = new[]\n{\nleft,\nright\n};\nFunc<int, int> scale = value =>\n{\nreturn value * 2;\n};\nreturn scale(values.Sum());\n}\n}\n}\n")
     (csharp-mode)
     (setq-local indent-tabs-mode nil)
     (setq-local c-basic-offset 4)
     (indent-region (point-min) (point-max))
     (let ((indented (buffer-substring-no-properties
                      (point-min) (point-max)))
           brace-state)
       (erase-buffer)
       (insert "if (ready)")
       (electric-pair-local-mode 1)
       (let ((last-command-event ?{))
         (call-interactively (key-binding (kbd "{"))))
       (setq brace-state
             (list :text (buffer-string)
                   :point (point)
                   :pair-mode electric-pair-mode
                   :command (key-binding (kbd "{"))))
       (list :indented indented :brace brace-state)))))
"####,
        expect![[
            r#"OK (:source "0fa4030003726d8e8e05d25546d02e0eb0f07fc93cd16d3d74a0b54f000fdd40" :result (:indented "namespace Demo\n{\n    class Calculator\n    {\n        public int Sum(int left, int right)\n        {\n            var values = new[]\n            {\n                left,\n                right\n            };\n            Func<int, int> scale = value =>\n            {\n                return value * 2;\n            };\n            return scale(values.Sum());\n        }\n    }\n}\n" :brace (:text "if (ready){}" :point 12 :pair-mode t :command c-electric-brace)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :fixture-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_defun_and_statement_navigation_cross_csharp_members() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_defun_and_statement_navigation_cross_csharp_members",
        r####"
(csharp398-test-run
 nil
 (lambda (_root)
   (with-temp-buffer
     (insert "namespace Shop {\n    public delegate void Changed(string value);\n    public enum Status { Ready, Closed }\n    public class Calculator {\n        public int Total { get; private set; }\n        public int Compute(int left, int right) {\n            return left + right;\n        }\n        private void Trace(string message) {\n            Console.WriteLine(message);\n        }\n    }\n}\n")
     (csharp-mode)
     (let (compute-begin compute-end statement-begin statement-end)
       (goto-char (point-min))
       (search-forward "return left")
       (beginning-of-defun)
       (setq compute-begin (csharp398-test-line-state))
       (search-forward "return left")
       (end-of-defun)
       (setq compute-end (csharp398-test-line-state))
       (goto-char (point-min))
       (search-forward "Console.WriteLine")
       (c-beginning-of-statement 1)
       (setq statement-begin (csharp398-test-line-state))
       (c-end-of-statement 1)
       (setq statement-end (csharp398-test-line-state))
       (list :compute-begin compute-begin
             :compute-end compute-end
             :statement-begin statement-begin
             :statement-end statement-end)))))
"####,
        expect![[
            r#"OK (:source "0fa4030003726d8e8e05d25546d02e0eb0f07fc93cd16d3d74a0b54f000fdd40" :result (:compute-begin (:file nil :line 6 :column 0 :text "        public int Compute(int left, int right) {") :compute-end (:file nil :line 9 :column 0 :text "        private void Trace(string message) {") :statement-begin (:file nil :line 10 :column 12 :text "            Console.WriteLine(message);") :statement-end (:file nil :line 10 :column 39 :text "            Console.WriteLine(message);")) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :fixture-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_compilation_goto_error_navigates_msbuild_error_and_warning() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_compilation_goto_error_navigates_msbuild_error_and_warning",
        r####"
(csharp398-test-run
 '(("Folder\\Class1.cs" . "namespace ClassLibrary1.Folder\n{\n    public class Class1\n    {\n        private int seed;\n\n        public void Run()\n        {\n            seed++;\n        }\n        int foo\n    }\n}\n"))
 (lambda (root)
   (let ((build (get-buffer-create "*Csharp Build*")) first second faces
         error-counts warning-counts selected)
     (with-current-buffer build
       (setq default-directory root)
       (compilation-mode)
       (let ((inhibit-read-only t))
         (erase-buffer)
         ;; Exact record from the pinned upstream msbuild-error.txt fixture.
         (insert "Folder\\Class1.cs(11,12): error CS1002: ; expected [c:\\Users\\jesse_000\\Dropbox\\barfapp\\ConsoleApplication1\\ClassLibrary1\\ClassLibrary1.csproj]\n"))
       (let ((inhibit-read-only t))
         (compilation-parse-errors (point-min) (point-max) 'msbuild-error))
       (setq error-counts
             (list compilation-num-errors-found
                   compilation-num-warnings-found
                   compilation-num-infos-found))
       (setq faces
             (list (csharp398-test-compilation-face-probe
                    'error-file "Folder\\Class1.cs")))
       (push (csharp398-test-select-first-compilation-message) selected)
       (call-interactively #'compile-goto-error))
     (setq first (with-current-buffer (window-buffer (selected-window))
                   (csharp398-test-line-state)))
     (with-current-buffer build
       (let ((inhibit-read-only t))
         (erase-buffer))
       (compilation-mode)
       (let ((inhibit-read-only t))
         ;; Exact record from the pinned upstream msbuild-warning.txt fixture.
         (insert "Folder\\Class1.cs(11,9): warning CS0169: The field 'ClassLibrary1.Folder.Class1.foo' is never used [c:\\Users\\jesse_000\\Dropbox\\barfapp\\ConsoleApplication1\\ClassLibrary1\\ClassLibrary1.csproj]\n"))
       (let ((inhibit-read-only t))
         (compilation-parse-errors (point-min) (point-max) 'msbuild-warning))
       (setq warning-counts
             (list compilation-num-errors-found
                   compilation-num-warnings-found
                   compilation-num-infos-found))
       (setq faces
             (append faces
                     (list (csharp398-test-compilation-face-probe
                            'warning-file "Folder\\Class1.cs"))))
       (push (csharp398-test-select-first-compilation-message) selected)
       (call-interactively #'compile-goto-error))
     (setq second (with-current-buffer (window-buffer (selected-window))
                    (csharp398-test-line-state)))
     (list :counts (list error-counts warning-counts)
           :selected (nreverse selected)
           :faces faces :first first :second second))))
"####,
        expect![[
            r#"OK (:source "0fa4030003726d8e8e05d25546d02e0eb0f07fc93cd16d3d74a0b54f000fdd40" :result (:counts ((1 0 0) (0 1 0)) :selected ((:line 1 :column 0 :text "Folder\\Class1.cs(11,12): error CS1002: ; expected [c:\\Users\\jesse_000\\Dropbox\\barfapp\\ConsoleApplication1\\ClassLibrary1\\ClassLibrary1.csproj]") (:line 1 :column 0 :text "Folder\\Class1.cs(11,9): warning CS0169: The field 'ClassLibrary1.Folder.Class1.foo' is never used [c:\\Users\\jesse_000\\Dropbox\\barfapp\\ConsoleApplication1\\ClassLibrary1\\ClassLibrary1.csproj]")) :faces ((error-file "Folder\\Class1.cs" (compilation-error underline)) (warning-file "Folder\\Class1.cs" (compilation-warning underline))) :first (:file "Folder\\Class1.cs" :line 11 :column 11 :text "        int foo") :second (:file "Folder\\Class1.cs" :line 11 :column 8 :text "        int foo")) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :fixture-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn invalid_multiline_string_recovers_after_public_edit() -> ParityBatchCase {
    ParityBatchCase::value(
        "invalid_multiline_string_recovers_after_public_edit",
        r####"
(csharp398-test-run
 nil
 (lambda (_root)
   (with-temp-buffer
     (insert "class Message {\n    string Text = \"first\nsecond\";\n}\n")
     (csharp-mode)
     (font-lock-ensure)
     (let ((before
            (list (csharp398-test-face-probe 'invalid-quote "\"first")
                  (csharp398-test-face-probe 'invalid-body "second"))))
       (goto-char (point-min))
       (search-forward "= \"")
       (backward-char 1)
       (let ((last-command-event ?@))
         (self-insert-command 1))
       (font-lock-flush)
       (font-lock-ensure)
       (list :before before
             :after
             (list (csharp398-test-face-probe 'repaired-quote "\"first")
                   (csharp398-test-face-probe 'repaired-body "second"))
             :source (buffer-substring-no-properties (point-min) (point-max))
             :modified (buffer-modified-p))))))
"####,
        expect![[
            r#"OK (:source "0fa4030003726d8e8e05d25546d02e0eb0f07fc93cd16d3d74a0b54f000fdd40" :result (:before ((invalid-quote "\"first" font-lock-warning-face code) (invalid-body "second" nil string)) :after ((repaired-quote "\"first" font-lock-string-face code) (repaired-body "second" font-lock-string-face string)) :source "class Message {\n    string Text = @\"first\nsecond\";\n}\n" :modified t) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :fixture-restored t :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn csharp_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        automatic_cs_file_activation_and_semantic_fontification(),
        public_indentation_and_electric_brace_edit_real_csharp(),
        public_defun_and_statement_navigation_cross_csharp_members(),
        public_compilation_goto_error_navigates_msbuild_error_and_warning(),
        invalid_multiline_string_recovers_after_public_edit(),
    ];
    assert_oracle_batch_cases(oracle(), "csharp-mode-rank398", "Csharp Mode", &cases);
}

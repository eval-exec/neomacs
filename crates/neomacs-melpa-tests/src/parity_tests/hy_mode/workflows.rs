use expect_test::expect;

use super::ParityBatchCase;

/// Opening a `.hy' file: the autoloaded `auto-mode-alist' entry activates
/// `hy-mode' (a `prog-mode' derivative), the Lisp-derived syntax table
/// carries the Hy-specific character classes, the comment and indentation
/// variables are set, `font-lock-defaults' names the Hy keyword corpus plus
/// the syntax alist and the docstring-aware syntactic face function, and
/// the keymap hangs off `lisp-mode-shared-map' with the shell, describe,
/// and pdb bindings.  The `interpreter-mode-alist' entry is pinned too.
fn opening_a_hy_file_activates_the_mode_and_sets_up_its_editing_surface() -> ParityBatchCase {
    ParityBatchCase::value(
        "opening_a_hy_file_activates_the_mode_and_sets_up_its_editing_surface",
        r##"(unwind-protect
    (progn
      (hy-test-reset)
      (let* ((file (hy-test-open "surface.hy"))
             (buffer (find-file-noselect file)))
        (with-current-buffer buffer
          (let ((defaults font-lock-defaults))
            (list
             :file (file-relative-name file default-directory)
             :mode major-mode
             :parent (derived-mode-p 'prog-mode)
             :source (hy-test-source-state)
             :syntax
             (list :brace (char-syntax ?{)
                   :bracket (char-syntax ?\[)
                   :tilde (char-syntax ?~)
                   :at (char-syntax ?@)
                   :comma (char-syntax ?,)
                   :pipe (char-syntax ?|)
                   :hash (char-syntax ?#)
                   :paren (char-syntax ?\)))
             :comments (list :start comment-start
                             :start-skip comment-start-skip
                             :add comment-add)
             :indent (list :tabs indent-tabs-mode
                           :line-fn indent-line-function
                           :lisp-fn lisp-indent-function
                           :exactly hy-indent--exactly
                           :fuzzily hy-indent--fuzzily)
             :font-lock
             (list :multiline font-lock-multiline
                   :kwds (length (symbol-value (car defaults)))
                   :syntax-alist (caddr defaults)
                   :mark-block (cadddr defaults)
                   :syntactic-face
                   (cdr (nth 6 defaults)))
             :syntax-propertize syntax-propertize-function
             :ahs ahs-include
             :keymap
             (list :parent (keymap-parent hy-mode-map)
                   :shell (lookup-key hy-mode-map (kbd "C-c C-z"))
                   :eval-buffer (lookup-key hy-mode-map (kbd "C-c C-b"))
                   :eval-region (lookup-key hy-mode-map (kbd "C-c C-r"))
                   :eval-last-sexp (lookup-key hy-mode-map (kbd "C-c C-e"))
                   :eval-current-form (lookup-key hy-mode-map (kbd "C-M-x"))
                   :describe (lookup-key hy-mode-map (kbd "C-c C-d d"))
                   :pdb (lookup-key hy-mode-map (kbd "C-c C-t")))
             :alists
             (list :auto-mode (and (assoc "\\.hy\\'" auto-mode-alist) t)
                   :interpreter (and (assoc "hy" interpreter-mode-alist) t))
             :commands
             (list :run-hy (fboundp 'run-hy)
                   :inferior (fboundp 'inferior-hy-mode)
                   :inferior-parent
                   (and (fboundp 'inferior-hy-mode)
                        (derived-mode-p 'comint-mode))
                   :describe (fboundp 'hy-describe-thing-at-point)
                   :insert-pdb (fboundp 'hy-insert-pdb)))))))
  (hy-test-reset))"##,
        expect![[
            r#"OK (:file "surface.hy" :mode hy-mode :parent prog-mode :source (:upstream-tree "2245e7658c4a87285218aa72a71c368a5d504245" :feature t :version "20211016.2011" :dash "20260221.1346" :s "20220902.1511") :syntax (:brace 40 :bracket 40 :tilde 39 :at 39 :comma 95 :pipe 95 :hash 95 :paren 41) :comments (:start ";" :start-skip "\\(\\(^\\|[^\\\\\n]\\)\\(\\\\\\\\\\)*\\)\\(;+\\|#|\\) *" :add 1) :indent (:tabs nil :line-fn lisp-indent-line :lisp-fn hy-indent-function :exactly ("when" "unless" "for" "for*" "for/a" "for/a*" "while" "except" "catch") :fuzzily ("def" "let" "with" "with/a" "fn" "fn/a")) :font-lock (:multiline t :kwds 17 :syntax-alist nil :mark-block (("+-*/.<>=!?$%_&~^:@" . "w")) :syntactic-face hy-font-lock-syntactic-face-function) :syntax-propertize hy-syntax-propertize-function :ahs "^[0-9A-Za-z/_.,:;*+=&%|$#@!^?-~-]+$" :keymap (:parent (keymap (127 . backward-delete-char-untabify) (27 keymap (17 . indent-sexp)) keymap (27 keymap (113 . prog-fill-reindent-defun) (17 . prog-indent-sexp))) :shell run-hy :eval-buffer hy-shell-eval-buffer :eval-region hy-shell-eval-region :eval-last-sexp hy-shell-eval-last-sexp :eval-current-form hy-shell-eval-current-form :describe hy-describe-thing-at-point :pdb hy-insert-pdb) :alists (:auto-mode t :interpreter t) :commands (:run-hy t :inferior t :inferior-parent nil :describe t :insert-pdb t))"#
        ]],
    )
}

/// Fontifying a real Hy program: every construct the package documents --
/// definitions, builtins, anaphorics, class and decorator forms, comments,
/// strings, the module docstring, a bracket string (fenced by the
/// syntax-propertize function), and a tag macro with an anonymous `%i' --
/// receives its documented font-lock face on the whole line it lives on.
fn fontifying_a_real_hy_program_paints_each_construct() -> ParityBatchCase {
    ParityBatchCase::value(
        "fontifying_a_real_hy_program_paints_each_construct",
        r##"(unwind-protect
    (progn
      (hy-test-reset)
      (let* ((file (hy-test-open "program.hy"))
             (buffer (find-file-noselect file)))
        (with-current-buffer buffer
          (insert
           ";;; program.hy --- module fixture\n"
           "\"Module docstring.\"\n\n"
           "(defn greet [name]\n"
           "  \"Greet NAME.\"\n"
           "  (print (+ \"Hello, \" name \"!\")))\n\n"
           "(defclass Point [object]\n"
           "  (defn __init__ [self x y]\n"
           "    (setv self.x x)\n"
           "    (setv self.y y)))\n\n"
           "(setv primes [2 3 5 7])\n\n"
           "(when (coll? primes)\n"
           "  (print (ap-filter (fn [x] (> x 2)) primes)))\n\n"
           "#[d[bracket string with \"quotes\"]]\n\n"
           ";; a comment line\n"
           "(print #tag(odd-format \"%i %s\" 1 \"x\"))\n\n"
           "(with/a [f (open \"file.txt\")] (print (.read f)))\n")
          (save-buffer)
          (font-lock-ensure)
          (list
           :mode major-mode
           :lines
           (mapcar (lambda (needle) (hy-test-line-runs needle))
                   '(";;; program.hy"
                     "\"Module docstring.\""
                     "(defn greet"
                     "\"Greet NAME.\""
                     "(defclass Point"
                     "(setv primes"
                     "(when (coll?"
                     "#[d[bracket string"
                     ";; a comment line"
                     "#tag(odd-format"
                     "(with/a [f"))
           :ppss-bracket-string
           (let ((pos (save-excursion
                        (goto-char (point-min))
                        (search-forward "#[d[bracket string")
                        (point))))
             (list :in-string (nth 3 (syntax-ppss pos))
                   :string-start (nth 8 (syntax-ppss pos))))))))
  (hy-test-reset))"##,
        expect![[
            r##"OK (:mode hy-mode :lines ((:needle ";;; program.hy" :line ";;; program.hy --- module fixture" :runs ((";;; " (font-lock-comment-delimiter-face)) ("program.hy --- module fixture" (font-lock-comment-face)))) (:needle "\"Module docstring.\"" :line "\"Module docstring.\"" :runs (("\"Module docstring.\"" (font-lock-string-face)))) (:needle "(defn greet" :line "(defn greet [name]" :runs (("(" nil) ("defn" (font-lock-keyword-face)) (" " nil) ("greet" (font-lock-function-name-face)) (" [" nil) ("name" (font-lock-builtin-face)) ("]" nil))) (:needle "\"Greet NAME.\"" :line "  \"Greet NAME.\"" :runs (("  " nil) ("\"Greet NAME.\"" (font-lock-doc-face)))) (:needle "(defclass Point" :line "(defclass Point [object]" :runs (("(" nil) ("defclass" (font-lock-keyword-face)) (" " nil) ("Point" (font-lock-type-face)) (" [" nil) ("object" (font-lock-builtin-face)) ("]" nil))) (:needle "(setv primes" :line "(setv primes [2 3 5 7])" :runs (("(" nil) ("setv" (font-lock-builtin-face)) (" " nil) ("primes" (font-lock-variable-name-face)) (" [2 3 5 7])" nil))) (:needle "(when (coll?" :line "(when (coll? primes)" :runs (("(" nil) ("when" (font-lock-keyword-face)) (" (" nil) ("coll?" (font-lock-builtin-face)) (" primes)" nil))) (:needle "#[d[bracket string" :line "#[d[bracket string with \"quotes\"]]" :runs (("#[d" nil) ("[bracket string with \"quotes\"]" (font-lock-string-face)) ("]" nil))) (:needle ";; a comment line" :line ";; a comment line" :runs ((";; " (font-lock-comment-delimiter-face)) ("a comment line" (font-lock-comment-face)))) (:needle "#tag(odd-format" :line "(print #tag(odd-format \"%i %s\" 1 \"x\"))" :runs (("(" nil) ("print" (font-lock-keyword-face)) (" " nil) ("#tag" (font-lock-function-name-face)) ("(odd-format " nil) ("\"%i %s\"" (font-lock-string-face)) (" 1 " nil) ("\"x\"" (font-lock-string-face)) ("))" nil))) (:needle "(with/a [f" :line "(with/a [f (open \"file.txt\")] (print (.read f)))" :runs (("(" nil) ("with/a" (font-lock-keyword-face)) (" [f (" nil) ("open" (font-lock-builtin-face)) (" " nil) ("\"file.txt\"" (font-lock-string-face)) (")] (" nil) ("print" (font-lock-keyword-face)) (" (.read f)))" nil)))) :ppss-bracket-string (:in-string t :string-start 319))"##
        ]],
    )
}

/// The indentation rules: forms on `hy-indent--exactly' indent their body
/// one column deeper, `hy-indent--fuzzily' prefixes match at the form
/// start, nested lists fall back to `calculate-lisp-indent', and the
/// bracket-string fence keeps multi-line strings out of the indenter's
/// way.  Each probe inserts an unindented body line and runs
/// `lisp-indent-line' the way the mode binds it.
fn the_indentation_rules_indent_hy_forms() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_indentation_rules_indent_hy_forms",
        r##"(unwind-protect
    (progn
      (hy-test-reset)
      (let* ((file (hy-test-open "indent.hy"))
             (buffer (find-file-noselect file)))
        (with-current-buffer buffer
          (let ((probe
                 (lambda (text)
                   (erase-buffer)
                   (insert text)
                   (goto-char (point-max))
                   (insert "\nbody-form")
                   (lisp-indent-line)
                   (current-column))))
            (list
             :mode major-mode
             :when (funcall probe "(when foo")
             :unless (funcall probe "(unless foo")
             :while (funcall probe "(while test")
             :defn (funcall probe "(defn f [x]")
             :let (funcall probe "(let [x 1]")
             :with-fuzzy (funcall probe "(with-extra foo")
             :fn-fuzzy (funcall probe "(fn/awesome [x]")
             :nested (funcall probe "(print (+ 1")
             :unmatched (funcall probe "(print \"lit\""))))))
  (hy-test-reset))"##,
        expect![
            "OK (:mode hy-mode :when 11 :unless 11 :while 11 :defn 11 :let 11 :with-fuzzy 11 :fn-fuzzy 11 :nested 19 :unmatched 16)"
        ],
    )
}

/// The command surface: `hy-insert-pdb' inserts the documented pdb form,
/// the shell command set is defined, `inferior-hy-mode' derives from
/// comint, and the describe command exists.  None of them launch the
/// external `hy' binary in batch.
fn the_command_surface_inserts_the_pdb_form_and_keeps_the_shell_bindings() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_command_surface_inserts_the_pdb_form_and_keeps_the_shell_bindings",
        r##"(unwind-protect
    (progn
      (hy-test-reset)
      (let* ((file (hy-test-open "commands.hy"))
             (buffer (find-file-noselect file)))
        (with-current-buffer buffer
          (goto-char (point-max))
          (let ((before (point)))
            (hy-insert-pdb)
            (list
             :mode major-mode
             :inserted
             (buffer-substring-no-properties before (point))
             :inferior-doc
             (and (fboundp 'inferior-hy-mode)
                  (substring-no-properties
                   (documentation 'inferior-hy-mode) 0 12))
             :run-hy-doc
             (substring-no-properties (documentation 'run-hy) 0 12)
             :describe-doc
             (substring-no-properties
              (documentation 'hy-describe-thing-at-point) 0 12))))))
  (hy-test-reset))"##,
        expect![[
            r#"OK (:mode hy-mode :inserted "(do (import pdb) (pdb.set-trace))" :inferior-doc "Major mode f" :run-hy-doc "Startup and/" :describe-doc "Describe sym")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        opening_a_hy_file_activates_the_mode_and_sets_up_its_editing_surface(),
        fontifying_a_real_hy_program_paints_each_construct(),
        the_indentation_rules_indent_hy_forms(),
        the_command_surface_inserts_the_pdb_form_and_keeps_the_shell_bindings(),
    ]
}

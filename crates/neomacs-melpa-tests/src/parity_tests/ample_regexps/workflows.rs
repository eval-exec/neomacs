use expect_test::expect;

use super::ParityBatchCase;

/// One `define-arx' call is the whole installation: it leaves behind a macro, a
/// `-to-string' function for building the same regexp at run time, a
/// `-bindings' variable holding the translated forms, and the properties that
/// mark them as belonging to this arx.  The regexp a realistic composition
/// produces is pinned whole, from both the macro and the function, and the
/// bindings are pinned as the package translated them.
fn define_arx_builds_the_macro_its_to_string_function_and_its_bindings() -> ParityBatchCase {
    ParityBatchCase::value(
        "define_arx_builds_the_macro_its_to_string_function_and_its_bindings",
        r##"(progn
  (arx-test-define-log-rx)
  (list :surface (arx-test-surface 'log-rx)
        :bindings
        (mapcar
         (lambda (binding)
           (if (eq (car binding) 'bracketed)
               (let* ((evaluator (nth 2 binding))
                      (application (nth 1 evaluator))
                      (function (nth 3 application)))
                 (list (nth 0 binding)
                       (nth 1 binding)
                       (list :evaluator (car evaluator)
                             :dispatcher (car application)
                             :arity (eval (nth 1 application) t)
                             :predicate (nth 2 application)
                             :byte-code-function
                             (and (functionp function)
                                  (byte-code-function-p function)
                                  t)
                             :name (eval (nth 4 application) t)
                             :arguments (eval (nth 5 application) t))))
             binding))
         log-rx-bindings)
        :log-line (arx-test-expand '(log-rx stamp ws level ws qualified))
        :same-from-to-string (log-rx-to-string '(seq stamp ws level ws qualified) t)
        :macro-and-function-agree
        (equal (arx-test-expand '(log-rx stamp ws level ws qualified))
               (log-rx-to-string '(seq stamp ws level ws qualified) t))))"##,
        expect![[
            r#"OK (:surface (:macro t :to-string t :bindings-bound t :arx-name "log-rx" :to-string-arx-name "log-rx" :form-count 6) :bindings ((ws (regexp "[ \11]+")) (level (or "DEBUG" "INFO" "WARN" "ERROR")) (ident (regexp "[A-Za-z_][A-Za-z0-9_]*")) (qualified (seq ident (* "." ident))) (stamp (seq (= 4 digit) "-" (= 2 digit) "-" (= 2 digit))) (bracketed (&rest bracketed-args) (:evaluator eval :dispatcher arx--apply-func-post-27 :arity (1 2) :predicate nil :byte-code-function t :name bracketed :arguments (bracketed-args)))) :log-line "[[:digit:]]\\{4\\}-[[:digit:]]\\{2\\}-[[:digit:]]\\{2\\}\\(?:[ \11]+\\)\\(?:DEBUG\\|ERROR\\|INFO\\|WARN\\)\\(?:[ \11]+\\)\\(?:[A-Za-z_][A-Za-z0-9_]*\\)\\(?:\\.\\(?:[A-Za-z_][A-Za-z0-9_]*\\)\\)*" :same-from-to-string "[[:digit:]]\\{4\\}-[[:digit:]]\\{2\\}-[[:digit:]]\\{2\\}\\(?:[ \11]+\\)\\(?:DEBUG\\|ERROR\\|INFO\\|WARN\\)\\(?:[ \11]+\\)\\(?:[A-Za-z_][A-Za-z0-9_]*\\)\\(?:\\.\\(?:[A-Za-z_][A-Za-z0-9_]*\\)\\)*" :macro-and-function-agree t)"#
        ]],
    )
}

fn named_forms_compose_with_each_other_and_with_plain_rx_forms() -> ParityBatchCase {
    ParityBatchCase::value(
        "named_forms_compose_with_each_other_and_with_plain_rx_forms",
        r##"(progn
  (arx-test-define-log-rx)
  (list
   :each-form
   (mapcar (lambda (form) (cons form (log-rx-to-string form t)))
           '(ws level ident qualified stamp))
   :composition
   (list :grouped (arx-test-expand '(log-rx (group level) ": " (group qualified)))
         :alternation (arx-test-expand '(log-rx (or level ident)))
         :anchored (arx-test-expand '(log-rx line-start stamp ws level line-end))
         :repetition (arx-test-expand '(log-rx (one-or-more ident)
                                               (zero-or-one ws)
                                               (= 3 level)
                                               (** 1 4 ident)
                                               (repeat 2 stamp)))
         :classes (arx-test-expand '(log-rx (any "a-z" ?_) (not (any digit)) word-boundary))
         :nested (arx-test-expand '(log-rx (seq (or (seq stamp ws) (seq level ws))
                                                (zero-or-more qualified)))))))"##,
        expect![[
            r#"OK (:each-form ((ws . "[ \11]+") (level . "\\(?:DEBUG\\|ERROR\\|INFO\\|WARN\\)") (ident . "[A-Za-z_][A-Za-z0-9_]*") (qualified . "\\(?:[A-Za-z_][A-Za-z0-9_]*\\)\\(?:\\.\\(?:[A-Za-z_][A-Za-z0-9_]*\\)\\)*") (stamp . "[[:digit:]]\\{4\\}-[[:digit:]]\\{2\\}-[[:digit:]]\\{2\\}")) :composition (:grouped "\\(\\(?:DEBUG\\|ERROR\\|INFO\\|WARN\\)\\): \\(\\(?:[A-Za-z_][A-Za-z0-9_]*\\)\\(?:\\.\\(?:[A-Za-z_][A-Za-z0-9_]*\\)\\)*\\)" :alternation "\\(?:DEBUG\\|ERROR\\|INFO\\|WARN\\)\\|[A-Za-z_][A-Za-z0-9_]*" :anchored "^[[:digit:]]\\{4\\}-[[:digit:]]\\{2\\}-[[:digit:]]\\{2\\}\\(?:[ \11]+\\)\\(?:DEBUG\\|ERROR\\|INFO\\|WARN\\)$" :repetition "\\(?:[A-Za-z_][A-Za-z0-9_]*\\)+\\(?:[ \11]+\\)?\\(?:DEBUG\\|ERROR\\|INFO\\|WARN\\)\\{3\\}\\(?:[A-Za-z_][A-Za-z0-9_]*\\)\\{1,4\\}\\(?:[[:digit:]]\\{4\\}-[[:digit:]]\\{2\\}-[[:digit:]]\\{2\\}\\)\\{2\\}" :classes "[_a-z][^[:digit:]]\\b" :nested "\\(?:[[:digit:]]\\{4\\}-[[:digit:]]\\{2\\}-[[:digit:]]\\{2\\}\\(?:[ \11]+\\)\\|\\(?:DEBUG\\|ERROR\\|INFO\\|WARN\\)\\(?:[ \11]+\\)\\)\\(?:\\(?:[A-Za-z_][A-Za-z0-9_]*\\)\\(?:\\.\\(?:[A-Za-z_][A-Za-z0-9_]*\\)\\)*\\)*"))"#
        ]],
    )
}

fn func_forms_run_while_expanding_and_their_arity_and_predicate_are_enforced() -> ParityBatchCase {
    ParityBatchCase::value(
        "func_forms_run_while_expanding_and_their_arity_and_predicate_are_enforced",
        r##"(progn
  (arx-test-define-log-rx)
  (eval '(define-arx pred-rx
           `((tagged (:func ,(lambda (form &rest args)
                               (rx-to-string `(seq "<" (seq ,@args) ">") t))
                            :predicate ,(symbol-function 'stringp)))))
        t)
  (eval '(define-arx symbol-pred-rx
           `((tagged (:func ,(lambda (form &rest args)
                               (rx-to-string `(seq "<" (seq ,@args) ">") t))
                            :predicate stringp))))
        t)
  (list :one-argument (arx-test-expand '(log-rx (bracketed level)))
        :two-arguments (arx-test-expand '(log-rx (bracketed level ws)))
        :inside-a-composition
        (arx-test-expand '(log-rx line-start (bracketed level) ws qualified))
        :too-few (arx-test-expand '(log-rx (bracketed)))
        :too-many (arx-test-expand '(log-rx (bracketed level ws level)))
        :unknown-form (arx-test-expand '(log-rx nosuchform))
        :predicate-satisfied (arx-test-expand '(pred-rx (tagged "a" "b")))
        :predicate-violated (arx-test-expand '(pred-rx (tagged 42)))
        :predicate-named-as-a-symbol
        (arx-test-expand '(symbol-pred-rx (tagged "a")))))"##,
        expect![[
            r#"OK (:one-argument "\\[\\(?:DEBUG\\|ERROR\\|INFO\\|WARN\\)]" :two-arguments "\\[\\(?:DEBUG\\|ERROR\\|INFO\\|WARN\\)\\(?:[ \11]+\\)]" :inside-a-composition "^\\(?:\\[\\(?:DEBUG\\|ERROR\\|INFO\\|WARN\\)]\\)\\(?:[ \11]+\\)\\(?:[A-Za-z_][A-Za-z0-9_]*\\)\\(?:\\.\\(?:[A-Za-z_][A-Za-z0-9_]*\\)\\)*" :too-few (error "rx form ‘bracketed’ requires at least 1 arg") :too-many (error "rx form ‘bracketed’ accepts at most 2 args") :unknown-form (error "Unknown rx symbol ‘nosuchform’") :predicate-satisfied "<ab>" :predicate-violated "<\\*>" :predicate-named-as-a-symbol (void-variable stringp))"#
        ]],
    )
}

fn the_generated_macro_produces_the_same_regexps_when_byte_compiled() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_generated_macro_produces_the_same_regexps_when_byte_compiled",
        r##"(let ((source (arx-test-write
                "lib/logmatch.el"
                (concat ";;; logmatch.el --- fixture  -*- lexical-binding: t; -*-\n"
                        "(require 'ample-regexps)\n"
                        "(define-arx bc-rx\n"
                        "  `((ws (regexp \"[ \\t]+\"))\n"
                        "    (level (or \"DEBUG\" \"INFO\" \"WARN\" \"ERROR\"))\n"
                        "    (ident (regexp \"[A-Za-z_][A-Za-z0-9_]*\"))\n"
                        "    (qualified (seq ident (* \".\" ident)))\n"
                        "    (wrapped (:func ,(lambda (form &rest args)\n"
                        "                       (rx-to-string `(seq \"<\" (seq ,@args) \">\") t))))))\n"
                        "(defun bc-line-regexp ()\n"
                        "  (bc-rx line-start level ws qualified line-end))\n"
                        "(defun bc-wrapped-regexp () (bc-rx (wrapped ident)))\n"
                        "(provide 'logmatch)\n"))))
  (require 'bytecomp)
  (let ((compiled (let ((byte-compile-verbose nil)
                        (byte-compile-warnings nil))
                    (byte-compile-file source))))
    (load (concat source "c") nil t t)
    (list :compiled compiled
          :elc-exists (file-exists-p (concat source "c"))
          :functions-are-byte-code
          (list (byte-code-function-p (symbol-function 'bc-line-regexp))
                (byte-code-function-p (symbol-function 'bc-wrapped-regexp)))
          :surface (arx-test-surface 'bc-rx)
          :line (bc-line-regexp)
          :wrapped (bc-wrapped-regexp)
          :matches-interpreted
          (list (equal (bc-line-regexp)
                       (bc-rx-to-string '(seq line-start level ws qualified line-end) t))
                (equal (bc-wrapped-regexp)
                       (bc-rx-to-string '(wrapped ident) t))))))"##,
        expect![[
            r#"OK (:compiled t :elc-exists t :functions-are-byte-code (t t) :surface (:macro t :to-string t :bindings-bound t :arx-name "bc-rx" :to-string-arx-name "bc-rx" :form-count 5) :line "^\\(?:DEBUG\\|ERROR\\|INFO\\|WARN\\)\\(?:[ \11]+\\)\\(?:[A-Za-z_][A-Za-z0-9_]*\\)\\(?:\\.\\(?:[A-Za-z_][A-Za-z0-9_]*\\)\\)*$" :wrapped "<\\(?:[A-Za-z_][A-Za-z0-9_]*\\)>" :matches-interpreted (t t))"#
        ]],
    )
}

fn the_generated_macro_documents_every_named_form_it_offers() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_generated_macro_documents_every_named_form_it_offers",
        r##"(progn
  (arx-test-define-log-rx)
  (list :macro-documentation (documentation 'log-rx)
        :to-string-documentation (documentation 'log-rx-to-string)
        :bindings-documentation (get 'log-rx-bindings 'variable-documentation)))"##,
        expect![[
            r#"OK (:macro-documentation "Translate regular expressions REGEXPS in sexp form to a regexp string.\n\nSee macro ‘rx’ for more documentation on REGEXPS parameter.\nThis macro additionally supports the following forms:\n\n‘ws’\n    An alias for (regexp \"[ \\11]+\").\n\n‘level’\n    An alias for (or \"DEBUG\" \"INFO\" \"WARN\" \"ERROR\").\n\n‘ident’\n    An alias for (regexp \"[A-Za-z_][A-Za-z0-9_]*\").\n\n‘qualified’\n    An alias for (seq ident (* \".\" ident)).\n\n‘stamp’\n    An alias for (seq (= 4 digit) \"-\" (= 2 digit) \"-\" (= 2 digit)).\n\n‘(bracketed &rest args)’\n    Function without documentation.\n\nUse function ‘log-rx-to-string’ to do such a translation at run-time." :to-string-documentation "Parse and produce code for regular expression FORM.\n\nFORM is a regular expression in sexp form as supported by ‘log-rx’.\nNO-GROUP non-nil means don’t put shy groups around the result." :bindings-documentation "List of bindings for `log-rx' and `log-rx-to-string' functions.\n\nSee `log-rx' for a human readable list of defined forms.\n\nSee parameter BINDINGS for function `rx-let' for more information\nabout format of elements of this list.")"#
        ]],
    )
}

fn the_func_helpers_and_the_builder_no_longer_work_on_a_current_emacs() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_func_helpers_and_the_builder_no_longer_work_on_a_current_emacs",
        r##"(progn
  (arx-test-define-log-rx)
  (list
   :arx-and (condition-case failure (arx-and '("a" "b")) (error failure))
   :arx-or (condition-case failure (arx-or '("a" "b")) (error failure))
   :arx-builder (condition-case failure (arx-builder "log-rx") (error failure))
   :symbol-func
   (list :defining (arx-test-expand '(define-arx symbol-rx
                                       '((wrapped (:func arx-test-wrap)))))
         :using (arx-test-expand '(symbol-rx (wrapped "x"))))
   :sharp-quoted-func
   (arx-test-expand '(define-arx sharp-rx
                       '((wrapped (:func #'arx-test-wrap)))))
   :lambda-func-works (arx-test-expand '(log-rx (bracketed level)))))"##,
        expect![[
            r#"OK (:arx-and (void-function rx-and) :arx-or (void-function rx-or) :arx-builder (void-variable log-rx-constituents) :symbol-func (:defining symbol-rx :using (void-variable arx-test-wrap)) :sharp-quoted-func (error "Not a function: #'arx-test-wrap") :lambda-func-works "\\[\\(?:DEBUG\\|ERROR\\|INFO\\|WARN\\)]")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        define_arx_builds_the_macro_its_to_string_function_and_its_bindings(),
        named_forms_compose_with_each_other_and_with_plain_rx_forms(),
        func_forms_run_while_expanding_and_their_arity_and_predicate_are_enforced(),
        the_generated_macro_produces_the_same_regexps_when_byte_compiled(),
        the_generated_macro_documents_every_named_form_it_offers(),
        the_func_helpers_and_the_builder_no_longer_work_on_a_current_emacs(),
    ]
}

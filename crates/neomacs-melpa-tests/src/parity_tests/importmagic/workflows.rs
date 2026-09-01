use expect_test::expect;

use super::ParityBatchCase;

/// The documented mode gate: enabling importmagic-mode in a buffer that
/// is not derived from python-mode signals the error (with the mode
/// variable already set, since define-minor-mode toggles it before the
/// body runs).
fn the_mode_gate_rejects_non_python_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_mode_gate_rejects_non_python_buffers",
        r####"(let ((gate-error nil)
      (mode-after nil))
  (with-temp-buffer
    (condition-case err
        (importmagic-mode 1)
      (error (setq gate-error (list (car err) (cadr err)))))
    (setq mode-after importmagic-mode))
  (list :error gate-error :mode mode-after))"####,
        expect![[r#"OK (:error (error "Importmagic only works with Python buffers") :mode t)"#]],
    )
}

/// The README workflow: `os.path.join' with no import is fixed through
/// `importmagic-fix-symbol' -- the real candidates query, the real
/// import-statement RPC, and the block inserted at the computed range.
fn the_readme_os_example_is_fixed_end_to_end() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_readme_os_example_is_fixed_end_to_end",
        r####"(unwind-protect
    (progn
      (importmagic--test-open "app.py" "os.path.join('path1', 'path2')\n")
      (with-current-buffer "app.py"
        (importmagic--test-with-ui-capture
         (importmagic-mode 1)
         (let ((server-live (and importmagic-server
                                  (epc:live-p importmagic-server))))
           (importmagic-fix-symbol "os")
           (importmagic--test-result
            :source (importmagic--test-source-state)
            :major-mode major-mode
            :buffer (buffer-substring-no-properties (point-min) (point-max))
            :server-live server-live
            :mode importmagic-mode)))))
  (importmagic--test-reset))"####,
        expect![[
            r#"OK (:source (:upstream-tree "ac43d5984b59b8bbff3d7707f1eb74730d1a49ee" :feature t :version "20180520.303") :major-mode python-mode :buffer "import os\n\n\nos.path.join('path1', 'path2')\n" :server-live (open listen connect stop) :mode t :messages ("[importmagic] Indexed ([ORACLE-SANDBOX]/importmagic-proj)" "[importmagic] Inserted (import os)") :reads ((:prompt "Querying for os: " :options ("import os"))) :calls "(call 4 add_path_to_index \"@@ROOT@@/importmagic-proj\")\n\n(call 5 get_candidates_for_symbol \"os\")\n\n(call 6 get_import_statement (\"os.path.join('path1', 'path2')\\12\" \"import os\" ((multiline max_columns) (parentheses 79))))\n\n")"#
        ]],
    )
}

/// `importmagic-fix-symbol-at-point' invoked interactively on a class
/// defined in the indexed project: the symbol at point is queried and
/// the candidate block lands at the top of the buffer.
fn fix_symbol_at_point_imports_the_project_class() -> ParityBatchCase {
    ParityBatchCase::value(
        "fix_symbol_at_point_imports_the_project_class",
        r####"(unwind-protect
    (progn
      (importmagic--test-open "app.py" "print(Widget())\n")
      (with-current-buffer "app.py"
        (importmagic--test-with-ui-capture
         (importmagic-mode 1)
         (goto-char (point-min))
         (search-forward "Widget")
         (call-interactively 'importmagic-fix-symbol-at-point)
         (importmagic--test-result
          :buffer (buffer-substring-no-properties (point-min) (point-max))
          :point (point)))))
  (importmagic--test-reset))"####,
        expect![[
            r#"OK (:buffer "from widgets import Widget\n\n\nprint(Widget())\n" :point 42 :messages ("[importmagic] Indexed ([ORACLE-SANDBOX]/importmagic-proj)" "[importmagic] Inserted (from widgets import Widget)") :reads ((:prompt "Querying for Widget: " :options ("from widgets import Widget"))) :calls "(call 9 add_path_to_index \"@@ROOT@@/importmagic-proj\")\n\n(call 10 get_candidates_for_symbol \"Widget\")\n\n(call 11 get_import_statement (\"print(Widget())\\12\" \"from widgets import Widget\" ((multiline max_columns) (parentheses 79))))\n\n")"#
        ]],
    )
}

/// `importmagic-fix-imports' fixes every unresolved symbol in one pass:
/// the second statement is merged into the block the first one created,
/// at the server-computed line range.
fn fix_imports_merges_both_statements_into_one_block() -> ParityBatchCase {
    ParityBatchCase::value(
        "fix_imports_merges_both_statements_into_one_block",
        r####"(unwind-protect
    (progn
      (importmagic--test-open "app.py"
                              "print(Widget())\nprint(Spinner())\n")
      (with-current-buffer "app.py"
        (importmagic--test-with-ui-capture
         (importmagic-mode 1)
         (importmagic-fix-imports)
         (importmagic--test-result
          :buffer (buffer-substring-no-properties (point-min) (point-max))))))
  (importmagic--test-reset))"####,
        expect![[
            r#"OK (:buffer "from gadgets.spinner import Spinner\nfrom widgets import Widget\n\n\nprint(Widget())\nprint(Spinner())\n" :messages ("[importmagic] Indexed ([ORACLE-SANDBOX]/importmagic-proj)" "[importmagic] Inserted (from gadgets.spinner import Spinner)" "[importmagic] Inserted (from widgets import Widget)") :reads ((:prompt "Querying for Spinner: " :options ("from gadgets.spinner import Spinner")) (:prompt "Querying for Widget: " :options ("from widgets import Widget"))) :calls "(call 14 add_path_to_index \"@@ROOT@@/importmagic-proj\")\n\n(call 15 get_unresolved_symbols \"print(Widget())\\12print(Spinner())\\12\")\n\n(call 16 get_candidates_for_symbol \"Spinner\")\n\n(call 17 get_import_statement (\"print(Widget())\\12print(Spinner())\\12\" \"from gadgets.spinner import Spinner\" ((multiline max_columns) (parentheses 79))))\n\n(call 18 get_candidates_for_symbol \"Widget\")\n\n(call 19 get_import_statement (\"from gadgets.spinner import Spinner\\12\\12\\12print(Widget())\\12print(Spinner())\\12\" \"from widgets import Widget\" ((multiline max_columns) (parentheses 79))))\n\n")"#
        ]],
    )
}

/// The style configuration crosses the wire: a customized
/// `importmagic-style-configuration-alist' reflows a long existing
/// import into parentheses when the next statement lands.
fn the_customized_style_reflows_the_long_import_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_customized_style_reflows_the_long_import_line",
        r####"(unwind-protect
    (progn
      (importmagic--test-open
       "app.py"
       (concat "from some.long.module.path import very_long_symbol_name_here\n"
               "\n\nprint(Widget())\n"))
      (with-current-buffer "app.py"
        (importmagic--test-with-ui-capture
         (setq-local importmagic-style-configuration-alist
                     '((multiline . parentheses) (max_columns . 40)))
         (importmagic-mode 1)
         (importmagic-fix-symbol "Widget")
         (importmagic--test-result
          :buffer (buffer-substring-no-properties (point-min) (point-max))))))
  (importmagic--test-reset))"####,
        expect![[
            r#"OK (:buffer "from some.long.module.path import (\n    very_long_symbol_name_here)\nfrom widgets import Widget\n\n\nprint(Widget())\n" :messages ("[importmagic] Indexed ([ORACLE-SANDBOX]/importmagic-proj)" "[importmagic] Inserted (from widgets import Widget)") :reads ((:prompt "Querying for Widget: " :options ("from widgets import Widget"))) :calls "(call 22 add_path_to_index \"@@ROOT@@/importmagic-proj\")\n\n(call 23 get_candidates_for_symbol \"Widget\")\n\n(call 24 get_import_statement (\"from some.long.module.path import very_long_symbol_name_here\\12\\12\\12print(Widget())\\12\" \"from widgets import Widget\" ((multiline max_columns) (parentheses 40))))\n\n")"#
        ]],
    )
}

/// A symbol with no recorded candidates: the real error is signalled and
/// the buffer is left untouched.
fn a_symbol_with_no_candidates_signals_and_leaves_the_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_symbol_with_no_candidates_signals_and_leaves_the_buffer",
        r####"(unwind-protect
    (progn
      (importmagic--test-open "app.py" "frobnicate()\n")
      (with-current-buffer "app.py"
        (importmagic--test-with-ui-capture
         (importmagic-mode 1)
         (let ((no-candidate-error nil))
           (condition-case err
               (importmagic-fix-symbol "frobnicate")
             (error (setq no-candidate-error (list (car err) (cadr err)))))
           (importmagic--test-result
            :error no-candidate-error
            :buffer (buffer-substring-no-properties (point-min) (point-max)))))))
  (importmagic--test-reset))"####,
        expect![[
            r#"OK (:error (error "[importmagic] No suitable candidates found for frobnicate") :buffer "frobnicate()\n" :messages ("[importmagic] Indexed ([ORACLE-SANDBOX]/importmagic-proj)") :reads nil :calls "(call 27 add_path_to_index \"@@ROOT@@/importmagic-proj\")\n\n(call 28 get_candidates_for_symbol \"frobnicate\")\n\n")"#
        ]],
    )
}

/// `importmagic-fix-imports' reports the symbols it could not fix while
/// still fixing the resolvable ones in the same buffer.
fn fix_imports_reports_the_unresolvable_symbol_and_fixes_the_rest() -> ParityBatchCase {
    ParityBatchCase::value(
        "fix_imports_reports_the_unresolvable_symbol_and_fixes_the_rest",
        r####"(unwind-protect
    (progn
      (importmagic--test-open "app.py" "frobnicate()\nprint(Widget())\n")
      (with-current-buffer "app.py"
        (importmagic--test-with-ui-capture
         (importmagic-mode 1)
         (importmagic-fix-imports)
         (importmagic--test-result
          :buffer (buffer-substring-no-properties (point-min) (point-max))))))
  (importmagic--test-reset))"####,
        expect![[
            r#"OK (:buffer "from widgets import Widget\n\n\nfrobnicate()\nprint(Widget())\n" :messages ("[importmagic] Indexed ([ORACLE-SANDBOX]/importmagic-proj)" "[importmagic] Inserted (from widgets import Widget)" "[importmagic] Symbols with no candidates: ((frobnicate))") :reads ((:prompt "Querying for Widget: " :options ("from widgets import Widget"))) :calls "(call 31 add_path_to_index \"@@ROOT@@/importmagic-proj\")\n\n(call 32 get_unresolved_symbols \"frobnicate()\\12print(Widget())\\12\")\n\n(call 33 get_candidates_for_symbol \"frobnicate\")\n\n(call 34 get_candidates_for_symbol \"Widget\")\n\n(call 35 get_import_statement (\"frobnicate()\\12print(Widget())\\12\" \"from widgets import Widget\" ((multiline max_columns) (parentheses 79))))\n\n")"#
        ]],
    )
}

/// The documented graceful degradation: without the Python backend the
/// mode start fails, the user is told, and the mode turns itself off
/// with no server left behind.
fn a_missing_backend_disables_the_mode_with_a_message() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_missing_backend_disables_the_mode_with_a_message",
        r####"(unwind-protect
    (progn
      (importmagic--test-open "app.py" "print(Widget())\n")
      (with-current-buffer "app.py"
        (importmagic--test-with-ui-capture
         (setq importmagic-python-interpreter
               (expand-file-name "no-such-interpreter" importmagic--test-root))
         (importmagic-mode 1)
         (importmagic--test-result
          :mode importmagic-mode
          :server importmagic-server))))
  (importmagic--test-reset))"####,
        expect![[
            r#"OK (:mode nil :server nil :messages ("Importmagic and/or epc not found. importmagic.el will not be working.") :reads nil :calls "")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_mode_gate_rejects_non_python_buffers(),
        the_readme_os_example_is_fixed_end_to_end(),
        fix_symbol_at_point_imports_the_project_class(),
        fix_imports_merges_both_statements_into_one_block(),
        the_customized_style_reflows_the_long_import_line(),
        a_symbol_with_no_candidates_signals_and_leaves_the_buffer(),
        fix_imports_reports_the_unresolvable_symbol_and_fixes_the_rest(),
        a_missing_backend_disables_the_mode_with_a_message(),
    ]
}

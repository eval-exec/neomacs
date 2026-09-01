use expect_test::expect;

use super::ParityBatchCase;

/// A modeler opens a badly formatted Abs class, gets the Abs editing
/// environment set up by `auto-mode-alist', reindents the whole file with the
/// bundled "abs" cc-mode style, comments a line out with the Abs comment
/// syntax, and expands one of the snippets shipped inside the package.
fn visiting_an_abs_model_gives_the_abs_editing_environment() -> ParityBatchCase {
    ParityBatchCase::value(
        "visiting_an_abs_model_gives_the_abs_editing_environment",
        r##"(let ((buffer (abs-test-open "editing/counter.abs" abs-test-counter-model)))
  (unwind-protect
      (with-current-buffer buffer
        (let ((environment
               (list major-mode
                     mode-name
                     (and (derived-mode-p 'prog-mode) t)
                     c-buffer-is-cc-mode
                     (bound-and-true-p c-indentation-style)
                     c-basic-offset
                     comment-start
                     comment-end
                     comment-start-skip
                     indent-line-function
                     beginning-of-defun-function
                     end-of-defun-function
                     outline-regexp
                     (and (bound-and-true-p outline-minor-mode) t)
                     (and (bound-and-true-p flymake-mode) t)
                     (and (bound-and-true-p yas-minor-mode) t)
                     (key-binding (kbd "C-c C-c"))
                     (key-binding (kbd "TAB"))
                     (buffer-modified-p))))
          (indent-region (point-min) (point-max))
          (let ((indented (buffer-substring-no-properties (point-min) (point-max))))
            (goto-char (point-min))
            (forward-line 3)
            (comment-region (point) (line-beginning-position 2))
            (let ((commented (buffer-substring-no-properties
                              (line-beginning-position) (line-beginning-position 2))))
              (uncomment-region (line-beginning-position) (line-beginning-position 2))
              (goto-char (point-max))
              (insert "\ninterface")
              (let ((tab-binding (key-binding (kbd "TAB"))))
                (call-interactively tab-binding)
                (list environment
                      indented
                      commented
                      tab-binding
                      (buffer-substring-no-properties (point-min) (point-max))
                      (point)
                      (length (yas-active-snippets))
                      (and (member abs--yas-snippets-dir yas-snippet-dirs) t)))))))
    (kill-buffer buffer)))"##,
        expect![[
            r#"OK ((abs-mode "Abs//l" t abs-mode "abs" 4 "//" "" "//+\\s-*" c-indent-line abs-beginning-of-definition abs-end-of-definition "^\\(?:class\\|d\\(?:ata\\|e\\(?:f\\|lta\\)\\)\\|exception\\|\\(?:interfac\\|modul\\|typ\\)e\\)" t t t abs-next-action c-indent-line-or-region nil) "module Counter;\n[HTTPName: \"counter\"]\nclass Counter(Int start) implements Countable {\n    Int count = start;\n    Unit inc() {\n\11count = count + 1;\n\11if (count > 10) {\n\11    count = 0;\n\11}\n    }\n    Int classify(Int n) {\n\11case n {\n\11    0 => return 0;\n\11    _ => return 1;\n\11}\n    }\n}\n" "    // Int count = start;\n" yas-expand "module Counter;\n[HTTPName: \"counter\"]\nclass Counter(Int start) implements Countable {\n    Int count = start;\n    Unit inc() {\n\11count = count + 1;\n\11if (count > 10) {\n\11    count = 0;\n\11}\n    }\n    Int classify(Int n) {\n\11case n {\n\11    0 => return 0;\n\11    _ => return 1;\n\11}\n    }\n}\n\ninterface InterfaceName extends OtherInterface {\n    \n}\n" 289 1 t)"#
        ]],
    )
}

fn font_lock_separates_abs_keywords_types_functions_and_unicode_literals() -> ParityBatchCase {
    ParityBatchCase::value(
        "font_lock_separates_abs_keywords_types_functions_and_unicode_literals",
        r##"(let ((buffer (abs-test-open "editing/faces.abs" abs-test-bank-model)))
  (unwind-protect
      (with-current-buffer buffer
        (goto-char (point-min))
        (list (abs-test-face-runs (point-min) (line-beginning-position 12))
              (abs-test-face-runs (line-beginning-position 21)
                                  (line-beginning-position 23))
              (abs-test-face-runs (line-beginning-position 34)
                                  (line-beginning-position 39))
              (buffer-modified-p)))
    (kill-buffer buffer)))"##,
        expect![[
            r#"OK ((("module" . abs-keyword-face) ("Bank" . abs-type-face) ("export" . abs-keyword-face) ("Account" . abs-type-face) ("SavingsAccount" . abs-type-face) ("import" . abs-keyword-face) ("from" . abs-keyword-face) ("Util" . abs-type-face) ("// " . font-lock-comment-delimiter-face) ("Zinsen: 3 % pro Jahr — für Sparkonten\n" . font-lock-comment-face) ("data" . abs-keyword-face) ("AccountId" . abs-type-face) ("AccountId" . abs-type-face) ("String" . abs-type-face) ("label" . abs-variable-name-face) ("type" . abs-keyword-face) ("Balance" . abs-type-face) ("Rat" . abs-type-face) ("exception" . abs-keyword-face) ("InsufficientFunds" . abs-type-face) ("Rat" . abs-type-face) ("requested" . abs-variable-name-face) ("interface" . abs-keyword-face) ("Account" . abs-type-face)) (("println" . abs-function-name-face) ("\"Überweisung €50 ✓\"" . font-lock-string-face) ("return" . abs-keyword-face) ("balance" . abs-variable-name-face)) (("Nil" . abs-constant-face) ("Cons" . abs-constant-face) ("x" . abs-variable-name-face) ("rest" . abs-variable-name-face) ("x" . abs-variable-name-face) ("total" . abs-function-name-face) ("rest" . abs-variable-name-face) ("delta" . abs-keyword-face) ("DFee" . abs-type-face)) nil)"#
        ]],
    )
}

fn imenu_indexes_every_abs_declaration_kind_and_jumps_to_a_class() -> ParityBatchCase {
    ParityBatchCase::value(
        "imenu_indexes_every_abs_declaration_kind_and_jumps_to_a_class",
        r##"(let ((buffer (abs-test-open "editing/index.abs" abs-test-bank-model)))
  (unwind-protect
      (with-current-buffer buffer
        (let* ((index (imenu--make-index-alist))
               (classes (cdr (assoc "Classes" index)))
               (positions (abs-test-index-positions index)))
          (goto-char (point-max))
          (imenu (assoc "SavingsAccount" classes))
          (list positions
                imenu-syntax-alist
                (point)
                (buffer-substring-no-properties (point) (line-end-position))
                (marker-position (mark-marker))
                (buffer-modified-p))))
    (kill-buffer buffer)))"##,
        expect![[
            r#"OK ((("*Rescan*" . -99) ("Modules" ("Bank" . 1)) ("Interfaces" ("Account" . 216)) ("Classes" ("SavingsAccount" . 298)) ("Exceptions" ("InsufficientFunds" . 171)) ("Datatypes" ("AccountId" . 109) ("Balance" . 151)) ("Functions" ("total" . 683)) ("Deltas" ("DFee" . 805))) (("." . "_")) 298 "class SavingsAccount(Rat initial) implements Account {" 964 nil)"#
        ]],
    )
}

fn defun_motion_walks_definitions_and_ignores_comment_and_string_lookalikes() -> ParityBatchCase {
    ParityBatchCase::value(
        "defun_motion_walks_definitions_and_ignores_comment_and_string_lookalikes",
        r##"(let ((buffer (abs-test-open "editing/ledger.abs" abs-test-ledger-model)))
  (unwind-protect
      (with-current-buffer buffer
        (goto-char (point-max))
        (let ((backward nil))
          (dotimes (_ 5)
            (beginning-of-defun)
            (push (list (line-number-at-pos)
                        (point)
                        (buffer-substring-no-properties (point) (line-end-position)))
                  backward))
          (goto-char (point-min))
          (forward-line 3)
          (end-of-defun)
          (let ((after-interface (list (line-number-at-pos) (point)))
                (inside-class
                 (progn
                   (goto-char (point-min))
                   (forward-line 9)
                   (condition-case error
                       (progn (end-of-defun) :moved)
                     (error error)))))
            (list (nreverse backward)
                  after-interface
                  inside-class
                  (line-number-at-pos)
                  (point)
                  (buffer-modified-p)))))
    (kill-buffer buffer)))"##,
        expect![[
            r#"OK (((14 243 "def Int one() = 1;") (8 114 "class FileLedger implements Ledger {") (4 62 "interface Ledger {") (1 1 "module Nav;") (1 1 "module Nav;")) (7 113) (search-failed "[;}]") 10 218 nil)"#
        ]],
    )
}

fn c_c_c_c_compiles_a_multi_file_model_and_then_runs_it_on_the_erlang_backend() -> ParityBatchCase {
    ParityBatchCase::value(
        "c_c_c_c_compiles_a_multi_file_model_and_then_runs_it_on_the_erlang_backend",
        r##"(let ((buffer nil))
  (abs-test-setup-compiler)
  (abs-test-write "model/util.abs" abs-test-util-model)
  (setq buffer (abs-test-open "model/bank.abs" abs-test-bank-model))
  (unwind-protect
      (with-current-buffer buffer
        (let ((compilation-read-command nil)
              (compilation-ask-about-save nil)
              (abs-clock-limit 25)
              (abs-local-port 8080))
          (let* ((inputs (abs--input-files))
                 (compilation
                  (abs-test-compile
                   (lambda ()
                     (let ((current-prefix-arg nil))
                       (call-interactively (key-binding (kbd "C-c C-c")))))))
                 (compiled-files (abs-test-relative-files "model"))
                 (needs-compilation (abs--needs-compilation)))
            (let ((current-prefix-arg nil))
              (call-interactively (key-binding (kbd "C-c C-c"))))
            (let ((erlang-output (abs-test-wait-for-process "*erlang*")))
              (list inputs
                    compilation
                    (abs-test-compilation-text)
                    compiled-files
                    needs-compilation
                    erlang-output
                    (with-current-buffer "*erlang*" major-mode)
                    (buffer-name (window-buffer (selected-window)))
                    (abs-test-commands)
                    (with-current-buffer buffer
                      (list (buffer-modified-p) (buffer-file-name) default-directory)))))))
    (kill-buffer buffer)
    (dolist (name '("*compilation*" "*erlang*"))
      (when (get-buffer name)
        (let ((kill-buffer-query-functions nil))
          (kill-buffer name))))))"##,
        expect![[
            r#"OK (("bank.abs" "util.abs") ("*compilation*" "finished") "-*- mode: compilation; default-directory: \"[ORACLE-SANDBOX]/model/\" -*-\nCompilation started at <TIME>\n\nabsc --erlang \"bank.abs\" \"util.abs\" \nCompiled erlang model.\n\nCompilation finished at <TIME>\n" ("bank.abs" "gen/erl/absmodel/Emakefile" "gen/erl/run" "util.abs") nil "Bank.Main terminated.\n\nProcess inferior-erlang finished\n" erlang-shell-mode "*erlang*" ("absc --erlang bank.abs util.abs" "run -l 25 -p 8080") (nil "[ORACLE-SANDBOX]/model/bank.abs" "[ORACLE-SANDBOX]/model/"))"#
        ]],
    )
}

fn file_local_variables_drive_a_timed_maude_compilation_and_reject_unsafe_settings()
-> ParityBatchCase {
    ParityBatchCase::value(
        "file_local_variables_drive_a_timed_maude_compilation_and_reject_unsafe_settings",
        r##"(let ((buffer nil))
  (abs-test-setup-compiler)
  (abs-test-write "timed/helper.abs" abs-test-util-model)
  (setq buffer (let ((enable-local-variables :safe))
                 (abs-test-open "timed/timed.abs" abs-test-timed-model)))
  (unwind-protect
      (with-current-buffer buffer
        (let ((compilation-read-command nil)
              (compilation-ask-about-save nil))
          (list
           (list abs-backend
                 abs-clock-limit
                 abs-default-resourcecost
                 abs-input-files
                 abs-maude-output-file
                 abs-product-name
                 abs-compiler-program
                 abs-output-directory)
           (mapcar (lambda (variable) (and (local-variable-p variable) t))
                   '(abs-backend abs-clock-limit abs-default-resourcecost
                     abs-input-files abs-maude-output-file abs-product-name
                     abs-compiler-program abs-output-directory))
           (let (locals)
             (dolist (entry file-local-variables-alist (nreverse locals))
               (when (string-prefix-p "abs-" (symbol-name (car entry)))
                 (push entry locals))))
           (abs-test-compile
            (lambda ()
              (let ((current-prefix-arg nil))
                (call-interactively (key-binding (kbd "C-c C-c"))))))
           (abs-test-compilation-text)
           (abs-test-commands)
           (abs-test-relative-files "timed")
           (with-temp-buffer
             (insert-file-contents (abs-test-path "timed/timed.maude"))
             (buffer-string))
           (buffer-modified-p))))
    (kill-buffer buffer)
    (when (get-buffer "*compilation*")
      (kill-buffer "*compilation*"))))"##,
        expect![[
            r#"OK ((maude 42 7 #1=("timed.abs" "helper.abs") "timed.maude" "Deluxe" "absc" nil) (t t t t t t nil nil) ((abs-backend . maude) (abs-clock-limit . 42) (abs-default-resourcecost . 7) (abs-input-files . #1#) (abs-maude-output-file . "timed.maude") (abs-product-name . "Deluxe")) ("*compilation*" "finished") "-*- mode: compilation; default-directory: \"[ORACLE-SANDBOX]/timed/\" -*-\nCompilation started at <TIME>\n\nabsc --maude \"timed.abs\" \"helper.abs\" -o \"timed.maude\" --product Deluxe --timed --limit=42 --defaultcost 7 \nCompiled maude model.\n\nCompilation finished at <TIME>\n" ("absc --maude timed.abs helper.abs -o timed.maude --product Deluxe --timed --limit=42 --defaultcost 7") ("helper.abs" "timed.abs" "timed.maude") "load abs-interpreter .\n" nil)"#
        ]],
    )
    .fresh_process()
}

fn a_failing_compilation_reports_abs_diagnostics_that_next_error_can_visit() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_failing_compilation_reports_abs_diagnostics_that_next_error_can_visit",
        r##"(let ((buffer nil))
  (abs-test-setup-compiler)
  (abs-test-write "broken/util.abs" abs-test-util-model)
  (setq buffer (abs-test-open "broken/bank.abs" abs-test-bank-model))
  (setenv "ABS_COMPILER_ERROR"
          (concat "bank.abs:17:9: warning: unused variable initial\n"
                  "bank.abs:21:9: error: Cannot resolve name Uberweisung\n"
                  "util.abs:5:22: error: Type mismatch: expected Rat, found Int"))
  (unwind-protect
      (with-current-buffer buffer
        (let ((compilation-read-command nil)
              (compilation-ask-about-save nil)
              (visits nil))
          (let ((compilation
                 (abs-test-compile
                  (lambda ()
                    (let ((current-prefix-arg nil))
                      (call-interactively (key-binding (kbd "C-c C-c"))))))))
            (dotimes (_ 3)
              (next-error)
              (push (with-current-buffer (window-buffer (selected-window))
                      (list (buffer-name)
                            (line-number-at-pos)
                            (point)
                            (buffer-substring-no-properties
                             (line-beginning-position) (line-end-position))))
                    visits))
            (list compilation
                  (abs-test-compilation-text)
                  (nreverse visits)
                  (condition-case error (progn (next-error) :moved)
                    (error error))
                  (with-current-buffer "*compilation*"
                    (list compilation-num-errors-found
                          compilation-num-warnings-found
                          (line-number-at-pos)))
                  (abs-test-commands)
                  (abs-test-relative-files "broken")))))
    (setenv "ABS_COMPILER_ERROR" nil)
    (dolist (name '("*compilation*" "bank.abs" "util.abs"))
      (when (get-buffer name)
        (kill-buffer name)))))"##,
        expect![[
            r#"OK (("*compilation*" "exited abnormally with code 1") "-*- mode: compilation; default-directory: \"[ORACLE-SANDBOX]/broken/\" -*-\nCompilation started at <TIME>\n\nabsc --erlang \"bank.abs\" \"util.abs\" \nbank.abs:17:9: warning: unused variable initial\nbank.abs:21:9: error: Cannot resolve name Uberweisung\nutil.abs:5:22: error: Type mismatch: expected Rat, found Int\n\nCompilation exited abnormally with code 1 at <TIME>\n" (("bank.abs" 17 361 "    Rat balance = initial;") ("bank.abs" 21 455 "        println(\"Überweisung €50 ✓\");") ("util.abs" 5 55 "def Rat toBalance(Int cents) = cents / 100;")) (user-error "Past last error") (2 1 7) ("absc --erlang bank.abs util.abs") ("bank.abs" "util.abs"))"#
        ]],
    )
    .fresh_process()
}

fn flymake_checks_the_unsaved_abs_buffer_with_the_abs_compiler() -> ParityBatchCase {
    ParityBatchCase::value(
        "flymake_checks_the_unsaved_abs_buffer_with_the_abs_compiler",
        r##"(let ((buffer nil))
  (abs-test-setup-compiler)
  (abs-test-write "check/util.abs" abs-test-util-model)
  (setq buffer (abs-test-open "check/bank.abs" abs-test-bank-model))
  (setenv "ABS_COMPILER_ERROR"
          (concat "@SOURCE@:17:9: warning: unused variable initial\n"
                  "@SOURCE@:21:9: error: Cannot resolve name Uberweisung\n"
                  "util.abs:5:22: error: Type mismatch: expected Rat, found Int"))
  (unwind-protect
      (with-current-buffer buffer
        (setq-local create-lockfiles nil)
        (goto-char (point-min))
        (search-forward "balance = balance + amount;")
        (replace-match "balance = balance + amount + 1;")
        (list (and (bound-and-true-p flymake-mode) t)
              flymake-diagnostic-functions
              (abs-test-flymake-diagnostics)
              (mapcar #'abs-test-normalize-temp-names (abs-test-commands))
              (abs-test-relative-files "check")
              (flymake-disabled-backends)
              (buffer-modified-p)))
    (setenv "ABS_COMPILER_ERROR" nil)
    (set-buffer-modified-p nil)
    (kill-buffer buffer)))"##,
        expect![[
            r#"OK (t (flymake-proc-legacy-flymake) ((:warning 361 368 17 "balance" "warning: unused variable initial") (:error 459 466 21 "println" "error: Cannot resolve name Uberweisung")) ("absc [ORACLE-SANDBOX]/check/bank_<TEMP>_flymake.abs util.abs") ("bank.abs" "util.abs") nil t)"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        visiting_an_abs_model_gives_the_abs_editing_environment(),
        font_lock_separates_abs_keywords_types_functions_and_unicode_literals(),
        imenu_indexes_every_abs_declaration_kind_and_jumps_to_a_class(),
        defun_motion_walks_definitions_and_ignores_comment_and_string_lookalikes(),
        c_c_c_c_compiles_a_multi_file_model_and_then_runs_it_on_the_erlang_backend(),
        file_local_variables_drive_a_timed_maude_compilation_and_reject_unsafe_settings(),
        a_failing_compilation_reports_abs_diagnostics_that_next_error_can_visit(),
        flymake_checks_the_unsaved_abs_buffer_with_the_abs_compiler(),
    ]
}

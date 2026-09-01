use expect_test::expect;

use super::ParityBatchCase;

/// The elisp-symbol Capf: the obarray table, the annotation, and the
/// quote-skip bound behavior.
fn the_elisp_symbol_capf_completes_from_the_obarray() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_elisp_symbol_capf_completes_from_the_obarray",
        r####"(unwind-protect
    (progn
      (with-temp-buffer
        (emacs-lisp-mode)
        (insert "(with-tem")
        (let* ((capf (cape-elisp-symbol))
               (beg (nth 0 capf))
               (end (nth 1 capf))
               (table (nth 2 capf))
               (annotation (plist-get (nthcdr 3 capf) :annotation-function))
               (category (plist-get (nthcdr 3 capf) :category)))
          (goto-char (point-max))
          (insert "'sym")
          (let ((quoted (cape-elisp-symbol)))
            (list :source (cape--test-source-state)
                  :bounds (list beg end
                                (buffer-substring-no-properties beg end))
                  :category category
                  :completions
                  (seq-take (cape--test-completions table "with-tem") 6)
                  :annotation (funcall annotation "with-temp-buffer")
                  :quoted-bounds
                  (list (nth 0 quoted) (nth 1 quoted)
                        (buffer-substring-no-properties
                         (nth 0 quoted) (nth 1 quoted))))))))
  nil)"####,
        expect![[
            r#"OK (:source (:upstream-tree "5275a3af96874e280eb82814412ff6a7ce7ff5f9" :feature t :version "20260804.2303") :bounds (2 10 "with-tem") :category symbol :completions ("with-temp-buffer" "with-temp-buffer-window" "with-temp-file" "with-temp-message") :annotation " Macro" :quoted-bounds (11 14 "sym"))"#
        ]],
    )
}

/// The file Capf completes sandbox paths and annotates directories and
/// files differently.
fn the_file_capf_completes_sandbox_paths_with_annotations() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_file_capf_completes_sandbox_paths_with_annotations",
        r####"(unwind-protect
    (progn
      (cape--test-write (expand-file-name "aaa-directory/placeholder"
                                          cape--test-fixtures)
                        "")
      (cape--test-write (expand-file-name "aaa-file.txt" cape--test-fixtures)
                        "")
      (with-temp-buffer
        (insert "aaa")
        (goto-char (point-max))
        (let* ((cape-file-directory-must-exist nil) ; documented option
               (default-directory (directory-file-name
                                   cape--test-fixtures))
               (capf (cape-file))
               (beg (nth 0 capf))
               (end (nth 1 capf))
               (table (nth 2 capf))
               (annotation (plist-get (nthcdr 3 capf)
                                      :annotation-function))
               (exclusive (plist-get (nthcdr 3 capf) :exclusive)))
          (list :bounds (list beg end)
                :completions (cape--test-completions table "aaa")
                :dir-annotation
                (funcall annotation
                         (file-name-as-directory "aaa-directory"))
                :file-annotation (funcall annotation "aaa-file.txt")
                :exclusive exclusive))))
  nil)"####,
        expect![[
            r#"OK (:bounds (1 4) :completions (#("aaa-directory/" 0 1 (completion--unquoted "aaa-directory/" face completions-common-part) 1 3 (face completions-common-part)) #("aaa-file.txt" 0 1 (completion--unquoted "aaa-file.txt" face completions-common-part) 1 3 (face completions-common-part))) :dir-annotation " Dir" :file-annotation " File" :exclusive no)"#
        ]],
    )
}

/// The dabbrev Capf completes words from the same buffer.
fn the_dabbrev_capf_completes_words_from_the_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_dabbrev_capf_completes_words_from_the_buffer",
        r####"(unwind-protect
    (progn
      (with-temp-buffer
        (insert "fortune fortune-teller fortuitous\nfor")
        (goto-char (point-max))
        (let* ((capf (cape-dabbrev))
               (beg (nth 0 capf))
               (end (nth 1 capf))
               (table (nth 2 capf)))
          (list :bounds (list beg end
                              (buffer-substring-no-properties beg end))
                :completions (cape--test-completions table "for")))))
  nil)"####,
        expect![[
            r#"OK (:bounds (35 38 "for") :completions ("fortuitous" "fortune" "fortune-teller"))"#
        ]],
    )
}

/// The line Capf completes the current partial line from other lines.
fn the_line_capf_completes_whole_lines() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_line_capf_completes_whole_lines",
        r####"(unwind-protect
    (progn
      (with-temp-buffer
        (insert "hello world\nhello brave new world\nhel")
        (goto-char (point-max))
        (let* ((capf (cape-line))
               (beg (nth 0 capf))
               (end (nth 1 capf))
               (table (nth 2 capf)))
          (list :bounds (list beg end)
                :completions (cape--test-completions table "hel")))))
  nil)"####,
        expect![[r#"OK (:bounds (35 38) :completions ("hello brave new world" "hello world"))"#]],
    )
}

/// The abbrev Capf completes registered abbreviations.
fn the_abbrev_capf_completes_registered_abbreviations() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_abbrev_capf_completes_registered_abbreviations",
        r####"(unwind-protect
    (progn
      (with-temp-buffer
        (insert "gnu")
        (goto-char (point-max))
        (define-abbrev global-abbrev-table "gnu" "GNU's Not Unix")
        (let* ((capf (cape-abbrev))
               (beg (nth 0 capf))
               (end (nth 1 capf))
               (table (nth 2 capf))
               (annotation (plist-get (nthcdr 3 capf)
                                      :annotation-function)))
          (list :bounds (list beg end)
                :completions (cape--test-completions table "gnu")
                :annotation (funcall annotation "gnu")))))
  nil)"####,
        expect![[r#"OK (:bounds (1 4) :completions ("gnu") :annotation " GNU's Not Unix")"#]],
    )
}

/// The interactive entry point: the Capf table feeds completing-read and
/// the chosen completion is inserted by the exit function.
fn cape_interactive_runs_the_capf_and_inserts_the_choice() -> ParityBatchCase {
    ParityBatchCase::value(
        "cape_interactive_runs_the_capf_and_inserts_the_choice",
        r####"(unwind-protect
    (progn
      (with-temp-buffer
        (emacs-lisp-mode)
        (insert "(with-tem")
        (goto-char (point-max))
        (cape--test-with-ui-capture
         (cape-interactive #'cape-elisp-symbol)
         (list :source (cape--test-source-state)
               :buffer (buffer-substring-no-properties
                        (point-min) (point-max))
               :point (point)))))
  nil)"####,
        expect![[
            r#"OK (:source (:upstream-tree "5275a3af96874e280eb82814412ff6a7ce7ff5f9" :feature t :version "20260804.2303") :buffer "(with-temp-" :point 12)"#
        ]],
    )
}

/// The super wrapper preserves the wrapped Capf's annotation through the
/// super-property dispatch.
fn the_super_wrapper_preserves_the_annotation() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_super_wrapper_preserves_the_annotation",
        r####"(unwind-protect
    (progn
      (with-temp-buffer
        (emacs-lisp-mode)
        (insert "(with-tem")
        (goto-char (point-max))
        (let* ((wrapped (cape-capf-super #'cape-elisp-symbol))
               (capf (funcall wrapped))
               (beg (nth 0 capf))
               (end (nth 1 capf))
               (table (nth 2 capf))
               (annotation (plist-get (nthcdr 3 capf)
                                      :annotation-function)))
          (list :bounds (list beg end)
                :annotation (funcall annotation "with-temp-buffer")))))
  nil)"####,
        expect!["OK (:bounds (2 10) :annotation nil)"],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_elisp_symbol_capf_completes_from_the_obarray(),
        the_file_capf_completes_sandbox_paths_with_annotations(),
        the_dabbrev_capf_completes_words_from_the_buffer(),
        the_line_capf_completes_whole_lines(),
        the_abbrev_capf_completes_registered_abbreviations(),
        cape_interactive_runs_the_capf_and_inserts_the_choice(),
        the_super_wrapper_preserves_the_annotation(),
    ]
}

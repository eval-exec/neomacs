use crate::common;

use common::{run_neovm_eval, run_oracle_eval};

struct QuotingCase {
    name: &'static str,
    form: &'static str,
}

/// `text-quoting-style`'s nil case, compared against the pinned GNU.
///
/// GNU resolves nil in `default_to_grave_quoting_style` (`src/doc.c:638-650`)
/// from two inputs: `text_quoting_flag` (Lisp's
/// `internal--text-quoting-flag`, set from the process locale in `main`,
/// `src/emacs.c:1653-1656`, and forced to t by interactive startup,
/// `lisp/startup.el:1619-1620`) and the standard display table's U+2018 entry.
/// The oracle harness pins GNU to its UTF-8 snapshot locale
/// (`common::run_oracle_eval`), and the port's own default is the same UTF-8
/// answer, so these cases exercise both inputs and every branch:
///
///   * flag nil                -> grave, display table never consulted
///   * flag t, no table        -> curve
///   * flag t, table entry [?`] -> grave (the only substitution doc.c accepts)
///   * flag t, any other entry -> curve (including the homoglyph glyph code
///                                `startup--setup-quote-display` installs,
///                                `lisp/startup.el:1026-1029`)
///   * a char-table that is not a display table (purpose or extra-slot count
///     wrong, `src/disptab.h:26-29`) -> curve
///   * a parent display table's entry is reached through `char_table_ref`
///     (`src/xdisp.c:18425-18432`)
#[test]
fn compat_text_quoting_semantics_matches_gnu_emacs() {
    let cases = [
        QuotingCase {
            name: "nil_default_follows_the_utf8_locale",
            form: r#"(list (text-quoting-style) (format-message "`x'"))"#,
        },
        QuotingCase {
            name: "nil_style_with_flag_nil_is_grave",
            form: r#"(let ((internal--text-quoting-flag nil))
  (list (text-quoting-style)
        (format-message "`x'")
        (condition-case err (error "`x'") (error (error-message-string err)))))"#,
        },
        QuotingCase {
            name: "nil_style_with_flag_nil_ignores_a_backtick_display_table",
            form: r#"(let ((internal--text-quoting-flag nil)
       (standard-display-table (make-display-table)))
  (aset standard-display-table #x2018 [?`])
  (list (text-quoting-style) (format-message "`x'")))"#,
        },
        QuotingCase {
            name: "explicit_styles_pass_through",
            form: r#"(list
  (let ((text-quoting-style 'grave)) (list (text-quoting-style) (format-message "`x'")))
  (let ((text-quoting-style 'straight)) (list (text-quoting-style) (format-message "`x'")))
  (let ((text-quoting-style 'curve)) (list (text-quoting-style) (format-message "`x'")))
  (let ((text-quoting-style 'unknown-style)) (list (text-quoting-style) (format-message "`x'"))))"#,
        },
        QuotingCase {
            name: "display_table_backtick_vector_is_grave",
            form: r#"(let ((internal--text-quoting-flag t)
       (standard-display-table (make-display-table)))
  (aset standard-display-table #x2018 [?`])
  (list (text-quoting-style)
        (format-message "`x'")
        (condition-case err (error "`x'") (error (error-message-string err)))))"#,
        },
        QuotingCase {
            name: "display_table_standard_display_ascii_is_grave",
            form: r#"(let ((internal--text-quoting-flag t))
  (setq standard-display-table (make-display-table))
  (standard-display-ascii #x2018 "`")
  (list (text-quoting-style) (aref standard-display-table #x2018)))"#,
        },
        QuotingCase {
            name: "display_table_homoglyph_glyph_is_not_grave",
            // The face NAME and character are printed, not the raw glyph
            // code: face ids are per-process numbers, so the integer is not
            // an oracle question (the face-list lookup is).
            form: r#"(let ((internal--text-quoting-flag t)
       (standard-display-table (make-display-table)))
  (aset standard-display-table #x2018 (vector (make-glyph-code ?` 'homoglyph)))
  (let ((glyph (aref (aref standard-display-table #x2018) 0)))
    (list (text-quoting-style)
          (glyph-char glyph)
          (glyph-face glyph))))"#,
        },
        QuotingCase {
            name: "display_table_other_and_multi_element_vectors_are_not_grave",
            form: r#"(let ((internal--text-quoting-flag t))
  (list
   (let ((standard-display-table (make-display-table)))
     (aset standard-display-table #x2018 [?<])
     (text-quoting-style))
   (let ((standard-display-table (make-display-table)))
     (aset standard-display-table #x2018 [?` ?`])
     (text-quoting-style))
   (let ((standard-display-table (make-display-table)))
     (aset standard-display-table #x2018 "`")
     (text-quoting-style))))"#,
        },
        QuotingCase {
            name: "non_display_char_tables_are_ignored",
            form: r#"(let ((internal--text-quoting-flag t))
  (list
   (let ((standard-display-table (make-char-table 'foo-table nil)))
     (aset standard-display-table #x2018 [?`])
     (text-quoting-style))
   (let ((standard-display-table (make-char-table 'display-table nil)))
     (aset standard-display-table #x2018 [?`])
     (list (text-quoting-style)
           (char-table-subtype standard-display-table)))))"#,
        },
        QuotingCase {
            name: "display_table_parent_entry_is_reached",
            form: r#"(let ((parent (make-display-table))
       (child (make-display-table)))
  (aset parent #x2018 [?`])
  (set-char-table-parent child parent)
  (let ((internal--text-quoting-flag t)
        (standard-display-table child))
    (text-quoting-style)))"#,
        },
    ];

    for case in cases {
        let gnu = run_oracle_eval(case.form).expect("GNU Emacs evaluation");
        let neovm = run_neovm_eval(case.form).expect("NeoVM evaluation");
        assert_eq!(
            neovm, gnu,
            "text quoting semantics mismatch for {}:\nGNU: {}\nNeoVM: {}",
            case.name, gnu, neovm
        );
    }
}

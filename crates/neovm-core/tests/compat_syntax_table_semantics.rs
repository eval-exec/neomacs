mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

struct SyntaxCase {
    name: &'static str,
    form: &'static str,
}

#[test]
fn compat_syntax_table_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping syntax-table audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let cases = [
        SyntaxCase {
            name: "syntax_after_uses_syntax_table_text_properties",
            form: r#"(with-temp-buffer
  (insert "ab")
  (put-text-property 2 3 'syntax-table (string-to-syntax " "))
  (list
   (equal (syntax-after 2) (string-to-syntax " "))
   (char-syntax (char-after 2))))"#,
        },
        SyntaxCase {
            name: "scan_sexps_honors_parse_sexp_lookup_properties",
            form: r#"(with-temp-buffer
  (insert "x@y@z")
  (put-text-property 2 3 'syntax-table (string-to-syntax "|"))
  (put-text-property 4 5 'syntax-table (string-to-syntax "|"))
  (list
   (let ((parse-sexp-lookup-properties nil))
     (condition-case err
         (scan-sexps 2 1)
       (error (list 'error (car err)))))
   (let ((parse-sexp-lookup-properties t))
     (condition-case err
         (scan-sexps 2 1)
       (error (list 'error (car err)))))))"#,
        },
        SyntaxCase {
            name: "forward_comment_honors_syntax_table_text_properties",
            form: r##"(with-temp-buffer
  (insert "#hi#x")
  (put-text-property 1 2 'syntax-table (string-to-syntax "!"))
  (put-text-property 4 5 'syntax-table (string-to-syntax "!"))
  (list
   (let ((parse-sexp-lookup-properties nil))
     (goto-char 1)
     (list (forward-comment 1) (point)))
   (let ((parse-sexp-lookup-properties t))
     (goto-char 1)
     (list (forward-comment 1) (point)))))"##,
        },
        SyntaxCase {
            name: "backward_prefix_chars_uses_prefix_class_and_properties",
            form: r#"(list
  (with-temp-buffer
    (insert "'x")
    (goto-char 2)
    (backward-prefix-chars)
    (point))
  (with-temp-buffer
    (insert "+x")
    (put-text-property 1 2 'syntax-table (string-to-syntax "'"))
    (list
     (let ((parse-sexp-lookup-properties nil))
       (goto-char 2)
       (backward-prefix-chars)
       (point))
     (let ((parse-sexp-lookup-properties t))
       (goto-char 2)
       (backward-prefix-chars)
       (point)))))"#,
        },
        SyntaxCase {
            name: "regexp_syntax_class_search_prepares_syntax_properties",
            form: r#"(with-temp-buffer
  (setq-local syntax-propertize-function
              (syntax-propertize-rules ("x" (0 (ignore)))))
  (setq-local parse-sexp-lookup-properties t)
  (insert "a b\n")
  (goto-char (point-min))
  (list (re-search-forward "\\s-" nil t)
        syntax-propertize--done))"#,
        },
        SyntaxCase {
            name: "regexp_syntax_class_search_reads_syntax_table_text_properties",
            form: r#"(with-temp-buffer
  (setq-local parse-sexp-lookup-properties t)
  (insert "x")
  (put-text-property 1 2 'syntax-table (string-to-syntax " "))
  (goto-char (point-min))
  (list (re-search-forward "\\s-" nil t)
        (point)))"#,
        },
        SyntaxCase {
            name: "regexp_syntax_class_search_ignores_properties_when_lookup_is_disabled",
            form: r#"(with-temp-buffer
  (setq-local parse-sexp-lookup-properties nil)
  (insert "x")
  (put-text-property 1 2 'syntax-table (string-to-syntax " "))
  (goto-char (point-min))
  (list (re-search-forward "\\s-" nil t)
        (point)))"#,
        },
        SyntaxCase {
            name: "syntax_independent_regexp_does_not_trigger_propertization",
            form: r#"(with-temp-buffer
  (setq-local syntax-propertize-function
              (syntax-propertize-rules ("x" (0 (ignore)))))
  (setq-local parse-sexp-lookup-properties t)
  (insert "abc")
  (goto-char (point-min))
  (list (re-search-forward "a" nil t)
        syntax-propertize--done))"#,
        },
        SyntaxCase {
            name: "looking_at_prepares_and_reads_syntax_table_properties",
            form: r#"(with-temp-buffer
  (setq-local syntax-propertize-function
              (syntax-propertize-rules ("x" (0 " "))))
  (setq-local parse-sexp-lookup-properties t)
  (insert "x")
  (goto-char (point-min))
  (list (looking-at "\\s-")
        syntax-propertize--done))"#,
        },
        SyntaxCase {
            name: "regexp_syntax_preparation_preserves_later_string_match_data",
            form: r#"(with-temp-buffer
  (setq-local syntax-propertize-function
              (syntax-propertize-rules ("x" (0 (ignore)))))
  (setq-local parse-sexp-lookup-properties t)
  (insert "a b c d e f g h i j k l m n o p q r s t u v w x y z\n")
  (goto-char (point-min))
  (re-search-forward "\\s-" nil t)
  (let ((subject "'zeta'"))
    (string-match "'[^']+'" subject 0)
    (syntax-ppss)
    (match-string 0 subject)))"#,
        },
    ];

    for case in cases {
        eprintln!("syntax-table case: {}", case.name);
        let gnu = run_oracle_eval(case.form).expect("GNU Emacs evaluation");
        let neovm = run_neovm_eval(case.form).expect("NeoVM evaluation");
        assert_eq!(
            neovm, gnu,
            "syntax-table semantics mismatch for {}:\nGNU: {}\nNeoVM: {}",
            case.name, gnu, neovm
        );
    }
}

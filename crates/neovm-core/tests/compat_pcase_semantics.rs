mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

struct PcaseCase {
    name: &'static str,
    form: &'static str,
}

#[test]
fn compat_pcase_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping pcase semantics audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let cases = [
        PcaseCase {
            name: "pcase_surface_shape",
            form: r#"(let ((symbols '(pcase
                         pcase-defmacro
                         pcase--make-docstring
                         pcase-exhaustive
                         pcase-lambda
                         pcase-let
                         pcase-let*
                         pcase-dolist
                         pcase-setq)))
  (mapcar
   (lambda (sym)
     (let ((fn (symbol-function sym)))
       (list sym
             (fboundp sym)
             (macrop sym)
             (special-form-p sym)
             (autoloadp fn)
             (subrp fn)
             (car-safe fn))))
   symbols))"#,
        },
        PcaseCase {
            name: "pcase_feature_surface",
            form: r#"(featurep 'pcase)"#,
        },
        PcaseCase {
            name: "gensym_counter_runtime_surface",
            form: r#"(symbol-value 'gensym-counter)"#,
        },
        PcaseCase {
            name: "pcase_backquote_pattern_macroexpand",
            form: r#"(prin1-to-string
 (macroexpand
  '(pcase '(cond (t 1))
     (`(cond . ,clauses) clauses)
     (_ 'no-match))))"#,
        },
        PcaseCase {
            name: "pcase_backquote_pattern_eval",
            form: r#"(pcase '(cond (t 1))
  (`(cond . ,clauses) clauses)
  (_ 'no-match))"#,
        },
        PcaseCase {
            name: "pcase_let_star_destructuring",
            form: r#"(pcase-let* ((`(,a ,b . ,rest) '(1 2 3 4))
               (`[,c ,d] [5 6]))
  (list a b rest c d))"#,
        },
        PcaseCase {
            name: "pcase_dolist_destructuring",
            form: r#"(let (out)
  (pcase-dolist (`(,a ,b) '((1 2) (3 4) (5 6)))
    (push (+ a b) out))
  (nreverse out))"#,
        },
        PcaseCase {
            name: "macroexpand_all_with_pcase",
            form: r#"(prin1-to-string
 (macroexpand-all
  '(lambda (x)
     (pcase-let ((`(,a ,b) x))
       (pcase x
         (`(,lhs ,rhs) (+ lhs rhs a b))
         (_ nil))))))"#,
        },
    ];

    for case in cases {
        let gnu = run_oracle_eval(case.form)
            .unwrap_or_else(|err| panic!("GNU Emacs evaluation failed for {}: {err}", case.name));
        let neovm = run_neovm_eval(case.form)
            .unwrap_or_else(|err| panic!("NeoVM evaluation failed for {}: {err}", case.name));
        assert_eq!(
            neovm, gnu,
            "pcase mismatch for {}:\nGNU: {}\nNeoVM: {}",
            case.name, gnu, neovm
        );
    }
}

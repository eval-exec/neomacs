mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

struct HistoryCase {
    name: &'static str,
    form: &'static str,
}

#[test]
fn compat_window_history_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping window history semantics audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let cases = [
        HistoryCase {
            name: "window_use_time_matches_gnu_selection_history",
            form: r#"(save-window-excursion
  (delete-other-windows)
  (let* ((w1 (selected-window))
         (w2 (split-window nil nil 'right)))
    (list
     (window-use-time w1)
     (window-use-time w2)
     (window-bump-use-time w2)
     (window-use-time w1)
     (window-use-time w2)
     (window-bump-use-time w1))))"#,
        },
        HistoryCase {
            name: "split_window_starts_new_history_fresh",
            form: r#"(save-window-excursion
  (delete-other-windows)
  (let* ((w1 (selected-window))
         (_ (set-window-prev-buffers w1 '((foo 1 2))))
         (_ (set-window-next-buffers w1 '(foo bar)))
         (w2 (split-window nil nil 'right)))
    (list
     (window-prev-buffers w1)
     (window-next-buffers w1)
     (window-use-time w1)
     (window-prev-buffers w2)
     (window-next-buffers w2)
     (window-use-time w2))))"#,
        },
    ];

    for case in cases {
        let expected = run_oracle_eval(case.form)
            .unwrap_or_else(|err| panic!("oracle failed for {}: {err}", case.name));
        let actual = run_neovm_eval(case.form)
            .unwrap_or_else(|err| panic!("neovm failed for {}: {err}", case.name));
        assert_eq!(
            actual, expected,
            "history semantics mismatch for {}\nexpected: {}\nactual: {}",
            case.name, expected, actual
        );
    }
}

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use common::{gnu_window_c_path, oracle_enabled, run_neovm_eval, run_oracle_eval};
use native_regex::Regex as NativeRegex;

fn parse_gnu_window_defuns(path: &Path) -> BTreeSet<String> {
    let source = fs::read_to_string(path).expect("read GNU window.c");
    let re = NativeRegex::new(r#"DEFUN \("([^"]+)","#).expect("window.c DEFUN regex");
    re.captures_iter(&source)
        .map(|caps| caps[1].to_string())
        .collect()
}

fn symbol_list_literal(names: &BTreeSet<String>) -> String {
    let body = names.iter().cloned().collect::<Vec<_>>().join(" ");
    format!("'({body})")
}

#[test]
fn compat_window_surface_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping window surface audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let Some(gnu_window_c) = gnu_window_c_path() else {
        eprintln!("skipping window surface audit: GNU window.c not found");
        return;
    };

    let gnu_window_defuns = parse_gnu_window_defuns(&gnu_window_c);
    let symbol_list = symbol_list_literal(&gnu_window_defuns);
    let form = format!(
        r#"(mapcar
             (lambda (name)
               (let ((function (symbol-function name)))
                 (list name (subrp function)
                       (and (subrp function) (subr-arity function)))))
             {symbol_list})"#
    );

    let gnu = run_oracle_eval(&form).expect("GNU Emacs window surface evaluation");
    let neovm = run_neovm_eval(&form).expect("NeoVM window surface evaluation");
    assert_eq!(
        neovm, gnu,
        "window subr surface mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}

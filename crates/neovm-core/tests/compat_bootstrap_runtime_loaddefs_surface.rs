mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use common::{oracle_enabled, repo_root, run_neovm_eval, run_oracle_eval};

fn collect_autoload_symbols(
    path: &Path,
    allowed_files: Option<&BTreeSet<&str>>,
) -> BTreeSet<String> {
    let source = fs::read_to_string(path).expect("read loaddefs source");
    let mut symbols = BTreeSet::new();

    for line in source.lines() {
        let Some(rest) = line.strip_prefix("(autoload '") else {
            continue;
        };
        let Some((name, rest)) = rest.split_once(' ') else {
            continue;
        };
        let Some(start) = rest.find('"') else {
            continue;
        };
        let rest = &rest[start + 1..];
        let Some(end) = rest.find('"') else {
            continue;
        };
        let file = &rest[..end];
        if let Some(allowed) = allowed_files
            && !allowed.contains(file)
        {
            continue;
        }
        symbols.insert(name.to_string());
    }

    symbols
}

fn lisp_symbol_list(symbols: &BTreeSet<String>) -> String {
    let body = symbols.iter().cloned().collect::<Vec<_>>().join(" ");
    format!("({body})")
}

#[test]
fn compat_bootstrap_runtime_generated_loaddefs_surface_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping generated loaddefs runtime audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let root = repo_root();
    let mut symbols = collect_autoload_symbols(&root.join("lisp/emacs-lisp/cl-loaddefs.el"), None);
    let runtime_files = BTreeSet::from(["gv", "icons", "pcase"]);
    symbols.extend(collect_autoload_symbols(
        &root.join("lisp/ldefs-boot.el"),
        Some(&runtime_files),
    ));

    let form = format!(
        r#"(let ((symbols '{}))
  (mapcar
   (lambda (sym)
     (list sym
           (fboundp sym)
           (autoloadp (symbol-function sym))
           (get sym 'autoload-macro)))
   symbols))"#,
        lisp_symbol_list(&symbols)
    );

    let gnu = run_oracle_eval(&form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(&form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "generated loaddefs runtime surface mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};
use native_regex::Regex as NativeRegex;

fn gnu_xfaces_c_path() -> Option<std::path::PathBuf> {
    let mut dir = std::path::PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    for _ in 0..5 {
        let candidate = dir.join("emacs-mirror/emacs/src/xfaces.c");
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

fn parse_gnu_xfaces_defuns(path: &Path) -> BTreeSet<String> {
    let source = fs::read_to_string(path).expect("read GNU xfaces.c");
    let re = NativeRegex::new(r#"DEFUN \("([^"]+)","#).expect("xfaces.c DEFUN regex");
    re.captures_iter(&source)
        .map(|caps| caps[1].to_string())
        .collect()
}

fn symbol_list_literal(names: &BTreeSet<String>) -> String {
    let body = names.iter().cloned().collect::<Vec<_>>().join(" ");
    format!("'({body})")
}

fn parse_symbol_list(output: &str) -> BTreeSet<String> {
    let payload = output.strip_prefix("OK ").unwrap_or(output);
    let re = NativeRegex::new(r#"([A-Za-z0-9!$%&*+\-./:<=>?@^_~]+)"#).expect("symbol regex");
    re.captures_iter(payload)
        .map(|caps| caps[1].to_string())
        .collect()
}

#[test]
fn compat_face_surface_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping face surface audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let Some(gnu_xfaces_c) = gnu_xfaces_c_path() else {
        eprintln!("skipping face surface audit: GNU xfaces.c not found");
        return;
    };

    let xfaces_defuns = parse_gnu_xfaces_defuns(&gnu_xfaces_c);
    let source_name_list = symbol_list_literal(&xfaces_defuns);
    let exported_form = format!(
        r#"(delq nil (mapcar (lambda (name) (and (fboundp name) name)) {source_name_list}))"#
    );
    let exported = parse_symbol_list(
        &run_oracle_eval(&exported_form).expect("GNU Emacs xfaces surface evaluation"),
    );

    let exported_list = symbol_list_literal(&exported);
    let form = format!(
        r#"(mapcar
             (lambda (name)
               (let ((function (symbol-function name)))
                 (list name (subrp function)
                       (and (subrp function) (subr-arity function)))))
             {exported_list})"#
    );

    let gnu = run_oracle_eval(&form).expect("GNU Emacs face surface evaluation");
    let neovm = run_neovm_eval(&form).expect("NeoVM face surface evaluation");
    assert_eq!(
        neovm, gnu,
        "face subr surface mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}

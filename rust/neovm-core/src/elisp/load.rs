//! File loading and module system (require/provide/load).

use super::error::EvalError;
use super::expr::Expr;
use super::value::Value;
use std::fs;
use std::path::{Path, PathBuf};

fn has_load_suffix(name: &str) -> bool {
    name.ends_with(".el") || name.ends_with(".elc")
}

fn suffixed_paths(base: &Path) -> (PathBuf, PathBuf) {
    let base_str = base.to_string_lossy();
    (
        PathBuf::from(format!("{base_str}.elc")),
        PathBuf::from(format!("{base_str}.el")),
    )
}

fn source_is_newer(source: &Path, compiled: &Path) -> bool {
    let source_mtime = fs::metadata(source).and_then(|m| m.modified());
    let compiled_mtime = fs::metadata(compiled).and_then(|m| m.modified());
    match (source_mtime, compiled_mtime) {
        (Ok(s), Ok(c)) => s > c,
        _ => false,
    }
}

fn pick_suffixed(base: &Path, prefer_newer: bool) -> Option<PathBuf> {
    let (elc, el) = suffixed_paths(base);
    let has_elc = elc.exists();
    let has_el = el.exists();

    if prefer_newer && has_el && has_elc && source_is_newer(&el, &elc) {
        return Some(el);
    }
    if has_elc {
        return Some(elc);
    }
    if has_el {
        return Some(el);
    }
    None
}

fn find_for_base(
    base: &Path,
    original_name: &str,
    no_suffix: bool,
    must_suffix: bool,
    prefer_newer: bool,
) -> Option<PathBuf> {
    if no_suffix || has_load_suffix(original_name) {
        if base.exists() {
            return Some(base.to_path_buf());
        }
        return None;
    }

    if let Some(suffixed) = pick_suffixed(base, prefer_newer) {
        return Some(suffixed);
    }

    if !must_suffix && base.exists() {
        return Some(base.to_path_buf());
    }

    None
}

/// Search for a file in the load path.
pub fn find_file_in_load_path(name: &str, load_path: &[String]) -> Option<PathBuf> {
    find_file_in_load_path_with_flags(name, load_path, false, false, false)
}

/// Search for a file in load-path with `load` optional suffix flags.
///
/// Behavior follows Emacs:
/// - `no_suffix`: load only the exact filename.
/// - `must_suffix`: require a suffixed file when FILE has no suffix.
/// - `prefer_newer`: choose source `.el` over `.elc` when source is newer.
/// - default: search each load-path directory in order, preferring suffixed
///   files within each directory before bare names.
pub fn find_file_in_load_path_with_flags(
    name: &str,
    load_path: &[String],
    no_suffix: bool,
    must_suffix: bool,
    prefer_newer: bool,
) -> Option<PathBuf> {
    let path = Path::new(name);
    if path.is_absolute() {
        return find_for_base(path, name, no_suffix, must_suffix, prefer_newer);
    }

    // Emacs searches load-path directory-by-directory; suffix preference
    // is evaluated within each directory.
    for dir in load_path {
        let full = Path::new(dir).join(name);
        if let Some(found) = find_for_base(&full, name, no_suffix, must_suffix, prefer_newer) {
            return Some(found);
        }
    }

    None
}

/// Extract `load-path` from the evaluator's obarray as a Vec<String>.
pub fn get_load_path(obarray: &super::symbol::Obarray) -> Vec<String> {
    let default_directory = obarray
        .symbol_value("default-directory")
        .and_then(|v| v.as_str())
        .unwrap_or(".");

    let val = obarray
        .symbol_value("load-path")
        .cloned()
        .unwrap_or(Value::Nil);
    super::value::list_to_vec(&val)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| match v {
            Value::Nil => Some(default_directory.to_string()),
            _ => v.as_str().map(|s| s.to_string()),
        })
        .collect()
}

/// Load and evaluate a file. Returns the last result.
pub fn load_file(eval: &mut super::eval::Evaluator, path: &Path) -> Result<Value, EvalError> {
    let content = std::fs::read_to_string(path).map_err(|e| EvalError::Signal {
        symbol: "file-error".to_string(),
        data: vec![Value::string(format!(
            "Cannot read file: {}: {}",
            path.display(),
            e
        ))],
    })?;

    // Save dynamic loader context and restore it even on parse/eval errors.
    let old_lexical = eval.lexical_binding();
    let old_load_file = eval.obarray().symbol_value("load-file-name").cloned();

    // Check for lexical-binding file variable in file-local line.
    let first_line: &str = content.lines().next().unwrap_or("");
    if first_line.contains("lexical-binding: t") {
        eval.set_lexical_binding(true);
    }

    eval.set_variable(
        "load-file-name",
        Value::string(path.to_string_lossy().to_string()),
    );

    let result = (|| -> Result<Value, EvalError> {
        let mut forms = match super::parser::parse_forms(&content) {
            Ok(forms) => forms,
            Err(e) => {
                // Temporary compatibility path: when we cannot read an .elc file,
                // transparently try sibling source until native .elc execution lands.
                if let Some(source_path) = source_sibling_for_elc(path) {
                    return load_file(eval, &source_path);
                }
                return Err(EvalError::Signal {
                    symbol: "invalid-read-syntax".to_string(),
                    data: vec![Value::string(format!(
                        "Parse error in {}: {:?}",
                        path.display(),
                        e
                    ))],
                });
            }
        };

        // Until native `.elc` execution is implemented, prefer source if this
        // file contains compiled-function literals and a sibling `.el` exists.
        if forms_contain_compiled_literals(&forms) {
            if let Some(source_path) = source_sibling_for_elc(path) {
                return load_file(eval, &source_path);
            }
        }
        if path.extension().and_then(|s| s.to_str()) == Some("elc") {
            rewrite_compiled_literal_source_markers(
                &mut forms,
                path.to_string_lossy().as_ref(),
            );
        }

        for form in forms.iter() {
            if let Err(err) = eval.eval_expr(form) {
                if let Some(source_path) = source_sibling_for_elc(path) {
                    return load_file(eval, &source_path);
                }
                return Err(err);
            }
        }

        record_load_history(eval, path);

        // Emacs `load` returns non-nil on success (typically `t`).
        Ok(Value::True)
    })();

    eval.set_lexical_binding(old_lexical);
    if let Some(old) = old_load_file {
        eval.set_variable("load-file-name", old);
    } else {
        eval.set_variable("load-file-name", Value::Nil);
    }

    result
}

fn source_sibling_for_elc(path: &Path) -> Option<PathBuf> {
    if path.extension().and_then(|s| s.to_str()) != Some("elc") {
        return None;
    }
    let source = path.with_extension("el");
    if source.exists() {
        Some(source)
    } else {
        None
    }
}

fn record_load_history(eval: &mut super::eval::Evaluator, path: &Path) {
    let path_str = path.to_string_lossy().to_string();
    let entry = Value::cons(Value::string(path_str), Value::Nil);
    let history = eval
        .obarray()
        .symbol_value("load-history")
        .cloned()
        .unwrap_or(Value::Nil);
    eval.set_variable("load-history", Value::cons(entry, history));
}

fn forms_contain_compiled_literals(forms: &[Expr]) -> bool {
    forms.iter().any(expr_contains_compiled_literal)
}

fn expr_contains_compiled_literal(expr: &Expr) -> bool {
    match expr {
        Expr::List(items) => {
            if is_compiled_literal_quote(items) {
                return true;
            }
            items.iter().any(expr_contains_compiled_literal)
        }
        Expr::Vector(items) => items.iter().any(expr_contains_compiled_literal),
        Expr::DottedList(items, last) => {
            items.iter().any(expr_contains_compiled_literal) || expr_contains_compiled_literal(last)
        }
        _ => false,
    }
}

fn is_compiled_literal_quote(items: &[Expr]) -> bool {
    if items.len() != 2 {
        return false;
    }
    if !matches!(&items[0], Expr::Symbol(s) if s == "quote") {
        return false;
    }
    let Expr::Vector(vector_items) = &items[1] else {
        return false;
    };
    matches!(
        (
            vector_items.first(),
            vector_items.get(1),
            vector_items.get(2),
            vector_items.get(3),
        ),
        (
            Some(Expr::List(_)),
            Some(Expr::Str(_)),
            Some(Expr::Vector(_)),
            Some(Expr::Int(_)),
        )
    )
}

fn rewrite_compiled_literal_source_markers(forms: &mut [Expr], load_file_name: &str) {
    for form in forms.iter_mut() {
        rewrite_compiled_literal_source_markers_in_expr(form, load_file_name);
    }
}

fn rewrite_compiled_literal_source_markers_in_expr(expr: &mut Expr, load_file_name: &str) {
    match expr {
        Expr::List(items) => {
            if is_compiled_literal_quote(items) {
                if let Expr::Vector(vector_items) = &mut items[1] {
                    for item in vector_items.iter_mut() {
                        replace_load_file_name_symbol(item, load_file_name);
                    }
                }
                return;
            }
            for item in items.iter_mut() {
                rewrite_compiled_literal_source_markers_in_expr(item, load_file_name);
            }
        }
        Expr::Vector(items) => {
            for item in items.iter_mut() {
                rewrite_compiled_literal_source_markers_in_expr(item, load_file_name);
            }
        }
        Expr::DottedList(items, last) => {
            for item in items.iter_mut() {
                rewrite_compiled_literal_source_markers_in_expr(item, load_file_name);
            }
            rewrite_compiled_literal_source_markers_in_expr(last, load_file_name);
        }
        _ => {}
    }
}

fn replace_load_file_name_symbol(expr: &mut Expr, load_file_name: &str) {
    match expr {
        Expr::Symbol(name) if name == "load-file-name" => {
            *expr = Expr::Str(load_file_name.to_string());
        }
        Expr::List(items) => {
            for item in items.iter_mut() {
                replace_load_file_name_symbol(item, load_file_name);
            }
        }
        Expr::Vector(items) => {
            for item in items.iter_mut() {
                replace_load_file_name_symbol(item, load_file_name);
            }
        }
        Expr::DottedList(items, last) => {
            for item in items.iter_mut() {
                replace_load_file_name_symbol(item, load_file_name);
            }
            replace_load_file_name_symbol(last, load_file_name);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn find_file_nonexistent() {
        assert!(find_file_in_load_path("nonexistent", &[]).is_none());
    }

    #[test]
    fn load_path_extraction() {
        let mut ob = super::super::symbol::Obarray::new();
        ob.set_symbol_value("default-directory", Value::string("/tmp/project"));
        ob.set_symbol_value(
            "load-path",
            Value::list(vec![
                Value::string("/usr/share/emacs/lisp"),
                Value::Nil,
                Value::string("/home/user/.emacs.d"),
            ]),
        );
        let paths = get_load_path(&ob);
        assert_eq!(
            paths,
            vec![
                "/usr/share/emacs/lisp",
                "/tmp/project",
                "/home/user/.emacs.d"
            ]
        );
    }

    #[test]
    fn find_file_with_suffix_flags() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("neovm-load-flags-{unique}"));
        fs::create_dir_all(&dir).expect("create temp fixture dir");

        let plain = dir.join("choice");
        let el = dir.join("choice.el");
        fs::write(&plain, "plain").expect("write plain fixture");
        fs::write(&el, "el").expect("write el fixture");

        let load_path = vec![dir.to_string_lossy().to_string()];

        // Default mode prefers suffixed files.
        assert_eq!(
            find_file_in_load_path_with_flags("choice", &load_path, false, false, false),
            Some(el.clone())
        );
        // no-suffix mode only tries exact name.
        assert_eq!(
            find_file_in_load_path_with_flags("choice", &load_path, true, false, false),
            Some(plain.clone())
        );
        // must-suffix mode rejects plain file and requires suffixed one.
        assert_eq!(
            find_file_in_load_path_with_flags("choice", &load_path, false, true, false),
            Some(el)
        );
        // no-suffix takes precedence if both flags are set.
        assert_eq!(
            find_file_in_load_path_with_flags("choice", &load_path, true, true, false),
            Some(plain)
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_file_prefers_earlier_load_path_directory() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("neovm-load-path-order-{unique}"));
        let d1 = root.join("d1");
        let d2 = root.join("d2");
        fs::create_dir_all(&d1).expect("create d1");
        fs::create_dir_all(&d2).expect("create d2");

        let plain = d1.join("choice");
        let el = d2.join("choice.el");
        fs::write(&plain, "plain").expect("write plain fixture");
        fs::write(&el, "el").expect("write el fixture");

        let load_path = vec![
            d1.to_string_lossy().to_string(),
            d2.to_string_lossy().to_string(),
        ];
        assert_eq!(
            find_file_in_load_path_with_flags("choice", &load_path, false, false, false),
            Some(plain)
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_file_prefers_newer_source_when_enabled() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("neovm-load-prefer-newer-{unique}"));
        fs::create_dir_all(&dir).expect("create temp fixture dir");

        let elc = dir.join("choice.elc");
        let el = dir.join("choice.el");
        fs::write(&elc, "compiled").expect("write compiled fixture");
        std::thread::sleep(std::time::Duration::from_secs(1));
        fs::write(&el, "source").expect("write source fixture");

        let load_path = vec![dir.to_string_lossy().to_string()];
        assert_eq!(
            find_file_in_load_path_with_flags("choice", &load_path, false, false, false),
            Some(elc)
        );
        assert_eq!(
            find_file_in_load_path_with_flags("choice", &load_path, false, false, true),
            Some(el)
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_file_records_load_history() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("neovm-load-history-{unique}"));
        fs::create_dir_all(&dir).expect("create temp fixture dir");
        let file = dir.join("probe.el");
        fs::write(&file, "(setq vm-load-history-probe t)\n").expect("write fixture");

        let mut eval = super::super::eval::Evaluator::new();
        let loaded = load_file(&mut eval, &file).expect("load file");
        assert_eq!(loaded, Value::True);

        let history = eval
            .obarray()
            .symbol_value("load-history")
            .cloned()
            .unwrap_or(Value::Nil);
        let entries = super::super::value::list_to_vec(&history).expect("load-history is a list");
        assert!(!entries.is_empty(), "load-history should have at least one entry");
        let first = super::super::value::list_to_vec(&entries[0]).expect("entry is a list");
        let path_str = file.to_string_lossy().to_string();
        assert_eq!(first.first().and_then(Value::as_str), Some(path_str.as_str()));
        assert_eq!(
            eval.obarray().symbol_value("load-file-name").cloned(),
            Some(Value::Nil)
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_elc_falls_back_to_source_sibling() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("neovm-load-elc-fallback-{unique}"));
        fs::create_dir_all(&dir).expect("create temp fixture dir");
        let source = dir.join("probe.el");
        let compiled = dir.join("probe.elc");
        fs::write(&source, "(setq vm-load-elc-fallback 'source)\n").expect("write source fixture");
        // Intentionally unreadable as Elisp for current parser.
        fs::write(&compiled, "#[broken\n").expect("write compiled fixture");

        let mut eval = super::super::eval::Evaluator::new();
        let loaded = load_file(&mut eval, &compiled).expect("load file with source fallback");
        assert_eq!(loaded, Value::True);
        assert_eq!(
            eval.obarray().symbol_value("vm-load-elc-fallback").cloned(),
            Some(Value::symbol("source"))
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_elc_without_source_still_errors() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("neovm-load-elc-no-source-{unique}"));
        fs::create_dir_all(&dir).expect("create temp fixture dir");
        let compiled = dir.join("probe.elc");
        fs::write(&compiled, "#[broken\n").expect("write compiled fixture");

        let mut eval = super::super::eval::Evaluator::new();
        let err = load_file(&mut eval, &compiled).expect_err("load should error");
        match err {
            EvalError::Signal { symbol, .. } => assert_eq!(symbol, "invalid-read-syntax"),
            other => panic!("unexpected error: {other:?}"),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_elc_falls_back_when_first_eval_form_errors() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("neovm-load-elc-eval-fallback-{unique}"));
        fs::create_dir_all(&dir).expect("create temp fixture dir");
        let source = dir.join("probe.el");
        let compiled = dir.join("probe.elc");
        fs::write(&source, "(setq vm-load-elc-eval-fallback 'source)\n")
            .expect("write source fixture");
        // Reader can parse this, but evaluation fails because `byte-code`
        // isn't implemented yet in NeoVM.
        fs::write(&compiled, "(byte-code \"\\301\\207\" [x] 1)\n").expect("write compiled fixture");

        let mut eval = super::super::eval::Evaluator::new();
        let loaded = load_file(&mut eval, &compiled).expect("load file with eval fallback");
        assert_eq!(loaded, Value::True);
        assert_eq!(
            eval.obarray()
                .symbol_value("vm-load-elc-eval-fallback")
                .cloned(),
            Some(Value::symbol("source"))
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_elc_falls_back_when_later_eval_form_errors() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("neovm-load-elc-late-eval-fallback-{unique}"));
        fs::create_dir_all(&dir).expect("create temp fixture dir");
        let source = dir.join("probe.el");
        let compiled = dir.join("probe.elc");
        fs::write(&source, "(setq vm-load-elc-late-eval-fallback 'source)\n")
            .expect("write source fixture");
        fs::write(
            &compiled,
            "(setq vm-load-elc-late-eval-fallback 'compiled-prefix)\n(byte-code \"\\301\\207\" [x] 1)\n",
        )
        .expect("write compiled fixture");

        let mut eval = super::super::eval::Evaluator::new();
        let loaded = load_file(&mut eval, &compiled).expect("load file with late eval fallback");
        assert_eq!(loaded, Value::True);
        assert_eq!(
            eval.obarray()
                .symbol_value("vm-load-elc-late-eval-fallback")
                .cloned(),
            Some(Value::symbol("source"))
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_elc_bytecode_literal_without_source_succeeds() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("neovm-load-elc-bytecode-literal-{unique}"));
        fs::create_dir_all(&dir).expect("create temp fixture dir");
        let compiled = dir.join("probe.elc");
        fs::write(
            &compiled,
            ";ELC\x1e\0\0\0\n#@4data\n(defalias 'vm-bytecode-probe #[(x) \"\\bT\\207\" [x] 1 (#$ . 83)])\n(provide 'vm-bytecode-probe)\n",
        )
        .expect("write compiled fixture");

        let mut eval = super::super::eval::Evaluator::new();
        let loaded = load_file(&mut eval, &compiled).expect("load bytecode-literal elc");
        assert_eq!(loaded, Value::True);
        let func = eval
            .obarray()
            .symbol_function("vm-bytecode-probe")
            .cloned()
            .expect("function cell should be set");
        assert!(
            matches!(func, Value::ByteCode(_)),
            "defalias should install a compiled function cell",
        );

        let features = eval
            .obarray()
            .symbol_value("features")
            .cloned()
            .unwrap_or(Value::Nil);
        let feature_values = super::super::value::list_to_vec(&features).expect("features list");
        assert!(
            feature_values
                .iter()
                .any(|v| matches!(v, Value::Symbol(s) if s == "vm-bytecode-probe")),
            "feature should be present after provide",
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_elc_bytecode_literal_without_source_signals_explicit_error_on_call() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("neovm-load-elc-bytecode-call-{unique}"));
        fs::create_dir_all(&dir).expect("create temp fixture dir");
        let compiled = dir.join("probe.elc");
        fs::write(
            &compiled,
            ";ELC\x1e\0\0\0\n#@4data\n(defalias 'vm-bytecode-call-probe #[(x) \"\\bT\\207\" [x] 1 (#$ . 83)])\n",
        )
        .expect("write compiled fixture");

        let mut eval = super::super::eval::Evaluator::new();
        let loaded = load_file(&mut eval, &compiled).expect("load bytecode-literal elc");
        assert_eq!(loaded, Value::True);

        let forms = super::super::parser::parse_forms(
            "(condition-case err (vm-bytecode-call-probe 1) (error (car err)))",
        )
        .expect("parse probe form");
        let mut result = Value::Nil;
        for form in &forms {
            result = eval.eval(form).expect("evaluate probe form");
        }
        assert_eq!(result, Value::symbol("error"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_elc_with_source_prefers_source_for_compiled_literals() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("neovm-load-elc-source-fallback-compiled-{unique}"));
        fs::create_dir_all(&dir).expect("create temp fixture dir");
        let source = dir.join("probe.el");
        let compiled = dir.join("probe.elc");
        fs::write(
            &source,
            "(setq vm-load-elc-compiled-literal-fallback 'source)\n",
        )
        .expect("write source fixture");
        fs::write(
            &compiled,
            "(defalias 'vm-load-elc-compiled-literal-fallback-fn #[(x) \"\\bT\\207\" [x] 1 (#$ . 83)])\n",
        )
        .expect("write compiled fixture");

        let mut eval = super::super::eval::Evaluator::new();
        let loaded = load_file(&mut eval, &compiled).expect("load file with source fallback");
        assert_eq!(loaded, Value::True);
        assert_eq!(
            eval.obarray()
                .symbol_value("vm-load-elc-compiled-literal-fallback")
                .cloned(),
            Some(Value::symbol("source"))
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_elc_paren_bytecode_literal_without_source_succeeds() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("neovm-load-elc-paren-bytecode-{unique}"));
        fs::create_dir_all(&dir).expect("create temp fixture dir");
        let compiled = dir.join("probe.elc");
        fs::write(
            &compiled,
            "(defalias 'vm-paren-bytecode-probe #((x) \"\\bT\\207\" [x] 1 (#$ . 83)))\n(provide 'vm-paren-bytecode-probe)\n",
        )
        .expect("write compiled fixture");

        let mut eval = super::super::eval::Evaluator::new();
        let loaded = load_file(&mut eval, &compiled).expect("load paren-bytecode elc");
        assert_eq!(loaded, Value::True);
        assert!(
            matches!(
                eval.obarray().symbol_function("vm-paren-bytecode-probe"),
                Some(Value::ByteCode(_))
            ),
            "defalias should install a compiled function cell",
        );

        let _ = fs::remove_dir_all(&dir);
    }
}

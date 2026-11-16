//! Guard: oracle rows must not carry an empty-body binding form whose later
//! siblings read one of its bindings (peer audit 5793 T1; lint spec peer
//! 5794).
//!
//! The audited bug shape is a `(let ((VAR ...))))` whose body was closed
//! early: under `--batch -Q` (dynamic binding) the let returns, unwinds VAR,
//! and every later sibling that names VAR signals `void-variable` in BOTH
//! editors — so the recorded expect pins an error and the row tests nothing
//! (`divergence_process_exit_status` tested no exit status at all). Rows can
//! also die more quietly: the early close leaves an enclosing form open, so
//! later forms run in the wrong scope (the eieio temp-buffer row read a
//! buffer-local from inside the temp buffer).
//!
//! The rule is deliberately narrow (peer 5794): an empty body is legal Elisp
//! and some rows pin exactly that, so only an empty-body `let`/`let*` whose
//! later siblings reference one of its bound names is a hard error. Every
//! other empty-body `let`/`let*`, `progn`, `when` and `unless` in an oracle
//! form must appear in `ORACLE-FORM-EMPTY-BODY-ALLOWLIST.tsv` with a one-line
//! reason; a quoted sub-form is data, not code, and is never scanned. The
//! check parses the Elisp with a reader, never a regex.
//!
//! The live scan runs over `neovm-oracle-tests` sources: each raw literal
//! that is the first argument of an `assert_oracle_*` call inside a `#[test]`
//! function. The `old_texts` module pins the eleven audited rows verbatim so
//! the red side of the proof survives history.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::architecture_production_source_roots::crates_dir;
use syn::visit::Visit;
use syn::{Expr, ItemFn, Lit};

/// Rows whose empty body is deliberate, with the reason. Keys are
/// `<crate-relative file>::<test fn>[#<n>]`; `#n` disambiguates repeated
/// findings inside one row in source order.
const ALLOWLIST_TEXT: &str = include_str!("ORACLE-FORM-EMPTY-BODY-ALLOWLIST.tsv");

// ---------------------------------------------------------------------------
// Elisp reader
// ---------------------------------------------------------------------------

/// One read form: a parenthesized list, a vector (self-quoting data), or an
/// atom (symbol, string, char, number). Strings keep their quotes and chars
/// keep their `?` so both stay distinguishable from symbols.
#[derive(Debug)]
enum Form {
    List(Vec<Form>),
    Vector(Vec<Form>),
    Atom(String),
}

impl Form {
    fn items(&self) -> Option<&[Form]> {
        match self {
            Form::List(items) | Form::Vector(items) => Some(items),
            Form::Atom(_) => None,
        }
    }

    fn head_symbol(&self) -> Option<&str> {
        match self.items()?.first() {
            Some(Form::Atom(name)) if !name.starts_with('"') && !name.starts_with('?') => {
                Some(name.as_str())
            }
            _ => None,
        }
    }
}

fn read_forms(source: &str) -> Result<Vec<Form>, String> {
    let chars: Vec<char> = source.chars().collect();
    let mut pos = 0usize;

    fn skip_space(chars: &[char], pos: &mut usize) {
        loop {
            while *pos < chars.len() && chars[*pos].is_whitespace() {
                *pos += 1;
            }
            if *pos < chars.len() && chars[*pos] == ';' {
                while *pos < chars.len() && chars[*pos] != '\n' {
                    *pos += 1;
                }
                continue;
            }
            break;
        }
    }

    fn read_form(chars: &[char], pos: &mut usize) -> Result<Form, String> {
        skip_space(chars, pos);
        let Some(&c) = chars.get(*pos) else {
            return Err("unexpected end of input".to_string());
        };
        match c {
            '(' => {
                *pos += 1;
                read_sequence(chars, pos, ')').map(Form::List)
            }
            '[' => {
                *pos += 1;
                read_sequence(chars, pos, ']').map(Form::Vector)
            }
            ')' | ']' => Err(format!("unbalanced close at {pos}")),
            '\'' => {
                *pos += 1;
                let quoted = read_form(chars, pos)?;
                Ok(Form::List(vec![Form::Atom("quote".into()), quoted]))
            }
            '`' => {
                *pos += 1;
                let quoted = read_form(chars, pos)?;
                Ok(Form::List(vec![Form::Atom("backquote".into()), quoted]))
            }
            ',' | '@' => {
                let unquote = if c == ',' && chars.get(*pos + 1) == Some(&'@') {
                    *pos += 2;
                    ",@"
                } else {
                    *pos += 1;
                    ","
                };
                let inner = read_form(chars, pos)?;
                Ok(Form::List(vec![Form::Atom(unquote.to_string()), inner]))
            }
            '"' => {
                let start = *pos;
                *pos += 1;
                while *pos < chars.len() {
                    if chars[*pos] == '\\' {
                        *pos += 2;
                        continue;
                    }
                    if chars[*pos] == '"' {
                        *pos += 1;
                        let text: String = chars[start..*pos].iter().collect();
                        return Ok(Form::Atom(text));
                    }
                    *pos += 1;
                }
                Err("unterminated string".to_string())
            }
            '?' => {
                let start = *pos;
                *pos += 1;
                if chars.get(*pos) == Some(&'\\') {
                    // Consume the backslash and the escaped designator (`?\(`
                    // is a paren, `?\\` a backslash, `?\C` a modifier head).
                    *pos += 2;
                    // A modifier escape (`?\C-X`, `?\M-x`) also consumes the
                    // `-` and the modified char, which may itself be a
                    // delimiter (`?\C-[` is ESC) or a further escape.
                    if matches!(
                        chars.get(*pos - 1),
                        Some('C') | Some('M') | Some('S') | Some('H') | Some('A')
                    ) && chars.get(*pos) == Some(&'-')
                    {
                        *pos += 1;
                        if chars.get(*pos) == Some(&'\\') {
                            *pos += 2;
                        } else {
                            *pos += 1;
                        }
                    }
                } else {
                    *pos += 1;
                }
                let text: String = chars[start..*pos].iter().collect();
                Ok(Form::Atom(text))
            }
            _ => {
                let start = *pos;
                while *pos < chars.len() {
                    let d = chars[*pos];
                    // A backslash outside a string quotes the next character
                    // into the token (`\"x\"` is one symbol, like GNU lread).
                    if d == '\\' {
                        *pos = (*pos + 2).min(chars.len());
                        continue;
                    }
                    if d.is_whitespace()
                        || matches!(d, '(' | ')' | '[' | ']' | '\'' | '`' | ',' | ';' | '"')
                    {
                        break;
                    }
                    *pos += 1;
                }
                let text: String = chars[start..*pos].iter().collect();
                if text.is_empty() {
                    return Err(format!("unreadable character at {pos}"));
                }
                Ok(Form::Atom(text))
            }
        }
    }

    fn read_sequence(chars: &[char], pos: &mut usize, close: char) -> Result<Vec<Form>, String> {
        let mut items = Vec::new();
        skip_space(chars, pos);
        while *pos < chars.len() && chars[*pos] != close {
            items.push(read_form(chars, pos)?);
            skip_space(chars, pos);
        }
        if *pos >= chars.len() {
            return Err("unclosed form".to_string());
        }
        *pos += 1;
        Ok(items)
    }

    let mut forms = Vec::new();
    skip_space(&chars, &mut pos);
    while pos < chars.len() {
        forms.push(read_form(&chars, &mut pos)?);
        skip_space(&chars, &mut pos);
    }
    Ok(forms)
}

// ---------------------------------------------------------------------------
// Analysis
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FindingKind {
    /// Empty-body `let`/`let*` whose later siblings reference a binding.
    Bug,
    /// Any other empty-body form: legal, but must be allowlisted.
    Empty,
}

impl FindingKind {
    fn label(self) -> &'static str {
        match self {
            Self::Bug => "empty-binding-with-sibling-reference",
            Self::Empty => "empty-body",
        }
    }
}

#[derive(Debug)]
struct Finding {
    kind: FindingKind,
    head: String,
    bound: Vec<String>,
}

/// Binding names of a `(let BINDINGS)` with an empty body. Bare symbols bind
/// nil; `(SYM ...)` pairs bind SYM.
fn binding_names(bindings: &Form) -> Vec<String> {
    let Some(items) = bindings.items() else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for binding in items {
        match binding {
            Form::Atom(name) if !name.starts_with('"') && !name.starts_with('?') => {
                names.push(name.clone());
            }
            Form::List(pair) => {
                if let Some(Form::Atom(name)) = pair.first() {
                    if !name.starts_with('"') && !name.starts_with('?') {
                        names.push(name.clone());
                    }
                }
            }
            _ => {}
        }
    }
    names
}

/// Whether `nodes` reference any of `names`. Quoted and vector contents are
/// data, not code, and are skipped; lambda bodies are code and are scanned.
fn references_any(nodes: &[Form], names: &BTreeSet<String>) -> bool {
    for node in nodes {
        match node {
            Form::Atom(text) => {
                if !text.starts_with('"') && !text.starts_with('?') && names.contains(text) {
                    return true;
                }
            }
            Form::Vector(_) => {}
            Form::List(items) => {
                if matches!(items.first(), Some(Form::Atom(head)) if head == "quote") {
                    continue;
                }
                if references_any(items, names) {
                    return true;
                }
            }
        }
    }
    false
}

/// Scan one evaluation sequence: for every empty-body form, classify it by
/// what the LATER SIBLINGS in this sequence do (the bug shape is a sibling
/// reading the unwound binding). The form's own items are then scanned as
/// the next sequence, so every subform is eventually an element checked
/// against its own later siblings.
fn analyze_sequence(sequence: &[Form], findings: &mut Vec<Finding>) {
    for (index, form) in sequence.iter().enumerate() {
        let Some(items) = form.items() else {
            continue;
        };
        let later = &sequence[index + 1..];
        let head = form.head_symbol();
        if let Some(head) = head {
            let empty_body = match head {
                "let" | "let*" => items.len() == 2,
                "progn" => items.len() == 1,
                "when" | "unless" => items.len() == 2,
                _ => false,
            };
            if empty_body {
                let (kind, bound) = if matches!(head, "let" | "let*") {
                    let names: BTreeSet<String> = binding_names(&items[1]).into_iter().collect();
                    if references_any(later, &names) {
                        (FindingKind::Bug, names.into_iter().collect())
                    } else {
                        (FindingKind::Empty, names.into_iter().collect())
                    }
                } else {
                    (FindingKind::Empty, Vec::new())
                };
                findings.push(Finding {
                    kind,
                    head: head.to_string(),
                    bound,
                });
            }
        }
        // Quoted and function-wrapped lists are data, not code; vectors have
        // no elements to scan. Everything else's items form the next sequence.
        if matches!(head, Some("quote") | Some("function")) {
            continue;
        }
        analyze_sequence(items, findings);
    }
}

fn analyze_source(source: &str) -> Result<Vec<Finding>, String> {
    let forms = read_forms(source)?;
    let mut findings = Vec::new();
    analyze_sequence(&forms, &mut findings);
    Ok(findings)
}

// ---------------------------------------------------------------------------
// Source extraction (syn)
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct RowForm {
    test: String,
    form: String,
}

/// Collect the first-argument string of every `assert_oracle_*` call inside a
/// `#[test]` function. The argument is usually a literal; a plain local
/// holding a string literal (`let form = r#"..."#;`) is followed too, so
/// rows that pass their form through a local are still scanned.
struct OracleFormsVisitor {
    test: String,
    forms: Vec<RowForm>,
    locals: BTreeMap<String, String>,
}

impl<'ast> Visit<'ast> for OracleFormsVisitor {
    fn visit_local(&mut self, local: &'ast syn::Local) {
        if let (Some(init), syn::Pat::Ident(pat)) = (&local.init, &local.pat) {
            if pat.subpat.is_none() {
                if let Expr::Lit(lit) = &*init.expr {
                    if let Lit::Str(text) = &lit.lit {
                        self.locals
                            .entry(pat.ident.to_string())
                            .or_insert_with(|| text.value());
                    }
                }
            }
        }
        syn::visit::visit_local(self, local);
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = &*call.func {
            let ident = path.path.segments.last().map(|s| s.ident.to_string());
            if ident.is_some_and(|name| name.starts_with("assert_oracle")) {
                let text = match call.args.first() {
                    Some(Expr::Lit(lit)) => match &lit.lit {
                        Lit::Str(text) => Some(text.value()),
                        _ => None,
                    },
                    Some(Expr::Path(arg)) if arg.path.segments.len() == 1 => {
                        let name = arg.path.segments[0].ident.to_string();
                        self.locals.get(&name).cloned()
                    }
                    _ => None,
                };
                if let Some(form) = text {
                    if form.trim_start().starts_with('(') {
                        self.forms.push(RowForm {
                            test: self.test.clone(),
                            form,
                        });
                    }
                }
            }
        }
        syn::visit::visit_expr_call(self, call);
    }
}

fn oracle_row_forms(file: &Path) -> Result<Vec<RowForm>, String> {
    let text =
        std::fs::read_to_string(file).map_err(|error| format!("{}: {error}", file.display()))?;
    let syntax = syn::parse_file(&text).map_err(|error| format!("{}: {error}", file.display()))?;
    let mut rows = Vec::new();
    for item in &syntax.items {
        if let syn::Item::Fn(function) = item {
            let is_test = function
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("test"));
            if !is_test {
                continue;
            }
            let mut visitor = OracleFormsVisitor {
                test: function.sig.ident.to_string(),
                forms: Vec::new(),
                locals: BTreeMap::new(),
            };
            visitor.visit_block(&function.block);
            rows.extend(visitor.forms);
        }
    }
    Ok(rows)
}

fn oracle_tests_source_dir() -> PathBuf {
    crates_dir().join("neovm-oracle-tests/src")
}

// ---------------------------------------------------------------------------
// Allowlist
// ---------------------------------------------------------------------------

fn parse_allowlist() -> BTreeMap<String, String> {
    let mut entries = BTreeMap::new();
    for line in ALLOWLIST_TEXT.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, reason)) = line.split_once('\t') else {
            panic!(
                "allowlist row without a reason column: {line:?} \
                 (expected `<key>\\t<one-line reason>`)"
            );
        };
        let reason = reason.trim();
        assert!(
            !reason.is_empty(),
            "allowlist row {key:?} has an empty reason"
        );
        entries
            .entry(key.trim().to_string())
            .or_insert_with(|| reason.to_string());
    }
    entries
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

fn scan_live_sources() -> Vec<(String, Vec<Finding>)> {
    let root = oracle_tests_source_dir();
    assert!(
        root.is_dir(),
        "missing oracle-tests source root: {}",
        root.display()
    );
    let mut files: Vec<PathBuf> = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap_or_else(|error| panic!("read_dir {}: {error}", dir.display()))
            .flatten()
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut findings = Vec::new();
    for file in files {
        for row in oracle_row_forms(&file).unwrap_or_else(|error| panic!("{error}")) {
            let analyzed = analyze_source(&row.form).unwrap_or_else(|error| {
                panic!(
                    "{}::{}: unreadable oracle form: {error}",
                    file.display(),
                    row.test
                )
            });
            if !analyzed.is_empty() {
                let relative = file
                    .strip_prefix(&root)
                    .expect("file under the scanned root")
                    .to_string_lossy()
                    .replace('\\', "/");
                findings.push((format!("{relative}::{}", row.test), analyzed));
            }
        }
    }
    findings
}

#[test]
fn architecture_oracle_forms_have_no_empty_binding_read_by_later_siblings() {
    let allowlist = parse_allowlist();
    let mut used_allowlist: BTreeSet<String> = BTreeSet::new();
    let mut violations: Vec<String> = Vec::new();

    for (row, findings) in scan_live_sources() {
        for (index, finding) in findings.iter().enumerate() {
            let key = if index == 0 {
                row.clone()
            } else {
                format!("{row}#{}", index + 1)
            };
            match finding.kind {
                FindingKind::Bug => violations.push(format!(
                    "{key}: ({}) binds {} and a later sibling reads one of them",
                    finding.head,
                    finding.bound.join(", "),
                )),
                FindingKind::Empty => match allowlist.get(&key) {
                    Some(_) => {
                        used_allowlist.insert(key);
                    }
                    None => violations.push(format!(
                        "{key}: empty-body ({}) binds [{}] not in the allowlist",
                        finding.head,
                        finding.bound.join(", "),
                    )),
                },
            }
        }
    }

    for stale in allowlist
        .keys()
        .filter(|key| !used_allowlist.contains(*key))
    {
        violations.push(format!("stale allowlist row: {stale}"));
    }

    assert!(
        violations.is_empty(),
        "oracle-form lint violations ({}):\n{}\n\
         Bug rows must be fixed (re-nest the binding); Empty rows need an \
         allowlist reason or a fix.",
        violations.len(),
        violations.join("\n")
    );
}

#[test]
fn architecture_oracle_lint_flags_the_eleven_audited_row_texts() {
    // Ten of the audited rows are the trap shape: the empty-body let's
    // binding is read by a later sibling that runs after the let unwound.
    let trapped: &[(&str, &str)] = &[
        (
            "divergence_process_exit_status",
            include_str!("old_oracle_forms/exit_status.el"),
        ),
        (
            "divergence_process_arg_parsing",
            include_str!("old_oracle_forms/arg_parsing.el"),
        ),
        (
            "divergence_process_multibyte_to_temp_buffer",
            include_str!("old_oracle_forms/multibyte.el"),
        ),
        (
            "divergence_process_coding_system_conversion",
            include_str!("old_oracle_forms/coding.el"),
        ),
        (
            "divergence_process_sentinel_with_buffer",
            include_str!("old_oracle_forms/sentinel.el"),
        ),
        (
            "div_cx14_process_kill_buffer_filter",
            include_str!("old_oracle_forms/cx14_kill_filter.el"),
        ),
        (
            "div_cx100_mega_combination_eleven_subsystems_full_stress",
            include_str!("old_oracle_forms/cx100_mega.el"),
        ),
        (
            "div_cx106_subprocess_with_marker_overlay_undo_narrow_mega",
            include_str!("old_oracle_forms/cx106_mega.el"),
        ),
        (
            "div_cx150_mega_buflocal_textprop_overlay_marker_undo_narrow_process_env_timer",
            include_str!("old_oracle_forms/cx150_mega.el"),
        ),
    ];
    for (row, source) in trapped {
        let findings = analyze_source(source)
            .unwrap_or_else(|error| panic!("{row}: unreadable fixture: {error}"));
        assert!(
            findings
                .iter()
                .any(|finding| finding.kind == FindingKind::Bug),
            "{row}: the pre-fix text must be flagged, saw {findings:?}"
        );
    }

    // Two of the audited rows were dead, not trapped: no later sibling
    // reads the binding (cx444's sibling reads only a QUOTED symbol; the
    // eieio row computed temp-sub and never used it — that row was dead for
    // a different reason, a void my-tb-log inside with-temp-buffer). Both
    // are Empty findings — visible to the lint, allowlist-eligible, but not
    // the void-variable bug shape.
    let dead = [
        (
            "div_cx444_ewoc_ops",
            include_str!("old_oracle_forms/cx444_ewoc.el"),
        ),
        (
            "combo_eieio_temp_buffer_insert_from_main",
            include_str!("old_oracle_forms/eieio_temp_insert.el"),
        ),
    ];
    for (row, source) in dead {
        let findings = analyze_source(source)
            .unwrap_or_else(|error| panic!("{row}: unreadable fixture: {error}"));
        assert!(
            !findings.is_empty()
                && findings
                    .iter()
                    .all(|finding| finding.kind == FindingKind::Empty),
            "{row}: the pre-fix text must be seen as empty-body only, saw {findings:?}"
        );
    }
}

#[test]
fn architecture_oracle_lint_accepts_deliberate_empty_bodies() {
    // Legal, non-bug shapes (peer 5794): these are Empty findings at most,
    // never Bug — never a sibling that reads the binding.
    let deliberate = [
        "(progn)",
        "(when t)",
        "(unless nil)",
        "(let ((x 1)))",
        "(let* ((y (point))))\n(list 2 3)",
        "(let ((proc (start-process \"n\" nil \"echo\" \"hi\"))))\n(sit-for 0.1)",
        // A quoted list naming the binding is data, not a reference.
        "(let ((snapshot nil)))\n(list 'snapshot 2)",
        // A sibling reading a DIFFERENT name is not the bug shape.
        "(let ((a 1)))\n(list b)",
    ];
    for source in deliberate {
        let findings =
            analyze_source(source).unwrap_or_else(|error| panic!("{source}: unreadable: {error}"));
        assert!(
            findings
                .iter()
                .all(|finding| finding.kind == FindingKind::Empty),
            "{source}: deliberate empty body misclassified: {findings:?}"
        );
    }

    // The bug shape and only the bug shape is a Bug.
    let malformed = [
        "(progn\n  (let ((proc (start-process \"n\" nil \"true\"))))\n    (process-exit-status proc))",
        "(let ((buf (generate-new-buffer \"b\"))))\n(with-current-buffer buf (point))",
        // A lambda sibling that closes over the binding counts: it would run
        // void outside the let.
        "(let ((h nil)))\n(add-hook 'x (lambda () h))",
    ];
    for source in malformed {
        let findings =
            analyze_source(source).unwrap_or_else(|error| panic!("{source}: unreadable: {error}"));
        assert!(
            findings
                .iter()
                .any(|finding| finding.kind == FindingKind::Bug),
            "{source}: the malformed shape must be flagged, saw {findings:?}"
        );
    }
}

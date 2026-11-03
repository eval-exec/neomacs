//! Shared prep guards owned by memq's Prep E guard set (peer PC10/R6/R7).
//!
//! Four guards, each with an in-file mutation proof:
//!
//! 1. **N13 carried pairs**: every `(placeholder, swap)` pair named by
//!    `CARRIERS` may be referenced only from `eval/green/carried.rs`, its own
//!    definition line, a `use` re-export line, or an internal qualified call
//!    to a child module that defines its own same-named function.  The pair
//!    list is derived from the table, so a later pair joins automatically.
//! 2. **PC1 fenced hot-path methods**: rows in `pc1_hot_path_methods.tsv`
//!    list `(type, method, protected caller)`.  No workspace crate other than
//!    neovm-core and neovm-value may call a listed method.
//! 3. **PC2 hook installers**: rows in `pc2_hook_seams.tsv` list
//!    `(installer, seam file:function, seam file:function)`; calls to the
//!    installer are allowed only at those two seams.
//! 4. **TA2 authority fence**: `test_support::authority` may be named only
//!    from `crates/neovm-core/core_tests/**`, never from `src/**`, including
//!    aliases and glob imports.
//!
//! All scans are fail-closed: unparseable sources, missing tables, malformed
//! rows and any occurrence not accounted for by an allowed site fail.

use proc_macro2::Span;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use syn::visit::{self, Visit};
use syn::{Expr, Item, UseTree};

const PC1_TABLE: &str = include_str!("pc1_hot_path_methods.tsv");
const PC2_TABLE: &str = include_str!("pc2_hook_seams.tsv");

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/neovm-core is two levels below the workspace root")
        .to_path_buf()
}

// ---------------------------------------------------------------------------
// Source text: blank comments and literal contents, preserving line numbers.
// ---------------------------------------------------------------------------

fn skip_normal_string(bytes: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    i
}

fn skip_raw_string(bytes: &[u8], prefix_len: usize) -> Option<usize> {
    let mut i = prefix_len;
    let mut hashes = 0;
    while i < bytes.len() && bytes[i] == b'#' {
        hashes += 1;
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'"' {
        return None;
    }
    i += 1;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let mut j = i + 1;
            let mut seen = 0;
            while j < bytes.len() && bytes[j] == b'#' && seen < hashes {
                seen += 1;
                j += 1;
            }
            if seen == hashes {
                return Some(j);
            }
        }
        i += 1;
    }
    Some(bytes.len())
}

fn skip_char_or_lifetime(bytes: &[u8], i: usize) -> usize {
    let mut j = i + 1;
    if j >= bytes.len() {
        return i + 1;
    }
    if bytes[j] == b'\\' {
        j += 2;
        while j < bytes.len() && bytes[j] != b'\'' && bytes[j] != b'\n' {
            j += 1;
        }
        return if j < bytes.len() && bytes[j] == b'\'' {
            j + 1
        } else {
            i + 1
        };
    }
    j += 1;
    while j < bytes.len() && (bytes[j] & 0b1100_0000) == 0b1000_0000 {
        j += 1;
    }
    if j < bytes.len() && bytes[j] == b'\'' {
        j + 1
    } else {
        i + 1
    }
}

fn skip_literal(bytes: &[u8], i: usize) -> usize {
    let b = bytes[i];
    let raw_prefix = match b {
        b'r' => Some(1),
        b'b' | b'c' if i + 1 < bytes.len() && bytes[i + 1] == b'r' => Some(2),
        _ => None,
    };
    if let Some(plen) = raw_prefix {
        if let Some(end) = skip_raw_string(bytes, i + plen) {
            return end;
        }
    }
    if b == b'"' {
        return skip_normal_string(bytes, i);
    }
    if b == b'\'' {
        return skip_char_or_lifetime(bytes, i);
    }
    i + 1
}

/// Comments and literal contents become spaces; newlines stay so every later
/// line number matches the original file.
fn cleaned_code(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let push_blank = |out: &mut String, start: usize, end: usize| {
        for &b in &bytes[start..end] {
            out.push(if b == b'\n' { '\n' } else { ' ' });
        }
    };
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            push_blank(&mut out, start, i);
            continue;
        }
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            let start = i;
            let mut depth = 1usize;
            i += 2;
            while i < bytes.len() && depth > 0 {
                if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    depth += 1;
                    i += 2;
                } else if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            push_blank(&mut out, start, i);
            continue;
        }
        let raw_prefix = match b {
            b'r' => Some(1),
            b'b' | b'c' if i + 1 < bytes.len() && bytes[i + 1] == b'r' => Some(2),
            _ => None,
        };
        if let Some(plen) = raw_prefix {
            if let Some(end) = skip_raw_string(bytes, i + plen) {
                push_blank(&mut out, i, end);
                i = end;
                continue;
            }
        }
        if b == b'"' {
            let end = skip_normal_string(bytes, i);
            push_blank(&mut out, i, end);
            i = end;
            continue;
        }
        if b == b'\'' {
            let end = skip_char_or_lifetime(bytes, i);
            push_blank(&mut out, i, end);
            i = end;
            continue;
        }
        out.push(b as char);
        i += 1;
    }
    out
}

fn line_of(span: Span) -> usize {
    span.start().line
}

fn rs_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn parse_file(path: &Path) -> Result<(String, syn::File), String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let file = syn::parse_file(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok((text, file))
}

// ---------------------------------------------------------------------------
// N13: CARRIERS-derived reference guard.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CarriedPair {
    names: Vec<String>,
    modules: BTreeSet<String>,
}

fn path_ident(expr: &Expr) -> Option<&syn::Path> {
    match expr {
        Expr::Path(path) => Some(&path.path),
        Expr::Paren(paren) => path_ident(&paren.expr),
        Expr::Reference(reference) => path_ident(&reference.expr),
        _ => None,
    }
}

fn path_names(path: &syn::Path) -> Vec<String> {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect()
}

fn carried_pairs(carried_path: &Path) -> Result<Vec<CarriedPair>, String> {
    let (_, file) = parse_file(carried_path)?;
    let mut pairs = Vec::new();
    for item in &file.items {
        let Item::Const(item) = item else {
            continue;
        };
        if item.ident != "CARRIERS" {
            continue;
        }
        let Expr::Reference(reference) = &*item.expr else {
            return Err(format!(
                "{}: CARRIERS is not a reference",
                carried_path.display()
            ));
        };
        let Expr::Array(array) = &*reference.expr else {
            return Err(format!(
                "{}: CARRIERS is not an array",
                carried_path.display()
            ));
        };
        for elem in &array.elems {
            let Expr::Tuple(tuple) = elem else {
                return Err(format!(
                    "{}: CARRIERS entry is not a tuple",
                    carried_path.display()
                ));
            };
            let mut names = Vec::new();
            let mut modules = BTreeSet::new();
            for expr in &tuple.elems {
                let Some(path) = path_ident(expr) else {
                    return Err(format!(
                        "{}: CARRIERS entry member is not a path",
                        carried_path.display()
                    ));
                };
                let segments = path_names(path);
                let Some(name) = segments.last() else {
                    return Err(format!("{}: empty CARRIERS path", carried_path.display()));
                };
                if let Some(module) = segments.iter().rev().nth(1) {
                    modules.insert(module.clone());
                }
                names.push(name.clone());
            }
            if names.len() != 2 {
                return Err(format!(
                    "{}: CARRIERS entry has {} members, expected 2",
                    carried_path.display(),
                    names.len()
                ));
            }
            pairs.push(CarriedPair { names, modules });
        }
    }
    if pairs.is_empty() {
        return Err(format!(
            "{}: no CARRIERS entries found",
            carried_path.display()
        ));
    }
    Ok(pairs)
}

struct OccurrenceVisitor<'a, 'ast> {
    names: &'a BTreeSet<String>,
    defining_modules: &'a BTreeMap<String, BTreeSet<String>>,
    aliases: BTreeSet<String>,
    local_fns: BTreeSet<String>,
    allowed: BTreeMap<(usize, String), usize>,
    _marker: std::marker::PhantomData<&'ast ()>,
}

impl<'a, 'ast> OccurrenceVisitor<'a, 'ast> {
    fn new(
        names: &'a BTreeSet<String>,
        defining_modules: &'a BTreeMap<String, BTreeSet<String>>,
    ) -> Self {
        Self {
            names,
            defining_modules,
            aliases: BTreeSet::new(),
            local_fns: BTreeSet::new(),
            allowed: BTreeMap::new(),
            _marker: std::marker::PhantomData,
        }
    }

    fn allow_ident(&mut self, ident: &syn::Ident) {
        let name = ident.to_string();
        *self
            .allowed
            .entry((line_of(ident.span()), name))
            .or_insert(0) += 1;
    }

    /// One identifier inside a macro token stream: allow only a qualified
    /// call to a sibling module's same-named function (dbusbind's
    /// `connection::world_state_placeholder` inside `std::cfg_select!`).
    fn allow_macro_ident(&mut self, tokens: &[proc_macro2::TokenTree], index: usize) {
        let proc_macro2::TokenTree::Ident(ident) = &tokens[index] else {
            return;
        };
        let name = ident.to_string();
        if !self.names.contains(&name) {
            return;
        }
        let qualified_internal = index >= 3
            && matches!(&tokens[index - 1], proc_macro2::TokenTree::Punct(p) if p.as_char() == ':')
            && matches!(&tokens[index - 2], proc_macro2::TokenTree::Punct(p) if p.as_char() == ':')
            && match &tokens[index - 3] {
                proc_macro2::TokenTree::Ident(qualifier) => {
                    let qualifier = qualifier.to_string();
                    !self
                        .defining_modules
                        .get(&name)
                        .is_some_and(|modules| modules.contains(&qualifier))
                }
                _ => false,
            };
        if qualified_internal && self.local_fns.contains(&name) {
            self.allow_ident(ident);
        }
    }

    fn scan_tokens(&mut self, stream: proc_macro2::TokenStream) {
        let tokens: Vec<proc_macro2::TokenTree> = stream.into_iter().collect();
        for (index, token) in tokens.iter().enumerate() {
            match token {
                proc_macro2::TokenTree::Group(group) => self.scan_tokens(group.stream()),
                proc_macro2::TokenTree::Ident(_) => self.allow_macro_ident(&tokens, index),
                _ => {}
            }
        }
    }
}

impl<'a, 'ast> Visit<'ast> for OccurrenceVisitor<'a, 'ast> {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let name = item.sig.ident.to_string();
        if self.names.contains(&name) {
            self.allow_ident(&item.sig.ident);
        }
        self.local_fns.insert(name);
        visit::visit_item_fn(self, item);
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        let mut occurrences = Vec::new();
        collect_use_occurrences(&item.tree, self.names, &mut self.aliases, &mut occurrences);
        for (name, span) in occurrences {
            *self.allowed.entry((line_of(span), name)).or_insert(0) += 1;
        }
        visit::visit_item_use(self, item);
    }

    fn visit_expr_path(&mut self, expr: &'ast syn::ExprPath) {
        let segments = path_names(&expr.path);
        if let Some(last) = expr.path.segments.last() {
            let name = last.ident.to_string();
            if self.names.contains(&name) {
                let defining = self.defining_modules.get(&name);
                let qualifier = segments.iter().rev().nth(1);
                let qualifier_is_defining = qualifier
                    .zip(defining)
                    .is_some_and(|(module, modules)| modules.contains(module));
                if segments.len() >= 2 && !qualifier_is_defining && self.local_fns.contains(&name) {
                    self.allow_ident(&last.ident);
                }
            }
        }
        visit::visit_expr_path(self, expr);
    }

    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        self.scan_tokens(item.mac.tokens.clone());
        visit::visit_item_macro(self, item);
    }

    fn visit_expr_macro(&mut self, expr: &'ast syn::ExprMacro) {
        self.scan_tokens(expr.mac.tokens.clone());
        visit::visit_expr_macro(self, expr);
    }

    fn visit_stmt_macro(&mut self, stmt: &'ast syn::StmtMacro) {
        self.scan_tokens(stmt.mac.tokens.clone());
        visit::visit_stmt_macro(self, stmt);
    }
}

fn collect_use_occurrences(
    tree: &UseTree,
    names: &BTreeSet<String>,
    aliases: &mut BTreeSet<String>,
    out: &mut Vec<(String, Span)>,
) {
    match tree {
        UseTree::Path(path) => collect_use_occurrences(&path.tree, names, aliases, out),
        UseTree::Name(name) => {
            let ident = name.ident.to_string();
            if names.contains(&ident) {
                out.push((ident, name.ident.span()));
            }
        }
        UseTree::Rename(rename) => {
            let ident = rename.ident.to_string();
            if names.contains(&ident) {
                out.push((ident, rename.ident.span()));
                let alias = rename.rename.to_string();
                out.push((alias.clone(), rename.rename.span()));
                aliases.insert(alias);
            }
        }
        UseTree::Group(group) => {
            for item in &group.items {
                collect_use_occurrences(item, names, aliases, out);
            }
        }
        UseTree::Glob(_) => {}
    }
}

fn count_occurrences(cleaned: &str, names: &BTreeSet<String>) -> BTreeMap<(usize, String), usize> {
    let mut out: BTreeMap<(usize, String), usize> = BTreeMap::new();
    let bytes = cleaned.as_bytes();
    let mut line = 1usize;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &cleaned[start..i];
            if names.contains(word) {
                *out.entry((line, word.to_string())).or_insert(0) += 1;
            }
            continue;
        }
        i += 1;
    }
    out
}

fn carried_references_in_file(
    path: &Path,
    names: &BTreeSet<String>,
    defining_modules: &BTreeMap<String, BTreeSet<String>>,
) -> Result<Vec<String>, String> {
    let (text, file) = parse_file(path)?;
    let mut visitor = OccurrenceVisitor::new(names, defining_modules);
    visitor.visit_file(&file);
    let mut all_names = names.clone();
    all_names.extend(visitor.aliases.iter().cloned());
    let mut counts = count_occurrences(&cleaned_code(&text), &all_names);
    for ((line, name), allowed) in &visitor.allowed {
        if let Some(slot) = counts.get_mut(&(*line, name.clone())) {
            *slot = slot.saturating_sub(*allowed);
        }
    }
    let mut violations = Vec::new();
    for ((line, name), remaining) in counts {
        if remaining > 0 {
            violations.push(format!(
                "{}:{}: carried-state pair reference {name:?} x{remaining}",
                path.display(),
                line
            ));
        }
    }
    Ok(violations)
}

pub(crate) fn scan_carried_references(
    core_src: &Path,
    value_src: &Path,
) -> Result<(), Vec<String>> {
    let carried = core_src.join("emacs_core/runtime/eval/green/carried.rs");
    let pairs = match carried_pairs(&carried) {
        Ok(pairs) => pairs,
        Err(error) => return Err(vec![error]),
    };
    let mut names = BTreeSet::new();
    let mut defining_modules: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for pair in &pairs {
        for name in &pair.names {
            names.insert(name.clone());
            defining_modules
                .entry(name.clone())
                .or_default()
                .extend(pair.modules.iter().cloned());
        }
    }
    let mut files = Vec::new();
    rs_files(core_src, &mut files);
    if value_src.exists() {
        rs_files(value_src, &mut files);
    }
    let mut violations = Vec::new();
    for path in files {
        if path == carried {
            continue;
        }
        match carried_references_in_file(&path, &names, &defining_modules) {
            Ok(mut found) => violations.append(&mut found),
            Err(error) => violations.push(error),
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

#[test]
fn architecture_n13_carried_pairs_have_one_reference_site() {
    let root = workspace_root();
    if let Err(violations) = scan_carried_references(
        &root.join("crates/neovm-core/src"),
        &root.join("crates/neovm-value/src"),
    ) {
        panic!(
            "{} carried-state reference violations (only eval/green/carried.rs may swap World state):\n{}",
            violations.len(),
            violations
                .iter()
                .take(80)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

#[test]
fn architecture_n13_guard_self_test() {
    let dir = tempfile::tempdir().expect("tempdir");
    let core = dir.path().join("crates/neovm-core/src");
    let value = dir.path().join("crates/neovm-value/src");
    std::fs::create_dir_all(core.join("emacs_core/runtime/eval/green")).unwrap();
    std::fs::create_dir_all(core.join("tagged")).unwrap();
    std::fs::create_dir_all(&value).unwrap();
    std::fs::write(
        core.join("emacs_core/runtime/eval/green/carried.rs"),
        "const CARRIERS: &[(P, S)] = &[(\n    crate::tagged::pair::placeholder,\n    crate::tagged::pair::swap,\n)];\n",
    )
    .unwrap();
    std::fs::write(
        core.join("tagged/pair.rs"),
        "pub fn placeholder() {}\npub fn swap() {}\npub fn outer() {\n    child::swap();\n}\n",
    )
    .unwrap();
    std::fs::write(core.join("tagged/child.rs"), "pub fn swap() {}\n").unwrap();
    let green = scan_carried_references(&core, &value);
    assert!(green.is_ok(), "false positive: {green:?}");
    std::fs::write(
        core.join("tagged/external.rs"),
        "pub fn violate() {\n    crate::tagged::pair::swap();\n}\n",
    )
    .unwrap();
    let red = scan_carried_references(&core, &value);
    assert!(red.is_err(), "mutation not detected");
    assert!(
        format!("{red:?}").contains("external.rs"),
        "wrong violation site: {red:?}"
    );
}

// ---------------------------------------------------------------------------
// PC1: hot-path fenced method table.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Pc1Row {
    type_name: String,
    method: String,
    protected_caller: String,
}

fn parse_pc1_table(text: &str) -> Result<Vec<Pc1Row>, String> {
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.starts_with('#') || line.starts_with("type\t") || line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 3 {
            return Err(format!("pc1 table line {}: expected 3 columns", index + 1));
        }
        rows.push(Pc1Row {
            type_name: fields[0].to_string(),
            method: fields[1].to_string(),
            protected_caller: fields[2].to_string(),
        });
    }
    Ok(rows)
}

fn line_calls_method(line: &str, method: &str) -> bool {
    let bytes = line.as_bytes();
    let needle = method.as_bytes();
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            let qualifies = match i.checked_sub(1).and_then(|j| bytes.get(j)) {
                Some(b'.') => true,
                Some(b':') => i >= 2 && bytes[i - 2] == b':',
                _ => false,
            };
            if qualifies {
                let mut j = i + needle.len();
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'(' {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

pub(crate) fn scan_pc1_table(workspace: &Path, text: &str) -> Result<Vec<String>, String> {
    let rows = parse_pc1_table(text)?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let crates_dir = workspace.join("crates");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&crates_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "neovm-core" || name == "neovm-value" {
            continue;
        }
        let src = entry.path().join("src");
        if src.is_dir() {
            rs_files(&src, &mut files);
        }
    }
    let mut violations = Vec::new();
    for row in &rows {
        for file in &files {
            let Ok(text) = std::fs::read_to_string(file) else {
                violations.push(format!("{}: unreadable", file.display()));
                continue;
            };
            for (index, line) in cleaned_code(&text).lines().enumerate() {
                if line_calls_method(line, &row.method) {
                    violations.push(format!(
                        "{}:{}: calls fenced hot-path method {}.{} (protected caller: {})",
                        file.display(),
                        index + 1,
                        row.type_name,
                        row.method,
                        row.protected_caller
                    ));
                }
            }
        }
    }
    Ok(violations)
}

#[test]
fn architecture_pc1_fenced_hot_path_methods_are_called_only_from_core_or_value() {
    match scan_pc1_table(&workspace_root(), PC1_TABLE) {
        Ok(violations) if violations.is_empty() => {}
        Ok(violations) => panic!(
            "{} PC1 hot-path fence violations:\n{}",
            violations.len(),
            violations
                .iter()
                .take(80)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        ),
        Err(error) => panic!("PC1 table: {error}"),
    }
}

#[test]
fn architecture_pc1_guard_self_test() {
    let dir = tempfile::tempdir().expect("tempdir");
    let crates = dir.path().join("crates");
    for name in ["neovm-core", "neovm-value", "neomacs-other"] {
        std::fs::create_dir_all(crates.join(name).join("src")).unwrap();
    }
    let table = "type\tmethod\tprotected_caller\nValue\tfenced_raw_read\trun_loop\n";
    std::fs::write(
        crates.join("neovm-value/src/ok.rs"),
        "fn f(v: V) { v.fenced_raw_read(); }\n",
    )
    .unwrap();
    let green = scan_pc1_table(dir.path(), table).unwrap();
    assert!(green.is_empty(), "false positive: {green:?}");
    std::fs::write(
        crates.join("neomacs-other/src/bad.rs"),
        "fn g(v: V) { v.fenced_raw_read(); }\n",
    )
    .unwrap();
    let red = scan_pc1_table(dir.path(), table).unwrap();
    assert!(!red.is_empty(), "mutation not detected");
    assert!(red[0].contains("neomacs-other"), "wrong site: {red:?}");
}

// ---------------------------------------------------------------------------
// PC2: hook installer two-seam table.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Pc2Row {
    installer: String,
    seams: BTreeSet<(String, String)>,
}

fn parse_pc2_table(text: &str) -> Result<Vec<Pc2Row>, String> {
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.starts_with('#') || line.starts_with("installer\t") || line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 3 {
            return Err(format!("pc2 table line {}: expected 3 columns", index + 1));
        }
        let installer = fields[0].to_string();
        let mut seams = BTreeSet::new();
        for pair in &fields[1..] {
            let Some((file, function)) = pair.split_once(':') else {
                return Err(format!(
                    "pc2 table line {}: seam is not file:function",
                    index + 1
                ));
            };
            seams.insert((file.to_string(), function.to_string()));
        }
        if seams.len() != 2 {
            return Err(format!(
                "pc2 table line {}: expected exactly two seams",
                index + 1
            ));
        }
        rows.push(Pc2Row { installer, seams });
    }
    Ok(rows)
}

struct InstallerCallCollector<'a> {
    installer: &'a str,
    current_fn: Option<String>,
    sites: BTreeSet<String>,
}

impl<'a, 'ast> Visit<'ast> for InstallerCallCollector<'a> {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let previous = self.current_fn.replace(item.sig.ident.to_string());
        visit::visit_item_fn(self, item);
        self.current_fn = previous;
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let Expr::Path(path) = &*call.func {
            if path
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == self.installer)
            {
                if let Some(enclosing) = self.current_fn.clone() {
                    self.sites.insert(enclosing);
                }
            }
        }
        visit::visit_expr_call(self, call);
    }
}

pub(crate) fn scan_pc2_table(workspace: &Path, text: &str) -> Result<Vec<String>, String> {
    let rows = parse_pc2_table(text)?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let crates_dir = workspace.join("crates");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&crates_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.path().is_dir() {
            continue;
        }
        let src = entry.path().join("src");
        if src.is_dir() {
            rs_files(&src, &mut files);
        }
        let tests = entry.path().join("tests");
        if tests.is_dir() {
            rs_files(&tests, &mut files);
        }
    }
    let mut violations = Vec::new();
    for row in &rows {
        for file in &files {
            let Ok(text) = std::fs::read_to_string(file) else {
                continue;
            };
            let Ok(parsed) = syn::parse_file(&text) else {
                violations.push(format!("{}: unparseable", file.display()));
                continue;
            };
            let mut collector = InstallerCallCollector {
                installer: &row.installer,
                current_fn: None,
                sites: BTreeSet::new(),
            };
            collector.visit_file(&parsed);
            let relative = file
                .strip_prefix(workspace)
                .unwrap_or(file)
                .to_string_lossy()
                .into_owned();
            for function in &collector.sites {
                let signed = row.seams.iter().any(|(seam_file, seam_fn)| {
                    seam_fn == function
                        && (relative.ends_with(seam_file) || seam_file.ends_with(&relative))
                });
                if !signed {
                    violations.push(format!(
                        "{}:{}: {} installer call outside the two signed seams ({:?})",
                        relative, function, row.installer, row.seams
                    ));
                }
            }
        }
    }
    Ok(violations)
}

#[test]
fn architecture_pc2_hook_installers_have_exactly_two_seams() {
    match scan_pc2_table(&workspace_root(), PC2_TABLE) {
        Ok(violations) if violations.is_empty() => {}
        Ok(violations) => panic!(
            "{} PC2 hook-installer violations:\n{}",
            violations.len(),
            violations
                .iter()
                .take(80)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        ),
        Err(error) => panic!("PC2 table: {error}"),
    }
}

#[test]
fn architecture_pc2_guard_self_test() {
    let dir = tempfile::tempdir().expect("tempdir");
    let core = dir.path().join("crates/neovm-core/src");
    std::fs::create_dir_all(&core).unwrap();
    std::fs::write(
        core.join("construct.rs"),
        "pub fn new_inner() { unsafe { install_scratch_hook(1); } }\n",
    )
    .unwrap();
    std::fs::write(
        core.join("pdump_reconstruct.rs"),
        "pub fn restore() { unsafe { install_scratch_hook(2); } }\n",
    )
    .unwrap();
    let table = "installer\tseam1\tseam2\n\
                 install_scratch_hook\tcrates/neovm-core/src/construct.rs:new_inner\tcrates/neovm-core/src/pdump_reconstruct.rs:restore\n";
    let green = scan_pc2_table(dir.path(), table).unwrap();
    assert!(green.is_empty(), "false positive: {green:?}");
    std::fs::write(
        core.join("extra.rs"),
        "pub fn break_it() { unsafe { install_scratch_hook(3); } }\n",
    )
    .unwrap();
    let red = scan_pc2_table(dir.path(), table).unwrap();
    assert!(!red.is_empty(), "mutation not detected");
    assert!(red[0].contains("extra.rs"), "wrong site: {red:?}");
}

// ---------------------------------------------------------------------------
// TA2: authority fence is core_tests-only.
// ---------------------------------------------------------------------------

fn compact_whitespace(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

pub(crate) fn scan_authority_fence(core_root: &Path) -> Result<(), Vec<String>> {
    let src = core_root.join("src");
    let mut files = Vec::new();
    rs_files(&src, &mut files);
    let mut violations = Vec::new();
    for path in files {
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                violations.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        let cleaned = cleaned_code(&text);
        let compact = compact_whitespace(&cleaned);
        if compact.contains("test_support::authority") {
            violations.push(format!(
                "{}: names test_support::authority from core src (fence is core_tests/** only)",
                path.display()
            ));
        }
        if compact.contains("test_support::*") {
            violations.push(format!(
                "{}: glob-imports test_support (can reach authority)",
                path.display()
            ));
        }
        if let Some(rest) = compact.split("test_supportas").nth(1) {
            let alias: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !alias.is_empty() && compact.contains(&format!("{alias}::authority")) {
                violations.push(format!(
                    "{}: aliases test_support as {alias} and names {alias}::authority",
                    path.display()
                ));
            }
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

#[test]
fn architecture_test_support_authority_is_named_only_from_core_tests() {
    if let Err(violations) = scan_authority_fence(&workspace_root().join("crates/neovm-core")) {
        panic!(
            "{} TA2 authority-fence violations:\n{}",
            violations.len(),
            violations
                .iter()
                .take(80)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

#[test]
fn architecture_ta2_guard_self_test() {
    let dir = tempfile::tempdir().expect("tempdir");
    let core = dir.path().join("crates/neovm-core");
    std::fs::create_dir_all(core.join("src")).unwrap();
    std::fs::create_dir_all(core.join("core_tests")).unwrap();
    std::fs::write(
        core.join("src/clean.rs"),
        "pub fn f() { neovm_value::test_support::some_query(); }\n",
    )
    .unwrap();
    let green = scan_authority_fence(&core);
    assert!(green.is_ok(), "false positive: {green:?}");
    std::fs::write(
        core.join("src/bad.rs"),
        "pub fn g() { neovm_value::test_support::authority::forge(); }\n",
    )
    .unwrap();
    let red = scan_authority_fence(&core);
    assert!(red.is_err(), "mutation not detected");
    std::fs::remove_file(core.join("src/bad.rs")).unwrap();
    std::fs::write(
        core.join("core_tests/allowed.rs"),
        "pub fn h() { neovm_value::test_support::authority::forge(); }\n",
    )
    .unwrap();
    let allowed = scan_authority_fence(&core);
    assert!(allowed.is_ok(), "core_tests must be allowed: {allowed:?}");
    std::fs::write(
        core.join("src/alias.rs"),
        "use neovm_value::test_support as ts;\npub fn i() { ts::authority::forge(); }\n",
    )
    .unwrap();
    let alias = scan_authority_fence(&core);
    assert!(alias.is_err(), "alias mutation not detected");
}

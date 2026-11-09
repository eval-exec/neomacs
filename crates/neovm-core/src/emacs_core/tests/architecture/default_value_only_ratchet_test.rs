//! Per-crate ratchet: callers of the default-only variable accessor may not
//! grow.
//!
//! The default-only accessor (`Context::default_value_only` /
//! `default_value_only_by_name`, and the `EditHost` forwarding of the same)
//! reads the variable's DEFAULT-ONLY value: for a buffer-local variable the
//! default (owner) binding, never the current buffer's binding. GNU spells
//! these reads `Fdefault_value` (src/data.c) or
//! `BVAR (&buffer_defaults, slot)` (src/buffer.c). A GNU C read of a
//! `DEFVAR_*` global, or a Lisp `symbol-value`, is the VISIBLE read and uses
//! `Context::builtin_var_value` / `builtin_var_value_by_name` instead.
//!
//! Counts at the C2 rename:
//!
//! - `neovm-eval` 3 -- the audited residue; every one is a genuine GNU
//!   default read and carries a GNU citation at the site:
//!   * `runtime/eval/mod.rs` `buffer_default_value` and
//!     `buffer_default_value_fixed`: GNU `BVAR (&buffer_defaults, slot)` for a
//!     slot-backed per-buffer variable (src/buffer.c:1116, 2289), else the
//!     variable's default cell;
//!   * `lisp/print/mod.rs` `bind_unreadable_printer_default`: GNU
//!     `specbind`'s LET_DEFAULT default-cell save (src/eval.c:3674-3710) for
//!     the print-unreadable-function nil binding at src/print.c:1711, which
//!     runs after print_prepare's destination-buffer switch
//!     (src/print.c:143-151). The printer_settings_gnu_*_local_callback
//!     fixtures require it.
//! - `neomacs-layout-engine` and `neomacs` -- AUDITED by the
//!   P2-CORE-BACKLOG "DISPLAY-SIDE DEFAULT-ONLY READS" row: all 41 callers
//!   were classified against GNU and migrated
//!   (integ-inbox/LAYOUT-DEFAULT-READS-AUDIT.md): 17 are `find_symbol_value`
//!   with the displayed buffer current (V-BUF, now
//!   `VariableView::value_in_buffer`), 24 are C-global reads from the
//!   command/startup context (V-CUR, now `builtin_var_value`).  The helper
//!   escapes `neovm_bridge::effective_buffer_value` and
//!   `resolve_layout_vars`'s captures_default fallback read the same visible
//!   path, pinned under a buffer-local binding and a live dynamic `let`.
//!   Their counts are 0 today.
//! - every other crate 0.
//!
//! The audited crates' total is 3 today. It may only DECREASE (the
//! print_buffer follow-up is expected to remove the binder read and lower the
//! baseline to 2); any new caller must be justified with GNU source lines here
//! and at the accessor site.

use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;

/// Frozen per-crate caller counts at the C2 rename. Every crate with a caller
/// is listed; a crate absent from this list must have none.
const BASELINE: &[(&str, usize)] = &[
    // Audited by the P2-CORE-BACKLOG default-reads audit; total must be 3.
    ("neovm-edit", 0),
    ("neovm-display", 0),
    ("neovm-system", 0),
    ("neovm-eval", 3),
    ("neovm-core", 0),
    // Audited by LAYOUT-DEFAULT-READS; all 41 callers migrated to the
    // visible read (see the module docs and the audit document).
    ("neomacs-layout-engine", 0),
    ("neomacs", 0),
];

/// The seven crates the default-reads audit covered (the C1/C3/C2 wave plus
/// LAYOUT-DEFAULT-READS).
const AUDITED: &[&str] = &[
    "neovm-edit",
    "neovm-display",
    "neovm-system",
    "neovm-eval",
    "neovm-core",
    "neomacs-layout-engine",
    "neomacs",
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn count_crate(root: &Path, crate_name: &str) -> Vec<String> {
    fn walk(dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name == "tests" || name == "target" || name.starts_with("moved_") {
                    continue;
                }
                walk(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                for (index, line) in text.lines().enumerate() {
                    let is_call = line.contains(".default_value_only(")
                        || line.contains(".default_value_only_by_name(");
                    if !is_call || line.contains("fn default_value_only") {
                        continue;
                    }
                    out.push(format!("{}:{}: {}", path.display(), index + 1, line.trim()));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&root.join("crates").join(crate_name).join("src"), &mut out);
    out
}

/// Every crate directory that carries a `src/` tree.
fn workspace_crates(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root.join("crates"))
        .expect("crates directory")
        .flatten()
        .filter(|entry| entry.path().join("src").is_dir())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .collect();
    names.sort();
    names
}

#[test]
fn default_value_only_callers_never_grow() {
    let root = workspace_root();
    let mut problems = Vec::new();
    let mut audited_total = 0;
    for crate_name in workspace_crates(&root) {
        let sites = count_crate(&root, &crate_name);
        let baseline = BASELINE
            .iter()
            .find_map(|&(name, count)| (name == crate_name).then_some(count))
            .unwrap_or(0);
        if AUDITED.contains(&crate_name.as_str()) {
            audited_total += sites.len();
        }
        if sites.len() > baseline {
            let note = if AUDITED.contains(&crate_name.as_str()) {
                "audited crate: a new caller must be a genuine GNU Fdefault_value read, \
                 justified with GNU source lines at the site"
            } else {
                "UNAUDITED display-side crate: new default-only reads are not allowed; \
                 use the visible read (P2-CORE-BACKLOG DISPLAY-SIDE DEFAULT-ONLY READS)"
            };
            problems.push(format!(
                "{crate_name}: {} caller(s), frozen at {baseline} ({note}):\n  {}",
                sites.len(),
                sites.join("\n  ")
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "the default-only accessor grew (audited total {audited_total}, expected 3):\n{}",
        problems.join("\n")
    );
    assert_eq!(
        audited_total, 3,
        "the audited default-only accessor's total changed ({audited_total}, expected 3). If a \
         caller was legitimately removed, lower the per-crate BASELINE here; if one was added, it \
         must be a genuine GNU Fdefault_value read justified at the site with GNU source lines."
    );
}

/// Every name bound to a `VariableView` (or `Option<VariableView>`) in one
/// source file.
///
/// `VariableView::value` is GNU `Vfoo` WITHOUT a buffer: for a buffer-local
/// variable it answers the default cell.  A display-side read runs with the
/// window's buffer current (`set_buffer_internal_1`, xdisp.c:20555; the C swap
/// is `set_buffer_internal_2`, buffer.c:2374-2391), so it must use
/// `VariableView::value_in_buffer` (V-BUF) or the evaluator's
/// `builtin_var_value` (V-CUR).  `value` is too common a method name for a
/// whole-workspace scan, so the match is receiver-shaped -- but not
/// name-shaped: every binding of the view type is a candidate receiver, not
/// only the conventional `vars`.
#[derive(Default)]
struct VariableViewBindings {
    names: std::collections::BTreeSet<String>,
    /// Lines of direct producer chains (`ctx.view().value(..)`), which have no
    /// named receiver at all (LD-5c).
    chains: Vec<usize>,
    problems: Vec<String>,
}

/// The two `Context` methods that hand out a `VariableView`
/// (`variable_access.rs:417`, `:575`).
fn is_view_producer_method(method: &syn::Ident) -> bool {
    method == "variables" || method == "view"
}

/// Whether `expr` IS a view-producer call (`eval.variables()`,
/// `ctx.view()`), its fallible form (`...?`), or a receiver chain that ends in
/// one.  Over-approximates on purpose: a false positive fails the ratchet
/// loudly instead of hiding a default-only read.
fn expr_ends_in_view_producer(expr: &syn::Expr) -> bool {
    match expr {
        syn::Expr::MethodCall(call) => {
            is_view_producer_method(&call.method) || expr_ends_in_view_producer(&call.receiver)
        }
        syn::Expr::Try(try_expr) => expr_ends_in_view_producer(&try_expr.expr),
        syn::Expr::Paren(paren) => expr_ends_in_view_producer(&paren.expr),
        syn::Expr::Group(group) => expr_ends_in_view_producer(&group.expr),
        _ => false,
    }
}

/// `VariableView<..>` or `Option<VariableView<..>>`, behind any reference,
/// paren or group wrapper (the shapes the display crates use).
fn is_variable_view_type(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Reference(reference) => is_variable_view_type(&reference.elem),
        syn::Type::Paren(paren) => is_variable_view_type(&paren.elem),
        syn::Type::Group(group) => is_variable_view_type(&group.elem),
        syn::Type::Path(path) => {
            let Some(segment) = path.path.segments.last() else {
                return false;
            };
            if segment.ident == "VariableView" {
                return true;
            }
            if segment.ident != "Option" {
                return false;
            }
            let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                return false;
            };
            let mut inner = arguments.args.iter().filter_map(|argument| match argument {
                syn::GenericArgument::Type(inner) => Some(inner),
                _ => None,
            });
            let (Some(inner), None) = (inner.next(), inner.next()) else {
                return false;
            };
            is_variable_view_type(inner)
        }
        _ => false,
    }
}

/// A `VariableView` anywhere in a type, even in a shape the binding visitor
/// cannot name (a tuple, `Vec<VariableView>`, a generic argument).  Those
/// shapes fail closed instead of escaping the scan.
struct MentionsVariableView(bool);

impl<'ast> syn::visit::Visit<'ast> for MentionsVariableView {
    fn visit_path_segment(&mut self, segment: &'ast syn::PathSegment) {
        if segment.ident == "VariableView" {
            self.0 = true;
        }
        syn::visit::visit_path_segment(self, segment);
    }
}

fn mentions_variable_view(ty: &syn::Type) -> bool {
    let mut finder = MentionsVariableView(false);
    finder.visit_type(ty);
    finder.0
}

impl<'ast> syn::visit::Visit<'ast> for VariableViewBindings {
    fn visit_pat_type(&mut self, node: &'ast syn::PatType) {
        if is_variable_view_type(&node.ty) {
            match &*node.pat {
                syn::Pat::Ident(ident) => {
                    self.names.insert(ident.ident.to_string());
                }
                // A wildcard binds no receiver, so no `NAME.value` can exist.
                syn::Pat::Wild(_) => {}
                _ => self
                    .problems
                    .push("unnameable VariableView binding pattern".to_string()),
            }
        } else if mentions_variable_view(&node.ty) {
            self.problems
                .push("unhandled VariableView binding shape".to_string());
        }
        syn::visit::visit_pat_type(self, node);
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        // LD-5c (i): an untyped let whose initializer is a view producer --
        // `let vars = eval.variables();`, optionally `?` -- binds a receiver
        // of the view type even though no annotation names the type.
        // A typed `let` is a `Pat::Type` (handled by visit_pat_type); only a
        // bare `Pat::Ident` here is untyped.
        if let syn::Pat::Ident(pat) = &local.pat
            && let Some(init) = &local.init
            && expr_ends_in_view_producer(&init.expr)
        {
            self.names.insert(pat.ident.to_string());
        }
        syn::visit::visit_local(self, local);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        // LD-5c (ii): a direct chain (`ctx.variables().value(..)`,
        // `ctx.view().value(..)`) has no named receiver; flag the call itself.
        if call.method == "value" && expr_ends_in_view_producer(&call.receiver) {
            self.chains.push(call.method.span().start().line);
        }
        syn::visit::visit_expr_method_call(self, call);
    }

    fn visit_field(&mut self, field: &'ast syn::Field) {
        if is_variable_view_type(&field.ty) {
            match &field.ident {
                Some(ident) => {
                    self.names.insert(ident.to_string());
                }
                None => self
                    .problems
                    .push("unnameable VariableView tuple-field binding".to_string()),
            }
        } else if mentions_variable_view(&field.ty) {
            self.problems
                .push("unhandled VariableView field shape".to_string());
        }
        syn::visit::visit_field(self, field);
    }

    fn visit_item_const(&mut self, item: &'ast syn::ItemConst) {
        if is_variable_view_type(&item.ty) {
            self.names.insert(item.ident.to_string());
        } else if mentions_variable_view(&item.ty) {
            self.problems
                .push("unhandled VariableView const shape".to_string());
        }
        syn::visit::visit_item_const(self, item);
    }

    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        if is_variable_view_type(&item.ty) {
            self.names.insert(item.ident.to_string());
        } else if mentions_variable_view(&item.ty) {
            self.problems
                .push("unhandled VariableView static shape".to_string());
        }
        syn::visit::visit_item_static(self, item);
    }
}

/// Every default-only `VariableView::value` read in one source file, as
/// `label:line: NAME.value` entries.  Fails closed: an unparseable file or a
/// `VariableView` binding whose receiver name cannot be recovered is an
/// error, never a silent skip.
fn variable_view_default_reads_in_source(label: &str, text: &str) -> Result<Vec<String>, String> {
    let file = syn::parse_file(text).map_err(|error| {
        format!("{label}: the VariableView ratchet cannot parse this source: {error}")
    })?;
    let mut bindings = VariableViewBindings::default();
    bindings.visit_file(&file);
    if !bindings.problems.is_empty() {
        return Err(format!("{label}: {}", bindings.problems.join("; ")));
    }
    let mut out = Vec::new();
    for name in &bindings.names {
        for (start, _) in text.match_indices(name.as_str()) {
            let previous = text[..start].chars().next_back();
            if previous.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
                continue;
            }
            // `NAME.value(` and the fallible-chain spelling `NAME?.value(`.
            let after = &text[start + name.len()..];
            let trimmed = after.trim_start();
            let chain = trimmed
                .strip_prefix('?')
                .map(str::trim_start)
                .unwrap_or(trimmed);
            if chain.starts_with(".value(") {
                let line = text[..start].matches('\n').count() + 1;
                out.push(format!("{label}:{line}: {name}.value"));
            }
        }
    }
    // Direct producer chains (`ctx.variables().value(..)`, `ctx.view()?.value(..)`)
    // have no name to scan; the visitor recorded each `.value(` call.
    for line in bindings.chains {
        out.push(format!("{label}:{line}: <producer>.value"));
    }
    out.sort();
    Ok(out)
}

/// Count `VariableView::value` callers in a crate's production sources, for
/// every name the source binds to the view type.  Fails closed on unreadable
/// directories/files and on unnameable binding shapes.
fn count_variable_view_default_reads(root: &Path, crate_name: &str) -> Result<Vec<String>, String> {
    fn walk(dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
        let entries =
            std::fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        for entry in entries {
            let path = entry
                .map_err(|error| format!("{}: {error}", dir.display()))?
                .path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name == "tests" || name == "target" || name.starts_with("moved_") {
                    continue;
                }
                walk(&path, out)?;
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                let text = std::fs::read_to_string(&path)
                    .map_err(|error| format!("{}: {error}", path.display()))?;
                out.extend(variable_view_default_reads_in_source(
                    &path.display().to_string(),
                    &text,
                )?);
            }
        }
        Ok(())
    }
    let src = root.join("crates").join(crate_name).join("src");
    if !src.is_dir() {
        return Err(format!(
            "{}: display crate source directory is missing",
            src.display()
        ));
    }
    let mut out = Vec::new();
    walk(&src, &mut out)?;
    Ok(out)
}

/// The display crates may not (re)introduce the default-only
/// `VariableView::value` read; LAYOUT-DEFAULT-READS LD-4 migrated the last
/// six variable reads (composition-function-table, nobreak-char-ascii-display,
/// x-stretch-cursor, mode-line-in-non-selected-windows,
/// scroll-minibuffer-conservatively, x-cursor-fore-pixel) to the visible
/// accessors.  Their count is 0 today.
#[test]
fn variable_view_default_reads_never_grow_in_the_display_crates() {
    let root = workspace_root();
    let mut problems = Vec::new();
    for crate_name in ["neomacs-layout-engine", "neomacs"] {
        match count_variable_view_default_reads(&root, crate_name) {
            Ok(sites) if sites.is_empty() => {}
            Ok(sites) => problems.push(format!(
                "{crate_name}: {} VariableView::value caller(s), frozen at 0; the display read \
                 runs with the window's buffer current, so use value_in_buffer (V-BUF) or \
                 builtin_var_value (V-CUR) instead:\n  {}",
                sites.len(),
                sites.join("\n  ")
            )),
            Err(error) => problems.push(error),
        }
    }
    assert!(
        problems.is_empty(),
        "the default-only VariableView::value accessor grew in a display crate:\n{}",
        problems.join("\n")
    );
}

#[cfg(test)]
mod variable_view_ratchet_self_tests {
    use super::*;
    use syn::visit::Visit;

    #[test]
    fn every_typed_variable_view_binding_is_discovered() {
        let source = r#"
use neovm_core::emacs_core::eval::VariableView;
fn walk(vars: VariableView<'_>, variables: Option<VariableView<'_>>, other: &u8) {
    let view: VariableView<'a> = todo!();
    let maybe: Option<VariableView<'a>> = None;
}
struct Holder<'a> {
    slot: VariableView<'a>,
}
const UNRELATED: u8 = 0;
"#;
        let file = syn::parse_file(source).expect("sample parses");
        let mut bindings = VariableViewBindings::default();
        bindings.visit_file(&file);
        assert!(bindings.problems.is_empty(), "{:?}", bindings.problems);
        let expected: std::collections::BTreeSet<String> =
            ["vars", "variables", "view", "maybe", "slot"]
                .into_iter()
                .map(str::to_string)
                .collect();
        assert_eq!(
            bindings.names, expected,
            "every typed VariableView binding must be discovered, not just `vars`"
        );
    }

    #[test]
    fn planted_non_vars_receiver_default_read_is_red() {
        // The receiver is `variables`, not the conventional `vars`, and the
        // call uses the fallible-chain spelling: a name-shaped scan misses it.
        let source = r#"
use neovm_core::emacs_core::eval::VariableView;
fn read(variables: Option<VariableView<'_>>) {
    let _ = variables?.value(sym);
}
"#;
        let sites = variable_view_default_reads_in_source("planted.rs", source).expect("scan");
        assert_eq!(sites, vec!["planted.rs:4: variables.value".to_string()]);
    }

    #[test]
    fn untyped_let_producer_binding_and_direct_chain_are_red() {
        // LD-5c: (i) an untyped let fed by a producer, and (ii) a direct
        // producer chain, both reach VariableView::value without a typed
        // receiver.  The `?.` form is the fallible chain.
        let source = r#"
use neovm_core::emacs_core::eval::VariableView;
fn read(eval: &Context, widget: &Widget) {
    let vars = eval.variables();
    let _ = vars.value(sym);
    let _ = eval.variables().value(sym);
    let _ = widget.view()?.value(sym);
}
"#;
        let sites = variable_view_default_reads_in_source("chains.rs", source).expect("scan");
        assert!(
            sites.contains(&"chains.rs:5: vars.value".to_string()),
            "{sites:?}"
        );
        assert!(
            sites.contains(&"chains.rs:6: <producer>.value".to_string()),
            "{sites:?}"
        );
        assert!(
            sites.contains(&"chains.rs:7: <producer>.value".to_string()),
            "{sites:?}"
        );
        assert_eq!(sites.len(), 3, "{sites:?}");
    }

    #[test]
    fn visible_producer_chains_and_unrelated_lets_stay_green() {
        let source = r#"
use neovm_core::emacs_core::eval::VariableView;
fn read(eval: &Context, widget: &Widget) {
    let _ = eval.variables().value_in_buffer(Some(buffer), sym);
    let _ = widget.view()?.value_in_buffer(Some(buffer), sym);
    let other = compute();
    let _ = other.value(sym);
}
"#;
        let sites = variable_view_default_reads_in_source("green_chain.rs", source).expect("scan");
        assert!(sites.is_empty(), "{sites:?}");
    }

    #[test]
    fn visible_reads_and_non_view_receivers_stay_green() {
        let source = r#"
use neovm_core::emacs_core::eval::VariableView;
fn read(vars: VariableView<'_>, other: &Widget) {
    let _ = vars.value_in_buffer(Some(buffer), sym);
    let _ = other.value(sym);
}
"#;
        let sites = variable_view_default_reads_in_source("green.rs", source).expect("scan");
        assert!(sites.is_empty(), "{sites:?}");
    }

    #[test]
    fn unnameable_binding_shapes_fail_closed() {
        let source = "fn f(pair: (VariableView<'a>, VariableView<'a>)) {}";
        let error =
            variable_view_default_reads_in_source("bad.rs", source).expect_err("unnameable");
        assert!(
            error.contains("unhandled VariableView binding shape"),
            "{error}"
        );
        let error =
            variable_view_default_reads_in_source("broken.rs", "fn broken(").expect_err("parse");
        assert!(error.contains("cannot parse"), "{error}");
    }
}

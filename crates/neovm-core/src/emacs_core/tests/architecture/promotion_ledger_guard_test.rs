//! PC11 B3: the temporary bulk surface is explicit and can only shrink.
//!
//! Inputs are runtime-read, so a missing promotion ledger or ratchet is a
//! fail-closed architecture failure without obstructing a library-only check.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use syn::visit::{self, Visit};

use super::architecture_production_source_roots::workspace_root;

const LEDGER: &str = "crates/neovm-core/src/emacs_core/tests/architecture/PROMOTION-LEDGER.tsv";
const CEILING: &str =
    "crates/neovm-core/src/emacs_core/tests/architecture/PROMOTION-LEDGER.ceiling";
const BUMP_SUBJECT: &str = "PC11 PROMOTION-LEDGER ceiling bump:";

fn root() -> PathBuf {
    workspace_root()
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|e| format!("B3_MISSING_OR_UNREADABLE {}: {e}", path.display()))
}

/// Metadata resolves workspace inheritance, dependency renames and all target
/// tables. Do not select the current target: target-specific edges count too.
fn only_core_normal_dependency(metadata: &serde_json::Value) -> Result<(), String> {
    let packages = metadata
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or("B3_METADATA missing packages")?;
    let members = metadata
        .get("workspace_members")
        .and_then(serde_json::Value::as_array)
        .ok_or("B3_METADATA missing workspace members")?;
    let mut unseen: BTreeSet<String> = members
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .ok_or("B3_METADATA member id")
        })
        .collect::<Result<_, _>>()?;
    let mut has_core = false;
    let mut has_value = false;
    let mut core_edge = false;
    let mut violations = Vec::new();
    for package in packages {
        let id = package
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or("B3_METADATA package id")?;
        if !unseen.remove(id) {
            continue;
        }
        let name = package
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or("B3_METADATA package name")?;
        has_core |= name == "neovm-core";
        has_value |= name == "neovm-value";
        let deps = package
            .get("dependencies")
            .and_then(serde_json::Value::as_array)
            .ok_or("B3_METADATA dependencies")?;
        for dep in deps {
            let dep_name = dep
                .get("name")
                .and_then(serde_json::Value::as_str)
                .ok_or("B3_METADATA dependency name")?;
            let kind = dep.get("kind").ok_or("B3_METADATA dependency kind")?;
            let normal = match kind {
                serde_json::Value::Null => true,
                serde_json::Value::String(kind) if kind == "dev" || kind == "build" => false,
                other => return Err(format!("B3_METADATA unknown dependency kind {other}")),
            };
            if dep_name == "neovm-value" && normal {
                if name == "neovm-core" {
                    core_edge = true;
                } else {
                    violations.push(format!(
                        "{name}: normal dependency on neovm-value (rename {:?}, target {:?})",
                        dep.get("rename"),
                        dep.get("target")
                    ));
                }
            }
        }
    }
    if !unseen.is_empty() || !has_core || !has_value || !core_edge {
        return Err(format!(
            "B3_METADATA incomplete workspace/core edge: unseen={unseen:?}, core={has_core}, value={has_value}, core_edge={core_edge}"
        ));
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!("B3_SOLE_DEPENDENT\n{}", violations.join("\n")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Function,
    Method,
    FieldAccessor,
    Field,
    Const,
    Struct,
    Enum,
    Union,
    Type,
    Module,
    Use,
    Glob,
    Reexport,
    UnsafeFunction,
    Trait,
    ConstFunction,
    Def,
    Static,
    Pc2,
    Seal,
    MacroRules,
}

impl Kind {
    fn parse(text: &str) -> Result<Self, String> {
        match text {
            "fn" => Ok(Self::Function),
            "method" => Ok(Self::Method),
            "field-accessor" => Ok(Self::FieldAccessor),
            "field" => Ok(Self::Field),
            "const" => Ok(Self::Const),
            "struct" => Ok(Self::Struct),
            "enum" => Ok(Self::Enum),
            "union" => Ok(Self::Union),
            "type" => Ok(Self::Type),
            "mod" => Ok(Self::Module),
            "use" => Ok(Self::Use),
            "glob" => Ok(Self::Glob),
            "reexport" => Ok(Self::Reexport),
            "unsafe fn" => Ok(Self::UnsafeFunction),
            "trait" => Ok(Self::Trait),
            "const fn" => Ok(Self::ConstFunction),
            "def" => Ok(Self::Def),
            "static" => Ok(Self::Static),
            "pc2" => Ok(Self::Pc2),
            "seal" => Ok(Self::Seal),
            "macro_rules" => Ok(Self::MacroRules),
            _ => Err(format!("B3_UNKNOWN_KIND {text:?}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Identity {
    item: String,
    kind: Kind,
    file: String,
}

#[derive(Debug, Clone)]
struct LedgerRow {
    identity: Identity,
    line: String,
    module: String,
    old_vis: String,
    reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct RowCount(usize);

fn ledger(text: &str) -> Result<Vec<LedgerRow>, String> {
    let mut lines = text.lines();
    if lines.next() != Some("item\tkind\told_vis\tfile:line\tmodule\treason") {
        return Err("B3_LEDGER exact six-column header required".into());
    }
    let mut keys = BTreeSet::new();
    let mut rows = Vec::new();
    for (line, record) in lines.enumerate() {
        let fields: Vec<_> = record.split('\t').collect();
        if fields.len() != 6 || fields.iter().any(|field| field.is_empty()) {
            return Err(format!("B3_LEDGER invalid row {}", line + 2));
        }
        let (file, span) = fields[3]
            .rsplit_once(':')
            .ok_or("B3_LEDGER missing source line")?;
        if span.is_empty() {
            return Err("B3_LEDGER missing line annotation".into());
        }
        if !span.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(format!("B3_LEDGER non-numeric line annotation {span:?}"));
        }
        if Path::new(file).is_absolute()
            || Path::new(file)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err("B3_LEDGER expected workspace-relative defining file".into());
        }
        let mut item = fields[0].to_string();
        let kind = Kind::parse(fields[1])?;
        if kind == Kind::Reexport {
            for label in ["::gc_reexport", "::root_reexport"] {
                if let Some(without) = item.strip_suffix(label) {
                    item = without.to_string();
                    break;
                }
            }
            // The signed reexport identity is exported leaf + exporting file.
            // No other kind drops an owner or module-qualified item path.
            item = item
                .rsplit("::")
                .next()
                .ok_or("B3_LEDGER empty reexport leaf")?
                .into();
        }
        let identity = Identity {
            item,
            kind,
            file: file.into(),
        };
        if !keys.insert(identity.clone()) {
            return Err(format!(
                "B3_LEDGER duplicate identity at row {}: {identity:?}",
                line + 2
            ));
        }
        rows.push(LedgerRow {
            identity,
            line: span.into(),
            module: fields[4].into(),
            old_vis: fields[2].into(),
            reason: fields[5].into(),
        });
    }
    Ok(rows)
}

fn doc_hidden(attrs: &[syn::Attribute]) -> Result<bool, String> {
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let syn::Meta::List(list) = &attr.meta {
                let args = list
                    .parse_args_with(
                        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                    )
                    .map_err(|e| format!("B3_SOURCE unparseable doc metadata: {e}"))?;
                if args
                    .iter()
                    .any(|arg| matches!(arg, syn::Meta::Path(p) if p.is_ident("hidden")))
                {
                    return Ok(true);
                }
            }
        }
        if attr.path().is_ident("cfg_attr") {
            let args = attr
                .parse_args_with(
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                )
                .map_err(|e| format!("B3_SOURCE unparseable cfg_attr: {e}"))?;
            for arg in args.iter().skip(1) {
                if let syn::Meta::List(list) = arg {
                    if list.path.is_ident("doc")
                        && list
                            .tokens
                            .clone()
                            .into_iter()
                            .any(|t| matches!(t, proc_macro2::TokenTree::Ident(i) if i == "hidden"))
                    {
                        return Err(
                            "B3_SOURCE conditional doc(hidden) needs exact metadata review".into(),
                        );
                    }
                }
            }
        }
    }
    Ok(false)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SourceKind {
    Function {
        inherent: bool,
        constant: bool,
        unsafe_fn: bool,
    },
    Struct,
    Enum,
    Union,
    Alias,
    Const,
    Trait,
    Module,
    Use,
    MacroRules,
}

#[derive(Debug, Clone)]
struct Candidate {
    item: String,
    module: String,
    file: String,
    line: usize,
    kind: SourceKind,
    public: bool,
    hidden: bool,
}

impl Candidate {
    fn compatible(&self, kind: Kind) -> bool {
        match (kind, &self.kind) {
            (
                Kind::Function,
                SourceKind::Function {
                    inherent: false, ..
                },
            ) => true,
            (Kind::Method | Kind::FieldAccessor, SourceKind::Function { inherent: true, .. }) => {
                true
            }
            (Kind::ConstFunction, SourceKind::Function { constant: true, .. }) => true,
            (
                Kind::UnsafeFunction,
                SourceKind::Function {
                    unsafe_fn: true, ..
                },
            ) => true,
            (Kind::Struct, SourceKind::Struct) | (Kind::Enum, SourceKind::Enum) => true,
            (
                Kind::Type,
                SourceKind::Struct | SourceKind::Enum | SourceKind::Union | SourceKind::Alias,
            ) => true,
            // A pc2 exact-caller row names an installer/adapter item; in
            // the value bulk ledger that item is a public+doc(hidden) fn.
            (Kind::Pc2, SourceKind::Function { .. }) => true,
            (Kind::Const, SourceKind::Const)
            | (Kind::Trait, SourceKind::Trait)
            | (Kind::Module, SourceKind::Module)
            | (Kind::Use | Kind::Reexport, SourceKind::Use)
            | (Kind::MacroRules, SourceKind::MacroRules) => true,
            _ => false,
        }
    }
    fn matches_item(&self, item: &str, kind: Kind) -> bool {
        if kind == Kind::Reexport {
            return self.item == item;
        }
        item == self.item
            || item == format!("{}::{}", self.module, self.item)
            || self
                .module
                .strip_prefix("neovm_value::")
                .is_some_and(|module| item == format!("{module}::{}", self.item))
            || item.strip_prefix("crate::").is_some_and(|path| {
                format!("neovm_value::{path}") == format!("{}::{}", self.module, self.item)
            })
    }
}

#[derive(Default)]
struct HiddenAttrs {
    lines: BTreeSet<usize>,
    errors: Vec<String>,
}
impl<'ast> Visit<'ast> for HiddenAttrs {
    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        match doc_hidden(std::slice::from_ref(attr)) {
            Ok(true) => {
                if let Some(segment) = attr.path().segments.first() {
                    self.lines.insert(segment.ident.span().start().line);
                }
            }
            Ok(false) => {}
            Err(error) => self.errors.push(error),
        }
        visit::visit_attribute(self, attr);
    }
}

struct Surface<'a> {
    workspace: &'a Path,
    candidates: Vec<Candidate>,
    visited: BTreeMap<PathBuf, String>,
    uses: Vec<UseSite>,
    modules: BTreeSet<String>,
    accounted: BTreeSet<(PathBuf, usize)>,
}

#[derive(Clone)]
struct UseSite {
    file: PathBuf,
    module: String,
    line: usize,
    public: bool,
    hidden: bool,
    leaves: Vec<(Vec<String>, Option<String>)>,
    absolute: bool,
}

fn use_leaves(
    tree: &syn::UseTree,
    prefix: &[String],
    out: &mut Vec<(Vec<String>, Option<String>)>,
) {
    match tree {
        syn::UseTree::Path(p) => {
            let mut next = prefix.to_vec();
            next.push(p.ident.to_string());
            use_leaves(&p.tree, &next, out);
        }
        syn::UseTree::Name(n) => {
            let mut path = prefix.to_vec();
            if n.ident != "self" {
                path.push(n.ident.to_string());
            }
            let leaf = path.last().cloned();
            out.push((path, leaf));
        }
        syn::UseTree::Rename(n) => {
            let mut path = prefix.to_vec();
            if n.ident != "self" {
                path.push(n.ident.to_string());
            }
            out.push((path, Some(n.rename.to_string())));
        }
        syn::UseTree::Glob(_) => out.push((prefix.to_vec(), None)),
        syn::UseTree::Group(g) => {
            for item in &g.items {
                use_leaves(item, prefix, out);
            }
        }
    }
}

impl Surface<'_> {
    fn record(
        &mut self,
        file: &Path,
        module: &str,
        item: String,
        span: proc_macro2::Span,
        public: bool,
        attrs: &[syn::Attribute],
        kind: SourceKind,
    ) -> Result<(), String> {
        let file = file
            .strip_prefix(self.workspace)
            .map_err(|_| "B3_SOURCE outside workspace")?
            .to_string_lossy()
            .replace('\\', "/");
        self.candidates.push(Candidate {
            item,
            module: module.into(),
            file: file.clone(),
            line: span.start().line,
            kind,
            public,
            hidden: doc_hidden(attrs)?,
        });
        self.account_attrs(&self.workspace.join(file), attrs)?;
        Ok(())
    }
    fn account_attrs(&mut self, file: &Path, attrs: &[syn::Attribute]) -> Result<(), String> {
        for attr in attrs {
            if doc_hidden(std::slice::from_ref(attr))? {
                let first = attr
                    .path()
                    .segments
                    .first()
                    .ok_or("B3_SOURCE empty attribute path")?;
                self.accounted
                    .insert((file.to_path_buf(), first.ident.span().start().line));
            }
        }
        Ok(())
    }
    fn reject_hidden(&self, attrs: &[syn::Attribute], what: &str) -> Result<(), String> {
        if doc_hidden(attrs)? {
            Err(format!("B3_UNRESOLVED_HIDDEN unsupported {what}"))
        } else {
            Ok(())
        }
    }
    fn macro_tokens(&self, stream: proc_macro2::TokenStream) -> Result<(), String> {
        let tokens: Vec<_> = stream.into_iter().collect();
        for (index, token) in tokens.iter().enumerate() {
            if matches!(token, proc_macro2::TokenTree::Punct(p) if p.as_char() == '#') {
                if let Some(proc_macro2::TokenTree::Group(group)) = tokens.get(index + 1) {
                    if group.delimiter() == proc_macro2::Delimiter::Bracket {
                        if let Ok(syn::Meta::List(meta)) = syn::parse2::<syn::Meta>(group.stream())
                        {
                            if meta.path.is_ident("doc") && meta.tokens.into_iter().any(|t| matches!(t, proc_macro2::TokenTree::Ident(i) if i == "hidden")) {
                                return Err("B3_UNRESOLVED_HIDDEN_MACRO template identity must be source-resolved".into());
                            }
                        }
                    }
                }
            }
            if let proc_macro2::TokenTree::Group(group) = token {
                self.macro_tokens(group.stream())?;
            }
        }
        Ok(())
    }
    fn file(&mut self, file: &Path, module: &str) -> Result<(), String> {
        let file = file
            .canonicalize()
            .map_err(|e| format!("B3_SOURCE {}: {e}", file.display()))?;
        if let Some(previous) = self.visited.get(&file) {
            if previous == module {
                return Ok(());
            }
            return Err(format!(
                "B3_SOURCE ambiguous file module {}: {previous}/{module}",
                file.display()
            ));
        }
        self.visited.insert(file.clone(), module.into());
        let text = read(&file)?;
        let ast =
            syn::parse_file(&text).map_err(|e| format!("B3_SOURCE {}: {e}", file.display()))?;
        let parent = file.parent().ok_or("B3_SOURCE missing parent")?;
        let dir = if module == "neovm_value"
            || file.file_name().and_then(|s| s.to_str()) == Some("mod.rs")
        {
            parent.to_path_buf()
        } else {
            file.with_extension("")
        };
        self.items(&ast.items, &file, module, &dir, parent)?;
        let mut hidden = HiddenAttrs::default();
        hidden.visit_file(&ast);
        if !hidden.errors.is_empty() {
            return Err(hidden.errors.join("\n"));
        }
        for line in hidden.lines {
            if !self.accounted.contains(&(file.clone(), line)) {
                return Err(format!(
                    "B3_UNTRACKED_HIDDEN_ATTRIBUTE {}:{line}",
                    file.display()
                ));
            }
        }
        Ok(())
    }
    fn items(
        &mut self,
        items: &[syn::Item],
        file: &Path,
        module: &str,
        dir: &Path,
        path_base: &Path,
    ) -> Result<(), String> {
        self.modules.insert(module.into());
        for item in items {
            match item {
                syn::Item::Fn(f) => self.record(
                    file,
                    module,
                    f.sig.ident.to_string(),
                    f.sig.ident.span(),
                    matches!(f.vis, syn::Visibility::Public(_)),
                    &f.attrs,
                    SourceKind::Function {
                        inherent: false,
                        constant: f.sig.constness.is_some(),
                        unsafe_fn: matches!(f.sig.safety, syn::Safety::Unsafe(_)),
                    },
                )?,
                syn::Item::Struct(s) => {
                    self.record(
                        file,
                        module,
                        s.ident.to_string(),
                        s.ident.span(),
                        matches!(s.vis, syn::Visibility::Public(_)),
                        &s.attrs,
                        SourceKind::Struct,
                    )?;
                    for field in &s.fields {
                        self.reject_hidden(&field.attrs, "field")?;
                    }
                }
                syn::Item::Enum(e) => {
                    self.record(
                        file,
                        module,
                        e.ident.to_string(),
                        e.ident.span(),
                        matches!(e.vis, syn::Visibility::Public(_)),
                        &e.attrs,
                        SourceKind::Enum,
                    )?;
                    for variant in &e.variants {
                        self.reject_hidden(&variant.attrs, "variant")?;
                        for field in &variant.fields {
                            self.reject_hidden(&field.attrs, "field")?;
                        }
                    }
                }
                syn::Item::Union(u) => {
                    self.record(
                        file,
                        module,
                        u.ident.to_string(),
                        u.ident.span(),
                        matches!(u.vis, syn::Visibility::Public(_)),
                        &u.attrs,
                        SourceKind::Union,
                    )?;
                    for field in &u.fields.named {
                        self.reject_hidden(&field.attrs, "field")?;
                    }
                }
                syn::Item::Type(t) => self.record(
                    file,
                    module,
                    t.ident.to_string(),
                    t.ident.span(),
                    matches!(t.vis, syn::Visibility::Public(_)),
                    &t.attrs,
                    SourceKind::Alias,
                )?,
                syn::Item::Const(c) => self.record(
                    file,
                    module,
                    c.ident.to_string(),
                    c.ident.span(),
                    matches!(c.vis, syn::Visibility::Public(_)),
                    &c.attrs,
                    SourceKind::Const,
                )?,
                syn::Item::Trait(t) => {
                    self.record(
                        file,
                        module,
                        t.ident.to_string(),
                        t.ident.span(),
                        matches!(t.vis, syn::Visibility::Public(_)),
                        &t.attrs,
                        SourceKind::Trait,
                    )?;
                    for member in &t.items {
                        match member {
                            syn::TraitItem::Fn(f) => {
                                self.reject_hidden(&f.attrs, "trait member")?
                            }
                            syn::TraitItem::Const(c) => {
                                self.reject_hidden(&c.attrs, "trait const")?
                            }
                            syn::TraitItem::Type(t) => {
                                self.reject_hidden(&t.attrs, "trait type")?
                            }
                            syn::TraitItem::Macro(m) => {
                                self.reject_hidden(&m.attrs, "trait macro")?;
                                self.macro_tokens(m.mac.tokens.clone())?;
                            }
                            syn::TraitItem::Verbatim(tokens) => {
                                self.macro_tokens(tokens.clone())?
                            }
                            _ => return Err("B3_SOURCE unknown trait member".into()),
                        }
                    }
                }
                syn::Item::Impl(i) => {
                    for member in &i.items {
                        match member {
                            syn::ImplItem::Fn(f) => {
                                if i.trait_.is_some() {
                                    self.reject_hidden(&f.attrs, "trait impl method")?;
                                    continue;
                                }
                                let owner = match &*i.self_ty {
                                    syn::Type::Path(p) if p.qself.is_none() => p
                                        .path
                                        .segments
                                        .iter()
                                        .map(|s| s.ident.to_string())
                                        .collect::<Vec<_>>()
                                        .join("::"),
                                    _ => {
                                        self.reject_hidden(&f.attrs, "unresolved impl owner")?;
                                        continue;
                                    }
                                };
                                // Retain the source-declared owner, including
                                // its path; never infer an omitted owner by line.
                                self.record(
                                    file,
                                    module,
                                    format!("{owner}::{}", f.sig.ident),
                                    f.sig.ident.span(),
                                    matches!(f.vis, syn::Visibility::Public(_)),
                                    &f.attrs,
                                    SourceKind::Function {
                                        inherent: true,
                                        constant: f.sig.constness.is_some(),
                                        unsafe_fn: matches!(f.sig.safety, syn::Safety::Unsafe(_)),
                                    },
                                )?;
                            }
                            syn::ImplItem::Const(c) => {
                                if i.trait_.is_some() {
                                    self.reject_hidden(&c.attrs, "trait impl const")?;
                                    continue;
                                }
                                let owner = match &*i.self_ty {
                                    syn::Type::Path(p) if p.qself.is_none() => p
                                        .path
                                        .segments
                                        .iter()
                                        .map(|s| s.ident.to_string())
                                        .collect::<Vec<_>>()
                                        .join("::"),
                                    _ => {
                                        self.reject_hidden(&c.attrs, "unresolved const owner")?;
                                        continue;
                                    }
                                };
                                self.record(
                                    file,
                                    module,
                                    format!("{owner}::{}", c.ident),
                                    c.ident.span(),
                                    matches!(c.vis, syn::Visibility::Public(_)),
                                    &c.attrs,
                                    SourceKind::Const,
                                )?;
                            }
                            syn::ImplItem::Type(t) => {
                                self.reject_hidden(&t.attrs, "associated type")?
                            }
                            syn::ImplItem::Macro(m) => {
                                self.reject_hidden(&m.attrs, "impl macro")?;
                                self.macro_tokens(m.mac.tokens.clone())?;
                            }
                            syn::ImplItem::Verbatim(tokens) => self.macro_tokens(tokens.clone())?,
                            _ => return Err("B3_SOURCE unknown impl member".into()),
                        }
                    }
                }
                syn::Item::Mod(m) => {
                    self.record(
                        file,
                        module,
                        m.ident.to_string(),
                        m.ident.span(),
                        matches!(m.vis, syn::Visibility::Public(_)),
                        &m.attrs,
                        SourceKind::Module,
                    )?;
                    let child = format!("{module}::{}", m.ident);
                    let directory = dir.join(m.ident.to_string());
                    if let Some((_, items)) = &m.content {
                        self.items(items, file, &child, &directory, &directory)?;
                    } else {
                        let mut paths = Vec::new();
                        for attr in &m.attrs {
                            if attr.path().is_ident("path") {
                                let syn::Meta::NameValue(meta) = &attr.meta else {
                                    return Err("B3_SOURCE module path".into());
                                };
                                let syn::Expr::Lit(expr) = &meta.value else {
                                    return Err("B3_SOURCE module path expression".into());
                                };
                                let syn::Lit::Str(value) = &expr.lit else {
                                    return Err("B3_SOURCE module path string".into());
                                };
                                paths.push(path_base.join(value.value()));
                            }
                            if attr.path().is_ident("cfg_attr") {
                                let args = attr.parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated).map_err(|e| format!("B3_SOURCE cfg_attr: {e}"))?;
                                if args.iter().skip(1).any(|meta| meta.path().is_ident("path")) {
                                    return Err(
                                        "B3_SOURCE conditional module path requires review".into(),
                                    );
                                }
                            }
                        }
                        if paths.is_empty() {
                            paths = [
                                dir.join(format!("{}.rs", m.ident)),
                                directory.join("mod.rs"),
                            ]
                            .into_iter()
                            .filter(|p| p.is_file())
                            .collect();
                        }
                        if paths.len() != 1 {
                            return Err(format!("B3_SOURCE unresolved module {child}"));
                        }
                        self.file(&paths[0], &child)?;
                    }
                }
                syn::Item::Use(u) => {
                    self.account_attrs(file, &u.attrs)?;
                    let mut leaves = Vec::new();
                    use_leaves(&u.tree, &[], &mut leaves);
                    self.uses.push(UseSite {
                        file: file.into(),
                        module: module.into(),
                        line: u.use_token.span.start().line,
                        public: matches!(u.vis, syn::Visibility::Public(_)),
                        hidden: doc_hidden(&u.attrs)?,
                        leaves,
                        absolute: u.leading_colon.is_some(),
                    });
                }
                syn::Item::Macro(m) => {
                    if let Some(name) = &m.ident {
                        if m.mac.path.is_ident("macro_rules") {
                            self.record(
                                file,
                                module,
                                name.to_string(),
                                name.span(),
                                m.attrs.iter().any(|a| a.path().is_ident("macro_export")),
                                &m.attrs,
                                SourceKind::MacroRules,
                            )?;
                        } else {
                            self.reject_hidden(&m.attrs, "unrecognized macro declaration")?;
                        }
                    } else {
                        self.reject_hidden(&m.attrs, "macro invocation")?;
                    }
                    self.macro_tokens(m.mac.tokens.clone())?;
                }
                syn::Item::Static(s) => self.reject_hidden(&s.attrs, "static/TLS")?,
                syn::Item::ExternCrate(e) => self.reject_hidden(&e.attrs, "extern crate")?,
                syn::Item::ForeignMod(f) => self.reject_hidden(&f.attrs, "foreign block")?,
                syn::Item::TraitAlias(t) => self.reject_hidden(&t.attrs, "trait alias")?,
                syn::Item::Verbatim(tokens) => self.macro_tokens(tokens.clone())?,
                _ => return Err("B3_SOURCE unknown item kind".into()),
            }
        }
        Ok(())
    }
    fn use_target(&self, module: &str, path: &[String]) -> Result<String, String> {
        let mut current: Vec<_> = module.split("::").map(str::to_string).collect();
        let mut offset = 0;
        match path.first().map(String::as_str) {
            Some("crate" | "neovm_value") => {
                current = vec!["neovm_value".into()];
                offset = 1;
            }
            Some("self") => offset = 1,
            Some("super") => {
                while path.get(offset).is_some_and(|p| p == "super") {
                    if current.len() <= 1 {
                        return Err("B3_SOURCE use above crate root".into());
                    }
                    current.pop();
                    offset += 1;
                }
            }
            _ => {}
        }
        current.extend_from_slice(&path[offset..]);
        let target = current.join("::");
        if self.modules.contains(&target) {
            Ok(target)
        } else {
            Err(format!("B3_SOURCE unresolved glob target {target}"))
        }
    }
    fn expanded_exports(
        &self,
        module: &str,
        stack: &mut BTreeSet<String>,
    ) -> Result<Vec<String>, String> {
        if !stack.insert(module.into()) {
            return Err(format!("B3_SOURCE cyclic glob {module}"));
        }
        let mut names: Vec<_> = self
            .candidates
            .iter()
            .filter(|c| c.module == module && c.public && !c.item.contains("::"))
            .map(|c| c.item.clone())
            .collect();
        for site in self
            .uses
            .iter()
            .filter(|site| site.module == module && site.public)
        {
            for (path, alias) in &site.leaves {
                match alias {
                    Some(alias) => {
                        names.push(alias.clone());
                    }
                    None => {
                        if site.absolute {
                            return Err("B3_SOURCE unresolved absolute glob".into());
                        }
                        let target = self.use_target(module, path)?;
                        names.extend(self.expanded_exports(&target, &mut stack.clone())?);
                    }
                }
            }
        }
        Ok(names)
    }
    fn finish_uses(&mut self) -> Result<(), String> {
        for site in self.uses.clone() {
            for (path, alias) in site.leaves {
                // Ordinary non-hidden globs are not bulk metadata. A listed
                // row that relies on one remains missing/not-hidden and red;
                // unrelated external facade globs cannot invent candidates.
                if alias.is_none() && !site.hidden {
                    continue;
                }
                if alias.is_none() && site.absolute {
                    return Err("B3_SOURCE unresolved absolute hidden glob".into());
                }
                let names = match alias {
                    Some(alias) => vec![alias],
                    None => {
                        let target = self.use_target(&site.module, &path)?;
                        self.expanded_exports(&target, &mut BTreeSet::new())?
                            .into_iter()
                            .collect()
                    }
                };
                for name in names {
                    let file = site
                        .file
                        .strip_prefix(self.workspace)
                        .map_err(|_| "B3_SOURCE exporting file outside workspace")?
                        .to_string_lossy()
                        .replace('\\', "/");
                    self.candidates.push(Candidate {
                        item: name,
                        module: site.module.clone(),
                        file,
                        line: site.line,
                        kind: SourceKind::Use,
                        public: site.public,
                        hidden: site.hidden,
                    });
                }
            }
        }
        Ok(())
    }
}

fn source_rows(workspace: &Path) -> Result<Vec<Candidate>, String> {
    let workspace = workspace.canonicalize().map_err(|e| e.to_string())?;
    let value = workspace.join("crates/neovm-value/src");
    let mut surface = Surface {
        workspace: &workspace,
        candidates: Vec::new(),
        visited: BTreeMap::new(),
        uses: Vec::new(),
        modules: BTreeSet::new(),
        accounted: BTreeSet::new(),
    };
    surface.file(&value.join("lib.rs"), "neovm_value")?;
    fn all_rs(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                all_rs(&entry.path(), files)?;
            } else if kind.is_file() && entry.path().extension().is_some_and(|s| s == "rs") {
                files.push(entry.path());
            } else if kind.is_symlink() {
                return Err(format!(
                    "B3_SOURCE unresolved symlink {}",
                    entry.path().display()
                ));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    all_rs(&value, &mut files)?;
    for file in files {
        let path = file.canonicalize().map_err(|e| e.to_string())?;
        if !surface.visited.contains_key(&path) {
            let text = read(&path)?;
            let ast =
                syn::parse_file(&text).map_err(|e| format!("B3_SOURCE {}: {e}", path.display()))?;
            let mut hidden = HiddenAttrs::default();
            hidden.visit_file(&ast);
            if !hidden.lines.is_empty() || !hidden.errors.is_empty() {
                return Err(format!(
                    "{}: B3_SOURCE hidden metadata in unmapped module {:?} {:?}",
                    path.display(),
                    hidden.lines,
                    hidden.errors
                ));
            }
            let tokens: proc_macro2::TokenStream = text
                .parse()
                .map_err(|e| format!("B3_SOURCE token parse: {e}"))?;
            surface.macro_tokens(tokens)?;
        }
    }
    surface.finish_uses()?;
    Ok(surface.candidates)
}

fn equal_rows(ledger: &[LedgerRow], source: &[Candidate]) -> Result<(), String> {
    let mut used = BTreeSet::new();
    for row in ledger {
        let key = &row.identity;
        let matches: Vec<_> = source
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                c.file == key.file && c.compatible(key.kind) && c.matches_item(&key.item, key.kind)
            })
            .collect();
        if matches.len() != 1 {
            return Err(format!(
                "B3_LEDGER_SOURCE_MISMATCH missing/ambiguous {row:?}: {} actual candidates",
                matches.len()
            ));
        }
        let (index, candidate) = matches[0];
        if !candidate.public || !candidate.hidden {
            return Err(format!(
                "B3_LEDGER_SOURCE_MISMATCH listed definition is not public + doc(hidden): {row:?}; actual {candidate:?}"
            ));
        }
        if !used.insert(index) {
            return Err(format!(
                "B3_LEDGER_SOURCE_MISMATCH duplicate ledger aliases for one actual definition: {row:?}; {candidate:?}"
            ));
        }
    }
    for (index, candidate) in source.iter().enumerate() {
        if candidate.public && candidate.hidden && !used.contains(&index) {
            return Err(format!(
                "B3_LEDGER_SOURCE_MISMATCH unlisted public hidden definition: {candidate:?}"
            ));
        }
    }
    Ok(())
}

fn ceiling(text: &str) -> Result<RowCount, String> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err("B3_RATCHET expected one nonnegative integer".into());
    }
    text.parse()
        .map(RowCount)
        .map_err(|_| "B3_RATCHET count overflow".into())
}

fn at_most(rows: RowCount, limit: RowCount) -> Result<(), String> {
    if rows <= limit {
        Ok(())
    } else {
        Err(format!(
            "B3_RATCHET {} rows exceed ceiling {}",
            rows.0, limit.0
        ))
    }
}

fn monotonic(
    previous: RowCount,
    next: RowCount,
    subject: &str,
    changed: &[String],
    ceiling_path: &str,
) -> Result<(), String> {
    if next <= previous {
        return Ok(());
    }
    if subject.starts_with(BUMP_SUBJECT)
        && !subject[BUMP_SUBJECT.len()..].trim().is_empty()
        && changed.len() == 1
        && changed[0] == ceiling_path
    {
        return Ok(());
    }
    Err(format!(
        "B3_RATCHET silent ceiling increase {} -> {}; requires a named ceiling-only bump commit",
        previous.0, next.0
    ))
}

fn git(workspace: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(workspace)
        .args(args)
        .output()
        .map_err(|e| format!("B3_RATCHET git: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "B3_RATCHET git {:?}: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("B3_RATCHET non-UTF8 history: {e}"))
}

fn ratchet(workspace: &Path, rows: RowCount, ceiling_path: &str) -> Result<(), String> {
    let current = ceiling(&read(&workspace.join(ceiling_path))?)?;
    at_most(rows, current)?;
    let log = git(
        workspace,
        &[
            "log",
            "--first-parent",
            "--reverse",
            "--format=%H",
            "HEAD",
            "--",
            ceiling_path,
        ],
    )?;
    let revisions: Vec<_> = log.lines().filter(|line| !line.is_empty()).collect();
    if revisions.is_empty() {
        return Err("B3_RATCHET ceiling must have a tracked history anchor".into());
    }
    let mut previous = None;
    for revision in revisions {
        let value = ceiling(&git(
            workspace,
            &["show", &format!("{revision}:{ceiling_path}")],
        )?)?;
        if let Some(previous) = previous {
            let subject = git(workspace, &["show", "-s", "--format=%s", revision])?;
            let files = git(
                workspace,
                &["diff-tree", "--no-commit-id", "--name-only", "-r", revision],
            )?;
            let files: Vec<_> = files.lines().map(str::to_string).collect();
            monotonic(previous, value, subject.trim(), &files, ceiling_path)?;
        }
        previous = Some(value);
    }
    let previous = previous.ok_or("B3_RATCHET missing history anchor")?;
    if current > previous {
        return Err("B3_RATCHET uncommitted ceiling increase".into());
    }
    Ok(())
}

#[test]
fn architecture_pc11_only_core_normally_depends_on_value() {
    let output = Command::new("cargo")
        .current_dir(root())
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .expect("B3_METADATA cargo metadata");
    assert!(
        output.status.success(),
        "B3_METADATA {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata = serde_json::from_slice(&output.stdout).expect("B3_METADATA JSON");
    only_core_normal_dependency(&metadata).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn architecture_pc11_bulk_ledger_matches_public_hidden_source_both_ways() {
    let root = root();
    let ledger = ledger(&read(&root.join(LEDGER)).unwrap_or_else(|e| panic!("{e}")))
        .unwrap_or_else(|e| panic!("{e}"));
    let source = source_rows(&root).unwrap_or_else(|e| panic!("{e}"));
    equal_rows(&ledger, &source).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn architecture_pc11_bulk_ledger_ceiling_only_decreases() {
    let root = root();
    let rows = ledger(&read(&root.join(LEDGER)).unwrap_or_else(|e| panic!("{e}")))
        .unwrap_or_else(|e| panic!("{e}"));
    ratchet(&root, RowCount(rows.len()), CEILING).unwrap_or_else(|e| panic!("{e}"));
}

/// Every extracted domain keeps its own K1 ledger and ceiling ratchet. The
/// value ledger above additionally keeps its full source bijection; the other
/// domains are ratcheted here until their own bijection guards activate
/// (peer 01:3xZ item 2).
const DOMAIN_LEDGERS: &[(&str, &str)] = &[
    (LEDGER, CEILING),
    (
        "crates/neovm-core/src/emacs_core/tests/architecture/PROMOTION-LEDGER-EDIT.tsv",
        "crates/neovm-core/src/emacs_core/tests/architecture/PROMOTION-LEDGER-EDIT.ceiling",
    ),
    (
        "crates/neovm-core/src/emacs_core/tests/architecture/PROMOTION-LEDGER-TEXT.tsv",
        "crates/neovm-core/src/emacs_core/tests/architecture/PROMOTION-LEDGER-TEXT.ceiling",
    ),
    (
        "crates/neovm-core/src/emacs_core/tests/architecture/PROMOTION-LEDGER-EVAL.tsv",
        "crates/neovm-core/src/emacs_core/tests/architecture/PROMOTION-LEDGER-EVAL.ceiling",
    ),
    (
        "crates/neovm-core/src/emacs_core/tests/architecture/PROMOTION-LEDGER-DISPLAY.tsv",
        "crates/neovm-core/src/emacs_core/tests/architecture/PROMOTION-LEDGER-DISPLAY.ceiling",
    ),
    (
        "crates/neovm-core/src/emacs_core/tests/architecture/PROMOTION-LEDGER-SYSTEM.tsv",
        "crates/neovm-core/src/emacs_core/tests/architecture/PROMOTION-LEDGER-SYSTEM.ceiling",
    ),
];

#[test]
fn architecture_pc11_every_domain_ledger_ceiling_only_decreases() {
    let root = root();
    for (ledger_path, ceiling_path) in DOMAIN_LEDGERS {
        let rows =
            ledger(&read(&root.join(ledger_path)).unwrap_or_else(|e| panic!("{ledger_path}: {e}")))
                .unwrap_or_else(|e| panic!("{ledger_path}: {e}"));
        ratchet(&root, RowCount(rows.len()), ceiling_path)
            .unwrap_or_else(|e| panic!("{ceiling_path}: {e}"));
    }
}

#[test]
fn architecture_pc11_mutation_normal_dependency_is_rejected() {
    let mut metadata = serde_json::json!({"workspace_members":["core", "value", "other"], "packages":[
        {"id":"core","name":"neovm-core","dependencies":[{"name":"neovm-value","kind":null,"rename":"lower","target":"cfg(unix)"}]},
        {"id":"value","name":"neovm-value","dependencies":[]},
        {"id":"other","name":"other","dependencies":[]}
    ]});
    assert!(only_core_normal_dependency(&metadata).is_ok());
    metadata["packages"][2]["dependencies"] = serde_json::json!([{"name":"neovm-value","kind":null,"rename":"hidden_lower","target":"cfg(windows)"}]);
    let red = only_core_normal_dependency(&metadata).unwrap_err();
    assert!(
        red.contains("B3_SOLE_DEPENDENT") && red.contains("other"),
        "{red}"
    );
    metadata["packages"][2]["dependencies"] = serde_json::json!([]);
    assert!(only_core_normal_dependency(&metadata).is_ok());
}

#[test]
fn architecture_pc11_mutation_ledger_and_hidden_source_are_bijective() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("crates/neovm-value/src");
    std::fs::create_dir_all(&source).unwrap();
    let promoted = "#[doc(hidden)]\npub fn plain() {}\n";
    std::fs::write(source.join("lib.rs"), promoted).unwrap();
    let text = "item\tkind\told_vis\tfile:line\tmodule\treason\nneovm_value::plain\tfn\tpub(crate)\tcrates/neovm-value/src/lib.rs:2\tneovm_value\tbulk\n";
    let drifted = text
        .replace("lib.rs:2", "lib.rs:999")
        .replace("\tneovm_value\tbulk", "\told_facade_module\tbulk");
    let drifted_rows = ledger(&drifted).unwrap();
    assert!(
        equal_rows(&drifted_rows, &source_rows(dir.path()).unwrap()).is_ok(),
        "B3_INFORMATIONAL_LINE_AND_MODULE"
    );
    let ledger = ledger(text).unwrap();
    assert!(equal_rows(&ledger, &source_rows(dir.path()).unwrap()).is_ok());
    std::fs::write(source.join("lib.rs"), "pub fn plain() {}\n").unwrap();
    let red = equal_rows(&ledger, &source_rows(dir.path()).unwrap()).unwrap_err();
    assert!(red.contains("B3_LEDGER_SOURCE_MISMATCH"), "{red}");
    std::fs::write(
        source.join("lib.rs"),
        format!("{promoted}#[doc(hidden)]\npub fn unlisted() {{}}\n"),
    )
    .unwrap();
    let red = equal_rows(&ledger, &source_rows(dir.path()).unwrap()).unwrap_err();
    assert!(red.contains("unlisted"), "{red}");
    std::fs::write(source.join("lib.rs"), promoted).unwrap();
    assert!(equal_rows(&ledger, &source_rows(dir.path()).unwrap()).is_ok());
}

#[test]
fn architecture_pc11_mutation_silent_ceiling_raise_is_rejected() {
    assert!(at_most(RowCount(1), RowCount(1)).is_ok());
    assert!(
        at_most(RowCount(2), RowCount(1))
            .unwrap_err()
            .contains("B3_RATCHET")
    );
    assert!(
        monotonic(
            RowCount(1),
            RowCount(2),
            "unrelated change",
            &[CEILING.into()],
            CEILING,
        )
        .unwrap_err()
        .contains("silent ceiling increase")
    );
    assert!(
        monotonic(
            RowCount(1),
            RowCount(1),
            "restored",
            &[CEILING.into()],
            CEILING
        )
        .is_ok()
    );
}

#[test]
fn architecture_pc11_mutation_reexport_keys_and_source_ambiguity_are_strict() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("crates/neovm-value/src");
    std::fs::create_dir_all(&source).unwrap();
    let hidden =
        "#[doc(hidden)] pub fn plain() {}\n#[doc(hidden)] pub use self::plain as exported;\n";
    std::fs::write(source.join("lib.rs"), hidden).unwrap();
    let text = "item\tkind\told_vis\tfile:line\tmodule\treason\nplain\tfn\tpub(crate)\tcrates/neovm-value/src/lib.rs:999\told_facade\tbulk\nexported::root_reexport\treexport\tpub(crate)\tcrates/neovm-value/src/lib.rs:999\told_facade\tsigned reexport\n";
    let rows = ledger(text).unwrap();
    assert!(
        equal_rows(&rows, &source_rows(dir.path()).unwrap()).is_ok(),
        "B3_REEXPORT_EXPORTED_NAME"
    );
    let duplicate = format!(
        "{text}exported::gc_reexport\treexport\tpub(crate)\tcrates/neovm-value/src/lib.rs:1\tother\tsigned reexport\n"
    );
    assert!(
        ledger(&duplicate)
            .unwrap_err()
            .contains("duplicate identity"),
        "B3_REEXPORT_DUPLICATE_LABEL"
    );
    let normal = text.lines().take(2).collect::<Vec<_>>().join("\n") + "\n";
    std::fs::write(source.join("lib.rs"), "mod first { #[doc(hidden)] pub fn plain() {} } mod second { #[doc(hidden)] pub fn plain() {} }\n").unwrap();
    let red = equal_rows(&ledger(&normal).unwrap(), &source_rows(dir.path()).unwrap()).unwrap_err();
    assert!(red.contains("ambiguous"), "B3_SOURCE_DUPLICATE_KEY: {red}");
    std::fs::write(source.join("lib.rs"), hidden).unwrap();
    assert!(
        equal_rows(&rows, &source_rows(dir.path()).unwrap()).is_ok(),
        "B3_REEXPORT_RESTORED"
    );
}

// ---------------------------------------------------------------------------
// S-1 (peer 06:4xZ): a SEAL row's mint item may never appear in any ledger.
//
// The regression: `EvalHooksInstalled` kept its SEAL row while a K1 `mint`
// row in the same module promoted the private constructor. The rule is
// fail-closed over every promotion ledger and every TEST-SUPPORT-VIS ledger:
// a `mint` leaf in a sealed module (promotion) or sealed defining file (VIS)
// is a violation.
// ---------------------------------------------------------------------------

/// Raw-TSV scan so it works for every ledger kind spelling, independent of
/// the (stricter) `Kind` parser used by the bijection guard.
fn seal_mint_violation(promotions: &[&str], vis: &[&str]) -> Result<(), String> {
    let mut sealed_modules = BTreeSet::new();
    let mut sealed_files = BTreeSet::new();
    for text in promotions {
        for line in text.lines().skip(1) {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() != 6 {
                continue;
            }
            if fields[1] == "seal" {
                sealed_modules.insert(fields[4].to_string());
                sealed_files.insert(
                    fields[3]
                        .rsplit_once(':')
                        .map(|(file, _)| file.to_string())
                        .unwrap_or_default(),
                );
            }
        }
    }
    for (name, text) in promotions.iter().enumerate() {
        for (index, line) in text.lines().enumerate().skip(1) {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() != 6 {
                continue;
            }
            let leaf = fields[0].rsplit("::").next().unwrap_or_default();
            if leaf == "mint" && sealed_modules.contains(fields[4]) {
                return Err(format!(
                    "B3_SEAL_MINT_ROW: promotion ledger {name} line {} promotes `mint` in sealed module {}",
                    index + 2,
                    fields[4]
                ));
            }
        }
    }
    for (name, text) in vis.iter().enumerate() {
        for (index, line) in text.lines().enumerate().skip(1) {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 6 {
                continue;
            }
            let leaf = fields[1].rsplit("::").next().unwrap_or_default();
            if leaf == "mint" && sealed_files.contains(fields[3]) {
                return Err(format!(
                    "B3_SEAL_MINT_ROW: VIS ledger {name} line {} widens `mint` in sealed file {}",
                    index + 2,
                    fields[3]
                ));
            }
        }
    }
    Ok(())
}

fn read_ledger_family(prefix: &str) -> Vec<(String, String)> {
    let directory = root().join("crates/neovm-core/src/emacs_core/tests/architecture");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&directory).expect("B3 ledger directory") {
        let path = entry.expect("B3 ledger entry").path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with(prefix) && name.ends_with(".tsv") {
            out.push((
                name.to_string(),
                std::fs::read_to_string(&path).expect("B3 ledger read"),
            ));
        }
    }
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// B3 field rows (LEAD 09:0xZ / 10:1xZ): every `field` promotion row names
// `Struct.field`, and that field must exist on the named struct/union/enum in
// the defining file. The old guard had no `field` kind at all, so the whole
// PC11 ceiling test failed at parse.
// ---------------------------------------------------------------------------

fn fields_named(
    items: &[syn::Item],
    owner: &str,
    field: &str,
    owners: &mut usize,
    found: &mut bool,
) {
    for item in items {
        match item {
            syn::Item::Mod(module) => {
                if let Some((_, inner)) = &module.content {
                    fields_named(inner, owner, field, owners, found);
                }
            }
            syn::Item::Struct(item) if item.ident == owner => {
                *owners += 1;
                *found |= item
                    .fields
                    .iter()
                    .filter_map(|field| field.ident.as_ref())
                    .any(|ident| ident == field);
            }
            syn::Item::Union(item) if item.ident == owner => {
                *owners += 1;
                *found |= item
                    .fields
                    .named
                    .iter()
                    .filter_map(|field| field.ident.as_ref())
                    .any(|ident| ident == field);
            }
            syn::Item::Enum(item) if item.ident == owner => {
                *owners += 1;
                *found |= item
                    .variants
                    .iter()
                    .flat_map(|variant| variant.fields.iter())
                    .filter_map(|field| field.ident.as_ref())
                    .any(|ident| ident == field);
            }
            _ => {}
        }
    }
}

fn field_rows_violation(workspace: &Path, ledger_name: &str, text: &str) -> Result<(), String> {
    for (index, line) in text.lines().enumerate().skip(1) {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 6 || fields[1] != "field" {
            continue;
        }
        let item = fields[0];
        let Some((owner, field)) = item.split_once('.') else {
            return Err(format!(
                "B3_FIELD_UNNAMED_STRUCT: {ledger_name} line {}: field row {item:?} must spell Struct.field",
                index + 2
            ));
        };
        if owner.is_empty() || field.is_empty() || field.contains('.') {
            return Err(format!(
                "B3_FIELD_SHAPE: {ledger_name} line {}: field row {item:?} must spell exactly Struct.field",
                index + 2
            ));
        }
        let file = fields[3]
            .rsplit_once(':')
            .map(|(file, _)| file)
            .ok_or_else(|| {
                format!(
                    "B3_FIELD_SHAPE: {ledger_name} line {}: missing file:line",
                    index + 2
                )
            })?;
        let path = workspace.join(file);
        let source = std::fs::read_to_string(&path)
            .map_err(|error| format!("B3_FIELD_READ {file}: {error}"))?;
        let ast =
            syn::parse_file(&source).map_err(|error| format!("B3_FIELD_PARSE {file}: {error}"))?;
        let mut owners = 0usize;
        let mut found = false;
        fields_named(&ast.items, owner, field, &mut owners, &mut found);
        if owners == 0 {
            return Err(format!(
                "B3_FIELD_MISSING_STRUCT: {ledger_name} line {}: {owner:?} is not declared in {file}",
                index + 2
            ));
        }
        if !found {
            return Err(format!(
                "B3_FIELD_MISSING_FIELD: {ledger_name} line {}: {owner:?} in {file} has no field {field:?}",
                index + 2
            ));
        }
    }
    Ok(())
}

#[test]
fn architecture_seal_mint_items_are_never_ledger_rows() {
    let promotions = read_ledger_family("PROMOTION-LEDGER");
    let vis = read_ledger_family("TEST-SUPPORT-VIS-");
    let promotions: Vec<&str> = promotions.iter().map(|(_, text)| text.as_str()).collect();
    let vis: Vec<&str> = vis.iter().map(|(_, text)| text.as_str()).collect();
    seal_mint_violation(&promotions, &vis).unwrap_or_else(|error| panic!("{error}"));
}

#[test]
fn architecture_mutation_seal_mint_row_is_rejected() {
    let seal = "item\tkind\told_vis\tfile:line\tmodule\treason\nProof\tseal\t-\tcrates/x/src/proof.rs:10\tm::proof\tSEAL-producer\nProof\tstruct\tprivate\tcrates/x/src/proof.rs:10\tm::proof\tsealed type\n";
    let mint = "item\tkind\told_vis\tfile:line\tmodule\treason\nmint\tfn\tprivate\tcrates/x/src/proof.rs:11\tm::proof\tconstructor\n";
    let clean = "item\tkind\told_vis\tfile:line\tmodule\treason\nother\tfn\tprivate\tcrates/x/src/proof.rs:12\tm::proof\tunrelated\n";
    assert!(seal_mint_violation(&[seal, clean], &[]).is_ok());
    assert!(
        seal_mint_violation(&[seal, mint], &[])
            .unwrap_err()
            .contains("B3_SEAL_MINT_ROW")
    );
    // A mint row in an unsealed module is legal.
    let loose = "item\tkind\told_vis\tfile:line\tmodule\treason\nmint\tfn\tprivate\tcrates/x/src/other.rs:1\tm::other\tconstructor\n";
    assert!(seal_mint_violation(&[seal, loose], &[]).is_ok());
    // The VIS spelling is checked against the sealed defining file.
    let vis_header = "crate\titem\tkind\tfile\ttests\treason\n";
    let vis_mint = format!(
        "{vis_header}neovm-eval\tm::proof::mint\tfn\tcrates/x/src/proof.rs\tsome/test.rs\trow\n"
    );
    let vis_clean = format!(
        "{vis_header}neovm-eval\tm::proof::other\tfn\tcrates/x/src/proof.rs\tsome/test.rs\trow\n"
    );
    assert!(seal_mint_violation(&[seal], &[&vis_clean]).is_ok());
    assert!(
        seal_mint_violation(&[seal], &[&vis_mint])
            .unwrap_err()
            .contains("B3_SEAL_MINT_ROW")
    );
}

// ---------------------------------------------------------------------------
// B3 pc2 rows (LEAD 10:0xZ / 10:1xZ): each pc2 row names the exact callers of
// its installer/table, and those named callers are the ONLY production call
// sites. Test-only call sites are out of scope (the PC2 table tracks its own
// signed test seams).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Pc2Target {
    Name { name: String, qualifier: String },
    Method { owner: String, name: String },
}

impl Pc2Target {
    fn name(&self) -> &str {
        match self {
            Self::Name { name, .. } | Self::Method { name, .. } => name,
        }
    }
    fn matches(&self, segments: &[String], file: &str, defining_file: &str) -> bool {
        match self {
            Self::Name { name, qualifier } => {
                segments.last().is_some_and(|segment| segment == name)
                    && (file == defining_file
                        || segments[..segments.len().saturating_sub(1)]
                            .iter()
                            .any(|segment| segment == qualifier))
            }
            Self::Method { owner, name } => {
                segments.last().is_some_and(|segment| segment == name)
                    && segments[..segments.len().saturating_sub(1)]
                        .iter()
                        .any(|segment| segment == owner)
            }
        }
    }
}

fn pc2_target(item: &str, defining_file: &str) -> Pc2Target {
    if let Some((owner, name)) = item.rsplit_once("::") {
        return Pc2Target::Method {
            owner: owner.to_string(),
            name: name.to_string(),
        };
    }
    let path = Path::new(defining_file);
    let qualifier = if path.file_name().and_then(|name| name.to_str()) == Some("mod.rs") {
        path.parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string()
    } else {
        path.file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string()
    };
    Pc2Target::Name {
        name: item.to_string(),
        qualifier,
    }
}

struct Pc2Seam {
    installer: String,
    adapter: Option<String>,
    seams: Vec<(String, String)>,
}

impl Pc2Seam {
    fn covers(&self, target: &Pc2Target) -> bool {
        self.installer == target.name() || self.adapter.as_deref() == Some(target.name())
    }
    fn allows(&self, file: &str, function: Option<&str>) -> bool {
        self.seams.iter().any(|(seam_file, seam_function)| {
            file.ends_with(seam_file.trim_start_matches('/'))
                && function.is_some_and(|function| {
                    seam_function
                        .rsplit("::")
                        .next()
                        .is_some_and(|seam| seam == function)
                })
        })
    }
}

fn pc2_seams(workspace: &Path) -> Result<Vec<Pc2Seam>, String> {
    let path =
        workspace.join("crates/neovm-core/src/emacs_core/tests/architecture/pc2_hook_seams.tsv");
    let text = read(&path)?;
    let mut seams = Vec::new();
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 5 || fields[0] == "kind" {
            continue;
        }
        let last = |path: &str| path.rsplit("::").next().unwrap_or(path).to_string();
        let installer = last(fields[2]);
        let adapter = (fields[3] != "-").then(|| last(fields[3]));
        let seam_sites = if fields[4] == "-" {
            Vec::new()
        } else {
            fields[4]
                .split(';')
                .filter_map(|seam| seam.rsplit_once(':'))
                .map(|(file, function)| (file.to_string(), function.to_string()))
                .collect()
        };
        seams.push(Pc2Seam {
            installer,
            adapter,
            seams: seam_sites,
        });
    }
    Ok(seams)
}

fn pc2_declaration(items: &[syn::Item], target: &Pc2Target) -> bool {
    items.iter().any(|item| match item {
        syn::Item::Mod(module) => module
            .content
            .as_ref()
            .is_some_and(|(_, items)| pc2_declaration(items, target)),
        syn::Item::Fn(item) => match target {
            Pc2Target::Name { name, .. } => item.sig.ident == *name,
            Pc2Target::Method { .. } => false,
        },
        syn::Item::Static(item) => match target {
            Pc2Target::Name { name, .. } => item.ident == *name,
            Pc2Target::Method { .. } => false,
        },
        syn::Item::Const(item) => match target {
            Pc2Target::Name { name, .. } => item.ident == *name,
            Pc2Target::Method { .. } => false,
        },
        syn::Item::Struct(item) => match target {
            Pc2Target::Name { name, .. } => item.ident == *name,
            Pc2Target::Method { .. } => false,
        },
        syn::Item::Impl(block) => match target {
            Pc2Target::Method { owner, name } => {
                let owner_matches = match &*block.self_ty {
                    syn::Type::Path(path) => path
                        .path
                        .segments
                        .last()
                        .is_some_and(|segment| segment.ident == *owner),
                    _ => false,
                };
                owner_matches
                    && block.items.iter().any(
                        |item| matches!(item, syn::ImplItem::Fn(item) if item.sig.ident == *name),
                    )
            }
            Pc2Target::Name { .. } => false,
        },
        _ => false,
    })
}

fn pc2_gated(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if attr.path().is_ident("test") {
            return true;
        }
        if !attr.path().is_ident("cfg") {
            return false;
        }
        attr.meta
            .require_list()
            .map(|list| {
                list.tokens
                    .clone()
                    .into_iter()
                    .any(|token| matches!(token, proc_macro2::TokenTree::Ident(id) if id == "test"))
            })
            .unwrap_or(false)
    })
}

struct Pc2Scan<'a> {
    target: &'a Pc2Target,
    defining_file: &'a str,
    file: &'a str,
    stack: Vec<String>,
    references: Vec<Option<String>>,
}

impl<'ast> Visit<'ast> for Pc2Scan<'_> {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if pc2_gated(&item.attrs) {
            return;
        }
        self.stack.push(item.sig.ident.to_string());
        visit::visit_item_fn(self, item);
        self.stack.pop();
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if pc2_gated(&item.attrs) {
            return;
        }
        self.stack.push(item.sig.ident.to_string());
        visit::visit_impl_item_fn(self, item);
        self.stack.pop();
    }
    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        if pc2_gated(&module.attrs) {
            return;
        }
        visit::visit_item_mod(self, module);
    }
    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        let segments: Vec<String> = path
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        if self
            .target
            .matches(&segments, self.file, self.defining_file)
        {
            self.references.push(self.stack.last().cloned());
        }
        visit::visit_expr_path(self, path);
    }
}

fn pc2_source_files(workspace: &Path) -> Result<Vec<PathBuf>, String> {
    fn walk(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                let name = path.file_name().and_then(|name| name.to_str());
                if name == Some("tests") || name == Some("core_tests") {
                    continue;
                }
                walk(&path, files)?;
            } else if kind.is_file() && path.extension().is_some_and(|extension| extension == "rs")
            {
                // Test-gated source files are not production call sites.
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("");
                if name == "tests.rs" || name.ends_with("_test.rs") || name.ends_with("_tests.rs") {
                    continue;
                }
                files.push(path);
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    for crate_dir in [
        "neovm-value",
        "neovm-edit",
        "neovm-text",
        "neovm-eval",
        "neovm-display",
        "neovm-system",
        "neovm-core",
    ] {
        let src = workspace.join("crates").join(crate_dir).join("src");
        if src.is_dir() {
            walk(&src, &mut files)?;
        }
    }
    Ok(files)
}

fn pc2_reason_names_file(reason: &str, file: &str) -> bool {
    if reason.contains(file) {
        return true;
    }
    if let Some((_, suffix)) = file.split_once("/src/") {
        if reason.contains(suffix) {
            return true;
        }
    }
    let path = Path::new(file);
    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if stem == "mod" {
        path.parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .is_some_and(|parent| reason.contains(parent))
    } else {
        !stem.is_empty() && stem != "lib" && reason.contains(stem)
    }
}

fn pc2_rows_violation(
    workspace: &Path,
    ledger_name: &str,
    text: &str,
    seams: &[Pc2Seam],
) -> Result<(), String> {
    let files = pc2_source_files(workspace)?;
    for (index, line) in text.lines().enumerate().skip(1) {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 6 || fields[1] != "pc2" {
            continue;
        }
        let item = fields[0];
        let (defining_file, _) = fields[3].rsplit_once(':').ok_or_else(|| {
            format!(
                "B3_PC2_SHAPE: {ledger_name} line {}: missing file:line",
                index + 2
            )
        })?;
        let reason = fields[5];
        let target = pc2_target(item, defining_file);
        let source = read(&workspace.join(defining_file))?;
        let ast = syn::parse_file(&source)
            .map_err(|error| format!("B3_PC2_PARSE {defining_file}: {error}"))?;
        if !pc2_declaration(&ast.items, &target) {
            return Err(format!(
                "B3_PC2_MISSING_DECLARATION: {ledger_name} line {}: {item:?} is not declared in {defining_file}",
                index + 2
            ));
        }
        let mut unnamed = BTreeSet::new();
        for file in &files {
            let Ok(relative) = file.strip_prefix(workspace) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            let text = match std::fs::read_to_string(file) {
                Ok(text) => text,
                Err(_) => continue,
            };
            if !text.contains(target.name()) {
                continue;
            }
            let ast = syn::parse_file(&text)
                .map_err(|error| format!("B3_PC2_PARSE {relative}: {error}"))?;
            let mut scan = Pc2Scan {
                target: &target,
                defining_file,
                file: &relative,
                stack: Vec::new(),
                references: Vec::new(),
            };
            scan.visit_file(&ast);
            for function in scan.references {
                if relative == defining_file {
                    continue;
                }
                let named_file = pc2_reason_names_file(reason, &relative);
                let named_function = function
                    .as_deref()
                    .is_some_and(|function| reason.contains(function));
                let named_seam = seams
                    .iter()
                    .filter(|seam| seam.covers(&target))
                    .any(|seam| seam.allows(&relative, function.as_deref()));
                if !(named_file || named_function || named_seam) {
                    unnamed.insert(format!(
                        "{relative}:{}",
                        function.as_deref().unwrap_or("<item>")
                    ));
                }
            }
        }
        if !unnamed.is_empty() {
            return Err(format!(
                "B3_PC2_UNNAMED_CALL_SITE: {ledger_name} line {}: {item:?} has call sites the row does not name: {}",
                index + 2,
                unnamed.into_iter().collect::<Vec<_>>().join(", ")
            ));
        }
    }
    Ok(())
}

#[test]
fn architecture_pc11_field_rows_name_real_struct_fields() {
    let workspace = root();
    for (name, text) in read_ledger_family("PROMOTION-LEDGER") {
        field_rows_violation(&workspace, &name, &text).unwrap_or_else(|error| panic!("{error}"));
    }
}

#[test]
fn architecture_mutation_field_rows_reject_missing_struct_and_field() {
    let dir = tempfile::tempdir().unwrap();
    let file = "crates/neovm-value/src/lib.rs";
    std::fs::create_dir_all(dir.path().join("crates/neovm-value/src")).unwrap();
    std::fs::write(
        dir.path().join(file),
        "pub struct Thing { pub visible: u8 }\n",
    )
    .unwrap();
    let header = "item\tkind\told_vis\tfile:line\tmodule\treason\n";
    let row = |item: &str| format!("{header}{item}\tfield\tprivate\t{file}:1\tm\treason\n");
    assert!(
        field_rows_violation(dir.path(), "PROMOTION-LEDGER-X.tsv", &row("Thing.visible")).is_ok()
    );
    let error = field_rows_violation(dir.path(), "PROMOTION-LEDGER-X.tsv", &row("Thing.absent"))
        .expect_err("missing field");
    assert!(error.contains("B3_FIELD_MISSING_FIELD"), "{error}");
    let error = field_rows_violation(dir.path(), "PROMOTION-LEDGER-X.tsv", &row("Absent.visible"))
        .expect_err("missing struct");
    assert!(error.contains("B3_FIELD_MISSING_STRUCT"), "{error}");
    let error = field_rows_violation(dir.path(), "PROMOTION-LEDGER-X.tsv", &row("visible"))
        .expect_err("unnamed struct");
    assert!(error.contains("B3_FIELD_UNNAMED_STRUCT"), "{error}");
    // A type declared in an inline module is still in the defining file.
    std::fs::write(
        dir.path().join(file),
        "mod nested { pub struct Deep { pub inner: u8 } }\n",
    )
    .unwrap();
    assert!(field_rows_violation(dir.path(), "PROMOTION-LEDGER-X.tsv", &row("Deep.inner")).is_ok());
}

#[test]
fn architecture_pc11_pc2_rows_name_all_call_sites() {
    let workspace = root();
    let seams = pc2_seams(&workspace).unwrap_or_else(|error| panic!("{error}"));
    for (name, text) in read_ledger_family("PROMOTION-LEDGER") {
        pc2_rows_violation(&workspace, &name, &text, &seams)
            .unwrap_or_else(|error| panic!("{error}"));
    }
}

#[test]
fn architecture_mutation_pc2_unnamed_call_site_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("crates/neovm-value/src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("installer_mod.rs"), "pub fn installer() {}\n").unwrap();
    std::fs::write(
        src.join("probe.rs"),
        "fn named_caller() { crate::installer_mod::installer(); }\nfn secret_caller() { crate::installer_mod::installer(); }\n",
    )
    .unwrap();
    let header = "item\tkind\told_vis\tfile:line\tmodule\treason\n";
    let row = |reason: &str| {
        format!(
            "{header}installer\tpc2\t-\tcrates/neovm-value/src/installer_mod.rs:1\tm\t{reason}\n"
        )
    };
    let error = pc2_rows_violation(
        dir.path(),
        "PROMOTION-LEDGER-X.tsv",
        &row("exact caller named_caller only"),
        &[],
    )
    .expect_err("unnamed call site");
    assert!(
        error.contains("B3_PC2_UNNAMED_CALL_SITE") && error.contains("secret_caller"),
        "{error}"
    );
    assert!(
        pc2_rows_violation(
            dir.path(),
            "PROMOTION-LEDGER-X.tsv",
            &row("exact callers named_caller and secret_caller"),
            &[],
        )
        .is_ok()
    );
    // A row for a missing declaration is red.
    std::fs::write(
        src.join("probe.rs"),
        "fn named_caller() { crate::installer_mod::installer(); }\n",
    )
    .unwrap();
    let missing = format!(
        "{header}absent\tpc2\t-\tcrates/neovm-value/src/installer_mod.rs:1\tm\texact caller named_caller\n"
    );
    let error = pc2_rows_violation(dir.path(), "PROMOTION-LEDGER-X.tsv", &missing, &[])
        .expect_err("missing declaration");
    assert!(error.contains("B3_PC2_MISSING_DECLARATION"), "{error}");
}

#[test]
fn architecture_pc11_every_promotion_ledger_parses() {
    for (name, text) in read_ledger_family("PROMOTION-LEDGER") {
        ledger(&text).unwrap_or_else(|error| panic!("{name}: {error}"));
    }
}

#[test]
fn architecture_mutation_promotion_kinds_are_exact() {
    let header = "item\tkind\told_vis\tfile:line\tmodule\treason\n";
    let ok = format!(
        "{header}Thing.field\tfield\tprivate\tcrates/x/src/a.rs:1\tm\tr\n\
         P\tpc2\t-\tcrates/x/src/a.rs:2\tm\tr\n\
         D\tdef\tpub(crate)\tcrates/x/src/a.rs:3\tm\tr\n\
         G\tglob\tpub(crate)\tcrates/x/src/a.rs:4\tm\tr\n\
         S\tstatic\tprivate\tcrates/x/src/a.rs:5\tm\tr\n\
         U\tunion\tprivate\tcrates/x/src/a.rs:6\tm\tr\n\
         TheSeal\tseal\t-\tcrates/x/src/a.rs:7\tm\tr\n"
    );
    assert_eq!(ledger(&ok).unwrap().len(), 7);
    let bad = ok.replace("\tpc2\t", "\tbogus\t");
    assert!(ledger(&bad).unwrap_err().contains("B3_UNKNOWN_KIND"));
    let bad = ok.replace(":5\t", ":five\t");
    assert!(ledger(&bad).unwrap_err().contains("non-numeric line"));
}

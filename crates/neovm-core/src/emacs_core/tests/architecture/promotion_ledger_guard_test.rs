//! PC11 B3: the temporary bulk surface is explicit and can only shrink.
//!
//! Inputs are runtime-read, so a missing promotion ledger or ratchet is a
//! fail-closed architecture failure without obstructing a library-only check.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use syn::parse::Parser;
use syn::visit::{self, Visit};

use super::architecture_production_source_roots::workspace_root;

const LEDGER: &str = "crates/neovm-core/src/emacs_core/tests/architecture/PROMOTION-LEDGER.tsv";
const CEILING: &str =
    "crates/neovm-core/src/emacs_core/tests/architecture/PROMOTION-LEDGER.ceiling";
const EVAL_CEILING: &str =
    "crates/neovm-core/src/emacs_core/tests/architecture/PROMOTION-LEDGER-EVAL.ceiling";
const BUMP_SUBJECT: &str = "PC11 PROMOTION-LEDGER ceiling bump:";

fn root() -> PathBuf {
    workspace_root()
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|e| format!("B3_MISSING_OR_UNREADABLE {}: {e}", path.display()))
}

/// LEAD 11:0xZ ruling (a): the stale "only core normally depends on value"
/// invariant is replaced by the signed LAYERING-DAG check. The seven moved
/// crates form a total order; a normal dependency edge between them may only
/// go DOWNWARD. Workspace crates outside the seven reach moved crates only
/// through neovm-core. The exception table is empty. Metadata resolves
/// workspace inheritance, dependency renames and all target tables; do not
/// select the current target, since target-specific edges count too.
const LAYERS: &[&str] = &[
    "neovm-value",
    "neovm-edit",
    "neovm-text",
    "neovm-eval",
    "neovm-display",
    "neovm-system",
    "neovm-core",
];

fn layering_violation(metadata: &serde_json::Value) -> Result<(), String> {
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
    let mut seen_layers = BTreeSet::new();
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
        let layer = LAYERS.iter().position(|layer| *layer == name);
        if let Some(layer) = layer {
            seen_layers.insert(layer);
        }
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
            if !normal {
                continue;
            }
            let Some(dep_layer) = LAYERS.iter().position(|layer| *layer == dep_name) else {
                continue;
            };
            match layer {
                Some(layer) => {
                    if dep_layer >= layer {
                        violations.push(format!(
                            "{name} -> {dep_name}: moved-crate edge is not downward"
                        ));
                    }
                }
                None => {
                    if dep_name != "neovm-core" {
                        violations.push(format!(
                            "{name} -> {dep_name}: outside crates reach moved crates only through neovm-core"
                        ));
                    }
                }
            }
        }
    }
    if !unseen.is_empty() || seen_layers.len() != LAYERS.len() {
        return Err(format!(
            "B3_METADATA incomplete workspace/layers: unseen={unseen:?}, layers={seen_layers:?}"
        ));
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!("B3_LAYERING\n{}", violations.join("\n")))
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

/// LEAD 11:0xZ ruling (b): public `#[doc(hidden)]` items that only exist as
/// macro expansions are invisible to the syn item walk, so a template that
/// generates them cannot be scanned as ordinary items. This is the closed,
/// fail-closed list of item-level macros whose templates may generate such
/// items. An entry binds the macro to the fingerprint of its reviewed template
/// and to the exact doc(hidden) product identities it generates. Any template
/// edit changes the fingerprint and fails closed until the entry is
/// re-reviewed; an invocation whose arguments are not the recorded identifiers
/// and an invocation outside the defining file fail closed as well. The
/// declared products are materialized as source candidates per invocation so
/// the ledger bijection sees generated items rather than silently missing
/// them.
struct MacroProduct {
    file: &'static str,
    macro_name: &'static str,
    /// FNV-1a 64 over `TokenStream::to_string()` of the reviewed macro body.
    template_fingerprint: u64,
    params: &'static [&'static str],
    /// `(identity template, kind)` for every public doc(hidden) product;
    /// `{0}`, `{1}`, ... are the invocation identifiers in `params` order.
    products: &'static [(&'static str, SourceKind)],
}

const MACRO_PRODUCTS: &[MacroProduct] = &[
    MacroProduct {
        file: "crates/neovm-value/src/heap_types/threading.rs",
        macro_name: "native_id",
        template_fingerprint: 0x7859_0052_9bd7_07a6,
        params: &["name"],
        products: &[
            ("{0}", SourceKind::Struct),
            (
                "{0}::heap_identity",
                SourceKind::Function {
                    inherent: true,
                    constant: true,
                    unsafe_fn: false,
                },
            ),
            (
                "{0}::native_key",
                SourceKind::Function {
                    inherent: true,
                    constant: true,
                    unsafe_fn: false,
                },
            ),
        ],
    },
    MacroProduct {
        file: "crates/neovm-value/src/tagged/gc/mutator_registration/wrappers.rs",
        macro_name: "era_operations",
        // PC11 re-derivation (R4 fixup, lead #652/#654): R4 converted the
        // `symbol_table_admission_charge_for_test` method's cfg from bare
        // `test` to `any(test, feature = "test-support")`, which changes the
        // reviewed template's tokens.  The doc(hidden) products below are
        // unchanged (that method is pub(crate), not a product); the pinned
        // fingerprint is re-derived from the reviewed template.
        template_fingerprint: 0xbded_6d03_3d28_96b4,
        params: &["era", "failure"],
        products: &[
            (
                "{0}::collect_stopped_with",
                SourceKind::Function {
                    inherent: true,
                    constant: false,
                    unsafe_fn: true,
                },
            ),
            (
                "{0}::collect_stopped_minor_with",
                SourceKind::Function {
                    inherent: true,
                    constant: false,
                    unsafe_fn: true,
                },
            ),
            (
                "{0}::finish",
                SourceKind::Function {
                    inherent: true,
                    constant: false,
                    unsafe_fn: false,
                },
            ),
        ],
    },
];

fn fnv1a64(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

fn macro_products_for(file: &str, macro_name: &str) -> Option<(usize, &'static MacroProduct)> {
    MACRO_PRODUCTS
        .iter()
        .enumerate()
        .find(|(_, product)| product.file == file && product.macro_name == macro_name)
}

/// Returns the table index for a macro definition this table covers, or
/// `None` when the definition is not a table macro. A covered macro whose
/// reviewed template no longer matches fails closed.
fn verify_macro_definition(
    file: &str,
    macro_name: &str,
    tokens: &proc_macro2::TokenStream,
) -> Result<Option<usize>, String> {
    let Some((index, product)) = macro_products_for(file, macro_name) else {
        return Ok(None);
    };
    let actual = fnv1a64(&tokens.to_string());
    if actual != product.template_fingerprint {
        return Err(format!(
            "B3_MACRO_PRODUCTS {} `{macro_name}!` template changed (fingerprint {actual:#018x}, table {:#018x}); re-review its doc(hidden) products and update MACRO_PRODUCTS",
            product.file, product.template_fingerprint
        ));
    }
    Ok(Some(index))
}

/// Materializes a table macro's products for one invocation. The invocation
/// must pass exactly the recorded number of identifier arguments.
fn macro_product_identities(
    product: &MacroProduct,
    args: &[String],
) -> Result<Vec<(String, SourceKind)>, String> {
    if args.len() != product.params.len() {
        return Err(format!(
            "B3_MACRO_PRODUCTS {}! expects {} identifier argument(s) {:?}, got {}",
            product.macro_name,
            product.params.len(),
            product.params,
            args.len()
        ));
    }
    let mut products = Vec::new();
    for (identity, kind) in product.products {
        let mut item = (*identity).to_string();
        for (index, arg) in args.iter().enumerate() {
            item = item.replace(&format!("{{{index}}}"), arg);
        }
        if item.contains('{') {
            return Err(format!(
                "B3_MACRO_PRODUCTS {}! product {identity:?} has an unbound placeholder",
                product.macro_name
            ));
        }
        products.push((item, kind.clone()));
    }
    Ok(products)
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
    /// Files reachable only through a definitively test-only `mod`
    /// declaration: not production code, not scanned, and skipped by the
    /// unmapped-file fail-closed pass.
    gated_files: BTreeSet<PathBuf>,
    uses: Vec<UseSite>,
    modules: BTreeSet<String>,
    accounted: BTreeSet<(PathBuf, usize)>,
    verified_macro_products: BTreeSet<usize>,
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

/// B3-d: the attribute list of a top-level item, for the production gate.
fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Const(item) => &item.attrs,
        syn::Item::Enum(item) => &item.attrs,
        syn::Item::ExternCrate(item) => &item.attrs,
        syn::Item::Fn(item) => &item.attrs,
        syn::Item::ForeignMod(item) => &item.attrs,
        syn::Item::Impl(item) => &item.attrs,
        syn::Item::Macro(item) => &item.attrs,
        syn::Item::Mod(item) => &item.attrs,
        syn::Item::Static(item) => &item.attrs,
        syn::Item::Struct(item) => &item.attrs,
        syn::Item::Trait(item) => &item.attrs,
        syn::Item::TraitAlias(item) => &item.attrs,
        syn::Item::Type(item) => &item.attrs,
        syn::Item::Union(item) => &item.attrs,
        syn::Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

fn trait_item_attrs(item: &syn::TraitItem) -> &[syn::Attribute] {
    match item {
        syn::TraitItem::Const(item) => &item.attrs,
        syn::TraitItem::Fn(item) => &item.attrs,
        syn::TraitItem::Type(item) => &item.attrs,
        syn::TraitItem::Macro(item) => &item.attrs,
        _ => &[],
    }
}

fn impl_item_attrs(item: &syn::ImplItem) -> &[syn::Attribute] {
    match item {
        syn::ImplItem::Const(item) => &item.attrs,
        syn::ImplItem::Fn(item) => &item.attrs,
        syn::ImplItem::Type(item) => &item.attrs,
        syn::ImplItem::Macro(item) => &item.attrs,
        _ => &[],
    }
}

/// Resolve a non-inline module declaration's file path(s). Returns whether a
/// `#[path]` attribute supplied the location; the child directory of such a
/// module is derived beside the referenced file.
fn module_paths(
    module: &syn::ItemMod,
    dir: &Path,
    path_base: &Path,
) -> Result<(Vec<PathBuf>, bool), String> {
    let mut paths = Vec::new();
    let mut path_attr = false;
    for attr in &module.attrs {
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
            path_attr = true;
        }
        if attr.path().is_ident("cfg_attr") {
            let args = attr
                .parse_args_with(
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                )
                .map_err(|e| format!("B3_SOURCE cfg_attr: {e}"))?;
            if args.iter().skip(1).any(|meta| meta.path().is_ident("path")) {
                return Err("B3_SOURCE conditional module path requires review".into());
            }
        }
    }
    if paths.is_empty() {
        let name = module.ident.to_string();
        paths = [
            dir.join(format!("{name}.rs")),
            dir.join(&name).join("mod.rs"),
        ]
        .into_iter()
        .filter(|path| path.is_file())
        .collect();
    }
    Ok((paths, path_attr))
}

impl Surface<'_> {
    fn relative(&self, file: &Path) -> Result<String, String> {
        Ok(file
            .strip_prefix(self.workspace)
            .map_err(|_| format!("B3_SOURCE outside workspace: {}", file.display()))?
            .to_string_lossy()
            .replace('\\', "/"))
    }
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
        let file = self.relative(file)?;
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
    /// B3-d: account the doc(hidden) attributes inside a skipped test-only
    /// item so the file's hidden-metadata pass still proves every hidden
    /// attribute belongs to a scanned or explicitly test-only item.
    fn account_gated_with(
        &mut self,
        file: &Path,
        visit: impl FnOnce(&mut HiddenAttrs),
    ) -> Result<(), String> {
        let mut hidden = HiddenAttrs::default();
        visit(&mut hidden);
        if !hidden.errors.is_empty() {
            return Err(hidden.errors.join("\n"));
        }
        for line in hidden.lines {
            self.accounted.insert((file.to_path_buf(), line));
        }
        Ok(())
    }
    /// B3-d: collect every file reachable only through a test-gated module
    /// declaration so the unmapped-file fail-closed pass skips test-only code.
    fn collect_gated_module(
        &mut self,
        module: &syn::ItemMod,
        dir: &Path,
        path_base: &Path,
    ) -> Result<(), String> {
        if let Some((_, items)) = &module.content {
            let inline_dir = dir.join(module.ident.to_string());
            return self.collect_gated_items(items, &inline_dir, &inline_dir);
        }
        let (paths, path_attr) = module_paths(module, dir, path_base)?;
        if paths.len() != 1 {
            return Err(format!(
                "B3_SOURCE unresolved test-only module {}",
                module.ident
            ));
        }
        self.collect_gated_file(&paths[0], path_attr, dir)
    }
    fn collect_gated_items(
        &mut self,
        items: &[syn::Item],
        dir: &Path,
        path_base: &Path,
    ) -> Result<(), String> {
        for item in items {
            let syn::Item::Mod(module) = item else {
                continue;
            };
            if let Some((_, inner)) = &module.content {
                let inline_dir = dir.join(module.ident.to_string());
                self.collect_gated_items(inner, &inline_dir, &inline_dir)?;
            } else {
                let (paths, path_attr) = module_paths(module, dir, path_base)?;
                if paths.len() != 1 {
                    return Err(format!(
                        "B3_SOURCE unresolved test-only module {}",
                        module.ident
                    ));
                }
                self.collect_gated_file(&paths[0], path_attr, dir)?;
            }
        }
        Ok(())
    }
    fn collect_gated_file(
        &mut self,
        file: &Path,
        path_attr: bool,
        parent_dir: &Path,
    ) -> Result<(), String> {
        let canonical = file
            .canonicalize()
            .map_err(|e| format!("B3_SOURCE {}: {e}", file.display()))?;
        if !self.gated_files.insert(canonical.clone()) {
            return Ok(());
        }
        let text = read(&canonical)?;
        let ast = syn::parse_file(&text)
            .map_err(|e| format!("B3_SOURCE {}: {e}", canonical.display()))?;
        let child_dir = if path_attr
            || canonical.file_name().and_then(|name| name.to_str()) == Some("mod.rs")
        {
            canonical.parent().unwrap_or(parent_dir).to_path_buf()
        } else {
            canonical.with_extension("")
        };
        self.collect_gated_items(
            &ast.items,
            &child_dir,
            canonical.parent().unwrap_or(parent_dir),
        )
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
    /// Materializes the doc(hidden) products of a table macro invocation.
    /// Invocations outside the macro's defining file, qualified paths to an
    /// unexported `macro_rules!` name, and argument shapes other than the
    /// recorded identifier list fail closed.
    fn expand_macro_invocation(
        &mut self,
        relative: &str,
        module: &str,
        invocation: &syn::Macro,
    ) -> Result<(), String> {
        let Some(segment) = invocation.path.segments.last() else {
            return Ok(());
        };
        let invoked = segment.ident.to_string();
        let Some((_, product)) = macro_products_for(relative, &invoked) else {
            if MACRO_PRODUCTS
                .iter()
                .any(|product| product.macro_name == invoked)
            {
                return Err(format!(
                    "B3_MACRO_PRODUCTS {relative} invokes `{invoked}!` outside its defining file; invocation location requires review"
                ));
            }
            return Ok(());
        };
        if invocation.path.segments.len() != 1 {
            return Err(format!(
                "B3_MACRO_PRODUCTS {relative} `{invoked}!` invocation must use its bare macro_rules! name"
            ));
        }
        let parsed = syn::punctuated::Punctuated::<syn::Ident, syn::Token![,]>::parse_terminated
            .parse2(invocation.tokens.clone())
            .map_err(|e| {
                format!(
                    "B3_MACRO_PRODUCTS {relative} `{invoked}!` arguments must be identifiers: {e}"
                )
            })?;
        let args: Vec<String> = parsed.iter().map(ToString::to_string).collect();
        let products = macro_product_identities(product, &args)?;
        let line = segment.ident.span().start().line;
        for (item, kind) in products {
            self.candidates.push(Candidate {
                item,
                module: module.into(),
                file: relative.into(),
                line,
                kind,
                public: true,
                hidden: true,
            });
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
        self.items(&ast.items, &file, module, &dir, parent)
            .map_err(|e| format!("{}: {e}", file.display()))?;
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
            // B3-d: PROMOTION records PRODUCTION promotions only. An item
            // whose cfg is definitively FALSE in the production environment is
            // test-only: it is not a candidate, its doc(hidden) metadata is
            // accounted so the file's hidden pass stays meaningful, and a
            // test-only module's files are not scanned at all.
            if production_gated(item_attrs(item)) {
                self.account_gated_with(file, |hidden| hidden.visit_item(item))?;
                if let syn::Item::Mod(module) = item {
                    self.collect_gated_module(module, dir, path_base)?;
                }
                continue;
            }
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
                        if production_gated(&field.attrs) {
                            self.account_gated_with(file, |hidden| hidden.visit_field(field))?;
                        } else {
                            self.reject_hidden(&field.attrs, "field")?;
                        }
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
                        if production_gated(&variant.attrs) {
                            self.account_gated_with(file, |hidden| hidden.visit_variant(variant))?;
                            continue;
                        }
                        self.reject_hidden(&variant.attrs, "variant")?;
                        for field in &variant.fields {
                            if production_gated(&field.attrs) {
                                self.account_gated_with(file, |hidden| hidden.visit_field(field))?;
                            } else {
                                self.reject_hidden(&field.attrs, "field")?;
                            }
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
                        if production_gated(&field.attrs) {
                            self.account_gated_with(file, |hidden| hidden.visit_field(field))?;
                        } else {
                            self.reject_hidden(&field.attrs, "field")?;
                        }
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
                        if production_gated(trait_item_attrs(member)) {
                            self.account_gated_with(file, |hidden| {
                                hidden.visit_trait_item(member)
                            })?;
                            continue;
                        }
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
                        if production_gated(impl_item_attrs(member)) {
                            self.account_gated_with(file, |hidden| hidden.visit_impl_item(member))?;
                            continue;
                        }
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
                        let (paths, _) = module_paths(m, dir, path_base)?;
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
                    let relative = self.relative(file)?;
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
                            match verify_macro_definition(
                                &relative,
                                &name.to_string(),
                                &m.mac.tokens,
                            )? {
                                Some(index) => {
                                    self.verified_macro_products.insert(index);
                                }
                                None => self.macro_tokens(m.mac.tokens.clone())?,
                            }
                        } else {
                            self.reject_hidden(&m.attrs, "unrecognized macro declaration")?;
                            self.macro_tokens(m.mac.tokens.clone())?;
                        }
                    } else {
                        self.reject_hidden(&m.attrs, "macro invocation")?;
                        self.expand_macro_invocation(&relative, module, &m.mac)?;
                        self.macro_tokens(m.mac.tokens.clone())?;
                    }
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
        // The ledger identity is (item, kind, file); a file may hide the same
        // re-exported leaf through more than one use/glob site, and a second
        // candidate with the same identity would make every row for it
        // ambiguous. Collapse those sites into one candidate, promoting the
        // visibility flags, so the identity stays single-valued exactly like
        // the ledger's own duplicate-identity rejection.
        let mut emitted: BTreeMap<(String, String), usize> = BTreeMap::new();
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
                    let key = (name.clone(), file.clone());
                    if let Some(&index) = emitted.get(&key) {
                        let existing = &mut self.candidates[index];
                        existing.public |= site.public;
                        existing.hidden |= site.hidden;
                        continue;
                    }
                    self.candidates.push(Candidate {
                        item: name,
                        module: site.module.clone(),
                        file,
                        line: site.line,
                        kind: SourceKind::Use,
                        public: site.public,
                        hidden: site.hidden,
                    });
                    emitted.insert(key, self.candidates.len() - 1);
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
        gated_files: BTreeSet::new(),
        uses: Vec::new(),
        modules: BTreeSet::new(),
        accounted: BTreeSet::new(),
        verified_macro_products: BTreeSet::new(),
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
        // B3-d: a file reachable only through a test-only module declaration
        // is not production code; it is not an unmapped-module failure.
        if surface.gated_files.contains(&path) {
            continue;
        }
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
                .map_err(|e| format!("B3_SOURCE {} token parse: {e}", path.display()))?;
            surface
                .macro_tokens(tokens)
                .map_err(|e| format!("B3_SOURCE {}: {e}", path.display()))?;
        }
    }
    surface.finish_uses()?;
    for (index, product) in MACRO_PRODUCTS.iter().enumerate() {
        if workspace.join(product.file).is_file()
            && !surface.verified_macro_products.contains(&index)
        {
            return Err(format!(
                "B3_MACRO_PRODUCTS {} `{}!` definition was not scanned; the closed table entry requires source review",
                product.file, product.macro_name
            ));
        }
    }
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

/// LEAD 11:0xZ ruling (c): historical ceiling bumps that were not
/// ceiling-file-only are grandfathered by exact (ceiling path, revision).
/// No history rewrite: the value-ledger entry is the SHA the ruling named;
/// the EVAL entries are the sixteen pre-convention stage-4 move commits
/// (79 -> 80 at 6654f82bc5 through 137 -> 140 at 34aaf7f206) that carried
/// source, ledger rows and the ceiling together before the ceiling-only
/// convention existed. These are the complete violation set from the
/// first-parent ceiling histories; every later revision still needs a named
/// ceiling-only bump commit, and the path key keeps one ledger's exemption
/// from excusing another.
const GRANDFATHERED_CEILING_BUMPS: &[(&str, &str)] = &[
    (CEILING, "c4025399e3d9a6838923b06fb7d553ae07019c87"),
    (EVAL_CEILING, "6654f82bc57d49f182fdcf0839e6bb046fb4a654"),
    (EVAL_CEILING, "588c8f1107f06dfb5a55dcac4204bdef2042b0bd"),
    (EVAL_CEILING, "53870fd1405f8805639c69597ad5641c6938d1e1"),
    (EVAL_CEILING, "a15b88ebe5ad58b5190d9936348d32fc60a27005"),
    (EVAL_CEILING, "660475878caba80a1a5f021763995fa4b5044b8b"),
    (EVAL_CEILING, "2ad2e0c2032245e4937d8c9f90513c09f19429ce"),
    (EVAL_CEILING, "0512e2de1153ecccd3bba7aaf69a5bf9adfdda62"),
    (EVAL_CEILING, "80cafc865a0370cab65f5fcb1bca14cd051bd66a"),
    (EVAL_CEILING, "de58f774b49d97457b340be2eacbe1a62413e75a"),
    (EVAL_CEILING, "f01a9631c2b919058500fe9131ef098922f819a0"),
    (EVAL_CEILING, "a790aff9b6305eaff96d5134108ff9107b0f81b8"),
    (EVAL_CEILING, "18b2b078a3ffc86b33d9f1d65f037890a8462209"),
    (EVAL_CEILING, "aca933c0cc05881ced475fafa103e437e26b45e5"),
    (EVAL_CEILING, "23ce70e64442faff5e8bcb15abbe886fed79bf14"),
    (EVAL_CEILING, "fa0b0c9f60f80592ecf2e5536d4d7c093e9c92af"),
    (EVAL_CEILING, "34aaf7f206a4fdba2782b7be1d56c5500073e5ba"),
];

fn monotonic(
    previous: RowCount,
    next: RowCount,
    subject: &str,
    changed: &[String],
    ceiling_path: &str,
    revision: &str,
) -> Result<(), String> {
    if next <= previous {
        return Ok(());
    }
    if GRANDFATHERED_CEILING_BUMPS
        .iter()
        .any(|(path, sha)| *path == ceiling_path && *sha == revision)
    {
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
            monotonic(
                previous,
                value,
                subject.trim(),
                &files,
                ceiling_path,
                revision,
            )?;
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
fn architecture_pc11_layering_dag_is_downward() {
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
    layering_violation(&metadata).unwrap_or_else(|e| panic!("{e}"));
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
fn architecture_pc11_mutation_layering_dag_is_strict() {
    fn deps(names: &[&str]) -> serde_json::Value {
        serde_json::Value::Array(
            names
                .iter()
                .map(|name| serde_json::json!({"name": name, "kind": null}))
                .collect(),
        )
    }
    fn package(name: &str, dependencies: serde_json::Value) -> serde_json::Value {
        serde_json::json!({"id": name, "name": name, "dependencies": dependencies})
    }
    let mut members: Vec<&str> = LAYERS.to_vec();
    members.push("outside");
    let mut metadata = serde_json::json!({"workspace_members": members, "packages": [
        package("neovm-value", deps(&[])),
        package("neovm-edit", deps(&["neovm-value"])),
        package("neovm-text", deps(&["neovm-value", "neovm-edit"])),
        package("neovm-eval", deps(&["neovm-value", "neovm-edit", "neovm-text"])),
        package("neovm-display", deps(&["neovm-value", "neovm-edit", "neovm-text", "neovm-eval"])),
        package("neovm-system", deps(&["neovm-value", "neovm-edit", "neovm-text", "neovm-eval", "neovm-display"])),
        package("neovm-core", deps(&["neovm-value", "neovm-edit", "neovm-text", "neovm-eval", "neovm-display", "neovm-system"])),
        package("outside", deps(&["neovm-core"])),
    ]});
    assert!(layering_violation(&metadata).is_ok());
    // Upward edge: eval -> display.
    metadata["packages"][3]["dependencies"] =
        deps(&["neovm-value", "neovm-edit", "neovm-text", "neovm-display"]);
    let red = layering_violation(&metadata).unwrap_err();
    assert!(
        red.contains("B3_LAYERING") && red.contains("neovm-eval -> neovm-display"),
        "{red}"
    );
    metadata["packages"][3]["dependencies"] = deps(&["neovm-value", "neovm-edit", "neovm-text"]);
    assert!(layering_violation(&metadata).is_ok());
    // An outside crate may not skip core.
    metadata["packages"][7]["dependencies"] = deps(&["neovm-eval"]);
    let red = layering_violation(&metadata).unwrap_err();
    assert!(red.contains("only through neovm-core"), "{red}");
    metadata["packages"][7]["dependencies"] = deps(&["neovm-core"]);
    assert!(layering_violation(&metadata).is_ok());
    // A lateral (self) edge is rejected.
    metadata["packages"][0]["dependencies"] = deps(&["neovm-value"]);
    let red = layering_violation(&metadata).unwrap_err();
    assert!(red.contains("not downward"), "{red}");
    metadata["packages"][0]["dependencies"] = deps(&[]);
    assert!(layering_violation(&metadata).is_ok());
    // A dev edge in the "wrong" direction is not a layering edge.
    metadata["packages"][0]["dependencies"] =
        serde_json::json!([{"name": "neovm-core", "kind": "dev"}]);
    assert!(layering_violation(&metadata).is_ok());
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
fn architecture_mutation_b3d_promotion_is_production_only() {
    // B3-d: PROMOTION ledgers hold PRODUCTION promotions only. The scan
    // routes by cfg in the production environment (test=false,
    // feature "test-support"=false): a production `#[cfg(not(test))]` item is
    // required, while `#[cfg(test)]` and `#[cfg(any(test, feature =
    // "test-support"))]` items and test-only module files are out of scope.
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("crates/neovm-value/src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(
        source.join("lib.rs"),
        "#[doc(hidden)]\npub fn prod_row() {}\n\
         #[cfg(not(test))]\n#[doc(hidden)]\npub fn prod_arm() {}\n\
         #[cfg(test)]\n#[doc(hidden)]\npub fn test_only() {}\n\
         #[cfg(any(test, feature = \"test-support\"))]\n#[doc(hidden)]\npub fn test_support_only() {}\n\
         #[cfg(not(test))]\nmod prodmod;\n\
         #[cfg(test)]\nmod testmod;\n",
    )
    .unwrap();
    std::fs::write(
        source.join("prodmod.rs"),
        "#[doc(hidden)]\npub fn hidden_prod_item() {}\n",
    )
    .unwrap();
    std::fs::write(
        source.join("testmod.rs"),
        "#[doc(hidden)]\npub fn hidden_test_item() {}\n",
    )
    .unwrap();
    let header = "item\tkind\told_vis\tfile:line\tmodule\treason\n";
    let row = |item: &str, file: &str| {
        format!("{header}{item}\tfn\tpub(crate)\tcrates/neovm-value/src/{file}:1\tm\tbulk\n")
    };
    let rows = |items: &[(&str, &str)]| {
        let mut text = header.to_string();
        for (item, file) in items {
            text.push_str(&format!(
                "{item}\tfn\tpub(crate)\tcrates/neovm-value/src/{file}:1\tm\tbulk\n"
            ));
        }
        text
    };
    let production = [
        ("neovm_value::prod_row", "lib.rs"),
        ("neovm_value::prod_arm", "lib.rs"),
        ("hidden_prod_item", "prodmod.rs"),
    ];
    let ledger_text = rows(&production);
    let ledger_rows = ledger(&ledger_text).unwrap();
    // The gated test items and the gated module file are not scanned and not
    // required, while both production arms are.
    equal_rows(&ledger_rows, &source_rows(dir.path()).unwrap())
        .unwrap_or_else(|error| panic!("B3_D PRODUCTION ROUTING: {error}"));
    // A ledger row for a test-only item is red: it is not production surface.
    for (item, file) in [
        ("neovm_value::test_only", "lib.rs"),
        ("neovm_value::test_support_only", "lib.rs"),
        ("neovm_value::hidden_test_item", "testmod.rs"),
    ] {
        let flagged = ledger(&row(item, file)).unwrap();
        let red = equal_rows(&flagged, &source_rows(dir.path()).unwrap()).unwrap_err();
        assert!(
            red.contains("B3_LEDGER_SOURCE_MISMATCH")
                && red.contains(item.rsplit("::").next().unwrap()),
            "{error}",
            error = red
        );
    }
    // Removing a production row is red: production surface must stay listed.
    for keep in [
        vec![production[0], production[1]],
        vec![production[0], production[2]],
    ] {
        let flagged = ledger(&rows(&keep)).unwrap();
        let red = equal_rows(&flagged, &source_rows(dir.path()).unwrap()).unwrap_err();
        assert!(red.contains("unlisted public hidden definition"), "{red}");
    }
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
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
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
            CEILING,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .is_ok()
    );
    // Ruling (c): the value-ledger historical bump is accepted at its exact
    // (path, SHA) even though the commit touched more than the ceiling file...
    assert!(
        monotonic(
            RowCount(1228),
            RowCount(1231),
            "PC11 PROMOTION-LEDGER ceiling bump: 1228 -> 1231 (stage-3b T5 frontend knob)",
            &[CEILING.into(), "other.rs".into()],
            CEILING,
            GRANDFATHERED_CEILING_BUMPS[0].1,
        )
        .is_ok()
    );
    // ...and an EVAL-era entry is likewise accepted only for the EVAL ceiling...
    assert!(
        monotonic(
            RowCount(137),
            RowCount(140),
            "stage4 boot: install eval hooks through one typed-proof entry (E3a)",
            &[EVAL_CEILING.into(), "other.rs".into()],
            EVAL_CEILING,
            "34aaf7f206a4fdba2782b7be1d56c5500073e5ba",
        )
        .is_ok()
    );
    // ...the same SHA at a different ledger is red (the key is the pair)...
    assert!(
        monotonic(
            RowCount(1228),
            RowCount(1231),
            "PC11 PROMOTION-LEDGER ceiling bump: 1228 -> 1231 (stage-3b T5 frontend knob)",
            &[EVAL_CEILING.into(), "other.rs".into()],
            EVAL_CEILING,
            GRANDFATHERED_CEILING_BUMPS[0].1,
        )
        .unwrap_err()
        .contains("silent ceiling increase")
    );
    // ...and the same subject/files at any other revision is red.
    assert!(
        monotonic(
            RowCount(1228),
            RowCount(1231),
            "PC11 PROMOTION-LEDGER ceiling bump: 1228 -> 1231 (stage-3b T5 frontend knob)",
            &[CEILING.into(), "other.rs".into()],
            CEILING,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )
        .unwrap_err()
        .contains("silent ceiling increase")
    );
}

#[test]
fn architecture_pc11_mutation_macro_products_are_fail_closed() {
    // The table is source-verified: each real template must still fingerprint
    // to its entry, and the entry must resolve by (file, macro).
    let root = root();
    for (expected, product) in MACRO_PRODUCTS.iter().enumerate() {
        let text = read(&root.join(product.file)).unwrap();
        let ast = syn::parse_file(&text).unwrap();
        let tokens = ast
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Macro(m)
                    if m.mac.path.is_ident("macro_rules")
                        && m.ident
                            .as_ref()
                            .is_some_and(|name| name == product.macro_name) =>
                {
                    Some(m.mac.tokens.clone())
                }
                _ => None,
            })
            .unwrap_or_else(|| {
                panic!(
                    "B3_MACRO_PRODUCTS missing {} `{}!`",
                    product.file, product.macro_name
                )
            });
        assert_eq!(
            verify_macro_definition(product.file, product.macro_name, &tokens).unwrap(),
            Some(expected)
        );
        assert!(macro_products_for(product.file, product.macro_name).is_some());
        // The entry is keyed by file as well as by name.
        assert!(macro_products_for("crates/neovm-value/src/lib.rs", product.macro_name).is_none());
    }
    // A template edit, including a new doc(hidden) item, changes the
    // fingerprint and fails closed until the table is re-reviewed.
    let edited: proc_macro2::TokenStream =
        "{ ($name:ident) => { #[doc(hidden)] pub struct $name; }; }"
            .parse()
            .unwrap();
    let err = verify_macro_definition(
        MACRO_PRODUCTS[0].file,
        MACRO_PRODUCTS[0].macro_name,
        &edited,
    )
    .unwrap_err();
    assert!(err.contains("B3_MACRO_PRODUCTS"), "{err}");
    assert!(err.contains("template changed"), "{err}");
    assert!(
        verify_macro_definition("crates/neovm-value/src/other.rs", "native_id", &edited)
            .unwrap()
            .is_none()
    );
    // Product identities bind the invocation identifiers positionally and
    // reject an invocation shape the reviewed template cannot consume.
    let products = macro_product_identities(&MACRO_PRODUCTS[0], &["ThreadId".to_string()]).unwrap();
    for expected in [
        "ThreadId",
        "ThreadId::heap_identity",
        "ThreadId::native_key",
    ] {
        assert!(
            products.iter().any(|(item, _)| item == expected),
            "missing {expected}: {products:?}"
        );
    }
    let err = macro_product_identities(&MACRO_PRODUCTS[0], &[]).unwrap_err();
    assert!(err.contains("identifier argument"), "{err}");
    let err =
        macro_product_identities(&MACRO_PRODUCTS[1], &["OnlyOneBinding".to_string()]).unwrap_err();
    assert!(err.contains("identifier argument"), "{err}");
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
    /// LEAD 11:1xZ: a method-call expression can only call an inherent method
    /// that takes `self`; an associated function (`fn f(...) -> Self`) is
    /// path-call-only, and matching `.f()` by name for it would only flag
    /// unrelated types. The declaration is inspected per row.
    fn method_form(&self) -> Option<&str> {
        match self {
            Self::Method { name, .. } => Some(name),
            Self::Name { .. } => None,
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

/// Does the pc2 method declaration take `self`? Only then can method-call
/// syntax call it (LEAD 11:1xZ); associated functions are path-call-only.
/// PC2-F (peer 09:11Z): a lookup miss is an error, not a silent `false` --
/// `pc2_declaration` already required the declaration, so an unresolved
/// receiver would otherwise disable method-call matching fail-open.
fn pc2_method_takes_self(items: &[syn::Item], target: &Pc2Target) -> Result<bool, String> {
    fn takes_self(inputs: &syn::punctuated::Punctuated<syn::FnArg, syn::Token![,]>) -> bool {
        inputs
            .first()
            .is_some_and(|argument| matches!(argument, syn::FnArg::Receiver(_)))
    }
    fn find(items: &[syn::Item], owner: &str, name: &str) -> Option<bool> {
        for item in items {
            match item {
                syn::Item::Mod(module) => {
                    if let Some((_, inner)) = &module.content {
                        if let Some(found) = find(inner, owner, name) {
                            return Some(found);
                        }
                    }
                }
                syn::Item::Impl(block) => {
                    let owner_matches = match &*block.self_ty {
                        syn::Type::Path(path) => path
                            .path
                            .segments
                            .last()
                            .is_some_and(|segment| segment.ident == owner),
                        _ => false,
                    };
                    if !owner_matches {
                        continue;
                    }
                    for item in &block.items {
                        if let syn::ImplItem::Fn(item) = item {
                            if item.sig.ident == name {
                                return Some(takes_self(&item.sig.inputs));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }
    let Pc2Target::Method { owner, name } = target else {
        return Ok(false);
    };
    find(items, owner, name).ok_or_else(|| {
        format!(
            "B3_PC2_UNRESOLVED_METHOD: {owner}::{name} has no inherent declaration to resolve its receiver"
        )
    })
}

/// PC2-G (peer 09:11Z): evaluate a cfg predicate in the production scan
/// environment -- `test` is false and `feature = "test-support"` is false.
/// `#[cfg(not(test))]` is a PRODUCTION arm and must stay scanned; only a
/// predicate that is definitively FALSE removes an item or module. Unknown
/// predicates stay in scope, and an unknown operand makes any/all unknown
/// rather than silently excluding code.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CfgValue {
    True,
    False,
    Unknown,
}

fn cfg_value(meta: &syn::Meta) -> CfgValue {
    match meta {
        syn::Meta::Path(path) => {
            if path.is_ident("test") {
                CfgValue::False
            } else {
                CfgValue::Unknown
            }
        }
        syn::Meta::NameValue(value) => {
            if value.path.is_ident("feature") {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(feature),
                    ..
                }) = &value.value
                {
                    if feature.value() == "test-support" {
                        return CfgValue::False;
                    }
                }
            }
            CfgValue::Unknown
        }
        syn::Meta::List(list) => {
            if list.path.is_ident("not") {
                return match list.parse_args::<syn::Meta>() {
                    Ok(inner) => match cfg_value(&inner) {
                        CfgValue::True => CfgValue::False,
                        CfgValue::False => CfgValue::True,
                        CfgValue::Unknown => CfgValue::Unknown,
                    },
                    Err(_) => CfgValue::Unknown,
                };
            }
            let Ok(items) = list.parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            ) else {
                return CfgValue::Unknown;
            };
            if list.path.is_ident("any") {
                let mut unknown = false;
                for item in &items {
                    match cfg_value(item) {
                        CfgValue::True => return CfgValue::True,
                        CfgValue::False => {}
                        CfgValue::Unknown => unknown = true,
                    }
                }
                return if unknown {
                    CfgValue::Unknown
                } else {
                    CfgValue::False
                };
            }
            if list.path.is_ident("all") {
                let mut unknown = false;
                for item in &items {
                    match cfg_value(item) {
                        CfgValue::False => return CfgValue::False,
                        CfgValue::True => {}
                        CfgValue::Unknown => unknown = true,
                    }
                }
                return if unknown {
                    CfgValue::Unknown
                } else {
                    CfgValue::True
                };
            }
            CfgValue::Unknown
        }
    }
}

/// True when an item is test-only in the production scan environment
/// (`test`=false, `feature = "test-support"`=false): `#[test]` itself, or a
/// `#[cfg(...)]` that is definitively FALSE there. Used by both the PC2
/// caller scan (PC2-G) and the B3 PROMOTION scanner (B3-d), so both route by
/// the same production predicate.
fn production_gated(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if attr.path().is_ident("test") {
            return true;
        }
        if !attr.path().is_ident("cfg") {
            return false;
        }
        attr.parse_args::<syn::Meta>()
            .map(|meta| cfg_value(&meta) == CfgValue::False)
            .unwrap_or(false)
    })
}

struct Pc2Scan<'a> {
    target: &'a Pc2Target,
    defining_file: &'a str,
    file: &'a str,
    stack: Vec<String>,
    references: Vec<Option<String>>,
    /// The pc2 method takes `self`, so method-call syntax is a candidate call
    /// site. Name-only matching over-approximates, which is fail-closed and
    /// acceptable (LEAD 11:1xZ); the enclosing caller still has to be named.
    method_calls: bool,
}

impl<'ast> Visit<'ast> for Pc2Scan<'_> {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if production_gated(&item.attrs) {
            return;
        }
        self.stack.push(item.sig.ident.to_string());
        visit::visit_item_fn(self, item);
        self.stack.pop();
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if production_gated(&item.attrs) {
            return;
        }
        self.stack.push(item.sig.ident.to_string());
        visit::visit_impl_item_fn(self, item);
        self.stack.pop();
    }
    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        if production_gated(&module.attrs) {
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
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if self.method_calls
            && self
                .target
                .method_form()
                .is_some_and(|name| call.method == name)
        {
            self.references.push(self.stack.last().cloned());
        }
        visit::visit_expr_method_call(self, call);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if macro_tokens_mention_call(&mac.tokens, self.target, self.method_calls) {
            self.references.push(self.stack.last().cloned());
        }
        visit::visit_macro(self, mac);
    }
}

/// PC2-M (peer 09:11Z): syn's Visit never enters macro token streams, so a
/// path or method call written inside a macro invocation or a macro_rules!
/// body is otherwise invisible. Scan the tokens name-only for a call shape --
/// the target identifier followed by a parenthesized group, with method-call
/// syntax `.name(...)` accepted only when the row's declaration takes self.
/// A hit is attributed to the enclosing function, which is the same
/// fail-closed name-only over-approximation the AST arms use.
fn macro_tokens_mention_call(
    stream: &proc_macro2::TokenStream,
    target: &Pc2Target,
    method_calls: bool,
) -> bool {
    let tokens: Vec<_> = stream.clone().into_iter().collect();
    for (index, token) in tokens.iter().enumerate() {
        if let proc_macro2::TokenTree::Ident(ident) = token {
            let called = matches!(
                tokens.get(index + 1),
                Some(proc_macro2::TokenTree::Group(group))
                    if group.delimiter() == proc_macro2::Delimiter::Parenthesis
            );
            if called {
                let preceded_by_dot = index > 0
                    && matches!(
                        &tokens[index - 1],
                        proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '.'
                    );
                let name_matches = *ident == target.name()
                    || (method_calls && target.method_form().is_some_and(|name| *ident == name));
                if name_matches && (method_calls || !preceded_by_dot) {
                    return true;
                }
            }
        }
        if let proc_macro2::TokenTree::Group(group) = token {
            if macro_tokens_mention_call(&group.stream(), target, method_calls) {
                return true;
            }
        }
    }
    false
}

/// Every workspace member's production `src/**`. Consumers outside the seven
/// moved crates (notably `crates/neomacs`) can call a pc2 item too, so they
/// are scanned as well; a planted method call there must go RED.
fn pc2_workspace_members(workspace: &Path) -> Result<Vec<PathBuf>, String> {
    let path = workspace.join("Cargo.toml");
    let text = read(&path)?;
    let root: toml::Value = toml::from_str(&text)
        .map_err(|error| format!("B3_PC2_MANIFEST {}: {error}", path.display()))?;
    let members = root
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(toml::Value::as_array)
        .ok_or("B3_PC2_MANIFEST: missing workspace members")?;
    let mut out = Vec::new();
    for member in members {
        let member = member
            .as_str()
            .ok_or("B3_PC2_MANIFEST: non-string member")?;
        let member_root = workspace.join(member);
        let manifest_path = member_root.join("Cargo.toml");
        let text = read(&manifest_path)?;
        let manifest: toml::Value = toml::from_str(&text)
            .map_err(|error| format!("B3_PC2_MANIFEST {}: {error}", manifest_path.display()))?;
        if manifest.get("package").is_none() {
            continue;
        }
        out.push(member_root);
    }
    Ok(out)
}

/// PC2-T (peer 09:11Z): production source files come from the module tree, not
/// from a directory sweep with a tests.rs/*_test.rs filename heuristic. The
/// walker starts at each crate root (lib.rs, main.rs, src/bin/*) and follows
/// `mod` declarations, carrying the evaluated cfg gate of every ancestor: a
/// subtree whose gate is definitively FALSE is excluded as test-only, while a
/// production `#[cfg(not(test))]` arm stays in scope. Files without a module
/// root are not compiled production code and are not scanned; a member with
/// source files but no root fails closed instead of silently scanning nothing.
fn pc2_walk_module_file(
    file: &Path,
    child_dir: &Path,
    excluded: bool,
    visited: &mut BTreeSet<PathBuf>,
    excluded_files: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    let canonical = file
        .canonicalize()
        .map_err(|error| format!("B3_PC2_SOURCE {}: {error}", file.display()))?;
    if !visited.insert(canonical.clone()) {
        if excluded {
            excluded_files.insert(canonical);
        }
        return Ok(());
    }
    if excluded {
        // A definitively test-only subtree is not production code: it is not
        // scanned, so its own module declarations do not need resolution.
        excluded_files.insert(canonical);
        return Ok(());
    }
    let text = read(&canonical)?;
    let ast = syn::parse_file(&text)
        .map_err(|error| format!("B3_PC2_PARSE {}: {error}", canonical.display()))?;
    let parent = canonical
        .parent()
        .ok_or("B3_PC2_SOURCE module file without parent")?;
    pc2_walk_module_items(&ast.items, child_dir, parent, visited, excluded_files)
}

fn pc2_walk_module_items(
    items: &[syn::Item],
    dir: &Path,
    path_base: &Path,
    visited: &mut BTreeSet<PathBuf>,
    excluded_files: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    for item in items {
        let syn::Item::Mod(module) = item else {
            continue;
        };
        // `mod r#if;` names the plain file `if.rs`; strip the raw prefix.
        let ident = module.ident.to_string();
        let name = ident.strip_prefix("r#").unwrap_or(&ident).to_string();
        if production_gated(&module.attrs) {
            // cfg(test)/(test-support)-only subtree: not production, no scan.
            continue;
        }
        if let Some((_, inner)) = &module.content {
            let inline_dir = dir.join(&name);
            pc2_walk_module_items(inner, &inline_dir, &inline_dir, visited, excluded_files)?;
            continue;
        }
        let mut paths = Vec::new();
        let mut path_attr = false;
        for attr in &module.attrs {
            if attr.path().is_ident("path") {
                let syn::Meta::NameValue(meta) = &attr.meta else {
                    return Err(format!("B3_PC2_SOURCE {name}: module path metadata"));
                };
                let syn::Expr::Lit(expr) = &meta.value else {
                    return Err(format!("B3_PC2_SOURCE {name}: module path expression"));
                };
                let syn::Lit::Str(value) = &expr.lit else {
                    return Err(format!("B3_PC2_SOURCE {name}: module path string"));
                };
                paths.push(path_base.join(value.value()));
                path_attr = true;
            }
            if attr.path().is_ident("cfg_attr") {
                let args = attr
                    .parse_args_with(
                        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                    )
                    .map_err(|error| format!("B3_PC2_SOURCE {name}: cfg_attr: {error}"))?;
                if args.iter().skip(1).any(|meta| meta.path().is_ident("path")) {
                    return Err(format!(
                        "B3_PC2_SOURCE {name}: conditional module path requires review"
                    ));
                }
            }
        }
        if paths.is_empty() {
            paths = [
                dir.join(format!("{name}.rs")),
                dir.join(&name).join("mod.rs"),
            ]
            .into_iter()
            .filter(|path| path.is_file())
            .collect();
        }
        if paths.len() != 1 {
            return Err(format!(
                "B3_PC2_SOURCE unresolved module {name} in {}",
                dir.display()
            ));
        }
        // A #[path] module's children resolve beside the referenced file; a
        // default module's children live under the module-name directory.
        let next_dir = if path_attr {
            paths[0]
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| dir.to_path_buf())
        } else {
            dir.join(&name)
        };
        pc2_walk_module_file(&paths[0], &next_dir, false, visited, excluded_files)?;
    }
    Ok(())
}

fn pc2_source_files(workspace: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for member in pc2_workspace_members(workspace)? {
        let src = member.join("src");
        if !src.is_dir() {
            continue;
        }
        let mut roots = Vec::new();
        for name in ["lib.rs", "main.rs"] {
            let root = src.join(name);
            if root.is_file() {
                roots.push(root);
            }
        }
        let bins = src.join("bin");
        if bins.is_dir() {
            let mut entries: Vec<_> = std::fs::read_dir(&bins)
                .map_err(|error| error.to_string())?
                .map(|entry| entry.map_err(|error| error.to_string()))
                .collect::<Result<_, _>>()?;
            entries.sort_by_key(|entry| entry.path());
            for entry in entries {
                let path = entry.path();
                if entry
                    .file_type()
                    .map_err(|error| error.to_string())?
                    .is_file()
                    && path.extension().is_some_and(|extension| extension == "rs")
                {
                    roots.push(path);
                } else if entry
                    .file_type()
                    .map_err(|error| error.to_string())?
                    .is_dir()
                    && path.join("main.rs").is_file()
                {
                    roots.push(path.join("main.rs"));
                }
            }
        }
        if roots.is_empty() {
            let has_source = std::fs::read_dir(&src)
                .map_err(|error| error.to_string())?
                .any(|entry| {
                    entry
                        .map(|entry| {
                            entry
                                .path()
                                .extension()
                                .is_some_and(|extension| extension == "rs")
                        })
                        .unwrap_or(false)
                });
            if has_source {
                return Err(format!(
                    "B3_PC2_SOURCE member {} has source files but no lib.rs/main.rs root",
                    member.display()
                ));
            }
            continue;
        }
        let mut visited = BTreeSet::new();
        let mut excluded_files = BTreeSet::new();
        for root in &roots {
            let child_dir = root.parent().unwrap_or(src.as_path()).to_path_buf();
            pc2_walk_module_file(root, &child_dir, false, &mut visited, &mut excluded_files)?;
        }
        files.extend(
            visited
                .into_iter()
                .filter(|file| !excluded_files.contains(file)),
        );
    }
    files.sort();
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

/// Map one collected source file to its workspace-relative ledger spelling
/// and read it.  Fail closed: a file the walker collected outside the
/// workspace, and an unreadable file, are errors that carry the path; neither
/// may `continue`, because a skipped file hides every call site in it from the
/// PC2 scan.
fn pc2_relative_source(workspace: &Path, file: &Path) -> Result<(String, String), String> {
    let relative = file
        .strip_prefix(workspace)
        .map_err(|_| format!("B3_PC2_FILE_OUTSIDE_WORKSPACE {}", file.display()))?;
    let relative = relative.to_string_lossy().replace('\\', "/");
    let text = std::fs::read_to_string(file)
        .map_err(|error| format!("B3_PC2_SOURCE_READ {relative}: {error}"))?;
    Ok((relative, text))
}

fn pc2_rows_violation(
    workspace: &Path,
    ledger_name: &str,
    text: &str,
    seams: &[Pc2Seam],
) -> Result<(), String> {
    // pc2_source_files returns canonicalized paths (the module walker
    // canonicalizes every file it visits). The workspace may be named through
    // a symlinked temp root -- macOS /tmp and /var are symlinks to /private,
    // and tempdir() keeps the $TMPDIR spelling -- so canonicalize it too, or
    // the file mapping below treats every file as outside the workspace and
    // a planted unnamed call site is silently GREEN.
    let workspace = workspace
        .canonicalize()
        .map_err(|error| format!("B3_PC2_WORKSPACE {}: {error}", workspace.display()))?;
    let files = pc2_source_files(&workspace)?;
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
        let method_calls = pc2_method_takes_self(&ast.items, &target)?;
        let mut unnamed = BTreeSet::new();
        for file in &files {
            let (relative, text) = pc2_relative_source(&workspace, file)?;
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
                method_calls,
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

fn pc2_fixture_workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/neovm-value\", \"crates/neovm-core\"]\n",
    )
    .unwrap();
    for (member, dependency) in [
        ("neovm-value", ""),
        (
            "neovm-core",
            "\n[dependencies]\nneovm-value = { path = \"../neovm-value\" }\n",
        ),
    ] {
        let root = dir.path().join("crates").join(member);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{member}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n{dependency}"
            ),
        )
        .unwrap();
    }
    dir
}

#[test]
fn architecture_mutation_pc2_unnamed_call_site_is_rejected() {
    let dir = pc2_fixture_workspace();
    let src = dir.path().join("crates/neovm-value/src");
    std::fs::write(
        src.join("lib.rs"),
        "pub mod installer_mod;\npub mod probe;\n",
    )
    .unwrap();
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
#[cfg(unix)]
fn architecture_mutation_pc2_symlinked_workspace_still_detects_call_sites() {
    // The module walker canonicalizes every source file it returns, while a
    // workspace may be named through a symlinked temp root: macOS /tmp and
    // /var are symlinks to /private, and tempdir() keeps the $TMPDIR spelling.
    // If the workspace side is not canonicalized too, strip_prefix misses
    // every file, the scan sees no call sites, and a planted unnamed caller
    // is silently GREEN.
    let dir = pc2_fixture_workspace();
    let src = dir.path().join("crates/neovm-value/src");
    std::fs::write(
        src.join("lib.rs"),
        "pub mod installer_mod;\npub mod probe;\n",
    )
    .unwrap();
    std::fs::write(src.join("installer_mod.rs"), "pub fn installer() {}\n").unwrap();
    std::fs::write(
        src.join("probe.rs"),
        "fn named_caller() { crate::installer_mod::installer(); }\nfn secret_caller() { crate::installer_mod::installer(); }\n",
    )
    .unwrap();
    let alias_root = tempfile::tempdir().unwrap();
    let alias = alias_root.path().join("workspace");
    std::os::unix::fs::symlink(dir.path(), &alias).unwrap();
    let header = "item\tkind\told_vis\tfile:line\tmodule\treason\n";
    let row = format!(
        "{header}installer\tpc2\t-\tcrates/neovm-value/src/installer_mod.rs:1\tm\texact caller named_caller only\n"
    );
    let error = pc2_rows_violation(&alias, "PROMOTION-LEDGER-X.tsv", &row, &[])
        .expect_err("unnamed call site through a symlinked workspace");
    assert!(
        error.contains("B3_PC2_UNNAMED_CALL_SITE") && error.contains("secret_caller"),
        "{error}"
    );
}

#[test]
fn architecture_mutation_pc2_source_file_failures_are_fail_closed() {
    // PC2-H1: the scan must never skip a collected file.  A strip_prefix miss
    // (a file outside the workspace) and an unreadable file both error with
    // their path; a `continue` would hide every call site in that file.
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(workspace.path().join("crate/src")).unwrap();
    let inside = workspace.path().join("crate/src/lib.rs");
    std::fs::write(&inside, "fn named_caller() {}\n").unwrap();

    let outside_dir = tempfile::tempdir().unwrap();
    let outside = outside_dir.path().join("outside.rs");
    std::fs::write(&outside, "fn outside() {}\n").unwrap();
    let error = pc2_relative_source(workspace.path(), &outside).unwrap_err();
    assert!(
        error.contains("B3_PC2_FILE_OUTSIDE_WORKSPACE") && error.contains("outside.rs"),
        "{error}"
    );

    let missing = workspace.path().join("crate/src/missing.rs");
    let error = pc2_relative_source(workspace.path(), &missing).unwrap_err();
    assert!(
        error.contains("B3_PC2_SOURCE_READ") && error.contains("crate/src/missing.rs"),
        "{error}"
    );

    let (relative, text) = pc2_relative_source(workspace.path(), &inside).unwrap();
    assert_eq!(relative, "crate/src/lib.rs");
    assert!(text.contains("fn named_caller"), "{text}");
}

#[test]
fn architecture_mutation_pc2_method_call_site_is_rejected() {
    // LEAD 11:1xZ: method-call syntax must be a candidate call site when the
    // pc2 method takes self; name-only matching over-approximates on purpose.
    let dir = pc2_fixture_workspace();
    let value_src = dir.path().join("crates/neovm-value/src");
    let core_src = dir.path().join("crates/neovm-core/src");
    std::fs::write(value_src.join("lib.rs"), "pub mod owner_mod;\n").unwrap();
    std::fs::write(
        value_src.join("owner_mod.rs"),
        "pub struct Owner;\nimpl Owner {\n    pub fn attach(&mut self, _r: u8) -> bool { true }\n}\n",
    )
    .unwrap();
    std::fs::write(core_src.join("lib.rs"), "pub mod probe;\n").unwrap();
    std::fs::write(
        core_src.join("probe.rs"),
        "use neovm_value::owner_mod::Owner;\nfn named_caller(owner: &mut Owner) { owner.attach(1); }\nfn secret_caller(owner: &mut Owner) { owner.attach(2); }\nfn macro_caller(owner: &mut Owner) { debug_assert!(owner.attach(3)); }\n",
    )
    .unwrap();
    let header = "item\tkind\told_vis\tfile:line\tmodule\treason\n";
    let row = |reason: &str| {
        format!(
            "{header}Owner::attach\tpc2\t-\tcrates/neovm-value/src/owner_mod.rs:1\tm\t{reason}\n"
        )
    };
    let error = pc2_rows_violation(
        dir.path(),
        "PROMOTION-LEDGER-X.tsv",
        &row("exact caller named_caller only"),
        &[],
    )
    .expect_err("unnamed method call");
    assert!(
        error.contains("B3_PC2_UNNAMED_CALL_SITE")
            && error.contains("secret_caller")
            && error.contains("macro_caller"),
        "{error}"
    );
    assert!(
        pc2_rows_violation(
            dir.path(),
            "PROMOTION-LEDGER-X.tsv",
            &row("exact callers named_caller and secret_caller and macro_caller"),
            &[],
        )
        .is_ok()
    );
    // An associated function (no self) is path-call-only; a same-named method
    // call on another type is not a call site for it.
    std::fs::write(
        value_src.join("owner_mod.rs"),
        "pub struct Owner;\nimpl Owner {\n    pub fn attach(_r: u8) -> Self { Owner }\n}\n",
    )
    .unwrap();
    assert!(
        pc2_rows_violation(
            dir.path(),
            "PROMOTION-LEDGER-X.tsv",
            &row("exact caller named_caller only"),
            &[],
        )
        .is_ok()
    );
    // PC2-F: a receiver that cannot be resolved from the declaring file is an
    // error, not a silent path-call-only downgrade.
    let items = syn::parse_file("pub struct Owner;\nimpl Owner {\n    pub fn other(&self) {}\n}\n")
        .unwrap()
        .items;
    let error = pc2_method_takes_self(&items, &pc2_target("Owner::attach", "x.rs")).unwrap_err();
    assert!(error.contains("B3_PC2_UNRESOLVED_METHOD"), "{error}");
    assert_eq!(
        pc2_method_takes_self(&items, &pc2_target("free_function", "x.rs")).unwrap(),
        false
    );
}

#[test]
fn architecture_mutation_pc2_cfg_and_module_walk_are_fail_closed() {
    // PC2-G: #[cfg(not(test))] is production and must be scanned; #[cfg(test)]
    // is not. PC2-T: the module-tree walker replaces the tests.rs filename
    // heuristic, so an ungated file named tests.rs is production while a
    // cfg(test)-gated module with any name is excluded. PC2-M: a call written
    // inside macro tokens is a call site.
    let dir = pc2_fixture_workspace();
    let value_src = dir.path().join("crates/neovm-value/src");
    let core_src = dir.path().join("crates/neovm-core/src");
    std::fs::write(value_src.join("lib.rs"), "pub mod installer_mod;\n").unwrap();
    std::fs::write(
        value_src.join("installer_mod.rs"),
        "pub fn installer() {}\n",
    )
    .unwrap();
    std::fs::write(
        core_src.join("lib.rs"),
        "pub mod probe;\npub mod tests;\n#[cfg(test)]\npub mod gated;\n#[cfg(not(test))]\npub mod production;\n",
    )
    .unwrap();
    std::fs::write(
        core_src.join("probe.rs"),
        "fn named_caller() { neovm_value::installer_mod::installer(); }\nfn macro_path_caller() { debug_assert!(neovm_value::installer_mod::installer()); }\n",
    )
    .unwrap();
    std::fs::write(
        core_src.join("tests.rs"),
        "fn walker_caller() { neovm_value::installer_mod::installer(); }\n",
    )
    .unwrap();
    std::fs::write(
        core_src.join("gated.rs"),
        "fn gated_caller() { neovm_value::installer_mod::installer(); }\n",
    )
    .unwrap();
    std::fs::write(
        core_src.join("production.rs"),
        "fn cfg_caller() { neovm_value::installer_mod::installer(); }\n",
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
        &row("exact caller named_caller"),
        &[],
    )
    .expect_err("unnamed production call sites");
    assert!(error.contains("B3_PC2_UNNAMED_CALL_SITE"), "{error}");
    assert!(error.contains("macro_path_caller"), "macro tokens: {error}");
    assert!(error.contains("walker_caller"), "tests.rs walker: {error}");
    assert!(error.contains("cfg_caller"), "cfg(not(test)) arm: {error}");
    assert!(
        !error.contains("gated_caller"),
        "cfg(test) module must be excluded: {error}"
    );
    assert!(
        pc2_rows_violation(
            dir.path(),
            "PROMOTION-LEDGER-X.tsv",
            &row(
                "exact callers named_caller and macro_path_caller and walker_caller and cfg_caller"
            ),
            &[],
        )
        .is_ok()
    );
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

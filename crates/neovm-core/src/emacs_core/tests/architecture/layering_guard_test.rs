//! Ratchet upward references in every production module, in every feature
//! configuration. Follow module declarations and resolve facade re-exports so
//! moving a reference behind a public name cannot hide a layering dependency.

use proc_macro2::Span;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use syn::ext::IdentExt;
use syn::visit::{self, Visit};
use syn::{Attribute, Expr, Item, ItemMod, Lit, Meta};

/// Whether a cfg predicate can be true with `test` disabled. Other cfg values
/// vary across production builds, so both possibilities remain in the scan.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ProductionCfg {
    Disabled,
    Enabled,
    Conditional,
}

fn production_cfg(meta: &Meta) -> ProductionCfg {
    match meta {
        Meta::Path(path) if path.is_ident("test") => ProductionCfg::Disabled,
        Meta::List(list) => {
            let Ok(predicates) = list.parse_args_with(
                syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
            ) else {
                return ProductionCfg::Conditional;
            };
            if list.path.is_ident("all") {
                if predicates
                    .iter()
                    .any(|meta| production_cfg(meta) == ProductionCfg::Disabled)
                {
                    ProductionCfg::Disabled
                } else if predicates
                    .iter()
                    .all(|meta| production_cfg(meta) == ProductionCfg::Enabled)
                {
                    ProductionCfg::Enabled
                } else {
                    ProductionCfg::Conditional
                }
            } else if list.path.is_ident("any") {
                if predicates
                    .iter()
                    .any(|meta| production_cfg(meta) == ProductionCfg::Enabled)
                {
                    ProductionCfg::Enabled
                } else if predicates
                    .iter()
                    .all(|meta| production_cfg(meta) == ProductionCfg::Disabled)
                {
                    ProductionCfg::Disabled
                } else {
                    ProductionCfg::Conditional
                }
            } else if list.path.is_ident("not") && predicates.len() == 1 {
                match production_cfg(predicates.first().expect("one cfg predicate")) {
                    ProductionCfg::Disabled => ProductionCfg::Enabled,
                    ProductionCfg::Enabled => ProductionCfg::Disabled,
                    ProductionCfg::Conditional => ProductionCfg::Conditional,
                }
            } else {
                ProductionCfg::Conditional
            }
        }
        _ => ProductionCfg::Conditional,
    }
}

fn test_only(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("test")
            || (attribute.path().is_ident("cfg")
                && attribute
                    .parse_args::<Meta>()
                    .is_ok_and(|meta| production_cfg(&meta) == ProductionCfg::Disabled))
    })
}

fn item_attributes(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        // syn is an external, non-exhaustive enum. Unknown syntax must be
        // scanned conservatively, never treated as test-only.
        _ => &[],
    }
}

fn explicit_module_path(module: &ItemMod) -> Option<PathBuf> {
    module.attrs.iter().find_map(|attribute| {
        if !attribute.path().is_ident("path") {
            return None;
        }
        let Meta::NameValue(value) = &attribute.meta else {
            return None;
        };
        let Expr::Lit(value) = &value.value else {
            return None;
        };
        let Lit::Str(path) = &value.lit else {
            return None;
        };
        Some(PathBuf::from(path.value()))
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum LowerRoot {
    /// The emacs_core direct submodules that move into the bottom
    /// `neovm-value` crate together with `tagged`, `heap_types` and
    /// `gc_trace` (DESIGN.md §4), measured as one lower root. This names a
    /// group of emacs_core submodules, not a top-level module, so only the
    /// ceilings file spells it.
    G0,
    Buffer,
    CodeConversionWorkspace,
    DisplayEvaluation,
    Encoding,
    Face,
    FrontendEvents,
    FuzzSupport,
    GcTrace,
    HeapTypes,
    ImageIdentity,
    Keyboard,
    KeyboardInput,
    Logging,
    Tagged,
    TlsScope,
    Window,
}

impl LowerRoot {
    fn parse(name: &str) -> Self {
        match name {
            "g0" => Self::G0,
            "buffer" => Self::Buffer,
            "code_conversion_workspace" => Self::CodeConversionWorkspace,
            "display_evaluation" => Self::DisplayEvaluation,
            "encoding" => Self::Encoding,
            "face" => Self::Face,
            "frontend_events" => Self::FrontendEvents,
            "fuzz_support" => Self::FuzzSupport,
            "gc_trace" => Self::GcTrace,
            "heap_types" => Self::HeapTypes,
            "image_identity" => Self::ImageIdentity,
            "keyboard" => Self::Keyboard,
            "keyboard_input" => Self::KeyboardInput,
            "logging" => Self::Logging,
            "tagged" => Self::Tagged,
            "tls_scope" => Self::TlsScope,
            "window" => Self::Window,
            unknown => {
                panic!("unknown top-level production module {unknown}; add a LowerRoot variant")
            }
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::G0 => "g0",
            Self::Buffer => "buffer",
            Self::CodeConversionWorkspace => "code_conversion_workspace",
            Self::DisplayEvaluation => "display_evaluation",
            Self::Encoding => "encoding",
            Self::Face => "face",
            Self::FrontendEvents => "frontend_events",
            Self::FuzzSupport => "fuzz_support",
            Self::GcTrace => "gc_trace",
            Self::HeapTypes => "heap_types",
            Self::ImageIdentity => "image_identity",
            Self::Keyboard => "keyboard",
            Self::KeyboardInput => "keyboard_input",
            Self::Logging => "logging",
            Self::Tagged => "tagged",
            Self::TlsScope => "tls_scope",
            Self::Window => "window",
        }
    }
}

/// The emacs_core direct submodules that join `tagged`, `heap_types` and
/// `gc_trace` in the bottom `neovm-value` crate (DESIGN.md §4). A reference
/// from one of them to anything outside this set and the bottom peers is
/// upward.
const G0_MEMBERS: [&str; 10] = [
    "value",
    "symbol",
    "intern",
    "error",
    "errors",
    "forward",
    "emacs_char",
    "boolvec",
    "alloc",
    "hashtab",
];

fn is_g0_member(name: &str) -> bool {
    G0_MEMBERS.contains(&name)
}

/// Top-level modules that move into `neovm-value` with G0, so a G0 reference
/// to one of them stays inside the bottom crate.
fn is_bottom_peer(root: LowerRoot) -> bool {
    matches!(
        root,
        LowerRoot::Tagged | LowerRoot::HeapTypes | LowerRoot::GcTrace
    )
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct EcSub(String);

impl EcSub {
    /// Items defined in `emacs_core/mod.rs` itself, above every submodule. They
    /// are as upward as a submodule reference, so they ratchet under this name.
    const ROOT: &'static str = "(root)";

    fn root() -> Self {
        Self(Self::ROOT.to_owned())
    }

    fn validated(name: &str, modules: &BTreeSet<String>) -> Self {
        assert!(
            name == Self::ROOT || modules.contains(name),
            "unknown emacs_core direct submodule {name}"
        );
        Self(name.to_owned())
    }
}

/// The upper end of a measured pair.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Upper {
    /// An emacs_core direct submodule (or a facade name that resolves to
    /// one).
    Ec(EcSub),
    /// A top-level module. G0 is the first measured lower root that can sit
    /// under a top-level module, so both kinds are now nameable.
    Top(LowerRoot),
}

impl Upper {
    /// The ceilings-file spelling: an emacs_core submodule by name, a
    /// top-level module as `top:<name>`. `:` cannot occur in an identifier,
    /// so the two namespaces cannot collide.
    fn text(&self) -> String {
        match self {
            Self::Ec(sub) => sub.0.clone(),
            Self::Top(root) => format!("top:{}", root.name()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Pair {
    lower: LowerRoot,
    upper: Upper,
}

#[derive(Debug)]
enum Classification {
    Upward(Pair),
    NotUpward,
}

#[derive(Clone, Copy)]
enum ScanPhase {
    Discover,
    References,
}

#[derive(Debug)]
struct Site {
    source: PathBuf,
    line: usize,
}

type Sites = BTreeMap<Pair, Vec<Site>>;
type Ceilings = BTreeMap<Pair, usize>;

#[derive(Default)]
struct ModuleIndex {
    ec_modules: BTreeSet<String>,
    /// Top-level production modules declared in `lib.rs`. G0 can name one
    /// directly (`crate::buffer::...`) without going through `emacs_core`.
    crate_modules: BTreeSet<String>,
    /// Items defined directly in `emacs_core/mod.rs` (`eval_source`,
    /// `MenuBarPopupAnchor`, ...), which a `crate::emacs_core::Name` path can
    /// reach without naming a submodule.
    ec_root_items: BTreeSet<String>,
    crate_exports: BTreeMap<String, EcSub>,
    ec_exports: BTreeMap<String, EcSub>,
}

// Expand nested imports and aliases for the facade maps. Reference counting
// below visits a shared use-tree prefix once, rather than once per leaf.
fn use_leaves(
    tree: &syn::UseTree,
    prefix: &mut Vec<String>,
    leaves: &mut Vec<(Vec<String>, String)>,
) {
    match tree {
        syn::UseTree::Path(path) => {
            prefix.push(path.ident.unraw().to_string());
            use_leaves(&path.tree, prefix, leaves);
            prefix.pop();
        }
        syn::UseTree::Name(name) => {
            let ident = name.ident.unraw().to_string();
            let mut path = prefix.clone();
            if ident != "self" {
                path.push(ident.clone());
            }
            let name = path.last().cloned().expect("nonempty use path");
            leaves.push((path, name));
        }
        syn::UseTree::Rename(rename) => {
            let mut path = prefix.clone();
            if rename.ident != "self" {
                path.push(rename.ident.unraw().to_string());
            }
            leaves.push((path, rename.rename.unraw().to_string()));
        }
        syn::UseTree::Group(group) => {
            for tree in &group.items {
                use_leaves(tree, prefix, leaves);
            }
        }
        syn::UseTree::Glob(_) => {
            let mut path = prefix.clone();
            path.push("*".to_owned());
            leaves.push((path, "*".to_owned()));
        }
    }
}

/// This is a test-local, single-threaded source walker, with no runtime state.
struct ProductionWalker {
    phase: ScanPhase,
    source: PathBuf,
    module_directory: PathBuf,
    // #[path] uses the containing source; ordinary foo.rs children use foo/.
    path_directory: PathBuf,
    logical_path: Vec<String>,
    index: ModuleIndex,
    exports: Vec<(Vec<String>, Vec<String>, String)>,
    sites: Sites,
}

impl ProductionWalker {
    fn file(&mut self, path: &Path) {
        let source = std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let syntax = syn::parse_file(&source)
            .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
        if test_only(&syntax.attrs) {
            return;
        }
        self.discover_module();
        let parent = path.parent().expect("source has parent");
        let stem = path.file_stem().expect("Rust source has stem");
        let directory = if stem == "lib" || stem == "mod" {
            parent.to_path_buf()
        } else {
            parent.join(stem)
        };
        let previous_source = std::mem::replace(&mut self.source, path.to_path_buf());
        let previous_module = std::mem::replace(&mut self.module_directory, directory);
        let previous_path = std::mem::replace(&mut self.path_directory, parent.to_path_buf());
        self.visit_file(&syntax);
        self.source = previous_source;
        self.module_directory = previous_module;
        self.path_directory = previous_path;
    }

    fn discover_module(&mut self) {
        match self.phase {
            ScanPhase::Discover => match self.logical_path.as_slice() {
                [name] if name != "emacs_core" => {
                    LowerRoot::parse(name);
                    self.index.crate_modules.insert(name.clone());
                }
                [root, name] if root == "emacs_core" => {
                    self.index.ec_modules.insert(name.clone());
                }
                _ => {}
            },
            ScanPhase::References => {}
        }
    }

    fn classify(&self, segments: &[String], span: Span) -> Classification {
        let Some(lower) = self.logical_path.first() else {
            return Classification::NotUpward;
        };
        if lower == "emacs_core" {
            return self.classify_from_emacs_core(segments, span);
        }
        let Some(first) = segments.first() else {
            return Classification::NotUpward;
        };
        let remainder = if first == "crate" {
            &segments[1..]
        } else if first == "super" {
            let pops = segments.iter().take_while(|part| *part == "super").count();
            if pops > self.logical_path.len() {
                return Classification::NotUpward;
            }
            let remainder = &segments[pops..];
            if pops != self.logical_path.len()
                && remainder.first().is_none_or(|part| part != "emacs_core")
            {
                return Classification::NotUpward;
            }
            remainder
        } else {
            return Classification::NotUpward;
        };
        let Some(name) = remainder.first() else {
            return Classification::NotUpward;
        };
        let upper = if name == "emacs_core" {
            let Some(name) = remainder.get(1) else {
                return Classification::NotUpward;
            };
            if self.index.ec_modules.contains(name) {
                EcSub::validated(name, &self.index.ec_modules)
            } else if let Some(origin) = self.index.ec_exports.get(name) {
                origin.clone()
            } else if self.index.ec_root_items.contains(name) {
                EcSub::root()
            } else {
                panic!(
                    "{}:{}: unknown emacs_core submodule or re-export {name}",
                    self.source.display(),
                    span.start().line
                )
            }
        } else if let Some(origin) = self.index.crate_exports.get(name) {
            origin.clone()
        } else {
            return Classification::NotUpward;
        };
        Classification::Upward(Pair {
            lower: LowerRoot::parse(lower),
            upper: Upper::Ec(upper),
        })
    }

    /// Resolve a written path to crate-root-relative segments. `crate::`
    /// restarts at the root, `super::` pops one module per keyword, and
    /// `self::` stays in the containing module. Names that do not start at
    /// the crate root (locals, associated items, imported names) are counted
    /// at their `use` item instead, exactly as for the top-level sources.
    fn absolute_segments(&self, segments: &[String]) -> Option<Vec<String>> {
        let first = segments.first()?;
        match first.as_str() {
            "crate" => Some(segments[1..].to_vec()),
            "super" => {
                let pops = segments.iter().take_while(|part| *part == "super").count();
                if pops > self.logical_path.len() {
                    return None;
                }
                let mut absolute = self.logical_path[..self.logical_path.len() - pops].to_vec();
                absolute.extend_from_slice(&segments[pops..]);
                Some(absolute)
            }
            "self" => {
                let mut absolute = self.logical_path.clone();
                absolute.extend_from_slice(&segments[1..]);
                Some(absolute)
            }
            _ => None,
        }
    }

    /// A reference written inside an emacs_core module is upward only when
    /// the source is a G0 member and the target leaves the bottom group: an
    /// emacs_core submodule outside G0, or a top-level module outside the
    /// bottom peers. References between G0 members, to tagged/heap_types/
    /// gc_trace, and every reference from the upper modules stay outside the
    /// ratchet, exactly as before.
    fn classify_from_emacs_core(&self, segments: &[String], span: Span) -> Classification {
        let Some(member) = self.logical_path.get(1) else {
            return Classification::NotUpward;
        };
        if !is_g0_member(member) {
            return Classification::NotUpward;
        }
        let Some(absolute) = self.absolute_segments(segments) else {
            return Classification::NotUpward;
        };
        let Some(first) = absolute.first() else {
            return Classification::NotUpward;
        };
        if first == "emacs_core" {
            let Some(name) = absolute.get(1) else {
                return Classification::NotUpward;
            };
            let upper = if self.index.ec_modules.contains(name) {
                EcSub::validated(name, &self.index.ec_modules)
            } else if let Some(origin) = self.index.ec_exports.get(name) {
                origin.clone()
            } else if self.index.ec_root_items.contains(name) {
                EcSub::root()
            } else {
                panic!(
                    "{}:{}: unknown emacs_core submodule or re-export {name}",
                    self.source.display(),
                    span.start().line
                )
            };
            if is_g0_member(&upper.0) {
                return Classification::NotUpward;
            }
            return Classification::Upward(Pair {
                lower: LowerRoot::G0,
                upper: Upper::Ec(upper),
            });
        }
        if let Some(origin) = self.index.crate_exports.get(first) {
            if is_g0_member(&origin.0) {
                return Classification::NotUpward;
            }
            return Classification::Upward(Pair {
                lower: LowerRoot::G0,
                upper: Upper::Ec(origin.clone()),
            });
        }
        assert!(
            self.index.crate_modules.contains(first),
            "{}:{}: unknown top-level module or crate re-export {first}",
            self.source.display(),
            span.start().line
        );
        let root = LowerRoot::parse(first);
        if is_bottom_peer(root) {
            return Classification::NotUpward;
        }
        Classification::Upward(Pair {
            lower: LowerRoot::G0,
            upper: Upper::Top(root),
        })
    }

    fn record(&mut self, segments: &[String], span: Span) -> bool {
        match self.phase {
            ScanPhase::Discover => false,
            ScanPhase::References => match self.classify(segments, span) {
                Classification::Upward(pair) => {
                    self.sites.entry(pair).or_default().push(Site {
                        source: self.source.clone(),
                        line: span.start().line,
                    });
                    true
                }
                Classification::NotUpward => false,
            },
        }
    }

    fn use_references(&mut self, tree: &syn::UseTree, prefix: &mut Vec<String>, span: Span) {
        match tree {
            syn::UseTree::Path(path) => {
                prefix.push(path.ident.unraw().to_string());
                if !self.record(prefix, span) {
                    self.use_references(&path.tree, prefix, span);
                }
                prefix.pop();
            }
            syn::UseTree::Name(name) => {
                let terminal_self = name.ident == "self" && !prefix.is_empty();
                if !terminal_self {
                    prefix.push(name.ident.unraw().to_string());
                }
                self.record(prefix, span);
                if !terminal_self {
                    prefix.pop();
                }
            }
            syn::UseTree::Rename(rename) => {
                let terminal_self = rename.ident == "self" && !prefix.is_empty();
                if !terminal_self {
                    prefix.push(rename.ident.unraw().to_string());
                }
                self.record(prefix, span);
                if !terminal_self {
                    prefix.pop();
                }
            }
            syn::UseTree::Group(group) => {
                for tree in &group.items {
                    self.use_references(tree, prefix, span);
                }
            }
            syn::UseTree::Glob(_) => {
                self.record(prefix, span);
            }
        }
    }
}

// Macro bodies are token streams, not ordinary syn paths. Only literal
// crate::emacs_core::<ident> sequences count; super is intentionally skipped.
fn macro_references(mut cursor: syn::buffer::Cursor<'_>, paths: &mut Vec<(Vec<String>, Span)>) {
    while !cursor.eof() {
        if let Some((ident, next)) = cursor.ident() {
            if ident.unraw() == "crate" {
                let mut look = next;
                let mut segments = vec!["crate".to_owned()];
                loop {
                    let Some((colon, next)) = look.punct() else {
                        break;
                    };
                    if colon.as_char() != ':' {
                        break;
                    }
                    let Some((colon, next)) = next.punct() else {
                        break;
                    };
                    if colon.as_char() != ':' {
                        break;
                    }
                    let Some((part, next)) = next.ident() else {
                        break;
                    };
                    segments.push(part.unraw().to_string());
                    look = next;
                    if segments.len() == 3 {
                        break;
                    }
                }
                if segments.len() == 3 && segments[1] == "emacs_core" {
                    paths.push((segments, ident.span()));
                }
            }
            cursor = next;
        } else if let Some((group, _, _, next)) = cursor.any_group() {
            macro_references(group, paths);
            cursor = next;
        } else {
            cursor = cursor.token_tree().expect("nonempty token cursor").1;
        }
    }
}

// Names an item defines directly in its module, for emacs_core root items.
fn item_name(item: &Item) -> Option<String> {
    match item {
        Item::Const(item) => Some(item.ident.to_string()),
        Item::Enum(item) => Some(item.ident.to_string()),
        Item::Fn(item) => Some(item.sig.ident.to_string()),
        Item::Static(item) => Some(item.ident.to_string()),
        Item::Struct(item) => Some(item.ident.to_string()),
        Item::Trait(item) => Some(item.ident.to_string()),
        Item::TraitAlias(item) => Some(item.ident.to_string()),
        Item::Type(item) => Some(item.ident.to_string()),
        Item::Union(item) => Some(item.ident.to_string()),
        // Module declarations are handled by visit_item_mod; macro invocations
        // and foreign items define no path-reachable name at this level.
        _ => None,
    }
}

impl<'ast> Visit<'ast> for ProductionWalker {
    fn visit_item(&mut self, item: &'ast Item) {
        if matches!(self.phase, ScanPhase::Discover)
            && self.logical_path.as_slice() == ["emacs_core"]
        {
            if let Some(name) = item_name(item) {
                self.index.ec_root_items.insert(name);
            }
        }
        if !test_only(item_attributes(item)) {
            visit::visit_item(self, item);
        }
    }

    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        let attributes = match item {
            syn::ImplItem::Const(item) => &item.attrs[..],
            syn::ImplItem::Fn(item) => &item.attrs[..],
            syn::ImplItem::Type(item) => &item.attrs[..],
            syn::ImplItem::Macro(item) => &item.attrs[..],
            _ => &[],
        };
        if !test_only(attributes) {
            visit::visit_impl_item(self, item);
        }
    }

    fn visit_trait_item(&mut self, item: &'ast syn::TraitItem) {
        let attributes = match item {
            syn::TraitItem::Const(item) => &item.attrs[..],
            syn::TraitItem::Fn(item) => &item.attrs[..],
            syn::TraitItem::Type(item) => &item.attrs[..],
            syn::TraitItem::Macro(item) => &item.attrs[..],
            _ => &[],
        };
        if !test_only(attributes) {
            visit::visit_trait_item(self, item);
        }
    }

    fn visit_item_mod(&mut self, module: &'ast ItemMod) {
        let name = module.ident.unraw().to_string();
        self.logical_path.push(name);
        if let Some((_, items)) = &module.content {
            self.discover_module();
            let directory = self.module_directory.join(module.ident.to_string());
            let previous_module = std::mem::replace(&mut self.module_directory, directory.clone());
            let previous_path = std::mem::replace(&mut self.path_directory, directory);
            for item in items {
                self.visit_item(item);
            }
            self.module_directory = previous_module;
            self.path_directory = previous_path;
        } else {
            let path = explicit_module_path(module)
                .map(|path| self.path_directory.join(path))
                .unwrap_or_else(|| {
                    let path = self.module_directory.join(module.ident.to_string());
                    let file = path.with_extension("rs");
                    if file.is_file() {
                        file
                    } else {
                        path.join("mod.rs")
                    }
                });
            self.file(&path);
        }
        self.logical_path.pop();
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        match self.phase {
            ScanPhase::Discover => {
                if matches!(item.vis, syn::Visibility::Public(_))
                    && (self.logical_path.is_empty() || self.logical_path == ["emacs_core"])
                {
                    let mut leaves = Vec::new();
                    use_leaves(&item.tree, &mut Vec::new(), &mut leaves);
                    for (path, name) in leaves {
                        self.exports.push((self.logical_path.clone(), path, name));
                    }
                }
            }
            ScanPhase::References => {
                use syn::spanned::Spanned;
                self.use_references(&item.tree, &mut Vec::new(), item.span());
            }
        }
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        use syn::spanned::Spanned;
        let segments = path
            .segments
            .iter()
            .map(|part| part.ident.unraw().to_string())
            .collect::<Vec<_>>();
        self.record(&segments, path.span());
        visit::visit_path(self, path);
    }

    fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
        if invocation.path.is_ident("include") {
            let expression = syn::parse2::<Expr>(invocation.tokens.clone())
                .expect("include! must have a source path expression");
            let path = include_path(&expression)
                .expect("teach the layering guard to resolve this include! source path");
            let path = self.source.parent().expect("source has parent").join(path);
            self.file(&path);
            return;
        }
        let tokens = syn::buffer::TokenBuffer::new2(invocation.tokens.clone());
        let mut paths = Vec::new();
        macro_references(tokens.begin(), &mut paths);
        for (path, span) in paths {
            self.record(&path, span);
        }
        self.visit_path(&invocation.path);
    }
}

// Resolve source includes rather than scanning their path string. OUT_DIR is
// supplied by this crate's build script; unknown forms fail closed so a new
// include cannot silently hide conversions from the ratchet.
fn include_path(expression: &Expr) -> Option<String> {
    match expression {
        Expr::Lit(value) => match &value.lit {
            Lit::Str(path) => Some(path.value()),
            _ => None,
        },
        Expr::Macro(value) if value.mac.path.is_ident("concat") => {
            let parts = value
                .mac
                .parse_body_with(
                    syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated,
                )
                .ok()?;
            parts.iter().map(include_path).collect::<Option<String>>()
        }
        Expr::Macro(value) if value.mac.path.is_ident("env") => {
            let name = value.mac.parse_body::<syn::LitStr>().ok()?;
            match name.value().as_str() {
                "OUT_DIR" => Some(env!("OUT_DIR").to_owned()),
                "CARGO_MANIFEST_DIR" => Some(env!("CARGO_MANIFEST_DIR").to_owned()),
                _ => None,
            }
        }
        _ => None,
    }
}

fn production_sites(root: &Path) -> (ModuleIndex, Sites) {
    let mut walker = ProductionWalker {
        phase: ScanPhase::Discover,
        source: PathBuf::new(),
        module_directory: PathBuf::new(),
        path_directory: PathBuf::new(),
        logical_path: Vec::new(),
        index: ModuleIndex::default(),
        exports: Vec::new(),
        sites: BTreeMap::new(),
    };
    walker.file(&root.join("lib.rs"));
    for (module, mut path, name) in std::mem::take(&mut walker.exports) {
        let crate_qualified = path.first().is_some_and(|part| part == "crate");
        if crate_qualified {
            path.remove(0);
        }
        let map = if module.is_empty() {
            if path.first().is_none_or(|part| part != "emacs_core") {
                continue;
            }
            path.remove(0);
            &mut walker.index.crate_exports
        } else {
            if crate_qualified {
                assert_eq!(
                    path.first().map(String::as_str),
                    Some("emacs_core"),
                    "emacs_core facade must name its origin"
                );
                path.remove(0);
            } else if path.first().is_some_and(|part| part == "self") {
                path.remove(0);
            }
            &mut walker.index.ec_exports
        };
        assert_ne!(
            name, "*",
            "facade glob re-export must name its origins explicitly"
        );
        let origin = path.first().expect("facade re-export has an origin");
        let origin = EcSub::validated(origin, &walker.index.ec_modules);
        if let Some(previous) = map.insert(name.clone(), origin.clone()) {
            assert_eq!(previous, origin, "ambiguous facade re-export {name}");
        }
    }
    walker.phase = ScanPhase::References;
    walker.file(&root.join("lib.rs"));
    (walker.index, walker.sites)
}

fn parse_ceilings(text: &str, index: &ModuleIndex) -> Ceilings {
    let mut ceilings = BTreeMap::new();
    for line in text.lines() {
        let columns = line.split_whitespace().collect::<Vec<_>>();
        assert_eq!(
            columns.len(),
            3,
            "expected <lower_root> <upper> <count>: {line}"
        );
        let upper = if let Some(top) = columns[1].strip_prefix("top:") {
            let root = LowerRoot::parse(top);
            assert!(
                !matches!(root, LowerRoot::G0),
                "g0 is a lower-root group, not a top-level module: {line}"
            );
            Upper::Top(root)
        } else {
            Upper::Ec(EcSub::validated(columns[1], &index.ec_modules))
        };
        let pair = Pair {
            lower: LowerRoot::parse(columns[0]),
            upper,
        };
        let count = columns[2].parse::<usize>().expect("integer ceiling");
        assert!(count > 0, "list nonzero pairs only");
        assert!(
            ceilings.insert(pair, count).is_none(),
            "duplicate ceiling: {line}"
        );
    }
    ceilings
}

fn current_table(sites: &Sites) -> String {
    sites
        .iter()
        .map(|(pair, sites)| {
            format!(
                "{} {} {}\n",
                pair.lower.name(),
                pair.upper.text(),
                sites.len()
            )
        })
        .collect()
}

fn assert_production_ceilings(root: &Path, sites: &Sites, ceilings: &Ceilings) {
    let mut failures = Vec::new();
    for pair in sites.keys().chain(ceilings.keys()).collect::<BTreeSet<_>>() {
        let current = sites.get(pair).map_or(0, Vec::len);
        let ceiling = ceilings.get(pair).copied().unwrap_or(0);
        if current > ceiling {
            let locations = sites[pair]
                .iter()
                .take(5)
                .map(|site| {
                    format!(
                        "{}:{}",
                        site.source
                            .strip_prefix(root)
                            .unwrap_or(&site.source)
                            .display(),
                        site.line
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            failures.push(format!("{} {}: {current} production upward references exceed the ceiling {ceiling}. Remove upward dependencies; this ceiling may only decrease.\n{locations}", pair.lower.name(), pair.upper.text()));
        } else if current < ceiling {
            failures.push(format!("{} {}: current {current} below ceiling {ceiling}; this ceiling may only decrease. Paste the FULL current table into layering_ceilings.list.", pair.lower.name(), pair.upper.text()));
        }
    }
    assert!(
        failures.is_empty(),
        "{}\nFULL CURRENT TABLE\n{}END CURRENT TABLE",
        failures.join("\n"),
        current_table(sites)
    );
}

#[test]
fn production_layering_counts_can_only_decrease() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let (index, sites) = production_sites(&root);
    let ceilings = parse_ceilings(include_str!("layering_ceilings.list"), &index);
    assert_production_ceilings(&root, &sites, &ceilings);
}

#[cfg(test)]
fn fixture(source: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("create source fixture");
    std::fs::write(directory.path().join("lib.rs"), source).expect("write crate fixture");
    directory
}

#[cfg(test)]
fn panic_message(action: impl FnOnce() + std::panic::UnwindSafe) -> String {
    let error = std::panic::catch_unwind(action).expect_err("guard must fail closed");
    if let Some(message) = error.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = error.downcast_ref::<&str>() {
        (*message).to_owned()
    } else {
        panic!("expected a string panic")
    }
}

#[test]
fn layering_guard_counts_and_names_upward_use() {
    let directory = fixture(
        "mod emacs_core { pub mod text {} }\nmod tagged { use crate::emacs_core::text::Glyph; }",
    );
    let root = directory.path();
    let (index, sites) = production_sites(root);
    assert_eq!(current_table(&sites), "tagged text 1\n");
    let message =
        panic_message(|| assert_production_ceilings(root, &sites, &parse_ceilings("", &index)));
    assert!(message.contains("lib.rs:2"), "{message}");
    assert!(
        message.contains("tagged text: 1 production upward references exceed the ceiling 0"),
        "{message}"
    );
}

#[test]
fn layering_guard_skips_test_only_modules() {
    let directory = fixture(
        r#"
        mod emacs_core { pub mod text {} }
        mod tagged {
            #[cfg(test)] mod tests { use crate::emacs_core::text::Glyph; }
            #[cfg(all(unix, test))] mod missing;
            #[cfg(not(not(test)))] mod negated;
            #[cfg(any(test, all(test, feature = "fuzzing")))] mod combined;
            #[test] fn test() { crate::emacs_core::text::run(); }
        }
    "#,
    );
    let (index, sites) = production_sites(directory.path());
    assert!(sites.is_empty());
    assert_production_ceilings(directory.path(), &sites, &parse_ceilings("", &index));
}

#[test]
fn layering_guard_resolves_facades_and_super_chains() {
    let directory = fixture(
        r#"
        mod emacs_core { pub mod value {} pub use value::{Value, ValueKind as Kind}; }
        pub use emacs_core::value::{Value, ValueKind as Kind};
        mod tagged {
            use super::Value;
            mod inner {
                use crate::Value;
                use super::super::Value;
                use super::Value;
                use super::emacs_core::value::Value;
                use crate::emacs_core::Kind;
                use crate::Kind;
                use self::Value;
                use super::gc;
            }
        }
    "#,
    );
    let (index, sites) = production_sites(directory.path());
    assert_eq!(current_table(&sites), "tagged value 6\n");
    assert_production_ceilings(
        directory.path(),
        &sites,
        &parse_ceilings("tagged value 6", &index),
    );
}

#[test]
#[should_panic(expected = "unknown top-level production module surprise; add a LowerRoot variant")]
fn layering_guard_rejects_unknown_lower_root() {
    let directory = fixture("mod surprise {}");
    production_sites(directory.path());
}

#[test]
fn layering_guard_requires_pasting_lowered_ceilings() {
    let directory = fixture(
        "mod emacs_core { pub mod value {} pub mod text {} } mod tagged { use crate::emacs_core::value::Value; }",
    );
    let (index, sites) = production_sites(directory.path());
    let ceilings = parse_ceilings("tagged value 2\ntagged text 1", &index);
    let message = panic_message(|| assert_production_ceilings(directory.path(), &sites, &ceilings));
    assert!(
        message.contains("this ceiling may only decrease"),
        "{message}"
    );
    assert!(
        message.contains("tagged text: current 0 below ceiling 1"),
        "{message}"
    );
    assert!(
        message.contains("FULL CURRENT TABLE\ntagged value 1\nEND CURRENT TABLE"),
        "{message}"
    );
}

#[test]
fn layering_guard_follows_paths_includes_and_macro_tokens() {
    let directory = fixture(
        r###"
        mod emacs_core { pub mod text {} pub mod value {} }
        #[path = "tests/runtime.rs"] mod tagged;
        mod buffer { mod nested { mod child; } }
    "###,
    );
    let root = directory.path();
    std::fs::create_dir(root.join("tests")).expect("create path fixture");
    std::fs::write(
        root.join("tests/runtime.rs"),
        r###"
        include!(concat!("included", ".rs"));
        #[cfg(not(test))] fn run() { crate::emacs_core::text::run(); }
        #[cfg(any(test, feature = "fuzzing"))] fn mixed() { crate::emacs_core::text::run(); }
        macro_rules! paths { () => { (crate::emacs_core::r#value::Value, super::Value) }; }
        macro_rules! strings { () => { r#"crate::emacs_core::text::Glyph"# }; }
    "###,
    )
    .expect("write path fixture");
    std::fs::write(
        root.join("tests/included.rs"),
        "use super::emacs_core::text::{Glyph, Other};",
    )
    .expect("write included fixture");
    std::fs::create_dir_all(root.join("buffer/nested")).expect("create nested fixture");
    std::fs::write(
        root.join("buffer/nested/child.rs"),
        "use super::super::super::emacs_core::text::Glyph;",
    )
    .expect("write nested fixture");
    let (index, sites) = production_sites(root);
    assert_eq!(
        current_table(&sites),
        "buffer text 1\ntagged text 3\ntagged value 1\n"
    );
    assert_production_ceilings(
        root,
        &sites,
        &parse_ceilings(&current_table(&sites), &index),
    );
}

#[test]
fn layering_guard_rejects_unknown_emacs_core_origin() {
    let directory = fixture(
        "mod emacs_core { pub mod value {} }\nmod tagged { use crate::emacs_core::Missing; }",
    );
    let message = panic_message(|| {
        production_sites(directory.path());
    });
    assert!(
        message.contains("lib.rs:2: unknown emacs_core submodule or re-export Missing"),
        "{message}"
    );
}

#[test]
fn layering_guard_counts_emacs_core_root_items() {
    let directory = fixture(
        "mod emacs_core { pub mod text {} pub fn eval_source() {} }\nmod tagged { use crate::emacs_core::eval_source; }",
    );
    let (index, sites) = production_sites(directory.path());
    assert_eq!(current_table(&sites), "tagged (root) 1\n");
    assert_production_ceilings(
        directory.path(),
        &sites,
        &parse_ceilings("tagged (root) 1", &index),
    );
}

#[test]
fn layering_guard_resolves_qualified_nested_exports() {
    let directory = fixture(
        r#"
        mod emacs_core {
            pub mod value {}
            pub mod text {}
            pub use crate::emacs_core::value::{Value, ValueKind as Kind};
            pub use self::text::Glyph;
        }
        pub use tagged::*;
        pub use crate::emacs_core::{value::Value, text::Glyph as Shape};
        mod tagged {
            use crate::{Value, Shape};
            use crate::emacs_core::{Kind, Glyph};
            use crate::emacs_core::{self, self as ec};
            use crate::emacs_core::value::{Value, ValueKind};
        }
    "#,
    );
    let (index, sites) = production_sites(directory.path());
    assert_eq!(current_table(&sites), "tagged text 2\ntagged value 3\n");
    assert_production_ceilings(
        directory.path(),
        &sites,
        &parse_ceilings(&current_table(&sites), &index),
    );
}

#[test]
fn layering_guard_skips_file_level_test_modules() {
    let directory = fixture("mod surprise; mod emacs_core { mod checks; }");
    std::fs::write(
        directory.path().join("surprise.rs"),
        "#![cfg(test)] use crate::emacs_core::Missing;",
    )
    .expect("write test-only module");
    std::fs::create_dir(directory.path().join("emacs_core")).expect("create module directory");
    std::fs::write(
        directory.path().join("emacs_core/checks.rs"),
        "#![cfg(test)] mod missing;",
    )
    .expect("write test-only submodule");
    let (index, sites) = production_sites(directory.path());
    assert!(index.ec_modules.is_empty());
    assert!(sites.is_empty());
}

#[test]
fn layering_guard_counts_g0_emacs_core_edges() {
    let directory = fixture(
        r#"
        mod emacs_core {
            pub mod value {
                use crate::emacs_core::print::Glyph;
                use crate::buffer::BufferManager;
            }
            pub mod error {
                use crate::emacs_core::value::Value;
                use crate::tagged::SymbolMode;
            }
            pub mod print { pub struct Glyph; }
        }
        mod buffer {}
        mod tagged {}
    "#,
    );
    let root = directory.path();
    let (index, sites) = production_sites(root);
    assert_eq!(current_table(&sites), "g0 print 1\ng0 top:buffer 1\n");
    assert_production_ceilings(
        root,
        &sites,
        &parse_ceilings(&current_table(&sites), &index),
    );
}

#[test]
fn layering_guard_g0_planted_upward_reference_fails() {
    let directory = fixture(
        "mod emacs_core { pub mod value { use crate::emacs_core::print::Glyph; } pub mod print {} }",
    );
    let root = directory.path();
    let (index, sites) = production_sites(root);
    let message =
        panic_message(|| assert_production_ceilings(root, &sites, &parse_ceilings("", &index)));
    assert!(message.contains("lib.rs:1"), "{message}");
    assert!(
        message.contains("g0 print: 1 production upward references exceed the ceiling 0"),
        "{message}"
    );
}

#[test]
fn layering_guard_g0_requires_pasting_lowered_ceilings() {
    let directory = fixture(
        "mod emacs_core { pub mod value { use crate::buffer::BufferManager; } } mod buffer {}",
    );
    let root = directory.path();
    let (index, sites) = production_sites(root);
    let ceilings = parse_ceilings("g0 top:buffer 2", &index);
    let message = panic_message(|| assert_production_ceilings(root, &sites, &ceilings));
    assert!(
        message.contains("g0 top:buffer: current 1 below ceiling 2"),
        "{message}"
    );
    assert!(
        message.contains("FULL CURRENT TABLE\ng0 top:buffer 1\nEND CURRENT TABLE"),
        "{message}"
    );
}

#[test]
#[should_panic(expected = "unknown emacs_core submodule or re-export Missing")]
fn layering_guard_rejects_unknown_g0_emacs_core_target() {
    let directory = fixture("mod emacs_core { pub mod value { use crate::emacs_core::Missing; } }");
    production_sites(directory.path());
}

#[test]
#[should_panic(expected = "unknown top-level module or crate re-export surprise")]
fn layering_guard_rejects_unknown_g0_top_level_target() {
    let directory = fixture("mod emacs_core { pub mod value { use crate::surprise::Glyph; } }");
    production_sites(directory.path());
}

//! Ratchet upward references in every production module, in every feature
//! configuration. Follow module declarations and resolve facade re-exports so
//! moving a reference behind a public name cannot hide a layering dependency.

use proc_macro2::Span;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use syn::ext::IdentExt;
use syn::visit::{self, Visit};
use syn::{Attribute, Expr, Item, ItemMod, Lit, Meta};

use super::architecture_moving_set::{
    BOTTOM_PEER_MODULES, G0_MEMBERS, is_bottom_peer_name, is_g0_member, is_moving_crate,
};
use super::architecture_production_source_roots::{crates_dir, production_source_roots};

/// Frozen pre-split item map (ruling M-a): (source crate, written path) ->
/// historical target. Generated from the last pre-split tree (773d3b64a5) by
/// the approved M1-M3 derivation. The row count is pinned and may only
/// shrink; an unmapped item falls back to the current module model (M-b).
const ITEM_MAP_TSV: &str = include_str!("layering_item_map.tsv");
const ITEM_MAP_ROWS: usize = 128;

fn item_map() -> &'static BTreeMap<(String, String), String> {
    static MAP: std::sync::OnceLock<BTreeMap<(String, String), String>> =
        std::sync::OnceLock::new();
    MAP.get_or_init(|| {
        let mut map = BTreeMap::new();
        for line in ITEM_MAP_TSV.lines() {
            if line.is_empty()
                || line.starts_with("source_file")
                || line.starts_with("source_crate")
            {
                continue;
            }
            let columns = line.split('\t').collect::<Vec<_>>();
            assert_eq!(columns.len(), 3, "bad item-map row: {line}");
            let previous = map.insert(
                (columns[0].to_owned(), columns[1].to_owned()),
                columns[2].to_owned(),
            );
            assert!(
                previous.is_none(),
                "duplicate item-map row: {} {}",
                columns[0],
                columns[1]
            );
        }
        map
    })
}

fn check_item_map_count(rows: usize) {
    assert!(
        rows <= ITEM_MAP_ROWS,
        "item map row count increased: {rows} > pinned {ITEM_MAP_ROWS}; the map is frozen (M-a)"
    );
    assert_eq!(
        rows, ITEM_MAP_ROWS,
        "item map row count is pinned at {ITEM_MAP_ROWS}; lower the pin in the commit that prunes rows"
    );
}

fn stale_item_map_rows<'a>(
    map: &'a BTreeMap<(String, String), String>,
    observed: &BTreeSet<(String, String)>,
) -> Vec<&'a (String, String)> {
    map.keys()
        .filter(|key| !observed.contains(*key))
        .collect::<Vec<_>>()
}

/// Dump accounting is opt-in: tests must not pay for the site ledger.
fn dump_all() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var_os("NEOVM_LAYERING_DUMP_ALL").is_some()
            || std::env::var_os("NEOVM_LAYERING_DUMP").is_some()
    })
}

/// The workspace-relative source path the item map is keyed by. The map is
/// per SOURCE FILE, not per crate: the same written path in a counted lower
/// root and in an upper emacs_core member file has different accounting (R2).
fn source_map_key(source: &Path) -> Option<String> {
    let text = source.to_string_lossy().replace('\\', "/");
    if let Some(index) = text.find("/crates/") {
        return Some(text[index + 1..].to_owned());
    }
    text.starts_with("crates/").then_some(text)
}

fn item_map_target(source: &Path, written: &str) -> Option<&'static str> {
    let key = source_map_key(source)?;
    item_map()
        .get(&(key, written.to_owned()))
        .map(String::as_str)
}

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
    /// The host input publisher/connection seam added with the parallel-Lisp
    /// input work (main b243, ipc CONT38). A top-level production module like
    /// the other lower roots: references out of it are counted as upward.
    HostInput,
    /// The owner-selected native host-effect contracts added with the
    /// parallel-Lisp effect work. A top-level production module like the
    /// other lower roots; its contracts name no emacs_core item today.
    HostEffect,
    /// The debug-only carrier TLS fingerprint helpers added with the bounded
    /// carrier pool (main f479e27499). A top-level production module like the
    /// other lower roots; it names no emacs_core item.
    TlsFingerprint,
    /// The cross-thread quit request and the asynchronous attention word
    /// moved from eval with stage-3b T4.1. A top-level production module like
    /// the other lower roots; it names no emacs_core item.
    Quit,
    /// The system vocabulary moved down for the stage-4 eval split (W1/E3):
    /// errno, platform facts, the C-feature table, path_exec, the wait
    /// timeout and startup constants. A top-level production module like the
    /// other lower roots; it names no emacs_core item.
    SystemVocab,
    /// The Context-owned semantic-registry substrate moved from the core
    /// runtime with stage-3b T3. A top-level production module like the
    /// other lower roots; its one reach below the evaluator is
    /// `neovm_value::tagged::gc`, and it names no emacs_core item.
    HeapRegistry,
    /// The Lisp-command plist layer moved to the bottom `neovm-value` crate
    /// with the stage-3 split (C10), beside its `symbol::plist_core`
    /// primitives. A top-level lower root like the other bottom-peer modules.
    Plist,
    /// The string-byte observation cache re-homed to `neovm-value` with the
    /// split. A bottom-peer top-level module like the other lower roots; it
    /// names no evaluator item.
    StringPosCache,
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
            "host_input" => Self::HostInput,
            "host_effect" => Self::HostEffect,
            "tls_fingerprint" => Self::TlsFingerprint,
            "quit" => Self::Quit,
            "system_vocab" => Self::SystemVocab,
            "heap_registry" => Self::HeapRegistry,
            "plist" => Self::Plist,
            "string_pos_cache" => Self::StringPosCache,
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
            Self::HostInput => "host_input",
            Self::HostEffect => "host_effect",
            Self::TlsFingerprint => "tls_fingerprint",
            Self::Quit => "quit",
            Self::SystemVocab => "system_vocab",
            Self::HeapRegistry => "heap_registry",
            Self::Plist => "plist",
            Self::StringPosCache => "string_pos_cache",
        }
    }
}

/// Bottom-peer membership is shared with the coherence ratchet through
/// `architecture_moving_set`, so the two guards and the move manifest use one
/// rule for which root is inside the bottom crate.
fn is_bottom_peer(root: LowerRoot) -> bool {
    is_bottom_peer_name(root.name())
}

/// Historical `emacs_core` submodules that now live behind a facade in a
/// lower crate (`emacs_core::plist` and the stage-3/4 movers). Their ceiling
/// rows keep the historical Ec spelling, so the parser accepts the name even
/// though the post-split facade no longer declares a direct submodule.
fn is_historical_ec_bottom_peer(name: &str) -> bool {
    matches!(
        name,
        "plist" | "string_pos_cache" | "heap_registry" | "system_vocab"
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

    /// A name recorded by the ceilings file itself. The file IS the
    /// historical vocabulary: after the split a row can name an emacs_core
    /// submodule that is no longer a direct submodule (window_edge_drag,
    /// plist, ...). Scanning still validates; only the historical parse is
    /// permissive.
    fn historical(name: &str) -> Self {
        Self(name.to_owned())
    }

    fn validated(name: &str, modules: &BTreeSet<String>) -> Self {
        assert!(
            name == Self::ROOT
                || modules.contains(name)
                || is_historical_ec_bottom_peer(name)
                || is_g0_member(name),
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
    /// A workspace-sibling crate (for example `neomacs_display_protocol`)
    /// named by a moving-set file. Registry dependencies belong to the value
    /// crate's Cargo manifest, not to the layering ratchet; workspace
    /// siblings would be a value -> core/UI edge and are counted.
    Extern(String),
    /// A named module of a workspace-sibling crate. After the split the
    /// historical emacs_core modules live in the extracted crates, and upper
    /// code reaches them either through a crate-qualified import
    /// (`neovm_value::value::Value`) or through the crate's `emacs_core`
    /// facade re-export. Naming the module keeps the ratchet's rows
    /// re-attributable one-to-one instead of collapsing them into the crate.
    ExternModule { crate_name: String, module: String },
}

impl Upper {
    /// The ceilings-file spelling: an emacs_core submodule by name, a
    /// top-level module as `top:<name>`, a workspace sibling crate as
    /// `extern:<crate>`. `:` cannot occur in an identifier, so the
    /// namespaces cannot collide.
    fn text(&self) -> String {
        match self {
            Self::Ec(sub) => sub.0.clone(),
            Self::Top(root) => format!("top:{}", root.name()),
            Self::Extern(name) => format!("extern:{name}"),
            Self::ExternModule { crate_name, module } => {
                format!("extern:{crate_name}::{module}")
            }
        }
    }
}

/// Workspace-sibling crates use the neomacs/neovm naming families. The
/// current crate itself (`neovm_core`) is not a sibling reference.
fn is_workspace_sibling(name: &str) -> bool {
    (name.starts_with("neomacs_") || name.starts_with("neovm_")) && name != "neovm_core"
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
    reference: String,
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
    /// Items defined directly in `lib.rs`, reachable as `crate::name` without
    /// naming a module. These are not layering edges.
    crate_root_items: BTreeSet<String>,
    /// Crate-root re-exports of modules that live in the moving crate (for
    /// example `pub(crate) use neovm_value::tls_scope;`). A moving file that
    /// names one through `crate::` still targets the moving set.
    moving_crate_exports: BTreeMap<String, String>,
    /// Every (source crate, written path) the reference scan observed; the
    /// completeness self-test proves the frozen item map has no stale rows.
    observed_paths: BTreeSet<(String, String)>,
    /// Dump accounting (D-a): every NotUpward site with its reason and its
    /// logical lower root. Collected only when NEOVM_LAYERING_DUMP_ALL is set.
    not_upward_sites: Vec<(PathBuf, usize, String, String, String)>,
    /// Dump diagnostics: (source file, logical path) for every visited file.
    visited_files: Vec<(PathBuf, Vec<String>)>,
    crate_exports: BTreeMap<String, EcSub>,
    /// `emacs_core/mod.rs` facade re-exports. The origin is usually a direct
    /// submodule, but after the split the facade also re-exports extracted
    /// crates (`pub use neovm_display::emacs_core::display;`), which are
    /// workspace-sibling edges.
    ec_exports: BTreeMap<String, Upper>,
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
    /// One physical file under one logical module is scanned once per phase.
    /// The split wires the same source under several cfg arms
    /// (`#[cfg(not(feature = "test-support"))] mod x;` beside
    /// `#[cfg(feature = "test-support")] pub mod x;` with the same `#[path]`),
    /// and counting both declarations would double every reference in the
    /// file.
    visited: BTreeSet<(u8, PathBuf, Vec<String>)>,
}

impl ProductionWalker {
    fn file(&mut self, path: &Path) {
        let phase = match self.phase {
            ScanPhase::Discover => 0,
            ScanPhase::References => 1,
        };
        if !self
            .visited
            .insert((phase, path.to_path_buf(), self.logical_path.clone()))
        {
            return;
        }
        if dump_all() && matches!(self.phase, ScanPhase::References) {
            self.index
                .visited_files
                .push((path.to_path_buf(), self.logical_path.clone()));
        }
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

    /// Walk `path` as if its module were declared at `logical_path`. The
    /// extracted crates are seeded this way: their physical `lib.rs` no longer
    /// names the historical root modules (and the value crate renamed them).
    fn file_with_logical(&mut self, path: &Path, logical_path: Vec<String>) {
        let previous = std::mem::replace(&mut self.logical_path, logical_path);
        self.file(path);
        self.logical_path = previous;
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

    /// Dump accounting (D-a): why a reference was not counted.
    fn not_upward_reason(&self, segments: &[String]) -> String {
        let written = segments.join("::");
        if item_map_target(&self.source, &written) == Some("lateral") {
            return "lateral".to_owned();
        }
        let Some(first) = segments.first() else {
            return "self".to_owned();
        };
        if matches!(first.as_str(), "self" | "super") {
            return "self".to_owned();
        }
        if is_workspace_sibling(first) {
            return format!("r1-cross-crate:{}", first.replace('-', "_"));
        }
        if let Some(directory) = source_crate_dir(&self.source) {
            let source_crate = directory.replace('-', "_");
            if is_moving_crate(&source_crate) {
                return "moving-internal".to_owned();
            }
        }
        "moving-internal".to_owned()
    }

    /// Record a NotUpward site for the dump when the accounting env is on.
    fn observe_not_upward(&mut self, segments: &[String], span: Span) {
        if !dump_all() || !matches!(self.phase, ScanPhase::References) {
            return;
        }
        let reason = self.not_upward_reason(segments);
        let lower = match self.logical_path.as_slice() {
            [root, member, ..] if root == "emacs_core" => {
                if is_g0_member(member) {
                    "g0".to_owned()
                } else {
                    "(upper)".to_owned()
                }
            }
            [root, ..] => root.clone(),
            [] => "(root)".to_owned(),
        };
        self.index.not_upward_sites.push((
            self.source.clone(),
            span.start().line,
            segments.join("::"),
            reason,
            lower,
        ));
    }

    /// Record a written path for the item-map completeness self-test.
    fn observe_path(&mut self, segments: &[String]) {
        if !matches!(self.phase, ScanPhase::References) {
            return;
        }
        let Some(key) = source_map_key(&self.source) else {
            return;
        };
        self.index.observed_paths.insert((key, segments.join("::")));
    }

    /// Classification of a frozen item-map target. The generator already
    /// applied the moving rule to the historical target, so the spec is
    /// authoritative: `lateral` is NotUpward, `ec:`/`root` are counted rows,
    /// and `top:` is a top-level module row.
    fn map_classification(&self, spec: &str, lower: LowerRoot) -> Classification {
        if spec == "lateral" {
            return Classification::NotUpward;
        }
        if spec == "root" {
            return Classification::Upward(Pair {
                lower,
                upper: Upper::Ec(EcSub::root()),
            });
        }
        if let Some(name) = spec.strip_prefix("top:") {
            return Classification::Upward(Pair {
                lower,
                upper: Upper::Top(LowerRoot::parse(name)),
            });
        }
        let Some(name) = spec.strip_prefix("ec:") else {
            panic!("unknown item-map target {spec}");
        };
        Classification::Upward(Pair {
            lower,
            upper: Upper::Ec(EcSub::historical(name)),
        })
    }

    fn classify(&self, segments: &[String], span: Span) -> Classification {
        let Some(lower) = self.logical_path.first() else {
            return Classification::NotUpward;
        };
        if lower == "emacs_core" {
            // The item map applies to a file's HISTORY when that file is a
            // G0 member. The same physical file is also visited through its
            // crate-local emacs_core facade (logical emacs_core::<member>);
            // that visit is the upper-module model and stays NotUpward, so
            // the map must not double-count it with a G0 lower root.
            let member_is_g0 = self
                .logical_path
                .get(1)
                .is_some_and(|member| is_g0_member(member));
            if member_is_g0 {
                if let Some(spec) = item_map_target(&self.source, &segments.join("::")) {
                    return self.map_classification(spec, LowerRoot::G0);
                }
            }
            return self.classify_from_emacs_core(segments, span);
        }
        let source_root = LowerRoot::parse(lower);
        if let Some(spec) = item_map_target(&self.source, &segments.join("::")) {
            return self.map_classification(spec, source_root);
        }
        let moving = is_bottom_peer(source_root);
        let Some(first) = segments.first() else {
            return Classification::NotUpward;
        };
        // Peer ruling (16:0xZ): frontend_events -> super::keyboard stays in
        // the moving set. The exact old-model LowerRoot line is
        // guard-old-proof.rs:397 `"keyboard" => Self::Keyboard,` (with
        // guard-old-proof.rs:392 `"frontend_events" => Self::FrontendEvents,`).
        if first == "super"
            && self.logical_path.first().map(String::as_str) == Some("frontend_events")
            && segments.get(1).map(String::as_str) == Some("keyboard")
        {
            return Classification::NotUpward;
        }
        if !matches!(first.as_str(), "crate" | "super") {
            // `self::` stays inside the module.
            if is_workspace_sibling(first) {
                // R1: a reference into another crate is a normal cargo
                // dependency edge. Cargo guarantees the crate graph is
                // acyclic, so a cross-crate edge is never an upward layering
                // edge, whatever the historical module names say.
                return Classification::NotUpward;
            }
            return Classification::NotUpward;
        }
        let remainder = if first == "crate" {
            &segments[1..]
        } else {
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
        };
        let Some(name) = remainder.first() else {
            return Classification::NotUpward;
        };
        if name == "emacs_core" {
            let Some(name) = remainder.get(1) else {
                return Classification::NotUpward;
            };
            let upper = if self.index.ec_modules.contains(name) {
                Upper::Ec(EcSub::validated(name, &self.index.ec_modules))
            } else if let Some(origin) = self.index.ec_exports.get(name) {
                origin.clone()
            } else if self.index.ec_root_items.contains(name) {
                Upper::Ec(EcSub::root())
            } else {
                panic!(
                    "{}:{}: unknown emacs_core submodule or re-export {name}",
                    self.source.display(),
                    span.start().line
                )
            };
            // R1: a facade that re-exports another crate is a cross-crate
            // edge, never upward.
            if matches!(&upper, Upper::Extern(_) | Upper::ExternModule { .. }) {
                return Classification::NotUpward;
            }
            // A G0 emacs_core submodule moves with this source; references
            // between moving-set files stay inside the extracted crate.
            if moving && matches!(&upper, Upper::Ec(sub) if is_g0_member(&sub.0)) {
                return Classification::NotUpward;
            }
            return Classification::Upward(Pair {
                lower: source_root,
                upper,
            });
        }
        if let Some(origin) = self.index.crate_exports.get(name) {
            if moving && is_g0_member(&origin.0) {
                return Classification::NotUpward;
            }
            return Classification::Upward(Pair {
                lower: source_root,
                upper: Upper::Ec(origin.clone()),
            });
        }
        if moving {
            // A bottom-peer module's `crate::` reference to a G0 member (the
            // value crate's `value`, `symbol`, ... roots) stays inside the
            // moved crate. G0 members are seeded under their historical
            // `emacs_core` paths, so they never enter `crate_modules`.
            if is_g0_member(name) {
                return Classification::NotUpward;
            }
            if self.index.crate_modules.contains(name) {
                let root = LowerRoot::parse(name);
                if is_bottom_peer(root) {
                    return Classification::NotUpward;
                }
                return Classification::Upward(Pair {
                    lower: source_root,
                    upper: Upper::Top(root),
                });
            }
            if self.index.moving_crate_exports.contains_key(name) || is_moving_crate(name) {
                return Classification::NotUpward;
            }
            if is_workspace_sibling(name) {
                // R1: cross-crate edges are never upward.
                return Classification::NotUpward;
            }
            if self.index.crate_root_items.contains(name) {
                return Classification::NotUpward;
            }
            // Q1 + R3: inside a bottom crate, a crate-root path to a module
            // that physically lives in this crate stays internal.
            if same_crate_bottom_module(&self.source, name) {
                return Classification::NotUpward;
            }
            // A moving file's crate-root path that names no module, facade,
            // moving-crate re-export or root item is an edge kind this guard
            // cannot classify. Fail closed instead of silently skipping it.
            panic!(
                "{}:{}: unknown top-level production module {name}; teach the layering guard this edge",
                self.source.display(),
                span.start().line
            );
        }
        Classification::NotUpward
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
        if !matches!(
            segments.first().map(String::as_str),
            Some("crate" | "super" | "self")
        ) {
            // A G0 member must not reference a sibling workspace crate; that
            // would be a value -> core/UI dependency after extraction.
            if let Some(first) = segments.first()
                && is_workspace_sibling(first)
            {
                // R1: cross-crate edges are never upward.
                return Classification::NotUpward;
            }
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
                Upper::Ec(EcSub::validated(name, &self.index.ec_modules))
            } else if let Some(origin) = self.index.ec_exports.get(name) {
                origin.clone()
            } else if self.index.ec_root_items.contains(name) {
                Upper::Ec(EcSub::root())
            } else {
                panic!(
                    "{}:{}: unknown emacs_core submodule or re-export {name}",
                    self.source.display(),
                    span.start().line
                )
            };
            if matches!(&upper, Upper::Extern(_) | Upper::ExternModule { .. }) {
                return Classification::NotUpward;
            }
            if matches!(&upper, Upper::Ec(sub) if is_g0_member(&sub.0)) {
                return Classification::NotUpward;
            }
            return Classification::Upward(Pair {
                lower: LowerRoot::G0,
                upper,
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
        if self.index.moving_crate_exports.contains_key(first) || is_moving_crate(first) {
            return Classification::NotUpward;
        }
        if self.index.crate_root_items.contains(first) {
            return Classification::NotUpward;
        }
        if is_workspace_sibling(first) {
            // R1: cross-crate edges are never upward.
            return Classification::NotUpward;
        }
        // A G0 member's `crate::` reference to another G0 member or a
        // bottom-peer module stays inside the moved crate. Only bottom peers
        // have a historical crate-root spelling in `crate_modules` after the
        // split, so G0 targets (`crate::symbol`, `crate::alloc`, ...) must be
        // recognized here before the unknown-module assertion.
        if is_g0_member(first) || is_bottom_peer_name(first) {
            return Classification::NotUpward;
        }
        // Q1 + R3: the value crate is bottom; its crate-root refs to modules
        // that physically live in it (crate::symbol, crate::alloc, ...) stay
        // internal instead of failing the unknown-module assertion.
        if same_crate_bottom_module(&self.source, first) {
            return Classification::NotUpward;
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
        self.observe_path(segments);
        match self.phase {
            ScanPhase::Discover => false,
            ScanPhase::References => match self.classify(segments, span) {
                Classification::Upward(pair) => {
                    assert_intra_crate_upper(&pair.upper);
                    self.sites.entry(pair).or_default().push(Site {
                        source: self.source.clone(),
                        line: span.start().line,
                        reference: segments.join("::"),
                    });
                    true
                }
                Classification::NotUpward => {
                    self.observe_not_upward(segments, span);
                    false
                }
            },
        }
    }

    /// Every leaf of a use statement is classified and the statement records
    /// one site per distinct target pair. The frozen item map overrides the
    /// shallow prefix classification for mapped leaves; unmapped leaves keep
    /// the current module model (M-b). A glob import counts at its module.
    /// Pre-split counting rule: a use statement records ONE site per shared
    /// upward prefix of its use tree (the old walk recorded at the first
    /// upward prefix and stopped descending). Item-level attribution happens
    /// at that same boundary: the site names the historical row of the first
    /// upward leaf under the prefix, so a grouped import under one prefix
    /// stays one site even when its leaves name several items.
    fn use_references(&mut self, tree: &syn::UseTree, prefix: &mut Vec<String>, span: Span) {
        self.use_walk(tree, prefix, span);
    }

    fn use_walk(&mut self, tree: &syn::UseTree, prefix: &mut Vec<String>, span: Span) {
        match tree {
            syn::UseTree::Path(path) => {
                prefix.push(path.ident.unraw().to_string());
                if matches!(self.classify(prefix, span), Classification::Upward(_)) {
                    self.record_use_subtree(&path.tree, prefix, span);
                } else {
                    self.use_walk(&path.tree, prefix, span);
                }
                prefix.pop();
            }
            syn::UseTree::Name(name) => {
                let terminal_self = name.ident == "self" && !prefix.is_empty();
                if !terminal_self {
                    prefix.push(name.ident.unraw().to_string());
                }
                self.record_use_leaf(prefix, span);
                if !terminal_self {
                    prefix.pop();
                }
            }
            syn::UseTree::Rename(rename) => {
                let terminal_self = rename.ident == "self" && !prefix.is_empty();
                if !terminal_self {
                    prefix.push(rename.ident.unraw().to_string());
                }
                self.record_use_leaf(prefix, span);
                if !terminal_self {
                    prefix.pop();
                }
            }
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    self.use_walk(item, prefix, span);
                }
            }
            syn::UseTree::Glob(_) => {
                self.record_use_leaf(prefix, span);
            }
        }
    }

    fn push_site(&mut self, pair: Pair, reference: String, span: Span) {
        self.sites.entry(pair).or_default().push(Site {
            source: self.source.clone(),
            line: span.start().line,
            reference,
        });
    }

    fn record_use_leaf(&mut self, segments: &[String], span: Span) {
        self.observe_path(segments);
        match self.classify(segments, span) {
            Classification::Upward(pair) => {
                assert_intra_crate_upper(&pair.upper);
                self.push_site(pair, segments.join("::"), span);
            }
            Classification::NotUpward => self.observe_not_upward(segments, span),
        }
    }

    /// Attribute the subtree under an already-upward prefix to its first
    /// upward leaf; the whole subtree is ONE site (pre-split shared-prefix
    /// rule). Every leaf is observed for the frozen-map completeness test.
    fn record_use_subtree(&mut self, subtree: &syn::UseTree, prefix: &mut Vec<String>, span: Span) {
        let mut leaves = Vec::new();
        use_leaves(subtree, prefix, &mut leaves);
        let mut chosen: Option<(Pair, String)> = None;
        for (path, _name) in leaves {
            let segments = if path.last().is_some_and(|part| part == "*") {
                path[..path.len() - 1].to_vec()
            } else {
                path
            };
            self.observe_path(&segments);
            if chosen.is_none() {
                match self.classify(&segments, span) {
                    Classification::Upward(pair) => {
                        assert_intra_crate_upper(&pair.upper);
                        chosen = Some((pair, segments.join("::")));
                    }
                    Classification::NotUpward => self.observe_not_upward(&segments, span),
                }
            }
        }
        if let Some((pair, reference)) = chosen {
            self.push_site(pair, reference, span);
        }
        // No leaf resolved upward: the subtree's leaves are lateral/self or
        // outside the model. The facade prefix is NOT a substitute site; a
        // mapped lateral item must stay NotUpward (T3a), never re-counted
        // through the module it was re-exported from.
    }
}

// Macro bodies are token streams, not ordinary syn paths. Only literal
// crate:: paths count (three segments, enough for crate::<module>::<item>);
// super is intentionally skipped. classify() decides whether the source
// moves and whether the target leaves the moving set.
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
                // `$crate::` and `crate::` paths can reach an emacs_core
                // submodule, a top-level module or a workspace sibling;
                // classify() decides whether the source moves and whether
                // the target leaves the moving set.
                if segments.len() >= 2 {
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
        if matches!(self.phase, ScanPhase::Discover) {
            if self.logical_path.as_slice() == ["emacs_core"] {
                if let Some(name) = item_name(item) {
                    self.index.ec_root_items.insert(name);
                }
            } else if self.logical_path.is_empty() {
                if let Some(name) = item_name(item) {
                    self.index.crate_root_items.insert(name);
                }
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
                let mut leaves = Vec::new();
                use_leaves(&item.tree, &mut Vec::new(), &mut leaves);
                if self.logical_path.is_empty() {
                    // Any root binding of a moving-crate module is the
                    // crate:: facade a moving file can use, whatever its
                    // visibility: descendants can see the root's private
                    // imports.
                    for (path, name) in &leaves {
                        if path.first().is_some_and(|part| is_moving_crate(part)) {
                            let module = path.get(1).cloned().unwrap_or_else(|| name.clone());
                            self.index.moving_crate_exports.insert(name.clone(), module);
                        }
                    }
                }
                if matches!(item.vis, syn::Visibility::Public(_))
                    && (self.logical_path.is_empty() || self.logical_path == ["emacs_core"])
                {
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

fn new_walker() -> ProductionWalker {
    ProductionWalker {
        phase: ScanPhase::Discover,
        source: PathBuf::new(),
        module_directory: PathBuf::new(),
        path_directory: PathBuf::new(),
        logical_path: Vec::new(),
        index: ModuleIndex::default(),
        exports: Vec::new(),
        sites: BTreeMap::new(),
        visited: BTreeSet::new(),
    }
}

/// Physical entry file for a module root in `src` (`name/mod.rs` or
/// `name.rs`).
fn module_entry(src: &Path, name: &str) -> Option<PathBuf> {
    let directory = src.join(name).join("mod.rs");
    if directory.is_file() {
        return Some(directory);
    }
    let file = src.join(format!("{name}.rs"));
    file.is_file().then_some(file)
}

/// The crate directory below `crates/` that owns `path`.
fn source_crate_dir(path: &Path) -> Option<String> {
    let parts = path
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let index = parts.iter().position(|part| part == "crates")?;
    parts.get(index + 1).cloned()
}

fn source_crate_src(path: &Path) -> Option<PathBuf> {
    let directory = source_crate_dir(path)?;
    let src = crates_dir().join(directory).join("src");
    src.is_dir().then_some(src)
}

/// Internal dependency names from one manifest (all dependency tables).
fn internal_dependencies(doc: &toml::Table) -> BTreeSet<String> {
    fn collect(dependencies: Option<&toml::Value>, sink: &mut BTreeSet<String>) {
        let Some(dependencies) = dependencies.and_then(|value| value.as_table()) else {
            return;
        };
        for name in dependencies.keys() {
            if name.starts_with("neovm-") || name.starts_with("neomacs-") {
                sink.insert(name.replace('-', "_"));
            }
        }
    }
    let mut sink = BTreeSet::new();
    collect(doc.get("dependencies"), &mut sink);
    collect(doc.get("build-dependencies"), &mut sink);
    collect(doc.get("dev-dependencies"), &mut sink);
    if let Some(target) = doc.get("target").and_then(|value| value.as_table()) {
        for platform in target.values().filter_map(|value| value.as_table()) {
            collect(platform.get("dependencies"), &mut sink);
            collect(platform.get("build-dependencies"), &mut sink);
            collect(platform.get("dev-dependencies"), &mut sink);
        }
    }
    sink
}

/// R3: a crate is bottom exactly when every internal dependency is a leaf
/// (a leaf has no internal dependencies). Derived from the manifests, never
/// from a name list.
fn bottom_crates_from(graph: &BTreeMap<String, BTreeSet<String>>) -> BTreeSet<String> {
    let leaves = graph
        .iter()
        .filter(|(_name, dependencies)| dependencies.is_empty())
        .map(|(name, _dependencies)| name.clone())
        .collect::<BTreeSet<_>>();
    graph
        .iter()
        .filter(|(_name, dependencies)| dependencies.iter().all(|dep| leaves.contains(dep)))
        .map(|(name, _dependencies)| name.clone())
        .collect()
}

fn workspace_crate_graph() -> BTreeMap<String, BTreeSet<String>> {
    let mut graph = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(crates_dir()) else {
        return graph;
    };
    for entry in entries.flatten() {
        let Some(directory) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(entry.path().join("Cargo.toml")) else {
            continue;
        };
        let Ok(doc) = toml::from_str::<toml::Table>(&text) else {
            continue;
        };
        graph.insert(directory.replace('-', "_"), internal_dependencies(&doc));
    }
    graph
}

fn bottom_crates() -> &'static BTreeSet<String> {
    static BOTTOM: std::sync::OnceLock<BTreeSet<String>> = std::sync::OnceLock::new();
    BOTTOM.get_or_init(|| bottom_crates_from(&workspace_crate_graph()))
}

fn source_crate_is_bottom(path: &Path) -> bool {
    source_crate_dir(path)
        .map(|directory| directory.replace('-', "_"))
        .is_some_and(|name| bottom_crates().contains(&name))
}

/// Q1 + R3: a crate-root reference to a module that physically lives in the
/// same crate is NotUpward ONLY when that crate is bottom as a whole. In a
/// layered crate an unknown module keeps failing loudly.
fn same_crate_bottom_module(source: &Path, name: &str) -> bool {
    if !source_crate_is_bottom(source) {
        return false;
    }
    source_crate_src(source)
        .and_then(|src| module_entry(&src, name))
        .is_some()
}

/// R1: cross-crate edges are acyclic by construction, so an edge classified
/// upward can never name one. Any attempt is a model bug and panics.
fn assert_intra_crate_upper(upper: &Upper) {
    assert!(
        !matches!(upper, Upper::Extern(_) | Upper::ExternModule { .. }),
        "model bug: cross-crate edge classified upward ({})",
        upper.text()
    );
}

/// The extracted crates keep their historical module names below `src/`, so
/// each historical root is seeded explicitly with the logical path the
/// pre-split ratchet used. The value crate moved G0 members out of
/// `emacs_core` and the bottom peers to its crate root; the other crates kept
/// the `emacs_core/<member>` and crate-root layouts verbatim.
fn moved_crate_seeds() -> Vec<(PathBuf, Vec<String>)> {
    fn push(seeds: &mut Vec<(PathBuf, Vec<String>)>, src: &Path, prefix: &[&str], name: &str) {
        if let Some(entry) = module_entry(src, name) {
            let mut logical: Vec<String> = prefix.iter().map(|part| (*part).to_owned()).collect();
            logical.push(name.to_owned());
            seeds.push((entry, logical));
        }
    }
    fn push_file(seeds: &mut Vec<(PathBuf, Vec<String>)>, path: PathBuf, name: &str) {
        if path.is_file() {
            seeds.push((path, vec![name.to_owned()]));
        }
    }
    let mut seeds = Vec::new();
    for root in production_source_roots() {
        let src = root.src;
        match root.crate_dir.as_str() {
            "neovm-core" => {}
            "neovm-display-identity" => {
                // Below the value crate; it names no upper item.
            }
            "neovm-value" => {
                for name in G0_MEMBERS {
                    push(&mut seeds, &src, &["emacs_core"], name);
                }
                for name in BOTTOM_PEER_MODULES {
                    push(&mut seeds, &src, &[], name);
                }
            }
            "neovm-edit" => {
                push(&mut seeds, &src, &[], "emacs_core");
                push(&mut seeds, &src, &[], "buffer");
            }
            "neovm-text" => {
                push(&mut seeds, &src, &[], "emacs_core");
                push(&mut seeds, &src, &[], "code_conversion_workspace");
                push(&mut seeds, &src, &[], "encoding");
            }
            "neovm-eval" => {
                push(&mut seeds, &src, &[], "emacs_core");
                push(&mut seeds, &src, &[], "host_effect");
                // The stage-4/5 moves kept these historical top-level roots
                // physically inside the evaluator's emacs_core tree; seed
                // them under their historical names so their ceiling rows
                // survive the move.
                push_file(
                    &mut seeds,
                    src.join("emacs_core/runtime/eval/keyboard.rs"),
                    "keyboard",
                );
                push_file(
                    &mut seeds,
                    src.join("emacs_core/runtime/eval/keyboard_input.rs"),
                    "keyboard_input",
                );
                push_file(
                    &mut seeds,
                    src.join("emacs_core/runtime/eval/frontend_events.rs"),
                    "frontend_events",
                );
            }
            "neovm-display" => {
                push(&mut seeds, &src, &[], "emacs_core");
                push(&mut seeds, &src, &[], "display_evaluation");
                push(&mut seeds, &src, &[], "face");
                push(&mut seeds, &src, &[], "image_identity");
                push(&mut seeds, &src, &[], "window");
            }
            "neovm-system" => {
                push(&mut seeds, &src, &[], "emacs_core");
                // The publisher half of the historical host_input root lives
                // in the system crate's emacs_core tree (core re-exports it
                // as emacs_core::host_input).
                push_file(
                    &mut seeds,
                    src.join("emacs_core/system/host_input.rs"),
                    "host_input",
                );
            }
            other => panic!("unclassified moving crate {other}"),
        }
    }
    seeds.sort();
    seeds
}

/// Finish a scan: resolve the facade re-exports, then walk every production
/// root in reference mode.
fn finalize_walker(
    mut walker: ProductionWalker,
    library_roots: &[PathBuf],
    seeds: &[(PathBuf, Vec<String>)],
) -> (ModuleIndex, Sites) {
    for (module, mut path, name) in std::mem::take(&mut walker.exports) {
        let crate_qualified = path.first().is_some_and(|part| part == "crate");
        if crate_qualified {
            path.remove(0);
        }
        if module.is_empty() {
            if path.first().is_none_or(|part| part != "emacs_core") {
                continue;
            }
            path.remove(0);
            assert_ne!(
                name, "*",
                "facade glob re-export must name its origins explicitly"
            );
            let origin = path.first().expect("facade re-export has an origin");
            let origin = EcSub::validated(origin, &walker.index.ec_modules);
            if let Some(previous) = walker
                .index
                .crate_exports
                .insert(name.clone(), origin.clone())
            {
                assert_eq!(previous, origin, "ambiguous facade re-export {name}");
            }
        } else {
            assert_ne!(
                name, "*",
                "facade glob re-export must name its origins explicitly"
            );
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
            let origin = path.first().expect("facade re-export has an origin");
            let upper = if is_workspace_sibling(origin) {
                // `pub use neovm_value::value;` in a crate's emacs_core
                // facade carries the moved module's name; keep it so the
                // ceiling row re-attributes to the same module in its new
                // crate. A bare crate rebind stays crate-level.
                match path.last().filter(|_| path.len() > 1) {
                    Some(module) if is_moving_crate(origin) => Upper::ExternModule {
                        crate_name: origin.clone(),
                        module: module.clone(),
                    },
                    _ => Upper::Extern(origin.clone()),
                }
            } else {
                Upper::Ec(EcSub::validated(origin, &walker.index.ec_modules))
            };
            if let Some(previous) = walker.index.ec_exports.insert(name.clone(), upper.clone()) {
                assert_eq!(previous, upper, "ambiguous facade re-export {name}");
            }
        }
    }
    walker.phase = ScanPhase::References;
    for root in library_roots {
        walker.file(&root.join("lib.rs"));
    }
    for (path, logical_path) in seeds {
        walker.file_with_logical(path, logical_path.clone());
    }
    (walker.index, walker.sites)
}

/// The fixture form: one crate tree rooted at `root`.
fn production_sites(root: &Path) -> (ModuleIndex, Sites) {
    let mut walker = new_walker();
    walker.file(&root.join("lib.rs"));
    let libraries = vec![root.to_path_buf()];
    finalize_walker(walker, &libraries, &[])
}

/// The production form: the core tree plus every extracted crate seeded at its
/// historical module roots (RULE 19 scanner re-root).
fn production_sites_across_crates() -> (ModuleIndex, Sites) {
    let roots = production_source_roots();
    let core = roots
        .iter()
        .find(|root| root.crate_dir == "neovm-core")
        .expect("core production root");
    let seeds = moved_crate_seeds();
    let mut walker = new_walker();
    walker.file(&core.src.join("lib.rs"));
    for (path, logical_path) in &seeds {
        walker.file_with_logical(path, logical_path.clone());
    }
    let libraries = vec![core.src.clone()];
    finalize_walker(walker, &libraries, &seeds)
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
        assert!(
            !columns[1].contains(':')
                || columns[1].starts_with("top:")
                || columns[1].starts_with("extern:"),
            "unknown layering edge kind `{}`: {line}",
            columns[1]
        );
        let upper = if let Some(top) = columns[1].strip_prefix("top:") {
            let root = LowerRoot::parse(top);
            assert!(
                !matches!(root, LowerRoot::G0),
                "g0 is a lower-root group, not a top-level module: {line}"
            );
            Upper::Top(root)
        } else if let Some(name) = columns[1].strip_prefix("extern:") {
            if let Some((crate_name, module)) = name.split_once("::") {
                assert!(
                    is_workspace_sibling(crate_name),
                    "extern ceiling must name a workspace-sibling crate: {line}"
                );
                assert!(
                    !module.is_empty() && !module.contains(':'),
                    "extern module ceiling must name one module: {line}"
                );
                Upper::ExternModule {
                    crate_name: crate_name.to_owned(),
                    module: module.to_owned(),
                }
            } else {
                assert!(
                    is_workspace_sibling(name),
                    "extern ceiling must name a workspace-sibling crate: {line}"
                );
                Upper::Extern(name.to_owned())
            }
        } else {
            Upper::Ec(EcSub::historical(columns[1]))
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
            let locations = sites.get(pair).map_or_else(String::new, |sites| {
                let mut locations = sites
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
                if !locations.is_empty() {
                    locations.insert(0, '\n');
                }
                locations
            });
            failures.push(format!(
                "{} {}: current {current} below ceiling {ceiling}; this ceiling may only decrease. Attribute each removed reference to the moved item in the commit message and paste the FULL current table into layering_ceilings.list.{locations}",
                pair.lower.name(),
                pair.upper.text()
            ));
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
    let roots = production_source_roots();
    let core = roots
        .iter()
        .find(|root| root.crate_dir == "neovm-core")
        .expect("core production root");
    let (index, sites) = production_sites_across_crates();
    let ceilings = parse_ceilings(include_str!("layering_ceilings.list"), &index);
    assert_production_ceilings(&core.src, &sites, &ceilings);
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
        mod emacs_core { pub mod text {} pub use text::{Glyph, GlyphKind as Kind}; }
        pub use emacs_core::text::{Glyph, GlyphKind as Kind};
        mod tagged {
            use super::Glyph;
            mod inner {
                use crate::Glyph;
                use super::super::Glyph;
                use super::Glyph;
                use super::emacs_core::text::Glyph;
                use crate::emacs_core::Kind;
                use crate::Kind;
                use self::Glyph;
                use super::gc;
            }
        }
    "#,
    );
    let (index, sites) = production_sites(directory.path());
    assert_eq!(current_table(&sites), "tagged text 6\n");
    assert_production_ceilings(
        directory.path(),
        &sites,
        &parse_ceilings("tagged text 6", &index),
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
        "mod emacs_core { pub mod value {} pub mod text {} } mod tagged { use crate::emacs_core::text::Glyph; }",
    );
    let (index, sites) = production_sites(directory.path());
    let ceilings = parse_ceilings("tagged text 2", &index);
    let message = panic_message(|| assert_production_ceilings(directory.path(), &sites, &ceilings));
    assert!(
        message.contains("this ceiling may only decrease"),
        "{message}"
    );
    assert!(
        message.contains("tagged text: current 1 below ceiling 2"),
        "{message}"
    );
    assert!(
        message.contains("FULL CURRENT TABLE\ntagged text 1\nEND CURRENT TABLE"),
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
        macro_rules! paths { () => { (crate::emacs_core::text::Glyph, super::Value) }; }
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
    assert_eq!(current_table(&sites), "buffer text 1\ntagged text 4\n");
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
            pub mod alpha {}
            pub mod text {}
            pub use crate::emacs_core::alpha::{Alpha, AlphaKind as Kind};
            pub use self::text::Glyph;
        }
        pub use tagged::*;
        pub use crate::emacs_core::{alpha::Alpha, text::Glyph as Shape};
        mod tagged {
            use crate::{Alpha, Shape};
            use crate::emacs_core::{Kind, Glyph};
            use crate::emacs_core::{self, self as ec};
            use crate::emacs_core::alpha::{Alpha, AlphaKind};
        }
    "#,
    );
    let (index, sites) = production_sites(directory.path());
    assert_eq!(current_table(&sites), "tagged alpha 3\ntagged text 2\n");
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

#[test]
fn layering_guard_counts_moving_set_outside_edges() {
    let directory = fixture(
        r#"
        mod emacs_core { pub mod value {} pub mod print {} }
        mod tagged { use crate::buffer::BufferManager; use crate::tls_scope::TlsScope; }
        mod heap_types { use crate::buffer::BufferId; }
        mod gc_trace { use crate::emacs_core::value::Value; }
        mod buffer {}
        mod tls_scope {}
    "#,
    );
    let root = directory.path();
    let (index, sites) = production_sites(root);
    assert_eq!(
        current_table(&sites),
        "heap_types top:buffer 1\ntagged top:buffer 1\ntagged top:tls_scope 1\n"
    );
    assert_production_ceilings(
        root,
        &sites,
        &parse_ceilings(&current_table(&sites), &index),
    );
}

/// R1: a reference into another crate is a cargo dependency edge. The crate
/// graph is acyclic, so a cross-crate edge can never be an upward layering
/// edge and is never counted.
#[test]
fn layering_guard_does_not_count_workspace_sibling_edges() {
    let directory = fixture(
        r#"
        mod tagged { use neomacs_display_protocol::WebViewId; use serde::Serialize; }
        mod emacs_core {
            pub mod value { use neomacs_display_protocol::VideoId; }
        }
    "#,
    );
    let (index, sites) = production_sites(directory.path());
    assert!(sites.is_empty(), "{}", current_table(&sites));
    assert_production_ceilings(directory.path(), &sites, &parse_ceilings("", &index));
}

/// R1 plant: if any classification path ever turns a cross-crate edge into
/// an Upward pair, the guard panics as a model bug.
#[test]
#[should_panic(expected = "model bug: cross-crate edge classified upward")]
fn layering_guard_panics_on_cross_crate_upward_model_bug() {
    assert_intra_crate_upper(&Upper::Extern("neovm_value".to_owned()));
}

/// After the split, an upper (non-moving) root reaches a historical
/// emacs_core module through the extracted crate: either a crate-qualified
/// import (`neovm_value::value::Value`) or the crate's `emacs_core` facade
/// re-export. Both must count as the same named module so a ceiling row
/// re-attributes one-to-one; a moving-set source stays internal.
/// R1: a crate-qualified import of an extracted crate is a cross-crate
/// (cargo) edge; only the same-crate facade ref is an intra-crate row.
#[test]
fn layering_guard_does_not_count_cross_crate_moving_imports() {
    let directory = fixture(
        r#"
        mod emacs_core { pub use neovm_value::value; }
        mod buffer {
            use neovm_value::value::{Value, eq_value};
            use crate::emacs_core::value::Value as FacadeValue;
        }
        mod tagged { use neovm_value::value::Value; }
    "#,
    );
    let (_index, sites) = production_sites(directory.path());
    assert!(sites.is_empty(), "{}", current_table(&sites));
}

/// R2 plant: a reference through the crate's OWN facade to a higher module
/// still counts, so the guard turns RED when the row is over its ceiling.
#[test]
fn layering_guard_counts_a_ref_through_the_own_facade() {
    let directory = fixture(
        r#"
        mod emacs_core {
            pub mod eval { pub mod keymap_lookup {} }
            pub use eval::keymap_lookup;
        }
        mod keyboard { use crate::emacs_core::keymap_lookup; }
    "#,
    );
    let root = directory.path();
    let (index, sites) = production_sites(root);
    assert_eq!(current_table(&sites), "keyboard eval 1\n");
    let message =
        panic_message(|| assert_production_ceilings(root, &sites, &parse_ceilings("", &index)));
    assert!(
        message.contains("keyboard eval: 1 production upward references exceed the ceiling 0"),
        "{message}"
    );
}

/// Q1 + R3 pinned case: neovm-value is bottom (its only internal dependency,
/// neovm-display-identity, is a leaf), so `use crate::symbol::Obarray;` in
/// neovm-value/src/alloc/mod.rs is NotUpward. neovm-core is layered, so the
/// same-crate rule does not cover it and an unknown module still fails.
#[test]
fn layering_guard_bottom_crate_crate_root_module_is_not_upward() {
    let value_source = Path::new("crates/neovm-value/src/alloc/mod.rs");
    assert!(
        source_crate_is_bottom(value_source),
        "value must derive bottom"
    );
    assert!(
        same_crate_bottom_module(value_source, "symbol"),
        "crate::symbol in the bottom value crate must stay internal"
    );
    let core_source = Path::new("crates/neovm-core/src/lib.rs");
    assert!(
        !source_crate_is_bottom(core_source),
        "core must derive layered"
    );
    assert!(!same_crate_bottom_module(core_source, "emacs_core"));
}

/// Peer ruling (16:0xZ): frontend_events -> super::keyboard is NotUpward.
/// Old-model LowerRoot lines: guard-old-proof.rs:392
/// `"frontend_events" => Self::FrontendEvents,` and guard-old-proof.rs:397
/// `"keyboard" => Self::Keyboard,`.
#[test]
fn layering_guard_frontend_events_super_keyboard_is_not_upward() {
    let mut walker = new_walker();
    walker.logical_path = vec!["frontend_events".to_owned()];
    let segments = vec!["super".to_owned(), "keyboard".to_owned()];
    assert!(matches!(
        walker.classify(&segments, Span::call_site()),
        Classification::NotUpward
    ));
}

/// M-d: the frozen item map row count is pinned; an increase fails.
#[test]
fn layering_item_map_row_count_is_pinned() {
    check_item_map_count(item_map().len());
}

#[test]
#[should_panic(expected = "item map row count increased")]
fn layering_item_map_row_count_increase_fails() {
    check_item_map_count(ITEM_MAP_ROWS + 1);
}

/// M-c: every frozen row must still be observed by the current scan.
#[test]
fn layering_item_map_has_no_stale_rows() {
    let (index, _sites) = production_sites_across_crates();
    let stale = stale_item_map_rows(item_map(), &index.observed_paths);
    assert!(stale.is_empty(), "stale item-map rows: {stale:?}");
}

/// M-d: a row absent from the scan is stale and fails.
#[test]
fn layering_item_map_stale_row_fails() {
    let mut map = BTreeMap::new();
    map.insert(
        (
            "neovm_edit".to_owned(),
            "crate::emacs_core::gone::Item".to_owned(),
        ),
        "lateral".to_owned(),
    );
    let observed = BTreeSet::new();
    let stale = stale_item_map_rows(&map, &observed);
    assert_eq!(stale.len(), 1, "a row absent from the scan is stale");
}

fn real_tree_walker_for(source: &str, logical: &str) -> ProductionWalker {
    let (index, _sites) = production_sites_across_crates();
    let mut walker = new_walker();
    walker.phase = ScanPhase::References;
    walker.index = index;
    walker.logical_path = vec![logical.to_owned()];
    walker.source = Path::new(source).to_path_buf();
    walker
}

fn real_tree_walker() -> ProductionWalker {
    real_tree_walker_for("crates/neovm-edit/src/buffer/buffer.rs", "buffer")
}

/// M-d: a mapped lateral item stays NotUpward.
#[test]
fn layering_item_map_mapped_lateral_stays_not_upward() {
    let walker = real_tree_walker();
    let segments = ["crate", "emacs_core", "display_metrics", "WindowId"].map(str::to_owned);
    assert!(matches!(
        walker.classify(&segments, Span::call_site()),
        Classification::NotUpward
    ));
}

/// M-d: an unmapped emacs_core item keeps the conservative module model and
/// counts Upward (RED).
#[test]
fn layering_item_map_unmapped_emacs_core_item_counts_upward() {
    let walker = real_tree_walker();
    let segments = ["crate", "emacs_core", "eval", "BRAND_NEW_ITEM"].map(str::to_owned);
    assert!(matches!(
        walker.classify(&segments, Span::call_site()),
        Classification::Upward(_)
    ));
}

/// R3 plant: a synthetic non-leaf internal dependency flips a crate from
/// bottom to layered, so the Q1 same-crate rule stops covering it.
#[test]
fn bottom_derivation_rejects_a_non_leaf_internal_dependency() {
    let mut graph = BTreeMap::new();
    graph.insert(
        "neovm_value".to_owned(),
        BTreeSet::from(["neovm_display_identity".to_owned()]),
    );
    graph.insert("neovm_display_identity".to_owned(), BTreeSet::new());
    graph.insert(
        "neovm_edit".to_owned(),
        BTreeSet::from(["neovm_value".to_owned()]),
    );
    let bottom = bottom_crates_from(&graph);
    assert!(bottom.contains("neovm_value"));
    assert!(bottom.contains("neovm_display_identity"));
    assert!(
        !bottom.contains("neovm_edit"),
        "a dependency on a non-leaf crate makes the crate layered"
    );
    graph.insert(
        "neovm_value".to_owned(),
        BTreeSet::from(["neovm_edit".to_owned()]),
    );
    let flipped = bottom_crates_from(&graph);
    assert!(
        !flipped.contains("neovm_value"),
        "a synthetic non-leaf dependency must flip value to layered"
    );
}

/// Q1 plant: in a LAYERED crate an unknown module keeps failing loudly.
#[test]
#[should_panic(expected = "unknown top-level production module surprise")]
fn layering_guard_rejects_unknown_module_in_a_layered_crate() {
    let directory = fixture("mod tagged { use crate::surprise::Thing; }");
    production_sites(directory.path());
}

#[test]
fn layering_guard_moving_set_internal_references_are_not_counted() {
    let directory = fixture(
        r#"
        mod emacs_core { pub mod value {} }
        mod tagged { use crate::emacs_core::value::Value; use crate::heap_types::Payload; }
        mod heap_types { pub struct Payload; }
    "#,
    );
    let (index, sites) = production_sites(directory.path());
    assert!(sites.is_empty(), "{:?}", current_table(&sites));
    assert_production_ceilings(directory.path(), &sites, &parse_ceilings("", &index));
}

#[test]
#[should_panic(expected = "unknown top-level production module missing")]
fn layering_guard_rejects_unknown_moving_set_target() {
    let directory = fixture("mod tagged { use crate::missing::Glyph; }");
    production_sites(directory.path());
}

#[test]
#[should_panic(expected = "unknown layering edge kind")]
fn layering_guard_rejects_unknown_ceiling_edge_kind() {
    let directory = fixture("mod tagged {}");
    let (index, _) = production_sites(directory.path());
    parse_ceilings("tagged mystery:x 1", &index);
}

#[test]
fn layering_guard_treats_moving_crate_and_root_items_as_internal() {
    let directory = fixture(
        r#"
        pub(crate) use neovm_value::tls_scope;
        pub fn root_helper() {}
        mod tagged {
            use crate::root_helper;
            use crate::tls_scope::TlsScope;
            use neovm_value::heap_types::Payload;
        }
        mod emacs_core {
            pub mod value { use crate::tls_scope::TlsScope; }
        }
    "#,
    );
    let (index, sites) = production_sites(directory.path());
    assert!(sites.is_empty(), "{}", current_table(&sites));
    assert_production_ceilings(directory.path(), &sites, &parse_ceilings("", &index));
}

#[test]
fn layering_guard_moving_set_planted_tagged_buffer_reference_fails() {
    let directory = fixture("mod tagged { use crate::buffer::BufferManager; } mod buffer {}");
    let root = directory.path();
    let (index, sites) = production_sites(root);
    let message =
        panic_message(|| assert_production_ceilings(root, &sites, &parse_ceilings("", &index)));
    assert!(message.contains("lib.rs:1"), "{message}");
    assert!(
        message.contains("tagged top:buffer: 1 production upward references exceed the ceiling 0"),
        "{message}"
    );
}

/// The historical lower root a production source file belongs to.
fn source_lower_of(path: &Path) -> String {
    let parts = path
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let Some(source) = parts.iter().position(|part| part == "src") else {
        return "(unknown)".to_owned();
    };
    let relative = &parts[source + 1..];
    match relative.first().map(String::as_str) {
        None => "(root)".to_owned(),
        Some("emacs_core") => relative
            .get(1)
            .cloned()
            .unwrap_or_else(|| "emacs_core".to_owned()),
        Some(first) => first.trim_end_matches(".rs").to_owned(),
    }
}

/// D-a/D-b: dump every counted site and every NotUpward site with its reason,
/// plus per-lower-root accounting (scanned = Upward + NotUpward).
#[test]
fn layering_dump_sites_for_analysis() {
    let Some(out) = std::env::var_os("NEOVM_LAYERING_DUMP") else {
        return;
    };
    let (index, sites) = production_sites_across_crates();
    let mut text = String::new();
    let mut accounting: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for (pair, list) in &sites {
        let entry = accounting.entry(pair.lower.name().to_owned()).or_default();
        entry.0 += list.len();
        for site in list {
            text.push_str(&format!(
                "SITE\t{}\t{}\t{}\t{}\t{}\n",
                pair.lower.name(),
                pair.upper.text(),
                site.source.display(),
                site.line,
                site.reference
            ));
        }
    }
    for (_source, _line, written, reason, lower) in &index.not_upward_sites {
        let entry = accounting.entry(lower.clone()).or_default();
        entry.1 += 1;
        text.push_str(&format!(
            "NOTUPWARD\t{}\t{}\t{}\t{}\n",
            _source.display(),
            _line,
            reason,
            written
        ));
    }
    for (path, logical) in &index.visited_files {
        text.push_str(&format!(
            "VISITED\t{}\t{}\n",
            path.display(),
            logical.join("::")
        ));
    }
    for (lower, (upward, not_upward)) in accounting {
        text.push_str(&format!(
            "ACCOUNT\t{lower}\t{}\t{upward}\t{not_upward}\n",
            upward + not_upward
        ));
    }
    std::fs::write(&out, text).expect("write layering dump");
}

/// T3a plant pair: keyboard -> window_state stays NotUpward (the moving
/// rule), while tagged -> window_state turns RED (a bottom peer names an
/// upper module).
#[test]
fn layering_item_map_keyboard_window_state_stays_lateral() {
    let (index, _sites) = production_sites_across_crates();
    let mut walker = new_walker();
    walker.phase = ScanPhase::References;
    walker.index = index;
    walker.logical_path = vec!["keyboard".to_owned()];
    walker.source =
        Path::new("crates/neovm-eval/src/emacs_core/runtime/eval/keyboard.rs").to_path_buf();
    let segments = [
        "crate",
        "emacs_core",
        "eval",
        "data",
        "window_state",
        "Window",
    ]
    .map(str::to_owned);
    assert!(matches!(
        walker.classify(&segments, Span::call_site()),
        Classification::NotUpward
    ));
}

#[test]
fn layering_item_map_tagged_window_state_turns_red() {
    let (index, _sites) = production_sites_across_crates();
    let mut walker = new_walker();
    walker.phase = ScanPhase::References;
    walker.index = index;
    walker.logical_path = vec!["tagged".to_owned()];
    walker.source = Path::new("crates/neovm-value/src/tagged/gc.rs").to_path_buf();
    let segments = [
        "crate",
        "emacs_core",
        "eval",
        "data",
        "window_state",
        "Window",
    ]
    .map(str::to_owned);
    assert!(matches!(
        walker.classify(&segments, Span::call_site()),
        Classification::Upward(_)
    ));
}

/// T3a-PROXY plant: the eval resize/window_regions hooks purely forward to
/// the moved lateral Frame methods (pre window/mod.rs:6484), so the refs stay
/// NotUpward through the frozen map; the unmapped-direct-ref plant above
/// proves a genuinely new edge still turns RED.
#[test]
fn layering_item_map_proxy_hooks_are_lateral() {
    let walker = real_tree_walker_for(
        "crates/neovm-eval/src/emacs_core/runtime/eval/keyboard.rs",
        "keyboard",
    );
    for segments in [
        [
            "crate",
            "emacs_core",
            "eval",
            "lifecycle",
            "resize_frame_pixelwise_with_buffer_constraints",
        ],
        [
            "crate",
            "emacs_core",
            "eval",
            "display_query_hooks",
            "window_regions_at",
        ],
    ] {
        let segments = segments.map(str::to_owned);
        assert!(matches!(
            walker.classify(&segments, Span::call_site()),
            Classification::NotUpward
        ));
    }
}

/// (g1) A `super::`-chain upward reference is invisible without the frozen
/// map row: the post-split physical sibling `super::kbd` must resolve to the
/// historical `ec:kbd` row (the signed T1 transfer: pre
/// keyboard.rs:524/525/680/681 -> post :476/:477/:635/:636). Without the two
/// transfer rows this test is RED: classify falls through to the `super`
/// branch and returns NotUpward.
#[test]
fn layering_item_map_super_chain_upward_ref_counts() {
    let walker = real_tree_walker_for(
        "crates/neovm-eval/src/emacs_core/runtime/eval/keyboard.rs",
        "keyboard",
    );
    for segments in [
        ["super", "kbd", "parse_kbd_string"],
        ["super", "kbd", "key_events_from_designator"],
    ] {
        let segments = segments.map(str::to_owned);
        match walker.classify(&segments, Span::call_site()) {
            Classification::Upward(pair) => assert_eq!(pair.upper.text(), "kbd"),
            other => panic!("super::kbd chain must count as upward kbd, got {other:?}"),
        }
    }
}

/// (g2) A use-rename alias does not hide the edge: the walker pushes the
/// original name for `UseTree::Rename`, so
/// `use crate::emacs_core::eval::Context as Ctx;` still records one eval
/// site. The alias is the imported name only; the written path is the
/// original one.
#[test]
fn layering_guard_use_rename_alias_counts() {
    let mut walker = real_tree_walker_for(
        "crates/neovm-eval/src/emacs_core/runtime/eval/keyboard.rs",
        "keyboard",
    );
    let item: syn::ItemUse =
        syn::parse_str("use crate::emacs_core::eval::Context as Ctx;").expect("use item");
    let mut prefix = Vec::new();
    walker.use_references(&item.tree, &mut prefix, Span::call_site());
    let sites: Vec<_> = walker
        .sites
        .iter()
        .filter(|(pair, _)| pair.upper.text() == "eval")
        .collect();
    assert_eq!(sites.len(), 1, "aliased use must record one eval site");
    let (pair, list) = sites[0];
    assert_eq!(pair.lower.name(), "keyboard");
    assert_eq!(list.len(), 1);
    assert!(
        list[0].reference.ends_with("Context"),
        "alias must not replace the written path: {}",
        list[0].reference
    );
}

/// (g3) A renamed item resolves through the frozen map instead of vanishing
/// into the module model: the post `eval::lifecycle::dbusbind_drain_events`
/// is the pre `dbusbind::drain_events` signed transfer, so it must stay on
/// the historical `ec:dbusbind` row. Without the map it would land on the
/// `eval` module row (the rename dodges the facade).
#[test]
fn layering_item_map_renamed_item_resolves_through_map() {
    let walker = real_tree_walker_for(
        "crates/neovm-eval/src/emacs_core/runtime/eval/keyboard.rs",
        "keyboard",
    );
    let segments = [
        "crate",
        "emacs_core",
        "eval",
        "lifecycle",
        "dbusbind_drain_events",
    ]
    .map(str::to_owned);
    match walker.classify(&segments, Span::call_site()) {
        Classification::Upward(pair) => assert_eq!(pair.upper.text(), "dbusbind"),
        other => panic!("renamed item must resolve to ec:dbusbind, got {other:?}"),
    }
}

const DROPPED_ROWS_TSV: &str = include_str!("layering_dropped_rows.tsv");
const CEILINGS_LIST: &str = include_str!("layering_ceilings.list");

/// #149 decrease accounting. The three historical rows dropped by the 2i
/// ceiling commit are dropped by MEASUREMENT, not by silence: every ref in
/// the ledger must classify NotUpward with the recorded cross-crate reason,
/// and the row must be absent from the ceilings. A ref that vanishes or
/// turns upward fails this test (R1: cross-crate edges are never upward).
#[test]
fn layering_dropped_rows_are_measured_cross_crate() {
    let mut rows = 0;
    for (index, line) in DROPPED_ROWS_TSV.lines().enumerate() {
        if index == 0 || line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 7, "bad dropped-row ledger line {}", index + 1);
        let (lower, upper, post_file, _post_line, written, _pre_ref, reason) = (
            fields[0], fields[1], fields[2], fields[3], fields[4], fields[5], fields[6],
        );
        assert!(
            !CEILINGS_LIST
                .lines()
                .any(|ceiling| ceiling.starts_with(&format!("{lower} {upper} "))),
            "dropped row {lower} {upper} must not reappear in the ceilings"
        );
        let mut walker = new_walker();
        walker.phase = ScanPhase::References;
        walker.logical_path = vec![lower.to_owned()];
        walker.source = Path::new(post_file).to_path_buf();
        let segments: Vec<String> = written.split("::").map(str::to_owned).collect();
        assert_eq!(
            walker.not_upward_reason(&segments),
            reason,
            "dropped ref {post_file}:{written} must stay measured as {reason}"
        );
        rows += 1;
    }
    assert_eq!(rows, 7, "the drop ledger is pinned to seven measured refs");
}

/// Counting-rule plant: a grouped use under ONE shared upward prefix records
/// ONE site no matter how many leaves it names (the pre-split rule). The
/// second statement spans two modules and correctly records two sites, one
/// per shared prefix.
#[test]
fn layering_guard_counts_one_site_per_shared_use_prefix() {
    let directory = fixture(
        r#"
        mod emacs_core { pub mod text {} pub mod keymap {} }
        mod tagged {
            use crate::emacs_core::text::{Glyph, GlyphKind};
            use crate::emacs_core::{text::A, keymap::B};
        }
    "#,
    );
    let root = directory.path();
    let (index, sites) = production_sites(root);
    assert_eq!(current_table(&sites), "tagged keymap 1\ntagged text 2\n");
    assert_production_ceilings(
        root,
        &sites,
        &parse_ceilings(&current_table(&sites), &index),
    );
}

/// The grouped keymap use attributes item-level to keymap through the frozen
/// map (its first upward leaf is DefaultBindingMode -> ec:keymap).
#[test]
fn layering_item_map_grouped_keymap_use_attributes_to_keymap() {
    let walker = real_tree_walker_for(
        "crates/neovm-eval/src/emacs_core/runtime/eval/keyboard.rs",
        "keyboard",
    );
    let segments = [
        "crate",
        "emacs_core",
        "eval",
        "keymap_lookup",
        "DefaultBindingMode",
    ]
    .map(str::to_owned);
    match walker.classify(&segments, Span::call_site()) {
        Classification::Upward(pair) => assert_eq!(pair.upper.text(), "keymap"),
        other => panic!("expected Upward keymap, got {other:?}"),
    }
}

/// D-c plants: one NotUpward site per reason.
#[test]
fn layering_guard_records_not_upward_reasons() {
    let mut keyboard = new_walker();
    keyboard.phase = ScanPhase::References;
    keyboard.logical_path = vec!["keyboard".to_owned()];
    keyboard.source =
        Path::new("crates/neovm-eval/src/emacs_core/runtime/eval/keyboard.rs").to_path_buf();
    let cross = ["neovm_value", "value", "Value"].map(str::to_owned);
    assert_eq!(
        keyboard.not_upward_reason(&cross),
        "r1-cross-crate:neovm_value"
    );

    let mut edit = new_walker();
    edit.phase = ScanPhase::References;
    edit.logical_path = vec!["buffer".to_owned()];
    edit.source = Path::new("crates/neovm-edit/src/buffer/buffer.rs").to_path_buf();
    let lateral = ["crate", "emacs_core", "display_metrics", "WindowId"].map(str::to_owned);
    assert_eq!(edit.not_upward_reason(&lateral), "lateral");
    let self_reference = ["self", "item"].map(str::to_owned);
    assert_eq!(edit.not_upward_reason(&self_reference), "self");

    let mut tagged = new_walker();
    tagged.phase = ScanPhase::References;
    tagged.logical_path = vec!["tagged".to_owned()];
    tagged.source = Path::new("crates/neovm-value/src/tagged/gc.rs").to_path_buf();
    let moving = ["crate", "value", "Value"].map(str::to_owned);
    assert_eq!(tagged.not_upward_reason(&moving), "moving-internal");
}

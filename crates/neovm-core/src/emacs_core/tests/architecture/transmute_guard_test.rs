//! Ratchet unsafe conversions in every production module, in every feature
//! configuration. Follow module declarations rather than guessing from file
//! names: a production module named `tests` must still be checked.

use super::architecture_production_source_roots::production_source_roots;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use syn::ext::IdentExt;
use syn::visit::{self, Visit};
use syn::{Attribute, Expr, Item, ItemMod, Lit, Meta};

// Base 9bb11e25b0 had 14 production sites. This lane removes six pdump enum
// conversions and two regex opcode conversions. Lower this ceiling whenever
// another conversion is removed; never raise it for a new conversion.
//
// 2026-11-10 peer-signed exception (cired2 fatal P4, pid f70ad89b): 6 -> 7.
// 2026-11-21 (peer 6316/6319/6330): the conversion now lives in one helper,
// neovm-system/src/emacs_core/system/os_signal/mod.rs
// `handler_from_address`, which reconstitutes a callback from
// `sigaction.sa_sigaction` (an integer by ABI). It serves both the
// SA_SIGINFO fault chain (so a synchronous SIGSEGV/SIGBUS fault can chain to
// Rust std's stack-overflow detector before the fatal report) and the
// one-argument SIGCHLD chain. There is no safe route from an integer to a
// function pointer. The value is only ever read back from a sigaction this
// process installed, after filtering SIG_DFL/SIG_IGN and checking
// SA_SIGINFO for the fault chain. The helper spells the conversion
// `transmute_copy` behind a compile-time pointer-size check, so this ratchet
// counts BOTH `transmute` and `transmute_copy` and the total stays 7. A
// union spelling of the same conversion was rejected because it hides it
// from this ratchet.
const TRANSMUTE_CEILING: usize = 7;

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

/// One production `include!(...)` whose target could not be resolved, read, or
/// parsed. Unanalysable input fails closed as a named violation rather than
/// panicking inside the `syn` visitor with a raw `io::Error`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct UnanalysableInclude {
    crate_name: String,
    file: String,
    macro_call: String,
    reason: String,
}

impl std::fmt::Display for UnanalysableInclude {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "crate={} file={} macro={} reason={}",
            self.crate_name, self.file, self.macro_call, self.reason
        )
    }
}

struct RootScan {
    sites: BTreeMap<PathBuf, usize>,
    unanalysable: Vec<UnanalysableInclude>,
}

struct AllScan {
    sites: BTreeMap<String, usize>,
    unanalysable: Vec<UnanalysableInclude>,
}

/// This is a test-local, single-threaded source walker, with no runtime state.
struct ProductionWalker {
    crate_name: String,
    crate_root: PathBuf,
    symbol: &'static str,
    source: PathBuf,
    module_directory: PathBuf,
    // #[path] on an out-of-line module is relative to its containing source,
    // whereas ordinary children of foo.rs are below foo/.
    path_directory: PathBuf,
    sites: BTreeMap<PathBuf, usize>,
    unanalysable: Vec<UnanalysableInclude>,
}

impl ProductionWalker {
    fn format_source_file(&self) -> String {
        match self.source.strip_prefix(&self.crate_root) {
            Ok(relative) => format!(
                "{}/src/{}",
                self.crate_name,
                relative.to_string_lossy().replace('\\', "/")
            ),
            Err(_) => self.source.display().to_string(),
        }
    }

    fn record_unanalysable(&mut self, macro_call: String, reason: String) {
        self.unanalysable.push(UnanalysableInclude {
            crate_name: self.crate_name.clone(),
            file: self.format_source_file(),
            macro_call,
            reason,
        });
    }

    fn visit_parsed_file(&mut self, path: &Path, syntax: &syn::File) {
        if test_only(&syntax.attrs) {
            return;
        }
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
        self.visit_file(syntax);
        self.source = previous_source;
        self.module_directory = previous_module;
        self.path_directory = previous_path;
    }

    fn file(&mut self, path: &Path) {
        let source = std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let syntax = syn::parse_file(&source)
            .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
        self.visit_parsed_file(path, &syntax);
    }

    fn include_file(&mut self, path: &Path, macro_call: String) {
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) => {
                self.record_unanalysable(
                    macro_call,
                    format!("unreadable include! target {}: {error}", path.display()),
                );
                return;
            }
        };
        let syntax = match syn::parse_file(&source) {
            Ok(syntax) => syntax,
            Err(error) => {
                self.record_unanalysable(
                    macro_call,
                    format!("unparseable include! target {}: {error}", path.display()),
                );
                return;
            }
        };
        self.visit_parsed_file(path, &syntax);
    }

    fn record(&mut self, count: usize) {
        if count != 0 {
            *self.sites.entry(self.source.clone()).or_default() += count;
        }
    }
}

// Macro bodies are token streams, not ordinary syn paths. Scan identifiers
// recursively, ignoring literal tokens (including raw strings) and comments.
fn macro_references(mut cursor: syn::buffer::Cursor<'_>, symbol: &str) -> usize {
    let mut count = 0;
    while !cursor.eof() {
        if let Some((ident, next)) = cursor.ident() {
            count += usize::from(ident.unraw() == symbol);
            cursor = next;
        } else if let Some((group, _, _, next)) = cursor.any_group() {
            count += macro_references(group, symbol);
            cursor = next;
        } else {
            cursor = cursor.token_tree().expect("nonempty token cursor").1;
        }
    }
    count
}

impl ProductionWalker {
    /// An alias could hide subsequent conversions from a path-based count.
    /// Keep unsafe conversions explicit and qualified instead. `public`
    /// permits a same-name re-export but never a rename.
    fn check_use_tree(&self, tree: &syn::UseTree, public: bool) {
        match tree {
            syn::UseTree::Path(path) => self.check_use_tree(&path.tree, public),
            syn::UseTree::Name(name) => {
                if !public {
                    assert_ne!(
                        name.ident.unraw(),
                        self.symbol,
                        "qualify {} paths",
                        self.symbol
                    );
                }
            }
            syn::UseTree::Rename(rename) => {
                assert_ne!(
                    rename.ident.unraw(),
                    self.symbol,
                    "do not alias {}",
                    self.symbol
                );
            }
            syn::UseTree::Glob(_) => {}
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    self.check_use_tree(item, public);
                }
            }
        }
    }
}

fn is_include_macro(path: &syn::Path) -> bool {
    if path.is_ident("include") {
        return true;
    }
    let mut segments = path.segments.iter();
    match (segments.next(), segments.next(), segments.next()) {
        (Some(first), Some(second), None) => {
            (first.ident == "core" || first.ident == "std") && second.ident == "include"
        }
        _ => false,
    }
}

impl<'ast> Visit<'ast> for ProductionWalker {
    fn visit_item(&mut self, item: &'ast Item) {
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
        if let Some((_, items)) = &module.content {
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
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        if path
            .segments
            .last()
            .is_some_and(|segment| segment.ident.unraw() == self.symbol)
        {
            self.record(1);
        }
        visit::visit_path(self, path);
    }

    fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
        if is_include_macro(&invocation.path) {
            let macro_call = format!("include!({})", invocation.tokens);
            let expression = match syn::parse2::<Expr>(invocation.tokens.clone()) {
                Ok(expression) => expression,
                Err(error) => {
                    self.record_unanalysable(
                        macro_call,
                        format!("unparseable include! path expression: {error}"),
                    );
                    return;
                }
            };
            let path = match include_path(&expression, &self.crate_name, &self.crate_root) {
                Ok(path) => path,
                Err(reason) => {
                    self.record_unanalysable(macro_call, reason);
                    return;
                }
            };
            let path = self.source.parent().expect("source has parent").join(path);
            self.include_file(&path, macro_call);
            return;
        }
        let tokens = syn::buffer::TokenBuffer::new2(invocation.tokens.clone());
        self.record(macro_references(tokens.begin(), self.symbol));
        self.visit_path(&invocation.path);
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        // A PUBLIC `use name;` re-export is a facade definition of the
        // existing public path (AUTHORITY_PATHS names it), not a call-site
        // import that could hide a conversion: the signed cfg-gated
        // `pub use gc_thread::set_tagged_heap;` beside the glob in value's
        // tagged/gc.rs is exactly this shape. Private imports and every alias
        // stay rejected: an alias could hide a later conversion from the
        // path-based count.
        let public = matches!(item.vis, syn::Visibility::Public(_));
        self.check_use_tree(&item.tree, public);
    }
}

// Resolve source includes without expanding arbitrary macros. `OUT_DIR` is
// known only for `neovm-core` itself (this test binary's build script); a
// foreign crate's `env!("OUT_DIR")` and any unknown macro expression fail
// closed as named unanalysable violations rather than panicking with io::Error.
fn include_path(expression: &Expr, crate_name: &str, crate_root: &Path) -> Result<String, String> {
    match expression {
        Expr::Lit(value) => match &value.lit {
            Lit::Str(path) => Ok(path.value()),
            _ => Err("non-string literal in include! path expression".to_owned()),
        },
        Expr::Macro(value) if value.mac.path.is_ident("concat") => {
            let parts = value
                .mac
                .parse_body_with(
                    syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated,
                )
                .map_err(|error| format!("unparseable concat! arguments in include!: {error}"))?;
            parts
                .iter()
                .map(|part| include_path(part, crate_name, crate_root))
                .collect::<Result<String, String>>()
        }
        Expr::Macro(value) if value.mac.path.is_ident("env") => {
            let name = value
                .mac
                .parse_body::<syn::LitStr>()
                .map_err(|error| format!("unparseable env! argument in include!: {error}"))?;
            match name.value().as_str() {
                "OUT_DIR" => {
                    if crate_name == "neovm-core" {
                        Ok(env!("OUT_DIR").to_owned())
                    } else {
                        Err(format!(
                            "env!(\"OUT_DIR\") belongs to {crate_name}'s build script and cannot be resolved from neovm-core's OUT_DIR"
                        ))
                    }
                }
                "CARGO_MANIFEST_DIR" => {
                    if crate_name == "neovm-core" {
                        Ok(env!("CARGO_MANIFEST_DIR").to_owned())
                    } else {
                        crate_root
                            .parent()
                            .map(|dir| dir.to_string_lossy().into_owned())
                            .ok_or_else(|| {
                                format!(
                                    "cannot derive CARGO_MANIFEST_DIR for {crate_name} from {}",
                                    crate_root.display()
                                )
                            })
                    }
                }
                other => Err(format!("unsupported env!(\"{other}\") in include! path")),
            }
        }
        _ => Err(
            "unsupported include! path expression (only string literals, concat!, and resolvable env! are analysable without macro expansion)"
                .to_owned(),
        ),
    }
}

fn scan_production_root(crate_name: &str, root: &Path, symbol: &'static str) -> RootScan {
    let mut walker = ProductionWalker {
        crate_name: crate_name.to_owned(),
        crate_root: root.to_path_buf(),
        symbol,
        source: PathBuf::new(),
        module_directory: PathBuf::new(),
        path_directory: PathBuf::new(),
        sites: BTreeMap::new(),
        unanalysable: Vec::new(),
    };
    walker.file(&root.join("lib.rs"));
    RootScan {
        sites: walker.sites,
        unanalysable: walker.unanalysable,
    }
}

fn assert_no_unanalysable(unanalysable: &[UnanalysableInclude]) {
    if unanalysable.is_empty() {
        return;
    }
    let details = unanalysable
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    panic!(
        "{} unanalysable production include! macro(s) (fail-closed):\n{details}",
        unanalysable.len()
    );
}

fn production_sites(root: &Path) -> BTreeMap<PathBuf, usize> {
    production_sites_for(root, "transmute")
}

fn production_sites_for(root: &Path, symbol: &'static str) -> BTreeMap<PathBuf, usize> {
    let scan = scan_production_root("fixture", root, symbol);
    assert_no_unanalysable(&scan.unanalysable);
    scan.sites
}

fn assert_production_ceiling(root: &Path, sites: &BTreeMap<PathBuf, usize>) {
    let count = sites.values().sum::<usize>();
    let locations = sites
        .iter()
        .map(|(path, count)| {
            format!(
                "{}: {count}",
                path.strip_prefix(root)
                    .expect("source below root")
                    .display()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        count <= TRANSMUTE_CEILING,
        "{count} production transmute sites exceed the ceiling {TRANSMUTE_CEILING}. \
         Decode enums with TryFromPrimitive or FromRepr; this ceiling may only decrease.\n{locations}"
    );
}

/// Production sites across every extracted source tree, keyed by
/// workspace-relative path. The pre-split guard walked the single core module
/// graph; the split moved most of that graph into the lower crates, so the
/// ceiling covers all of them or it would silently stop counting.
fn all_production_scan(symbol: &'static str) -> AllScan {
    let mut sites = BTreeMap::new();
    let mut unanalysable = Vec::new();
    for root in production_source_roots() {
        let scan = scan_production_root(&root.crate_dir, &root.src, symbol);
        unanalysable.extend(scan.unanalysable);
        for (path, count) in scan.sites {
            let key = match path.strip_prefix(&root.src) {
                Ok(relative) => format!(
                    "{}/src/{}",
                    root.crate_dir,
                    relative.to_string_lossy().replace('\\', "/")
                ),
                Err(_) => format!("{}:{}", root.crate_dir, path.display()),
            };
            *sites.entry(key).or_insert(0) += count;
        }
    }
    AllScan {
        sites,
        unanalysable,
    }
}

fn assert_total_ceiling(sites: &BTreeMap<String, usize>) {
    let count = sites.values().sum::<usize>();
    let locations = sites
        .iter()
        .map(|(path, count)| format!("{path}: {count}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        count <= TRANSMUTE_CEILING,
        "{count} production transmute sites exceed the ceiling {TRANSMUTE_CEILING}. \
         Decode enums with TryFromPrimitive or FromRepr; this ceiling may only decrease.\n{locations}"
    );
}

#[test]
fn production_transmute_count_can_only_decrease() {
    // `transmute_copy` is the same conversion with less static checking, so
    // it counts against the same ceiling. The one handler conversion
    // (`handler_from_address`) spells it that way to stay generic over the
    // one- and three-argument callback ABIs.
    let transmute = all_production_scan("transmute");
    assert_no_unanalysable(&transmute.unanalysable);
    let transmute_copy = all_production_scan("transmute_copy");
    assert_no_unanalysable(&transmute_copy.unanalysable);
    let mut sites = transmute.sites;
    for (path, count) in transmute_copy.sites {
        *sites.entry(path).or_insert(0) += count;
    }
    assert_total_ceiling(&sites);
}

#[test]
fn transmute_guard_parses_code_and_follows_production_module_paths() {
    let directory = tempfile::tempdir().expect("create source fixture");
    let root = directory.path();
    std::fs::write(root.join("lib.rs"), r###"
        // core::mem::transmute(0)
        const MESSAGE: &str = "std::mem::transmute(0)";
        const RAW: &str = r#"std::mem::transmute(0)"#;
        #[cfg(test)] mod checks;
        #[cfg(all(unix, test))] mod more_checks { fn helper() { unsafe { core::mem::transmute(0); } } }
        #[cfg(not(not(test)))] mod negated_checks;
        #[cfg(any(test, all(test, feature = "fuzzing")))] mod combined_checks;
        #[test] fn check() { unsafe { core::mem::transmute(0); } }
        #[cfg(not(test))] fn runtime() { unsafe { core::mem::r#transmute::<u8, i8>(0); } }
        #[cfg(any(test, feature = "fuzzing"))] fn mixed() { unsafe { core::mem::transmute(0); } }
        #[path = "tests/runtime.rs"] mod production;
        mod nested { mod child; }
        include!(concat!("included", ".rs"));
        macro_rules! conversion { () => { unsafe { core::mem::r#transmute(0) } }; }
        macro_rules! message { () => { "core::mem::transmute(0)" }; }
    "###).expect("write crate fixture");
    // A missing test-only file must never be read.
    std::fs::create_dir(root.join("tests")).expect("create production test-shaped path");
    std::fs::write(
        root.join("tests/runtime.rs"),
        "fn run() { unsafe { core::mem::transmute(0); } }",
    )
    .expect("write production module");
    std::fs::create_dir(root.join("nested")).expect("create inline module directory");
    std::fs::write(
        root.join("nested/child.rs"),
        "fn run() { unsafe { core::mem::transmute(0); } }",
    )
    .expect("write nested production module");
    std::fs::write(
        root.join("included.rs"),
        "fn included() { unsafe { core::mem::transmute(0); } }",
    )
    .expect("write included production source");
    let sites = production_sites(root);
    assert_eq!(sites.values().sum::<usize>(), 6);
    assert_eq!(sites[&root.join("tests/runtime.rs")], 1);
    assert_eq!(sites[&root.join("nested/child.rs")], 1);
    assert_eq!(sites[&root.join("included.rs")], 1);
    assert_production_ceiling(root, &sites);
}

#[test]
#[should_panic(expected = "production transmute sites exceed the ceiling")]
fn transmute_guard_rejects_count_above_ceiling() {
    let directory = tempfile::tempdir().expect("create source fixture");
    let root = directory.path();
    let source = format!(
        "fn run() {{ unsafe {{ {} }} }}",
        "core::mem::transmute::<u8, i8>(0);".repeat(TRANSMUTE_CEILING + 1)
    );
    std::fs::write(root.join("lib.rs"), source).expect("write source fixture");
    assert_production_ceiling(root, &production_sites(root));
}

#[test]
#[should_panic(expected = "do not alias transmute")]
fn transmute_guard_rejects_aliases_that_hide_conversions() {
    let directory = tempfile::tempdir().expect("create source fixture");
    std::fs::write(
        directory.path().join("lib.rs"),
        "use core::mem::r#transmute as convert; fn run() { unsafe { convert::<u8, i8>(0); } }",
    )
    .expect("write source fixture");
    production_sites(directory.path());
}

/// The public same-name re-export relaxation must not let a RENAME hide the
/// installer: an aliased public re-export still reddens.
#[test]
#[should_panic(expected = "do not alias set_tagged_heap")]
fn transmute_guard_rejects_a_public_rename_of_the_installer() {
    let directory = tempfile::tempdir().expect("create source fixture");
    std::fs::write(
        directory.path().join("lib.rs"),
        "mod gc_thread { pub fn set_tagged_heap() {} }\n\
         pub use gc_thread::set_tagged_heap as renamed;\n",
    )
    .expect("write source fixture");
    production_sites_for(directory.path(), "set_tagged_heap");
}

/// Only a PUBLIC use re-export is the facade shape; a private same-name
/// import still reddens because it could hide a later conversion.
#[test]
#[should_panic(expected = "qualify set_tagged_heap paths")]
fn transmute_guard_rejects_a_private_same_name_import() {
    let directory = tempfile::tempdir().expect("create source fixture");
    std::fs::write(
        directory.path().join("lib.rs"),
        "mod gc_thread { pub fn set_tagged_heap() {} }\n\
         use gc_thread::set_tagged_heap;\n",
    )
    .expect("write source fixture");
    production_sites_for(directory.path(), "set_tagged_heap");
}

/// The accepted shape itself: a public same-name re-export is a facade
/// definition, not a call-site import.
#[test]
fn transmute_guard_accepts_a_public_same_name_reexport() {
    let directory = tempfile::tempdir().expect("create source fixture");
    std::fs::write(
        directory.path().join("lib.rs"),
        "mod gc_thread { pub fn set_tagged_heap() {} }\n\
         pub use gc_thread::set_tagged_heap;\n",
    )
    .expect("write source fixture");
    let sites = production_sites_for(directory.path(), "set_tagged_heap");
    assert!(sites.values().all(|count| *count == 0), "{sites:?}");
}

#[test]
fn production_heap_installers_use_the_allocation_root() {
    let scan = all_production_scan("set_tagged_heap");
    assert_no_unanalysable(&scan.unanalysable);
    assert!(
        scan.sites.is_empty(),
        "production must install TLS from the allocation root: {:?}",
        scan.sites
    );
}

#[test]
fn transmute_guard_reports_foreign_crate_out_dir_include_as_named_violation() {
    let directory = tempfile::tempdir().expect("create source fixture");
    let src = directory.path().join("src");
    std::fs::create_dir(&src).expect("create src dir");
    std::fs::write(src.join("lib.rs"), "mod window_identity;\n").expect("write lib.rs");
    std::fs::write(
        src.join("window_identity.rs"),
        "#[cfg(target_os = \"linux\")]\n\
         mod generated {\n\
             include!(concat!(env!(\"OUT_DIR\"), \"/application_identity.rs\"));\n\
         }\n",
    )
    .expect("write window_identity.rs");

    let scan = scan_production_root("neomacs-display-runtime", &src, "transmute");
    assert_eq!(
        scan.unanalysable.len(),
        1,
        "foreign crate OUT_DIR include! must record one named violation, not panic: {:?}",
        scan.unanalysable
    );
    let violation = &scan.unanalysable[0];
    assert_eq!(violation.crate_name, "neomacs-display-runtime");
    assert_eq!(
        violation.file,
        "neomacs-display-runtime/src/window_identity.rs"
    );
    assert!(
        violation.macro_call.contains("include!")
            && violation.macro_call.contains("OUT_DIR")
            && violation.macro_call.contains("application_identity.rs"),
        "macro field must identify the include! invocation: {}",
        violation.macro_call
    );
    assert!(
        violation.reason.contains("OUT_DIR")
            && violation.reason.contains("neomacs-display-runtime"),
        "reason must name the unresolvable foreign OUT_DIR: {}",
        violation.reason
    );
}

#[test]
fn transmute_guard_reports_unreadable_and_unsupported_includes_as_named_violations() {
    let directory = tempfile::tempdir().expect("create source fixture");
    let src = directory.path().join("src");
    std::fs::create_dir(&src).expect("create src dir");
    std::fs::write(
        src.join("lib.rs"),
        "include!(\"does_not_exist.rs\");\n\
         include!(\"broken_syntax.rs\");\n\
         std::include!(custom_generated_path!());\n\
         include!(1 +);\n",
    )
    .expect("write lib.rs");
    std::fs::write(src.join("broken_syntax.rs"), "fn (\n").expect("write broken_syntax.rs");

    let scan = scan_production_root("synthetic-crate", &src, "transmute");
    assert_eq!(
        scan.unanalysable.len(),
        4,
        "all four unanalysable includes must be recorded without panicking: {:?}",
        scan.unanalysable
    );
    for violation in &scan.unanalysable {
        assert_eq!(violation.crate_name, "synthetic-crate");
        assert_eq!(violation.file, "synthetic-crate/src/lib.rs");
        assert!(violation.macro_call.starts_with("include!("));
    }
    assert!(
        scan.unanalysable[0]
            .reason
            .contains("unreadable include! target"),
        "{}",
        scan.unanalysable[0]
    );
    assert!(
        scan.unanalysable[1]
            .reason
            .contains("unparseable include! target"),
        "{}",
        scan.unanalysable[1]
    );
    assert!(
        scan.unanalysable[2]
            .reason
            .contains("unsupported include! path expression"),
        "{}",
        scan.unanalysable[2]
    );
    assert!(
        scan.unanalysable[3]
            .reason
            .contains("unparseable include! path expression"),
        "{}",
        scan.unanalysable[3]
    );
}

#[test]
fn transmute_guard_resolves_foreign_crate_manifest_dir_include_and_counts_sites() {
    let directory = tempfile::tempdir().expect("create source fixture");
    let src = directory.path().join("src");
    std::fs::create_dir(&src).expect("create src dir");
    std::fs::write(
        src.join("lib.rs"),
        "include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/x.rs\"));\n",
    )
    .expect("write lib.rs");
    let included = directory.path().join("x.rs");
    std::fs::write(
        &included,
        "fn foreign_site() { unsafe { core::mem::transmute::<u8, i8>(0); } }\n",
    )
    .expect("write x.rs");

    let scan = scan_production_root("foreign-crate", &src, "transmute");
    assert!(
        scan.unanalysable.is_empty(),
        "foreign crate CARGO_MANIFEST_DIR include! must resolve cleanly: {:?}",
        scan.unanalysable
    );
    assert_eq!(scan.sites.values().sum::<usize>(), 1, "{:?}", scan.sites);
    assert_eq!(scan.sites[&included], 1);
}

#[test]
#[should_panic(expected = "unanalysable production include! macro(s) (fail-closed)")]
fn transmute_guard_fails_closed_when_asserting_unanalysable_include() {
    let directory = tempfile::tempdir().expect("create source fixture");
    let src = directory.path().join("src");
    std::fs::create_dir(&src).expect("create src dir");
    std::fs::write(
        src.join("lib.rs"),
        "include!(concat!(env!(\"OUT_DIR\"), \"/missing.rs\"));\n",
    )
    .expect("write lib.rs");
    let scan = scan_production_root("foreign-crate", &src, "transmute");
    assert_no_unanalysable(&scan.unanalysable);
}

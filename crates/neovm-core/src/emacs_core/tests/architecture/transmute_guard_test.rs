//! Ratchet unsafe conversions in every production module, in every feature
//! configuration. Follow module declarations rather than guessing from file
//! names: a production module named `tests` must still be checked.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use syn::ext::IdentExt;
use syn::visit::{self, Visit};
use syn::{Attribute, Expr, Item, ItemMod, Lit, Meta};

// Base 9bb11e25b0 had 14 production sites. This lane removes six pdump enum
// conversions and two regex opcode conversions. Lower this ceiling whenever
// another conversion is removed; never raise it for a new conversion.
const TRANSMUTE_CEILING: usize = 6;

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

/// This is a test-local, single-threaded source walker, with no runtime state.
struct ProductionWalker<'generated> {
    x11_colors: &'generated str,
    source: PathBuf,
    module_directory: PathBuf,
    // #[path] on an out-of-line module is relative to its containing source,
    // whereas ordinary children of foo.rs are below foo/.
    path_directory: PathBuf,
    sites: BTreeMap<PathBuf, usize>,
}

impl ProductionWalker<'_> {
    fn file(&mut self, path: &Path) {
        let source = std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        self.source_file(path, &source);
    }

    fn source_file(&mut self, path: &Path, source: &str) {
        let syntax = syn::parse_file(source)
            .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
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
        self.visit_file(&syntax);
        self.source = previous_source;
        self.module_directory = previous_module;
        self.path_directory = previous_path;
    }

    fn record(&mut self, count: usize) {
        if count != 0 {
            *self.sites.entry(self.source.clone()).or_default() += count;
        }
    }
}

// Macro bodies are token streams, not ordinary syn paths. Scan identifiers
// recursively, ignoring literal tokens (including raw strings) and comments.
fn macro_transmutes(mut cursor: syn::buffer::Cursor<'_>) -> usize {
    let mut count = 0;
    while !cursor.eof() {
        if let Some((ident, next)) = cursor.ident() {
            count += usize::from(ident.unraw() == "transmute");
            cursor = next;
        } else if let Some((group, _, _, next)) = cursor.any_group() {
            count += macro_transmutes(group);
            cursor = next;
        } else {
            cursor = cursor.token_tree().expect("nonempty token cursor").1;
        }
    }
    count
}

impl<'ast> Visit<'ast> for ProductionWalker<'_> {
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
            .is_some_and(|segment| segment.ident.unraw() == "transmute")
        {
            self.record(1);
        }
        visit::visit_path(self, path);
    }

    fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
        if invocation.path.is_ident("include") {
            let expression = syn::parse2::<Expr>(invocation.tokens.clone())
                .expect("include! must have a source path expression");
            let path = include_path(&expression)
                .expect("teach the transmute guard to resolve this include! source path");
            let path = self.source.parent().expect("source has parent").join(path);
            if path == Path::new(env!("OUT_DIR")).join("x11_colors.rs") {
                // Archived tests do not retain the producer's OUT_DIR. Scan
                // the same generated source embedded when this test was built.
                let generated_path = self
                    .source
                    .parent()
                    .expect("source has parent")
                    .join("x11_colors.rs");
                self.source_file(&generated_path, self.x11_colors);
            } else {
                self.file(&path);
            }
            return;
        }
        let tokens = syn::buffer::TokenBuffer::new2(invocation.tokens.clone());
        self.record(macro_transmutes(tokens.begin()));
        self.visit_path(&invocation.path);
    }

    fn visit_use_tree(&mut self, tree: &'ast syn::UseTree) {
        // An alias could hide subsequent conversions from a path-based count.
        // Keep unsafe conversions explicit and qualified instead.
        match tree {
            syn::UseTree::Name(name) => {
                assert_ne!(name.ident.unraw(), "transmute", "qualify transmute paths")
            }
            syn::UseTree::Rename(rename) => {
                assert_ne!(rename.ident.unraw(), "transmute", "do not alias transmute")
            }
            _ => visit::visit_use_tree(self, tree),
        }
    }
}

// Resolve source includes rather than scanning their path string. Only the
// known generated source is embedded; unknown forms fail closed so a new
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

fn production_sites(root: &Path) -> BTreeMap<PathBuf, usize> {
    production_sites_with_generated(
        root,
        include_str!(concat!(env!("OUT_DIR"), "/x11_colors.rs")),
    )
}

fn production_sites_with_generated(root: &Path, x11_colors: &str) -> BTreeMap<PathBuf, usize> {
    let mut walker = ProductionWalker {
        x11_colors,
        source: PathBuf::new(),
        module_directory: PathBuf::new(),
        path_directory: PathBuf::new(),
        sites: BTreeMap::new(),
    };
    walker.file(&root.join("lib.rs"));
    walker.sites
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

#[test]
fn production_transmute_count_can_only_decrease() {
    let root = neomacs_infra::crate_root!().join("src");
    assert_production_ceiling(&root, &production_sites(&root));
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
fn transmute_guard_scans_embedded_generated_source() {
    let directory = tempfile::tempdir().expect("create source fixture");
    let root = directory.path();
    std::fs::write(
        root.join("lib.rs"),
        r#"include!(concat!(env!("OUT_DIR"), "/x11_colors.rs"));"#,
    )
    .expect("write generated include fixture");
    let generated = r#"
        // core::mem::transmute(0)
        const MESSAGE: &str = "core::mem::transmute(0)";
        #[cfg(test)] fn check() { unsafe { core::mem::transmute(0); } }
        fn generated() { unsafe { core::mem::transmute::<u8, i8>(0); } }
        macro_rules! conversion { () => { unsafe { core::mem::transmute(0) } }; }
    "#;
    let sites = production_sites_with_generated(root, generated);
    assert_eq!(sites.values().sum::<usize>(), 2);
    assert_eq!(sites[&root.join("x11_colors.rs")], 2);
}

#[test]
#[should_panic(expected = "teach the transmute guard to resolve this include! source path")]
fn transmute_guard_rejects_unknown_generated_include() {
    let directory = tempfile::tempdir().expect("create source fixture");
    std::fs::write(
        directory.path().join("lib.rs"),
        r#"include!(concat!(env!("UNKNOWN_SOURCE_DIR"), "/x11_colors.rs"));"#,
    )
    .expect("write unsupported include fixture");
    production_sites(directory.path());
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

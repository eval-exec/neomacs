//! Ratchet the readers that hand out a `&Value` into variable storage.
//!
//! `Obarray::symbol_value`, `symbol_value_id` and `default_value_id` return a
//! reference into a symbol's value cell or, through `LispFwd::load_ref`, into
//! a forwarder descriptor's slot. Both are written in place while such a
//! reference may still be live, so the reference cannot stay: P7.4
//! (`AtomicValue` forwarders) moves these callers to copied values. Until it
//! lands, no new caller may appear: each count below may only decrease.
//!
//! Calls are counted by method name and argument count in the sources of
//! every shipping workspace crate, including inside macro invocations. Test
//! crates, test files (anything below a `tests` directory, `*_test.rs`) and
//! items gated on `test` are not production code and are not counted.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use syn::visit::{self, Visit};
use syn::{Attribute, Meta};

/// A `&Value` reader: its method name, its argument count (which tells
/// `Obarray::symbol_value(name)` from unrelated zero-argument methods of the
/// same name), and the most production call sites it may have.
struct Reader {
    method: &'static str,
    arguments: usize,
    ceiling: usize,
}

// Base 3a3ffdfb1c. Lower a ceiling whenever callers move to a copying reader;
// never raise one for a new caller.
const READERS: &[Reader] = &[
    Reader {
        method: "symbol_value",
        arguments: 1,
        ceiling: 197,
    },
    Reader {
        method: "symbol_value_id",
        arguments: 1,
        ceiling: 37,
    },
    Reader {
        method: "default_value_id",
        arguments: 1,
        ceiling: 12,
    },
    Reader {
        method: "load_ref",
        arguments: 0,
        ceiling: 2,
    },
    Reader {
        method: "plain_value_ref",
        arguments: 0,
        ceiling: 2,
    },
];

/// Whether an attribute confines its item to test builds: `#[test]`, or a
/// `cfg` that cannot hold without `test`.
fn test_only(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("test")
            || (attribute.path().is_ident("cfg")
                && attribute
                    .parse_args::<Meta>()
                    .is_ok_and(|meta| requires_test(&meta)))
    })
}

fn requires_test(meta: &Meta) -> bool {
    match meta {
        Meta::Path(path) => path.is_ident("test"),
        Meta::List(list) if list.path.is_ident("all") => list
            .parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
            .is_ok_and(|predicates| predicates.iter().any(requires_test)),
        Meta::List(list) if list.path.is_ident("any") => list
            .parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
            .is_ok_and(|predicates| !predicates.is_empty() && predicates.iter().all(requires_test)),
        _ => false,
    }
}

fn reader(method: &str, arguments: usize) -> Option<&'static Reader> {
    READERS
        .iter()
        .find(|reader| reader.method == method && reader.arguments == arguments)
}

/// The number of top-level comma-separated arguments in a call's
/// parenthesized tokens.
fn argument_count(mut cursor: syn::buffer::Cursor<'_>) -> usize {
    let mut count = 0;
    let mut pending = false;
    while !cursor.eof() {
        if let Some((punct, next)) = cursor.punct()
            && punct.as_char() == ','
        {
            count += usize::from(pending);
            pending = false;
            cursor = next;
            continue;
        }
        pending = true;
        cursor = cursor.token_tree().expect("nonempty token cursor").1;
    }
    count + usize::from(pending)
}

#[derive(Default)]
struct Counter {
    sites: BTreeMap<&'static str, BTreeMap<PathBuf, usize>>,
    source: PathBuf,
}

impl Counter {
    fn record(&mut self, reader: &'static Reader) {
        *self
            .sites
            .entry(reader.method)
            .or_default()
            .entry(self.source.clone())
            .or_default() += 1;
    }

    /// Macro bodies are token streams: count `. name (ARGS)` sequences.
    fn macro_calls(&mut self, mut cursor: syn::buffer::Cursor<'_>) {
        while !cursor.eof() {
            if let Some((dot, after_dot)) = cursor.punct()
                && dot.as_char() == '.'
                && let Some((name, after_name)) = after_dot.ident()
                && let Some((arguments, _, _, _)) = after_name.any_group()
                && let Some(reader) = reader(&name.to_string(), argument_count(arguments))
            {
                self.record(reader);
            }
            if let Some((inside, _, _, next)) = cursor.any_group() {
                self.macro_calls(inside);
                cursor = next;
            } else {
                cursor = cursor.token_tree().expect("nonempty token cursor").1;
            }
        }
    }
}

impl<'ast> Visit<'ast> for Counter {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        let attributes = match item {
            syn::Item::Fn(item) => &item.attrs[..],
            syn::Item::Impl(item) => &item.attrs[..],
            syn::Item::Mod(item) => &item.attrs[..],
            syn::Item::Const(item) => &item.attrs[..],
            syn::Item::Static(item) => &item.attrs[..],
            syn::Item::Macro(item) => &item.attrs[..],
            syn::Item::Trait(item) => &item.attrs[..],
            _ => &[],
        };
        if !test_only(attributes) {
            visit::visit_item(self, item);
        }
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if !test_only(&item.attrs) {
            visit::visit_impl_item_fn(self, item);
        }
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if let Some(reader) = reader(&call.method.to_string(), call.args.len()) {
            self.record(reader);
        }
        visit::visit_expr_method_call(self, call);
    }

    fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
        let tokens = syn::buffer::TokenBuffer::new2(invocation.tokens.clone());
        self.macro_calls(tokens.begin());
    }
}

/// Whether PATH (relative to a crate's `src`) is a test file.
fn test_file(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == "tests")
        || path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .is_some_and(|stem| stem.ends_with("_test") || stem == "test_utils")
}

fn sources(directory: &Path, root: &Path, files: &mut Vec<PathBuf>) {
    let mut entries = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        let relative = path.strip_prefix(root).expect("below the source root");
        if test_file(relative) {
            continue;
        }
        if path.is_dir() {
            sources(&path, root, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

/// Whether the crate in DIRECTORY is a test or tooling crate rather than one
/// that ships (`neovm-oracle-tests`, `neomacs-test-fonts`, `xtask`, ...).
fn test_crate(directory: &Path) -> bool {
    directory
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.contains("test") || name == "xtask")
}

/// Count the reader calls in the `src` of every shipping crate below
/// WORKSPACE_CRATES.
fn production_sites(workspace_crates: &Path) -> BTreeMap<&'static str, BTreeMap<PathBuf, usize>> {
    let mut crates = std::fs::read_dir(workspace_crates)
        .unwrap_or_else(|error| panic!("read {}: {error}", workspace_crates.display()))
        .map(|entry| entry.expect("crate entry").path())
        .filter(|directory| !test_crate(directory))
        .map(|directory| directory.join("src"))
        .filter(|source| source.is_dir())
        .collect::<Vec<_>>();
    crates.sort();
    let mut counter = Counter::default();
    for root in crates {
        let mut files = Vec::new();
        sources(&root, &root, &mut files);
        for file in files {
            let source = std::fs::read_to_string(&file)
                .unwrap_or_else(|error| panic!("read {}: {error}", file.display()));
            let syntax = syn::parse_file(&source)
                .unwrap_or_else(|error| panic!("parse {}: {error}", file.display()));
            if test_only(&syntax.attrs) {
                continue;
            }
            counter.source = file;
            counter.visit_file(&syntax);
        }
    }
    counter.sites
}

fn assert_ceilings(
    workspace_crates: &Path,
    sites: &BTreeMap<&'static str, BTreeMap<PathBuf, usize>>,
) {
    let mut over = Vec::new();
    for reader in READERS {
        let files = sites.get(reader.method);
        let count = files.map_or(0, |files| files.values().sum::<usize>());
        if count <= reader.ceiling {
            continue;
        }
        let locations = files
            .into_iter()
            .flatten()
            .map(|(path, count)| {
                format!(
                    "  {}: {count}",
                    path.strip_prefix(workspace_crates)
                        .unwrap_or(path)
                        .display()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        over.push(format!(
            "{count} production calls of `{}` exceed the ceiling {}:\n{locations}",
            reader.method, reader.ceiling
        ));
    }
    assert!(
        over.is_empty(),
        "These readers return a reference into variable storage that writes change \
         in place; read a copied `Value` instead. A ceiling may only decrease.\n{}",
        over.join("\n")
    );
}

#[test]
fn value_reference_readers_can_only_decrease() {
    let workspace_crates = neomacs_infra::crate_root!()
        .parent()
        .expect("the crate sits in the workspace's crates directory")
        .to_path_buf();
    assert_ceilings(&workspace_crates, &production_sites(&workspace_crates));
}

#[test]
fn value_ref_guard_counts_production_calls_only() {
    let directory = tempfile::tempdir().expect("create source fixture");
    let source = directory.path().join("fixture/src");
    std::fs::create_dir_all(source.join("tests")).expect("create fixture crate");
    std::fs::write(
        source.join("lib.rs"),
        r#"
        fn read(obarray: &Obarray, id: SymId) {
            let _ = obarray.symbol_value("x");
            let _ = obarray.symbol_value_id(id).copied();
            let _ = format!("{:?}", obarray.default_value_id(id));
            let _ = KeySym::new().symbol_value();
            let _ = "obarray.symbol_value(\"x\")";
        }
        #[cfg(test)]
        fn checked(obarray: &Obarray) { let _ = obarray.symbol_value("x"); }
        #[cfg(all(test, feature = "jit"))]
        mod more { fn f(o: &Obarray) { let _ = o.symbol_value("x"); } }
        #[cfg(not(test))]
        fn runtime(fwd: &'static LispFwd) { let _ = fwd.load_ref(); }
        "#,
    )
    .expect("write fixture crate");
    std::fs::write(
        source.join("tests/mod.rs"),
        "fn t(o: &Obarray) { let _ = o.symbol_value(\"x\"); }",
    )
    .expect("write fixture tests");
    std::fs::write(
        source.join("probe_test.rs"),
        "fn t(o: &Obarray) { let _ = o.symbol_value(\"x\"); }",
    )
    .expect("write fixture test file");
    let skipped = directory.path().join("fixture-tests/src");
    std::fs::create_dir_all(&skipped).expect("create fixture test crate");
    std::fs::write(
        skipped.join("lib.rs"),
        "fn t(o: &Obarray) { let _ = o.symbol_value(\"x\"); }",
    )
    .expect("write fixture test crate");
    let sites = production_sites(directory.path());
    let count = |method: &str| {
        sites
            .get(method)
            .map_or(0, |files| files.values().sum::<usize>())
    };
    assert_eq!(count("symbol_value"), 1);
    assert_eq!(count("symbol_value_id"), 1);
    assert_eq!(count("default_value_id"), 1);
    assert_eq!(count("load_ref"), 1);
}

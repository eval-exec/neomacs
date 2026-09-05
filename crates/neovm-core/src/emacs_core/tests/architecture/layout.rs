use std::path::{Path, PathBuf};
use syn::{Attribute, Expr, Item, ItemMod, Lit, Meta};

const DOMAINS: &[&str] = &[
    "commands", "display", "editing", "lisp", "runtime", "system", "tests", "text",
];
fn emacs_core_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/emacs_core")
}

fn unexpected_root_domains(root: &Path) -> Vec<String> {
    let mut unexpected = std::fs::read_dir(root)
        .expect("read emacs_core root")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir() && contains_rust_source(&entry.path()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| !DOMAINS.contains(&name.as_str()))
        .collect::<Vec<_>>();
    unexpected.sort();
    unexpected
}

fn contains_rust_source(directory: &Path) -> bool {
    std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .filter_map(Result::ok)
        .any(|entry| {
            let path = entry.path();
            if path.is_dir() {
                contains_rust_source(&path)
            } else {
                path.extension().is_some_and(|extension| extension == "rs")
            }
        })
}

fn rust_files_below(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("read emacs_core source directory") {
        let path = entry.expect("read emacs_core source entry").path();
        if path.is_dir() {
            rust_files_below(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn parsed_rust_file(path: &Path) -> syn::File {
    let source = std::fs::read_to_string(path).expect("read emacs_core Rust source");
    syn::parse_file(&source).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

fn is_test_source(relative: &Path) -> bool {
    relative
        .components()
        .any(|component| component.as_os_str() == "tests")
}

fn cfg_meta_requires_test(meta: &Meta) -> bool {
    match meta {
        Meta::Path(path) => path.is_ident("test"),
        Meta::List(list) => {
            let Ok(nested) = list.parse_args_with(
                syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
            ) else {
                return false;
            };
            if list.path.is_ident("all") {
                nested.iter().any(cfg_meta_requires_test)
            } else if list.path.is_ident("any") {
                !nested.is_empty() && nested.iter().all(cfg_meta_requires_test)
            } else {
                // `not(test)` and unknown cfg predicates do not establish that
                // an item is compiled only by the test configuration.
                false
            }
        }
        Meta::NameValue(_) => false,
    }
}

fn is_cfg_test(attribute: &Attribute) -> bool {
    attribute.path().is_ident("cfg")
        && attribute
            .parse_args::<Meta>()
            .is_ok_and(|meta| cfg_meta_requires_test(&meta))
}

fn is_test_attribute(attribute: &Attribute) -> bool {
    attribute
        .path()
        .segments
        .last()
        .is_some_and(|segment| matches!(segment.ident.to_string().as_str(), "test" | "rstest"))
}

fn path_attribute(module: &ItemMod) -> Option<PathBuf> {
    module.attrs.iter().find_map(|attribute| {
        if !attribute.path().is_ident("path") {
            return None;
        }
        let Meta::NameValue(name_value) = &attribute.meta else {
            return None;
        };
        let Expr::Lit(expression) = &name_value.value else {
            return None;
        };
        let Lit::Str(path) = &expression.lit else {
            return None;
        };
        Some(PathBuf::from(path.value()))
    })
}

fn has_misplaced_test_syntax(syntax: &syn::File) -> bool {
    if syntax.attrs.iter().any(is_cfg_test)
        || syntax.items.iter().any(
            |item| matches!(item, Item::Fn(function) if function.attrs.iter().any(is_test_attribute)),
        )
    {
        return true;
    }

    syntax.items.iter().any(|item| {
        let Item::Mod(module) = item else {
            return false;
        };
        if module.content.is_some() || !module.attrs.iter().any(is_cfg_test) {
            return false;
        }
        path_attribute(module).map_or(module.ident != "tests", |path| !is_test_source(&path))
    })
}

#[test]
fn emacs_core_root_is_a_facade_over_domain_directories() {
    let root = emacs_core_root();
    let mut root_files = std::fs::read_dir(&root)
        .expect("read emacs_core root")
        .filter_map(Result::ok)
        .filter(|entry| {
            let path = entry.path();
            path.is_file() && path.extension().is_some_and(|ext| ext == "rs")
        })
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    root_files.sort();

    assert_eq!(
        root_files,
        ["mod.rs"],
        "emacs_core root is a stable facade; put subsystem files in their owning directory"
    );

    assert_eq!(
        unexpected_root_domains(&root),
        Vec::<String>::new(),
        "emacs_core contains unrecognized root domains; add subsystems below one of DOMAINS"
    );

    for domain in DOMAINS {
        assert!(
            root.join(domain).is_dir(),
            "emacs_core domain directory is missing: {domain}"
        );
    }
}

#[test]
fn production_domains_contain_subsystem_directories_not_loose_rust_files() {
    let root = emacs_core_root();

    for domain in DOMAINS.iter().copied().filter(|domain| *domain != "tests") {
        let loose_rust_files = std::fs::read_dir(root.join(domain))
            .unwrap_or_else(|error| panic!("read emacs_core/{domain}: {error}"))
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "rs"))
            .collect::<Vec<_>>();

        assert!(
            loose_rust_files.is_empty(),
            "emacs_core/{domain} contains loose Rust files: {loose_rust_files:?}; every subsystem owns a directory"
        );
    }
}

#[test]
fn out_of_line_subsystem_tests_live_in_tests_directories() {
    let root = emacs_core_root();
    let mut rust_files = Vec::new();
    rust_files_below(&root, &mut rust_files);

    let mut misplaced = rust_files
        .into_iter()
        .filter_map(|path| {
            let relative = path.strip_prefix(&root).expect("path below emacs_core");
            if is_test_source(relative) {
                return None;
            }
            let stem = relative.file_stem()?.to_string_lossy();
            let test_shaped_name =
                stem == "tests" || stem.ends_with("_test") || stem.ends_with("_tests");
            let syntax = parsed_rust_file(&path);
            (test_shaped_name || has_misplaced_test_syntax(&syntax)).then(|| relative.to_path_buf())
        })
        .collect::<Vec<_>>();
    misplaced.sort();

    assert!(
        misplaced.is_empty(),
        "out-of-line subsystem tests belong in <subsystem>/tests/: {misplaced:?}"
    );
}

#[test]
fn test_placement_guard_reads_rust_test_attributes_and_module_paths() {
    let top_level_test = syn::parse_file("#[test] fn behavior() {}").expect("parse test");
    assert!(has_misplaced_test_syntax(&top_level_test));

    let test_only_file = syn::parse_file("#![cfg(test)] fn helper() {}").expect("parse test");
    assert!(has_misplaced_test_syntax(&test_only_file));

    let non_test_file = syn::parse_file("#![cfg(not(test))] fn helper() {}").expect("parse test");
    assert!(!has_misplaced_test_syntax(&non_test_file));

    let mixed_cfg = syn::parse_file("#![cfg(any(test, feature = \"fuzzing\"))] fn helper() {}")
        .expect("parse test");
    assert!(!has_misplaced_test_syntax(&mixed_cfg));

    let test_conjunction =
        syn::parse_file("#![cfg(all(test, unix))] fn helper() {}").expect("parse test");
    assert!(has_misplaced_test_syntax(&test_conjunction));

    let external_test_module = syn::parse_file("#[cfg(test)] mod checks;").expect("parse test");
    assert!(has_misplaced_test_syntax(&external_test_module));

    let external_test_directory =
        syn::parse_file("#[cfg(test)] #[path = \"tests/checks.rs\"] mod checks;")
            .expect("parse test");
    assert!(!has_misplaced_test_syntax(&external_test_directory));

    let inline_white_box_tests = syn::parse_file(
        "fn implementation() {} #[cfg(test)] mod tests { #[test] fn behavior() {} }",
    )
    .expect("parse test");
    assert!(!has_misplaced_test_syntax(&inline_white_box_tests));
}

#[test]
fn root_domain_guard_reports_unrecognized_domain_directories() {
    let root = tempfile::tempdir().expect("create emacs_core fixture");
    for domain in DOMAINS {
        std::fs::create_dir(root.path().join(domain)).expect("create expected domain");
    }
    std::fs::create_dir(root.path().join(".cache")).expect("create ignored cache directory");
    std::fs::write(root.path().join(".cache/state.json"), "{}")
        .expect("create ignored cache state");
    std::fs::create_dir(root.path().join("notes")).expect("create ignored documentation directory");
    std::fs::write(root.path().join("notes/design.md"), "# Notes")
        .expect("create ignored documentation");
    assert_eq!(unexpected_root_domains(root.path()), Vec::<String>::new());

    std::fs::create_dir(root.path().join("misc")).expect("create unexpected domain");
    std::fs::write(root.path().join("misc/mod.rs"), "// fixture")
        .expect("create unexpected Rust source");

    assert_eq!(unexpected_root_domains(root.path()), ["misc"]);
}

#[test]
fn localized_subr_architecture_is_derived_from_the_compiled_catalog() {
    let catalog = crate::emacs_core::builtins::localized_subr_catalog();
    let expected_startup_order = catalog
        .iter()
        .map(|batch| batch.owner())
        .collect::<Vec<_>>();

    crate::emacs_core::subr::reset_installed_subr_batches();
    let _ctx = crate::emacs_core::eval::Context::new();
    let actual_startup_order = crate::emacs_core::subr::take_installed_subr_batches();
    assert_eq!(
        actual_startup_order, expected_startup_order,
        "production startup must install every compiled batch in catalog order"
    );

    let mut names = std::collections::HashSet::new();
    for batch in catalog {
        assert!(batch.source_file().ends_with("subrs.rs"));
        for spec in batch.specs() {
            assert!(
                names.insert(spec.name()),
                "duplicate localized subr declaration: {}",
                spec.name()
            );
        }
    }
}

/// `ByteCodeObj` (and thus its `.data` ByteCodeFunction) may be NAMED only by
/// its chokepoints: the tagged heap that owns it, `get_bytecode_data` in
/// value/mod.rs, and the pdump load/dump machinery. Everything else must go
/// through `Value::get_bytecode_data` (or the materialized-only peek /
/// interactive probe beside it) — that single seam is where lazy pdump stubs
/// will materialize, and a bypass reading a stub's fields would silently see
/// empty vectors. Test sources are exempt (they build fixtures directly).
#[test]
fn bytecode_obj_is_only_named_by_its_chokepoints() {
    use syn::visit::Visit;

    struct IdentFinder {
        hits: usize,
    }
    impl Visit<'_> for IdentFinder {
        fn visit_ident(&mut self, ident: &syn::Ident) {
            if ident == "ByteCodeObj" {
                self.hits += 1;
            }
        }
    }

    let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let allowed_exact = [
        "tagged/header.rs",
        "tagged/gc.rs",
        "tagged/mutate.rs",
        "tagged/tests.rs",
        "emacs_core/runtime/value/mod.rs",
    ];
    let allowed_prefix = "emacs_core/runtime/pdump/";

    let mut files = Vec::new();
    rust_files_below(&src_root, &mut files);
    let mut violations = Vec::new();
    for path in files {
        let relative = path
            .strip_prefix(&src_root)
            .expect("under src root")
            .to_string_lossy()
            .replace('\\', "/");
        if allowed_exact.contains(&relative.as_str())
            || relative.starts_with(allowed_prefix)
            || is_test_source(Path::new(&relative))
        {
            continue;
        }
        let parsed = parsed_rust_file(&path);
        let mut finder = IdentFinder { hits: 0 };
        finder.visit_file(&parsed);
        if finder.hits > 0 {
            violations.push(format!("{relative} ({} mentions)", finder.hits));
        }
    }
    assert!(
        violations.is_empty(),
        "ByteCodeObj named outside its chokepoints — route through \
         Value::get_bytecode_data instead:\n{}",
        violations.join("\n")
    );
}

/// `runtime/eval/mod.rs` was split from 19,565 lines into domain child modules
/// (gc_pacing, command_loop, apply, construct, special_forms, specpdl,
/// signal_dispatch, vm_shared, pdump_reconstruct, macroexpand) because a
/// single file taking 7% of all commits was the repository's largest
/// merge-conflict surface and every investigation paid a navigation tax
/// across ten domains. What remains is the `Context` struct, its types, the
/// accessor surface, and the public evaluation API. New evaluator work goes
/// in the child module for its domain, or a new one; this ceiling keeps the
/// facade from silently re-absorbing them.
#[test]
fn eval_mod_stays_a_facade_after_the_domain_split() {
    const CEILING: usize = 8_000;
    let path = emacs_core_root().join("runtime/eval/mod.rs");
    let lines = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
        .lines()
        .count();
    assert!(
        lines <= CEILING,
        "runtime/eval/mod.rs is {lines} lines (ceiling {CEILING}); put the new code in the \
         eval/ child module for its domain instead of growing the facade"
    );
}

/// `system/process/mod.rs` was split from 17,770 lines into domain child
/// modules (types, builtins, helpers, bootstrap_vars) because it was the
/// crate's second-largest file and a heavy merge-conflict surface (453
/// commits, 62 in a recent month). What remains is the module's imports and
/// the child/test declarations. New process work goes in the child module for
/// its domain, or a new one; this ceiling keeps the facade honest.
#[test]
fn process_mod_stays_a_facade_after_the_domain_split() {
    const CEILING: usize = 1_500;
    let path = emacs_core_root().join("system/process/mod.rs");
    let lines = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
        .lines()
        .count();
    assert!(
        lines <= CEILING,
        "system/process/mod.rs is {lines} lines (ceiling {CEILING}); put the new code in the \
         process/ child module for its domain instead of growing the facade"
    );
}

/// `runtime/jit/compile.rs` was split from 15,948 lines: its 5,152-line inline
/// test module went out-of-line, then the compiled-leaf data model, the
/// lowering engine, the runtime shims, and the dispatch/control-flow shims each
/// moved to a child module under `jit/compile/`. What remains is the compile
/// driver, the profitability and gate policy, `analyze_cfg`, the
/// op-materialization helpers, and the AOT const-reloc glue -- the
/// orchestration core. Unlike the mod.rs facades, compile.rs is a file module
/// heavily referenced by sibling modules, so its children widen to pub(crate)
/// and it re-exports them; new codegen work goes in the child module for its
/// role, or a new one.
#[test]
fn jit_compile_stays_an_orchestrator_after_the_domain_split() {
    const CEILING: usize = 5_000;
    let path = emacs_core_root().join("runtime/jit/compile.rs");
    let lines = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
        .lines()
        .count();
    assert!(
        lines <= CEILING,
        "runtime/jit/compile.rs is {lines} lines (ceiling {CEILING}); put the new code in the \
         jit/compile/ child module for its role instead of growing the orchestrator"
    );
}

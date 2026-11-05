//! PC8: exact registry mint/getter owners across the value/core boundary.

use syn::visit::{self, Visit};

use super::architecture_production_source_roots::production_source_files;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum RegistryEntry {
    RawConstructor,
    RawStruct,
    CheckedGetter,
    RenamedRegistryType,
    RenamedGetter,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RegistrySite {
    entry: RegistryEntry,
    file: String,
    function: String,
}

fn cfg_is_test(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(path) => path.is_ident("test"),
        syn::Meta::List(list) => {
            list.path.is_ident("all")
                && list
                    .parse_args_with(
                        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                    )
                    .is_ok_and(|predicates| predicates.iter().any(cfg_is_test))
        }
        syn::Meta::NameValue(_) => false,
    }
}

fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attribute| {
        attribute.path().is_ident("test")
            || (attribute.path().is_ident("cfg")
                && attribute
                    .parse_args::<syn::Meta>()
                    .is_ok_and(|meta| cfg_is_test(&meta)))
    })
}

struct RegistryScan<'a> {
    file: &'a str,
    implementation: Option<String>,
    functions: Vec<String>,
    sites: Vec<RegistrySite>,
}

impl RegistryScan<'_> {
    fn record(&mut self, entry: RegistryEntry) {
        let function = self
            .functions
            .last()
            .cloned()
            .unwrap_or_else(|| "<module>".to_owned());
        self.sites.push(RegistrySite {
            entry,
            file: self.file.to_owned(),
            function,
        });
    }

    fn is_registry_path(&self, path: &syn::Path) -> bool {
        path.is_ident("InFlightRegistryHandle")
            || path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "InFlightRegistryHandle")
            || (path.is_ident("Self")
                && self.implementation.as_deref() == Some("InFlightRegistryHandle"))
    }
}

impl<'ast> Visit<'ast> for RegistryScan<'_> {
    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        if !test_only(&module.attrs) {
            visit::visit_item_mod(self, module);
        }
    }

    fn visit_item_impl(&mut self, implementation: &'ast syn::ItemImpl) {
        if test_only(&implementation.attrs) {
            return;
        }
        let owner = match &*implementation.self_ty {
            syn::Type::Path(path) => path.path.segments.last().map(|part| part.ident.to_string()),
            _ => None,
        };
        let previous = std::mem::replace(&mut self.implementation, owner);
        visit::visit_item_impl(self, implementation);
        self.implementation = previous;
    }

    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        if !test_only(&function.attrs) {
            self.functions.push(function.sig.ident.to_string());
            visit::visit_item_fn(self, function);
            self.functions.pop();
        }
    }

    fn visit_impl_item_fn(&mut self, function: &'ast syn::ImplItemFn) {
        if !test_only(&function.attrs) {
            let name = match &self.implementation {
                Some(owner) => format!("{owner}::{}", function.sig.ident),
                None => function.sig.ident.to_string(),
            };
            self.functions.push(name);
            visit::visit_impl_item_fn(self, function);
            self.functions.pop();
        }
    }

    // Inspect references, not just calls: taking a constructor/getter as a
    // function pointer must not bypass the exact containing-function guard.
    fn visit_expr_path(&mut self, expression: &'ast syn::ExprPath) {
        let path = &expression.path;
        if let Some(last) = path.segments.last() {
            if last.ident == "current_in_flight_registry_handle" {
                self.record(RegistryEntry::CheckedGetter);
            } else if last.ident == "new" {
                let mut owner = path.clone();
                owner.segments.pop();
                let qualified_owner = expression.qself.as_ref().is_some_and(|qualified| {
                    matches!(&*qualified.ty, syn::Type::Path(owner) if self.is_registry_path(&owner.path))
                });
                if self.is_registry_path(&owner) || qualified_owner {
                    self.record(RegistryEntry::RawConstructor);
                }
            }
        }
        visit::visit_expr_path(self, expression);
    }

    // Reject renamed imports rather than letting a local alias hide the
    // constructor/getter from this narrow name-based source guard.
    fn visit_item_type(&mut self, alias: &'ast syn::ItemType) {
        if !test_only(&alias.attrs) {
            if let syn::Type::Path(path) = &*alias.ty
                && self.is_registry_path(&path.path)
            {
                self.record(RegistryEntry::RenamedRegistryType);
            }
            visit::visit_item_type(self, alias);
        }
    }

    fn visit_use_tree(&mut self, tree: &'ast syn::UseTree) {
        if let syn::UseTree::Rename(rename) = tree {
            if rename.ident == "InFlightRegistryHandle" {
                self.record(RegistryEntry::RenamedRegistryType);
            } else if rename.ident == "current_in_flight_registry_handle" {
                self.record(RegistryEntry::RenamedGetter);
            }
        }
        visit::visit_use_tree(self, tree);
    }

    fn visit_expr_struct(&mut self, expression: &'ast syn::ExprStruct) {
        if self.is_registry_path(&expression.path) {
            self.record(RegistryEntry::RawStruct);
        }
        visit::visit_expr_struct(self, expression);
    }
}

fn registry_sites(file: &str, source: &str) -> Vec<RegistrySite> {
    let syntax = syn::parse_file(source).expect("parse actual registry caller source");
    if test_only(&syntax.attrs) {
        return Vec::new();
    }
    let mut scan = RegistryScan {
        file,
        implementation: None,
        functions: Vec::new(),
        sites: Vec::new(),
    };
    scan.visit_file(&syntax);
    scan.sites
}

fn expected_sites() -> Vec<RegistrySite> {
    [
        (
            RegistryEntry::RawConstructor,
            "neovm-value/src/error/mod.rs",
            "current_in_flight_registry_handle",
        ),
        (
            RegistryEntry::RawStruct,
            "neovm-value/src/error/mod.rs",
            "InFlightRegistryHandle::new",
        ),
        (
            RegistryEntry::CheckedGetter,
            "neovm-value/src/error/mod.rs",
            "InFlightRoots::pin",
        ),
        // S4 retains the checked getter's original lazy point and exports
        // only copied observations through the separately owned surface.
        (
            RegistryEntry::CheckedGetter,
            "neovm-value/src/error/registry_support.rs",
            "current_in_flight_registry_observer_for_test",
        ),
        (
            RegistryEntry::CheckedGetter,
            "neovm-eval/src/emacs_core/runtime/eval/construct.rs",
            "RegistryHandles::current",
        ),
        (
            RegistryEntry::CheckedGetter,
            "neovm-eval/src/emacs_core/runtime/eval/pdump_reconstruct.rs",
            "Context::from_dump",
        ),
    ]
    .into_iter()
    .map(|(entry, file, function)| RegistrySite {
        entry,
        file: file.to_owned(),
        function: function.to_owned(),
    })
    .collect()
}

fn validate_sites(mut actual: Vec<RegistrySite>) -> Result<(), Vec<RegistrySite>> {
    actual.sort();
    let mut expected = expected_sites();
    expected.sort();
    if actual == expected {
        Ok(())
    } else {
        Err(actual)
    }
}

#[test]
fn actual_registry_mints_and_getters_have_exact_product_owners() {
    let mut actual = Vec::new();
    for file in production_source_files() {
        if !matches!(
            file.crate_dir.as_str(),
            "neovm-core" | "neovm-value" | "neovm-eval"
        ) || file.relative.split('/').any(|part| part == "tests")
        {
            continue;
        }
        let source = std::fs::read_to_string(&file.path).expect("read actual product source");
        if source.contains("InFlightRegistryHandle")
            || source.contains("current_in_flight_registry_handle")
        {
            actual.extend(registry_sites(&file.workspace_relative(), &source));
        }
    }
    assert_eq!(
        validate_sites(actual),
        Ok(()),
        "PC8 exact registry authority callers changed"
    );
}

#[test]
fn source_guard_detects_extra_raw_constructor_and_alias_references() {
    let source = "fn escaped() { let raw = InFlightRegistryHandle::new; raw(None); }";
    let sites = registry_sites("neovm-value/src/error/mod.rs", source);
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].entry, RegistryEntry::RawConstructor);
    assert!(!expected_sites().contains(&sites[0]));
    let mut mutated = expected_sites();
    mutated.extend(sites);
    assert!(validate_sites(mutated).is_err());
}

#[test]
fn source_guard_detects_duplicate_mints_inside_the_allowed_getter() {
    let source = "fn current_in_flight_registry_handle() { \
        InFlightRegistryHandle::new(None); InFlightRegistryHandle::new(None); }";
    let sites = registry_sites("neovm-value/src/error/mod.rs", source);
    assert_eq!(sites.len(), 2);
    let expected = expected_sites();
    assert!(sites.iter().all(|site| expected.contains(site)));
    assert_eq!(expected.iter().filter(|site| **site == sites[0]).count(), 1);
    let mut mutated = expected_sites();
    mutated.retain(|site| site.entry != RegistryEntry::RawConstructor);
    mutated.extend(sites);
    assert!(validate_sites(mutated).is_err());
}

#[test]
fn source_guard_detects_getter_in_the_wrong_registry_handles_method() {
    let source = "impl RegistryHandles { \
        fn of() { current_in_flight_registry_handle(); } }";
    let sites = registry_sites(
        "neovm-eval/src/emacs_core/runtime/eval/construct.rs",
        source,
    );
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].entry, RegistryEntry::CheckedGetter);
    assert!(!expected_sites().contains(&sites[0]));
    let mut mutated = expected_sites();
    mutated.extend(sites);
    assert!(validate_sites(mutated).is_err());
}

#[test]
fn source_guard_detects_raw_struct_forgery_outside_the_constructor() {
    let source = "fn forged() { \
        InFlightRegistryHandle { heap_identity: None, table: Default::default() }; }";
    let sites = registry_sites("neovm-value/src/error/mod.rs", source);
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].entry, RegistryEntry::RawStruct);
    assert!(!expected_sites().contains(&sites[0]));
    let mut mutated = expected_sites();
    mutated.extend(sites);
    assert!(validate_sites(mutated).is_err());
}

#[test]
fn source_guard_detects_renamed_registry_imports() {
    let source = "use super::InFlightRegistryHandle as Alias; \
        fn escaped() { Alias::new(None); }";
    let sites = registry_sites("neovm-value/src/error/mod.rs", source);
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].entry, RegistryEntry::RenamedRegistryType);
    let mut mutated = expected_sites();
    mutated.extend(sites);
    assert!(validate_sites(mutated).is_err());
}

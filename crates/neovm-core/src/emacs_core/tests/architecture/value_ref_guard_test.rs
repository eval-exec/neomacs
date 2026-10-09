//! Forbid borrowed variable-slot readers after the P7.4 copied-value cutover.
//!
//! All five old reader call counts are zero across shipping workspace crates.
//! Within symbol/forward modules, their names and get_ref are also forbidden
//! for every arity in calls and free/impl/trait/foreign function declarations.
//! Direct &Value/&TaggedValue result types, including nested containers, are
//! rejected there too; input borrows are legitimate and are not checked.
//!
//! Outside those modules, the historical reader arities distinguish unrelated
//! zero-argument keysym constructors. Unrelated Pin/image get_ref operations
//! remain allowed. Test crates/files and test-only items remain excluded.
//! The check parses source syntax, not expanded/name-resolved Rust: it does
//! not resolve type aliases, identifier metavariables or include expansions.
//! Unparseable macro templates use fixed-name token checks rather than a
//! complete macro expansion or a type/result parser.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use syn::parse::Parser;
use syn::visit::{self, Visit};
use syn::{Attribute, Meta};

struct Reader {
    method: &'static str,
    arguments: usize,
}

const READERS: &[Reader] = &[
    Reader {
        method: "symbol_value",
        arguments: 1,
    },
    Reader {
        method: "symbol_value_id",
        arguments: 1,
    },
    Reader {
        method: "default_value_id",
        arguments: 1,
    },
    Reader {
        method: "load_ref",
        arguments: 0,
    },
    Reader {
        method: "plain_value_ref",
        arguments: 0,
    },
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum OwnerScope {
    #[default]
    Other,
    SymbolOrForward,
}

impl OwnerScope {
    fn within(self, name: &str) -> Self {
        if matches!(self, Self::SymbolOrForward) || matches!(name, "symbol" | "forward") {
            Self::SymbolOrForward
        } else {
            Self::Other
        }
    }

    fn for_path(path: &Path) -> Self {
        if path
            .components()
            .any(|part| matches!(part.as_os_str().to_str(), Some("symbol" | "forward")))
            || matches!(
                path.file_stem().and_then(|stem| stem.to_str()),
                Some("symbol" | "forward")
            )
        {
            Self::SymbolOrForward
        } else {
            Self::Other
        }
    }
}

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

fn forbidden_name(method: &str) -> Option<&'static str> {
    READERS
        .iter()
        .find(|reader| reader.method == method)
        .map(|reader| reader.method)
        .or_else(|| (method == "get_ref").then_some("get_ref"))
}

fn call_reader(method: &str, arguments: usize, scope: OwnerScope) -> Option<&'static str> {
    if scope == OwnerScope::SymbolOrForward {
        forbidden_name(method)
    } else {
        READERS
            .iter()
            .find(|reader| reader.method == method && reader.arguments == arguments)
            .map(|reader| reader.method)
    }
}

fn owner_name(name: &str) -> bool {
    matches!(
        name,
        "symbol"
            | "forward"
            | "Obarray"
            | "LispSymbol"
            | "LispFwd"
            | "LispIntFwd"
            | "LispObjFwd"
            | "LispKboardObjFwd"
    )
}

fn owner_path(path: &syn::Path) -> bool {
    path.segments
        .iter()
        .any(|segment| owner_name(&segment.ident.to_string()))
}

/// Parse real argument expressions first so commas in turbofish arguments do
/// not change arity. A macro template may contain metavariables rather than
/// Rust expressions; the fallback retains the previous grouped-token count.
fn argument_count(mut cursor: syn::buffer::Cursor<'_>) -> usize {
    let parser = syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated;
    if let Ok(arguments) = parser.parse2(cursor.token_stream()) {
        return arguments.len();
    }
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

fn value_type(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Path(path) => {
            path.qself.is_none()
                && path.path.segments.last().is_some_and(|segment| {
                    matches!(segment.ident.to_string().as_str(), "Value" | "TaggedValue")
                })
        }
        syn::Type::Paren(ty) => value_type(&ty.elem),
        syn::Type::Group(ty) => value_type(&ty.elem),
        _ => false,
    }
}

#[derive(Default)]
struct ValueReferenceResult {
    found: bool,
}

impl<'ast> Visit<'ast> for ValueReferenceResult {
    fn visit_type_reference(&mut self, ty: &'ast syn::TypeReference) {
        self.found |= value_type(&ty.elem);
        visit::visit_type_reference(self, ty);
    }

    // A function returned by a function may legitimately accept &Value.
    // Inspect its output, without mistaking its parameters for slot views.
    fn visit_type_fn_ptr(&mut self, ty: &'ast syn::TypeFnPtr) {
        self.visit_return_type(&ty.output);
    }

    fn visit_parenthesized_generic_arguments(
        &mut self,
        arguments: &'ast syn::ParenthesizedGenericArguments,
    ) {
        self.visit_return_type(&arguments.output);
    }
}

fn borrowed_value_result(signature: &syn::Signature) -> bool {
    let mut result = ValueReferenceResult::default();
    result.visit_return_type(&signature.output);
    result.found
}

type Sites = BTreeMap<&'static str, BTreeMap<PathBuf, usize>>;

#[derive(Default)]
struct Survey {
    calls: Sites,
    declarations: Sites,
    borrowed_results: BTreeMap<PathBuf, Vec<String>>,
}

#[derive(Default)]
struct Counter {
    survey: Survey,
    source: PathBuf,
    scope: OwnerScope,
}

impl Counter {
    fn record_call(&mut self, method: &'static str) {
        *self
            .survey
            .calls
            .entry(method)
            .or_default()
            .entry(self.source.clone())
            .or_default() += 1;
    }

    fn record_declaration(&mut self, method: &'static str) {
        *self
            .survey
            .declarations
            .entry(method)
            .or_default()
            .entry(self.source.clone())
            .or_default() += 1;
    }

    fn signature(&mut self, signature: &syn::Signature) {
        if self.scope != OwnerScope::SymbolOrForward {
            return;
        }
        if let Some(method) = forbidden_name(&signature.ident.to_string()) {
            self.record_declaration(method);
        }
        if borrowed_value_result(signature) {
            self.survey
                .borrowed_results
                .entry(self.source.clone())
                .or_default()
                .push(signature.ident.to_string());
        }
    }

    /// Complete item streams inside macros use the AST visitor, retaining
    /// cfg(test) exclusions and result checks. Unexpanded templates/expressions
    /// fall back to fixed `.name(ARGS)`, qualified/free calls and `fn name`
    /// token recognition. Metavariable-generated identifiers remain unresolved.
    fn macro_tokens(&mut self, mut cursor: syn::buffer::Cursor<'_>) {
        if let Ok(file) = syn::parse2::<syn::File>(cursor.token_stream()) {
            if !test_only(&file.attrs) {
                self.visit_file(&file);
            }
            return;
        }
        // Trait/foreign signatures without a body are not standalone files.
        if let Ok(method) = syn::parse2::<syn::TraitItemFn>(cursor.token_stream()) {
            self.visit_trait_item_fn(&method);
            return;
        }
        // Macro argument lists and complete expressions can be parsed too.
        // This catches UFCS and turbofish calls inside format/assert macros
        // without manually interpreting Rust's generic-angle syntax.
        let parser = syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated;
        if let Ok(expressions) = parser.parse2(cursor.token_stream()) {
            for expression in &expressions {
                self.visit_expr(expression);
            }
            return;
        }
        while !cursor.eof() {
            if self.scope == OwnerScope::SymbolOrForward
                && let Some((keyword, after_keyword)) = cursor.ident()
                && keyword == "fn"
                && let Some((name, after_name)) = after_keyword.ident()
                && let Some(method) = forbidden_name(&name.to_string())
            {
                self.record_declaration(method);
                // The declared name is not also a free-function call.
                cursor = after_name;
                continue;
            }
            if let Some((dot, after_dot)) = cursor.punct()
                && dot.as_char() == '.'
                && let Some((name, after_name)) = after_dot.ident()
                && let Some((arguments, _, _, _)) = after_name.any_group()
                && let Some(method) =
                    call_reader(&name.to_string(), argument_count(arguments), self.scope)
            {
                self.record_call(method);
                cursor = after_name;
                continue;
            }
            if let Some((name, after_name)) = cursor.ident() {
                let mut last = name.to_string();
                let mut owner = owner_name(&last);
                let mut qualified = false;
                let mut after_path = after_name;
                while let Some((colon, next)) = after_path.punct()
                    && colon.as_char() == ':'
                    && let Some((second, next)) = next.punct()
                    && second.as_char() == ':'
                    && let Some((segment, next)) = next.ident()
                {
                    last = segment.to_string();
                    owner |= owner_name(&last);
                    qualified = true;
                    after_path = next;
                }
                if let Some((_, _, _, _)) = after_path.any_group()
                    && (self.scope == OwnerScope::SymbolOrForward || (qualified && owner))
                    && let Some(method) = forbidden_name(&last)
                {
                    self.record_call(method);
                    cursor = after_path;
                    continue;
                }
            }
            if let Some((inside, _, _, next)) = cursor.any_group() {
                self.macro_tokens(inside);
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
            syn::Item::TraitAlias(item) => &item.attrs[..],
            syn::Item::ForeignMod(item) => &item.attrs[..],
            syn::Item::Type(item) => &item.attrs[..],
            syn::Item::Struct(item) => &item.attrs[..],
            syn::Item::Enum(item) => &item.attrs[..],
            syn::Item::Union(item) => &item.attrs[..],
            syn::Item::Use(item) => &item.attrs[..],
            syn::Item::ExternCrate(item) => &item.attrs[..],
            _ => &[],
        };
        if !test_only(attributes) {
            visit::visit_item(self, item);
        }
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        let previous = self.scope;
        self.scope = self.scope.within(&item.ident.to_string());
        visit::visit_item_mod(self, item);
        self.scope = previous;
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if !test_only(&item.attrs) {
            self.signature(&item.sig);
            visit::visit_item_fn(self, item);
        }
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if !test_only(&item.attrs) {
            self.signature(&item.sig);
            visit::visit_impl_item_fn(self, item);
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

    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        if !test_only(&item.attrs) {
            self.signature(&item.sig);
            visit::visit_trait_item_fn(self, item);
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

    fn visit_foreign_item_fn(&mut self, item: &'ast syn::ForeignItemFn) {
        if !test_only(&item.attrs) {
            self.signature(&item.sig);
            visit::visit_foreign_item_fn(self, item);
        }
    }

    fn visit_foreign_item(&mut self, item: &'ast syn::ForeignItem) {
        let attributes = match item {
            syn::ForeignItem::Fn(item) => &item.attrs[..],
            syn::ForeignItem::Static(item) => &item.attrs[..],
            syn::ForeignItem::Type(item) => &item.attrs[..],
            syn::ForeignItem::Macro(item) => &item.attrs[..],
            _ => &[],
        };
        if !test_only(attributes) {
            visit::visit_foreign_item(self, item);
        }
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if let Some(method) = call_reader(&call.method.to_string(), call.args.len(), self.scope) {
            self.record_call(method);
        }
        visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(function) = &*call.func
            && let Some(segment) = function.path.segments.last()
            && (self.scope == OwnerScope::SymbolOrForward
                || owner_path(&function.path)
                || function.qself.as_ref().is_some_and(
                    |qself| matches!(&*qself.ty, syn::Type::Path(path) if owner_path(&path.path)),
                ))
            && let Some(method) = forbidden_name(&segment.ident.to_string())
        {
            self.record_call(method);
        }
        visit::visit_expr_call(self, call);
    }

    fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
        let tokens = syn::buffer::TokenBuffer::new2(invocation.tokens.clone());
        self.macro_tokens(tokens.begin());
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

/// Census every shipping crate, preserving the existing test-file exclusions.
fn production_sites(workspace_crates: &Path) -> Survey {
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
            counter.scope =
                OwnerScope::for_path(file.strip_prefix(&root).expect("source-relative path"));
            counter.source = file;
            counter.visit_file(&syntax);
        }
    }
    counter.survey
}

fn assert_no_borrowed_readers(workspace_crates: &Path, survey: &Survey) {
    let locations = |sites: &Sites, kind: &str| {
        sites
            .iter()
            .flat_map(|(method, files)| {
                files.iter().map(move |(path, count)| {
                    format!(
                        "  {kind} `{method}` at {}: {count}",
                        path.strip_prefix(workspace_crates)
                            .unwrap_or(path)
                            .display()
                    )
                })
            })
            .collect::<Vec<_>>()
    };
    let mut violations = locations(&survey.calls, "call");
    violations.extend(locations(&survey.declarations, "declaration"));
    violations.extend(survey.borrowed_results.iter().flat_map(|(path, names)| {
        names.iter().map(move |name| {
            format!(
                "  direct Value-reference result `{name}` at {}",
                path.strip_prefix(workspace_crates)
                    .unwrap_or(path)
                    .display()
            )
        })
    }));
    assert!(
        violations.is_empty(),
        "Borrowed variable-slot readers are forbidden; return a copied Value. \
         There are no editable ceilings.\n{}",
        violations.join("\n")
    );
}

#[test]
fn value_reference_readers_are_absent_from_shipping_code() {
    let workspace_crates = neomacs_infra::crate_root!()
        .parent()
        .expect("the crate sits in the workspace's crates directory")
        .to_path_buf();
    assert_no_borrowed_readers(&workspace_crates, &production_sites(&workspace_crates));
}

fn parsed_fixture(source: &str, scope: OwnerScope) -> Survey {
    let file = syn::parse_file(source).expect("valid Rust source fixture");
    let mut counter = Counter {
        source: PathBuf::from("fixture.rs"),
        scope,
        ..Counter::default()
    };
    counter.visit_file(&file);
    counter.survey
}

fn count(sites: &Sites, method: &str) -> usize {
    sites.get(method).map_or(0, |files| files.values().sum())
}

#[test]
fn value_ref_guard_checks_declarations_at_every_arity_only_in_owner_modules() {
    let survey = parsed_fixture(
        r#"
        fn symbol_value(vmctx: i64, id: i64) -> i64 { 0 }
        fn get_ref(value: &Value) -> bool { true }
        mod symbol {
            fn symbol_value() {}
            impl Obarray { fn symbol_value_id(&self, id: SymId, extra: bool) {} }
            trait Reads {
                fn default_value_id(&self);
                fn get_ref(&self) -> bool;
                #[cfg(test)] const IGNORED: () = { let _ = source.get_ref(); };
            }
            impl Obarray {
                #[cfg(test)] const IGNORED: () = { let _ = source.get_ref(); };
            }
            unsafe extern "C" { fn load_ref(a: i64, b: i64); }
            #[cfg(test)] fn plain_value_ref() -> &'static Value { todo!() }
        }
        mod forward { fn plain_value_ref(arg: usize) {} }
    "#,
        OwnerScope::Other,
    );
    for name in [
        "symbol_value",
        "symbol_value_id",
        "default_value_id",
        "get_ref",
        "load_ref",
        "plain_value_ref",
    ] {
        assert_eq!(count(&survey.declarations, name), 1, "declaration {name}");
    }
    assert!(survey.borrowed_results.is_empty());
    assert!(survey.calls.is_empty());
}

#[test]
fn value_ref_guard_checks_macro_calls_and_fixed_declarations() {
    let survey = parsed_fixture(
        r#"
        mod symbol {
            macro_rules! declaration { () => { fn get_ref<T>(&self, value: T) {} }; }
            macro_rules! method { ($o:expr) => { $o.symbol_value("x") }; }
            macro_rules! free { ($o:expr) => { default_value_id($o) }; }
            macro_rules! ignored { () => { #[cfg(test)] fn get_ref() -> &Value { todo!() } }; }
            fn call(o: &Obarray) {
                let _ = format!("{:?}", o.load_ref());
                let _ = o.plain_value_ref(1, 2);
            }
        }
        fn outside(o: &Obarray) {
            let _ = format!("{:?}", o.symbol_value(collection::<A, B>()));
            let _ = format!("{:?}", KeySym::new().symbol_value());
        }
        macro_rules! ufcs { ($o:expr) => { Obarray::symbol_value($o, "x") }; }
    "#,
        OwnerScope::Other,
    );
    assert_eq!(count(&survey.declarations, "get_ref"), 1);
    assert_eq!(count(&survey.calls, "symbol_value"), 3);
    assert_eq!(count(&survey.calls, "default_value_id"), 1);
    assert_eq!(count(&survey.calls, "load_ref"), 1);
    assert_eq!(count(&survey.calls, "plain_value_ref"), 1);
    assert!(survey.borrowed_results.is_empty());
}

#[test]
fn value_ref_guard_rejects_nested_value_results_without_flagging_inputs() {
    let survey = parsed_fixture(
        r#"
        mod forward {
            fn nested<'a>() -> Result<Option<&'a crate::tagged::TaggedValue>, Error> { todo!() }
            trait View { fn tuples(&self) -> (&Value, Option<&mut (Value)>); }
            fn borrowed_input(value: &Value, other: Option<&TaggedValue>) -> bool { true }
            fn callback_input() -> fn(&Value) -> bool { todo!() }
            fn closure_input() -> impl Fn(&TaggedValue) -> bool { todo!() }
            fn callback_output() -> fn(&Value) -> Option<&'static Value> { todo!() }
        }
        fn unrelated_view() -> &Value { todo!() }
    "#,
        OwnerScope::Other,
    );
    let names = survey
        .borrowed_results
        .get(Path::new("fixture.rs"))
        .expect("result violations");
    assert_eq!(
        names.iter().map(String::as_str).collect::<Vec<_>>(),
        ["nested", "tuples", "callback_output"]
    );
    assert!(survey.calls.is_empty());
    assert!(survey.declarations.is_empty());
}

#[test]
fn value_ref_guard_keeps_keysym_and_unrelated_get_ref_legal() {
    let survey = parsed_fixture(
        r#"
        fn symbol_value(vmctx: i64, id: i64) -> i64 { 0 }
        fn outside(pin: Pin<&Widget>, image: &Image) {
            let _ = KeySym::new().symbol_value();
            let _ = KeymapMarker::Keymap.symbol_value();
            let _ = pin.get_ref();
            let _ = image.get_ref(1);
            let _ = format!("{}", pin.get_ref());
            let _ = "obarray.symbol_value(\"x\") and fn get_ref() -> &Value";
            let _ = symbol_value(1, 2);
        }
    "#,
        OwnerScope::Other,
    );
    assert!(survey.calls.is_empty());
    assert!(survey.declarations.is_empty());
    assert!(survey.borrowed_results.is_empty());
}

#[test]
fn value_ref_guard_counts_ufcs_and_free_calls_in_owner_scope() {
    let survey = parsed_fixture(
        r#"
        fn outside(o: &Obarray) {
            let _ = Obarray::symbol_value(o, "x");
            let _ = format!("{:?}", Obarray::symbol_value(o, "x"));
            let _ = <Obarray as Reads>::symbol_value(o, "x");
        }
        mod symbol {
            fn call(o: &Obarray) {
                let _ = symbol_value(o, "x");
                let _ = Obarray::symbol_value_id(o, 1);
                let _ = get_ref();
            }
        }
    "#,
        OwnerScope::Other,
    );
    assert_eq!(count(&survey.calls, "symbol_value"), 4);
    assert_eq!(count(&survey.calls, "symbol_value_id"), 1);
    assert_eq!(count(&survey.calls, "get_ref"), 1);
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
    let survey = production_sites(directory.path());
    let sites = &survey.calls;
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

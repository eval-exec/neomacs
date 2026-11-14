//! Closed source guard for the canonical static descriptor contract.
//!
//! Coverage is deliberately syntactic: the canonical Core manifest, reachable
//! unit-runtime registrar functions, and conservatively all eval src Rust
//! files outside dedicated test/support paths. This is not a Rust alias/type
//! or macro expansion resolver.
//! Strings/comments and test/test-support-only items are not descriptor sites.
use super::StaticHome;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use syn::parse::{Parse, ParseStream};
use syn::visit::{self, Visit};

const CORE: &str = "crates/neovm-core/src/emacs_core/builtins/subrs.rs";
const UNIT: &str = "crates/neovm-eval/src/test_utils.rs";
const BUILTINS: &str = "crates/neovm-eval/src/emacs_core/lisp/native/builtins";

fn fail(message: impl AsRef<str>) -> String {
    format!("STATIC_SUBR_HOME: {}", message.as_ref())
}
fn read(root: &Path, file: &str) -> Result<syn::File, String> {
    let source =
        std::fs::read_to_string(root.join(file)).map_err(|e| fail(format!("read {file}: {e}")))?;
    syn::parse_file(&source).map_err(|e| fail(format!("parse {file}: {e}")))
}
fn path(path: &syn::Path) -> Option<Vec<String>> {
    if path.leading_colon.is_some()
        || path
            .segments
            .iter()
            .any(|s| !matches!(s.arguments, syn::PathArguments::None))
    {
        return None;
    }
    Some(path.segments.iter().map(|s| s.ident.to_string()).collect())
}
fn expr_path(expr: &syn::Expr) -> Option<Vec<String>> {
    match expr {
        syn::Expr::Path(p) if p.qself.is_none() => path(&p.path),
        syn::Expr::Paren(p) if p.attrs.is_empty() => expr_path(&p.expr),
        syn::Expr::Group(g) if g.attrs.is_empty() => expr_path(&g.expr),
        syn::Expr::Block(b) if b.attrs.is_empty() => {
            let [syn::Stmt::Expr(tail, None)] = b.block.stmts.as_slice() else {
                return None;
            };
            expr_path(tail)
        }
        _ => None,
    }
}
fn expected_path(home: &StaticHome, first: &str) -> Vec<String> {
    [
        first,
        "emacs_core",
        "builtins",
        &home.module,
        &home.static_ident,
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
fn spec_type(ty: &syn::Type) -> bool {
    matches!(ty, syn::Type::Path(p) if p.qself.is_none()
        && p.path.segments.last().is_some_and(|s| s.ident == "SubrSpec"))
}
fn exact_spec_type(ty: &syn::Type) -> bool {
    matches!(ty, syn::Type::Path(p) if p.qself.is_none() && p.path.is_ident("SubrSpec"))
}
fn cfg_value(meta: &syn::Meta) -> Option<bool> {
    match meta {
        syn::Meta::Path(p) if p.is_ident("test") => Some(false),
        syn::Meta::NameValue(n) if n.path.is_ident("feature") => match &n.value {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) if s.value() == "test-support" => Some(false),
            _ => None,
        },
        syn::Meta::List(l) if l.path.is_ident("not") => l
            .parse_args::<syn::Meta>()
            .ok()
            .and_then(|m| cfg_value(&m))
            .map(|v| !v),
        syn::Meta::List(l) if l.path.is_ident("all") || l.path.is_ident("any") => {
            let all = l.path.is_ident("all");
            let values = l
                .parse_args_with(
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                )
                .ok()?;
            let mut unknown = false;
            for m in values {
                match cfg_value(&m) {
                    Some(false) if all => return Some(false),
                    Some(true) if !all => return Some(true),
                    None => unknown = true,
                    _ => {}
                }
            }
            if unknown { None } else { Some(all) }
        }
        _ => None,
    }
}
fn excluded(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("test")
            || (a.path().is_ident("cfg")
                && a.parse_args::<syn::Meta>().ok().and_then(|m| cfg_value(&m)) == Some(false))
    })
}
fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Const(i) => &i.attrs,
        syn::Item::Enum(i) => &i.attrs,
        syn::Item::ExternCrate(i) => &i.attrs,
        syn::Item::Fn(i) => &i.attrs,
        syn::Item::ForeignMod(i) => &i.attrs,
        syn::Item::Impl(i) => &i.attrs,
        syn::Item::Macro(i) => &i.attrs,
        syn::Item::Mod(i) => &i.attrs,
        syn::Item::Static(i) => &i.attrs,
        syn::Item::Struct(i) => &i.attrs,
        syn::Item::Trait(i) => &i.attrs,
        syn::Item::TraitAlias(i) => &i.attrs,
        syn::Item::Type(i) => &i.attrs,
        syn::Item::Union(i) => &i.attrs,
        syn::Item::Use(i) => &i.attrs,
        _ => &[],
    }
}
#[derive(Clone, PartialEq, Eq)]
enum Owner {
    Other,
    Function(String),
    Static(String),
}
#[derive(Default)]
struct Facts {
    constructors: Vec<(Owner, String)>,
    calls: Vec<(Owner, Vec<String>)>,
    references: Vec<(Owner, Vec<String>)>,
    registrations: Vec<Option<Vec<String>>>,
}
struct Collector {
    owner: Owner,
    facts: Facts,
    errors: Vec<String>,
}
impl Collector {
    fn new() -> Self {
        Self {
            owner: Owner::Other,
            facts: Facts::default(),
            errors: vec![],
        }
    }
}
impl<'ast> Visit<'ast> for Collector {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        if !excluded(item_attrs(item)) {
            visit::visit_item(self, item);
        }
    }
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let prior = self.owner.clone();
        self.owner = Owner::Function(item.sig.ident.to_string());
        visit::visit_block(self, &item.block);
        self.owner = prior;
    }
    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        let prior = self.owner.clone();
        self.owner = Owner::Static(item.ident.to_string());
        self.visit_expr(&item.expr);
        self.owner = prior;
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        fn leaves(tree: &syn::UseTree, prefix: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
            match tree {
                syn::UseTree::Path(p) => {
                    prefix.push(p.ident.to_string());
                    leaves(&p.tree, prefix, out);
                    prefix.pop();
                }
                syn::UseTree::Name(n) => {
                    let mut p = prefix.clone();
                    p.push(n.ident.to_string());
                    out.push(p);
                }
                syn::UseTree::Rename(n) => {
                    let mut p = prefix.clone();
                    p.push(n.ident.to_string());
                    out.push(p);
                }
                syn::UseTree::Group(g) => {
                    for t in &g.items {
                        leaves(t, prefix, out)
                    }
                }
                syn::UseTree::Glob(_) => {}
            }
        }
        let mut references = vec![];
        leaves(&item.tree, &mut vec![], &mut references);
        for p in references {
            self.facts.references.push((self.owner.clone(), p));
        }
    }
    fn visit_local(&mut self, item: &'ast syn::Local) {
        if !excluded(&item.attrs) {
            visit::visit_local(self, item);
        }
    }
    fn visit_expr_block(&mut self, e: &'ast syn::ExprBlock) {
        if !excluded(&e.attrs) {
            visit::visit_expr_block(self, e);
        }
    }
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if excluded(&call.attrs) {
            return;
        }
        if let syn::Expr::Path(p) = call.func.as_ref() {
            if let Some(p) = path(&p.path) {
                self.facts.calls.push((self.owner.clone(), p.clone()));
                if p.len() >= 2 && p[p.len() - 2] == "SubrSpec" {
                    if let Some(syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(n),
                        ..
                    })) = call.args.first()
                    {
                        self.facts
                            .constructors
                            .push((self.owner.clone(), n.value()));
                    }
                }
            }
        }
        visit::visit_expr_call(self, call);
    }
    fn visit_expr_path(&mut self, e: &'ast syn::ExprPath) {
        if let Some(p) = path(&e.path) {
            self.facts.references.push((self.owner.clone(), p));
        }
        visit::visit_expr_path(self, e);
    }
    fn visit_expr_method_call(&mut self, e: &'ast syn::ExprMethodCall) {
        if excluded(&e.attrs) {
            return;
        }
        if e.method == "register_subr" {
            self.facts.registrations.push(if e.args.len() == 1 {
                expr_path(&e.args[0])
            } else {
                None
            });
        }
        visit::visit_expr_method_call(self, e);
    }
    fn visit_expr_macro(&mut self, e: &'ast syn::ExprMacro) {
        if !excluded(&e.attrs) {
            self.visit_macro(&e.mac);
        }
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if let Ok(expr) = syn::parse2::<syn::Expr>(mac.tokens.clone()) {
            self.visit_expr(&expr);
        } else if let Ok(file) = syn::parse2::<syn::File>(mac.tokens.clone()) {
            for item in &file.items {
                self.visit_item(item);
            }
        } else if let Ok(array) = syn::parse_str::<syn::Expr>(&format!("[{}]", mac.tokens)) {
            self.visit_expr(&array);
        } else {
            match syn::parse2::<MacroConstructors>(mac.tokens.clone()) {
                Ok(facts) => {
                    for name in facts.names {
                        self.facts.constructors.push((self.owner.clone(), name));
                    }
                    for call in facts.calls {
                        self.facts.calls.push((self.owner.clone(), call));
                    }
                    for reference in facts.references {
                        self.facts.references.push((self.owner.clone(), reference));
                    }
                }
                Err(e) => self.errors.push(format!("macro token scan: {e}")),
            }
        }
    }
}
// Literal tokens are opaque; this fallback never searches their text.
#[derive(Default)]
struct MacroConstructors {
    names: Vec<String>,
    calls: Vec<Vec<String>>,
    references: Vec<Vec<String>>,
}
impl Parse for MacroConstructors {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        use syn::parse::discouraged::Speculative;
        let mut facts = Self::default();
        while !input.is_empty() {
            if input.peek(syn::token::Paren) {
                let content;
                syn::parenthesized!(content in input);
                let nested = content.parse::<Self>()?;
                facts.names.extend(nested.names);
                facts.calls.extend(nested.calls);
                facts.references.extend(nested.references);
                continue;
            }
            if input.peek(syn::token::Brace) {
                let content;
                syn::braced!(content in input);
                let nested = content.parse::<Self>()?;
                facts.names.extend(nested.names);
                facts.calls.extend(nested.calls);
                facts.references.extend(nested.references);
                continue;
            }
            if input.peek(syn::token::Bracket) {
                let content;
                syn::bracketed!(content in input);
                let nested = content.parse::<Self>()?;
                facts.names.extend(nested.names);
                facts.calls.extend(nested.calls);
                facts.references.extend(nested.references);
                continue;
            }
            let fork = input.fork();
            if let Ok(p) = fork.call(syn::Path::parse_mod_style) {
                if let Some(p) = path(&p) {
                    facts.references.push(p.clone());
                    if fork.peek(syn::token::Paren) {
                        facts.calls.push(p.clone());
                        let look = fork.fork();
                        let content;
                        syn::parenthesized!(content in look);
                        if p.len() >= 2 && p[p.len() - 2] == "SubrSpec" {
                            if let Ok(args) =
                                content.parse_terminated(syn::Expr::parse, syn::Token![,])
                            {
                                if let Some(syn::Expr::Lit(syn::ExprLit {
                                    lit: syn::Lit::Str(n),
                                    ..
                                })) = args.first()
                                {
                                    facts.names.push(n.value());
                                }
                            }
                        }
                    }
                    input.advance_to(&fork);
                    continue;
                }
            }
            input.step(|cursor| {
                cursor
                    .token_tree()
                    .map(|(_, next)| ((), next))
                    .ok_or_else(|| cursor.error("expected a token"))
            })?;
        }
        Ok(facts)
    }
}

fn collect(items: &[syn::Item]) -> Result<Facts, String> {
    let mut c = Collector::new();
    for item in items {
        c.visit_item(item);
    }
    if !c.errors.is_empty() {
        return Err(fail(c.errors.join("; ")));
    }
    Ok(c.facts)
}
struct FlatModule {
    name: String,
    file: String,
    public_unconditional: bool,
    ast: syn::File,
}
fn flat_modules(root: &Path) -> Result<Vec<FlatModule>, String> {
    let declarations = read(root, &format!("{BUILTINS}/mod.rs"))?;
    let mut modules = vec![];
    for item in declarations.items {
        let syn::Item::Mod(m) = item else { continue };
        if excluded(&m.attrs) || m.content.is_some() {
            continue;
        }
        let file = format!("{BUILTINS}/{}.rs", m.ident);
        // Nested directory modules are outside this closed flat-provider grammar.
        if !root.join(&file).is_file() {
            continue;
        }
        let public_unconditional = matches!(m.vis, syn::Visibility::Public(_))
            && m.attrs.iter().all(|a| a.path().is_ident("doc"));
        modules.push(FlatModule {
            name: m.ident.to_string(),
            file: file.clone(),
            public_unconditional,
            ast: read(root, &file)?,
        });
    }
    Ok(modules)
}
fn production_sources(root: &Path) -> Result<Vec<String>, String> {
    fn walk(root: &Path, directory: &Path, out: &mut Vec<String>) -> Result<(), String> {
        for entry in std::fs::read_dir(directory)
            .map_err(|e| fail(format!("source directory {}: {e}", directory.display())))?
        {
            let entry = entry.map_err(|e| fail(e.to_string()))?;
            let p = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if p.is_dir() {
                if name != "tests" && name != "test_support" {
                    walk(root, &p, out)?;
                }
            } else if p.extension().is_some_and(|e| e == "rs")
                && name != "tests.rs"
                && !name.ends_with("_test.rs")
                && name != "test_utils.rs"
                && name != "test_support.rs"
                && name != "upper_subrs.generated.rs"
                && name != "upper_bootstrap_vars.generated.rs"
            {
                out.push(
                    p.strip_prefix(root)
                        .map_err(|e| fail(e.to_string()))?
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
        Ok(())
    }
    let directory = root.join("crates/neovm-eval/src");
    let mut out = vec![];
    walk(root, &directory, &mut out)?;
    out.sort();
    out.dedup();
    Ok(out)
}
fn factory_literal(
    root: &Path,
    home_file: &str,
    home_ast: &syn::File,
    function: &syn::ItemFn,
) -> Result<syn::Expr, String> {
    let s = &function.sig;
    if !matches!(function.vis, syn::Visibility::Inherited)
        || s.constness.is_none()
        || s.asyncness.is_some()
        || !matches!(s.safety, syn::Safety::Default)
        || s.abi.is_some()
        || !s.inputs.is_empty()
        || !s.generics.params.is_empty()
        || s.generics.where_clause.is_some()
        || s.variadic.is_some()
        || function.modifiers.defaultness.is_some()
        || function.attrs.iter().any(|a| !a.path().is_ident("doc"))
        || !matches!(&s.output, syn::ReturnType::Type(_, ty) if exact_spec_type(ty))
    {
        return Err(fail(
            "initializer factory must be private unconditional const fn () -> SubrSpec",
        ));
    }
    let [syn::Stmt::Expr(tail, None)] = function.block.stmts.as_slice() else {
        return Err(fail("initializer factory must contain one tail expression"));
    };
    super::initializer_world_literal(root, home_file, home_ast, tail)
}
struct Unit<'a> {
    functions: BTreeMap<String, &'a syn::ItemFn>,
    reachable: BTreeSet<String>,
    sink_families: BTreeSet<String>,
}
fn transparent(expr: &syn::Expr) -> Option<&syn::Expr> {
    match expr {
        syn::Expr::Paren(p) if p.attrs.is_empty() => transparent(&p.expr),
        syn::Expr::Group(g) if g.attrs.is_empty() => transparent(&g.expr),
        syn::Expr::Block(b) if b.attrs.is_empty() => match b.block.stmts.as_slice() {
            [syn::Stmt::Expr(e, None)] => transparent(e),
            _ => None,
        },
        _ => Some(expr),
    }
}
fn row_name(expr: &syn::Expr) -> bool {
    matches!(transparent(expr),Some(syn::Expr::Field(f)) if expr_path(&f.base)==Some(vec!["row".into()])
        && matches!(&f.member,syn::Member::Named(n) if n=="name"))
}
fn family_chain(
    expr: &syn::Expr,
    functions: &BTreeMap<String, &syn::ItemFn>,
) -> Option<Vec<String>> {
    match transparent(expr)? {
        syn::Expr::Call(c) if c.args.len() == 1 && row_name(&c.args[0]) => {
            let p = expr_path(&c.func)?;
            if p.len() == 1 && functions.contains_key(&p[0]) {
                Some(p)
            } else {
                None
            }
        }
        syn::Expr::MethodCall(m)
            if m.method == "or_else" && m.turbofish.is_none() && m.args.len() == 1 =>
        {
            let syn::Expr::Closure(c) = transparent(&m.args[0])? else {
                return None;
            };
            if !c.inputs.is_empty() || c.asyncness.is_some() {
                return None;
            }
            let mut names = family_chain(&m.receiver, functions)?;
            names.extend(family_chain(&c.body, functions)?);
            Some(names)
        }
        _ => None,
    }
}
struct Sinks<'a> {
    functions: &'a BTreeMap<String, &'a syn::ItemFn>,
    families: BTreeSet<String>,
}
impl<'ast> Visit<'ast> for Sinks<'_> {
    fn visit_expr_if(&mut self, e: &'ast syn::ExprIf) {
        if let syn::Expr::Let(binding) = e.cond.as_ref() {
            if let syn::Pat::TupleStruct(pat) = binding.pat.as_ref() {
                if pat.path.is_ident("Some") && pat.elems.len() == 1 {
                    if let syn::Pat::Ident(bound) = &pat.elems[0] {
                        if bound.by_ref.is_none()
                            && bound.mutability.is_none()
                            && bound.subpat.is_none()
                        {
                            if let Some(names) = family_chain(&binding.expr, self.functions) {
                                let id = bound.ident.to_string();
                                let mut sinks = 0;
                                let mut shadowed = false;
                                for stmt in &e.then_branch.stmts {
                                    match stmt {
                                        syn::Stmt::Local(local) if matches!(&local.pat,syn::Pat::Ident(p) if p.ident==id) => {
                                            shadowed = true
                                        }
                                        syn::Stmt::Expr(syn::Expr::Assign(a), _)
                                            if expr_path(&a.left) == Some(vec![id.clone()]) =>
                                        {
                                            shadowed = true
                                        }
                                        syn::Stmt::Expr(syn::Expr::MethodCall(call), _)
                                            if call.method == "register_subr"
                                                && expr_path(&call.receiver)
                                                    == Some(vec!["ctx".into()])
                                                && call.args.len() == 1
                                                && expr_path(&call.args[0])
                                                    == Some(vec![id.clone()]) =>
                                        {
                                            sinks += 1
                                        }
                                        _ => {}
                                    }
                                }
                                if sinks == 1 && !shadowed {
                                    self.families.extend(names);
                                }
                            }
                        }
                    }
                }
            }
        }
        visit::visit_expr_if(self, e);
    }
}
fn unit_registrar(ast: &syn::File) -> Result<Unit<'_>, String> {
    let modules: Vec<_> = ast
        .items
        .iter()
        .filter_map(|i| match i {
            syn::Item::Mod(m) if m.ident == "unit_runtime" => Some(m),
            _ => None,
        })
        .collect();
    let [module] = modules.as_slice() else {
        return Err(fail("one actual unit_runtime module required"));
    };
    let (_, items) = module
        .content
        .as_ref()
        .ok_or_else(|| fail("unit_runtime must be inline"))?;
    let functions: BTreeMap<_, _> = items
        .iter()
        .filter_map(|i| match i {
            syn::Item::Fn(f) if !excluded(&f.attrs) => Some((f.sig.ident.to_string(), f)),
            _ => None,
        })
        .collect();
    let root = "register_core_subrs";
    if !functions.contains_key(root) {
        return Err(fail("actual unit registration root missing"));
    }
    // SET is the actual hook-set constructor; do not accept a dead family helper.
    let sets: Vec<_> = items
        .iter()
        .filter_map(|i| match i {
            syn::Item::Static(s) if s.ident == "SET" => Some(s),
            _ => None,
        })
        .collect();
    let [set] = sets.as_slice() else {
        return Err(fail("one actual unit hook SET required"));
    };
    let syn::Expr::Call(call) = set.expr.as_ref() else {
        return Err(fail("unit SET constructor missing"));
    };
    if !matches!(call.func.as_ref(), syn::Expr::Path(p) if path(&p.path)==Some(vec!["EvalHookSet".into(),"new".into()]))
        || call
            .args
            .iter()
            .filter(|e| expr_path(e) == Some(vec![root.into()]))
            .count()
            != 1
    {
        return Err(fail("unit SET must bind the actual registration root once"));
    }
    struct InstalledSet(usize);
    impl<'ast> Visit<'ast> for InstalledSet {
        fn visit_expr_call(&mut self, e: &'ast syn::ExprCall) {
            if matches!(e.func.as_ref(), syn::Expr::Path(p) if path(&p.path)==Some(vec!["install_eval_hooks".into()]))
                && e.args.len() == 1
                && matches!(&e.args[0], syn::Expr::Reference(r) if expr_path(&r.expr)==Some(vec!["SET".into()]))
            {
                self.0 += 1;
            }
            visit::visit_expr_call(self, e);
        }
    }
    let entry = functions
        .get("unit_runtime")
        .ok_or_else(|| fail("actual unit runtime entry missing"))?;
    let mut installed = InstalledSet(0);
    installed.visit_block(&entry.block);
    if installed.0 != 1 {
        return Err(fail(
            "actual unit runtime must publish the guarded SET once",
        ));
    }
    let mut reachable = BTreeSet::new();
    let mut pending = vec![root.to_owned()];
    while let Some(name) = pending.pop() {
        if !reachable.insert(name.clone()) {
            continue;
        }
        let mut c = Collector::new();
        c.visit_block(&functions[&name].block);
        for (_, p) in c.facts.calls {
            if p.len() == 1 && functions.contains_key(&p[0]) {
                pending.push(p[0].clone());
            }
        }
    }
    let mut sinks = Sinks {
        functions: &functions,
        families: BTreeSet::new(),
    };
    sinks.visit_block(&functions[root].block);
    let sink_families = sinks.families;
    if sink_families.is_empty() {
        return Err(fail(
            "actual unit registration requires an if-let Some chain sent to ctx.register_subr(bound) in the same block",
        ));
    }
    Ok(Unit {
        functions,
        reachable,
        sink_families,
    })
}
struct Arms<'a> {
    selector: &'a str,
    name: &'a str,
    routes: Vec<Option<Vec<String>>>,
}
impl<'ast> Visit<'ast> for Arms<'_> {
    fn visit_expr_match(&mut self, e: &'ast syn::ExprMatch) {
        if expr_path(&e.expr) == Some(vec![self.selector.to_owned()]) {
            for arm in &e.arms {
                // Syn 3 represents a match guard inside Pat::Guard, not Arm.
                let (pattern, guarded) = match &arm.pat {
                    syn::Pat::Guard(guard) => (guard.pat.as_ref(), true),
                    pattern => (pattern, false),
                };
                if matches!(pattern, syn::Pat::Lit(p) if matches!(&p.lit, syn::Lit::Str(s) if s.value()==self.name))
                {
                    self.routes
                        .push(if guarded { None } else { expr_path(&arm.body) });
                }
            }
        }
        visit::visit_expr_match(self, e);
    }
}

/// Called by canonical_items(), before generating/checking canonical snapshots.
pub(super) fn validate_static_subr_sources(
    root: &Path,
    homes: &[StaticHome],
) -> Result<(), String> {
    let modules = flat_modules(root)?;
    // Discovery is independent of Core expressions, so replacing a Core static
    // copy with a literal/factory cannot make the guarded home disappear.
    let mut declarations = vec![];
    for module in &modules {
        for item in &module.ast.items {
            if let syn::Item::Static(s) = item {
                if !excluded(&s.attrs) && spec_type(&s.ty) {
                    declarations.push((module, s));
                }
            }
        }
    }
    if declarations.is_empty() && homes.is_empty() {
        return Ok(());
    }
    let mut keys = BTreeSet::new();
    let mut names = BTreeSet::new();
    for home in homes {
        if !keys.insert((home.file.as_str(), home.static_ident.as_str()))
            || !names.insert(home.name.as_str())
        {
            return Err(fail("duplicate static home/Core reference"));
        }
    }
    if declarations.len() != homes.len() {
        return Err(fail("orphan, duplicate or unreferenced descriptor static"));
    }
    for (module, declaration) in &declarations {
        if homes
            .iter()
            .filter(|h| h.file == module.file && h.static_ident == declaration.ident.to_string())
            .count()
            != 1
        {
            return Err(fail(format!(
                "orphan descriptor static {}::{}",
                module.name, declaration.ident
            )));
        }
    }
    let core = collect(&read(root, CORE)?.items)?;
    let unit_ast = read(root, UNIT)?;
    let unit = unit_registrar(&unit_ast)?;
    let mut corpus: Vec<(String, Facts)> = production_sources(root)?
        .into_iter()
        .map(|file| collect(&read(root, &file)?.items).map(|facts| (file, facts)))
        .collect::<Result<_, _>>()?;
    corpus.push((CORE.into(), collect(&read(root, CORE)?.items)?));
    let mut unit_facts = Collector::new();
    for name in &unit.reachable {
        unit_facts.visit_item_fn(unit.functions[name]);
    }
    corpus.push((UNIT.into(), unit_facts.facts));
    for home in homes {
        let matches: Vec<_> = declarations
            .iter()
            .filter(|(m, s)| m.file == home.file && s.ident == home.static_ident)
            .collect();
        let [(module, declaration)] = matches.as_slice() else {
            return Err(fail("static home declaration is not unique"));
        };
        if module.name != home.module
            || !module.public_unconditional
            || !matches!(declaration.vis, syn::Visibility::Public(_))
            || !matches!(declaration.mutability, syn::StaticMutability::None)
            || !exact_spec_type(&declaration.ty)
            || declaration.attrs.iter().any(|a| !a.path().is_ident("doc"))
        {
            return Err(fail(
                "home must be an unconditional public immutable SubrSpec static in a public flat module",
            ));
        }
        let (literal, allowed_owner) = if let Some(factory) = &home.initializer_factory {
            let syn::Expr::Call(call) = declaration.expr.as_ref() else {
                return Err(fail("static initializer factory call missing"));
            };
            if !call.args.is_empty() || expr_path(&call.func) != Some(vec![factory.clone()]) {
                return Err(fail(
                    "static must call its same-file private factory exactly once",
                ));
            }
            let functions: Vec<_> = module
                .ast
                .items
                .iter()
                .filter_map(|i| match i {
                    syn::Item::Fn(f) if f.sig.ident == factory => Some(f),
                    _ => None,
                })
                .collect();
            let [function] = functions.as_slice() else {
                return Err(fail("initializer factory must be unique and same-file"));
            };
            // No alias resolver: uncertain same-basename references fail closed.
            // Function definitions are not expression/use references.
            let uses: Vec<_> = corpus
                .iter()
                .flat_map(|(file, f)| {
                    f.references.iter().filter_map(move |(owner, p)| {
                        if p.last() == Some(factory) {
                            Some((file, owner, p))
                        } else {
                            None
                        }
                    })
                })
                .collect();
            if uses.len() != 1
                || uses[0].0 != &home.file
                || uses[0].1 != &Owner::Static(home.static_ident.clone())
                || uses[0].2 != &vec![factory.clone()]
            {
                return Err(fail(
                    "private initializer factory is used outside its one static initializer",
                ));
            }
            (
                factory_literal(root, &home.file, &module.ast, function)?,
                Owner::Function(factory.clone()),
            )
        } else {
            (
                super::initializer_world_literal(root, &home.file, &module.ast, &declaration.expr)?,
                Owner::Static(home.static_ident.clone()),
            )
        };
        let parsed = super::literal_spec(&literal, "static source guard")?;
        if (
            parsed.name.as_str(),
            parsed.min,
            parsed.max,
            parsed.cfg.as_str(),
        ) != (home.name.as_str(), home.min, home.max, home.cfg.as_str())
        {
            return Err(fail(
                "static descriptor metadata differs from parsed Core home",
            ));
        }
        let sites: Vec<_> = corpus
            .iter()
            .flat_map(|(file, f)| {
                f.constructors
                    .iter()
                    .filter(|(_, name)| name == &home.name)
                    .map(move |(owner, _)| (file, owner))
            })
            .collect();
        if sites.len() != 1 || sites[0].0 != &home.file || sites[0].1 != &allowed_owner {
            return Err(fail(format!(
                "{} has a duplicate/copied descriptor literal or wrong literal owner",
                home.name
            )));
        }
        let expected_core = expected_path(home, "neovm_eval");
        if core
            .registrations
            .iter()
            .filter(|p| p.as_ref() == Some(&expected_core))
            .count()
            != 1
        {
            return Err(fail(
                "Core must copy its unique canonical static exactly once",
            ));
        }
        let mut routes = vec![];
        for name in &unit.sink_families {
            let f = unit.functions[name];
            if f.sig.inputs.len() != 1 {
                continue;
            }
            let Some(syn::FnArg::Typed(arg)) = f.sig.inputs.first() else {
                continue;
            };
            let syn::Pat::Ident(selector) = arg.pat.as_ref() else {
                continue;
            };
            let selector_name = selector.ident.to_string();
            let mut arms = Arms {
                selector: &selector_name,
                name: &home.name,
                routes: vec![],
            };
            arms.visit_block(&f.block);
            routes.extend(arms.routes);
        }
        if routes != vec![Some(expected_path(home, "crate"))] {
            return Err(fail(
                "actual unit registrar must copy the SAME static in exactly one name arm",
            ));
        }
        // A second static/factory alias would recreate a descriptor home even
        // without another spelling of the descriptor's name.
        for (file, facts) in &corpus {
            for (owner, p) in &facts.references {
                let reference = p == &expected_path(home, "crate")
                    || p == &expected_path(home, "neovm_eval")
                    || (file == &home.file && p == &vec![home.static_ident.clone()]);
                if reference {
                    if let Owner::Function(name) = owner {
                        if let Some(module) = modules.iter().find(|m| &m.file == file) {
                            if module.ast.items.iter().any(|i| matches!(i,
                                syn::Item::Fn(f) if f.sig.ident==name &&
                                matches!(&f.sig.output,syn::ReturnType::Type(_,ty) if exact_spec_type(ty)))) {
                                return Err(fail("descriptor static factory alias is forbidden"));
                            }
                        }
                    }
                }
                if reference && matches!(owner, Owner::Static(_)) {
                    return Err(fail("descriptor static alias/duplicate home is forbidden"));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const MODULE: &str = r#"
        pub static SAMPLE_SUBR: SubrSpec = crate::emacs_core::subr::world_subr!(
            SubrSpec::new("sample-op", NativeFn::ContextVec(entry), SubrArity::new(1, Some(1))));
    "#;
    const CORE_SOURCE: &str = r#"
        fn register_subrs(ctx: &mut Context) {
            ctx.register_subr(neovm_eval::emacs_core::builtins::misc_pure::SAMPLE_SUBR);
        }
    "#;
    const UNIT_SOURCE: &str = r#"
        #[cfg(test)] mod unit_runtime {
            static SET: EvalHookSet = EvalHookSet::new(register_core_subrs);
            fn unit_runtime() { install_eval_hooks(&SET); }
            fn register_core_subrs(ctx: &mut Context) {
                if let Some(spec) = misc_family_subr(row.name) { ctx.register_subr(spec); }
            }
            fn misc_family_subr(name: &str) -> Option<SubrSpec> {
                Some(match name {
                    "sample-op" => crate::emacs_core::builtins::misc_pure::SAMPLE_SUBR,
                    _ => return None,
                })
            }
        }
    "#;
    fn home(factory: Option<&str>) -> StaticHome {
        StaticHome {
            module: "misc_pure".into(),
            static_ident: "SAMPLE_SUBR".into(),
            file: format!("{BUILTINS}/misc_pure.rs"),
            initializer_factory: factory.map(str::to_owned),
            name: "sample-op".into(),
            min: 1,
            max: Some(1),
            cfg: "always".into(),
        }
    }
    fn fixture(module: &str, core: &str, unit: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (file, source) in [
            (format!("{BUILTINS}/mod.rs"), "pub mod misc_pure;"),
            (format!("{BUILTINS}/misc_pure.rs"), module),
            (CORE.into(), core),
            (UNIT.into(), unit),
        ] {
            let target = dir.path().join(file);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(target, source).unwrap();
        }
        dir
    }
    #[test]
    fn static_guard_accepts_metadata_chains_only_at_the_literal_home() {
        for chain in [
            ".requires_eval_state()",
            ".requires_eval_state().interactive(crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::String(\"p\",))",
            ".placeholder(crate::emacs_core::subr::NoEvalPlaceholder::Nil)",
            ".interactive(crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec::Form(crate::emacs_core::builtins::misc_pure::interactive_form))",
            ".leaf(&crate::emacs_core::builtins::leaves::SAMPLE_LEAF)",
        ] {
            let literal = MODULE.replace("Some(1))));", &format!("Some(1)))){chain};"));
            let form = "\nfn interactive_form() -> neovm_value::value::Value { opaque_form() }";
            let dir = fixture(&format!("{literal}{form}"), CORE_SOURCE, UNIT_SOURCE);
            for (relative, source) in [
                ("crates/neovm-eval/src/lib.rs", "pub mod emacs_core;"),
                (
                    "crates/neovm-eval/src/emacs_core/mod.rs",
                    r#"#[path="runtime/eval/mod.rs"] pub mod eval; #[path="lisp/native/subr/mod.rs"] pub mod subr; #[path="lisp/native/builtins/mod.rs"] pub mod builtins;"#,
                ),
                (
                    "crates/neovm-eval/src/emacs_core/runtime/eval/mod.rs",
                    "pub mod interactive_registry;",
                ),
                (
                    "crates/neovm-eval/src/emacs_core/runtime/eval/interactive_registry.rs",
                    "use neovm_value::value::Value; pub enum BuiltinInteractiveSpec { String(&'static str), Form(fn() -> Value) }",
                ),
                (
                    "crates/neovm-eval/src/emacs_core/lisp/native/subr/mod.rs",
                    "pub enum NoEvalPlaceholder { Nil, FixnumZero, WindowLineHeight } pub(crate) mod leaf;",
                ),
                (
                    "crates/neovm-eval/src/emacs_core/lisp/native/subr/leaf.rs",
                    "pub struct LeafSpec;",
                ),
                (
                    "crates/neovm-eval/src/emacs_core/lisp/native/builtins/mod.rs",
                    "pub mod misc_pure; pub mod leaves;",
                ),
                (
                    "crates/neovm-eval/src/emacs_core/lisp/native/builtins/leaves/mod.rs",
                    "use crate::emacs_core::subr::leaf::LeafSpec; pub static SAMPLE_LEAF: LeafSpec = LeafSpec::new(opaque_effects());",
                ),
            ] {
                let path = dir.path().join(relative);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, source).unwrap();
            }
            assert!(validate_static_subr_sources(dir.path(), &[home(None)]).is_ok());
            let private = format!(
                "pub static SAMPLE_SUBR: SubrSpec = make_spec(); const fn make_spec() -> SubrSpec {{ {} }}",
                literal
                    .split_once(" = ")
                    .unwrap()
                    .1
                    .trim()
                    .trim_end_matches(';')
            );
            std::fs::write(
                dir.path().join(format!("{BUILTINS}/misc_pure.rs")),
                format!("{private}{form}"),
            )
            .unwrap();
            assert!(validate_static_subr_sources(dir.path(), &[home(Some("make_spec"))]).is_ok());
        }
    }

    #[test]
    fn static_guard_rejects_metadata_recomputation_and_copy_builders() {
        for chain in [
            ".unknown_metadata()",
            ".requires_eval_state(1)",
            ".requires_eval_state::<u8>()",
            ".interactive(build_interactive())",
            ".placeholder(make_placeholder())",
        ] {
            let module = MODULE.replace("Some(1))));", &format!("Some(1)))){chain};"));
            let dir = fixture(&module, CORE_SOURCE, UNIT_SOURCE);
            assert!(
                validate_static_subr_sources(dir.path(), &[home(None)]).is_err(),
                "accepted {chain}"
            );
            let private = format!(
                "pub static SAMPLE_SUBR: SubrSpec = make_spec(); const fn make_spec() -> SubrSpec {{ {} }}",
                module
                    .split_once(" = ")
                    .unwrap()
                    .1
                    .trim()
                    .trim_end_matches(';')
            );
            std::fs::write(dir.path().join(format!("{BUILTINS}/misc_pure.rs")), private).unwrap();
            assert!(
                validate_static_subr_sources(dir.path(), &[home(Some("make_spec"))]).is_err(),
                "accepted private {chain}"
            );
        }
        let module = MODULE.replace("Some(1))));", "Some(1)))).requires_eval_state();");
        let core = CORE_SOURCE.replace("::SAMPLE_SUBR)", "::SAMPLE_SUBR.requires_eval_state())");
        let dir = fixture(&module, &core, UNIT_SOURCE);
        assert!(validate_static_subr_sources(dir.path(), &[home(None)]).is_err());
        let unit = UNIT_SOURCE.replace("::SAMPLE_SUBR,", "::SAMPLE_SUBR.requires_eval_state(),");
        let dir = fixture(&module, CORE_SOURCE, &unit);
        assert!(validate_static_subr_sources(dir.path(), &[home(None)]).is_err());
        let duplicate = format!("{module}\n{}", module.replace("SAMPLE_SUBR", "SECOND_SUBR"));
        let dir = fixture(&duplicate, CORE_SOURCE, UNIT_SOURCE);
        assert!(validate_static_subr_sources(dir.path(), &[home(None)]).is_err());
    }

    #[test]
    fn static_guard_accepts_literal_home_and_transparent_unit_blocks() {
        let dir = fixture(
            MODULE,
            CORE_SOURCE,
            &UNIT_SOURCE.replace(
                "\"sample-op\" => crate::emacs_core::builtins::misc_pure::SAMPLE_SUBR,",
                "\"sample-op\" => { (crate::emacs_core::builtins::misc_pure::SAMPLE_SUBR) },",
            ),
        );
        assert!(validate_static_subr_sources(dir.path(), &[home(None)]).is_ok());
    }
    #[test]
    fn static_guard_accepts_private_const_initializer_used_only_there() {
        let module = r#"
            pub static SAMPLE_SUBR: SubrSpec = make_spec();
            const fn make_spec() -> SubrSpec { crate::emacs_core::subr::world_subr!(
                SubrSpec::new("sample-op", NativeFn::ContextVec(entry),SubrArity::new(1,Some(1)))) }
        "#;
        let dir = fixture(module, CORE_SOURCE, UNIT_SOURCE);
        assert!(validate_static_subr_sources(dir.path(), &[home(Some("make_spec"))]).is_ok());
    }
    #[test]
    fn static_guard_rejects_core_literal_and_factory_zero_home_bypasses() {
        for core in [CORE_SOURCE.replace("neovm_eval::emacs_core::builtins::misc_pure::SAMPLE_SUBR",
            "crate::emacs_core::subr::world_subr!(SubrSpec::new(\"sample-op\",NativeFn::ContextVec(entry),SubrArity::new(1,Some(1))))"),
            CORE_SOURCE.replace("neovm_eval::emacs_core::builtins::misc_pure::SAMPLE_SUBR",
                "neovm_eval::emacs_core::builtins::misc_pure::public_factory()")]
        {
            let dir=fixture(MODULE,&core,UNIT_SOURCE);
            assert!(validate_static_subr_sources(dir.path(),&[]).is_err());
        }
    }
    #[test]
    fn static_guard_rejects_duplicate_literal_home_and_core_reference() {
        for module in [
            format!(
                "{MODULE}\n pub static OTHER: SubrSpec = crate::emacs_core::subr::world_subr!(SubrSpec::new(\"sample-op\", NativeFn::ContextVec(entry), SubrArity::new(1,Some(1))));"
            ),
            format!(
                "{MODULE}\n fn duplicate() -> SubrSpec {{ SubrSpec::new(\"sample-op\",NativeFn::ContextVec(entry),SubrArity::new(1,Some(1))) }}"
            ),
            format!("{MODULE}\n pub static OTHER: SubrSpec = SAMPLE_SUBR;"),
        ] {
            let dir = fixture(&module, CORE_SOURCE, UNIT_SOURCE);
            assert!(validate_static_subr_sources(dir.path(), &[home(None)]).is_err());
        }
        let dir=fixture(MODULE,&CORE_SOURCE.replace("ctx.register_subr(","ctx.register_subr(neovm_eval::emacs_core::builtins::misc_pure::SAMPLE_SUBR); ctx.register_subr("),UNIT_SOURCE);
        assert!(validate_static_subr_sources(dir.path(), &[home(None), home(None)]).is_err());
    }
    #[test]
    fn static_guard_rejects_unit_copies_factory_calls_and_wrong_static() {
        for arm in [
            "SubrSpec::new(\"sample-op\",NativeFn::ContextVec(entry),SubrArity::new(1,Some(1)))",
            "crate::emacs_core::builtins::misc_pure::public_factory()",
            "crate::emacs_core::builtins::misc_pure::WRONG_SUBR",
            "{ let copy=crate::emacs_core::builtins::misc_pure::SAMPLE_SUBR; copy }",
        ] {
            let unit =
                UNIT_SOURCE.replace("crate::emacs_core::builtins::misc_pure::SAMPLE_SUBR", arm);
            let dir = fixture(MODULE, CORE_SOURCE, &unit);
            assert!(validate_static_subr_sources(dir.path(), &[home(None)]).is_err());
        }
    }
    #[test]
    fn static_guard_rejects_public_or_reused_initializer_factory() {
        let private = r#"pub static SAMPLE_SUBR: SubrSpec=make_spec();
            const fn make_spec()->SubrSpec { crate::emacs_core::subr::world_subr!(
                SubrSpec::new("sample-op",NativeFn::ContextVec(entry),SubrArity::new(1,Some(1)))) }"#;
        for module in [
            private.replace("const fn", "pub const fn"),
            format!("{private}\n fn misuse() {{ let _ = make_spec(); }}"),
            format!("{private}\n fn misuse() {{ let _ = vec![make_spec(); 2]; }}"),
            format!("{private}\n fn misuse() {{ let f=make_spec; let _=f(); }}"),
            format!("{private}\n fn misuse() {{ let _=self::make_spec(); }}"),
            format!("{private}\n mod child {{ fn misuse() {{ super::make_spec(); }} }}"),
            format!("{private}\n fn misuse() {{ use self::make_spec as f; let _=f(); }}"),
        ] {
            let dir = fixture(&module, CORE_SOURCE, UNIT_SOURCE);
            assert!(validate_static_subr_sources(dir.path(), &[home(Some("make_spec"))]).is_err());
        }
    }
    #[test]
    fn static_guard_ignores_fixture_literals_strings_and_comments() {
        let module = format!(
            r##"{MODULE}
            // SubrSpec::new("sample-op", fake, fake)
            const SOURCE: &str = "SubrSpec::new(\"sample-op\", fake, fake)";
            #[cfg(test)] mod fixtures {{ fn fake() {{ SubrSpec::new("sample-op",fake,fake); }} }}
            #[cfg(feature="test-support")] fn fixture() {{ SubrSpec::new("sample-op",fake,fake); }}
            fn text() {{ format!("SubrSpec::new(\"sample-op\", fake, fake)"); }}
        "##
        );
        let dir = fixture(&module, CORE_SOURCE, UNIT_SOURCE);
        assert!(validate_static_subr_sources(dir.path(), &[home(None)]).is_ok());
    }
    #[test]
    fn static_guard_rejects_dead_unit_arm_and_wrong_published_set() {
        for unit in [
            UNIT_SOURCE.replace("\"sample-op\" =>", "\"sample-op\" if allowed =>"),
            UNIT_SOURCE.replace("ctx.register_subr(spec);", ""),
            UNIT_SOURCE.replace(
                "if let Some(spec) = misc_family_subr(row.name) { ctx.register_subr(spec); }",
                "misc_family_subr(row.name);",
            ),
            UNIT_SOURCE.replace("ctx.register_subr(spec);", "ctx.register_subr(other);"),
            UNIT_SOURCE.replace(
                "ctx.register_subr(spec);",
                "let spec=other; ctx.register_subr(spec);",
            ),
            UNIT_SOURCE.replace("install_eval_hooks(&SET)", "install_eval_hooks(&OTHER_SET)"),
        ] {
            let dir = fixture(MODULE, CORE_SOURCE, &unit);
            assert!(validate_static_subr_sources(dir.path(), &[home(None)]).is_err());
        }
    }
    #[test]
    fn static_guard_requires_actual_or_else_sink_chain_and_covers_other_production_files() {
        let unit = UNIT_SOURCE
            .replace(
                "misc_family_subr(row.name)",
                "empty_family(row.name).or_else(|| misc_family_subr(row.name))",
            )
            .replace(
                "fn misc_family_subr",
                "fn empty_family(name:&str)->Option<SubrSpec>{None} fn misc_family_subr",
            );
        let dir = fixture(MODULE, CORE_SOURCE, &unit);
        assert!(validate_static_subr_sources(dir.path(), &[home(None)]).is_ok());
        for file in [
            format!("{BUILTINS}/mod.rs"),
            "crates/neovm-eval/src/emacs_core/runtime/rogue.rs".into(),
        ] {
            let dir = fixture(MODULE, CORE_SOURCE, UNIT_SOURCE);
            let target = dir.path().join(file);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            let prior = std::fs::read_to_string(&target).unwrap_or_default();
            std::fs::write(target,format!("{prior} fn duplicate()->SubrSpec {{SubrSpec::new(\"sample-op\",NativeFn::ContextVec(entry),SubrArity::new(1,Some(1)))}}")).unwrap();
            assert!(validate_static_subr_sources(dir.path(), &[home(None)]).is_err());
        }
    }
}

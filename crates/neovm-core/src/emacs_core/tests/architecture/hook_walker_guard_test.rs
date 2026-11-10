//! The hook-walk token has one constructor. Direct fileio callback
//! collectors stay outside that entry.
//!
//! GNU checks `Vrun_hooks` once at `run_hook_with_args` (`eval.c:3004-3006`).
//! `fileio.c:5243` and `:5890` call insert and write-region annotation
//! callbacks themselves, so those collectors are classified direct calls.
//! `fileio.c:6370` is another direct `!NILP (Vrun_hooks)` site before a
//! directory is created. It is not a hook walk.

use syn::visit::Visit;

use super::architecture_production_source_roots::production_source_files;

fn source_text(relative_suffix: &str) -> String {
    let file = production_source_files()
        .into_iter()
        .find(|file| file.relative.ends_with(relative_suffix))
        .unwrap_or_else(|| panic!("missing production source {relative_suffix}"));
    std::fs::read_to_string(&file.path).unwrap_or_else(|error| {
        panic!("read {}: {error}", file.path.display());
    })
}

fn function_body<'a>(source: &'a str, name: &str) -> &'a str {
    let marker = format!("fn {name}");
    let start = source
        .find(&marker)
        .unwrap_or_else(|| panic!("{name} is not in the scanned source"));
    let rest = &source[start..];
    let next = rest
        .lines()
        .skip(1)
        .position(|line| {
            line.starts_with("fn ")
                || line.starts_with("pub fn ")
                || line.starts_with("pub(crate) fn ")
        })
        .map(|lines| {
            rest.lines()
                .take(lines + 1)
                .map(|line| line.len() + 1)
                .sum()
        })
        .unwrap_or(rest.len());
    &rest[..next]
}

struct ConstructorFinder {
    function: Vec<String>,
    sites: Vec<String>,
}

impl<'ast> Visit<'ast> for ConstructorFinder {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.function.push(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
        self.function.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.function.push(node.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, node);
        self.function.pop();
    }

    fn visit_expr_struct(&mut self, node: &'ast syn::ExprStruct) {
        let name = node
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string());
        if name.as_deref() == Some("HookWalkAuthorization") {
            let owner = self
                .function
                .last()
                .cloned()
                .unwrap_or_else(|| "<outside-function>".to_string());
            self.sites.push(owner);
        }
        syn::visit::visit_expr_struct(self, node);
    }
}

fn parsed_file(relative_suffix: &str) -> syn::File {
    let text = source_text(relative_suffix);
    syn::parse_file(&text).unwrap_or_else(|error| {
        panic!("parse {relative_suffix}: {error}");
    })
}

fn function_idents(items: &[syn::Item], name: &str) -> Vec<String> {
    fn find<'a>(items: &'a [syn::Item], name: &str) -> Option<&'a syn::Block> {
        for item in items {
            match item {
                syn::Item::Fn(item_fn) if item_fn.sig.ident == name => {
                    return Some(&item_fn.block);
                }
                syn::Item::Mod(item_mod) => {
                    if let Some((_, nested)) = &item_mod.content {
                        if let Some(found) = find(nested, name) {
                            return Some(found);
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }
    struct Idents(Vec<String>);
    impl<'ast> Visit<'ast> for Idents {
        fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
            if let Some(segment) = node.path.segments.last() {
                self.0.push(segment.ident.to_string());
            }
            syn::visit::visit_expr_path(self, node);
        }

        fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
            self.0.push(node.method.to_string());
            syn::visit::visit_expr_method_call(self, node);
        }
    }
    let block = find(items, name).unwrap_or_else(|| panic!("{name} is not in the parsed file"));
    let mut idents = Idents(Vec::new());
    idents.visit_block(block);
    idents.0
}

#[test]
fn hook_walk_authorization_constructor_has_one_site() {
    let mut sites = Vec::new();
    for file in production_source_files() {
        let text = std::fs::read_to_string(&file.path).unwrap_or_else(|error| {
            panic!("read {}: {error}", file.path.display());
        });
        let parsed = syn::parse_file(&text).unwrap_or_else(|error| {
            panic!("parse {}: {error}", file.relative);
        });
        let mut finder = ConstructorFinder {
            function: Vec::new(),
            sites: Vec::new(),
        };
        finder.visit_file(&parsed);
        for owner in finder.sites {
            sites.push(format!("{} in {}", file.relative, owner));
        }
    }
    assert_eq!(
        sites.len(),
        1,
        "the walk token is minted in with_hook_walk only, found {sites:?}"
    );
    assert!(
        sites[0].contains("lisp/hook_runtime/mod.rs") && sites[0].ends_with("in with_hook_walk"),
        "constructor site {}",
        sites[0]
    );
}

#[test]
fn window_walkers_enter_through_the_live_gate() {
    let parsed = parsed_file("commands/window_hooks/redisplay_gnu.rs");
    for name in ["walk", "run_funs"] {
        let idents = function_idents(&parsed.items, name);
        assert!(
            idents.iter().any(|ident| ident == "with_live_hook_walk"),
            "{name} must acquire the gate at entry"
        );
        assert!(
            idents.iter().any(|ident| ident == "cons_cdr"),
            "{name} must keep reading the live cdr"
        );
        assert!(
            idents
                .iter()
                .all(|ident| ident != "run_hook_value" && ident != "safe_run_hook_value"),
            "{name} must not delegate to a snapshot walker"
        );
    }
    let gate = parsed_file("lisp/hook_runtime/mod.rs");
    let entry = function_idents(&gate.items, "with_live_hook_walk");
    assert!(
        entry.iter().any(|ident| ident == "with_hook_walk"),
        "with_live_hook_walk is a gated entry and mints no token of its own"
    );
}

fn signature_mentions_walk_token(
    inputs: &syn::punctuated::Punctuated<syn::FnArg, syn::Token![,]>,
) -> bool {
    fn type_mentions(ty: &syn::Type) -> bool {
        match ty {
            syn::Type::Reference(reference) => type_mentions(&reference.elem),
            syn::Type::Path(path) => path
                .path
                .segments
                .iter()
                .any(|segment| segment.ident == "HookWalkAuthorization"),
            syn::Type::Paren(paren) => type_mentions(&paren.elem),
            syn::Type::Group(group) => type_mentions(&group.elem),
            _ => false,
        }
    }
    inputs.iter().any(|arg| match arg {
        syn::FnArg::Typed(pat) => type_mentions(&pat.ty),
        syn::FnArg::Receiver(_) => false,
    })
}

struct CallableDefinitions {
    file: String,
    count: usize,
    missing: Vec<String>,
}

impl<'ast> Visit<'ast> for CallableDefinitions {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.note(&node.sig);
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.note(&node.sig);
        syn::visit::visit_impl_item_fn(self, node);
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        self.note(&node.sig);
        syn::visit::visit_trait_item_fn(self, node);
    }
}

impl CallableDefinitions {
    fn note(&mut self, sig: &syn::Signature) {
        if sig.ident != "call_hook_callable" {
            return;
        }
        self.count += 1;
        if !signature_mentions_walk_token(&sig.inputs) {
            self.missing.push(self.file.clone());
        }
    }
}

fn expr_is_walk_token(expr: &syn::Expr) -> bool {
    match expr {
        syn::Expr::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "authorization"),
        syn::Expr::Reference(reference) => expr_is_walk_token(&reference.expr),
        syn::Expr::Paren(paren) => expr_is_walk_token(&paren.expr),
        syn::Expr::Group(group) => expr_is_walk_token(&group.expr),
        _ => false,
    }
}

struct CallableCalls {
    file: String,
    count: usize,
    missing: Vec<String>,
}

impl CallableCalls {
    fn note(&mut self, args: &syn::punctuated::Punctuated<syn::Expr, syn::Token![,]>) {
        self.count += 1;
        if !args.iter().any(expr_is_walk_token) {
            self.missing.push(self.file.clone());
        }
    }
}

impl<'ast> Visit<'ast> for CallableCalls {
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if node.method == "call_hook_callable" {
            self.note(&node.args);
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        let named = match &*node.func {
            syn::Expr::Path(path) => path
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "call_hook_callable"),
            _ => false,
        };
        if named {
            self.note(&node.args);
        }
        syn::visit::visit_expr_call(self, node);
    }
}

#[test]
fn hook_callable_invocation_takes_the_walk_token() {
    let mut definitions = CallableDefinitions {
        file: String::new(),
        count: 0,
        missing: Vec::new(),
    };
    let mut calls = CallableCalls {
        file: String::new(),
        count: 0,
        missing: Vec::new(),
    };
    for file in production_source_files() {
        // This test's own source names the ident. It is a test path, so the
        // visitors never see it. Definitions and calls are syn items only.
        if file.is_test_path() {
            continue;
        }
        let text = std::fs::read_to_string(&file.path).unwrap_or_else(|error| {
            panic!("read {}: {error}", file.path.display());
        });
        let parsed = syn::parse_file(&text).unwrap_or_else(|error| {
            panic!("parse {}: {error}", file.relative);
        });
        definitions.file = file.relative.clone();
        definitions.visit_file(&parsed);
        calls.file = file.relative.clone();
        calls.visit_file(&parsed);
    }
    assert!(
        definitions.missing.is_empty(),
        "definition lacks the token: {:?}",
        definitions.missing
    );
    assert!(
        definitions.count >= 2,
        "expected the Context and Vm impls, found {}",
        definitions.count
    );
    assert!(
        calls.missing.is_empty(),
        "call does not pass the token: {:?}",
        calls.missing
    );
    assert!(
        calls.count >= 5,
        "expected the walker calls, found {}",
        calls.count
    );
}

#[test]
fn fileio_callback_collectors_are_classified_direct_calls() {
    let source = source_text("system/fileio/mod.rs");
    for name in ["run_after_insert_file_pipeline", "prepare_write_region"] {
        let body = function_body(&source, name);
        assert!(
            !body.contains("with_hook_walk") && !body.contains("HookWalkAuthorization"),
            "{name} must stay a classified direct call, not a hook walk"
        );
        assert!(
            body.contains("apply(") || body.contains("funcall_general("),
            "{name} should still invoke its callbacks directly"
        );
    }
}

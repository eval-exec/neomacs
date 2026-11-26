//! Source guard for the `TaggedValue::DEAD` sentinel and null string pointers (vc A4).
//!
//! A live Lisp string carries `TAG_STRING` over a non-null, 8-byte-aligned
//! `*const StringObj`. Making a null `TAG_STRING` word unrepresentable in safe
//! Rust requires four invariants, enforced by AST inspection across every
//! production crate in the workspace:
//!
//! 1. `TaggedValue::DEAD` must not be `pub` (it is allocator-internal free-list
//!    poison; external callers use `Value::is_dead()`), and its initializer must
//!    not carry `TAG_STRING` or any other heap-pointer tag (`TAG_CONS`,
//!    `TAG_VECLIKE`, `TAG_FLOAT`).
//! 2. No `const` or `static` constructing `TaggedValue` / `Self` inside
//!    `neovm-value` may carry a heap-pointer tag (`TAG_STRING`, `TAG_CONS`,
//!    `TAG_VECLIKE`, `TAG_FLOAT`), preventing any new null-pointer heap sentinel.
//! 3. No safe `fn` in `neovm-value` may construct `Self(...)` / `TaggedValue(...)`
//!    carrying `TAG_STRING` — only `unsafe fn` constructors (such as
//!    `TaggedValue::from_string_ptr`) may mint a string-tagged word from a raw
//!    pointer.
//! 4. No safe production `fn` anywhere in the workspace may cast an expression
//!    to `*const StringObj` or `*mut StringObj` outside the audited allowlist
//!    below, preventing new safe sites from bypassing `as_string_ptr()` and
//!    dereferencing an unvalidated `StringObj` pointer.

use std::collections::BTreeSet;

use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{
    Expr, ExprCast, Ident, ImplItemConst, ImplItemFn, Item, ItemConst, ItemFn, ItemStatic, Type,
    Visibility,
};

use super::architecture_production_source_roots::production_source_files;

const HEAP_TAG_IDENTS: &[&str] = &["TAG_STRING", "TAG_CONS", "TAG_VECLIKE", "TAG_FLOAT"];

/// Exact `(workspace-relative path, fn name)` allowlist of safe production
/// functions that perform an audited cast to `*const StringObj` or
/// `*mut StringObj`. Adding a new safe raw `StringObj` cast anywhere in the
/// workspace fails this guard; callers should route through
/// `Value::as_string_ptr()` / `Value::as_lisp_string()` instead.
const ALLOWED_SAFE_STRING_OBJ_CASTS: &[(&str, &str)] = &[
    (
        "crates/neovm-eval/src/emacs_core/lisp/native/builtins/collections.rs",
        "aref_fast",
    ),
    (
        "crates/neovm-value/src/tagged/gc/collector_reads.rs",
        "gc_string_ptr",
    ),
    (
        "crates/neovm-value/src/tagged/gc/incremental.rs",
        "mark_value",
    ),
    (
        "crates/neovm-value/src/tagged/gc/incremental.rs",
        "verify_marked_objects_owned",
    ),
    (
        "crates/neovm-value/src/tagged/mutate.rs",
        "clear_string_intervals",
    ),
    (
        "crates/neovm-value/src/tagged/mutate.rs",
        "set_string_byte_same_char_count_in_store",
    ),
    (
        "crates/neovm-value/src/tagged/mutate.rs",
        "with_lisp_string_mut",
    ),
    (
        "crates/neovm-value/src/tagged/mutate.rs",
        "with_string_bytes_mut",
    ),
    (
        "crates/neovm-value/src/tagged/mutate.rs",
        "with_string_text_props_mut_with_had",
    ),
    ("crates/neovm-value/src/tagged/value.rs", "as_string_ptr"),
];

fn is_cfg_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("cfg") {
            return false;
        }
        let tokens = attr.meta.require_list().ok().map(|l| l.tokens.to_string());
        matches!(tokens.as_deref(), Some(s) if s.trim() == "test")
    })
}

fn type_is_tagged_value(ty: &Type) -> bool {
    match ty {
        Type::Path(tp) => tp.path.segments.last().is_some_and(|seg| {
            matches!(
                seg.ident.to_string().as_str(),
                "Self" | "TaggedValue" | "Value"
            )
        }),
        _ => false,
    }
}

fn type_is_string_obj_ptr(ty: &Type) -> bool {
    match ty {
        Type::Ptr(ptr) => match ptr.elem.as_ref() {
            Type::Path(tp) => tp
                .path
                .segments
                .last()
                .is_some_and(|seg| seg.ident == "StringObj"),
            _ => false,
        },
        _ => false,
    }
}

struct IdentCollector<'a> {
    needles: &'a [&'a str],
    found: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for IdentCollector<'_> {
    fn visit_ident(&mut self, ident: &'ast Ident) {
        let name = ident.to_string();
        if self.needles.contains(&name.as_str()) {
            self.found.insert(name);
        }
    }
}

fn expr_mentions_any_ident(expr: &Expr, needles: &[&str]) -> Vec<String> {
    let mut collector = IdentCollector {
        needles,
        found: BTreeSet::new(),
    };
    collector.visit_expr(expr);
    collector.found.into_iter().collect()
}

fn expr_constructs_tagged_value_with_tag_string(expr: &Expr) -> Vec<usize> {
    struct CtorFinder {
        lines: Vec<usize>,
    }
    impl<'ast> Visit<'ast> for CtorFinder {
        fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
            let is_value_ctor = match call.func.as_ref() {
                Expr::Path(ep) => ep.path.segments.last().is_some_and(|seg| {
                    matches!(seg.ident.to_string().as_str(), "Self" | "TaggedValue")
                }),
                _ => false,
            };
            if is_value_ctor
                && call
                    .args
                    .iter()
                    .any(|arg| !expr_mentions_any_ident(arg, &["TAG_STRING"]).is_empty())
            {
                self.lines.push(call.span().start().line);
            }
            visit::visit_expr_call(self, call);
        }
    }
    let mut finder = CtorFinder { lines: Vec::new() };
    finder.visit_expr(expr);
    finder.lines
}

struct RawStringObjCastFinder {
    lines: Vec<usize>,
}

impl<'ast> Visit<'ast> for RawStringObjCastFinder {
    fn visit_expr_cast(&mut self, cast: &'ast ExprCast) {
        if type_is_string_obj_ptr(&cast.ty) {
            self.lines.push(cast.span().start().line);
        }
        visit::visit_expr_cast(self, cast);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if call.method == "cast"
            && let Some(turbofish) = &call.turbofish
            && turbofish.args.iter().any(|arg| {
                matches!(
                    arg,
                    syn::GenericArgument::Type(Type::Path(tp))
                        if tp.path.segments.last().is_some_and(|seg| seg.ident == "StringObj")
                )
            })
        {
            self.lines.push(call.span().start().line);
        }
        visit::visit_expr_method_call(self, call);
    }

    fn visit_item(&mut self, item: &'ast Item) {
        // Do not descend into nested `unsafe fn` items from an enclosing safe fn.
        if let Item::Fn(nested) = item
            && matches!(nested.sig.safety, syn::Safety::Unsafe(_))
        {
            return;
        }
        visit::visit_item(self, item);
    }
}

struct GuardScanner<'a> {
    rel_path: &'a str,
    in_neovm_value: bool,
    in_tagged_value_impl: bool,
    violations: Vec<String>,
    observed_safe_casts: BTreeSet<(String, String)>,
}

impl<'a> GuardScanner<'a> {
    fn check_const_or_static(
        &mut self,
        name: &Ident,
        vis: &Visibility,
        ty: &Type,
        expr: &Expr,
        line: usize,
    ) {
        if !self.in_neovm_value {
            return;
        }
        let is_dead = name == "DEAD";
        if is_dead && matches!(vis, Visibility::Public(_)) {
            self.violations.push(format!(
                "{}:{}: `DEAD` must not be `pub` (use `pub(crate)` and expose `is_dead()`)",
                self.rel_path, line
            ));
        }
        let constructs_value = (self.in_tagged_value_impl && type_is_tagged_value(ty))
            || is_dead
            || matches!(ty, Type::Path(tp) if tp.path.segments.last().is_some_and(|s| s.ident == "TaggedValue"));
        if constructs_value {
            let heap_tags = expr_mentions_any_ident(expr, HEAP_TAG_IDENTS);
            if !heap_tags.is_empty() {
                self.violations.push(format!(
                    "{}:{}: `{}` constant must not use heap-pointer tag(s) {:?} (makes null heap pointers representable)",
                    self.rel_path, line, name, heap_tags
                ));
            }
        }
    }

    fn check_fn(&mut self, sig: &syn::Signature, block: &syn::Block) {
        if matches!(sig.safety, syn::Safety::Unsafe(_)) {
            return;
        }
        let fn_name = sig.ident.to_string();
        if self.in_neovm_value {
            for stmt in &block.stmts {
                if let syn::Stmt::Expr(expr, _) = stmt {
                    for line in expr_constructs_tagged_value_with_tag_string(expr) {
                        self.violations.push(format!(
                            "{}:{}: safe fn `{}` constructs a `TAG_STRING` TaggedValue; minting string tags must be `unsafe fn`",
                            self.rel_path, line, fn_name
                        ));
                    }
                } else if let syn::Stmt::Local(local) = stmt
                    && let Some(init) = &local.init
                {
                    for line in expr_constructs_tagged_value_with_tag_string(&init.expr) {
                        self.violations.push(format!(
                            "{}:{}: safe fn `{}` constructs a `TAG_STRING` TaggedValue; minting string tags must be `unsafe fn`",
                            self.rel_path, line, fn_name
                        ));
                    }
                }
            }
        }

        let mut finder = RawStringObjCastFinder { lines: Vec::new() };
        finder.visit_block(block);
        if !finder.lines.is_empty() {
            let key = (self.rel_path.to_string(), fn_name.clone());
            self.observed_safe_casts.insert(key);
            let allowed = ALLOWED_SAFE_STRING_OBJ_CASTS
                .iter()
                .any(|&(p, f)| p == self.rel_path && f == fn_name);
            if !allowed {
                for line in finder.lines {
                    self.violations.push(format!(
                        "{}:{}: safe fn `{}` casts directly to `*const/*mut StringObj` outside the audited allowlist",
                        self.rel_path, line, fn_name
                    ));
                }
            }
        }
    }
}

impl<'ast> Visit<'ast> for GuardScanner<'_> {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if is_cfg_test_attr(&item.attrs) {
            return;
        }
        visit::visit_item_mod(self, item);
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        if is_cfg_test_attr(&item.attrs) {
            return;
        }
        let prev = self.in_tagged_value_impl;
        if type_is_tagged_value(&item.self_ty) {
            self.in_tagged_value_impl = true;
        }
        visit::visit_item_impl(self, item);
        self.in_tagged_value_impl = prev;
    }

    fn visit_item_const(&mut self, item: &'ast ItemConst) {
        if is_cfg_test_attr(&item.attrs) {
            return;
        }
        self.check_const_or_static(
            &item.ident,
            &item.vis,
            &item.ty,
            &item.expr,
            item.ident.span().start().line,
        );
        visit::visit_item_const(self, item);
    }

    fn visit_impl_item_const(&mut self, item: &'ast ImplItemConst) {
        if is_cfg_test_attr(&item.attrs) {
            return;
        }
        self.check_const_or_static(
            &item.ident,
            &item.vis,
            &item.ty,
            &item.expr,
            item.ident.span().start().line,
        );
        visit::visit_impl_item_const(self, item);
    }

    fn visit_item_static(&mut self, item: &'ast ItemStatic) {
        if is_cfg_test_attr(&item.attrs) {
            return;
        }
        self.check_const_or_static(
            &item.ident,
            &item.vis,
            &item.ty,
            &item.expr,
            item.ident.span().start().line,
        );
        visit::visit_item_static(self, item);
    }

    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        if is_cfg_test_attr(&item.attrs) {
            return;
        }
        self.check_fn(&item.sig, &item.block);
        visit::visit_item_fn(self, item);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        if is_cfg_test_attr(&item.attrs) {
            return;
        }
        self.check_fn(&item.sig, &item.block);
        visit::visit_impl_item_fn(self, item);
    }
}

fn scan_source(rel_path: &str, src: &str) -> (Vec<String>, BTreeSet<(String, String)>) {
    let file = syn::parse_file(src).unwrap_or_else(|err| {
        panic!("dead_string_tag_guard failed to parse {rel_path}: {err}");
    });
    let mut scanner = GuardScanner {
        rel_path,
        in_neovm_value: rel_path.starts_with("crates/neovm-value/"),
        in_tagged_value_impl: false,
        violations: Vec::new(),
        observed_safe_casts: BTreeSet::new(),
    };
    scanner.visit_file(&file);
    (scanner.violations, scanner.observed_safe_casts)
}

#[test]
fn dead_sentinel_and_string_ptr_unrepresentability_guard() {
    let mut violations = Vec::new();
    let mut observed_casts = BTreeSet::new();

    for entry in production_source_files() {
        if entry.is_test_path() {
            continue;
        }
        let rel = format!("crates/{}", entry.workspace_relative());
        let src = std::fs::read_to_string(&entry.path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", entry.path.display()));
        let (file_violations, file_casts) = scan_source(&rel, &src);
        violations.extend(file_violations);
        observed_casts.extend(file_casts);
    }

    // Keep the allowlist exact so removed raw casts cannot leave stale entries.
    for &(path, fn_name) in ALLOWED_SAFE_STRING_OBJ_CASTS {
        if !observed_casts.contains(&(path.to_string(), fn_name.to_string())) {
            violations.push(format!(
                "stale ALLOWED_SAFE_STRING_OBJ_CASTS entry: ({path}, {fn_name})"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "dead_string_tag_guard violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn dead_string_tag_guard_rejects_synthetic_offenders() {
    // 1. Original bc06ca3f5c defect: pub const DEAD carrying TAG_STRING.
    let (v, _) = scan_source(
        "crates/neovm-value/src/tagged/value.rs",
        r#"
        impl TaggedValue {
            pub const DEAD: Self = Self(TAG_STRING, PhantomData);
        }
        "#,
    );
    assert!(
        v.len() == 2,
        "expected both visibility and heap-tag violations, got: {v:?}"
    );

    // 2. Private DEAD still carrying TAG_STRING.
    let (v, _) = scan_source(
        "crates/neovm-value/src/tagged/value.rs",
        r#"
        impl TaggedValue {
            pub(crate) const DEAD: Self = Self(TAG_STRING, PhantomData);
        }
        "#,
    );
    assert!(
        v.len() == 1 && v[0].contains("TAG_STRING"),
        "expected heap-tag violation, got: {v:?}"
    );

    // 3. Non-string DEAD still pub.
    let (v, _) = scan_source(
        "crates/neovm-value/src/tagged/value.rs",
        r#"
        impl TaggedValue {
            pub const DEAD: Self = Self((1 << TAG_BITS) | 0b001, PhantomData);
        }
        "#,
    );
    assert!(
        v.len() == 1 && v[0].contains("must not be `pub`"),
        "expected visibility violation, got: {v:?}"
    );

    // 4. Safe fn minting a TAG_STRING TaggedValue.
    let (v, _) = scan_source(
        "crates/neovm-value/src/tagged/value.rs",
        r#"
        impl TaggedValue {
            pub fn null_string() -> Self {
                Self(TAG_STRING, PhantomData)
            }
        }
        "#,
    );
    assert!(
        v.len() == 1 && v[0].contains("safe fn `null_string`"),
        "expected safe TAG_STRING ctor violation, got: {v:?}"
    );

    // 5. New safe fn anywhere in workspace casting raw bits to *const StringObj.
    let (v, _) = scan_source(
        "crates/neovm-eval/src/emacs_core/new_site.rs",
        r#"
        pub fn peek_string(v: Value) -> bool {
            let p = (v.bits() & !TAG_MASK) as *const StringObj;
            !p.is_null()
        }
        "#,
    );
    assert!(
        v.len() == 1 && v[0].contains("outside the audited allowlist"),
        "expected raw StringObj cast violation, got: {v:?}"
    );

    // 5b. New safe fn casting via `.cast::<StringObj>()` outside the allowlist.
    let (v, _) = scan_source(
        "crates/neovm-eval/src/emacs_core/new_site.rs",
        r#"
        pub fn peek_string_turbofish(p: *const u8) -> bool {
            let s = p.cast::<StringObj>();
            !s.is_null()
        }
        "#,
    );
    assert!(
        v.len() == 1 && v[0].contains("outside the audited allowlist"),
        "expected turbofish StringObj cast violation, got: {v:?}"
    );

    // 6. Clean code passes: pub(crate) non-heap DEAD + unsafe fn from_string_ptr.
    let (v, _) = scan_source(
        "crates/neovm-value/src/tagged/value.rs",
        r#"
        impl TaggedValue {
            pub(crate) const DEAD: Self = Self((1 << TAG_BITS) | 0b001, PhantomData);
            pub fn is_dead(self) -> bool { self.0 == Self::DEAD.0 }
            pub unsafe fn from_string_ptr(obj: *const StringObj) -> Self {
                Self(obj as usize | TAG_STRING, PhantomData)
            }
        }
        "#,
    );
    assert!(v.is_empty(), "clean code must pass, got: {v:?}");
}

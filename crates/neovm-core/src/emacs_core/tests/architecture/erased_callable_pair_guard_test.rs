//! Closed-allowlist tripwire for the erased-callable-pair hazard.
//!
//! An *erased callable pair* is an untyped pointer stored beside the function
//! pointer that casts it back to a concrete type. The soundness obligation is
//! a type-identity obligation — the callee's `F` must be the pointee type of
//! `data` — and nothing in the syntax of a single cast establishes it. Design
//! note: `tmp/v10x/integ-inbox/ERASED-CALLABLE-GUARD.md`.
//!
//! **The rule, keyed on the pair rather than on a spelling.** A struct, union,
//! or enum variant that declares both a field of untyped-pointer type
//! (`*mut ()`, `*const ()`, `*mut c_void`, `*const c_void`, `NonNull<()>`) and
//! a field of function-pointer type — spelled `fn(…)` inline, or a path naming
//! a `type` alias in the same file that resolves to one — must appear in
//! `erased_callable_pair_scopes.tsv` with a reason. Deny by default; a source
//! file that will not parse is a violation.
//!
//! This is deliberately narrower than "a file that has both spellings
//! somewhere". `jit/compile/shims.rs` declares 83 `*const ()` addresses for
//! shim identity *and* reconstitutes typed pointers (`ctx as *mut Context`)
//! from arguments the JIT guarantees: two unrelated concerns in one file, and
//! no item holds both halves. Keying on the file would make that file a false
//! positive and teach contributors to allowlist it.
//!
//! **What this row does not catch.** A pair split across two items, or one
//! whose halves meet only at a call site, is out of scope: catching that needs
//! dataflow, which is the road back to the spelling-guessing scanners this
//! project is retiring. The containment that carries the safety argument is
//! the private constructor that derives both halves from one generic
//! instantiation — `ErasedParkBridge::new`. This row is a tripwire that tells
//! a reviewer where to look; it is not the enforcement.

use super::super::architecture_production_source_roots::production_source_files;
use super::code_only;
use syn::{Item, Type};

/// The reviewed pairs: `workspace-relative path`, item name, then a reason.
/// Tab-separated; `#` starts a comment line.
const SCOPES: &str = include_str!("erased_callable_pair_scopes.tsv");

/// Every `type X = …` in the file, by name, so a field typed `X` can be asked
/// whether the alias it names carries a function pointer.
fn aliases(items: &[Item]) -> Vec<(String, Type)> {
    let mut found = Vec::new();
    for item in items {
        match item {
            Item::Type(alias) => found.push((alias.ident.to_string(), (*alias.ty).clone())),
            Item::Mod(module) => {
                if let Some((_, nested)) = &module.content {
                    found.extend(aliases(nested));
                }
            }
            _ => {}
        }
    }
    found
}

/// `*mut ()`, `*const ()`, `*mut c_void`, `*const c_void`, `NonNull<()>`.
fn is_untyped_pointer(ty: &Type) -> bool {
    match ty {
        Type::Ptr(pointer) => match &*pointer.elem {
            Type::Tuple(tuple) => tuple.elems.is_empty(),
            Type::Path(path) => path
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "c_void"),
            _ => false,
        },
        Type::Path(path) => {
            let Some(segment) = path.path.segments.last() else {
                return false;
            };
            if segment.ident != "NonNull" {
                return false;
            }
            let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                return false;
            };
            match arguments.args.first() {
                // `NonNull<()>`
                Some(syn::GenericArgument::Type(Type::Tuple(tuple))) => tuple.elems.is_empty(),
                // `NonNull<c_void>`, spelled bare or path-qualified.
                Some(syn::GenericArgument::Type(Type::Path(path))) => path
                    .path
                    .segments
                    .last()
                    .is_some_and(|inner| inner.ident == "c_void"),
                _ => false,
            }
        }
        _ => false,
    }
}

/// Whether `ty` is a function pointer, directly or through an alias declared
/// in the same file.
///
/// A wrapper around a function pointer is still a function pointer field, so
/// `Option<fn()>`, `Box<unsafe fn()>`, `[fn(); N]`, `(u32, fn())` and
/// `&fn()` all count: a pair spelled that way is the same hazard, and a rule
/// that missed it would be recognising a spelling instead of the class.
/// Aliases resolve transitively; an alias that names itself stops at the
/// cycle rather than looping.
fn is_function_pointer(ty: &Type, aliases: &[(String, Type)], depth: u8) -> bool {
    match ty {
        Type::FnPtr(_) => true,
        Type::Paren(inner) => is_function_pointer(&inner.elem, aliases, depth),
        Type::Group(inner) => is_function_pointer(&inner.elem, aliases, depth),
        Type::Reference(inner) => is_function_pointer(&inner.elem, aliases, depth),
        Type::Slice(inner) => is_function_pointer(&inner.elem, aliases, depth),
        Type::Array(inner) => is_function_pointer(&inner.elem, aliases, depth),
        Type::Tuple(tuple) => tuple
            .elems
            .iter()
            .any(|element| is_function_pointer(element, aliases, depth)),
        Type::Path(path) => {
            let Some(segment) = path.path.segments.last() else {
                return false;
            };
            // `PhantomData<fn()>` is a variance marker: zero-sized, it stores
            // no callable, so it is not a half. It is common in this tree
            // (`Invariant<'id>` in `parallel_entry/capability.rs`), and a
            // struct that also holds an erased pointer would otherwise be a
            // false pair.
            if segment.ident == "PhantomData" {
                return false;
            }
            if let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments {
                let wrapped = arguments.args.iter().any(|argument| match argument {
                    syn::GenericArgument::Type(inner) => is_function_pointer(inner, aliases, depth),
                    _ => false,
                });
                if wrapped {
                    return true;
                }
            }
            if depth == 0 || !segment.arguments.is_empty() {
                return false;
            }
            aliases
                .iter()
                .filter(|(name, _)| *name == segment.ident.to_string())
                .any(|(_, aliased)| is_function_pointer(aliased, aliases, depth - 1))
        }
        _ => false,
    }
}

/// Whether these fields hold an untyped pointer beside a function pointer.
fn holds_both<'a>(
    fields: impl Iterator<Item = &'a syn::Field>,
    aliases: &[(String, Type)],
) -> bool {
    let (mut pointer, mut callable) = (false, false);
    for field in fields {
        pointer |= is_untyped_pointer(&field.ty);
        callable |= is_function_pointer(&field.ty, aliases, 4);
    }
    pointer && callable
}

/// Every struct, union, or enum variant in `items` that holds both halves.
fn pairs_in(items: &[Item], aliases: &[(String, Type)], found: &mut Vec<String>) {
    for item in items {
        match item {
            Item::Struct(item) => {
                if holds_both(item.fields.iter(), aliases) {
                    found.push(item.ident.to_string());
                }
            }
            Item::Union(item) => {
                if holds_both(item.fields.named.iter(), aliases) {
                    found.push(item.ident.to_string());
                }
            }
            Item::Enum(item) => {
                for variant in &item.variants {
                    if holds_both(variant.fields.iter(), aliases) {
                        found.push(format!("{}::{}", item.ident, variant.ident));
                    }
                }
            }
            Item::Mod(module) => {
                if let Some((_, nested)) = &module.content {
                    pairs_in(nested, aliases, found);
                }
            }
            _ => {}
        }
    }
}

/// The erased callable pairs one source text declares, in item-name form.
fn erased_callable_pairs(source: &str) -> Result<Vec<String>, String> {
    let file = syn::parse_file(source).map_err(|error| format!("unanalysable: {error}"))?;
    let aliases = aliases(&file.items);
    let mut found = Vec::new();
    pairs_in(&file.items, &aliases, &mut found);
    found.sort();
    found.dedup();
    Ok(found)
}

/// The reviewed rows, keyed by `(workspace-relative path, item)`.
fn recorded_scopes() -> Vec<(&'static str, &'static str, &'static str)> {
    let mut rows = Vec::new();
    for (number, line) in SCOPES.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split('\t');
        let (Some(path), Some(item), Some(reason), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            panic!(
                "erased-callable scopes line {} is not `path<TAB>item<TAB>reason`: {line:?}",
                number + 1
            );
        };
        assert!(
            !reason.trim().is_empty(),
            "line {}: an allowlisted pair needs a reason",
            number + 1
        );
        assert!(
            !rows
                .iter()
                .any(|(recorded_path, recorded_item, _)| *recorded_path == path
                    && *recorded_item == item),
            "erased-callable scopes lists {path}:{item} twice"
        );
        rows.push((path, item, reason));
    }
    rows
}

#[test]
fn a_struct_holding_both_halves_is_a_pair() {
    let source = PAIR_SHAPE;
    assert_eq!(
        erased_callable_pairs(source).expect("parses"),
        vec!["ErasedParkBridge".to_owned()]
    );
}

/// The reviewed shape, in the tree's own spelling: the trampoline alias is a
/// separate item, and the struct names it.
const PAIR_SHAPE: &str = "\
type Call = unsafe fn(data: *mut (), ev: u32);
struct ErasedParkBridge { data: *mut (), call: Call }";

#[test]
fn inline_fn_fields_and_enum_variants_are_pairs() {
    for (source, expected) in [
        ("struct P { data: *mut (), call: fn(*mut ()) }", "P"),
        ("union P { a: *const (), b: extern \"C\" fn(u32) }", "P"),
        ("enum P { A { data: *mut () , call: fn() } }", "P::A"),
        (
            "type C = fn(); type D = C; struct P { d: *mut (), c: D }",
            "P",
        ),
        (
            "struct P { d: NonNull<()>, c: usize } struct Q { d: NonNull<()>, c: fn() }",
            "Q",
        ),
        // A wrapped function pointer is the same hazard, not a new spelling.
        ("struct P { d: *mut (), c: Option<fn()> }", "P"),
        (
            "struct P { d: *mut c_void, c: Box<unsafe fn(*mut ())> }",
            "P",
        ),
        ("struct P { d: NonNull<c_void>, c: [fn(); 2] }", "P"),
        (
            "struct P { d: *const (), c: (u32, extern \"C\" fn()) }",
            "P",
        ),
    ] {
        assert_eq!(
            erased_callable_pairs(source).expect("parses"),
            vec![expected.to_owned()],
            "{source}"
        );
    }
}

#[test]
fn one_half_alone_is_not_a_pair() {
    for source in [
        // An fn-address newtype: identity, not a callable pair (shims.rs).
        "struct ShimAddr(*const ());",
        // A typed reconstitution from a documented argument (shims.rs).
        "fn f(ctx: *mut ()) -> *mut u32 { ctx as *mut u32 }",
        "struct P { data: *mut (), n: usize }",
        "struct P { call: fn() }",
        "type Call = fn(); struct P { call: Call }",
        // A variance marker is zero-sized and stores no callable.
        "struct P { d: *mut (), m: PhantomData<fn()> }",
        "type Invariant<'id> = PhantomData<fn(&'id mut ()) -> &'id mut ()>;\
         struct P { d: *mut (), m: Invariant<'static> }",
    ] {
        assert!(
            erased_callable_pairs(source).expect("parses").is_empty(),
            "{source}"
        );
    }
}

#[test]
fn an_unparsable_source_is_a_violation_not_a_skip() {
    let error = erased_callable_pairs("struct {").expect_err("must refuse");
    assert!(error.starts_with("unanalysable:"), "{error}");
}

#[test]
fn production_erased_callable_pairs_are_recorded() {
    let recorded = recorded_scopes();
    let mut violations = Vec::new();
    let mut seen: Vec<(String, String)> = Vec::new();
    for file in production_source_files() {
        if file.is_test_path() {
            continue;
        }
        let source = std::fs::read_to_string(&file.path)
            .unwrap_or_else(|error| panic!("read {}: {error}", file.path.display()));
        let path = file.workspace_relative();
        let pairs =
            erased_callable_pairs(&source).unwrap_or_else(|error| panic!("{path}: {error}"));
        for item in pairs {
            let allowed = recorded.iter().any(|(recorded_path, recorded_item, _)| {
                *recorded_path == path && *recorded_item == item
            });
            if !allowed {
                violations.push(format!(
                    "{path}: {item} holds an untyped pointer beside a function pointer and \
                     is not in erased_callable_pair_scopes.tsv. Seal it in a private \
                     constructor that derives both halves from one generic instantiation, \
                     or record it with a reason"
                ));
            }
            seen.push((path.clone(), item));
        }
    }
    for (path, item, _) in &recorded {
        if !seen
            .iter()
            .any(|(seen_path, seen_item)| seen_path == path && seen_item == item)
        {
            violations.push(format!(
                "{path}: {item} is allowlisted but no longer holds both halves; \
                 drop the stale row"
            ));
        }
    }
    assert!(
        violations.is_empty(),
        "{} erased-callable-pair violation(s):\n{}",
        violations.len(),
        violations.join("\n")
    );
}

#[test]
fn a_listed_pair_file_contains_no_macros() {
    // A `macro_rules!` body is not in the AST this row walks, so a pair
    // generated by a macro inside a file we have reviewed would be invisible:
    // the file would be "checked" and the pair would never be seen. Both
    // listed files are macro-free today, so this lands green; it is here so
    // the first contributor who adds a macro to one of them has to say why.
    let recorded = recorded_scopes();
    for file in production_source_files() {
        if file.is_test_path() {
            continue;
        }
        let path = file.workspace_relative();
        if !recorded
            .iter()
            .any(|(recorded_path, _, _)| *recorded_path == path)
        {
            continue;
        }
        let source = std::fs::read_to_string(&file.path)
            .unwrap_or_else(|error| panic!("read {}: {error}", file.path.display()));
        let code = code_only(&source);
        assert!(
            !code.contains("macro_rules!"),
            "{path} holds a recorded erased callable pair and also defines a \
             `macro_rules!`. A macro body is invisible to this row, so either \
             move the macro out of this file or extend the row to expand it"
        );
    }
}

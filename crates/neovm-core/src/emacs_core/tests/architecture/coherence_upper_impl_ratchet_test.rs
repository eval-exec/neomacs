//! Coherence ratchet for the neovm-value move.
//!
//! After the g0 set (value, symbol, intern, error, errors, forward,
//! emacs_char, boolvec, alloc, hashtab) and the bottom peers (tagged,
//! heap_types, gc_trace, tls_fingerprint) move into a separate crate, Rust
//! coherence forbids two forms in neovm-core:
//!
//! * an inherent impl on a moved type;
//! * an impl of a foreign trait (std/core/another crate) for a moved type
//!   unless a local type argument satisfies the RFC 2451 covered rule.
//!
//! This guard counts the remaining illegal impls and requires the count to
//! equal [`REMAINING_ALLOWED`]. Every coherence-prep commit converts one
//! seam and decrements the number in the same commit. A new illegal impl
//! fails immediately; a stale allowlist row fails as a mismatch too.
//!
//! The moving set is **derived mechanically from the tree under test**: a file
//! is moving when its declared module is a bottom-peer top-level module or a
//! G0 `emacs_core` member (`architecture_moving_set`, the same single source
//! of truth as the layering ratchet and the move manifest), or when it belongs
//! to the extracted `neovm-value` crate. It is deliberately not a type list
//! taken from main: a split cut can move a type into a moving module
//! (T1-storage moves `ByteCodeFunction` into `tagged/bytecode/chunk.rs`; T1
//! lowers `Op` and the function_slots storage types under `tagged/bytecode`;
//! T1-pre moves the `jit_runtime` files under `runtime/value`), and any upper
//! impl on it must start counting on that candidate tree without editing the
//! guard.
//! The layering ratchet cannot see calls that resolve to an upper inherent
//! method from lower code; this guard catches that class by flagging the impl
//! itself.
//!
//! Type resolution works on the crate's own source tree. A self type written
//! with a qualified moving-module path (`crate::tagged::...`,
//! `crate::emacs_core::value::...`) is the moved type even when an unrelated
//! upper definition shares its final segment. An unqualified name defined only
//! inside moving files is a moved type; a name defined in a non-moving file is
//! an upper type; same-name collisions make an unqualified name ambiguous and
//! it is not counted, except the pinned identities below. Traits defined in
//! non-moving files are upper-local (legal); a foreign trait whose type
//! arguments name an upper type is the legal orphan-rule case. The scan parses
//! every source file and walks nested modules, so test modules in the same
//! crate are included.
//! The first extracted sibling crate (`neovm-value`, N5) is scanned the same
//! way under a `neovm-value/` path prefix: a type defined there is moving, and
//! an upper impl on it counts, while impls inside that crate are lower-crate
//! impls and stay legal.

use super::architecture_moving_set as moving_set;
/// Illegal upper impls still to convert. Decremented by every seam commit.
///
/// The 24 at the 85055db807 base were the deferred half of the T1-pre data cut
/// `8105f97a11` (the per-source JIT runtime and feedback data moved into
/// `runtime/value/jit_runtime`, every implementation left in
/// `emacs_core::jit`). N17 moves those impls with their types (lower-crate
/// inherent impls and storage data) and keeps only upper-local extension
/// traits, so the row is now zero. Any new illegal impl fails immediately and
/// every removed one must lower this number in the same commit.
use std::collections::{BTreeMap, BTreeSet};

use super::architecture_production_source_roots::production_source_files;
use std::sync::OnceLock;
use syn::spanned::Spanned;
use syn::{Item, Type};
const REMAINING_ALLOWED: usize = 0;

/// Moved types whose name also has a non-moving definition in this crate, so
/// the general name rule cannot classify them. Each entry is
/// `(file, self type, trait or None for inherent)`; every entry must match a
/// real impl or the guard reports a stale row, and each match counts once.
///
/// The six `Runtime` impls below are the deferred half of the T1-pre data cut
/// `8105f97a11`: it moved `Runtime`/`RuntimeState` into
/// `runtime/value/jit_runtime` but deliberately left every implementation in
/// `emacs_core::jit`. `sort_buffered.rs` defines an unrelated local `Runtime`,
/// so the unqualified name is ambiguous here. They stay counted (and pinned)
/// until the JIT cut moves the impls with the type; converting them is not
/// part of this coherence series.
const AMBIGUOUS_MOVED_IMPLS: &[(&str, &str, Option<&str>)] = &[
    // N17 converted the six pinned `Runtime` impls (feedback recording, the
    // array registry, `new`, `Deref`, `Default`, `Clone`) into lower-crate
    // impls or upper-local extension traits; no ambiguous row remains.
];

/// Moved type names that have no plain definition inside the moving files
/// because they are aliases of a moved type (`OpenedFontMetrics` is an upper
/// `type` alias of the moving `FontObjectMetrics`; `GenericProofEra`/
/// `WorldProofEra` are re-export chains onto the moving test-era struct).
/// An impl written on any of these names still extends the moved type after
/// the alias resolves, so they count exactly like unambiguous definitions.
const PINNED_MOVED_NAMES: &[&str] = &["OpenedFontMetrics", "GenericProofEra", "WorldProofEra"];

/// Physical source prefixes of the derived moving set. The membership rule is
/// the shared move-manifest rule in [`moving_set`] (bottom-peer top-level
/// modules + G0 `emacs_core` members); each member's physical path comes from
/// the crate's own `mod` declarations (`#[path = "..."]` or the default
/// `name.rs` / `name/` form). A T1 cut that lowers a type into
/// `tagged/bytecode/...` or `runtime/value/jit_runtime/...` is therefore
/// covered without editing this guard. The extracted `neovm-value` crate is
/// scanned under a `neovm-value/` prefix.
fn moving_prefixes() -> &'static [String] {
    static PREFIXES: OnceLock<Vec<String>> = OnceLock::new();
    PREFIXES.get_or_init(|| {
        // Every extracted crate is part of the moving set. A file below one of
        // these prefixes is a lower-crate file: its own impls are legal and a
        // type it defines is a moved type for upper-impl classification.
        moving_set::MOVING_CRATES
            .iter()
            .map(|name| format!("{}/", name.replace('_', "-")))
            .collect()
    })
}

/// Mechanical moving-set membership for a workspace-relative source path
/// (`<crate>/src/...`): a file belongs to the moving set when its crate is one
/// of the extracted crates.
///
/// The synthetic unit tests below feed pre-move module-tree spellings
/// (`tagged/bytecode/chunk.rs`, `emacs_core/runtime/value/new_module.rs`)
/// rather than production keys. A production key always contains `/src/`;
/// for those the crate prefix is the whole rule. For a bare module-tree
/// spelling, membership is the shared move-manifest rule (bottom-peer
/// top-level modules + G0 `emacs_core` members), which is what makes a T1
/// destination path moving without editing this guard.
fn is_moving_path(relative: &str) -> bool {
    if moving_prefixes()
        .iter()
        .any(|prefix| relative.starts_with(prefix))
    {
        return true;
    }
    if relative.contains("/src/") {
        return false;
    }
    relative.split('/').any(|part| {
        let stem = part.strip_suffix(".rs").unwrap_or(part);
        moving_set::is_bottom_peer_name(stem) || moving_set::is_g0_member(stem)
    })
}

fn base_ident(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string()),
        Type::Reference(reference) => base_ident(&reference.elem),
        Type::Paren(paren) => base_ident(&paren.elem),
        Type::Group(group) => base_ident(&group.elem),
        Type::Ptr(pointer) => base_ident(&pointer.elem),
        Type::Slice(slice) => base_ident(&slice.elem),
        Type::Array(array) => base_ident(&array.elem),
        _ => None,
    }
}

/// A self type written with a qualified path rooted at a moving module names
/// the moved type even when an unrelated upper definition shares its final
/// segment: `impl crate::tagged::value::Fixnum` is visible even when
/// `from_value.rs` defines an unrelated local `Fixnum`.
fn qualified_moving_self_type(ty: &Type) -> bool {
    match ty {
        Type::Path(path) => {
            let segments: Vec<String> = path
                .path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect();
            let rooted = |module: &[&str]| {
                segments.len() > module.len() + 1
                    && segments[0] == "crate"
                    && module
                        .iter()
                        .enumerate()
                        .all(|(index, name)| segments[index + 1] == *name)
            };
            rooted(&["tagged"])
                || rooted(&["heap_types"])
                || rooted(&["gc_trace"])
                || rooted(&["tls_fingerprint"])
                || rooted(&["emacs_core", "value"])
                || rooted(&["emacs_core", "symbol"])
                || rooted(&["emacs_core", "intern"])
                || rooted(&["emacs_core", "error"])
                || rooted(&["emacs_core", "errors"])
                || rooted(&["emacs_core", "forward"])
                || rooted(&["emacs_core", "alloc"])
                || rooted(&["emacs_core", "hashtab"])
                || rooted(&["emacs_core", "text", "emacs_char"])
                || rooted(&["emacs_core", "text", "boolvec"])
        }
        Type::Reference(reference) => qualified_moving_self_type(&reference.elem),
        Type::Paren(paren) => qualified_moving_self_type(&paren.elem),
        Type::Group(group) => qualified_moving_self_type(&group.elem),
        Type::Ptr(pointer) => qualified_moving_self_type(&pointer.elem),
        _ => false,
    }
}

fn type_arg_idents(ty: &Type, out: &mut BTreeSet<String>) {
    match ty {
        Type::Path(path) => {
            for segment in &path.path.segments {
                if let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments {
                    for argument in &arguments.args {
                        if let syn::GenericArgument::Type(argument_type) = argument {
                            type_arg_idents(argument_type, out);
                        }
                    }
                }
            }
            if let Some(ident) = base_ident(ty) {
                out.insert(ident);
            }
        }
        Type::Reference(reference) => type_arg_idents(&reference.elem, out),
        Type::Paren(paren) => type_arg_idents(&paren.elem, out),
        Type::Group(group) => type_arg_idents(&group.elem, out),
        Type::Ptr(pointer) => type_arg_idents(&pointer.elem, out),
        Type::Slice(slice) => type_arg_idents(&slice.elem, out),
        Type::Array(array) => type_arg_idents(&array.elem, out),
        Type::Tuple(tuple) => {
            for element in &tuple.elems {
                type_arg_idents(element, out);
            }
        }
        _ => {}
    }
}

fn record_def(
    is_moving: bool,
    name: &str,
    relative: &str,
    moving_defs: &mut BTreeMap<String, Vec<String>>,
    upper_defs: &mut BTreeSet<String>,
    upper_traits: &mut BTreeSet<String>,
    is_trait: bool,
) {
    if is_moving {
        moving_defs
            .entry(name.to_string())
            .or_default()
            .push(relative.to_string());
    } else {
        upper_defs.insert(name.to_string());
        if is_trait {
            upper_traits.insert(name.to_string());
        }
    }
}

fn collect_defs(
    items: &[Item],
    is_moving: bool,
    relative: &str,
    moving_defs: &mut BTreeMap<String, Vec<String>>,
    upper_defs: &mut BTreeSet<String>,
    upper_traits: &mut BTreeSet<String>,
) {
    for item in items {
        match item {
            Item::Struct(item) => record_def(
                is_moving,
                &item.ident.to_string(),
                relative,
                moving_defs,
                upper_defs,
                upper_traits,
                false,
            ),
            Item::Enum(item) => record_def(
                is_moving,
                &item.ident.to_string(),
                relative,
                moving_defs,
                upper_defs,
                upper_traits,
                false,
            ),
            Item::Union(item) => record_def(
                is_moving,
                &item.ident.to_string(),
                relative,
                moving_defs,
                upper_defs,
                upper_traits,
                false,
            ),
            Item::Type(item) => record_def(
                is_moving,
                &item.ident.to_string(),
                relative,
                moving_defs,
                upper_defs,
                upper_traits,
                false,
            ),
            Item::Trait(item) => record_def(
                is_moving,
                &item.ident.to_string(),
                relative,
                moving_defs,
                upper_defs,
                upper_traits,
                true,
            ),
            Item::Mod(module) => {
                if let Some((_, inner)) = &module.content {
                    collect_defs(
                        inner,
                        is_moving,
                        relative,
                        moving_defs,
                        upper_defs,
                        upper_traits,
                    );
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug)]
struct Violation {
    file: String,
    line: usize,
    self_type: String,
    trait_name: Option<String>,
}

fn collect_impls(
    items: &[Item],
    relative: &str,
    is_moving: bool,
    moving_defs: &BTreeMap<String, Vec<String>>,
    upper_defs: &BTreeSet<String>,
    upper_traits: &BTreeSet<String>,
    pinned: &mut BTreeSet<usize>,
    violations: &mut Vec<Violation>,
) {
    for item in items {
        match item {
            Item::Impl(implementation) => {
                if is_moving {
                    continue;
                }
                let Some(self_type) = base_ident(&implementation.self_ty) else {
                    continue;
                };
                let trait_name = implementation
                    .trait_
                    .as_ref()
                    .and_then(|(path, _)| path.segments.last())
                    .map(|segment| segment.ident.to_string());
                let pinned_index =
                    AMBIGUOUS_MOVED_IMPLS
                        .iter()
                        .position(|(file, self_name, trait_item)| {
                            *file == relative
                                && *self_name == self_type
                                && *trait_item == trait_name.as_deref()
                        });
                let moved = qualified_moving_self_type(&implementation.self_ty)
                    || (moving_defs.contains_key(&self_type) && !upper_defs.contains(&self_type))
                    || PINNED_MOVED_NAMES.contains(&self_type.as_str());
                if !moved {
                    let Some(index) = pinned_index else {
                        continue;
                    };
                    pinned.insert(index);
                } else if let Some(index) = pinned_index {
                    // A pinned identity must stay ambiguous; if the upper
                    // definition disappears, the general rule now covers it.
                    pinned.insert(index);
                }
                let legal_local_trait = trait_name
                    .as_ref()
                    .is_some_and(|name| upper_traits.contains(name));
                let mut trait_arguments = BTreeSet::new();
                if let Some((path, _)) = &implementation.trait_ {
                    for segment in &path.segments {
                        if let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments {
                            for argument in &arguments.args {
                                if let syn::GenericArgument::Type(argument_type) = argument {
                                    type_arg_idents(argument_type, &mut trait_arguments);
                                }
                            }
                        }
                    }
                }
                let legal_local_argument = trait_arguments
                    .iter()
                    .any(|argument| upper_defs.contains(argument));
                if implementation.trait_.is_none() || !(legal_local_trait || legal_local_argument) {
                    violations.push(Violation {
                        file: relative.to_string(),
                        line: implementation.self_ty.span().start().line,
                        self_type,
                        trait_name,
                    });
                }
            }
            Item::Mod(module) => {
                if let Some((_, inner)) = &module.content {
                    collect_impls(
                        inner,
                        relative,
                        is_moving,
                        moving_defs,
                        upper_defs,
                        upper_traits,
                        pinned,
                        violations,
                    );
                }
            }
            _ => {}
        }
    }
}

/// Scan parsed source files, classifying each file's moving membership from
/// its path. Returns the illegal upper impls and the pinned rows that matched.
fn scan_sources(parsed: &[(String, syn::File)]) -> (Vec<Violation>, BTreeSet<usize>) {
    let mut moving_defs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut upper_defs: BTreeSet<String> = BTreeSet::new();
    let mut upper_traits: BTreeSet<String> = BTreeSet::new();
    for (relative, syntax) in parsed {
        collect_defs(
            &syntax.items,
            is_moving_path(relative),
            relative,
            &mut moving_defs,
            &mut upper_defs,
            &mut upper_traits,
        );
    }

    let mut pinned = BTreeSet::new();
    let mut violations = Vec::new();
    for (relative, syntax) in parsed {
        collect_impls(
            &syntax.items,
            relative,
            is_moving_path(relative),
            &moving_defs,
            &upper_defs,
            &upper_traits,
            &mut pinned,
            &mut violations,
        );
    }
    (violations, pinned)
}

fn render_violations(violations: &[Violation]) -> String {
    violations
        .iter()
        .map(|violation| {
            format!(
                "{}:{}: impl {} for {}",
                violation.file,
                violation.line,
                violation.trait_name.as_deref().unwrap_or("(inherent)"),
                violation.self_type,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn coherence_upper_impl_ratchet_equals_remaining_allowed() {
    let mut parsed = Vec::new();
    for file in production_source_files() {
        let relative = file.workspace_relative();
        let source = std::fs::read_to_string(&file.path)
            .unwrap_or_else(|error| panic!("read {}: {error}", file.path.display()));
        let syntax =
            syn::parse_file(&source).unwrap_or_else(|error| panic!("parse {relative}: {error}"));
        parsed.push((relative, syntax));
    }
    assert!(
        parsed.len() > 500,
        "the scan found only {} source files; check the production roots",
        parsed.len()
    );

    let moving = parsed
        .iter()
        .filter(|(relative, _)| is_moving_path(relative))
        .count();
    assert!(
        moving > 400,
        "the derived moving set has only {moving} files; check the module paths"
    );
    assert!(
        parsed.iter().any(
            |(relative, _)| relative == "neovm-value/src/tls_fingerprint.rs"
                && is_moving_path(relative)
        ),
        "the bottom-peer fingerprint module is part of the move"
    );

    let (violations, pinned) = scan_sources(&parsed);
    assert_eq!(
        pinned.len(),
        AMBIGUOUS_MOVED_IMPLS.len(),
        "a pinned ambiguous moved-type impl is stale: remove its row"
    );
    assert_eq!(
        violations.len(),
        REMAINING_ALLOWED,
        "coherence ratchet: found {} illegal upper impls, allowed {}:\n{}",
        violations.len(),
        REMAINING_ALLOWED,
        render_violations(&violations),
    );
    eprintln!(
        "coherence ratchet residual (allowed {}):\n{}",
        REMAINING_ALLOWED,
        render_violations(&violations),
    );
}

/// The guard must redden when a type lands in a moving path and an upper file
/// impls it — the T1-storage case (`ByteCodeFunction` into
/// `tagged/bytecode/chunk.rs`) without waiting for T1 to land.
#[test]
fn coherence_ratchet_reddens_on_a_synthetic_moved_type() {
    let parsed = vec![
        (
            "tagged/bytecode/chunk.rs".to_string(),
            syn::parse_file("pub struct ByteCodeFunction;").expect("moved source"),
        ),
        (
            "emacs_core/runtime/eval/upper_consumer.rs".to_string(),
            syn::parse_file(
                "impl ByteCodeFunction { fn inspect(&self) {} }\n\
                 impl Clone for ByteCodeFunction { fn clone(&self) -> Self { ByteCodeFunction } }",
            )
            .expect("upper source"),
        ),
    ];
    let (violations, _) = scan_sources(&parsed);
    let rendered = render_violations(&violations);
    assert_eq!(
        violations.len(),
        2,
        "the synthetic T1-style move must count both upper impls:\n{rendered}"
    );
    assert!(
        rendered.contains(
            "emacs_core/runtime/eval/upper_consumer.rs:1: impl (inherent) for ByteCodeFunction"
        ),
        "{rendered}"
    );
    assert!(
        rendered.contains(
            "emacs_core/runtime/eval/upper_consumer.rs:2: impl Clone for ByteCodeFunction"
        ),
        "{rendered}"
    );
}

/// A qualified moving path is the moved type regardless of an unrelated
/// upper name collision: an inherent impl on `crate::tagged::value::Fixnum`
/// stays visible although `from_value.rs` defines its own `Fixnum`. (A foreign
/// trait with an upper type argument remains legal via RFC 2451 and is covered
/// by the local-forms fixture.)
#[test]
fn coherence_ratchet_counts_a_qualified_moving_self_type() {
    let parsed = vec![
        (
            "tagged/value.rs".to_string(),
            syn::parse_file("pub(crate) struct Fixnum(i64);").expect("moved source"),
        ),
        (
            "buffer/position.rs".to_string(),
            syn::parse_file(
                "pub(crate) struct BufferLispPos(i64);\n\
                 pub(crate) struct Fixnum;\n\
                 impl crate::tagged::value::Fixnum {\n\
                     pub(crate) fn from_pos(_: BufferLispPos) -> Self { Fixnum(0) }\n\
                 }",
            )
            .expect("upper source"),
        ),
    ];
    let (violations, _) = scan_sources(&parsed);
    let rendered = render_violations(&violations);
    assert_eq!(
        violations.len(),
        1,
        "the qualified moving path must count despite the upper Fixnum collision:\n{rendered}"
    );
    assert!(
        rendered.contains("buffer/position.rs:3: impl (inherent) for Fixnum"),
        "{rendered}"
    );
}

/// Positive fixture for the T1 shape: an upper-local extension trait
/// (`ByteCodeFunctionExt`, the trait jitcall adds) implemented for a lowered
/// type is legal after the move because the trait is local to the implementing
/// crate. The moving-file `Clone` impl is equally legal. Neither may be
/// reported; only upper inhabits/foreign traits on the moved type count.
#[test]
fn coherence_ratchet_accepts_an_upper_local_trait_impl_on_a_moved_type() {
    let parsed = vec![
        (
            "tagged/bytecode/chunk.rs".to_string(),
            syn::parse_file(
                "pub struct ByteCodeFunction;\n\
                 impl Clone for ByteCodeFunction { fn clone(&self) -> Self { ByteCodeFunction } }",
            )
            .expect("moved source"),
        ),
        (
            "emacs_core/runtime/bytecode/chunk.rs".to_string(),
            syn::parse_file(
                "pub trait ByteCodeFunctionExt { fn inspect(&self); }\n\
                 impl ByteCodeFunctionExt for ByteCodeFunction { fn inspect(&self) {} }",
            )
            .expect("upper source"),
        ),
    ];
    let (violations, _) = scan_sources(&parsed);
    assert_eq!(
        violations.len(),
        0,
        "an upper-local trait impl on a moved type is legal:\n{}",
        render_violations(&violations)
    );
}

/// Legal forms stay uncounted: an impl written in the moving file itself, an
/// upper-local trait for a moved type, and a foreign trait with an upper type
/// argument (the RFC 2451 covered rule).
#[test]
fn coherence_ratchet_ignores_legal_moving_and_local_forms() {
    let parsed = vec![
        (
            "tagged/bytecode/chunk.rs".to_string(),
            syn::parse_file(
                "pub struct ByteCodeFunction;\n\
                 impl ByteCodeFunction { fn own(&self) {} }",
            )
            .expect("moved source"),
        ),
        (
            "emacs_core/runtime/eval/upper_consumer.rs".to_string(),
            syn::parse_file(
                "pub trait UpperExt { fn f(&self); }\n\
                 impl UpperExt for ByteCodeFunction { fn f(&self) {} }\n\
                 pub struct UpperKey;\n\
                 impl From<UpperKey> for ByteCodeFunction { fn from(_: UpperKey) -> Self { ByteCodeFunction } }",
            )
            .expect("upper source"),
        ),
    ];
    let (violations, _) = scan_sources(&parsed);
    assert_eq!(
        violations.len(),
        0,
        "legal forms must not count:\n{}",
        render_violations(&violations)
    );
}

/// Membership is structural: the T1 destination path is moving even though no
/// such file exists on this tree, and upper runtime modules are not.
#[test]
fn coherence_moving_set_is_derived_from_module_paths() {
    assert!(is_moving_path("tagged/bytecode/chunk.rs"));
    assert!(is_moving_path("tagged/bytecode/function_slots.rs"));
    assert!(is_moving_path("tagged/bytecode/opcode.rs"));
    assert!(is_moving_path("tagged/header/sqlite_finalizer.rs"));
    assert!(is_moving_path("emacs_core/runtime/value/new_module.rs"));
    assert!(is_moving_path(
        "emacs_core/runtime/value/jit_runtime/mod.rs"
    ));
    assert!(is_moving_path(
        "emacs_core/runtime/value/jit_runtime/feedback.rs"
    ));
    assert!(is_moving_path(
        "emacs_core/runtime/symbol/tests/new_test.rs"
    ));
    assert!(is_moving_path("emacs_core/text/emacs_char/new_module.rs"));
    assert!(is_moving_path("heap_types.rs"));
    assert!(is_moving_path("gc_trace.rs"));
    assert!(is_moving_path("tls_fingerprint.rs"));
    assert!(is_moving_path("neovm-value/tls_scope.rs"));
    assert!(!is_moving_path("emacs_core/runtime/bytecode/chunk.rs"));
    assert!(!is_moving_path("emacs_core/runtime/eval/mod.rs"));
    assert!(!is_moving_path("emacs_core/display/font/mod.rs"));
    assert!(!is_moving_path("emacs_core/system/sqlite/mod.rs"));
}

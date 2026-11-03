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
//! is moving when its path lies under one of [`MOVING_FILE_PATHS`] or is one
//! of [`MOVING_ROOT_FILES`]. It is deliberately not a type list taken from
//! main: a split cut can move a type into a moving path (T1-storage moves
//! `ByteCodeFunction` into `tagged/bytecode/chunk.rs`), and any upper impl on
//! it must start counting on that candidate tree without editing the guard.
//! The layering ratchet cannot see calls that resolve to an upper inherent
//! method from lower code; this guard catches that class by flagging the impl
//! itself.
//!
//! Type resolution works on the crate's own source tree: a name defined only
//! inside moving files is a moved type; a name defined in a non-moving file is
//! an upper type (same-name collisions make the name ambiguous and are not
//! counted, except the pinned identities below). Traits defined in non-moving
//! files are upper-local (legal); a foreign trait whose type arguments name an
//! upper type is the legal orphan-rule case. The scan parses every source file
//! and walks nested modules, so test modules in the same crate are included.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::{Item, Type};

/// Illegal upper impls still to convert. Decremented by every seam commit.
const REMAINING_ALLOWED: usize = 0;

/// Moved types whose name also has a non-moving definition in this crate, so
/// the general name rule cannot classify them. Each entry is
/// `(file, self type, trait or None for inherent)`; every entry must match a
/// real impl or the guard reports a stale row, and each match counts once.
/// Empty since B1 moved `Display for EvalError` into the moving error module;
/// the mechanism stays for a future ambiguous identity.
const AMBIGUOUS_MOVED_IMPLS: &[(&str, &str, Option<&str>)] = &[];

/// Moved type names that have no plain definition inside the moving files
/// because they are aliases of a moved type (`OpenedFontMetrics` is an upper
/// `type` alias of the moving `FontObjectMetrics`; `GenericProofEra`/
/// `WorldProofEra` are re-export chains onto the moving test-era struct).
/// An impl written on any of these names still extends the moved type after
/// the alias resolves, so they count exactly like unambiguous definitions.
const PINNED_MOVED_NAMES: &[&str] = &["OpenedFontMetrics", "GenericProofEra", "WorldProofEra"];

/// Directories whose complete contents move into neovm-value, relative to
/// `crates/neovm-core/src`. Derived from the split design's module list, never
/// from a branch's type inventory.
const MOVING_FILE_PATHS: &[&str] = &[
    "emacs_core/runtime/value/",
    "emacs_core/runtime/symbol/",
    "emacs_core/runtime/intern/",
    "emacs_core/runtime/error/",
    "emacs_core/runtime/errors/",
    "emacs_core/runtime/forward/",
    "emacs_core/runtime/alloc/",
    "emacs_core/runtime/hashtab/",
    "emacs_core/text/emacs_char/",
    "emacs_core/text/boolvec/",
    "tagged/",
];

/// Single-file moving modules at the crate root.
const MOVING_ROOT_FILES: &[&str] = &["heap_types.rs", "gc_trace.rs", "tls_fingerprint.rs"];

/// Mechanical moving-set membership for a source path relative to
/// `crates/neovm-core/src` (forward slashes).
fn is_moving_path(relative: &str) -> bool {
    MOVING_ROOT_FILES.contains(&relative)
        || MOVING_FILE_PATHS
            .iter()
            .any(|prefix| relative.starts_with(prefix))
}

fn source_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("read source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            source_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
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
                let moved = (moving_defs.contains_key(&self_type)
                    && !upper_defs.contains(&self_type))
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
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source_root = crate_root.join("src");
    let mut files = Vec::new();
    source_files(&source_root, &mut files);
    files.sort();
    assert!(
        files.len() > 500,
        "the scan found only {} source files; check the crate root",
        files.len()
    );

    let mut parsed = Vec::new();
    for file in &files {
        let relative = file
            .strip_prefix(&source_root)
            .expect("source file is inside src")
            .to_string_lossy()
            .replace('\\', "/");
        let source = std::fs::read_to_string(file)
            .unwrap_or_else(|error| panic!("read {}: {error}", file.display()));
        let syntax =
            syn::parse_file(&source).unwrap_or_else(|error| panic!("parse {relative}: {error}"));
        parsed.push((relative, syntax));
    }

    let moving = parsed
        .iter()
        .filter(|(relative, _)| is_moving_path(relative))
        .count();
    assert!(
        moving > 400,
        "the derived moving set has only {moving} files; check the module paths"
    );
    assert!(
        parsed
            .iter()
            .any(|(relative, _)| relative == "tls_fingerprint.rs" && is_moving_path(relative)),
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
    assert!(is_moving_path("tagged/header/sqlite_finalizer.rs"));
    assert!(is_moving_path("emacs_core/runtime/value/new_module.rs"));
    assert!(is_moving_path(
        "emacs_core/runtime/symbol/tests/new_test.rs"
    ));
    assert!(is_moving_path("emacs_core/text/emacs_char/new_module.rs"));
    assert!(is_moving_path("heap_types.rs"));
    assert!(is_moving_path("gc_trace.rs"));
    assert!(is_moving_path("tls_fingerprint.rs"));
    assert!(!is_moving_path("emacs_core/runtime/bytecode/chunk.rs"));
    assert!(!is_moving_path("emacs_core/runtime/eval/mod.rs"));
    assert!(!is_moving_path("emacs_core/display/font/mod.rs"));
    assert!(!is_moving_path("emacs_core/system/sqlite/mod.rs"));
}

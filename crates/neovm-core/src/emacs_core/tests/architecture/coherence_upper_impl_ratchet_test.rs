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
//! The moving file set is pinned in `coherence_moving_files.list` (the
//! design's move manifest plus `tls_fingerprint.rs`). Type resolution works
//! on the crate's own source tree: a name defined only inside moving files
//! is a moved type; a name defined in a non-moving file is an upper type
//! (same-name collisions make the name ambiguous and are not counted, except
//! the pinned identities below). Traits defined in non-moving files are
//! upper-local (legal); a foreign trait whose type arguments name an upper
//! type is the legal orphan-rule case. The scan parses every source file and
//! walks nested modules, so test modules in the same crate are included.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::{Item, Type};

/// Illegal upper impls still to convert. Decremented by every seam commit.
const REMAINING_ALLOWED: usize = 14;

/// Moved types whose name also has a non-moving definition in this crate, so
/// the general name rule cannot classify them. Each entry is
/// `(file, self type, trait or None for inherent)`; every entry must match a
/// real impl or the guard reports a stale row, and each match counts once.
const AMBIGUOUS_MOVED_IMPLS: &[(&str, &str, Option<&str>)] = &[(
    "emacs_core/lisp/print/error_format.rs",
    "EvalError",
    Some("Display"),
)];

/// Moved type names that have no plain definition inside the moving files
/// because they are aliases of a moved type (`OpenedFontMetrics` is an upper
/// `type` alias of the moving `FontObjectMetrics`; `GenericProofEra`/
/// `WorldProofEra` are re-export chains onto the moving test-era struct).
/// An impl written on any of these names still extends the moved type after
/// the alias resolves, so they count exactly like unambiguous definitions.
const PINNED_MOVED_NAMES: &[&str] = &["OpenedFontMetrics", "GenericProofEra", "WorldProofEra"];

const MOVING_FILES: &str = include_str!("coherence_moving_files.list");

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

#[test]
fn coherence_upper_impl_ratchet_equals_remaining_allowed() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let moving_files: BTreeSet<&str> = MOVING_FILES
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert!(
        moving_files.len() > 400,
        "the pinned moving list has only {} files",
        moving_files.len()
    );
    assert!(
        moving_files.contains("tls_fingerprint.rs"),
        "the bottom-peer fingerprint module is part of the move"
    );

    let mut files = Vec::new();
    source_files(&crate_root.join("src"), &mut files);
    files.sort();
    assert!(
        files.len() > 500,
        "the scan found only {} source files; check the crate root",
        files.len()
    );

    let mut parsed = Vec::new();
    for file in &files {
        let relative = file
            .strip_prefix(crate_root.join("src"))
            .expect("source file is inside src")
            .to_string_lossy()
            .replace('\\', "/");
        let source = std::fs::read_to_string(file)
            .unwrap_or_else(|error| panic!("read {}: {error}", file.display()));
        let syntax =
            syn::parse_file(&source).unwrap_or_else(|error| panic!("parse {relative}: {error}"));
        parsed.push((relative, syntax));
    }

    let mut moving_defs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut upper_defs: BTreeSet<String> = BTreeSet::new();
    let mut upper_traits: BTreeSet<String> = BTreeSet::new();
    for (relative, syntax) in &parsed {
        collect_defs(
            &syntax.items,
            moving_files.contains(relative.as_str()),
            relative,
            &mut moving_defs,
            &mut upper_defs,
            &mut upper_traits,
        );
    }

    let mut pinned = BTreeSet::new();
    let mut violations = Vec::new();
    for (relative, syntax) in &parsed {
        collect_impls(
            &syntax.items,
            relative,
            moving_files.contains(relative.as_str()),
            &moving_defs,
            &upper_defs,
            &upper_traits,
            &mut pinned,
            &mut violations,
        );
    }

    assert_eq!(
        pinned.len(),
        AMBIGUOUS_MOVED_IMPLS.len(),
        "a pinned ambiguous moved-type impl is stale: remove its row"
    );
    let rendered: Vec<String> = violations
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
        .collect();
    assert_eq!(
        violations.len(),
        REMAINING_ALLOWED,
        "coherence ratchet: found {} illegal upper impls, allowed {}:\n{}",
        violations.len(),
        REMAINING_ALLOWED,
        rendered.join("\n"),
    );
}

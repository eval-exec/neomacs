//! Syntactic regression ratchet over the listed item kinds and legacy raw
//! `&mut BufferManager` / `&mut EditRuntime` accessor table (rt A3 2c (d)).
//!
//! ## Honest Scope — NOT a Soundness Boundary Today (see #844)
//!
//! **This scanner is a syntactic regression ratchet over the listed item kinds
//! and legacy raw-accessor table; it is NOT a soundness boundary while the
//! following two `pub` fields remain reachable in production code:**
//!
//! 1. `crates/neovm-eval/src/emacs_core/runtime/eval/mod.rs:3433` —
//!    `pub edit: neovm_edit::buffer::EditRuntime,`
//! 2. `crates/neovm-edit/src/buffer/edit_runtime.rs:23` —
//!    `pub buffers: BufferManager,`
//!
//! (`Context::buffer_manager_mut` at
//! `crates/neovm-eval/src/emacs_core/runtime/eval/context_api.rs:605` is now
//! gated behind `#[cfg(any(test, feature = "test-support"))]` per #844 Slice
//! 2a, and this test verifies via `toml::Table` manifest auditing and Cargo's
//! resolved dependency graph (`cargo metadata --format-version 1 --offline`
//! with both default features and `--all-features`) that `test-support` is
//! unreachable along all normal and build dependency edges from production
//! workspace crates.)
//! Narrowing the remaining two `pub` fields so that Rust's type system and
//! privacy rules enforce the boundary workspace-wide is tracked in GitHub
//! issue #844 (Slices 2b/2c).
//!
//! ## Documented Scanner Blind Spots
//!
//! * **`impl Deref { type Target = evil::BufferManager }`:** because a `syn`
//!   scanner cannot resolve module paths, `is_allowed_deref_target_type`
//!   matches the last path segment (`BufferManager` / `EditRuntime`), so a
//!   qualified path ending in `BufferManager` (e.g. `evil::BufferManager`) is
//!   syntactically allowed on `impl Deref`. `is_allowed_deref_method` only
//!   prevents a direct `&mut Self::Target` return from `Deref::deref` itself
//!   (requiring `&self -> &Self::Target`); it does **not** prevent `&mut
//!   BufferManager` extraction via interior mutability (e.g.
//!   `Cell<*mut BufferManager>`) or a sibling-module `DerefMut` handle.
//! * **External/sibling `DerefMut` wrapper handles:** because
//!   `FORBIDDEN_VIEW_MUT_TRAITS` only checks impls on `BufferManagerMut` /
//!   `EditRuntimeMut` and `record_raw_accessor_if_matching` inspects direct
//!   `&mut BufferManager` / `&mut EditRuntime` signatures, a newtype wrapper
//!   outside `manager_view.rs` implementing `DerefMut<Target = BufferManager>`
//!   (where `deref_mut` returns `&mut Self::Target`) is not caught
//!   syntactically. Both blind spots remain code-review concerns until #844
//!   Slices 2b/2c establish a compiler-enforced privacy boundary.
//!
//! ## What Is Enforced Today
//!
//! * **Compiler-enforced (on callers holding only a view capability):** private
//!   `manager: &'a mut BufferManager` / `runtime: &'a mut EditRuntime` fields in
//!   `crates/neovm-edit/src/buffer/manager_view.rs`, absence of `DerefMut` /
//!   `AsMut` / `BorrowMut` / `IndexMut` and safe raw unwrap methods,
//!   `!Send + !Sync + !Copy + !Clone` (`static_assertions::assert_not_impl_any!`),
//!   and the `trybuild` compile-fail contracts (`buffer_manager_view_compile_contracts`).
//! * **Scanner-enforced (this file — syntactic ratchet, deny-by-default on
//!   listed surfaces):**
//!   - **Closed top-level item allowlist** for `manager_view.rs`: ONLY `use`
//!     (no rename, no glob), `struct BufferManagerMut` / `struct EditRuntimeMut`
//!     (private fields only), `impl BufferManagerMut` / `impl EditRuntimeMut`,
//!     exact `static_assertions::assert_not_impl_any!`, and `#[cfg(test)] mod`.
//!     Any other `syn::Item` (`enum`, `union`, `trait`, `fn`, `const`, `static`,
//!     `type`, non-view `struct`/`impl`, non-`#[cfg(test)]` `mod`, etc.) fails
//!     closed with `ViolationKind::UnscannedItemKind`.
//!   - **Closed `syn::ImplItem` allowlist** for `BufferManagerMut` /
//!     `EditRuntimeMut` impls: ONLY `ImplItem::Fn` and `impl Deref { type Target }`.
//!     `ImplItem::Const`, `ImplItem::Macro`, and any other `ImplItem` fail closed.
//!   - **Closed safe-method return & callback allowlists** on `BufferManagerMut`
//!     and `EditRuntimeMut` (denying `&mut`, `QSelf`, unallowlisted opaque
//!     returns, unallowlisted wrapper return types such as sibling-module
//!     `DerefMut` newtypes, and unallowlisted callback trait bounds).
//!   - **Workspace-wide ratchets & unanalysable-input fail-closed checks:**
//!     shrink-only `ALLOWED_RAW_BUFFER_OR_EDIT_MUT_ACCESSORS` and
//!     `MAX_BUFFER_MUT_NO_SWAP_PRODUCTION_CALLS`, workspace-wide rejection of
//!     generic `&mut` type aliases, renamed imports of guarded idents/macros,
//!     non-literal/unparseable `#[path]` / `#[cfg_attr]`, unallowlisted
//!     `include!`, unreadable directories, and parse errors.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use syn::visit::{self, Visit};

use super::architecture_production_source_roots::crates_dir;

const VIEW_MODULE_REL: &str = "neovm-edit/src/buffer/manager_view.rs";
const VIEW_TYPES: &[&str] = &["BufferManagerMut", "EditRuntimeMut"];
const FORBIDDEN_VIEW_MUT_TRAITS: &[&str] = &["DerefMut", "AsMut", "BorrowMut", "IndexMut"];
const FORBIDDEN_UNWRAP_METHODS: &[&str] = &["into_manager", "into_runtime"];
const FORBIDDEN_SAFE_MUT_TARGETS: &[&str] = &["Buffer", "BufferManager", "EditRuntime"];
const RAW_ACCESSOR_TARGETS: &[&str] = &["BufferManager", "EditRuntime"];
const ALL_GUARDED_TYPE_IDENTS: &[&str] = &[
    "Buffer",
    "BufferManager",
    "EditRuntime",
    "BufferManagerMut",
    "EditRuntimeMut",
];
const GUARDED_IMPORT_IDENTS: &[&str] = &[
    "BufferManagerMut",
    "EditRuntimeMut",
    "BufferManager",
    "EditRuntime",
    "Buffer",
    "DerefMut",
    "AsMut",
    "BorrowMut",
    "IndexMut",
    "static_assertions",
    "Iterator",
    "include",
];
const ALLOWED_ELEMENT_ESCAPE: &str = "with_buffer_mut_no_swap";

/// Closed allowlist of nominal return types permitted on safe non-`Deref`
/// methods of `BufferManagerMut` and `EditRuntimeMut` (in addition to `()`,
/// `*mut MarkerObj`, `impl Iterator<Item = BufferId>`, and the method's own
/// generic return type parameter such as `R`). Any unlisted type — including
/// sibling-module wrapper newtypes (`super::SneakyDeref<'_>`) or cross-file
/// aliases (`MutRef<'_, BufferManager>`) — fails closed.
const ALLOWED_SAFE_VIEW_RETURN_TYPES: &[&str] = &[
    "Self",
    "BufferManagerMut",
    "EditRuntimeMut",
    "bool",
    "Option",
    "Result",
    "Vec",
    "BufferId",
    "EmacsBytePos",
    "TextPositionAnchor",
    "RuntimeBindingValue",
    "UndoBoundaryOutcome",
    "SavedRestrictionState",
    "OutermostRestrictionResetState",
    "TextExtent",
    "Flow",
];

/// Test-only / harness / tooling workspace crates excluded from production
/// source scanning. Every entry MUST exist under `crates/`, and every crate
/// under `crates/` with a `src/` directory MUST be either scanned or listed
/// here (bidirectional crate-list completeness).
const EXCLUDED_WORKSPACE_CRATES: &[&str] = &[
    "neomacs-gui-tests",
    "neomacs-melpa-test-support",
    "neomacs-melpa-tests",
    "neomacs-test-env",
    "neomacs-test-fonts",
    "neomacs-test-oracle",
    "neomacs-tui-tests",
    "neovm-oracle-tests",
    "xtask",
];

/// Exact shrink-only allowlist of existing production `include!(...)` macros
/// (generated lookup tables / FFI bindings). Any new `include!` anywhere in a
/// production crate fails closed.
const ALLOWED_PRODUCTION_INCLUDE_MACROS: &[(&str, &str)] = &[
    (
        "neomacs-display-protocol/src/x11_colors.rs",
        "concat ! (env ! (\"OUT_DIR\") , \"/x11_colors.rs\")",
    ),
    (
        "neomacs-display-runtime/src/window_identity.rs",
        "concat ! (env ! (\"OUT_DIR\") , \"/application_identity.rs\")",
    ),
    (
        "neomacs-webview/src/platform/linux/sys.rs",
        "concat ! (env ! (\"OUT_DIR\") , \"/wpe_platform_sys.rs\")",
    ),
    (
        "neomacs-webview/src/platform/linux/sys.rs",
        "concat ! (env ! (\"OUT_DIR\") , \"/wpe_webkit_sys.rs\")",
    ),
    (
        "neovm-eval/src/emacs_core/lisp/native/subr/effect_catalog.rs",
        "\"effect_inventory.inc\"",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/aot.rs",
        "\"shim_names.rs\"",
    ),
    (
        "neovm-value/src/tagged/gc/gc_thread.rs",
        "\"concurrent_mark_result.rs\"",
    ),
];

/// Exact shrink-only allowlist of existing production `type` aliases that
/// mention `BufferManager` inside a cold display hook `fn(...)` signature.
const ALLOWED_RAW_MUT_TYPE_ALIASES: &[(&str, &str)] = &[
    (
        "neovm-eval/src/emacs_core/runtime/eval/display_query_hooks.rs",
        "EnsureSelectedFrameId",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/display_query_hooks.rs",
        "RememberSelectedWindowPoint",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/display_query_hooks.rs",
        "WindowBodyDimensions",
    ),
];

/// Exact shrink-only allowlist of existing unconditional `#[path = "tests/..."]`
/// facade modules whose target file is still parsed and scanned directly.
const ALLOWED_SCANNED_TEST_FACADE_PATHS: &[(&str, &str)] = &[(
    "neovm-core/src/emacs_core/mod.rs",
    "tests/compat_regressions/mod.rs",
)];

/// Shrink-only production call-site budget for
/// `BufferManagerMut::with_buffer_mut_no_swap`.
const MAX_BUFFER_MUT_NO_SWAP_PRODUCTION_CALLS: usize = 0;

/// Strict shrink-only allowlist of legacy production functions, trait items,
/// and methods across the workspace that return `&mut BufferManager` or
/// `&mut EditRuntime` (or pass one to a caller callback).
///
/// Format: `("<crate>/src/<path>", "<fn_name>")`.
/// Adding any new raw `&mut BufferManager` / `&mut EditRuntime` accessor fails
/// immediately; removing a legacy accessor requires shrinking this table.
const ALLOWED_RAW_BUFFER_OR_EDIT_MUT_ACCESSORS: &[(&str, &str)] = &[
    (
        "neovm-edit/src/emacs_core/editing/host/mod.rs",
        "edit_and_vars",
    ),
    (
        "neovm-edit/src/emacs_core/editing/textprop/mod.rs",
        "with_buffer_property_change",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/context_api.rs",
        "frame_manager_and_buffers_mut",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/context_api.rs",
        "frame_manager_buffers_and_display_host_mut",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/context_api.rs",
        "frame_manager_buffers_and_minibuffer_selection_mut",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/context_api.rs",
        "frame_manager_buffers_and_minibuffers_mut",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/context_api.rs",
        "frame_manager_read_and_buffers_mut",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/context_api.rs",
        "variable_view_and_frames_buffers_mut",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/context_api.rs",
        "with_frame_buffers_and_minibuffer_selection_mut",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/edit_host.rs",
        "edit_and_vars",
    ),
];

/// Sanity floors so a rename or move cannot make this scanner pass vacuously.
const MIN_BUFFER_MANAGER_MUT_IMPLS: usize = 2; // inherent + Deref
const MIN_EDIT_RUNTIME_MUT_IMPLS: usize = 2; // inherent + Deref
const REQUIRED_UPPER_CRATES: &[&str] = &["neomacs-layout-engine", "neomacs"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViolationKind {
    ForbiddenMutTraitImpl,
    ForbiddenUnwrapMethod,
    UnexpectedUnsafeMethod,
    SafeMutReturnEscape,
    SafeQSelfReturnEscape,
    SafeUnallowlistedReturn,
    SafeOpaqueReturnEscape,
    SafeCallbackEscape,
    AssocTypeMutEscape,
    TypeAliasEscape,
    RenamedGuardedImport,
    UnscannedItemKind,
    GuardedImplMacro,
    GuardedFnBodyItem,
    GuardedFnBodyMacro,
    GuardedExprMacro,
    GuardedTypeMacro,
    GuardedPatMacro,
    GuardedFileMacro,
    GuardedUnknownAttribute,
    GuardedViewModuleEscape,
    UnscannedPathAttribute,
    DisallowedIncludeMacro,
}

#[derive(Debug)]
struct Violation {
    kind: ViolationKind,
    file: String,
    line: usize,
    detail: String,
}

fn normalize_lexical_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn walk_production_files(
    crate_name: &str,
    src_root: &Path,
    dir: &Path,
    out: &mut Vec<(String, String, PathBuf)>,
) -> std::io::Result<()> {
    let mut sorted = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        sorted.push(entry?);
    }
    sorted.sort_by_key(|entry| entry.path());
    for entry in sorted {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(name, "tests" | "test_support" | "restricted_lisp") {
                continue;
            }
            walk_production_files(crate_name, src_root, &path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with("test_")
                || name.ends_with("_test.rs")
                || name.ends_with("_tests.rs")
                || name == "tests.rs"
                || name == "test_support.rs"
                || name == "test_utils.rs"
                || name == "restricted_lisp.rs"
            {
                continue;
            }
            let rel = path
                .strip_prefix(src_root)
                .expect("file under crate src")
                .to_string_lossy()
                .replace('\\', "/");
            out.push((
                crate_name.to_string(),
                format!("{crate_name}/src/{rel}"),
                normalize_lexical_path(&path),
            ));
        }
    }
    Ok(())
}

struct WorkspaceScanPlan {
    files: Vec<(String, String, PathBuf)>,
    discovered_crates_with_src: BTreeSet<String>,
    excluded_crates_present: BTreeSet<String>,
}

fn workspace_production_source_files() -> WorkspaceScanPlan {
    let root = crates_dir();
    let mut crate_dirs = Vec::new();
    for entry in std::fs::read_dir(&root).expect("read workspace crates/") {
        let entry = entry.expect("read workspace crates/ entry");
        if entry.path().is_dir() {
            crate_dirs.push(entry);
        }
    }
    crate_dirs.sort_by_key(|entry| entry.path());

    let mut files = Vec::new();
    let mut discovered_crates_with_src = BTreeSet::new();
    let mut excluded_crates_present = BTreeSet::new();

    for entry in crate_dirs {
        let crate_name = entry
            .file_name()
            .into_string()
            .expect("valid utf-8 crate dir");
        let src = entry.path().join("src");
        if src.is_dir() {
            discovered_crates_with_src.insert(crate_name.clone());
        }
        if EXCLUDED_WORKSPACE_CRATES.contains(&crate_name.as_str()) {
            excluded_crates_present.insert(crate_name);
            continue;
        }
        if src.is_dir() {
            walk_production_files(&crate_name, &src, &src, &mut files)
                .unwrap_or_else(|err| panic!("walk {crate_name}/src failed: {err}"));
        }
    }

    WorkspaceScanPlan {
        files,
        discovered_crates_with_src,
        excluded_crates_present,
    }
}

/// AST-based `#[cfg(test)]` classifier (mirrors `unsafe_shim_guard_test`).
///
/// Returns `false` for `#[cfg(not(test))]`, `#[cfg(not(any(test, ...)))]`, and
/// `#[cfg(any(test, feature = "test-support"))]`, so production code guarded by
/// those attributes is NEVER skipped.
fn meta_is_cfg_test(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(path) => path.is_ident("test"),
        syn::Meta::List(list) if list.path.is_ident("all") => list
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            )
            .is_ok_and(|nested| nested.iter().any(meta_is_cfg_test)),
        syn::Meta::List(list) if list.path.is_ident("any") => list
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            )
            .is_ok_and(|nested| !nested.is_empty() && nested.iter().all(meta_is_cfg_test)),
        _ => false,
    }
}

fn has_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("cfg") {
            return false;
        }
        attr.parse_args::<syn::Meta>()
            .is_ok_and(|meta| meta_is_cfg_test(&meta))
    })
}

/// Returns `true` ONLY when a `#[cfg(...)]` guarantees the item is excluded
/// from ordinary production builds — either `test` or `feature = "test-support"`.
/// Never returns `true` for `#[cfg(not(test))]` or bare feature flags.
fn meta_is_cfg_test_or_test_support(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(path) => path.is_ident("test"),
        syn::Meta::NameValue(nv) if nv.path.is_ident("feature") => matches!(
            &nv.value,
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) if s.value() == "test-support"
        ),
        syn::Meta::List(list) if list.path.is_ident("all") => list
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            )
            .is_ok_and(|nested| nested.iter().any(meta_is_cfg_test_or_test_support)),
        syn::Meta::List(list) if list.path.is_ident("any") => list
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            )
            .is_ok_and(|nested| {
                !nested.is_empty() && nested.iter().all(meta_is_cfg_test_or_test_support)
            }),
        _ => false,
    }
}

fn has_cfg_test_or_test_support(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("cfg") {
            return false;
        }
        attr.parse_args::<syn::Meta>()
            .is_ok_and(|meta| meta_is_cfg_test_or_test_support(&meta))
    })
}

fn path_is_inside_crate_src_test_area(crate_src: &Path, resolved: &Path) -> bool {
    let Ok(rel) = resolved.strip_prefix(crate_src) else {
        return false;
    };
    let file_name = rel.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let in_tests_dir = rel
        .components()
        .any(|c| matches!(c.as_os_str().to_str(), Some("tests" | "test_support")));
    in_tests_dir
        || file_name.starts_with("test_")
        || file_name.ends_with("_test.rs")
        || file_name.ends_with("_tests.rs")
        || file_name == "tests.rs"
}

enum PathAttrFinding {
    Literal {
        raw_path: String,
        line: usize,
        via_cfg_attr: bool,
    },
    UnparseableOrDynamic {
        line: usize,
        reason: String,
    },
}

fn meta_line(meta: &syn::Meta) -> usize {
    meta.path()
        .segments
        .last()
        .map(|s| s.ident.span().start().line)
        .unwrap_or(0)
}

fn collect_path_attrs_from_meta(
    meta: &syn::Meta,
    via_cfg_attr: bool,
    out: &mut Vec<PathAttrFinding>,
) {
    if meta.path().is_ident("path") {
        match meta {
            syn::Meta::NameValue(nv) => {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(lit_str),
                    ..
                }) = &nv.value
                {
                    out.push(PathAttrFinding::Literal {
                        raw_path: lit_str.value(),
                        line: lit_str.span().start().line,
                        via_cfg_attr,
                    });
                } else {
                    out.push(PathAttrFinding::UnparseableOrDynamic {
                        line: meta_line(meta),
                        reason: "non-literal `#[path = ...]` expression".to_string(),
                    });
                }
            }
            _ => {
                out.push(PathAttrFinding::UnparseableOrDynamic {
                    line: meta_line(meta),
                    reason: "malformed `#[path(...)]` attribute".to_string(),
                });
            }
        }
        return;
    }

    if meta.path().is_ident("cfg_attr") {
        match meta {
            syn::Meta::List(list) => {
                match list.parse_args_with(
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                ) {
                    Ok(nested) => {
                        for item in nested.iter().skip(1) {
                            collect_path_attrs_from_meta(item, true, out);
                        }
                    }
                    Err(err) => {
                        out.push(PathAttrFinding::UnparseableOrDynamic {
                            line: meta_line(meta),
                            reason: format!("unparseable `#[cfg_attr(...)]`: {err}"),
                        });
                    }
                }
            }
            _ => {
                out.push(PathAttrFinding::UnparseableOrDynamic {
                    line: meta_line(meta),
                    reason: "malformed `#[cfg_attr]` attribute".to_string(),
                });
            }
        }
    }
}

fn extract_path_attrs(attrs: &[syn::Attribute]) -> Vec<PathAttrFinding> {
    let mut out = Vec::new();
    for attr in attrs {
        collect_path_attrs_from_meta(&attr.meta, false, &mut out);
    }
    out
}

/// Closed allowlist of attributes permitted inside guarded view scopes
/// (`manager_view.rs` and `impl BufferManagerMut` / `impl EditRuntimeMut`).
/// Any unknown attribute, proc-macro attribute, `cfg_attr`, or non-`Debug`
/// `#[derive(...)]` fails closed.
fn guarded_attribute_violation(attr: &syn::Attribute) -> Option<String> {
    let path = attr.path();
    let Some(ident) = path.get_ident() else {
        return Some(format!(
            "multi-segment/proc-macro attribute `#[{}]` is forbidden in guarded scope",
            path.segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::")
        ));
    };
    let name = ident.to_string();
    match name.as_str() {
        "doc" | "inline" | "must_use" => None,
        "cfg" => {
            if attr.parse_args::<syn::Meta>().is_ok() {
                None
            } else {
                Some("unparseable `#[cfg(...)]` attribute in guarded scope".to_string())
            }
        }
        "derive" => {
            let Ok(derives) = attr.parse_args_with(
                syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
            ) else {
                return Some("unparseable `#[derive(...)]` attribute in guarded scope".to_string());
            };
            for derive_path in derives {
                if !derive_path.is_ident("Debug") {
                    let rendered = derive_path
                        .segments
                        .iter()
                        .map(|s| s.ident.to_string())
                        .collect::<Vec<_>>()
                        .join("::");
                    return Some(format!(
                        "`#[derive({rendered})]` is forbidden in guarded scope (only `Debug` is permitted)"
                    ));
                }
            }
            None
        }
        _ => Some(format!(
            "unknown or proc-macro attribute `#[{name}]` is forbidden in guarded scope (fail closed)"
        )),
    }
}

fn type_last_ident(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(path) => path.path.segments.last().map(|s| s.ident.to_string()),
        syn::Type::Reference(r) => type_last_ident(&r.elem),
        syn::Type::Group(g) => type_last_ident(&g.elem),
        syn::Type::Paren(p) => type_last_ident(&p.elem),
        _ => None,
    }
}

/// Finds ANY identifier from `idents` anywhere inside a `syn::Type` AST
/// (including nested generics like `Option<Buffer>`, tuples, references, etc.).
struct TypeIdentFinder<'a> {
    idents: &'a [&'a str],
    found: Option<String>,
}

impl<'ast> Visit<'ast> for TypeIdentFinder<'_> {
    fn visit_path_segment(&mut self, seg: &'ast syn::PathSegment) {
        if self.found.is_some() {
            return;
        }
        let name = seg.ident.to_string();
        if self.idents.contains(&name.as_str()) {
            self.found = Some(name);
            return;
        }
        visit::visit_path_segment(self, seg);
    }
}

fn find_any_ident_in_type(ty: &syn::Type, idents: &[&str]) -> Option<String> {
    let mut finder = TypeIdentFinder {
        idents,
        found: None,
    };
    finder.visit_type(ty);
    finder.found
}

/// Finds a mutable reference to any target type name (`&mut T`), even nested
/// inside `Option<...>`, `Result<...>`, tuples, fn pointers, etc.
struct MutTargetFinder<'a> {
    targets: &'a [&'a str],
    found: Option<String>,
}

impl<'ast> Visit<'ast> for MutTargetFinder<'_> {
    fn visit_type_reference(&mut self, reference: &'ast syn::TypeReference) {
        if self.found.is_some() {
            return;
        }
        if reference.mutability.is_some()
            && let Some(ident) = type_last_ident(&reference.elem)
            && self.targets.contains(&ident.as_str())
        {
            self.found = Some(ident);
            return;
        }
        visit::visit_type_reference(self, reference);
    }
}

fn find_mut_target_in_type(ty: &syn::Type, targets: &[&str]) -> Option<String> {
    let mut finder = MutTargetFinder {
        targets,
        found: None,
    };
    finder.visit_type(ty);
    finder.found
}

fn find_mut_target_in_return(output: &syn::ReturnType, targets: &[&str]) -> Option<String> {
    let syn::ReturnType::Type(_, ty) = output else {
        return None;
    };
    find_mut_target_in_type(ty, targets)
}

/// Fail-closed `&mut` detector for safe methods on `BufferManagerMut` /
/// `EditRuntimeMut` and guarded type aliases: catches ANY `&mut ...` reference
/// regardless of spelling (`&mut Buffer`, `&mut Self::Target`,
/// `&mut <Self as Deref>::Target`, `&mut T`, `Option<&mut X>`, etc.).
struct AnyMutRefFinder {
    found: Option<String>,
}

impl<'ast> Visit<'ast> for AnyMutRefFinder {
    fn visit_type_reference(&mut self, reference: &'ast syn::TypeReference) {
        if self.found.is_some() {
            return;
        }
        if reference.mutability.is_some() {
            let rendered = match &*reference.elem {
                syn::Type::Path(tp) => {
                    let prefix = tp.qself.as_ref().map(|_| "<...>::").unwrap_or("");
                    let segs = tp
                        .path
                        .segments
                        .iter()
                        .map(|s| s.ident.to_string())
                        .collect::<Vec<_>>()
                        .join("::");
                    format!("&mut {prefix}{segs}")
                }
                _ => "&mut <non-path>".to_string(),
            };
            self.found = Some(rendered);
            return;
        }
        visit::visit_type_reference(self, reference);
    }
}

fn find_any_mut_ref_in_type(ty: &syn::Type) -> Option<String> {
    let mut finder = AnyMutRefFinder { found: None };
    finder.visit_type(ty);
    finder.found
}

fn find_any_mut_ref_in_return(output: &syn::ReturnType) -> Option<String> {
    let syn::ReturnType::Type(_, ty) = output else {
        return None;
    };
    find_any_mut_ref_in_type(ty)
}

/// Checks whether a return type contains a `QSelf` associated-type projection
/// (`<Self as MutOf<'_>>::Out`, `<BufferManager as ...>::Assoc`, etc.).
struct QSelfFinder {
    found: Option<String>,
}

impl<'ast> Visit<'ast> for QSelfFinder {
    fn visit_type_path(&mut self, type_path: &'ast syn::TypePath) {
        if self.found.is_some() {
            return;
        }
        if type_path.qself.is_some() {
            let projected = type_path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            self.found = Some(format!("<...>::{projected}"));
            return;
        }
        visit::visit_type_path(self, type_path);
    }
}

fn find_qself_in_type(ty: &syn::Type) -> Option<String> {
    let mut finder = QSelfFinder { found: None };
    finder.visit_type(ty);
    finder.found
}

fn find_qself_in_return(output: &syn::ReturnType) -> Option<String> {
    let syn::ReturnType::Type(_, ty) = output else {
        return None;
    };
    find_qself_in_type(ty)
}

/// Enforces the closed allowlist of safe non-`Deref` return types on
/// `BufferManagerMut` and `EditRuntimeMut`. Any unlisted nominal type
/// (such as a sibling-module `DerefMut` wrapper `super::SneakyDeref<'_>` or
/// a cross-file alias `MutRef<'_, BufferManager>`) fails closed.
fn check_allowed_safe_return_type(
    ty: &syn::Type,
    method_type_params: &BTreeSet<String>,
) -> Option<String> {
    match ty {
        syn::Type::Tuple(tuple) => {
            for elem in &tuple.elems {
                if let Some(bad) = check_allowed_safe_return_type(elem, method_type_params) {
                    return Some(bad);
                }
            }
            None
        }
        syn::Type::Ptr(ptr) => {
            if matches!(ptr.mutability, syn::PointerMutability::Mut(_))
                && type_last_ident(&ptr.elem).as_deref() == Some("MarkerObj")
            {
                None
            } else {
                Some("unallowlisted raw pointer return".to_string())
            }
        }
        syn::Type::ImplTrait(_) => {
            // Validated separately by `OpaqueReturnFinder`.
            None
        }
        syn::Type::Path(tp) => {
            if tp.qself.is_some() {
                // Handled by `find_qself_in_return` before this check.
                return Some("QSelf projection".to_string());
            }
            let Some(last) = tp.path.segments.last() else {
                return Some("empty type path".to_string());
            };
            let ident = last.ident.to_string();
            if !ALLOWED_SAFE_VIEW_RETURN_TYPES.contains(&ident.as_str())
                && !method_type_params.contains(&ident)
            {
                let full = tp
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");
                return Some(full);
            }
            if let syn::PathArguments::AngleBracketed(args) = &last.arguments {
                for arg in &args.args {
                    if let syn::GenericArgument::Type(inner) = arg
                        && let Some(bad) = check_allowed_safe_return_type(inner, method_type_params)
                    {
                        return Some(bad);
                    }
                }
            }
            None
        }
        syn::Type::Group(g) => check_allowed_safe_return_type(&g.elem, method_type_params),
        syn::Type::Paren(p) => check_allowed_safe_return_type(&p.elem, method_type_params),
        syn::Type::Macro(_) => {
            // Guarded by `visit_type_macro` (`GuardedTypeMacro`).
            None
        }
        _ => Some("unallowlisted non-nominal return type".to_string()),
    }
}

fn find_unallowlisted_safe_return_type(
    output: &syn::ReturnType,
    method_type_params: &BTreeSet<String>,
) -> Option<String> {
    let syn::ReturnType::Type(_, ty) = output else {
        return None;
    };
    check_allowed_safe_return_type(ty, method_type_params)
}

/// Checks whether a return type contains an opaque `impl Trait` or `dyn Trait`
/// that could hide `&mut` / `DerefMut` / `AsMut` access. Only the single
/// audited `impl Iterator<Item = BufferId>` shape (`buffer_list_cursor`) is
/// permitted on `BufferManagerMut`.
struct OpaqueReturnFinder {
    found: Option<String>,
}

fn is_std_deref_trait_path(path: &syn::Path) -> bool {
    let idents: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    let refs: Vec<&str> = idents.iter().map(String::as_str).collect();
    matches!(
        refs.as_slice(),
        ["Deref"] | ["core", "ops", "Deref"] | ["std", "ops", "Deref"]
    )
}

fn is_allowed_deref_target_type(assoc_ty: &syn::ImplItemType, self_name: &str) -> bool {
    let expected = match self_name {
        "BufferManagerMut" => "BufferManager",
        "EditRuntimeMut" => "EditRuntime",
        _ => return false,
    };
    if assoc_ty.ident != "Target"
        || !assoc_ty.generics.params.is_empty()
        || assoc_ty.generics.where_clause.is_some()
    {
        return false;
    }
    let syn::Type::Path(tp) = &assoc_ty.ty else {
        return false;
    };
    if tp.qself.is_some() {
        return false;
    }
    let Some(last) = tp.path.segments.last() else {
        return false;
    };
    last.ident == expected && matches!(last.arguments, syn::PathArguments::None)
}

fn is_allowed_deref_method(sig: &syn::Signature, self_name: &str) -> bool {
    let expected_target = match self_name {
        "BufferManagerMut" => "BufferManager",
        "EditRuntimeMut" => "EditRuntime",
        _ => return false,
    };
    let Some(syn::FnArg::Receiver(recv)) = sig.inputs.first() else {
        return false;
    };
    if sig.inputs.len() != 1
        || !matches!(recv.kind, syn::ReceiverKind::Reference(_, _, None))
        || recv.mutability.is_some()
    {
        return false;
    }
    let syn::ReturnType::Type(_, ret_ty) = &sig.output else {
        return false;
    };
    let syn::Type::Reference(ref_ty) = &**ret_ty else {
        return false;
    };
    if ref_ty.mutability.is_some() {
        return false;
    }
    let syn::Type::Path(tp) = &*ref_ty.elem else {
        return false;
    };
    if tp.qself.is_some() {
        return false;
    }
    let idents: Vec<String> = tp
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    let refs: Vec<&str> = idents.iter().map(String::as_str).collect();
    refs.as_slice() == ["Self", "Target"] || refs.as_slice() == [expected_target]
}

fn is_std_iterator_trait_path(path: &syn::Path) -> bool {
    let idents: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    let refs: Vec<&str> = idents.iter().map(String::as_str).collect();
    matches!(
        refs.as_slice(),
        ["Iterator"] | ["core", "iter", "Iterator"] | ["std", "iter", "Iterator"]
    )
}

fn impl_trait_is_allowed_buffer_id_iterator(impl_trait: &syn::TypeImplTrait) -> bool {
    let mut saw_iterator = false;
    for bound in &impl_trait.bounds {
        match bound {
            syn::TypeParamBound::Trait(tb) => {
                if !is_std_iterator_trait_path(&tb.path) {
                    return false;
                }
                let Some(last) = tb.path.segments.last() else {
                    return false;
                };
                let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
                    return false;
                };
                if args.args.len() != 1 {
                    return false;
                }
                let Some(syn::GenericArgument::AssocType(assoc)) = args.args.first() else {
                    return false;
                };
                if assoc.ident != "Item"
                    || type_last_ident(&assoc.ty).as_deref() != Some("BufferId")
                {
                    return false;
                }
                saw_iterator = true;
            }
            syn::TypeParamBound::PreciseCapture(_) | syn::TypeParamBound::Lifetime(_) => {}
            _ => return false,
        }
    }
    saw_iterator
}

impl<'ast> Visit<'ast> for OpaqueReturnFinder {
    fn visit_type_impl_trait(&mut self, impl_trait: &'ast syn::TypeImplTrait) {
        if self.found.is_some() {
            return;
        }
        if !impl_trait_is_allowed_buffer_id_iterator(impl_trait) {
            self.found = Some("unallowlisted `impl Trait` return".to_string());
            return;
        }
        visit::visit_type_impl_trait(self, impl_trait);
    }

    fn visit_type_trait_object(&mut self, _obj: &'ast syn::TypeTraitObject) {
        if self.found.is_none() {
            self.found = Some("opaque `dyn Trait` return".to_string());
        }
    }
}

fn find_unallowlisted_opaque_return(output: &syn::ReturnType) -> Option<String> {
    let syn::ReturnType::Type(_, ty) = output else {
        return None;
    };
    let mut finder = OpaqueReturnFinder { found: None };
    finder.visit_type(ty);
    finder.found
}

/// Checks whether any callback parameter (`impl Fn*(&mut T)`, `dyn Fn*(&mut T)`,
/// bare `fn(&mut T)`, or generic `<F: Fn*(&mut T)>` / `where F: Fn*(&mut T)`)
/// hands a `&mut` reference to a caller callback.
struct CallbackArgMutTargetFinder<'a> {
    targets: Option<&'a [&'a str]>,
    found: Option<String>,
}

impl<'ast> Visit<'ast> for CallbackArgMutTargetFinder<'_> {
    fn visit_type_fn_ptr(&mut self, bare: &'ast syn::TypeFnPtr) {
        if self.found.is_some() {
            return;
        }
        for arg in &bare.inputs {
            let hit = match self.targets {
                Some(targets) => find_mut_target_in_type(&arg.ty, targets),
                None => find_any_mut_ref_in_type(&arg.ty),
            };
            if let Some(hit) = hit {
                self.found = Some(hit);
                return;
            }
        }
        visit::visit_type_fn_ptr(self, bare);
    }

    fn visit_parenthesized_generic_arguments(
        &mut self,
        args: &'ast syn::ParenthesizedGenericArguments,
    ) {
        if self.found.is_some() {
            return;
        }
        for input in &args.inputs {
            let hit = match self.targets {
                Some(targets) => find_mut_target_in_type(&input.ty, targets),
                None => find_any_mut_ref_in_type(&input.ty),
            };
            if let Some(hit) = hit {
                self.found = Some(hit);
                return;
            }
        }
        visit::visit_parenthesized_generic_arguments(self, args);
    }
}

fn find_mut_target_in_callbacks(sig: &syn::Signature, targets: &[&str]) -> Option<String> {
    let mut finder = CallbackArgMutTargetFinder {
        targets: Some(targets),
        found: None,
    };
    for input in &sig.inputs {
        if let syn::FnArg::Typed(pat_type) = input {
            finder.visit_type(&pat_type.ty);
            if finder.found.is_some() {
                return finder.found;
            }
        }
    }
    finder.visit_generics(&sig.generics);
    finder.found
}

/// Enforces a closed callback & trait-bound allowlist on safe non-`Deref`
/// methods of `BufferManagerMut` and `EditRuntimeMut`:
/// - Any bare `fn(...)` or `FnOnce`/`FnMut`/`Fn` callback argument mentioning
///   `&mut ...`, a `QSelf` projection, or any `FORBIDDEN_SAFE_MUT_TARGETS`
///   (`Buffer`, `BufferManager`, `EditRuntime`) fails closed.
/// - Any custom trait bound on an `impl Trait` / `dyn Trait` parameter or
///   generic type parameter / `where` predicate (such as `F: super::WithMgr`)
///   fails closed.
struct SafeViewCallbackAndBoundChecker {
    found: Option<String>,
}

impl SafeViewCallbackAndBoundChecker {
    fn check_callback_input_ty(&mut self, ty: &syn::Type) {
        if self.found.is_some() {
            return;
        }
        if let Some(mut_ref) = find_any_mut_ref_in_type(ty) {
            self.found = Some(mut_ref);
        } else if let Some(qself) = find_qself_in_type(ty) {
            self.found = Some(qself);
        } else if let Some(unallowlisted) = check_allowed_safe_return_type(ty, &BTreeSet::new()) {
            self.found = Some(format!("unallowlisted callback argument `{unallowlisted}`"));
        }
    }

    fn check_trait_bound(&mut self, tb: &syn::TraitBound) {
        if self.found.is_some() {
            return;
        }
        let Some(last) = tb.path.segments.last() else {
            self.found = Some("empty trait bound".to_string());
            return;
        };
        let ident = last.ident.to_string();
        if matches!(ident.as_str(), "FnOnce" | "FnMut" | "Fn") {
            if let syn::PathArguments::Parenthesized(args) = &last.arguments {
                for input in &args.inputs {
                    self.check_callback_input_ty(&input.ty);
                }
            } else {
                self.found = Some(format!("non-parenthesized `{ident}` bound"));
            }
        } else {
            let full = tb
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            self.found = Some(format!("unallowlisted trait bound `{full}`"));
        }
    }
}

impl<'ast> Visit<'ast> for SafeViewCallbackAndBoundChecker {
    fn visit_type_fn_ptr(&mut self, bare: &'ast syn::TypeFnPtr) {
        if self.found.is_some() {
            return;
        }
        for arg in &bare.inputs {
            self.check_callback_input_ty(&arg.ty);
        }
        visit::visit_type_fn_ptr(self, bare);
    }

    fn visit_type_impl_trait(&mut self, impl_trait: &'ast syn::TypeImplTrait) {
        if self.found.is_some() {
            return;
        }
        for bound in &impl_trait.bounds {
            if let syn::TypeParamBound::Trait(tb) = bound {
                self.check_trait_bound(tb);
            }
        }
    }

    fn visit_type_trait_object(&mut self, obj: &'ast syn::TypeTraitObject) {
        if self.found.is_some() {
            return;
        }
        for bound in &obj.bounds {
            if let syn::TypeParamBound::Trait(tb) = bound {
                self.check_trait_bound(tb);
            }
        }
    }

    fn visit_type_param(&mut self, param: &'ast syn::TypeParam) {
        if self.found.is_some() {
            return;
        }
        for bound in &param.bounds {
            if let syn::TypeParamBound::Trait(tb) = bound {
                self.check_trait_bound(tb);
            }
        }
    }

    fn visit_predicate_type(&mut self, pred: &'ast syn::PredicateType) {
        if self.found.is_some() {
            return;
        }
        for bound in &pred.bounds {
            if let syn::TypeParamBound::Trait(tb) = bound {
                self.check_trait_bound(tb);
            }
        }
    }
}

fn find_unsafe_callback_or_trait_bound(sig: &syn::Signature) -> Option<String> {
    let mut checker = SafeViewCallbackAndBoundChecker { found: None };
    for input in &sig.inputs {
        if let syn::FnArg::Typed(pat_type) = input {
            checker.visit_type(&pat_type.ty);
            if checker.found.is_some() {
                return checker.found;
            }
        }
    }
    checker.visit_generics(&sig.generics);
    checker.found
}

fn find_unsafe_callback_or_trait_bound_in_generics(generics: &syn::Generics) -> Option<String> {
    let mut checker = SafeViewCallbackAndBoundChecker { found: None };
    checker.visit_generics(generics);
    checker.found
}

/// Checks whether a type contains any `&mut ...` reference outside a bare
/// `fn(...)` pointer (catches `type MutRef<'a, T> = &'a mut T;` and
/// `type MutAssoc<'a, T: Tr> = &'a mut T::Out;` without false-positiving on
/// `fn(&mut Context)` or `PhantomData<fn(&mut ())>`).
struct NonFnMutRefFinder {
    found: Option<String>,
}

impl<'ast> Visit<'ast> for NonFnMutRefFinder {
    fn visit_type_fn_ptr(&mut self, _bare: &'ast syn::TypeFnPtr) {
        // Do not descend into bare `fn(...)` pointers.
    }

    fn visit_type_reference(&mut self, reference: &'ast syn::TypeReference) {
        if self.found.is_some() {
            return;
        }
        if reference.mutability.is_some() {
            let label = match &*reference.elem {
                syn::Type::Path(tp) => tp
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::"),
                _ => "<non-path>".to_string(),
            };
            self.found = Some(format!("&mut {label}"));
            return;
        }
        visit::visit_type_reference(self, reference);
    }
}

fn find_mut_ref_outside_bare_fn(ty: &syn::Type) -> Option<String> {
    let mut finder = NonFnMutRefFinder { found: None };
    finder.visit_type(ty);
    finder.found
}

fn macro_last_ident(mac: &syn::Macro) -> Option<String> {
    mac.path.segments.last().map(|s| s.ident.to_string())
}

#[derive(Default)]
struct Scanner {
    file: String,
    file_path: PathBuf,
    crate_src_root: PathBuf,
    scanned_paths: BTreeSet<PathBuf>,
    extra_path_targets: Vec<(String, PathBuf, PathBuf)>,
    inline_mod_stack: Vec<String>,
    in_guarded_scope: bool,
    current_guarded_method: Option<String>,
    violations: Vec<Violation>,
    buffer_manager_mut_impls: usize,
    edit_runtime_mut_impls: usize,
    buffer_manager_mut_unsafe_fns: Vec<(String, String, usize)>,
    edit_runtime_mut_unsafe_fns: Vec<(String, String, usize)>,
    escape_call_sites: Vec<(String, usize)>,
    raw_accessor_sites: Vec<(String, String, usize)>,
}

impl Scanner {
    fn is_view_module(&self) -> bool {
        self.file == VIEW_MODULE_REL
    }

    fn check_guarded_attrs(&mut self, attrs: &[syn::Attribute], context: &str) {
        for attr in attrs {
            if let Some(reason) = guarded_attribute_violation(attr) {
                let line = attr
                    .path()
                    .segments
                    .last()
                    .map(|s| s.ident.span().start().line)
                    .unwrap_or(0);
                self.violations.push(Violation {
                    kind: ViolationKind::GuardedUnknownAttribute,
                    file: self.file.clone(),
                    line,
                    detail: format!("{context}: {reason}"),
                });
            }
        }
    }

    fn record_raw_accessor_if_matching(&mut self, attrs: &[syn::Attribute], sig: &syn::Signature) {
        if has_cfg_test_or_test_support(attrs) {
            return;
        }
        if find_mut_target_in_return(&sig.output, RAW_ACCESSOR_TARGETS).is_some()
            || find_mut_target_in_callbacks(sig, RAW_ACCESSOR_TARGETS).is_some()
        {
            self.raw_accessor_sites.push((
                self.file.clone(),
                sig.ident.to_string(),
                sig.ident.span().start().line,
            ));
        }
    }

    fn check_include_macro(&mut self, mac: &syn::Macro) {
        if macro_last_ident(mac).as_deref() != Some("include") {
            return;
        }
        let tokens = mac.tokens.to_string();
        let allowed = ALLOWED_PRODUCTION_INCLUDE_MACROS
            .iter()
            .any(|(f, expected)| *f == self.file && *expected == tokens);
        if !allowed {
            let line = mac
                .path
                .segments
                .last()
                .map(|s| s.ident.span().start().line)
                .unwrap_or(0);
            self.violations.push(Violation {
                kind: ViolationKind::DisallowedIncludeMacro,
                file: self.file.clone(),
                line,
                detail: format!("unallowlisted `include!({tokens})` in production code"),
            });
        }
    }

    fn check_use_tree(&mut self, prefix: &mut Vec<String>, tree: &syn::UseTree) {
        match tree {
            syn::UseTree::Rename(rename) => {
                let original = rename.ident.to_string();
                let alias = rename.rename.to_string();
                if self.is_view_module()
                    || GUARDED_IMPORT_IDENTS.contains(&original.as_str())
                    || GUARDED_IMPORT_IDENTS.contains(&alias.as_str())
                {
                    self.violations.push(Violation {
                        kind: ViolationKind::RenamedGuardedImport,
                        file: self.file.clone(),
                        line: rename.ident.span().start().line,
                        detail: format!(
                            "renamed import `use ...::{original} as {alias}` is forbidden"
                        ),
                    });
                }
            }
            syn::UseTree::Glob(glob) => {
                if self.is_view_module() {
                    self.violations.push(Violation {
                        kind: ViolationKind::RenamedGuardedImport,
                        file: self.file.clone(),
                        line: glob.star_token.span.start().line,
                        detail: format!(
                            "glob import `use ...::*` is forbidden inside `{VIEW_MODULE_REL}` (prevents shadowing `static_assertions` / `Iterator` / `Deref`)"
                        ),
                    });
                }
            }
            syn::UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                self.check_use_tree(prefix, &path.tree);
                prefix.pop();
            }
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    self.check_use_tree(prefix, item);
                }
            }
            syn::UseTree::Name(name) => {
                if self.is_view_module() {
                    let ident = name.ident.to_string();
                    if matches!(
                        ident.as_str(),
                        "static_assertions" | "Iterator" | "Deref" | "include"
                    ) {
                        let mut full: Vec<&str> = prefix.iter().map(String::as_str).collect();
                        full.push(ident.as_str());
                        let is_allowed_std_import = matches!(
                            full.as_slice(),
                            ["std", "ops", "Deref"]
                                | ["core", "ops", "Deref"]
                                | ["std", "iter", "Iterator"]
                                | ["core", "iter", "Iterator"]
                        );
                        if !is_allowed_std_import {
                            self.violations.push(Violation {
                                kind: ViolationKind::RenamedGuardedImport,
                                file: self.file.clone(),
                                line: name.ident.span().start().line,
                                detail: format!(
                                    "import `use {}` inside `{VIEW_MODULE_REL}` shadows guarded item `{ident}` (only std/core Deref & Iterator are permitted)",
                                    full.join("::")
                                ),
                            });
                        }
                    }
                }
            }
        }
    }

    fn resolve_mod_path_attr(&self, raw_path: &str) -> PathBuf {
        let base_dir = self
            .file_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let mut dir = base_dir;
        for seg in &self.inline_mod_stack {
            dir.push(seg);
        }
        let candidate = normalize_lexical_path(&dir.join(raw_path));
        if self.scanned_paths.contains(&candidate) {
            return candidate;
        }
        if let Some(stem) = self.file_path.file_stem().and_then(|s| s.to_str())
            && !matches!(stem, "mod" | "lib" | "main")
        {
            let mut alt = self
                .file_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(stem);
            for seg in &self.inline_mod_stack {
                alt.push(seg);
            }
            let alt_candidate = normalize_lexical_path(&alt.join(raw_path));
            if self.scanned_paths.contains(&alt_candidate) {
                return alt_candidate;
            }
        }
        candidate
    }

    fn check_type_alias(&mut self, item_type: &syn::ItemType) {
        let alias_name = item_type.ident.to_string();
        let line = item_type.ident.span().start().line;
        if ALLOWED_RAW_MUT_TYPE_ALIASES
            .iter()
            .any(|(f, n)| *f == self.file && *n == alias_name)
        {
            return;
        }
        if let Some(target) = find_mut_target_in_type(&item_type.ty, FORBIDDEN_SAFE_MUT_TARGETS) {
            self.violations.push(Violation {
                kind: ViolationKind::TypeAliasEscape,
                file: self.file.clone(),
                line,
                detail: format!(
                    "type alias `{alias_name}` exposes `&mut {target}` (potential escape indirection)"
                ),
            });
            return;
        }
        if let Some(mentioned) = find_any_ident_in_type(&item_type.ty, ALL_GUARDED_TYPE_IDENTS) {
            self.violations.push(Violation {
                kind: ViolationKind::TypeAliasEscape,
                file: self.file.clone(),
                line,
                detail: format!(
                    "type alias `{alias_name}` mentions guarded type `{mentioned}` (forbidden indirection)"
                ),
            });
            return;
        }
        if item_type.generics.type_params().next().is_some()
            && let Some(mut_ref) = find_mut_ref_outside_bare_fn(&item_type.ty)
        {
            self.violations.push(Violation {
                kind: ViolationKind::TypeAliasEscape,
                file: self.file.clone(),
                line,
                detail: format!(
                    "generic production type alias `{alias_name}` abstracts `{mut_ref}` (workspace-wide generic `&mut` alias ban)"
                ),
            });
        }
    }

    /// Exhaustive closed allowlist of top-level `syn::Item` variants permitted
    /// inside `neovm-edit/src/buffer/manager_view.rs`. Any unlisted item kind
    /// (`Enum`, `Union`, `Trait`, `Fn`, `Const`, `Static`, `Type`, non-view
    /// `Struct`/`Impl`, non-`#[cfg(test)]` `Mod`, `ExternCrate`, `ForeignMod`,
    /// `TraitAlias`, `Verbatim`) fails closed with `UnscannedItemKind`.
    fn visit_view_module_top_level_item(&mut self, item: &syn::Item) {
        match item {
            syn::Item::Use(item_use) => self.visit_item_use(item_use),
            syn::Item::Struct(item_struct) => {
                if has_cfg_test(&item_struct.attrs) {
                    return;
                }
                if !VIEW_TYPES.contains(&item_struct.ident.to_string().as_str()) {
                    self.violations.push(Violation {
                        kind: ViolationKind::UnscannedItemKind,
                        file: self.file.clone(),
                        line: item_struct.ident.span().start().line,
                        detail: format!(
                            "non-view struct `{}` is not allowed in `{VIEW_MODULE_REL}` (closed item allowlist)",
                            item_struct.ident
                        ),
                    });
                    return;
                }
                self.visit_item_struct(item_struct);
            }
            syn::Item::Impl(item_impl) => {
                if has_cfg_test(&item_impl.attrs) {
                    return;
                }
                let view_name = type_last_ident(&item_impl.self_ty)
                    .filter(|name| VIEW_TYPES.contains(&name.as_str()));
                if view_name.is_none() {
                    self.violations.push(Violation {
                        kind: ViolationKind::UnscannedItemKind,
                        file: self.file.clone(),
                        line: item_impl.impl_token.span.start().line,
                        detail: format!(
                            "impl block targeting a non-view type is not allowed in `{VIEW_MODULE_REL}` (closed item allowlist)"
                        ),
                    });
                    return;
                }
                self.visit_item_impl(item_impl);
            }
            syn::Item::Macro(item_macro) => self.visit_item_macro(item_macro),
            syn::Item::Mod(item_mod) => {
                if has_cfg_test(&item_mod.attrs) {
                    return;
                }
                self.violations.push(Violation {
                    kind: ViolationKind::UnscannedItemKind,
                    file: self.file.clone(),
                    line: item_mod.ident.span().start().line,
                    detail: format!(
                        "non-#[cfg(test)] `mod {}` is not allowed in `{VIEW_MODULE_REL}` (closed item allowlist)",
                        item_mod.ident
                    ),
                });
            }
            syn::Item::Const(i) if has_cfg_test(&i.attrs) => {}
            syn::Item::Enum(i) if has_cfg_test(&i.attrs) => {}
            syn::Item::Fn(i) if has_cfg_test(&i.attrs) => {}
            syn::Item::Static(i) if has_cfg_test(&i.attrs) => {}
            syn::Item::Trait(i) if has_cfg_test(&i.attrs) => {}
            syn::Item::Type(i) if has_cfg_test(&i.attrs) => {}
            syn::Item::Union(i) if has_cfg_test(&i.attrs) => {}
            other => {
                let kind_label = match other {
                    syn::Item::Const(i) => format!("const {}", i.ident),
                    syn::Item::Enum(i) => format!("enum {}", i.ident),
                    syn::Item::ExternCrate(i) => format!("extern crate {}", i.ident),
                    syn::Item::Fn(i) => format!("fn {}", i.sig.ident),
                    syn::Item::ForeignMod(_) => "extern block".to_string(),
                    syn::Item::Static(i) => format!("static {}", i.ident),
                    syn::Item::Trait(i) => format!("trait {}", i.ident),
                    syn::Item::TraitAlias(i) => format!("trait alias {}", i.ident),
                    syn::Item::Type(i) => format!("type {}", i.ident),
                    syn::Item::Union(i) => format!("union {}", i.ident),
                    syn::Item::Verbatim(_) => "verbatim item".to_string(),
                    _ => "unrecognized item".to_string(),
                };
                self.violations.push(Violation {
                    kind: ViolationKind::UnscannedItemKind,
                    file: self.file.clone(),
                    line: 0,
                    detail: format!(
                        "`{kind_label}` is not allowed in `{VIEW_MODULE_REL}` (closed item allowlist)"
                    ),
                });
            }
        }
    }
}

impl<'ast> Visit<'ast> for Scanner {
    fn visit_file(&mut self, file: &'ast syn::File) {
        if self.is_view_module() {
            self.check_guarded_attrs(&file.attrs, "view module file");
            let prev_guarded = self.in_guarded_scope;
            self.in_guarded_scope = true;
            for item in &file.items {
                self.visit_view_module_top_level_item(item);
            }
            self.in_guarded_scope = prev_guarded;
            return;
        }
        visit::visit_file(self, file);
    }

    fn visit_item_struct(&mut self, item_struct: &'ast syn::ItemStruct) {
        if has_cfg_test(&item_struct.attrs) {
            return;
        }
        let is_view_struct = VIEW_TYPES.contains(&item_struct.ident.to_string().as_str());
        if self.is_view_module() || is_view_struct {
            self.check_guarded_attrs(&item_struct.attrs, &format!("struct {}", item_struct.ident));
        }
        if self.is_view_module() {
            for (idx, field) in item_struct.fields.iter().enumerate() {
                let field_label = field
                    .ident
                    .as_ref()
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| idx.to_string());
                self.check_guarded_attrs(
                    &field.attrs,
                    &format!("struct {}::{field_label}", item_struct.ident),
                );
                if !matches!(field.vis, syn::Visibility::Inherited) {
                    let line = field
                        .ident
                        .as_ref()
                        .map(|id| id.span().start().line)
                        .unwrap_or_else(|| item_struct.ident.span().start().line);
                    self.violations.push(Violation {
                        kind: ViolationKind::GuardedViewModuleEscape,
                        file: self.file.clone(),
                        line,
                        detail: format!(
                            "struct `{}::{field_label}` inside `{VIEW_MODULE_REL}` must have private visibility",
                            item_struct.ident
                        ),
                    });
                }
            }
        }
        visit::visit_item_struct(self, item_struct);
    }

    fn visit_item_enum(&mut self, item_enum: &'ast syn::ItemEnum) {
        if has_cfg_test(&item_enum.attrs) {
            return;
        }
        if self.is_view_module() {
            self.check_guarded_attrs(&item_enum.attrs, &format!("enum {}", item_enum.ident));
        }
        visit::visit_item_enum(self, item_enum);
    }

    fn visit_item_const(&mut self, item_const: &'ast syn::ItemConst) {
        if has_cfg_test(&item_const.attrs) {
            return;
        }
        let prev_guarded = self.in_guarded_scope;
        let prev_method = self.current_guarded_method.take();
        if self.is_view_module() {
            self.check_guarded_attrs(&item_const.attrs, &format!("const {}", item_const.ident));
            self.in_guarded_scope = true;
            self.current_guarded_method = Some(format!("const {}", item_const.ident));
        }
        visit::visit_item_const(self, item_const);
        self.in_guarded_scope = prev_guarded;
        self.current_guarded_method = prev_method;
    }

    fn visit_item_static(&mut self, item_static: &'ast syn::ItemStatic) {
        if has_cfg_test(&item_static.attrs) {
            return;
        }
        let prev_guarded = self.in_guarded_scope;
        let prev_method = self.current_guarded_method.take();
        if self.is_view_module() {
            self.check_guarded_attrs(&item_static.attrs, &format!("static {}", item_static.ident));
            self.in_guarded_scope = true;
            self.current_guarded_method = Some(format!("static {}", item_static.ident));
        }
        visit::visit_item_static(self, item_static);
        self.in_guarded_scope = prev_guarded;
        self.current_guarded_method = prev_method;
    }

    fn visit_item_mod(&mut self, item_mod: &'ast syn::ItemMod) {
        if has_cfg_test(&item_mod.attrs) {
            return;
        }
        if self.is_view_module() {
            self.check_guarded_attrs(&item_mod.attrs, &format!("mod {}", item_mod.ident));
        }
        for finding in extract_path_attrs(&item_mod.attrs) {
            match finding {
                PathAttrFinding::UnparseableOrDynamic { line, reason } => {
                    self.violations.push(Violation {
                        kind: ViolationKind::UnscannedPathAttribute,
                        file: self.file.clone(),
                        line,
                        detail: format!(
                            "production `mod {}`: {reason} (fail closed)",
                            item_mod.ident
                        ),
                    });
                }
                PathAttrFinding::Literal {
                    raw_path,
                    line,
                    via_cfg_attr,
                } => {
                    let resolved = self.resolve_mod_path_attr(&raw_path);
                    if via_cfg_attr || !self.scanned_paths.contains(&resolved) {
                        let is_test_support_fixture = !via_cfg_attr
                            && has_cfg_test_or_test_support(&item_mod.attrs)
                            && path_is_inside_crate_src_test_area(&self.crate_src_root, &resolved);
                        let is_allowlisted_facade = !via_cfg_attr
                            && ALLOWED_SCANNED_TEST_FACADE_PATHS
                                .iter()
                                .any(|(f, p)| *f == self.file && *p == raw_path);
                        if is_allowlisted_facade {
                            self.extra_path_targets.push((
                                format!("{} -> {raw_path}", self.file),
                                self.crate_src_root.clone(),
                                resolved,
                            ));
                        } else if !is_test_support_fixture {
                            self.violations.push(Violation {
                                kind: ViolationKind::UnscannedPathAttribute,
                                file: self.file.clone(),
                                line,
                                detail: format!(
                                    "production `{}` on `mod {}` resolves to `{}` which is not unconditionally scanned",
                                    if via_cfg_attr {
                                        format!("#[cfg_attr(..., path = \"{raw_path}\")]")
                                    } else {
                                        format!("#[path = \"{raw_path}\"]")
                                    },
                                    item_mod.ident,
                                    resolved.display()
                                ),
                            });
                        }
                    }
                }
            }
        }
        if item_mod.content.is_some() {
            self.inline_mod_stack.push(item_mod.ident.to_string());
            visit::visit_item_mod(self, item_mod);
            self.inline_mod_stack.pop();
        } else if self.is_view_module() {
            self.violations.push(Violation {
                kind: ViolationKind::UnscannedPathAttribute,
                file: self.file.clone(),
                line: item_mod.ident.span().start().line,
                detail: format!(
                    "out-of-line production `mod {};` inside `{VIEW_MODULE_REL}` is forbidden (could gain private field access outside the guarded file)",
                    item_mod.ident
                ),
            });
        }
    }

    fn visit_item_use(&mut self, item_use: &'ast syn::ItemUse) {
        if has_cfg_test(&item_use.attrs) {
            return;
        }
        if self.is_view_module() {
            self.check_guarded_attrs(&item_use.attrs, "use item");
        }
        let mut prefix = Vec::new();
        self.check_use_tree(&mut prefix, &item_use.tree);
        visit::visit_item_use(self, item_use);
    }

    fn visit_item_type(&mut self, item_type: &'ast syn::ItemType) {
        if has_cfg_test(&item_type.attrs) {
            return;
        }
        if self.is_view_module() {
            self.check_guarded_attrs(&item_type.attrs, &format!("type {}", item_type.ident));
        }
        self.check_type_alias(item_type);
        visit::visit_item_type(self, item_type);
    }

    fn visit_item_macro(&mut self, item_macro: &'ast syn::ItemMacro) {
        if has_cfg_test(&item_macro.attrs) {
            return;
        }
        self.check_include_macro(&item_macro.mac);
        if self.is_view_module() {
            self.check_guarded_attrs(&item_macro.attrs, "item macro");
            let idents: Vec<String> = item_macro
                .mac
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            let segs: Vec<&str> = idents.iter().map(String::as_str).collect();
            let is_allowed_static_assertion = matches!(
                segs.as_slice(),
                ["static_assertions", "assert_not_impl_any"]
            );
            if !is_allowed_static_assertion {
                let name = idents.join("::");
                let line = item_macro
                    .mac
                    .path
                    .segments
                    .last()
                    .map(|s| s.ident.span().start().line)
                    .unwrap_or(0);
                self.violations.push(Violation {
                    kind: ViolationKind::GuardedFileMacro,
                    file: self.file.clone(),
                    line,
                    detail: format!(
                        "unexpected item macro `{name}!` in guarded view module `{VIEW_MODULE_REL}` (only `static_assertions::assert_not_impl_any!` is permitted)"
                    ),
                });
            }
        }
        visit::visit_item_macro(self, item_macro);
    }

    fn visit_item_fn(&mut self, item_fn: &'ast syn::ItemFn) {
        if has_cfg_test(&item_fn.attrs) {
            return;
        }
        self.record_raw_accessor_if_matching(&item_fn.attrs, &item_fn.sig);
        visit::visit_item_fn(self, item_fn);
    }

    fn visit_item_trait(&mut self, item_trait: &'ast syn::ItemTrait) {
        if has_cfg_test(&item_trait.attrs) {
            return;
        }
        for item in &item_trait.items {
            match item {
                syn::TraitItem::Fn(method) => {
                    if has_cfg_test(&method.attrs) {
                        continue;
                    }
                    self.record_raw_accessor_if_matching(&method.attrs, &method.sig);
                    self.visit_trait_item_fn(method);
                }
                syn::TraitItem::Type(assoc_ty) => {
                    if has_cfg_test(&assoc_ty.attrs) {
                        continue;
                    }
                    let line = assoc_ty.ident.span().start().line;
                    if let Some((_, default_ty)) = &assoc_ty.default
                        && let Some(target) =
                            find_mut_target_in_type(default_ty, FORBIDDEN_SAFE_MUT_TARGETS)
                    {
                        self.violations.push(Violation {
                            kind: ViolationKind::AssocTypeMutEscape,
                            file: self.file.clone(),
                            line,
                            detail: format!(
                                "trait {}::{} default associated type exposes `&mut {target}`",
                                item_trait.ident, assoc_ty.ident
                            ),
                        });
                    }
                    for bound in &assoc_ty.bounds {
                        let mut finder = MutTargetFinder {
                            targets: FORBIDDEN_SAFE_MUT_TARGETS,
                            found: None,
                        };
                        finder.visit_type_param_bound(bound);
                        if let Some(target) = finder.found {
                            self.violations.push(Violation {
                                kind: ViolationKind::AssocTypeMutEscape,
                                file: self.file.clone(),
                                line,
                                detail: format!(
                                    "trait {}::{} associated type bound exposes `&mut {target}`",
                                    item_trait.ident, assoc_ty.ident
                                ),
                            });
                        }
                    }
                    self.visit_trait_item_type(assoc_ty);
                }
                syn::TraitItem::Macro(mac) => {
                    if has_cfg_test(&mac.attrs) {
                        continue;
                    }
                    self.check_include_macro(&mac.mac);
                    self.visit_trait_item_macro(mac);
                }
                other => self.visit_trait_item(other),
            }
        }
    }

    fn visit_item_impl(&mut self, item_impl: &'ast syn::ItemImpl) {
        if has_cfg_test(&item_impl.attrs) {
            return;
        }
        let view_name =
            type_last_ident(&item_impl.self_ty).filter(|name| VIEW_TYPES.contains(&name.as_str()));
        let trait_ident = item_impl
            .trait_
            .as_ref()
            .and_then(|(trait_path, _)| trait_path.segments.last().map(|s| s.ident.to_string()));
        let is_deref_impl = item_impl
            .trait_
            .as_ref()
            .is_some_and(|(trait_path, _)| is_std_deref_trait_path(trait_path));
        let is_guarded_impl = view_name.is_some() || self.is_view_module();

        if is_guarded_impl {
            self.check_guarded_attrs(
                &item_impl.attrs,
                &format!(
                    "impl {}",
                    view_name.as_deref().unwrap_or("<view-module-impl>")
                ),
            );
        }

        if let Some(self_name) = &view_name {
            match self_name.as_str() {
                "BufferManagerMut" => self.buffer_manager_mut_impls += 1,
                "EditRuntimeMut" => self.edit_runtime_mut_impls += 1,
                _ => {}
            }

            if let Some(trait_name) = &trait_ident
                && FORBIDDEN_VIEW_MUT_TRAITS.contains(&trait_name.as_str())
            {
                self.violations.push(Violation {
                    kind: ViolationKind::ForbiddenMutTraitImpl,
                    file: self.file.clone(),
                    line: item_impl.impl_token.span.start().line,
                    detail: format!("{self_name} must not implement {trait_name}"),
                });
            }
        }

        if self.is_view_module() && view_name.is_none() {
            self.violations.push(Violation {
                kind: ViolationKind::GuardedViewModuleEscape,
                file: self.file.clone(),
                line: item_impl.impl_token.span.start().line,
                detail: format!(
                    "impl block inside `{VIEW_MODULE_REL}` must target BufferManagerMut or EditRuntimeMut (prevents inverted `impl From<BufferManagerMut> for &mut BufferManager` escapes)"
                ),
            });
        }

        let prev_guarded = self.in_guarded_scope;
        if is_guarded_impl {
            self.in_guarded_scope = true;
            self.visit_generics(&item_impl.generics);
            if let Some(bad_bound) =
                find_unsafe_callback_or_trait_bound_in_generics(&item_impl.generics)
            {
                self.violations.push(Violation {
                    kind: ViolationKind::SafeCallbackEscape,
                    file: self.file.clone(),
                    line: item_impl.impl_token.span.start().line,
                    detail: format!(
                        "guarded impl block exposes `{bad_bound}` via impl-level generic bound"
                    ),
                });
            }
            if let Some((trait_path, _)) = &item_impl.trait_ {
                self.visit_path(trait_path);
            }
        }

        // Visit each impl item manually so `#[cfg(test)]` methods and their
        // bodies are genuinely skipped (never falling through to
        // `visit::visit_item_impl` which visits every child regardless of cfg).
        for item in &item_impl.items {
            match item {
                syn::ImplItem::Fn(method) => {
                    if has_cfg_test(&method.attrs) {
                        continue;
                    }
                    if is_guarded_impl {
                        self.check_guarded_attrs(
                            &method.attrs,
                            &format!(
                                "{}::{}",
                                view_name.as_deref().unwrap_or("<view-module-impl>"),
                                method.sig.ident
                            ),
                        );
                    }
                    self.record_raw_accessor_if_matching(&method.attrs, &method.sig);

                    if let Some(self_name) = &view_name {
                        let method_name = method.sig.ident.to_string();
                        let line = method.sig.ident.span().start().line;

                        if FORBIDDEN_UNWRAP_METHODS.contains(&method_name.as_str()) {
                            self.violations.push(Violation {
                                kind: ViolationKind::ForbiddenUnwrapMethod,
                                file: self.file.clone(),
                                line,
                                detail: format!(
                                    "{self_name}::{method_name} exposes raw mutable unwrap (forbidden)"
                                ),
                            });
                        }

                        if matches!(method.sig.safety, syn::Safety::Unsafe(_)) {
                            match self_name.as_str() {
                                "BufferManagerMut" => {
                                    self.buffer_manager_mut_unsafe_fns.push((
                                        self.file.clone(),
                                        method_name.clone(),
                                        line,
                                    ));
                                    if method_name != ALLOWED_ELEMENT_ESCAPE {
                                        self.violations.push(Violation {
                                            kind: ViolationKind::UnexpectedUnsafeMethod,
                                            file: self.file.clone(),
                                            line,
                                            detail: format!(
                                                "unexpected unsafe fn {self_name}::{method_name}; only `{ALLOWED_ELEMENT_ESCAPE}` is permitted"
                                            ),
                                        });
                                    }
                                }
                                "EditRuntimeMut" => {
                                    self.edit_runtime_mut_unsafe_fns.push((
                                        self.file.clone(),
                                        method_name.clone(),
                                        line,
                                    ));
                                    self.violations.push(Violation {
                                        kind: ViolationKind::UnexpectedUnsafeMethod,
                                        file: self.file.clone(),
                                        line,
                                        detail: format!(
                                            "EditRuntimeMut must have zero unsafe methods; found `{method_name}`"
                                        ),
                                    });
                                }
                                _ => {}
                            }
                        } else {
                            // Fail closed on ANY `&mut ...` return in safe view methods
                            // (catches `&mut Buffer`, `&mut BufferManager`, `&mut Self::Target`,
                            // `&mut <Self as Deref>::Target`, `&mut T`, etc.).
                            if let Some(mut_ref) = find_any_mut_ref_in_return(&method.sig.output) {
                                self.violations.push(Violation {
                                    kind: ViolationKind::SafeMutReturnEscape,
                                    file: self.file.clone(),
                                    line,
                                    detail: format!(
                                        "safe method {self_name}::{method_name} returns `{mut_ref}`"
                                    ),
                                });
                            } else if let Some(qself) = find_qself_in_return(&method.sig.output) {
                                self.violations.push(Violation {
                                    kind: ViolationKind::SafeQSelfReturnEscape,
                                    file: self.file.clone(),
                                    line,
                                    detail: format!(
                                        "safe method {self_name}::{method_name} returns QSelf projection `{qself}`"
                                    ),
                                });
                            } else if let Some(opaque) =
                                find_unallowlisted_opaque_return(&method.sig.output)
                            {
                                self.violations.push(Violation {
                                    kind: ViolationKind::SafeOpaqueReturnEscape,
                                    file: self.file.clone(),
                                    line,
                                    detail: format!(
                                        "safe method {self_name}::{method_name} returns {opaque}"
                                    ),
                                });
                            } else if is_deref_impl && method_name == "deref" {
                                if !is_allowed_deref_method(&method.sig, self_name) {
                                    self.violations.push(Violation {
                                        kind: ViolationKind::SafeUnallowlistedReturn,
                                        file: self.file.clone(),
                                        line,
                                        detail: format!(
                                            "Deref::deref on {self_name} must take `&self` and return `&Self::Target` (or `&BufferManager` / `&EditRuntime`)"
                                        ),
                                    });
                                }
                            } else {
                                let method_type_params: BTreeSet<String> = method
                                    .sig
                                    .generics
                                    .type_params()
                                    .map(|tp| tp.ident.to_string())
                                    .collect();
                                if let Some(unallowlisted) = find_unallowlisted_safe_return_type(
                                    &method.sig.output,
                                    &method_type_params,
                                ) {
                                    self.violations.push(Violation {
                                        kind: ViolationKind::SafeUnallowlistedReturn,
                                        file: self.file.clone(),
                                        line,
                                        detail: format!(
                                            "safe method {self_name}::{method_name} returns unallowlisted type `{unallowlisted}` (closed return allowlist)"
                                        ),
                                    });
                                }
                            }
                            if let Some(bad_cb) = find_unsafe_callback_or_trait_bound(&method.sig) {
                                self.violations.push(Violation {
                                    kind: ViolationKind::SafeCallbackEscape,
                                    file: self.file.clone(),
                                    line,
                                    detail: format!(
                                        "safe method {self_name}::{method_name} exposes `{bad_cb}` via callback/bound"
                                    ),
                                });
                            }
                        }
                    }

                    let prev_method = self.current_guarded_method.take();
                    if is_guarded_impl {
                        self.current_guarded_method = Some(method.sig.ident.to_string());
                    }
                    self.visit_impl_item_fn(method);
                    self.current_guarded_method = prev_method;
                }
                syn::ImplItem::Type(assoc_ty) => {
                    if has_cfg_test(&assoc_ty.attrs) {
                        continue;
                    }
                    let line = assoc_ty.ident.span().start().line;
                    if is_guarded_impl {
                        self.check_guarded_attrs(
                            &assoc_ty.attrs,
                            &format!("impl type {}", assoc_ty.ident),
                        );
                        let is_allowed = is_deref_impl
                            && view_name
                                .as_deref()
                                .is_some_and(|vn| is_allowed_deref_target_type(assoc_ty, vn));
                        if !is_allowed {
                            if let Some(target) =
                                find_mut_target_in_type(&assoc_ty.ty, FORBIDDEN_SAFE_MUT_TARGETS)
                            {
                                self.violations.push(Violation {
                                    kind: ViolationKind::AssocTypeMutEscape,
                                    file: self.file.clone(),
                                    line,
                                    detail: format!(
                                        "associated type `{}` exposes `&mut {target}` (indirection escape)",
                                        assoc_ty.ident
                                    ),
                                });
                            } else {
                                self.violations.push(Violation {
                                    kind: ViolationKind::UnscannedItemKind,
                                    file: self.file.clone(),
                                    line,
                                    detail: format!(
                                        "associated `type {}` is not allowed in guarded impl block (only `impl Deref {{ type Target = BufferManager | EditRuntime }}` is permitted)",
                                        assoc_ty.ident
                                    ),
                                });
                            }
                            continue;
                        }
                    } else if let Some(target) =
                        find_mut_target_in_type(&assoc_ty.ty, FORBIDDEN_SAFE_MUT_TARGETS)
                    {
                        self.violations.push(Violation {
                            kind: ViolationKind::AssocTypeMutEscape,
                            file: self.file.clone(),
                            line,
                            detail: format!(
                                "associated type `{}` exposes `&mut {target}` (indirection escape)",
                                assoc_ty.ident
                            ),
                        });
                    }
                    self.visit_impl_item_type(assoc_ty);
                }
                syn::ImplItem::Macro(mac) => {
                    if has_cfg_test(&mac.attrs) {
                        continue;
                    }
                    self.check_include_macro(&mac.mac);
                    if is_guarded_impl {
                        let name = macro_last_ident(&mac.mac).unwrap_or_default();
                        let line = mac
                            .mac
                            .path
                            .segments
                            .last()
                            .map(|s| s.ident.span().start().line)
                            .unwrap_or(0);
                        self.violations.push(Violation {
                            kind: ViolationKind::GuardedImplMacro,
                            file: self.file.clone(),
                            line,
                            detail: format!(
                                "macro invocation `{name}!` inside guarded impl block is forbidden (fail closed)"
                            ),
                        });
                    }
                    self.visit_impl_item_macro(mac);
                }
                syn::ImplItem::Const(assoc_const) => {
                    if has_cfg_test(&assoc_const.attrs) {
                        continue;
                    }
                    if is_guarded_impl {
                        self.violations.push(Violation {
                            kind: ViolationKind::UnscannedItemKind,
                            file: self.file.clone(),
                            line: assoc_const.ident.span().start().line,
                            detail: format!(
                                "associated `const {}` is not allowed in guarded impl block (closed ImplItem allowlist)",
                                assoc_const.ident
                            ),
                        });
                        continue;
                    }
                    self.visit_impl_item_const(assoc_const);
                }
                other => {
                    if is_guarded_impl {
                        self.violations.push(Violation {
                            kind: ViolationKind::UnscannedItemKind,
                            file: self.file.clone(),
                            line: item_impl.impl_token.span.start().line,
                            detail: "unrecognized ImplItem variant is not allowed in guarded impl block (closed ImplItem allowlist)".to_string(),
                        });
                        continue;
                    }
                    self.visit_impl_item(other);
                }
            }
        }

        self.in_guarded_scope = prev_guarded;
    }

    fn visit_stmt(&mut self, stmt: &'ast syn::Stmt) {
        if self.in_guarded_scope {
            match stmt {
                syn::Stmt::Local(local) => {
                    self.check_guarded_attrs(&local.attrs, "guarded local stmt");
                }
                syn::Stmt::Item(item) => {
                    let is_cfg_test_item = match item {
                        syn::Item::Fn(i) => has_cfg_test(&i.attrs),
                        syn::Item::Impl(i) => has_cfg_test(&i.attrs),
                        syn::Item::Mod(i) => has_cfg_test(&i.attrs),
                        syn::Item::Macro(i) => has_cfg_test(&i.attrs),
                        syn::Item::Type(i) => has_cfg_test(&i.attrs),
                        syn::Item::Use(i) => has_cfg_test(&i.attrs),
                        syn::Item::Struct(i) => has_cfg_test(&i.attrs),
                        syn::Item::Trait(i) => has_cfg_test(&i.attrs),
                        _ => false,
                    };
                    if !is_cfg_test_item {
                        self.violations.push(Violation {
                            kind: ViolationKind::GuardedFnBodyItem,
                            file: self.file.clone(),
                            line: 0,
                            detail: format!(
                                "nested item inside guarded `{}` body is forbidden (fail closed on Stmt::Item)",
                                self.current_guarded_method.as_deref().unwrap_or("<fn>")
                            ),
                        });
                    }
                }
                syn::Stmt::Macro(stmt_mac) => {
                    if !has_cfg_test(&stmt_mac.attrs) {
                        self.check_guarded_attrs(&stmt_mac.attrs, "guarded stmt macro");
                        self.check_include_macro(&stmt_mac.mac);
                        let name = macro_last_ident(&stmt_mac.mac).unwrap_or_default();
                        let allowed_debug_assert = self.current_guarded_method.as_deref()
                            == Some(ALLOWED_ELEMENT_ESCAPE)
                            && matches!(name.as_str(), "debug_assert" | "debug_assert_eq");
                        if !allowed_debug_assert {
                            let line = stmt_mac
                                .mac
                                .path
                                .segments
                                .last()
                                .map(|s| s.ident.span().start().line)
                                .unwrap_or(0);
                            self.violations.push(Violation {
                                kind: ViolationKind::GuardedFnBodyMacro,
                                file: self.file.clone(),
                                line,
                                detail: format!(
                                    "macro `{name}!` inside guarded `{}` body is forbidden (fail closed on Stmt::Macro)",
                                    self.current_guarded_method.as_deref().unwrap_or("<fn>")
                                ),
                            });
                        }
                    }
                }
                _ => {}
            }
        } else if let syn::Stmt::Macro(stmt_mac) = stmt
            && !has_cfg_test(&stmt_mac.attrs)
        {
            self.check_include_macro(&stmt_mac.mac);
        }
        visit::visit_stmt(self, stmt);
    }

    fn visit_expr_macro(&mut self, expr_macro: &'ast syn::ExprMacro) {
        if has_cfg_test(&expr_macro.attrs) {
            return;
        }
        self.check_include_macro(&expr_macro.mac);
        if self.in_guarded_scope {
            let name = macro_last_ident(&expr_macro.mac).unwrap_or_default();
            let line = expr_macro
                .mac
                .path
                .segments
                .last()
                .map(|s| s.ident.span().start().line)
                .unwrap_or(0);
            self.violations.push(Violation {
                kind: ViolationKind::GuardedExprMacro,
                file: self.file.clone(),
                line,
                detail: format!(
                    "expression macro `{name}!` inside guarded `{}` is forbidden (fail closed on Expr::Macro)",
                    self.current_guarded_method.as_deref().unwrap_or("<guarded>")
                ),
            });
        }
        visit::visit_expr_macro(self, expr_macro);
    }

    fn visit_type_macro(&mut self, type_macro: &'ast syn::TypeMacro) {
        self.check_include_macro(&type_macro.mac);
        if self.in_guarded_scope || self.is_view_module() {
            let name = macro_last_ident(&type_macro.mac).unwrap_or_default();
            let line = type_macro
                .mac
                .path
                .segments
                .last()
                .map(|s| s.ident.span().start().line)
                .unwrap_or(0);
            self.violations.push(Violation {
                kind: ViolationKind::GuardedTypeMacro,
                file: self.file.clone(),
                line,
                detail: format!(
                    "type macro `{name}!` inside guarded scope is forbidden (fail closed on Type::Macro)"
                ),
            });
        }
        visit::visit_type_macro(self, type_macro);
    }

    fn visit_pat(&mut self, pat: &'ast syn::Pat) {
        if let syn::Pat::Macro(pat_macro) = pat {
            if has_cfg_test(&pat_macro.attrs) {
                return;
            }
            self.check_include_macro(&pat_macro.mac);
            if self.in_guarded_scope {
                let name = macro_last_ident(&pat_macro.mac).unwrap_or_default();
                let line = pat_macro
                    .mac
                    .path
                    .segments
                    .last()
                    .map(|s| s.ident.span().start().line)
                    .unwrap_or(0);
                self.violations.push(Violation {
                    kind: ViolationKind::GuardedPatMacro,
                    file: self.file.clone(),
                    line,
                    detail: format!(
                        "pattern macro `{name}!` inside guarded scope is forbidden (fail closed on Pat::Macro)"
                    ),
                });
            }
            visit::visit_macro(self, &pat_macro.mac);
            return;
        }
        visit::visit_pat(self, pat);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if call.method == ALLOWED_ELEMENT_ESCAPE {
            self.escape_call_sites
                .push((self.file.clone(), call.method.span().start().line));
        }
        visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        if path
            .path
            .segments
            .last()
            .is_some_and(|seg| seg.ident == ALLOWED_ELEMENT_ESCAPE)
        {
            let line = path
                .path
                .segments
                .last()
                .map(|seg| seg.ident.span().start().line)
                .unwrap_or(0);
            self.escape_call_sites.push((self.file.clone(), line));
        }
        visit::visit_expr_path(self, path);
    }
}

fn scan_synthetic(file: &str, source: &str) -> Scanner {
    let ast = syn::parse_file(source).expect("parse synthetic");
    let file_path = normalize_lexical_path(Path::new(file));
    let crate_src_root = file_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let mut scanned_paths = BTreeSet::new();
    scanned_paths.insert(file_path.clone());
    let mut scanner = Scanner {
        file: file.to_string(),
        file_path,
        crate_src_root,
        scanned_paths,
        ..Scanner::default()
    };
    scanner.visit_file(&ast);
    scanner
}

#[test]
fn buffer_manager_view_surface_and_escape_ratchet() {
    let plan = workspace_production_source_files();
    let root = crates_dir();
    let mut scanned_crates = BTreeSet::new();
    let scanned_paths: BTreeSet<PathBuf> = plan.files.iter().map(|(_, _, p)| p.clone()).collect();

    // Bidirectional crate-list completeness:
    // 1. Every entry in EXCLUDED_WORKSPACE_CRATES must actually exist under crates/.
    for excluded in EXCLUDED_WORKSPACE_CRATES {
        assert!(
            plan.excluded_crates_present.contains(*excluded),
            "stale EXCLUDED_WORKSPACE_CRATES entry `{excluded}` does not exist under crates/"
        );
    }

    let mut scanner = Scanner {
        scanned_paths,
        ..Scanner::default()
    };

    for (crate_name, workspace_rel, path) in plan.files {
        let crate_src_root = normalize_lexical_path(&root.join(&crate_name).join("src"));
        scanned_crates.insert(crate_name);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("read {workspace_rel}: {err}"));
        let ast =
            syn::parse_file(&text).unwrap_or_else(|err| panic!("parse {workspace_rel}: {err}"));
        scanner.file = workspace_rel;
        scanner.file_path = path;
        scanner.crate_src_root = crate_src_root;
        scanner.inline_mod_stack.clear();
        scanner.in_guarded_scope = false;
        scanner.current_guarded_method = None;
        scanner.visit_file(&ast);
    }

    // Also parse and scan any allowlisted unconditional `#[path = "..."]`
    // facade targets with their own crate's `src/` root so they cannot hide
    // production items either.
    let extra_targets = std::mem::take(&mut scanner.extra_path_targets);
    for (label, target_crate_src_root, target_path) in extra_targets {
        let text = std::fs::read_to_string(&target_path)
            .unwrap_or_else(|err| panic!("read allowlisted path target {label}: {err}"));
        let ast = syn::parse_file(&text)
            .unwrap_or_else(|err| panic!("parse allowlisted path target {label}: {err}"));
        scanner.file = label;
        scanner.file_path = target_path;
        scanner.crate_src_root = target_crate_src_root;
        scanner.inline_mod_stack.clear();
        scanner.in_guarded_scope = false;
        scanner.current_guarded_method = None;
        scanner.visit_file(&ast);
    }

    // 2. Every crate under crates/ with a src/ dir must be either scanned or
    //    explicitly listed in EXCLUDED_WORKSPACE_CRATES.
    for crate_name in &plan.discovered_crates_with_src {
        let accounted = scanned_crates.contains(crate_name)
            || EXCLUDED_WORKSPACE_CRATES.contains(&crate_name.as_str());
        assert!(
            accounted,
            "crate `{crate_name}` has a src/ directory but was neither scanned nor explicitly excluded"
        );
    }

    for required in REQUIRED_UPPER_CRATES {
        assert!(
            scanned_crates.contains(*required),
            "workspace scan missed required upper crate `{required}`; scanned: {scanned_crates:?}"
        );
    }
    assert!(
        scanner.buffer_manager_mut_impls >= MIN_BUFFER_MANAGER_MUT_IMPLS,
        "sanity floor failed: saw {} BufferManagerMut impls (expected >= {MIN_BUFFER_MANAGER_MUT_IMPLS})",
        scanner.buffer_manager_mut_impls
    );
    assert!(
        scanner.edit_runtime_mut_impls >= MIN_EDIT_RUNTIME_MUT_IMPLS,
        "sanity floor failed: saw {} EditRuntimeMut impls (expected >= {MIN_EDIT_RUNTIME_MUT_IMPLS})",
        scanner.edit_runtime_mut_impls
    );
    assert!(
        scanner.buffer_manager_mut_unsafe_fns.len() <= 1,
        "BufferManagerMut may expose at most one unsafe element escape; found {:?}",
        scanner.buffer_manager_mut_unsafe_fns
    );
    assert!(
        scanner.edit_runtime_mut_unsafe_fns.is_empty(),
        "EditRuntimeMut must expose zero unsafe methods; found {:?}",
        scanner.edit_runtime_mut_unsafe_fns
    );
    assert!(
        scanner.violations.is_empty(),
        "BufferManagerMut / EditRuntimeMut capability violations:\n{}",
        scanner
            .violations
            .iter()
            .map(|v| format!("  {}:{}: [{:?}] {}", v.file, v.line, v.kind, v.detail))
            .collect::<Vec<_>>()
            .join("\n")
    );

    let actual_calls = scanner.escape_call_sites.len();
    assert!(
        actual_calls <= MAX_BUFFER_MUT_NO_SWAP_PRODUCTION_CALLS,
        "{ALLOWED_ELEMENT_ESCAPE} production call count increased: found {actual_calls} > budget {MAX_BUFFER_MUT_NO_SWAP_PRODUCTION_CALLS}.\nSites:\n{}",
        scanner
            .escape_call_sites
            .iter()
            .map(|(file, line)| format!("  {file}:{line}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert_eq!(
        actual_calls, MAX_BUFFER_MUT_NO_SWAP_PRODUCTION_CALLS,
        "{ALLOWED_ELEMENT_ESCAPE} production call count shrank from {MAX_BUFFER_MUT_NO_SWAP_PRODUCTION_CALLS} to {actual_calls}: tighten MAX_BUFFER_MUT_NO_SWAP_PRODUCTION_CALLS"
    );

    let mut actual_raw: Vec<(String, String)> = scanner
        .raw_accessor_sites
        .iter()
        .map(|(file, name, _)| (file.clone(), name.clone()))
        .collect();
    actual_raw.sort();

    let mut expected_raw: Vec<(String, String)> = ALLOWED_RAW_BUFFER_OR_EDIT_MUT_ACCESSORS
        .iter()
        .map(|(file, name)| ((*file).to_string(), (*name).to_string()))
        .collect();
    expected_raw.sort();

    let unexpected: Vec<_> = scanner
        .raw_accessor_sites
        .iter()
        .filter(|(file, name, _)| {
            !ALLOWED_RAW_BUFFER_OR_EDIT_MUT_ACCESSORS
                .iter()
                .any(|(af, an)| af == file && an == name)
        })
        .collect();
    assert!(
        unexpected.is_empty(),
        "new raw `&mut BufferManager` / `&mut EditRuntime` accessors are forbidden (use BufferManagerMut / EditRuntimeMut instead):\n{}",
        unexpected
            .iter()
            .map(|(file, name, line)| format!("  {file}:{line}: fn {name}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert_eq!(
        actual_raw, expected_raw,
        "raw `&mut BufferManager` / `&mut EditRuntime` accessor inventory changed (shrink-only ratchet): tighten ALLOWED_RAW_BUFFER_OR_EDIT_MUT_ACCESSORS when legacy accessors are removed"
    );
}

#[test]
fn ratchet_scanner_catches_cfg_not_test_callbacks_mut_traits_and_new_raw_accessors() {
    let synthetic = r#"
        #[cfg(not(test))]
        impl std::ops::DerefMut for BufferManagerMut<'_> {
            fn deref_mut(&mut self) -> &mut BufferManager { loop {} }
        }

        #[cfg(not(any(test, feature = "test-support")))]
        fn prod_escape_caller(mut view: BufferManagerMut<'_>, id: BufferId) {
            unsafe { let _ = view.with_buffer_mut_no_swap(id, |_| ()); }
        }

        impl AsMut<BufferManager> for BufferManagerMut<'_> {
            fn as_mut(&mut self) -> &mut BufferManager { loop {} }
        }

        impl BorrowMut<EditRuntime> for EditRuntimeMut<'_> {
            fn borrow_mut(&mut self) -> &mut EditRuntime { loop {} }
        }

        impl IndexMut<BufferId> for BufferManagerMut<'_> {
            fn index_mut(&mut self, _id: BufferId) -> &mut Self::Output { loop {} }
        }

        impl BufferManagerMut<'_> {
            pub fn with_buf_cb(&mut self, _f: impl FnOnce(&mut Buffer)) {}
            pub fn with_mgr_generic<F>(&mut self, _f: F)
            where
                F: FnMut(&mut BufferManager),
            {}
        }

        impl Context {
            pub fn extra_buffers_mut(&mut self) -> &mut BufferManager { loop {} }
        }

        trait ExtraHost {
            fn extra_edit_mut(&mut self) -> &mut EditRuntime;
        }

        #[cfg(test)]
        mod skipped_tests {
            impl std::ops::DerefMut for BufferManagerMut<'_> {
                fn deref_mut(&mut self) -> &mut BufferManager { loop {} }
            }
            fn test_escape(mut view: BufferManagerMut<'_>, id: BufferId) {
                unsafe { let _ = view.with_buffer_mut_no_swap(id, |_| ()); }
            }
            fn test_raw_accessor(ctx: &mut Context) -> &mut BufferManager { loop {} }
        }
    "#;

    let scanner = scan_synthetic("synthetic/src/lib.rs", synthetic);

    let details: Vec<&str> = scanner
        .violations
        .iter()
        .map(|v| v.detail.as_str())
        .collect();
    assert!(
        details
            .iter()
            .any(|d| d.contains("BufferManagerMut must not implement DerefMut")),
        "cfg(not(test)) DerefMut must be caught: {details:?}"
    );
    assert!(
        details
            .iter()
            .any(|d| d.contains("BufferManagerMut must not implement AsMut")),
        "AsMut must be caught: {details:?}"
    );
    assert!(
        details
            .iter()
            .any(|d| d.contains("EditRuntimeMut must not implement BorrowMut")),
        "BorrowMut must be caught: {details:?}"
    );
    assert!(
        details
            .iter()
            .any(|d| d.contains("BufferManagerMut must not implement IndexMut")),
        "IndexMut must be caught: {details:?}"
    );
    assert!(
        details
            .iter()
            .any(|d| d.contains("with_buf_cb") && d.contains("callback")),
        "impl FnOnce(&mut Buffer) callback escape must be caught: {details:?}"
    );
    assert!(
        details
            .iter()
            .any(|d| d.contains("with_mgr_generic") && d.contains("callback")),
        "where F: FnMut(&mut BufferManager) callback escape must be caught: {details:?}"
    );
    assert_eq!(
        scanner.escape_call_sites.len(),
        1,
        "cfg(not(any(test, ...))) call must be counted while #[cfg(test)] call is skipped: {:?}",
        scanner.escape_call_sites
    );

    let raw_names: Vec<&str> = scanner
        .raw_accessor_sites
        .iter()
        .map(|(_, name, _)| name.as_str())
        .collect();
    assert!(
        raw_names.contains(&"extra_buffers_mut"),
        "new Context raw &mut BufferManager accessor must be caught: {raw_names:?}"
    );
    assert!(
        raw_names.contains(&"extra_edit_mut"),
        "new trait raw &mut EditRuntime accessor must be caught: {raw_names:?}"
    );
    assert!(
        !raw_names.contains(&"test_raw_accessor"),
        "#[cfg(test)] items must be skipped: {raw_names:?}"
    );
}

// --- Dedicated single-violation negative rows (#14785 + #14932 + #15008) ---
// Each row asserts `violations.len() == 1` and the exact `ViolationKind`, so
// removing the specific check turns that exact test red without masking.

#[test]
fn negative_assoc_type_impl_mut_buffer_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        trait CustomEscape<'a> {
            type Slot;
        }
        impl<'a> CustomEscape<'a> for BufferManagerMut<'a> {
            type Slot = &'a mut Buffer;
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one AssocTypeMutEscape violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::AssocTypeMutEscape
    );
    assert!(
        scanner.violations[0].detail.contains("Slot")
            && scanner.violations[0].detail.contains("&mut Buffer"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_assoc_type_trait_default_mut_manager_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        trait DefaultEscape {
            type MgrSlot = &'static mut BufferManager;
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one AssocTypeMutEscape violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::AssocTypeMutEscape
    );
    assert!(
        scanner.violations[0].detail.contains("MgrSlot")
            && scanner.violations[0].detail.contains("&mut BufferManager"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_module_type_alias_mut_buffer_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        type BufSlot<'a> = &'a mut Buffer;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one TypeAliasEscape violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::TypeAliasEscape);
    assert!(
        scanner.violations[0].detail.contains("BufSlot")
            && scanner.violations[0].detail.contains("&mut Buffer"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_module_type_alias_to_buffer_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        type Buf = Buffer;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one TypeAliasEscape violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::TypeAliasEscape);
    assert!(
        scanner.violations[0].detail.contains("Buf")
            && scanner.violations[0].detail.contains("Buffer"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_module_type_alias_nested_buffer_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        type OptBuf = Option<Buffer>;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one TypeAliasEscape violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::TypeAliasEscape);
    assert!(
        scanner.violations[0].detail.contains("OptBuf")
            && scanner.violations[0].detail.contains("Buffer"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_workspace_generic_mut_type_alias_is_rejected() {
    // #15197 required row 6: workspace-wide generic `&mut` type alias outside
    // `manager_view.rs`.
    let scanner = scan_synthetic(
        "synthetic/src/alias_def.rs",
        r#"
        pub type MutRef<'a, T> = &'a mut T;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one TypeAliasEscape violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::TypeAliasEscape);
    assert!(
        scanner.violations[0].detail.contains("MutRef")
            && scanner.violations[0].detail.contains("&mut T"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_module_type_alias_to_view_type_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        type HiddenView<'a> = BufferManagerMut<'a>;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one TypeAliasEscape violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::TypeAliasEscape);
    assert!(
        scanner.violations[0].detail.contains("HiddenView")
            && scanner.violations[0].detail.contains("BufferManagerMut"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_safe_method_mut_self_target_return_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        impl BufferManagerMut<'_> {
            pub fn leak(&mut self) -> &mut Self::Target {
                loop {}
            }
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one SafeMutReturnEscape violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::SafeMutReturnEscape
    );
    assert!(
        scanner.violations[0].detail.contains("leak")
            && scanner.violations[0].detail.contains("Self::Target"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_safe_method_mut_qself_assoc_return_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        impl BufferManagerMut<'_> {
            pub fn leak(&mut self) -> &mut <Self as std::ops::Deref>::Target {
                loop {}
            }
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one SafeMutReturnEscape violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::SafeMutReturnEscape
    );
    assert!(
        scanner.violations[0].detail.contains("leak")
            && scanner.violations[0].detail.contains("Target"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_safe_method_opaque_impl_trait_return_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        impl BufferManagerMut<'_> {
            pub fn leak(&mut self) -> impl CustomOpaqueCap {
                loop {}
            }
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one SafeOpaqueReturnEscape violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::SafeOpaqueReturnEscape
    );
    assert!(
        scanner.violations[0].detail.contains("leak")
            && scanner.violations[0].detail.contains("impl Trait"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_renamed_guarded_import_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        use neovm_edit::buffer::BufferManagerMut as AliasedView;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one RenamedGuardedImport violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::RenamedGuardedImport
    );
    assert!(
        scanner.violations[0].detail.contains("BufferManagerMut")
            && scanner.violations[0].detail.contains("AliasedView"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_guarded_impl_macro_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        impl BufferManagerMut<'_> {
            generate_hidden_escape!();
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one GuardedImplMacro violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::GuardedImplMacro);
    assert!(
        scanner.violations[0]
            .detail
            .contains("generate_hidden_escape"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_view_module_trait_is_rejected() {
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        pub trait ViewExt {
            fn leak_buf<'a>(v: &'a mut BufferManagerMut<'_>, id: BufferId) -> Option<&'a mut Buffer> {
                v.manager.get_mut(id)
            }
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one UnscannedItemKind violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::UnscannedItemKind);
    assert!(
        scanner.violations[0].detail.contains("trait ViewExt"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_guarded_fn_body_stmt_item_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        impl BufferManagerMut<'_> {
            pub fn innocuous(&mut self) {
                macro_rules! hidden { () => {} }
            }
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one GuardedFnBodyItem violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::GuardedFnBodyItem);
    assert!(
        scanner.violations[0].detail.contains("Stmt::Item")
            && scanner.violations[0].detail.contains("innocuous"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_guarded_fn_body_stmt_macro_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        impl BufferManagerMut<'_> {
            pub fn innocuous(&mut self) {
                hidden_stmt_macro!();
            }
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one GuardedFnBodyMacro violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::GuardedFnBodyMacro
    );
    assert!(
        scanner.violations[0].detail.contains("hidden_stmt_macro")
            && scanner.violations[0].detail.contains("Stmt::Macro"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_guarded_expr_macro_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        impl BufferManagerMut<'_> {
            pub fn innocuous(&mut self) {
                let _ = hidden_expr_macro!();
            }
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one GuardedExprMacro violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::GuardedExprMacro);
    assert!(
        scanner.violations[0].detail.contains("hidden_expr_macro")
            && scanner.violations[0].detail.contains("Expr::Macro"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_guarded_type_macro_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        impl BufferManagerMut<'_> {
            pub fn leak(&mut self) -> hide!(BufferManager) {
                loop {}
            }
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one GuardedTypeMacro violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::GuardedTypeMacro);
    assert!(
        scanner.violations[0].detail.contains("hide")
            && scanner.violations[0].detail.contains("Type::Macro"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_guarded_pat_macro_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        impl BufferManagerMut<'_> {
            pub fn innocuous(&mut self) {
                let hidden_pat!() = 0;
            }
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one GuardedPatMacro violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::GuardedPatMacro);
    assert!(
        scanner.violations[0].detail.contains("hidden_pat")
            && scanner.violations[0].detail.contains("Pat::Macro"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_view_module_const_item_is_rejected() {
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        pub const LEAK_MGR: for<'a> fn(&'a mut BufferManagerMut<'_>) -> &'a mut BufferManager =
            |v| v.manager;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one UnscannedItemKind violation for const item, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::UnscannedItemKind);
    assert!(
        scanner.violations[0].detail.contains("const LEAK_MGR"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_view_module_static_item_is_rejected() {
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        pub static LEAK_MGR: for<'a> fn(&'a mut BufferManagerMut<'_>) -> &'a mut BufferManager =
            |v| v.manager;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one UnscannedItemKind violation for static item, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::UnscannedItemKind);
    assert!(
        scanner.violations[0].detail.contains("static LEAK_MGR"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_guarded_file_item_macro_is_rejected() {
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        cfg_if::cfg_if! {
            if #[cfg(unix)] {}
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one GuardedFileMacro violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::GuardedFileMacro);
    assert!(
        scanner.violations[0].detail.contains("cfg_if"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_guarded_unknown_attribute_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        #[some_proc_macro]
        impl BufferManagerMut<'_> {}
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one GuardedUnknownAttribute violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::GuardedUnknownAttribute
    );
    assert!(
        scanner.violations[0].detail.contains("some_proc_macro"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_guarded_forbidden_derive_is_rejected() {
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        #[derive(DerefMut)]
        pub struct BufferManagerMut<'a>(&'a mut BufferManager);
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one GuardedUnknownAttribute violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::GuardedUnknownAttribute
    );
    assert!(
        scanner.violations[0].detail.contains("DerefMut"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_unscanned_path_attribute_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        #[path = "../outside_src_escape.rs"]
        mod outside;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one UnscannedPathAttribute violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::UnscannedPathAttribute
    );
    assert!(
        scanner.violations[0]
            .detail
            .contains("outside_src_escape.rs"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_cfg_not_test_path_into_tests_dir_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        #[cfg(not(test))]
        #[path = "tests/sneaky_prod.rs"]
        mod sneaky;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one UnscannedPathAttribute violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::UnscannedPathAttribute
    );
    assert!(
        scanner.violations[0]
            .detail
            .contains("tests/sneaky_prod.rs"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_cfg_attr_path_attribute_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        #[cfg_attr(unix, path = "other.rs")]
        mod swapped;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one UnscannedPathAttribute violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::UnscannedPathAttribute
    );
    assert!(
        scanner.violations[0].detail.contains("cfg_attr")
            && scanner.violations[0].detail.contains("other.rs"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_non_literal_path_attribute_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        #[path = concat!("../", "escape.rs")]
        mod dynamic_path;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one UnscannedPathAttribute violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::UnscannedPathAttribute
    );
    assert!(
        scanner.violations[0].detail.contains("non-literal"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_cfg_attr_non_literal_path_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        #[cfg_attr(unix, path = concat!("a", "b"))]
        mod dynamic_cfg_attr;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one UnscannedPathAttribute violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::UnscannedPathAttribute
    );
    assert!(
        scanner.violations[0].detail.contains("non-literal"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_unparseable_cfg_attr_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        #[cfg_attr(unix, 123)]
        mod malformed_cfg_attr;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one UnscannedPathAttribute violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::UnscannedPathAttribute
    );
    assert!(
        scanner.violations[0].detail.contains("unparseable"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_disallowed_include_macro_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        include!("generated_escape.rs");
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one DisallowedIncludeMacro violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::DisallowedIncludeMacro
    );
    assert!(
        scanner.violations[0].detail.contains("generated_escape.rs"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_walk_production_files_errors_on_unreadable_directory() {
    let unique = format!(
        "neomacs-walk-err-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    );
    let base = std::env::temp_dir().join(unique);
    let missing = base.join("does_not_exist");
    let mut out = Vec::new();
    let missing_err = walk_production_files("synthetic", &base, &missing, &mut out);
    assert!(
        missing_err.is_err(),
        "walk_production_files must return Err on a missing/unreadable directory instead of silently returning Ok"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        struct DirGuard(PathBuf);
        impl Drop for DirGuard {
            fn drop(&mut self) {
                let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700));
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let locked_root = base.join("locked_root");
        let locked_sub = locked_root.join("unreadable_sub");
        std::fs::create_dir_all(&locked_sub).expect("create temp sub");
        let _guard = DirGuard(locked_sub.clone());
        std::fs::set_permissions(&locked_sub, std::fs::Permissions::from_mode(0o000))
            .expect("chmod 000");
        // Only assert permission-denied when not running as root (uid 0 ignores 0o000).
        if std::fs::read_dir(&locked_sub).is_err() {
            let perm_err = walk_production_files("synthetic", &locked_root, &locked_root, &mut out);
            assert!(
                perm_err.is_err(),
                "walk_production_files must return Err on an unreadable subdirectory"
            );
        }
        drop(_guard);
        let _ = std::fs::remove_dir_all(&base);
    }
}

#[test]
fn negative_visit_item_impl_skips_cfg_test_method_body_only() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        struct Demo;
        impl Demo {
            #[cfg(test)]
            fn test_only_helper(mut view: BufferManagerMut<'_>, id: BufferId) {
                unsafe { let _ = view.with_buffer_mut_no_swap(id, |_| ()); }
            }

            fn production_helper(mut view: BufferManagerMut<'_>, id: BufferId) {
                unsafe { let _ = view.with_buffer_mut_no_swap(id, |_| ()); }
            }
        }
        "#,
    );
    assert!(
        scanner.violations.is_empty(),
        "expected zero violations, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.escape_call_sites.len(),
        1,
        "visit_item_impl must skip #[cfg(test)] method bodies while still counting production method bodies: {:?}",
        scanner.escape_call_sites
    );
}

// --- Closed-allowlist & adversarial bypass regression rows (#15097 / #15190 / #15197) ---

#[test]
fn negative_safe_method_cross_file_type_alias_return_is_rejected() {
    // Safe method returning cross-file alias `MutRef<'_, BufferManager>` fails
    // closed against the closed return-type allowlist.
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        impl<'a> BufferManagerMut<'a> {
            pub fn leak(&mut self) -> MutRef<'_, BufferManager> {
                loop {}
            }
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one SafeUnallowlistedReturn violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::SafeUnallowlistedReturn
    );
    assert!(
        scanner.violations[0].detail.contains("MutRef"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_safe_method_qself_blanket_assoc_return_is_rejected() {
    // Revert-sensitive QSelf projection row (#15190): uses `<Self as ...>::Out`
    // (no forbidden target ident) and asserts `SafeQSelfReturnEscape`, so
    // deleting `find_qself_in_return` turns this row red directly.
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        impl<'a> BufferManagerMut<'a> {
            pub fn leak(&mut self) -> <Self as super::MutOf<'_>>::Out {
                loop {}
            }
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one SafeQSelfReturnEscape violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::SafeQSelfReturnEscape
    );
    assert!(
        scanner.violations[0].detail.contains("MutOf"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_guarded_impl_const_fn_ptr_is_rejected() {
    // #15197 required row 1: associated `pub const` fn-pointer on `impl BufferManagerMut`.
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        impl BufferManagerMut<'_> {
            pub const LEAK_MGR: for<'a> fn(&'a mut BufferManagerMut<'_>) -> &'a mut BufferManager =
                |v| v.manager;
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one UnscannedItemKind violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::UnscannedItemKind);
    assert!(
        scanner.violations[0].detail.contains("LEAK_MGR"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_view_module_pub_enum_wrapper_is_rejected() {
    // #15197 required row 2: `pub enum` wrapper with `&mut BufferManager` variant.
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        pub enum Leak<'a> {
            Raw(&'a mut BufferManager),
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one UnscannedItemKind violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::UnscannedItemKind);
    assert!(
        scanner.violations[0].detail.contains("enum Leak"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_view_module_pub_union_is_rejected() {
    // #15197 required row 3: `pub union` (also with arbitrary proc-macro attr).
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        #[some_proc_macro]
        pub union Leak<'a> {
            pub m: std::mem::ManuallyDrop<&'a mut BufferManager>,
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one UnscannedItemKind violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::UnscannedItemKind);
    assert!(
        scanner.violations[0].detail.contains("union Leak"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_safe_method_sibling_deref_wrapper_return_is_rejected() {
    // #15197 required row 4: sibling-module `Deref`/`DerefMut` newtype launder.
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        impl<'a> BufferManagerMut<'a> {
            pub fn into_deref(&mut self) -> super::SneakyDeref<'_> {
                loop {}
            }
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one SafeUnallowlistedReturn violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::SafeUnallowlistedReturn
    );
    assert!(
        scanner.violations[0].detail.contains("SneakyDeref"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_safe_method_custom_trait_bound_callback_is_rejected() {
    // #15197 required row 5: callback with a custom trait bound (`F: super::WithMgr`)
    // instead of a syntactic `&mut`.
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        impl<'a> BufferManagerMut<'a> {
            pub fn run<F: super::WithMgr>(&mut self, _f: F) {}
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one SafeCallbackEscape violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::SafeCallbackEscape
    );
    assert!(
        scanner.violations[0].detail.contains("WithMgr"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_safe_method_indirect_callback_arg_is_rejected() {
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        impl<'a> BufferManagerMut<'a> {
            pub fn with_mgr(&mut self, _f: impl FnOnce(MutRef<'_, BufferManager>)) {}
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one SafeCallbackEscape violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::SafeCallbackEscape
    );
    assert!(
        scanner.violations[0].detail.contains("MutRef"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_view_module_inverted_from_impl_is_rejected() {
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        impl<'a> From<BufferManagerMut<'a>> for &'a mut BufferManager {
            fn from(view: BufferManagerMut<'a>) -> Self {
                view.manager
            }
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one UnscannedItemKind violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::UnscannedItemKind);
    assert!(
        scanner.violations[0].detail.contains("non-view type"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_view_module_free_pub_fn_is_rejected() {
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        pub fn leak_buffer<'a>(view: &'a mut BufferManagerMut<'_>, id: BufferId) -> Option<&'a mut Buffer> {
            view.manager.get_mut(id)
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one UnscannedItemKind violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::UnscannedItemKind);
    assert!(
        scanner.violations[0].detail.contains("fn leak_buffer"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_view_module_non_view_wrapper_struct_is_rejected() {
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        pub struct Leak<'a>(pub &'a mut BufferManager);
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one UnscannedItemKind violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::UnscannedItemKind);
    assert!(
        scanner.violations[0]
            .detail
            .contains("non-view struct `Leak`"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_view_module_pub_field_on_view_struct_is_rejected() {
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        pub struct BufferManagerMut<'a> {
            pub manager: &'a mut BufferManager,
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one GuardedViewModuleEscape violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::GuardedViewModuleEscape
    );
    assert!(
        scanner.violations[0]
            .detail
            .contains("BufferManagerMut::manager"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_view_module_out_of_line_mod_is_rejected() {
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        pub mod test_escape;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one UnscannedItemKind violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::UnscannedItemKind);
    assert!(
        scanner.violations[0].detail.contains("mod test_escape"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_spoofed_assert_not_impl_any_macro_is_rejected() {
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        super::assert_not_impl_any!();
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one GuardedFileMacro violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::GuardedFileMacro);
    assert!(
        scanner.violations[0]
            .detail
            .contains("super::assert_not_impl_any"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_view_module_use_rename_static_assertions_is_rejected() {
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        use super::fake_sa as static_assertions;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one RenamedGuardedImport violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::RenamedGuardedImport
    );
    assert!(
        scanner.violations[0].detail.contains("static_assertions"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_view_module_glob_import_is_rejected() {
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        use super::*;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one RenamedGuardedImport violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::RenamedGuardedImport
    );
    assert!(
        scanner.violations[0].detail.contains("glob import"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_renamed_include_import_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        use core::include as inc;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one RenamedGuardedImport violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::RenamedGuardedImport
    );
    assert!(
        scanner.violations[0].detail.contains("include"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_type_macro_include_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        fn sneaky() -> include!("mut_type.inc") {
            loop {}
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one DisallowedIncludeMacro violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::DisallowedIncludeMacro
    );
    assert!(
        scanner.violations[0].detail.contains("mut_type.inc"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_view_module_top_level_type_macro_is_rejected() {
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        pub struct BufferManagerMut<'a>(hide!(BufferManager));
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one GuardedTypeMacro violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::GuardedTypeMacro);
    assert!(
        scanner.violations[0].detail.contains("hide"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_guarded_impl_generics_type_macro_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        impl<T = hide!(u8)> BufferManagerMut<'_> {}
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one GuardedTypeMacro violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::GuardedTypeMacro);
    assert!(
        scanner.violations[0].detail.contains("hide"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_guarded_impl_trait_path_type_macro_is_rejected() {
    let scanner = scan_synthetic(
        "synthetic/src/lib.rs",
        r#"
        impl Marker<hide!(u8)> for BufferManagerMut<'_> {}
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one GuardedTypeMacro violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::GuardedTypeMacro);
    assert!(
        scanner.violations[0].detail.contains("hide"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_spoofed_iterator_opaque_return_is_rejected() {
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        impl<'a> BufferManagerMut<'a> {
            pub fn leak(&mut self) -> impl super::Iterator<Item = BufferId> + '_ {
                loop {}
            }
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one SafeOpaqueReturnEscape violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::SafeOpaqueReturnEscape
    );
    assert!(
        scanner.violations[0].detail.contains("impl Trait"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

// --- Round-4 closed ImplItem::Type & shadowing regression rows (#15274) ---

#[test]
fn negative_guarded_impl_non_deref_assoc_type_is_rejected() {
    // Attack A (#15274): `impl super::CustomTrait for BufferManagerMut<'_> { type Assoc = &'static mut Self::Target; }`
    // must fail closed with `UnscannedItemKind` (only `impl Deref { type Target = BufferManager | EditRuntime }` is allowed).
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        impl super::CustomTrait for BufferManagerMut<'_> {
            type Assoc = &'static mut Self::Target;
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one UnscannedItemKind violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::UnscannedItemKind);
    assert!(
        scanner.violations[0].detail.contains("type Assoc"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_spoofed_deref_impl_bypass_is_rejected() {
    // Attack B (#15274): `impl super::Deref for BufferManagerMut<'_> { fn deref(&mut self) -> super::SneakyDeref<'_> }`
    // must not match `is_deref_impl` and must fail closed on the return allowlist.
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        impl super::Deref for BufferManagerMut<'_> {
            fn deref(&mut self) -> super::SneakyDeref<'_> {
                loop {}
            }
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one SafeUnallowlistedReturn violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::SafeUnallowlistedReturn
    );
    assert!(
        scanner.violations[0].detail.contains("SneakyDeref"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_guarded_impl_level_custom_trait_bound_is_rejected() {
    // Attack C (#15274): `impl<'a, F: super::WithMgr> super::RunWith<F> for BufferManagerMut<'a>`
    // must fail closed on impl-level generic bounds, not only method-level bounds.
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        impl<'a, F: super::WithMgr> super::RunWith<F> for BufferManagerMut<'a> {
            fn run(&mut self, _f: F) {}
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one SafeCallbackEscape violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::SafeCallbackEscape
    );
    assert!(
        scanner.violations[0].detail.contains("WithMgr")
            && scanner.violations[0].detail.contains("impl-level"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_safe_method_callback_unallowlisted_wrapper_arg_is_rejected() {
    // Attack D (#15274): `pub fn with_deref(&mut self, f: impl FnOnce(super::SneakyDeref<'_>))`
    // must fail closed against the closed callback argument allowlist.
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        impl<'a> BufferManagerMut<'a> {
            pub fn with_deref(&mut self, _f: impl FnOnce(super::SneakyDeref<'_>)) {}
        }
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one SafeCallbackEscape violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::SafeCallbackEscape
    );
    assert!(
        scanner.violations[0].detail.contains("SneakyDeref"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_view_module_name_shadowing_static_assertions_is_rejected() {
    // Attack E1 (#15274): `use super::static_assertions;` via `UseTree::Name`
    // inside `manager_view.rs` must fail closed.
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        use super::static_assertions;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one RenamedGuardedImport violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::RenamedGuardedImport
    );
    assert!(
        scanner.violations[0]
            .detail
            .contains("super::static_assertions"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_view_module_name_shadowing_iterator_is_rejected() {
    // Attack E2 (#15274): `use super::Iterator;` via `UseTree::Name` inside
    // `manager_view.rs` must fail closed.
    let scanner = scan_synthetic(
        VIEW_MODULE_REL,
        r#"
        use super::Iterator;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one RenamedGuardedImport violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(
        scanner.violations[0].kind,
        ViolationKind::RenamedGuardedImport
    );
    assert!(
        scanner.violations[0].detail.contains("super::Iterator"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_workspace_generic_assoc_mut_type_alias_is_rejected() {
    // Bonus check from r4 review: `pub type MutAssoc<'a, T: Tr> = &'a mut T::Out;`.
    let scanner = scan_synthetic(
        "synthetic/src/alias_def.rs",
        r#"
        pub type MutAssoc<'a, T: Tr> = &'a mut T::Out;
        "#,
    );
    assert_eq!(
        scanner.violations.len(),
        1,
        "expected exactly one TypeAliasEscape violation, got: {:?}",
        scanner.violations
    );
    assert_eq!(scanner.violations[0].kind, ViolationKind::TypeAliasEscape);
    assert!(
        scanner.violations[0].detail.contains("MutAssoc")
            && scanner.violations[0].detail.contains("&mut T::Out"),
        "unexpected detail: {}",
        scanner.violations[0].detail
    );
}

#[test]
fn negative_raw_accessor_cfg_not_test_support_is_caught_while_test_support_is_skipped() {
    // #844 Slice 2a: `#[cfg(any(test, feature = "test-support"))]` raw
    // accessors are unreachable in production builds and excluded from the
    // production allowlist, while `#[cfg(not(any(test, feature = "test-support")))]`
    // remains caught.
    let scanner = scan_synthetic(
        "synthetic/src/context_api.rs",
        r#"
        impl Context {
            #[cfg(any(test, feature = "test-support"))]
            pub fn test_only_buffers_mut(&mut self) -> &mut BufferManager {
                loop {}
            }

            #[cfg(not(any(test, feature = "test-support")))]
            pub fn prod_only_buffers_mut(&mut self) -> &mut BufferManager {
                loop {}
            }
        }
        "#,
    );
    let raw_names: Vec<&str> = scanner
        .raw_accessor_sites
        .iter()
        .map(|(_, name, _)| name.as_str())
        .collect();
    assert_eq!(
        raw_names,
        vec!["prod_only_buffers_mut"],
        "expected only prod_only_buffers_mut in raw_accessor_sites, got: {raw_names:?}"
    );
}

fn is_test_support_feature_spec(spec: &str) -> bool {
    spec == "test-support"
        || spec.ends_with("/test-support")
        || spec.starts_with("test-support/")
        || spec.starts_with("test-support?/")
}

fn check_features_value_for_test_support(
    context: &str,
    features_val: &toml::Value,
    violations: &mut Vec<String>,
) {
    let Some(arr) = features_val.as_array() else {
        violations.push(format!("{context}: `features` must be a TOML array"));
        return;
    };
    for entry in arr {
        let Some(spec) = entry.as_str() else {
            violations.push(format!("{context}: non-string entry in `features` array"));
            return;
        };
        if is_test_support_feature_spec(spec) {
            violations.push(format!("{context}: enables `{spec}`"));
        }
    }
}

fn check_dep_table_for_test_support(
    section_label: &str,
    deps: &toml::Table,
    violations: &mut Vec<String>,
) {
    for (dep_key, dep_val) in deps {
        let Some(dep_tbl) = dep_val.as_table() else {
            continue;
        };
        if let Some(features_val) = dep_tbl.get("features") {
            check_features_value_for_test_support(
                &format!("{section_label}.{dep_key}"),
                features_val,
                violations,
            );
        }
    }
}

fn check_patch_and_replace_tables(
    manifest_label: &str,
    table: &toml::Table,
    workspace_member_names: &BTreeSet<String>,
    violations: &mut Vec<String>,
) {
    if let Some(replace_val) = table.get("replace") {
        match replace_val.as_table() {
            Some(replace_tbl) if !replace_tbl.is_empty() => {
                violations.push(format!(
                    "{manifest_label}: `[replace]` section is forbidden in workspace manifests"
                ));
            }
            Some(_) => {}
            None => {
                violations.push(format!("{manifest_label}: `replace` must be a TOML table"));
            }
        }
    }

    if let Some(patch_val) = table.get("patch") {
        let Some(registries) = patch_val.as_table() else {
            violations.push(format!("{manifest_label}: `patch` must be a TOML table"));
            return;
        };
        for (registry_name, registry_val) in registries {
            let Some(entries) = registry_val.as_table() else {
                violations.push(format!(
                    "{manifest_label}: `[patch.{registry_name}]` must be a TOML table"
                ));
                continue;
            };
            for (patch_key, patch_spec) in entries {
                let patched_pkg = patch_spec
                    .as_table()
                    .and_then(|t| t.get("package"))
                    .and_then(toml::Value::as_str)
                    .unwrap_or(patch_key.as_str());
                if workspace_member_names.contains(patched_pkg) {
                    violations.push(format!(
                        "{manifest_label}: `[patch.{registry_name}.{patch_key}]` must not patch workspace crate `{patched_pkg}`"
                    ));
                }
                if let Some(spec_tbl) = patch_spec.as_table() {
                    if let Some(features_val) = spec_tbl.get("features") {
                        check_features_value_for_test_support(
                            &format!("{manifest_label}: [patch.{registry_name}.{patch_key}]"),
                            features_val,
                            violations,
                        );
                    }
                }
            }
        }
    }
}

fn audit_workspace_root_manifest(
    root_table: &toml::Table,
    workspace_member_names: &BTreeSet<String>,
) -> Result<(), Vec<String>> {
    let mut violations = Vec::new();
    let Some(ws_tbl) = root_table.get("workspace").and_then(toml::Value::as_table) else {
        return Err(vec![
            "root Cargo.toml: missing `[workspace]` table".to_string(),
        ]);
    };

    match ws_tbl.get("resolver").and_then(toml::Value::as_str) {
        Some("2" | "3") => {}
        other => violations.push(format!(
            "root Cargo.toml: `[workspace].resolver` must be \"2\" or \"3\" so dev-dependencies do not unify into production builds (got {other:?})"
        )),
    }

    if let Some(ws_deps_val) = ws_tbl.get("dependencies") {
        if let Some(ws_deps) = ws_deps_val.as_table() {
            check_dep_table_for_test_support(
                "root Cargo.toml [workspace.dependencies]",
                ws_deps,
                &mut violations,
            );
        } else {
            violations.push(
                "root Cargo.toml: `[workspace.dependencies]` must be a TOML table".to_string(),
            );
        }
    }

    check_patch_and_replace_tables(
        "root Cargo.toml",
        root_table,
        workspace_member_names,
        &mut violations,
    );

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

fn audit_member_manifest(
    crate_name: &str,
    table: &toml::Table,
    workspace_member_names: &BTreeSet<String>,
) -> Result<(), Vec<String>> {
    let mut violations = Vec::new();

    for section in ["dependencies", "build-dependencies"] {
        if let Some(val) = table.get(section) {
            if let Some(dep_tbl) = val.as_table() {
                check_dep_table_for_test_support(&format!("[{section}]"), dep_tbl, &mut violations);
            } else {
                violations.push(format!("[{section}] must be a TOML table"));
            }
        }
    }

    if let Some(target_val) = table.get("target") {
        if let Some(targets_tbl) = target_val.as_table() {
            for (cfg_expr, cfg_val) in targets_tbl {
                let Some(cfg_tbl) = cfg_val.as_table() else {
                    violations.push(format!("[target.{cfg_expr}] must be a TOML table"));
                    continue;
                };
                for section in ["dependencies", "build-dependencies"] {
                    if let Some(val) = cfg_tbl.get(section) {
                        if let Some(dep_tbl) = val.as_table() {
                            check_dep_table_for_test_support(
                                &format!("[target.{cfg_expr}.{section}]"),
                                dep_tbl,
                                &mut violations,
                            );
                        } else {
                            violations.push(format!(
                                "[target.{cfg_expr}.{section}] must be a TOML table"
                            ));
                        }
                    }
                }
            }
        } else {
            violations.push("[target] must be a TOML table".to_string());
        }
    }

    if let Some(features_val) = table.get("features") {
        if let Some(features_tbl) = features_val.as_table() {
            for (feature_name, val) in features_tbl {
                let Some(arr) = val.as_array() else {
                    violations.push(format!(
                        "[features].{feature_name} must be a TOML array of strings"
                    ));
                    continue;
                };
                let is_allowlisted_test_feature = feature_name == "test-support"
                    || (crate_name == "neovm-value" && feature_name == "test-thread-fallback-heap");
                for entry in arr {
                    let Some(spec) = entry.as_str() else {
                        violations.push(format!(
                            "[features].{feature_name} contains a non-string entry"
                        ));
                        continue;
                    };
                    if is_test_support_feature_spec(spec) && !is_allowlisted_test_feature {
                        violations.push(format!(
                            "non-test-support feature `[features].{feature_name}` enables `{spec}`"
                        ));
                    }
                }
            }
        } else {
            violations.push("[features] must be a TOML table".to_string());
        }
    }

    check_patch_and_replace_tables(crate_name, table, workspace_member_names, &mut violations);

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

fn glob_match_segment(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let mut dp = vec![vec![false; t.len() + 1]; p.len() + 1];
    dp[0][0] = true;
    for i in 1..=p.len() {
        if p[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }
    for i in 1..=p.len() {
        for j in 1..=t.len() {
            dp[i][j] = match p[i - 1] {
                '*' => dp[i - 1][j] || dp[i][j - 1],
                '?' => dp[i - 1][j - 1],
                c => dp[i - 1][j - 1] && c == t[j - 1],
            };
        }
    }
    dp[p.len()][t.len()]
}

fn expand_workspace_member_pattern(
    repo_root: &Path,
    pattern: &str,
) -> Result<Vec<PathBuf>, String> {
    let segments: Vec<&str> = pattern
        .split('/')
        .filter(|seg| !seg.is_empty() && *seg != ".")
        .collect();
    if segments.is_empty() {
        return Ok(vec![repo_root.to_path_buf()]);
    }
    let mut dirs = vec![repo_root.to_path_buf()];
    for seg in segments {
        let mut next = Vec::new();
        let has_glob = seg.contains('*') || seg.contains('?');
        for dir in dirs {
            if has_glob {
                let entries = std::fs::read_dir(&dir)
                    .map_err(|err| format!("read {}: {err}", dir.display()))?;
                for entry in entries {
                    let entry = entry.map_err(|err| format!("read {}: {err}", dir.display()))?;
                    let name = entry.file_name();
                    let Some(name_str) = name.to_str() else {
                        continue;
                    };
                    if glob_match_segment(seg, name_str) && entry.path().is_dir() {
                        next.push(entry.path());
                    }
                }
            } else {
                let candidate = dir.join(seg);
                if candidate.is_dir() {
                    next.push(candidate);
                }
            }
        }
        dirs = next;
    }
    Ok(dirs)
}

fn discover_workspace_member_manifests(
    repo_root: &Path,
    root_table: &toml::Table,
) -> Result<Vec<PathBuf>, String> {
    let ws_tbl = root_table
        .get("workspace")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| "root Cargo.toml missing `[workspace]` table".to_string())?;
    let members_arr = ws_tbl
        .get("members")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| "root Cargo.toml missing `[workspace].members` array".to_string())?;

    let mut excluded_dirs = BTreeSet::new();
    if let Some(exclude_val) = ws_tbl.get("exclude") {
        let exclude_arr = exclude_val
            .as_array()
            .ok_or_else(|| "root Cargo.toml `[workspace].exclude` must be an array".to_string())?;
        for entry in exclude_arr {
            let pat = entry.as_str().ok_or_else(|| {
                "root Cargo.toml `[workspace].exclude` entries must be strings".to_string()
            })?;
            for dir in expand_workspace_member_pattern(repo_root, pat)? {
                excluded_dirs.insert(normalize_lexical_path(&dir));
            }
        }
    }

    let mut manifests = BTreeSet::new();
    if root_table.contains_key("package") {
        manifests.insert(normalize_lexical_path(&repo_root.join("Cargo.toml")));
    }
    for entry in members_arr {
        let pat = entry.as_str().ok_or_else(|| {
            "root Cargo.toml `[workspace].members` entries must be strings".to_string()
        })?;
        let matched_dirs = expand_workspace_member_pattern(repo_root, pat)?;
        if matched_dirs.is_empty() {
            return Err(format!(
                "workspace member pattern `{pat}` matched 0 directories"
            ));
        }
        for dir in matched_dirs {
            let norm_dir = normalize_lexical_path(&dir);
            if excluded_dirs.contains(&norm_dir) {
                continue;
            }
            let manifest = norm_dir.join("Cargo.toml");
            if !manifest.is_file() {
                return Err(format!(
                    "workspace member `{pat}` directory {} is missing Cargo.toml",
                    norm_dir.display()
                ));
            }
            manifests.insert(manifest);
        }
    }
    if manifests.is_empty() {
        return Err("discovered 0 workspace member manifests".to_string());
    }
    Ok(manifests.into_iter().collect())
}

#[derive(Debug)]
struct MetaDep {
    name: String,
    rename: Option<String>,
    kind: Option<String>,
    optional: bool,
    uses_default_features: bool,
    features: Vec<String>,
}

impl MetaDep {
    fn dep_key(&self) -> &str {
        self.rename.as_deref().unwrap_or(self.name.as_str())
    }

    fn is_non_dev(&self) -> bool {
        self.kind.as_deref() != Some("dev")
    }
}

#[derive(Debug)]
struct MetaPackage {
    name: String,
    manifest_path: PathBuf,
    features: BTreeMap<String, Vec<String>>,
    dependencies: Vec<MetaDep>,
}

#[derive(Debug)]
struct MetaResolveDep {
    extern_name: String,
    pkg_id: String,
    has_non_dev_kind: bool,
}

fn parse_cargo_metadata(
    metadata: &serde_json::Value,
) -> Result<
    (
        BTreeMap<String, MetaPackage>,
        BTreeSet<String>,
        BTreeMap<String, Vec<MetaResolveDep>>,
    ),
    String,
> {
    let pkgs_arr = metadata
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "cargo metadata missing `packages` array".to_string())?;
    let mut packages = BTreeMap::new();
    for pkg_val in pkgs_arr {
        let id = pkg_val
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "cargo metadata package missing `id`".to_string())?
            .to_string();
        let name = pkg_val
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("cargo metadata package `{id}` missing `name`"))?
            .to_string();
        let manifest_path = pkg_val
            .get("manifest_path")
            .and_then(serde_json::Value::as_str)
            .map(PathBuf::from)
            .ok_or_else(|| format!("cargo metadata package `{name}` missing `manifest_path`"))?;

        let feat_obj = pkg_val
            .get("features")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| format!("cargo metadata package `{name}` missing `features` object"))?;
        let mut features = BTreeMap::new();
        for (feat_name, acts_val) in feat_obj {
            let acts_arr = acts_val.as_array().ok_or_else(|| {
                format!("package `{name}` feature `{feat_name}` must be an array")
            })?;
            let mut acts = Vec::with_capacity(acts_arr.len());
            for act in acts_arr {
                let s = act.as_str().ok_or_else(|| {
                    format!("package `{name}` feature `{feat_name}` has non-string activation")
                })?;
                acts.push(s.to_string());
            }
            features.insert(feat_name.clone(), acts);
        }

        let deps_arr = pkg_val
            .get("dependencies")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                format!("cargo metadata package `{name}` missing `dependencies` array")
            })?;
        let mut dependencies = Vec::with_capacity(deps_arr.len());
        for dep_val in deps_arr {
            let dep_name = dep_val
                .get("name")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("package `{name}` dependency missing `name`"))?
                .to_string();
            let rename = match dep_val.get("rename") {
                None | Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::String(s)) => Some(s.clone()),
                Some(_) => {
                    return Err(format!(
                        "package `{name}` dep `{dep_name}` has non-string `rename`"
                    ));
                }
            };
            let kind = match dep_val.get("kind") {
                None | Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::String(s)) => Some(s.clone()),
                Some(_) => {
                    return Err(format!(
                        "package `{name}` dep `{dep_name}` has non-string `kind`"
                    ));
                }
            };
            let optional = dep_val
                .get("optional")
                .and_then(serde_json::Value::as_bool)
                .ok_or_else(|| {
                    format!("package `{name}` dep `{dep_name}` missing bool `optional`")
                })?;
            let uses_default_features = dep_val
                .get("uses_default_features")
                .and_then(serde_json::Value::as_bool)
                .ok_or_else(|| {
                    format!(
                        "package `{name}` dep `{dep_name}` missing bool `uses_default_features`"
                    )
                })?;
            let dep_features_arr = dep_val
                .get("features")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| {
                    format!("package `{name}` dep `{dep_name}` missing `features` array")
                })?;
            let mut dep_features = Vec::with_capacity(dep_features_arr.len());
            for v in dep_features_arr {
                let s = v.as_str().ok_or_else(|| {
                    format!("package `{name}` dep `{dep_name}` has non-string feature")
                })?;
                dep_features.push(s.to_string());
            }
            dependencies.push(MetaDep {
                name: dep_name,
                rename,
                kind,
                optional,
                uses_default_features,
                features: dep_features,
            });
        }

        packages.insert(
            id,
            MetaPackage {
                name,
                manifest_path,
                features,
                dependencies,
            },
        );
    }

    let ws_members_arr = metadata
        .get("workspace_members")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "cargo metadata missing `workspace_members` array".to_string())?;
    let mut workspace_members = BTreeSet::new();
    for val in ws_members_arr {
        let id = val
            .as_str()
            .ok_or_else(|| "cargo metadata `workspace_members` entry must be string".to_string())?;
        if !packages.contains_key(id) {
            return Err(format!(
                "cargo metadata workspace member `{id}` is missing from `packages`"
            ));
        }
        workspace_members.insert(id.to_string());
    }

    let nodes_arr = metadata
        .get("resolve")
        .and_then(|r| r.get("nodes"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "cargo metadata missing `resolve.nodes` array".to_string())?;
    let mut resolve_deps: BTreeMap<String, Vec<MetaResolveDep>> = BTreeMap::new();
    for node_val in nodes_arr {
        let id = node_val
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "cargo metadata resolve node missing `id`".to_string())?
            .to_string();
        if !packages.contains_key(&id) {
            return Err(format!(
                "cargo metadata resolve node `{id}` is missing from `packages`"
            ));
        }
        let deps_arr = node_val
            .get("deps")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| format!("cargo metadata resolve node `{id}` missing `deps` array"))?;
        let mut deps = Vec::with_capacity(deps_arr.len());
        for rdep in deps_arr {
            let extern_name = rdep
                .get("name")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("resolve node `{id}` dep missing `name`"))?
                .to_string();
            let pkg_id = rdep
                .get("pkg")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("resolve node `{id}` dep missing `pkg`"))?
                .to_string();
            if !packages.contains_key(&pkg_id) {
                return Err(format!(
                    "resolve node `{id}` references unknown package `{pkg_id}` absent from `packages`"
                ));
            }
            let dep_kinds = rdep
                .get("dep_kinds")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| {
                    format!("resolve node `{id}` dep `{pkg_id}` missing `dep_kinds` array")
                })?;
            let has_non_dev_kind = dep_kinds
                .iter()
                .any(|dk| dk.get("kind").and_then(serde_json::Value::as_str) != Some("dev"));
            deps.push(MetaResolveDep {
                extern_name,
                pkg_id,
                has_non_dev_kind,
            });
        }
        resolve_deps.insert(id, deps);
    }

    for pid in &workspace_members {
        if !resolve_deps.contains_key(pid) {
            return Err(format!(
                "cargo metadata workspace member `{pid}` is missing from `resolve.nodes`"
            ));
        }
    }

    Ok((packages, workspace_members, resolve_deps))
}

fn resolve_non_dev_dep_targets(
    pkg_id: &str,
    pkg_name: &str,
    dep: &MetaDep,
    packages: &BTreeMap<String, MetaPackage>,
    resolve_deps: &BTreeMap<String, Vec<MetaResolveDep>>,
) -> Result<Vec<String>, String> {
    let expected_extern = dep.dep_key().replace('-', "_");
    let Some(rdeps) = resolve_deps.get(pkg_id) else {
        return Err(format!(
            "package `{pkg_name}` (`{pkg_id}`) is missing from `resolve.nodes`"
        ));
    };
    let exact: Vec<String> = rdeps
        .iter()
        .filter(|rd| {
            rd.has_non_dev_kind
                && rd.extern_name == expected_extern
                && packages.get(&rd.pkg_id).is_some_and(|p| p.name == dep.name)
        })
        .map(|rd| rd.pkg_id.clone())
        .collect();
    if !exact.is_empty() {
        return Ok(exact);
    }
    let by_name: Vec<String> = rdeps
        .iter()
        .filter(|rd| {
            rd.has_non_dev_kind && packages.get(&rd.pkg_id).is_some_and(|p| p.name == dep.name)
        })
        .map(|rd| rd.pkg_id.clone())
        .collect();
    if !by_name.is_empty() {
        return Ok(by_name);
    }
    Err(format!(
        "enabled non-dev dependency `{}` (key `{}`) of `{pkg_name}` did not resolve to any non-dev target in `resolve.nodes`",
        dep.name,
        dep.dep_key()
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetadataSeedMode {
    DefaultOnly,
    AllProductionFeatures,
}

#[derive(Default)]
struct ReachabilityState {
    active_pkgs: BTreeSet<String>,
    reached_via_edge: BTreeSet<String>,
    active_features: BTreeMap<String, BTreeSet<String>>,
    active_opt_deps: BTreeMap<String, BTreeSet<String>>,
    weak_dep_features: BTreeMap<String, BTreeSet<(String, String)>>,
    feature_origin: BTreeMap<(String, String), String>,
}

impl ReachabilityState {
    fn add_pkg(&mut self, pid: &str) -> bool {
        let inserted = self.active_pkgs.insert(pid.to_string());
        if inserted {
            self.active_features.entry(pid.to_string()).or_default();
            self.active_opt_deps.entry(pid.to_string()).or_default();
            self.weak_dep_features.entry(pid.to_string()).or_default();
        }
        inserted
    }

    fn add_feat(&mut self, pid: &str, feat: &str, origin: String) -> bool {
        let mut changed = self.add_pkg(pid);
        if self
            .active_features
            .entry(pid.to_string())
            .or_default()
            .insert(feat.to_string())
        {
            self.feature_origin
                .entry((pid.to_string(), feat.to_string()))
                .or_insert(origin);
            changed = true;
        }
        changed
    }

    fn add_opt_dep(&mut self, pid: &str, dep_key: &str) -> bool {
        let mut changed = self.add_pkg(pid);
        if self
            .active_opt_deps
            .entry(pid.to_string())
            .or_default()
            .insert(dep_key.to_string())
        {
            changed = true;
        }
        changed
    }

    fn add_weak_dep_feat(&mut self, pid: &str, dep_key: &str, target_feat: &str) -> bool {
        let mut changed = self.add_pkg(pid);
        if self
            .weak_dep_features
            .entry(pid.to_string())
            .or_default()
            .insert((dep_key.to_string(), target_feat.to_string()))
        {
            changed = true;
        }
        changed
    }
}

fn audit_cargo_metadata_prod_test_support_reachability(
    metadata: &serde_json::Value,
    excluded_crates: &[&str],
    seed_mode: MetadataSeedMode,
) -> Result<(), Vec<String>> {
    let (packages, workspace_members, resolve_deps) =
        parse_cargo_metadata(metadata).map_err(|err| vec![err])?;

    let mut state = ReachabilityState::default();
    let mut resolution_errors: BTreeSet<String> = BTreeSet::new();

    for pid in &workspace_members {
        let Some(pkg) = packages.get(pid) else {
            return Err(vec![format!(
                "workspace member `{pid}` missing from `packages`"
            )]);
        };
        if excluded_crates.contains(&pkg.name.as_str()) {
            continue;
        }
        state.add_pkg(pid);
        match seed_mode {
            MetadataSeedMode::DefaultOnly => {
                if pkg.features.contains_key("default") {
                    state.add_feat(
                        pid,
                        "default",
                        format!("seed default feature `{}[default]`", pkg.name),
                    );
                }
            }
            MetadataSeedMode::AllProductionFeatures => {
                for feat in pkg.features.keys() {
                    if feat == "test-support"
                        || (pkg.name == "neovm-value" && feat == "test-thread-fallback-heap")
                    {
                        continue;
                    }
                    state.add_feat(
                        pid,
                        feat,
                        format!("seed production feature `{}[{feat}]`", pkg.name),
                    );
                }
            }
        }
    }

    // Fixed-point propagation along ALL normal (`kind: null`) and build
    // (`kind: "build"`) dependency edges across the entire resolved graph
    // (including any excluded or external/patched crate reachable from a
    // production crate). Any unresolvable enabled edge fails closed.
    let mut changed = true;
    while changed {
        changed = false;
        let current_pkgs: Vec<String> = state.active_pkgs.iter().cloned().collect();
        for pid in current_pkgs {
            let Some(pkg) = packages.get(&pid) else {
                resolution_errors
                    .insert(format!("active package `{pid}` is missing from `packages`"));
                continue;
            };
            let non_dev_deps: Vec<&MetaDep> =
                pkg.dependencies.iter().filter(|d| d.is_non_dev()).collect();

            // 1. Propagate active non-dev dependencies (required or enabled optional).
            for dep in &non_dev_deps {
                let dep_key = dep.dep_key();
                let dep_enabled = !dep.optional
                    || state
                        .active_opt_deps
                        .get(&pid)
                        .is_some_and(|s| s.contains(dep_key));
                if !dep_enabled {
                    continue;
                }
                let kind_label = dep.kind.as_deref().unwrap_or("normal");
                match resolve_non_dev_dep_targets(&pid, &pkg.name, dep, &packages, &resolve_deps) {
                    Ok(target_ids) => {
                        for target_id in target_ids {
                            state.reached_via_edge.insert(target_id.clone());
                            changed |= state.add_pkg(&target_id);
                            if dep.uses_default_features
                                && packages
                                    .get(&target_id)
                                    .is_some_and(|q| q.features.contains_key("default"))
                            {
                                changed |= state.add_feat(
                                    &target_id,
                                    "default",
                                    format!(
                                        "{} --({kind_label} default-features)--> {}",
                                        pkg.name, dep.name
                                    ),
                                );
                            }
                            for feat in &dep.features {
                                changed |= state.add_feat(
                                    &target_id,
                                    feat,
                                    format!(
                                        "{} --({kind_label} dep feature `{feat}`)--> {}",
                                        pkg.name, dep.name
                                    ),
                                );
                            }
                        }
                    }
                    Err(err) => {
                        resolution_errors.insert(err);
                    }
                }
            }

            // 2. Propagate active features on `pkg`.
            let current_feats: Vec<String> = state
                .active_features
                .get(&pid)
                .map(|s| s.iter().cloned().collect())
                .unwrap_or_default();
            for feat in current_feats {
                let Some(activations) = pkg.features.get(&feat) else {
                    // Implicit optional-dependency feature (`feat` matches an optional dep key).
                    if non_dev_deps
                        .iter()
                        .any(|d| d.optional && d.dep_key() == feat)
                    {
                        changed |= state.add_opt_dep(&pid, &feat);
                    }
                    continue;
                };
                for act in activations {
                    if let Some(dep_key) = act.strip_prefix("dep:") {
                        changed |= state.add_opt_dep(&pid, dep_key);
                    } else if let Some((dep_key, target_feat)) = act.split_once("?/") {
                        changed |= state.add_weak_dep_feat(&pid, dep_key, target_feat);
                    } else if let Some((dep_key, target_feat)) = act.split_once('/') {
                        changed |= state.add_opt_dep(&pid, dep_key);
                        // Cargo semantics: `dep_key/target_feat` also activates the
                        // same-crate `[features].dep_key` feature when one exists.
                        if pkg.features.contains_key(dep_key) {
                            changed |= state.add_feat(
                                &pid,
                                dep_key,
                                format!(
                                    "{}[{feat}] --(`{act}` activates same-crate feature `{dep_key}`)--> {}[{dep_key}]",
                                    pkg.name, pkg.name
                                ),
                            );
                        }
                        let matching_deps: Vec<&MetaDep> = non_dev_deps
                            .iter()
                            .copied()
                            .filter(|d| d.dep_key() == dep_key)
                            .collect();
                        if matching_deps.is_empty() {
                            resolution_errors.insert(format!(
                                "{}[{feat}] references `{act}` but `{dep_key}` is not a non-dev dependency of `{}`",
                                pkg.name, pkg.name
                            ));
                        }
                        for dep in matching_deps {
                            match resolve_non_dev_dep_targets(
                                &pid,
                                &pkg.name,
                                dep,
                                &packages,
                                &resolve_deps,
                            ) {
                                Ok(target_ids) => {
                                    for target_id in target_ids {
                                        state.reached_via_edge.insert(target_id.clone());
                                        changed |= state.add_feat(
                                            &target_id,
                                            target_feat,
                                            format!(
                                                "{}[{feat}] --(`{act}`)--> {}[{target_feat}]",
                                                pkg.name, dep.name
                                            ),
                                        );
                                    }
                                }
                                Err(err) => {
                                    resolution_errors.insert(err);
                                }
                            }
                        }
                    } else {
                        changed |= state.add_feat(
                            &pid,
                            act,
                            format!("{}[{feat}] -> {}[{act}]", pkg.name, pkg.name),
                        );
                    }
                }
            }

            // 3. Propagate weak dependency features (`dep?/feat`) whenever `dep` is enabled.
            let current_weak: Vec<(String, String)> = state
                .weak_dep_features
                .get(&pid)
                .map(|s| s.iter().cloned().collect())
                .unwrap_or_default();
            for (dep_key, target_feat) in current_weak {
                for dep in &non_dev_deps {
                    if dep.dep_key() != dep_key {
                        continue;
                    }
                    let dep_enabled = !dep.optional
                        || state
                            .active_opt_deps
                            .get(&pid)
                            .is_some_and(|s| s.contains(&dep_key));
                    if !dep_enabled {
                        continue;
                    }
                    match resolve_non_dev_dep_targets(
                        &pid,
                        &pkg.name,
                        dep,
                        &packages,
                        &resolve_deps,
                    ) {
                        Ok(target_ids) => {
                            for target_id in target_ids {
                                state.reached_via_edge.insert(target_id.clone());
                                changed |= state.add_feat(
                                    &target_id,
                                    &target_feat,
                                    format!(
                                        "{} --(weak `{dep_key}?/{target_feat}`)--> {}[{target_feat}]",
                                        pkg.name, dep.name
                                    ),
                                );
                            }
                        }
                        Err(err) => {
                            resolution_errors.insert(err);
                        }
                    }
                }
            }
        }
    }

    let mut violations: Vec<String> = resolution_errors.into_iter().collect();

    // Non-tautological sanity floor: `neovm-eval` must be reached by traversing
    // at least one resolved normal/build dependency edge, not merely seeded as a
    // workspace member.
    let eval_reached_via_edge = state
        .reached_via_edge
        .iter()
        .any(|pid| packages.get(pid).is_some_and(|p| p.name == "neovm-eval"));
    if !eval_reached_via_edge {
        violations.push(
            "sanity floor failed: `neovm-eval` was not reached via any traversed production dependency edge"
                .to_string(),
        );
    }

    for pid in &workspace_members {
        let Some(pkg) = packages.get(pid) else {
            continue;
        };
        if state
            .active_features
            .get(pid)
            .is_some_and(|feats| feats.contains("test-support"))
        {
            let origin = state
                .feature_origin
                .get(&(pid.clone(), "test-support".to_string()))
                .cloned()
                .unwrap_or_else(|| "<unknown origin>".to_string());
            violations.push(format!(
                "workspace crate `{}` has feature `test-support` reachable in production normal/build graph ({seed_mode:?}, via {origin})",
                pkg.name
            ));
        }
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

fn run_cargo_metadata_json(workspace_root: &Path, all_features: bool) -> serde_json::Value {
    let cargo_bin = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(&cargo_bin);
    cmd.current_dir(workspace_root)
        .args(["metadata", "--format-version", "1", "--offline"]);
    if all_features {
        cmd.arg("--all-features");
    }
    let output = cmd
        .output()
        .unwrap_or_else(|err| panic!("failed to run `cargo metadata`: {err}"));
    assert!(
        output.status.success(),
        "`cargo metadata --format-version 1 --offline{}` failed in {} (status {:?}):\n{}",
        if all_features { " --all-features" } else { "" },
        workspace_root.display(),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|err| panic!("failed to parse `cargo metadata` JSON: {err}"))
}

#[test]
fn no_production_crate_dependencies_enable_neovm_eval_test_support() {
    // #844 Slice 2a: `Context::buffer_manager_mut` is gated behind
    // `#[cfg(any(test, feature = "test-support"))]`. Enforce production absence
    // through two independent fail-closed layers:
    //   1. `toml::Table` AST audit of root `Cargo.toml` (`[workspace].resolver`,
    //      `[workspace.dependencies]`, `[patch]`, `[replace]`, and member
    //      discovery via `[workspace].members`) plus every non-excluded member's
    //      `[dependencies]`, `[build-dependencies]`, `[target.*.dependencies]`,
    //      `[target.*.build-dependencies]`, `[features]`, `[patch]`, `[replace]`.
    //   2. Cargo's resolved dependency graph from BOTH `cargo metadata
    //      --format-version 1 --offline` (default-features pass) and `cargo
    //      metadata --format-version 1 --offline --all-features`
    //      (all-production-features pass), cross-checked against
    //      `[workspace].members` discovery. Both passes fail closed on any
    //      unresolved dependency edge or schema defect and prove `test-support`
    //      is unreachable on `neovm-eval` and all other workspace crates across
    //      all normal and build edges (including excluded/external/patched
    //      transitive crates such as `neomacs-perf -> neomacs-melpa-test-support`).
    let crates = crates_dir();
    let repo_root = crates
        .parent()
        .expect("crates_dir() must live directly under repo root");
    let root_manifest_path = repo_root.join("Cargo.toml");
    let root_text = std::fs::read_to_string(&root_manifest_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", root_manifest_path.display()));
    let root_table: toml::Table = toml::from_str(&root_text)
        .unwrap_or_else(|err| panic!("parse {}: {err}", root_manifest_path.display()));

    let member_manifests = discover_workspace_member_manifests(repo_root, &root_table)
        .unwrap_or_else(|err| panic!("discover workspace members: {err}"));

    let mut parsed_members: Vec<(PathBuf, String, toml::Table)> = Vec::new();
    let mut workspace_member_names = BTreeSet::new();
    for manifest_path in &member_manifests {
        let text = std::fs::read_to_string(manifest_path)
            .unwrap_or_else(|err| panic!("read {}: {err}", manifest_path.display()));
        let table: toml::Table = toml::from_str(&text)
            .unwrap_or_else(|err| panic!("parse {}: {err}", manifest_path.display()));
        let crate_name = table
            .get("package")
            .and_then(toml::Value::as_table)
            .and_then(|pkg| pkg.get("name"))
            .and_then(toml::Value::as_str)
            .unwrap_or_else(|| {
                panic!(
                    "{}: workspace member missing `[package].name`",
                    manifest_path.display()
                )
            })
            .to_string();
        workspace_member_names.insert(crate_name.clone());
        parsed_members.push((manifest_path.clone(), crate_name, table));
    }

    if let Err(violations) = audit_workspace_root_manifest(&root_table, &workspace_member_names) {
        panic!(
            "root workspace manifest violated test-support production guard:\n  - {}",
            violations.join("\n  - ")
        );
    }

    for (manifest_path, crate_name, table) in &parsed_members {
        if EXCLUDED_WORKSPACE_CRATES.contains(&crate_name.as_str()) {
            continue;
        }
        if let Err(violations) = audit_member_manifest(crate_name, table, &workspace_member_names) {
            panic!(
                "{} (`{crate_name}`): production manifest violated test-support guard:\n  - {}",
                manifest_path.display(),
                violations.join("\n  - ")
            );
        }
    }

    // Layer 2a: Default-features metadata pass + `[workspace].members` parity check.
    let default_metadata = run_cargo_metadata_json(repo_root, false);
    let all_features_metadata = run_cargo_metadata_json(repo_root, true);

    let discovered_paths: BTreeSet<PathBuf> = member_manifests
        .iter()
        .map(|p| normalize_lexical_path(p))
        .collect();
    for (label, metadata) in [
        ("default", &default_metadata),
        ("--all-features", &all_features_metadata),
    ] {
        let (meta_pkgs, meta_ws_ids, _) = parse_cargo_metadata(metadata)
            .unwrap_or_else(|err| panic!("parse {label} cargo metadata: {err}"));
        let meta_ws_names: BTreeSet<String> = meta_ws_ids
            .iter()
            .map(|id| meta_pkgs[id].name.clone())
            .collect();
        assert_eq!(
            workspace_member_names, meta_ws_names,
            "`[workspace].members` discovery must match `{label}` cargo metadata workspace_members"
        );
        let meta_ws_paths: BTreeSet<PathBuf> = meta_ws_ids
            .iter()
            .map(|id| normalize_lexical_path(&meta_pkgs[id].manifest_path))
            .collect();
        assert_eq!(
            discovered_paths, meta_ws_paths,
            "`[workspace].members` manifest paths must match `{label}` cargo metadata paths"
        );
    }

    if let Err(violations) = audit_cargo_metadata_prod_test_support_reachability(
        &default_metadata,
        EXCLUDED_WORKSPACE_CRATES,
        MetadataSeedMode::DefaultOnly,
    ) {
        panic!(
            "resolved Cargo default normal/build graph violated production test-support guard:\n  - {}",
            violations.join("\n  - ")
        );
    }

    if let Err(violations) = audit_cargo_metadata_prod_test_support_reachability(
        &all_features_metadata,
        EXCLUDED_WORKSPACE_CRATES,
        MetadataSeedMode::AllProductionFeatures,
    ) {
        panic!(
            "resolved Cargo --all-features normal/build graph violated production test-support guard:\n  - {}",
            violations.join("\n  - ")
        );
    }
}

#[test]
fn negative_manifest_prod_dep_enabling_test_support_is_rejected() {
    let ws_members = BTreeSet::from(["neovm-eval".to_string(), "neovm-core".to_string()]);
    let assert_rejected = |label: &str, toml_src: &str, expected_substr: &str| {
        let table: toml::Table = toml::from_str(toml_src)
            .unwrap_or_else(|err| panic!("{label}: fixture must be valid TOML: {err}"));
        let Err(violations) = audit_member_manifest("synthetic", &table, &ws_members) else {
            panic!("{label}: expected audit_member_manifest to reject manifest");
        };
        assert!(
            violations.iter().any(|v| v.contains(expected_substr)),
            "{label}: expected violation containing `{expected_substr}`, got: {violations:?}"
        );
    };
    let assert_allowed = |label: &str, crate_name: &str, toml_src: &str| {
        let table: toml::Table = toml::from_str(toml_src)
            .unwrap_or_else(|err| panic!("{label}: fixture must be valid TOML: {err}"));
        let result = audit_member_manifest(crate_name, &table, &ws_members);
        assert!(
            result.is_ok(),
            "{label}: expected audit_member_manifest to allow manifest, got: {result:?}"
        );
    };

    assert_rejected(
        "inline double-quoted [dependencies]",
        r#"
[dependencies]
neovm-eval = { path = "../neovm-eval", features = ["test-support"] }
"#,
        "[dependencies].neovm-eval: enables `test-support`",
    );
    assert_rejected(
        "subtable [dependencies.neovm-eval]",
        r#"
[dependencies.neovm-eval]
path = "../neovm-eval"
features = ["test-support"]
"#,
        "[dependencies].neovm-eval: enables `test-support`",
    );
    assert_rejected(
        "single-quoted TOML string in [dependencies]",
        r#"
[dependencies]
neovm-eval = { path = '../neovm-eval', features = ['test-support'] }
"#,
        "[dependencies].neovm-eval: enables `test-support`",
    );
    assert_rejected(
        "unicode escape in [dependencies]",
        r#"
[dependencies]
neovm-eval = { path = "../neovm-eval", features = ["\u0074est-support"] }
"#,
        "[dependencies].neovm-eval: enables `test-support`",
    );
    assert_rejected(
        "dotted key outside [dependencies] header",
        r#"
dependencies.neovm-eval.path = "../neovm-eval"
dependencies.neovm-eval.features = ["test-support"]
"#,
        "[dependencies].neovm-eval: enables `test-support`",
    );
    assert_rejected(
        "spaced [ dependencies ] header with '#' inside preceding string",
        r#"
[ dependencies ]
neovm-eval = { git = "https://example.com/repo#main", features = ["test-support"] }
"#,
        "[dependencies].neovm-eval: enables `test-support`",
    );
    assert_rejected(
        "[build-dependencies] enabling test-support",
        r#"
[build-dependencies]
neovm-eval = { path = "../neovm-eval", features = ["test-support"] }
"#,
        "[build-dependencies].neovm-eval: enables `test-support`",
    );
    assert_rejected(
        "[target.'cfg(unix)'.build-dependencies] enabling test-support",
        r#"
[target.'cfg(unix)'.build-dependencies]
neovm-eval = { path = "../neovm-eval", features = ['test-support'] }
"#,
        "[target.cfg(unix).build-dependencies].neovm-eval: enables `test-support`",
    );
    assert_rejected(
        "dotted key under target.'cfg(unix)'.dependencies",
        r#"
target.'cfg(unix)'.dependencies.neovm-eval.features = ['test-support']
"#,
        "[target.cfg(unix).dependencies].neovm-eval: enables `test-support`",
    );
    assert_rejected(
        "spaced [target.'cfg(unix)' . dependencies] header",
        r#"
[target.'cfg(unix)' . dependencies]
neovm-eval = { path = "../neovm-eval", features = ['test-support'] }
"#,
        "[target.cfg(unix).dependencies].neovm-eval: enables `test-support`",
    );
    assert_rejected(
        "non-test-support feature forwarding neovm-eval/test-support",
        r#"
[features]
sneaky = ["neovm-eval/test-support"]
"#,
        "non-test-support feature `[features].sneaky` enables `neovm-eval/test-support`",
    );
    assert_rejected(
        "weak optional dependency feature with single quotes",
        r#"
[features]
my_feat = ["dep:neovm-eval", 'neovm-eval?/test-support']
"#,
        "non-test-support feature `[features].my_feat` enables `neovm-eval?/test-support`",
    );
    assert_rejected(
        "default feature enabling test-support with single quotes",
        r#"
[features]
default = ['test-support']
"#,
        "non-test-support feature `[features].default` enables `test-support`",
    );
    assert_rejected(
        "default feature activating renamed test-support dependency via test-support/bar",
        r#"
[features]
default = ["test-support/bar"]
test-support = ["dep:test-support", "neovm-eval/test-support"]
"#,
        "non-test-support feature `[features].default` enables `test-support/bar`",
    );
    assert_rejected(
        "non-test-support feature activating test-support?/bar",
        r#"
[features]
sneaky = ["test-support?/bar"]
"#,
        "non-test-support feature `[features].sneaky` enables `test-support?/bar`",
    );
    assert_rejected(
        "dotted key under features",
        r#"
features.sneaky = ['neovm-eval/test-support']
"#,
        "non-test-support feature `[features].sneaky` enables `neovm-eval/test-support`",
    );
    assert_rejected(
        "member-level [patch.crates-io] patching a workspace member",
        r#"
[patch.crates-io]
neovm-eval = { path = "../evil-eval" }
"#,
        "must not patch workspace crate `neovm-eval`",
    );
    assert_rejected(
        "member-level [replace] section",
        r#"
[replace]
"neovm-eval:0.0.21" = { path = "../evil-eval" }
"#,
        "`[replace]` section is forbidden",
    );

    // Positive controls: dev-dependencies (including target-specific) and
    // multi-line formatting of the legitimate `test-support` feature must pass.
    assert_allowed(
        "[dev-dependencies] enabling test-support",
        "synthetic",
        r#"
[dev-dependencies]
neovm-eval = { path = "../neovm-eval", features = ["test-support"] }

[target.'cfg(unix)'.dev-dependencies]
neovm-eval = { path = "../neovm-eval", features = ['test-support'] }
"#,
    );
    assert_allowed(
        "multi-line [features].test-support array",
        "synthetic",
        r#"
[features]
test-support = [
    "neovm-eval/test-support",
]
"#,
    );
    assert_allowed(
        "neovm-value allowlisted test-thread-fallback-heap feature",
        "neovm-value",
        r#"
[features]
test-support = ["dep:visibility"]
test-thread-fallback-heap = ["test-support"]
"#,
    );
}

#[test]
fn negative_workspace_root_manifest_deps_patch_replace_and_resolver_rejected() {
    let ws_members = BTreeSet::from(["neovm-eval".to_string(), "neovm-core".to_string()]);
    let assert_root_rejected = |label: &str, toml_src: &str, expected_substr: &str| {
        let table: toml::Table = toml::from_str(toml_src)
            .unwrap_or_else(|err| panic!("{label}: fixture must be valid TOML: {err}"));
        let Err(violations) = audit_workspace_root_manifest(&table, &ws_members) else {
            panic!("{label}: expected audit_workspace_root_manifest to reject root manifest");
        };
        assert!(
            violations.iter().any(|v| v.contains(expected_substr)),
            "{label}: expected violation containing `{expected_substr}`, got: {violations:?}"
        );
    };

    assert_root_rejected(
        "[workspace.dependencies] enabling test-support",
        r#"
[workspace]
resolver = "2"
[workspace.dependencies]
neovm-eval = { path = "crates/neovm-eval", features = ['test-support'] }
"#,
        "[workspace.dependencies].neovm-eval: enables `test-support`",
    );
    assert_root_rejected(
        "[patch.crates-io] patching a workspace member directly",
        r#"
[workspace]
resolver = "2"
[patch.crates-io]
neovm-eval = { path = "evil/neovm-eval" }
"#,
        "must not patch workspace crate `neovm-eval`",
    );
    assert_root_rejected(
        "[patch.crates-io] patching a workspace member via package rename",
        r#"
[workspace]
resolver = "2"
[patch.crates-io]
sneaky-alias = { package = "neovm-eval", path = "evil/neovm-eval" }
"#,
        "must not patch workspace crate `neovm-eval`",
    );
    assert_root_rejected(
        "[patch.crates-io] enabling test-support feature",
        r#"
[workspace]
resolver = "2"
[patch.crates-io]
third-party = { path = "vendor/third-party", features = ["test-support"] }
"#,
        "[patch.crates-io.third-party]: enables `test-support`",
    );
    assert_root_rejected(
        "non-empty [replace] section",
        r#"
[workspace]
resolver = "2"
[replace]
"neovm-eval:0.0.21" = { path = "evil/neovm-eval" }
"#,
        "`[replace]` section is forbidden",
    );
    assert_root_rejected(
        "resolver = 1 allows dev-dep feature unification",
        r#"
[workspace]
resolver = "1"
"#,
        "`[workspace].resolver` must be",
    );
    assert_root_rejected(
        "omitted resolver defaults to resolver = 1 in virtual workspaces",
        r#"
[workspace]
members = ["crates/neovm-eval"]
"#,
        "`[workspace].resolver` must be",
    );

    let clean_root: toml::Table = toml::from_str(
        r#"
[workspace]
resolver = "2"
[workspace.dependencies]
neovm-core = { path = "crates/neovm-core", default-features = false }
[patch.crates-io]
winit = { git = "https://github.com/eval-exec/winit", rev = "0000000" }
"#,
    )
    .expect("valid clean root fixture");
    assert!(
        audit_workspace_root_manifest(&clean_root, &ws_members).is_ok(),
        "clean root manifest with external [patch.crates-io] and resolver = 2 must pass"
    );

    // Verify `[workspace].members` discovery handles globs, nested directories
    // outside `crates/`, `[workspace].exclude`, and a root `[package]`.
    let tmp = tempfile::tempdir().expect("create temp workspace for member discovery");
    for rel in [
        "Cargo.toml",
        "crates/alpha/Cargo.toml",
        "crates/excluded-crate/Cargo.toml",
        "tools/nested/beta/Cargo.toml",
    ] {
        let full = tmp.path().join(rel);
        std::fs::create_dir_all(full.parent().expect("parent")).expect("mkdir");
        std::fs::write(&full, "[package]\nname = \"dummy\"\nversion = \"0.1.0\"\n").expect("write");
    }
    let root_with_globs: toml::Table = toml::from_str(
        r#"
[package]
name = "root-pkg"
version = "0.1.0"

[workspace]
resolver = "2"
members = ["crates/*", "tools/nested/beta"]
exclude = ["crates/excluded-crate"]
"#,
    )
    .expect("parse discovery fixture");
    let discovered = discover_workspace_member_manifests(tmp.path(), &root_with_globs)
        .expect("discover members");
    let rel_paths: BTreeSet<String> = discovered
        .iter()
        .map(|p| {
            p.strip_prefix(tmp.path())
                .expect("under tempdir")
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    assert_eq!(
        rel_paths,
        BTreeSet::from([
            "Cargo.toml".to_string(),
            "crates/alpha/Cargo.toml".to_string(),
            "tools/nested/beta/Cargo.toml".to_string(),
        ]),
        "workspace member discovery must handle root [package], globs, nested non-crates paths, and exclude"
    );
}

fn build_temp_workspace(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create temp cargo workspace");
    for (rel_path, content) in files {
        let full = dir.path().join(rel_path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|err| panic!("mkdir {}: {err}", parent.display()));
        }
        std::fs::write(&full, content)
            .unwrap_or_else(|err| panic!("write {}: {err}", full.display()));
    }
    dir
}

fn assert_real_cargo_reachability_rejected(
    label: &str,
    files: &[(&str, &str)],
    excluded_crates: &[&str],
    all_features: bool,
    seed_mode: MetadataSeedMode,
    expected_substrings: &[&str],
) {
    let ws = build_temp_workspace(files);
    let metadata = run_cargo_metadata_json(ws.path(), all_features);
    let Err(violations) =
        audit_cargo_metadata_prod_test_support_reachability(&metadata, excluded_crates, seed_mode)
    else {
        panic!(
            "{label}: expected real cargo metadata ({seed_mode:?}, all_features={all_features}) reachability audit to fail"
        );
    };
    let joined = violations.join("\n");
    for substr in expected_substrings {
        assert!(
            joined.contains(substr),
            "{label}: expected violation containing `{substr}`, got:\n{joined}"
        );
    }
}

fn assert_real_cargo_reachability_allowed(
    label: &str,
    files: &[(&str, &str)],
    excluded_crates: &[&str],
) {
    let ws = build_temp_workspace(files);
    let default_meta = run_cargo_metadata_json(ws.path(), false);
    let all_meta = run_cargo_metadata_json(ws.path(), true);
    let default_res = audit_cargo_metadata_prod_test_support_reachability(
        &default_meta,
        excluded_crates,
        MetadataSeedMode::DefaultOnly,
    );
    assert!(
        default_res.is_ok(),
        "{label}: expected real cargo default metadata to pass, got: {default_res:?}"
    );
    let all_res = audit_cargo_metadata_prod_test_support_reachability(
        &all_meta,
        excluded_crates,
        MetadataSeedMode::AllProductionFeatures,
    );
    assert!(
        all_res.is_ok(),
        "{label}: expected real cargo --all-features metadata to pass, got: {all_res:?}"
    );
}

#[test]
fn negative_real_cargo_metadata_end_to_end_catches_all_bypass_vectors() {
    // Every fixture below runs REAL `cargo metadata --format-version 1 --offline
    // [--all-features]` against minimal Cargo workspaces in `tempfile::TempDir`
    // (never hand-fabricated `resolve.nodes` JSON), proving our reachability
    // closure matches Cargo's actual output and fails closed on every bypass.
    const EVAL_TOML: &str = r#"
[package]
name = "neovm-eval"
version = "0.1.0"
edition = "2024"

[features]
test-support = []
"#;
    const LEAF_TOML: &str = r#"
[package]
name = "leaf"
version = "0.1.0"
edition = "2024"

[features]
bar = []
"#;

    // 1. Workspace inheritance (`[workspace.dependencies]` enables `test-support`,
    //    member inherits `neovm-eval.workspace = true`).
    assert_real_cargo_reachability_rejected(
        "workspace inheritance via [workspace.dependencies]",
        &[
            (
                "Cargo.toml",
                r#"
[workspace]
resolver = "2"
members = ["prod", "neovm-eval"]

[workspace.dependencies]
neovm-eval = { path = "neovm-eval", features = ["test-support"] }
"#,
            ),
            (
                "prod/Cargo.toml",
                r#"
[package]
name = "prod"
version = "0.1.0"
edition = "2024"

[dependencies]
neovm-eval.workspace = true
"#,
            ),
            ("prod/src/lib.rs", ""),
            ("neovm-eval/Cargo.toml", EVAL_TOML),
            ("neovm-eval/src/lib.rs", ""),
        ],
        &[],
        true,
        MetadataSeedMode::AllProductionFeatures,
        &["neovm-eval", "test-support"],
    );

    // 2. Reviewer bypass #1 — Optional path dep on a `[workspace].exclude` crate
    //    enabling `neovm-eval/test-support` (`sneaky = ["dep:excluded-helper"]`).
    let excluded_optional_ws: &[(&str, &str)] = &[
        (
            "Cargo.toml",
            r#"
[workspace]
resolver = "2"
members = ["prod", "neovm-eval"]
exclude = ["excluded-helper"]
"#,
        ),
        (
            "prod/Cargo.toml",
            r#"
[package]
name = "prod"
version = "0.1.0"
edition = "2024"

[dependencies]
neovm-eval = { path = "../neovm-eval" }
excluded-helper = { path = "../excluded-helper", optional = true }

[features]
sneaky = ["dep:excluded-helper"]
"#,
        ),
        ("prod/src/lib.rs", ""),
        (
            "excluded-helper/Cargo.toml",
            r#"
[package]
name = "excluded-helper"
version = "0.1.0"
edition = "2024"

[dependencies]
neovm-eval = { path = "../neovm-eval", features = ["test-support"] }
"#,
        ),
        ("excluded-helper/src/lib.rs", ""),
        ("neovm-eval/Cargo.toml", EVAL_TOML),
        ("neovm-eval/src/lib.rs", ""),
    ];
    assert_real_cargo_reachability_rejected(
        "optional [workspace].exclude path dep enabling neovm-eval/test-support",
        excluded_optional_ws,
        &[],
        true,
        MetadataSeedMode::AllProductionFeatures,
        &["neovm-eval", "test-support", "excluded-helper"],
    );
    // Also prove fail-closed behavior: running `AllProductionFeatures` against
    // real non-`--all-features` metadata (where Cargo omits `excluded-helper`
    // from `resolve.nodes`) MUST fail closed rather than silently dropping the edge!
    assert_real_cargo_reachability_rejected(
        "fail-closed when enabled optional dep is absent from non---all-features resolve.nodes",
        excluded_optional_ws,
        &[],
        false,
        MetadataSeedMode::AllProductionFeatures,
        &["excluded-helper", "did not resolve to any non-dev target"],
    );

    // 3. Reviewer bypass #2 — Optional `[target.'cfg(target_os = "windows")'.dependencies]`
    //    path dep on a `[workspace].exclude` crate (must be caught even on Linux).
    assert_real_cargo_reachability_rejected(
        "optional target-cfg [workspace].exclude dep enabling neovm-eval/test-support",
        &[
            (
                "Cargo.toml",
                r#"
[workspace]
resolver = "2"
members = ["prod", "neovm-eval"]
exclude = ["excluded-win-helper"]
"#,
            ),
            (
                "prod/Cargo.toml",
                r#"
[package]
name = "prod"
version = "0.1.0"
edition = "2024"

[dependencies]
neovm-eval = { path = "../neovm-eval" }

[target.'cfg(target_os = "windows")'.dependencies]
excluded-win-helper = { path = "../excluded-win-helper", optional = true }

[features]
win-helper = ["dep:excluded-win-helper"]
"#,
            ),
            ("prod/src/lib.rs", ""),
            (
                "excluded-win-helper/Cargo.toml",
                r#"
[package]
name = "excluded-win-helper"
version = "0.1.0"
edition = "2024"

[dependencies]
neovm-eval = { path = "../neovm-eval", features = ["test-support"] }
"#,
            ),
            ("excluded-win-helper/src/lib.rs", ""),
            ("neovm-eval/Cargo.toml", EVAL_TOML),
            ("neovm-eval/src/lib.rs", ""),
        ],
        &[],
        true,
        MetadataSeedMode::AllProductionFeatures,
        &["neovm-eval", "test-support", "excluded-win-helper"],
    );

    // 4. Reviewer bypass #3 — Optional `[patch.crates-io]` target whose local
    //    manifest enables `neovm-eval/test-support` (100% offline).
    assert_real_cargo_reachability_rejected(
        "optional [patch.crates-io] target enabling neovm-eval/test-support",
        &[
            (
                "Cargo.toml",
                r#"
[workspace]
resolver = "2"
members = ["prod", "neovm-eval"]
exclude = ["patched-ext"]

[patch.crates-io]
fake-ext-xyz = { path = "patched-ext" }
"#,
            ),
            (
                "prod/Cargo.toml",
                r#"
[package]
name = "prod"
version = "0.1.0"
edition = "2024"

[dependencies]
neovm-eval = { path = "../neovm-eval" }
fake-ext-xyz = { version = "0.1.0", optional = true }

[features]
enable-patched = ["dep:fake-ext-xyz"]
"#,
            ),
            ("prod/src/lib.rs", ""),
            (
                "patched-ext/Cargo.toml",
                r#"
[package]
name = "fake-ext-xyz"
version = "0.1.0"
edition = "2024"

[dependencies]
neovm-eval = { path = "../neovm-eval", features = ["test-support"] }
"#,
            ),
            ("patched-ext/src/lib.rs", ""),
            ("neovm-eval/Cargo.toml", EVAL_TOML),
            ("neovm-eval/src/lib.rs", ""),
        ],
        &[],
        true,
        MetadataSeedMode::AllProductionFeatures,
        &["neovm-eval", "test-support", "fake-ext-xyz"],
    );

    // 5. Reviewer bypass #4 — Single-crate DEFAULT-build `dep_key/target_feat`
    //    activating same-crate `[features].test-support` (must fail on BOTH
    //    `DefaultOnly` and `AllProductionFeatures`).
    let dep_key_default_bypass: &[(&str, &str)] = &[
        (
            "Cargo.toml",
            r#"
[workspace]
resolver = "2"
members = ["prod", "leaf", "neovm-eval"]
"#,
        ),
        (
            "prod/Cargo.toml",
            r#"
[package]
name = "prod"
version = "0.1.0"
edition = "2024"

[dependencies]
test-support = { package = "leaf", path = "../leaf", optional = true }
neovm-eval = { path = "../neovm-eval" }

[features]
default = ["test-support/bar"]
test-support = ["dep:test-support", "neovm-eval/test-support"]
"#,
        ),
        ("prod/src/lib.rs", ""),
        ("leaf/Cargo.toml", LEAF_TOML),
        ("leaf/src/lib.rs", ""),
        ("neovm-eval/Cargo.toml", EVAL_TOML),
        ("neovm-eval/src/lib.rs", ""),
    ];
    assert_real_cargo_reachability_rejected(
        "single-crate default-build dep_key/target_feat activating [features].test-support (DefaultOnly)",
        dep_key_default_bypass,
        &[],
        false,
        MetadataSeedMode::DefaultOnly,
        &["neovm-eval", "test-support", "activates same-crate feature"],
    );
    assert_real_cargo_reachability_rejected(
        "single-crate default-build dep_key/target_feat activating [features].test-support (AllProductionFeatures)",
        dep_key_default_bypass,
        &[],
        true,
        MetadataSeedMode::AllProductionFeatures,
        &["neovm-eval", "test-support", "activates same-crate feature"],
    );

    // 6. Excluded-crate `trigger = ["leaf/bar"]` where `leaf = ["dep:leaf", "neovm-eval/test-support"]`.
    assert_real_cargo_reachability_rejected(
        "excluded-crate dep_key/target_feat activating same-crate feature enabling neovm-eval/test-support",
        &[
            (
                "Cargo.toml",
                r#"
[workspace]
resolver = "2"
members = ["prod", "neomacs-melpa-test-support", "leaf", "neovm-eval"]
"#,
            ),
            (
                "prod/Cargo.toml",
                r#"
[package]
name = "prod"
version = "0.1.0"
edition = "2024"

[dependencies]
neomacs-melpa-test-support = { path = "../neomacs-melpa-test-support" }
"#,
            ),
            ("prod/src/lib.rs", ""),
            (
                "neomacs-melpa-test-support/Cargo.toml",
                r#"
[package]
name = "neomacs-melpa-test-support"
version = "0.1.0"
edition = "2024"

[dependencies]
leaf = { path = "../leaf", optional = true }
neovm-eval = { path = "../neovm-eval" }

[features]
default = ["leaf/bar"]
leaf = ["dep:leaf", "neovm-eval/test-support"]
"#,
            ),
            ("neomacs-melpa-test-support/src/lib.rs", ""),
            ("leaf/Cargo.toml", LEAF_TOML),
            ("leaf/src/lib.rs", ""),
            ("neovm-eval/Cargo.toml", EVAL_TOML),
            ("neovm-eval/src/lib.rs", ""),
        ],
        &["neomacs-melpa-test-support"],
        false,
        MetadataSeedMode::DefaultOnly,
        &["neovm-eval", "test-support"],
    );

    // 7. Two-hop optional renamed + weak dep feature (`eval_alias?/test-support`)
    //    verified with REAL `cargo metadata --all-features` output.
    assert_real_cargo_reachability_rejected(
        "two-hop optional renamed weak dep feature forwarding",
        &[
            (
                "Cargo.toml",
                r#"
[workspace]
resolver = "2"
members = ["prod", "mid", "neovm-eval"]
"#,
            ),
            (
                "prod/Cargo.toml",
                r#"
[package]
name = "prod"
version = "0.1.0"
edition = "2024"

[dependencies]
mid = { path = "../mid" }

[features]
extra = ["mid/bridge"]
"#,
            ),
            ("prod/src/lib.rs", ""),
            (
                "mid/Cargo.toml",
                r#"
[package]
name = "mid"
version = "0.1.0"
edition = "2024"

[dependencies]
eval_alias = { package = "neovm-eval", path = "../neovm-eval", optional = true }

[features]
bridge = ["dep:eval_alias", "eval_alias?/test-support"]
"#,
            ),
            ("mid/src/lib.rs", ""),
            ("neovm-eval/Cargo.toml", EVAL_TOML),
            ("neovm-eval/src/lib.rs", ""),
        ],
        &[],
        true,
        MetadataSeedMode::AllProductionFeatures,
        &["neovm-eval", "test-support", "eval_alias?/test-support"],
    );

    // 8. `[build-dependencies]` enabling `neovm-eval/test-support`.
    assert_real_cargo_reachability_rejected(
        "[build-dependencies] enabling neovm-eval/test-support",
        &[
            (
                "Cargo.toml",
                r#"
[workspace]
resolver = "2"
members = ["prod", "neovm-eval"]
"#,
            ),
            (
                "prod/Cargo.toml",
                r#"
[package]
name = "prod"
version = "0.1.0"
edition = "2024"

[build-dependencies]
neovm-eval = { path = "../neovm-eval", features = ["test-support"] }
"#,
            ),
            ("prod/src/lib.rs", ""),
            ("neovm-eval/Cargo.toml", EVAL_TOML),
            ("neovm-eval/src/lib.rs", ""),
        ],
        &[],
        true,
        MetadataSeedMode::AllProductionFeatures,
        &["neovm-eval", "test-support", "build dep feature"],
    );

    // 9. Production crate enabling `neovm-value/test-thread-fallback-heap`
    //    (which implies `neovm-value/test-support`).
    assert_real_cargo_reachability_rejected(
        "production crate enabling neovm-value/test-thread-fallback-heap",
        &[
            (
                "Cargo.toml",
                r#"
[workspace]
resolver = "2"
members = ["prod", "neovm-eval", "neovm-value"]
"#,
            ),
            (
                "prod/Cargo.toml",
                r#"
[package]
name = "prod"
version = "0.1.0"
edition = "2024"

[dependencies]
neovm-eval = { path = "../neovm-eval" }
neovm-value = { path = "../neovm-value", features = ["test-thread-fallback-heap"] }
"#,
            ),
            ("prod/src/lib.rs", ""),
            ("neovm-eval/Cargo.toml", EVAL_TOML),
            ("neovm-eval/src/lib.rs", ""),
            (
                "neovm-value/Cargo.toml",
                r#"
[package]
name = "neovm-value"
version = "0.1.0"
edition = "2024"

[features]
test-support = []
test-thread-fallback-heap = ["test-support"]
"#,
            ),
            ("neovm-value/src/lib.rs", ""),
        ],
        &[],
        true,
        MetadataSeedMode::AllProductionFeatures,
        &["neovm-value", "test-support"],
    );

    // 10. Positive control: `[dev-dependencies]` enabling `neovm-eval/test-support`
    //     is permitted under resolver = "2".
    assert_real_cargo_reachability_allowed(
        "[dev-dependencies] enabling neovm-eval/test-support",
        &[
            (
                "Cargo.toml",
                r#"
[workspace]
resolver = "2"
members = ["prod", "neovm-eval"]
"#,
            ),
            (
                "prod/Cargo.toml",
                r#"
[package]
name = "prod"
version = "0.1.0"
edition = "2024"

[dependencies]
neovm-eval = { path = "../neovm-eval" }

[dev-dependencies]
neovm-eval = { path = "../neovm-eval", features = ["test-support"] }
"#,
            ),
            ("prod/src/lib.rs", ""),
            ("neovm-eval/Cargo.toml", EVAL_TOML),
            ("neovm-eval/src/lib.rs", ""),
        ],
        &[],
    );

    // 11. Positive control: `default-features = false` on an excluded crate
    //     whose `default = ["neovm-eval/test-support"]` does NOT spuriously fail.
    assert_real_cargo_reachability_allowed(
        "default-features = false suppresses excluded crate default test-support",
        &[
            (
                "Cargo.toml",
                r#"
[workspace]
resolver = "2"
members = ["prod", "neomacs-melpa-test-support", "neovm-eval"]
"#,
            ),
            (
                "prod/Cargo.toml",
                r#"
[package]
name = "prod"
version = "0.1.0"
edition = "2024"

[dependencies]
neovm-eval = { path = "../neovm-eval" }
neomacs-melpa-test-support = { path = "../neomacs-melpa-test-support", default-features = false }
"#,
            ),
            ("prod/src/lib.rs", ""),
            (
                "neomacs-melpa-test-support/Cargo.toml",
                r#"
[package]
name = "neomacs-melpa-test-support"
version = "0.1.0"
edition = "2024"

[dependencies]
neovm-eval = { path = "../neovm-eval" }

[features]
default = ["neovm-eval/test-support"]
"#,
            ),
            ("neomacs-melpa-test-support/src/lib.rs", ""),
            ("neovm-eval/Cargo.toml", EVAL_TOML),
            ("neovm-eval/src/lib.rs", ""),
        ],
        &["neomacs-melpa-test-support"],
    );
}

#[test]
fn negative_malformed_cargo_metadata_and_zero_edge_sanity_floor_fail_closed() {
    // 1. Non-tautological sanity floor on a real Cargo temp workspace where
    //    `neovm-eval` is a workspace member with 0 incoming dependency edges:
    //    must fail closed rather than passing vacuously because it was seeded.
    assert_real_cargo_reachability_rejected(
        "zero-edge neovm-eval fails non-tautological sanity floor",
        &[
            (
                "Cargo.toml",
                r#"
[workspace]
resolver = "2"
members = ["prod", "neovm-eval"]
"#,
            ),
            (
                "prod/Cargo.toml",
                r#"
[package]
name = "prod"
version = "0.1.0"
edition = "2024"
"#,
            ),
            ("prod/src/lib.rs", ""),
            (
                "neovm-eval/Cargo.toml",
                r#"
[package]
name = "neovm-eval"
version = "0.1.0"
edition = "2024"
[features]
test-support = []
"#,
            ),
            ("neovm-eval/src/lib.rs", ""),
        ],
        &[],
        true,
        MetadataSeedMode::AllProductionFeatures,
        &["sanity floor failed", "neovm-eval"],
    );

    // 2. Schema-corruption cases in `parse_cargo_metadata` must fail closed
    //    rather than silently defaulting to empty collections.
    let missing_features = serde_json::json!({
        "workspace_members": ["p#0.1"],
        "packages": [{ "id": "p#0.1", "name": "p", "manifest_path": "/p/Cargo.toml", "dependencies": [] }],
        "resolve": { "nodes": [{ "id": "p#0.1", "deps": [] }] }
    });
    assert!(
        parse_cargo_metadata(&missing_features)
            .unwrap_err()
            .contains("missing `features` object")
    );

    let missing_deps = serde_json::json!({
        "workspace_members": ["p#0.1"],
        "packages": [{ "id": "p#0.1", "name": "p", "manifest_path": "/p/Cargo.toml", "features": {} }],
        "resolve": { "nodes": [{ "id": "p#0.1", "deps": [] }] }
    });
    assert!(
        parse_cargo_metadata(&missing_deps)
            .unwrap_err()
            .contains("missing `dependencies` array")
    );

    let dangling_resolve_pkg = serde_json::json!({
        "workspace_members": ["p#0.1"],
        "packages": [{ "id": "p#0.1", "name": "p", "manifest_path": "/p/Cargo.toml", "features": {}, "dependencies": [] }],
        "resolve": {
            "nodes": [{
                "id": "p#0.1",
                "deps": [{ "name": "ghost", "pkg": "ghost#0.1", "dep_kinds": [{ "kind": null }] }]
            }]
        }
    });
    assert!(
        parse_cargo_metadata(&dangling_resolve_pkg)
            .unwrap_err()
            .contains("references unknown package `ghost#0.1`")
    );
}

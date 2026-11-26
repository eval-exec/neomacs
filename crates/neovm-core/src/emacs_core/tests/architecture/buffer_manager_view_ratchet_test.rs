//! Syntactic regression ratchet over the listed item kinds and legacy raw
//! `&mut BufferManager` / `&mut EditRuntime` accessor table (rt A3 2c (d)).
//!
//! ## Honest Scope — NOT a Soundness Boundary Today (see #844)
//!
//! **This scanner is a syntactic regression ratchet over the listed item kinds
//! and legacy raw-accessor table; it is NOT a soundness boundary while the
//! following three `pub` sites remain reachable in production code:**
//!
//! 1. `crates/neovm-eval/src/emacs_core/runtime/eval/mod.rs:3433` —
//!    `pub edit: neovm_edit::buffer::EditRuntime,`
//! 2. `crates/neovm-edit/src/buffer/edit_runtime.rs:23` —
//!    `pub buffers: BufferManager,`
//! 3. `crates/neovm-eval/src/emacs_core/runtime/eval/context_api.rs:605` —
//!    `pub fn buffer_manager_mut(&mut self) -> &mut BufferManager`
//!
//! Narrowing those three `pub` items so that Rust's type system and privacy
//! rules enforce the boundary workspace-wide is tracked in GitHub issue #844
//! (Slices 2a/2b/2c).
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

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

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
        "buffer_manager_mut",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/context_api.rs",
        "buffers_and_obarray_view_mut",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/context_api.rs",
        "edit_mut",
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
        "mode_line_formatting_inputs_mut",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/context_api.rs",
        "obarray_and_buffers_mut",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/context_api.rs",
        "processes_and_buffers_mut",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/context_api.rs",
        "processes_threads_and_buffers_mut",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/context_api.rs",
        "processes_threads_buffers_and_coding_systems_mut",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/context_api.rs",
        "runtime_binding_parts_mut",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/context_api.rs",
        "variable_view_and_buffers_mut",
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
    (
        "neovm-eval/src/emacs_core/runtime/eval/text_host.rs",
        "buffers_mut",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/vm_shared.rs",
        "gui_frame_creation_state",
    ),
    ("neovm-text/src/emacs_core/text/host/mod.rs", "buffers_mut"),
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

    fn record_raw_accessor_if_matching(&mut self, sig: &syn::Signature) {
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
        self.record_raw_accessor_if_matching(&item_fn.sig);
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
                    self.record_raw_accessor_if_matching(&method.sig);
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
                    self.record_raw_accessor_if_matching(&method.sig);

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

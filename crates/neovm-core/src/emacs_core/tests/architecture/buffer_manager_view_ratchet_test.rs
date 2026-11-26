//! Architecture guard and shrink-only ratchet for [`BufferManagerMut`],
//! [`EditRuntimeMut`], and the legacy raw `&mut BufferManager` / `&mut EditRuntime`
//! accessor surface (rt A3 2c (d)).
//!
//! Walks **every production source crate in the workspace** (including
//! `neomacs-layout-engine` and `neomacs`, skipping only test/bench crates and
//! test files/modules using AST-based `#[cfg(test)]` detection that never skips
//! `#[cfg(not(test))]` production code) and enforces six invariants:
//!
//! 1. Neither `BufferManagerMut` nor `EditRuntimeMut` implements `DerefMut`,
//!    `AsMut`, `BorrowMut`, or `IndexMut`.
//! 2. Neither type exposes an `into_manager` or `into_runtime` unwrap method.
//! 3. No safe method on either type returns `&mut Buffer`, `&mut BufferManager`,
//!    or `&mut EditRuntime`, nor hands one to a caller callback (`Fn*` / bare
//!    `fn` parameter or generic `Fn*` bound).
//! 4. `BufferManagerMut` has at most one `unsafe fn` escape hatch
//!    (`with_buffer_mut_no_swap`), and `EditRuntimeMut` has zero `unsafe fn`s.
//! 5. Production call sites of `with_buffer_mut_no_swap` obey a shrink-only
//!    ratchet (`MAX_BUFFER_MUT_NO_SWAP_PRODUCTION_CALLS`).
//! 6. Production functions/methods/trait items across the workspace that return
//!    `&mut BufferManager` or `&mut EditRuntime` (or pass one to a caller
//!    callback) are pinned to a strict shrink-only allowlist
//!    (`ALLOWED_RAW_BUFFER_OR_EDIT_MUT_ACCESSORS`) so no new raw `&mut` surface
//!    can be introduced before Commit B migrates and removes the legacy entries.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use syn::visit::{self, Visit};

use super::architecture_production_source_roots::crates_dir;

const VIEW_TYPES: &[&str] = &["BufferManagerMut", "EditRuntimeMut"];
const FORBIDDEN_VIEW_MUT_TRAITS: &[&str] = &["DerefMut", "AsMut", "BorrowMut", "IndexMut"];
const FORBIDDEN_UNWRAP_METHODS: &[&str] = &["into_manager", "into_runtime"];
const FORBIDDEN_SAFE_MUT_TARGETS: &[&str] = &["Buffer", "BufferManager", "EditRuntime"];
const RAW_ACCESSOR_TARGETS: &[&str] = &["BufferManager", "EditRuntime"];
const ALLOWED_ELEMENT_ESCAPE: &str = "with_buffer_mut_no_swap";

/// Test-only / harness / benchmark workspace crates excluded from production
/// source scanning (mirrors `unsafe_shim_guard_test::EXCLUDED_WORKSPACE_CRATES`
/// plus the dedicated test-only workspace crates).
const EXCLUDED_WORKSPACE_CRATES: &[&str] = &[
    "neomacs-bench",
    "neomacs-gui-tests",
    "neomacs-melpa-test-support",
    "neomacs-melpa-tests",
    "neomacs-test-env",
    "neomacs-test-fonts",
    "neomacs-test-oracle",
    "neomacs-tui-tests",
    "neomacs-xtask",
    "neovm-oracle-tests",
    "xtask",
];

/// Shrink-only production call-site budget for
/// `BufferManagerMut::with_buffer_mut_no_swap`.
///
/// Commit A introduces the view types and guards without migrating callers, so
/// the production count starts at `0`. When Commit B migrates callers that do
/// not yet have dedicated in-place methods, it sets this ceiling to the exact
/// initial audited count and subsequent cleanups must only shrink it.
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

#[derive(Debug)]
struct Violation {
    file: String,
    line: usize,
    detail: String,
}

fn workspace_production_source_files() -> Vec<(String, String, PathBuf)> {
    fn walk(
        crate_name: &str,
        src_root: &Path,
        dir: &Path,
        out: &mut Vec<(String, String, PathBuf)>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut sorted: Vec<_> = entries.filter_map(Result::ok).collect();
        sorted.sort_by_key(|entry| entry.path());
        for entry in sorted {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if matches!(name, "tests" | "test_support" | "restricted_lisp") {
                    continue;
                }
                walk(crate_name, src_root, &path, out);
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
                    path,
                ));
            }
        }
    }

    let root = crates_dir();
    let mut crate_dirs: Vec<_> = std::fs::read_dir(&root)
        .expect("read workspace crates/")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .collect();
    crate_dirs.sort_by_key(|entry| entry.path());

    let mut files = Vec::new();
    for entry in crate_dirs {
        let crate_name = entry
            .file_name()
            .into_string()
            .expect("valid utf-8 crate dir");
        if EXCLUDED_WORKSPACE_CRATES.contains(&crate_name.as_str()) {
            continue;
        }
        let src = entry.path().join("src");
        if src.is_dir() {
            walk(&crate_name, &src, &src, &mut files);
        }
    }
    files
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

fn type_last_ident(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(path) => path.path.segments.last().map(|s| s.ident.to_string()),
        syn::Type::Reference(r) => type_last_ident(&r.elem),
        syn::Type::Group(g) => type_last_ident(&g.elem),
        syn::Type::Paren(p) => type_last_ident(&p.elem),
        _ => None,
    }
}

/// Finds a mutable reference to any target type name (`&mut T`), even nested
/// inside `Option<...>`, `Result<...>`, tuples, etc.
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

/// Checks whether any callback parameter (`impl Fn*(&mut T)`, `dyn Fn*(&mut T)`,
/// bare `fn(&mut T)`, or generic `<F: Fn*(&mut T)>` / `where F: Fn*(&mut T)`)
/// hands a `&mut T` to a caller callback.
struct CallbackArgMutTargetFinder<'a> {
    targets: &'a [&'a str],
    found: Option<String>,
}

impl<'ast> Visit<'ast> for CallbackArgMutTargetFinder<'_> {
    fn visit_type_fn_ptr(&mut self, bare: &'ast syn::TypeFnPtr) {
        if self.found.is_some() {
            return;
        }
        for arg in &bare.inputs {
            if let Some(hit) = find_mut_target_in_type(&arg.ty, self.targets) {
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
            if let Some(hit) = find_mut_target_in_type(&input.ty, self.targets) {
                self.found = Some(hit);
                return;
            }
        }
        visit::visit_parenthesized_generic_arguments(self, args);
    }
}

fn find_mut_target_in_callbacks(sig: &syn::Signature, targets: &[&str]) -> Option<String> {
    let mut finder = CallbackArgMutTargetFinder {
        targets,
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

#[derive(Default)]
struct Scanner {
    file: String,
    violations: Vec<Violation>,
    buffer_manager_mut_impls: usize,
    edit_runtime_mut_impls: usize,
    buffer_manager_mut_unsafe_fns: Vec<(String, String, usize)>,
    edit_runtime_mut_unsafe_fns: Vec<(String, String, usize)>,
    escape_call_sites: Vec<(String, usize)>,
    raw_accessor_sites: Vec<(String, String, usize)>,
}

impl Scanner {
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
}

impl<'ast> Visit<'ast> for Scanner {
    fn visit_item_mod(&mut self, item_mod: &'ast syn::ItemMod) {
        if has_cfg_test(&item_mod.attrs) {
            return;
        }
        visit::visit_item_mod(self, item_mod);
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
            if let syn::TraitItem::Fn(method) = item
                && !has_cfg_test(&method.attrs)
            {
                self.record_raw_accessor_if_matching(&method.sig);
            }
        }
        visit::visit_item_trait(self, item_trait);
    }

    fn visit_item_impl(&mut self, item_impl: &'ast syn::ItemImpl) {
        if has_cfg_test(&item_impl.attrs) {
            return;
        }
        let view_name =
            type_last_ident(&item_impl.self_ty).filter(|name| VIEW_TYPES.contains(&name.as_str()));

        if let Some(self_name) = &view_name {
            match self_name.as_str() {
                "BufferManagerMut" => self.buffer_manager_mut_impls += 1,
                "EditRuntimeMut" => self.edit_runtime_mut_impls += 1,
                _ => {}
            }

            if let Some((trait_path, _)) = &item_impl.trait_
                && let Some(trait_ident) = trait_path.segments.last().map(|s| s.ident.to_string())
                && FORBIDDEN_VIEW_MUT_TRAITS.contains(&trait_ident.as_str())
            {
                self.violations.push(Violation {
                    file: self.file.clone(),
                    line: item_impl.impl_token.span.start().line,
                    detail: format!("{self_name} must not implement {trait_ident}"),
                });
            }
        }

        for item in &item_impl.items {
            let syn::ImplItem::Fn(method) = item else {
                continue;
            };
            if has_cfg_test(&method.attrs) {
                continue;
            }
            self.record_raw_accessor_if_matching(&method.sig);

            let Some(self_name) = &view_name else {
                continue;
            };
            let method_name = method.sig.ident.to_string();
            let line = method.sig.ident.span().start().line;

            if FORBIDDEN_UNWRAP_METHODS.contains(&method_name.as_str()) {
                self.violations.push(Violation {
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
                if let Some(target) =
                    find_mut_target_in_return(&method.sig.output, FORBIDDEN_SAFE_MUT_TARGETS)
                {
                    self.violations.push(Violation {
                        file: self.file.clone(),
                        line,
                        detail: format!(
                            "safe method {self_name}::{method_name} returns `&mut {target}`"
                        ),
                    });
                }
                if let Some(target) =
                    find_mut_target_in_callbacks(&method.sig, FORBIDDEN_SAFE_MUT_TARGETS)
                {
                    self.violations.push(Violation {
                        file: self.file.clone(),
                        line,
                        detail: format!(
                            "safe method {self_name}::{method_name} exposes `&mut {target}` via callback"
                        ),
                    });
                }
            }
        }

        visit::visit_item_impl(self, item_impl);
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

#[test]
fn buffer_manager_view_surface_and_escape_ratchet() {
    let mut scanner = Scanner::default();
    let mut scanned_crates = BTreeSet::new();

    for (crate_name, workspace_rel, path) in workspace_production_source_files() {
        scanned_crates.insert(crate_name);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("read {workspace_rel}: {err}"));
        let ast =
            syn::parse_file(&text).unwrap_or_else(|err| panic!("parse {workspace_rel}: {err}"));
        scanner.file = workspace_rel;
        scanner.visit_file(&ast);
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
            .map(|v| format!("  {}:{}: {}", v.file, v.line, v.detail))
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
            fn deref_mut(&mut self) -> &mut BufferManager { todo!() }
        }

        #[cfg(not(any(test, feature = "test-support")))]
        fn prod_escape_caller(mut view: BufferManagerMut<'_>, id: BufferId) {
            unsafe { let _ = view.with_buffer_mut_no_swap(id, |_| ()); }
        }

        impl AsMut<BufferManager> for BufferManagerMut<'_> {
            fn as_mut(&mut self) -> &mut BufferManager { todo!() }
        }

        impl BorrowMut<EditRuntime> for EditRuntimeMut<'_> {
            fn borrow_mut(&mut self) -> &mut EditRuntime { todo!() }
        }

        impl IndexMut<BufferId> for BufferManagerMut<'_> {
            fn index_mut(&mut self, _id: BufferId) -> &mut Self::Output { todo!() }
        }

        impl BufferManagerMut<'_> {
            pub fn with_buf_cb(&mut self, _f: impl FnOnce(&mut Buffer)) {}
            pub fn with_mgr_generic<F>(&mut self, _f: F)
            where
                F: FnMut(&mut BufferManager),
            {}
        }

        impl Context {
            pub fn extra_buffers_mut(&mut self) -> &mut BufferManager { todo!() }
        }

        trait ExtraHost {
            fn extra_edit_mut(&mut self) -> &mut EditRuntime;
        }

        #[cfg(test)]
        mod skipped_tests {
            impl std::ops::DerefMut for BufferManagerMut<'_> {
                fn deref_mut(&mut self) -> &mut BufferManager { todo!() }
            }
            fn test_escape(mut view: BufferManagerMut<'_>, id: BufferId) {
                unsafe { let _ = view.with_buffer_mut_no_swap(id, |_| ()); }
            }
            fn test_raw_accessor(ctx: &mut Context) -> &mut BufferManager { todo!() }
        }
    "#;

    let ast = syn::parse_file(synthetic).expect("parse synthetic");
    let mut scanner = Scanner {
        file: "synthetic/src/lib.rs".to_string(),
        ..Scanner::default()
    };
    scanner.visit_file(&ast);

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

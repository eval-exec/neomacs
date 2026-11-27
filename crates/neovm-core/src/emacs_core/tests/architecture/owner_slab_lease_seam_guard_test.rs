//! `OwnerSlabHandle` and `OwnerSlabLease` are minted only by `owner_bindings.rs`
//! (N-e4p2-slab-lease-seam-guard, `#15397`).
//!
//! This source boundary complements the private-field capability seal on
//! `OwnerSlabHandle` and `OwnerSlabLease`; it reuses the plist seam's code-only
//! scanner so comments, string literals, and `#[cfg(test)]` items obey the same
//! production scanning rule.

use std::path::Path;

use super::super::architecture_production_source_roots::production_source_files;
use super::{code_only, find_all, test_gated_ranges};

const OWNER_FILE: &str = "src/emacs_core/runtime/eval/owner_bindings.rs";

#[derive(Clone, Copy, Debug)]
enum SlabCapability {
    OwnerSlabHandle,
    OwnerSlabLease,
}

impl SlabCapability {
    const ALL: [Self; 2] = [Self::OwnerSlabHandle, Self::OwnerSlabLease];

    fn name(self) -> &'static str {
        match self {
            Self::OwnerSlabHandle => "OwnerSlabHandle",
            Self::OwnerSlabLease => "OwnerSlabLease",
        }
    }
}

/// Match struct-literal construction (`Name {`) or associated `Name::new(`
/// calls for `capability` in comment/literal-stripped code.
fn mint_sites(code: &str, capability: SlabCapability) -> Vec<usize> {
    let name = capability.name();
    find_all(code, name)
        .into_iter()
        .filter(|&offset| {
            // Do not mistake a longer identifier for this capability's name.
            if code[..offset]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
            {
                return false;
            }
            // Do not match `struct OwnerSlabHandle {` or `struct OwnerSlabLease<...> {`.
            let prefix = code[..offset].trim_end();
            if prefix.strip_suffix("struct").is_some_and(|before| {
                before
                    .chars()
                    .next_back()
                    .is_none_or(|ch| !ch.is_alphanumeric() && ch != '_')
            }) {
                return false;
            }
            let tail = code[offset + name.len()..].trim_start();
            if tail.starts_with('{') {
                return true;
            }
            if let Some(after_colons) = tail.strip_prefix("::")
                && let Some(after_new) = after_colons.trim_start().strip_prefix("new")
            {
                return after_new.trim_start().starts_with('(');
            }
            false
        })
        .collect()
}

fn production_sites(source: &str, capability: SlabCapability) -> Vec<usize> {
    let code = code_only(source);
    let ranges = test_gated_ranges(&code);
    mint_sites(&code, capability)
        .into_iter()
        .filter(|offset| {
            !ranges
                .iter()
                .any(|(start, end)| *offset >= *start && *offset <= *end)
        })
        .collect()
}

fn owner_allows(relative: &Path) -> bool {
    relative == Path::new(OWNER_FILE)
}

fn assert_owner_bindings_mint(capability: SlabCapability) {
    let files = production_source_files();
    assert!(
        files.len() > 100,
        "guard must scan every production source tree"
    );
    let mut sites = Vec::new();
    for file in files {
        if file.relative.split('/').any(|part| part == "tests") {
            continue;
        }
        let relative = file.legacy();
        let source = std::fs::read_to_string(&file.path).expect("read source file");
        let code = code_only(&source);
        for offset in production_sites(&source, capability) {
            let line = code[..offset].matches('\n').count() + 1;
            let site = format!("{relative}:{line}");
            assert!(
                owner_allows(Path::new(&relative)),
                "{} minted outside {OWNER_FILE}: {site}",
                capability.name()
            );
            sites.push(site);
        }
    }
    assert!(
        !sites.is_empty(),
        "the guard must observe an actual {OWNER_FILE} {} mint site",
        capability.name()
    );
}

#[test]
fn owner_slab_handle_is_minted_only_by_owner_bindings() {
    assert_owner_bindings_mint(SlabCapability::OwnerSlabHandle);
}

#[test]
fn owner_slab_lease_is_minted_only_by_owner_bindings() {
    assert_owner_bindings_mint(SlabCapability::OwnerSlabLease);
}

fn assert_synthetic_controls(capability: SlabCapability) {
    for source in [
        "fn bypass() { let _ = OwnerSlabHandle { slab, heap }; }",
        "fn bypass() { let _ = crate::owner_bindings::OwnerSlabHandle\n { slab, heap }; }",
        "fn bypass() { let _ = OwnerSlabHandle::new(slab); }",
        "fn bypass() { let _ = OwnerSlabHandle /* bypass */ :: new (slab); }",
    ] {
        let source = source.replace("OwnerSlabHandle", capability.name());
        assert_eq!(
            production_sites(&source, capability).len(),
            1,
            "must detect: {source}"
        );
    }
    for source in [
        "pub struct OwnerSlabHandle { slab: usize }",
        "// OwnerSlabHandle { slab }\nconst S: &str = \"OwnerSlabHandle { slab }\";",
        "/* outer /* OwnerSlabHandle { slab } */ comment */",
        "const S: &str = r###\"OwnerSlabHandle::new(slab)\"###;",
        "#[cfg(test)] fn fixture() { let _ = OwnerSlabHandle { slab }; }",
        "#[cfg(all(test, feature = \"fixture\"))] mod f { fn g() { OwnerSlabHandle::new(s); } }",
        "fn unrelated() { FakeOwnerSlabHandle { slab }; OwnerSlabHandle::lease(&mut h, &mut r); }",
    ] {
        let source = source.replace("OwnerSlabHandle", capability.name());
        assert!(
            production_sites(&source, capability).is_empty(),
            "must not flag: {source}"
        );
    }
}

#[test]
fn owner_slab_lease_seam_guard_controls_detect_synthetic_bypasses() {
    for capability in SlabCapability::ALL {
        assert_synthetic_controls(capability);
    }
    assert!(owner_allows(Path::new(OWNER_FILE)));
    for bypass in [
        "src/emacs_core/runtime/eval/parallel_exec/canonical_plain.rs",
        "src/emacs_core/runtime/eval/parallel_entry/live_admission.rs",
        "src/emacs_core/runtime/eval/world/parallel_roots.rs",
    ] {
        assert!(!owner_allows(Path::new(bypass)), "must reject: {bypass}");
    }
}

//! Opaque keeper World designators and frozen legacy roots are minted only by
//! eval's World owner.
//!
//! This source boundary complements the constructor's unsafe contract; it does
//! not attempt Rust alias resolution. Reuse the plist seam's code-only scanner
//! so comments, literals and test-only items obey the same scanning rule.

use std::path::{Path, PathBuf};

use super::{code_only, find_all, source_files, test_gated_ranges};

const OWNER: &str = "src/emacs_core/runtime/eval/world";

#[derive(Clone, Copy, Debug)]
enum Constructor {
    KeeperWorldDesignator,
    FrozenLegacyRoots,
}

impl Constructor {
    const ALL: [Self; 2] = [Self::KeeperWorldDesignator, Self::FrozenLegacyRoots];

    fn name(self) -> &'static str {
        match self {
            Self::KeeperWorldDesignator => "KeeperWorldDesignator",
            Self::FrozenLegacyRoots => "FrozenLegacyRoots",
        }
    }
}

fn constructor_sites(code: &str, constructor: Constructor) -> Vec<usize> {
    let name = constructor.name();
    find_all(code, name)
        .into_iter()
        .filter(|&offset| {
            // Do not mistake a longer identifier for this capability's name.
            if code[..offset]
                .chars()
                .next_back()
                .is_some_and(|character| character.is_alphanumeric() || character == '_')
            {
                return false;
            }
            let tail = code[offset + name.len()..].trim_start();
            let Some(tail) = tail.strip_prefix("::") else {
                return false;
            };
            let Some(tail) = tail.trim_start().strip_prefix("new") else {
                return false;
            };
            tail.trim_start().starts_with('(')
        })
        .collect()
}

fn production_sites(source: &str, constructor: Constructor) -> Vec<usize> {
    let code = code_only(source);
    let ranges = test_gated_ranges(&code);
    constructor_sites(&code, constructor)
        .into_iter()
        .filter(|offset| {
            !ranges
                .iter()
                .any(|(start, end)| *offset >= *start && *offset <= *end)
        })
        .collect()
}

fn owner_allows(relative: &Path) -> bool {
    relative.starts_with(OWNER)
}

fn assert_world_owner_constructor(constructor: Constructor) {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    source_files(&manifest.join("src"), &mut files);
    assert!(files.len() > 100, "guard must scan the whole crate source");
    let mut constructors = Vec::new();
    for path in files {
        let relative = path.strip_prefix(&manifest).expect("crate source path");
        if relative
            .components()
            .any(|component| component.as_os_str() == "tests")
        {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("read source file");
        let code = code_only(&source);
        for offset in production_sites(&source, constructor) {
            let line = code[..offset].matches('\n').count() + 1;
            let site = format!("{}:{line}", relative.display());
            assert!(
                owner_allows(relative),
                "{}::new outside the eval World owner: {site}",
                constructor.name()
            );
            constructors.push(site);
        }
    }
    assert!(
        !constructors.is_empty(),
        "the guard must observe an actual World-owner {}::new call",
        constructor.name()
    );
}

#[test]
fn keeper_world_designators_are_minted_only_by_the_world_owner() {
    assert_world_owner_constructor(Constructor::KeeperWorldDesignator);
}

#[test]
fn frozen_legacy_roots_are_minted_only_by_the_world_owner() {
    assert_world_owner_constructor(Constructor::FrozenLegacyRoots);
}

fn assert_synthetic_controls(constructor: Constructor) {
    // The fixture inventory is shared by both constructor seams; replacement
    // also exercises the longer-identifier negative control for each name.
    for source in [
        "fn bypass() { unsafe { KeeperWorldDesignator::new(ptr) }; }",
        "fn bypass() { unsafe { crate::tagged::KeeperWorldDesignator :: new (ptr) }; }",
        "fn bypass() { unsafe { KeeperWorldDesignator /* explanation */ ::\n new(ptr) }; }",
    ] {
        let source = source.replace("KeeperWorldDesignator", constructor.name());
        assert_eq!(
            production_sites(&source, constructor).len(),
            1,
            "must detect: {source}"
        );
    }
    for source in [
        "// KeeperWorldDesignator::new(ptr)\nconst S: &str = \"KeeperWorldDesignator::new(ptr)\";",
        "/* outer /* KeeperWorldDesignator::new(ptr) */ comment */",
        "const S: &str = r###\"KeeperWorldDesignator::new(ptr)\"###;",
        "#[cfg(test)] fn fixture() { KeeperWorldDesignator::new(ptr); }",
        "#[cfg(all(test, feature = \"fixture\"))] mod fixture { fn f() { KeeperWorldDesignator::new(ptr); } }",
        "fn unrelated() { OtherKeeperWorldDesignator::new(ptr); KeeperWorldDesignator::new_unchecked(ptr); }",
    ] {
        let source = source.replace("KeeperWorldDesignator", constructor.name());
        assert!(
            production_sites(&source, constructor).is_empty(),
            "must not flag: {source}"
        );
    }
}

#[test]
fn keeper_world_designator_guard_controls_detect_synthetic_bypasses() {
    for constructor in Constructor::ALL {
        assert_synthetic_controls(constructor);
    }
    assert!(owner_allows(Path::new(
        "src/emacs_core/runtime/eval/world/parallel_roots.rs"
    )));
    for bypass in [
        "src/tagged/gc/mutator_registration/collection.rs",
        "src/emacs_core/runtime/eval/world.rs",
        "src/emacs_core/runtime/eval/world_extra/adapter.rs",
    ] {
        assert!(!owner_allows(Path::new(bypass)), "must reject: {bypass}");
    }
}

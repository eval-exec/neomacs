//! Source gate for the dormant E4-S0 executor substrate.
//!
//! Rule: every detachable frame struct in `parallel_exec/frames.rs` is named
//! by `FrameKind::ALL`, and each kind publishes only canonical E3 root groups.
//! The root walk itself is compile-checked (the `Self` destructure in each
//! `FrameRoots` impl has no `..`), so this gate enumerates the set and pins
//! the group vocabulary; the substrate must also stay dormant (no TLS and no
//! mutable statics).

use crate::emacs_core::eval::parallel_exec::frames::{FrameKind, RootGroup};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn module_dir() -> PathBuf {
    neomacs_infra::crate_root!().join("src/emacs_core/runtime/eval/parallel_exec")
}

fn source_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("read substrate directory") {
        let path = entry.expect("substrate entry").path();
        if path.is_dir() {
            source_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

/// Every `pub(crate) struct <Name>Frame` declared in the substrate source.
fn declared_frame_structs() -> BTreeSet<String> {
    let source = std::fs::read_to_string(module_dir().join("frames.rs"))
        .expect("read the frame substrate source");
    let mut declared = BTreeSet::new();
    for line in source.lines() {
        let Some(rest) = line.trim_start().strip_prefix("pub(crate) struct ") else {
            continue;
        };
        let name = rest
            .split(|character: char| !(character.is_alphanumeric() || character == '_'))
            .next()
            .unwrap_or_default();
        if name.ends_with("Frame") {
            assert!(declared.insert(name.to_string()), "duplicate frame {name}");
        }
    }
    declared
}

#[test]
fn architecture_every_detachable_frame_is_enumerated() {
    let declared = declared_frame_structs();
    let enumerated = FrameKind::ALL
        .iter()
        .map(|kind| kind.struct_name().to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        declared, enumerated,
        "a frame struct and FrameKind::ALL must name the same set"
    );
    assert_eq!(declared.len(), FrameKind::ALL.len());
}

#[test]
fn architecture_frame_kinds_publish_only_canonical_root_groups() {
    let canonical = RootGroup::ALL
        .iter()
        .map(|group| group.name())
        .collect::<BTreeSet<_>>();
    assert_eq!(canonical.len(), RootGroup::ALL.len(), "unique group names");
    for kind in FrameKind::ALL {
        assert!(
            !kind.root_groups().is_empty(),
            "{} must publish a root group",
            kind.struct_name()
        );
        for group in kind.root_groups() {
            assert!(
                canonical.contains(group.name()),
                "{} publishes non-canonical group {}",
                kind.struct_name(),
                group.name()
            );
        }
    }
}

#[test]
fn architecture_substrate_stays_dormant() {
    let mut files = Vec::new();
    source_files(&module_dir(), &mut files);
    assert!(!files.is_empty());
    for path in files {
        let source = std::fs::read_to_string(&path).expect("read substrate source");
        assert!(
            !source.contains("thread_local!"),
            "{} must not allocate TLS",
            path.display()
        );
        assert!(
            !source.contains("static mut "),
            "{} must not declare mutable statics",
            path.display()
        );
    }
}

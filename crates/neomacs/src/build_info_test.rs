use super::{BuildProfile, BuildProvenance, SourceRevision, WorktreeState, usable_embedded_value};

#[path = "../build_support.rs"]
mod build_support;

#[test]
fn provenance_format_keeps_source_and_build_domains_explicit() {
    let provenance = BuildProvenance {
        source: SourceRevision::Git {
            sha: "0123456789abcdef0123456789abcdef01234567",
            commit_timestamp: Some("2026-08-28T14:31:05.000000000Z"),
            worktree: WorktreeState::Clean,
        },
        target: "aarch64-apple-darwin",
        profile: BuildProfile("release-pgo"),
        rustc: "1.96.1",
    };
    let mut output = String::new();

    provenance.write_to(&mut output);

    assert_eq!(
        output,
        "Git commit: 0123456789abcdef0123456789abcdef01234567\n\
         Source date: 2026-08-28T14:31:05.000000000Z\n\
         Build: release-pgo for aarch64-apple-darwin with rustc 1.96.1\n"
    );
}

#[test]
fn provenance_marks_a_revision_built_from_modified_tracked_sources() {
    let provenance = BuildProvenance {
        source: SourceRevision::Git {
            sha: "0123456789abcdef0123456789abcdef01234567",
            commit_timestamp: Some("2026-08-28T14:31:05.000000000Z"),
            worktree: WorktreeState::Dirty,
        },
        target: "x86_64-unknown-linux-gnu",
        profile: BuildProfile("dev"),
        rustc: "1.96.1",
    };
    let mut output = String::new();

    provenance.write_to(&mut output);

    assert!(output.contains("Git commit: 0123456789abcdef0123456789abcdef01234567 (dirty)\n"));
}

#[test]
fn provenance_format_names_missing_source_metadata() {
    let provenance = BuildProvenance {
        source: SourceRevision::Unknown,
        target: "x86_64-unknown-linux-gnu",
        profile: BuildProfile("profiling"),
        rustc: "unknown",
    };
    let mut output = String::new();

    provenance.write_to(&mut output);

    assert!(output.contains("Git commit: unknown\n"));
    assert!(output.contains("Source date: unknown\n"));
    assert!(output.contains("Build: profiling for x86_64-unknown-linux-gnu with rustc unknown\n"));
}

#[test]
fn idempotent_build_metadata_is_presented_as_unknown() {
    assert_eq!(
        usable_embedded_value(Some("VERGEN_IDEMPOTENT_OUTPUT")),
        None
    );
    assert_eq!(usable_embedded_value(Some("")), None);
    assert_eq!(usable_embedded_value(None), None);
    assert_eq!(
        usable_embedded_value(Some("release-pgo-gen")),
        Some("release-pgo-gen")
    );
}

#[test]
fn cargo_profile_name_preserves_custom_profiles_and_names_shared_builtin_classes() {
    use std::path::Path;

    let cases = [
        ("target/debug/build/neomacs-hash/out", "dev/test"),
        ("target/release/build/neomacs-hash/out", "release/bench"),
        ("target/release-pgo/build/neomacs-hash/out", "release-pgo"),
        (
            "target/release-pgo-profiling/build/neomacs-hash/out",
            "release-pgo-profiling",
        ),
        (
            "target/release-pgo-gen/build/neomacs-hash/out",
            "release-pgo-gen",
        ),
        ("target/profiling/build/neomacs-hash/out", "profiling"),
    ];

    for (out_dir, expected) in cases {
        let profile = build_support::CargoProfileName::from_build_inputs(Path::new(out_dir), None)
            .expect("valid Cargo OUT_DIR");
        assert_eq!(profile.as_str(), expected);
    }
    assert!(
        build_support::CargoProfileName::from_build_inputs(Path::new("target/debug/out"), None)
            .is_none()
    );
}

#[test]
fn explicit_profile_disambiguates_test_and_bench_and_must_match_the_artifact_directory() {
    use std::path::Path;

    let test = build_support::CargoProfileName::from_build_inputs(
        Path::new("target/debug/build/neomacs-hash/out"),
        Some("test"),
    )
    .expect("test uses Cargo's debug artifact directory");
    let bench = build_support::CargoProfileName::from_build_inputs(
        Path::new("target/release/build/neomacs-hash/out"),
        Some("bench"),
    )
    .expect("bench uses Cargo's release artifact directory");

    assert_eq!(test.as_str(), "test");
    assert_eq!(bench.as_str(), "bench");
    assert!(
        build_support::CargoProfileName::from_build_inputs(
            Path::new("target/debug/build/neomacs-hash/out"),
            Some("release"),
        )
        .is_none()
    );
}

#[test]
fn tracked_worktree_inputs_are_compacted_by_top_level_path() {
    let entries = build_support::tracked_top_level_entries(
        b"Cargo.toml\0crates/neomacs/src/main.rs\0crates/neovm-core/src/lib.rs\0crates/neomacs/build.rs\0",
    )
    .expect("UTF-8 Git paths");

    assert_eq!(
        entries.into_iter().collect::<Vec<_>>(),
        ["Cargo.toml", "crates"]
    );
}

#[test]
fn git_provenance_requires_the_worktree_to_own_neomacs() {
    use std::path::PathBuf;

    let workspace = PathBuf::from("/checkout/neomacs");
    let packaging_repository = PathBuf::from("/checkout");

    assert!(
        build_support::OwnedGitWorktree::from_canonical_roots(
            workspace.clone(),
            &packaging_repository,
            true,
        )
        .is_none()
    );
    assert!(
        build_support::OwnedGitWorktree::from_canonical_roots(
            workspace.clone(),
            &workspace,
            false,
        )
        .is_none()
    );
    let owned =
        build_support::OwnedGitWorktree::from_canonical_roots(workspace.clone(), &workspace, true)
            .expect("Neomacs-owned worktree");
    assert_eq!(owned.as_path(), workspace.as_path());
    assert_eq!(owned.into_path_buf(), workspace);
}

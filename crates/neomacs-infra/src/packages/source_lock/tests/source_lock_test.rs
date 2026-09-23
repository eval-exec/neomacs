
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{
    AUCTEX_RUNTIME_EXCLUDED_PATHS, AUCTEX_RUNTIME_REQUIRED_FILES, LockedPackageCatalog,
    LockedPackageSource, SourceBuild, SourceBuildTools, extract_docstrip_source,
    prepare_cached_source_artifact_with_tools, prepare_source_checkout, prune_auctex_elpa_ignored,
    runtime_tree_file_spec, verify_auctex_runtime_tree,
};
use crate::packages::test_support::{TestRuntime, TestSandbox};

#[test]
fn docstrip_extraction_selects_positive_negative_and_combined_guards() {
    let source = "%<*style>\nbase   \n%<*!active>\ninactive\n%</!active>\n%<*active>\nactive\n%</active>\n%</style>\n%<installer&make>make\n%<installer&!make>interactive\n";

    assert_eq!(
        extract_docstrip_source(source, &["style"]).expect("extract inactive style"),
        "base\ninactive\n"
    );
    assert_eq!(
        extract_docstrip_source(source, &["style", "active"]).expect("extract active style"),
        "base\nactive\n"
    );
    assert_eq!(
        extract_docstrip_source(source, &["installer", "make"])
            .expect("extract combined installer selector"),
        "make\n"
    );
}

#[test]
fn auctex_runtime_tree_honors_elpaignore_and_keeps_recursive_payloads() {
    let fixture = TestSandbox::new("runtime-tree-files").expect("create runtime-tree fixture");
    let checkout = fixture.root().join("checkout");
    for directory in [
        ".git",
        "admin",
        "build-aux",
        "images",
        "latex",
        "style",
        "tests",
    ] {
        fs::create_dir_all(checkout.join(directory)).expect("create source-tree directory");
    }
    for file in [
        "GNUmakefile",
        "README",
        "README.GIT",
        "ChangeLog.1",
        "auctex.el",
        "lpath.el",
    ] {
        fs::write(checkout.join(file), file).expect("write source-tree file");
    }
    fs::write(checkout.join("latex/Makefile.in"), "development input")
        .expect("write nested ignored source");
    for file in AUCTEX_RUNTIME_REQUIRED_FILES {
        let path = checkout.join(file);
        fs::create_dir_all(path.parent().expect("required runtime file has a parent"))
            .expect("create required runtime parent");
        fs::write(path, "runtime payload").expect("write required runtime file");
    }
    fs::write(
        checkout.join(".elpaignore"),
        "*.in\n.elpaignore\nChangeLog.1\nREADME.GIT\nadmin\nbuild-aux\nlpath.el\ntests\n",
    )
    .expect("write hidden package metadata");

    prune_auctex_elpa_ignored(&checkout).expect("apply pinned AUCTeX exclusions");
    verify_auctex_runtime_tree(&checkout).expect("verify exact runtime topology");

    let file_spec = runtime_tree_file_spec(&checkout).expect("enumerate recursive package entries");
    for required in AUCTEX_RUNTIME_REQUIRED_FILES {
        assert!(file_spec.contains(&format!("(:rename \"{required}\" \"{required}\")")));
    }
    for excluded in AUCTEX_RUNTIME_EXCLUDED_PATHS {
        assert!(!checkout.join(excluded).exists());
        assert!(!file_spec.contains(excluded));
    }
}

#[cfg(unix)]
fn initialize_git_repository(directory: &Path, marker: &str) -> (String, String) {
    fs::create_dir_all(directory).expect("create contract Git repository");
    fs::write(directory.join("source.el"), format!(";; {marker}\n"))
        .expect("write contract Git source");
    assert!(
        Command::new("git")
            .args(["init", "--quiet"])
            .arg(directory)
            .status()
            .expect("initialize contract Git repository")
            .success()
    );
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(directory)
            .args(["add", "source.el"])
            .status()
            .expect("stage contract Git source")
            .success()
    );
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(directory)
            .args([
                "-c",
                "user.name=Neomacs MELPA contract",
                "-c",
                "user.email=melpa-contract@invalid",
                "commit",
                "--quiet",
                "-m",
                "contract source",
            ])
            .status()
            .expect("commit contract Git source")
            .success()
    );
    let revision = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("read contract Git revision");
    assert!(revision.status.success());
    let revision = String::from_utf8(revision.stdout).expect("contract Git revision is UTF-8");
    (
        format!(
            "file://{}",
            directory
                .canonicalize()
                .expect("canonicalize contract Git repository")
                .display()
        ),
        revision.trim().to_string(),
    )
}

#[cfg(unix)]
fn source_cache_contract(
    label: &str,
    fail: bool,
) -> (Vec<Result<std::path::PathBuf, String>>, String) {
    use std::os::unix::fs::PermissionsExt;

    let fixture = TestSandbox::new(label).expect("create source cache contract sandbox");
    let marker = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch")
        .as_nanos()
        .to_string();
    let repository = fixture.root().join("repository");
    let (repository, revision) = initialize_git_repository(&repository, &marker);
    let invocation_log = fixture.root().join("invocations");
    let runtime_script = fixture.root().join("fake-emacs");
    fs::write(
        &runtime_script,
        r##"#!/bin/sh
printf '%s\n' invoke >> "$SOURCE_CACHE_INVOCATIONS"
sleep 1
if [ "$SOURCE_CACHE_FAIL" = 1 ]; then
  printf '%s\n' 'source preparation unavailable' >&2
  exit 24
fi
mkdir -p "$NEOMACS_PACKAGE_BUILD_ROOT/packages"
: > "$NEOMACS_PACKAGE_BUILD_ROOT/packages/$NEOMACS_PACKAGE_NAME-$NEOMACS_PACKAGE_VERSION.tar"
printf 'NEOMACS-SOURCE-PACKAGE:ready:%s:%s\n' \
  "$NEOMACS_PACKAGE_NAME" "$NEOMACS_PACKAGE_VERSION"
"##,
    )
    .expect("write fake source package builder");
    fs::set_permissions(&runtime_script, fs::Permissions::from_mode(0o755))
        .expect("make fake source package builder executable");

    let package_name = format!(
        "neomacs-source-{}-{}",
        if fail { "failure" } else { "success" },
        marker
    );
    let source = LockedPackageSource {
        name: &package_name,
        version: "0.0.1",
        upstream_repository: &repository,
        upstream_revision: &revision,
        repository: &repository,
        revision: &revision,
        fallback_repository: None,
        build: SourceBuild::DefaultFiles,
    };
    let tools = SourceBuildTools {
        melpa_repository: &repository,
        melpa_revision: &revision,
        package_build_repository: &repository,
        package_build_revision: &revision,
    };
    let runtime = TestRuntime::new("fake-source-builder", runtime_script)
        .with_env("SOURCE_CACHE_INVOCATIONS", &invocation_log)
        .with_env("SOURCE_CACHE_FAIL", if fail { "1" } else { "0" })
        .with_timeout(Duration::from_secs(30));
    let barrier = std::sync::Barrier::new(3);
    let results = std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            barrier.wait();
            prepare_cached_source_artifact_with_tools(&runtime, source, tools)
        });
        let second = scope.spawn(|| {
            barrier.wait();
            prepare_cached_source_artifact_with_tools(&runtime, source, tools)
        });
        barrier.wait();
        vec![
            first.join().expect("join first source cache caller"),
            second.join().expect("join second source cache caller"),
        ]
    });
    let invocations = fs::read_to_string(invocation_log).expect("read source builder invocations");
    (results, invocations)
}

#[test]
fn package_lock_rejects_a_branch_in_place_of_a_full_revision() {
    let error = LockedPackageCatalog::parse(
            "package\tversion\tupstream\tupstream-revision\trepository\trevision\tfallback-repository\tbuild\tdependencies\n\
             demo\t1.0\thttps://upstream.example.invalid/demo\tmain\thttps://upstream.example.invalid/demo\tmain\thttps://github.com/emacsmirror/demo\tsource-default\t-\n",
        )
        .expect_err("a branch is not an immutable checkout identity");

    assert!(error.contains("full lowercase revision"));
}

#[test]
fn package_lock_accepts_an_exact_shallow_checkout_identity() {
    let catalog = LockedPackageCatalog::parse(
            "package\tversion\tupstream\tupstream-revision\trepository\trevision\tfallback-repository\tbuild\tdependencies\n\
             demo\t1.0\thttps://upstream.example.invalid/demo\t0123456789abcdef0123456789abcdef01234567\thttps://upstream.example.invalid/demo\t0123456789abcdef0123456789abcdef01234567\thttps://github.com/emacsmirror/demo\tsource-default\t-\n",
        )
        .expect("parse an exact source lock");
    let source = catalog.packages[0].source;

    assert_eq!(source.build(), SourceBuild::DefaultFiles);
    assert_eq!(source.repository(), "https://upstream.example.invalid/demo");
    assert_eq!(
        source.fallback_repository(),
        Some("https://github.com/emacsmirror/demo")
    );
    assert_eq!(
        source.revision(),
        "0123456789abcdef0123456789abcdef01234567"
    );
}

#[test]
fn package_lock_keeps_dependencies_with_their_owning_source_row() {
    let catalog = LockedPackageCatalog::parse(
            "package\tversion\tupstream\tupstream-revision\trepository\trevision\tfallback-repository\tbuild\tdependencies\n\
             dependency\t1.0\thttps://example.invalid/dependency\t0123456789abcdef0123456789abcdef01234567\thttps://example.invalid/dependency\t0123456789abcdef0123456789abcdef01234567\t\tsource-default\t-\n\
             root\t2.0\thttps://example.invalid/root\t89abcdef0123456789abcdef0123456789abcdef\thttps://example.invalid/root\t89abcdef0123456789abcdef0123456789abcdef\t\tmelpa-recipe\tdependency\n",
        )
        .expect("parse one package graph");

    assert_eq!(
        catalog
            .install_plan(("root", "2.0"))
            .expect("resolve dependency-first plan")
            .into_iter()
            .map(|source| source.package())
            .collect::<Vec<_>>(),
        [("dependency", "1.0"), ("root", "2.0")]
    );
}

#[test]
fn package_lock_rejects_unsorted_dependency_names() {
    let error = LockedPackageCatalog::parse(
            "package\tversion\tupstream\tupstream-revision\trepository\trevision\tfallback-repository\tbuild\tdependencies\n\
             alpha\t1.0\thttps://example.invalid/alpha\t0123456789abcdef0123456789abcdef01234567\thttps://example.invalid/alpha\t0123456789abcdef0123456789abcdef01234567\t\tsource-default\t-\n\
             root\t1.0\thttps://example.invalid/root\t0123456789abcdef0123456789abcdef01234567\thttps://example.invalid/root\t0123456789abcdef0123456789abcdef01234567\t\tsource-default\tzeta,alpha\n\
             zeta\t1.0\thttps://example.invalid/zeta\t0123456789abcdef0123456789abcdef01234567\thttps://example.invalid/zeta\t0123456789abcdef0123456789abcdef01234567\t\tsource-default\t-\n",
        )
        .expect_err("dependency names must have one canonical order");

    assert!(error.contains("sorted"));
}

#[test]
fn package_lock_rejects_self_dependencies_at_the_owning_row() {
    let error = LockedPackageCatalog::parse(
            "package\tversion\tupstream\tupstream-revision\trepository\trevision\tfallback-repository\tbuild\tdependencies\n\
             recursive\t1.0\thttps://example.invalid/recursive\t0123456789abcdef0123456789abcdef01234567\thttps://example.invalid/recursive\t0123456789abcdef0123456789abcdef01234567\t\tsource-default\trecursive\n",
        )
        .expect_err("a package cannot directly depend on itself");

    assert!(error.contains("depends on itself"));
    assert!(error.contains("line 2"));
}

#[test]
fn package_lock_rejects_unsorted_package_rows() {
    let error = LockedPackageCatalog::parse(
            "package\tversion\tupstream\tupstream-revision\trepository\trevision\tfallback-repository\tbuild\tdependencies\n\
             zeta\t1.0\thttps://example.invalid/zeta\t0123456789abcdef0123456789abcdef01234567\thttps://example.invalid/zeta\t0123456789abcdef0123456789abcdef01234567\t\tsource-default\t-\n\
             alpha\t1.0\thttps://example.invalid/alpha\t0123456789abcdef0123456789abcdef01234567\thttps://example.invalid/alpha\t0123456789abcdef0123456789abcdef01234567\t\tsource-default\t-\n",
        )
        .expect_err("package rows must have one canonical order");

    assert!(error.contains("package rows must be sorted"));
}

#[test]
fn package_lock_requires_an_explicit_empty_dependency_cell() {
    let error = LockedPackageCatalog::parse(
            "package\tversion\tupstream\tupstream-revision\trepository\trevision\tfallback-repository\tbuild\tdependencies\n\
             demo\t1.0\thttps://example.invalid/demo\t0123456789abcdef0123456789abcdef01234567\thttps://example.invalid/demo\t0123456789abcdef0123456789abcdef01234567\t\tsource-default\t\n",
        )
        .expect_err("an empty final field is ambiguous and leaves trailing whitespace");

    assert!(error.contains("use `-` for no dependencies"));
}

#[cfg(unix)]
#[test]
fn source_checkout_uses_the_mirror_when_the_primary_commit_is_unavailable() {
    let fixture = TestSandbox::new("source-fallback-contract").expect("create fallback sandbox");
    let repository = fixture.root().join("mirror");
    let (repository, revision) = initialize_git_repository(&repository, "fallback-contract");
    let missing_repository = format!(
        "file://{}",
        fixture
            .root()
            .join("missing-primary")
            .canonicalize()
            .unwrap_or_else(|_| fixture.root().join("missing-primary"))
            .display()
    );
    let source = LockedPackageSource {
        name: "source-fallback-contract",
        version: "0.0.1",
        upstream_repository: &missing_repository,
        upstream_revision: &revision,
        repository: &missing_repository,
        revision: &revision,
        fallback_repository: Some(&repository),
        build: SourceBuild::DefaultFiles,
    };
    let checkout = fixture.root().join("checkout");

    prepare_source_checkout(source, &checkout, Duration::from_secs(30))
        .expect("fall back to the exact mirrored commit");

    let actual_revision = Command::new("git")
        .arg("-C")
        .arg(&checkout)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("read fallback checkout revision");
    assert!(actual_revision.status.success());
    assert_eq!(
        String::from_utf8(actual_revision.stdout)
            .expect("fallback checkout revision is UTF-8")
            .trim(),
        revision
    );
}

#[cfg(unix)]
#[test]
fn concurrent_source_build_callers_publish_one_successful_preparation() {
    let (results, invocations) = source_cache_contract("source-cache-success-contract", false);

    assert_eq!(results[0], results[1]);
    let artifact = results[0]
        .as_ref()
        .expect("the shared source preparation succeeds");
    assert!(artifact.starts_with(crate::workspace_root().join("tmp/melpa")));
    assert!(!artifact.starts_with(Path::new("/tmp")));
    assert_eq!(
        invocations.lines().count(),
        1,
        "concurrent callers repeated a successful source build"
    );
}

#[cfg(unix)]
#[test]
fn concurrent_source_build_callers_share_one_failed_preparation() {
    let (results, invocations) = source_cache_contract("source-cache-failure-contract", true);

    assert_eq!(results[0], results[1]);
    let error = results[0]
        .as_ref()
        .expect_err("the shared source preparation fails");
    assert!(error.contains("source preparation unavailable"));
    assert_eq!(
        invocations.lines().count(),
        1,
        "concurrent callers retried a known source build failure"
    );
}

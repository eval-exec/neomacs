use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{
    AUCTEX_RUNTIME_EXCLUDED_PATHS, AUCTEX_RUNTIME_REQUIRED_FILES, FETCH_MAX_ATTEMPTS,
    FETCH_RETRY_BACKOFF, FetchFailureClass, LockedPackageCatalog, LockedPackageSource,
    MirrorRepository, SourceAcquisition, SourceBuild, SourceBuildTools, SourceOrigin,
    extract_docstrip_source, prepare_cached_source_artifact_with_tools, prepare_source_checkout,
    prune_auctex_elpa_ignored, run_fetch_with_retry, runtime_tree_file_spec,
    verify_auctex_runtime_tree,
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

    let prepared = prepare_source_checkout(source, &checkout, Duration::from_secs(30))
        .expect("fall back to the exact mirrored commit");
    assert_eq!(
        prepared.origin,
        SourceOrigin::Mirror(MirrorRepository::Recorded(&repository)),
        "the checkout records the mirror that provided the revision"
    );

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

/// A Savannah-hosted GNU row with no recorded fallback derives the
/// per-package emacsmirror repository, which carries the same commit sha as
/// Savannah's elpa.git.  The mirror is typed and derived, never stored as a
/// URL list in the lock.
#[test]
fn savannah_source_derives_the_per_package_mirror() {
    let savannah = "https://git.savannah.gnu.org/git/emacs/elpa.git";
    let revision = "a16e9d8b0952de1badf6da8e652b178a7f6c4498";
    let source = LockedPackageSource {
        name: "csv-mode",
        version: "1.27",
        upstream_repository: savannah,
        upstream_revision: revision,
        repository: savannah,
        revision,
        fallback_repository: None,
        build: SourceBuild::DefaultFiles,
    };
    let origins = SourceAcquisition::for_source(source)
        .origins()
        .collect::<Vec<_>>();
    assert_eq!(
        origins,
        vec![
            SourceOrigin::Savannah(savannah),
            SourceOrigin::Mirror(MirrorRepository::EmacsPackage("csv-mode")),
        ]
    );
    assert_eq!(
        origins[1].repository(),
        "https://github.com/emacsmirror/csv-mode",
        "the mirror URL is derived from the package name"
    );
    assert_eq!(origins[1].to_string(), "Mirror(EmacsPackage(csv-mode))");
}

/// The lock's recorded per-package emacsmirror fallbacks are the typed
/// package-mirror origin, not an opaque URL; a recorded mirror of a different
/// repository stays recorded.
#[test]
fn recorded_emacs_mirror_fallbacks_keep_their_typed_meaning() {
    let auctex = LockedPackageSource {
        name: "auctex",
        version: "14.1.2",
        upstream_repository: "https://git.savannah.gnu.org/git/auctex.git",
        upstream_revision: "5850499d0182b4ca53c078e74711a13995651337",
        repository: "https://git.savannah.gnu.org/git/auctex.git",
        revision: "5850499d0182b4ca53c078e74711a13995651337",
        fallback_repository: Some("https://github.com/emacsmirror/auctex"),
        build: SourceBuild::AuctexRuntime,
    };
    assert_eq!(
        SourceAcquisition::for_source(auctex)
            .origins()
            .collect::<Vec<_>>(),
        vec![
            SourceOrigin::Savannah("https://git.savannah.gnu.org/git/auctex.git"),
            SourceOrigin::Mirror(MirrorRepository::EmacsPackage("auctex")),
        ]
    );

    let emacsql_sqlite = LockedPackageSource {
        name: "emacsql-sqlite",
        version: "20240825.1837",
        upstream_repository: "https://github.com/magit/emacsql",
        upstream_revision: "b9f19ac5e17a90d5b7314d67e3b790992be7d82d",
        repository: "https://github.com/magit/emacsql",
        revision: "b9f19ac5e17a90d5b7314d67e3b790992be7d82d",
        fallback_repository: Some("https://github.com/emacsmirror/emacsql"),
        build: SourceBuild::Files("emacsql-sqlite.el"),
    };
    assert_eq!(
        SourceAcquisition::for_source(emacsql_sqlite)
            .origins()
            .collect::<Vec<_>>(),
        vec![
            SourceOrigin::Recorded("https://github.com/magit/emacsql"),
            SourceOrigin::Mirror(MirrorRepository::Recorded(
                "https://github.com/emacsmirror/emacsql"
            )),
        ],
        "a mirror of a different package repository is recorded, not derived"
    );
}

/// A non-Savannah row without a recorded mirror has exactly one origin.
#[test]
fn non_savannah_source_without_a_fallback_has_one_origin() {
    let source = LockedPackageSource {
        name: "example-package",
        version: "1.0",
        upstream_repository: "https://github.com/example/example-package",
        upstream_revision: "0123456789abcdef0123456789abcdef01234567",
        repository: "https://github.com/example/example-package",
        revision: "0123456789abcdef0123456789abcdef01234567",
        fallback_repository: None,
        build: SourceBuild::DefaultFiles,
    };
    assert_eq!(
        SourceAcquisition::for_source(source)
            .origins()
            .collect::<Vec<_>>(),
        vec![SourceOrigin::Recorded(
            "https://github.com/example/example-package"
        )]
    );
}

/// Both origins are tried before preparation fails, and the failure names
/// every origin with its error.  Deleting the fallback would leave one origin
/// and fail this contract.
#[test]
fn source_unavailable_reports_every_origin_tried() {
    let fixture =
        TestSandbox::new("source-unavailable-contract").expect("create unavailable sandbox");
    let missing_primary = format!(
        "file://{}",
        fixture.root().join("missing-primary").display()
    );
    let missing_mirror = fixture
        .root()
        .join("missing-mirror")
        .to_string_lossy()
        .into_owned();
    let revision = "0123456789abcdef0123456789abcdef01234567";
    let source = LockedPackageSource {
        name: "source-unavailable-contract",
        version: "0.0.1",
        upstream_repository: &missing_primary,
        upstream_revision: revision,
        repository: &missing_primary,
        revision,
        fallback_repository: Some(&missing_mirror),
        build: SourceBuild::DefaultFiles,
    };
    let checkout = fixture.root().join("checkout");

    let error = prepare_source_checkout(source, &checkout, Duration::from_secs(30))
        .expect_err("neither origin provides the revision");
    let rendered = error.to_string();
    assert!(
        rendered.contains("SourceUnavailable { package: \"source-unavailable-contract\""),
        "{rendered}"
    );
    assert!(rendered.contains("origins: [Recorded("), "{rendered}");
    assert!(rendered.contains("Mirror(Recorded("), "{rendered}");
    assert!(
        rendered.contains("Recorded(") && rendered.contains(" failed:"),
        "each origin's failure is reported: {rendered}"
    );
    assert!(
        format!("{error:?}").contains("SourceUnavailable"),
        "the typed failure is what a harness reports: {error:?}"
    );
    assert_eq!(
        error.attempts.len(),
        2,
        "removing an origin must change this contract: {rendered}"
    );
}

/// The exact git stderr from the CI red on main ea687401ee: Savannah answered
/// HTTP 500 while the MELPA package jobs prepared their source checkouts, and
/// the fetch had no tolerance for a transient upstream failure.
const TRANSIENT_HTTP_500: &str = "fatal: unable to access 'https://git.savannah.gnu.org/git/emacs/elpa.git/': The requested URL returned error: 500";

#[test]
fn fetch_failure_classifier_separates_transient_upstream_failures() {
    for transient in [
        TRANSIENT_HTTP_500,
        "fatal: unable to access 'https://example.invalid/repo/': Could not resolve host: example.invalid",
        "fatal: unable to access 'https://example.invalid/repo/': Failed to connect to example.invalid port 443 after 30001 ms: Connection timed out",
        "fatal: unable to access 'https://example.invalid/repo/': Failed to connect to example.invalid port 443 after 30001 ms: Couldn't connect to server",
        "shallow Git fetch timed out after 30s: `git -C /tmp/checkout fetch --depth=1 --no-tags origin 0123456789abcdef0123456789abcdef01234567`",
        "error: RPC failed; curl 56 GnuTLS recv error (-9): Error decoding the received TLS packet.",
        "fatal: the remote end hung up unexpectedly",
        "fatal: early EOF",
        "fatal: unable to access 'https://example.invalid/repo/': The requested URL returned error: 503",
        "fatal: unable to access 'https://example.invalid/repo/': The requested URL returned error: 429",
    ] {
        assert_eq!(
            FetchFailureClass::classify(transient),
            FetchFailureClass::Transient,
            "expected a transient classification for: {transient}"
        );
    }
}

#[test]
fn fetch_failure_classifier_keeps_request_errors_permanent() {
    for permanent in [
        "fatal: couldn't find remote ref 0123456789abcdef0123456789abcdef01234567",
        "fatal: unable to access 'https://example.invalid/repo/': The requested URL returned error: 404",
        "fatal: Authentication failed for 'https://github.com/example/repo/'",
        "fatal: could not read Username for 'https://github.com': terminal prompts disabled",
        "remote: Repository not found.\nfatal: repository 'https://github.com/example/missing/' not found",
        "fatal: some unexpected git failure with no retryable marker",
    ] {
        assert_eq!(
            FetchFailureClass::classify(permanent),
            FetchFailureClass::Permanent,
            "expected a permanent classification for: {permanent}"
        );
    }
}

#[test]
fn fetch_retry_retries_transient_failures_before_succeeding() {
    assert_eq!(
        FETCH_RETRY_BACKOFF,
        [
            Duration::from_secs(2),
            Duration::from_secs(5),
            Duration::from_secs(15),
        ],
        "the bounded retry policy is part of the contract"
    );
    let no_backoff = [Duration::ZERO; FETCH_MAX_ATTEMPTS - 1];
    let mut attempts = 0;
    let result = run_fetch_with_retry(&no_backoff, |_| {
        attempts += 1;
        if attempts < 3 {
            Err(TRANSIENT_HTTP_500.to_string())
        } else {
            Ok("fetched".to_string())
        }
    });
    assert_eq!(
        result.expect("two transient failures followed by success"),
        "fetched"
    );
    assert_eq!(attempts, 3, "the transient failure is retried");
}

#[test]
fn fetch_retry_does_not_repeat_a_permanent_failure() {
    let no_backoff = [Duration::ZERO; FETCH_MAX_ATTEMPTS - 1];
    let mut attempts = 0;
    let error = run_fetch_with_retry(&no_backoff, |_| {
        attempts += 1;
        Err("fatal: couldn't find remote ref 0123456789abcdef0123456789abcdef01234567".to_string())
    })
    .expect_err("a permanent request error must not be retried");
    assert_eq!(attempts, 1, "a permanent failure is attempted exactly once");
    assert!(error.contains("after 1 attempt"), "{error}");
    assert!(error.contains("couldn't find remote ref"), "{error}");
}

#[test]
fn fetch_retry_bounds_transient_attempts_and_keeps_every_attempt() {
    let no_backoff = [Duration::ZERO; FETCH_MAX_ATTEMPTS - 1];
    let mut attempts = 0;
    let error = run_fetch_with_retry(&no_backoff, |index| {
        attempts += 1;
        Err(format!("attempt marker {index}: {TRANSIENT_HTTP_500}"))
    })
    .expect_err("the bounded attempt budget is exhausted");
    assert_eq!(attempts, FETCH_MAX_ATTEMPTS);
    assert!(error.contains("after 4 attempts"), "{error}");
    for index in 0..FETCH_MAX_ATTEMPTS {
        assert!(
            error.contains(&format!("attempt marker {index}:")),
            "the final report must keep attempt {index}: {error}"
        );
    }
}

#[cfg(unix)]
#[test]
fn concurrent_source_build_callers_publish_one_successful_preparation() {
    let (results, invocations) = source_cache_contract("source-cache-success-contract", false);

    assert_eq!(results[0], results[1]);
    let artifact = results[0]
        .as_ref()
        .expect("the shared source preparation succeeds");
    assert!(artifact.starts_with(crate::melpa_cache_root()));
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

/// The account-free Telega GUI fixture mounts the real Telega Lisp frontend
/// from a pinned MELPA source.  The lock row must therefore carry the exact
/// upstream revision and the full dependency closure Telega's
/// `Package-Requires' names, so an offline scenario never resolves a branch,
/// the user's package directory, or an unpinned dependency.
#[test]
fn telega_frontend_pin_resolves_to_the_pinned_source_and_dependency_closure() {
    let pin = crate::packages::TELEGA_PIN;
    let source = crate::packages::locked_melpa_source(pin)
        .expect("telega has a revision-pinned source lock");
    assert_eq!(
        source.upstream_repository(),
        "https://github.com/zevlg/telega.el"
    );
    assert_eq!(
        source.upstream_revision(),
        "a6abce419828fc63c7698c7d4951614e8d12d2ff"
    );
    assert_eq!(source.build(), SourceBuild::MelpaRecipe);

    let plan =
        crate::packages::locked_melpa_install_plan(pin).expect("telega install plan resolves");
    let names = plan
        .iter()
        .map(|locked| locked.package().0)
        .collect::<Vec<_>>();
    for dependency in ["telega", "transient", "visual-fill-column"] {
        assert!(
            names.contains(&dependency),
            "telega install plan is missing {dependency}: {names:?}"
        );
    }
    assert_eq!(
        names.last(),
        Some(&"telega"),
        "the install plan must end with the root package: {names:?}"
    );
}

//! Regression tests for the source install cache's readiness contract.
//!
//! A scenario hands one prepared package tree to more than one editor at a
//! time (the batch oracle runs GNU Emacs and Neomacs concurrently).  Some
//! packages build a native module from their own load path -- `sqlite3.el`
//! runs `make all` on first load -- so two editors that both find the module
//! missing would link the same object file twice and one would fail.  The
//! cache therefore runs those load-time builds itself, under the cache lock,
//! before publishing readiness.  The fixture below reproduces the hazard with
//! a native-source package whose load path builds a module in place, and the
//! scenario side is a forced rendezvous: without the warm phase both editors
//! build, deterministically, rather than by timing.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use neomacs_melpa_test_support::{
    EmacsRuntime, LockedPackageSource, MelpaSandbox, SourceBuild, elisp_string,
    output_with_timeout_pinned, prepare_cached_locked_package_plan,
};

const FIXTURE_VERSION: &str = "1.0";
const SCENARIO_WAIT_ENV: &str = "NEOMACS_FIXTURE_WAIT_FOR_PAIR";
const SCENARIO_LOADED_MARKER: &str = "NEOMACS-NATIVE-CACHE-FIXTURE:loaded";
const SCENARIO_DEADLINE_SECONDS: u64 = 60;
const SCENARIO_TIMEOUT: Duration = Duration::from_secs(180);

fn unique_marker() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch")
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
}

fn run_git(repository: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .status()
        .expect("run git for the native-cache fixture");
    assert!(status.success(), "git {arguments:?} failed");
}

/// A package whose first load builds `<name>.so` in its own directory.
///
/// Both scenario editors append a start record and wait until both are
/// present before building, so concurrent scenarios enter the build together
/// even when one editor starts later than the other.  The install cache's
/// warm phase loads the package while it holds the cache lock, so the module
/// is already in the tree and scenarios never enter that rendezvous.
fn fixture_source(package: &str, build_log: &Path, start_log: &Path) -> String {
    let module = format!("{package}.so");
    format!(
        r#";;; {package}.el --- native build cache fixture -*- lexical-binding: t; -*-
;; Version: {FIXTURE_VERSION}
;; Package-Requires: ((emacs "27.1"))
;;; Commentary:
;; Harness fixture.  Its load path builds a native module in place; the
;; install cache must run that build once, before two scenarios load it.
;;; Code:

(defconst {package}--build-log {build_log})
(defconst {package}--start-log {start_log})
(defconst {package}--module
  (expand-file-name {module_literal}
                    (file-name-directory (or load-file-name buffer-file-name))))

(defun {package}--lines (file)
  (if (file-exists-p file)
      (with-temp-buffer
        (insert-file-contents file)
        (count-lines (point-min) (point-max)))
    0))

(defun {package}--append (file line)
  (write-region line nil file 'append))

(unless (file-exists-p {package}--module)
  (when (getenv {wait_env})
    ({package}--append {package}--start-log "started\n")
    (let ((deadline (+ (float-time) {deadline})))
      (while (and (< ({package}--lines {package}--start-log) 2)
                  (< (float-time) deadline))
        (sleep-for 0.01))))
  ({package}--append {package}--build-log "built\n")
  (with-temp-file {package}--module
    (insert "fixture native module\n")))

(provide '{package})
;;; {package}.el ends here
"#,
        build_log = elisp_string(&build_log.to_string_lossy()),
        start_log = elisp_string(&start_log.to_string_lossy()),
        module_literal = elisp_string(&module),
        wait_env = elisp_string(SCENARIO_WAIT_ENV),
        deadline = SCENARIO_DEADLINE_SECONDS,
    )
}

fn line_count(path: &Path) -> usize {
    fs::read_to_string(path)
        .map(|contents| contents.lines().count())
        .unwrap_or(0)
}

fn scenario_load(
    package_dir: &Path,
    package: &str,
    label: &str,
) -> Result<std::process::Output, String> {
    let elpa = package_dir.parent().ok_or_else(|| {
        format!(
            "prepared package directory {} has no root",
            package_dir.display()
        )
    })?;
    let source = package_dir.join(format!("{package}.el"));
    let form = format!(
        r#"(progn
               (require 'package)
               (setq package-user-dir {})
               (package-initialize)
               (load {} nil t t)
               (princ {}))"#,
        elisp_string(&elpa.to_string_lossy()),
        elisp_string(&source.to_string_lossy()),
        elisp_string(SCENARIO_LOADED_MARKER),
    );
    let sandbox = MelpaSandbox::new(label)?;
    let runtime = EmacsRuntime::gnu_emacs();
    let mut command = sandbox.command(&runtime);
    command
        .env(SCENARIO_WAIT_ENV, "1")
        .args(["--batch", "--quick", "--eval", &form]);
    output_with_timeout_pinned(&mut command, SCENARIO_TIMEOUT)
        .map_err(|error| format!("scenario editor {label} failed: {error:?}"))
}

#[test]
fn concurrent_scenarios_build_a_native_module_once() {
    let marker = unique_marker();
    let package = format!("neomacs-native-cache-fixture-{marker}");
    let fixture_root = neomacs_infra::melpa_cache_root().join(format!("native-cache-{marker}"));
    let build_log = fixture_root.join("builds");
    let start_log = fixture_root.join("starts");
    fs::create_dir_all(&fixture_root).expect("create native-cache fixture root");

    let repository = fixture_root.join("repository");
    fs::create_dir_all(&repository).expect("create native-cache fixture repository");
    fs::write(
        repository.join(format!("{package}.el")),
        fixture_source(&package, &build_log, &start_log),
    )
    .expect("write fixture package source");
    fs::write(
        repository.join(format!("{package}.c")),
        "int neomacs_native_cache_fixture(void) { return 0; }\n",
    )
    .expect("write fixture native source");
    run_git(&repository, &["init", "--quiet"]);
    run_git(&repository, &["add", &format!("{package}.el")]);
    run_git(&repository, &["add", &format!("{package}.c")]);
    run_git(
        &repository,
        &[
            "-c",
            "user.name=Neomacs native cache fixture",
            "-c",
            "user.email=native-cache@invalid",
            "commit",
            "--quiet",
            "-m",
            "native cache fixture",
        ],
    );
    let revision = Command::new("git")
        .arg("-C")
        .arg(&repository)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("read native-cache fixture revision");
    assert!(revision.status.success());
    let revision = String::from_utf8(revision.stdout)
        .expect("fixture revision is UTF-8")
        .trim()
        .to_string();
    let repository = repository.to_string_lossy().to_string();

    // The source has a native extension and the recipe stages it, so the
    // cached tree is exactly the "native sources, no artifact" shape that
    // the warm phase exists for.
    let source = LockedPackageSource::new(
        &package,
        FIXTURE_VERSION,
        &repository,
        &revision,
        &repository,
        &revision,
        None,
        // `*` is rejected by package-build's recipe validation; `*.*` stages
        // both the fixture's `.el` and its native `.c` source, which the
        // default files spec would drop.
        SourceBuild::Files("*.*"),
    );
    let plan = [source];
    let driver = EmacsRuntime::gnu_emacs();

    // Two scenarios race to prepare the same cache key.  The cache lock
    // serializes them and only the winner runs the install and warm phase.
    let (first, second) = thread::scope(|scope| {
        let first = scope.spawn(|| prepare_cached_locked_package_plan(&driver, &plan));
        let second = scope.spawn(|| prepare_cached_locked_package_plan(&driver, &plan));
        (
            first.join().expect("join first cache preparation"),
            second.join().expect("join second cache preparation"),
        )
    });
    let first = first.expect("first scenario prepares the shared cache");
    let second = second.expect("second scenario prepares the shared cache");
    assert_eq!(
        first.package_dir(),
        second.package_dir(),
        "both scenarios must observe one cache entry"
    );

    let package_dir = first.package_dir().to_path_buf();
    let module = package_dir.join(format!("{package}.so"));

    // Two editors now load the prepared tree concurrently, as the batch
    // oracle does for GNU and Neomacs.  The scenario side is a forced
    // rendezvous: without the warm phase both loads find no module, record
    // their start, wait for each other, and both build.  With the warm phase
    // the module is already in the cached tree, so neither load may enter the
    // rendezvous at all.
    let (first_load, second_load) = thread::scope(|scope| {
        let first = scope.spawn(|| scenario_load(&package_dir, &package, "native-cache-a"));
        let second = scope.spawn(|| scenario_load(&package_dir, &package, "native-cache-b"));
        (
            first.join().expect("join first scenario load"),
            second.join().expect("join second scenario load"),
        )
    });
    for output in [first_load, second_load] {
        let output = output.expect("scenario load runs");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && stdout.contains(SCENARIO_LOADED_MARKER),
            "scenario load failed\nstatus: {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            output.status.code()
        );
    }
    assert!(
        module.is_file(),
        "the warm phase must leave the native module in the cached tree at {}",
        module.display()
    );
    assert_eq!(
        line_count(&build_log),
        1,
        "the shared cache must build the native module exactly once; build log:\n{}",
        fs::read_to_string(&build_log).unwrap_or_default()
    );
    assert_eq!(
        line_count(&start_log),
        0,
        "scenarios must not enter the forced build rendezvous; start log:\n{}",
        fs::read_to_string(&start_log).unwrap_or_default()
    );
}

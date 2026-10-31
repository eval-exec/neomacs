use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use sha2::{Digest, Sha256};

use super::install::{
    InstallCommandError, PackageInstallDriver, elisp_string, output_with_timeout,
    package_environment_with_runtime, package_preparation_run_id,
    publish_package_preparation_failure,
};
use crate::workspace_root;

const LOCKED_PACKAGE_MANIFEST: &str = include_str!("melpa-package-lock.tsv");
static LOCKED_PACKAGE_CATALOG: OnceLock<Result<LockedPackageCatalog, String>> = OnceLock::new();

/// Exact MELPA transaction used by the account-free Clatter notification fixture.
pub const CLATTER_PIN: (&str, &str) = ("clatter", "20261007.1819");

/// The pinned Telega frontend the account-free GUI fixture mounts.  Tests and
/// the fixture harness must use this pin instead of a literal so the lock row
/// and its consumers cannot drift apart.
pub const TELEGA_PIN: (&str, &str) = ("telega", "20261002.1709");

const MELPA_RECIPE_REPOSITORY: &str = "https://github.com/melpa/melpa";
const MELPA_RECIPE_REVISION: &str = "517749e477c16c0437cae029be71e672061a6c19";
const PACKAGE_BUILD_REPOSITORY: &str = "https://github.com/melpa/package-build";
const PACKAGE_BUILD_REVISION: &str = "d31dec67631f14ef8be3ad6438e172a07298082b";
pub const SHALLOW_GIT_FETCH_ARGS: &[&str] = &["fetch", "--depth=1", "--no-tags"];

type PackagePin = (&'static str, &'static str);

#[derive(Clone, Copy)]
struct SourceBuildTools<'a> {
    melpa_repository: &'a str,
    melpa_revision: &'a str,
    package_build_repository: &'a str,
    package_build_revision: &'a str,
}

const SOURCE_BUILD_TOOLS: SourceBuildTools<'static> = SourceBuildTools {
    melpa_repository: MELPA_RECIPE_REPOSITORY,
    melpa_revision: MELPA_RECIPE_REVISION,
    package_build_repository: PACKAGE_BUILD_REPOSITORY,
    package_build_revision: PACKAGE_BUILD_REVISION,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceBuild<'a> {
    MelpaRecipe,
    DefaultFiles,
    AuctexRuntime,
    Files(&'a str),
}

impl SourceBuild<'_> {
    fn identity(self) -> String {
        match self {
            // Bump this identity whenever the deterministic docstrip or
            // `.elpaignore` preparation contract changes.
            Self::AuctexRuntime => "AuctexRuntime(v3)".to_string(),
            build => format!("{build:?}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LockedPackageSource<'a> {
    name: &'a str,
    version: &'a str,
    upstream_repository: &'a str,
    upstream_revision: &'a str,
    repository: &'a str,
    revision: &'a str,
    fallback_repository: Option<&'a str>,
    build: SourceBuild<'a>,
}

impl<'a> LockedPackageSource<'a> {
    /// Assemble a recorded source row from its identity fields.
    ///
    /// Production rows come from the locked manifest ([`locked_melpa_source`]).
    /// Harness regression fixtures use this constructor to drive the same
    /// install cache and artifact build against a local repository.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        name: &'a str,
        version: &'a str,
        upstream_repository: &'a str,
        upstream_revision: &'a str,
        repository: &'a str,
        revision: &'a str,
        fallback_repository: Option<&'a str>,
        build: SourceBuild<'a>,
    ) -> Self {
        Self {
            name,
            version,
            upstream_repository,
            upstream_revision,
            repository,
            revision,
            fallback_repository,
            build,
        }
    }

    pub const fn package(self) -> (&'a str, &'a str) {
        (self.name, self.version)
    }

    pub const fn upstream_repository(self) -> &'a str {
        self.upstream_repository
    }

    pub const fn upstream_revision(self) -> &'a str {
        self.upstream_revision
    }

    pub const fn repository(self) -> &'a str {
        self.repository
    }

    pub const fn revision(self) -> &'a str {
        self.revision
    }

    pub const fn fallback_repository(self) -> Option<&'a str> {
        self.fallback_repository
    }

    pub const fn build(self) -> SourceBuild<'a> {
        self.build
    }

    fn identity(self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            self.name,
            self.version,
            self.upstream_repository,
            self.upstream_revision,
            self.repository,
            self.revision,
            self.fallback_repository.unwrap_or_default(),
            self.build.identity()
        )
    }
}

#[derive(Debug)]
struct LockedPackage {
    source: LockedPackageSource<'static>,
    dependencies: Vec<PackagePin>,
}

#[derive(Debug)]
struct LockedPackageCatalog {
    packages: Vec<LockedPackage>,
}

impl LockedPackageCatalog {
    fn parse(manifest: &'static str) -> Result<Self, String> {
        let mut lines = manifest.lines();
        if lines.next()
            != Some(
                "package\tversion\tupstream\tupstream-revision\trepository\trevision\tfallback-repository\tbuild\tdependencies",
            )
        {
            return Err("locked package manifest has an invalid header".to_string());
        }

        let mut unresolved = Vec::new();
        let mut pins = BTreeMap::new();
        let mut previous_package_name = None;
        for (index, line) in lines.enumerate() {
            let line_number = index + 2;
            let fields = line.split('\t').collect::<Vec<_>>();
            let [
                name,
                version,
                upstream_repository,
                upstream_revision,
                repository,
                revision,
                fallback_repository,
                build,
                dependency_names,
            ] = fields.as_slice()
            else {
                return Err(format!(
                    "locked package manifest line {line_number} is invalid"
                ));
            };
            if dependency_names.is_empty() {
                return Err(format!(
                    "locked package manifest line {line_number} must use `-` for no dependencies"
                ));
            }
            let build = match *build {
                "melpa-recipe" => SourceBuild::MelpaRecipe,
                "source-default" => SourceBuild::DefaultFiles,
                "source-auctex-runtime" => SourceBuild::AuctexRuntime,
                build => match build.strip_prefix("source-glob:") {
                    Some(path) if safe_source_path(path) => SourceBuild::Files(path),
                    _ => {
                        return Err(format!(
                            "locked package manifest line {line_number} has invalid build rule `{build}`"
                        ));
                    }
                },
            };
            if !safe_package_pin(name, version)
                || !upstream_repository.starts_with("https://")
                || upstream_repository.contains(['\n', '\r', '\t'])
                || !full_lowercase_revision(upstream_revision)
                || !repository.starts_with("https://")
                || repository.contains(['\n', '\r', '\t'])
                || !full_lowercase_revision(revision)
                || (!fallback_repository.is_empty()
                    && (!fallback_repository.starts_with("https://")
                        || fallback_repository.contains(['\n', '\r', '\t'])
                        || fallback_repository == repository))
            {
                return Err(format!(
                    "locked package manifest line {line_number} must contain a safe exact pin, HTTPS provenance/acquisition repositories, and full lowercase revisions"
                ));
            }
            if previous_package_name.is_some_and(|previous| previous >= *name) {
                return Err(format!(
                    "locked package manifest line {line_number} package rows must be sorted and unique"
                ));
            }
            let pin = (*name, *version);
            pins.insert(*name, pin);
            previous_package_name = Some(*name);
            unresolved.push((
                LockedPackageSource {
                    name,
                    version,
                    upstream_repository,
                    upstream_revision,
                    repository,
                    revision,
                    fallback_repository: (!fallback_repository.is_empty())
                        .then_some(*fallback_repository),
                    build,
                },
                *dependency_names,
                line_number,
            ));
        }
        if unresolved.is_empty() {
            return Err("locked package manifest is empty".to_string());
        }

        let mut packages = Vec::with_capacity(unresolved.len());
        for (source, dependency_names, line_number) in unresolved {
            let mut dependencies = Vec::new();
            let mut previous_dependency = None;
            if dependency_names != "-" {
                for dependency_name in dependency_names.split(',') {
                    if !safe_package_name(dependency_name) {
                        return Err(format!(
                            "locked package manifest line {line_number} has invalid dependency name `{dependency_name}`"
                        ));
                    }
                    if dependency_name == source.name {
                        return Err(format!(
                            "locked package manifest line {line_number} package {dependency_name} depends on itself"
                        ));
                    }
                    if previous_dependency.is_some_and(|previous| previous >= dependency_name) {
                        return Err(format!(
                            "locked package manifest line {line_number} dependency names must be sorted and unique"
                        ));
                    }
                    dependencies.push(pins.get(dependency_name).copied().ok_or_else(|| {
                        format!(
                            "locked package manifest line {line_number} names dependency {dependency_name}, which has no package row"
                        )
                    })?);
                    previous_dependency = Some(dependency_name);
                }
            }
            packages.push(LockedPackage {
                source,
                dependencies,
            });
        }

        Ok(Self { packages })
    }

    fn install_plan(
        &self,
        package: (&str, &str),
    ) -> Result<Vec<LockedPackageSource<'static>>, String> {
        fn visit(
            package: PackagePin,
            packages: &[LockedPackage],
            visiting: &mut BTreeSet<PackagePin>,
            visited: &mut BTreeSet<PackagePin>,
            plan: &mut Vec<LockedPackageSource<'static>>,
        ) -> Result<(), String> {
            if visited.contains(&package) {
                return Ok(());
            }
            if !visiting.insert(package) {
                return Err(format!(
                    "locked package dependency cycle includes {} {}",
                    package.0, package.1
                ));
            }
            let locked_package = packages
                .iter()
                .find(|candidate| candidate.source.package() == package)
                .ok_or_else(|| {
                    format!(
                        "exact package {} {} has no revision-pinned source lock",
                        package.0, package.1
                    )
                })?;
            for dependency in &locked_package.dependencies {
                visit(*dependency, packages, visiting, visited, plan)?;
            }
            visiting.remove(&package);
            visited.insert(package);
            plan.push(locked_package.source);
            Ok(())
        }

        let root = self
            .packages
            .iter()
            .find(|candidate| candidate.source.package() == package)
            .ok_or_else(|| {
                format!(
                    "exact package {} {} has no revision-pinned source lock",
                    package.0, package.1
                )
            })?;
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        let mut plan = Vec::new();
        visit(
            root.source.package(),
            &self.packages,
            &mut visiting,
            &mut visited,
            &mut plan,
        )?;
        Ok(plan)
    }
}

fn locked_package_catalog() -> Result<&'static LockedPackageCatalog, String> {
    LOCKED_PACKAGE_CATALOG
        .get_or_init(|| LockedPackageCatalog::parse(LOCKED_PACKAGE_MANIFEST))
        .as_ref()
        .map_err(Clone::clone)
}

fn safe_package_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '+' | '@')
        })
}

fn safe_package_pin(name: &str, version: &str) -> bool {
    safe_package_name(name)
        && !version.is_empty()
        && version.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+')
        })
}

fn full_lowercase_revision(revision: &str) -> bool {
    revision.len() == 40
        && revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn safe_source_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\n', '\r', '\t'])
        && !Path::new(path).is_absolute()
        && Path::new(path).components().all(|component| {
            matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::ParentDir
            )
        })
        && !path.split('/').any(|component| component == "..")
}

pub fn locked_melpa_sources() -> Result<Vec<LockedPackageSource<'static>>, String> {
    Ok(locked_package_catalog()?
        .packages
        .iter()
        .map(|package| package.source)
        .collect())
}

pub fn locked_melpa_source(package: (&str, &str)) -> Result<LockedPackageSource<'static>, String> {
    locked_package_catalog()?
        .packages
        .iter()
        .map(|package| package.source)
        .find(|source| source.package() == package)
        .ok_or_else(|| {
            format!(
                "exact package {} {} has no revision-pinned source lock",
                package.0, package.1
            )
        })
}

pub fn locked_melpa_install_plan(
    package: (&str, &str),
) -> Result<Vec<LockedPackageSource<'static>>, String> {
    locked_package_catalog()?.install_plan(package)
}

fn run_command(
    command: &mut Command,
    timeout: std::time::Duration,
    label: &str,
) -> Result<String, String> {
    let display = format!("{command:?}");
    let output = output_with_timeout(command, timeout).map_err(|error| match error {
        InstallCommandError::Launch(error) => {
            format!("failed to launch {label} `{display}`: {error}")
        }
        InstallCommandError::TimedOut(_) => {
            format!("{label} timed out after {timeout:?}: `{display}`")
        }
        InstallCommandError::Capture(error) => {
            format!("failed to capture {label} output for `{display}`: {error}")
        }
    })?;
    if !output.status.success() {
        return Err(format!(
            "{label} failed with status {:?}: `{display}`\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("{label} emitted non-UTF-8 output: {error}"))
}

/// How many times one `git fetch` is attempted before preparation fails.
const FETCH_MAX_ATTEMPTS: usize = 4;

/// Delay before the next fetch attempt after a transient failure
/// (2 s, 5 s, 15 s). The upstream being briefly unavailable is exactly the
/// case this tolerates (Savannah answered HTTP 500 to the CI MELPA jobs).
const FETCH_RETRY_BACKOFF: [std::time::Duration; FETCH_MAX_ATTEMPTS - 1] = [
    std::time::Duration::from_secs(2),
    std::time::Duration::from_secs(5),
    std::time::Duration::from_secs(15),
];

/// Whether repeating a failed `git fetch` can succeed without changing it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FetchFailureClass {
    /// The remote or the network was temporarily unavailable: HTTP 5xx (and
    /// the retryable 408/429), DNS failure, timeout, connection reset, or a
    /// truncated transfer. The same request can succeed later.
    Transient,
    /// The request itself is wrong -- HTTP 4xx, authentication, or an unknown
    /// ref. Repeating it reproduces the same failure.
    Permanent,
}

impl FetchFailureClass {
    /// Classify a failed fetch from `run_command`'s error text, which carries
    /// git's stderr.
    fn classify(error: &str) -> Self {
        let error = error.to_ascii_lowercase();
        const PERMANENT: &[&str] = &[
            "couldn't find remote ref",
            "could not find remote ref",
            "unknown revision",
            "not our ref",
            "no such ref",
            "authentication failed",
            "could not read username",
            "could not read password",
            "permission denied",
            "access denied",
            "invalid username or password",
            "repository not found",
            "does not appear to be a git repository",
            "host key verification failed",
            "returned error: 401",
            "returned error: 403",
            "returned error: 404",
        ];
        if PERMANENT.iter().any(|marker| error.contains(marker)) {
            return Self::Permanent;
        }
        if let Some(status) = fetch_http_status(&error)
            && ((500..600).contains(&status) || status == 408 || status == 429)
        {
            return Self::Transient;
        }
        const TRANSIENT: &[&str] = &[
            "could not resolve host",
            "temporary failure in name resolution",
            "timed out",
            "connection reset",
            "connection refused",
            "couldn't connect to server",
            "network is unreachable",
            "no route to host",
            "early eof",
            "the remote end hung up unexpectedly",
            "unexpected disconnect while reading sideband packet",
            "empty reply from server",
            "not closed cleanly",
            "rpc failed; curl 56",
            "rpc failed; curl 92",
            "gnutls_handshake() failed",
            "ssl_error_syscall",
            "transfer closed with outstanding read data remaining",
        ];
        if TRANSIENT.iter().any(|marker| error.contains(marker)) {
            return Self::Transient;
        }
        // An unrecognized failure is not evidence of a transient outage; the
        // bounded retry budget is spent only where a retry is justified.
        Self::Permanent
    }
}

/// The HTTP status in git's `The requested URL returned error: NNN` stderr.
fn fetch_http_status(error: &str) -> Option<u16> {
    const MARKER: &str = "returned error: ";
    let start = error.find(MARKER)? + MARKER.len();
    let digits: String = error[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

/// Run one fetch attempt until it succeeds, a permanent failure occurs, or the
/// bounded attempt budget is exhausted.
///
/// `attempt` builds and executes a fresh command for each attempt (a
/// `Command` cannot be reused). Its error text must carry the attempt's
/// stderr so both the classifier and the final report can read it.
fn run_fetch_with_retry(
    backoff: &[std::time::Duration],
    mut attempt: impl FnMut(usize) -> Result<String, String>,
) -> Result<String, String> {
    let mut failures: Vec<(FetchFailureClass, String)> = Vec::new();
    for index in 0..FETCH_MAX_ATTEMPTS {
        match attempt(index) {
            Ok(output) => return Ok(output),
            Err(error) => {
                let class = FetchFailureClass::classify(&error);
                failures.push((class, error));
                if class == FetchFailureClass::Permanent || index + 1 == FETCH_MAX_ATTEMPTS {
                    break;
                }
                if let Some(delay) = backoff.get(index) {
                    std::thread::sleep(*delay);
                }
            }
        }
    }
    let mut message = format!(
        "shallow Git fetch failed after {} attempt{}",
        failures.len(),
        if failures.len() == 1 { "" } else { "s" }
    );
    for (index, (class, error)) in failures.iter().enumerate() {
        let class = match class {
            FetchFailureClass::Transient => "transient",
            FetchFailureClass::Permanent => "permanent",
        };
        message.push_str(&format!("\nattempt {} ({class}):\n{error}", index + 1));
    }
    Err(message)
}

/// Fetch one exact revision, tolerating a transient upstream failure.
fn fetch_git_revision(
    directory: &Path,
    revision: &str,
    timeout: std::time::Duration,
) -> Result<String, String> {
    run_fetch_with_retry(&FETCH_RETRY_BACKOFF, |_| {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(directory)
            .args(SHALLOW_GIT_FETCH_ARGS)
            .args(["origin", revision]);
        run_command(&mut command, timeout, "shallow Git fetch")
    })
}

fn prepare_git_checkout(
    directory: &Path,
    repository: &str,
    revision: &str,
    timeout: std::time::Duration,
) -> Result<u64, String> {
    fs::create_dir_all(directory).map_err(|error| {
        format!(
            "failed to create shallow Git checkout {}: {error}",
            directory.display()
        )
    })?;
    run_command(
        Command::new("git").args(["init", "--quiet"]).arg(directory),
        timeout,
        "Git initialization",
    )?;
    run_command(
        Command::new("git")
            .arg("-C")
            .arg(directory)
            .args(["remote", "add", "origin", repository]),
        timeout,
        "Git remote setup",
    )?;
    fetch_git_revision(directory, revision, timeout)?;
    run_command(
        Command::new("git").arg("-C").arg(directory).args([
            "checkout",
            "--detach",
            "--force",
            "FETCH_HEAD",
        ]),
        timeout,
        "Git checkout",
    )?;
    let actual_revision = run_command(
        Command::new("git")
            .arg("-C")
            .arg(directory)
            .args(["rev-parse", "HEAD"]),
        timeout,
        "Git revision verification",
    )?;
    if actual_revision.trim() != revision {
        return Err(format!(
            "shallow Git checkout {} resolved to {}, expected {revision}",
            directory.display(),
            actual_revision.trim()
        ));
    }
    let is_shallow = run_command(
        Command::new("git")
            .arg("-C")
            .arg(directory)
            .args(["rev-parse", "--is-shallow-repository"]),
        timeout,
        "Git checkout depth verification",
    )?;
    if is_shallow.trim() != "true" {
        return Err(format!(
            "Git checkout {} did not preserve the required depth-1 history",
            directory.display()
        ));
    }
    let timestamp = run_command(
        Command::new("git")
            .arg("-C")
            .arg(directory)
            .args(["show", "-s", "--format=%ct", "HEAD"]),
        timeout,
        "Git commit-time lookup",
    )?;
    timestamp.trim().parse::<u64>().map_err(|error| {
        format!(
            "Git checkout {} returned invalid commit time `{}`: {error}",
            directory.display(),
            timestamp.trim()
        )
    })
}

fn prepare_source_checkout(
    source: LockedPackageSource<'_>,
    directory: &Path,
    timeout: std::time::Duration,
) -> Result<u64, String> {
    match prepare_git_checkout(directory, source.repository, source.revision, timeout) {
        result @ Ok(_) => result,
        Err(primary_error) => {
            let Some(fallback_repository) = source.fallback_repository else {
                return Err(primary_error);
            };
            if directory.exists() {
                fs::remove_dir_all(directory).map_err(|error| {
                    format!(
                        "failed to reset source checkout {} before fallback: {error}",
                        directory.display()
                    )
                })?;
            }
            prepare_git_checkout(directory, fallback_repository, source.revision, timeout).map_err(
                |fallback_error| {
                    format!(
                        "primary source repository {} failed:\n{primary_error}\nfallback source repository {fallback_repository} failed:\n{fallback_error}",
                        source.repository
                    )
                },
            )
        }
    }
}

fn prepare_tool_checkout(
    label: &str,
    repository: &str,
    revision: &str,
    timeout: std::time::Duration,
) -> Result<PathBuf, String> {
    let root = crate::melpa_cache_root()
        .join("source-build-tools")
        .join(label)
        .join(revision);
    fs::create_dir_all(&root).map_err(|error| {
        format!(
            "failed to create source-build tool cache {}: {error}",
            root.display()
        )
    })?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(root.join("prepare.lock"))
        .map_err(|error| {
            format!(
                "failed to open source-build tool cache lock {}: {error}",
                root.display()
            )
        })?;
    fs4::FileExt::lock(&lock).map_err(|error| {
        format!(
            "failed to lock source-build tool cache {}: {error}",
            root.display()
        )
    })?;

    let checkout = root.join("source");
    let ready = root.join("ready");
    let failed = root.join("failed");
    let expected = format!("{repository}\t{revision}\n");
    if checkout.join(".git").is_dir()
        && fs::read_to_string(&ready).is_ok_and(|contents| contents == expected)
    {
        return Ok(checkout);
    }
    let failure_prefix = format!(
        "run-id\t{}\nidentity\t{expected}error\n",
        package_preparation_run_id()
    );
    if let Ok(contents) = fs::read_to_string(&failed)
        && let Some(error) = contents.strip_prefix(&failure_prefix)
    {
        return Err(error.to_string());
    }
    if checkout.exists() {
        fs::remove_dir_all(&checkout).map_err(|error| {
            format!(
                "failed to remove incomplete source-build tool checkout {}: {error}",
                checkout.display()
            )
        })?;
    }
    for marker in [&ready, &failed] {
        if marker.exists() {
            fs::remove_file(marker).map_err(|error| {
                format!(
                    "failed to remove stale source-build tool marker {}: {error}",
                    marker.display()
                )
            })?;
        }
    }
    if let Err(error) = prepare_git_checkout(&checkout, repository, revision, timeout) {
        return Err(publish_package_preparation_failure(
            &failed,
            &failure_prefix,
            error,
        ));
    }
    fs::write(&ready, expected).map_err(|error| {
        format!(
            "failed to publish source-build tool marker {}: {error}",
            ready.display()
        )
    })?;
    Ok(checkout)
}

fn docstrip_selector_matches(expression: &str, options: &[&str]) -> bool {
    expression.split('|').any(|alternative| {
        alternative.split('&').all(|selector| {
            let selector = selector.trim();
            let (negated, name) = selector
                .strip_prefix('!')
                .map_or((false, selector), |name| (true, name));
            !name.is_empty() && (options.contains(&name) != negated)
        })
    })
}

fn extract_docstrip_source(source: &str, options: &[&str]) -> Result<String, String> {
    let mut guards = Vec::<(String, bool)>::new();
    let mut output = String::new();
    for line in source.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        if let Some(expression) = content
            .strip_prefix("%<*")
            .and_then(|line| line.strip_suffix('>'))
        {
            guards.push((
                expression.to_string(),
                docstrip_selector_matches(expression, options),
            ));
            continue;
        }
        if let Some(expression) = content
            .strip_prefix("%</")
            .and_then(|line| line.strip_suffix('>'))
        {
            let Some((opened, _)) = guards.pop() else {
                return Err(format!("docstrip closes unopened guard `{expression}`"));
            };
            if opened != expression {
                return Err(format!(
                    "docstrip closes guard `{expression}` while `{opened}` is open"
                ));
            }
            continue;
        }
        let active = guards.iter().all(|(_, selected)| *selected);
        if let Some(selected) = content.strip_prefix("%<") {
            let Some((expression, selected)) = selected.split_once('>') else {
                return Err(format!(
                    "docstrip has an unterminated selector in `{content}`"
                ));
            };
            if active && docstrip_selector_matches(expression, options) {
                output.push_str(selected.trim_end_matches([' ', '\t']));
                output.push('\n');
            }
        } else if active && !guards.is_empty() && !content.starts_with('%') {
            output.push_str(content.trim_end_matches([' ', '\t']));
            output.push('\n');
        }
    }
    if let Some((guard, _)) = guards.last() {
        return Err(format!("docstrip leaves guard `{guard}` open"));
    }
    Ok(output)
}

fn prepare_auctex_runtime_tree(checkout: &Path) -> Result<(), String> {
    const OUTPUTS: &[(&str, &[&[&str]], &str)] = &[
        (
            "preview-mk.ins",
            &[&["installer", "make"]],
            "2e8d1f2756c65af41517187b9b3eec40f4b2bb48155a9d16e508368f8801675b",
        ),
        (
            "preview.drv",
            &[&["driver"]],
            "f47904bed6aff6c72582c88f9fbe0c0ea6abd8ddab469582084ea9915f0e36d9",
        ),
        (
            "preview.sty",
            &[&["style"], &["style", "active"]],
            "4854833239af46a188431bb7494cd3c2952a548a5d32661771c393b058abd016",
        ),
        (
            "prauctex.def",
            &[&["auctex"]],
            "16ea1388906ba2689819482f77f4fe5af7dbdecdf47423c4bd2d3e59f35e4bbd",
        ),
        (
            "prauctex.cfg",
            &[&["auccfg"]],
            "948e22505aaffa4c16339af3316af3918def3b2abb1dac3218598794d404b16e",
        ),
        (
            "prshowbox.def",
            &[&["showbox"]],
            "069e11c8430749947158915ac5dccdf3163f0791b7ba5be050fcb46cb774508e",
        ),
        (
            "prshowlabels.def",
            &[&["showlabels"]],
            "eb695e66f4741c6da94a6d0c2438bab57fd402e4d4166e2dea2a16d1f082f94f",
        ),
        (
            "prtracingall.def",
            &[&["tracingall"]],
            "d24e36e7a78d02b090e835dfaff1839dd6ba2cd4712782e542464c393a29f1b0",
        ),
        (
            "prtightpage.def",
            &[&["tightpage"]],
            "02eae9ce6a4a9899465e47dc7430074d499ad54c4c34c551af83a4b10dbaa6f7",
        ),
        (
            "prlyx.def",
            &[&["lyx"]],
            "c4d71b8bb81ad693a73ac89d9c0ee16cc87e7962a5ea784d02480d378898d135",
        ),
        (
            "prcounters.def",
            &[&["counters"]],
            "524d84b2a220cedf90d948e051ff2bf3c254312c93440013d5b379562275e55a",
        ),
        (
            "prfootnotes.def",
            &[&["footnotes"]],
            "127ee999a9da11dbe70f240c601230d0e2a71bc2071d08df632e280d070291ba",
        ),
    ];

    let latex = checkout.join("latex");
    let source_path = latex.join("preview.dtx");
    let source = fs::read_to_string(&source_path).map_err(|error| {
        format!(
            "failed to read pinned AUCTeX docstrip source {}: {error}",
            source_path.display()
        )
    })?;
    for (file, option_sets, expected_sha256) in OUTPUTS {
        let mut generated = format!(
            "%% Generated from pinned AUCTeX preview.dtx for {file}.\n%% The source remains in this package under the GNU GPL.\n"
        );
        for options in *option_sets {
            generated.push_str(&extract_docstrip_source(&source, options)?);
        }
        if !generated.ends_with("\\endinput\n") {
            generated.push_str("\\endinput\n");
        }
        let actual_sha256 = Sha256::digest(generated.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if actual_sha256 != *expected_sha256 {
            return Err(format!(
                "generated pinned AUCTeX runtime file {file} has SHA-256 {actual_sha256}, expected {expected_sha256}"
            ));
        }
        fs::write(latex.join(file), generated).map_err(|error| {
            format!(
                "failed to generate pinned AUCTeX runtime file {}: {error}",
                latex.join(file).display()
            )
        })?;
    }
    Ok(())
}

const AUCTEX_RUNTIME_REQUIRED_FILES: &[&str] = &[
    "images/execpdftex.xpm",
    "latex/prauctex.cfg",
    "latex/prauctex.def",
    "latex/prcounters.def",
    "latex/preview-mk.ins",
    "latex/preview.drv",
    "latex/preview.dtx",
    "latex/preview.sty",
    "latex/prfootnotes.def",
    "latex/prlyx.def",
    "latex/prshowbox.def",
    "latex/prshowlabels.def",
    "latex/prtightpage.def",
    "latex/prtracingall.def",
    "style/amsmath.el",
    "style/article.el",
];

const AUCTEX_RUNTIME_EXCLUDED_PATHS: &[&str] = &[
    "ChangeLog.1",
    "README.GIT",
    "admin",
    "build-aux",
    "latex/Makefile.in",
    "lpath.el",
    "tests",
];

fn verify_auctex_runtime_tree(checkout: &Path) -> Result<(), String> {
    let missing = AUCTEX_RUNTIME_REQUIRED_FILES
        .iter()
        .filter(|relative| !checkout.join(relative).is_file())
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "prepared pinned AUCTeX runtime tree is missing required files: {}",
            missing.join(", ")
        ));
    }

    let retained = AUCTEX_RUNTIME_EXCLUDED_PATHS
        .iter()
        .filter(|relative| checkout.join(relative).exists())
        .copied()
        .collect::<Vec<_>>();
    if !retained.is_empty() {
        return Err(format!(
            "prepared pinned AUCTeX runtime tree retained excluded paths: {}",
            retained.join(", ")
        ));
    }
    Ok(())
}

fn auctex_elpa_ignored_name(pattern: &str, name: &str) -> Result<bool, String> {
    if let Some(suffix) = pattern.strip_prefix('*')
        && !suffix.contains(['*', '?', '[', ']', '/'])
    {
        return Ok(name.ends_with(suffix));
    }
    if !pattern.contains(['*', '?', '[', ']', '/']) {
        return Ok(name == pattern);
    }
    Err(format!(
        "unsupported pinned AUCTeX .elpaignore pattern `{pattern}`"
    ))
}

fn prune_auctex_elpa_ignored(checkout: &Path) -> Result<(), String> {
    let ignore_path = checkout.join(".elpaignore");
    let patterns = fs::read_to_string(&ignore_path)
        .map_err(|error| {
            format!(
                "failed to read pinned AUCTeX exclusions {}: {error}",
                ignore_path.display()
            )
        })?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect::<Vec<_>>();

    fn prune(directory: &Path, patterns: &[String]) -> Result<(), String> {
        for entry in fs::read_dir(directory).map_err(|error| {
            format!(
                "failed to enumerate pinned AUCTeX tree {}: {error}",
                directory.display()
            )
        })? {
            let entry = entry.map_err(|error| {
                format!(
                    "failed to inspect pinned AUCTeX tree {}: {error}",
                    directory.display()
                )
            })?;
            let path = entry.path();
            let name = entry.file_name().into_string().map_err(|name| {
                format!(
                    "pinned AUCTeX tree {} contains a non-UTF-8 entry {:?}",
                    directory.display(),
                    name
                )
            })?;
            if name == ".git" {
                continue;
            }
            let ignored = patterns.iter().try_fold(false, |ignored, pattern| {
                Ok::<_, String>(ignored || auctex_elpa_ignored_name(pattern, &name)?)
            })?;
            if ignored {
                if path.is_dir() {
                    fs::remove_dir_all(&path)
                } else {
                    fs::remove_file(&path)
                }
                .map_err(|error| {
                    format!(
                        "failed to exclude pinned AUCTeX development entry {}: {error}",
                        path.display()
                    )
                })?;
            } else if path.is_dir() {
                prune(&path, patterns)?;
            }
        }
        Ok(())
    }

    prune(checkout, &patterns)
}

fn runtime_tree_file_spec(checkout: &Path) -> Result<String, String> {
    fn visit(root: &Path, directory: &Path, files: &mut Vec<String>) -> Result<(), String> {
        for entry in fs::read_dir(directory).map_err(|error| {
            format!(
                "failed to enumerate source runtime tree {}: {error}",
                directory.display()
            )
        })? {
            let entry = entry.map_err(|error| {
                format!(
                    "failed to inspect source runtime tree {}: {error}",
                    directory.display()
                )
            })?;
            let path = entry.path();
            if path == root.join(".git") {
                continue;
            }
            if path.is_dir() {
                visit(root, &path, files)?;
            } else if path.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .expect("runtime entry is below root");
                let relative = relative.to_str().ok_or_else(|| {
                    format!(
                        "source runtime tree {} contains a non-UTF-8 path {}",
                        root.display(),
                        relative.display()
                    )
                })?;
                if !safe_source_path(relative) {
                    return Err(format!(
                        "source runtime tree {} contains unsafe path `{relative}`",
                        root.display()
                    ));
                }
                files.push(relative.to_string());
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    visit(checkout, checkout, &mut files)?;
    files.sort();
    if files.is_empty() {
        return Err(format!(
            "source runtime tree {} has no packageable entries",
            checkout.display()
        ));
    }
    Ok(files
        .iter()
        .map(|file| format!("(:rename {} {})", elisp_string(file), elisp_string(file)))
        .collect::<Vec<_>>()
        .join(" "))
}

fn synthetic_recipe(source: LockedPackageSource<'_>, checkout: &Path) -> Result<String, String> {
    let files = match source.build {
        SourceBuild::DefaultFiles => String::new(),
        SourceBuild::AuctexRuntime => {
            prepare_auctex_runtime_tree(checkout)?;
            prune_auctex_elpa_ignored(checkout)?;
            verify_auctex_runtime_tree(checkout)?;
            format!(" :files ({})", runtime_tree_file_spec(checkout)?)
        }
        SourceBuild::Files(pattern) => format!(" :files ({})", elisp_string(pattern)),
        SourceBuild::MelpaRecipe => {
            return Err(format!(
                "{} uses the locked MELPA recipe, not a synthetic recipe",
                source.name
            ));
        }
    };
    Ok(format!(
        "({} :fetcher git :url {}{files})\n",
        source.name,
        elisp_string(source.repository)
    ))
}

fn prepare_cached_source_artifact(
    driver: &dyn PackageInstallDriver,
    source: LockedPackageSource<'_>,
) -> Result<PathBuf, String> {
    prepare_cached_source_artifact_with_tools(driver, source, SOURCE_BUILD_TOOLS)
}

fn prepare_cached_source_artifact_with_tools(
    driver: &dyn PackageInstallDriver,
    source: LockedPackageSource<'_>,
    tools: SourceBuildTools<'_>,
) -> Result<PathBuf, String> {
    let root = crate::melpa_cache_root()
        .join("source-package-cache")
        .join(source.name)
        .join(source.version)
        .join(source.revision)
        .join(tools.melpa_revision)
        .join(tools.package_build_revision);
    fs::create_dir_all(&root).map_err(|error| {
        format!(
            "failed to create source package cache {}: {error}",
            root.display()
        )
    })?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(root.join("prepare.lock"))
        .map_err(|error| {
            format!(
                "failed to open source package cache lock {}: {error}",
                root.display()
            )
        })?;
    fs4::FileExt::lock(&lock).map_err(|error| {
        format!(
            "failed to lock source package cache {}: {error}",
            root.display()
        )
    })?;

    let artifact = root
        .join("packages")
        .join(format!("{}-{}.tar", source.name, source.version));
    let ready = root.join("ready");
    let failed = root.join("failed");
    let expected = format!(
        "{}melpa-recipes\t{}\t{}\npackage-build\t{}\t{}\n",
        source.identity(),
        tools.melpa_repository,
        tools.melpa_revision,
        tools.package_build_repository,
        tools.package_build_revision
    );
    if artifact.is_file() && fs::read_to_string(&ready).is_ok_and(|contents| contents == expected) {
        return Ok(artifact);
    }
    let failure_prefix = format!(
        "run-id\t{}\nidentity\t{expected}error\n",
        package_preparation_run_id()
    );
    if let Ok(contents) = fs::read_to_string(&failed)
        && let Some(error) = contents.strip_prefix(&failure_prefix)
    {
        return Err(error.to_string());
    }

    for directory in [
        root.join("working"),
        root.join("packages"),
        root.join("recipes"),
        root.join("home"),
        root.join("tmp"),
        root.join("xdg"),
    ] {
        if directory.exists() {
            fs::remove_dir_all(&directory).map_err(|error| {
                format!(
                    "failed to remove incomplete source package build {}: {error}",
                    directory.display()
                )
            })?;
        }
    }
    for marker in [&ready, &failed] {
        if marker.exists() {
            fs::remove_file(marker).map_err(|error| {
                format!(
                    "failed to remove stale source package marker {}: {error}",
                    marker.display()
                )
            })?;
        }
    }
    let home = root.join("home");
    let editor_tmp = root.join("tmp");
    for directory in [
        root.join("working"),
        root.join("packages"),
        root.join("recipes"),
        home.join(".emacs.d"),
        editor_tmp.clone(),
        root.join("xdg/config"),
        root.join("xdg/cache"),
        root.join("xdg/data"),
        root.join("xdg/state"),
    ] {
        fs::create_dir_all(&directory).map_err(|error| {
            format!(
                "failed to create source package build directory {}: {error}",
                directory.display()
            )
        })?;
    }

    let build_result = (|| {
        let melpa = prepare_tool_checkout(
            "melpa",
            tools.melpa_repository,
            tools.melpa_revision,
            driver.timeout(),
        )?;
        let package_build = prepare_tool_checkout(
            "package-build",
            tools.package_build_repository,
            tools.package_build_revision,
            driver.timeout(),
        )?;
        let working = root.join("working").join(source.name);
        let commit_time = prepare_source_checkout(source, &working, driver.timeout())?;
        let recipes = match source.build {
            SourceBuild::MelpaRecipe => melpa.join("recipes"),
            SourceBuild::DefaultFiles | SourceBuild::AuctexRuntime | SourceBuild::Files(_) => {
                let recipes = root.join("recipes");
                fs::write(
                    recipes.join(source.name),
                    synthetic_recipe(source, &working)?,
                )
                .map_err(|error| {
                    format!(
                        "failed to write source recipe for {} below {}: {error}",
                        source.name,
                        recipes.display()
                    )
                })?;
                recipes
            }
        };
        let build_script = workspace_root()
            .join("crates/neomacs-melpa-test-support/elisp/build-package-from-source.el");
        let environment =
            package_environment_with_runtime(&root, &home, &editor_tmp, &root.join("xdg/runtime"))
                .expect("package sandbox roots must be absolute");
        let mut command = driver.command(&environment);
        command
            .env("NEOMACS_PACKAGE_BUILD_DIR", &package_build)
            .env("NEOMACS_PACKAGE_RECIPES_DIR", &recipes)
            .env("NEOMACS_PACKAGE_BUILD_ROOT", &root)
            .env("NEOMACS_PACKAGE_NAME", source.name)
            .env("NEOMACS_PACKAGE_VERSION", source.version)
            .env("NEOMACS_PACKAGE_REVISION", source.revision)
            .env("NEOMACS_PACKAGE_COMMIT_TIME", commit_time.to_string())
            .args(["--batch", "--quick", "--load"])
            .arg(&build_script);
        let output = driver.run(&mut command).map_err(|error| match error {
            InstallCommandError::Launch(error) => format!(
                "failed to launch {} to build {} from source: {error}",
                driver.name(),
                source.name
            ),
            InstallCommandError::TimedOut(_) => format!(
                "{} source build for {} timed out after {:?}",
                driver.name(),
                source.name,
                driver.timeout()
            ),
            InstallCommandError::Capture(error) => format!(
                "failed to capture {} source build for {}: {error}",
                driver.name(),
                source.name
            ),
        })?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let marker = format!(
            "NEOMACS-SOURCE-PACKAGE:ready:{}:{}",
            source.name, source.version
        );
        if !output.status.success() || !stdout.contains(&marker) || !artifact.is_file() {
            return Err(format!(
                "failed to build {} {} from locked source {} below {}\nstatus: {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
                source.name,
                source.version,
                source.revision,
                root.display(),
                output.status.code()
            ));
        }
        Ok(())
    })();
    if let Err(error) = build_result {
        return Err(publish_package_preparation_failure(
            &failed,
            &failure_prefix,
            error,
        ));
    }

    let marker_tmp = root.join(format!("ready.{}.tmp", std::process::id()));
    fs::write(&marker_tmp, &expected).map_err(|error| {
        format!(
            "failed to write source package marker {}: {error}",
            marker_tmp.display()
        )
    })?;
    fs::rename(&marker_tmp, &ready).map_err(|error| {
        format!(
            "failed to publish source package marker {}: {error}",
            ready.display()
        )
    })?;
    Ok(artifact)
}

/// Revision of the source install cache's warm phase.
///
/// A cached tree is handed to scenarios only after this phase has run; bump
/// the revision when the phase's behavior changes so existing caches run it
/// again instead of being trusted.
const SOURCE_INSTALL_WARM_REVISION: &str = "1";

/// File extensions that mark a package tree as carrying native sources.
const NATIVE_SOURCE_EXTENSIONS: &[&str] = &["c", "cc", "cpp", "cxx", "m", "mm"];

/// File extensions of a built native artifact Emacs can load.
const NATIVE_ARTIFACT_EXTENSIONS: &[&str] = &["so", "dylib", "dll"];

/// A source install cache tree that is ready for scenario processes.
///
/// [`prepare_cached_locked_package_plan`] constructs this only after the
/// cache lock has covered the package install and the load-time native build
/// warm phase below.  A plain `PathBuf` therefore cannot be mistaken for a
/// prepared tree: holding this value is the proof that the shared package
/// directory has already run the builds that package loads trigger, so two
/// editors may load the tree concurrently without racing one build directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedSourceInstall {
    package_dir: PathBuf,
}

impl PreparedSourceInstall {
    fn after_warm(package_dir: PathBuf) -> Self {
        Self { package_dir }
    }

    /// The prepared directory of the plan's root package.
    pub fn package_dir(&self) -> &Path {
        &self.package_dir
    }

    /// Take the prepared directory, consuming the readiness proof.
    pub fn into_package_dir(self) -> PathBuf {
        self.package_dir
    }
}

impl AsRef<Path> for PreparedSourceInstall {
    fn as_ref(&self) -> &Path {
        &self.package_dir
    }
}

impl From<PreparedSourceInstall> for PathBuf {
    fn from(prepared: PreparedSourceInstall) -> Self {
        prepared.package_dir
    }
}

/// The next step an install cache needs before it can be handed to scenarios.
enum InstallCacheState {
    /// Installed and warmed under the cache lock.
    Ready(PreparedSourceInstall),
    /// Installed, but populated before the warm phase (or by a cache whose
    /// warm marker is stale): only the load-time builds remain.
    NeedsWarm,
    /// Not installed, or installed for a different source identity.
    NeedsInstall,
}

fn installed_package_dir(home: &Path, source: LockedPackageSource<'_>) -> PathBuf {
    home.join(".emacs.d/elpa")
        .join(format!("{}-{}", source.name, source.version))
}

fn installed_package_is_present(home: &Path, source: LockedPackageSource<'_>) -> bool {
    installed_package_dir(home, source)
        .join(format!("{}-pkg.el", source.name))
        .is_file()
}

fn source_install_cache_state(
    home: &Path,
    plan: &[LockedPackageSource<'_>],
    ready: &Path,
    warm: &Path,
    expected: &str,
    warm_expected: &str,
    package_dir: &Path,
) -> InstallCacheState {
    let installed = plan
        .iter()
        .all(|source| installed_package_is_present(home, *source))
        && fs::read_to_string(ready).is_ok_and(|contents| contents == expected);
    if !installed {
        return InstallCacheState::NeedsInstall;
    }
    if fs::read_to_string(warm).is_ok_and(|contents| contents == warm_expected) {
        return InstallCacheState::Ready(PreparedSourceInstall::after_warm(
            package_dir.to_path_buf(),
        ));
    }
    InstallCacheState::NeedsWarm
}

fn contains_file_with_extension(
    directory: &Path,
    extensions: &[&str],
    depth: usize,
) -> Result<bool, String> {
    const MAX_DEPTH: usize = 4;
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("failed to scan {}: {error}", directory.display()))?;
    let mut subdirectories = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("failed to scan {}: {error}", directory.display()))?;
        let file_type = entry.file_type().map_err(|error| {
            format!(
                "failed to inspect {} while scanning {}: {error}",
                entry.path().display(),
                directory.display()
            )
        })?;
        if file_type.is_dir() {
            if depth < MAX_DEPTH {
                subdirectories.push(entry.path());
            }
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        let matching = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extensions
                    .iter()
                    .any(|wanted| extension.eq_ignore_ascii_case(wanted))
            });
        if matching {
            return Ok(true);
        }
    }
    for subdirectory in subdirectories {
        if contains_file_with_extension(&subdirectory, extensions, depth + 1)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether a package tree still needs the load-time build that the warm
/// phase exists to run.
///
/// A package with native sources and no built artifact may compile its module
/// from its own load path (`sqlite3.el` runs `make all` on first load).  The
/// warm phase loads exactly these packages; a tree that already contains an
/// artifact has had that build run and is left untouched.
fn package_needs_native_build_warm(package_dir: &Path) -> Result<bool, String> {
    Ok(
        contains_file_with_extension(package_dir, NATIVE_SOURCE_EXTENSIONS, 0)?
            && !contains_file_with_extension(package_dir, NATIVE_ARTIFACT_EXTENSIONS, 0)?,
    )
}

/// Run each installed package's load-time native build once, serially, while
/// the install cache lock is held.
///
/// Scenarios load the same package files in parallel, so a build that the
/// package's load path starts would otherwise run twice in one package
/// directory.  Loading the native-source packages here, before the readiness
/// markers exist, puts the module in the cached tree so scenarios only load
/// it.
///
/// A load that signals is reported on stderr and does not fail the cache:
/// some packages ask the user before compiling (`vterm`) or compile on a
/// later command (`ghostel`), and the cache must preserve that behavior
/// rather than invent a build behind the package's back.  The diagnostics
/// stay visible instead of being silently swallowed.
fn warm_load_time_native_builds(
    driver: &dyn PackageInstallDriver,
    root: &Path,
    home: &Path,
    editor_tmp: &Path,
    plan: &[LockedPackageSource<'_>],
) -> Result<(), String> {
    let mut candidates = Vec::new();
    for source in plan {
        let package_dir = installed_package_dir(home, *source);
        if package_needs_native_build_warm(&package_dir)? {
            candidates.push((*source, package_dir));
        }
    }
    if candidates.is_empty() {
        return Ok(());
    }

    let loads = candidates
        .iter()
        .map(|(source, package_dir)| {
            elisp_string(
                &package_dir
                    .join(format!("{}.el", source.name))
                    .to_string_lossy(),
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let form = format!(
        r##"(progn
               (require 'package)
               (setq package-user-dir
                     (expand-file-name ".emacs.d/elpa" (getenv "HOME"))
                     package-check-signature nil)
               (package-initialize)
               (let ((failures 0))
                 (dolist (file (list {loads}))
                   (condition-case error
                       (load file nil t t)
                     (error
                      (setq failures (1+ failures))
                      (message
                       "NEOMACS-SOURCE-INSTALL:native-build-load-failed:%s:%S"
                       file error))))
                 (princ
                  (format
                   "NEOMACS-SOURCE-INSTALL:native-build-failures:%d\n"
                   failures)))
               (princ "NEOMACS-SOURCE-INSTALL:warm"))"##
    );
    let environment =
        package_environment_with_runtime(root, home, editor_tmp, &root.join("xdg/runtime"))
            .expect("package sandbox roots must be absolute");
    let names = candidates
        .iter()
        .map(|(source, _)| format!("{} {}", source.name, source.version))
        .collect::<Vec<_>>()
        .join(", ");
    let mut command = driver.command(&environment);
    command.args(["--batch", "--quick", "--eval", &form]);
    let output = driver.run(&mut command).map_err(|error| match error {
        InstallCommandError::Launch(error) => format!(
            "failed to launch {} to warm native builds for {names}: {error}",
            driver.name()
        ),
        InstallCommandError::TimedOut(_) => format!(
            "{} native build warm-up for {names} timed out after {:?}",
            driver.name(),
            driver.timeout()
        ),
        InstallCommandError::Capture(error) => format!(
            "failed to capture {} native build warm-up for {names}: {error}",
            driver.name()
        ),
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() || !stdout.contains("NEOMACS-SOURCE-INSTALL:warm") {
        return Err(format!(
            "failed to warm load-time native builds for {names} below {}\nstatus: {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            root.display(),
            output.status.code()
        ));
    }
    for line in stderr
        .lines()
        .filter(|line| line.contains("native-build-load-failed"))
    {
        eprintln!("{line}");
    }
    Ok(())
}

fn publish_cache_marker(marker: &Path, contents: &str, label: &str) -> Result<(), String> {
    let file_name = marker
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            format!(
                "source install marker {} has no file name",
                marker.display()
            )
        })?;
    let temporary = marker.with_file_name(format!("{file_name}.{}.tmp", std::process::id()));
    fs::write(&temporary, contents)
        .map_err(|error| format!("failed to write {label} {}: {error}", temporary.display()))?;
    fs::rename(&temporary, marker)
        .map_err(|error| format!("failed to publish {label} {}: {error}", marker.display()))?;
    Ok(())
}

pub fn prepare_cached_locked_package_plan(
    driver: &dyn PackageInstallDriver,
    plan: &[LockedPackageSource<'_>],
) -> Result<PreparedSourceInstall, String> {
    let Some(root_source) = plan.last().copied() else {
        return Err("locked source package preparation requires a non-empty plan".to_string());
    };
    let mut pins = BTreeSet::new();
    for source in plan {
        if !pins.insert(source.package()) {
            return Err(format!(
                "locked source package plan repeats {} {}",
                source.name, source.version
            ));
        }
    }

    let mut artifacts = Vec::with_capacity(plan.len());
    for source in plan {
        artifacts.push((*source, prepare_cached_source_artifact(driver, *source)?));
    }

    let root = crate::melpa_cache_root()
        .join("source-install-cache")
        .join(root_source.name)
        .join(root_source.version)
        .join(root_source.revision)
        .join(MELPA_RECIPE_REVISION)
        .join(PACKAGE_BUILD_REVISION);
    fs::create_dir_all(&root).map_err(|error| {
        format!(
            "failed to create source install cache {}: {error}",
            root.display()
        )
    })?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(root.join("prepare.lock"))
        .map_err(|error| {
            format!(
                "failed to open source install cache lock {}: {error}",
                root.display()
            )
        })?;
    fs4::FileExt::lock(&lock).map_err(|error| {
        format!(
            "failed to lock source install cache {}: {error}",
            root.display()
        )
    })?;

    let home = root.join("home");
    let editor_tmp = root.join("tmp");
    let package_dir = installed_package_dir(&home, root_source);
    let ready = root.join("ready");
    let warm = root.join("warm");
    let failed = root.join("failed");
    let expected = plan
        .iter()
        .map(|source| source.identity())
        .collect::<String>();
    let warm_expected = format!("{SOURCE_INSTALL_WARM_REVISION}\n{expected}");
    let failure_prefix = format!(
        "run-id\t{}\nidentity\t{expected}error\n",
        package_preparation_run_id()
    );

    match source_install_cache_state(
        &home,
        plan,
        &ready,
        &warm,
        &expected,
        &warm_expected,
        &package_dir,
    ) {
        InstallCacheState::Ready(prepared) => return Ok(prepared),
        InstallCacheState::NeedsWarm => {
            if let Ok(contents) = fs::read_to_string(&failed)
                && let Some(error) = contents.strip_prefix(&failure_prefix)
            {
                return Err(error.to_string());
            }
            // A cache populated before the warm phase exists: install is
            // complete, only the load-time builds remain.
            warm_load_time_native_builds(driver, &root, &home, &editor_tmp, plan).map_err(
                |error| publish_package_preparation_failure(&failed, &failure_prefix, error),
            )?;
            publish_cache_marker(&warm, &warm_expected, "source install warm marker")?;
            return Ok(PreparedSourceInstall::after_warm(package_dir));
        }
        InstallCacheState::NeedsInstall => {}
    }
    if let Ok(contents) = fs::read_to_string(&failed)
        && let Some(error) = contents.strip_prefix(&failure_prefix)
    {
        return Err(error.to_string());
    }

    for directory in [&home, &editor_tmp, &root.join("xdg"), &root.join("install")] {
        if directory.exists() {
            fs::remove_dir_all(directory).map_err(|error| {
                format!(
                    "failed to remove incomplete source install cache {}: {error}",
                    directory.display()
                )
            })?;
        }
    }
    for marker in [&ready, &warm, &failed] {
        if marker.exists() {
            fs::remove_file(marker).map_err(|error| {
                format!(
                    "failed to remove stale source install marker {}: {error}",
                    marker.display()
                )
            })?;
        }
    }
    let install_dir = root.join("install");
    for directory in [
        home.join(".emacs.d"),
        editor_tmp.clone(),
        root.join("xdg/config"),
        root.join("xdg/cache"),
        root.join("xdg/data"),
        root.join("xdg/state"),
        install_dir.clone(),
    ] {
        fs::create_dir_all(&directory).map_err(|error| {
            format!(
                "failed to create source install directory {}: {error}",
                directory.display()
            )
        })?;
    }

    // `package-install-file' VISITS the file it installs -- it calls
    // `set-visited-file-name', and tar-mode then touches the modified bit
    // (lisp/emacs-lisp/package.el:2316-2338) -- so Emacs takes a lock beside
    // that file. Handing it a path inside the shared artifact cache therefore
    // makes every plan lock the same tar, and two plans sharing a dependency
    // race: the loser signals `file-locked', which is correct behaviour and
    // must not be suppressed.
    //
    // The artifact cache is a build output, so keep it read-only for consumers
    // and install from a private copy instead. Only the builder writes there,
    // under the lock in `prepare_cached_source_artifact_with_tools'. This
    // removes the shared mutable path rather than scheduling access to it, so
    // there is no cross-plan lock order to get wrong.
    let artifacts = artifacts
        .iter()
        .map(|(source, artifact)| {
            let file_name = artifact.file_name().ok_or_else(|| {
                format!("source artifact {} has no file name", artifact.display())
            })?;
            let private = install_dir.join(file_name);
            fs::copy(artifact, &private).map_err(|error| {
                format!(
                    "failed to stage source artifact {} for install: {error}",
                    artifact.display()
                )
            })?;
            Ok((*source, private))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let install_steps = artifacts
        .iter()
        .map(|(source, artifact)| {
            format!(
                r##"(progn
                       (package-install-file {})
                       (package-initialize)
                       (let* ((package-symbol (intern {}))
                              (installed (cadr (assq package-symbol package-alist)))
                              (installed-version
                               (and installed
                                    (package-version-join
                                     (package-desc-version installed)))))
                         (unless (equal installed-version {})
                           (error
                            "Installed source package mismatch: %s expected %s, got %s"
                            package-symbol {} installed-version))))"##,
                elisp_string(&artifact.to_string_lossy()),
                elisp_string(source.name),
                elisp_string(source.version),
                elisp_string(source.version),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let form = format!(
        r##"(progn
               (require 'package)
               (setq package-user-dir
                     (expand-file-name ".emacs.d/elpa" (getenv "HOME"))
                     package-check-signature nil)
               {install_steps}
               (princ "NEOMACS-SOURCE-INSTALL:ready"))"##
    );
    let environment =
        package_environment_with_runtime(&root, &home, &editor_tmp, &root.join("xdg/runtime"))
            .expect("package sandbox roots must be absolute");
    let mut command = driver.command(&environment);
    command.args(["--batch", "--quick", "--eval", &form]);
    let output = match driver.run(&mut command) {
        Ok(output) => output,
        Err(error) => {
            let error = match error {
                InstallCommandError::Launch(error) => format!(
                    "failed to launch {} to install {} from source: {error}",
                    driver.name(),
                    root_source.name
                ),
                InstallCommandError::TimedOut(_) => format!(
                    "{} source installation for {} timed out after {:?}",
                    driver.name(),
                    root_source.name,
                    driver.timeout()
                ),
                InstallCommandError::Capture(error) => format!(
                    "failed to capture {} source installation for {}: {error}",
                    driver.name(),
                    root_source.name
                ),
            };
            return Err(publish_package_preparation_failure(
                &failed,
                &failure_prefix,
                error,
            ));
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success()
        || !stdout.contains("NEOMACS-SOURCE-INSTALL:ready")
        || !plan
            .iter()
            .all(|source| installed_package_is_present(&home, *source))
    {
        let error = format!(
            "failed to install {} {} from locked source below {}\nstatus: {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            root_source.name,
            root_source.version,
            root.display(),
            output.status.code()
        );
        return Err(publish_package_preparation_failure(
            &failed,
            &failure_prefix,
            error,
        ));
    }

    // The ready marker alone is not readiness: scenarios require the warm
    // marker too, so a crash between these two writes only repeats the warm
    // phase instead of reinstalling the whole cache.
    publish_cache_marker(&ready, &expected, "source install marker")?;
    warm_load_time_native_builds(driver, &root, &home, &editor_tmp, plan)
        .map_err(|error| publish_package_preparation_failure(&failed, &failure_prefix, error))?;
    publish_cache_marker(&warm, &warm_expected, "source install warm marker")?;
    Ok(PreparedSourceInstall::after_warm(package_dir))
}

pub fn prepare_cached_locked_melpa_package(
    driver: &dyn PackageInstallDriver,
    package: (&str, &str),
) -> Result<PreparedSourceInstall, String> {
    let plan = locked_melpa_install_plan(package)?;
    prepare_cached_locked_package_plan(driver, &plan)
}

pub fn preflight_locked_melpa_packages(
    driver: &dyn PackageInstallDriver,
    packages: &[(&str, &str)],
) -> Result<Vec<PathBuf>, String> {
    packages
        .iter()
        .map(|package| {
            prepare_cached_locked_melpa_package(driver, *package)
                .map(PreparedSourceInstall::into_package_dir)
                .map_err(|error| {
                    format!(
                        "MELPA source preflight failed before parity tests for {} {}:\n{error}",
                        package.0, package.1
                    )
                })
        })
        .collect()
}

#[cfg(test)]
#[path = "source_lock/tests/source_lock_test.rs"]
mod tests;

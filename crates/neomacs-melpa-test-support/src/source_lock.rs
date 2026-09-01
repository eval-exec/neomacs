use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use sha2::{Digest, Sha256};

use crate::{
    CommandError, EmacsRuntime, configure_process_environment, elisp_string, output_with_timeout,
    package_preparation_run_id, publish_package_preparation_failure, workspace_root,
};

const LOCKED_PACKAGE_MANIFEST: &str = include_str!("../melpa-package-lock.tsv");
static LOCKED_PACKAGE_CATALOG: OnceLock<Result<LockedPackageCatalog, String>> = OnceLock::new();

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
        CommandError::Launch(error) => format!("failed to launch {label} `{display}`: {error}"),
        CommandError::TimedOut(_) => format!("{label} timed out after {timeout:?}: `{display}`"),
        CommandError::Capture(error) => {
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
    run_command(
        Command::new("git")
            .arg("-C")
            .arg(directory)
            .args(SHALLOW_GIT_FETCH_ARGS)
            .args(["origin", revision]),
        timeout,
        "shallow Git fetch",
    )?;
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
    let root = workspace_root()
        .join("tmp/melpa/source-build-tools")
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
    gnu_emacs: &EmacsRuntime,
    source: LockedPackageSource<'_>,
) -> Result<PathBuf, String> {
    prepare_cached_source_artifact_with_tools(gnu_emacs, source, SOURCE_BUILD_TOOLS)
}

fn prepare_cached_source_artifact_with_tools(
    gnu_emacs: &EmacsRuntime,
    source: LockedPackageSource<'_>,
    tools: SourceBuildTools<'_>,
) -> Result<PathBuf, String> {
    let root = workspace_root()
        .join("tmp/melpa/source-package-cache")
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
            gnu_emacs.timeout,
        )?;
        let package_build = prepare_tool_checkout(
            "package-build",
            tools.package_build_repository,
            tools.package_build_revision,
            gnu_emacs.timeout,
        )?;
        let working = root.join("working").join(source.name);
        let commit_time = prepare_source_checkout(source, &working, gnu_emacs.timeout)?;
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
        let mut command = gnu_emacs.command();
        configure_process_environment(&mut command, &root, &home, &editor_tmp);
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
        let output =
            output_with_timeout(&mut command, gnu_emacs.timeout).map_err(|error| match error {
                CommandError::Launch(error) => format!(
                    "failed to launch {} to build {} from source: {error}",
                    gnu_emacs.name, source.name
                ),
                CommandError::TimedOut(_) => format!(
                    "{} source build for {} timed out after {:?}",
                    gnu_emacs.name, source.name, gnu_emacs.timeout
                ),
                CommandError::Capture(error) => format!(
                    "failed to capture {} source build for {}: {error}",
                    gnu_emacs.name, source.name
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

pub fn prepare_cached_locked_package_plan(
    gnu_emacs: &EmacsRuntime,
    plan: &[LockedPackageSource<'_>],
) -> Result<PathBuf, String> {
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
        artifacts.push((*source, prepare_cached_source_artifact(gnu_emacs, *source)?));
    }

    let root = workspace_root()
        .join("tmp/melpa/source-install-cache")
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
    let package_dir = home
        .join(".emacs.d/elpa")
        .join(format!("{}-{}", root_source.name, root_source.version));
    let ready = root.join("ready");
    let failed = root.join("failed");
    let expected = plan
        .iter()
        .map(|source| source.identity())
        .collect::<String>();
    let cache_is_ready = plan.iter().all(|source| {
        home.join(".emacs.d/elpa")
            .join(format!("{}-{}", source.name, source.version))
            .join(format!("{}-pkg.el", source.name))
            .is_file()
    }) && fs::read_to_string(&ready)
        .is_ok_and(|contents| contents == expected);
    if cache_is_ready {
        return Ok(package_dir);
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
    for marker in [&ready, &failed] {
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
    let mut command = gnu_emacs.command();
    configure_process_environment(&mut command, &root, &home, &editor_tmp);
    command.args(["--batch", "--quick", "--eval", &form]);
    let output = match output_with_timeout(&mut command, gnu_emacs.timeout) {
        Ok(output) => output,
        Err(error) => {
            let error = match error {
                CommandError::Launch(error) => format!(
                    "failed to launch {} to install {} from source: {error}",
                    gnu_emacs.name, root_source.name
                ),
                CommandError::TimedOut(_) => format!(
                    "{} source installation for {} timed out after {:?}",
                    gnu_emacs.name, root_source.name, gnu_emacs.timeout
                ),
                CommandError::Capture(error) => format!(
                    "failed to capture {} source installation for {}: {error}",
                    gnu_emacs.name, root_source.name
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
        || !plan.iter().all(|source| {
            home.join(".emacs.d/elpa")
                .join(format!("{}-{}", source.name, source.version))
                .join(format!("{}-pkg.el", source.name))
                .is_file()
        })
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

    let marker_tmp = root.join(format!("ready.{}.tmp", std::process::id()));
    fs::write(&marker_tmp, &expected).map_err(|error| {
        format!(
            "failed to write source install marker {}: {error}",
            marker_tmp.display()
        )
    })?;
    fs::rename(&marker_tmp, &ready).map_err(|error| {
        format!(
            "failed to publish source install marker {}: {error}",
            ready.display()
        )
    })?;
    Ok(package_dir)
}

pub fn prepare_cached_locked_melpa_package(
    gnu_emacs: &EmacsRuntime,
    package: (&str, &str),
) -> Result<PathBuf, String> {
    let plan = locked_melpa_install_plan(package)?;
    prepare_cached_locked_package_plan(gnu_emacs, &plan)
}

pub fn preflight_locked_melpa_packages(
    gnu_emacs: &EmacsRuntime,
    packages: &[(&str, &str)],
) -> Result<Vec<PathBuf>, String> {
    packages
        .iter()
        .map(|package| {
            prepare_cached_locked_melpa_package(gnu_emacs, *package).map_err(|error| {
                format!(
                    "MELPA source preflight failed before parity tests for {} {}:\n{error}",
                    package.0, package.1
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::process::Command;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use super::{
        AUCTEX_RUNTIME_EXCLUDED_PATHS, AUCTEX_RUNTIME_REQUIRED_FILES, LockedPackageCatalog,
        LockedPackageSource, SourceBuild, SourceBuildTools, extract_docstrip_source,
        prepare_cached_source_artifact_with_tools, prepare_source_checkout,
        prune_auctex_elpa_ignored, runtime_tree_file_spec, verify_auctex_runtime_tree,
    };
    use crate::{EmacsRuntime, MelpaSandbox};

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
        let fixture = MelpaSandbox::new("runtime-tree-files").expect("create runtime-tree fixture");
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

        let file_spec =
            runtime_tree_file_spec(&checkout).expect("enumerate recursive package entries");
        for required in AUCTEX_RUNTIME_REQUIRED_FILES {
            assert!(file_spec.contains(&format!("(:rename \"{required}\" \"{required}\")")));
        }
        for excluded in AUCTEX_RUNTIME_EXCLUDED_PATHS {
            assert!(!checkout.join(excluded).exists());
            assert!(!file_spec.contains(excluded));
        }
    }

    #[cfg(unix)]
    fn initialize_git_repository(directory: &Path, nonce: &str) -> (String, String) {
        fs::create_dir_all(directory).expect("create contract Git repository");
        fs::write(directory.join("source.el"), format!(";; {nonce}\n"))
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

        let fixture = MelpaSandbox::new(label).expect("create source cache contract sandbox");
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos()
            .to_string();
        let repository = fixture.root().join("repository");
        let (repository, revision) = initialize_git_repository(&repository, &nonce);
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
            nonce
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
        let runtime = EmacsRuntime::new("fake-source-builder", runtime_script)
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
        let invocations =
            fs::read_to_string(invocation_log).expect("read source builder invocations");
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
        let fixture =
            MelpaSandbox::new("source-fallback-contract").expect("create fallback sandbox");
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
}

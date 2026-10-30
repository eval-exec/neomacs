//! Native Linux PGO ownership: prepare, default/VM train, merge, then consume.
//!
//! Every invocation owns a fresh persistent run directory. Preparation counters
//! never enter training, profile contents never hide behind a reused path, and
//! normal fingerprint/dump production precedes execution of a new editor.
use super::{BuildProfile, FreshBuildOptions, Result};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};
use strum::IntoEnumIterator;

const HOST: &str = "x86_64-unknown-linux-gnu";
const ENCODED_FLAGS: &str = "CARGO_ENCODED_RUSTFLAGS";

#[derive(Clone, Copy, Debug, Serialize, strum::EnumIter, strum::IntoStaticStr)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
enum TrainingMode {
    Default,
    Vm,
}
impl TrainingMode {
    fn name(self) -> &'static str {
        self.into()
    }
    fn result_name(self) -> &'static str {
        match self {
            Self::Default => "batch-result.json",
            Self::Vm => "vm-result.json",
        }
    }
}

/// The entry decision is made before any PGO tool query or filesystem change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FreshBuildOperation {
    ExistingProduct,
    BuildProduct,
    NativePgo,
}
impl FreshBuildOperation {
    pub(super) fn for_options(options: &FreshBuildOptions) -> Self {
        if options.skip_build {
            Self::ExistingProduct
        } else if options.profile.consumes_pgo() {
            Self::NativePgo
        } else {
            Self::BuildProduct
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum CodegenContract {
    Shipping,
    Diagnostic,
}
impl CodegenContract {
    fn for_consumer(profile: &BuildProfile) -> Result<Self> {
        match profile {
            BuildProfile::ReleasePgo => Ok(Self::Shipping),
            BuildProfile::ReleasePgoProfiling => Ok(Self::Diagnostic),
            BuildProfile::Release
            | BuildProfile::Profiling
            | BuildProfile::ReleasePgoGen
            | BuildProfile::Dev
            | BuildProfile::Debug
            | BuildProfile::Test
            | BuildProfile::Custom(_) => Err("a consuming PGO profile is required".into()),
        }
    }
    fn debug(self) -> &'static str {
        match self {
            Self::Shipping => "0",
            Self::Diagnostic => "2",
        }
    }
}
#[derive(Clone, Copy, Debug, strum::EnumIter, strum::IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum BuildRole {
    Generation,
    Consumer,
}
impl BuildRole {
    fn name(self) -> &'static str {
        self.into()
    }
}
#[derive(Debug, Serialize)]
struct NativeToolchain {
    rustc: PathBuf,
    host: String,
    sysroot: PathBuf,
    profdata: PathBuf,
    rustc_version: String,
}
fn checked_output(program: &Path, args: &[&str]) -> Result<String> {
    checked_query_output(program, args, Command::new(program).args(args).output()?)
}
fn checked_query_output(program: &Path, args: &[&str], output: Output) -> Result<String> {
    if !output.status.success() {
        return Err(format!(
            "{} {} failed: {}",
            program.display(),
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let text = String::from_utf8(output.stdout)?.trim().to_owned();
    if text.is_empty() {
        return Err(format!(
            "{} {} returned empty output",
            program.display(),
            args.join(" ")
        )
        .into());
    }
    Ok(text)
}
fn validate_native_host(host: &str, requested: Option<&OsStr>) -> Result<()> {
    if host != HOST || requested.is_some_and(|target| target != OsStr::new(host)) {
        return Err(format!("PGO currently requires native {HOST}; host={host}, requested={requested:?}; use an ordinary release build for other/cross targets").into());
    }
    Ok(())
}
fn profdata_path(sysroot: &Path, host: &str) -> PathBuf {
    sysroot
        .join("lib/rustlib")
        .join(host)
        .join("bin/llvm-profdata")
}
impl NativeToolchain {
    fn discover() -> Result<Self> {
        let rustc = env::var_os("RUSTC")
            .map(PathBuf::from)
            .unwrap_or_else(|| super::tool_program("rustc"));
        let host = checked_output(&rustc, &["--print", "host-tuple"])?;
        validate_native_host(&host, env::var_os("CARGO_BUILD_TARGET").as_deref())?;
        let sysroot = PathBuf::from(checked_output(&rustc, &["--print", "sysroot"])?);
        if !sysroot.is_absolute() {
            return Err("rustc sysroot must be absolute".into());
        }
        let profdata = profdata_path(&sysroot, &host);
        if !profdata.is_file() {
            return Err(format!(
                "matching llvm-profdata missing at {}; install rustup component llvm-tools",
                profdata.display()
            )
            .into());
        }
        Ok(Self {
            rustc_version: checked_output(&rustc, &["-vV"])?,
            rustc,
            host,
            sysroot,
            profdata,
        })
    }
}

#[derive(Clone, Debug)]
struct RustFlags(Vec<String>);
impl RustFlags {
    fn parse(encoded: Option<&str>, plain: Option<&str>, target: Option<&str>) -> Result<Self> {
        let mut values: Vec<String> = match encoded {
            Some(value) => value
                .split('\u{1f}')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
            None => plain
                .or(target)
                .unwrap_or("")
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
        };
        let mut codegen = Vec::new();
        let mut tokens = values.iter();
        while let Some(token) = tokens.next() {
            if token == "-C" {
                codegen.push(
                    tokens
                        .next()
                        .ok_or("incomplete -C in inherited rustflags")?
                        .clone(),
                );
            } else if let Some(option) = token.strip_prefix("-C") {
                codegen.push(option.to_owned());
            }
        }
        if codegen
            .iter()
            .any(|s| s.starts_with("profile-use") || s.starts_with("profile-generate"))
        {
            return Err("PGO owns the profile treatment; remove inherited profile-use/profile-generate flags".into());
        }
        if codegen.iter().any(|s| {
            s.contains("align-all-functions=") && s.as_str() != "llvm-args=-align-all-functions=6"
        }) {
            return Err("PGO requires the qualified align-all-functions=6 contract".into());
        }
        // The encoded flags replace config rustflags, so every flag the
        // shipping link must carry is restated here. The 2 MiB congruence
        // flags mirror .cargo/config.toml's Linux block: without them the
        // release-pgo binary loses the alignment the startup text-collapse
        // module depends on.
        for required in [
            "llvm-args=-align-all-functions=6",
            "link-arg=-Wl,--build-id",
            "link-arg=-Wl,-z,max-page-size=0x200000",
            "link-arg=-Wl,-z,common-page-size=0x200000",
        ] {
            if !codegen.iter().any(|s| s == required) {
                values.push(format!("-C{required}"));
            }
        }
        Ok(Self(values))
    }
    fn inherited() -> Result<Self> {
        fn value(name: &str) -> Result<Option<String>> {
            match env::var(name) {
                Ok(value) => Ok(Some(value)),
                Err(env::VarError::NotPresent) => Ok(None),
                Err(env::VarError::NotUnicode(_)) => {
                    Err(format!("{name} must be UTF-8 for the owned PGO recipe").into())
                }
            }
        }
        let encoded = value(ENCODED_FLAGS)?;
        let plain = value("RUSTFLAGS")?;
        let target = value("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS")?;
        Self::parse(encoded.as_deref(), plain.as_deref(), target.as_deref())
    }
    fn with_treatment(&self, flag: &str, path: &Path) -> String {
        let mut values = self.0.clone();
        values.push(format!("-C{flag}={}", path.display()));
        values.join("\u{1f}")
    }
}

#[derive(Clone, Debug, Default)]
struct ChildEnvironment {
    set: BTreeMap<OsString, OsString>,
    remove: Vec<OsString>,
}
impl ChildEnvironment {
    fn set(&mut self, key: &str, value: impl Into<OsString>) {
        self.set.insert(key.into(), value.into());
    }
    fn pairs(&self) -> Vec<(OsString, OsString)> {
        self.set
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
    fn apply(&self, command: &mut Command) {
        for key in &self.remove {
            command.env_remove(key);
        }
        super::configure_fresh_build_environment(command, &self.pairs());
    }
    fn cargo(flags: String, profile: &BuildProfile, contract: CodegenContract) -> Self {
        let mut result = Self::default();
        result.remove = ["RUSTFLAGS", "LLVM_PROFILE_FILE", "CARGO_BUILD_TARGET"]
            .map(OsString::from)
            .to_vec();
        result.set(ENCODED_FLAGS, flags);
        result.set("NEOMACS_BUILD_PROFILE", profile.as_name());
        result.set("CARGO_INCREMENTAL", "0");
        let debug = contract.debug();
        let name = profile.as_name().replace('-', "_").to_uppercase();
        for (key, value) in [
            ("LTO", "fat"),
            ("DEBUG", debug),
            ("CODEGEN_UNITS", "1"),
            ("INCREMENTAL", "false"),
            ("OPT_LEVEL", "3"),
            ("STRIP", "none"),
            ("PANIC", "unwind"),
            ("DEBUG_ASSERTIONS", "false"),
            ("OVERFLOW_CHECKS", "false"),
            ("SPLIT_DEBUGINFO", "off"),
        ] {
            result.set(&format!("CARGO_PROFILE_{name}_{key}"), value);
        }
        result
    }
    fn training(mode: TrainingMode, runtime: &Path, raw: &Path, work: &Path) -> Self {
        let mut result = Self::default();
        result.remove = training_controls_to_remove(env::vars_os().map(|(key, _)| key));
        result.set("NEOMACS_RUNTIME_ROOT", runtime.as_os_str());
        result.set("NEOMACS_PGO_TRAIN_PASS", mode.name());
        result.set("NEOMACS_PGO_TRAIN_ROOT", work.as_os_str());
        result.set(
            "LLVM_PROFILE_FILE",
            RunPaths::profile_file(raw).into_os_string(),
        );
        match mode {
            TrainingMode::Default => {}
            TrainingMode::Vm => {
                result.set("NEOVM_JIT", "0");
            }
        }
        result
    }
}

/// Natural training owns all VM, profiling, and training controls. Deriving
/// removals from names (rather than mutating the parent environment) also keeps
/// unrelated concurrent build processes untouched.
fn training_controls_to_remove(keys: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    let mut result = keys
        .into_iter()
        .filter(|key| {
            key.to_string_lossy().starts_with("NEOVM_")
                || key.to_string_lossy().starts_with("NEOMACS_PERF_")
                || key.to_string_lossy().starts_with("NEOMACS_PGO_")
        })
        .collect::<Vec<_>>();
    result.extend(["NEOVM_JIT", "LLVM_PROFILE_FILE"].map(OsString::from));
    result.sort();
    result.dedup();
    result
}

#[derive(Debug)]
struct RunPaths {
    root: PathBuf,
    preparation_raw: PathBuf,
    merged: PathBuf,
}
impl RunPaths {
    fn new(target: &Path) -> Result<Self> {
        let target_text = target.to_str().ok_or("PGO target path must be UTF-8")?;
        if target_text.contains('\u{1f}') {
            return Err("PGO target path contains the encoded-rustflags delimiter".into());
        }
        let parent = target.join("pgo");
        fs::create_dir_all(&parent)?;
        // Keep the unique merged profile/receipts after success or failure.
        // Its fresh path changes Cargo/sccache's treatment key; intermediate
        // raw profiles and instrumented products are pruned only after merge.
        let root = tempfile::Builder::new()
            .prefix("run-")
            .tempdir_in(parent)?
            .keep();
        let preparation_raw = root.join("preparation-raw");
        fs::create_dir(&preparation_raw)?;
        Ok(Self {
            merged: root.join("merged.profdata"),
            root,
            preparation_raw,
        })
    }
    /// Online merging shares four slots per instrumented module, including
    /// across the trainer's fully drained child processes. Each owner/pass
    /// still has a distinct directory, so preparation never enters training.
    fn profile_file(raw: &Path) -> PathBuf {
        raw.join("default_%4m.profraw")
    }
    fn raw(&self, mode: TrainingMode) -> PathBuf {
        self.root.join("training-raw").join(mode.name())
    }
    fn work(&self, mode: TrainingMode) -> PathBuf {
        self.root.join("training").join(mode.name())
    }
    fn training_fixture(&self, passes: &Value) -> Result<PathBuf> {
        let row = passes
            .as_array()
            .and_then(|rows| {
                rows.iter()
                    .find(|row| row["mode"] == json!(TrainingMode::Default))
            })
            .ok_or("default training proof missing")?;
        let proof = row["receipts"]
            .as_array()
            .and_then(|rows| rows.first())
            .ok_or("default batch receipt missing")?;
        let path = PathBuf::from(
            proof["path"]
                .as_str()
                .ok_or("default batch receipt path missing")?,
        );
        let run = path.parent().ok_or("default training run missing")?;
        if path.file_name() != Some(OsStr::new(TrainingMode::Default.result_name()))
            || run.parent() != Some(self.work(TrainingMode::Default).as_path())
            || !run
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("run-"))
            || identity(&path)? != *proof
        {
            return Err("default batch receipt does not prove the owned training fixture".into());
        }
        Ok(run.join("repository"))
    }
}
/// A native PGO invocation owns these shared generation profile directories
/// until its explicit cleanup completes. The file lock is released by File's
/// Drop on every exit; independent processes using this owner serialize.
#[derive(Debug)]
struct TargetOwnership {
    target: PathBuf,
    canonical_target: PathBuf,
    _lock: File,
}
impl TargetOwnership {
    fn acquire(target: &Path) -> Result<Self> {
        if !target.is_absolute()
            || target
                .components()
                .any(|p| matches!(p, std::path::Component::ParentDir))
        {
            return Err("PGO cleanup requires an absolute target without parent traversal".into());
        }
        fs::create_dir_all(target)?;
        let canonical_target = target.canonicalize()?;
        let parent = target.join("pgo");
        match fs::symlink_metadata(&parent) {
            Ok(meta) if !meta.file_type().is_dir() => {
                return Err("PGO ownership directory must not be a symlink or file".into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&parent)?,
            Err(error) => return Err(error.into()),
        }
        let path = parent.join("owner.lock");
        match fs::symlink_metadata(&path) {
            Ok(meta) if !meta.file_type().is_file() => {
                return Err("PGO ownership lock must be a regular file".into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let lock = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        lock.lock()?;
        if path.canonicalize()? != canonical_target.join("pgo/owner.lock") {
            return Err("PGO target ownership path changed".into());
        }
        Ok(Self {
            target: target.to_owned(),
            canonical_target,
            _lock: lock,
        })
    }
    fn validate_run(&self, paths: &RunPaths) -> Result<()> {
        if self.target.canonicalize()? != self.canonical_target
            || paths.root.parent() != Some(self.target.join("pgo").as_path())
            || !paths
                .root
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("run-"))
            || paths.preparation_raw != paths.root.join("preparation-raw")
            || paths.merged != paths.root.join("merged.profdata")
        {
            return Err("PGO cleanup paths do not belong to this target/run".into());
        }
        Ok(())
    }
    fn validate_directory(&self, path: &Path) -> Result<()> {
        let relative = path.strip_prefix(&self.target)?;
        let mut current = self.target.clone();
        for part in relative.components() {
            let std::path::Component::Normal(part) = part else {
                return Err("PGO cleanup traversal rejected".into());
            };
            current.push(part);
            match fs::symlink_metadata(&current) {
                Ok(meta) if !meta.file_type().is_dir() => {
                    return Err(format!(
                        "PGO cleanup refuses symlink/non-directory {}",
                        current.display()
                    )
                    .into());
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
}

/// Only this closed set of disposable generation artifacts can be removed.
/// The runtime-named fixture parent comes from the validated batch receipt;
/// only its fixed repository child is removable. Consumer products are excluded.
#[derive(Clone, Copy, Debug, Serialize, strum::EnumIter)]
#[serde(rename_all = "kebab-case")]
enum DisposableArtifact {
    PreparationRaw,
    TrainingRaw,
    GenerationRawLinked,
    GenerationStaged,
    GenerationPublic,
    GenerationNative,
    TrainingFixture,
}
impl DisposableArtifact {
    fn path(self, owner: &TargetOwnership, paths: &RunPaths, fixture: &Path) -> PathBuf {
        match self {
            Self::PreparationRaw => paths.preparation_raw.clone(),
            Self::TrainingRaw => paths.root.join("training-raw"),
            Self::GenerationRawLinked => paths.root.join("generation/raw-linked"),
            Self::GenerationStaged => paths.root.join("generation/staged"),
            Self::GenerationPublic => owner
                .target
                .join(BuildProfile::ReleasePgoGen.target_subdir()),
            Self::GenerationNative => owner
                .target
                .join(HOST)
                .join(BuildProfile::ReleasePgoGen.target_subdir()),
            Self::TrainingFixture => fixture.to_owned(),
        }
    }
}

/// Sealed only after a successful matching merge and a second inventory check.
/// No raw/profile data is deleted when merge fails or its inputs change.
#[derive(Debug)]
struct VerifiedMerge {
    profile: Value,
    inputs: Value,
    training_fixture: PathBuf,
}
impl VerifiedMerge {
    fn seal(paths: &RunPaths, passes: &Value, command: &Value) -> Result<Self> {
        let args = merge_arguments(paths, passes)?;
        let expected = args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        if command["returncode"] != 0 || command["args"] != json!(expected) {
            return Err("successful matching PGO merge required before cleanup".into());
        }
        let profile = identity(&paths.merged)?;
        if profile["bytes"].as_u64().unwrap_or(0) == 0 {
            return Err("merged PGO profile is empty".into());
        }
        let inputs = json!({"preparation":inventory(&paths.preparation_raw)?,"training":passes});
        let training_fixture = paths.training_fixture(passes)?;
        Ok(Self {
            profile,
            inputs,
            training_fixture,
        })
    }
    fn verify_retained(&self, paths: &RunPaths, receipt: &Value) -> Result<()> {
        if identity(&paths.merged)? != self.profile
            || receipt["merged_profile"] != self.profile
            || receipt["cleanup"]["merged_profile"] != self.profile
            || receipt["cleanup"]["pre_prune_inventory"] != self.inputs
            || receipt["preparation_profiles"] != self.inputs["preparation"]
            || receipt["training_passes"] != self.inputs["training"]
            || receipt["cleanup"]["status"] != "complete"
        {
            return Err("sealed PGO merge/cleanup evidence changed during consumption".into());
        }
        Ok(())
    }
    fn prune(
        &self,
        owner: &TargetOwnership,
        paths: &RunPaths,
        receipt: &mut Value,
        receipt_path: &Path,
    ) -> Result<()> {
        owner.validate_run(paths)?;
        if identity(&paths.merged)? != self.profile
            || receipt["merged_profile"] != self.profile
            || receipt["preparation_profiles"] != self.inputs["preparation"]
            || receipt["training_passes"] != self.inputs["training"]
            || json!(inventory(&paths.preparation_raw)?) != self.inputs["preparation"]
        {
            return Err("sealed PGO merge/preparation changed before cleanup".into());
        }
        merge_arguments(paths, &self.inputs["training"])?;
        let artifacts = DisposableArtifact::iter()
            .map(|kind| (kind, kind.path(owner, paths, &self.training_fixture)))
            .collect::<Vec<_>>();
        // Validate every boundary before removing even the first directory.
        for (_, path) in &artifacts {
            owner.validate_directory(path)?;
        }
        receipt["cleanup"] = json!({"status":"running","after":"verified-merge-before-consumer","started":clock(),"merged_profile":self.profile,"pre_prune_inventory":self.inputs,"paths":artifacts.iter().map(|(kind,path)| json!({"kind":kind,"path":path,"status":"pending"})).collect::<Vec<_>>()});
        write_json(receipt_path, receipt)?;
        for (index, (_, path)) in artifacts.iter().enumerate() {
            // remove_dir_all does not follow symlink children. The target and
            // all ancestors/leaf were separately validated as directories.
            let result = match fs::remove_dir_all(path) {
                Ok(()) => Ok("removed"),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok("absent"),
                Err(error) => Err(error),
            };
            match result {
                Ok(status) => receipt["cleanup"]["paths"][index]["status"] = json!(status),
                Err(error) => {
                    receipt["cleanup"]["status"] = json!("failed");
                    receipt["cleanup"]["paths"][index]["error"] = json!(error.to_string());
                    write_json(receipt_path, receipt)?;
                    return Err(error.into());
                }
            }
            write_json(receipt_path, receipt)?;
        }
        receipt["cleanup"]["status"] = json!("complete");
        receipt["cleanup"]["finished"] = json!(clock());
        write_json(receipt_path, receipt)?;
        Ok(())
    }
}

fn target_root(options: &FreshBuildOptions) -> PathBuf {
    // Existing default_bin_dir already resolves absolute/relative target roots.
    super::default_bin_dir(&options.repo_root, &BuildProfile::ReleasePgo)
        .parent()
        .unwrap_or(&options.repo_root)
        .to_path_buf()
}
fn clock() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}
fn identity(path: &Path) -> Result<Value> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut chunk = [0_u8; 65536];
    loop {
        let n = file.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        hash.update(&chunk[..n]);
    }
    let digest = super::uppercase_hex(&hash.finalize()).to_ascii_lowercase();
    Ok(json!({"path":path,"bytes":file.metadata()?.len(),"sha256":digest}))
}
fn inventory(raw: &Path) -> Result<Vec<Value>> {
    let mut paths = fs::read_dir(raw)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    let mut result = Vec::new();
    for path in paths {
        if !fs::symlink_metadata(&path)?.file_type().is_file()
            || path.extension() != Some(OsStr::new("profraw"))
        {
            return Err(format!("unexpected PGO raw entry {}", path.display()).into());
        }
        result.push(identity(&path)?);
    }
    if result.is_empty() {
        return Err(format!("no training profiles in {}", raw.display()).into());
    }
    Ok(result)
}
fn write_json(path: &Path, value: &Value) -> Result<()> {
    let temporary = path.with_extension("json.new");
    fs::write(&temporary, serde_json::to_vec_pretty(value)?)?;
    fs::rename(temporary, path)?;
    Ok(())
}
fn git_value(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
fn source(root: &Path) -> Value {
    json!({"path":root,"commit":git_value(root,&["rev-parse","HEAD"]),"tree":git_value(root,&["rev-parse","HEAD^{tree}"]),"tracked_changes":git_value(root,&["status","--porcelain","--untracked-files=no"])})
}
fn recorded(
    root: &Path,
    label: &str,
    cwd: &Path,
    program: &Path,
    args: &[OsString],
    environment: &ChildEnvironment,
) -> Result<Value> {
    let stdout = root.join(format!("{label}.out"));
    let stderr = root.join(format!("{label}.err"));
    let mut row = json!({"program":program,"cwd":cwd,"args":args.iter().map(|s|s.to_string_lossy()).collect::<Vec<_>>(),"set_env":environment.set.iter().map(|(k,v)|(k.to_string_lossy(),v.to_string_lossy())).collect::<BTreeMap<_,_>>(),"remove_env":environment.remove.iter().map(|s|s.to_string_lossy()).collect::<Vec<_>>(),"started":clock(),"stdout":stdout,"stderr":stderr});
    let path = root.join(format!("{label}-command.json"));
    write_json(&path, &row)?;
    super::print_command(program.as_os_str(), args);
    let mut command = Command::new(program);
    command.args(args).current_dir(cwd);
    environment.apply(&mut command);
    if program.file_name() == Some(OsStr::new("cargo")) {
        super::remove_outer_cargo_env(&mut command);
    }
    let outcome = command
        .stdout(File::create(&stdout)?)
        .stderr(File::create(&stderr)?)
        .status();
    match outcome {
        Ok(status) => {
            row["returncode"] = json!(status.code());
            row["finished"] = json!(clock());
            row["stdout_proof"] = identity(&stdout)?;
            row["stderr_proof"] = identity(&stderr)?;
            write_json(&path, &row)?;
            if !status.success() {
                return Err(format!(
                    "PGO {label} failed ({status}); evidence retained at {}",
                    path.display()
                )
                .into());
            }
        }
        Err(error) => {
            row["error"] = json!(error.to_string());
            row["finished"] = json!(clock());
            write_json(&path, &row)?;
            return Err(error.into());
        }
    }
    Ok(row)
}
#[derive(Clone, Copy, Debug, strum::EnumIter, strum::IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum NativeBuildStep {
    DepsFirst,
    DepsSecond,
    Binary,
}
impl NativeBuildStep {
    fn name(self) -> &'static str {
        self.into()
    }
    fn selection(self) -> &'static [&'static str] {
        match self {
            Self::DepsFirst | Self::DepsSecond => &["--lib"],
            Self::Binary => &["--bin", "neomacs"],
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Products {
    Editor,
    EditorAndClient,
}
fn copy_products(native: &Path, stage: &Path, products: Products) -> Result<()> {
    fs::create_dir_all(stage)?;
    let names: &[&str] = match products {
        Products::Editor => &["neomacs"],
        Products::EditorAndClient => &["neomacs", "neomacsclient"],
    };
    for name in names {
        let from = native.join(super::executable_name(name));
        let to = stage.join(super::executable_name(name));
        if from != to {
            fs::copy(from, to)?;
        }
    }
    Ok(())
}
/// The retained default corpus is a closed ordered training population.
#[derive(Clone, Copy, Debug, strum::EnumIter, strum::IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum DefaultPhase {
    ElispEdit,
    OrgFontification,
    ByteCompile,
    Search,
    Text,
    LispData,
    OtherSourceFiles,
    BytecodeExecution,
    ProcessIo,
    TtyInputRedisplay,
}
impl DefaultPhase {
    fn name(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Copy, Debug, strum::EnumIter, strum::IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum VmPhase {
    VmLists,
    VmVectors,
    VmArithmetic,
    VmClosures,
    VmBundledSequences,
    VmBundledStrings,
}
impl VmPhase {
    fn name(self) -> &'static str {
        self.into()
    }
    fn checksum(self) -> i64 {
        match self {
            Self::VmLists | Self::VmVectors => 524288,
            Self::VmArithmetic => 16515072,
            Self::VmClosures => 20185088,
            Self::VmBundledSequences => 262144,
            Self::VmBundledStrings => 49152,
        }
    }
}

fn check_training_receipts(root: &Path, mode: TrainingMode) -> Result<Vec<Value>> {
    let children = fs::read_dir(root)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    if children.len() != 1
        || !children[0].is_dir()
        || !children[0]
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("run-"))
    {
        return Err("trainer must produce exactly one private run-* directory".into());
    }
    let run = &children[0];
    let result = run.join(mode.result_name());
    let data: Value = serde_json::from_slice(&fs::read(&result)?)?;
    if data["status"] != "complete"
        || !data["phases"]
            .as_array()
            .is_some_and(|p| !p.is_empty() && p.iter().all(|v| v["status"] == "complete"))
    {
        return Err(format!("incomplete {:?} trainer receipt", mode).into());
    }
    let mut proofs = vec![identity(&result)?];
    match mode {
        TrainingMode::Default => {
            let phases = data["phases"]
                .as_array()
                .ok_or("default training phases missing")?;
            if phases.len() != DefaultPhase::iter().count()
                || !phases
                    .iter()
                    .zip(DefaultPhase::iter())
                    .all(|(row, phase)| row["phase"] == phase.name())
            {
                return Err("default training coverage/order incomplete".into());
            }
            for name in ["driver-result.json", "tty-result.json"] {
                let path = run.join("tty").join(name);
                let row: Value = serde_json::from_slice(&fs::read(&path)?)?;
                if row["status"] != "complete" {
                    return Err("default TTY training did not complete".into());
                }
                if name == "driver-result.json"
                    && (row["returncode"] != 0
                        || !row["commands"].as_array().is_some_and(|v| v.len() == 104))
                {
                    return Err("TTY child did not exit successfully/complete all commands".into());
                }
                if name == "tty-result.json"
                    && (row["commands"] != 104 || row["buffers_restored"] != true)
                {
                    return Err("TTY did not restore buffers/complete all commands".into());
                }
                proofs.push(identity(&path)?);
            }
        }
        TrainingMode::Vm => {
            if data["pass"] != "vm"
                || data["jit"] != "0"
                || data["rounds"] != 8192
                || data["helpers"] != VmPhase::iter().count()
                || !data["phases"].as_array().is_some_and(|p| {
                    p.len() == VmPhase::iter().count()
                        && p.iter().zip(VmPhase::iter()).all(|(v, phase)| {
                            v["phase"] == phase.name()
                                && v["result"]["calls"] == 8192
                                && v["result"]["results_verified"] == true
                                && v["result"]["checksum"] == phase.checksum()
                                && v["result"]["checksum"] == v["result"]["expected_checksum"]
                        })
                })
            {
                return Err("VM training checksum/JIT contract failed".into());
            }
        }
    }
    Ok(proofs)
}

fn merge_arguments(paths: &RunPaths, passes: &Value) -> Result<Vec<OsString>> {
    let rows = passes.as_array().ok_or("training pass receipts missing")?;
    if rows.len() != TrainingMode::iter().count() {
        return Err("both training passes must complete before merge".into());
    }
    let mut args = vec![
        OsString::from("merge"),
        OsString::from("-o"),
        paths.merged.clone().into_os_string(),
    ];
    for (row, mode) in rows.iter().zip(TrainingMode::iter()) {
        if row["mode"] != serde_json::to_value(mode)?
            || row["raw_inventory"] != json!(inventory(&paths.raw(mode))?)
        {
            return Err("training pass identity/inventory changed before merge".into());
        }
        args.push(paths.raw(mode).into_os_string());
    }
    Ok(args)
}

pub(super) fn run(options: &FreshBuildOptions) -> Result<()> {
    let contract = CodegenContract::for_consumer(&options.profile)?;
    let toolchain = NativeToolchain::discover()?;
    let flags = RustFlags::inherited()?;
    let target = target_root(options);
    if options.dry_run {
        println!(
            "+ native PGO: explicit --target {}; fresh run under {}; preparation raw excluded; default then NEOVM_JIT=0 trainer; matching llvm-profdata {}; final standard stage {}",
            toolchain.host,
            target.join("pgo").display(),
            toolchain.profdata.display(),
            options.bin_dir.display()
        );
        return Ok(());
    }
    let owner = TargetOwnership::acquire(&target)?;
    let paths = RunPaths::new(&target)?;
    let path = paths.root.join("manifest.json");
    let mut receipt = json!({"schema":"neomacs-native-pgo-v1","status":"running","started":clock(),"source":source(&options.repo_root),"selected_profile":options.profile.as_name(),"runtime_root":options.runtime_root,"initial_cargo_args":super::initial_cargo_build_args(options).iter().map(|s|s.to_string_lossy().into_owned()).collect::<Vec<_>>(),"toolchain":toolchain,"run_root":paths.root,"preparation_raw":paths.preparation_raw,"merged_path":paths.merged,"common_rustflags":flags.0,"commands":[],"training_passes":[]});
    println!("+ PGO receipt: {}", path.display());
    write_json(&path, &receipt)?;
    let outcome = (|| -> Result<()> {
        let cargo = super::cargo_program();
        let generation = FreshBuildOptions {
            profile: BuildProfile::ReleasePgoGen,
            bin_dir: super::default_bin_dir(&options.repo_root, &BuildProfile::ReleasePgoGen),
            skip_build: true,
            ..options.clone()
        };
        let mut verified_merge = None;
        for role in BuildRole::iter() {
            let label = role.name();
            let (build, treatment, products) = match role {
                BuildRole::Generation => (
                    &generation,
                    flags.with_treatment("profile-generate", &paths.preparation_raw),
                    Products::Editor,
                ),
                BuildRole::Consumer => (
                    options,
                    flags.with_treatment("profile-use", &paths.merged),
                    Products::EditorAndClient,
                ),
            };
            if matches!(role, BuildRole::Consumer) {
                let merge_args = merge_arguments(&paths, &receipt["training_passes"])?;
                let row = recorded(
                    &paths.root,
                    "merge",
                    &options.repo_root,
                    &toolchain.profdata,
                    &merge_args,
                    &ChildEnvironment::default(),
                )?;
                let merged = VerifiedMerge::seal(&paths, &receipt["training_passes"], &row)?;
                receipt["commands"]
                    .as_array_mut()
                    .ok_or("commands schema")?
                    .push(row);
                receipt["merged_profile"] = merged.profile.clone();
                write_json(&path, &receipt)?;
                merged.prune(&owner, &paths, &mut receipt, &path)?;
                verified_merge = Some(merged);
            }
            let phase = paths.root.join(label);
            fs::create_dir(&phase)?;
            let mut environment = ChildEnvironment::cargo(treatment, &build.profile, contract);
            if matches!(role, BuildRole::Generation) {
                environment.set(
                    "LLVM_PROFILE_FILE",
                    RunPaths::profile_file(&paths.preparation_raw).into_os_string(),
                );
            }
            // The first library pass produces dependencies, the second settles
            // linker-plugin-LTO library artifacts, then only the editor links.
            // The same ordered recipe is used for generation and consumption.
            for step in NativeBuildStep::iter() {
                let mut args = super::initial_cargo_build_args(build);
                args.extend([OsString::from("--target"), toolchain.host.clone().into()]);
                args.extend(step.selection().iter().map(|value| OsString::from(*value)));
                let row = recorded(
                    &phase,
                    step.name(),
                    &options.repo_root,
                    &cargo,
                    &args,
                    &environment,
                )?;
                receipt["commands"]
                    .as_array_mut()
                    .ok_or("commands schema")?
                    .push(row);
                write_json(&path, &receipt)?;
            }
            let native = target
                .join(&toolchain.host)
                .join(build.profile.target_subdir());
            if matches!(role, BuildRole::Consumer) {
                let mut args = super::initial_cargo_build_args(build);
                args.extend([
                    OsString::from("--target"),
                    toolchain.host.clone().into(),
                    OsString::from("--bin"),
                    OsString::from("neomacsclient"),
                ]);
                let row = recorded(
                    &phase,
                    "client",
                    &options.repo_root,
                    &cargo,
                    &args,
                    &environment,
                )?;
                receipt["commands"]
                    .as_array_mut()
                    .ok_or("commands schema")?
                    .push(row);
            }
            let raw = phase.join("raw-linked");
            fs::create_dir(&raw)?;
            fs::copy(native.join("neomacs"), raw.join("neomacs"))?;
            receipt[format!("{label}_raw_binary")] = identity(&raw.join("neomacs"))?;
            copy_products(&native, &build.bin_dir, products)?;
            let stage = FreshBuildOptions {
                skip_build: true,
                ..build.clone()
            };
            let runtime_envs = if matches!(role, BuildRole::Generation) {
                vec![(
                    OsString::from("LLVM_PROFILE_FILE"),
                    RunPaths::profile_file(&paths.preparation_raw).into_os_string(),
                )]
            } else {
                Vec::new()
            };
            receipt[format!("{label}_standard_pipeline")] = json!({"invocation":"in-process run_fresh_build_pipeline","profile":stage.profile.as_name(),"bin_dir":stage.bin_dir,"runtime_root":stage.runtime_root,"skip_rust_build":true,"runtime_env":runtime_envs.iter().map(|(k,v)|(k.to_string_lossy(),v.to_string_lossy())).collect::<BTreeMap<_,_>>(),"started":clock(),"status":"running"});
            write_json(&path, &receipt)?;
            super::run_fresh_build_pipeline(&stage, &[], &runtime_envs)?;
            receipt[format!("{label}_standard_pipeline")]["status"] = json!("complete");
            receipt[format!("{label}_standard_pipeline")]["finished"] = json!(clock());
            let preserved = phase.join("staged");
            fs::create_dir(&preserved)?;
            fs::copy(build.bin_dir.join("neomacs"), preserved.join("neomacs"))?;
            fs::copy(
                build.bin_dir.join("neomacs.pdump"),
                preserved.join("neomacs.pdump"),
            )?;
            receipt[format!("{label}_binary")] = identity(&preserved.join("neomacs"))?;
            receipt[format!("{label}_pdump")] = identity(&preserved.join("neomacs.pdump"))?;
            receipt[format!("{label}_public_bin_dir")] = json!(build.bin_dir);
            write_json(&path, &receipt)?;
            if matches!(role, BuildRole::Generation) {
                receipt["preparation_profiles"] = json!(inventory(&paths.preparation_raw)?);
                for mode in TrainingMode::iter() {
                    let raw = paths.raw(mode);
                    let work = paths.work(mode);
                    fs::create_dir_all(&raw)?;
                    fs::create_dir_all(&work)?;
                    let env = ChildEnvironment::training(mode, &options.runtime_root, &raw, &work);
                    let trainer = options.repo_root.join("crates/xtask/pgo-train.el");
                    let args = [
                        OsString::from("--batch"),
                        OsString::from("-Q"),
                        OsString::from("-l"),
                        trainer.into_os_string(),
                    ];
                    let row = recorded(
                        &paths.root,
                        &format!("train-{}", mode.name()),
                        &options.repo_root,
                        &generation.bin_dir.join("neomacs"),
                        &args,
                        &env,
                    )?;
                    let pass = json!({"mode":mode,"command":row,"receipts":check_training_receipts(&work,mode)?,"raw_inventory":inventory(&raw)?});
                    receipt["training_passes"]
                        .as_array_mut()
                        .ok_or("training schema")?
                        .push(pass);
                    write_json(&path, &receipt)?;
                }
            }
        }
        verified_merge
            .ok_or("PGO consumer completed without a verified merge")?
            .verify_retained(&paths, &receipt)?;
        receipt["status"] = json!("complete");
        Ok(())
    })();
    if let Err(error) = &outcome {
        receipt["status"] = json!("failed-preserved");
        receipt["error"] = json!(error.to_string());
    }
    receipt["source_after"] = source(&options.repo_root);
    receipt["finished"] = json!(clock());
    write_json(&path, &receipt)?;
    outcome
}

#[cfg(test)]
#[path = "pgo/tests/pgo_test.rs"]
mod tests;

//! Verification of the assembled Android application boundary.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::NamedTempFile;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const BUILD_TOOLS_VERSION: &str = "36.0.0";
const NDK_VERSION: &str = "28.2.13676358";
const NATIVE_LIBRARY_ENTRY: &str = "lib/arm64-v8a/libneomacs_android.so";
const PORTABLE_ASSETS: [&str; 4] = [
    "neomacs.portable",
    "neomacs.portable.sha256",
    "neomacs-runtime.bundle",
    "neomacs-runtime.sha256",
];
const ALLOWED_NATIVE_DEPENDENCIES: [&str; 5] = [
    "libandroid.so",
    "libc.so",
    "libdl.so",
    "liblog.so",
    "libm.so",
];
const REQUIRED_NATIVE_EXPORTS: [&str; 3] = [
    "GameActivity_onCreate",
    "Java_com_google_androidgamesdk_GameActivity_initializeNativeCode",
    "android_main",
];

pub(super) fn run(repo_root: &Path, args: impl IntoIterator<Item = OsString>) -> Result<()> {
    let mut args = args.into_iter();
    let mut apk = None;
    let mut portable_assets = None;
    let mut android_sdk = None;
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--apk") => {
                apk = Some(resolve_path(
                    repo_root,
                    args.next().ok_or("--apk requires a path")?,
                ));
            }
            Some("--portable-assets") => {
                portable_assets = Some(resolve_path(
                    repo_root,
                    args.next().ok_or("--portable-assets requires a path")?,
                ));
            }
            Some("--android-sdk") => {
                android_sdk = Some(resolve_path(
                    repo_root,
                    args.next().ok_or("--android-sdk requires a path")?,
                ));
            }
            Some("-h" | "--help" | "help") => {
                print_usage();
                return Ok(());
            }
            Some(other) => return Err(format!("unknown verify-android-apk option: {other}").into()),
            None => return Err("verify-android-apk arguments must be valid Unicode".into()),
        }
    }

    let apk = apk.ok_or("verify-android-apk requires --apk PATH")?;
    let portable_assets =
        portable_assets.ok_or("verify-android-apk requires --portable-assets DIR")?;
    let android_sdk = android_sdk
        .or_else(|| env::var_os("ANDROID_SDK_ROOT").map(PathBuf::from))
        .or_else(|| env::var_os("ANDROID_HOME").map(PathBuf::from))
        .ok_or("set ANDROID_SDK_ROOT or pass --android-sdk DIR")?;

    verify(&apk, &portable_assets, &android_sdk)?;
    println!("+ verified Android package {}", apk.display());
    Ok(())
}

fn verify(apk: &Path, portable_assets: &Path, android_sdk: &Path) -> Result<()> {
    require_file(apk, "Android APK")?;
    if !portable_assets.is_dir() {
        return Err(format!(
            "portable asset directory was not found: {}",
            portable_assets.display()
        )
        .into());
    }

    let build_tools = android_sdk.join("build-tools").join(BUILD_TOOLS_VERSION);
    let aapt2 = require_tool(&build_tools.join(executable_name("aapt2")))?;
    let zipalign = require_tool(&build_tools.join(executable_name("zipalign")))?;
    let readelf = find_ndk_tool(android_sdk, "llvm-readelf")?;

    let listing = command_text(Command::new("unzip").arg("-lv").arg(apk), "list APK")?;
    validate_zip_listing(&listing)?;
    compare_packaged_assets(apk, portable_assets)?;

    let mut aapt = android_tool_command(&aapt2, &build_tools);
    aapt.args(["dump", "badging"]).arg(apk);
    validate_badging(&command_text(&mut aapt, "read Android manifest")?)?;

    let mut align = android_tool_command(&zipalign, &build_tools);
    align.args(["-c", "-P", "16", "-v", "4"]).arg(apk);
    command_output(&mut align, "verify Android ZIP alignment")?;

    let native_bytes = command_bytes(
        Command::new("unzip")
            .arg("-p")
            .arg(apk)
            .arg(NATIVE_LIBRARY_ENTRY),
        "extract Android native library",
    )?;
    let mut native = NamedTempFile::new()?;
    native.write_all(&native_bytes)?;
    native.as_file().sync_all()?;
    let report = command_text(
        Command::new(readelf).args([
            OsStr::new("--file-header"),
            OsStr::new("--program-headers"),
            OsStr::new("--dynamic"),
            OsStr::new("--dyn-syms"),
            OsStr::new("--wide"),
            native.path().as_os_str(),
        ]),
        "inspect Android native library",
    )?;
    validate_elf_report(&report)
}

fn compare_packaged_assets(apk: &Path, portable_assets: &Path) -> Result<()> {
    for asset in PORTABLE_ASSETS {
        let expected_path = portable_assets.join(asset);
        require_file(&expected_path, "portable Android asset")?;
        let archive_path = format!("assets/{asset}");
        let actual = command_bytes(
            Command::new("unzip").arg("-p").arg(apk).arg(&archive_path),
            "extract packaged portable asset",
        )?;
        let expected = fs::read(&expected_path)?;
        if actual != expected {
            return Err(format!(
                "packaged {archive_path} differs from {}",
                expected_path.display()
            )
            .into());
        }
    }
    Ok(())
}

pub(super) fn validate_zip_listing(listing: &str) -> Result<()> {
    let entries = listing
        .lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() >= 4 && (fields[1] == "Stored" || fields[1].starts_with("Defl")) {
                Some((fields.last()?.to_string(), fields[1].to_string()))
            } else {
                None
            }
        })
        .collect::<BTreeMap<_, _>>();

    let expected = std::iter::once(NATIVE_LIBRARY_ENTRY.to_owned())
        .chain(
            PORTABLE_ASSETS
                .into_iter()
                .map(|asset| format!("assets/{asset}")),
        )
        .collect::<BTreeSet<_>>();
    for entry in &expected {
        match entries.get(entry) {
            Some(method) if method == "Stored" => {}
            Some(method) => {
                return Err(
                    format!("Android package entry {entry} uses {method}, not Stored").into(),
                );
            }
            None => return Err(format!("Android package is missing {entry}").into()),
        }
    }

    for entry in entries.keys() {
        let neomacs_owned = entry.starts_with("assets/neomacs") || entry.starts_with("lib/");
        if neomacs_owned && !expected.contains(entry) {
            return Err(format!("unexpected Neomacs-owned Android package entry {entry}").into());
        }
    }
    Ok(())
}

pub(super) fn validate_badging(badging: &str) -> Result<()> {
    for required in [
        "package: name='org.neomacs'",
        "compileSdkVersion='36'",
        "minSdkVersion:'24'",
        "targetSdkVersion:'36'",
        "launchable-activity: name='com.google.androidgamesdk.GameActivity'",
        "native-code: 'arm64-v8a'",
    ] {
        if !badging.contains(required) {
            return Err(format!("Android manifest report is missing {required}").into());
        }
    }
    Ok(())
}

pub(super) fn validate_elf_report(report: &str) -> Result<()> {
    if !report.contains("Class:") || !report.contains("ELF64") {
        return Err("Android native library is not ELF64".into());
    }
    if !report.contains("Machine:") || !report.contains("AArch64") {
        return Err("Android native library is not AArch64".into());
    }

    let mut load_segments = 0;
    let mut dependencies = BTreeSet::new();
    let mut exports = BTreeSet::new();
    for line in report.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.first() == Some(&"LOAD") {
            load_segments += 1;
            let alignment = fields
                .last()
                .and_then(|value| value.strip_prefix("0x"))
                .and_then(|value| u64::from_str_radix(value, 16).ok())
                .ok_or("could not parse Android ELF LOAD alignment")?;
            if alignment < 0x4000 {
                return Err(
                    format!("Android ELF LOAD alignment 0x{alignment:x} is below 16 KiB").into(),
                );
            }
        }
        if let Some((_, tail)) = line.split_once("Shared library: [")
            && let Some(name) = tail.split(']').next()
        {
            dependencies.insert(name.to_owned());
        }
        if fields.contains(&"FUNC")
            && fields.contains(&"GLOBAL")
            && let Some(name) = fields.last()
        {
            exports.insert((*name).to_owned());
        }
    }
    if load_segments == 0 {
        return Err("Android ELF report contains no LOAD segments".into());
    }

    let allowed = ALLOWED_NATIVE_DEPENDENCIES
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    if dependencies != allowed {
        return Err(format!(
            "Android native dependencies differ: expected {allowed:?}, got {dependencies:?}"
        )
        .into());
    }
    for required in REQUIRED_NATIVE_EXPORTS {
        if !exports.contains(required) {
            return Err(format!("Android native library does not export {required}").into());
        }
    }
    Ok(())
}

fn command_text(command: &mut Command, description: &str) -> Result<String> {
    let output = command_output(command, description)?;
    String::from_utf8(output.stdout)
        .map_err(|_| format!("{description} produced non-UTF-8 output").into())
}

fn command_bytes(command: &mut Command, description: &str) -> Result<Vec<u8>> {
    Ok(command_output(command, description)?.stdout)
}

fn command_output(command: &mut Command, description: &str) -> Result<Output> {
    let output = command
        .output()
        .map_err(|error| format!("failed to {description}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "failed to {description} ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(output)
}

fn android_tool_command(program: &Path, build_tools: &Path) -> Command {
    let mut command = Command::new(program);
    command.env_remove("NIX_LD");
    command.env_remove("NIX_LD_LIBRARY_PATH");
    #[cfg(target_os = "linux")]
    command.env("LD_LIBRARY_PATH", build_tools.join("lib64"));
    command
}

fn find_ndk_tool(android_sdk: &Path, tool: &str) -> Result<PathBuf> {
    let prebuilt = android_sdk
        .join("ndk")
        .join(NDK_VERSION)
        .join("toolchains/llvm/prebuilt");
    let mut candidates = fs::read_dir(&prebuilt)
        .map_err(|error| format!("cannot inspect Android NDK {}: {error}", prebuilt.display()))?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path().join("bin").join(executable_name(tool)))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    candidates.sort();
    match candidates.as_slice() {
        [path] => Ok(path.clone()),
        [] => Err(format!(
            "Android NDK {NDK_VERSION} does not provide {tool} under {}",
            prebuilt.display()
        )
        .into()),
        _ => Err(format!(
            "Android NDK {NDK_VERSION} has multiple host {tool} tools under {}",
            prebuilt.display()
        )
        .into()),
    }
}

fn require_tool(path: &Path) -> Result<PathBuf> {
    require_file(path, "Android SDK tool")?;
    Ok(path.to_path_buf())
}

fn require_file(path: &Path, description: &str) -> Result<()> {
    if !path.is_file() {
        return Err(format!("{description} was not found: {}", path.display()).into());
    }
    Ok(())
}

fn executable_name(name: &str) -> OsString {
    #[cfg(windows)]
    return OsString::from(format!("{name}.exe"));
    #[cfg(not(windows))]
    OsString::from(name)
}

fn resolve_path(repo_root: &Path, value: OsString) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}

fn print_usage() {
    println!(
        "Usage: cargo xtask verify-android-apk --apk PATH --portable-assets DIR [--android-sdk DIR]"
    );
}

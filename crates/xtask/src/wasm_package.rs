//! Reproducible browser distribution assembly for `neomacs-wasm`.

use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::Builder;
use wasm_bindgen_cli_support::Bindgen;

use super::portable_assets::{
    PORTABLE_RUNTIME_IMAGE_ASSET, PORTABLE_RUNTIME_IMAGE_ID_ASSET, RUNTIME_RESOURCE_ARCHIVE_ASSET,
    RUNTIME_RESOURCE_ID_ASSET, sha256_file, validate_nonempty_file,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const WEB_SOURCE_FILES: [&str; 3] = ["index.html", "main.js", "style.css"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WasmPackageOptions {
    repo_root: PathBuf,
    portable_assets: PathBuf,
    output_dir: PathBuf,
    skip_build: bool,
}

impl WasmPackageOptions {
    pub(super) fn parse(
        repo_root: &Path,
        arguments: impl IntoIterator<Item = OsString>,
    ) -> Result<Self> {
        let mut arguments = arguments.into_iter();
        let mut portable_assets = None;
        let mut output_dir = None;
        let mut skip_build = false;

        while let Some(argument) = arguments.next() {
            match argument.to_str() {
                Some("--portable-assets") => {
                    portable_assets = Some(resolve_path(
                        repo_root,
                        arguments
                            .next()
                            .ok_or("--portable-assets requires a directory")?,
                    ));
                }
                Some("--output-dir") => {
                    output_dir = Some(resolve_path(
                        repo_root,
                        arguments
                            .next()
                            .ok_or("--output-dir requires a directory")?,
                    ));
                }
                Some("--skip-build") => skip_build = true,
                Some(other) => return Err(format!("unknown build-wasm option: {other}").into()),
                None => return Err("build-wasm arguments must be valid Unicode".into()),
            }
        }

        Ok(Self {
            repo_root: repo_root.to_path_buf(),
            portable_assets: portable_assets.ok_or("build-wasm requires --portable-assets DIR")?,
            output_dir: output_dir.ok_or("build-wasm requires --output-dir DIR")?,
            skip_build,
        })
    }
}

pub(super) fn run(repo_root: &Path, arguments: impl IntoIterator<Item = OsString>) -> Result<()> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    if matches!(
        arguments.as_slice(),
        [argument] if matches!(argument.to_str(), Some("-h" | "--help" | "help"))
    ) {
        print_usage();
        return Ok(());
    }

    package(WasmPackageOptions::parse(repo_root, arguments)?)
}

fn package(options: WasmPackageOptions) -> Result<()> {
    validate_output_destination(&options.output_dir)?;
    validate_portable_assets(&options.portable_assets)?;

    if !options.skip_build {
        build_wasm(&options.repo_root)?;
    }

    let input_wasm = wasm_artifact(&options.repo_root);
    validate_nonempty_file(&input_wasm, "optimized neomacs-wasm artifact")?;

    let parent = options
        .output_dir
        .parent()
        .ok_or("build-wasm output directory must have a parent")?;
    fs::create_dir_all(parent)?;
    let staging = Builder::new()
        .prefix(".neomacs-wasm-package-")
        .tempdir_in(parent)?;

    generate_bindings(&input_wasm, staging.path())?;
    copy_web_sources(&options.repo_root, staging.path())?;
    copy_portable_assets(&options.portable_assets, staging.path())?;

    // The destination was required not to exist, and the staging directory is
    // its sibling. A rename therefore publishes one complete tree without
    // exposing a partially assembled browser distribution.
    fs::rename(staging.path(), &options.output_dir)?;

    println!(
        "+ assembled neomacs-wasm browser distribution {}",
        options.output_dir.display()
    );
    Ok(())
}

pub(super) fn validate_output_destination(destination: &Path) -> Result<()> {
    if destination.exists() {
        return Err(format!(
            "build-wasm output directory must not already exist: {}",
            destination.display()
        )
        .into());
    }
    Ok(())
}

pub(super) fn validate_portable_assets(directory: &Path) -> Result<()> {
    if !directory.is_dir() {
        return Err(format!(
            "portable asset directory was not found: {}",
            directory.display()
        )
        .into());
    }

    validate_asset_digest(
        &directory.join(PORTABLE_RUNTIME_IMAGE_ASSET),
        &directory.join(PORTABLE_RUNTIME_IMAGE_ID_ASSET),
    )?;
    validate_asset_digest(
        &directory.join(RUNTIME_RESOURCE_ARCHIVE_ASSET),
        &directory.join(RUNTIME_RESOURCE_ID_ASSET),
    )
}

fn validate_asset_digest(asset: &Path, digest_file: &Path) -> Result<()> {
    validate_nonempty_file(asset, "portable browser asset")?;
    validate_nonempty_file(digest_file, "portable browser asset digest")?;

    let expected = fs::read_to_string(digest_file)?;
    let expected = expected.trim();
    if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "portable asset digest is not one SHA-256 value: {}",
            digest_file.display()
        )
        .into());
    }

    let actual = sha256_file(asset)?;
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(format!(
            "portable asset {} has SHA-256 {actual}, expected {expected}",
            asset.display()
        )
        .into());
    }
    Ok(())
}

fn build_wasm(repo_root: &Path) -> Result<()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let status = Command::new(cargo)
        .current_dir(repo_root)
        .args([
            "build",
            "--release",
            "--package",
            "neomacs-wasm",
            "--target",
            "wasm32-unknown-unknown",
        ])
        .status()?;
    if !status.success() {
        return Err(format!("optimized neomacs-wasm build failed with {status}").into());
    }
    Ok(())
}

fn wasm_artifact(repo_root: &Path) -> PathBuf {
    let target_root = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                repo_root.join(path)
            }
        })
        .unwrap_or_else(|| repo_root.join("target"));
    target_root
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("neomacs_wasm.wasm")
}

fn generate_bindings(input_wasm: &Path, output: &Path) -> Result<()> {
    let mut bindgen = Bindgen::new();
    bindgen
        .input_path(input_wasm)
        .out_name("neomacs_wasm")
        .typescript(true);
    bindgen.web(true)?;
    bindgen.generate(output)?;
    Ok(())
}

fn copy_web_sources(repo_root: &Path, output: &Path) -> Result<()> {
    let source_root = repo_root.join("crates/neomacs-wasm/web");
    for filename in WEB_SOURCE_FILES {
        copy_nonempty_file(
            &source_root.join(filename),
            &output.join(filename),
            "browser harness source",
        )?;
    }
    Ok(())
}

fn copy_portable_assets(source: &Path, output: &Path) -> Result<()> {
    let destination = output.join("assets");
    fs::create_dir(&destination)?;
    for filename in [
        PORTABLE_RUNTIME_IMAGE_ASSET,
        PORTABLE_RUNTIME_IMAGE_ID_ASSET,
        RUNTIME_RESOURCE_ARCHIVE_ASSET,
        RUNTIME_RESOURCE_ID_ASSET,
    ] {
        copy_nonempty_file(
            &source.join(filename),
            &destination.join(filename),
            "portable browser asset",
        )?;
    }
    Ok(())
}

fn copy_nonempty_file(source: &Path, destination: &Path, description: &str) -> Result<()> {
    validate_nonempty_file(source, description)?;
    fs::copy(source, destination)?;
    Ok(())
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
        "Usage: cargo xtask build-wasm --portable-assets DIR --output-dir DIR [--skip-build]\n\
         \n\
         Builds the optimized wasm32 neomacs-wasm artifact, runs the exactly pinned\n\
         wasm-bindgen transform, verifies the portable runtime assets, and publishes\n\
         a complete static browser distribution without overwriting existing output.\n\
         \n\
         --skip-build reuses target/wasm32-unknown-unknown/release/neomacs_wasm.wasm."
    );
}

use std::ffi::OsString;

use tempfile::tempdir;

use super::portable_assets::{
    PORTABLE_RUNTIME_IMAGE_ASSET, PORTABLE_RUNTIME_IMAGE_ID_ASSET, RUNTIME_RESOURCE_ARCHIVE_ASSET,
    RUNTIME_RESOURCE_ID_ASSET, sha256_file,
};
use super::wasm_package::{
    WasmArtifact, WasmPackageOptions, validate_output_destination, validate_portable_assets,
    wasm_artifact,
};

#[test]
fn wasm_package_options_require_portable_assets_and_output_directory() {
    let repo = tempdir().unwrap();
    let error = WasmPackageOptions::parse(repo.path(), Vec::<OsString>::new())
        .expect_err("missing product inputs must be rejected");

    assert!(error.to_string().contains("--portable-assets"));

    let error = WasmPackageOptions::parse(
        repo.path(),
        ["--portable-assets", "assets"].map(OsString::from),
    )
    .expect_err("missing output directory must be rejected");

    assert!(error.to_string().contains("--output-dir"));
}

#[test]
fn wasm_package_refuses_to_mix_with_an_existing_directory() {
    let temporary = tempdir().unwrap();
    let destination = temporary.path().join("dist");
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(destination.join("unrelated.txt"), "keep me").unwrap();

    let error = validate_output_destination(&destination)
        .expect_err("package output must not overwrite an existing tree");

    assert!(error.to_string().contains("must not already exist"));
    assert_eq!(
        std::fs::read_to_string(destination.join("unrelated.txt")).unwrap(),
        "keep me"
    );
}

#[test]
fn wasm_package_accepts_a_new_destination() {
    let temporary = tempdir().unwrap();

    validate_output_destination(&temporary.path().join("neomacs-wasm")).unwrap();
}

#[test]
fn wasm_package_addresses_frontend_and_editor_worker_as_distinct_artifacts() {
    let repo = std::path::Path::new("/workspace/neomacs");

    assert_eq!(
        wasm_artifact(repo, WasmArtifact::Frontend),
        repo.join("target/wasm32-unknown-unknown/release/neomacs_wasm.wasm"),
    );
    assert_eq!(
        wasm_artifact(repo, WasmArtifact::EditorWorker),
        repo.join("target/wasm32-unknown-unknown/release/neomacs_wasm_worker.wasm"),
    );
}

#[test]
fn wasm_package_accepts_portable_assets_with_matching_digests() {
    let temporary = tempdir().unwrap();
    write_valid_portable_assets(temporary.path());

    validate_portable_assets(temporary.path()).unwrap();
}

#[test]
fn wasm_package_rejects_a_portable_asset_changed_after_its_digest() {
    let temporary = tempdir().unwrap();
    write_valid_portable_assets(temporary.path());
    std::fs::write(
        temporary.path().join(PORTABLE_RUNTIME_IMAGE_ASSET),
        "tampered image",
    )
    .unwrap();

    let error = validate_portable_assets(temporary.path())
        .expect_err("a stale digest must not authorize different runtime bytes");

    assert!(error.to_string().contains("has SHA-256"));
}

fn write_valid_portable_assets(directory: &std::path::Path) {
    for (asset_name, digest_name, contents) in [
        (
            PORTABLE_RUNTIME_IMAGE_ASSET,
            PORTABLE_RUNTIME_IMAGE_ID_ASSET,
            b"portable image".as_slice(),
        ),
        (
            RUNTIME_RESOURCE_ARCHIVE_ASSET,
            RUNTIME_RESOURCE_ID_ASSET,
            b"runtime resources".as_slice(),
        ),
    ] {
        let asset = directory.join(asset_name);
        std::fs::write(&asset, contents).unwrap();
        let digest = format!("{}\n", sha256_file(&asset).unwrap());
        std::fs::write(directory.join(digest_name), digest).unwrap();
    }
}

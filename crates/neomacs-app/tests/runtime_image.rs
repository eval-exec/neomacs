#![cfg(not(target_family = "wasm"))]

use std::cell::Cell;
use std::io::Cursor;

use neomacs_app::host::{ExecutionEngine, HostProfile, RuntimeImageModel};
use neomacs_app::runtime_image::{
    ExtractedRuntimeImage, RuntimeImageError, RuntimeImageInstall, RuntimeImageSource,
};
use neovm_core::emacs_core::eval::Context;
use neovm_core::emacs_core::pdump::encode_portable_snapshot;
use neovm_core::emacs_core::value::Value;

#[test]
fn browser_profile_loads_linear_memory_snapshot() {
    let mut eval = Context::new();
    eval.set_variable("runtime-image-value", Value::fixnum(42));
    let bytes = encode_portable_snapshot(&eval).expect("encode portable image");

    let loaded = RuntimeImageSource::LinearMemory(&bytes)
        .load_for(HostProfile::WASM)
        .expect("load browser runtime image");

    assert_eq!(
        loaded.obarray().symbol_value("runtime-image-value"),
        Some(&Value::fixnum(42))
    );
}

#[test]
fn source_must_match_the_host_runtime_image_model() {
    let bytes = encode_portable_snapshot(&Context::new()).expect("encode portable image");
    let desktop = HostProfile::desktop(ExecutionEngine::Interpreter);

    assert!(matches!(
        RuntimeImageSource::LinearMemory(&bytes).load_for(desktop),
        Err(RuntimeImageError::ModelMismatch {
            host: RuntimeImageModel::MappedFile,
            source: RuntimeImageModel::LinearMemory,
        })
    ));
}

#[test]
fn extracted_image_is_installed_atomically_and_reused_without_reopening_asset() {
    let directory = tempfile::tempdir().expect("runtime image directory");
    let opens = Cell::new(0);
    let open_asset = || {
        opens.set(opens.get() + 1);
        Ok::<_, std::io::Error>(Cursor::new(b"runtime-image"))
    };

    let first = ExtractedRuntimeImage::prepare(directory.path(), "neomacs-test.pdump", open_asset)
        .expect("install runtime image");
    assert_eq!(first.install(), RuntimeImageInstall::Installed);
    assert_eq!(std::fs::read(first.path()).unwrap(), b"runtime-image");

    let second = ExtractedRuntimeImage::prepare(
        directory.path(),
        "neomacs-test.pdump",
        || -> std::io::Result<Cursor<&'static [u8]>> {
            panic!("an existing immutable image must not reopen the packaged asset")
        },
    )
    .expect("reuse runtime image");
    assert_eq!(second.install(), RuntimeImageInstall::Reused);
    assert_eq!(first.path(), second.path());
    assert_eq!(opens.get(), 1);
}

#[test]
fn extracted_image_rejects_paths_disguised_as_asset_names() {
    let directory = tempfile::tempdir().expect("runtime image directory");

    let error = ExtractedRuntimeImage::prepare(directory.path(), "../neomacs.pdump", || {
        Ok::<_, std::io::Error>(Cursor::new(b"runtime-image"))
    })
    .expect_err("asset name must be one path component");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
}

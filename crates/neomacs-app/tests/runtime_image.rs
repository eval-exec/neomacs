#![cfg(not(target_family = "wasm"))]

use std::cell::Cell;
use std::io::Cursor;

use neomacs_app::host::{ExecutionEngine, HostKind, HostProfile, RuntimeImageModel};
use neomacs_app::runtime_image::{
    ExtractedRuntimeImage, PORTABLE_FINAL_RUNTIME_IMAGE_ASSET,
    PORTABLE_FINAL_RUNTIME_IMAGE_ID_ASSET, RuntimeImageError, RuntimeImageInstall,
    RuntimeImageProvisionError, RuntimeImageSource,
};
use neovm_core::emacs_core::eval::Context;
use neovm_core::emacs_core::pdump::{dump_to_file, encode_portable_snapshot, load_from_dump};
use neovm_core::emacs_core::value::Value;
use sha2::{Digest, Sha256};

fn content_id(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn browser_profile_loads_linear_memory_snapshot() {
    let mut eval = Context::new();
    eval.set_variable("runtime-image-value", Value::fixnum(42));
    eval.set_variable("load-in-progress", Value::T);
    let bytes = encode_portable_snapshot(&eval).expect("encode portable image");

    let mut loaded = RuntimeImageSource::LinearMemory(&bytes)
        .load_for(HostProfile::WASM)
        .expect("load browser runtime image");

    assert_eq!(
        loaded.obarray().symbol_value("runtime-image-value"),
        Some(&Value::fixnum(42))
    );
    assert_eq!(
        loaded.obarray().symbol_value("load-in-progress"),
        Some(&Value::NIL),
        "transient image-construction state must not leak into a live session",
    );
    assert_eq!(
        loaded
            .eval_str("(+ runtime-image-value 1)")
            .expect("restored final image has callable Rust builtins"),
        Value::fixnum(43),
    );
    assert_eq!(
        loaded
            .obarray()
            .symbol_value("data-directory")
            .and_then(|value| value.as_utf8_str()),
        Some("/neomacs/etc/"),
        "browser startup must not discover a build-machine filesystem root",
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
fn android_requires_an_explicit_app_private_runtime_root() {
    assert!(matches!(
        RuntimeImageSource::ExtractedFile(std::path::Path::new("never-read.pdump"))
            .load_for(HostProfile::android()),
        Err(RuntimeImageError::RuntimeRootRequired {
            host: HostKind::Android,
        })
    ));
}

#[test]
fn android_finalization_uses_the_supplied_app_private_runtime_root() {
    let directory = tempfile::tempdir().unwrap();
    let runtime_root = directory.path().join("runtime");
    std::fs::create_dir_all(runtime_root.join("lisp")).unwrap();
    std::fs::create_dir_all(runtime_root.join("etc/charsets")).unwrap();
    let image_path = directory.path().join("android.pdump");
    dump_to_file(&Context::new(), &image_path).unwrap();

    let loaded = RuntimeImageSource::ExtractedFile(&image_path)
        .load_for_in_runtime_root(HostProfile::android(), &runtime_root)
        .expect("load Android image with explicit resources");

    let expected = format!("{}/etc/", runtime_root.display());
    assert_eq!(
        loaded
            .obarray()
            .symbol_value("data-directory")
            .and_then(|value| value.as_utf8_str()),
        Some(expected.as_str()),
    );
}

#[test]
fn final_asset_extraction_owns_the_fingerprinted_product_name() {
    let directory = tempfile::tempdir().unwrap();
    let image = ExtractedRuntimeImage::prepare_final(directory.path(), |asset_name| {
        assert!(asset_name.starts_with("neomacs-"));
        assert!(asset_name.ends_with(".pdump"));
        Ok::<_, std::io::Error>(Cursor::new(b"final-image"))
    })
    .unwrap();

    assert_eq!(std::fs::read(image.path()).unwrap(), b"final-image");
}

#[test]
fn portable_asset_materializes_and_reuses_a_target_native_final_image() {
    let directory = tempfile::tempdir().unwrap();
    let mut source = Context::new();
    source.set_variable("portable-runtime-value", Value::fixnum(73));
    let portable = encode_portable_snapshot(&source).expect("encode portable image");
    let portable_id = format!("{}\n", content_id(&portable));
    let opens = Cell::new(0);

    let first =
        ExtractedRuntimeImage::prepare_final_from_portable(directory.path(), |asset_name| {
            opens.set(opens.get() + 1);
            let bytes = match asset_name {
                PORTABLE_FINAL_RUNTIME_IMAGE_ID_ASSET => portable_id.as_bytes(),
                PORTABLE_FINAL_RUNTIME_IMAGE_ASSET => portable.as_slice(),
                other => panic!("unexpected portable runtime asset {other}"),
            };
            Ok::<_, std::io::Error>(Cursor::new(bytes))
        })
        .expect("materialize target-native image");

    assert_eq!(first.install(), RuntimeImageInstall::Installed);
    assert!(
        first
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("neomacs-") && name.ends_with(".pdump")),
    );
    let loaded = load_from_dump(first.path()).expect("load materialized native image");
    assert_eq!(
        loaded.obarray().symbol_value("portable-runtime-value"),
        Some(&Value::fixnum(73)),
    );

    let second = ExtractedRuntimeImage::prepare_final_from_portable(
        directory.path(),
        |asset_name| -> std::io::Result<Cursor<&[u8]>> {
            assert_eq!(asset_name, PORTABLE_FINAL_RUNTIME_IMAGE_ID_ASSET);
            Ok(Cursor::new(portable_id.as_bytes()))
        },
    )
    .expect("reuse target-native image");
    assert_eq!(second.install(), RuntimeImageInstall::Reused);
    assert_eq!(first.path(), second.path());
    assert_eq!(opens.get(), 2);
}

#[test]
fn portable_image_content_selects_the_cached_native_image() {
    let directory = tempfile::tempdir().unwrap();
    let portable = |value| {
        let mut source = Context::new();
        source.set_variable("portable-runtime-value", Value::fixnum(value));
        encode_portable_snapshot(&source).expect("encode portable image")
    };

    let first_portable = portable(1);
    let first_id = format!("{}\n", content_id(&first_portable));
    let first =
        ExtractedRuntimeImage::prepare_final_from_portable(directory.path(), |asset_name| {
            Ok::<_, std::io::Error>(Cursor::new(match asset_name {
                PORTABLE_FINAL_RUNTIME_IMAGE_ID_ASSET => first_id.as_bytes(),
                PORTABLE_FINAL_RUNTIME_IMAGE_ASSET => first_portable.as_slice(),
                other => panic!("unexpected portable runtime asset {other}"),
            }))
        })
        .expect("materialize first target-native image");

    let second_portable = portable(2);
    let second_id = format!("{}\n", content_id(&second_portable));
    let second =
        ExtractedRuntimeImage::prepare_final_from_portable(directory.path(), |asset_name| {
            Ok::<_, std::io::Error>(Cursor::new(match asset_name {
                PORTABLE_FINAL_RUNTIME_IMAGE_ID_ASSET => second_id.as_bytes(),
                PORTABLE_FINAL_RUNTIME_IMAGE_ASSET => second_portable.as_slice(),
                other => panic!("unexpected portable runtime asset {other}"),
            }))
        })
        .expect("materialize second target-native image");

    assert_ne!(first.path(), second.path());
    assert_eq!(second.install(), RuntimeImageInstall::Installed);
    let loaded = load_from_dump(second.path()).expect("load second native image");
    assert_eq!(
        loaded.obarray().symbol_value("portable-runtime-value"),
        Some(&Value::fixnum(2)),
    );
}

#[test]
fn portable_image_must_match_its_packaged_content_id() {
    let directory = tempfile::tempdir().unwrap();
    let portable = encode_portable_snapshot(&Context::new()).expect("encode portable image");
    let wrong_id = format!("{}\n", content_id(b"different portable image"));

    let error =
        ExtractedRuntimeImage::prepare_final_from_portable(directory.path(), |asset_name| {
            Ok::<_, std::io::Error>(Cursor::new(match asset_name {
                PORTABLE_FINAL_RUNTIME_IMAGE_ID_ASSET => wrong_id.as_bytes(),
                PORTABLE_FINAL_RUNTIME_IMAGE_ASSET => portable.as_slice(),
                other => panic!("unexpected portable runtime asset {other}"),
            }))
        })
        .expect_err("portable image digest mismatch must fail closed");

    assert!(matches!(
        error,
        RuntimeImageProvisionError::PortableImageDigestMismatch { .. }
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

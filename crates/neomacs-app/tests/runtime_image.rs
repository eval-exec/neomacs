use neomacs_app::host::{ExecutionEngine, HostProfile, RuntimeImageModel};
use neomacs_app::runtime_image::{RuntimeImageError, RuntimeImageSource};
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

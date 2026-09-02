use neomacs_android::host_profile;
use neomacs_app::host::{ExecutionEngine, HostKind, ProcessModel, RuntimeImageModel, StorageModel};

#[test]
fn android_adapter_declares_its_restricted_native_profile() {
    let profile = host_profile();

    assert_eq!(profile.kind(), HostKind::Android);
    assert_eq!(profile.storage(), StorageModel::AndroidDocuments);
    assert_eq!(profile.processes(), ProcessModel::AndroidRestricted);
    assert_eq!(profile.runtime_images(), RuntimeImageModel::ExtractedFile);
    assert_eq!(profile.execution(), ExecutionEngine::Interpreter);
}

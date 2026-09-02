use neomacs_app::host::{ExecutionEngine, HostKind, ProcessModel, RuntimeImageModel, StorageModel};
use neomacs_wasm::host_profile;

#[test]
fn wasm_adapter_declares_its_browser_profile() {
    let profile = host_profile();

    assert_eq!(profile.kind(), HostKind::Wasm);
    assert_eq!(profile.storage(), StorageModel::BrowserHandles);
    assert_eq!(profile.processes(), ProcessModel::Unavailable);
    assert_eq!(profile.runtime_images(), RuntimeImageModel::LinearMemory);
    assert_eq!(profile.execution(), ExecutionEngine::Interpreter);
}

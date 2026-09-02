use neomacs_app::host::{
    ExecutionEngine, HostKind, HostOperation, HostOperationError, HostProfile,
};

const NATIVE_OPERATIONS: [HostOperation; 6] = [
    HostOperation::AccessNativePath,
    HostOperation::SpawnProcess,
    HostOperation::OpenPty,
    HostOperation::MapRuntimeImage,
    HostOperation::LoadNativeModule,
    HostOperation::CompileNativeCode,
];

#[test]
fn native_jit_desktop_accepts_every_declared_host_operation() {
    let profile = HostProfile::desktop(ExecutionEngine::NativeJit);

    for operation in NATIVE_OPERATIONS {
        assert_eq!(profile.require(operation), Ok(()), "{operation:?}");
    }
}

#[test]
fn android_can_map_an_extracted_private_runtime_image() {
    assert_eq!(
        HostProfile::android().require(HostOperation::MapRuntimeImage),
        Ok(())
    );
}

#[test]
fn wasm_rejects_process_creation_before_dispatch() {
    assert_eq!(
        HostProfile::WASM.require(HostOperation::SpawnProcess),
        Err(HostOperationError::Unsupported {
            host: HostKind::Wasm,
            operation: HostOperation::SpawnProcess,
        })
    );
}

#[test]
fn wasm_rejects_pty_creation_before_dispatch() {
    assert_eq!(
        HostProfile::WASM.require(HostOperation::OpenPty),
        Err(HostOperationError::Unsupported {
            host: HostKind::Wasm,
            operation: HostOperation::OpenPty,
        })
    );
}

#[test]
fn browser_documents_do_not_claim_native_path_access() {
    assert_eq!(
        HostProfile::WASM.require(HostOperation::AccessNativePath),
        Err(HostOperationError::Unsupported {
            host: HostKind::Wasm,
            operation: HostOperation::AccessNativePath,
        })
    );
}

#[test]
fn linear_memory_runtime_images_cannot_use_native_mmap() {
    assert_eq!(
        HostProfile::WASM.require(HostOperation::MapRuntimeImage),
        Err(HostOperationError::Unsupported {
            host: HostKind::Wasm,
            operation: HostOperation::MapRuntimeImage,
        })
    );
}

#[test]
fn interpreter_only_profile_rejects_native_code_generation() {
    assert_eq!(
        HostProfile::WASM.require(HostOperation::CompileNativeCode),
        Err(HostOperationError::Unsupported {
            host: HostKind::Wasm,
            operation: HostOperation::CompileNativeCode,
        })
    );
}

#[test]
fn wasm_rejects_native_dynamic_modules() {
    assert_eq!(
        HostProfile::WASM.require(HostOperation::LoadNativeModule),
        Err(HostOperationError::Unsupported {
            host: HostKind::Wasm,
            operation: HostOperation::LoadNativeModule,
        })
    );
}

#[test]
fn unsupported_operation_has_stable_user_facing_text() {
    let error = HostProfile::WASM
        .require(HostOperation::SpawnProcess)
        .expect_err("browser process creation must be rejected");

    assert_eq!(
        error.to_string(),
        "neomacs-wasm does not support spawning native processes"
    );
}

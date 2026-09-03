//! Typed descriptions of the facilities supplied by an embedding host.

use std::fmt::{Display, Formatter};

/// Product host running the editor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostKind {
    /// Native desktop application.
    Desktop,
    /// Native Android application.
    Android,
    /// Browser WebAssembly application.
    Wasm,
}

impl HostKind {
    /// Product host represented by the current compilation target.
    pub const CURRENT: Self = std::cfg_select! {
        target_family = "wasm" => { Self::Wasm }
        target_os = "android" => { Self::Android }
        _ => { Self::Desktop }
    };

    const fn product_name(self) -> &'static str {
        match self {
            Self::Desktop => "neomacs",
            Self::Android => "neomacs-android",
            Self::Wasm => "neomacs-wasm",
        }
    }
}

/// Source of `initial-environment` and the initial `process-environment`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessEnvironmentModel {
    /// Inherit the native process environment supplied by the operating system.
    InheritedNative,
    /// Browser-owned virtual paths replace an unavailable native environment.
    BrowserVirtualPaths,
}

impl ProcessEnvironmentModel {
    /// Environment model compiled into the current editor executable.
    pub const CURRENT: Self = std::cfg_select! {
        target_family = "wasm" => { Self::BrowserVirtualPaths }
        _ => { Self::InheritedNative }
    };
}

/// How user-visible documents are addressed by the host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageModel {
    /// Native filesystem paths.
    NativePaths,
    /// Android document-provider handles and application-private paths.
    AndroidDocuments,
    /// Browser file handles and origin-private storage.
    BrowserHandles,
}

/// Process facilities exposed by the host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessModel {
    /// Ordinary native child processes and process groups.
    Native,
    /// Processes constrained by the Android application sandbox.
    AndroidRestricted,
    /// The host cannot create operating-system processes.
    Unavailable,
}

/// Storage used for a prebuilt runtime image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeImageModel {
    /// A native file mapped into the process address space.
    MappedFile,
    /// A packaged image extracted to an application-private native file.
    ExtractedFile,
    /// A portable image copied into WebAssembly linear memory.
    LinearMemory,
}

/// Engine selected for Lisp execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionEngine {
    /// Portable interpreter and byte-code virtual machine.
    Interpreter,
    /// Native Cranelift just-in-time compilation.
    NativeJit,
}

/// Native dynamic-module facilities exposed by the host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeModuleModel {
    /// Native shared libraries can be loaded from ordinary filesystem paths.
    Native,
    /// Android permits only application-controlled native loading paths.
    AndroidRestricted,
    /// Native dynamic modules cannot be loaded.
    Unavailable,
}

/// A host operation whose availability must be decided before dispatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostOperation {
    /// Open a user document by an unrestricted native path.
    AccessNativePath,
    /// Start an arbitrary operating-system child process.
    SpawnProcess,
    /// Create a pseudo-terminal for a child process.
    OpenPty,
    /// Map a runtime image directly from a native file.
    MapRuntimeImage,
    /// Load a native dynamic module.
    LoadNativeModule,
    /// Generate and execute native machine code.
    CompileNativeCode,
}

impl HostOperation {
    const fn description(self) -> &'static str {
        match self {
            Self::AccessNativePath => "accessing unrestricted native paths",
            Self::SpawnProcess => "spawning native processes",
            Self::OpenPty => "opening native pseudo-terminals",
            Self::MapRuntimeImage => "memory-mapping runtime images",
            Self::LoadNativeModule => "loading native dynamic modules",
            Self::CompileNativeCode => "compiling native machine code",
        }
    }
}

/// Why a host cannot perform an operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostOperationError {
    /// The host has no implementation of the operation.
    Unsupported {
        /// Host which rejected the operation.
        host: HostKind,
        /// Rejected operation.
        operation: HostOperation,
    },
    /// The host OS has the facility, but its application sandbox restricts it.
    Restricted {
        /// Host which restricted the operation.
        host: HostKind,
        /// Restricted operation.
        operation: HostOperation,
    },
}

impl Display for HostOperationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::Unsupported { host, operation } => write!(
                formatter,
                "{} does not support {}",
                host.product_name(),
                operation.description()
            ),
            Self::Restricted { host, operation } => write!(
                formatter,
                "{} restricts {}",
                host.product_name(),
                operation.description()
            ),
        }
    }
}

impl std::error::Error for HostOperationError {}

/// Closed description of the semantic facilities available to one frontend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostProfile {
    kind: HostKind,
    storage: StorageModel,
    processes: ProcessModel,
    runtime_images: RuntimeImageModel,
    execution: ExecutionEngine,
    native_modules: NativeModuleModel,
}

impl HostProfile {
    /// Browser profile used by `neomacs-wasm`.
    pub const WASM: Self = Self {
        kind: HostKind::Wasm,
        storage: StorageModel::BrowserHandles,
        processes: ProcessModel::Unavailable,
        runtime_images: RuntimeImageModel::LinearMemory,
        execution: ExecutionEngine::Interpreter,
        native_modules: NativeModuleModel::Unavailable,
    };

    /// Construct the native desktop profile with the selected execution engine.
    pub const fn desktop(execution: ExecutionEngine) -> Self {
        Self {
            kind: HostKind::Desktop,
            storage: StorageModel::NativePaths,
            processes: ProcessModel::Native,
            runtime_images: RuntimeImageModel::MappedFile,
            execution,
            native_modules: NativeModuleModel::Native,
        }
    }

    /// Initial Android profile. Native-JIT support is intentionally deferred.
    pub const fn android() -> Self {
        Self {
            kind: HostKind::Android,
            storage: StorageModel::AndroidDocuments,
            processes: ProcessModel::AndroidRestricted,
            runtime_images: RuntimeImageModel::ExtractedFile,
            execution: ExecutionEngine::Interpreter,
            native_modules: NativeModuleModel::AndroidRestricted,
        }
    }

    /// Product host represented by this profile.
    pub const fn kind(self) -> HostKind {
        self.kind
    }

    /// User-document addressing model.
    pub const fn storage(self) -> StorageModel {
        self.storage
    }

    /// Process model.
    pub const fn processes(self) -> ProcessModel {
        self.processes
    }

    /// Runtime-image storage model.
    pub const fn runtime_images(self) -> RuntimeImageModel {
        self.runtime_images
    }

    /// Selected Lisp execution engine.
    pub const fn execution(self) -> ExecutionEngine {
        self.execution
    }

    /// Native dynamic-module model.
    pub const fn native_modules(self) -> NativeModuleModel {
        self.native_modules
    }

    /// Check an operation before it is sent to a host adapter.
    pub const fn require(self, operation: HostOperation) -> Result<(), HostOperationError> {
        match operation {
            HostOperation::AccessNativePath => match self.storage {
                StorageModel::NativePaths => Ok(()),
                StorageModel::AndroidDocuments => Err(HostOperationError::Restricted {
                    host: self.kind,
                    operation,
                }),
                StorageModel::BrowserHandles => Err(HostOperationError::Unsupported {
                    host: self.kind,
                    operation,
                }),
            },
            HostOperation::SpawnProcess | HostOperation::OpenPty => match self.processes {
                ProcessModel::Native => Ok(()),
                ProcessModel::AndroidRestricted => Err(HostOperationError::Restricted {
                    host: self.kind,
                    operation,
                }),
                ProcessModel::Unavailable => Err(HostOperationError::Unsupported {
                    host: self.kind,
                    operation,
                }),
            },
            HostOperation::MapRuntimeImage => match self.runtime_images {
                RuntimeImageModel::MappedFile | RuntimeImageModel::ExtractedFile => Ok(()),
                RuntimeImageModel::LinearMemory => Err(HostOperationError::Unsupported {
                    host: self.kind,
                    operation,
                }),
            },
            HostOperation::LoadNativeModule => match self.native_modules {
                NativeModuleModel::Native => Ok(()),
                NativeModuleModel::AndroidRestricted => Err(HostOperationError::Restricted {
                    host: self.kind,
                    operation,
                }),
                NativeModuleModel::Unavailable => Err(HostOperationError::Unsupported {
                    host: self.kind,
                    operation,
                }),
            },
            HostOperation::CompileNativeCode => match self.execution {
                ExecutionEngine::NativeJit => Ok(()),
                ExecutionEngine::Interpreter => Err(HostOperationError::Unsupported {
                    host: self.kind,
                    operation,
                }),
            },
        }
    }
}

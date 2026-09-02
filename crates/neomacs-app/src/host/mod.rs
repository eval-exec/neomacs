//! Typed descriptions of the facilities supplied by an application host.
//!
//! The vocabulary lives in `neovm-host-abi` because both the VM and its
//! embedding frontends must make capability decisions with the same closed
//! set of types.  Re-exporting it here preserves the application-facing API.

pub use neovm_host_abi::{
    ExecutionEngine, HostKind, HostOperation, HostOperationError, HostProfile, NativeModuleModel,
    ProcessModel, RuntimeImageModel, StorageModel,
};

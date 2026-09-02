//! Host-checked sources for evaluator startup images.

std::cfg_select! {
    target_family = "wasm" => {}
    _ => {
        mod extracted;
        pub use extracted::{
            ExtractedRuntimeImage, RuntimeImageInstall, RuntimeImageProvisionError,
        };
    }
}

use std::fmt::{Display, Formatter};
use std::path::Path;

use neovm_core::emacs_core::eval::Context;
use neovm_core::emacs_core::load::{
    RuntimeImageRole, finalize_restored_runtime_image, finalize_restored_runtime_image_at_root,
};
use neovm_core::emacs_core::pdump::{DumpError, load_from_dump, load_from_portable_snapshot};

use crate::host::{HostKind, HostProfile, RuntimeImageModel};

const WASM_RUNTIME_ROOT: &str = "/neomacs";

/// Target-independent final image bundled by portable product adapters.
///
/// Android turns this seed into a fingerprint-matched native pdump in
/// app-private storage. Browser WASM restores the same bytes directly into
/// linear memory.
pub const PORTABLE_FINAL_RUNTIME_IMAGE_ASSET: &str = "neomacs.portable";

/// SHA-256 content ID selecting the packaged portable final image.
pub const PORTABLE_FINAL_RUNTIME_IMAGE_ID_ASSET: &str = "neomacs.portable.sha256";

/// Concrete storage supplied by a frontend for one runtime image.
///
/// The variants intentionally mirror [`RuntimeImageModel`]. Keeping extracted
/// Android assets distinct from ordinary desktop files preserves the host
/// contract even though both ultimately use the native mmap loader.
#[derive(Clone, Copy, Debug)]
pub enum RuntimeImageSource<'a> {
    /// Desktop image available at an ordinary native path.
    MappedFile(&'a Path),
    /// Packaged Android image extracted into app-private storage.
    ExtractedFile(&'a Path),
    /// Target-independent snapshot already present in linear memory.
    LinearMemory(&'a [u8]),
}

impl RuntimeImageSource<'_> {
    /// Storage semantics represented by this source.
    #[must_use]
    pub const fn model(self) -> RuntimeImageModel {
        match self {
            Self::MappedFile(_) => RuntimeImageModel::MappedFile,
            Self::ExtractedFile(_) => RuntimeImageModel::ExtractedFile,
            Self::LinearMemory(_) => RuntimeImageModel::LinearMemory,
        }
    }

    /// Validate this source against the host profile and restore its evaluator.
    pub fn load_for(self, host: HostProfile) -> Result<Context, RuntimeImageError> {
        self.validate_model(host)?;
        if host.kind() == HostKind::Android {
            return Err(RuntimeImageError::RuntimeRootRequired { host: host.kind() });
        }

        let mut evaluator = self.restore()?;
        match host.kind() {
            HostKind::Desktop => {
                finalize_restored_runtime_image(&mut evaluator, RuntimeImageRole::Final, &[])
            }
            HostKind::Wasm => finalize_restored_runtime_image_at_root(
                &mut evaluator,
                RuntimeImageRole::Final,
                &[],
                Path::new(WASM_RUNTIME_ROOT),
            ),
            HostKind::Android => unreachable!("Android runtime root checked above"),
        }
        .map_err(RuntimeImageError::Finalize)?;
        Ok(evaluator)
    }

    /// Restore using a runtime resource root selected by a sandboxed host.
    ///
    /// Android adapters pass their extracted app-private `lisp/` and `etc/`
    /// parent here. Keeping it an argument avoids process-global environment
    /// mutation after the Activity and winit runtime have started threads.
    pub fn load_for_in_runtime_root(
        self,
        host: HostProfile,
        runtime_root: &Path,
    ) -> Result<Context, RuntimeImageError> {
        self.validate_model(host)?;
        let mut evaluator = self.restore()?;
        finalize_restored_runtime_image_at_root(
            &mut evaluator,
            RuntimeImageRole::Final,
            &[],
            runtime_root,
        )
        .map_err(RuntimeImageError::Finalize)?;
        Ok(evaluator)
    }

    fn validate_model(self, host: HostProfile) -> Result<(), RuntimeImageError> {
        let source_model = self.model();
        let host_model = host.runtime_images();
        if source_model != host_model {
            return Err(RuntimeImageError::ModelMismatch {
                host: host_model,
                source: source_model,
            });
        }
        Ok(())
    }

    fn restore(self) -> Result<Context, RuntimeImageError> {
        match self {
            Self::MappedFile(path) | Self::ExtractedFile(path) => {
                load_from_dump(path).map_err(RuntimeImageError::Load)
            }
            Self::LinearMemory(bytes) => {
                load_from_portable_snapshot(bytes).map_err(RuntimeImageError::Load)
            }
        }
    }
}

/// Failure to select or restore a host runtime image.
#[derive(Debug)]
pub enum RuntimeImageError {
    /// The adapter supplied storage with semantics different from its profile.
    ModelMismatch {
        /// Runtime-image model required by the host.
        host: RuntimeImageModel,
        /// Runtime-image model supplied by the adapter.
        source: RuntimeImageModel,
    },
    /// Sandboxed native hosts must provide the extracted runtime resource
    /// tree rather than relying on executable-relative discovery.
    RuntimeRootRequired {
        /// Host which omitted its runtime resource root.
        host: HostKind,
    },
    /// The selected image failed validation or reconstruction.
    Load(DumpError),
    /// Deserialization succeeded but the live Rust/host surface could not be
    /// rebuilt.
    Finalize(neovm_core::emacs_core::error::EvalError),
}

impl Display for RuntimeImageError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModelMismatch { host, source } => write!(
                formatter,
                "runtime image source {source:?} does not match host model {host:?}"
            ),
            Self::RuntimeRootRequired { host } => {
                write!(formatter, "{host:?} must provide an explicit runtime root")
            }
            Self::Load(error) => Display::fmt(error, formatter),
            Self::Finalize(error) => {
                write!(formatter, "runtime image finalization failed: {error}")
            }
        }
    }
}

impl std::error::Error for RuntimeImageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ModelMismatch { .. } => None,
            Self::RuntimeRootRequired { .. } => None,
            Self::Load(error) => Some(error),
            Self::Finalize(error) => Some(error),
        }
    }
}

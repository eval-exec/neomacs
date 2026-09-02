//! Atomic installation of packaged runtime images into native host storage.

use std::ffi::OsStr;
use std::fmt::{Display, Formatter};
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use neovm_core::emacs_core::load::RuntimeImageRole;
use neovm_core::emacs_core::pdump::{DumpError, dump_to_file, load_from_portable_snapshot};

use crate::content_id::ContentId;

use super::{PORTABLE_FINAL_RUNTIME_IMAGE_ASSET, PORTABLE_FINAL_RUNTIME_IMAGE_ID_ASSET};

static TEMPORARY_IMAGE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Whether this process installed an immutable image or found it ready.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeImageInstall {
    Installed,
    Reused,
}

/// Failure to provision a target-native image from a packaged portable seed.
#[derive(Debug)]
pub enum RuntimeImageProvisionError {
    /// Reading or atomically installing the image failed.
    Io(io::Error),
    /// The portable seed was invalid or native serialization failed.
    Image(DumpError),
    /// The portable seed's packaged content ID was not canonical SHA-256.
    InvalidPortableImageId,
    /// The portable seed bytes did not match their packaged content ID.
    PortableImageDigestMismatch { expected: String, actual: String },
}

impl Display for RuntimeImageProvisionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => Display::fmt(error, formatter),
            Self::Image(error) => Display::fmt(error, formatter),
            Self::InvalidPortableImageId => write!(
                formatter,
                "portable runtime image ID must be exactly 64 lowercase hexadecimal digits"
            ),
            Self::PortableImageDigestMismatch { expected, actual } => write!(
                formatter,
                "portable runtime image digest does not match its ID: expected {expected}, got {actual}"
            ),
        }
    }
}

impl std::error::Error for RuntimeImageProvisionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Image(error) => Some(error),
            Self::InvalidPortableImageId | Self::PortableImageDigestMismatch { .. } => None,
        }
    }
}

impl From<io::Error> for RuntimeImageProvisionError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<DumpError> for RuntimeImageProvisionError {
    fn from(error: DumpError) -> Self {
        Self::Image(error)
    }
}

/// Runtime image materialized as a real file in native host storage.
#[derive(Debug)]
pub struct ExtractedRuntimeImage {
    path: PathBuf,
    install: RuntimeImageInstall,
}

impl ExtractedRuntimeImage {
    /// Install the product's final runtime image under its schema/content
    /// fingerprinted name.
    ///
    /// The opener receives the exact packaged asset name. This keeps product
    /// adapters independent of Neovm's pdump naming and invalidation rules.
    pub fn prepare_final<R: Read>(
        directory: &Path,
        open_source: impl FnOnce(&str) -> io::Result<R>,
    ) -> io::Result<Self> {
        let file_name = RuntimeImageRole::Final.fingerprinted_image_file_name();
        Self::prepare(directory, &file_name, || open_source(&file_name))
    }

    /// Materialize a packaged target-independent final image as the native
    /// pdump required by this executable.
    ///
    /// Every call reads the small portable content ID. The large portable
    /// image is opened only when the native image selected by both executable
    /// schema and portable content is absent. The generated image is flushed
    /// and atomically published, so interrupted first-launch provisioning
    /// cannot poison later startups.
    pub fn prepare_final_from_portable<R: Read>(
        directory: &Path,
        mut open_source: impl FnMut(&str) -> io::Result<R>,
    ) -> Result<Self, RuntimeImageProvisionError> {
        let portable_id =
            read_portable_image_id(open_source(PORTABLE_FINAL_RUNTIME_IMAGE_ID_ASSET)?)?;
        let file_name = native_image_file_name(&portable_id);
        validate_file_name(&file_name)?;
        std::fs::create_dir_all(directory)?;
        let destination = directory.join(&file_name);
        if destination.is_file() {
            return Ok(Self {
                path: destination,
                install: RuntimeImageInstall::Reused,
            });
        }
        reject_non_file_destination(&destination)?;

        let mut source = open_source(PORTABLE_FINAL_RUNTIME_IMAGE_ASSET)?;
        let mut portable = Vec::new();
        source.read_to_end(&mut portable)?;
        let actual_id = ContentId::for_bytes(&portable);
        if actual_id != portable_id {
            return Err(RuntimeImageProvisionError::PortableImageDigestMismatch {
                expected: portable_id.to_string(),
                actual: actual_id.to_string(),
            });
        }
        let evaluator = load_from_portable_snapshot(&portable)?;
        let temporary_path = create_temporary_image_path(directory, &file_name)?;
        let install_result = (|| {
            dump_to_file(&evaluator, &temporary_path)?;
            File::open(&temporary_path)?.sync_all()?;
            publish_temporary_image(&temporary_path, &destination)
                .map_err(RuntimeImageProvisionError::Io)
        })();

        if temporary_path.exists() {
            let _ = std::fs::remove_file(&temporary_path);
        }
        let install = install_result?;
        Ok(Self {
            path: destination,
            install,
        })
    }

    /// Install an immutable packaged image into `directory` exactly once.
    ///
    /// `file_name` must be one path component. Callers should use a
    /// content/schema-fingerprinted name so a successfully installed file can
    /// be reused without reopening or comparing the packaged asset. The copy
    /// is flushed and atomically renamed; a concurrent installer that wins the
    /// destination race is reused.
    pub fn prepare<R: Read>(
        directory: &Path,
        file_name: &str,
        open_source: impl FnOnce() -> io::Result<R>,
    ) -> io::Result<Self> {
        validate_file_name(file_name)?;
        std::fs::create_dir_all(directory)?;
        let destination = directory.join(file_name);
        if destination.is_file() {
            return Ok(Self {
                path: destination,
                install: RuntimeImageInstall::Reused,
            });
        }
        reject_non_file_destination(&destination)?;

        let mut source = open_source()?;
        let (temporary_path, mut temporary) = create_temporary_image(directory, file_name)?;
        let install_result = (|| {
            io::copy(&mut source, &mut temporary)?;
            temporary.sync_all()?;
            drop(temporary);
            publish_temporary_image(&temporary_path, &destination)
        })();

        if temporary_path.exists() {
            let _ = std::fs::remove_file(&temporary_path);
        }
        let install = install_result?;
        Ok(Self {
            path: destination,
            install,
        })
    }

    /// Real path accepted by the native mmap runtime-image loader.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether this call installed or reused the immutable file.
    #[must_use]
    pub const fn install(&self) -> RuntimeImageInstall {
        self.install
    }
}

fn read_portable_image_id(source: impl Read) -> Result<ContentId, RuntimeImageProvisionError> {
    let mut text = String::new();
    source.take(128).read_to_string(&mut text)?;
    ContentId::parse(text.trim()).map_err(|_| RuntimeImageProvisionError::InvalidPortableImageId)
}

fn native_image_file_name(portable_id: &ContentId) -> String {
    let native = RuntimeImageRole::Final.fingerprinted_image_file_name();
    let stem = native.strip_suffix(".pdump").unwrap_or(&native);
    format!("{stem}-{portable_id}.pdump")
}

fn reject_non_file_destination(destination: &Path) -> io::Result<()> {
    if destination.exists() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "runtime image destination is not a regular file: {}",
                destination.display()
            ),
        ));
    }
    Ok(())
}

fn publish_temporary_image(
    temporary_path: &Path,
    destination: &Path,
) -> io::Result<RuntimeImageInstall> {
    match std::fs::rename(temporary_path, destination) {
        Ok(()) => Ok(RuntimeImageInstall::Installed),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::AlreadyExists | io::ErrorKind::PermissionDenied
            ) && destination.is_file() =>
        {
            Ok(RuntimeImageInstall::Reused)
        }
        Err(error) => Err(error),
    }
}

fn validate_file_name(file_name: &str) -> io::Result<()> {
    let path = Path::new(file_name);
    if file_name.is_empty()
        || path.file_name() != Some(OsStr::new(file_name))
        || path.components().count() != 1
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "runtime image asset name must be exactly one path component",
        ));
    }
    Ok(())
}

fn create_temporary_image(directory: &Path, file_name: &str) -> io::Result<(PathBuf, File)> {
    loop {
        let sequence = TEMPORARY_IMAGE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary_path = directory.join(format!(
            ".{file_name}.{}.{}.partial",
            std::process::id(),
            sequence
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => return Ok((temporary_path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

fn create_temporary_image_path(directory: &Path, file_name: &str) -> io::Result<PathBuf> {
    let (path, file) = create_temporary_image(directory, file_name)?;
    drop(file);
    Ok(path)
}

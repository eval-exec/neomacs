//! Atomic installation of packaged runtime images into native host storage.

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMPORARY_IMAGE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Whether this process installed an immutable image or found it ready.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeImageInstall {
    Installed,
    Reused,
}

/// Runtime image materialized as a real file in native host storage.
#[derive(Debug)]
pub struct ExtractedRuntimeImage {
    path: PathBuf,
    install: RuntimeImageInstall,
}

impl ExtractedRuntimeImage {
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
        if destination.exists() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "runtime image destination is not a regular file: {}",
                    destination.display()
                ),
            ));
        }

        let mut source = open_source()?;
        let (temporary_path, mut temporary) = create_temporary_image(directory, file_name)?;
        let install_result = (|| {
            io::copy(&mut source, &mut temporary)?;
            temporary.sync_all()?;
            drop(temporary);
            match std::fs::rename(&temporary_path, &destination) {
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

//! Authenticated target-independent runtime images.

use std::fmt::{Display, Formatter};

use neovm_core::emacs_core::eval::Context;

use crate::content_id::ContentId;
use crate::host::HostProfile;
use crate::runtime_resources::MountedRuntimeResources;

use super::{RuntimeImageError, RuntimeImageSource};

/// Portable image bytes whose packaged SHA-256 identity has been verified.
///
/// Browser startup accepts this type instead of a bare byte slice so restoring
/// an unauthenticated downloaded image is not representable at that boundary.
#[derive(Debug)]
pub struct AuthenticatedPortableRuntimeImage<'a> {
    bytes: &'a [u8],
}

impl<'a> AuthenticatedPortableRuntimeImage<'a> {
    /// Verify `bytes` against the canonical lowercase SHA-256 in `image_id`.
    pub fn from_assets(
        bytes: &'a [u8],
        image_id: &[u8],
    ) -> Result<Self, PortableRuntimeImageError> {
        let image_id = std::str::from_utf8(image_id)
            .ok()
            .and_then(|text| ContentId::parse(text.trim()).ok())
            .ok_or(PortableRuntimeImageError::InvalidImageId)?;
        let actual = ContentId::for_bytes(bytes);
        if actual != image_id {
            return Err(PortableRuntimeImageError::DigestMismatch {
                expected: image_id.to_string(),
                actual: actual.to_string(),
            });
        }
        Ok(Self { bytes })
    }

    /// Restore an authenticated browser image with its immutable resource
    /// tree installed before final-image hooks run.
    pub fn load_for_with_mounted_runtime_resources(
        self,
        host: HostProfile,
        resources: MountedRuntimeResources,
    ) -> Result<Context, RuntimeImageError> {
        RuntimeImageSource::LinearMemory(self.bytes)
            .load_authenticated_browser_image(host, resources)
    }
}

/// Failure to authenticate the two assets forming a portable runtime image.
#[derive(Debug, Eq, PartialEq)]
pub enum PortableRuntimeImageError {
    /// The ID was not exactly 64 lowercase hexadecimal digits.
    InvalidImageId,
    /// The downloaded image did not have the packaged identity.
    DigestMismatch { expected: String, actual: String },
}

impl Display for PortableRuntimeImageError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidImageId => write!(
                formatter,
                "portable runtime image ID must be exactly 64 lowercase hexadecimal digits"
            ),
            Self::DigestMismatch { expected, actual } => write!(
                formatter,
                "portable runtime image digest does not match its ID: expected {expected}, got {actual}"
            ),
        }
    }
}

impl std::error::Error for PortableRuntimeImageError {}

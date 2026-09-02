use std::num::NonZeroU32;

/// Whether a native surface currently has drawable physical dimensions.
///
/// Wgpu rejects zero-sized surface configurations. Keeping the drawable
/// dimensions non-zero by construction makes resize and presentation code
/// handle minimized, hidden, and not-yet-laid-out windows explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceExtent {
    /// The host has no drawable surface at this point in its lifecycle.
    Suspended,
    /// Both physical dimensions are non-zero and safe to configure.
    Drawable {
        width: NonZeroU32,
        height: NonZeroU32,
    },
}

impl SurfaceExtent {
    /// Classify physical dimensions reported by the host window system.
    pub const fn from_physical_size(width: u32, height: u32) -> Self {
        match (NonZeroU32::new(width), NonZeroU32::new(height)) {
            (Some(width), Some(height)) => Self::Drawable { width, height },
            _ => Self::Suspended,
        }
    }

    /// Drawable width, or `None` while the surface is suspended.
    pub const fn width(self) -> Option<u32> {
        match self {
            Self::Suspended => None,
            Self::Drawable { width, .. } => Some(width.get()),
        }
    }

    /// Drawable height, or `None` while the surface is suspended.
    pub const fn height(self) -> Option<u32> {
        match self {
            Self::Suspended => None,
            Self::Drawable { height, .. } => Some(height.get()),
        }
    }

    /// Drawable dimensions as ordinary integers for wgpu configuration.
    pub const fn dimensions(self) -> Option<(u32, u32)> {
        match self {
            Self::Suspended => None,
            Self::Drawable { width, height } => Some((width.get(), height.get())),
        }
    }
}

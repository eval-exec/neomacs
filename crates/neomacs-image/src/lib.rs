//! CPU-only image decoding. No window, GPU, or browser binding dependency.

#![allow(clippy::too_many_arguments)]

pub mod decoder;
mod image_sequence;
pub mod portable;
mod svg;
pub mod xbm;
pub mod xpm;

pub use image_sequence::ImageSequenceCache;
pub use svg::SvgResourceContext;

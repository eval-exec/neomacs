//! Font selection, metrics, and probing (moved from flat font_* files).

pub mod catalog;
pub mod font_match;
#[cfg(target_os = "linux")]
pub mod fontconfig;
pub(crate) mod frame_metrics;
mod instance;
pub mod metrics;
pub mod policy;
pub mod probe;
pub mod resolver;
pub(crate) mod selection;
pub mod sizing;
pub mod subpixel;

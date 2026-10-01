//! Native presentation ownership. Menu behavior and GPU painting stay elsewhere.
mod host;
pub(crate) mod pacing;
mod platform;
mod retirement;
mod surface;
pub(crate) use host::PopupHost;
pub(crate) use retirement::Retirements;
use surface::PopupSurface;

pub(crate) use crate::render_thread::PopupCommit;

/// Native input policy, independent of the content painted into a popup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PopupRole {
    Menu,
    Tooltip,
}

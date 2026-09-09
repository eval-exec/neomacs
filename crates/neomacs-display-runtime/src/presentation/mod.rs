//! Native presentation ownership. Menu behavior and GPU painting stay elsewhere.
mod host;
mod platform;
mod surface;
pub(crate) use host::PopupHost;
use surface::PopupSurface;

/// Native input policy, independent of the content painted into a popup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PopupRole {
    Menu,
    Tooltip,
}

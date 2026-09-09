//! Native presentation ownership. Menu behavior and GPU painting stay elsewhere.
mod host;
mod platform;
mod surface;
pub(crate) use host::PopupHost;
pub(crate) use surface::PopupSurface;

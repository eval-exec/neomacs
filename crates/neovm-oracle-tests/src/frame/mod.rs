//! Frame oracle parity tests.

mod configuration_semantics;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod implied_resize_default;
mod mouse_position_in_root_frame;
mod pixelwise_geometry_semantics;
mod window_strict_edge_semantics;
mod windows_min_size_semantics;

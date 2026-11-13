//! Stable `emacs_core` module roots moved from the core crate.

#[path = "display/chrome_dirty/mod.rs"]
pub mod chrome_dirty;
#[path = "display/display/mod.rs"]
pub mod display;
#[path = "display/display_host/mod.rs"]
pub mod display_host;
#[doc(hidden)]
#[path = "display/display_subrs/mod.rs"]
pub mod display_subrs;
#[path = "display/display_when/mod.rs"]
pub mod display_when;
#[path = "display/dispnew/mod.rs"]
pub mod dispnew;
#[path = "display/effect_profile/mod.rs"]
pub mod effect_profile;
#[path = "display/font/mod.rs"]
pub mod font;
#[path = "display/fontset/mod.rs"]
pub mod fontset;
#[path = "display/frame/mod.rs"]
pub mod frame;
#[path = "display/frame_vars/mod.rs"]
pub mod frame_vars;
#[path = "display/hscroll/mod.rs"]
pub mod hscroll;
#[path = "display/image/mod.rs"]
pub mod image;
#[path = "display/image_catalog/mod.rs"]
pub mod image_catalog;
#[path = "display/image_path/mod.rs"]
pub mod image_path;
#[path = "display/invisibility/mod.rs"]
pub mod invisibility;
#[path = "display/menu/mod.rs"]
pub mod menu;
#[path = "display/neo/mod.rs"]
pub mod neo;
#[path = "display/shader_surface/mod.rs"]
pub mod shader_surface;
#[path = "display/sound/mod.rs"]
pub mod sound;
#[path = "display/terminal/mod.rs"]
pub mod terminal;
#[path = "display/video/mod.rs"]
pub mod video;
#[path = "display/window_cmds/mod.rs"]
pub mod window_cmds;
#[path = "display/xdisp/mod.rs"]
pub mod xdisp;
#[path = "display/xfaces/mod.rs"]
pub mod xfaces;
#[path = "display/xwidget/mod.rs"]
pub mod xwidget;

#[cfg(test)]
#[path = "display/shader_surface/tests/runtime_test.rs"]
mod shader_surface_test;

#[cfg(test)]
#[path = "display/video/tests/runtime_test.rs"]
mod video_test;

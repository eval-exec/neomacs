//! Menu behavior and native presentation. No menu behavior lives in the renderer.

mod controller;
mod help;
mod interaction;
mod menu_bar;
pub(crate) use menu_bar::{HeadingAction, MenuHeading};
pub(crate) use controller::{MenuPresentation, MenuRequest};
pub(crate) use neomacs_app::frontend::menu::{MenuLifetime, MenuSession};

#[cfg(all(test, target_os = "linux"))]
mod native_test;

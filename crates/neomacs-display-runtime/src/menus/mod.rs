//! Menu behavior and native presentation. No menu behavior lives in the renderer.

mod controller;
mod help;
mod interaction;
mod layout;
mod menu_bar;
pub(crate) use menu_bar::{HeadingAction, MenuHeading};
mod session;
pub(crate) use controller::{MenuPresentation, MenuRequest};
pub(crate) use session::MenuSession;

#[cfg(all(test, target_os = "linux"))]
#[path = "tests/native_test.rs"]
mod native_test;

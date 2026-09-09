//! Menu behavior and native presentation. No menu behavior lives in the renderer.

mod interaction;
mod layout;
mod platform;
mod presentation;
mod session;
pub(crate) use presentation::{MenuPresentation, MenuRequest};
pub(crate) use session::MenuSession;

#[cfg(all(test, target_os = "linux"))]
mod native_test;

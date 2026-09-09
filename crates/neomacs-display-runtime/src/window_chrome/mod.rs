//! Native top-level appearance. Editor bars and popup ownership stay elsewhere.
mod controller;
mod platform;
pub(crate) use controller::WindowChromeController;

#[cfg(test)]
#[path = "controller_test.rs"]
mod controller_test;

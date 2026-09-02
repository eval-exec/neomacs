//! Host-neutral application contracts shared by Neomacs frontends.

#![forbid(unsafe_code)]

#[cfg(not(target_family = "wasm"))]
mod content_id;

pub mod evaluator_input;
pub mod frontend_event;
pub mod host;
pub mod initial_surface;
pub mod lifecycle;
pub mod presentation;
pub mod runtime_image;
pub mod runtime_resources;
pub mod session;
pub mod startup;

//! The Neomacs display session layer (DESIGN §6.1-§6.6, §6.12, §6.18-§6.19).
//!
//! This crate frames, schedules and keeps alive the byte stream between a
//! frontend and an evaluator. It knows nothing about displays: services are
//! negotiated by name and revision, and their message bodies are opaque bytes.
//!
//! The core is sans-I/O. [`session::FrontendEndpoint`] and
//! [`session::EvaluatorEndpoint`] consume bytes with `receive`, produce bytes
//! with `poll_transmit(now)` and advance timers with `tick(now)`; time is a
//! parameter supplied by a [`Clock`]. [`pump`] drives an endpoint over any
//! `Read + Write`.

pub mod handshake;
pub mod keepalive;
pub mod lane;
pub mod preface;
pub mod pump;
pub mod record;
pub mod relay;
pub mod session;
#[cfg(feature = "testing")]
pub mod testing;

mod frozen;

pub use keepalive::{Clock, SessionTime};

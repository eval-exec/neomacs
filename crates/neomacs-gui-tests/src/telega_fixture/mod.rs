//! Account-free Telega integration fixture.
//!
//! - [`protocol`] — Telega's byte-length framed plist wire protocol, typed
//!   requests, typed events, and explicit rejection of unmodeled requests.
//! - [`scenario`] — deterministic synthetic chats and locally generated PNG
//!   avatars (no personal or captured data).
//! - [`mock`] — the offline `telega-server` stand-in Telega launches through
//!   its real `telega-server-command`, including the structured log and
//!   control channel the GUI tests use as readiness checkpoints.
//!
//! The GUI integration tests live in `tests/telega_fixture_gui.rs` and the
//! editor-side scenario in `fixtures/telega-fixture-gui.el`; scope, isolation,
//! commands, and the separate retained-layout regression are documented in
//! `fixtures/telega-fixture.md`.  This fixture exercises the Telega
//! frontend/process/rendering integration, not TDLib or MTProto.
pub mod mock;
pub mod protocol;
pub mod scenario;

//! Continuity between immutable editor presentations.
//!
//! This module answers questions about what changed between two sealed
//! presentations — which panes persisted, entered or exited, and how far a
//! viewport moved — so the compositor can decide how to bridge them visually.
//!
//! Everything here derives facts by comparing presentations. It never asks the
//! producer what a change *meant*: geometry is diffed, not declared. Producers
//! supply provenance only where pixels genuinely cannot say (that a buffer
//! replacement was a navigation, for instance).

pub(in crate::render_thread) mod scroll;

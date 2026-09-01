//! Layout-internal row ranges used while publishing exact displayed positions.
//!
//! Pointer consumers use the presentation-scoped semantic hit index in
//! `neomacs-display-protocol`; this module no longer owns a parallel hit map.

/// One displayed row's vertical range and covered buffer positions.
#[derive(Clone, Debug)]
// All fields participate in construction and testable geometry contracts; the
// production snapshot consumer currently needs only `y_start`.
#[allow(dead_code)]
pub(crate) struct HitRow {
    pub y_start: f32,
    pub y_end: f32,
    pub charpos_start: i64,
    pub charpos_end: i64,
}

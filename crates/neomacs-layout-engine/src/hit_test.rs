//! Layout-internal row ranges used while publishing exact displayed positions.
//!
//! Pointer consumers use the presentation-scoped semantic hit index in
//! `neomacs-display-protocol`; this module no longer owns a parallel hit map.

/// One displayed row's vertical range and covered buffer positions.
///
/// The row-lifecycle collection that fills this is itself vestigial: the
/// production snapshot discards the whole `Vec<HitRow>` it builds and only
/// `render_plan.rs`'s ordering reads `y_start`. The other three fields are
/// written by production and read only by the geometry tests that pin what a
/// row covers. Unpicking the collection is a change to live row-lifecycle
/// plumbing across eight files, not a deletion, so the allow stays until that
/// is done deliberately.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct HitRow {
    pub y_start: f32,
    pub y_end: f32,
    pub charpos_start: i64,
    pub charpos_end: i64,
}

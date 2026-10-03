//! GNU posn object extents. Physical layout metrics remain separate.
//!
//! | Knob | Default | Values | Effect |
//! | --- | --- | --- | --- |
//! | `NEOMACS_POSN_OBJECT_EXTENT` | `off` | `off`; `on`/`1`/`true`/`yes` | Resolve canonical/presented TTY TEXT object dimensions from retained current-matrix provenance; cold leaves may read accepted full-frame-width numeric partitions with proved reuse sources. Unsupported accepted horizontal/shifted rows are Undrawn; named areas retain baseline. |

use super::WindowDisplaySnapshot;
use neomacs_display_protocol::glyph_matrix::GlyphArea;
pub use neomacs_display_protocol::posn_object_extent::PosnObjectExtent;
use std::ffi::OsStr;

/// Immutable process policy, safely published by OnceLock. Independent
/// mutators share only this numeric enum; there is no Lisp state or TLS cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PosnObjectExtentMode {
    Off,
    On,
}

impl PosnObjectExtentMode {
    pub fn parse(setting: Option<&OsStr>) -> Self {
        match setting
            .and_then(OsStr::to_str)
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("on" | "1" | "true" | "yes") => Self::On,
            _ => Self::Off,
        }
    }

    pub const fn enabled(self) -> bool {
        matches!(self, Self::On)
    }
}

#[inline]
pub fn posn_object_extent_mode() -> PosnObjectExtentMode {
    #[cfg(test)]
    if let Some(mode) = TEST_OVERRIDE.with(std::cell::Cell::get) {
        return mode;
    }
    static MODE: std::sync::OnceLock<PosnObjectExtentMode> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| {
        PosnObjectExtentMode::parse(std::env::var_os("NEOMACS_POSN_OBJECT_EXTENT").as_deref())
    })
}

#[cfg(test)]
thread_local! {
    /// A test-local numeric selector override, never compiled into production.
    /// Each test mutator owns its thread's scoped enum; no Lisp values, runtime
    /// caches, or cross-mutator mutable state are retained.
    static TEST_OVERRIDE: std::cell::Cell<Option<PosnObjectExtentMode>> = const {
        std::cell::Cell::new(None)
    };
}

#[cfg(test)]
pub(crate) fn force_posn_object_extent_for_test(
    mode: Option<PosnObjectExtentMode>,
) -> Option<PosnObjectExtentMode> {
    TEST_OVERRIDE.with(|slot| slot.replace(mode))
}

/// Read accepted matrix state at the live walk's row/index. Snapshot freshness
/// does not apply: GNU intentionally reads the current matrix even while the
/// iterator sees different live text. Missing accepted rows are Undrawn.
#[inline]
pub fn retained_posn_extent(
    retained: Option<&WindowDisplaySnapshot>,
    row: i64,
    column: i64,
    area: GlyphArea,
) -> PosnObjectExtent {
    retained
        .and_then(|snapshot| snapshot.posn_matrix.as_deref())
        .map_or(PosnObjectExtent::Undrawn, |matrix| {
            matrix.at(row, column, area)
        })
}

#[cfg(test)]
#[path = "tests/posn_object_extent_test.rs"]
mod tests;

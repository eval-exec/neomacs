//! Bounded, value-free coverage for overlays whose only display effect is a
//! face. Keep both generations' coverage: deleting or moving hl-line must
//! repaint its old row as well as its new row.

use neovm_core::buffer::Buffer;
use neovm_core::emacs_core::Value;

/// The standard `(face font-lock-face)` fallback only influences face
/// selection. Other aliases could make a face overlay supply a structural
/// property, so retain the full walk for those. Inspect only this bounded
/// shape; malformed or cyclic user lists cannot make the check unbounded.
pub(crate) fn face_overlay_aliases_supported(aliases: Option<Value>) -> bool {
    let Some(aliases) = aliases.filter(|value| !value.is_nil()) else {
        return true;
    };
    if !aliases.is_cons() || !aliases.cons_cdr().is_nil() {
        return false;
    }
    let entry = aliases.cons_car();
    if !entry.is_cons() || entry.cons_car().as_bare_symbol_name().as_deref() != Some("face") {
        return false;
    }
    let rest = entry.cons_cdr();
    rest.is_cons()
        && rest.cons_car().as_bare_symbol_name().as_deref() == Some("font-lock-face")
        && rest.cons_cdr().is_nil()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FaceOverlayCoverage {
    #[default]
    Unsupported,
    Empty,
    Span {
        start: i64,
        end: i64,
    },
}

impl FaceOverlayCoverage {
    pub(crate) fn capture(buffer: &Buffer) -> Self {
        let overlays = buffer.overlays();
        // This optimization targets a few moving highlights. Do not add an
        // unbounded scan to each retained-key collection in diagnostic buffers.
        if overlays.len() > 32 {
            return Self::Unsupported;
        }
        let mut coverage = Self::Empty;
        for overlay in overlays.overlays_in_gnu_lists_order() {
            let Some(mut plist) = overlays.overlay_plist(overlay) else {
                return Self::Unsupported;
            };
            let mut pairs = 0;
            while !plist.is_nil() {
                if pairs == 16 || !plist.is_cons() {
                    return Self::Unsupported;
                }
                let key = plist.cons_car();
                let rest = plist.cons_cdr();
                if !rest.is_cons()
                    || !matches!(
                        key.as_bare_symbol_name().as_deref(),
                        Some("face" | "priority" | "window")
                    )
                {
                    // In particular: no category indirection, display strings,
                    // invisibility, prefixes or cursor decorations. Unknown
                    // keys conservatively keep the canonical full walk.
                    return Self::Unsupported;
                }
                plist = rest.cons_cdr();
                pairs += 1;
            }
            let (Some(start), Some(end)) = (
                overlays.overlay_start_emacs_byte_pos(overlay),
                overlays.overlay_end_emacs_byte_pos(overlay),
            ) else {
                return Self::Unsupported;
            };
            let span = Self::Span {
                start: buffer.emacs_byte_pos_to_char_pos_clamped(start).get() as i64,
                end: buffer.emacs_byte_pos_to_char_pos_clamped(end).get() as i64,
            };
            coverage = coverage.union(span);
        }
        coverage
    }

    fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unsupported, _) | (_, Self::Unsupported) => Self::Unsupported,
            (Self::Empty, other) | (other, Self::Empty) => other,
            (Self::Span { start: a, end: b }, Self::Span { start: c, end: d }) => Self::Span {
                start: a.min(c),
                end: b.max(d),
            },
        }
    }

    pub(crate) fn damage(self, current: Self) -> Option<(i64, i64)> {
        match self.union(current) {
            Self::Span { start, end } => Some((start, end)),
            _ => None,
        }
    }
}

//! Bounded, value-free coverage for overlays whose only display effect is a
//! face. Keep both generations' coverage: deleting or moving hl-line must
//! repaint its old row as well as its new row.
//!
//! Coverage is a bounded list of `(span, plist fingerprint)` entries, not one
//! bounding box: a `hl-line` move must not rewalk from an unrelated `region`
//! overlay on the first line, and a change outside the viewport must decline
//! the replay instead of falling back to a full relayout (issue #609).

use neovm_core::buffer::Buffer;
use neovm_core::emacs_core::Value;
use neovm_core::emacs_core::value::{ValueKind, VecLikeType};

/// The standard face-overlay plists are tiny; this bound keeps capture and
/// the multiset diff linear on pathological buffers.
const MAX_FACE_OVERLAYS: usize = 32;
const MAX_PLIST_PAIRS: usize = 16;
const MAX_VALUE_NODES: usize = 64;
const MAX_STRING_BYTES: usize = 64;
const MAX_ALIAS_ENTRIES: usize = 16;
const MAX_ALIASES_PER_ENTRY: usize = 16;

/// Distinct value-kind tags keep `t`, fixnum `1`, the symbol `nil` and an
/// opaque pointer from sharing a byte stream (`overlay_priority` treats `t`
/// as 0, so a `1` -> `t` change must be visible).
const TAG_NIL: u64 = 0;
const TAG_T: u64 = 1;
const TAG_FIXNUM: u64 = 2;
const TAG_SYMBOL: u64 = 3;
const TAG_UNBOUND: u64 = 4;
const TAG_STRING: u64 = 5;
const TAG_CONS: u64 = 6;
const TAG_OPAQUE: u64 = 7;
const TAG_PAIR_KEY: u64 = 8;
const TAG_PAIR_VALUE: u64 = 9;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The standard `(face font-lock-face)` fallback only influences face
/// selection. An entry whose *own* property is not `face` may not reach
/// `face` through its aliases: `((display face))` would let a face-only
/// overlay supply a structural `display` value. Alias chains are followed to
/// the same conclusion. Malformed or cyclic user lists stay bounded through
/// the entry/alias caps and a visited set.
pub(crate) fn face_overlay_aliases_supported(aliases: Option<Value>) -> bool {
    let Some(aliases) = aliases.filter(|value| !value.is_nil()) else {
        return true;
    };
    let empty = (Value::NIL, [Value::NIL; MAX_ALIASES_PER_ENTRY], 0usize);
    let mut entries = [empty; MAX_ALIAS_ENTRIES];
    let mut count = 0;
    let mut tail = aliases;
    while !tail.is_nil() {
        if count == MAX_ALIAS_ENTRIES || !tail.is_cons() {
            return false;
        }
        let entry = tail.cons_car();
        if !entry.is_cons() {
            return false;
        }
        let property = entry.cons_car();
        if property.as_bare_symbol_name().is_none() {
            return false;
        }
        let mut aliases_of = [Value::NIL; MAX_ALIASES_PER_ENTRY];
        let mut alias_count = 0;
        let mut rest = entry.cons_cdr();
        while !rest.is_nil() {
            if alias_count == MAX_ALIASES_PER_ENTRY || !rest.is_cons() {
                return false;
            }
            let alias = rest.cons_car();
            if alias.as_bare_symbol_name().is_none() {
                return false;
            }
            aliases_of[alias_count] = alias;
            alias_count += 1;
            rest = rest.cons_cdr();
        }
        entries[count] = (property, aliases_of, alias_count);
        count += 1;
        tail = tail.cons_cdr();
    }

    for (property, aliases_of, alias_count) in entries.into_iter().take(count) {
        let name = property.as_bare_symbol_name().expect("validated above");
        if name == "face" {
            continue;
        }
        let mut visited = [false; MAX_ALIAS_ENTRIES];
        if property_reaches_face(
            name,
            &entries[..count],
            &aliases_of,
            alias_count,
            &mut visited,
        ) {
            return false;
        }
    }
    true
}

fn property_reaches_face(
    _name: &str,
    entries: &[(Value, [Value; MAX_ALIASES_PER_ENTRY], usize)],
    aliases: &[Value; MAX_ALIASES_PER_ENTRY],
    alias_count: usize,
    visited: &mut [bool; MAX_ALIAS_ENTRIES],
) -> bool {
    for alias in aliases.iter().take(alias_count) {
        let Some(alias) = alias.as_bare_symbol_name() else {
            continue;
        };
        if alias == "face" {
            return true;
        }
        let Some((index, (_, next, next_count))) =
            entries.iter().enumerate().find(|(_, (property, _, _))| {
                property
                    .as_bare_symbol_name()
                    .is_some_and(|property| property == alias)
            })
        else {
            continue;
        };
        if visited[index] {
            continue;
        }
        visited[index] = true;
        if property_reaches_face(alias, entries, next, *next_count, visited) {
            return true;
        }
    }
    false
}

fn mix_bytes(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(FNV_PRIME);
    }
}

fn mix_value(hash: &mut u64, value: u64) {
    mix_bytes(hash, &value.to_le_bytes());
}

/// A cheap, value-free fingerprint of one face overlay's plist content.
///
/// `overlay-put` mutates the plist cells in place, so identity bits alone
/// cannot distinguish a changed face. Walk a bounded number of value nodes and
/// hash symbols by name, integers by value and strings by (bounded) bytes;
/// anything else conservatively declines the replay.
fn overlay_plist_fingerprint(plist: Value) -> Option<u64> {
    let mut hash = FNV_OFFSET;
    let mut pairs = 0;
    let mut tail = plist;
    while !tail.is_nil() {
        if pairs == MAX_PLIST_PAIRS || !tail.is_cons() {
            return None;
        }
        let key = tail.cons_car();
        let rest = tail.cons_cdr();
        if !rest.is_cons() {
            return None;
        }
        let name = key.as_bare_symbol_name()?;
        mix_value(&mut hash, TAG_PAIR_KEY);
        mix_bytes(&mut hash, name.as_bytes());
        mix_value(&mut hash, TAG_PAIR_VALUE);
        fingerprint_value(rest.cons_car(), &mut hash)?;
        tail = rest.cons_cdr();
        pairs += 1;
    }
    Some(hash)
}

fn fingerprint_value(root: Value, hash: &mut u64) -> Option<()> {
    let mut stack = [Value::NIL; MAX_VALUE_NODES];
    let mut top: usize = 0;
    stack[top] = root;
    top += 1;
    let mut nodes = 0;
    while top > 0 {
        top -= 1;
        let value = stack[top];
        nodes += 1;
        if nodes > MAX_VALUE_NODES {
            return None;
        }
        match value.kind() {
            ValueKind::Nil => mix_value(hash, TAG_NIL),
            ValueKind::T => mix_value(hash, TAG_T),
            ValueKind::Fixnum(number) => {
                mix_value(hash, TAG_FIXNUM);
                mix_value(hash, number as u64);
            }
            ValueKind::Symbol(_) => {
                let name = value.as_bare_symbol_name()?;
                mix_value(hash, TAG_SYMBOL);
                mix_bytes(hash, name.as_bytes());
            }
            ValueKind::Unbound => mix_value(hash, TAG_UNBOUND),
            ValueKind::String => {
                let string = value.as_lisp_string()?;
                let bytes = string.as_bytes();
                // Hashing a prefix would make two long strings with equal
                // prefixes indistinguishable, losing a real face change.
                if bytes.len() > MAX_STRING_BYTES {
                    return None;
                }
                mix_value(hash, TAG_STRING);
                mix_bytes(hash, bytes);
            }
            ValueKind::Cons => {
                mix_value(hash, TAG_CONS);
                if top + 2 > MAX_VALUE_NODES {
                    return None;
                }
                stack[top] = value.cons_car();
                top += 1;
                stack[top] = value.cons_cdr();
                top += 1;
            }
            ValueKind::Float => {
                let number = value.as_float()?;
                mix_value(hash, TAG_OPAQUE);
                mix_value(hash, number.to_bits());
            }
            // A window's identity is its meaning: the overlay is scoped to
            // that window and the object itself carries no face state.
            ValueKind::Veclike(VecLikeType::Window) => {
                mix_value(hash, TAG_OPAQUE);
                mix_value(hash, value.bits() as u64);
            }
            // Any other opaque value can be mutated in place (font-specs via
            // `font-put`, vectors, records, hash tables, markers, …), so its
            // identity bits cannot prove the rendered face is unchanged.
            // Keep the canonical full walk instead.
            _ => return None,
        }
    }
    Some(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FaceOverlayEntry {
    /// Creation serial: fully equal overlays are ordered by it, so a deleted
    /// and recreated overlay is a change even at the same span and plist.
    serial: u64,
    start: i64,
    end: i64,
    fingerprint: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FaceOverlayCoverage {
    #[default]
    Unsupported,
    Entries {
        count: u8,
        entries: [FaceOverlayEntry; MAX_FACE_OVERLAYS],
    },
}

impl FaceOverlayCoverage {
    pub(crate) fn capture(buffer: &Buffer) -> Self {
        let overlays = buffer.overlays();
        // This optimization targets a few moving highlights. Do not add an
        // unbounded scan to each retained-key collection in diagnostic buffers.
        if overlays.len() > MAX_FACE_OVERLAYS {
            return Self::Unsupported;
        }
        let mut entries = [FaceOverlayEntry {
            serial: 0,
            start: 0,
            end: 0,
            fingerprint: 0,
        }; MAX_FACE_OVERLAYS];
        let mut count = 0usize;
        for overlay in overlays.overlays_in_gnu_lists_order() {
            let Some(mut plist) = overlays.overlay_plist(overlay) else {
                return Self::Unsupported;
            };
            let original_plist = plist;
            let mut pairs = 0;
            while !plist.is_nil() {
                if pairs == MAX_PLIST_PAIRS || !plist.is_cons() {
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
            let Some(fingerprint) = overlay_plist_fingerprint(original_plist) else {
                return Self::Unsupported;
            };
            let Some(serial) = overlays.overlay_creation_serial(overlay) else {
                return Self::Unsupported;
            };
            entries[count] = FaceOverlayEntry {
                serial,
                start: buffer.emacs_byte_pos_to_char_pos_clamped(start).get() as i64,
                end: buffer.emacs_byte_pos_to_char_pos_clamped(end).get() as i64,
                fingerprint,
            };
            count += 1;
        }
        Self::Entries {
            count: count as u8,
            entries,
        }
    }

    /// Char range to rewalk, localized to the entries whose span or plist
    /// fingerprint differs between the retained and current generations.
    ///
    /// `viewport_start` is the first character of the body rows: damage above
    /// it is clamped away, and a change that ends before it declines the
    /// replay so the caller can keep its canonical layout.
    pub(crate) fn damage(self, current: Self, viewport_start: i64) -> Option<(i64, i64)> {
        let Self::Entries {
            count: previous_count,
            entries: previous,
        } = self
        else {
            return None;
        };
        let Self::Entries {
            count: current_count,
            entries: current,
        } = current
        else {
            return None;
        };
        let mut matched = [false; MAX_FACE_OVERLAYS];
        let mut damage: Option<(i64, i64)> = None;
        let mut add = |span: (i64, i64)| {
            damage = Some(match damage {
                Some((start, end)) => (start.min(span.0), end.max(span.1)),
                None => span,
            });
        };
        for index in 0..current_count as usize {
            let entry = current[index];
            let mut found = false;
            for (slot, retained) in matched.iter_mut().zip(previous.iter()) {
                if !*slot && *retained == entry {
                    *slot = true;
                    found = true;
                    break;
                }
            }
            if !found {
                add((entry.start, entry.end));
            }
        }
        for (index, retained) in previous.iter().enumerate().take(previous_count as usize) {
            if !matched[index] {
                add((retained.start, retained.end));
            }
        }
        let (start, end) = damage?;
        if end < viewport_start {
            return None;
        }
        Some((start.max(viewport_start), end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coverage(entries: &[(u64, i64, i64, u64)]) -> FaceOverlayCoverage {
        let mut slots = [FaceOverlayEntry {
            serial: 0,
            start: 0,
            end: 0,
            fingerprint: 0,
        }; MAX_FACE_OVERLAYS];
        for (slot, (serial, start, end, fingerprint)) in slots.iter_mut().zip(entries) {
            *slot = FaceOverlayEntry {
                serial: *serial,
                start: *start,
                end: *end,
                fingerprint: *fingerprint,
            };
        }
        FaceOverlayCoverage::Entries {
            count: entries.len() as u8,
            entries: slots,
        }
    }

    #[test]
    fn damage_covers_only_the_entry_that_changed() {
        let previous = coverage(&[(1, 1, 2, 0x11), (2, 50, 51, 0x22)]);
        let current = coverage(&[(1, 1, 2, 0x11), (2, 60, 61, 0x22)]);
        assert_eq!(
            previous.damage(current, 1),
            Some((50, 61)),
            "the unchanged far-away overlay must not widen the damage"
        );
    }

    #[test]
    fn damage_is_clamped_to_the_body_start() {
        let previous = coverage(&[(2, 50, 51, 0x22)]);
        let current = coverage(&[(2, 60, 61, 0x22)]);
        assert_eq!(previous.damage(current, 55), Some((55, 61)));
    }

    #[test]
    fn a_change_entirely_above_the_viewport_declines() {
        let previous = coverage(&[(1, 1, 2, 0x11)]);
        let current = coverage(&[(2, 2, 3, 0x11)]);
        assert_eq!(previous.damage(current, 10), None);
    }

    #[test]
    fn recreated_overlay_at_the_same_span_is_damage() {
        // Same span and plist, new creation serial: the winning face at an
        // overlap can change, so the entry is not reusable.
        let previous = coverage(&[(1, 10, 20, 0x33)]);
        let current = coverage(&[(2, 10, 20, 0x33)]);
        assert_eq!(previous.damage(current, 1), Some((10, 20)));
    }

    #[test]
    fn identical_coverage_declines() {
        let coverage = coverage(&[(1, 1, 2, 0x11), (2, 50, 51, 0x22)]);
        assert_eq!(coverage.damage(coverage, 1), None);
    }
}

//! Byte-splicing of computed values into the source text.
//!
//! The plan records where each animated attribute lives in the original
//! document bytes; applying an evaluation is replacing those ranges (or
//! inserting missing attributes) and nothing more. This is the same
//! technique the static pipeline already uses for face colors and root
//! dimensions (`crate::svg`), so the patched text flows through the
//! unchanged downstream path — one usvg parse, one raster — per sample.

use super::eval::AttributeOverride;

/// Apply `overrides` (rule indices into `plan`) to `data`, highest offset
/// first so splices never invalidate a later site's range.
///
/// Overrides are deduplicated by site before splicing: evaluation produces
/// one override per rule, and two rules resolved to the same attribute site
/// (already collapsed at compile time) cannot both write it.
pub(crate) fn apply(
    plan: &super::plan::AnimationPlan,
    data: &[u8],
    overrides: &[AttributeOverride],
) -> Option<Vec<u8>> {
    if overrides.is_empty() {
        return Some(data.to_vec());
    }

    enum Edit {
        Replace {
            range: std::ops::Range<usize>,
            value: String,
        },
        Insert {
            position: usize,
            attribute: String,
            value: String,
        },
    }
    impl Edit {
        fn position(&self) -> usize {
            match self {
                Self::Replace { range, .. } => range.start,
                Self::Insert { position, .. } => *position,
            }
        }
    }

    let mut edits: Vec<Edit> = overrides
        .iter()
        .map(|override_value| {
            let site = &plan.rules[override_value.rule].site;
            match &site.value_range {
                Some(range) => Edit::Replace {
                    range: range.clone(),
                    value: override_value.value.clone(),
                },
                None => Edit::Insert {
                    position: site.insert_pos,
                    attribute: site.attribute.clone(),
                    value: override_value.value.clone(),
                },
            }
        })
        .collect();
    // Highest offset first: an insertion and a replacement at the same
    // element must not see each other's shifted positions. Sites are
    // distinct by construction (compile deduplicates a shared range), so
    // ordering by position alone is stable for the splice loop.
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.position()));

    let mut patched = data.to_vec();
    for edit in edits {
        match edit {
            Edit::Replace { range, value } => {
                patched.splice(range, value.bytes());
            }
            Edit::Insert {
                position,
                attribute,
                value,
            } => {
                patched.splice(
                    position..position,
                    format!(" {attribute}=\"{value}\"").bytes(),
                );
            }
        }
    }
    Some(patched)
}

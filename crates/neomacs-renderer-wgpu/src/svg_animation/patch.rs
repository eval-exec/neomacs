//! Byte-splicing of computed values into the source text.
//!
//! The plan records where each animated attribute lives in the original
//! document bytes; applying an evaluation is replacing those ranges (or
//! inserting missing attributes) and nothing more. This is the same
//! technique the static pipeline already uses for face colors and root
//! dimensions (`crate::svg`), so the patched text flows through the
//! unchanged downstream path — one usvg parse, one raster — per sample.

use super::eval::AttributeOverride;

/// XML syntax produced from a decoded attribute value. Keeping this type
/// separate from evaluation's strings makes every splice pass through the
/// escaping boundary exactly once, regardless of the source's quote style.
struct XmlAttributeValue(String);

impl XmlAttributeValue {
    fn from_decoded(value: &str) -> Self {
        let mut escaped = String::with_capacity(value.len());
        for character in value.chars() {
            match character {
                '&' => escaped.push_str("&amp;"),
                '<' => escaped.push_str("&lt;"),
                '>' => escaped.push_str("&gt;"),
                '"' => escaped.push_str("&quot;"),
                '\'' => escaped.push_str("&apos;"),
                // Literal XML attribute whitespace normalizes to spaces;
                // character references preserve the evaluated value.
                '\t' => escaped.push_str("&#x9;"),
                '\n' => escaped.push_str("&#xA;"),
                '\r' => escaped.push_str("&#xD;"),
                other => escaped.push(other),
            }
        }
        Self(escaped)
    }
}

/// Apply `overrides` (rule indices into `plan`) to `data`, highest offset
/// first so splices never invalidate a later site's range.
///
/// Evaluation resolves competing animations before splicing and produces
/// at most one override for each attribute site.
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
            value: XmlAttributeValue,
        },
        Insert {
            position: usize,
            attribute: String,
            value: XmlAttributeValue,
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
                    value: XmlAttributeValue::from_decoded(&override_value.value),
                },
                None => Edit::Insert {
                    position: site.insert_pos,
                    attribute: site.attribute.clone(),
                    value: XmlAttributeValue::from_decoded(&override_value.value),
                },
            }
        })
        .collect();
    // Highest offset first: an insertion and a replacement at the same
    // element must not see each other's shifted positions. Evaluation
    // selects distinct sites, so ordering by position alone is stable
    // for the splice loop.
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.position()));

    let mut patched = data.to_vec();
    for edit in edits {
        match edit {
            Edit::Replace { range, value } => {
                patched.splice(range, value.0.bytes());
            }
            Edit::Insert {
                position,
                attribute,
                value,
            } => {
                patched.splice(
                    position..position,
                    format!(" {attribute}=\"{}\"", value.0).bytes(),
                );
            }
        }
    }
    Some(patched)
}

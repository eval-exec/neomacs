//! Evaluation of a compiled plan at a document time.
//!
//! Pure in both directions: no clocks (callers bring document time), no
//! I/O, no mutation of the plan. The output is exactly the attribute values
//! the document shows at that instant, as source-text fragments ready for
//! [`super::patch`] — which keeps this module unit-testable as plain
//! `(plan, time) → values` data in, data out.

use std::time::Duration;

use super::plan::{AnimatedValue, AnimationPlan, AnimationRule, CalcMode, Repeat};

/// One computed attribute value and the rule it belongs to.
///
/// The rule travels as an index into [`AnimationPlan::rules`], not a
/// reference: evaluation results stay plain owned data, sendable to the
/// decode pool without lifetime plumbing, and the index lookup costs what
/// the borrow would have folded in anyway.
#[derive(Clone, Debug)]
pub(crate) struct AttributeOverride {
    pub(crate) rule: usize,
    pub(crate) value: String,
}

/// The values the document shows at `doc_time`.
///
/// Rules that have not begun, or whose active duration has passed without
/// `fill="freeze"`, contribute nothing — the base attribute value in the
/// source text is what shows, exactly as a renderer that ignores animations
/// would draw it.
pub(crate) fn evaluate(plan: &AnimationPlan, doc_time: Duration) -> Vec<AttributeOverride> {
    let mut overrides = Vec::new();
    for (index, rule) in plan.rules.iter().enumerate() {
        if let Some(value) = evaluate_rule(rule, doc_time) {
            overrides.push(AttributeOverride {
                rule: index,
                value,
            });
        }
    }
    overrides
}

fn evaluate_rule(rule: &AnimationRule, doc_time: Duration) -> Option<String> {
    let timeline = &rule.timeline;
    if doc_time < timeline.begin {
        return None;
    }
    let local = doc_time - timeline.begin;
    let active = match timeline.repeat {
        Repeat::Indefinite => None,
        Repeat::Count(count) => timeline.dur.checked_mul(count),
    };
    if let Some(active) = active
        && local >= active
    {
        // Past the end: frozen at the last value, or back to the base.
        return if timeline.freeze {
            Some(serialize(timeline.values.last()?, rule))
        } else {
            None
        };
    }
    if timeline.values.is_empty() {
        return None;
    }

    let dur = timeline.dur.as_secs_f64();
    let cycle = if dur > 0.0 {
        (local.as_secs_f64() % dur) / dur
    } else {
        0.0
    };
    let value = match timeline.calc {
        CalcMode::Discrete => discrete_value(timeline, cycle),
        CalcMode::Linear => interpolate_value(timeline, cycle)?,
    };
    Some(serialize(&value, rule))
}

/// The keyframe holding at cycle fraction `cycle` under discrete stepping.
fn discrete_value(timeline: &super::plan::Timeline, cycle: f64) -> AnimatedValue {
    let values = &timeline.values;
    debug_assert!(!values.is_empty());
    let index = match &timeline.key_times {
        Some(times) => times
            .iter()
            .rposition(|time| *time <= cycle)
            .unwrap_or(0)
            .min(values.len() - 1),
        None => {
            let count = values.len();
            ((cycle * count as f64) as usize).min(count - 1)
        }
    };
    values[index].clone()
}

/// The interpolated value at cycle fraction `cycle`.
fn interpolate_value(timeline: &super::plan::Timeline, cycle: f64) -> Option<AnimatedValue> {
    let values = &timeline.values;
    if values.len() < 2 {
        return values.first().cloned();
    }
    let (start_fraction, end_fraction, index) = match &timeline.key_times {
        Some(times) => {
            // The first window whose far edge is strictly past `cycle`; a
            // cycle fraction that reached 1.0 through rounding falls to the
            // last window rather than past every segment.
            let mut segment = times.windows(2).enumerate().find(|(_, window)| {
                cycle < window[1] || (window[1] - window[0]).abs() < f64::EPSILON
            });
            if segment.is_none() {
                segment = Some((times.len() - 2, &times[times.len() - 2..]));
            }
            let (index, window) = segment?;
            (window[0], window[1], index)
        }
        None => {
            // N keyframes span N-1 uniform segments; a value at its own
            // key time is the start of the segment leaving it, which is
            // what SMIL's implicit uniform keyTimes define.
            let segments = values.len() - 1;
            let segment = ((cycle * segments as f64) as usize).min(segments - 1);
            let start = segment as f64 / segments as f64;
            let end = (segment + 1) as f64 / segments as f64;
            (start, end, segment)
        }
    };
    let span = end_fraction - start_fraction;
    let progress = if span <= 0.0 {
        0.0
    } else {
        ((cycle - start_fraction) / span).clamp(0.0, 1.0)
    };

    let from = &values[index];
    let to = &values.get(index + 1).or_else(|| values.last())?;
    match (from, to) {
        (AnimatedValue::Numbers(from), AnimatedValue::Numbers(to)) => {
            let numbers = from
                .iter()
                .zip(to.iter())
                .map(|(from, to)| from + (to - from) * progress)
                .collect();
            Some(AnimatedValue::Numbers(numbers))
        }
        (AnimatedValue::Color(from), AnimatedValue::Color(to)) => {
            let channels = from
                .iter()
                .zip(to.iter())
                .map(|(from, to)| from + (to - from) * progress)
                .collect::<Vec<_>>();
            Some(AnimatedValue::Color([channels[0], channels[1], channels[2]]))
        }
        // Mixed representations cannot blend; hold the segment's start.
        _ => Some(from.clone()),
    }
}

/// Render one value as source text for its attribute.
///
/// A value carrying a double quote would break out of the attribute the
/// patch writes it into. Supported value types cannot produce one; an
/// author-supplied opaque value can, so quotes are stripped rather than
/// trusted.
fn serialize(value: &AnimatedValue, rule: &AnimationRule) -> String {
    let rendered = match (&rule.timeline.transform, value) {
        (Some(kind), AnimatedValue::Numbers(numbers)) => {
            let arguments = numbers
                .iter()
                .map(|number| format_number(*number))
                .collect::<Vec<_>>()
                .join(" ");
            format!("{}({arguments})", kind.function())
        }
        (_, AnimatedValue::Numbers(numbers)) => numbers
            .iter()
            .map(|number| format_number(*number))
            .collect::<Vec<_>>()
            .join(" "),
        (_, AnimatedValue::Color([r, g, b])) => format!(
            "#{:02x}{:02x}{:02x}",
            round_channel(*r),
            round_channel(*g),
            round_channel(*b)
        ),
        (_, AnimatedValue::Opaque(text)) => text.clone(),
    };
    rendered.replace('"', "")
}

fn round_channel(channel: f64) -> u8 {
    channel.round().clamp(0.0, 255.0) as u8
}

/// Compact decimal form: integers without a fraction, everything else as
/// Rust's shortest round-trip display.
fn format_number(number: f64) -> String {
    if number.fract() == 0.0 && number.abs() < 1e15 {
        format!("{}", number as i64)
    } else {
        format!("{number}")
    }
}

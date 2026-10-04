//! One-time compilation of a document's animation elements into a plan.
//!
//! The plan is pure data: byte sites in the source text, parsed value
//! lists, timing. Every later stage (evaluation, patching, sampling) reads
//! it and nothing re-parses the document — parsing is the expensive part of
//! the SVG pipeline, and a looping animation must not repeat it per frame.
//!
//! The supported subset is the SMIL that real-world icons and spinners use:
//! `<animate>` and `<animateTransform>` (and `<set>`) with `from`/`to`,
//! `values`, `dur`, `begin` offsets, `repeatCount`, `fill`, `calcMode`
//! `linear`/`discrete` (spline and paced degrade to linear), and
//! `keyTimes`. Anything outside the subset drops its rule and the document
//! falls back toward its static frame — unsupported animation never becomes
//! a failed load.

use std::time::Duration;

use resvg::usvg;
use neomacs_display_protocol::animated_visual::AnimatedVisual;

/// A compiled document timeline.
#[derive(Clone, Debug, Default)]
pub(crate) struct AnimationPlan {
    pub(crate) rules: Vec<AnimationRule>,
}

/// One animation element resolved against the source text.
#[derive(Clone, Debug)]
pub(crate) struct AnimationRule {
    /// Where the computed value goes in the source bytes.
    pub(crate) site: AttributeSite,
    /// What value the document shows at a given time.
    pub(crate) timeline: Timeline,
}

/// A splice site for one attribute of one element.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AttributeSite {
    /// The attribute's local name.
    pub(crate) attribute: String,
    /// Byte range of the attribute's value in the source text, when the
    /// element already carries the attribute.
    pub(crate) value_range: Option<std::ops::Range<usize>>,
    /// Index of the start tag's `>` (before the `/` of a self-closing tag):
    /// where ` attribute="value"` is inserted when the attribute is absent.
    pub(crate) insert_pos: usize,
}

/// Timing and values of one rule.
#[derive(Clone, Debug)]
pub(crate) struct Timeline {
    /// Document time the rule activates at.
    pub(crate) begin: Duration,
    /// One cycle of the rule's values.
    pub(crate) dur: Duration,
    /// How many cycles the rule runs.
    pub(crate) repeat: Repeat,
    /// Parsed keyframe values, in cycle order.
    pub(crate) values: Vec<AnimatedValue>,
    /// Author-stated key times as cycle fractions in `[0, 1]`, first 0 and
    /// last 1; `None` means uniform segments.
    pub(crate) key_times: Option<Vec<f64>>,
    /// How values move between keyframes.
    pub(crate) calc: CalcMode,
    /// Whether the last value holds past the active duration
    /// (`fill="freeze"`); the SMIL default removes the animation, restoring
    /// the base value.
    pub(crate) freeze: bool,
    /// The `type` of an `animateTransform`; plain attributes are `None`.
    pub(crate) transform: Option<TransformKind>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Repeat {
    /// `repeatCount="indefinite"`.
    Indefinite,
    /// `repeatCount` as a count (default 1).
    Count(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CalcMode {
    Linear,
    Discrete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TransformKind {
    Rotate,
    Translate,
    Scale,
    SkewX,
    SkewY,
}

impl TransformKind {
    /// The CSS transform function name, as it appears in a `transform`
    /// attribute value.
    pub(crate) const fn function(self) -> &'static str {
        match self {
            Self::Rotate => "rotate",
            Self::Translate => "translate",
            Self::Scale => "scale",
            Self::SkewX => "skewX",
            Self::SkewY => "skewY",
        }
    }
}

/// One keyframe value, in the representation it can animate in.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AnimatedValue {
    /// Space-separated number list (`opacity="0.5"`, `values="0 50;360 50"`).
    Numbers(Vec<f64>),
    /// A color, interpolable channel-wise and re-serialized as `#rrggbb`.
    Color([f64; 3]),
    /// Anything else: displayed at its keyframe, never interpolated.
    Opaque(String),
}

/// Uniform cycle fractions for `count` keyframes: 0, 1/(n-1), …, 1.
fn uniform_fractions(count: usize) -> Vec<f64> {
    match count {
        0 | 1 => vec![0.0],
        n => (0..n).map(|index| index as f64 / (n - 1) as f64).collect(),
    }
}

impl AnimationPlan {
    /// Whether any rule survived compilation.
    pub(crate) fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Total active duration of one rule: cycles × `dur`.
    fn rule_active_duration(rule: &AnimationRule) -> Option<Duration> {
        match rule.timeline.repeat {
            Repeat::Indefinite => None,
            Repeat::Count(count) => rule.timeline.dur.checked_mul(count),
        }
    }

    /// The document's loop period: the total active duration of its rules.
    ///
    /// A plan of finite rules has a finite end and replays from there; a
    /// plan with an indefinite rule loops forever, and its period is the
    /// longest total active duration among its rules. (Rules with different
    /// `dur`s would strictly need an LCM; the sampler quantizes on one
    /// grid, so the maximum keeps every rule's keyframes expressible while
    /// staying honest that the composite loop is no shorter than its
    /// slowest rule.)
    pub(crate) fn loop_period(&self) -> Option<Duration> {
        let mut period = Duration::ZERO;
        for rule in &self.rules {
            // An indefinite rule contributes its cycle length; the loop at
            // least covers every rule's own cycle.
            let duration =
                Self::rule_active_duration(rule).unwrap_or(rule.timeline.dur);
            period = period.max(duration);
        }
        (period > Duration::ZERO).then_some(period)
    }
}

impl AnimatedVisual for AnimationPlan {
    fn next_event(&self, doc_time: Duration) -> Option<Duration> {
        let now = doc_time.as_secs_f64();
        let mut next: Option<f64> = None;
        for rule in &self.rules {
            let timeline = &rule.timeline;
            let dur = timeline.dur.as_secs_f64();
            if !(dur > 0.0) {
                continue;
            }
            let begin = timeline.begin.as_secs_f64();
            let end = match Self::rule_active_duration(rule) {
                Some(active) => begin + active.as_secs_f64(),
                None => f64::INFINITY,
            };
            let fractions = timeline
                .key_times
                .clone()
                .unwrap_or_else(|| uniform_fractions(timeline.values.len()));
            // The cycle the query falls in — possibly one before
            // activation — and its two successors are the only cycles that
            // can hold the next boundary.
            let cycle = ((now - begin) / dur).floor();
            for step in [-1.0, 0.0, 1.0] {
                let cycle_start = begin + (cycle + step) * dur;
                if cycle_start >= end {
                    break;
                }
                for fraction in &fractions {
                    let event = cycle_start + fraction * dur;
                    if event > now && event < end {
                        next = Some(next.map_or(event, |current: f64| current.min(event)));
                    }
                }
            }
        }
        next.and_then(|seconds| Duration::try_from_secs_f64(seconds).ok())
    }

    fn is_continuous(&self) -> bool {
        self.rules.iter().any(|rule| {
            rule.timeline.calc == CalcMode::Linear
                && rule.timeline.values.len() > 1
                && rule
                    .timeline
                    .values
                    .iter()
                    .any(|value| !matches!(value, AnimatedValue::Opaque(_)))
        })
    }

    fn period(&self) -> Option<Duration> {
        self.loop_period().filter(|_| {
            self.rules
                .iter()
                .any(|rule| rule.timeline.repeat == Repeat::Indefinite)
        })
    }
}

/// Compile the animation elements of `data` into a plan.
///
/// `data` must be the exact text the byte ranges will splice into — the
/// pipeline's own rewrites (face colors, root dimensions) run after this on
/// the patched text, so their range arithmetic stays self-consistent.
pub(crate) fn compile(data: &[u8]) -> Option<AnimationPlan> {
    let text = std::str::from_utf8(data).ok()?;
    let document = usvg::roxmltree::Document::parse(text).ok()?;
    let root = document.root_element();
    if root.tag_name().name() != "svg" {
        return None;
    }

    let mut plan = AnimationPlan::default();
    for node in root.descendants() {
        if !node.is_element() {
            continue;
        }
        if !matches!(node.tag_name().name(), "animate" | "animateTransform" | "set") {
            continue;
        }
        if let Some(rule) = compile_rule(data, node) {
            // Two rules on one attribute cannot both splice it; document
            // order decides, last wins, matching the SMIL sandwich's
            // later-document priority for the common non-additive case.
            // The attribute name is part of the key: two absent attributes
            // of one element share an insert position but not a target.
            plan.rules.retain(|existing| {
                existing.site.attribute != rule.site.attribute
                    || existing.site.value_range != rule.site.value_range
            });
            plan.rules.push(rule);
        }
    }
    Some(plan)
}

fn compile_rule(data: &[u8], node: usvg::roxmltree::Node<'_, '_>) -> Option<AnimationRule> {
    let transform = match node.tag_name().name() {
        "animateTransform" => Some(match node.attribute("type") {
            Some("translate") | None => TransformKind::Translate,
            Some("rotate") => TransformKind::Rotate,
            Some("scale") => TransformKind::Scale,
            Some("skewX") => TransformKind::SkewX,
            Some("skewY") => TransformKind::SkewY,
            // Unknown transform types cannot be serialized back faithfully.
            _ => return None,
        }),
        _ => None,
    };

    let target = resolve_target(node)?;
    let attribute = node
        .attribute("attributeName")
        .map(str::to_owned)
        .or_else(|| transform.is_some().then(|| "transform".to_owned()))?;
    if node.attribute("repeatDur").is_some()
        || node.attribute("additive").is_some_and(|value| value == "sum")
    {
        // Composing onto base values needs the base value at evaluation
        // time; v1 declines rather than approximating silently.
        return None;
    }

    let site = attribute_site(data, target, &attribute)?;

    let mut calc = match node.attribute("calcMode") {
        Some("discrete") => CalcMode::Discrete,
        // `spline` needs keySplines; `paced` needs path-length math. Both
        // degrade to linear interpolation, which is visually close for the
        // icon-class documents this subset targets.
        _ => CalcMode::Linear,
    };
    if node.tag_name().name() == "set" {
        calc = CalcMode::Discrete;
    }

    let begin = clock_value(node.attribute("begin").unwrap_or("0s"))?;
    let dur = clock_value(node.attribute("dur")?)?;
    if dur.is_zero() {
        return None;
    }
    let repeat = match node.attribute("repeatCount") {
        Some("indefinite") => Repeat::Indefinite,
        Some(count) => Repeat::Count(
            count
                .parse::<f64>()
                .ok()
                .filter(|count| count.is_finite() && *count >= 0.0)
                .and_then(|count| u32::try_from(count as u64).ok())
                .unwrap_or(1),
        ),
        None => Repeat::Count(1),
    };
    let freeze = node.attribute("fill") == Some("freeze");

    let raw_values = raw_keyframe_values(node)?;
    let values: Vec<AnimatedValue> = raw_values.iter().map(|value| parse_value(value)).collect();
    if calc == CalcMode::Linear
        && values
            .iter()
            .any(|value| matches!(value, AnimatedValue::Opaque(_)))
    {
        // Interpolation needs numeric or color keyframes on both ends of
        // every segment; anything else steps.
        calc = CalcMode::Discrete;
    }
    // Number lists must agree in width to interpolate component-wise.
    if calc == CalcMode::Linear {
        let widths: Option<Vec<usize>> = values
            .iter()
            .map(|value| match value {
                AnimatedValue::Numbers(numbers) => Some(numbers.len()),
                AnimatedValue::Color(_) => Some(3),
                AnimatedValue::Opaque(_) => None,
            })
            .collect();
        let widths = widths?;
        if widths.windows(2).any(|pair| pair[0] != pair[1]) {
            calc = CalcMode::Discrete;
        }
    }

    let key_times = match node.attribute("keyTimes").and_then(parse_key_times) {
        Some(times) if times.len() == values.len() => Some(times),
        // A malformed count is ignored rather than dropping the rule: the
        // uniform fallback keeps the animation expressible.
        _ => None,
    };

    Some(AnimationRule {
        site,
        timeline: Timeline {
            begin,
            dur,
            repeat,
            values,
            key_times,
            calc,
            freeze,
            transform,
        },
    })
}

/// The element a rule animates: its `href`/`xlink:href` target when it has
/// one, else its parent element.
fn resolve_target<'a, 'input>(
    node: usvg::roxmltree::Node<'a, 'input>,
) -> Option<usvg::roxmltree::Node<'a, 'input>> {
    match node
        .attribute("href")
        .or_else(|| node.attribute(("http://www.w3.org/1999/xlink", "href")))
    {
        Some(reference) => {
            let id = reference.strip_prefix('#')?;
            node.document()
                .root_element()
                .descendants()
                .find(|candidate| candidate.attribute("id") == Some(id))
        }
        None => node.parent_element(),
    }
}

fn raw_keyframe_values(node: usvg::roxmltree::Node<'_, '_>) -> Option<Vec<String>> {
    if let Some(values) = node.attribute("values") {
        let split: Vec<String> = values
            .split(';')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect();
        return (!split.is_empty()).then_some(split);
    }
    if node.tag_name().name() == "set" {
        let to = node.attribute("to")?.trim().to_owned();
        return (!to.is_empty()).then_some(vec![to]);
    }
    // `from`/`to` is `values` of two; a lone `from` cannot move.
    let from = node.attribute("from")?.trim().to_owned();
    let to = node.attribute("to")?.trim().to_owned();
    Some(vec![from, to])
}

fn parse_value(raw: &str) -> AnimatedValue {
    if let Some(color) = parse_color(raw) {
        return AnimatedValue::Color(color);
    }
    let components: Vec<&str> = raw.split_whitespace().collect();
    let numbers: Vec<f64> = components
        .iter()
        .map(|component| component.parse::<f64>())
        .collect::<Result<Vec<_>, _>>()
        .ok()
        .unwrap_or_default();
    if !components.is_empty() && numbers.len() == components.len() {
        return AnimatedValue::Numbers(numbers);
    }
    AnimatedValue::Opaque(raw.to_owned())
}

fn parse_color(raw: &str) -> Option<[f64; 3]> {
    let raw = raw.trim();
    let hex = raw.strip_prefix('#')?;
    let octet = |slice: &str| u8::from_str_radix(slice, 16).ok();
    let (r, g, b) = match hex.len() {
        3 => (octet(&hex[0..1])? * 17, octet(&hex[1..2])? * 17, octet(&hex[2..3])? * 17),
        6 => (octet(&hex[0..2])?, octet(&hex[2..4])?, octet(&hex[4..6])?),
        _ => return None,
    };
    Some([f64::from(r), f64::from(g), f64::from(b)])
}

/// SMIL clock values, restricted to the offset form: `2`, `2s`, `250ms`.
///
/// Event-based (`begin="click"`) and syncbase (`begin="other.begin"`)
/// clocks have no document-only meaning and are rejected, dropping the
/// rule rather than guessing an offset.
fn clock_value(raw: &str) -> Option<Duration> {
    let raw = raw.trim();
    let split = raw
        .find(|byte: char| !(byte.is_ascii_digit() || byte == '.' || byte == '-' || byte == '+'))
        .unwrap_or(raw.len());
    let (number, unit) = raw.split_at(split);
    let seconds: f64 = number.parse().ok()?;
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    let seconds = match unit.trim() {
        "" | "s" => seconds,
        "ms" => seconds / 1_000.0,
        _ => return None,
    };
    Duration::try_from_secs_f64(seconds).ok()
}

fn parse_key_times(raw: &str) -> Option<Vec<f64>> {
    let times: Vec<f64> = raw
        .split(';')
        .map(str::trim)
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    match times.as_slice() {
        [first, .., last] if first == &0.0 && last == &1.0 => Some(times),
        _ => None,
    }
}

fn attribute_site(
    data: &[u8],
    target: usvg::roxmltree::Node<'_, '_>,
    attribute: &str,
) -> Option<AttributeSite> {
    let value_range = target
        .attributes()
        .find(|candidate| candidate.name() == attribute)
        .map(|candidate| candidate.range_value());
    Some(AttributeSite {
        attribute: attribute.to_owned(),
        value_range,
        insert_pos: find_start_tag_close(data, target.range().start)?,
    })
}

/// Index of the `>` closing the start tag at `start`, before the `/` of a
/// self-closing tag — the point a missing attribute can be inserted at.
fn find_start_tag_close(data: &[u8], start: usize) -> Option<usize> {
    let mut quote = None;
    for (offset, byte) in data.get(start..)?.iter().copied().enumerate() {
        match (quote, byte) {
            (Some(expected), actual) if actual == expected => quote = None,
            (None, b'\'' | b'"') => quote = Some(byte),
            (None, b'>') => {
                let close = start + offset;
                return Some(if data.get(close.wrapping_sub(1)) == Some(&b'/') {
                    close - 1
                } else {
                    close
                });
            }
            _ => {}
        }
    }
    None
}

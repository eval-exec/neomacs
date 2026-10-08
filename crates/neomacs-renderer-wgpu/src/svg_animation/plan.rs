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

use neomacs_display_protocol::animated_visual::AnimatedVisual;
use resvg::usvg;

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
    /// last 1 for linear mode; discrete mode may end earlier.
    /// `None` means uniform segments.
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
    /// Validated active span, including fractional repeats (default one cycle).
    Finite(Duration),
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
            Repeat::Finite(active) => Some(active),
        }
    }

    /// Whether any rule repeats indefinitely.
    pub(crate) fn has_indefinite(&self) -> bool {
        self.rules
            .iter()
            .any(|rule| rule.timeline.repeat == Repeat::Indefinite)
    }

    /// Document time after all starts and finite endings.
    ///
    /// The sampler starts a looping plan's grid here: everything before is
    /// introductory state, and a loop restarting the introduction every
    /// cycle would insert a base-value gap SMIL does not have.
    pub(crate) fn intro_end(&self) -> Duration {
        self.rules
            .iter()
            .filter_map(|rule| match rule.timeline.repeat {
                Repeat::Indefinite => Some(rule.timeline.begin),
                Repeat::Finite(active) => rule.timeline.begin.checked_add(active),
            })
            .max()
            .unwrap_or_default()
    }

    /// Exact common period of indefinite rules, or the finite document span.
    /// Checked nanosecond LCM preserves every rule's phase on wrap. Finite
    /// rules do not contribute to an indefinite period: they have finished
    /// by `intro_end`, and their frozen or removed effects remain stable.
    pub(crate) fn loop_period(&self) -> Option<Duration> {
        if self.has_indefinite() {
            let mut common = 1_u128;
            for rule in &self.rules {
                if rule.timeline.repeat != Repeat::Indefinite {
                    continue;
                }
                let duration = rule.timeline.dur.as_nanos();
                common = (common / gcd(common, duration)).checked_mul(duration)?;
            }
            return duration_from_nanos(common);
        }
        let mut period = Duration::ZERO;
        for rule in &self.rules {
            let timeline = &rule.timeline;
            let duration = match timeline.repeat {
                Repeat::Indefinite => timeline.dur,
                Repeat::Finite(active) => timeline.begin.checked_add(active)?,
            };
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
            if end > now && end.is_finite() {
                next = Some(next.map_or(end, |current| current.min(end)));
            }
            let fractions = timeline
                .key_times
                .clone()
                .unwrap_or_else(|| match timeline.calc {
                    CalcMode::Linear => uniform_fractions(timeline.values.len()),
                    CalcMode::Discrete => (0..timeline.values.len())
                        .map(|index| index as f64 / timeline.values.len() as f64)
                        .collect(),
                });
            // The cycle holding the next boundary, measured in the
            // rule's own cycles: clamped at zero so a query far before
            // activation still reaches the `begin` boundary instead of
            // windowing pre-activation cycles that hold no events.
            let cycle = (((now - begin) / dur).floor()).max(0.0);
            for step in [-1.0, 0.0, 1.0] {
                let cycle_start = begin + (cycle + step) * dur;
                if cycle_start >= end {
                    break;
                }
                for fraction in &fractions {
                    let event = cycle_start + fraction * dur;
                    // Boundaries of pre-activation cycles are not events:
                    // the rule holds the base value until `begin`.
                    if event > now && event >= begin && event < end {
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
    if root.tag_name().name() != "svg" || !is_svg_element(root) {
        return None;
    }

    let mut plan = AnimationPlan::default();
    for node in root.descendants() {
        if !node.is_element() || !is_svg_element(node) {
            continue;
        }
        if !matches!(
            node.tag_name().name(),
            "animate" | "animateTransform" | "set"
        ) {
            continue;
        }
        if let Some(rule) = compile_rule(data, node) {
            // Preserve all contributions. Evaluation selects one per site
            // using activation priority, with document order breaking ties.
            plan.rules.push(rule);
        }
    }
    Some(plan)
}

fn is_svg_element(node: usvg::roxmltree::Node<'_, '_>) -> bool {
    node.tag_name()
        .namespace()
        .is_none_or(|namespace| namespace == "http://www.w3.org/2000/svg")
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
        || node
            .attribute("additive")
            .is_some_and(|value| value == "sum")
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
        Some(count) => {
            let count = count.parse::<f64>().ok()?;
            if !count.is_finite() || count <= 0.0 {
                return None;
            }
            let active = Duration::try_from_secs_f64(dur.as_secs_f64() * count).ok()?;
            if active.is_zero() || begin.checked_add(active).is_none() {
                return None;
            }
            Repeat::Finite(active)
        }
        None => Repeat::Finite(dur),
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

    let key_times = match node
        .attribute("keyTimes")
        .and_then(|raw| parse_key_times(raw, calc))
    {
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
    // Hex digits are ASCII; anything multi-byte is not a color and slicing
    // it as octets would panic. Reject before slicing.
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let octet = |slice: &str| u8::from_str_radix(slice, 16).ok();
    let (r, g, b) = match hex.len() {
        3 => (
            octet(&hex[0..1])? * 17,
            octet(&hex[1..2])? * 17,
            octet(&hex[2..3])? * 17,
        ),
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

fn parse_key_times(raw: &str, calc: CalcMode) -> Option<Vec<f64>> {
    let times: Vec<f64> = raw
        .split(';')
        .map(str::trim)
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    (times.first() == Some(&0.0)
        && (calc == CalcMode::Discrete || times.last() == Some(&1.0))
        && times
            .iter()
            .all(|time| time.is_finite() && (0.0..=1.0).contains(time))
        && times.windows(2).all(|pair| pair[0] <= pair[1]))
    .then_some(times)
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn duration_from_nanos(nanos: u128) -> Option<Duration> {
    Some(Duration::new(
        u64::try_from(nanos / 1_000_000_000).ok()?,
        (nanos % 1_000_000_000) as u32,
    ))
}

/// Finite spans have a terminal state; only proven periodic spans wrap.
#[derive(Clone, Copy, Debug)]
pub(crate) enum SampleTimeline {
    Finite { end: Duration },
    Repeating { origin: Duration, period: Duration },
}

impl AnimationPlan {
    pub(crate) fn sample_timeline(&self) -> Option<SampleTimeline> {
        let span = self.loop_period()?;
        Some(if self.has_indefinite() {
            SampleTimeline::Repeating {
                origin: self.intro_end(),
                period: span,
            }
        } else {
            SampleTimeline::Finite { end: span }
        })
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

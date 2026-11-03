//! One decode of the `display` property union, shared by every consumer.
//!
//! A `display` property value is an untyped Lisp union with two levels, and GNU
//! decodes each in exactly one place:
//!
//! * the SHAPE — `handle_display_spec` (src/xdisp.c): a `(disable-eval SPEC)`
//!   wrapper, a LIST of specs, a VECTOR of specs, or a single spec;
//! * the single-spec HEAD — `handle_single_display_spec`: `when`, `height`,
//!   `space-width`, `min-width`, `slice`, `raise`, the fringe specs, a
//!   `((margin AREA) VALUE)` prefix, `space`, an image/xwidget spec, or a string.
//!
//! neomacs decoded both ad-hoc at several sites, and they had drifted:
//! `neovm-core`'s replacing-predicate and the layout engine's classifier each
//! carried their own copy of the head table (disagreeing on keyword heads), and
//! the layout classifier had no VECTOR arm at all, so a `display` value like
//! `["REPLACEMENT"]` — which GNU renders — was silently ignored.
//!
//! Decoding here once, into [`DisplaySpecKind`], makes a missed shape a compile
//! error in the `match` rather than a silently unrendered display property. The
//! payload of each spec stays with the consumer: the layout engine parses image
//! plists, space geometry and fringe layouts, and the interpreter only needs to
//! know whether the spec replaces text.
//!
//! The head table is read under the value's [`DisplayEval`] policy, so a
//! `(disable-eval …)` value cannot name a neomacs media spec through any
//! consumer: the policy is an argument of the one function every consumer
//! classifies with.

use super::value::Value;
use std::ops::ControlFlow;

/// Geometry operands of a stretch-space display specification. Shared by
/// geometry evaluation and its mutable-input freshness projection.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr, strum::EnumCount,
)]
#[repr(usize)]
#[strum(prefix = ":", serialize_all = "kebab-case")]
pub enum DisplaySpaceKey {
    Width,
    RelativeWidth,
    AlignTo,
    Height,
    RelativeHeight,
    Ascent,
}

impl DisplaySpaceKey {
    pub fn from_lisp_value(value: Value) -> Option<Self> {
        Self::from_keyword(value.as_bare_symbol_name()?)
    }

    pub fn from_keyword(name: &str) -> Option<Self> {
        name.strip_prefix(':')?.parse().ok()
    }

    pub fn keyword(self) -> &'static str {
        self.into()
    }

    pub fn value(self) -> Value {
        Value::keyword(self.keyword())
    }
}

/// GNU's single-spec taxonomy from `handle_single_display_spec`, as the head of
/// one spec: what KIND of spec this is, not its parsed payload.
///
/// [`DisplaySpecKind::Other`] is load-bearing, not a fallback: at the shape level
/// GNU treats a cons whose head is unrecognized as a LIST OF SPECS to iterate
/// (that is what makes diff-hl's `((left-fringe BITMAP FACE))` work), and as a
/// single spec it means "no display effect".
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplaySpecKind {
    /// `(when FORM . VALUE)` — conditional; VALUE is the spec if FORM is non-nil.
    When,
    /// `(height HEIGHT)` — modifier.
    Height,
    /// `(space-width FACTOR)` — modifier.
    SpaceWidth,
    /// `(min-width (WIDTH))` — modifier.
    MinWidth,
    /// `(slice X Y WIDTH HEIGHT)` — image-slice modifier.
    Slice,
    /// `(raise FACTOR)` — modifier.
    Raise,
    /// `(left-fringe BITMAP [FACE])` — replaces the text; bitmap goes in the
    /// left fringe.
    LeftFringe,
    /// `(right-fringe BITMAP [FACE])` — as [`Self::LeftFringe`], right fringe.
    RightFringe,
    /// `((margin left-margin|right-margin|nil) VALUE)` — the marginal-area
    /// prefix; VALUE is what is displayed, in the named margin.
    Margin,
    /// `(space ...)` — a stretch glyph; replaces the text.
    Space,
    /// `(image ...)` — replaces the text on a GUI frame.
    Image,
    /// `(xwidget ...)` — replaces the text on a GUI frame.
    Xwidget,
    /// `(video ...)`, `(webkit ...)`, `(surface ...)` — neomacs media
    /// extensions, classified like [`Self::Image`]. GNU has no such heads, so a
    /// GNU build reads such a cons as a LIST of specs; that is a deliberate
    /// neomacs superset, kept in ONE place, and recognized only under
    /// [`DisplayEval::Enabled`] — see [`DisplayEval`].
    Media(DisplayMediaSpecKind),
    /// A keyword-headed list such as `(:raise 0.2 :height 1.4)` — a neomacs
    /// convenience form parsed as one spec whose modifiers come from the whole
    /// plist. GNU has no such form: it would iterate the list and ignore every
    /// element. Also kept deliberately, and in ONE place, so the two crates
    /// cannot classify it oppositely (they did: one read it as a single spec,
    /// the other as a list of specs).
    KeywordPlist,
    /// A string — replaces the text with its contents.
    Text,
    /// Anything else: at the shape level a cons of this kind is a LIST OF SPECS;
    /// as a single spec it has no display effect.
    Other,
}

/// Which neomacs media extension a [`DisplaySpecKind::Media`] spec names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayMediaSpecKind {
    Video,
    Webkit,
    Surface,
}

/// GNU's `enable_eval_p` (`handle_display_spec`, src/xdisp.c): whether a
/// `display` value is trusted to do more than draw.
///
/// `(disable-eval SPEC)` marks a value an external agent produced: enriched.el
/// wraps every `display` property it decodes from an `<x-display>` annotation
/// in it (`enriched-decode-display-prop`), and Gnus decodes a `text/enriched`
/// MIME part with that same decoder (`mm-inline-text`). GNU's contract
/// (lispref, "display property, unsafe evaluation") is that no Lisp in SPEC is
/// evaluated, "while still supporting all the other display property
/// features". Suppressing `when` FORMs and `height` functions is enough for
/// GNU because none of its other specs runs code or fetches anything remote:
/// `image` decodes a local file or inline `:data`, and `xwidget` needs a live
/// xwidget object, which `read` cannot produce.
///
/// The neomacs media heads do: resolving `(video …)`, `(webkit …)` or
/// `(surface …)` opens an attacker-chosen URL or file in native demuxers and
/// decoders, loads a web page, or compiles a GPU shader and runs it every
/// frame. Under [`Self::Disabled`], [`display_spec_kind`] therefore does not
/// recognize them, and the value's shape decodes as in GNU, which has no such
/// heads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayEval {
    /// A plain `display` value: `when` FORMs and `height` functions are
    /// evaluated, and the neomacs media heads are display specs.
    Enabled,
    /// The SPEC of a `(disable-eval SPEC)` wrapper.
    Disabled,
}

impl DisplayEval {
    /// The kind of a neomacs media head under this policy: GNU's reading,
    /// [`DisplaySpecKind::Other`], when evaluation is disabled.
    fn media(self, kind: DisplayMediaSpecKind) -> DisplaySpecKind {
        match self {
            Self::Enabled => DisplaySpecKind::Media(kind),
            Self::Disabled => DisplaySpecKind::Other,
        }
    }
}

/// Typed destination from GNU's `(margin LOCATION)` display prefix.
///
/// GNU treats `nil` as the ordinary text area, not as a third marginal area.
/// Keeping that case explicit prevents callers from accidentally suppressing a
/// valid `((margin nil) CONTENT)` replacement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayMarginLocation {
    Text,
    Left,
    Right,
}

/// A validated GNU `((margin LOCATION) CONTENT)` display specification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayMarginSpec {
    location: DisplayMarginLocation,
    content: Value,
}

impl DisplayMarginSpec {
    pub fn location(self) -> DisplayMarginLocation {
        self.location
    }

    pub fn content(self) -> Value {
        self.content
    }
}

impl DisplaySpecKind {
    /// Whether a spec of this kind REPLACES the text it covers, per the
    /// `it == NULL` paths of GNU `handle_single_display_spec`.
    ///
    /// `frame_window_p` is GNU's `FRAME_WINDOW_P`: an image-class spec is only
    /// `valid_image_p` on a GUI frame, so on a tty it replaces nothing. A
    /// `(space ...)` spec and the fringe specs replace text on a tty too.
    ///
    /// [`Self::Margin`], [`Self::When`] and [`Self::Text`] are decided by the
    /// caller: the first two have an inner spec to look at, and a string always
    /// replaces.
    pub fn replaces_text(self, frame_window_p: bool) -> Option<bool> {
        match self {
            Self::Text | Self::Space | Self::LeftFringe | Self::RightFringe => Some(true),
            Self::Image | Self::Xwidget | Self::Media(_) => Some(frame_window_p),
            Self::Height
            | Self::SpaceWidth
            | Self::MinWidth
            | Self::Slice
            | Self::Raise
            | Self::KeywordPlist
            | Self::Other => Some(false),
            Self::When | Self::Margin => None,
        }
    }
}

/// Classify ONE display spec by its head, under the [`DisplayEval`] policy of the
/// `display` value it belongs to. This is the only place the spec-head table
/// exists.
pub fn display_spec_kind(spec: Value, eval: DisplayEval) -> DisplaySpecKind {
    if spec.is_string() {
        return DisplaySpecKind::Text;
    }
    if !spec.is_cons() {
        return DisplaySpecKind::Other;
    }
    let car = spec.cons_car();
    if car.is_cons() {
        // `((margin AREA) VALUE)`. GNU's SHAPE test looks at the `margin` head
        // only; whether AREA itself is usable is a separate question, answered by
        // [`display_spec_margin_value`], because an unusable AREA still keeps the
        // value a single spec rather than turning it into a list of specs.
        if car.cons_car().is_bare_symbol_named("margin") {
            return DisplaySpecKind::Margin;
        }
        return DisplaySpecKind::Other;
    }
    let Some(name) = car.as_bare_symbol_name() else {
        return DisplaySpecKind::Other;
    };
    match name {
        "when" => DisplaySpecKind::When,
        "height" => DisplaySpecKind::Height,
        "space-width" => DisplaySpecKind::SpaceWidth,
        "min-width" => DisplaySpecKind::MinWidth,
        "slice" => DisplaySpecKind::Slice,
        "raise" => DisplaySpecKind::Raise,
        "left-fringe" => DisplaySpecKind::LeftFringe,
        "right-fringe" => DisplaySpecKind::RightFringe,
        "space" => DisplaySpecKind::Space,
        "image" => DisplaySpecKind::Image,
        "xwidget" => DisplaySpecKind::Xwidget,
        "video" => eval.media(DisplayMediaSpecKind::Video),
        "webkit" => eval.media(DisplayMediaSpecKind::Webkit),
        "surface" => eval.media(DisplayMediaSpecKind::Surface),
        _ if name.starts_with(':') => DisplaySpecKind::KeywordPlist,
        _ => DisplaySpecKind::Other,
    }
}

/// GNU's AREA test for a `((margin AREA) VALUE)` prefix: `nil`, `left-margin`
/// or `right-margin`. `head_cdr` is the cdr of the `(margin . AREA-TAIL)` head.
fn display_margin_location(head_cdr: Value) -> Option<DisplayMarginLocation> {
    let area = if head_cdr.is_cons() {
        head_cdr.cons_car()
    } else {
        Value::NIL
    };
    if area.is_nil() {
        Some(DisplayMarginLocation::Text)
    } else if area.is_bare_symbol_named("left-margin") {
        Some(DisplayMarginLocation::Left)
    } else if area.is_bare_symbol_named("right-margin") {
        Some(DisplayMarginLocation::Right)
    } else {
        None
    }
}

/// `(when FORM . VALUE)` split into FORM and the inner spec VALUE.
///
/// GNU evaluates FORM (`handle_single_display_spec`, src/xdisp.c:6130-6164);
/// `Context::display_when_form_holds` is that evaluation, and the layout
/// engine consults its results.  A caller with no evaluation behind it
/// (GNU's own `single_display_spec_string_p` shortcut) takes a non-nil FORM
/// as holding.
pub fn display_spec_when_parts(spec: Value) -> Option<(Value, Value)> {
    // `when` is a GNU head, the same under either policy; the restrictive one
    // keeps this structural helper from ever seeing a media head.
    if !matches!(
        display_spec_kind(spec, DisplayEval::Disabled),
        DisplaySpecKind::When
    ) {
        return None;
    }
    let rest = spec.cons_cdr();
    if !rest.is_cons() {
        return None;
    }
    Some((rest.cons_car(), rest.cons_cdr()))
}

/// The VALUE displayed by a `((margin AREA) VALUE)` spec, or `None` when AREA is
/// not one GNU accepts — GNU then leaves its `location` unbound and tests the
/// whole cons for validity, which no cons of this shape passes, so nothing is
/// displayed.
pub fn display_margin_spec(spec: Value) -> Option<DisplayMarginSpec> {
    // As in `display_spec_when_parts`: `margin` is policy-independent.
    if !matches!(
        display_spec_kind(spec, DisplayEval::Disabled),
        DisplaySpecKind::Margin
    ) {
        return None;
    }
    let location = display_margin_location(spec.cons_car().cons_cdr())?;
    let after = spec.cons_cdr();
    let content = if after.is_cons() {
        after.cons_car()
    } else {
        Value::NIL
    };
    Some(DisplayMarginSpec { location, content })
}

/// Compatibility accessor for callers that only need the inner content.
pub fn display_spec_margin_value(spec: Value) -> Option<Value> {
    display_margin_spec(spec).map(DisplayMarginSpec::content)
}

/// The specs of one `display` property value, in GNU `handle_display_spec`
/// order: the shape decode, once.
#[derive(Clone, Copy, Debug)]
pub struct DisplayPropertySpecs {
    value: Value,
    /// [`DisplayEval::Disabled`] when the value was wrapped in
    /// `(disable-eval SPEC)` (enriched.el): GNU then refuses to evaluate `when`
    /// forms and `height` functions inside, and the neomacs media heads are not
    /// display specs. Consumers classify every spec of this value under it.
    pub eval: DisplayEval,
}

impl DisplayPropertySpecs {
    /// Decode the shape of a `display` property value, stripping an outer
    /// `(disable-eval SPEC)` wrapper.
    pub fn of(value: Value) -> Self {
        if value.is_cons() && value.cons_car().is_bare_symbol_named("disable-eval") {
            let rest = value.cons_cdr();
            return Self {
                value: if rest.is_cons() {
                    rest.cons_car()
                } else {
                    Value::NIL
                },
                eval: DisplayEval::Disabled,
            };
        }
        Self {
            value,
            eval: DisplayEval::Enabled,
        }
    }

    /// Whether the value is a LIST OF SPECS (to iterate) rather than one spec:
    /// GNU's test is a cons whose car is neither a recognized single-spec head
    /// nor a `(margin ...)` prefix nor nil.
    pub fn is_spec_list(&self) -> bool {
        self.value.is_cons()
            && !self.value.cons_car().is_nil()
            && matches!(
                display_spec_kind(self.value, self.eval),
                DisplaySpecKind::Other
            )
    }

    /// Visit each single spec in GNU order. A vector is iterated element by
    /// element (GNU's `VECTORP (spec)` arm), a list of specs cons by cons, and
    /// anything else is one spec.
    ///
    /// Returning [`ControlFlow::Break`] stops the walk, which is how GNU's
    /// callers stop after the element that replaced the text.
    pub fn for_each<F>(&self, mut visit: F)
    where
        F: FnMut(Value) -> ControlFlow<()>,
    {
        if let Some(items) = self.value.as_vector_data() {
            for item in items.iter() {
                if visit(*item).is_break() {
                    return;
                }
            }
            return;
        }
        if self.is_spec_list() {
            let mut cursor = self.value;
            let mut slow = cursor;
            let mut advance_slow = false;
            while cursor.is_cons() {
                if visit(cursor.cons_car()).is_break() {
                    return;
                }
                cursor = cursor.cons_cdr();
                // Allocation-free cycle detection. The fast cursor is also
                // the visitor, so every reachable list cell is visited before
                // a cycle is detected. Dotted tails retain their old behavior.
                advance_slow = !advance_slow;
                if !advance_slow {
                    if !slow.is_cons() {
                        return;
                    }
                    slow = slow.cons_cdr();
                    if cursor == slow {
                        return;
                    }
                }
            }
            return;
        }
        let _ = visit(self.value);
    }
}

#[cfg(test)]
#[path = "tests/display_spec_test.rs"]
mod tests;

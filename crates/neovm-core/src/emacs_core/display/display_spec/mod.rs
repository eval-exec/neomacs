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

use super::value::Value;
use std::ops::ControlFlow;

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
    /// GNU build iterates them as a list of unrecognized specs and displays
    /// nothing; that is a deliberate neomacs superset, kept in ONE place.
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

/// Classify ONE display spec by its head. This is the only place the spec-head
/// table exists.
pub fn display_spec_kind(spec: Value) -> DisplaySpecKind {
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
        if car.cons_car().is_symbol_named("margin") {
            return DisplaySpecKind::Margin;
        }
        return DisplaySpecKind::Other;
    }
    let Some(name) = car.as_symbol_name() else {
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
        "video" => DisplaySpecKind::Media(DisplayMediaSpecKind::Video),
        "webkit" => DisplaySpecKind::Media(DisplayMediaSpecKind::Webkit),
        "surface" => DisplaySpecKind::Media(DisplayMediaSpecKind::Surface),
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
    } else if area.is_symbol_named("left-margin") {
        Some(DisplayMarginLocation::Left)
    } else if area.is_symbol_named("right-margin") {
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
    if !matches!(display_spec_kind(spec), DisplaySpecKind::When) {
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
    if !matches!(display_spec_kind(spec), DisplaySpecKind::Margin) {
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
    /// False when the value was wrapped in `(disable-eval SPEC)` (enriched.el):
    /// GNU then refuses to evaluate `when` forms and `height` functions inside.
    pub eval_enabled: bool,
}

impl DisplayPropertySpecs {
    /// Decode the shape of a `display` property value, stripping an outer
    /// `(disable-eval SPEC)` wrapper.
    pub fn of(value: Value) -> Self {
        if value.is_cons() && value.cons_car().is_symbol_named("disable-eval") {
            let rest = value.cons_cdr();
            return Self {
                value: if rest.is_cons() {
                    rest.cons_car()
                } else {
                    Value::NIL
                },
                eval_enabled: false,
            };
        }
        Self {
            value,
            eval_enabled: true,
        }
    }

    /// Whether the value is a LIST OF SPECS (to iterate) rather than one spec:
    /// GNU's test is a cons whose car is neither a recognized single-spec head
    /// nor a `(margin ...)` prefix nor nil.
    pub fn is_spec_list(&self) -> bool {
        self.value.is_cons()
            && !self.value.cons_car().is_nil()
            && matches!(display_spec_kind(self.value), DisplaySpecKind::Other)
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
            while cursor.is_cons() {
                if visit(cursor.cons_car()).is_break() {
                    return;
                }
                cursor = cursor.cons_cdr();
            }
            return;
        }
        let _ = visit(self.value);
    }
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;

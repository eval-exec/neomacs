use crate::display_item::{
    DisplayItemLayout, DisplayLength, DisplayMediaReplacement, DisplayStretch, DisplayStretchWidth,
    DisplaySurfaceItem, DisplayXwidgetItem,
};
use crate::display_spec::{
    DisplayFringeLayout, DisplayImageSliceSpec, DisplaySpaceKey, parse_display_fringe_layout,
    parse_display_image_slice, parse_display_surface_layout, parse_display_xwidget_layout,
};
use neovm_core::emacs_core::Value;
use neovm_core::emacs_core::display_spec::{
    DisplayMarginLocation, DisplayMediaSpecKind, DisplayPropertySpecs, DisplaySpecKind,
    display_margin_spec, display_spec_kind, display_spec_when_parts,
};
use neovm_core::emacs_core::value::list_to_vec;
use std::ops::ControlFlow;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplayPropertyClassification {
    replacement: Option<DisplayReplacementProperty>,
    /// The single spec that produced `replacement` — GNU's `spec` argument to
    /// `handle_single_display_spec`, NOT the whole `display` value.
    ///
    /// They differ whenever the property is a list or vector of specs, or a
    /// `(when FORM . SPEC)` wrapper. Consumers that need the Lisp payload (the
    /// string to display, the image spec to resolve) must read it from HERE: they
    /// used to re-derive it from the top-level value, so a list-wrapped
    /// `("REPLACEMENT")` classified as a string replacement and then tried to
    /// display the LIST as that string, rendering the original text instead.
    replacement_spec: Value,
    modifiers: DisplayTextPropertyModifiers,
    image_slice: Option<DisplayImageSliceSpec>,
}

impl Default for DisplayPropertyClassification {
    fn default() -> Self {
        Self {
            replacement: None,
            replacement_spec: Value::NIL,
            modifiers: DisplayTextPropertyModifiers::default(),
            image_slice: None,
        }
    }
}

impl DisplayPropertyClassification {
    pub(crate) fn replacement(&self) -> Option<&DisplayReplacementProperty> {
        self.replacement.as_ref()
    }

    /// The spec that produced the replacement — see [`Self::replacement_spec`].
    pub(crate) fn replacement_spec(&self) -> Value {
        self.replacement_spec
    }

    pub(crate) fn modifiers(&self) -> DisplayTextPropertyModifiers {
        self.modifiers
    }

    pub(crate) fn image_slice(&self) -> Option<DisplayImageSliceSpec> {
        self.image_slice
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(
        replacement: Option<DisplayReplacementProperty>,
        replacement_spec: Value,
        modifiers: DisplayTextPropertyModifiers,
    ) -> Self {
        Self {
            replacement,
            replacement_spec,
            modifiers,
            image_slice: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DisplayReplacementProperty {
    String,
    Stretch(DisplayStretch),
    Media(DisplayMediaReplacementProperty),
    /// `(left-fringe BITMAP FACE)` / `(right-fringe BITMAP FACE)`: GNU renders
    /// the bitmap in the fringe and shows nothing inline for the covered text
    /// (the spec REPLACES the text in the text area). The parsed layout carries
    /// the bitmap symbol + side + optional face so the row-render path can
    /// record a fringe descriptor on the row; the inline text stays suppressed
    /// (zero inline width), matching GNU's text-area output.
    Fringe(DisplayFringeLayout),
    /// A parsed marginal-area replacement.  Side and content survive
    /// classification as typed data, so later exhaustive matches cannot silently
    /// turn a valid GNU margin spec into an empty inline replacement.
    Margin(DisplayMarginReplacement),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DisplayMarginSide {
    Left,
    Right,
}

impl DisplayMarginSide {
    pub(crate) fn glyph_area(self) -> neomacs_display_protocol::glyph_matrix::GlyphArea {
        match self {
            Self::Left => neomacs_display_protocol::glyph_matrix::GlyphArea::LeftMargin,
            Self::Right => neomacs_display_protocol::glyph_matrix::GlyphArea::RightMargin,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DisplayMarginContent {
    String(Value),
    Stretch {
        spec: Value,
        layout: DisplayStretch,
    },
    Media {
        spec: Value,
        replacement: DisplayMediaReplacementProperty,
        image_slice: Option<DisplayImageSliceSpec>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplayMarginReplacement {
    side: DisplayMarginSide,
    content: DisplayMarginContent,
}

impl DisplayMarginReplacement {
    pub(crate) fn side(&self) -> DisplayMarginSide {
        self.side
    }

    pub(crate) fn content(&self) -> &DisplayMarginContent {
        &self.content
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DisplayMediaReplacementProperty {
    Image,
    Video,
    Xwidget(DisplayMediaReplacement),
    Webkit,
    /// Shader surface by id: like Xwidget, the spec is self-contained (the id
    /// was allocated by `neomacs-surface-create`), so layout resolves it
    /// without a display-host round-trip.
    Surface(DisplayMediaReplacement),
    /// Declarative shader surface (`:shader` source, no id): resolved through
    /// the display host like Video, memoized by spec content.
    SurfaceSource,
}

impl DisplayMediaReplacementProperty {
    pub(crate) fn direct_replacement(&self) -> Option<DisplayMediaReplacement> {
        match self {
            Self::Xwidget(media) | Self::Surface(media) => Some(*media),
            Self::Image | Self::Video | Self::Webkit | Self::SurfaceSource => None,
        }
    }

    pub(crate) fn accepts_media_replacement(&self, media: &DisplayMediaReplacement) -> bool {
        matches!(
            (self, media.kind),
            (
                Self::Image,
                crate::display_item::DisplayMediaReplacementKind::Image { .. }
                    | crate::display_item::DisplayMediaReplacementKind::EmptyImageSlice
            ) | (
                Self::Video,
                crate::display_item::DisplayMediaReplacementKind::Video { .. }
            ) | (
                Self::Webkit,
                crate::display_item::DisplayMediaReplacementKind::Xwidget { .. }
            ) | (
                Self::SurfaceSource,
                crate::display_item::DisplayMediaReplacementKind::Surface { .. }
            )
        )
    }

    pub(crate) fn uses_xwidget_cursor_extents(&self) -> bool {
        matches!(self, Self::Xwidget(_))
    }

    pub(crate) fn media_fallback_placeholder(&self) -> Option<&'static str> {
        match self {
            Self::Image => Some("[img]"),
            Self::Video | Self::Webkit | Self::SurfaceSource => Some("     "),
            Self::Xwidget(_) | Self::Surface(_) => None,
        }
    }
}

pub(crate) type DisplayTextPropertyModifiers = DisplayItemLayout;

/// Classify a `display` property value into a typed replacement + text-property
/// modifiers.
///
/// The SHAPE of the value (single spec / list of specs / vector of specs, minus a
/// `(disable-eval …)` wrapper) is decoded by `neovm_core`'s
/// [`DisplayPropertySpecs`] — GNU `handle_display_spec` — so this crate and the
/// interpreter cannot disagree about it. They did: this classifier had no VECTOR
/// arm, so `(put-text-property … 'display ["REPLACEMENT"])` rendered nothing.
///
/// GNU keeps the LAST element whose `handle_single_display_spec` reported a
/// replacement (`replacing = rv`) and merges non-replacement modifiers
/// (`raise`/`height`) from every element; both are reproduced here.
pub(crate) fn classify_display_property(value: Value) -> DisplayPropertyClassification {
    let mut result = DisplayPropertyClassification::default();
    DisplayPropertySpecs::of(value).for_each(|spec| {
        let element = classify_single_display_spec(spec);
        if element.replacement.is_some() {
            result.replacement = element.replacement;
            result.replacement_spec = element.replacement_spec;
        }
        merge_modifiers(&mut result.modifiers, element.modifiers);
        if element.image_slice.is_some() {
            result.image_slice = element.image_slice;
        }
        ControlFlow::Continue(())
    });
    // GNU suppresses inline modifiers once a replacement claims the text.
    if result.replacement.is_some() {
        result.modifiers = DisplayTextPropertyModifiers::default();
    }
    result
}

/// GNU ignores replacing display specs inside strings that themselves came from
/// a display property, while still honoring non-replacing modifiers.
pub(crate) fn classify_display_property_modifiers_only(
    value: Value,
) -> DisplayTextPropertyModifiers {
    let mut modifiers = DisplayTextPropertyModifiers::default();
    DisplayPropertySpecs::of(value).for_each(|spec| {
        merge_modifiers(&mut modifiers, classify_single_display_spec(spec).modifiers);
        ControlFlow::Continue(())
    });
    modifiers
}

/// Classify ONE display spec, matching GNU `handle_single_display_spec`'s arms.
///
/// The head taxonomy is `neovm_core`'s [`display_spec_kind`], so the arms are
/// exhaustive: a spec kind this crate forgets to render is a compile error rather
/// than a display property that silently does nothing.
fn classify_single_display_spec(value: Value) -> DisplayPropertyClassification {
    let kind = display_spec_kind(value);

    // `(when FORM . SPEC)`: GNU continues its SINGLE-spec arms on SPEC (it does
    // not re-enter `handle_display_spec`, so SPEC is never a list of specs), with
    // FORM evaluated. Resolved structurally here — the text is being displayed, so
    // the condition held — matching GNU's own `single_display_spec_string_p`.
    if matches!(kind, DisplaySpecKind::When) {
        return match display_spec_when_parts(value) {
            Some((form, spec)) if !form.is_nil() => classify_single_display_spec(spec),
            _ => DisplayPropertyClassification::default(),
        };
    }

    if matches!(kind, DisplaySpecKind::Margin) {
        return classify_margin_display_spec(value);
    }

    let replacement = match kind {
        DisplaySpecKind::Text => Some(DisplayReplacementProperty::String),
        DisplaySpecKind::Space => {
            parse_display_space(value).map(DisplayReplacementProperty::Stretch)
        }
        DisplaySpecKind::Image => Some(DisplayReplacementProperty::Media(
            DisplayMediaReplacementProperty::Image,
        )),
        DisplaySpecKind::Media(DisplayMediaSpecKind::Video) => Some(
            DisplayReplacementProperty::Media(DisplayMediaReplacementProperty::Video),
        ),
        DisplaySpecKind::Media(DisplayMediaSpecKind::Webkit) => Some(
            DisplayReplacementProperty::Media(DisplayMediaReplacementProperty::Webkit),
        ),
        DisplaySpecKind::Media(DisplayMediaSpecKind::Surface) => {
            // `:id` form resolves directly; the declarative `:shader` form is a
            // marker resolved through the display host (like Video).
            parse_display_surface_layout(&value)
                .map(|layout| {
                    DisplayReplacementProperty::Media(DisplayMediaReplacementProperty::Surface(
                        DisplayMediaReplacement::surface(DisplaySurfaceItem {
                            surface_id: layout.surface_id.min(i32::MAX as u32) as i32,
                            width: layout.width,
                            height: layout.height,
                        }),
                    ))
                })
                .or(Some(DisplayReplacementProperty::Media(
                    DisplayMediaReplacementProperty::SurfaceSource,
                )))
        }
        DisplaySpecKind::Xwidget => parse_display_xwidget_layout(&value).map(|layout| {
            DisplayReplacementProperty::Media(DisplayMediaReplacementProperty::Xwidget(
                DisplayMediaReplacement::xwidget(DisplayXwidgetItem {
                    xwidget_id: layout.xwidget_id,
                    webview_id: layout.webview_id,
                    width: layout.width,
                    height: layout.height,
                }),
            ))
        }),
        DisplaySpecKind::LeftFringe | DisplaySpecKind::RightFringe => {
            parse_display_fringe_layout(&value).map(DisplayReplacementProperty::Fringe)
        }
        // `((margin AREA) VALUE)`: the covered text is replaced only when VALUE is
        // itself something GNU can display (`valid_p` after stripping the margin
        // prefix). An AREA GNU rejects, or a VALUE that is neither string, image,
        // `(space …)` nor xwidget, leaves the text alone.
        // Handled above so the typed location and content cannot be discarded.
        DisplaySpecKind::Margin => None,
        // Modifiers and specs with no display effect: handled below.
        DisplaySpecKind::Height
        | DisplaySpecKind::SpaceWidth
        | DisplaySpecKind::MinWidth
        | DisplaySpecKind::Slice
        | DisplaySpecKind::Raise
        | DisplaySpecKind::KeywordPlist
        | DisplaySpecKind::Other => None,
        // Handled above, before the replacement match.
        DisplaySpecKind::When => None,
    };

    let modifiers = if replacement.is_some() {
        DisplayTextPropertyModifiers::default()
    } else {
        DisplayTextPropertyModifiers {
            raise: parse_display_raise_factor(value),
            height: parse_display_height_factor(value),
            space_width: parse_display_space_width_factor(value),
            break_after_row: false,
        }
    };

    DisplayPropertyClassification {
        replacement,
        replacement_spec: value,
        modifiers,
        image_slice: matches!(kind, DisplaySpecKind::Slice)
            .then(|| parse_display_image_slice(value))
            .flatten(),
    }
}

fn classify_margin_display_spec(value: Value) -> DisplayPropertyClassification {
    let Some(spec) = display_margin_spec(value) else {
        return DisplayPropertyClassification::default();
    };
    let inner = classify_single_display_spec(spec.content());

    // GNU's `((margin nil) CONTENT)` selects TEXT_AREA and is otherwise the
    // ordinary CONTENT replacement.  Preserve the inner classification rather
    // than manufacturing a marginal replacement that suppresses it.
    if spec.location() == DisplayMarginLocation::Text {
        return inner;
    }

    // GNU accepts only string/image/space/xwidget-class content after a margin
    // prefix.  Encoding that closed set here makes fringe/margin/modifier forms
    // unrepresentable as marginal content in later stages.
    let content = match inner.replacement {
        Some(DisplayReplacementProperty::String) => DisplayMarginContent::String(spec.content()),
        Some(DisplayReplacementProperty::Stretch(layout)) => DisplayMarginContent::Stretch {
            spec: spec.content(),
            layout,
        },
        Some(DisplayReplacementProperty::Media(replacement)) => DisplayMarginContent::Media {
            spec: spec.content(),
            replacement,
            image_slice: inner.image_slice,
        },
        Some(DisplayReplacementProperty::Fringe(_) | DisplayReplacementProperty::Margin(_))
        | None => return DisplayPropertyClassification::default(),
    };
    let side = match spec.location() {
        DisplayMarginLocation::Left => DisplayMarginSide::Left,
        DisplayMarginLocation::Right => DisplayMarginSide::Right,
        DisplayMarginLocation::Text => unreachable!("text location returned above"),
    };

    DisplayPropertyClassification {
        replacement: Some(DisplayReplacementProperty::Margin(
            DisplayMarginReplacement { side, content },
        )),
        replacement_spec: value,
        modifiers: DisplayTextPropertyModifiers::default(),
        image_slice: inner.image_slice,
    }
}

fn merge_modifiers(
    modifiers: &mut DisplayTextPropertyModifiers,
    element: DisplayTextPropertyModifiers,
) {
    if let Some(raise) = element.raise {
        modifiers.raise = Some(raise);
    }
    if let Some(height) = element.height {
        modifiers.height = Some(height);
    }
    if let Some(space_width) = element.space_width {
        modifiers.space_width = Some(space_width);
    }
}

fn parse_display_space(value: Value) -> Option<DisplayStretch> {
    let items = list_to_vec(&value)?;
    let mut width = None;
    let mut height = None;
    let mut ascent = None;
    let mut i = 1usize;
    while i + 1 < items.len() {
        let key = items[i];
        let val = items[i + 1];
        match DisplaySpaceKey::from_lisp_value(key) {
            Some(DisplaySpaceKey::Width | DisplaySpaceKey::RelativeWidth) => {
                width = parse_display_length(val).map(DisplayStretchWidth::Length);
            }
            Some(DisplaySpaceKey::AlignTo) => {
                // GNU only tests `!NILP (prop)` here (xdisp.c:32837); whether the
                // expression is computable is decided by
                // `calc_pixel_width_or_height` at resolve time, and an
                // uncomputable one falls back to the canonical char width
                // (xdisp.c:32879). Parsing it into a typed mirror here meant any
                // form the mirror could not model silently became a 1-column
                // space — issue #204's left-aligned image.
                if !val.is_nil() {
                    width = Some(DisplayStretchWidth::AlignTo(val));
                }
            }
            Some(DisplaySpaceKey::Height | DisplaySpaceKey::RelativeHeight) => {
                height = parse_display_length(val);
            }
            Some(DisplaySpaceKey::Ascent) => {
                ascent = parse_display_length(val);
            }
            None => {}
        }
        i += 2;
    }

    Some(DisplayStretch {
        width: width.unwrap_or(DisplayStretchWidth::Length(DisplayLength::Em(1.0))),
        height,
        ascent,
    })
}

fn parse_display_length(value: Value) -> Option<DisplayLength> {
    if let Some(number) = lisp_number(value) {
        return Some(DisplayLength::Em(number));
    }
    if value.is_nil() {
        return None;
    }
    // Keep the operand as Lisp for the single GNU-faithful evaluator.
    Some(DisplayLength::Expr(value))
}

fn parse_display_raise_factor(value: Value) -> Option<f32> {
    if value.is_cons() {
        let car = value.cons_car();
        let cdr = value.cons_cdr();
        if car.is_symbol_named("raise") {
            if cdr.is_cons() {
                return cdr.cons_car().as_number_f64().map(|factor| factor as f32);
            }
            return cdr.as_number_f64().map(|factor| factor as f32);
        }
    }

    plist_number(value, ":raise")
}

fn parse_display_space_width_factor(value: Value) -> Option<f32> {
    if value.is_cons() {
        let car = value.cons_car();
        let cdr = value.cons_cdr();
        if car.is_symbol_named("space-width") {
            let factor = if cdr.is_cons() {
                cdr.cons_car().as_number_f64()
            } else {
                cdr.as_number_f64()
            }? as f32;
            return (factor.is_finite() && factor > 0.0).then_some(factor);
        }
    }
    None
}

fn parse_display_height_factor(value: Value) -> Option<f32> {
    if value.is_cons() {
        let car = value.cons_car();
        let cdr = value.cons_cdr();
        if car.is_symbol_named("height") {
            if cdr.is_cons() {
                return cdr.cons_car().as_number_f64().map(|factor| factor as f32);
            }
            return cdr.as_number_f64().map(|factor| factor as f32);
        }
    }

    plist_number(value, ":height")
}

fn plist_number(value: Value, key_name: &str) -> Option<f32> {
    let items = list_to_vec(&value)?;
    let mut i = 0;
    while i + 1 < items.len() {
        if items[i].is_symbol_named(key_name) {
            return items[i + 1].as_number_f64().map(|factor| factor as f32);
        }
        i += 1;
    }
    None
}

fn lisp_number(value: Value) -> Option<f32> {
    value
        .as_float()
        .or_else(|| value.as_fixnum().map(|number| number as f64))
        .filter(|number| number.is_finite())
        .map(|number| number as f32)
}

#[cfg(test)]
#[path = "display_property_test.rs"]
mod tests;

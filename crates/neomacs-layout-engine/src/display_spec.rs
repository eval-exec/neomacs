//! Display property parsing for replacement glyphs.
//!
//! GNU xdisp treats display specs as a small typed domain: strings, images,
//! spaces, and xwidgets.  Neomacs adds native video and retains a temporary
//! WebKit convenience spec.  Keep symbol/plist parsing in this module so layout
//! code consumes typed requests instead of open-coding display-spec shapes.

use neovm_core::emacs_core::Value;
use neovm_core::emacs_core::eval::{
    ShaderSurfaceLanguage, SurfaceResolveRequest, WebKitResolveRequest, WebKitResolveSource,
};
use neovm_core::emacs_core::image::{
    ImageSpecKey, image_frame_index_from_lisp, image_mask_policy_from_items,
    image_resolve_source_from_items,
};
use neovm_core::emacs_core::image_catalog::{
    AxisSize, ImageColorContext, ImageFrameIndex, ImageMaskPolicy, ImageResolveRequest,
    ImageResolveSource, ImageRotation, ImageScaleEnvironment, ImageScalePolicy, ImageSizeSpec,
    ImageSpecIdentity, numeric_image_scale,
};
use neovm_core::emacs_core::value::{ValueKind, list_to_vec};
use neovm_core::emacs_core::video::{VideoDisplayReference, parse_video_display_reference};
use neovm_core::face::Color as LispColor;
use strum::{EnumString, IntoStaticStr};

use neomacs_display_protocol::{ImageSourceRect, WebViewId, XwidgetId};

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub(crate) enum DisplaySpecHead {
    Image,
    Video,
    Webkit,
    Xwidget,
    Surface,
}

impl DisplaySpecHead {
    pub(crate) fn is_head_of(self, value: &Value) -> bool {
        value.is_cons() && value.cons_car().is_symbol_named(self.into())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
#[strum(prefix = ":", serialize_all = "kebab-case")]
enum DisplayMediaKey {
    File,
    Uri,
    Width,
    Height,
    Loop,
    LoopCount,
    Autoplay,
    Opacity,
    Xwidget,
    Id,
    Shader,
    Glsl,
    Uniforms,
    Animate,
    Channel0,
    Fps,
}

impl DisplayMediaKey {
    fn from_lisp_value(value: Value) -> Option<Self> {
        value.as_symbol_name()?.strip_prefix(':')?.parse().ok()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DisplayImageLayout {
    request: UnresolvedDisplayImageRequest,
    pub(crate) scale: ImageScalePolicy,
    pub(crate) ascent: DisplayImageAscentPolicy,
    pub(crate) margin: DisplayImageMargin,
}

/// Parsed image request before GNU's face-relative dimensions become logical
/// pixels.  Keeping this type private prevents unresolved Lisp units from
/// leaking into the async catalog/renderer protocol.
#[derive(Clone, Debug)]
struct UnresolvedDisplayImageRequest {
    spec: ImageSpecIdentity,
    source: ImageResolveSource,
    size: DisplayImageSizeSpec,
    rotation: ImageRotation,
    colors: ImageColorContext,
    mask: ImageMaskPolicy,
    frame: ImageFrameIndex,
}

/// Active-face metrics used by GNU image dimensions `(N . em/ch/cw)`.
///
/// These are logical layout pixels. Device scaling belongs to
/// `ImageRealization` and is deliberately not represented here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayImageDimensionEnvironment {
    font_size: f32,
    character_height: f32,
    character_width: f32,
}

impl DisplayImageDimensionEnvironment {
    #[must_use]
    pub fn new(font_size: f32, character_height: f32, character_width: f32) -> Self {
        let valid = |value: f32| {
            if value.is_finite() && value > 0.0 {
                value
            } else {
                1.0
            }
        };
        Self {
            font_size: valid(font_size),
            character_height: valid(character_height),
            character_width: valid(character_width),
        }
    }

    fn base(self, unit: DisplayImageRelativeUnit) -> f64 {
        f64::from(match unit {
            DisplayImageRelativeUnit::Em => self.font_size,
            DisplayImageRelativeUnit::CharacterHeight => self.character_height,
            DisplayImageRelativeUnit::CharacterWidth => self.character_width,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DisplayImageRelativeUnit {
    Em,
    CharacterHeight,
    CharacterWidth,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DisplayImageDimension {
    Pixels(u32),
    Relative {
        factor: f64,
        unit: DisplayImageRelativeUnit,
    },
}

impl DisplayImageDimension {
    fn from_lisp(value: Value) -> Option<Self> {
        if let Some(pixels) = value.as_fixnum().filter(|pixels| *pixels >= 0) {
            return Some(Self::Pixels(pixels.min(i64::from(u32::MAX)) as u32));
        }
        if !value.is_cons() {
            return None;
        }
        let factor = value.cons_car().as_number_f64()?;
        if !factor.is_finite() || factor < 0.0 {
            return None;
        }
        let unit = match value.cons_cdr().as_symbol_name()? {
            "em" => DisplayImageRelativeUnit::Em,
            "ch" => DisplayImageRelativeUnit::CharacterHeight,
            "cw" => DisplayImageRelativeUnit::CharacterWidth,
            _ => return None,
        };
        Some(Self::Relative { factor, unit })
    }

    fn resolve(self, environment: DisplayImageDimensionEnvironment) -> u32 {
        match self {
            Self::Pixels(pixels) => pixels,
            Self::Relative { factor, unit } => {
                let pixels = factor * environment.base(unit);
                if pixels >= f64::from(u32::MAX) {
                    u32::MAX
                } else {
                    pixels.ceil() as u32
                }
            }
        }
    }
}

/// GNU's mutually-exclusive intent for one unresolved image axis.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum DisplayImageAxisSize {
    #[default]
    Native,
    Exact(DisplayImageDimension),
    AtMost(DisplayImageDimension),
}

impl DisplayImageAxisSize {
    fn resolve_precedence(
        target: Option<DisplayImageDimension>,
        at_most: Option<DisplayImageDimension>,
    ) -> Self {
        match (target, at_most) {
            (Some(target), _) => Self::Exact(target),
            (None, Some(at_most)) => Self::AtMost(at_most),
            (None, None) => Self::Native,
        }
    }

    fn into_protocol(self, environment: DisplayImageDimensionEnvironment) -> AxisSize {
        match self {
            Self::Native => AxisSize::Native,
            Self::Exact(dimension) => AxisSize::Exact(dimension.resolve(environment)),
            Self::AtMost(dimension) => AxisSize::AtMost(dimension.resolve(environment)),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct DisplayImageSizeSpec {
    width: DisplayImageAxisSize,
    height: DisplayImageAxisSize,
}

impl DisplayImageSizeSpec {
    fn resolve(self, environment: DisplayImageDimensionEnvironment) -> ImageSizeSpec {
        ImageSizeSpec::new(
            self.width.into_protocol(environment),
            self.height.into_protocol(environment),
        )
    }
}

/// One GNU `(slice X Y WIDTH HEIGHT)` operand. Fixnums are logical pixels,
/// floats are fractions of the realized image axis, and every other Lisp value
/// has GNU's unspecified/default meaning.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisplayImageSliceValue {
    Unspecified,
    Pixels(f32),
    Fraction(f32),
}

impl DisplayImageSliceValue {
    fn from_lisp(value: Value) -> Self {
        if let Some(pixels) = value.as_fixnum() {
            return Self::Pixels(pixels as f32);
        }
        value
            .as_float()
            .filter(|fraction| fraction.is_finite())
            .map(|fraction| Self::Fraction(fraction as f32))
            .unwrap_or(Self::Unspecified)
    }

    fn resolve(self, axis_extent: f32, default: f32) -> f32 {
        match self {
            Self::Unspecified => default,
            Self::Pixels(pixels) => pixels,
            // GNU assigns the product to an integer glyph-slice field.
            Self::Fraction(fraction) => (fraction * axis_extent).trunc(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayImageSliceSpec {
    x: DisplayImageSliceValue,
    y: DisplayImageSliceValue,
    width: DisplayImageSliceValue,
    height: DisplayImageSliceValue,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ResolvedDisplayImageSlice {
    pub(crate) source_rect: ImageSourceRect,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

impl DisplayImageSliceSpec {
    /// Resolve only after the catalog has supplied the realized image extent,
    /// matching GNU `produce_image_glyph` rather than guessing during parsing.
    pub(crate) fn resolve(
        self,
        image_width: f32,
        image_height: f32,
    ) -> Option<ResolvedDisplayImageSlice> {
        if !image_width.is_finite()
            || !image_height.is_finite()
            || image_width <= 0.0
            || image_height <= 0.0
        {
            return None;
        }
        let x = self.x.resolve(image_width, 0.0).clamp(0.0, image_width);
        let y = self.y.resolve(image_height, 0.0).clamp(0.0, image_height);
        let width = self
            .width
            .resolve(image_width, image_width)
            .max(0.0)
            .min(image_width - x);
        let height = self
            .height
            .resolve(image_height, image_height)
            .max(0.0)
            .min(image_height - y);
        let source_rect = ImageSourceRect::new(
            x / image_width,
            y / image_height,
            width / image_width,
            height / image_height,
        )?;
        Some(ResolvedDisplayImageSlice {
            source_rect,
            width,
            height,
        })
    }
}

pub(crate) fn parse_display_image_slice(value: Value) -> Option<DisplayImageSliceSpec> {
    let items = list_to_vec(&value)?;
    if items.first()?.as_symbol_name() != Some("slice") {
        return None;
    }
    let operand = |index| {
        items
            .get(index)
            .copied()
            .map(DisplayImageSliceValue::from_lisp)
            .unwrap_or(DisplayImageSliceValue::Unspecified)
    };
    Some(DisplayImageSliceSpec {
        x: operand(1),
        y: operand(2),
        width: operand(3),
        height: operand(4),
    })
}

impl DisplayImageLayout {
    pub(crate) fn into_resolve_request(
        self,
        environment: ImageScaleEnvironment,
        dimensions: DisplayImageDimensionEnvironment,
    ) -> ImageResolveRequest {
        ImageResolveRequest {
            spec: self.request.spec,
            source: self.request.source,
            size: self.request.size.resolve(dimensions),
            rotation: self.request.rotation,
            colors: self.request.colors,
            mask: self.request.mask,
            frame: self.request.frame,
            realization: environment.resolve(self.scale),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct DisplayImageMargin {
    pub(crate) horizontal: f32,
    pub(crate) vertical: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisplayImageAscentPolicy {
    Percent(f32),
    Center,
}

impl Default for DisplayImageAscentPolicy {
    fn default() -> Self {
        Self::Percent(50.0)
    }
}

impl DisplayImageAscentPolicy {
    pub(crate) fn resolve(self, image_height: f32, text_height: f32, text_ascent: f32) -> f32 {
        match self {
            Self::Percent(percent) => image_height * (percent / 100.0),
            Self::Center => {
                let text_descent = (text_height - text_ascent).max(0.0);
                ((image_height + text_ascent - text_descent + 1.0) / 2.0).floor()
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DisplayVideoLayout {
    pub(crate) reference: DisplayVideoReference,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) opacity: f32,
}

pub(crate) type DisplayVideoReference = VideoDisplayReference;

#[derive(Clone, Debug)]
pub(crate) struct DisplayWebKitLayout {
    pub(crate) request: WebKitResolveRequest,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct DisplayXwidgetLayout {
    pub(crate) xwidget_id: XwidgetId,
    pub(crate) webview_id: WebViewId,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

/// Parsed `(surface :id N [:width W] [:height H])` display spec. The id is a
/// host-allocated shader-surface handle from `neomacs-surface-create`; layout
/// needs no host round-trip (`docs/display-engine/SHADER_SURFACES.md`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplaySurfaceLayout {
    pub(crate) surface_id: u32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

/// Parsed declarative `(surface :shader WGSL [:uniforms ALIST] [:animate B]
/// [:width W] [:height H])` display spec: no Lisp-side id — the resolver
/// memoizes the request content into a host surface id, like `(video :file …)`.
/// `channel0_value` carries the raw `:channel0` value (surface id integer, or
/// an image/video spec); the resolver interprets it — resolution needs the
/// display host, which the parser deliberately does not see.
#[derive(Clone, Debug)]
pub(crate) struct DisplaySurfaceSourceLayout {
    pub(crate) request: SurfaceResolveRequest,
    pub(crate) channel0_value: Option<Value>,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

/// Which fringe a `(left-fringe …)` / `(right-fringe …)` display spec targets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DisplayFringeSide {
    Left,
    Right,
}

/// Parsed `(left-fringe BITMAP FACE)` / `(right-fringe BITMAP FACE)` display
/// spec. The bitmap is kept as the raw symbol `Value` (resolved to a registry
/// index later, where the evaluator is available); FACE is the optional face
/// symbol the spec requests (a `set-fringe-bitmap-face` override wins over it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayFringeLayout {
    pub(crate) bitmap: Value,
    pub(crate) side: DisplayFringeSide,
    pub(crate) face: Option<Value>,
}

/// Parse a `(left-fringe BITMAP [FACE])` / `(right-fringe BITMAP [FACE])` spec.
/// Returns `None` if the head is not a fringe symbol or BITMAP is missing.
pub(crate) fn parse_display_fringe_layout(value: &Value) -> Option<DisplayFringeLayout> {
    if !value.is_cons() {
        return None;
    }
    let side = match value.cons_car().as_symbol_name()? {
        "left-fringe" => DisplayFringeSide::Left,
        "right-fringe" => DisplayFringeSide::Right,
        _ => return None,
    };
    let items = list_to_vec(value)?;
    // items[0] = head, items[1] = BITMAP, items[2] = optional FACE.
    let bitmap = *items.get(1)?;
    if bitmap.is_nil() {
        return None;
    }
    let face = items.get(2).copied().filter(|face| !face.is_nil());
    Some(DisplayFringeLayout { bitmap, side, face })
}

pub(crate) fn parse_display_image_layout(
    prop_val: &Value,
    default_fg: u32,
    default_bg: u32,
) -> Option<DisplayImageLayout> {
    let items = list_to_vec(prop_val)?;
    if items.first()?.as_symbol_name() != Some(DisplaySpecHead::Image.into()) {
        return None;
    }

    let source = image_resolve_source_from_items(&items)?;
    let spec = ImageSpecIdentity::from_lisp_spec(prop_val)?;
    // Kept apart per GNU: `:width`/`:height` are targets, `:max-*` are clamps.
    let (mut width, mut max_width) = (None, None);
    let (mut height, mut max_height) = (None, None);
    let mut rotation = ImageRotation::None;
    let mut frame = ImageFrameIndex::default();
    // Absent `:scale` is NOT `:scale default` — see ImageScalePolicy.
    let mut scale = ImageScalePolicy::Unspecified;
    let mut ascent = DisplayImageAscentPolicy::default();
    let mut margin = DisplayImageMargin::default();
    let mut fg_color = default_fg;
    let mut bg_color = default_bg;

    let mut i = 1usize;
    while i + 1 < items.len() {
        let value = items[i + 1];
        match ImageSpecKey::from_lisp_value(items[i]) {
            Some(ImageSpecKey::File | ImageSpecKey::Data | ImageSpecKey::BaseUri) => {}
            Some(ImageSpecKey::Rotation) => {
                rotation = value
                    .as_number_f64()
                    .map(ImageRotation::from_degrees)
                    .unwrap_or(ImageRotation::None);
            }
            Some(ImageSpecKey::Index) => {
                if let Some(index) = image_frame_index_from_lisp(value) {
                    frame = index;
                }
            }
            Some(ImageSpecKey::Width) => width = DisplayImageDimension::from_lisp(value).or(width),
            Some(ImageSpecKey::MaxWidth) => {
                max_width = DisplayImageDimension::from_lisp(value).or(max_width)
            }
            Some(ImageSpecKey::Height) => {
                height = DisplayImageDimension::from_lisp(value).or(height)
            }
            Some(ImageSpecKey::MaxHeight) => {
                max_height = DisplayImageDimension::from_lisp(value).or(max_height)
            }
            Some(ImageSpecKey::Scale) => {
                scale = parse_image_scale(value).unwrap_or(scale);
            }
            Some(ImageSpecKey::Ascent) => {
                ascent = parse_image_ascent(value).unwrap_or(ascent);
            }
            Some(ImageSpecKey::Margin) => {
                margin = parse_image_margin(value).unwrap_or(margin);
            }
            Some(ImageSpecKey::Foreground) => {
                fg_color = parse_image_color_pixel(value).unwrap_or(fg_color);
            }
            Some(ImageSpecKey::Background) => {
                if let Some(pixel) = parse_image_color_pixel(value) {
                    bg_color = pixel;
                }
            }
            _ => {}
        }
        i += 2;
    }

    Some(DisplayImageLayout {
        request: UnresolvedDisplayImageRequest {
            spec,
            source,
            size: DisplayImageSizeSpec {
                width: DisplayImageAxisSize::resolve_precedence(width, max_width),
                height: DisplayImageAxisSize::resolve_precedence(height, max_height),
            },
            rotation,
            colors: ImageColorContext::from_pixels(fg_color, bg_color),
            mask: image_mask_policy_from_items(&items),
            frame,
        },
        scale,
        ascent,
        margin,
    })
}

fn parse_image_margin(value: Value) -> Option<DisplayImageMargin> {
    let component = |value: Value| {
        value
            .as_int()
            .filter(|value| *value >= 0)
            .map(|value| value as f32)
    };
    if let Some(margin) = component(value) {
        return Some(DisplayImageMargin {
            horizontal: margin,
            vertical: margin,
        });
    }
    if !value.is_cons() {
        return None;
    }
    Some(DisplayImageMargin {
        horizontal: component(value.cons_car())?,
        vertical: component(value.cons_cdr())?,
    })
}

pub(crate) fn parse_display_video_layout(
    prop_val: &Value,
    fallback_width: f32,
    fallback_height: f32,
) -> Option<DisplayVideoLayout> {
    let items = list_to_vec(prop_val)?;
    if items.first()?.as_symbol_name() != Some(DisplaySpecHead::Video.into()) {
        return None;
    }

    let mut width = fallback_width.max(1.0);
    let mut height = fallback_height.max(1.0);
    let mut opacity = 1.0;

    let mut i = 1usize;
    while i + 1 < items.len() {
        let value = items[i + 1];
        match DisplayMediaKey::from_lisp_value(items[i]) {
            Some(DisplayMediaKey::Width) => {
                if let Some(parsed) = parse_image_dimension(value) {
                    width = parsed.max(1) as f32;
                }
            }
            Some(DisplayMediaKey::Height) => {
                if let Some(parsed) = parse_image_dimension(value) {
                    height = parsed.max(1) as f32;
                }
            }
            Some(DisplayMediaKey::Opacity) => {
                if let Some(number) = value.as_number_f64().filter(|number| number.is_finite()) {
                    opacity = number.clamp(0.0, 1.0) as f32;
                }
            }
            _ => {}
        }
        i += 2;
    }

    let reference = parse_video_display_reference(&items, false)?;
    Some(DisplayVideoLayout {
        reference,
        width,
        height,
        opacity,
    })
}

pub(crate) fn parse_display_webkit_layout(
    prop_val: &Value,
    fallback_width: f32,
    fallback_height: f32,
) -> Option<DisplayWebKitLayout> {
    let items = list_to_vec(prop_val)?;
    if items.first()?.as_symbol_name() != Some(DisplaySpecHead::Webkit.into()) {
        return None;
    }

    let mut source = None;
    let mut width = fallback_width.max(1.0);
    let mut height = fallback_height.max(1.0);

    let mut i = 1usize;
    while i + 1 < items.len() {
        let value = items[i + 1];
        match DisplayMediaKey::from_lisp_value(items[i]) {
            Some(DisplayMediaKey::File) => {
                source = value
                    .as_lisp_string()
                    .cloned()
                    .map(WebKitResolveSource::File);
            }
            Some(DisplayMediaKey::Uri) => {
                source = value
                    .as_lisp_string()
                    .cloned()
                    .map(WebKitResolveSource::Uri);
            }
            Some(DisplayMediaKey::Width) => {
                if let Some(parsed) = parse_image_dimension(value) {
                    width = parsed.max(1) as f32;
                }
            }
            Some(DisplayMediaKey::Height) => {
                if let Some(parsed) = parse_image_dimension(value) {
                    height = parsed.max(1) as f32;
                }
            }
            _ => {}
        }
        i += 2;
    }

    Some(DisplayWebKitLayout {
        request: WebKitResolveRequest {
            source: source?,
            width: width.round().max(1.0) as u32,
            height: height.round().max(1.0) as u32,
        },
        width,
        height,
    })
}

/// Extract a host surface id from a display-spec value: a GC-managed surface
/// handle (`neomacs-surface-create`'s return value) or a plain non-negative
/// integer id (backward compatibility).
fn parse_surface_id_value(value: Value) -> Option<u32> {
    value
        .as_surface_handle()
        .or_else(|| value.as_int().filter(|id| *id >= 0).map(|id| id as u32))
}

/// Parse a `(surface :id HANDLE-OR-N [:width W] [:height H])` spec. `:id` is
/// required; missing dimensions fall back to a small visible square so a
/// typo'd spec shows up instead of vanishing.
pub(crate) fn parse_display_surface_layout(prop_val: &Value) -> Option<DisplaySurfaceLayout> {
    let items = list_to_vec(prop_val)?;
    if items.first()?.as_symbol_name() != Some(DisplaySpecHead::Surface.into()) {
        return None;
    }

    let mut surface_id = None;
    let mut width = 64.0f32;
    let mut height = 64.0f32;

    let mut i = 1usize;
    while i + 1 < items.len() {
        let value = items[i + 1];
        match DisplayMediaKey::from_lisp_value(items[i]) {
            Some(DisplayMediaKey::Id) => {
                surface_id = parse_surface_id_value(value);
            }
            Some(DisplayMediaKey::Width) => {
                if let Some(parsed) = parse_image_dimension(value) {
                    width = parsed.max(1) as f32;
                }
            }
            Some(DisplayMediaKey::Height) => {
                if let Some(parsed) = parse_image_dimension(value) {
                    height = parsed.max(1) as f32;
                }
            }
            _ => {}
        }
        i += 2;
    }

    Some(DisplaySurfaceLayout {
        surface_id: surface_id?,
        width: width.max(1.0),
        height: height.max(1.0),
    })
}

/// Parse a `(name . VALUE)` uniform alist entry into `(name, f32 bits, count)`.
/// VALUE is a number or a vector of 1..=4 numbers. Lenient (redisplay path):
/// malformed entries yield `None` and are skipped by the caller.
fn parse_surface_uniform_entry(entry: Value) -> Option<(String, [u32; 4], u8)> {
    if !entry.is_cons() {
        return None;
    }
    let name_value = entry.cons_car();
    let name = name_value.as_symbol_name().map(str::to_owned).or_else(|| {
        name_value
            .as_lisp_string()
            .and_then(|s| s.as_utf8_str().map(str::to_owned))
    })?;
    let value = entry.cons_cdr();
    let number = |v: Value| -> Option<f32> {
        match v.kind() {
            ValueKind::Fixnum(_) => v.as_int().map(|n| n as f32),
            ValueKind::Float => v.as_float().map(|f| f as f32),
            _ => None,
        }
    };
    let mut bits = [0u32; 4];
    let count;
    if let Some(scalar) = number(value) {
        bits[0] = scalar.to_bits();
        count = 1u8;
    } else if let Some(elements) = value.as_vector_data() {
        let elements = elements.as_slice();
        if elements.is_empty() || elements.len() > 4 {
            return None;
        }
        for (slot, element) in elements.iter().enumerate() {
            bits[slot] = number(*element)?.to_bits();
        }
        count = elements.len() as u8;
    } else {
        return None;
    }
    Some((name, bits, count))
}

/// Parse a declarative `(surface :shader WGSL …)` spec. Returns `None` when
/// the head is not `surface`, `:shader` is missing (the `:id` form is parsed
/// by [`parse_display_surface_layout`]), or the source is not a string.
pub(crate) fn parse_display_surface_source_layout(
    prop_val: &Value,
    fallback_width: f32,
    fallback_height: f32,
) -> Option<DisplaySurfaceSourceLayout> {
    let items = list_to_vec(prop_val)?;
    if items.first()?.as_symbol_name() != Some(DisplaySpecHead::Surface.into()) {
        return None;
    }

    let mut source = None;
    let mut language = ShaderSurfaceLanguage::Wgsl;
    let mut uniforms = Vec::new();
    let mut width = fallback_width.max(1.0);
    let mut height = fallback_height.max(1.0);
    let mut animate = true;
    let mut channel0 = None;
    let mut fps = None;

    let mut i = 1usize;
    while i + 1 < items.len() {
        let value = items[i + 1];
        match DisplayMediaKey::from_lisp_value(items[i]) {
            Some(DisplayMediaKey::Shader) => {
                source = value
                    .as_lisp_string()
                    .and_then(|s| s.as_utf8_str().map(str::to_owned));
                language = ShaderSurfaceLanguage::Wgsl;
            }
            Some(DisplayMediaKey::Glsl) => {
                source = value
                    .as_lisp_string()
                    .and_then(|s| s.as_utf8_str().map(str::to_owned));
                language = ShaderSurfaceLanguage::Glsl;
            }
            Some(DisplayMediaKey::Uniforms) => {
                if let Some(entries) = list_to_vec(&value) {
                    uniforms.extend(entries.into_iter().filter_map(parse_surface_uniform_entry));
                }
            }
            Some(DisplayMediaKey::Animate) => {
                animate = parse_boolish(value);
            }
            Some(DisplayMediaKey::Channel0) => {
                channel0 = (!value.is_nil()).then_some(value);
            }
            Some(DisplayMediaKey::Fps) => {
                fps = parse_image_dimension(value).filter(|n| *n > 0);
            }
            Some(DisplayMediaKey::Width) => {
                if let Some(parsed) = parse_image_dimension(value) {
                    width = parsed.max(1) as f32;
                }
            }
            Some(DisplayMediaKey::Height) => {
                if let Some(parsed) = parse_image_dimension(value) {
                    height = parsed.max(1) as f32;
                }
            }
            _ => {}
        }
        i += 2;
    }

    Some(DisplaySurfaceSourceLayout {
        request: SurfaceResolveRequest {
            language,
            source: source?,
            uniforms,
            width: width.round().max(1.0) as u32,
            height: height.round().max(1.0) as u32,
            animate,
            fps,
            channel0: None,
        },
        channel0_value: channel0,
        width,
        height,
    })
}

pub(crate) fn parse_display_xwidget_layout(prop_val: &Value) -> Option<DisplayXwidgetLayout> {
    let items = list_to_vec(prop_val)?;
    if items.first()?.as_symbol_name() != Some(DisplaySpecHead::Xwidget.into()) {
        return None;
    }

    let mut xwidget = None;
    let mut i = 1usize;
    while i + 1 < items.len() {
        if DisplayMediaKey::from_lisp_value(items[i]) == Some(DisplayMediaKey::Xwidget) {
            xwidget = items[i + 1].as_xwidget();
            break;
        }
        i += 2;
    }

    let xwidget = xwidget?;
    Some(DisplayXwidgetLayout {
        xwidget_id: XwidgetId::new(xwidget.xwidget_id),
        webview_id: xwidget.webview_id,
        width: xwidget.width.max(0) as f32,
        height: xwidget.height.max(0) as f32,
    })
}

fn parse_image_dimension(value: Value) -> Option<u32> {
    match value.kind() {
        ValueKind::Fixnum(_) => Some(value.as_int()?.max(0) as u32),
        ValueKind::Float => Some(value.as_float()?.max(0.0).round() as u32),
        _ => None,
    }
}

fn parse_image_scale(value: Value) -> Option<ImageScalePolicy> {
    if value.is_symbol_named("default") {
        return Some(ImageScalePolicy::Default);
    }
    Some(ImageScalePolicy::Explicit(numeric_image_scale(value)?))
}

fn parse_image_ascent(value: Value) -> Option<DisplayImageAscentPolicy> {
    if value.is_symbol_named("center") {
        return Some(DisplayImageAscentPolicy::Center);
    }
    let percent = match value.kind() {
        ValueKind::Fixnum(_) => value.as_int()? as f32,
        _ => return None,
    };
    (percent.is_finite() && (0.0..=100.0).contains(&percent))
        .then_some(DisplayImageAscentPolicy::Percent(percent))
}

fn parse_image_color_pixel(value: Value) -> Option<u32> {
    let color = value
        .as_lisp_string()
        .and_then(|name| LispColor::parse(name.as_utf8_str()?))?;
    Some(((color.r as u32) << 16) | ((color.g as u32) << 8) | color.b as u32)
}

fn parse_boolish(value: Value) -> bool {
    !value.is_nil()
}

pub(crate) use neovm_core::emacs_core::display_spec::DisplaySpaceKey;

pub(crate) fn is_display_space_spec(value: &Value) -> bool {
    value.is_cons() && value.cons_car().is_symbol_named("space")
}

/// Note: the `left-fringe`/`right-fringe` DISPLAY SPEC (a cons headed by that
/// symbol, classified by `neovm_core`'s `display_spec_kind`) is distinct from the
/// `left-fringe`/`right-fringe` *length units* ([`DisplayLengthSymbol`]) parsed
/// here, which only ever appear as a bare symbol or inside a `space`
/// `:width`/`:align-to` pixel expression — never as the head of a spec.
pub(crate) fn display_space_positive_number(value: Value) -> Option<f32> {
    value
        .as_float()
        .or_else(|| value.as_int().map(|integer| integer as f64))
        .filter(|number| number.is_finite() && *number > 0.0)
        .map(|number| number as f32)
}

#[cfg(test)]
#[path = "display_spec/tests/display_spec_test.rs"]
mod tests;

#[cfg(test)]
#[path = "display_spec_surface_test.rs"]
mod surface_tests;

#[cfg(test)]
#[path = "display_spec_image_test.rs"]
mod image_tests;

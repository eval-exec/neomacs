//! Image type support builtins.
//!
//! Provides stub/partial implementations of Emacs image builtins:
//! - `image-type-available-p` — check if image type is available
//! - `create-image` — create image descriptor (property list)
//! - `image-size` — return (WIDTH . HEIGHT) cons
//! - `image-mask-p` — check for mask support
//! - `put-image` / `insert-image` / `remove-images` — display stubs
//! - `image-flush` / `clear-image-cache` — cache management stubs
//! - `image-type` — extract type from image spec
//! - `display-images-p` / `image-transforms-p` — capability queries
//!
//! Image specs are property lists: (:type png :file "foo.png" :width 100 ...)

use super::error::{EvalResult, Flow, signal};
use super::value::*;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_args_range, expect_max_args, expect_min_args};
use crate::emacs_core::eval::Context;
use crate::emacs_core::image_catalog::{
    AxisSize, ImageAnimationInvalidation, ImageColorContext, ImageDataSource, ImageFrameIndex,
    ImageHeuristicMask, ImageInvalidation, ImageMaskKind, ImageMaskPolicy, ImageResolveRequest,
    ImageResolveSource, ImageRotation, ImageScaleEnvironment, ImageScalePolicy, ImageSizeSpec,
    ImageSpecIdentity, image_scale_environment, numeric_image_scale,
};
use crate::window::FRAME_ID_BASE;
use strum::{EnumString, IntoStaticStr};

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn expect_frame_designator(_name: &str, value: &Value) -> Result<(), Flow> {
    match value.kind() {
        ValueKind::Nil => Ok(()),
        ValueKind::Fixnum(id) if id >= 0 && (id as u64) >= FRAME_ID_BASE => Ok(()),
        ValueKind::Veclike(VecLikeType::Frame) if value.as_frame_id().unwrap() >= FRAME_ID_BASE => {
            Ok(())
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *value],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn integer_or_marker_p(value: &Value) -> bool {
    value.as_int().is_some() || value.is_marker()
}

// ---------------------------------------------------------------------------
// Property list helpers
// ---------------------------------------------------------------------------

/// Get a value from a property list by keyword.
/// The plist is a flat list: (:key1 val1 :key2 val2 ...).
#[cfg(test)]
fn plist_get(plist: &Value, key: &Value) -> Value {
    let mut cursor = *plist;
    loop {
        match cursor.kind() {
            ValueKind::Cons => {
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                if eq_value(&pair_car, key) {
                    // Next element is the value.
                    match pair_cdr.kind() {
                        ValueKind::Cons => {
                            return pair_cdr.cons_car();
                        }
                        _ => return Value::NIL,
                    }
                }
                // Skip the value entry.
                match pair_cdr.kind() {
                    ValueKind::Cons => {
                        cursor = pair_cdr.cons_cdr();
                    }
                    _ => return Value::NIL,
                }
            }
            _ => return Value::NIL,
        }
    }
}

/// GNU image types available in this build.
///
/// GNU keeps these in `image_types[]` in `src/image.c`; optional entries are
/// present only when their backing decoder is built.  Neomacs currently ships
/// the portable decoder set below and deliberately leaves optional GNU entries
/// such as `postscript`, `imagemagick`, and `native-image` unavailable until
/// their loaders are implemented.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum ImageType {
    Svg,
    Webp,
    Png,
    Gif,
    Tiff,
    Jpeg,
    Xpm,
    Xbm,
    Pbm,
}

impl ImageType {
    const AVAILABLE_IN_GNU_LIST_ORDER: [Self; 9] = [
        Self::Svg,
        Self::Webp,
        Self::Png,
        Self::Gif,
        Self::Tiff,
        Self::Jpeg,
        Self::Xpm,
        Self::Xbm,
        Self::Pbm,
    ];

    pub fn from_symbol_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    pub fn from_file_extension(extension: &str) -> Option<Self> {
        ImageFilenameType::from_file_extension(extension)?.available_type()
    }

    pub fn from_file_name(path: &str) -> Option<Self> {
        ImageFilenameType::from_file_name(path)?.available_type()
    }

    pub fn name(self) -> &'static str {
        self.into()
    }

    pub fn value(self) -> Value {
        Value::symbol(self.name())
    }
}

/// Image types recognized by GNU filename inference.
///
/// GNU `lisp/image.el:image-type-file-name-regexps` has a wider domain than
/// `image-type-available-p`: unavailable loaders such as `bmp`, `postscript`,
/// and `heic` can still be inferred from file names for diagnostics and later
/// availability checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum ImageFilenameType {
    Png,
    Gif,
    Jpeg,
    Webp,
    Bmp,
    Xpm,
    Pbm,
    Xbm,
    Postscript,
    Tiff,
    Svg,
    Heic,
}

impl ImageFilenameType {
    fn from_file_extension(extension: &str) -> Option<Self> {
        let extension = extension
            .strip_prefix('.')
            .unwrap_or(extension)
            .to_ascii_lowercase();
        match extension.as_str() {
            "jpg" => Some(Self::Jpeg),
            "tif" => Some(Self::Tiff),
            "svgz" => Some(Self::Svg),
            "ps" => Some(Self::Postscript),
            "heics" | "heif" | "heifs" => Some(Self::Heic),
            name => name.parse().ok(),
        }
    }

    fn from_file_name(path: &str) -> Option<Self> {
        Self::from_file_extension(path.rsplit('.').next()?)
    }

    fn name(self) -> &'static str {
        self.into()
    }

    fn available_type(self) -> Option<ImageType> {
        ImageType::from_symbol_name(self.name())
    }
}

/// Exact image specification plist keys.
///
/// GNU `src/image.c:valid_image_p` and `image_spec_value` compare plist keys
/// with the keyword symbols such as `:type`, not with bare symbols like
/// `type`.  Keep this parser exact so invalid image specs don't become valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(prefix = ":", serialize_all = "kebab-case")]
pub enum ImageSpecKey {
    Type,
    File,
    Data,
    Width,
    MaxWidth,
    Height,
    MaxHeight,
    Scale,
    Foreground,
    Background,
    Ascent,
    Margin,
    Relief,
    Conversion,
    ColorSymbols,
    HeuristicMask,
    Index,
    Crop,
    Rotation,
    Matrix,
    TransformSmoothing,
    ColorAdjustment,
    Mask,
    Flip,
    Loader,
    PtWidth,
    PtHeight,
    BaseUri,
    Css,
    AnimateBuffer,
    AnimateTardiness,
    AnimatePosition,
    Format,
}

/// Parse GNU's non-negative `:index` domain without narrowing an Emacs fixnum.
/// Decoders decide whether that frame exists; parsers only preserve intent.
pub fn image_frame_index_from_lisp(value: Value) -> Option<ImageFrameIndex> {
    value
        .as_fixnum()
        .and_then(|index| u64::try_from(index).ok())
        .map(ImageFrameIndex::new)
}

impl ImageSpecKey {
    pub fn from_lisp_value(value: Value) -> Option<Self> {
        Self::from_keyword(value.as_symbol_name()?)
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

    pub fn is_value(self, value: Value) -> bool {
        value.as_symbol_name() == Some(self.keyword())
    }
}

/// Decode the mutually exclusive image source capability once for every image
/// consumer. A file source wins over in-memory data, and `:base-uri` has an
/// effect only when paired with `:data`; this prevents layout and image
/// builtins from constructing different catalog keys for the same Lisp spec.
pub fn image_resolve_source_from_items(items: &[Value]) -> Option<ImageResolveSource> {
    let mut file_source = None;
    let mut data_source = None;
    let mut base_uri = None;
    let mut index = 1;
    while index + 1 < items.len() {
        let value = items[index + 1];
        match ImageSpecKey::from_lisp_value(items[index]) {
            Some(ImageSpecKey::File) => file_source = value.as_lisp_string().cloned(),
            Some(ImageSpecKey::Data) => {
                data_source = value.as_lisp_string().map(|data| data.as_bytes().to_vec());
            }
            Some(ImageSpecKey::BaseUri) => base_uri = value.as_lisp_string().cloned(),
            _ => {}
        }
        index += 2;
    }
    match (file_source, data_source, base_uri) {
        (Some(path), _, _) => Some(ImageResolveSource::File(path)),
        (None, Some(data), Some(base_uri)) => {
            Some(ImageResolveSource::Data(ImageDataSource::WithBaseUri {
                data,
                base_uri,
            }))
        }
        (None, Some(data), None) => Some(ImageResolveSource::Data(ImageDataSource::Isolated(data))),
        (None, None, _) => None,
    }
}

/// AREA argument accepted by GNU `put-image` and `insert-image`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum ImageArea {
    LeftMargin,
    RightMargin,
}

impl ImageArea {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn from_symbol_value(value: Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }
}

/// GNU's `PATH_BITMAPS`, the `--bitmapdir` configure value that
/// `src/epaths.in:67` defaults to and `src/epaths.h:67` carries into the build.
///
/// It is a compiled-in path, not a probe of the running system: GNU installs
/// the same list on a machine that has no `/usr/include/X11/bitmaps` at all.
const PATH_BITMAPS: &str = "/usr/include/X11/bitmaps";

/// GNU's `decode_env_path (NULL, defalt, false)` (`src/emacs.c:3262-3300`),
/// restricted to the no-environment-variable case `syms_of_image` uses.
///
/// With `evarname == 0` nothing is read from the environment, so the whole
/// function is "split `defalt` on `SEPCHAR` and turn each element into a
/// string, substituting `.` for an empty one" -- the `empty` argument is false
/// at this call site, which is what selects `.` rather than nil.
fn decode_env_path_default(defalt: &str) -> Value {
    Value::list(
        defalt
            .split(':')
            .map(|element| Value::string(if element.is_empty() { "." } else { element }))
            .collect(),
    )
}

/// The Neomacs counterpart of GNU's `syms_of_image` (`src/image.c:13024`).
///
/// GNU compiles `syms_of_image` under `#ifdef HAVE_WINDOW_SYSTEM`
/// (`src/emacs.c:2364-2366`), which every window-system build satisfies, and
/// `lisp/cus-start.el` probes the same fact with `(fboundp 'x-create-frame)` --
/// the test `lisp/loadup.el` also uses to decide whether to preload `image.el`.
/// Neomacs answers `t` to it, so these variables must exist here, and with
/// GNU's initializers: a `DEFVAR_LISP` supplies the value and the
/// `declared_special` bit in one statement, so a bound-but-not-special
/// placeholder is a state GNU cannot be in.
pub fn register_bootstrap_vars(obarray: &mut crate::emacs_core::symbol::Obarray) {
    // image.c:13265, `Vx_bitmap_file_path = decode_env_path (0, PATH_BITMAPS, 0)'.
    obarray.define_special_variable("x-bitmap-file-path", decode_env_path_default(PATH_BITMAPS));
    // image.c:13034 DEFVAR_LISP, make_float (MAX_IMAGE_SIZE) = 10.0.
    obarray.define_special_variable("max-image-size", Value::make_float(10.0));
    // image.c:13269 DEFVAR_LISP, make_fixnum (300).
    obarray.define_special_variable("image-cache-eviction-delay", Value::fixnum(300));
    // image.c:13279 DEFVAR_LISP, Qauto.
    obarray.define_special_variable("image-scaling-factor", Value::symbol("auto"));
    // image.c:13028 DEFVAR_LISP; GNU's define_image_type fills the list at
    // C init, our equivalent enumerates the built-in decoders.
    obarray.define_special_variable("image-types", supported_image_types_value());
}

pub(crate) fn supported_image_types_value() -> Value {
    Value::list(
        ImageType::AVAILABLE_IN_GNU_LIST_ORDER
            .iter()
            .copied()
            .map(|image_type| Value::symbol(image_type.name()))
            .collect(),
    )
}

/// Check whether a symbol name represents a supported image type.
pub(crate) fn is_supported_image_type(name: &str) -> bool {
    ImageType::from_symbol_name(name).is_some()
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn normalize_image_type_name(name: &str) -> Option<&'static str> {
    ImageFilenameType::from_file_extension(name).map(ImageFilenameType::name)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn validate_image_area(area: Value) -> Result<(), Flow> {
    if area.is_nil() || ImageArea::from_symbol_value(area).is_some() {
        return Ok(());
    }
    let rendered = super::print::print_value(&area);
    Err(signal(
        "error",
        vec![Value::string(format!("Invalid area {rendered}"))],
    ))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn infer_image_type_from_filename(path: &str) -> Option<&'static str> {
    ImageFilenameType::from_file_name(path).map(ImageFilenameType::name)
}

fn parse_image_dimension(value: Value) -> Option<u32> {
    match value.kind() {
        ValueKind::Fixnum(_) => Some(value.as_int()?.max(0) as u32),
        ValueKind::Float => Some(value.as_float()?.max(0.0).round() as u32),
        _ => None,
    }
}

/// GNU image margins around the decoded bitmap (`:margin` / `:relief`).
///
/// `Fimage_size` reports `width + 2*hmargin` and `height + 2*vmargin`
/// (`src/image.c`). Relief magnitude is added into both margins.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ImageSpecMargins {
    hmargin: i64,
    vmargin: i64,
}

impl ImageSpecMargins {
    /// Parse `:margin` and `:relief` from an image SPEC plist (GNU rules).
    fn from_image_spec(spec: &Value) -> Self {
        let Some(items) = list_to_vec(spec) else {
            return Self::default();
        };
        let mut hmargin = 0i64;
        let mut vmargin = 0i64;
        let mut relief = 0i64;
        let mut i = 1usize;
        while i + 1 < items.len() {
            let value = items[i + 1];
            match ImageSpecKey::from_lisp_value(items[i]) {
                Some(ImageSpecKey::Margin) => {
                    if let Some(n) = value.as_int() {
                        let n = n.max(0);
                        hmargin = n;
                        vmargin = n;
                    } else if value.is_cons() {
                        // (H . V) or (H V)
                        let car = value.cons_car();
                        let cdr = value.cons_cdr();
                        if let Some(h) = car.as_int() {
                            hmargin = h.max(0);
                        }
                        if cdr.is_cons() {
                            if let Some(v) = cdr.cons_car().as_int() {
                                vmargin = v.max(0);
                            }
                        } else if let Some(v) = cdr.as_int() {
                            vmargin = v.max(0);
                        }
                    }
                }
                Some(ImageSpecKey::Relief) => {
                    if let Some(n) = value.as_int() {
                        relief = n;
                    }
                }
                _ => {}
            }
            i += 2;
        }
        // GNU: hmargin/vmargin += abs(relief)
        let relief_abs = relief.unsigned_abs() as i64;
        Self {
            hmargin: hmargin.saturating_add(relief_abs),
            vmargin: vmargin.saturating_add(relief_abs),
        }
    }

    fn add_to_pixel_size(self, width: i64, height: i64) -> (i64, i64) {
        (
            width.saturating_add(self.hmargin.saturating_mul(2)),
            height.saturating_add(self.vmargin.saturating_mul(2)),
        )
    }
}

pub(crate) fn image_resolve_request_from_spec(
    spec: &Value,
    environment: ImageScaleEnvironment,
    default_colors: (u32, u32),
) -> Option<ImageResolveRequest> {
    let items = list_to_vec(spec)?;
    if items.first()?.as_symbol_name() != Some("image") {
        return None;
    }

    let spec = ImageSpecIdentity::from_lisp_spec(spec)?;
    let source = image_resolve_source_from_items(&items)?;
    // GNU keeps these four apart: `:width`/`:height` are targets that override
    // their `:max-` counterpart, `:max-width`/`:max-height` are clamps.
    let (mut width, mut max_width) = (None, None);
    let (mut height, mut max_height) = (None, None);
    let mut rotation = ImageRotation::None;
    let mut frame = ImageFrameIndex::default();
    // Absent `:scale` is NOT `:scale default` — see ImageScalePolicy.
    let mut scale = ImageScalePolicy::Unspecified;

    let mut i = 1usize;
    while i + 1 < items.len() {
        let value = items[i + 1];
        match ImageSpecKey::from_lisp_value(items[i]) {
            Some(ImageSpecKey::File | ImageSpecKey::Data | ImageSpecKey::BaseUri) => {}
            Some(ImageSpecKey::Rotation) => {
                // GNU signals `Invalid image ':rotation' parameter` for a
                // non-number and leaves the image upright (src/image.c:2921).
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
            Some(ImageSpecKey::Width) => width = parse_image_dimension(value).or(width),
            Some(ImageSpecKey::MaxWidth) => max_width = parse_image_dimension(value).or(max_width),
            Some(ImageSpecKey::Height) => height = parse_image_dimension(value).or(height),
            Some(ImageSpecKey::MaxHeight) => {
                max_height = parse_image_dimension(value).or(max_height)
            }
            Some(ImageSpecKey::Scale) if value.is_symbol_named("default") => {
                scale = ImageScalePolicy::Default;
            }
            Some(ImageSpecKey::Scale) => {
                scale = numeric_image_scale(value)
                    .map(ImageScalePolicy::Explicit)
                    .unwrap_or(scale);
            }
            _ => {}
        }
        i += 2;
    }

    Some(ImageResolveRequest {
        spec,
        source,
        // GNU looks each key up independently, so precedence is by key rather
        // than by plist order.
        size: ImageSizeSpec::new(
            AxisSize::resolve(width, max_width),
            AxisSize::resolve(height, max_height),
        ),
        rotation,
        // GNU keys the image cache on the face's colors, and `Fimage_size`
        // resolves through `DEFAULT_FACE_ID` (image.c `lookup_image`). Using
        // zeros here gave the same spec a different key than the one layout
        // builds from the resolved face, so every measured image decoded twice.
        colors: ImageColorContext::from_pixels(default_colors.0, default_colors.1),
        mask: image_mask_policy_from_items(&items),
        frame,
        realization: environment.resolve(scale),
    })
}

fn image_mask_rgb16(value: Value) -> Option<[u16; 3]> {
    let components = list_to_vec(&value)?;
    let [red, green, blue] = components.as_slice() else {
        return None;
    };
    let component = |value: &Value| {
        value
            .as_fixnum()
            .filter(|value| *value >= 0)
            .map(|value| (value as u64 & u64::from(u16::MAX)) as u16)
    };
    Some([component(red)?, component(green)?, component(blue)?])
}

fn heuristic_mask_argument(value: Value) -> Option<ImageHeuristicMask> {
    if value.is_symbol_named("heuristic") {
        return Some(ImageHeuristicMask::FourCorners);
    }
    if !value.is_cons() || !value.cons_car().is_symbol_named("heuristic") {
        return None;
    }
    let tail = value.cons_cdr();
    let background = if tail.is_cons() {
        tail.cons_car()
    } else {
        tail
    };
    Some(
        image_mask_rgb16(background)
            .map(ImageHeuristicMask::Rgb16)
            .unwrap_or(ImageHeuristicMask::FourCorners),
    )
}

/// Reduce GNU's open Lisp `:mask` and legacy `:heuristic-mask` values to the
/// complete postprocessing policy understood by image decoders.
pub fn image_mask_policy_from_items(items: &[Value]) -> ImageMaskPolicy {
    let mut heuristic_mask = None;
    let mut mask = None;
    let mut mask_present = false;
    let mut index = 1;
    while index + 1 < items.len() {
        let value = items[index + 1];
        match ImageSpecKey::from_lisp_value(items[index]) {
            Some(ImageSpecKey::HeuristicMask) if heuristic_mask.is_none() => {
                heuristic_mask = Some(value);
            }
            Some(ImageSpecKey::Mask) if !mask_present => {
                mask = Some(value);
                mask_present = true;
            }
            _ => {}
        }
        index += 2;
    }

    if let Some(how) = heuristic_mask.filter(|value| !value.is_nil()) {
        return ImageMaskPolicy::Heuristic(
            image_mask_rgb16(how)
                .map(ImageHeuristicMask::Rgb16)
                .unwrap_or(ImageHeuristicMask::FourCorners),
        );
    }
    match mask {
        Some(value) if value.is_nil() => ImageMaskPolicy::Suppress,
        Some(value) => heuristic_mask_argument(value)
            .map(ImageMaskPolicy::Heuristic)
            .unwrap_or(ImageMaskPolicy::Preserve),
        None => ImageMaskPolicy::Preserve,
    }
}

pub(crate) fn image_scale_environment_for_frame(
    eval: &Context,
    frame_arg: Option<&Value>,
) -> Option<ImageScaleEnvironment> {
    let frame = image_frame_for_arg(eval, frame_arg)?;
    Some(image_scale_environment(frame, eval.obarray()))
}

/// Resolve FRAME (or the selected frame) for image builtins.
fn image_frame_for_arg<'a>(
    eval: &'a Context,
    frame_arg: Option<&Value>,
) -> Option<&'a crate::window::Frame> {
    if let Some(frame_arg) = frame_arg.filter(|value| !value.is_nil()) {
        let frame_id = match frame_arg.kind() {
            ValueKind::Fixnum(id) => crate::window::FrameId(id as u64),
            ValueKind::Veclike(VecLikeType::Frame) => {
                crate::window::FrameId(frame_arg.as_frame_id()?)
            }
            _ => return None,
        };
        eval.frame_manager().get(frame_id)
    } else {
        eval.frame_manager().selected_frame()
    }
}

/// Canonical character cell size in pixels for FRAME, matching
/// `frame-char-width` / `frame-char-height` (GNU `FRAME_COLUMN_WIDTH` /
/// `FRAME_LINE_HEIGHT` used by `Fimage_size`).
fn image_frame_char_cell_pixels(eval: &Context, frame_arg: Option<&Value>) -> Option<(f64, f64)> {
    let frame = image_frame_for_arg(eval, frame_arg)?;
    // Same truncation as `builtin_frame_char_width` / `builtin_frame_char_height`.
    let column_width = (frame.char_width as i64).max(1) as f64;
    let line_height = (frame.char_height as i64).max(1) as f64;
    Some((column_width, line_height))
}

/// Validate that a value looks like an image spec.
/// Oracle-compatible shape:
/// - list starts with symbol `image`
/// - plist includes a supported symbolic `:type`
/// - plist includes exactly one source key: `:file` or `:data`
/// - source value is a string
/// Whether VALUE is an image specification accepted by `imagep`.
/// Redisplay uses the same validation before accepting a replacement.
pub fn is_image_spec(value: &Value) -> bool {
    let items = match list_to_vec(value) {
        Some(v) => v,
        None => return false,
    };

    if items.is_empty() || items[0].as_symbol_name() != Some("image") {
        return false;
    }

    let mut type_seen = false;
    let mut type_ok = false;
    let mut file_seen = false;
    let mut file_ok = false;
    let mut data_seen = false;
    let mut data_ok = false;

    let mut i = 1usize;
    while i + 1 < items.len() {
        if let Some(key) = ImageSpecKey::from_lisp_value(items[i]) {
            let val = &items[i + 1];
            match key {
                ImageSpecKey::Type if !type_seen => {
                    type_seen = true;
                    type_ok = val.as_symbol_name().is_some_and(is_supported_image_type);
                }
                ImageSpecKey::File if !file_seen => {
                    file_seen = true;
                    file_ok = val.is_string();
                }
                ImageSpecKey::Data if !data_seen => {
                    data_seen = true;
                    data_ok = val.is_string();
                }
                _ => {}
            }
        }
        i += 2;
    }

    if !type_seen || !type_ok {
        return false;
    }

    match (file_seen, data_seen) {
        (true, false) => file_ok,
        (false, true) => data_ok,
        _ => false,
    }
}

/// Extract the plist portion of an image spec.
/// If the spec starts with `image`, skip that first element.
#[cfg(test)]
fn image_spec_plist(spec: &Value) -> Value {
    let items = match list_to_vec(spec) {
        Some(v) => v,
        None => return Value::NIL,
    };
    if items.is_empty() {
        return Value::NIL;
    }
    if let Some(name) = items[0].as_symbol_name() {
        if name == "image" {
            // Plist is everything after the `image` symbol.
            return Value::list(items[1..].to_vec());
        }
    }
    // Already a bare plist.
    *spec
}

// ---------------------------------------------------------------------------
// Pure builtins
// ---------------------------------------------------------------------------

/// (image-type-available-p TYPE) -> t or nil
///
/// Return t if image type TYPE is available in this Emacs instance.
/// Supported types: png, jpeg, gif, svg, webp, xpm, xbm, pbm, tiff, bmp.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_image_type_available_p(args: Vec<Value>) -> EvalResult {
    expect_args("image-type-available-p", &args, 1)?;
    let type_name = match args[0].as_symbol_name() {
        Some(name) => name.to_string(),
        None => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), args[0]],
            ));
        }
    };
    Ok(Value::bool_val(is_supported_image_type(&type_name)))
}

/// (create-image FILE-OR-DATA &optional TYPE DATA-P &rest PROPS) -> image descriptor
///
/// Create an image descriptor (a list starting with `image`).
/// FILE-OR-DATA is a file name string or raw data string.
/// TYPE is a symbol like `png`, `jpeg`, etc.
/// DATA-P if non-nil means FILE-OR-DATA is raw image data, not a file name.
/// PROPS are additional property-list pairs (e.g. :width 100 :height 200).
///
/// Returns: (image :type TYPE :file FILE-OR-DATA ... PROPS)
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_create_image(args: Vec<Value>) -> EvalResult {
    expect_min_args("create-image", &args, 1)?;

    let file_or_data = args[0];
    let data_p = args.len() > 2 && args[2].is_truthy();

    // TYPE argument (optional).
    let image_type = if args.len() > 1 && !args[1].is_nil() {
        match args[1].as_symbol_name() {
            Some(name) => {
                let normalized = normalize_image_type_name(name).unwrap_or(name);
                Value::symbol(normalized)
            }
            None => {
                let rendered = super::print::print_value(&args[1]);
                return Err(signal(
                    "error",
                    vec![Value::string(format!("Invalid image type `{rendered}`"))],
                ));
            }
        }
    } else {
        let inferred = if data_p {
            None
        } else {
            file_or_data
                .as_utf8_str()
                .and_then(infer_image_type_from_filename)
                .map(str::to_string)
        };
        match inferred {
            Some(name) => Value::symbol(name),
            None => Value::NIL,
        }
    };

    // Build the image spec property list.
    let mut spec_items: Vec<Value> = Vec::new();
    spec_items.push(Value::symbol("image"));
    spec_items.push(Value::keyword("type"));
    spec_items.push(image_type);

    if data_p {
        spec_items.push(Value::keyword("data"));
        spec_items.push(file_or_data);
    } else {
        spec_items.push(Value::keyword("file"));
        spec_items.push(file_or_data);
    }

    // Emacs adds :scale default on freshly created image specs.
    spec_items.push(Value::keyword("scale"));
    spec_items.push(Value::symbol("default"));

    // Append any extra PROPS (starting from index 3).
    if args.len() > 3 {
        for prop in &args[3..] {
            spec_items.push(*prop);
        }
    }

    Ok(Value::list(spec_items))
}

/// (image-size SPEC &optional PIXELS FRAME) -> (WIDTH . HEIGHT)
///
/// Batch/no-window semantics:
/// - invalid SPEC -> `(error "Invalid image specification")`
/// - valid SPEC in batch -> `(error "Window system frame should be used")`
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_image_size(args: Vec<Value>) -> EvalResult {
    expect_min_args("image-size", &args, 1)?;
    expect_max_args("image-size", &args, 3)?;

    if !is_image_spec(&args[0]) {
        return Err(signal(
            "error",
            vec![Value::string("Invalid image specification")],
        ));
    }
    Err(signal(
        "error",
        vec![Value::string("Window system frame should be used")],
    ))
}

/// `(image-size SPEC &optional PIXELS FRAME)` → `(WIDTH . HEIGHT)`.
///
/// Mirrors GNU `Fimage_size` (`src/image.c`): after resolving the image on
/// FRAME, return pixel size as fixnums when PIXELS is non-nil; otherwise
/// return size in canonical character units as floats
/// (`width / FRAME_COLUMN_WIDTH`, `height / FRAME_LINE_HEIGHT`).
pub(crate) fn builtin_image_size_in_context(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("image-size", &args, 1)?;
    expect_max_args("image-size", &args, 3)?;

    if !is_image_spec(&args[0]) {
        return Err(signal(
            "error",
            vec![Value::string("Invalid image specification")],
        ));
    }

    require_image_window_system_frame(eval, "image-size", args.get(2))?;

    let environment = image_scale_environment_for_frame(eval, args.get(2)).ok_or_else(|| {
        signal(
            "error",
            vec![Value::string("Window system frame should be used")],
        )
    })?;
    let Some(display_host) = eval.display_host.as_ref() else {
        return Err(signal(
            "error",
            vec![Value::string("Window system frame should be used")],
        ));
    };
    let Some(request) = image_resolve_request_from_spec(
        &args[0],
        environment,
        eval.face_table().default_face_colors(),
    ) else {
        return Err(signal(
            "error",
            vec![Value::string("Invalid image specification")],
        ));
    };

    let resolved = display_host
        .resolve_image_sync(request)
        .map_err(|message| signal("error", vec![Value::string(message)]))?;
    let Some(image) = resolved else {
        return Err(signal(
            "error",
            vec![Value::string("Invalid image specification")],
        ));
    };

    // Dual-extent metadata: `width`/`height` are logical layout pixels (char
    // units and redisplay); `pixel_width`/`pixel_height` are GNU Fimage_size
    // PIXELS space, filled at decode from `ImageRealization::report_scale`.
    // Margins/relief are added in each space separately so HiDPI `:scale
    // default` does not re-scale absolute margin integers.
    let margins = ImageSpecMargins::from_image_spec(&args[0]);
    let pixels = args.get(1).copied().unwrap_or(Value::NIL);
    if !pixels.is_nil() {
        let (width_px, height_px) = margins.add_to_pixel_size(
            i64::from(image.metadata.reported.width()),
            i64::from(image.metadata.reported.height()),
        );
        return Ok(Value::cons(
            Value::fixnum(width_px.max(1)),
            Value::fixnum(height_px.max(1)),
        ));
    }

    let (layout_w, layout_h) = margins.add_to_pixel_size(
        i64::from(image.metadata.layout.width()),
        i64::from(image.metadata.layout.height()),
    );
    let (column_width, line_height) =
        image_frame_char_cell_pixels(eval, args.get(2)).ok_or_else(|| {
            signal(
                "error",
                vec![Value::string("Window system frame should be used")],
            )
        })?;
    Ok(Value::cons(
        Value::make_float(layout_w as f64 / column_width),
        Value::make_float(layout_h as f64 / line_height),
    ))
}

/// (image-mask-p SPEC &optional FRAME) -> nil
///
/// Batch/no-window semantics:
/// - invalid SPEC -> `(error "Invalid image specification")`
/// - valid SPEC in batch -> `(error "Window system frame should be used")`
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_image_mask_p(args: Vec<Value>) -> EvalResult {
    expect_min_args("image-mask-p", &args, 1)?;
    expect_max_args("image-mask-p", &args, 2)?;

    if !is_image_spec(&args[0]) {
        return Err(signal(
            "error",
            vec![Value::string("Invalid image specification")],
        ));
    }
    Err(signal(
        "error",
        vec![Value::string("Window system frame should be used")],
    ))
}

fn image_frame_window_system(
    eval: &mut Context,
    builtin: &str,
    frame_arg: Option<&Value>,
) -> Result<Option<Value>, Flow> {
    if let Some(frame) = frame_arg {
        expect_frame_designator(builtin, frame)?;
    }

    if let Some(frame_arg) = frame_arg.filter(|value| !value.is_nil()) {
        let frame_id = match frame_arg.kind() {
            ValueKind::Fixnum(id) => crate::window::FrameId(id as u64),
            ValueKind::Veclike(VecLikeType::Frame) => {
                crate::window::FrameId(frame_arg.as_frame_id().expect("checked frame"))
            }
            _ => unreachable!("expect_frame_designator checked frame argument"),
        };
        Ok(eval
            .frames
            .get(frame_id)
            .and_then(|frame| frame.effective_window_system()))
    } else {
        Ok(eval
            .frames
            .selected_frame()
            .and_then(|frame| frame.effective_window_system()))
    }
}

fn require_image_window_system_frame(
    eval: &mut Context,
    builtin: &str,
    frame_arg: Option<&Value>,
) -> Result<(), Flow> {
    let frame_window_system = image_frame_window_system(eval, builtin, frame_arg)?;
    if frame_window_system
        .is_none_or(|window_system| !super::display::gui_window_system_active_value(window_system))
    {
        return Err(signal(
            "error",
            vec![Value::string("Window system frame should be used")],
        ));
    }
    Ok(())
}

pub(crate) fn builtin_image_mask_p_in_context(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("image-mask-p", &args, 1)?;
    expect_max_args("image-mask-p", &args, 2)?;

    if !is_image_spec(&args[0]) {
        return Err(signal(
            "error",
            vec![Value::string("Invalid image specification")],
        ));
    }

    require_image_window_system_frame(eval, "image-mask-p", args.get(1))?;

    let environment = image_scale_environment_for_frame(eval, args.get(1)).ok_or_else(|| {
        signal(
            "error",
            vec![Value::string("Window system frame should be used")],
        )
    })?;
    let Some(display_host) = eval.display_host.as_ref() else {
        return Err(signal(
            "error",
            vec![Value::string("Window system frame should be used")],
        ));
    };
    let Some(request) = image_resolve_request_from_spec(
        &args[0],
        environment,
        eval.face_table().default_face_colors(),
    ) else {
        return Err(signal(
            "error",
            vec![Value::string("Invalid image specification")],
        ));
    };

    // Prefer a sync resolve so mask/transparency reflects decoded state, not a
    // pending catalog probe. GNU inspects `img->mask` after `lookup_image`.
    let resolved = display_host
        .resolve_image_sync(request)
        .map_err(|message| signal("error", vec![Value::string(message)]))?;
    let Some(image) = resolved else {
        return Ok(Value::NIL);
    };
    Ok(Value::bool_val(image.metadata.mask.has_clipping_mask()))
}

/// (put-image IMAGE POINT &optional STRING AREA) -> nil
///
/// Display IMAGE at POINT in the current buffer as an overlay.
/// Stub: does nothing, returns nil.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_put_image(args: Vec<Value>) -> EvalResult {
    expect_min_args("put-image", &args, 2)?;
    expect_max_args("put-image", &args, 4)?;

    // Validate that first arg looks like an image spec.
    if !is_image_spec(&args[0]) {
        let rendered = super::print::print_value(&args[0]);
        return Err(signal(
            "error",
            vec![Value::string(format!("Not an image: {rendered}"))],
        ));
    }

    // Validate POINT is integer-or-marker in batch.
    if !integer_or_marker_p(&args[1]) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), args[1]],
        ));
    }

    if args.len() > 3 && !args[3].is_nil() {
        validate_image_area(args[3])?;
    }

    // Batch compatibility: return a truthy placeholder for inserted overlay.
    Ok(Value::T)
}

/// (insert-image IMAGE &optional STRING AREA SLICE) -> nil
///
/// Insert IMAGE into the current buffer at point.
/// Batch stub: validates IMAGE and returns t.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_insert_image(args: Vec<Value>) -> EvalResult {
    expect_min_args("insert-image", &args, 1)?;
    expect_max_args("insert-image", &args, 5)?;

    if !is_image_spec(&args[0]) {
        let rendered = super::print::print_value(&args[0]);
        return Err(signal(
            "error",
            vec![Value::string(format!("Not an image: {rendered}"))],
        ));
    }

    if args.len() > 2 && !args[2].is_nil() {
        validate_image_area(args[2])?;
    }

    Ok(Value::T)
}

/// (remove-images START END &optional BUFFER) -> nil
///
/// Remove images between START and END in BUFFER.
/// Stub: does nothing, returns nil.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_remove_images(args: Vec<Value>) -> EvalResult {
    expect_min_args("remove-images", &args, 2)?;
    expect_max_args("remove-images", &args, 3)?;

    // Validate START and END are integer-or-marker in batch.
    if !integer_or_marker_p(&args[0]) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), args[0]],
        ));
    }
    if !integer_or_marker_p(&args[1]) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), args[1]],
        ));
    }

    // Stub: no-op.
    Ok(Value::NIL)
}

/// (image-flush SPEC &optional FRAME) -> nil
///
/// Flush the image cache for image SPEC.
/// Batch semantics:
/// - invalid SPEC -> `(error "Invalid image specification")`
/// - FRAME = t -> nil (all-frames path)
/// - otherwise -> `(error "Window system frame should be used")`
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_image_flush(args: Vec<Value>) -> EvalResult {
    expect_min_args("image-flush", &args, 1)?;
    expect_max_args("image-flush", &args, 2)?;

    if !is_image_spec(&args[0]) {
        return Err(signal(
            "error",
            vec![Value::string("Invalid image specification")],
        ));
    }

    if let Some(frame) = args.get(1) {
        if frame.is_t() {
            return Ok(Value::NIL);
        }
        if !frame.is_nil() {
            expect_frame_designator("image-flush", frame)?;
        }
    }

    Err(signal(
        "error",
        vec![Value::string("Window system frame should be used")],
    ))
}

pub(crate) fn builtin_image_flush_in_context(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("image-flush", &args, 1)?;
    expect_max_args("image-flush", &args, 2)?;

    if !is_image_spec(&args[0]) {
        return Err(signal(
            "error",
            vec![Value::string("Invalid image specification")],
        ));
    }

    // GNU `FRAME t` flushes SPEC on every frame. Neomacs keeps one shared
    // catalog, so invalidating the source once is the all-frames path.
    let all_frames = args.get(1).is_some_and(|value| value.is_t());
    if !all_frames {
        require_image_window_system_frame(eval, "image-flush", args.get(1))?;
    } else if eval.display_host.is_none() {
        // Batch/no-host: accept FRAME=t without work (historic batch contract).
        return Ok(Value::NIL);
    }

    if eval.display_host.is_none() {
        return Err(signal(
            "error",
            vec![Value::string("Window system frame should be used")],
        ));
    }

    let frame_for_env = if all_frames { None } else { args.get(1) };
    let default_colors = eval.face_table().default_face_colors();
    let Some(request) = image_resolve_request_from_spec(
        &args[0],
        image_scale_environment_for_frame(eval, frame_for_env).unwrap_or_default(),
        default_colors,
    ) else {
        return Err(signal(
            "error",
            vec![Value::string("Invalid image specification")],
        ));
    };
    let invalidated = eval
        .display_host
        .as_ref()
        .and_then(|host| host.image_catalog())
        .is_some_and(|catalog| {
            catalog
                .invalidate(ImageInvalidation::Spec { spec: request.spec })
                .changed()
        });
    if invalidated {
        eval.invalidate_media();
    }

    Ok(Value::NIL)
}

/// `(clear-image-cache &optional FILTER ANIMATION-FILTER)` → nil.
///
/// Mirrors GNU `Fclear_image_cache` (`src/image.c`):
/// - non-nil `ANIMATION-FILTER` must be a list; clear only the matching
///   decoder/compositor sequence
/// - `FILTER` nil or a frame → clear the selected/that frame's images
/// - `FILTER` t → clear all frames
/// - other `FILTER` (usually a filename string) → clear images depending on it
///
/// Neomacs uses one shared image catalog, so frame-scoped clears clear the
/// whole catalog (equivalent for a single GUI display).
#[allow(dead_code)] // batch path kept for tests without a display host
pub(crate) fn builtin_clear_image_cache(args: Vec<Value>) -> EvalResult {
    expect_max_args("clear-image-cache", &args, 2)?;
    // Batch/no-host: historical tests expect an error for empty/nil filter.
    // GUI work goes through `builtin_clear_image_cache_in_context`.
    if args.len() == 2 {
        let animation_cache = &args[1];
        if !animation_cache.is_nil() && !animation_cache.is_cons() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("listp"), *animation_cache],
            ));
        }
        if !animation_cache.is_nil() {
            // GNU prunes only the animation cache and leaves image caches alone.
            return Ok(Value::NIL);
        }
    }

    let filter = args.first().copied().unwrap_or(Value::NIL);
    if filter.is_nil() {
        return Err(signal(
            "error",
            vec![Value::string("Window system frame should be used")],
        ));
    }
    if filter.is_t() || filter.as_utf8_str().is_some() {
        return Ok(Value::NIL);
    }
    expect_frame_designator("clear-image-cache", &filter)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_clear_image_cache_in_context(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("clear-image-cache", &args, 2)?;

    if args.len() == 2 {
        let animation_filter = &args[1];
        if !animation_filter.is_nil() && !animation_filter.is_cons() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("listp"), *animation_filter],
            ));
        }
        if !animation_filter.is_nil() {
            let source = list_to_vec(animation_filter)
                .as_deref()
                .and_then(image_resolve_source_from_items);
            if let Some(source) = source
                && let Some(catalog) = eval
                    .display_host
                    .as_ref()
                    .and_then(|host| host.image_catalog())
            {
                catalog.invalidate_animation(ImageAnimationInvalidation::Source(source));
            }
            return Ok(Value::NIL);
        }
    }

    let filter = args.first().copied().unwrap_or(Value::NIL);

    // Filename / other non-frame filter: clear matching sources on all frames.
    if !filter.is_nil() && !filter.is_t() && !is_frame_designator_value(&filter) {
        if let Some(path) = filter.as_utf8_str() {
            require_image_display_host(eval)?;
            let invalidated = eval
                .display_host
                .as_ref()
                .and_then(|host| host.image_catalog())
                .is_some_and(|catalog| {
                    catalog
                        .invalidate(ImageInvalidation::Dependency(ImageResolveSource::File(
                            crate::heap_types::LispString::from_utf8(path),
                        )))
                        .changed()
                });
            if invalidated {
                eval.invalidate_media();
            }
            if let Some(catalog) = eval
                .display_host
                .as_ref()
                .and_then(|host| host.image_catalog())
            {
                catalog.invalidate_animation(ImageAnimationInvalidation::All);
            }
            return Ok(Value::NIL);
        }
        // Unknown filter object: accept without clearing (no dependency match).
        if let Some(catalog) = eval
            .display_host
            .as_ref()
            .and_then(|host| host.image_catalog())
        {
            catalog.invalidate_animation(ImageAnimationInvalidation::All);
        }
        return Ok(Value::NIL);
    }

    if filter.is_t() {
        require_image_display_host(eval)?;
    } else if filter.is_nil() {
        require_image_window_system_frame(eval, "clear-image-cache", None)?;
        require_image_display_host(eval)?;
    } else {
        require_image_window_system_frame(eval, "clear-image-cache", Some(&filter))?;
        require_image_display_host(eval)?;
    }

    let invalidated = eval
        .display_host
        .as_ref()
        .and_then(|host| host.image_catalog())
        .is_some_and(|catalog| catalog.invalidate(ImageInvalidation::All).changed());
    if invalidated {
        eval.invalidate_media();
    }
    if let Some(catalog) = eval
        .display_host
        .as_ref()
        .and_then(|host| host.image_catalog())
    {
        catalog.invalidate_animation(ImageAnimationInvalidation::All);
    }

    Ok(Value::NIL)
}

fn is_frame_designator_value(value: &Value) -> bool {
    matches!(
        value.kind(),
        ValueKind::Fixnum(id) if id >= 0 && (id as u64) >= FRAME_ID_BASE
    ) || matches!(value.kind(), ValueKind::Veclike(VecLikeType::Frame))
}

/// Neomacs shares one image catalog across GUI frames; presence of a display
/// host is the practical gate for cache mutation (like a live window-system).
fn require_image_display_host(eval: &Context) -> Result<(), Flow> {
    if eval.display_host.is_none() {
        return Err(signal(
            "error",
            vec![Value::string("Window system frame should be used")],
        ));
    }
    Ok(())
}

/// (image-cache-size) -> integer
///
/// Without a display host this is 0. With a catalog, return the renderer's
/// resident texture plus decoded-sequence storage snapshot (see
/// [`ImageCatalog::cached_size_bytes`]).
#[allow(dead_code)] // batch path; live registration uses the context variant
pub(crate) fn builtin_image_cache_size(args: Vec<Value>) -> EvalResult {
    expect_args("image-cache-size", &args, 0)?;
    Ok(Value::fixnum(0))
}

pub(crate) fn builtin_image_cache_size_in_context(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("image-cache-size", &args, 0)?;
    let bytes = eval
        .display_host
        .as_ref()
        .and_then(|host| host.image_catalog())
        .map(|catalog| catalog.cached_size_bytes())
        .unwrap_or(0);
    Ok(Value::fixnum(bytes.max(0)))
}

/// (image-metadata SPEC &optional FRAME) -> metadata object or nil
///
/// Returns nil for non-image specifications. For valid image specs on
/// non-window-system frames, this signals the same error shape as GNU Emacs.
#[allow(dead_code)] // batch-only path used by unit tests without a display host
pub(crate) fn builtin_image_metadata(args: Vec<Value>) -> EvalResult {
    expect_args_range("image-metadata", &args, 1, 2)?;

    if !is_image_spec(&args[0]) {
        return Ok(Value::NIL);
    }

    if let Some(frame) = args.get(1) {
        expect_frame_designator("image-metadata", frame)?;
    }

    Err(signal(
        "error",
        vec![Value::string("Window system frame should be used")],
    ))
}

fn image_embedded_metadata_to_lisp(
    metadata: &crate::emacs_core::image_catalog::ImageEmbeddedMetadata,
) -> Value {
    let mut plist = Vec::with_capacity(4);
    if let Some(count) = metadata.frame_count() {
        plist.push(Value::symbol("count"));
        plist.push(Value::fixnum(i64::from(count)));
    }
    if let Some(delay) = metadata.frame_delay() {
        plist.push(Value::symbol("delay"));
        plist.push(match delay {
            crate::emacs_core::image_catalog::ImageFrameDelay::UseDefault => Value::T,
            crate::emacs_core::image_catalog::ImageFrameDelay::Milliseconds { .. } => {
                Value::make_float(delay.seconds().expect("numeric delay has seconds"))
            }
        });
    }
    Value::list(plist)
}

/// GUI path for `image-metadata`: like GNU, resolve SPEC (for the same
/// `lookup_image` caching side effect) and return only decoder-owned metadata.
/// Dimensions remain on the Neomacs-specific `neomacs-image-extent` API.
pub(crate) fn builtin_image_metadata_in_context(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("image-metadata", &args, 1, 2)?;
    if !is_image_spec(&args[0]) {
        return Ok(Value::NIL);
    }
    if let Some(frame) = args.get(1) {
        expect_frame_designator("image-metadata", frame)?;
    }
    let environment = image_scale_environment_for_frame(eval, args.get(1)).ok_or_else(|| {
        signal(
            "error",
            vec![Value::string("Window system frame should be used")],
        )
    })?;
    let Some(request) = image_resolve_request_from_spec(
        &args[0],
        environment,
        eval.face_table().default_face_colors(),
    ) else {
        return Ok(Value::NIL);
    };
    let display_host = eval.display_host.as_ref().ok_or_else(|| {
        signal(
            "error",
            vec![Value::string("Window system frame should be used")],
        )
    })?;
    let resolved = display_host
        .resolve_image_sync(request)
        .map_err(|message| signal("error", vec![Value::string(message)]))?;
    Ok(resolved
        .map(|image| image_embedded_metadata_to_lisp(&image.metadata.embedded))
        .unwrap_or(Value::NIL))
}

/// (neomacs-image-extent SPEC &optional FRAME) -> plist or nil
///
/// Neomacs-only companion to `image-metadata`: surfaces the resolved
/// dual-extent geometry as (:width :height :pixel-width :pixel-height
/// :background-transparent). pixel-width/pixel-height are the GNU
/// Fimage_size pixel space (differs from layout under :scale default on
/// HiDPI). Returns nil for non-image specs or unresolved images.
pub(crate) fn builtin_neomacs_image_extent_in_context(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("neomacs-image-extent", &args, 1, 2)?;

    if !is_image_spec(&args[0]) {
        return Ok(Value::NIL);
    }

    require_image_window_system_frame(eval, "neomacs-image-extent", args.get(1))?;
    require_image_display_host(eval)?;

    let environment = image_scale_environment_for_frame(eval, args.get(1)).ok_or_else(|| {
        signal(
            "error",
            vec![Value::string("Window system frame should be used")],
        )
    })?;
    let Some(request) = image_resolve_request_from_spec(
        &args[0],
        environment,
        eval.face_table().default_face_colors(),
    ) else {
        return Ok(Value::NIL);
    };
    let display_host = eval.display_host.as_ref().expect("checked host");
    let resolved = display_host
        .resolve_image_sync(request)
        .map_err(|message| signal("error", vec![Value::string(message)]))?;
    let Some(image) = resolved else {
        return Ok(Value::NIL);
    };

    // Surface dual extents: layout for redisplay, pixel-* for GNU Fimage_size
    // PIXELS space (differs under `:scale default` on HiDPI).
    Ok(Value::list(vec![
        Value::keyword("width"),
        Value::fixnum(i64::from(image.metadata.layout.width())),
        Value::keyword("height"),
        Value::fixnum(i64::from(image.metadata.layout.height())),
        Value::keyword("pixel-width"),
        Value::fixnum(i64::from(image.metadata.reported.width())),
        Value::keyword("pixel-height"),
        Value::fixnum(i64::from(image.metadata.reported.height())),
        Value::keyword("background-transparent"),
        Value::bool_val(image.metadata.background_transparent),
        Value::keyword("mask-kind"),
        Value::symbol(match image.metadata.mask {
            ImageMaskKind::None => "none",
            ImageMaskKind::Clipping => "clipping",
            ImageMaskKind::AlphaChannel => "alpha-channel",
        }),
    ]))
}

/// (imagep OBJECT) -> t if OBJECT looks like an image descriptor.
pub(crate) fn builtin_imagep(args: Vec<Value>) -> EvalResult {
    expect_args("imagep", &args, 1)?;
    Ok(Value::bool_val(is_image_spec(&args[0])))
}

/// (image-type SOURCE &optional TYPE DATA-P) -> symbol
///
/// Compatibility behavior:
/// - SOURCE must be a file name string.
/// - TYPE, when non-nil, must be a symbol and is returned (normalized aliases).
/// - Without TYPE, type is inferred from file extension.
/// - If type inference fails, signal `unknown-image-type`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_image_type(args: Vec<Value>) -> EvalResult {
    expect_min_args("image-type", &args, 1)?;
    expect_max_args("image-type", &args, 3)?;

    let source = &args[0];
    let explicit_type = args.get(1).cloned().unwrap_or(Value::NIL);
    let data_p = args.get(2).cloned().unwrap_or(Value::NIL);

    if source.as_utf8_str().is_none() {
        let rendered = super::print::print_value(source);
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "Invalid image file name `{rendered}`"
            ))],
        ));
    }

    let resolved = if explicit_type.is_nil() {
        if data_p.is_truthy() {
            None
        } else {
            source
                .as_utf8_str()
                .and_then(infer_image_type_from_filename)
                .map(str::to_string)
        }
    } else {
        let rendered = super::print::print_value(&explicit_type);
        let sym_name = explicit_type.as_symbol_name().ok_or_else(|| {
            signal(
                "error",
                vec![Value::string(format!("Invalid image type `{rendered}`"))],
            )
        })?;
        Some(
            normalize_image_type_name(sym_name)
                .unwrap_or(sym_name)
                .to_string(),
        )
    };

    let Some(resolved) = resolved else {
        return Err(signal(
            "unknown-image-type",
            vec![Value::list(vec![Value::string(
                "Cannot determine image type",
            )])],
        ));
    };

    Ok(Value::symbol(resolved))
}

/// A native image transform a window-system frame can perform.
///
/// GNU reports these from `image-transforms-p` (src/image.c:12843) so Lisp can
/// choose between native transforms and ImageMagick. Which ones a build offers
/// varies — GNU's own Windows build reports `scale` alone when rotation is
/// unavailable (image.c:12865) — so this is a set, not a boolean.
///
/// Add a variant only when the image pipeline actually performs it: callers
/// treat this as a promise and stop supplying their own fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum ImageTransform {
    /// Resize to `:scale` / `:width` / `:height`. Implemented by
    /// `ImageScalePolicy` and `ImageRealization` in the layout engine.
    Scale,
    /// Rotate by integral multiples of 90 degrees (`:rotation`). Implemented by
    /// `ImageRotation` in the decode path.
    Rotate90,
}

impl ImageTransform {
    /// The transforms neomacs performs today, in GNU's report order.
    const SUPPORTED: [Self; 2] = [Self::Scale, Self::Rotate90];

    pub fn name(self) -> &'static str {
        self.into()
    }

    pub fn value(self) -> Value {
        Value::symbol(self.name())
    }
}

/// `(image-transforms-p &optional FRAME)` -> list of capabilities, or nil.
///
/// GNU gates this on `FRAME_WINDOW_P` (src/image.c:12855): a TTY frame has no
/// native transforms and reports nil. A window-system frame reports the list
/// of transforms the build can perform — never `t`.
pub(crate) fn builtin_image_transforms_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("image-transforms-p", &args, 1)?;
    if let Some(frame_or_display) = args.first() {
        expect_frame_designator("image-transforms-p", frame_or_display)?;
    }
    // GNU decodes the FRAME and tests `FRAME_WINDOW_P` — it does not fall back
    // to the global `window-system`, the way the `display-*-p` predicates do.
    let window_system = super::display::frame_window_system_symbol(eval, args.first())?
        .is_some_and(|value| value.is_symbol());
    if !window_system {
        return Ok(Value::NIL);
    }
    Ok(Value::list(
        ImageTransform::SUPPORTED
            .iter()
            .map(|transform| transform.value())
            .collect(),
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;

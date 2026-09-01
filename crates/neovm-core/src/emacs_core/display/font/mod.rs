//! GNU Emacs `font.c` surface: font builtins for the Elisp interpreter.
//!
//! - `fontp`, `font-spec`, `font-get`, `font-put`, `list-fonts`, `find-font`,
//!   `clear-font-cache`, `font-family-list`, `font-xlfd-name`, `font-at`,
//!   `font-info`, `query-font`, `font-shape-gstring`, `font-get-glyphs`,
//!   `font-has-char-p`, `font-match-p`, `font-variation-glyphs`,
//!   `internal-char-font`
//!
//! The xfaces.c builtin surface (internal-*-lisp-face*, colors, face-id,
//! face-font) lives in `super::xfaces`.

mod subrs;
#[cfg(test)]
pub(crate) use subrs::SUBRS;
pub(crate) use subrs::register_subrs;

use crate::emacs_core::error::LispCondition;
pub(crate) use crate::emacs_core::error::{
    expect_args, expect_args_range, expect_max_args, expect_min_args,
};
use std::sync::{OnceLock, RwLock};

use num_enum::{IntoPrimitive, TryFromPrimitive};
use strum::EnumString;

use super::error::{EvalResult, Flow, signal};
use super::xfaces::{
    FrameFaceInitial, clear_font_cache_state, derived_face_attrs_from_font_value,
    ensure_frame_lisp_face_vector, lookup_frame_lisp_face_vector,
    realize_default_lisp_face_for_frame, runtime_face_from_lisp_face_vector,
    runtime_face_table_from_frame_lisp_faces, set_lisp_face_vector_attr,
};

use super::display_host::{FrameFontRequest, FrameFontSize};
use super::intern::{intern, resolve_sym};
use super::value::*;
use crate::buffer::{Buffer, CharPos0, EmacsBytePos, LispCharPos1};
use crate::emacs_core::SymId;
use crate::face::{
    Face as RuntimeFace, FaceHeight, FaceRemapping, FontSlant, FontWeight, FontWidth, LFaceAttr,
};
use crate::heap_types::LispString;
use crate::tagged::header::{FontObjectData, FontObjectMetrics};
use crate::window::{FRAME_ID_BASE, FrameId, FrameManager, FrameParam, WindowId};
use neomacs_display_protocol::font::ResolvedFontIdentity;

type AlternativeFontFamilyAlist = Vec<(SymId, Vec<SymId>)>;
type AlternativeFontRegistryAlist = Vec<(LispString, Vec<LispString>)>;

const FONT_WEIGHT_STYLE_TABLE: &[(i64, &[&str])] = &[
    (0, &["thin"]),
    (
        40,
        &["ultra-light", "ultralight", "extra-light", "extralight"],
    ),
    (50, &["light"]),
    (55, &["semi-light", "semilight", "demilight"]),
    (80, &["regular", "normal", "unspecified", "book"]),
    (100, &["medium"]),
    (
        180,
        &["semi-bold", "semibold", "demibold", "demi-bold", "demi"],
    ),
    (200, &["bold"]),
    (205, &["extra-bold", "extrabold", "ultra-bold", "ultrabold"]),
    (210, &["black", "heavy"]),
    (250, &["ultra-heavy", "ultraheavy"]),
];

const FONT_SLANT_STYLE_TABLE: &[(i64, &[&str])] = &[
    (0, &["reverse-oblique", "ro"]),
    (10, &["reverse-italic", "ri"]),
    (100, &["normal", "r", "unspecified"]),
    (200, &["italic", "i", "ot"]),
    (210, &["oblique", "o"]),
];

const FONT_WIDTH_STYLE_TABLE: &[(i64, &[&str])] = &[
    (50, &["ultra-condensed", "ultracondensed"]),
    (63, &["extra-condensed", "extracondensed"]),
    (75, &["condensed", "compressed", "narrow"]),
    (87, &["semi-condensed", "semicondensed", "demicondensed"]),
    (100, &["normal", "medium", "regular", "unspecified"]),
    (113, &["semi-expanded", "semiexpanded", "demiexpanded"]),
    (125, &["expanded"]),
    (150, &["extra-expanded", "extraexpanded"]),
    (200, &["ultra-expanded", "ultraexpanded", "wide"]),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoPrimitive, TryFromPrimitive)]
#[repr(i32)]
enum FontSpacing {
    #[strum(serialize = "p", serialize = "P")]
    Proportional = 0,
    #[strum(serialize = "d", serialize = "D")]
    Dual = 90,
    #[strum(serialize = "m", serialize = "M")]
    Mono = 100,
    #[strum(serialize = "c", serialize = "C")]
    Charcell = 110,
}

impl FontSpacing {
    const MAX_GNU_CODE: i64 = 110;

    fn from_symbol_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn from_gnu_code(code: i64) -> Option<Self> {
        let code = i32::try_from(code).ok()?;
        Self::try_from(code).ok()
    }

    fn gnu_code(self) -> i32 {
        self.into()
    }

    fn xlfd_letter(self) -> &'static str {
        match self {
            Self::Proportional => "p",
            Self::Dual => "d",
            Self::Mono => "m",
            Self::Charcell => "c",
        }
    }

    fn xlfd_bucket_for_gnu_code(code: i64) -> Option<Self> {
        match code {
            0 => Some(Self::Proportional),
            1..=90 => Some(Self::Dual),
            91..=100 => Some(Self::Mono),
            101..=Self::MAX_GNU_CODE => Some(Self::Charcell),
            _ => None,
        }
    }

    fn xlfd_letter_for_gnu_code(code: i64) -> Option<&'static str> {
        Self::xlfd_bucket_for_gnu_code(code).map(Self::xlfd_letter)
    }
}

static ALTERNATIVE_FONT_FAMILY_ALIST: OnceLock<RwLock<AlternativeFontFamilyAlist>> =
    OnceLock::new();
static ALTERNATIVE_FONT_REGISTRY_ALIST: OnceLock<RwLock<AlternativeFontRegistryAlist>> =
    OnceLock::new();

pub(crate) fn alternative_font_family_alist() -> &'static RwLock<AlternativeFontFamilyAlist> {
    ALTERNATIVE_FONT_FAMILY_ALIST.get_or_init(|| RwLock::new(Vec::new()))
}

pub(crate) fn alternative_font_registry_alist() -> &'static RwLock<AlternativeFontRegistryAlist> {
    ALTERNATIVE_FONT_REGISTRY_ALIST.get_or_init(|| RwLock::new(Vec::new()))
}

fn font_style_table(entries: &[(i64, &[&str])]) -> Value {
    Value::vector(
        entries
            .iter()
            .map(|(numeric, names)| {
                let mut row = Vec::with_capacity(names.len() + 1);
                row.push(Value::fixnum(*numeric));
                row.extend(names.iter().map(|name| Value::symbol(*name)));
                Value::vector(row)
            })
            .collect(),
    )
}

pub(crate) fn init_font_vars(obarray: &mut super::symbol::Obarray) {
    for (name, value) in [
        (
            "font-weight-table",
            font_style_table(FONT_WEIGHT_STYLE_TABLE),
        ),
        ("font-slant-table", font_style_table(FONT_SLANT_STYLE_TABLE)),
        ("font-width-table", font_style_table(FONT_WIDTH_STYLE_TABLE)),
    ] {
        obarray.set_symbol_value(name, value);
        obarray.make_special(name);
        obarray.set_constant(name);
    }

    obarray.set_symbol_value("font-log", Value::T);
    obarray.make_special("font-log");
}

pub fn alternative_font_families(family: &str) -> Vec<String> {
    let lookup = family.trim();
    if lookup.is_empty() {
        return Vec::new();
    }

    let Ok(alist) = alternative_font_family_alist().read() else {
        return vec![lookup.to_string()];
    };

    alist
        .iter()
        .find_map(|(name, families)| {
            // Issue #131: compare/return font-family names over their real Emacs
            // bytes (resolve_sym_lisp_string), so raw-unibyte families are not
            // confused with the PUA-sentinel storage form.
            crate::emacs_core::intern::resolve_sym_lisp_string(*name)
                .as_bytes()
                .eq_ignore_ascii_case(lookup.as_bytes())
                .then(|| {
                    families
                        .iter()
                        .map(|sym| {
                            crate::emacs_core::emacs_char::to_utf8_lossy(
                                crate::emacs_core::intern::resolve_sym_lisp_string(*sym).as_bytes(),
                            )
                        })
                        .collect()
                })
        })
        .unwrap_or_else(|| vec![lookup.to_string()])
}

pub fn alternative_font_registries(registry: &str) -> Vec<String> {
    let lookup = registry.trim();
    if lookup.is_empty() {
        return Vec::new();
    }

    let Ok(alist) = alternative_font_registry_alist().read() else {
        return vec![lookup.to_ascii_lowercase()];
    };

    alist
        .iter()
        .find_map(|(name, registries)| {
            name.as_bytes()
                .eq_ignore_ascii_case(lookup.as_bytes())
                .then(|| {
                    registries
                        .iter()
                        .map(|text| {
                            // Issue #131: font registry names are ASCII identifiers; render the
                            // string's Emacs bytes faithfully rather than via storage sentinels.
                            crate::emacs_core::emacs_char::to_utf8_lossy(text.as_bytes())
                        })
                        .collect()
                })
        })
        .unwrap_or_else(|| vec![lookup.to_ascii_lowercase()])
}

// ---------------------------------------------------------------------------
// Argument helpers (local to this module)
// ---------------------------------------------------------------------------

pub(crate) fn live_frame_designator_in_state(frames: &FrameManager, value: &Value) -> bool {
    match value.kind() {
        ValueKind::Fixnum(id) if id >= 0 => frames.get(FrameId(id as u64)).is_some(),
        ValueKind::Veclike(VecLikeType::Frame) => {
            frames.get(FrameId(value.as_frame_id().unwrap())).is_some()
        }
        _ => false,
    }
}

pub(crate) fn frame_id_from_designator(value: &Value) -> Option<FrameId> {
    match value.kind() {
        ValueKind::Fixnum(id) if id >= 0 => Some(FrameId(id as u64)),
        ValueKind::Veclike(VecLikeType::Frame) => Some(FrameId(value.as_frame_id().unwrap())),
        _ => None,
    }
}

pub(crate) fn font_string_text(value: &Value) -> Option<String> {
    // Issue #131: read the value's real Emacs bytes (lossy UTF-8 view) rather than
    // the PUA-sentinel storage form. Font/color/property names are ASCII, where
    // this is exact; raw-byte family names are interned faithfully elsewhere.
    value
        .as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
}

pub(crate) fn font_value_text(value: &Value) -> Option<String> {
    match value.kind() {
        ValueKind::String => font_string_text(value),
        ValueKind::Symbol(id) => Some(resolve_sym(id).to_owned()),
        _ => None,
    }
}

fn font_value_text_lisp_string(value: &Value) -> Option<LispString> {
    match value.kind() {
        ValueKind::String => value.as_lisp_string().cloned(),
        ValueKind::Symbol(id) => Some(LispString::from_utf8(resolve_sym(id))),
        _ => None,
    }
}

pub(crate) struct LiveFrameFontResolution {
    pub(crate) font_value: Value,
}

fn frame_font_request_from_named_font_string(name: &str) -> Option<FrameFontRequest> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut face = RuntimeFace::new("default");

    if !trimmed.starts_with('-') {
        if let Some((family, size)) = trimmed.rsplit_once('-')
            && !family.trim().is_empty()
            && size.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
            && size.chars().filter(|&ch| ch == '.').count() <= 1
            && let Ok(points) = size.parse::<f64>()
            && let Some(size) = FrameFontSize::points(points)
        {
            face.family = Some(Value::string(family.trim().to_string()));
            return Some(FrameFontRequest::with_size(face, size));
        }
        face.family = Some(Value::string(trimmed.to_string()));
        return Some(FrameFontRequest::from_face(face));
    }

    let fields = trimmed.split('-').collect::<Vec<_>>();
    if fields.len() < 12 {
        return None;
    }

    let foundry = fields[1];
    let family = fields[2];
    let weight = fields[3];
    let slant = fields[4];
    let set_width = fields[5];
    let pixel = fields[7];

    if foundry != "*" && !foundry.is_empty() {
        face.foundry = Some(Value::string(foundry.to_string()));
    }
    if family != "*" && !family.is_empty() {
        face.family = Some(Value::string(family.to_string()));
    }
    if let Some(parsed_weight) = FontWeight::from_symbol(weight) {
        face.weight = Some(parsed_weight);
    }
    face.slant = match slant {
        "i" | "italic" => Some(FontSlant::Italic),
        "o" | "oblique" => Some(FontSlant::Oblique),
        "ri" | "reverse-italic" => Some(FontSlant::ReverseItalic),
        "ro" | "reverse-oblique" => Some(FontSlant::ReverseOblique),
        "r" | "normal" | "*" => Some(FontSlant::Normal),
        _ => None,
    };
    face.width = match set_width {
        "normal" | "*" => Some(FontWidth::Normal),
        other => FontWidth::from_symbol(other),
    };
    let size = pixel
        .chars()
        .all(|ch| ch.is_ascii_digit())
        .then(|| pixel.parse::<i64>().ok())
        .flatten()
        .and_then(FrameFontSize::pixels)
        .unwrap_or(FrameFontSize::Default);

    Some(FrameFontRequest::with_size(face, size))
}

fn frame_font_request_from_value(value: &Value) -> Option<FrameFontRequest> {
    if let Some(text) = font_value_text(value) {
        return frame_font_request_from_named_font_string(&text);
    }
    if !is_font(value) {
        return None;
    }

    let pixel_sized_selector = is_font_spec(value) || is_font_entity(value);
    let elems = font_value_fields(value)?;
    let mut face = RuntimeFace::new("default");

    face.family = font_vector_get_flexible(&elems, "family")
        .and_then(|value| font_value_text(&value))
        .map(Value::string);
    face.foundry = font_vector_get_flexible(&elems, "foundry")
        .and_then(|value| font_value_text(&value))
        .map(Value::string);
    face.weight = font_vector_get_flexible(&elems, "weight").and_then(font_weight_from_value);
    face.slant = font_vector_get_flexible(&elems, "slant").and_then(font_slant_from_value);
    face.width = font_vector_get_flexible(&elems, "width").and_then(|value| match value.kind() {
        ValueKind::Symbol(id) => FontWidth::from_symbol(resolve_sym(id)),
        _ => None,
    });
    let size = if let Some(value) = font_vector_get_flexible(&elems, "height") {
        face.height = face_height_from_value(value);
        None
    } else if let Some(value) = font_vector_get_flexible(&elems, "size") {
        match value.kind() {
            ValueKind::Fixnum(px) if pixel_sized_selector => FrameFontSize::pixels(px),
            ValueKind::Float => FrameFontSize::points(value.xfloat()),
            _ => None,
        }
    } else {
        None
    };

    Some(match size {
        Some(size) => FrameFontRequest::with_size(face, size),
        None => FrameFontRequest::from_face(face),
    })
}

fn face_height_from_value(value: Value) -> Option<FaceHeight> {
    match value.kind() {
        ValueKind::Fixnum(n) if n > 0 => Some(FaceHeight::Absolute(n as i32)),
        ValueKind::Float if value.xfloat() > 0.0 => Some(FaceHeight::Relative(value.xfloat())),
        _ => None,
    }
}

fn build_frame_font_object_from_resolution(
    requested_face: &RuntimeFace,
    resolved: &super::eval::ResolvedFrameFont,
) -> Value {
    let opened = &resolved.font;
    let canonical = &opened.resolved;
    let mut selected = requested_face.clone();
    selected.family = Some(Value::string(canonical.family.clone()));
    selected.foundry = opened
        .foundry
        .clone()
        .map(Value::heap_string)
        .or(requested_face.foundry);
    selected.weight = Some(FontWeight::from_css_weight(canonical.weight));
    selected.slant = Some(opened.slant);
    selected.width = Some(opened.width());
    selected.height = Some(FaceHeight::Absolute(resolved.height_tenths));

    finish_opened_font(
        font_object_property_fields(&selected, Some(i64::from(opened.metrics.pixel_size))),
        canonical
            .identity
            .file_path
            .as_deref()
            .map(LispString::from_utf8)
            .as_ref(),
        canonical
            .full_name
            .as_deref()
            .map(LispString::from_utf8)
            .as_ref(),
        OpenedFontMetrics::from_probe(opened.metrics),
        opened
            .capability
            .as_ref()
            .map(otf_capability_to_lisp)
            .unwrap_or(Value::NIL),
        canonical.identity.clone(),
    )
}

pub(crate) fn resolve_live_frame_font_request(
    eval: &mut super::eval::Context,
    frame_id: FrameId,
    requested: &Value,
) -> LiveFrameFontResolution {
    resolve_live_frame_font_request_in_state(
        &eval.frames,
        &mut eval.display_host,
        frame_id,
        requested,
    )
}

fn resolve_live_frame_font_request_in_state(
    frames: &FrameManager,
    display_host: &mut Option<Box<dyn super::eval::DisplayHost>>,
    frame_id: FrameId,
    requested: &Value,
) -> LiveFrameFontResolution {
    // GNU only opens a font when the target (or, for new-frame defaults, the
    // selected reference frame) is graphical. A TTY frame retains the Lisp
    // selector without manufacturing native metrics.
    if frames
        .get(frame_id)
        .is_none_or(|frame| frame.effective_window_system().is_none())
    {
        return LiveFrameFontResolution {
            font_value: *requested,
        };
    }

    if is_font_object(requested) {
        return LiveFrameFontResolution {
            font_value: *requested,
        };
    }

    if let Some(frame) = frames.get(frame_id)
        && font_value_matches_frame_font_parameter(frame, requested)
        && let Some(font_value) = frame.parameter("font-parameter")
        && is_font(&font_value)
    {
        return LiveFrameFontResolution { font_value };
    }

    let Some(request) = frame_font_request_from_value(requested) else {
        return LiveFrameFontResolution {
            font_value: *requested,
        };
    };
    let requested_face = request.face().clone();

    let realized = display_host
        .as_mut()
        .and_then(|host| host.resolve_frame_font(frame_id, request).ok())
        .flatten();
    let font_value = realized
        .as_ref()
        .map(|resolved| build_frame_font_object_from_resolution(&requested_face, resolved))
        .unwrap_or(*requested);

    LiveFrameFontResolution { font_value }
}

pub(crate) fn sync_live_frame_font_state(
    eval: &mut super::eval::Context,
    frame_id: FrameId,
    requested: &Value,
    resolution: &LiveFrameFontResolution,
) {
    sync_live_frame_font_state_in_state(
        &mut eval.frames,
        &mut eval.display_host,
        frame_id,
        requested,
        resolution,
    );
}

fn sync_live_frame_font_state_in_state(
    frames: &mut FrameManager,
    display_host: &mut Option<Box<dyn super::eval::DisplayHost>>,
    frame_id: FrameId,
    requested: &Value,
    resolution: &LiveFrameFontResolution,
) {
    // A selector is public face/frame state, but it is not an opened font.
    // If the host could not realize the request, retain the last coherent
    // internal object and its geometry.  This is the same transaction boundary
    // as GNU `gui_set_font`, which restores the old frame parameter before an
    // open attempt that may fail.
    if !is_font_object(&resolution.font_value) {
        return;
    }

    let opened = OpenedFont::decode(resolution.font_value)
        .expect("an opened font object must carry native font data");
    let metrics = opened.data.metrics;

    let Some(frame) = frames.get_mut(frame_id) else {
        return;
    };

    let public_font_name = if requested.is_string() {
        *requested
    } else {
        font_name_value(&resolution.font_value).unwrap_or(*requested)
    };

    let font_changed = frame.parameter("font-parameter") != Some(resolution.font_value);
    let geometry_changed = frame.font_pixel_size != metrics.pixel_size.max(1) as f32
        || frame.char_width != metrics.average_width.max(1) as f32
        || frame.char_height != metrics.height.max(1) as f32;

    frame.set_known_parameter(FrameParam::Font, public_font_name);
    frame.set_parameter(Value::symbol("font-parameter"), resolution.font_value);
    frame.font_pixel_size = metrics.pixel_size.max(1) as f32;
    frame.char_width = metrics.average_width.max(1) as f32;
    frame.char_height = metrics.height.max(1) as f32;

    let mut geometry_hints = None;
    if font_changed || geometry_changed {
        let is_top_level_gui_frame =
            frame.effective_window_system().is_some() && frame.parent_frame.as_frame_id().is_none();
        if is_top_level_gui_frame {
            frame.defer_next_gui_parameter_resize();
            geometry_hints = Some(frame.gui_geometry_hints());
        }
    }

    if let Some(geometry_hints) = geometry_hints
        && let Some(host) = display_host.as_mut()
        && let Err(err) = host.set_gui_frame_geometry_hints(frame_id, geometry_hints)
    {
        tracing::warn!(
            "failed to update live frame geometry hints after font change for frame 0x{:x}: {}",
            frame_id.0,
            err
        );
    }
}

pub(crate) fn sync_live_frame_font_parameter_in_state(
    frames: &mut FrameManager,
    display_host: &mut Option<Box<dyn super::eval::DisplayHost>>,
    frame_id: FrameId,
    requested: Value,
) {
    let resolution =
        resolve_live_frame_font_request_in_state(frames, display_host, frame_id, &requested);
    sync_live_frame_font_state_in_state(frames, display_host, frame_id, &requested, &resolution);
}

pub(crate) fn default_face_font_attr_affects_frame_font(attr: LFaceAttr) -> bool {
    matches!(
        attr,
        LFaceAttr::Font
            | LFaceAttr::Family
            | LFaceAttr::Foundry
            | LFaceAttr::Height
            | LFaceAttr::Weight
            | LFaceAttr::Slant
            | LFaceAttr::Width
    )
}

pub(crate) fn sync_live_default_face_font_state(
    eval: &mut super::eval::Context,
    frame_id: FrameId,
) {
    if eval
        .frames
        .get(frame_id)
        .is_none_or(|frame| frame.effective_window_system().is_none())
    {
        return;
    }

    let Some(vector) = lookup_frame_lisp_face_vector(eval, frame_id, "default") else {
        return;
    };
    let requested_face = runtime_face_from_lisp_face_vector("default", vector);
    let realized = eval
        .display_host
        .as_mut()
        .and_then(|host| {
            host.resolve_frame_font(
                frame_id,
                FrameFontRequest::from_face(requested_face.clone()),
            )
            .ok()
        })
        .flatten();
    let Some(realized) = realized else {
        tracing::warn!(
            frame_id = frame_id.0,
            "default-face font change did not produce an opened font; preserving live frame state"
        );
        return;
    };
    let font_value = build_frame_font_object_from_resolution(&requested_face, &realized);
    let resolution = LiveFrameFontResolution { font_value };

    sync_live_frame_font_state(eval, frame_id, &font_value, &resolution);
}

fn expect_optional_frame_designator_in_state(
    frames: &FrameManager,
    value: Option<&Value>,
) -> Result<(), Flow> {
    if let Some(frame) = value
        && !frame.is_nil()
        && !live_frame_designator_in_state(frames, frame)
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *frame],
        ));
    }
    Ok(())
}

pub(crate) fn frame_device_designator_p(value: &Value) -> bool {
    match value.kind() {
        ValueKind::Fixnum(id) => id >= FRAME_ID_BASE as i64,
        ValueKind::Veclike(VecLikeType::Frame) => value.as_frame_id().unwrap() >= FRAME_ID_BASE,
        _ => false,
    }
}

pub(crate) fn update_face_from_frame_parameter(
    eval: &mut super::eval::Context,
    frame_id: FrameId,
    param: FrameParam,
    new_value: Value,
) -> Result<(), crate::emacs_core::error::Flow> {
    let attr = match param {
        FrameParam::ForegroundColor => LFaceAttr::Foreground,
        FrameParam::BackgroundColor => {
            if let Some(function) = eval.obarray().symbol_function("frame-set-background-mode") {
                let _ = eval.apply(function, vec![Value::make_frame(frame_id.0)])?;
            }
            LFaceAttr::Background
        }
        _ => return Ok(()),
    };

    // GNU `update_face_from_frame_parameter' writes the frame-local Lisp face
    // slot directly and then calls `realize_basic_faces'.  Do not route this
    // derived frame-parameter update back through the public face setter: that
    // gives the frame parameter a second, competing source of authority and
    // skips TTY default-face realization.
    if let Some(vector) =
        ensure_frame_lisp_face_vector(eval, frame_id, "default", FrameFaceInitial::SelectedBase)
    {
        let value = if new_value.is_string() {
            new_value
        } else {
            Value::symbol("unspecified")
        };
        set_lisp_face_vector_attr(vector, attr, value);
        realize_default_lisp_face_for_frame(eval, frame_id);
        eval.face_change_count += 1;
    }
    Ok(())
}

/// GNU `internal-set-lisp-face-attribute` reflects a small, fixed set of
/// frame-local face attributes back into frame parameters (xfaces.c).  Keep
/// that relationship in one table-shaped function: face state remains the
/// source of the change, while the frame-parameter primitive remains the
/// single publication seam used by frame backends.
pub(crate) fn frame_parameter_for_face_attribute(
    face_name: &str,
    attr: LFaceAttr,
) -> Option<FrameParam> {
    match (face_name, attr) {
        ("default", LFaceAttr::Foreground) => Some(FrameParam::ForegroundColor),
        ("default", LFaceAttr::Background) => Some(FrameParam::BackgroundColor),
        ("border", LFaceAttr::Background) => Some(FrameParam::BorderColor),
        ("cursor", LFaceAttr::Background) => Some(FrameParam::CursorColor),
        ("mouse", LFaceAttr::Background) => Some(FrameParam::MouseColor),
        ("scroll-bar", LFaceAttr::Foreground) if !cfg!(windows) => {
            Some(FrameParam::ScrollBarForeground)
        }
        ("scroll-bar", LFaceAttr::Background) if !cfg!(windows) => {
            Some(FrameParam::ScrollBarBackground)
        }
        _ => None,
    }
}

pub(crate) fn publish_face_attribute_to_frame_parameter(
    eval: &mut super::eval::Context,
    frame_id: FrameId,
    parameter: FrameParam,
    value: Value,
) -> Result<(), Flow> {
    // GNU calls Fmodify_frame_parameters directly from xfaces.c.  Preserve
    // that primitive-to-primitive seam: the parameter change is observable,
    // but Lisp advice around `modify-frame-parameters` is not invoked.
    super::frame::builtin_modify_frame_parameters(
        eval,
        vec![
            Value::make_frame(frame_id.0),
            Value::list(vec![Value::cons(parameter.symbol(), value)]),
        ],
    )?;
    Ok(())
}

/// Seed the selected frame's authoritative `default` Lisp face specification
/// from its `font-parameter` without mutating Lisp override state.
///
/// GNU keeps the defface for `default` empty and realizes the actual frame
/// font through the face subsystem in C.  Redisplay later derives the runtime
/// face table from this frame-local specification.
pub fn seed_live_frame_default_face_from_font_parameter(
    eval: &mut super::eval::Context,
    frame_id: FrameId,
) {
    let Some(font_value) = eval
        .frames
        .get(frame_id)
        .and_then(|frame| frame.parameter("font-parameter"))
    else {
        return;
    };

    let Some(vector) =
        ensure_frame_lisp_face_vector(eval, frame_id, "default", FrameFaceInitial::SelectedBase)
    else {
        return;
    };
    for (attr_name, attr_value) in derived_face_attrs_from_font_value(&font_value) {
        set_lisp_face_vector_attr(vector, attr_name, attr_value);
    }
    eval.face_change_count += 1;
}

// ---------------------------------------------------------------------------
// Font-spec helpers
// ---------------------------------------------------------------------------

/// The tag keyword used to identify font-spec vectors: `:font-spec`.
const FONT_SPEC_TAG: &str = "font-spec";
const FONT_ENTITY_TAG: &str = "font-entity";
pub(crate) const FONT_OBJECT_TAG: &str = "font-object";

type OpenedFontMetrics = FontObjectMetrics;

impl OpenedFontMetrics {
    fn from_probe(probe: super::eval::FontPxProbeResult) -> Self {
        Self {
            pixel_size: i64::from(probe.pixel_size),
            height: i64::from(probe.height.max(1)),
            max_width: i64::from(probe.max_width.max(0)),
            ascent: i64::from(probe.ascent.max(0)),
            descent: i64::from(probe.descent.max(0)),
            space_width: i64::from(probe.space_width.max(0)),
            average_width: i64::from(probe.average_width.max(0)),
        }
    }

    #[cfg(test)]
    fn fallback(pixel_size: i64) -> Self {
        let pixel_size = pixel_size.max(1);
        let height = pixel_size;
        let average_width = ((pixel_size + 1) / 2).max(1);
        let ascent = ((height * 3 + 2) / 4).max(1);
        Self {
            pixel_size,
            height,
            max_width: average_width,
            ascent,
            descent: (height - ascent).max(0),
            space_width: average_width,
            average_width,
        }
    }
}

#[derive(Clone, Copy)]
struct OpenedFont {
    data: &'static FontObjectData,
}

impl OpenedFont {
    fn decode(value: Value) -> Option<Self> {
        Some(Self {
            data: value.as_font_data()?,
        })
    }

    fn fields(self) -> &'static [Value] {
        self.data.fields.as_slice()
    }

    fn property(self, name: &str) -> Value {
        font_vector_get_flexible(self.fields(), name).unwrap_or(Value::NIL)
    }

    fn query_vector(self) -> Value {
        let m = self.data.metrics;
        Value::vector(vec![
            self.property("name"),
            self.property("file"),
            Value::fixnum(m.pixel_size),
            Value::fixnum(m.max_width),
            Value::fixnum(m.ascent),
            Value::fixnum(m.descent),
            Value::fixnum(m.space_width),
            Value::fixnum(m.average_width),
            self.data.capability,
        ])
    }

    fn info_vector(self) -> Value {
        let m = self.data.metrics;
        Value::vector(vec![
            self.property("name"),
            self.property("full-name"),
            Value::fixnum(m.pixel_size),
            Value::fixnum(m.height),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(m.ascent),
            Value::fixnum(m.max_width),
            Value::fixnum(m.ascent),
            Value::fixnum(m.descent),
            Value::fixnum(m.space_width),
            Value::fixnum(m.average_width),
            self.property("file"),
            self.data.capability,
        ])
    }
}

fn is_tagged_font_vector(val: &Value, tag: &str) -> bool {
    match val.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let elems = val.as_vector_data().unwrap().clone();
            elems
                .first()
                .and_then(|v| v.as_symbol_name())
                .is_some_and(|name| name.trim_start_matches(':') == tag)
        }
        _ => false,
    }
}

/// Check whether a Value is a font-spec (a vector whose first element is
/// the tag symbol/keyword `font-spec` / `:font-spec`.
pub(crate) fn is_font_spec(val: &Value) -> bool {
    is_tagged_font_vector(val, FONT_SPEC_TAG)
}

/// Check whether a value is a complete opened-font pseudovector.
pub(crate) fn is_font_object(val: &Value) -> bool {
    val.is_font_object()
}

/// Check whether a value is represented as a font-entity vector.
pub(crate) fn is_font_entity(val: &Value) -> bool {
    is_tagged_font_vector(val, FONT_ENTITY_TAG)
}

pub(crate) fn is_font(val: &Value) -> bool {
    is_font_spec(val) || is_font_entity(val) || is_font_object(val)
}

pub(crate) fn font_value_fields(value: &Value) -> Option<&'static [Value]> {
    match value.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => value.as_vector_data().map(|v| &v[..]),
        ValueKind::Veclike(VecLikeType::Font) => OpenedFont::decode(*value).map(OpenedFont::fields),
        _ => None,
    }
}

/// The `type-of`/`cl-type-of` symbol for a font value, mirroring GNU's
/// `PVEC_FONT` size discrimination (`font-spec` < `font-entity` <
/// `font-object`, src/font.h FONT_*_MAX). Specs and entities are tagged public
/// vectors; opened fonts use the opaque `PVEC_FONT` runtime tag. `None` for
/// non-font values.
pub(crate) fn font_value_type_symbol(val: &Value) -> Option<&'static str> {
    if is_font_spec(val) {
        Some(FONT_SPEC_TAG)
    } else if is_font_entity(val) {
        Some(FONT_ENTITY_TAG)
    } else if is_font_object(val) {
        Some(FONT_OBJECT_TAG)
    } else {
        None
    }
}

/// Extract a property from a tagged font vector.
///
/// Property lookup is strict: keys only match if they are exactly equal to
/// `prop` (keyword vs symbol distinction is preserved).
fn font_vector_get(vec_elems: &[Value], prop: &Value) -> Value {
    // Skip the tag at index 0; scan remaining pairs.
    let mut i = 1;
    while i + 1 < vec_elems.len() {
        if vec_elems[i] == *prop {
            return vec_elems[i + 1];
        }
        i += 2;
    }
    Value::NIL
}

/// Get a property from a tagged font vector while accepting both `family` and `:family`
/// style keys, and both keyword and symbol keys.
pub(crate) fn font_vector_get_flexible(vec_elems: &[Value], prop: &str) -> Option<Value> {
    let prop_norm = prop.trim_start_matches(':');
    let mut i = 1;
    while i + 1 < vec_elems.len() {
        let key = &vec_elems[i];
        let key_text = match key.kind() {
            ValueKind::Symbol(k) => resolve_sym(k),
            _ => {
                i += 2;
                continue;
            }
        };
        let key_norm = key_text.trim_start_matches(':');
        if key_norm == prop_norm {
            return Some(vec_elems[i + 1]);
        }
        i += 2;
    }
    None
}

fn font_spec_field_to_string(value: &Value) -> String {
    match value.kind() {
        ValueKind::String => font_string_text(value).expect("checked string"),
        ValueKind::Symbol(id) => resolve_sym(id).to_owned(),
        _ => "*".to_string(),
    }
}

fn xlfd_size_field(size_val: &Value) -> Option<String> {
    match size_val.kind() {
        ValueKind::Fixnum(size) => {
            if size > 0 {
                Some(format!("{}-*", size))
            } else {
                Some("*-*".to_string())
            }
        }
        ValueKind::Float => {
            let f = size_val.xfloat();
            let scaled = f * 10.0;
            if scaled.is_finite() {
                Some(format!("*-{}", scaled.round() as i64))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn fold_xlfd_wildcards(mut name: String) -> String {
    while let Some(pos) = name.find("-*-*") {
        name.replace_range(pos + 1..pos + 3, "");
    }
    name
}

fn normalize_registry_field(value: &Option<Value>) -> String {
    match value {
        None => "*-*".to_string(),
        Some(v) => match v.kind() {
            ValueKind::String => {
                let s = font_string_text(v).expect("checked string");
                if !s.contains('-') {
                    format!("{}-*", s)
                } else {
                    s
                }
            }
            ValueKind::Symbol(id) => {
                let s = resolve_sym(id);
                if !s.contains('-') {
                    format!("{}-*", s)
                } else {
                    s.to_owned()
                }
            }
            _ => "*-*".to_string(),
        },
    }
}

fn sanitize_style_field(value: &Value) -> String {
    match value.kind() {
        ValueKind::Symbol(id) => resolve_sym(id)
            .chars()
            .filter(|ch| *ch != '-' && *ch != '?' && *ch != ',' && *ch != '"')
            .collect(),
        ValueKind::String => {
            let s = font_string_text(value).expect("checked string");
            s.chars()
                .filter(|ch| *ch != '-' && *ch != '?' && *ch != ',' && *ch != '"')
                .collect()
        }
        _ => "*".to_string(),
    }
}

fn spacing_field(value: Option<&Value>) -> String {
    match value {
        None => "*".to_string(),
        Some(v) if v.is_fixnum() => {
            let spacing = v.as_fixnum().unwrap();
            FontSpacing::xlfd_letter_for_gnu_code(spacing)
                .unwrap_or("*")
                .to_string()
        }
        Some(v) => sanitize_style_field(v),
    }
}

fn avg_width_field(value: Option<&Value>) -> String {
    match value {
        Some(v) => match v.kind() {
            ValueKind::Fixnum(n) => n.to_string(),
            ValueKind::String => font_string_text(v).expect("checked string"),
            ValueKind::Symbol(id) => resolve_sym(id).to_owned(),
            _ => "*".to_string(),
        },
        None => "*".to_string(),
    }
}

fn xlfd_pixel_field(size: Option<&Value>) -> String {
    match size {
        Some(value) => xlfd_size_field(value).unwrap_or("*-*".to_string()),
        None => "*-*".to_string(),
    }
}

fn xlfd_resolution_field(dpi: Option<&Value>) -> String {
    match dpi {
        Some(v) if v.is_fixnum() => {
            let size = v.as_fixnum().unwrap();
            format!("{}-{}", size, size)
        }
        _ => "*-*".to_string(),
    }
}

fn xlfd_fields_from_font_vector(
    v: &[Value],
) -> (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
) {
    let foundry = font_vector_get_flexible(v, "foundry")
        .map(|value| font_spec_field_to_string(&value))
        .unwrap_or_else(|| "*".to_string());
    let family = font_vector_get_flexible(v, "family")
        .map(|value| font_spec_field_to_string(&value))
        .unwrap_or_else(|| "*".to_string());
    let weight = font_vector_get_flexible(v, "weight")
        .map(|value| sanitize_style_field(&value))
        .unwrap_or_else(|| "*".to_string());
    let slant = font_vector_get_flexible(v, "slant")
        .map(|value| sanitize_style_field(&value))
        .unwrap_or_else(|| "*".to_string());
    let set_width = font_vector_get_flexible(v, "set-width")
        .or_else(|| font_vector_get_flexible(v, "setwidth"))
        .or_else(|| font_vector_get_flexible(v, "width"))
        .map(|value| font_spec_field_to_string(&value))
        .unwrap_or_else(|| "*".to_string());
    let adstyle = font_vector_get_flexible(v, "adstyle")
        .map(|value| font_spec_field_to_string(&value))
        .unwrap_or_else(|| "*".to_string());

    let size = font_vector_get_flexible(v, "size");
    let dpi = font_vector_get_flexible(v, "dpi");
    let spacing = font_vector_get_flexible(v, "spacing");
    let avg_width = font_vector_get_flexible(v, "average_width")
        .or_else(|| font_vector_get_flexible(v, "avg_width"))
        .or_else(|| font_vector_get_flexible(v, "avg-width"));
    let registry = font_vector_get_flexible(v, "registry");

    let pixel = xlfd_pixel_field(size.as_ref());
    let resx = xlfd_resolution_field(dpi.as_ref());
    let spacing = spacing_field(spacing.as_ref());
    let avg_width = avg_width_field(avg_width.as_ref());
    let registry = normalize_registry_field(&registry);

    (
        foundry, family, weight, slant, set_width, adstyle, pixel, resx, spacing, avg_width,
        registry,
    )
}

/// Set (or add) a property in a font-spec in place.
fn font_spec_put(vec_elems: &mut Vec<Value>, prop: &Value, val: &Value) -> EvalResult {
    let normalized = normalize_font_prop_value(prop, val)?;
    let mut i = 1;
    while i + 1 < vec_elems.len() {
        if vec_elems[i] == *prop {
            vec_elems[i + 1] = normalized;
            return Ok(normalized);
        }
        i += 2;
    }
    vec_elems.push(*prop);
    vec_elems.push(normalized);
    Ok(normalized)
}

fn invalid_font_property(prop: &Value, val: &Value) -> Flow {
    signal(
        "error",
        vec![
            Value::string("invalid font property"),
            Value::cons(*prop, *val),
        ],
    )
}

fn font_style_table_for_key(key: &str) -> Option<&'static [(i64, &'static [&'static str])]> {
    match key {
        "weight" => Some(FONT_WEIGHT_STYLE_TABLE),
        "slant" => Some(FONT_SLANT_STYLE_TABLE),
        "width" => Some(FONT_WIDTH_STYLE_TABLE),
        _ => None,
    }
}

/// GNU `font_style_symbolic (font, prop, for_face=true)` (font.c:471-490):
/// canonicalize a stored weight/slant/width symbol to the first ("preferred")
/// name of its style-table row -- the value behind `AREF (elt, 1)`, i.e.
/// `names[0]`. This is what `Ffont_face_attributes` uses (heavy -> black,
/// ultra-bold -> extra-bold, normal -> regular). `font-get`/`font-spec`
/// storage keep the matched alias verbatim (`for_face=false`), so this is
/// applied only at the face-read boundary. Returns `None` for a symbol that is
/// not a known style word.
fn font_style_canonical_for_face(key: &str, name: &str) -> Option<&'static str> {
    let table = font_style_table_for_key(key)?;
    table
        .iter()
        .find(|(_, names)| names.iter().any(|alias| alias.eq_ignore_ascii_case(name)))
        .and_then(|(_, names)| names.first().copied())
}

fn font_style_symbol_from_gnu_code(
    table: &'static [(i64, &'static [&'static str])],
    code: i64,
) -> Option<&'static str> {
    let code = u16::try_from(code).ok()?;
    let numeric = i64::from(code >> 8);
    let row = usize::from((code >> 4) & 0x0f);
    let alias = usize::from(code & 0x0f);
    let (row_numeric, names) = table.get(row)?;
    if *row_numeric == numeric {
        names.get(alias).copied()
    } else {
        None
    }
}

fn font_style_symbol_from_name(
    table: &'static [(i64, &'static [&'static str])],
    name: &str,
) -> Option<&'static str> {
    table
        .iter()
        .flat_map(|(_, names)| names.iter().copied())
        .find(|candidate| *candidate == name)
        .or_else(|| {
            table
                .iter()
                .flat_map(|(_, names)| names.iter().copied())
                .find(|candidate| candidate.eq_ignore_ascii_case(name))
        })
}

fn validate_font_style_prop(key: &str, prop: &Value, val: &Value) -> EvalResult {
    if val.is_nil() {
        return Ok(*val);
    }
    match val.kind() {
        ValueKind::Symbol(id) => {
            let name = resolve_sym(id);
            font_style_table_for_key(key)
                .and_then(|table| font_style_symbol_from_name(table, name))
                .map(Value::symbol)
                .ok_or_else(|| invalid_font_property(prop, val))
        }
        ValueKind::Fixnum(n) => font_style_table_for_key(key)
            .and_then(|table| font_style_symbol_from_gnu_code(table, n))
            .map(Value::symbol)
            .ok_or_else(|| invalid_font_property(prop, val)),
        _ => Err(invalid_font_property(prop, val)),
    }
}

fn validate_non_negative_font_prop(prop: &Value, val: &Value) -> EvalResult {
    if val.is_nil()
        || matches!(val.kind(), ValueKind::Fixnum(n) if n >= 0)
        || matches!(val.kind(), ValueKind::Float if val.xfloat() >= 0.0)
    {
        Ok(*val)
    } else {
        Err(invalid_font_property(prop, val))
    }
}

fn validate_spacing_font_prop(prop: &Value, val: &Value) -> EvalResult {
    if val.is_nil() {
        return Ok(*val);
    }
    match val.kind() {
        ValueKind::Fixnum(n) if (0..=FontSpacing::MAX_GNU_CODE).contains(&n) => Ok(*val),
        ValueKind::Symbol(id) => FontSpacing::from_symbol_name(resolve_sym(id))
            .map(|spacing| Value::fixnum(i64::from(spacing.gnu_code())))
            .ok_or_else(|| invalid_font_property(prop, val)),
        _ => Err(invalid_font_property(prop, val)),
    }
}

fn normalize_font_prop_value(prop: &Value, val: &Value) -> EvalResult {
    let key = match prop.kind() {
        ValueKind::Symbol(id) => resolve_sym(id).trim_start_matches(':'),
        _ => return Ok(*val),
    };

    match key {
        "family" | "foundry" | "lang" | "adstyle" | "type" | "script" => match val.kind() {
            ValueKind::String => font_string_text(val)
                .map(|text| Value::from_sym_id(intern(&text)))
                .map(Ok)
                .unwrap_or(Ok(*val)),
            ValueKind::Symbol(_) | ValueKind::Nil => Ok(*val),
            _ => Err(invalid_font_property(prop, val)),
        },
        "registry" => match val.kind() {
            ValueKind::String => font_string_text(val)
                .map(|text| Value::from_sym_id(intern(&text.to_ascii_lowercase())))
                .map(Ok)
                .unwrap_or(Ok(*val)),
            ValueKind::Symbol(id) => Ok(Value::from_sym_id(intern(
                &resolve_sym(id).to_ascii_lowercase(),
            ))),
            ValueKind::Nil => Ok(*val),
            _ => Err(invalid_font_property(prop, val)),
        },
        "weight" | "slant" | "width" => validate_font_style_prop(key, prop, val),
        "size" | "dpi" | "avgwidth" | "average-width" | "avg-width" => {
            validate_non_negative_font_prop(prop, val)
        }
        "spacing" => validate_spacing_font_prop(prop, val),
        _ => Ok(*val),
    }
}

// ===========================================================================
// Font name parsing (fontconfig / XLFD)
//
// Ports GNU Emacs `font_parse_name` (src/font.c) which dispatches between
// `font_parse_xlfd` (names starting with '-' or containing '*'/'?') and
// `font_parse_fcname` (fontconfig "Family-Size:key=val" names).  The parsed
// properties are stored into a font-spec property vector using keyword keys,
// matching the layout produced by `font-spec`/`font-put`.
// ===========================================================================

/// Set a basic font-spec property (`:family`, `:size`, etc.) on a property
/// vector, replacing any existing entry.  Mirrors GNU's `ASET (font, IDX, val)`.
fn font_parse_set(elems: &mut Vec<Value>, key: &str, val: Value) {
    let prop = Value::keyword(key);
    let mut i = 1;
    while i + 1 < elems.len() {
        if elems[i]
            .as_symbol_name()
            .map(|name| name.trim_start_matches(':'))
            == Some(key)
        {
            elems[i + 1] = val;
            return;
        }
        i += 2;
    }
    elems.push(prop);
    elems.push(val);
}

/// Canonicalize and store a weight/slant/width style word the way GNU's
/// `FONT_SET_STYLE` does: look the word up in the style table and store the
/// canonical symbol (neomacs stores the symbol; `font-face-attributes` reads it
/// back directly, matching GNU's `font_style_symbolic`).
fn font_parse_set_style(elems: &mut Vec<Value>, key: &str, word: &str) {
    if let Some(name) =
        font_style_table_for_key(key).and_then(|table| font_style_symbol_from_name(table, word))
    {
        font_parse_set(elems, key, Value::symbol(name));
    }
}

/// Try to interpret a fontconfig property word as a weight, slant or spacing
/// keyword (the bare-word case from GNU `font_parse_fcname`).
fn font_parse_fcname_enum_word(elems: &mut Vec<Value>, word: &str) {
    match word {
        "thin" | "ultra-light" | "light" | "semi-light" | "book" | "medium" | "normal"
        | "semibold" | "demibold" | "bold" | "ultra-bold" | "black" | "heavy" | "ultra-heavy" => {
            font_parse_set_style(elems, "weight", word);
        }
        "roman" | "italic" | "oblique" => {
            font_parse_set_style(elems, "slant", word);
        }
        "charcell" => font_parse_set(elems, "spacing", Value::fixnum(110)),
        "mono" => font_parse_set(elems, "spacing", Value::fixnum(100)),
        "proportional" => font_parse_set(elems, "spacing", Value::fixnum(0)),
        _ => {}
    }
}

/// Store a `key=val` fontconfig property.  Recognized keys map to basic
/// font-spec slots; unknown keys are dropped (GNU would route them to the
/// font driver's `filter_properties`, which has no effect on a bare spec).
fn font_parse_fcname_keyval(elems: &mut Vec<Value>, key: &str, val: &str) {
    match key {
        "pixelsize" => {
            if let Ok(n) = val.parse::<i64>() {
                font_parse_set(elems, "size", Value::fixnum(n));
            }
        }
        "size" => {
            if let Ok(f) = val.parse::<f64>() {
                font_parse_set(elems, "size", Value::make_float(f));
            } else if let Ok(n) = val.parse::<i64>() {
                font_parse_set(elems, "size", Value::fixnum(n));
            }
        }
        "weight" | "slant" | "width" => font_parse_set_style(elems, key, val),
        "spacing" => {
            if let Some(spacing) = FontSpacing::from_symbol_name(val) {
                font_parse_set(
                    elems,
                    "spacing",
                    Value::fixnum(i64::from(spacing.gnu_code())),
                );
            } else if let Ok(n) = val.parse::<i64>() {
                font_parse_set(elems, "spacing", Value::fixnum(n));
            }
        }
        "foundry" | "family" | "adstyle" | "lang" | "script" => {
            font_parse_set(elems, key, Value::symbol(val));
        }
        "registry" => font_parse_set(elems, "registry", Value::symbol(val.to_ascii_lowercase())),
        "dpi" => {
            if let Ok(n) = val.parse::<i64>() {
                font_parse_set(elems, "dpi", Value::fixnum(n));
            }
        }
        _ => {}
    }
}

/// Port of GNU `font_parse_fcname` (src/font.c): parse a fontconfig-style name
/// such as `"Monospace-10"`, `"Family:weight=bold"`, or `"Family-12:bold"` into
/// font-spec properties.  Returns `false` on an empty name (GNU `-1`).
fn font_parse_fcname(elems: &mut Vec<Value>, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let bytes = name.as_bytes();
    let mut family_end: Option<usize> = None;
    let mut size_beg: Option<usize> = None;
    let mut props_beg: Option<usize> = None;

    // Scan forward for the first ':' (property data) or a '-NN[.NN]' size run.
    let mut p = 0;
    while p < bytes.len() {
        let c = bytes[p];
        if c == b'\\' && p + 1 < bytes.len() {
            p += 2;
            continue;
        } else if c == b':' {
            props_beg = Some(p);
            family_end = Some(p);
            break;
        } else if c == b'-' {
            // Everything up to the next ':' must be digits (and at most one '.').
            let mut decimal = false;
            let mut size_found = true;
            let mut q = p + 1;
            while q < bytes.len() && bytes[q] != b':' {
                let cq = bytes[q];
                if !cq.is_ascii_digit() {
                    if cq != b'.' || decimal {
                        size_found = false;
                        break;
                    }
                    decimal = true;
                }
                q += 1;
            }
            // GNU requires at least one char after '-' to count as a size.
            if size_found && q > p + 1 {
                family_end = Some(p);
                size_beg = Some(p + 1);
                break;
            }
        }
        p += 1;
    }

    let Some(family_end) = family_end else {
        // No size and no property data: a plain family name (possibly GTK-style
        // with trailing style words / size separated by spaces).
        return font_parse_fcname_plain(elems, name);
    };

    // Family.
    if family_end > 0 {
        let family = unescape_fcname(&name[..family_end]);
        font_parse_set(elems, "family", Value::symbol(&family));
    }

    // Point size (stored as a float, matching GNU `make_float`).
    if let Some(size_beg) = size_beg {
        // Read the numeric run starting at size_beg.
        let rest = &name[size_beg..];
        let end = rest.find(':').unwrap_or(rest.len());
        let size_str = &rest[..end];
        if let Ok(f) = size_str.parse::<f64>() {
            font_parse_set(elems, "size", Value::make_float(f));
        }
        // If a ':' follows the size, properties start there.
        if size_beg + end < bytes.len() && bytes[size_beg + end] == b':' {
            props_beg = Some(size_beg + end);
        }
    }

    // Parse ":KEY=VAL" / ":enumword" properties.
    if let Some(props_beg) = props_beg {
        for segment in name[props_beg..].split(':') {
            if segment.is_empty() {
                continue;
            }
            if let Some(eq) = segment.find('=') {
                let key = &segment[..eq];
                let val = &segment[eq + 1..];
                font_parse_fcname_keyval(elems, key, val);
            } else {
                font_parse_fcname_enum_word(elems, segment);
            }
        }
    }

    true
}

/// Strip fontconfig quoting backslashes from a family name.
fn unescape_fcname(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// GTK / plain fontconfig name with no size or property delimiters, e.g.
/// `"Monospace"`, `"DejaVu Sans Bold 12"`.  Ported from the `else` branch of
/// GNU `font_parse_fcname`: scan backwards for a numeric size, then for known
/// style words, the remainder being the family.
fn font_parse_fcname_plain(elems: &mut Vec<Value>, name: &str) -> bool {
    let bytes = name.as_bytes();
    let len = bytes.len();

    // Scan backwards for a trailing numeric size (preceded by a space or BOS).
    let mut p = len;
    let mut size: Option<f64> = None;
    {
        let mut i = len;
        while i > 0 && bytes[i - 1].is_ascii_digit() {
            i -= 1;
        }
        if i < len
            && (i == 0 || bytes[i - 1] == b' ')
            && let Ok(f) = name[i..].parse::<f64>()
        {
            size = Some(f);
            // Drop the size (and a preceding space) from the family scan.
            p = if i > 0 { i - 1 } else { i };
        }
    }

    // Scan backwards over space-separated words, recognizing style keywords.
    let mut weight: Option<&str> = None;
    let mut slant: Option<&str> = None;
    let mut width: Option<&str> = None;
    let mut family_end = p;
    while p > 0 {
        // Find the start of the current word.
        let mut q = p;
        while q > 0 {
            if q > 1 && bytes[q - 2] == b'\\' {
                q -= 1;
            } else if bytes[q - 1] == b' ' {
                break;
            }
            q -= 1;
        }
        let word = &name[q..p];
        let matched = match word {
            "Ultra-Light" => {
                weight.get_or_insert("ultra-light");
                true
            }
            "Light" => {
                weight.get_or_insert("light");
                true
            }
            "Book" => {
                weight.get_or_insert("book");
                true
            }
            "Medium" => {
                weight.get_or_insert("medium");
                true
            }
            "Semi-Bold" => {
                weight.get_or_insert("semi-bold");
                true
            }
            "Bold" => {
                weight.get_or_insert("bold");
                true
            }
            "Italic" => {
                slant.get_or_insert("italic");
                true
            }
            "Oblique" => {
                slant.get_or_insert("oblique");
                true
            }
            "Semi-Condensed" => {
                width.get_or_insert("semi-condensed");
                true
            }
            "Condensed" => {
                width.get_or_insert("condensed");
                true
            }
            _ => false,
        };
        if !matched {
            family_end = p;
            break;
        }
        // Move past the space before this word.
        p = if q > 0 { q - 1 } else { 0 };
        family_end = q;
        if q == 0 {
            break;
        }
    }

    if family_end > 0 {
        font_parse_set(
            elems,
            "family",
            Value::symbol(unescape_fcname(&name[..family_end])),
        );
    }
    if let Some(f) = size {
        font_parse_set(elems, "size", Value::make_float(f));
    }
    if let Some(w) = weight {
        font_parse_set_style(elems, "weight", w);
    }
    if let Some(s) = slant {
        font_parse_set_style(elems, "slant", s);
    }
    if let Some(w) = width {
        font_parse_set_style(elems, "width", w);
    }
    true
}

/// XLFD field indices (GNU `enum xlfd_field_index`).
const XLFD_FOUNDRY: usize = 0;
const XLFD_FAMILY: usize = 1;
const XLFD_WEIGHT: usize = 2;
const XLFD_SLANT: usize = 3;
const XLFD_SWIDTH: usize = 4;
const XLFD_ADSTYLE: usize = 5;
const XLFD_PIXEL: usize = 6;
const XLFD_POINT: usize = 7;
const XLFD_RESX: usize = 8;
const XLFD_RESY: usize = 9;
const XLFD_SPACING: usize = 10;
const XLFD_AVGWIDTH: usize = 11;
const XLFD_REGISTRY: usize = 12;
const XLFD_ENCODING: usize = 13;
const XLFD_LAST: usize = 14;

/// Port of GNU `font_parse_xlfd` (src/font.c): parse a hyphen-delimited XLFD
/// name such as `"-misc-fixed-medium-r-normal--13-120-..."`.  Only the
/// fully-specified (14-field) form is handled here, which covers the names
/// `font-spec :name` is given in practice.  Returns `false` on parse failure.
fn font_parse_xlfd(elems: &mut Vec<Value>, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    // Split into fields on '-'.  GNU treats a leading "*-" specially; for the
    // fully-specified form we simply split on '-'.
    let fields: Vec<&str> = name.split('-').collect();

    // A fully specified XLFD has a leading '-', so split() yields an empty
    // first element followed by exactly 14 fields.
    if fields.len() != XLFD_LAST + 1 || !fields[0].is_empty() {
        return false;
    }
    let f = &fields[1..]; // 14 fields, indices XLFD_FOUNDRY..XLFD_ENCODING

    let intern_field = |idx: usize| -> &str { f[idx] };

    // Foundry / family (interned as symbols).
    if !f[XLFD_FOUNDRY].is_empty() && f[XLFD_FOUNDRY] != "*" {
        font_parse_set(elems, "foundry", Value::symbol(f[XLFD_FOUNDRY]));
    }
    if !f[XLFD_FAMILY].is_empty() && f[XLFD_FAMILY] != "*" {
        font_parse_set(elems, "family", Value::symbol(f[XLFD_FAMILY]));
    }

    // Weight / slant / width style fields.
    for (xlfd_idx, key) in [
        (XLFD_WEIGHT, "weight"),
        (XLFD_SLANT, "slant"),
        (XLFD_SWIDTH, "width"),
    ] {
        let word = intern_field(xlfd_idx);
        if !word.is_empty() && word != "*" {
            font_parse_set_style(elems, key, word);
        }
    }

    // Adstyle: GNU stores the interned field unconditionally for a fully
    // specified XLFD (an empty field becomes the empty symbol `##`).
    let adstyle = intern_field(XLFD_ADSTYLE);
    if adstyle != "*" {
        font_parse_set(elems, "adstyle", Value::symbol(adstyle));
    }

    // Registry-encoding: "registry-encoding" combined.
    let registry = intern_field(XLFD_REGISTRY);
    let encoding = intern_field(XLFD_ENCODING);
    if !(registry == "*" && encoding == "*") {
        let combined = format!("{registry}-{encoding}");
        font_parse_set(
            elems,
            "registry",
            Value::symbol(combined.to_ascii_lowercase()),
        );
    }

    // Size: prefer pixel size (fixnum), else point size / 10 (float).
    let pixel = intern_field(XLFD_PIXEL);
    if let Ok(px) = pixel.parse::<i64>() {
        if px > 0 {
            font_parse_set(elems, "size", Value::fixnum(px));
        }
    } else {
        let point = intern_field(XLFD_POINT);
        if let Ok(pt) = point.parse::<i64>() {
            font_parse_set(elems, "size", Value::make_float(pt as f64 / 10.0));
        }
    }

    // DPI (resolution-y).
    let resy = intern_field(XLFD_RESY);
    if let Ok(dpi) = resy.parse::<i64>() {
        font_parse_set(elems, "dpi", Value::fixnum(dpi));
    }
    let _ = intern_field(XLFD_RESX);

    // Spacing letter (p/d/m/c).
    let spacing = intern_field(XLFD_SPACING);
    if let Some(sp) = FontSpacing::from_symbol_name(spacing) {
        font_parse_set(elems, "spacing", Value::fixnum(i64::from(sp.gnu_code())));
    }

    // Average width.
    let avg = intern_field(XLFD_AVGWIDTH).trim_start_matches('~');
    if let Ok(n) = avg.parse::<i64>() {
        font_parse_set(elems, "avgwidth", Value::fixnum(n));
    }

    true
}

/// Port of GNU `font_parse_name` (src/font.c): dispatch a font NAME string to
/// the XLFD or fontconfig parser and store the parsed properties into ELEMS
/// (a font-spec property vector).  Returns `false` if the name cannot be parsed.
fn font_parse_name(elems: &mut Vec<Value>, name: &str) -> bool {
    if name.starts_with('-') || name.contains('*') || name.contains('?') {
        font_parse_xlfd(elems, name)
    } else {
        font_parse_fcname(elems, name)
    }
}

/// Build a font-spec from a font NAME string (GNU `font_spec_from_name`):
/// parse NAME, then record it under `:name`.  Returns `None` on parse failure.
fn font_spec_from_name(name: &str) -> Option<Value> {
    let mut elems = vec![Value::keyword(FONT_SPEC_TAG)];
    if !font_parse_name(&mut elems, name) {
        return None;
    }
    font_parse_set(&mut elems, "name", Value::string(name.to_string()));
    Some(Value::vector(elems))
}

/// `(font-face-attributes FONT &optional FRAME)` -- return a plist of face
/// attributes generated by FONT.  Port of GNU `Ffont_face_attributes`
/// (src/font.c): FONT may be a font name string (parsed via
/// `font_spec_from_name`), a font-spec, font-entity, or font-object.  The result
/// is `(:family F :height H :weight W :slant S :width WD)` with absent keys
/// omitted.
pub(crate) fn font_face_attributes(args: Vec<Value>) -> EvalResult {
    expect_min_args("font-face-attributes", &args, 1)?;
    expect_max_args("font-face-attributes", &args, 2)?;

    let font = if args[0].is_string() {
        let name = font_string_text(&args[0]).unwrap_or_default();
        match font_spec_from_name(&name) {
            Some(spec) => spec,
            None => {
                return Err(signal(
                    "error",
                    vec![Value::string("Invalid font name"), args[0]],
                ));
            }
        }
    } else if is_font(&args[0]) {
        args[0]
    } else {
        return Err(signal(
            "error",
            vec![Value::string("Invalid font object"), args[0]],
        ));
    };

    let elems = font_value_fields(&font).expect("validated font values expose property slots");
    let mut plist: Vec<Value> = Vec::with_capacity(10);

    // :family (symbol name -> string).
    if let Some(family) = font_vector_get_flexible(&elems, "family")
        && !family.is_nil()
    {
        let family_str = match family.kind() {
            ValueKind::Symbol(id) => Value::string(resolve_sym(id).to_owned()),
            ValueKind::String => family,
            _ => Value::NIL,
        };
        if !family_str.is_nil() {
            plist.push(Value::keyword("family"));
            plist.push(family_str);
        }
    }

    // :height -- GNU maps the font size to a face height (10 * point size).
    // A fixnum size is a pixel size converted via PIXEL_TO_POINT; with no
    // display DPI here we follow GNU's float path (point size) for parsed
    // names, where size is stored as a float.
    if let Some(size) = font_vector_get_flexible(&elems, "size") {
        match size.kind() {
            ValueKind::Float => {
                let pts = size.xfloat();
                if pts > 0.0 {
                    plist.push(Value::keyword("height"));
                    plist.push(Value::fixnum(10 * (pts as i64)));
                }
            }
            ValueKind::Fixnum(px) if px > 0 => {
                // Pixel size: GNU converts via the frame resolution.  Without a
                // live display we approximate point size == pixel size (the
                // common 72-dpi identity used in batch contexts).
                plist.push(Value::keyword("height"));
                plist.push(Value::fixnum(px * 10));
            }
            _ => {}
        }
    }

    // :weight / :slant / :width -- GNU `Ffont_face_attributes` reads these via
    // the FONT_*_FOR_FACE macros (font_style_symbolic with for_face=true), which
    // canonicalize the stored alias to its row's preferred name
    // (heavy -> black, ultra-bold -> extra-bold, normal -> regular). The
    // storage path keeps the alias verbatim (matching `font-get`), so the
    // canonicalization happens here, at the face-read boundary.
    for key in ["weight", "slant", "width"] {
        if let Some(val) = font_vector_get_flexible(&elems, key)
            && !val.is_nil()
        {
            let canonical = val
                .as_symbol_name()
                .and_then(|name| font_style_canonical_for_face(key, name))
                .map(Value::symbol)
                .unwrap_or(val);
            plist.push(Value::keyword(key));
            plist.push(canonical);
        }
    }

    Ok(Value::list(plist))
}

// ===========================================================================
// Font builtins (pure)
// ===========================================================================

/// `(fontp OBJECT &optional EXTRA-TYPE)` -- return t if OBJECT is a font-spec,
/// font-entity, or font-object.  We represent all of these as tagged vectors
/// with `:font-spec` keyword at position 0.
pub(crate) fn fontp(args: Vec<Value>) -> EvalResult {
    expect_max_args("fontp", &args, 2)?;
    expect_min_args("fontp", &args, 1)?;
    let object = &args[0];
    let extra_type = args.get(1).copied().unwrap_or(Value::NIL);
    let value = if extra_type.is_nil() {
        is_font(object)
    } else if extra_type.is_symbol_named("font-spec") {
        is_font_spec(object)
    } else if extra_type.is_symbol_named("font-object") {
        is_font_object(object)
    } else if extra_type.is_symbol_named("font-entity") {
        is_font_entity(object)
    } else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("font-extra-type"), extra_type],
        ));
    };
    Ok(Value::bool_val(value))
}

/// `(font-spec &rest ARGS)` -- create a font spec from keyword args.
///
/// Usage: `(font-spec :family "Monospace" :weight 'normal :size 12)`
///
/// Returns a vector `[:font-spec :family "Monospace" :weight normal :size 12]`.
pub(crate) fn font_spec(args: Vec<Value>) -> EvalResult {
    let mut elems: Vec<Value> = Vec::with_capacity(1 + args.len());
    elems.push(Value::keyword(FONT_SPEC_TAG));

    for pair_index in (0..args.len()).step_by(2) {
        let key = &args[pair_index];
        let value = args.get(pair_index + 1);

        let Some(value) = value else {
            if key.is_keyword() || key.is_symbol() || key.is_nil() {
                let key_name = match key.kind() {
                    ValueKind::Symbol(id) => resolve_sym(id).to_owned(),
                    ValueKind::Nil => "nil".to_string(),
                    _ => "nil".to_string(),
                };
                return Err(signal(
                    "error",
                    vec![Value::string(format!("No value for key ‘{}’", key_name))],
                ));
            }
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), *key],
            ));
        };

        if key.is_nil() {
            return Err(signal(
                "error",
                vec![
                    Value::string("invalid font property"),
                    Value::list(vec![Value::cons(Value::keyword("type"), *value)]),
                ],
            ));
        }

        if !(key.is_keyword() || key.is_symbol()) {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), *key],
            ));
        }

        // GNU `Ffont_spec`: a `:name` argument is a font name string that is
        // parsed via `font_parse_name` into the spec's basic slots; the name
        // itself is also recorded under `:name`.
        if key
            .as_symbol_name()
            .map(|name| name.trim_start_matches(':'))
            == Some("name")
        {
            let Some(name) = font_string_text(value) else {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), *value],
                ));
            };
            if !font_parse_name(&mut elems, &name) {
                return Err(signal(
                    "error",
                    vec![Value::string(format!("Invalid font name: {name}"))],
                ));
            }
            font_parse_set(&mut elems, "name", *value);
            continue;
        }

        elems.push(*key);
        elems.push(normalize_font_prop_value(key, value)?);
    }

    Ok(Value::vector(elems))
}

/// `(font-get FONT PROP)` -- get a property value from a font-spec.
pub(crate) fn font_get(args: Vec<Value>) -> EvalResult {
    expect_args("font-get", &args, 2)?;
    if !is_font(&args[0]) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("font"), args[0]],
        ));
    }
    if !(args[1].is_keyword() || args[1].is_symbol()) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[1]],
        ));
    }

    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::Vector | VecLikeType::Font) => {
            let elems =
                font_value_fields(&args[0]).expect("validated font value exposes properties");
            let exact = font_vector_get(&elems, &args[1]);
            if !exact.is_nil() {
                return Ok(exact);
            }

            if let Some(id) = args[1].as_keyword_id() {
                return Ok(font_vector_get_flexible(&elems, resolve_sym(id)).unwrap_or(Value::NIL));
            }

            Ok(Value::NIL)
        }
        _ => unreachable!("font check above guarantees property storage"),
    }
}

/// `(font-put FONT PROP VAL)` -- set a property in a font-spec and return VAL.
pub(crate) fn font_put(args: Vec<Value>) -> EvalResult {
    expect_args("font-put", &args, 3)?;
    if !is_font_spec(&args[0]) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("font-spec"), args[0]],
        ));
    }
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let mut elems = args[0]
                .as_vector_data()
                .map(|items| items.to_vec())
                .unwrap_or_default();
            let normalized = font_spec_put(&mut elems, &args[1], &args[2])?;
            let _ = args[0].replace_vector_data(elems);
            Ok(normalized)
        }
        _ => unreachable!("font-spec check above guarantees vector"),
    }
}

/// Context-aware variant of `list-fonts`.
///
/// Accepts live frame designators in the optional FRAME slot.
pub(crate) fn list_fonts(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("list-fonts", &args, 1)?;
    expect_max_args("list-fonts", &args, 4)?;
    if !is_font_spec(&args[0]) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("font-spec"), args[0]],
        ));
    }
    expect_optional_frame_designator_in_state(&eval.frames, args.get(1))?;
    Ok(Value::NIL)
}

fn font_weight_from_value(value: Value) -> Option<FontWeight> {
    match value.kind() {
        ValueKind::Symbol(id) => FontWeight::from_symbol(resolve_sym(id)),
        _ => None,
    }
}

fn font_slant_from_value(value: Value) -> Option<FontSlant> {
    match value.kind() {
        ValueKind::Symbol(id) => FontSlant::from_symbol(resolve_sym(id)),
        _ => None,
    }
}

fn find_font_frame_id(
    eval: &mut super::eval::Context,
    frame: Option<&Value>,
) -> Result<FrameId, Flow> {
    match frame {
        None => Ok(super::window_cmds::ensure_selected_frame_id(eval)),
        Some(v) if v.is_nil() => Ok(super::window_cmds::ensure_selected_frame_id(eval)),
        Some(value) if live_frame_designator_in_state(&eval.frames, value) => {
            frame_id_from_designator(value).ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("frame-live-p"), *value],
                )
            })
        }
        Some(other) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *other],
        )),
    }
}

fn font_spec_resolve_request(
    eval: &mut super::eval::Context,
    font_spec: &Value,
    frame: Option<&Value>,
) -> Result<super::eval::FontSpecResolveRequest, Flow> {
    if !font_spec.is_vector() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("font-spec"), *font_spec],
        ));
    };

    let elems = font_spec.as_vector_data().unwrap().clone();
    let family = font_vector_get_flexible(&elems, "family")
        .and_then(|value| font_value_text_lisp_string(&value));
    let registry = font_vector_get_flexible(&elems, "registry")
        .and_then(|value| font_value_text_lisp_string(&value));
    let lang = font_vector_get_flexible(&elems, "lang")
        .and_then(|value| font_value_text_lisp_string(&value));
    let weight = font_vector_get_flexible(&elems, "weight").and_then(font_weight_from_value);
    let slant = font_vector_get_flexible(&elems, "slant").and_then(font_slant_from_value);
    let width = font_vector_get_flexible(&elems, "width").and_then(|value| match value.kind() {
        ValueKind::Symbol(id) => FontWidth::from_symbol(resolve_sym(id)),
        _ => None,
    });

    Ok(super::eval::FontSpecResolveRequest {
        frame_id: find_font_frame_id(eval, frame)?,
        family,
        registry,
        lang,
        weight,
        slant,
        width,
    })
}

/// Context-aware variant of `find-font`.
///
/// Accepts live frame designators in the optional FRAME slot.
pub(crate) fn find_font(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("find-font", &args, 1)?;
    expect_max_args("find-font", &args, 2)?;
    if !is_font_spec(&args[0]) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("font-spec"), args[0]],
        ));
    }

    let request = font_spec_resolve_request(eval, &args[0], args.get(1))?;
    let Some(host) = eval.display_host.as_mut() else {
        return Ok(Value::NIL);
    };
    let matched = host
        .resolve_font_for_spec(request)
        .map_err(|err| signal("error", vec![Value::string(err)]))?;
    let Some(matched) = matched else {
        return Ok(Value::NIL);
    };
    Ok(build_font_entity_for_spec_match(&matched))
}

/// `(clear-font-cache)` -- reset internal font/face caches and return nil.
pub(crate) fn clear_font_cache(args: Vec<Value>) -> EvalResult {
    expect_max_args("clear-font-cache", &args, 0)?;
    clear_font_cache_state();
    Ok(Value::NIL)
}

/// Context-aware variant of `font-family-list`.
///
/// Accepts live frame designators in the optional FRAME slot.
pub(crate) fn font_family_list(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_max_args("font-family-list", &args, 1)?;
    let frame_id = find_font_frame_id(eval, args.first())?;
    let Some(host) = eval.display_host.as_mut() else {
        return Ok(Value::NIL);
    };
    let families = host
        .list_font_families(frame_id)
        .map_err(|err| signal("error", vec![Value::string(err)]))?;
    Ok(Value::list(
        families
            .into_iter()
            .map(|family| Value::heap_string(family.into_lisp_string()))
            .collect(),
    ))
}

/// `(font-xlfd-name FONT &optional FOLD-WILDCARDS)` -- render font-spec fields
/// into an XLFD string; wildcard folding is supported in compatibility mode.
pub(crate) fn font_xlfd_name(args: Vec<Value>) -> EvalResult {
    expect_min_args("font-xlfd-name", &args, 1)?;
    expect_max_args("font-xlfd-name", &args, 3)?;
    if !is_font(&args[0]) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("font"), args[0]],
        ));
    }

    let elems = font_value_fields(&args[0]).expect("validated font value exposes properties");
    if is_font_object(&args[0])
        && font_vector_get_flexible(elems, "name").is_some_and(|v| v.is_string())
    {
        let font_name = font_vector_get_flexible(elems, "name")
            .unwrap()
            .as_utf8_str()
            .unwrap()
            .to_owned();
        if font_name.starts_with('-') {
            return Ok(Value::string(
                if args.get(1).is_some_and(|v| v.is_truthy()) {
                    fold_xlfd_wildcards(font_name)
                } else {
                    font_name
                },
            ));
        }
    }
    let fields = xlfd_fields_from_font_vector(elems);

    let (
        foundry,
        family,
        weight,
        slant,
        set_width,
        adstyle,
        pixel,
        resx,
        spacing,
        avg_width,
        registry,
    ) = fields;
    let rendered = if args.get(1).is_some_and(|v| v.is_truthy()) {
        let name = format!(
            "-{}-{}-{}-{}-{}-{}-{}-{}-{}-{}-{}",
            foundry,
            family,
            weight,
            slant,
            set_width,
            adstyle,
            pixel,
            resx,
            spacing,
            avg_width,
            registry
        );
        fold_xlfd_wildcards(name)
    } else {
        format!(
            "-{}-{}-{}-{}-{}-{}-{}-{}-{}-{}-{}",
            foundry,
            family,
            weight,
            slant,
            set_width,
            adstyle,
            pixel,
            resx,
            spacing,
            avg_width,
            registry
        )
    };
    Ok(Value::string(rendered))
}

/// `(close-font FONT-OBJECT &optional FRAME)` -- close an open font object.
///
/// NeoVM currently has no runtime font-object handles, so this validates the
/// argument shape and returns nil for accepted objects.
pub(crate) fn close_font(args: Vec<Value>) -> EvalResult {
    expect_min_args("close-font", &args, 1)?;
    expect_max_args("close-font", &args, 2)?;
    if !is_font_object(&args[0]) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("font-object"), args[0]],
        ));
    }
    Ok(Value::NIL)
}

#[derive(Clone, Debug)]
enum FaceLayer {
    Named(Vec<String>),
    Inline(RuntimeFace),
}

fn window_id_from_designator(value: &Value) -> Option<WindowId> {
    match value.kind() {
        ValueKind::Veclike(VecLikeType::Window) => Some(WindowId(value.as_window_id().unwrap())),
        ValueKind::Fixnum(n) if n >= 0 => Some(WindowId(n as u64)),
        _ => None,
    }
}

fn resolve_live_window_for_font_at(
    eval: &mut super::eval::Context,
    value: Option<&Value>,
) -> Result<(FrameId, WindowId), Flow> {
    match value {
        None => {
            let frame_id = super::window_cmds::ensure_selected_frame_id(eval);
            let frame = eval
                .frames
                .get(frame_id)
                .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
            Ok((frame_id, frame.selected_window))
        }
        Some(v) if v.is_nil() => {
            let frame_id = super::window_cmds::ensure_selected_frame_id(eval);
            let frame = eval
                .frames
                .get(frame_id)
                .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
            Ok((frame_id, frame.selected_window))
        }
        Some(other) => {
            let Some(window_id) = window_id_from_designator(other) else {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("window-live-p"), *other],
                ));
            };
            let Some(frame_id) = eval.frames.find_window_frame_id(window_id) else {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("window-live-p"), *other],
                ));
            };
            Ok((frame_id, window_id))
        }
    }
}

fn resolve_face_layers_from_value(value: &Value) -> Vec<FaceLayer> {
    match value.kind() {
        ValueKind::Nil => Vec::new(),
        ValueKind::Symbol(_) => value
            .as_symbol_name()
            .filter(|name| *name != "nil")
            .map(|name| vec![FaceLayer::Named(vec![name.to_string()])])
            .unwrap_or_default(),
        ValueKind::Cons => {
            let Some(items) = list_to_vec(value) else {
                return Vec::new();
            };
            if items.first().is_some_and(|item| item.is_keyword()) {
                vec![FaceLayer::Inline(RuntimeFace::from_plist(
                    "--font-at--",
                    &items,
                ))]
            } else {
                let names = items
                    .iter()
                    .filter_map(|item| {
                        item.as_symbol_name()
                            .filter(|name| *name != "nil")
                            .map(|name| name.to_string())
                    })
                    .collect::<Vec<_>>();
                if names.is_empty() {
                    Vec::new()
                } else {
                    vec![FaceLayer::Named(names)]
                }
            }
        }
        _ => Vec::new(),
    }
}

/// Extract the `face-remapping-alist` for a specific buffer.
///
/// Checks the buffer-local binding first; falls back to the global value.
fn face_remapping_value_for_buffer(eval: &super::eval::Context, buffer: &Buffer) -> Value {
    // Buffer-local binding takes priority
    buffer
        .get_buffer_local("face-remapping-alist")
        .or_else(|| eval.obarray().symbol_value("face-remapping-alist").copied())
        .unwrap_or(Value::NIL)
}

fn face_remapping_for_buffer(eval: &super::eval::Context, buffer: &Buffer) -> FaceRemapping {
    let value = face_remapping_value_for_buffer(eval, buffer);

    if value.is_nil() {
        FaceRemapping::new()
    } else {
        FaceRemapping::from_lisp(&value)
    }
}

fn face_remapping_value_for_current_buffer(eval: &super::eval::Context) -> Value {
    eval.buffers
        .current_buffer()
        .map(|buffer| face_remapping_value_for_buffer(eval, buffer))
        .unwrap_or_else(|| {
            // Reached only when there is NO current buffer, a state GNU never
            // occupies -- `Vface_remapping_alist` (`src/xfaces.c:7662`) is only
            // ever read during redisplay, with the window's buffer current. So
            // the localized arm is accepted here on purpose: there is no buffer
            // to ask. Ledger 196.
            eval.obarray()
                .value_without_buffer("face-remapping-alist")
                .any_arm()
                .unwrap_or(Value::NIL)
        })
}

/// Resolve the default face after applying the current buffer's local face
/// remapping and ask the display host for its actual cell metrics.
///
/// This is the evaluator-side equivalent of GNU `lookup_named_face` in
/// `window_body_width`/`window_body_height`: GNU reads the buffer-local
/// `Vface_remapping_alist` of the current buffer even when the query names a
/// different window.  It returns `None` when no remapping is active or when no
/// live host can realize the face; callers then use canonical frame metrics.
pub(crate) fn resolve_current_buffer_remapped_default_face_font(
    eval: &mut super::eval::Context,
    frame_id: FrameId,
) -> Option<super::eval::ResolvedFrameFont> {
    let remapping_value = face_remapping_value_for_current_buffer(eval);
    if remapping_value.is_nil() {
        return None;
    }

    let remapping = FaceRemapping::from_lisp(&remapping_value);
    let face_table = runtime_face_table_from_frame_lisp_faces(eval, frame_id, true);
    let remapped_default = face_table.resolve_with_remapping("default", &remapping);
    eval.display_host
        .as_mut()?
        .resolve_frame_font(frame_id, FrameFontRequest::from_face(remapped_default))
        .ok()
        .flatten()
}

/// Extract the `face-remapping-alist` from the current buffer (if any).
pub(crate) fn face_remapping_for_current_buffer(eval: &super::eval::Context) -> FaceRemapping {
    let value = face_remapping_value_for_current_buffer(eval);
    if value.is_nil() {
        FaceRemapping::new()
    } else {
        FaceRemapping::from_lisp(&value)
    }
}

fn apply_face_layers_with_remapping(
    face_table: &crate::face::FaceTable,
    layers: &[FaceLayer],
    remapping: &FaceRemapping,
) -> RuntimeFace {
    let mut face = if remapping.is_empty() {
        face_table.resolve("default")
    } else {
        face_table.resolve_with_remapping("default", remapping)
    };
    for layer in layers {
        match layer {
            FaceLayer::Named(names) => {
                let refs = names.iter().map(String::as_str).collect::<Vec<_>>();
                let merged = if remapping.is_empty() {
                    face_table.merge_faces(&refs)
                } else {
                    face_table.merge_faces_with_remapping(&refs, remapping)
                };
                face = face.merge(&merged);
            }
            FaceLayer::Inline(inline_face) => {
                face = face.merge(inline_face);
            }
        }
    }
    face
}

fn resolved_face_at_buffer_byte(
    eval: &super::eval::Context,
    face_table: &crate::face::FaceTable,
    buffer: &Buffer,
    bytepos: EmacsBytePos,
) -> RuntimeFace {
    let mut layers = Vec::new();

    let face_prop =
        buffer.text_props_get_property_at_emacs_byte_pos(bytepos, Value::symbol("face"));
    let font_lock_face_prop =
        buffer.text_props_get_property_at_emacs_byte_pos(bytepos, Value::symbol("font-lock-face"));
    if let Some(value) = face_prop.or(font_lock_face_prop) {
        layers.extend(resolve_face_layers_from_value(&value));
    }

    let mut overlay_layers = Vec::new();
    for overlay_id in buffer.overlays.iter_overlays_at_emacs_byte_pos(bytepos) {
        let priority = buffer
            .overlays
            .overlay_get_named(overlay_id, Value::symbol("priority"))
            .and_then(|value| value.as_int())
            .unwrap_or(0);
        if let Some(value) = buffer
            .overlays
            .overlay_get_named(overlay_id, Value::symbol("face"))
        {
            let resolved = resolve_face_layers_from_value(&value);
            if !resolved.is_empty() {
                overlay_layers.push((priority, resolved));
            }
        }
    }
    overlay_layers.sort_by_key(|(priority, _)| *priority);
    for (_, resolved) in overlay_layers {
        layers.extend(resolved);
    }

    // Consult buffer-local face-remapping-alist
    let remapping = face_remapping_for_buffer(eval, buffer);
    apply_face_layers_with_remapping(face_table, &layers, &remapping)
}

fn resolved_face_at_string_char_pos(
    eval: &super::eval::Context,
    face_table: &crate::face::FaceTable,
    str_value: Value,
    char_pos: CharPos0,
) -> RuntimeFace {
    let mut layers = Vec::new();
    if let Some(table) = get_string_text_properties_table_for_value(str_value) {
        let face_prop = table.get_property_at_char_pos(char_pos, Value::symbol("face"));
        let font_lock_face_prop =
            table.get_property_at_char_pos(char_pos, Value::symbol("font-lock-face"));
        if let Some(value) = face_prop.or(font_lock_face_prop) {
            layers.extend(resolve_face_layers_from_value(&value));
        }
    }
    // Use face-remapping-alist from the current buffer (strings inherit
    // the buffer context they're displayed in).
    let remapping = face_remapping_for_current_buffer(eval);
    apply_face_layers_with_remapping(face_table, &layers, &remapping)
}

fn face_height_to_font_value(height: &FaceHeight) -> Value {
    match height {
        FaceHeight::Absolute(n) => Value::fixnum(*n as i64),
        FaceHeight::Relative(f) => Value::make_float(*f),
    }
}

fn font_weight_symbol(weight: FontWeight) -> &'static str {
    weight.symbol_name()
}

#[cfg(test)]
pub(crate) fn build_font_object(face: &RuntimeFace) -> Value {
    build_font_object_with_pixel_size(face, None)
}

/// GNU font objects carry the OPENED pixel size in FONT_SIZE (the XLFD's
/// pixel field prints it); pass `pixel_size` when the resolver knows it.
#[cfg(test)]
fn build_font_object_with_pixel_size(face: &RuntimeFace, pixel_size: Option<i64>) -> Value {
    let fields = font_object_property_fields(face, pixel_size);
    let stable_name = font_name_for_face(face)
        .as_runtime_string_owned()
        .unwrap_or_else(|| "test-font".to_string());
    finish_opened_font(
        fields,
        None,
        None,
        OpenedFontMetrics::fallback(pixel_size.unwrap_or(1)),
        Value::NIL,
        neomacs_display_protocol::font::ResolvedFontIdentity::from_memory(
            neomacs_display_protocol::font::FontBackendKind::Fontconfig,
            format!("test:{stable_name}"),
            0,
            None,
        ),
    )
}

/// Render an unresolved face request to its public XLFD without pretending
/// that the request is an opened font object.
pub(crate) fn font_name_for_face(face: &RuntimeFace) -> Value {
    let mut fields = font_object_property_fields(face, None);
    fields[0] = Value::keyword(FONT_ENTITY_TAG);
    font_xlfd_name(vec![Value::vector(fields)]).unwrap_or(Value::NIL)
}

fn font_object_property_fields(face: &RuntimeFace, pixel_size: Option<i64>) -> Vec<Value> {
    let mut elems = vec![Value::keyword(FONT_OBJECT_TAG)];

    let mut push_field = |name: &str, value: Value| {
        elems.push(Value::keyword(name));
        elems.push(value);
    };

    if let Some(foundry) = face
        .foundry
        .as_ref()
        .and_then(font_value_text)
        .map(|text| Value::from_sym_id(intern(&text)))
    {
        push_field("foundry", foundry);
    }
    if let Some(family) = face
        .family
        .as_ref()
        .and_then(font_value_text)
        .map(|text| Value::from_sym_id(intern(&text)))
    {
        push_field("family", family);
    }
    // GNU's canonical style-table first names, as on entities.
    if let Some(weight) = face.weight {
        let name = font_weight_symbol(weight);
        let name = gnu_style_first_name(GNU_WEIGHT_TABLE, name).unwrap_or(name);
        push_field("weight", Value::symbol(name));
    }
    if let Some(slant) = face.slant {
        let name = slant.symbol_name();
        let name = gnu_style_first_name(GNU_SLANT_TABLE, name).unwrap_or(name);
        push_field("slant", Value::symbol(name));
    }
    if let Some(width) = face.width {
        let name = width.symbol_name();
        let name = gnu_style_first_name(GNU_WIDTH_TABLE, name).unwrap_or(name);
        push_field("width", Value::symbol(name));
    }
    if let Some(height) = &face.height {
        push_field("height", face_height_to_font_value(height));
    }
    if let Some(px) = pixel_size {
        push_field("size", Value::fixnum(px));
    } else if let Some(height) = &face.height {
        push_field("size", face_height_to_font_value(height));
    }
    if pixel_size.is_some() {
        // A resolver-opened font: like GNU's opened font objects, carry the
        // entity registry and the scalable avg-width 0 so the object XLFD
        // ends "-0-iso10646-1", not "-*-*".
        push_field("registry", Value::from_sym_id(intern("iso10646-1")));
        push_field("avg-width", Value::fixnum(0));
    }

    elems
}

fn finish_opened_font(
    mut fields: Vec<Value>,
    file: Option<&LispString>,
    full_name: Option<&LispString>,
    metrics: OpenedFontMetrics,
    capability: Value,
    identity: ResolvedFontIdentity,
) -> Value {
    // Render the public XLFD before changing the representation from an
    // ordinary property vector to the opaque `PVEC_FONT` tag.
    let mut xlfd_fields = fields.clone();
    xlfd_fields[0] = Value::keyword(FONT_ENTITY_TAG);
    let xlfd_source = Value::vector(xlfd_fields);
    let name = font_xlfd_name(vec![xlfd_source]).unwrap_or(Value::NIL);
    fields.push(Value::keyword("name"));
    fields.push(name);
    fields.push(Value::keyword("full-name"));
    fields.push(full_name.cloned().map(Value::heap_string).unwrap_or(name));
    fields.push(Value::keyword("file"));
    fields.push(file.cloned().map(Value::heap_string).unwrap_or(Value::NIL));
    Value::make_font(FontObjectData {
        fields: fields.into(),
        metrics,
        capability,
        identity,
    })
}

fn build_font_entity_for_spec_match(matched: &super::eval::ResolvedFontSpecMatch) -> Value {
    let mut elems = vec![Value::keyword(FONT_ENTITY_TAG)];

    let mut push_field = |name: &str, value: Value| {
        elems.push(Value::keyword(name));
        elems.push(value);
    };

    // GNU orders entity fields foundry-first (XLFD order); the foundry is
    // a symbol (e.g. GOOG) read from fontconfig FC_FOUNDRY.
    if let Some(foundry) = &matched.foundry {
        push_field(
            "foundry",
            Value::from_sym_id(intern(foundry.as_utf8_str().unwrap_or_default())),
        );
    }
    push_field(
        "family",
        Value::from_sym_id(intern(matched.family.as_utf8_str().unwrap_or_default())),
    );
    if let Some(registry) = &matched.registry {
        push_field(
            "registry",
            Value::from_sym_id(intern(registry.as_utf8_str().unwrap_or_default())),
        );
    }
    // Style symbols use GNU's canonical (first) style-table name —
    // font-get on a GNU entity reports e.g. `ultra-light`, never the
    // `extralight` alias; the XLFD's dashless spelling falls out of
    // `sanitize_style_field` stripping the dash.
    if let Some(weight) = matched.weight {
        let name = font_weight_symbol(weight);
        let name = gnu_style_first_name(GNU_WEIGHT_TABLE, name).unwrap_or(name);
        push_field("weight", Value::symbol(name));
    }
    if let Some(slant) = matched.slant {
        let name = slant.symbol_name();
        let name = gnu_style_first_name(GNU_SLANT_TABLE, name).unwrap_or(name);
        push_field("slant", Value::symbol(name));
    }
    if let Some(width) = matched.width {
        let name = width.symbol_name();
        let name = gnu_style_first_name(GNU_WIDTH_TABLE, name).unwrap_or(name);
        push_field("width", Value::symbol(name));
    }
    if let Some(spacing) = matched.spacing {
        push_field("spacing", Value::fixnum(spacing as i64));
    }
    if let Some(postscript_name) = &matched.postscript_name {
        push_field(
            "postscript-name",
            Value::heap_string(postscript_name.clone()),
        );
    }
    if let Some(file) = &matched.file {
        push_field("file", Value::heap_string(file.clone()));
    }
    // Scalable entities carry average width 0 (GNU src/ftfont.c sets
    // FONT_AVGWIDTH_INDEX to 0); the XLFD renders it as "0", not "*".
    push_field("avg-width", Value::fixnum(0));

    Value::vector(elems)
}

/// Materialize the core's opaque opened-font object from one host resolution.
/// This is the sole host-to-Lisp construction boundary: identity, metrics,
/// capability, and public properties enter together and cannot drift later.
pub fn opened_font_from_resolved_match(
    face: &RuntimeFace,
    matched: &super::eval::ResolvedFontMatch,
) -> Value {
    let opened = &matched.font;
    let canonical = &opened.resolved;
    let mut selected = face.clone();
    selected.family = Some(Value::from_sym_id(intern(&canonical.family)));
    selected.foundry = opened
        .foundry
        .as_ref()
        .map(|foundry| Value::from_sym_id(intern(foundry.as_utf8_str().unwrap_or_default())))
        .or(face.foundry);
    selected.weight = Some(FontWeight::from_css_weight(canonical.weight));
    selected.slant = Some(opened.slant);
    selected.width = Some(opened.width());
    let mut fields =
        font_object_property_fields(&selected, Some(i64::from(opened.metrics.pixel_size.max(1))));
    if let Some(postscript_name) = &canonical.postscript_name {
        fields.push(Value::keyword("postscript-name"));
        fields.push(Value::string(postscript_name.clone()));
    }
    finish_opened_font(
        fields,
        canonical
            .identity
            .file_path
            .as_deref()
            .map(LispString::from_utf8)
            .as_ref(),
        canonical
            .full_name
            .as_deref()
            .map(LispString::from_utf8)
            .as_ref(),
        OpenedFontMetrics::from_probe(opened.metrics),
        opened
            .capability
            .as_ref()
            .map(otf_capability_to_lisp)
            .unwrap_or(Value::NIL),
        canonical.identity.clone(),
    )
}

pub(crate) fn font_name_value(font_like: &Value) -> Option<Value> {
    match font_like.kind() {
        ValueKind::String => Some(*font_like),
        ValueKind::Veclike(VecLikeType::Vector | VecLikeType::Font) if is_font(font_like) => {
            let elems = font_value_fields(font_like)?;
            if let Some(value) = font_vector_get_flexible(&elems, "name") {
                return match value.kind() {
                    ValueKind::String => Some(value),
                    ValueKind::Symbol(sym) => Some(Value::string(resolve_sym(sym).to_owned())),
                    _ => None,
                };
            }
            match font_xlfd_name(vec![*font_like]) {
                Ok(v) if v.is_string() => Some(v),
                _ => None,
            }
        }
        _ => None,
    }
}

pub(crate) fn public_frame_font_parameter_value(font_like: Value) -> Value {
    if is_font(&font_like) {
        font_name_value(&font_like).unwrap_or(font_like)
    } else {
        font_like
    }
}

fn font_value_matches_frame_font_parameter(
    frame: &crate::window::Frame,
    requested: &Value,
) -> bool {
    let Some(frame_font) = frame.known_parameter(FrameParam::Font) else {
        return false;
    };
    match (frame_font.kind(), requested.kind()) {
        (ValueKind::String, ValueKind::String) => {
            frame_font.as_lisp_string() == requested.as_lisp_string()
        }
        _ => false,
    }
}

pub(crate) fn live_frame_font_attribute_fallback(
    eval: &super::eval::Context,
    frame_id: FrameId,
    attr: LFaceAttr,
) -> Option<Value> {
    let frame = eval.frames.get(frame_id)?;
    let font_value = frame.parameter("font-parameter")?;
    if !is_font(&font_value) {
        return None;
    }

    if attr == LFaceAttr::Font {
        // A live graphical face owns the opened font, not the request that
        // selected it.  GNU's `internal-get-lisp-face-attribute` consequently
        // exposes a font object here; callers such as `startup.el` rely on
        // that type when passing the result to `font-xlfd-name`/`font-match-p`.
        // Keep the requested designator separately in the frame's typed
        // `FrameParam::Font` slot.
        return Some(font_value);
    }

    derived_face_attrs_from_font_value(&font_value)
        .into_iter()
        .find_map(|(derived_attr, derived_value)| (derived_attr == attr).then_some(derived_value))
}

/// GNU font.c style tables (weight/slant/width). Each row is the aliases of
/// one numeric style value; `font_style_symbolic` reports the FIRST name,
/// which is what `font_unparse_fcname` prints in the fontconfig-style full
/// name (e.g. "extra-bold").
const GNU_WEIGHT_TABLE: &[&[&str]] = &[
    &["thin"],
    &["ultra-light", "ultralight", "extra-light", "extralight"],
    &["light"],
    &["semi-light", "semilight", "demilight"],
    &["regular", "normal", "unspecified", "book"],
    &["medium"],
    &["semi-bold", "semibold", "demibold", "demi-bold", "demi"],
    &["bold"],
    &["extra-bold", "extrabold", "ultra-bold", "ultrabold"],
    &["black", "heavy"],
    &["ultra-heavy", "ultraheavy"],
];
const GNU_SLANT_TABLE: &[&[&str]] = &[
    &["reverse-oblique", "ro"],
    &["reverse-italic", "ri"],
    &["normal", "r", "unspecified"],
    &["italic", "i", "ot"],
    &["oblique", "o"],
];
const GNU_WIDTH_TABLE: &[&[&str]] = &[
    &["ultra-condensed", "ultracondensed"],
    &["extra-condensed", "extracondensed"],
    &["condensed", "compressed", "narrow"],
    &["semi-condensed", "semicondensed", "demicondensed"],
    &["normal", "medium", "regular", "unspecified"],
    &["semi-expanded", "semiexpanded", "demiexpanded"],
    &["expanded"],
    &["extra-expanded", "extraexpanded"],
    &["ultra-expanded", "ultraexpanded", "wide"],
];

/// Map a style symbol name to GNU's canonical (first) table name.
fn gnu_style_first_name(
    table: &'static [&'static [&'static str]],
    name: &str,
) -> Option<&'static str> {
    table
        .iter()
        .find(|row| row.contains(&name))
        .map(|row| row[0])
}

/// `font-info` for a font ENTITY, following GNU font.c `Ffont_info`: open
/// the entity via `font_open_entity` (a scalable entity's size 0 probes
/// upward from 1px until the font is "manageable") and report the OPENED
/// font's metrics — the tiny pixelsize=1 numbers — not the frame's realized
/// font. Names: element 0 is the entity XLFD with the probed pixel size,
/// element 1 the fontconfig-style name `font_unparse_fcname` builds.
fn font_info_vector_for_entity(eval: &mut super::eval::Context, entity: &Value) -> Option<Value> {
    let elems = entity.as_vector_data()?.clone();
    let file_value = font_vector_get_flexible(&elems, "file").filter(|value| value.is_string())?;
    let file = file_value.as_utf8_str()?.to_owned();
    let px = font_vector_get_flexible(&elems, "size")
        .and_then(|value| match value.kind() {
            ValueKind::Fixnum(n) if n > 0 => Some(n as u32),
            _ => None,
        })
        .unwrap_or(0);
    // Variable fonts: probe the value's weight instance (OT wght axis units
    // are CSS weights).
    let wght = font_vector_get_flexible(&elems, "weight")
        .and_then(|value| value.as_symbol_name())
        .and_then(|name| name.trim_start_matches(':').parse::<FontWeight>().ok())
        .map(|weight| f32::from(weight.css_weight()));
    let probe = eval
        .display_host
        .as_mut()
        .and_then(|host| host.probe_font_px_metrics(&file, 0, px, wght).ok())
        .flatten()?;
    // Element 14: (opentype GSUB . GPOS) like GNU's
    // `Fcons (Qopentype, otf_capability (font))` (font.c Ffont_info).
    let capability = otf_capability_lisp(eval, &file);

    let (
        foundry,
        family,
        weight,
        slant,
        set_width,
        adstyle,
        _pixel,
        resx,
        spacing_field,
        avg_width,
        registry,
    ) = xlfd_fields_from_font_vector(&elems);
    let opened_name = format!(
        "-{}-{}-{}-{}-{}-{}-{}-*-{}-{}-{}-{}",
        foundry,
        family,
        weight,
        slant,
        set_width,
        adstyle,
        probe.pixel_size,
        resx,
        spacing_field,
        avg_width,
        registry
    );

    // font_unparse_fcname: family:pixelsize=N[:foundry=F][:weight=W]
    // [:slant=S][:width=W][:spacing=N]:scalable=true (avgwidth 0).
    let mut full_name = String::new();
    full_name.push_str(&family);
    full_name.push_str(&format!(":pixelsize={}", probe.pixel_size));
    if foundry != "*" {
        full_name.push_str(&format!(":foundry={foundry}"));
    }
    let style = |key: &str, table: &'static [&'static [&'static str]]| -> Option<&'static str> {
        font_vector_get_flexible(&elems, key)
            .and_then(|value| value.as_symbol_name())
            .and_then(|name| gnu_style_first_name(table, name.trim_start_matches(':')))
    };
    if let Some(name) = style("weight", GNU_WEIGHT_TABLE) {
        full_name.push_str(&format!(":weight={name}"));
    }
    if let Some(name) = style("slant", GNU_SLANT_TABLE).or(Some("normal")) {
        full_name.push_str(&format!(":slant={name}"));
    }
    full_name.push_str(&format!(
        ":width={}",
        style("width", GNU_WIDTH_TABLE).unwrap_or("normal")
    ));
    if let Some(spacing) =
        font_vector_get_flexible(&elems, "spacing").and_then(|value| match value.kind() {
            ValueKind::Fixnum(n) => Some(n),
            _ => None,
        })
    {
        full_name.push_str(&format!(":spacing={spacing}"));
    }
    full_name.push_str(":scalable=true");

    Some(Value::vector(vec![
        Value::string(opened_name),
        Value::string(full_name),
        Value::fixnum(probe.pixel_size as i64),
        Value::fixnum(probe.height as i64),
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(probe.max_width as i64),
        Value::fixnum(probe.ascent as i64),
        Value::fixnum(probe.descent as i64),
        Value::fixnum(probe.space_width as i64),
        Value::fixnum(probe.average_width as i64),
        file_value,
        capability,
    ]))
}

/// `(opentype GSUB . GPOS)` for a font file, or nil when unavailable.
fn otf_capability_lisp(eval: &mut super::eval::Context, file: &str) -> Value {
    eval.display_host
        .as_mut()
        .and_then(|host| host.font_otf_capability(file, 0).ok())
        .flatten()
        .as_ref()
        .map(otf_capability_to_lisp)
        .unwrap_or(Value::NIL)
}

fn otf_capability_to_lisp(caps: &super::eval::FontOtfCapability) -> Value {
    Value::cons(
        Value::symbol("opentype"),
        Value::cons(otf_side_to_lisp(&caps.gsub), otf_side_to_lisp(&caps.gpos)),
    )
}

/// Capability for any font VALUE carrying a `:file`, else nil.
fn font_value_otf_capability(eval: &mut super::eval::Context, font_like: &Value) -> Value {
    let Some(file) = font_value_fields(font_like)
        .and_then(|fields| font_vector_get_flexible(fields, "file"))
        .filter(|value| value.is_string())
        .and_then(|value| value.as_utf8_str().map(|s| s.to_owned()))
    else {
        return Value::NIL;
    };
    otf_capability_lisp(eval, &file)
}

/// Lisp form of one GSUB/GPOS side: list of `(SCRIPT (LANGSYS FEATURES...)
/// ...)`, default langsys printed as `nil`; `nil` for an empty side —
/// mirroring GNU `hbfont_otf_features`.
fn otf_side_to_lisp(side: &super::eval::OtfSideCapability) -> Value {
    let scripts: Vec<Value> = side
        .iter()
        .map(|(script, lang_syses)| {
            let langsys_values: Vec<Value> = lang_syses
                .iter()
                .map(|(tag, features)| {
                    let feature_values: Vec<Value> = features
                        .iter()
                        .map(|feature| Value::from_sym_id(intern(feature)))
                        .collect();
                    Value::cons(
                        tag.as_deref()
                            .map(|tag| Value::from_sym_id(intern(tag)))
                            .unwrap_or(Value::NIL),
                        Value::list(feature_values),
                    )
                })
                .collect();
            Value::cons(
                Value::from_sym_id(intern(script)),
                Value::list(langsys_values),
            )
        })
        .collect();
    Value::list(scripts)
}

fn font_info_vector_for_runtime_font(
    font_like: &Value,
    frame: &crate::window::Frame,
    capability: Value,
) -> Value {
    let opened_name = font_name_value(font_like).unwrap_or_else(|| Value::string(""));
    let full_name = opened_name;
    let file = match font_like.kind() {
        ValueKind::Veclike(VecLikeType::Vector | VecLikeType::Font) if is_font(font_like) => {
            font_value_fields(font_like)
                .and_then(|elems| font_vector_get_flexible(elems, "file"))
                .filter(|value| value.is_string())
                .unwrap_or(Value::NIL)
        }
        _ => Value::NIL,
    };
    let size = frame.font_pixel_size.max(1.0).round() as i64;
    let height = frame.char_height.max(1.0).round() as i64;
    let average_width = frame.char_width.max(1.0).round() as i64;
    let space_width = average_width;
    let max_width = average_width;
    let ascent = ((height as f32) * 0.75).round() as i64;
    let descent = (height - ascent).max(0);
    let default_ascent = ascent;

    Value::vector(vec![
        opened_name,
        full_name,
        Value::fixnum(size),
        Value::fixnum(height),
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(default_ascent),
        Value::fixnum(max_width),
        Value::fixnum(ascent),
        Value::fixnum(descent),
        Value::fixnum(space_width),
        Value::fixnum(average_width),
        file,
        capability,
    ])
}

pub(crate) fn resolve_font_match(
    eval: &mut super::eval::Context,
    frame_id: FrameId,
    character: crate::emacs_core::emacs_char::EmacsChar,
    face: &RuntimeFace,
    fontset_base_face: &RuntimeFace,
) -> Option<super::eval::ResolvedFontMatch> {
    eval.display_host
        .as_mut()
        .and_then(|host| {
            host.resolve_font_for_char(super::display_host::FontResolveRequest {
                frame_id,
                character,
                faces: super::display_host::RealizedFaceFontContext {
                    ascii_face: face.clone(),
                    fontset_base_face: fontset_base_face.clone(),
                },
            })
            .ok()
        })
        .flatten()
}

/// `(font-at POSITION &optional WINDOW STRING)` -- resolve the effective font
/// object for the target buffer or string position.
pub(crate) fn font_at(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("font-at", &args, 1)?;
    expect_max_args("font-at", &args, 3)?;

    let (frame_id, window_id) = resolve_live_window_for_font_at(eval, args.get(1))?;
    let (window_buffer_id, has_window_system) = {
        let frame = eval
            .frames
            .get(frame_id)
            .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
        let window = frame
            .find_window(window_id)
            .ok_or_else(|| signal("error", vec![Value::string("Window not found")]))?;
        (
            window.buffer_id(),
            frame.effective_window_system().is_some(),
        )
    };

    if let Some(string_value) = args.get(2)
        && !string_value.is_nil()
    {
        if !string_value.is_string() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), *string_value],
            ));
        };
        let pos = match args[0].kind() {
            ValueKind::Fixnum(n) => n,
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("fixnump"), args[0]],
                ));
            }
        };
        let string = string_value
            .as_lisp_string()
            .expect("string object must carry LispString payload");
        let char_len = string.schars() as i64;
        if !(0 <= pos && pos < char_len) {
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![*string_value, Value::fixnum(pos)],
            ));
        }
        if !has_window_system {
            return Ok(Value::NIL);
        }
        let face_table = runtime_face_table_from_frame_lisp_faces(eval, frame_id, true);
        let char_pos = usize::try_from(pos).expect("validated non-negative string position");
        let bytepos = if string.is_multibyte() {
            crate::emacs_core::emacs_char::char_to_byte_pos(string.as_bytes(), char_pos)
        } else {
            char_pos
        };
        let face = resolved_face_at_string_char_pos(
            eval,
            &face_table,
            *string_value,
            CharPos0::new(char_pos),
        );
        let code = if string.is_multibyte() {
            crate::emacs_core::emacs_char::string_char(&string.as_bytes()[bytepos..]).0
        } else {
            string.as_bytes()[bytepos] as u32
        };
        let Some(character) = crate::emacs_core::emacs_char::EmacsChar::from_code(code) else {
            return Ok(Value::NIL);
        };
        let fontset_base_face = face_table.resolve("default");
        if let Some(matched) =
            resolve_font_match(eval, frame_id, character, &face, &fontset_base_face)
        {
            return Ok(opened_font_from_resolved_match(&face, &matched));
        }
        return Ok(Value::NIL);
    }

    let current_buffer_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    if window_buffer_id != Some(current_buffer_id) {
        return Err(signal(
            "error",
            vec![Value::string(
                "Specified window is not displaying the current buffer",
            )],
        ));
    }

    let pos =
        crate::emacs_core::builtins::expect_integer_or_marker_in_buffers(&eval.buffers, &args[0])?;
    let buffer = eval
        .buffers
        .get(current_buffer_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let beg = buffer.point_min_lisp_char_pos().as_i64();
    let end = buffer.point_max_lisp_char_pos().as_i64();
    if !(beg <= pos && pos < end) {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![args[0], Value::fixnum(beg), Value::fixnum(end)],
        ));
    }

    if !has_window_system {
        return Ok(Value::NIL);
    }

    let face_table = runtime_face_table_from_frame_lisp_faces(eval, frame_id, true);
    let bytepos = buffer.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(pos));
    let face = resolved_face_at_buffer_byte(eval, &face_table, buffer, bytepos);
    let character = buffer
        .char_code_at_emacs_byte_pos(bytepos)
        .and_then(crate::emacs_core::emacs_char::EmacsChar::from_code)
        .ok_or_else(|| {
            signal(
                LispCondition::ArgsOutOfRange,
                vec![args[0], Value::fixnum(beg), Value::fixnum(end)],
            )
        })?;
    let fontset_base_face = face_table.resolve("default");
    if let Some(matched) = resolve_font_match(eval, frame_id, character, &face, &fontset_base_face)
    {
        return Ok(opened_font_from_resolved_match(&face, &matched));
    }
    Ok(Value::NIL)
}

/// `(internal-char-font POSITION &optional CH)` -- the `(FONT-OBJECT . GLYPH-CODE)`
/// that `describe-char` uses for its "display:" line and character-code-property
/// section. A non-nil POSITION resolves the face in a window displaying the
/// current buffer and uses CH when supplied, otherwise the buffer character at
/// POSITION. A nil POSITION resolves CH in the selected frame's default face.
/// Returns nil when the current buffer is not displayed, on a non-window frame,
/// or when no font can be found.
pub(crate) fn internal_char_font(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("internal-char-font", &args, 1)?;
    expect_max_args("internal-char-font", &args, 2)?;
    let position = args[0];
    let ch_arg = args.get(1).copied().unwrap_or(Value::NIL);

    let (frame_id, character, face, fontset_base_face) = if position.is_nil() {
        let code = crate::emacs_core::builtins::expect_character_code(&ch_arg)?;
        let Some(character) = u32::try_from(code)
            .ok()
            .and_then(crate::emacs_core::emacs_char::EmacsChar::from_code)
        else {
            return Ok(Value::NIL);
        };
        let frame_id = super::window_cmds::ensure_selected_frame_id(eval);
        let has_window_system = eval
            .frames
            .get(frame_id)
            .is_some_and(|frame| frame.effective_window_system().is_some());
        if !has_window_system {
            return Ok(Value::NIL);
        }
        let face_table = runtime_face_table_from_frame_lisp_faces(eval, frame_id, true);
        let default_face = face_table.resolve("default");
        (frame_id, character, default_face.clone(), default_face)
    } else {
        let current_buffer_id = eval
            .buffers
            .current_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let pos = crate::emacs_core::builtins::expect_integer_or_marker_in_buffers(
            &eval.buffers,
            &args[0],
        )?;
        let (beg, end, bytepos) = {
            let buffer = eval
                .buffers
                .get(current_buffer_id)
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
            let beg = buffer.point_min_lisp_char_pos().as_i64();
            let end = buffer.point_max_lisp_char_pos().as_i64();
            if !(beg <= pos && pos < end) {
                return Err(signal(
                    LispCondition::ArgsOutOfRange,
                    vec![args[0], Value::fixnum(beg), Value::fixnum(end)],
                ));
            }
            (
                beg,
                end,
                buffer.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(pos)),
            )
        };
        // GNU performs CHECK_FIXNAT before looking for the display window, so
        // an invalid CH still signals when the current buffer is hidden.
        let explicit_character_code = if ch_arg.is_nil() {
            None
        } else {
            Some(crate::emacs_core::builtins::expect_wholenump(&ch_arg)?)
        };

        // GNU asks `get-buffer-window` before choosing the frame and face.
        // In particular, a selected frame does not make a hidden current
        // buffer displayable by implication.
        let window = super::window_cmds::builtin_get_buffer_window(
            eval,
            vec![Value::make_buffer(current_buffer_id)],
        )?;
        let Some(window_id) = window.as_window_id().map(crate::window::WindowId) else {
            return Ok(Value::NIL);
        };
        let Some(frame_id) = eval.frames.find_window_frame_id(window_id) else {
            return Ok(Value::NIL);
        };
        let has_window_system = eval
            .frames
            .get(frame_id)
            .is_some_and(|frame| frame.effective_window_system().is_some());
        if !has_window_system {
            return Ok(Value::NIL);
        }

        let face_table = runtime_face_table_from_frame_lisp_faces(eval, frame_id, true);
        let buffer = eval
            .buffers
            .get(current_buffer_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let character = if let Some(code) = explicit_character_code {
            let Some(character) = u32::try_from(code)
                .ok()
                .and_then(crate::emacs_core::emacs_char::EmacsChar::from_code)
            else {
                return Ok(Value::NIL);
            };
            character
        } else {
            buffer
                .char_code_at_emacs_byte_pos(bytepos)
                .and_then(crate::emacs_core::emacs_char::EmacsChar::from_code)
                .ok_or_else(|| {
                    signal(
                        LispCondition::ArgsOutOfRange,
                        vec![args[0], Value::fixnum(beg), Value::fixnum(end)],
                    )
                })?
        };
        let face = resolved_face_at_buffer_byte(eval, &face_table, buffer, bytepos);
        let fontset_base_face = face_table.resolve("default");
        (frame_id, character, face, fontset_base_face)
    };

    let Some(matched) = resolve_font_match(eval, frame_id, character, &face, &fontset_base_face)
    else {
        return Ok(Value::NIL);
    };
    let Some(glyph_code) = matched.glyph_code else {
        return Ok(Value::NIL);
    };
    let font_object = opened_font_from_resolved_match(&face, &matched);
    Ok(Value::cons(
        font_object,
        Value::fixnum(i64::from(glyph_code)),
    ))
}

pub(crate) fn font_info(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("font-info", &args, 1)?;
    expect_max_args("font-info", &args, 2)?;

    if !(args[0].is_string()
        || is_font(&args[0])
        || is_font_entity(&args[0])
        || is_font_object(&args[0]))
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        ));
    }

    let frame_id = match args.get(1) {
        None => super::window_cmds::ensure_selected_frame_id(eval),
        Some(v) if v.is_nil() => super::window_cmds::ensure_selected_frame_id(eval),
        Some(frame) if live_frame_designator_in_state(&eval.frames, frame) => {
            frame_id_from_designator(frame)
                .expect("live frame designator should decode to frame id")
        }
        Some(other) => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("frame-live-p"), *other],
            ));
        }
    };
    let has_window_system = eval
        .frames
        .get(frame_id)
        .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?
        .window_system
        .is_some();
    if !has_window_system {
        return Ok(Value::NIL);
    }

    if let Some(opened) = OpenedFont::decode(args[0]) {
        return Ok(opened.info_vector());
    }
    if is_font_entity(&args[0]) {
        // GNU opens the font itself (font_open_entity for entities; a
        // font-at object is already opened at its pixel size) and reports
        // the OPENED font's metrics; only fall back to the frame font when
        // the value can't be probed (no file, unreadable, ...).
        if let Some(info) = font_info_vector_for_entity(eval, &args[0]) {
            return Ok(info);
        }
    }
    // GNU attaches (opentype . caps) to font-info for OPENED fonts too
    // (font-at objects); compute it from the font's file before borrowing
    // the frame.
    let capability = font_value_otf_capability(eval, &args[0]);
    let frame = eval
        .frames
        .get(frame_id)
        .ok_or_else(|| signal("error", vec![Value::string("No selected frame")]))?;
    if args[0].is_string() || is_font(&args[0]) {
        Ok(font_info_vector_for_runtime_font(
            &args[0], frame, capability,
        ))
    } else {
        Ok(Value::NIL)
    }
}

pub(crate) fn query_font(_eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("query-font", &args, 1)?;
    let Some(opened) = OpenedFont::decode(args[0]) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("font-object"), args[0]],
        ));
    };
    Ok(opened.query_vector())
}

fn expect_font_character(value: Value) -> Result<char, Flow> {
    match value.kind() {
        ValueKind::Fixnum(code) if code >= 0 => char::from_u32(code as u32).ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("characterp"), value],
            )
        }),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), value],
        )),
    }
}

pub(crate) fn font_get_glyphs(args: Vec<Value>) -> EvalResult {
    expect_args_range("font-get-glyphs", &args, 3, 4)?;
    if !is_font_object(&args[0]) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("font-object"), args[0]],
        ));
    }
    let _ = args[1].as_int().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("fixnump"), args[1]],
        )
    })?;
    let _ = args[2].as_int().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("fixnump"), args[2]],
        )
    })?;
    Ok(Value::NIL)
}

pub(crate) fn font_has_char_p(args: Vec<Value>) -> EvalResult {
    expect_args_range("font-has-char-p", &args, 2, 3)?;
    if !is_font(&args[0]) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("font"), args[0]],
        ));
    }
    let _ = expect_font_character(args[1])?;
    Ok(Value::NIL)
}

pub(crate) fn font_match_p(args: Vec<Value>) -> EvalResult {
    expect_args("font-match-p", &args, 2)?;
    for value in &args {
        if !is_font_spec(value) {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("font-spec"), *value],
            ));
        }
    }
    Ok(Value::NIL)
}

/// GNU `Ffont_shape_gstring`: validate through `composition_gstring_p`, honor
/// an already-cached ID, then dispatch to the opened font driver.  Neomacs's
/// shaping driver is not yet exposed at this Lisp seam, so an uncached valid
/// gstring currently reports no shaped result.
pub(crate) fn font_shape_gstring(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("font-shape-gstring", &args, 2)?;
    if !super::composite::composition_gstring_p(eval, args[0]) {
        return Err(signal(
            "error",
            vec![Value::string("Invalid glyph-string: "), args[0]],
        ));
    }
    let slots = args[0]
        .as_vector_data()
        .expect("validated glyph-string must be a vector");
    if !slots[1].is_nil() {
        return Ok(args[0]);
    }
    let header = slots[0]
        .as_vector_data()
        .expect("validated glyph-string header must be a vector");
    if !is_font_object(&header[0]) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("font-object"), header[0]],
        ));
    }
    Ok(Value::NIL)
}

pub(crate) fn font_variation_glyphs(args: Vec<Value>) -> EvalResult {
    expect_args("font-variation-glyphs", &args, 2)?;
    if !is_font_object(&args[0]) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("font-object"), args[0]],
        ));
    }
    let _ = expect_font_character(args[1])?;
    Ok(Value::NIL)
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;

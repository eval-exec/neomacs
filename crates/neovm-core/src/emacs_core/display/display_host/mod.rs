//! Host-side display capabilities and validated boundary values.
//!
//! This is Neomacs integration machinery, not a port of GNU Emacs
//! src/eval.c, so it stays outside the GNU mirror module.

use std::fmt::{Display, Formatter};
use std::num::{NonZeroU16, NonZeroU32};

use super::eval::{
    FontOtfCapability, FontPxProbeResult, FontSpecResolveRequest, GuiFrameHostRequest,
    GuiFrameHostSize, PopupMenuRequest, ResolvedFontMatch, ResolvedFontSpecMatch,
    ResolvedFrameFont, ResolvedSurface, ResolvedVideo, ResolvedWebKit, ShaderSurfaceCreateRequest,
    ShaderSurfaceLanguage, ShaderSurfaceUniformInit, SurfaceResolveRequest, VideoResolveRequest,
    WebKitResolveRequest,
};
use crate::buffer::BufferId;
use crate::face::{Face as RuntimeFace, FaceHeight};
use crate::heap_types::LispString;
use crate::window::FrameFullscreen;
use neomacs_display_protocol::WebViewId;

/// Evaluator-owned correlation ID for one asynchronous xwidget script call.
///
/// The display host cannot allocate this ID: only the evaluator knows which
/// GC-rooted Lisp callback must receive the eventual result.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct XwidgetScriptRequestId(u64);

impl XwidgetScriptRequestId {
    pub(crate) const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A non-empty font-family name crossing from the platform display host into
/// the evaluator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvailableFontFamilyName(LispString);

impl AvailableFontFamilyName {
    pub fn new(name: LispString) -> Option<Self> {
        name.as_utf8_str()
            .is_some_and(|name| !name.trim().is_empty())
            .then_some(Self(name))
    }

    pub fn from_utf8(name: &str) -> Option<Self> {
        Self::new(LispString::from_utf8(name))
    }

    pub fn into_lisp_string(self) -> LispString {
        self.0
    }
}

/// The display object that owns a neo-term instance.
///
/// Window terminals carry their buffer identity in the variant itself, so a
/// detached frame-wide `Window` state cannot be constructed accidentally.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalDisplayTarget {
    Window { buffer: BufferId },
    Inline,
    Floating,
}

/// A live terminal identifier. Zero is reserved as the Lisp failure sentinel,
/// so it cannot cross the typed display-host boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TerminalId(NonZeroU32);

impl TerminalId {
    pub fn new(id: u32) -> Option<Self> {
        NonZeroU32::new(id).map(Self)
    }

    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl Display for TerminalId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Non-empty terminal grid dimensions validated at the Lisp boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalGridSize {
    pub cols: NonZeroU16,
    pub rows: NonZeroU16,
}

impl TerminalGridSize {
    pub fn new(cols: u16, rows: u16) -> Option<Self> {
        Some(Self {
            cols: NonZeroU16::new(cols)?,
            rows: NonZeroU16::new(rows)?,
        })
    }
}

/// Validated terminal creation request owned by the evaluator/display-host
/// boundary. Renderer-specific command enums stay outside `neovm-core`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalCreateRequest {
    pub size: TerminalGridSize,
    pub target: TerminalDisplayTarget,
    pub shell: Option<String>,
}

/// The two face/fontset inputs GNU keeps distinct after face realization.
///
/// `ascii_face` owns the explicitly merged font attributes used for ASCII.
/// `fontset_base_face` owns the frame-local base fontset used to select a font
/// for non-ASCII characters. An inline `:family` changes the former without
/// silently replacing the latter (`xfaces.c:6277-6370`).
#[derive(Clone, Debug)]
pub struct RealizedFaceFontContext {
    pub ascii_face: RuntimeFace,
    pub fontset_base_face: RuntimeFace,
}

/// Typed request crossing from GNU-compatible face realization into the
/// platform font host. Keeping it here makes the host boundary—not the Lisp
/// evaluator—the owner of native font selection.
#[derive(Clone, Debug)]
pub struct FontResolveRequest {
    pub frame_id: crate::window::FrameId,
    /// Full GNU Emacs character domain, including raw-byte and non-Unicode
    /// codes. Backends explicitly decide which subset they can encode.
    pub character: crate::emacs_core::emacs_char::EmacsChar,
    pub faces: RealizedFaceFontContext,
}

/// A finite, positive scalar used by point-size and relative-size requests.
///
/// Lisp numbers are validated once when they cross the display-host seam, so
/// native font adapters never need to recover from NaN, infinity, zero, or a
/// negative size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositiveFontScalar(f64);

impl PositiveFontScalar {
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && value > 0.0).then_some(Self(value))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

/// The semantic size attached to a frame-font request.
///
/// GNU keeps integer font-spec sizes in pixels until font realization, while
/// floating sizes are points. Keeping those units in the type prevents a
/// frame-independent conversion from silently assuming one platform DPI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameFontSize {
    Default,
    Pixels(NonZeroU32),
    Points(PositiveFontScalar),
    Relative(PositiveFontScalar),
}

impl FrameFontSize {
    pub fn pixels(value: i64) -> Option<Self> {
        u32::try_from(value)
            .ok()
            .and_then(NonZeroU32::new)
            .map(Self::Pixels)
    }

    pub fn points(value: f64) -> Option<Self> {
        let value = PositiveFontScalar::new(value)?;
        ((1.0..=f64::from(i32::MAX)).contains(&(value.get() * 10.0))).then_some(Self::Points(value))
    }

    pub fn relative(value: f64) -> Option<Self> {
        PositiveFontScalar::new(value).map(Self::Relative)
    }

    fn from_face_height(height: FaceHeight) -> Option<Self> {
        match height {
            FaceHeight::Absolute(tenths) => Self::points(f64::from(tenths) / 10.0),
            FaceHeight::Relative(scale) => Self::relative(scale),
        }
    }
}

/// Typed request crossing from frame-local face state into native font
/// selection. The embedded face carries style; `size` is its sole sizing
/// authority and therefore cannot disagree with `Face::height`.
#[derive(Clone, Debug)]
pub struct FrameFontRequest {
    face: RuntimeFace,
    size: FrameFontSize,
}

impl FrameFontRequest {
    pub fn from_face(mut face: RuntimeFace) -> Self {
        let size = face
            .height
            .take()
            .and_then(FrameFontSize::from_face_height)
            .unwrap_or(FrameFontSize::Default);
        Self { face, size }
    }

    pub fn with_size(mut face: RuntimeFace, size: FrameFontSize) -> Self {
        face.height = None;
        Self { face, size }
    }

    pub const fn face(&self) -> &RuntimeFace {
        &self.face
    }

    pub const fn size(&self) -> FrameFontSize {
        self.size
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalFloatPlacement {
    x: f32,
    y: f32,
    opacity: f32,
}

impl TerminalFloatPlacement {
    pub fn new(x: f32, y: f32, opacity: f32) -> Option<Self> {
        (x.is_finite() && y.is_finite() && opacity.is_finite() && (0.0..=1.0).contains(&opacity))
            .then_some(Self { x, y, opacity })
    }

    pub fn x(self) -> f32 {
        self.x
    }

    pub fn y(self) -> f32 {
        self.y
    }

    pub fn opacity(self) -> f32 {
        self.opacity
    }
}

pub trait DisplayHost {
    fn realize_gui_frame(&mut self, request: GuiFrameHostRequest) -> Result<(), String>;
    fn resize_gui_frame(&mut self, request: GuiFrameHostRequest) -> Result<(), String>;
    fn set_clipboard_text(&mut self, _text: Option<&str>) -> Result<(), String> {
        Err("clipboard is unsupported by this display host".to_owned())
    }
    fn clipboard_text(&mut self) -> Result<Option<String>, String> {
        Err("clipboard is unsupported by this display host".to_owned())
    }
    fn set_primary_selection_text(&mut self, _text: Option<&str>) -> Result<(), String> {
        Err("PRIMARY selection is unsupported by this display host".to_owned())
    }
    fn primary_selection_text(&mut self) -> Result<Option<String>, String> {
        Err("PRIMARY selection is unsupported by this display host".to_owned())
    }
    fn set_gui_frame_geometry_hints(
        &mut self,
        _frame_id: crate::window::FrameId,
        _geometry_hints: crate::window::GuiFrameGeometryHints,
    ) -> Result<(), String> {
        Ok(())
    }
    fn set_gui_frame_fullscreen(
        &mut self,
        _frame_id: crate::window::FrameId,
        _fullscreen: FrameFullscreen,
    ) -> Result<(), String> {
        Ok(())
    }
    fn set_gui_frame_title(
        &mut self,
        _frame_id: crate::window::FrameId,
        _title: crate::heap_types::LispString,
    ) -> Result<(), String> {
        Ok(())
    }
    /// Apply the `undecorated` frame parameter to the platform window.
    ///
    /// GNU dispatches each frame parameter to a backend setter through
    /// `frame_parms[]` (src/frame.c); a parameter the backend cannot honour is
    /// still STORED, so `frame-parameter` reads back what Lisp set. Keep that
    /// split: the caller records the value regardless of what this returns.
    fn set_gui_frame_undecorated(
        &mut self,
        _frame_id: crate::window::FrameId,
        _undecorated: bool,
    ) -> Result<(), String> {
        Ok(())
    }
    fn opening_gui_frame_pending(&self) -> bool {
        false
    }
    fn destroy_gui_frame(&mut self, _frame_id: crate::window::FrameId) -> Result<(), String> {
        Ok(())
    }
    fn show_gui_child_frame(&mut self, _frame_id: crate::window::FrameId) -> Result<(), String> {
        Ok(())
    }
    fn remove_gui_child_frame(&mut self, _frame_id: crate::window::FrameId) -> Result<(), String> {
        Ok(())
    }
    fn show_popup_menu(&mut self, _menu: PopupMenuRequest) -> Result<(), String> {
        Ok(())
    }
    fn popup_menu_visible_rows(&self, _x: f32, _y: f32, _entry_count: usize) -> Option<usize> {
        None
    }
    fn hide_popup_menu(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn current_primary_window_size(&self) -> Option<GuiFrameHostSize> {
        None
    }
    fn list_font_families(
        &mut self,
        _frame_id: crate::window::FrameId,
    ) -> Result<Vec<AvailableFontFamilyName>, String> {
        Ok(Vec::new())
    }
    fn resolve_font_for_char(
        &mut self,
        _request: FontResolveRequest,
    ) -> Result<Option<ResolvedFontMatch>, String> {
        Ok(None)
    }
    fn resolve_frame_font(
        &mut self,
        _frame_id: crate::window::FrameId,
        _request: FrameFontRequest,
    ) -> Result<Option<ResolvedFrameFont>, String> {
        Ok(None)
    }
    fn resolve_font_for_spec(
        &mut self,
        _request: FontSpecResolveRequest,
    ) -> Result<Option<ResolvedFontSpecMatch>, String> {
        Ok(None)
    }
    fn probe_font_px_metrics(
        &mut self,
        _file: &str,
        _face_index: u32,
        _pixel_size: u32,
        _wght: Option<f32>,
    ) -> Result<Option<FontPxProbeResult>, String> {
        Ok(None)
    }
    fn font_otf_capability(
        &mut self,
        _file: &str,
        _face_index: u32,
    ) -> Result<Option<FontOtfCapability>, String> {
        Ok(None)
    }
    /// GNU-compatible synchronous image query used by explicit Lisp
    /// primitives such as `image-size`. This may wait for metadata and must
    /// never be called from redisplay.
    fn resolve_image_sync(
        &self,
        _request: super::image_catalog::ImageResolveRequest,
    ) -> Result<Option<super::image_catalog::ReadyImage>, String> {
        Ok(None)
    }
    /// Nonblocking image catalog used by redisplay. Synchronous Lisp image
    /// queries use `resolve_image_sync`; redisplay must use this catalog instead.
    fn image_catalog(&self) -> Option<&dyn super::image_catalog::ImageCatalog> {
        None
    }
    /// The same catalog as [`Self::image_catalog`], as a shared handle.
    ///
    /// Layout needs to resolve `(image …)` operands inside `(space :align-to …)`
    /// expressions (GNU does this inline with `lookup_image`), but the layout
    /// engine's pixel arithmetic must not hold a borrow of the host. Hosts with
    /// a catalog should return it here as well.
    fn image_catalog_shared(&self) -> Option<std::rc::Rc<dyn super::image_catalog::ImageCatalog>> {
        None
    }
    /// Called when renderer image-cache state changes, before retained layout
    /// matrices are invalidated. Hosts reconcile decode completion and lost
    /// residency so the rebuild observes one authoritative catalog state.
    fn reconcile_image_catalog_for_media_rebuild(
        &self,
        _event: super::image_catalog::ImageStateEvent,
    ) {
    }
    fn request_video(
        &self,
        _request: VideoResolveRequest,
    ) -> Result<Option<ResolvedVideo>, String> {
        Ok(None)
    }
    fn request_webkit(
        &self,
        _request: WebKitResolveRequest,
    ) -> Result<Option<ResolvedWebKit>, String> {
        Ok(None)
    }
    /// Nonblocking declarative shader-surface resolution used by redisplay
    /// for `(surface :shader …)` display specs. Memoized by request content;
    /// a WGSL validation failure logs and returns `Ok(None)` (redisplay can
    /// not signal), unlike `create_shader_surface` which reports errors to
    /// Lisp synchronously.
    fn request_surface(
        &self,
        _request: SurfaceResolveRequest,
    ) -> Result<Option<ResolvedSurface>, String> {
        Ok(None)
    }
    fn create_webkit_xwidget(
        &self,
        _id: WebViewId,
        _width: u32,
        _height: u32,
    ) -> Result<(), String> {
        Ok(())
    }
    fn load_webkit_xwidget_uri(
        &self,
        _id: WebViewId,
        _uri: crate::heap_types::LispString,
    ) -> Result<(), String> {
        Ok(())
    }
    fn execute_webkit_xwidget_script(
        &self,
        _id: WebViewId,
        _request: XwidgetScriptRequestId,
        _script: crate::heap_types::LispString,
    ) -> Result<(), String> {
        Ok(())
    }
    fn resize_webkit_xwidget(
        &self,
        _id: WebViewId,
        _width: u32,
        _height: u32,
    ) -> Result<(), String> {
        Ok(())
    }
    fn destroy_webkit_xwidget(&self, _id: WebViewId) -> Result<(), String> {
        Ok(())
    }
    /// Validate and create a shader surface, returning its host-allocated id.
    /// WGSL validation happens synchronously so `neomacs-surface-create` can
    /// signal compile errors as Lisp errors.
    fn create_shader_surface(&self, _request: ShaderSurfaceCreateRequest) -> Result<u32, String> {
        Err("shader surfaces are unsupported by this display host".to_owned())
    }
    fn set_shader_surface_uniform(
        &self,
        _id: u32,
        _name: &str,
        _value: [f32; 4],
    ) -> Result<(), String> {
        Ok(())
    }
    /// Install (Some source with its user uniforms in slot order, validated
    /// synchronously) or remove (None) the full-frame post shader.
    fn set_frame_shader(
        &self,
        _source: Option<(String, ShaderSurfaceLanguage, Vec<ShaderSurfaceUniformInit>)>,
    ) -> Result<(), String> {
        Err("frame shaders are unsupported by this display host".to_owned())
    }
    /// Update one named uniform on the installed full-frame post shader
    /// (cheap; no recompile). Errors when no frame shader is installed.
    fn set_frame_shader_uniform(&self, _name: &str, _value: [f32; 4]) -> Result<(), String> {
        Err("frame shaders are unsupported by this display host".to_owned())
    }
    fn destroy_shader_surface(&self, _id: u32) -> Result<(), String> {
        Ok(())
    }
    fn create_terminal(&self, _request: TerminalCreateRequest) -> Result<TerminalId, String> {
        Err("neo-term is unsupported by this display host".to_owned())
    }
    fn write_terminal(&self, _id: TerminalId, _data: Vec<u8>) -> Result<(), String> {
        Err("neo-term is unsupported by this display host".to_owned())
    }
    fn resize_terminal(&self, _id: TerminalId, _size: TerminalGridSize) -> Result<(), String> {
        Err("neo-term is unsupported by this display host".to_owned())
    }
    fn destroy_terminal(&self, _id: TerminalId) -> Result<(), String> {
        Err("neo-term is unsupported by this display host".to_owned())
    }
    fn set_floating_terminal(
        &self,
        _id: TerminalId,
        _placement: TerminalFloatPlacement,
    ) -> Result<(), String> {
        Err("neo-term is unsupported by this display host".to_owned())
    }
    fn terminal_text(&self, _id: TerminalId) -> Result<Option<String>, String> {
        Err("neo-term is unsupported by this display host".to_owned())
    }
    fn set_visual_config(
        &mut self,
        _config: neomacs_display_protocol::VisualConfig,
    ) -> Result<(), String> {
        Ok(())
    }
    /// The display rebuilt its GPU state after a device loss
    /// (`keyboard::InputEvent::DisplayReset`). Hosts drop every memo of
    /// renderer-resident media (so redisplay re-creates it), re-upload
    /// images, and re-send the frame shader. The caller then forces a full
    /// redisplay.
    fn display_reset(&self) {}
    /// Debug-only hook behind the hidden `neomacs--debug-lose-device`
    /// builtin: ask the display to simulate a GPU device loss so the whole
    /// recovery path can be exercised against a healthy device.
    fn debug_lose_device(&self) {}
}

#[cfg(test)]
#[path = "display_host_test.rs"]
mod tests;

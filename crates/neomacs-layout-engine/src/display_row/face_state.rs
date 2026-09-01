use crate::display_row::builder::{DisplayGlyphMeasurer, DisplayRowLayout};
use crate::display_row::metrics::{DisplayRowFallbackMetrics, DisplayRowMeasuredFaceMetrics};
use crate::display_row::width::DisplayRowCharWidthPolicy;
use crate::display_text_run_measurement::{
    ComplexTextRunAdvancePolicy, ComplexTextRunAdvanceResolver, DisplayTextRunAdvance,
    DisplayTextRunAdvancePolicy, DisplayTextRunMeasurement, DisplayTextRunMeasurementGeometry,
    DisplayTextRunMeasurementPlan,
};
use crate::font::metrics::{FontMetrics, FontMetricsService};
use crate::glyph_advance::GlyphAdvanceQuantization;
use crate::neovm_bridge::ResolvedFace;
use neomacs_display_protocol::DeviceScale;
use neomacs_display_protocol::TerminalColor;
use neomacs_display_protocol::face::{
    BoxBorderStyle, BoxLineWidth, BoxType, Face, FaceAttributes, UnderlinePosition, UnderlineStyle,
};
use neomacs_display_protocol::types::Color;
use neomacs_display_protocol::types::FaceId;

fn underline_style_from_code(code: u8) -> UnderlineStyle {
    UnderlineStyle::from_gnu_code(code).unwrap_or_default()
}

/// Shared render-facing face state for display rows.
#[derive(Debug, Clone)]
pub(crate) struct DisplayRowFace {
    pub(crate) face_id: FaceId,
    pub(crate) foreground: Color,
    pub(crate) background: Color,
    /// The realized terminal colours, carried beside the pixels the GUI paints:
    /// GNU's `face->foreground`/`face->background` on a tty frame
    /// (`map_tty_color`, src/xfaces.c:6620-6694).
    pub(crate) terminal_foreground: Option<TerminalColor>,
    pub(crate) terminal_background: Option<TerminalColor>,
    pub(crate) extend: bool,
    pub(crate) use_default_foreground: bool,
    pub(crate) use_default_background: bool,
    pub(crate) font_family: String,
    pub(crate) fontset_base_family: String,
    pub(crate) font_file_path: Option<String>,
    pub(crate) font_weight: u16,
    pub(crate) italic: bool,
    pub(crate) font_size: f32,
    pub(crate) underline_style: u8,
    pub(crate) underline_color: Option<Color>,
    /// The realized terminal underline colour, carried beside the pixel the
    /// GUI paints: GNU's `face->underline_color` on a tty frame
    /// (`map_tty_color`, src/xfaces.c:6748, :6777).
    pub(crate) terminal_underline_color: Option<TerminalColor>,
    pub(crate) strike_through: bool,
    pub(crate) strike_through_color: Option<Color>,
    pub(crate) overline: bool,
    pub(crate) overline_color: Option<Color>,
    pub(crate) box_type: BoxType,
    pub(crate) box_color: Option<Color>,
    pub(crate) box_line_width: BoxLineWidth,
    pub(crate) box_corner_radius: i32,
    pub(crate) box_border_style: BoxBorderStyle,
    pub(crate) box_border_speed: f32,
    pub(crate) box_color2: Option<Color>,
    pub(crate) terminal_inverse_video: bool,
    pub(crate) metrics: DisplayRowFaceMetrics,
    pub(crate) underline_position: UnderlinePosition,
    pub(crate) underline_thickness: i32,
    pub(crate) lisp_name: Option<String>,
    /// Realized `:stipple` bitmap tiled behind the face's glyphs, if any.
    pub(crate) stipple: Option<neomacs_display_protocol::StipplePattern>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DisplayRowFaceMetrics {
    char_width_px: f32,
    ascent_px: f32,
    descent_px: i32,
}

impl DisplayRowFaceMetrics {
    pub(crate) fn new(char_width_px: f32, ascent_px: f32, descent_px: i32) -> Self {
        Self {
            char_width_px,
            ascent_px,
            descent_px,
        }
    }

    pub(crate) fn from_resolved(face: &ResolvedFace) -> Self {
        let descent_px = if face.font_line_height > 0.0 && face.font_ascent > 0.0 {
            (face.font_line_height - face.font_ascent).max(0.0).ceil() as i32
        } else {
            0
        };
        Self::new(face.measured_char_width_px(), face.font_ascent, descent_px)
    }

    pub(crate) fn from_font_metrics(metrics: FontMetrics) -> Self {
        Self::new(
            metrics.char_width,
            metrics.ascent,
            metrics.descent.max(0.0).ceil() as i32,
        )
    }

    pub(crate) fn ascent_px(self) -> f32 {
        self.ascent_px
    }

    pub(crate) fn descent_px(self) -> i32 {
        self.descent_px
    }

    pub(crate) fn set_char_width_px(&mut self, char_width_px: f32) {
        self.char_width_px = char_width_px;
    }

    pub(crate) fn set_ascent_px(&mut self, ascent_px: f32) {
        self.ascent_px = ascent_px;
    }

    pub(crate) fn set_descent_px(&mut self, descent_px: i32) {
        self.descent_px = descent_px;
    }

    pub(crate) fn line_height_px(self) -> f32 {
        (self.ascent_px + self.descent_px as f32).max(1.0)
    }

    pub(crate) fn has_char_width(self, fallback_char_width: f32) -> bool {
        DisplayRowCharWidthPolicy::new(fallback_char_width).has_width(self.char_width_px)
    }

    pub(crate) fn char_width_px(self, fallback_char_width: f32) -> f32 {
        DisplayRowCharWidthPolicy::new(fallback_char_width).width(self.char_width_px)
    }

    pub(crate) fn char_width_or_measured_px(
        self,
        fallback_char_width: f32,
        measured_width: Option<f32>,
    ) -> f32 {
        DisplayRowCharWidthPolicy::new(fallback_char_width)
            .width_or_measured(self.char_width_px, measured_width)
    }

    pub(crate) fn normalize_char_width(&mut self, fallback_char_width: f32) {
        self.set_char_width_px(self.char_width_px(fallback_char_width));
    }

    pub(crate) fn include_in_layout(self, layout: &mut DisplayRowLayout) {
        let glyph_ascent = self.ascent_px().max(0.0);
        let glyph_height = self.line_height_px();
        let glyph_descent = (glyph_height - glyph_ascent).max(0.0);
        let row_descent = (layout.height_px - layout.ascent_px).max(0.0);
        layout.ascent_px = layout
            .ascent_px
            .max(glyph_ascent)
            .min(glyph_height.max(layout.height_px));
        layout.height_px = (layout.ascent_px + row_descent.max(glyph_descent)).max(glyph_height);
    }
}

impl DisplayRowFace {
    fn font_selection(&self) -> crate::font::metrics::RealizedFaceFontSelection<'_> {
        crate::font::metrics::RealizedFaceFontSelection::new(
            crate::font::metrics::PrimaryFontFamily::new(&self.font_family),
            crate::font::metrics::FontsetBaseFamily::new(&self.fontset_base_family),
            self.font_weight,
            self.italic,
            self.font_size.max(1.0),
        )
    }

    pub(crate) fn from_resolved(face_id: FaceId, face: &ResolvedFace) -> Self {
        let box_type = BoxType::from_gnu_code(face.box_type).unwrap_or_default();
        let font_family = if face.font_family.is_empty() {
            "monospace".to_string()
        } else {
            face.font_family.clone()
        };
        let fontset_base_family = if face.fontset_base_family.is_empty() {
            font_family.clone()
        } else {
            face.fontset_base_family.clone()
        };
        Self {
            face_id,
            foreground: Color::from_pixel(face.fg),
            background: Color::from_pixel(face.bg),
            terminal_foreground: face.terminal_fg,
            terminal_background: face.terminal_bg,
            terminal_underline_color: face.terminal_underline_color,
            extend: face.extend,
            use_default_foreground: face.use_default_foreground,
            use_default_background: face.use_default_background,
            font_family,
            fontset_base_family,
            font_file_path: None,
            font_weight: face.font_weight,
            italic: face.italic,
            font_size: face.font_size,
            underline_style: face.underline_style,
            underline_color: (face.underline_style > 0)
                .then(|| Color::from_pixel(face.underline_color)),
            strike_through: face.strike_through,
            strike_through_color: face
                .strike_through
                .then(|| Color::from_pixel(face.strike_through_color)),
            overline: face.overline,
            overline_color: face
                .overline
                .then(|| Color::from_pixel(face.overline_color)),
            box_type,
            // `ResolvedFace` has already resolved GNU's unspecified box
            // color to the face foreground.  Pixel zero is therefore an
            // explicit black color, not an absence sentinel.
            box_color: (box_type != BoxType::None).then(|| Color::from_pixel(face.box_color)),
            box_line_width: face.box_line_width,
            box_corner_radius: 0,
            box_border_style: BoxBorderStyle::Solid,
            box_border_speed: 1.0,
            box_color2: None,
            terminal_inverse_video: face.terminal_inverse_video,
            lisp_name: face.lisp_name.clone(),
            metrics: DisplayRowFaceMetrics::from_resolved(face),
            underline_position: match face.underline_position {
                neovm_core::face::UnderlinePosition::FontMetric => UnderlinePosition::FontMetric {
                    offset_from_baseline: 1,
                },
                neovm_core::face::UnderlinePosition::DescentLine { pixels_above } => {
                    UnderlinePosition::DescentLine { pixels_above }
                }
            },
            underline_thickness: 1,
            stipple: face.stipple.clone(),
        }
    }

    pub(crate) fn has_char_width(&self, fallback_char_width: f32) -> bool {
        self.metrics.has_char_width(fallback_char_width)
    }

    pub(crate) fn char_width_px(&self, fallback_char_width: f32) -> f32 {
        self.metrics.char_width_px(fallback_char_width)
    }

    pub(crate) fn char_width_or_measured_px(
        &self,
        fallback_char_width: f32,
        measured_width: Option<f32>,
    ) -> f32 {
        self.metrics
            .char_width_or_measured_px(fallback_char_width, measured_width)
    }

    pub(crate) fn normalize_char_width(&mut self, fallback_char_width: f32) {
        self.metrics.normalize_char_width(fallback_char_width);
    }

    pub(crate) fn render_face(&self) -> Face {
        let underline_style = underline_style_from_code(self.underline_style);
        let mut attrs = FaceAttributes::empty();
        if self.font_weight >= 700 {
            attrs |= FaceAttributes::BOLD;
        }
        if self.italic {
            attrs |= FaceAttributes::ITALIC;
        }
        if underline_style != UnderlineStyle::None {
            attrs |= FaceAttributes::UNDERLINE;
        }
        if self.strike_through {
            attrs |= FaceAttributes::STRIKE_THROUGH;
        }
        if self.overline {
            attrs |= FaceAttributes::OVERLINE;
        }
        if !matches!(self.box_type, BoxType::None) {
            attrs |= FaceAttributes::BOX;
        }
        if self.terminal_inverse_video {
            attrs |= FaceAttributes::INVERSE;
        }
        Face {
            id: self.face_id,
            foreground: self.foreground,
            background: self.background,
            terminal_foreground: self.terminal_foreground,
            terminal_background: self.terminal_background,
            use_default_foreground: self.use_default_foreground,
            use_default_background: self.use_default_background,
            underline_color: self.underline_color,
            terminal_underline_color: self.terminal_underline_color,
            overline_color: self.overline_color,
            strike_through_color: self.strike_through_color,
            box_color: self.box_color,
            font_family: self.font_family.clone(),
            fontset_base_family: Some(self.fontset_base_family.clone()),
            font_size: self.font_size,
            font_weight: self.font_weight,
            attributes: attrs,
            underline_style,
            box_type: self.box_type,
            box_line_width: self.box_line_width,
            box_corner_radius: self.box_corner_radius,
            box_border_style: self.box_border_style,
            box_border_speed: self.box_border_speed,
            box_color2: self.box_color2,
            font_file_path: self.font_file_path.clone(),
            font_ascent: self.metrics.ascent_px() as i32,
            font_descent: self.metrics.descent_px(),
            underline_position: match self.underline_position {
                UnderlinePosition::FontMetric {
                    offset_from_baseline,
                } => offset_from_baseline.max(1),
                UnderlinePosition::DescentLine { .. } => 1,
            },
            underline_thickness: self.underline_thickness.max(1),
            background_gradient: None,
            default_resolved_font_id: None,
            lisp_name: self.lisp_name.clone().or_else(|| {
                neomacs_display_protocol::face::BasicFaceId::from_gnu_code(self.face_id.get())
                    .map(|basic| basic.name().to_string())
            }),
            stipple: self.stipple.clone().map(Box::new),
            underline_placement: self.underline_position,
        }
    }
}

pub(crate) fn resolved_display_row_face(
    face_id: FaceId,
    face: &ResolvedFace,
    metrics: Option<FontMetrics>,
) -> DisplayRowFace {
    let mut render_face = DisplayRowFace::from_resolved(face_id, face);
    if let Some(metrics) = metrics {
        render_face.metrics = DisplayRowFaceMetrics::from_font_metrics(metrics);
    }
    render_face
}

/// Content-addressed dynamic face id for a resolved face.
///
/// The realization identity is computed by the SAME pipeline that later
/// builds the published face (`resolved_display_row_face(..).render_face()`
/// canonicalized by `face_realization_identity`), so the id key can never
/// drift from the published content — publish() debug-asserts exactly that.
/// Metrics are enrichment: the identity projection strips every
/// metrics-derived field, so keying with `None` here matches the
/// metrics-filled face published after font realization.
pub(crate) fn stable_face_id_for_resolved(
    face_ids: &mut crate::frame_face_arena::FrameFaceAttempt,
    face: &ResolvedFace,
) -> FaceId {
    face_ids.face_id_for_resolved(face, || {
        // The placeholder id must not collide with a basic face id:
        // render_face's lisp_name fallback names the face after its basic
        // id, and a basic name injected here would differ from the published
        // face (which carries the real dynamic id and therefore no
        // basic-name fallback).
        crate::frame_face_arena::face_realization_identity(
            &resolved_display_row_face(FaceId::new(u32::MAX), face, None).render_face(),
        )
    })
}

pub(crate) struct DisplayRowFaceRealizer<'a> {
    font_metrics: &'a mut Option<FontMetricsService>,
    device_scale: DeviceScale,
}

impl<'a> DisplayRowFaceRealizer<'a> {
    pub(crate) fn new(font_metrics: &'a mut Option<FontMetricsService>) -> Self {
        let device_scale = font_metrics
            .as_ref()
            .map(FontMetricsService::device_scale)
            .unwrap_or(DeviceScale::ONE);
        Self {
            font_metrics,
            device_scale,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_with_device_scale(
        font_metrics: &'a mut Option<FontMetricsService>,
        device_scale: DeviceScale,
    ) -> Self {
        Self {
            font_metrics,
            device_scale,
        }
    }

    pub(crate) fn has_font_metrics(&self) -> bool {
        self.font_metrics.is_some()
    }

    pub(crate) fn realize_face(
        &mut self,
        face_id: FaceId,
        face: &ResolvedFace,
        char_w: f32,
        ascent: f32,
        row_height: f32,
    ) -> DisplayRowFace {
        let mut face = DisplayRowFace::from_resolved(face_id, face);
        self.ensure_face_metrics(&mut face, char_w, ascent, row_height);
        face
    }

    pub(crate) fn row_metrics_for_face(
        &mut self,
        face: &ResolvedFace,
        fallback: DisplayRowFallbackMetrics,
    ) -> DisplayRowFallbackMetrics {
        // GNU TTY frames use 1x1 character cells for chrome rows; GUI font
        // metrics must not make mode/header/tab-line rows taller there.
        if fallback.char_width() <= 1.0 && fallback.row_height() <= 1.0 {
            return fallback;
        }
        // GNU's early redisplay path falls back to the frame font until the
        // chrome face has a realized font.  A zero/invalid size is that same
        // unresolved state, not a request for a one-pixel font.
        if !face.font_size.is_finite() || face.font_size <= 0.0 {
            return fallback;
        }
        let face = self.realize_face(
            FaceId::new(0),
            face,
            fallback.char_width(),
            fallback.ascent(),
            fallback.row_height(),
        );
        let line_height = face.metrics.line_height_px().ceil();
        let box_edge = if face.box_type != BoxType::None {
            face.box_line_width
                .logical_geometry(self.device_scale)
                .row_expansion_per_edge()
                .get()
        } else {
            0.0
        };
        let row_height = (line_height + 2.0 * box_edge).max(1.0);
        DisplayRowFallbackMetrics::from_default_face_extents(
            face.metrics.char_width_px(fallback.char_width()),
            row_height,
            (face.metrics.ascent_px() + box_edge).min(row_height),
        )
    }

    #[cfg(test)]
    pub(crate) fn row_height_for_face(
        &mut self,
        face: &ResolvedFace,
        char_w: f32,
        fallback_ascent: f32,
        fallback_row_height: f32,
    ) -> f32 {
        self.row_metrics_for_face(
            face,
            DisplayRowFallbackMetrics::from_default_face_extents(
                char_w,
                fallback_row_height,
                fallback_ascent,
            ),
        )
        .row_height()
    }

    pub(crate) fn char_width(&mut self, face: &DisplayRowFace, fallback_char_width: f32) -> f32 {
        let metrics = self.measured_font_metrics_for_face(face);
        face.char_width_or_measured_px(
            fallback_char_width,
            metrics.map(|metrics| metrics.char_width),
        )
    }

    fn measured_font_metrics_for_face(&mut self, face: &DisplayRowFace) -> Option<FontMetrics> {
        self.font_metrics.as_mut().map(|svc| {
            svc.font_metrics(
                &face.font_family,
                face.font_weight,
                face.italic,
                face.font_size,
            )
        })
    }

    pub(crate) fn font_metrics_service_mut(&mut self) -> Option<&mut FontMetricsService> {
        self.font_metrics.as_mut()
    }

    pub(crate) fn font_metrics_mut(&mut self) -> &mut Option<FontMetricsService> {
        self.font_metrics
    }

    fn ensure_face_metrics(
        &mut self,
        face: &mut DisplayRowFace,
        fallback_char_width: f32,
        fallback_ascent: f32,
        row_height: f32,
    ) {
        let needs_metrics = !face.has_char_width(fallback_char_width)
            || face.metrics.ascent_px() <= 0.0
            || face.metrics.descent_px() <= 0;

        if needs_metrics && let Some(metrics) = self.measured_font_metrics_for_face(face) {
            if !face.has_char_width(fallback_char_width) {
                face.metrics.set_char_width_px(
                    DisplayRowCharWidthPolicy::new(fallback_char_width).width(metrics.char_width),
                );
            }
            if face.metrics.ascent_px() <= 0.0 && metrics.ascent > 0.0 {
                face.metrics.set_ascent_px(metrics.ascent);
            }
            if face.metrics.descent_px() <= 0 && metrics.descent > 0.0 {
                face.metrics
                    .set_descent_px(metrics.descent.max(0.0).ceil() as i32);
            }
        }

        face.normalize_char_width(fallback_char_width);
        if face.metrics.ascent_px() <= 0.0 {
            face.metrics.set_ascent_px(fallback_ascent.max(1.0));
        }
        if face.metrics.descent_px() <= 0 && row_height > face.metrics.ascent_px() {
            face.metrics
                .set_descent_px((row_height - face.metrics.ascent_px()).max(0.0).ceil() as i32);
        }
    }
}

/// Selects the source of truth for display geometry.
///
/// Paint and face-run boundaries never participate in this choice.  GUI rows
/// use the concrete font selected for each character, matching GNU redisplay;
/// logical-cell rows retain column-based lower bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DisplayRowMeasurementMode {
    ConcreteFont,
    LogicalCells,
}

impl DisplayRowMeasurementMode {
    pub(crate) fn from_frame_window_system(window_system: bool) -> Self {
        if window_system {
            Self::ConcreteFont
        } else {
            Self::LogicalCells
        }
    }

    pub(crate) fn uses_concrete_font_geometry(self) -> bool {
        matches!(self, Self::ConcreteFont)
    }

    fn minimum_glyph_advance_px(self, measured: Option<f32>, logical_advance_px: f32) -> f32 {
        match self {
            // GNU GUI redisplay takes an ordinary character's width from the
            // concrete font's per-character metric.  Logical columns are only
            // the fallback when no font measurement exists.
            Self::ConcreteFont if measured.is_some() => 1.0,
            Self::ConcreteFont | Self::LogicalCells => logical_advance_px,
        }
    }

    fn quantization(self) -> GlyphAdvanceQuantization {
        match self {
            Self::ConcreteFont => GlyphAdvanceQuantization::PreserveLogicalPixels,
            Self::LogicalCells => GlyphAdvanceQuantization::SnapToIntegerPixels,
        }
    }
}

pub(crate) struct DisplayRowGlyphMeasurer<'a> {
    faces: &'a [DisplayRowFace],
    font_metrics: Option<&'a mut FontMetricsService>,
    fallback_char_width: f32,
    quantization: GlyphAdvanceQuantization,
    mode: DisplayRowMeasurementMode,
}

struct DisplayRowTextRunAdvancePolicy<'measurer, 'fonts> {
    measurer: &'measurer mut DisplayRowGlyphMeasurer<'fonts>,
    face: DisplayRowFace,
    fallback_char_width_px: f32,
}

impl DisplayTextRunAdvancePolicy for DisplayRowTextRunAdvancePolicy<'_, '_> {
    fn ordinary_advance_px(&mut self, ch: char) -> f32 {
        let columns = crate::composition::base_width_cols(ch).max(1);
        self.measurer
            .glyph_advance_for_face(&self.face, ch, columns, self.fallback_char_width_px)
    }

    fn shape_span(&mut self, text: &str) -> Vec<crate::font::metrics::ShapedGlyph> {
        self.measurer
            .font_metrics
            .as_mut()
            .map(|font_metrics| {
                font_metrics.shape_run_for_realized_face(text, self.face.font_selection())
            })
            .unwrap_or_default()
    }
}

impl<'a> DisplayRowGlyphMeasurer<'a> {
    #[cfg(test)]
    pub(crate) fn new(
        faces: &'a [DisplayRowFace],
        font_metrics: Option<&'a mut FontMetricsService>,
        fallback_char_width: f32,
    ) -> Self {
        Self::with_quantization(
            faces,
            font_metrics,
            fallback_char_width,
            GlyphAdvanceQuantization::PreserveLogicalPixels,
        )
    }

    pub(crate) fn with_mode(
        faces: &'a [DisplayRowFace],
        font_metrics: Option<&'a mut FontMetricsService>,
        fallback_char_width: f32,
        quantization: GlyphAdvanceQuantization,
        mode: DisplayRowMeasurementMode,
    ) -> Self {
        Self {
            faces,
            font_metrics,
            fallback_char_width,
            quantization,
            mode,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_quantization(
        faces: &'a [DisplayRowFace],
        font_metrics: Option<&'a mut FontMetricsService>,
        fallback_char_width: f32,
        quantization: GlyphAdvanceQuantization,
    ) -> Self {
        Self::with_mode(
            faces,
            font_metrics,
            fallback_char_width,
            quantization,
            DisplayRowMeasurementMode::LogicalCells,
        )
    }

    fn face(&self, face_id: FaceId) -> Option<&DisplayRowFace> {
        self.faces.iter().find(|face| face.face_id == face_id)
    }

    fn glyph_advance_for_face(
        &mut self,
        face: &DisplayRowFace,
        ch: char,
        columns: u8,
        fallback_advance_px: f32,
    ) -> f32 {
        let face_char_width = face.char_width_px(self.fallback_char_width);
        let column_advance = f32::from(columns) * face_char_width;
        let measured = self
            .font_metrics
            .as_mut()
            .map(|service| service.char_width_for_realized_face(ch, face.font_selection()));
        let minimum = self.mode.minimum_glyph_advance_px(measured, column_advance);
        self.quantization
            .resolve(measured, fallback_advance_px.max(column_advance), minimum)
    }
}

impl DisplayGlyphMeasurer for DisplayRowGlyphMeasurer<'_> {
    fn glyph_advance_px(
        &mut self,
        ch: char,
        face_id: FaceId,
        columns: u8,
        fallback_advance_px: f32,
    ) -> Option<f32> {
        if columns == 0 {
            return Some(0.0);
        }

        let face = self.face(face_id)?.clone();
        Some(self.glyph_advance_for_face(&face, ch, columns, fallback_advance_px))
    }

    fn glyph_vertical_metrics_px(
        &mut self,
        ch: char,
        face_id: FaceId,
    ) -> Option<crate::display_row::builder::DisplayRowVerticalMetrics> {
        if !self.mode.uses_concrete_font_geometry() {
            return None;
        }
        let face = self.face(face_id)?.clone();
        let font = self
            .font_metrics
            .as_mut()?
            .resolved_font_for_realized_face_char(ch, face.font_selection())?;
        let height = font.ascent_px + font.descent_px;
        (height > 0.0).then(|| {
            crate::display_row::builder::DisplayRowVerticalMetrics::new(height, font.ascent_px)
        })
    }

    fn face_vertical_metrics_px(
        &mut self,
        face_id: FaceId,
    ) -> Option<crate::display_row::builder::DisplayRowVerticalMetrics> {
        if !self.mode.uses_concrete_font_geometry() {
            return None;
        }
        let face = self.face(face_id)?.clone();
        let metrics = self.font_metrics.as_mut()?.font_metrics(
            &face.font_family,
            face.font_weight,
            face.italic,
            face.font_size.max(1.0),
        );
        (metrics.line_height > 0.0).then(|| {
            crate::display_row::builder::DisplayRowVerticalMetrics::new(
                metrics.line_height,
                metrics.ascent,
            )
        })
    }

    fn face_space_width_px(&mut self, face_id: FaceId) -> Option<f32> {
        if !self.mode.uses_concrete_font_geometry() {
            return None;
        }
        let face = self.face(face_id)?.clone();
        let metrics = self.font_metrics.as_mut()?.font_metrics(
            &face.font_family,
            face.font_weight,
            face.italic,
            face.font_size.max(1.0),
        );
        (metrics.space_width.is_finite() && metrics.space_width > 0.0)
            .then_some(metrics.space_width)
    }

    fn text_run_advances_px(
        &mut self,
        text: &str,
        face_id: FaceId,
        fallback_char_width_px: f32,
    ) -> DisplayTextRunMeasurement {
        if text.is_empty() {
            return DisplayTextRunMeasurement::PerChar;
        }

        let Some(face) = self.face(face_id).cloned() else {
            return DisplayTextRunMeasurement::PerChar;
        };
        if self.font_metrics.is_none() {
            return DisplayTextRunMeasurement::PerChar;
        }

        let face_char_width = face
            .char_width_px(self.fallback_char_width)
            .max(DisplayRowCharWidthPolicy::new(fallback_char_width_px).fallback());
        let geometry = DisplayTextRunMeasurementGeometry::new(
            face_char_width,
            DisplayRowCharWidthPolicy::new(fallback_char_width_px).fallback(),
            self.quantization,
            !self.mode.uses_concrete_font_geometry(),
        );
        let mut policy = DisplayRowTextRunAdvancePolicy {
            measurer: self,
            face,
            fallback_char_width_px,
        };
        DisplayTextRunMeasurementPlan::for_mixed_text(text, &mut policy, geometry)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DisplayRowMeasurementPolicy {
    mode: DisplayRowMeasurementMode,
}

impl DisplayRowMeasurementPolicy {
    pub(crate) fn for_mode(mode: DisplayRowMeasurementMode) -> Self {
        Self { mode }
    }

    pub(crate) fn uses_concrete_font_geometry(self) -> bool {
        self.mode.uses_concrete_font_geometry()
    }

    pub(crate) fn measurement_face(
        self,
        face_id: FaceId,
        face: &ResolvedFace,
        metrics: Option<FontMetrics>,
        fallback_char_width: f32,
    ) -> DisplayRowGlyphMeasurementFace {
        DisplayRowGlyphMeasurementFace::with_mode(
            resolved_display_row_face(face_id, face, metrics),
            self.mode,
            fallback_char_width,
            self.mode.quantization(),
        )
    }

    pub(crate) fn measured_face(
        self,
        face_id: FaceId,
        face: &ResolvedFace,
        metrics: Option<FontMetrics>,
        fallback_char_width: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
        font_metrics: &mut Option<FontMetricsService>,
    ) -> DisplayRowMeasuredFace {
        let measurement_width_policy = DisplayRowCharWidthPolicy::new(fallback_char_width);
        let fallback_width_policy = DisplayRowCharWidthPolicy::new(fallback_metrics.char_width);
        let measurement_face =
            self.measurement_face(face_id, face, metrics, measurement_width_policy.fallback());
        let space_width =
            measurement_face.advance_for_char(font_metrics, ' ', fallback_width_policy.fallback());
        let (char_width, row_height, ascent) = metrics
            .map(|metrics| {
                (
                    fallback_width_policy.width(metrics.char_width),
                    metrics.line_height,
                    metrics.ascent,
                )
            })
            .unwrap_or((
                fallback_width_policy.fallback(),
                fallback_metrics.row_height,
                fallback_metrics.ascent,
            ));
        DisplayRowMeasuredFace {
            measurement_face,
            metrics: DisplayRowMeasuredFaceMetrics::new(
                char_width,
                row_height,
                ascent,
                space_width,
            ),
        }
    }

    pub(crate) fn resolved_measured_face(
        self,
        face_id: FaceId,
        face: ResolvedFace,
        metrics: Option<FontMetrics>,
        fallback_char_width: f32,
        fallback_metrics: DisplayRowFallbackMetrics,
        font_metrics: &mut Option<FontMetricsService>,
    ) -> DisplayRowResolvedMeasuredFace {
        let measured_face = self.measured_face(
            face_id,
            &face,
            metrics,
            fallback_char_width,
            fallback_metrics,
            font_metrics,
        );
        DisplayRowResolvedMeasuredFace {
            face,
            metrics,
            measured_face,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DisplayRowGlyphMeasurementFace {
    face: DisplayRowFace,
    mode: DisplayRowMeasurementMode,
    fallback_char_width: f32,
    quantization: GlyphAdvanceQuantization,
}

impl DisplayRowGlyphMeasurementFace {
    pub(crate) fn face_id(&self) -> FaceId {
        self.face.face_id
    }

    pub(crate) fn with_mode(
        face: DisplayRowFace,
        mode: DisplayRowMeasurementMode,
        fallback_char_width: f32,
        quantization: GlyphAdvanceQuantization,
    ) -> Self {
        let width_policy = DisplayRowCharWidthPolicy::new(fallback_char_width);
        Self {
            face,
            mode,
            fallback_char_width: width_policy.fallback(),
            quantization,
        }
    }

    pub(crate) fn glyph_advance_px(
        &self,
        font_metrics: &mut Option<FontMetricsService>,
        ch: char,
        columns: u8,
        fallback_advance_px: f32,
    ) -> f32 {
        let faces = [self.face.clone()];
        let font_metrics = if self.mode.uses_concrete_font_geometry() {
            font_metrics.as_mut()
        } else {
            None
        };
        let mut measurer = DisplayRowGlyphMeasurer::with_mode(
            &faces,
            font_metrics,
            self.fallback_char_width,
            self.quantization,
            self.mode,
        );
        measurer
            .glyph_advance_px(ch, self.face.face_id, columns, fallback_advance_px)
            .unwrap_or(fallback_advance_px)
    }

    pub(crate) fn advance_for_char(
        &self,
        font_metrics: &mut Option<FontMetricsService>,
        ch: char,
        fallback_advance_px: f32,
    ) -> f32 {
        let columns = crate::composition::base_width_cols(ch);
        if columns == 0 {
            return 0.0;
        }
        self.glyph_advance_px(font_metrics, ch, columns, fallback_advance_px)
    }

    fn shaped_text_run_measurement(
        &self,
        font_metrics: &mut Option<FontMetricsService>,
        text: &str,
    ) -> DisplayTextRunMeasurement {
        if !self.mode.uses_concrete_font_geometry() {
            return DisplayTextRunMeasurement::PerChar;
        }
        let faces = [self.face.clone()];
        let mut measurer = DisplayRowGlyphMeasurer::with_mode(
            &faces,
            font_metrics.as_mut(),
            self.fallback_char_width,
            self.quantization,
            self.mode,
        );
        measurer.text_run_advances_px(text, self.face.face_id, self.fallback_char_width)
    }

    fn fallback_text_run_measurement(
        &self,
        font_metrics: &mut Option<FontMetricsService>,
        text: &str,
    ) -> DisplayTextRunMeasurement {
        let advances = text
            .char_indices()
            .enumerate()
            .map(|(char_offset, (byte_offset, ch))| {
                let columns = crate::composition::base_width_cols(ch).max(1);
                let fallback_advance_px = DisplayRowCharWidthPolicy::new(self.fallback_char_width)
                    .advance_for_columns(columns);
                DisplayTextRunAdvance::new(
                    char_offset,
                    byte_offset,
                    self.advance_for_char(font_metrics, ch, fallback_advance_px),
                )
            })
            .collect();
        DisplayTextRunMeasurement::Measured(advances)
    }

    pub(crate) fn text_run_measurement(
        &self,
        font_metrics: &mut Option<FontMetricsService>,
        text: &str,
    ) -> DisplayTextRunMeasurement {
        let measurement = self.shaped_text_run_measurement(font_metrics, text);
        if measurement.measured_advances().is_some() {
            return measurement;
        }
        self.fallback_text_run_measurement(font_metrics, text)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DisplayRowMeasuredFace {
    measurement_face: DisplayRowGlyphMeasurementFace,
    metrics: DisplayRowMeasuredFaceMetrics,
}

impl DisplayRowMeasuredFace {
    pub(crate) fn face_id(&self) -> FaceId {
        self.measurement_face.face_id()
    }

    pub(crate) fn into_measurement_face(self) -> DisplayRowGlyphMeasurementFace {
        self.measurement_face
    }

    pub(crate) fn metrics(&self) -> DisplayRowMeasuredFaceMetrics {
        self.metrics
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DisplayRowResolvedMeasuredFace {
    face: ResolvedFace,
    metrics: Option<FontMetrics>,
    measured_face: DisplayRowMeasuredFace,
}

impl DisplayRowResolvedMeasuredFace {
    pub(crate) fn face_id(&self) -> FaceId {
        self.measured_face.face_id()
    }

    pub(crate) fn into_active_face_state(self) -> DisplayRowActiveFaceState {
        DisplayRowActiveFaceState::new(self.face, self.measured_face)
    }

    pub(crate) fn resolved_face(&self) -> &ResolvedFace {
        &self.face
    }

    pub(crate) fn font_metrics(&self) -> Option<FontMetrics> {
        self.metrics
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DisplayRowActiveFaceRenderState {
    pub(crate) face_id: FaceId,
    pub(crate) background: Color,
    resolved_face: ResolvedFace,
}

impl DisplayRowActiveFaceRenderState {
    pub(crate) fn resolved_face(&self) -> &ResolvedFace {
        &self.resolved_face
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DisplayRowActiveFaceMeasurementState {
    measurement_face: DisplayRowGlyphMeasurementFace,
    metrics: DisplayRowMeasuredFaceMetrics,
}

impl DisplayRowActiveFaceMeasurementState {
    pub(crate) fn metrics(&self) -> DisplayRowMeasuredFaceMetrics {
        self.metrics
    }

    pub(crate) fn advance_for_char(
        &self,
        font_metrics: &mut Option<FontMetricsService>,
        ch: char,
        fallback_advance_px: f32,
    ) -> f32 {
        self.measurement_face
            .advance_for_char(font_metrics, ch, fallback_advance_px)
    }

    pub(crate) fn text_run_measurement(
        &self,
        font_metrics: &mut Option<FontMetricsService>,
        text: &str,
    ) -> DisplayTextRunMeasurement {
        self.measurement_face
            .text_run_measurement(font_metrics, text)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DisplayRowActiveFaceState {
    render: DisplayRowActiveFaceRenderState,
    measurement: DisplayRowActiveFaceMeasurementState,
}

struct DisplayRowComplexTextRunAdvancePolicy<'a> {
    active_face_state: &'a DisplayRowActiveFaceState,
    font_metrics: &'a mut Option<FontMetricsService>,
}

impl<'a> DisplayRowComplexTextRunAdvancePolicy<'a> {
    fn new(
        active_face_state: &'a DisplayRowActiveFaceState,
        font_metrics: &'a mut Option<FontMetricsService>,
    ) -> Self {
        Self {
            active_face_state,
            font_metrics,
        }
    }
}

impl ComplexTextRunAdvancePolicy for DisplayRowComplexTextRunAdvancePolicy<'_> {
    fn text_run_measurement(&mut self, text: &str) -> DisplayTextRunMeasurement {
        self.active_face_state
            .text_run_measurement(self.font_metrics, text)
    }

    fn advance_for_columns(&mut self, ch: char, columns: usize) -> f32 {
        self.active_face_state
            .advance_for_columns(self.font_metrics, ch, columns)
    }
}

impl DisplayRowActiveFaceState {
    pub(crate) fn new(resolved_face: ResolvedFace, measured_face: DisplayRowMeasuredFace) -> Self {
        let face_id = measured_face.face_id();
        let background = Color::from_pixel(resolved_face.bg);
        let metrics = measured_face.metrics();
        Self {
            render: DisplayRowActiveFaceRenderState {
                face_id,
                background,
                resolved_face,
            },
            measurement: DisplayRowActiveFaceMeasurementState {
                measurement_face: measured_face.into_measurement_face(),
                metrics,
            },
        }
    }

    pub(crate) fn face_id(&self) -> FaceId {
        self.render.face_id
    }

    pub(crate) fn background(&self) -> Color {
        self.render.background
    }

    pub(crate) fn resolved_face(&self) -> &ResolvedFace {
        self.render.resolved_face()
    }

    pub(crate) fn row_extend_fill(&self) -> Option<DisplayRowExtendFace> {
        self.resolved_face()
            .extend
            .then(|| DisplayRowExtendFace::new(self.background(), self.face_id(), self.metrics()))
    }

    pub(crate) fn metrics(&self) -> DisplayRowMeasuredFaceMetrics {
        self.measurement.metrics()
    }

    pub(crate) fn advance_for_char(
        &self,
        font_metrics: &mut Option<FontMetricsService>,
        ch: char,
        fallback_advance_px: f32,
    ) -> f32 {
        self.measurement
            .advance_for_char(font_metrics, ch, fallback_advance_px)
    }

    pub(crate) fn advance_for_columns(
        &self,
        font_metrics: &mut Option<FontMetricsService>,
        ch: char,
        columns: usize,
    ) -> f32 {
        if columns == 0 {
            return 0.0;
        }
        let fallback_advance_px = self.metrics().char_width() * columns as f32;
        self.advance_for_char(font_metrics, ch, fallback_advance_px)
    }

    pub(crate) fn display_replacement_string_cursor_slot_width(
        &self,
        font_metrics: &mut Option<FontMetricsService>,
        replacement: &str,
    ) -> f32 {
        let face_metrics = self.metrics();
        replacement
            .chars()
            .next()
            .map(|ch| self.advance_for_char(font_metrics, ch, face_metrics.char_width()))
            .unwrap_or_else(|| face_metrics.char_width().max(1.0))
    }

    pub(crate) fn display_replacement_stretch_source_char_width(
        &self,
        font_metrics: &mut Option<FontMetricsService>,
        source_char: char,
    ) -> f32 {
        self.advance_for_char(font_metrics, source_char, self.metrics().char_width())
    }

    pub(crate) fn complex_text_run_advance(
        &self,
        font_metrics: &mut Option<FontMetricsService>,
        resolver: &mut ComplexTextRunAdvanceResolver,
        text: &[u8],
        byte_idx: usize,
        ch: char,
        is_cluster_continuation: bool,
    ) -> f32 {
        let mut policy = DisplayRowComplexTextRunAdvancePolicy::new(self, font_metrics);
        resolver.advance_for_char(text, byte_idx, ch, is_cluster_continuation, &mut policy)
    }

    pub(crate) fn text_run_measurement(
        &self,
        font_metrics: &mut Option<FontMetricsService>,
        text: &str,
    ) -> DisplayTextRunMeasurement {
        self.measurement.text_run_measurement(font_metrics, text)
    }
}

/// Complete realized face state needed to replay GNU's end-of-line `:extend`
/// fill. Keeping metrics with the paint identity prevents a later item from
/// supplying its face metrics after word-wrap rewinds to an earlier source
/// boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowExtendFace {
    background: Color,
    face_id: FaceId,
    metrics: DisplayRowMeasuredFaceMetrics,
}

impl DisplayRowExtendFace {
    pub(crate) const fn new(
        background: Color,
        face_id: FaceId,
        metrics: DisplayRowMeasuredFaceMetrics,
    ) -> Self {
        Self {
            background,
            face_id,
            metrics,
        }
    }

    pub(crate) const fn background(self) -> Color {
        self.background
    }

    pub(crate) const fn face_id(self) -> FaceId {
        self.face_id
    }

    pub(crate) const fn metrics(self) -> DisplayRowMeasuredFaceMetrics {
        self.metrics
    }
}

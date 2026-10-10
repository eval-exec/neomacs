//! COLR paint-graph traversal (`ttf-parser`) and rasterization (`tiny-skia`).
//!
//! Traversal is deliberately split from rasterization: ttf-parser's `Painter`
//! callbacks are recorded into a flat step list in *font units*, then replayed
//! twice — once to measure the device-space bounds that size the bitmap, once
//! to paint.  Nothing in the traversal knows about pixels, so the recorded
//! steps stay a faithful, testable description of the paint graph.
//!
//! Spec rules implemented here rather than delegated:
//!
//! * A basic fill with no bounding ancestor (no `PaintGlyph` clip and no
//!   `ClipBox`) is unbounded, and must not render.
//! * A fill with no shape of its own paints the clip in force: a `ClipBox`
//!   alone (or an enclosing `PaintGlyph`) is what makes a bare fill bounded.
//! * The clip that is a fill's own geometry is not applied to that fill a
//!   second time; for a `PaintGlyph` the two are the same coverage field, and
//!   multiplying them would square every antialiased edge.
//! * Degenerate gradients paint nothing: a linear gradient whose axis
//!   collapses onto its rotation reference, identical radial circles, a sweep
//!   whose color line repeats around a single angle.
//! * A color line with a single offset is ill-formed for repeat/reflect and
//!   paints nothing.
//! * Gradient geometry and glyph outlines share one transform stack, so a
//!   transformed gradient is rasterized in the same space as its shape.

use super::{ColorFaceBytes, ColorGlyphRaster, ColorGlyphRequest};
use crate::raster_sources::ColorGlyphSource;
use tiny_skia::{
    BlendMode, FillRule, Mask, Paint as SkiaPaint, Path, PathBuilder, Pixmap, PixmapPaint, Point,
    Shader, SpreadMode, Transform as SkiaTransform,
};
use ttf_parser::colr::{
    ClipBox, ColorStop, CompositeMode, GradientExtend, Paint, Painter, Table as ColrTable,
};
use ttf_parser::{Face, GlyphId, NormalizedCoordinate, OutlineBuilder, RgbaColor, Transform};

/// Caps that keep a malformed paint graph from allocating an unbounded
/// surface.  Real color glyphs stay far below them: a 128 px emoji rasterizes
/// about 160 px across.
const MAX_SIDE: u32 = 1024;
const MAX_PIXELS: u64 = MAX_SIDE as u64 * MAX_SIDE as u64;

/// Caps on one recorded graph.  ttf-parser bounds recursion depth and detects
/// cycles, but a legal graph can still fan out exponentially through the one
/// `LayerList`; a real glyph stays orders of magnitude below these.
const MAX_STEPS: usize = 1 << 15;
const MAX_PATHS: usize = 1 << 12;

/// One `F2DOT14` step, the smallest representable stop-offset gap.
const F2DOT14_STEP: f32 = 1.0 / 16384.0;

/// Index into the recorded path arena.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PathId(u32);

/// A shape in font units plus the graph transform that was current when the
/// paint graph visited it.
#[derive(Clone, Copy, Debug)]
struct Positioned {
    path: Option<PathId>,
    transform: SkiaTransform,
}

/// One recorded step of the flattened paint graph, in traversal order.
#[derive(Clone, Debug)]
enum Step {
    Fill {
        /// The region the fill paints, with the transform it was outlined
        /// under: a transform pushed between a `PaintGlyph` and its child
        /// moves the paint, not the clip outline.
        shape: Positioned,
        /// The transform in force when the paint table itself was reached.
        /// Gradients are placed by it, relative to the shape.
        paint_transform: SkiaTransform,
        paint: PaintRecord,
        /// Whether a bounding ancestor covered this fill.
        bounded: bool,
    },
    PushClip(Positioned),
    PushClipBox {
        clip_box: ClipBox,
        transform: SkiaTransform,
    },
    PopClip,
    PushLayer(CompositeMode),
    PopLayer,
}

/// A recorded shader in font units.  Gradient stops are already sanitized and
/// normalized into the shader's `[0, 1]` domain; the geometry is remapped to
/// match, so no spread-mode adjustment is needed at paint time.
///
/// A sweep is recorded as a full-turn color line: the arc is folded into the
/// stop list at record time, because tiny-skia skips its angle scale/bias
/// stage when the start angle is zero and would then ignore the end angle.
#[derive(Clone, Debug)]
enum PaintRecord {
    Solid(RgbaColor),
    Linear {
        start: Point,
        end: Point,
        stops: Vec<(f32, RgbaColor)>,
        spread: SpreadMode,
    },
    Radial {
        start: Point,
        start_radius: f32,
        end: Point,
        end_radius: f32,
        stops: Vec<(f32, RgbaColor)>,
        spread: SpreadMode,
    },
    Sweep {
        center: Point,
        stops: Vec<(f32, RgbaColor)>,
    },
}

/// Where a clip's own coverage came from.
///
/// A `PaintGlyph` fill and the clip it recorded are the same path; identity
/// here is how a fill recognizes a clip that *is* its geometry rather than a
/// bound on it, so that clip is not applied a second time.
#[derive(Clone, Copy, Debug)]
enum ClipSource {
    Outline(Positioned),
    Box {
        clip_box: ClipBox,
        transform: SkiaTransform,
    },
}

impl ClipSource {
    /// Whether the two cover the same region, which makes applying the second
    /// a no-op for a fill that is the first.
    ///
    /// Outlines compare by geometry rather than by stored path: the same glyph
    /// outlined twice — nested identical `PaintGlyph` clips, say — is the same
    /// coverage field even though it has two path ids.
    fn same_geometry(&self, other: &ClipSource, paths: &[Path]) -> bool {
        match (self, other) {
            (ClipSource::Outline(left), ClipSource::Outline(right)) => {
                transform_matches(left.transform, right.transform)
                    && match (left.path, right.path) {
                        (Some(left), Some(right)) => paths_match(paths, left, right),
                        _ => false,
                    }
            }
            (
                ClipSource::Box {
                    clip_box: left,
                    transform: left_transform,
                },
                ClipSource::Box {
                    clip_box: right,
                    transform: right_transform,
                },
            ) => left == right && transform_matches(*left_transform, *right_transform),
            _ => false,
        }
    }
}

/// Exact geometry equality between two recorded paths.
fn paths_match(paths: &[Path], left: PathId, right: PathId) -> bool {
    if left == right {
        return true;
    }
    let (Some(left), Some(right)) = (paths.get(left.0 as usize), paths.get(right.0 as usize))
    else {
        return false;
    };
    left.verbs() == right.verbs() && left.points() == right.points()
}

/// Exact transform equality: the two were composed from the same stack.
fn transform_matches(left: SkiaTransform, right: SkiaTransform) -> bool {
    left.sx.to_bits() == right.sx.to_bits()
        && left.kx.to_bits() == right.kx.to_bits()
        && left.ky.to_bits() == right.ky.to_bits()
        && left.sy.to_bits() == right.sy.to_bits()
        && left.tx.to_bits() == right.tx.to_bits()
        && left.ty.to_bits() == right.ty.to_bits()
}

/// Maps a color line onto the shader's `t ∈ [0, 1]` domain.
///
/// `omega_start`/`omega_span` describe where the shader domain sits on the
/// color line's own parameter, the quantity gradient geometry is expressed in.
#[derive(Clone, Debug)]
struct ColorLine {
    stops: Vec<(f32, RgbaColor)>,
    spread: SpreadMode,
    omega_start: f32,
    omega_span: f32,
}

/// Records the paint graph into steps.
struct GraphRecorder<'a> {
    face: &'a Face<'a>,
    palette: u16,
    variation_coords: &'a [NormalizedCoordinate],
    paths: Vec<Path>,
    steps: Vec<Step>,
    /// Transform stack; the top is the transform current steps are recorded in.
    transforms: Vec<SkiaTransform>,
    /// The path most recently produced by `outline_glyph`, with the transform
    /// in force when it was outlined.  The transform is part of the record:
    /// a `PaintTransform` between a `PaintGlyph` and its child moves the
    /// child's paint, while the outline the clip was taken from stays put.
    outlined: Option<Positioned>,
    /// Depth of bounding ancestors (`PaintGlyph` clips and clip boxes).
    bounding_depth: usize,
    /// Set when the graph is malformed; the glyph then paints nothing.
    malformed: bool,
}

impl<'a> GraphRecorder<'a> {
    fn new(face: &'a Face<'a>, palette: u16, variation_coords: &'a [NormalizedCoordinate]) -> Self {
        Self {
            face,
            palette,
            variation_coords,
            paths: Vec::new(),
            steps: Vec::new(),
            transforms: vec![SkiaTransform::identity()],
            outlined: None,
            bounding_depth: 0,
            malformed: false,
        }
    }

    fn current_transform(&self) -> SkiaTransform {
        *self
            .transforms
            .last()
            .expect("transform stack never empties")
    }

    fn positioned(&self, path: Option<PathId>) -> Positioned {
        Positioned {
            path,
            transform: self.current_transform(),
        }
    }

    fn push_path(&mut self, path: Path) -> PathId {
        if self.paths.len() >= MAX_PATHS {
            self.malformed = true;
            // A dangling id: nothing resolves it, and the graph is dropped.
            return PathId(u32::MAX);
        }
        self.paths.push(path);
        PathId(self.paths.len() as u32 - 1)
    }

    /// Record one step, or give up on the whole graph once the cap is hit.
    ///
    /// ttf-parser keeps traversing after the cap (the `Painter` trait cannot
    /// stop it), so the recorder's job is only to stay cheap from here on.
    fn record_step(&mut self, step: Step) {
        if self.steps.len() >= MAX_STEPS {
            self.malformed = true;
            return;
        }
        self.steps.push(step);
    }

    fn record_paint(&mut self, paint: &Paint<'_>) -> Option<PaintRecord> {
        match paint {
            Paint::Solid(color) => Some(PaintRecord::Solid(*color)),
            Paint::LinearGradient(gradient) => {
                let start = Point::from_xy(gradient.x0, gradient.y0);
                let axis = Point::from_xy(gradient.x1 - gradient.x0, gradient.y1 - gradient.y0);
                if axis.x == 0.0 && axis.y == 0.0 {
                    // Spec: p1 coincident with p0 is ill-formed.
                    return None;
                }
                // p2 is a rotation reference only: colors are projected along
                // the direction perpendicular to p0p2, so the effective second
                // point is p1 projected onto that direction.
                let reference =
                    Point::from_xy(gradient.x2 - gradient.x0, gradient.y2 - gradient.y0);
                let direction = if reference.x == 0.0 && reference.y == 0.0 {
                    axis
                } else {
                    let squared = reference.x * reference.x + reference.y * reference.y;
                    let dot = (axis.x * reference.x + axis.y * reference.y) / squared;
                    Point::from_xy(axis.x - dot * reference.x, axis.y - dot * reference.y)
                };
                if direction.x == 0.0 && direction.y == 0.0 {
                    // The axis is parallel to the rotation reference: the spec
                    // calls this ill-formed and requires painting nothing.
                    return None;
                }
                let color_line = self.build_color_line(
                    gradient.stops(self.palette, self.variation_coords),
                    gradient.extend,
                )?;
                let shifted = |offset: f32| {
                    Point::from_xy(
                        start.x + direction.x * offset,
                        start.y + direction.y * offset,
                    )
                };
                Some(PaintRecord::Linear {
                    start: shifted(color_line.omega_start),
                    end: shifted(color_line.omega_start + color_line.omega_span),
                    stops: color_line.stops,
                    spread: color_line.spread,
                })
            }
            Paint::RadialGradient(gradient) => {
                let concentric = gradient.x0 == gradient.x1 && gradient.y0 == gradient.y1;
                if concentric && gradient.r0 == gradient.r1 {
                    // Spec: identical circles paint nothing.
                    return None;
                }
                if gradient.r0 < 0.0 || gradient.r1 < 0.0 {
                    // Negative radii are legal after variations, but no
                    // rasterizer here can express the resulting cone.
                    tracing::debug!(
                        target: "font_boundary",
                        start_radius = gradient.r0,
                        end_radius = gradient.r1,
                        "skipping radial gradient with a negative radius"
                    );
                    return None;
                }
                let color_line = self.build_color_line(
                    gradient.stops(self.palette, self.variation_coords),
                    gradient.extend,
                )?;
                let center = |offset: f32| {
                    Point::from_xy(
                        gradient.x0 + (gradient.x1 - gradient.x0) * offset,
                        gradient.y0 + (gradient.y1 - gradient.y0) * offset,
                    )
                };
                let radius = |offset: f32| gradient.r0 + (gradient.r1 - gradient.r0) * offset;
                let end = color_line.omega_start + color_line.omega_span;
                Some(PaintRecord::Radial {
                    start: center(color_line.omega_start),
                    start_radius: radius(color_line.omega_start),
                    end: center(end),
                    end_radius: radius(end),
                    stops: color_line.stops,
                    spread: color_line.spread,
                })
            }
            Paint::SweepGradient(gradient) => {
                if gradient.start_angle == gradient.end_angle
                    && !matches!(gradient.extend, GradientExtend::Pad)
                {
                    // Spec: a repeating color line around one angle paints
                    // nothing.
                    return None;
                }
                let color_line = self.build_color_line(
                    gradient.stops(self.palette, self.variation_coords),
                    // The arc itself is the domain: the color line is sampled
                    // along it and never tiles, so the unit-domain form is
                    // exactly the pad form.
                    GradientExtend::Pad,
                )?;
                // Spec: the stored F2DOT14 is degrees biased by 180 — "add 1.0
                // and multiply by 180°".  Halved, it is the fraction of a turn.
                let turn = |raw: f32| (raw + 1.0) * 0.5;
                let (start, end, stops) = if gradient.start_angle > gradient.end_angle {
                    // The arc runs the other way; the color line follows it.
                    let reversed = color_line
                        .stops
                        .iter()
                        .rev()
                        .map(|(offset, color)| (1.0 - offset, *color))
                        .collect();
                    (
                        turn(gradient.end_angle),
                        turn(gradient.start_angle),
                        reversed,
                    )
                } else {
                    (
                        turn(gradient.start_angle),
                        turn(gradient.end_angle),
                        color_line.stops,
                    )
                };
                // Fold the arc into a full-turn color line: before the arc the
                // first colour holds, after it the last.  Equal angles collapse
                // to a hard cut between the two, which is the pad limit.
                let first = stops.first().map(|(_, color)| *color)?;
                let last = stops.last().map(|(_, color)| *color)?;
                let mut folded = Vec::with_capacity(stops.len() + 2);
                if start > 0.0 {
                    folded.push((0.0, first));
                }
                folded.extend(
                    stops
                        .into_iter()
                        .map(|(offset, color)| (start + offset * (end - start), color)),
                );
                if end < 1.0 {
                    folded.push((1.0, last));
                }
                Some(PaintRecord::Sweep {
                    center: Point::from_xy(gradient.center_x, gradient.center_y),
                    stops: sanitize_unit_stops(folded),
                })
            }
        }
    }

    /// Sanitize a color line into the shader's unit domain.
    ///
    /// Stop offsets are `F2DOT14` and may lie outside `[0, 1)`.  Under pad the
    /// color function extrapolates constantly, so re-sampling at the domain
    /// boundary is exact; under repeat/reflect the color line's defined
    /// interval is one period, which the caller's geometry must be scaled to.
    fn build_color_line(
        &self,
        stops: impl Iterator<Item = ColorStop>,
        extend: GradientExtend,
    ) -> Option<ColorLine> {
        let mut stops: Vec<(f32, RgbaColor)> =
            stops.map(|stop| (stop.stop_offset, stop.color)).collect();
        if stops.is_empty() {
            // Spec: an empty color line is transparent black, which paints
            // nothing observable.
            return None;
        }
        stops.sort_by(|left, right| left.0.total_cmp(&right.0));
        let first = stops[0].0;
        let last = stops[stops.len() - 1].0;
        if stops.len() == 1 || first == last {
            if !matches!(extend, GradientExtend::Pad) {
                // Spec: repeat/reflect around a single offset is ill-formed.
                return None;
            }
            // Spec: with every stop at one offset the first colour holds
            // below it and the last holds at and above it — a hard cut, not a
            // ramp.  Resampling the two-stop form also lands the cut correctly
            // when the offset lies outside the unit domain.
            let cut = [
                (first, stops[0].1),
                (first + F2DOT14_STEP, stops[stops.len() - 1].1),
            ];
            return Some(ColorLine {
                stops: resample_to_unit_domain(&cut),
                spread: SpreadMode::Pad,
                omega_start: 0.0,
                omega_span: 1.0,
            });
        }
        match extend {
            GradientExtend::Pad => Some(ColorLine {
                stops: resample_to_unit_domain(&stops),
                spread: SpreadMode::Pad,
                omega_start: 0.0,
                omega_span: 1.0,
            }),
            GradientExtend::Repeat | GradientExtend::Reflect => {
                let period = last - first;
                Some(ColorLine {
                    stops: stops
                        .into_iter()
                        .map(|(offset, color)| ((offset - first) / period, color))
                        .collect(),
                    spread: if matches!(extend, GradientExtend::Repeat) {
                        SpreadMode::Repeat
                    } else {
                        SpreadMode::Reflect
                    },
                    omega_start: first,
                    omega_span: period,
                })
            }
        }
    }
}

/// Restrict a sorted color line to `[0, 1]`, re-sampling the boundary colors.
///
/// Clamping offsets would change the interpolation slope; sampling the color
/// function exactly at the boundary keeps it identical.
fn resample_to_unit_domain(stops: &[(f32, RgbaColor)]) -> Vec<(f32, RgbaColor)> {
    let mut domain: Vec<(f32, RgbaColor)> = Vec::with_capacity(stops.len() + 2);
    if stops[0].0 > 0.0 {
        // tiny-skia extends the first stop's color down to 0 on its own.
        domain.push((0.0, stops[0].1));
    } else if stops[0].0 < 0.0 {
        domain.push((0.0, sample_color_line(stops, 0.0)));
    }
    for (offset, color) in stops {
        if *offset >= 0.0 && *offset <= 1.0 {
            domain.push((*offset, *color));
        }
    }
    let last = stops[stops.len() - 1].0;
    if last < 1.0 {
        domain.push((1.0, stops[stops.len() - 1].1));
    } else if last > 1.0 {
        domain.push((1.0, sample_color_line(stops, 1.0)));
    }
    sanitize_unit_stops(domain)
}

/// Evaluate a sorted, piecewise-linear color line, constant outside its stops.
fn sample_color_line(stops: &[(f32, RgbaColor)], position: f32) -> RgbaColor {
    if position <= stops[0].0 {
        return stops[0].1;
    }
    let last = stops[stops.len() - 1];
    if position >= last.0 {
        return last.1;
    }
    let upper = stops.partition_point(|(offset, _)| *offset <= position);
    let (low_offset, low_color) = stops[upper - 1];
    let (high_offset, high_color) = stops[upper];
    let span = high_offset - low_offset;
    if span <= 0.0 {
        return low_color;
    }
    let t = (position - low_offset) / span;
    mix_color(low_color, high_color, t)
}

fn mix_color(from: RgbaColor, to: RgbaColor, t: f32) -> RgbaColor {
    let blend = |a: u8, b: u8| ((f32::from(a) + (f32::from(b) - f32::from(a)) * t).round()) as u8;
    RgbaColor {
        red: blend(from.red, to.red),
        green: blend(from.green, to.green),
        blue: blend(from.blue, to.blue),
        alpha: blend(from.alpha, to.alpha),
    }
}

/// Final sanitization: hard edges for equal offsets, strictly increasing.
///
/// The spec hard-cuts between equal-offset stops — the first color is used
/// below the offset, the last at and above it.  tiny-skia cannot draw two
/// stops at one position, so the later color moves by one `F2DOT14` step,
/// which is invisible at any raster size while still ordering the pair.
fn sanitize_unit_stops(stops: Vec<(f32, RgbaColor)>) -> Vec<(f32, RgbaColor)> {
    const STEP: f32 = F2DOT14_STEP;
    let mut out: Vec<(f32, RgbaColor)> = Vec::with_capacity(stops.len());
    let mut index = 0;
    while index < stops.len() {
        let offset = stops[index].0.clamp(0.0, 1.0);
        let mut end = index;
        while end + 1 < stops.len() && stops[end + 1].0.clamp(0.0, 1.0) <= offset {
            end += 1;
        }
        let first = stops[index].1;
        out.push((offset, first));
        if end > index {
            out.push(((offset + STEP).min(1.0), stops[end].1));
        }
        index = end + 1;
    }
    for position in 1..out.len() {
        if out[position].0 <= out[position - 1].0 {
            out[position].0 = (out[position - 1].0 + STEP).min(1.0);
        }
    }
    out
}

impl<'a> Painter<'a> for GraphRecorder<'a> {
    fn outline_glyph(&mut self, glyph_id: GlyphId) {
        if self.malformed {
            return;
        }
        let mut outline = GlyphPath::default();
        self.face.outline_glyph(glyph_id, &mut outline);
        let transform = self.current_transform();
        // No outline leaves the stored path empty, so a fill paints nothing
        // and a clip clips everything away.
        self.outlined = outline.finish().map(|path| Positioned {
            path: Some(self.push_path(path)),
            transform,
        });
    }

    fn paint(&mut self, paint: Paint<'a>) {
        if self.malformed {
            return;
        }
        let Some(record) = self.record_paint(&paint) else {
            return;
        };
        let step = Step::Fill {
            // A transform between the `PaintGlyph` and this paint moved the
            // paint, not the outline the clip was taken from: the shape keeps
            // the transform captured at outline time.
            shape: self.outlined.unwrap_or_else(|| self.positioned(None)),
            paint_transform: self.current_transform(),
            paint: record,
            // A fill is bounded by a bounding ancestor or by its own shape.
            // Version 0 layer records produce the second case: they outline a
            // layer glyph and fill it without any `PaintGlyph` clip. Only a
            // fill with neither — a bare `PaintSolid` reached without a path —
            // is unbounded, and the spec requires it not to render.
            bounded: self.bounding_depth > 0 || self.outlined.is_some(),
        };
        self.record_step(step);
    }

    fn push_clip(&mut self) {
        if self.malformed {
            return;
        }
        let step = Step::PushClip(self.outlined.unwrap_or_else(|| self.positioned(None)));
        self.record_step(step);
        self.bounding_depth += 1;
    }

    fn push_clip_box(&mut self, clip_box: ClipBox) {
        if self.malformed {
            return;
        }
        let step = Step::PushClipBox {
            clip_box,
            transform: self.current_transform(),
        };
        self.record_step(step);
        self.bounding_depth += 1;
    }

    fn pop_clip(&mut self) {
        if self.malformed {
            return;
        }
        self.record_step(Step::PopClip);
        self.bounding_depth = self.bounding_depth.saturating_sub(1);
        // The outline scoped to that clip is gone with it: a fill recorded
        // after the pop has no shape of its own and paints the clip in force
        // (or is unbounded, when none is).
        self.outlined = None;
    }

    fn push_layer(&mut self, mode: CompositeMode) {
        if self.malformed {
            return;
        }
        self.record_step(Step::PushLayer(mode));
    }

    fn pop_layer(&mut self) {
        if self.malformed {
            return;
        }
        self.record_step(Step::PopLayer);
    }

    fn push_transform(&mut self, transform: Transform) {
        if !transform_is_invertible(&transform) {
            // A non-invertible transform cannot be replayed; dropping the
            // sub-graph is the spec's error recovery.
            self.malformed = true;
            return;
        }
        self.transforms.push(
            self.current_transform()
                .pre_concat(colr_transform(transform)),
        );
    }

    fn pop_transform(&mut self) {
        if self.transforms.len() > 1 {
            self.transforms.pop();
        }
    }
}

/// Map a COLR graph transform onto tiny-skia's row convention.
///
/// COLR defines `x' = a·x + c·y + e; y' = b·x + d·y + f` in the design grid;
/// tiny-skia's `from_row(sx, ky, kx, sy, tx, ty)` is the same shape with the
/// axes named separately.
fn colr_transform(transform: Transform) -> SkiaTransform {
    SkiaTransform::from_row(
        transform.a,
        transform.b,
        transform.c,
        transform.d,
        transform.e,
        transform.f,
    )
}

fn transform_is_invertible(transform: &Transform) -> bool {
    let determinant = transform.a * transform.d - transform.b * transform.c;
    determinant.is_finite() && determinant != 0.0
}

/// Builds a tiny-skia path from a glyph outline in font units.
#[derive(Default)]
struct GlyphPath {
    builder: PathBuilder,
}

impl GlyphPath {
    /// `None` for an outline with no contours: `PathBuilder::finish` already
    /// reports an empty builder that way.
    fn finish(self) -> Option<Path> {
        self.builder.finish()
    }
}

impl OutlineBuilder for GlyphPath {
    fn move_to(&mut self, x: f32, y: f32) {
        self.builder.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.builder.line_to(x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.builder.quad_to(x1, y1, x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.builder.cubic_to(x1, y1, x2, y2, x, y);
    }

    fn close(&mut self) {
        self.builder.close();
    }
}

// --- rasterization -------------------------------------------------------

/// Rasterize one glyph from a COLR/CPAL face.
///
/// Returns `None` when the face has no color table, the glyph has no paint
/// graph, or the graph paints nothing.
pub(super) fn paint_colr_glyph(
    source: &ColorFaceBytes,
    request: &ColorGlyphRequest<'_>,
) -> Option<ColorGlyphRaster> {
    let mut face = source.face()?;
    if !request.variations.is_empty() {
        for (tag, value) in request.variations {
            face.set_variation(*tag, *value);
        }
    }
    let units_per_em = f32::from(face.units_per_em());
    if !units_per_em.is_finite() || units_per_em <= 0.0 {
        return None;
    }
    let coords: Vec<NormalizedCoordinate> = face.variation_coordinates().to_vec();

    let mut recorder = GraphRecorder::new(&face, request.palette, &coords);
    let colr: ColrTable<'_> = face.tables().colr?;
    colr.paint(
        GlyphId(request.glyph_id),
        request.palette,
        &mut recorder,
        &coords,
        RgbaColor {
            red: request.foreground[0],
            green: request.foreground[1],
            blue: request.foreground[2],
            alpha: request.foreground[3],
        },
    )?;
    if recorder.malformed || recorder.steps.is_empty() {
        return None;
    }

    let scale = request.px_size / units_per_em;
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    // Device space: font units scaled to pixels, y flipped so the baseline is
    // y = 0 and the glyph grows upwards, plus the sub-pixel bin offset.
    let device =
        SkiaTransform::from_row(scale, 0.0, 0.0, -scale, request.offset.0, request.offset.1);

    let bounds = measure(&recorder, device)?;
    let place = SkiaTransform::from_translate(-bounds.left_px as f32, -bounds.top_px as f32);
    // Pixel = place(device(font unit)) — the grid translation happens in
    // device space, after the graph transform has been applied.
    let placement = place.pre_concat(device);

    let mut surface = Pixmap::new(bounds.width_cells, bounds.height_cells)?;
    let mut state = RasterState::new(&recorder.paths, placement, &surface);
    if !state.replay(&recorder.steps, &mut surface) {
        return None;
    }
    let rgba = surface.take_demultiplied();
    let expected = bounds.width_cells as usize * bounds.height_cells as usize * 4;
    if rgba.len() != expected {
        return None;
    }
    Some(ColorGlyphRaster {
        left: bounds.left_px,
        top: -bounds.top_px,
        width: bounds.width_cells,
        height: bounds.height_cells,
        rgba,
        source: ColorGlyphSource::Colr,
    })
}

/// Device-space bounds of everything the graph can paint, snapped outwards.
#[derive(Clone, Copy, Debug)]
struct DeviceBounds {
    left_px: i32,
    /// Top edge in device (y-down) pixels.
    top_px: i32,
    width_cells: u32,
    height_cells: u32,
}

/// One clip in force during measurement: its own bounds, where that coverage
/// came from, and — for a clip box — the rect a shapeless fill can adopt.
struct MeasureClip {
    own: Option<RectF>,
    source: ClipSource,
}

/// Union of every bounded fill's transformed bounds, intersected with the clip
/// stack in force when it was recorded.
///
/// A clip whose source *is* the fill's own geometry is skipped: it bounds the
/// fill by being its shape, and multiplying it in again would square every
/// antialiased edge.
fn measure(recorder: &GraphRecorder<'_>, device: SkiaTransform) -> Option<DeviceBounds> {
    let mut clips: Vec<MeasureClip> = Vec::new();
    let mut union: Option<RectF> = None;
    for step in &recorder.steps {
        match step {
            Step::Fill { shape, bounded, .. } => {
                if !bounded {
                    continue;
                }
                let Some((bounds, source)) = measure_geometry(recorder, shape, &clips, device)
                else {
                    continue;
                };
                let clipped = clips
                    .iter()
                    .filter(|clip| !clip.source.same_geometry(&source, &recorder.paths))
                    .try_fold(bounds, |current, clip| {
                        // An empty clip — an outline with no contours — hides
                        // everything under it.
                        current.intersect(clip.own?)
                    });
                if let Some(clipped) = clipped {
                    union = Some(match union {
                        Some(union) => union.union(clipped),
                        None => clipped,
                    });
                }
            }
            Step::PushClip(positioned) => clips.push(MeasureClip {
                own: shape_bounds(recorder, positioned, device),
                source: ClipSource::Outline(*positioned),
            }),
            Step::PushClipBox {
                clip_box,
                transform,
            } => clips.push(MeasureClip {
                own: clip_box_bounds(*clip_box, *transform, device),
                source: ClipSource::Box {
                    clip_box: *clip_box,
                    transform: *transform,
                },
            }),
            Step::PopClip => {
                clips.pop();
            }
            Step::PushLayer(_) | Step::PopLayer => {}
        }
    }
    let union = union?;
    // Widths first, in the measurement's own space: casting the edges to i32
    // and subtracting them can overflow once a transform pushed the glyph
    // past the type's range.
    let left = union.x_min.floor();
    let top = union.y_min.floor();
    let width = union.x_max.ceil() - left;
    let height = union.y_max.ceil() - top;
    let cap = MAX_SIDE as f32;
    if !(1.0..=cap).contains(&width) || !(1.0..=cap).contains(&height) {
        return None;
    }
    let width_cells = width as u32;
    let height_cells = height as u32;
    if u64::from(width_cells) * u64::from(height_cells) > MAX_PIXELS {
        return None;
    }
    Some(DeviceBounds {
        left_px: left as i32,
        top_px: top as i32,
        width_cells,
        height_cells,
    })
}

/// The region a fill paints, per the recorded graph: its own shape, or the
/// clip in force when it has none — the spec makes that clip the fill's shape.
fn measure_geometry(
    recorder: &GraphRecorder<'_>,
    shape: &Positioned,
    clips: &[MeasureClip],
    device: SkiaTransform,
) -> Option<(RectF, ClipSource)> {
    if shape.path.is_some() {
        let bounds = shape_bounds(recorder, shape, device)?;
        return Some((bounds, ClipSource::Outline(*shape)));
    }
    match clips.last()?.source {
        ClipSource::Outline(outline) if outline.path.is_some() => {
            let bounds = shape_bounds(recorder, &outline, device)?;
            Some((bounds, ClipSource::Outline(outline)))
        }
        ClipSource::Box {
            clip_box,
            transform,
        } => {
            let bounds = clip_box_bounds(clip_box, transform, device)?;
            Some((
                bounds,
                ClipSource::Box {
                    clip_box,
                    transform,
                },
            ))
        }
        ClipSource::Outline(_) => None,
    }
}

/// Axis-aligned device-space bounds of a positioned path.
fn shape_bounds(
    recorder: &GraphRecorder<'_>,
    positioned: &Positioned,
    device: SkiaTransform,
) -> Option<RectF> {
    let path = positioned
        .path
        .and_then(|id| recorder.paths.get(id.0 as usize))?;
    let bounds = path.bounds();
    let matrix = device.pre_concat(positioned.transform);
    transformed_corners(
        matrix,
        [
            (bounds.left(), bounds.top()),
            (bounds.right(), bounds.top()),
            (bounds.right(), bounds.bottom()),
            (bounds.left(), bounds.bottom()),
        ],
    )
}

fn clip_box_bounds(
    clip_box: ClipBox,
    transform: SkiaTransform,
    device: SkiaTransform,
) -> Option<RectF> {
    let matrix = device.pre_concat(transform);
    transformed_corners(
        matrix,
        [
            (clip_box.x_min, clip_box.y_min),
            (clip_box.x_max, clip_box.y_min),
            (clip_box.x_max, clip_box.y_max),
            (clip_box.x_min, clip_box.y_max),
        ],
    )
}

fn transformed_corners(matrix: SkiaTransform, corners: [(f32, f32); 4]) -> Option<RectF> {
    let mut rect = RectF::empty();
    for (x, y) in corners {
        let mut point = Point::from_xy(x, y);
        matrix.map_point(&mut point);
        rect.x_min = rect.x_min.min(point.x);
        rect.y_min = rect.y_min.min(point.y);
        rect.x_max = rect.x_max.max(point.x);
        rect.y_max = rect.y_max.max(point.y);
    }
    rect.is_finite().then_some(rect)
}

/// A device-space rectangle used only for measurement.
#[derive(Clone, Copy, Debug)]
struct RectF {
    x_min: f32,
    y_min: f32,
    x_max: f32,
    y_max: f32,
}

impl RectF {
    /// An empty accumulation: minima start at `+INF`, maxima at `-INF`.
    fn empty() -> Self {
        Self {
            x_min: f32::INFINITY,
            y_min: f32::INFINITY,
            x_max: f32::NEG_INFINITY,
            y_max: f32::NEG_INFINITY,
        }
    }

    fn is_finite(self) -> bool {
        self.x_min.is_finite()
            && self.y_min.is_finite()
            && self.x_max.is_finite()
            && self.y_max.is_finite()
    }

    fn intersect(self, other: Self) -> Option<Self> {
        let rect = Self {
            x_min: self.x_min.max(other.x_min),
            y_min: self.y_min.max(other.y_min),
            x_max: self.x_max.min(other.x_max),
            y_max: self.y_max.min(other.y_max),
        };
        (rect.x_min < rect.x_max && rect.y_min < rect.y_max).then_some(rect)
    }

    fn union(self, other: Self) -> Self {
        Self {
            x_min: self.x_min.min(other.x_min),
            y_min: self.y_min.min(other.y_min),
            x_max: self.x_max.max(other.x_max),
            y_max: self.y_max.max(other.y_max),
        }
    }
}

/// One clip in force: its own coverage, the coverage of every clip down to it,
/// and where its own coverage came from.
struct ClipFrame {
    own: Mask,
    cumulative: Mask,
    source: ClipSource,
    /// The rect a `ClipBox` frame clips to, kept so a shapeless fill under it
    /// can adopt that rect as its own shape.
    box_path: Option<Path>,
}

/// Replays recorded steps onto a pixmap: clip stack, layer stack, fills.
struct RasterState<'a> {
    paths: &'a [Path],
    /// Pixel = transform(font units); graph transforms are composed per step.
    transform: SkiaTransform,
    clips: Vec<ClipFrame>,
    layers: Vec<(Pixmap, CompositeMode)>,
    size: (u32, u32),
}

/// The region a fill paints, for replay: its own recorded shape, or — when it
/// has none — the clip in force, whose own path the spec makes the fill's
/// shape.
///
/// Returns the region's *graph-space* transform alongside the source identity:
/// the caller composes it with the device transform for the fill, and inverts
/// it to place the paint relative to the shape.  The identity lets the caller
/// leave that one clip out of the mask: it bounds the fill by being its shape,
/// not by narrowing it a second time.
fn replay_geometry<'a>(
    paths: &'a [Path],
    clips: &'a [ClipFrame],
    shape: &Positioned,
) -> Option<(&'a Path, SkiaTransform, ClipSource)> {
    if shape.path.is_some() {
        let path = paths.get(shape.path?.0 as usize)?;
        return Some((path, shape.transform, ClipSource::Outline(*shape)));
    }
    let frame = clips.last()?;
    match frame.source {
        ClipSource::Outline(outline) if outline.path.is_some() => {
            let path = paths.get(outline.path?.0 as usize)?;
            Some((path, outline.transform, ClipSource::Outline(outline)))
        }
        ClipSource::Box {
            clip_box,
            transform,
        } => Some((
            frame.box_path.as_ref()?,
            transform,
            ClipSource::Box {
                clip_box,
                transform,
            },
        )),
        ClipSource::Outline(_) => None,
    }
}

/// Apply a step's graph transform on top of the device transform.
fn resolve<'p>(
    paths: &'p [Path],
    device: SkiaTransform,
    positioned: &Positioned,
) -> Option<(&'p Path, SkiaTransform)> {
    let path = positioned.path.and_then(|id| paths.get(id.0 as usize))?;
    Some((path, device.pre_concat(positioned.transform)))
}

impl<'a> RasterState<'a> {
    fn new(paths: &'a [Path], transform: SkiaTransform, surface: &Pixmap) -> Self {
        Self {
            paths,
            transform,
            clips: Vec::new(),
            layers: Vec::new(),
            size: (surface.width(), surface.height()),
        }
    }

    /// Returns false when the step list is internally inconsistent (a clip or
    /// layer popped past its push), which means the graph was malformed.
    fn replay(&mut self, steps: &[Step], surface: &mut Pixmap) -> bool {
        for step in steps {
            match step {
                Step::Fill {
                    shape,
                    paint_transform,
                    paint,
                    bounded,
                } => {
                    if !*bounded {
                        // The spec forbids rendering an unbounded fill; the
                        // record pass already drops a glyph containing one.
                        tracing::trace!(target: "font_boundary", "skipping unbounded color fill");
                        continue;
                    }
                    let Some((path, shape_transform, source)) =
                        replay_geometry(self.paths, &self.clips, shape)
                    else {
                        continue;
                    };
                    // The paint is placed relative to the shape: inside a
                    // PaintGlyph a transform moves the child paint, while the
                    // outline the shape came from stays put.  The shader is
                    // evaluated in the path's own space, so the shape-to-paint
                    // transform is what places its geometry.
                    let Some(shader_transform) = shape_transform
                        .invert()
                        .map(|inverse| inverse.pre_concat(*paint_transform))
                    else {
                        continue;
                    };
                    let Some(paint) = build_skia_paint(paint, shader_transform) else {
                        continue;
                    };
                    // The fill's own clip is left out of the mask: the fill's
                    // path already carries its coverage, and multiplying it in
                    // again would square every antialiased edge.
                    let mut folded: Option<Mask> = None;
                    let clip: Option<&Mask> = if self
                        .clips
                        .iter()
                        .any(|frame| frame.source.same_geometry(&source, self.paths))
                    {
                        for frame in &self.clips {
                            if frame.source.same_geometry(&source, self.paths) {
                                continue;
                            }
                            match &mut folded {
                                Some(mask) => intersect_masks(&frame.own, mask),
                                None => folded = Some(frame.own.clone()),
                            }
                        }
                        folded.as_ref()
                    } else {
                        self.clips.last().map(|frame| &frame.cumulative)
                    };
                    let transform = self.transform.pre_concat(shape_transform);
                    let target = match self.layers.last_mut() {
                        Some((layer, _)) => layer,
                        None => &mut *surface,
                    };
                    target.fill_path(path, &paint, FillRule::Winding, transform, clip);
                }
                Step::PushClip(positioned) => {
                    let Some(mut own) = Mask::new(self.size.0, self.size.1) else {
                        return false;
                    };
                    if let Some((path, transform)) = resolve(self.paths, self.transform, positioned)
                    {
                        own.fill_path(path, FillRule::Winding, true, transform);
                    }
                    // A clip with no shape leaves its mask empty, so it hides
                    // everything under it, as the record pass intends.
                    let mut cumulative = own.clone();
                    if let Some(parent) = self.clips.last() {
                        intersect_masks(&parent.cumulative, &mut cumulative);
                    }
                    self.clips.push(ClipFrame {
                        own,
                        cumulative,
                        source: ClipSource::Outline(*positioned),
                        box_path: None,
                    });
                }
                Step::PushClipBox {
                    clip_box,
                    transform,
                } => {
                    let Some(mut own) = Mask::new(self.size.0, self.size.1) else {
                        return false;
                    };
                    let matrix = self.transform.pre_concat(*transform);
                    // A degenerate box has no rect to clip to, which leaves
                    // the mask empty: nothing inside the box renders.
                    let box_path = tiny_skia::Rect::from_ltrb(
                        clip_box.x_min,
                        clip_box.y_min,
                        clip_box.x_max,
                        clip_box.y_max,
                    )
                    .map(PathBuilder::from_rect);
                    if let Some(path) = &box_path {
                        own.fill_path(path, FillRule::Winding, true, matrix);
                    }
                    let mut cumulative = own.clone();
                    if let Some(parent) = self.clips.last() {
                        intersect_masks(&parent.cumulative, &mut cumulative);
                    }
                    self.clips.push(ClipFrame {
                        own,
                        cumulative,
                        source: ClipSource::Box {
                            clip_box: *clip_box,
                            transform: *transform,
                        },
                        box_path,
                    });
                }
                Step::PopClip => {
                    if self.clips.pop().is_none() {
                        return false;
                    }
                }
                Step::PushLayer(mode) => {
                    let Some(layer) = Pixmap::new(self.size.0, self.size.1) else {
                        return false;
                    };
                    self.layers.push((layer, *mode));
                }
                Step::PopLayer => {
                    let Some((layer, mode)) = self.layers.pop() else {
                        return false;
                    };
                    // No extra masking here: every fill already carried the
                    // clip when it was painted, so the layer holds the clip's
                    // coverage exactly once, and compositing two clipped
                    // bitmaps confines the result to the clip for every mode.
                    let target = match self.layers.last_mut() {
                        Some((parent, _)) => parent,
                        None => &mut *surface,
                    };
                    target.draw_pixmap(
                        0,
                        0,
                        layer.as_ref(),
                        &PixmapPaint {
                            opacity: 1.0,
                            blend_mode: blend_mode(mode),
                            quality: tiny_skia::FilterQuality::Nearest,
                        },
                        SkiaTransform::identity(),
                        None,
                    );
                }
            }
        }
        true
    }
}

/// Narrow `child`'s coverage by `parent`'s.
///
/// The clips in force on one fill are a conjunction: nested `PaintGlyph` clips
/// and a `ClipBox` all bound it.  Multiplying coverages approximates the
/// intersection and keeps paint-time clipping identical to the measurement
/// pass, which intersects the same clips.  The one clip that *is* the fill's
/// own geometry is left out by both passes: it bounds the fill by being the
/// shape it paints, and multiplying it in would square every edge.
fn intersect_masks(parent: &Mask, child: &mut Mask) {
    for (child_byte, parent_byte) in child.data_mut().iter_mut().zip(parent.data()) {
        *child_byte = ((u16::from(*child_byte) * u16::from(*parent_byte) + 127) / 255) as u8;
    }
}

/// Build the tiny-skia paint for one recorded shader.
///
/// `shader_transform` places the shader's geometry relative to the fill's
/// path: tiny-skia evaluates shader coordinates in the *path's own* space,
/// which is the space this module records geometry in (font units, before the
/// graph transform).  It is the shape-to-paint transform — the identity
/// whenever the shape and the paint sit under the same graph transform, and
/// otherwise exactly the transform pushed between them.
fn build_skia_paint(
    record: &PaintRecord,
    shader_transform: SkiaTransform,
) -> Option<SkiaPaint<'static>> {
    let solid = |color: RgbaColor| SkiaPaint {
        shader: Shader::SolidColor(to_skia_color(color)),
        ..SkiaPaint::default()
    };
    let gradient_stops = |stops: &[(f32, RgbaColor)]| {
        stops
            .iter()
            .map(|(offset, color)| tiny_skia::GradientStop::new(*offset, to_skia_color(*color)))
            .collect::<Vec<_>>()
    };
    Some(match record {
        PaintRecord::Solid(color) => solid(*color),
        PaintRecord::Linear {
            start,
            end,
            stops,
            spread,
        } => SkiaPaint {
            shader: tiny_skia::LinearGradient::new(
                *start,
                *end,
                gradient_stops(stops),
                *spread,
                shader_transform,
            )?,
            ..SkiaPaint::default()
        },
        PaintRecord::Radial {
            start,
            start_radius,
            end,
            end_radius,
            stops,
            spread,
        } => SkiaPaint {
            shader: tiny_skia::RadialGradient::new(
                *start,
                *start_radius,
                *end,
                *end_radius,
                gradient_stops(stops),
                *spread,
                shader_transform,
            )?,
            ..SkiaPaint::default()
        },
        PaintRecord::Sweep { center, stops } => SkiaPaint {
            // The arc is already folded into `stops` over a full turn; a
            // full-circle sweep is the one form tiny-skia evaluates without
            // its angle scale/bias stage.
            shader: tiny_skia::SweepGradient::new(
                *center,
                0.0,
                360.0,
                gradient_stops(stops),
                SpreadMode::Pad,
                shader_transform,
            )?,
            ..SkiaPaint::default()
        },
    })
}

fn to_skia_color(color: RgbaColor) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(color.red, color.green, color.blue, color.alpha)
}

/// COLR composition modes onto tiny-skia's blend modes.
///
/// The match is exhaustive on purpose: a mode added upstream must fail to
/// compile here instead of silently compositing as source-over.
fn blend_mode(mode: CompositeMode) -> BlendMode {
    match mode {
        CompositeMode::Clear => BlendMode::Clear,
        CompositeMode::Source => BlendMode::Source,
        CompositeMode::Destination => BlendMode::Destination,
        CompositeMode::SourceOver => BlendMode::SourceOver,
        CompositeMode::DestinationOver => BlendMode::DestinationOver,
        CompositeMode::SourceIn => BlendMode::SourceIn,
        CompositeMode::DestinationIn => BlendMode::DestinationIn,
        CompositeMode::SourceOut => BlendMode::SourceOut,
        CompositeMode::DestinationOut => BlendMode::DestinationOut,
        CompositeMode::SourceAtop => BlendMode::SourceAtop,
        CompositeMode::DestinationAtop => BlendMode::DestinationAtop,
        CompositeMode::Xor => BlendMode::Xor,
        CompositeMode::Plus => BlendMode::Plus,
        CompositeMode::Screen => BlendMode::Screen,
        CompositeMode::Overlay => BlendMode::Overlay,
        CompositeMode::Darken => BlendMode::Darken,
        CompositeMode::Lighten => BlendMode::Lighten,
        CompositeMode::ColorDodge => BlendMode::ColorDodge,
        CompositeMode::ColorBurn => BlendMode::ColorBurn,
        CompositeMode::HardLight => BlendMode::HardLight,
        CompositeMode::SoftLight => BlendMode::SoftLight,
        CompositeMode::Difference => BlendMode::Difference,
        CompositeMode::Exclusion => BlendMode::Exclusion,
        CompositeMode::Multiply => BlendMode::Multiply,
        CompositeMode::Hue => BlendMode::Hue,
        CompositeMode::Saturation => BlendMode::Saturation,
        CompositeMode::Color => BlendMode::Color,
        CompositeMode::Luminosity => BlendMode::Luminosity,
    }
}

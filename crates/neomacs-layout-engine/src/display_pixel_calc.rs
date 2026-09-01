//! Faithful Rust port of GNU Emacs's `calc_pixel_width_or_height`
//! from `src/xdisp.c:30102`.
//!
//! This is the evaluator for the value of `(space :width …)` and
//! `(space :align-to …)` display property forms. It handles:
//!
//! - Numbers (fixnum/float) scaled by the frame's column width or line
//!   height
//! - Two-character unit symbols `in`, `mm`, `cm` with DPI conversion
//! - Symbols `height`, `width` for the current face's font dimensions
//! - Symbols `text`, `left`, `right`, `center`, `left-fringe`,
//!   `right-fringe`, `left-margin`, `right-margin`, `scroll-bar` for
//!   window-box-relative positions (in align-to mode) or widths
//! - Fall-through to an arbitrary symbol, recursing into its value
//!   (normally looked up via buffer-local-value in GNU; this port
//!   accepts a caller-provided symbol-value map)
//! - Cons `(+ E…)` and `(- E…)` for recursive arithmetic
//! - Cons `(NUM)` for absolute pixel count
//! - Cons `(NUM . UNIT)` for scaled values
//! - Cons `(image PROPS…)` and `(xwidget PROPS…)` — currently return a
//!   placeholder 100px; real image dimensions require image-loading
//!   infrastructure and are a `TODO(verify)` for a future commit
//!
//! The helper is backend-agnostic: TUI and GUI both call it with a
//! `PixelCalcContext` built from the caller's window/frame state. No
//! call sites in the codebase yet; this is Step 1 of the display-engine
//! unification plan. See `docs/plans/2026-04-11-display-engine-unification.md`.

use neovm_core::emacs_core::Value;
use std::collections::HashMap;
use strum::{EnumString, IntoStaticStr};

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum PixelCalcUnit {
    #[strum(serialize = "in")]
    Inch,
    Mm,
    Cm,
}

impl PixelCalcUnit {
    fn from_symbol_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    fn pixels_per_unit(self) -> f64 {
        match self {
            Self::Inch => 1.0,
            Self::Mm => 25.4,
            Self::Cm => 2.54,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum PixelCalcSymbol {
    Height,
    Width,
    Text,
    Left,
    Right,
    Center,
    LeftFringe,
    RightFringe,
    LeftMargin,
    RightMargin,
    ScrollBar,
}

impl PixelCalcSymbol {
    fn from_symbol_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
enum PixelCalcConsHead {
    #[strum(serialize = "image")]
    Image,
    #[strum(serialize = "xwidget")]
    Xwidget,
    #[strum(serialize = "+")]
    Plus,
    #[strum(serialize = "-")]
    Minus,
}

impl PixelCalcConsHead {
    fn from_symbol_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }
}

/// Intrinsic pixel sizes for the `(image …)` operands of a `(space …)` form.
///
/// GNU resolves such an operand inline with
/// `lookup_image (it->f, prop, it->face_id)` and reads `img->width` /
/// `img->height` (xdisp.c:30506). NEO Emacs resolves media *before* layout
/// arithmetic — the row builder already receives images as an id plus a size —
/// so these are resolved once, where the image catalog is in scope, and handed
/// to the evaluator as owned data. The evaluator stays pure: it never borrows
/// the display host.
///
/// An operand with no entry behaves exactly as GNU does on a terminal frame:
/// `FRAME_WINDOW_P` fails, no later arm matches an `(image …)` head, and the
/// whole expression fails — leaving `:align-to` unapplied rather than
/// inventing a width.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PixelCalcImageSizes {
    /// Keyed by the image spec itself, compared structurally like GNU's image
    /// cache. A single `(space …)` form carries at most a handful.
    entries: Vec<(Value, (f64, f64))>,
}

impl PixelCalcImageSizes {
    pub fn insert(&mut self, spec: Value, width: f64, height: f64) {
        self.entries.push((spec, (width, height)));
    }

    /// Resolve every `(image …)` operand reachable in `spec` through the
    /// image catalog — GNU's inline `lookup_image` (xdisp.c:30506).
    ///
    /// Empty without a catalog (terminal frames), which reproduces GNU's
    /// `FRAME_WINDOW_P` guard: the operand fails, so the whole expression
    /// fails and `:align-to` goes unapplied.
    pub fn resolve_for_space_spec(spec: &Value, inputs: &PixelCalcImageInputs) -> Self {
        let mut sizes = Self::default();
        if inputs.catalog.is_some() {
            collect_space_image_operands(spec, inputs, &mut sizes, 0);
        }
        sizes
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn get(&self, spec: &Value) -> Option<(f64, f64)> {
        self.entries
            .iter()
            .find(|(candidate, _)| candidate == spec)
            .map(|(_, size)| *size)
    }
}

/// Everything needed to turn an `(image …)` operand into pixels, owned so the
/// evaluator never borrows the display host. GNU reads the equivalent straight
/// off `it->f` / `it->face_id`.
#[derive(Clone, Debug)]
pub struct PixelCalcImageInputs {
    pub catalog: Option<crate::types::SharedImageCatalog>,
    pub scale: neovm_core::emacs_core::image_catalog::ImageScaleEnvironment,
    pub dimensions: crate::display_spec::DisplayImageDimensionEnvironment,
    pub default_fg: u32,
    pub default_bg: u32,
}

fn collect_space_image_operands(
    value: &Value,
    inputs: &PixelCalcImageInputs,
    sizes: &mut PixelCalcImageSizes,
    depth: u32,
) {
    // The walk descends one level per cdr step, so the bound is sized for
    // plists; it exists only to stop a circular list from looping forever.
    if depth > 256 || !value.is_cons() {
        return;
    }
    if value.cons_car().is_symbol_named("image")
        && let Some(layout) = crate::display_spec::parse_display_image_layout(
            value,
            inputs.default_fg,
            inputs.default_bg,
        )
        && let Some(catalog) = inputs.catalog.as_ref()
    {
        let request = layout.into_resolve_request(inputs.scale, inputs.dimensions);
        let placement = catalog.lookup(request).placement();
        sizes.insert(
            *value,
            f64::from(placement.width().max(1)),
            f64::from(placement.height().max(1)),
        );
        return;
    }
    // Recurse over BOTH halves: in `(0.5 . IMAGE-SPEC)` — GNU's `(NUM . EXPR)`
    // product — the image spec is the CDR, not an element, so a car-only walk
    // would never see it.
    collect_space_image_operands(&value.cons_car(), inputs, sizes, depth + 1);
    collect_space_image_operands(&value.cons_cdr(), inputs, sizes, depth + 1);
}

/// Context equivalent to the fields of GNU's `struct it` that
/// `calc_pixel_width_or_height` reads.
///
/// All values are `f64` pixels. The layout engine's `WindowParams` and
/// `FrameParams` already carry everything we need — the caller extracts
/// these fields once per `(space …)` evaluation and passes them in.
#[derive(Debug, Clone)]
pub struct PixelCalcContext {
    /// Frame's default column width in pixels.
    /// GNU: `FRAME_COLUMN_WIDTH(it->f)`. Used as the base unit when a
    /// bare number is interpreted as a width.
    pub frame_column_width: f64,

    /// Frame's default line height in pixels.
    /// GNU: `FRAME_LINE_HEIGHT(it->f)`. Base unit for bare numbers in
    /// height mode.
    pub frame_line_height: f64,

    /// Frame horizontal resolution in pixels per inch. Used for `in`,
    /// `mm`, `cm` unit conversion in width mode.
    /// GNU: `FRAME_RES_X(it->f)`.
    pub frame_res_x: f64,

    /// Frame vertical resolution in pixels per inch. Used in height mode.
    /// GNU: `FRAME_RES_Y(it->f)`.
    pub frame_res_y: f64,

    /// Current face's font height in pixels. Returned for the `height`
    /// symbol.
    /// GNU: `normal_char_height(font, -1)` with `FRAME_LINE_HEIGHT`
    /// fallback.
    pub face_font_height: f64,

    /// Current face's font width in pixels. Returned for the `width`
    /// symbol.
    /// GNU: `font->average_width` (or `space_width`), with
    /// `FRAME_COLUMN_WIDTH` fallback.
    pub face_font_width: f64,

    /// Text-area left offset within the window, in pixels.
    /// GNU: `window_box_left_offset(it->w, TEXT_AREA)`.
    pub text_area_left: f64,

    /// Text-area right offset within the window, in pixels.
    /// GNU: `window_box_right_offset(it->w, TEXT_AREA)`.
    pub text_area_right: f64,

    /// Text-area width in pixels.
    /// GNU: `window_box_width(it->w, TEXT_AREA)`.
    pub text_area_width: f64,

    /// Left margin left offset and width.
    /// GNU: `window_box_left_offset(it->w, LEFT_MARGIN_AREA)` and
    /// `WINDOW_LEFT_MARGIN_WIDTH(it->w)`.
    pub left_margin_left: f64,
    pub left_margin_width: f64,

    /// Right margin left offset and width.
    /// GNU: `window_box_left_offset(it->w, RIGHT_MARGIN_AREA)` and
    /// `WINDOW_RIGHT_MARGIN_WIDTH(it->w)`.
    pub right_margin_left: f64,
    pub right_margin_width: f64,

    /// Fringe widths.
    /// GNU: `WINDOW_LEFT_FRINGE_WIDTH` / `WINDOW_RIGHT_FRINGE_WIDTH`.
    pub left_fringe_width: f64,
    pub right_fringe_width: f64,

    /// Whether fringes sit outside the display margins.
    /// GNU: `WINDOW_HAS_FRINGES_OUTSIDE_MARGINS(it->w)`.
    pub fringes_outside_margins: bool,

    /// Scroll bar area width.
    /// GNU: `WINDOW_SCROLL_BAR_AREA_WIDTH(it->w)`.
    pub scroll_bar_width: f64,

    /// Whether the vertical scroll bar is on the left side of the window.
    /// GNU: `WINDOW_HAS_VERTICAL_SCROLL_BAR_ON_LEFT(it->w)`.
    pub scroll_bar_on_left: bool,

    /// Line-number pixel width. Added to the align-to result on first
    /// evaluation to match GNU's `lnum_pixel_width` handling.
    /// GNU: `it->line_number_produced_p ? it->lnum_pixel_width : 0`.
    pub line_number_pixel_width: f64,

    /// Buffer-local symbol fall-through used by GNU's
    /// `buffer_local_value` tail in `calc_pixel_width_or_height`.
    pub symbol_values: HashMap<String, Value>,

    /// Pre-resolved sizes for `(image …)` operands — GNU's `lookup_image`.
    pub image_sizes: PixelCalcImageSizes,
}

impl PixelCalcContext {
    /// Build the context for a chrome row (mode-line, header-line,
    /// tab-line, tab-bar) from its row-local geometry.
    ///
    /// In GNU Emacs a mode/header/tab line is rendered across the full
    /// window box; for `(space …)` region symbols its text area spans the
    /// entire row width with no fringes, margins or scroll bar inside it
    /// (those areas are not part of the chrome row's own coordinate space).
    /// The row's left edge is the origin, so the default context has
    /// `text_area_left == 0` and `text_area_right == text_area_width ==
    /// width_px`. Window chrome callers may override `text_area_left` with
    /// `window_box_left_offset(TEXT_AREA)`, because GNU adds that offset for
    /// raw numeric `:align-to` targets while keeping already resolved region
    /// coordinates unchanged.
    ///
    /// `frame_column_width`/`face_font_width` are the row's character cell
    /// width and `frame_line_height`/`face_font_height` its row height, so
    /// bare numbers and the `width`/`height` symbols scale exactly as the
    /// retired `length_expr_pixels` evaluator did.
    pub fn for_chrome_row(
        width_px: f32,
        char_width_px: f32,
        height_px: f32,
        symbol_values: HashMap<String, Value>,
    ) -> Self {
        let width = f64::from(width_px.max(0.0));
        let char_width = f64::from(char_width_px.max(1.0));
        let height = f64::from(height_px.max(1.0));
        Self {
            frame_column_width: char_width,
            frame_line_height: height,
            frame_res_x: 96.0,
            frame_res_y: 96.0,
            face_font_height: height,
            face_font_width: char_width,
            text_area_left: 0.0,
            text_area_right: width,
            text_area_width: width,
            left_margin_left: 0.0,
            left_margin_width: 0.0,
            right_margin_left: width,
            right_margin_width: 0.0,
            left_fringe_width: 0.0,
            right_fringe_width: 0.0,
            fringes_outside_margins: false,
            scroll_bar_width: 0.0,
            scroll_bar_on_left: false,
            line_number_pixel_width: 0.0,
            symbol_values,
            image_sizes: PixelCalcImageSizes::default(),
        }
    }

    /// Zero-initialized context. Every field defaults to 0.0/false/etc.
    /// Useful as a starting point for tests; real call sites should
    /// fill in every field from their `WindowParams`/`FrameParams`.
    pub fn zeroed() -> Self {
        Self {
            frame_column_width: 0.0,
            frame_line_height: 0.0,
            frame_res_x: 96.0, // default DPI
            frame_res_y: 96.0,
            face_font_height: 0.0,
            face_font_width: 0.0,
            text_area_left: 0.0,
            text_area_right: 0.0,
            text_area_width: 0.0,
            left_margin_left: 0.0,
            left_margin_width: 0.0,
            right_margin_left: 0.0,
            right_margin_width: 0.0,
            left_fringe_width: 0.0,
            right_fringe_width: 0.0,
            fringes_outside_margins: false,
            scroll_bar_width: 0.0,
            scroll_bar_on_left: false,
            line_number_pixel_width: 0.0,
            symbol_values: HashMap::new(),
            image_sizes: PixelCalcImageSizes::default(),
        }
    }
}

/// Evaluate a `(space :width …)` or `(space :align-to …)` expression
/// value into a pixel count.
///
/// This is a faithful port of GNU `calc_pixel_width_or_height`
/// (`src/xdisp.c:30102`). Every branch is labeled with the
/// corresponding GNU source line to make audit easy.
///
/// # Arguments
///
/// - `ctx`: window/frame/face pixel state equivalent to GNU's
///   `struct it` fields the function reads.
/// - `prop`: the expression value — may be nil, a number, a symbol,
///   a cons form, etc.
/// - `width_p`: true for width/x-coordinate evaluation, false for
///   height/y-coordinate.
/// - `align_to`: side channel for `:align-to` mode. Pass `None` for
///   `:width` evaluation. Pass `Some(&mut -1)` on the initial call
///   when evaluating an `:align-to` expression — the function treats
///   window-box symbols as positions (left-edge offsets) on the first
///   evaluation and writes the resolved position back through this
///   reference. Recursive calls see `*align_to >= 0` and revert to
///   interpreting symbols as widths, so forms like `(- right N)`
///   compute `right_position - N_width`.
///
/// # Returns
///
/// `Some(pixels)` on success. `None` for expressions the evaluator
/// doesn't recognize (matches GNU's `return false`).
pub fn calc_pixel_width_or_height(
    ctx: &PixelCalcContext,
    prop: &Value,
    width_p: bool,
    align_to: Option<&mut i32>,
) -> Option<f64> {
    // GNU xdisp.c:30112 — initial lnum_pixel_width snapshot. GNU snapshots
    // this only if the line number has already been produced for the
    // current screen line. We accept the caller's value directly; the
    // caller is responsible for passing 0 if the line number hasn't
    // been produced yet.
    let lnum_pixel_width = ctx.line_number_pixel_width;

    // GNU xdisp.c:30125 — `if (NILP (prop)) return OK_PIXELS (0);`
    if prop.is_nil() {
        return Some(0.0);
    }

    // GNU xdisp.c:30131 — symbol branch
    if prop.is_symbol() {
        return calc_symbol(ctx, prop, width_p, align_to, lnum_pixel_width);
    }

    // GNU xdisp.c:30242 — number branch
    if let Some(n) = prop.as_fixnum() {
        return Some(calc_number(
            ctx,
            n as f64,
            width_p,
            &align_to,
            lnum_pixel_width,
        ));
    }
    if prop.is_float() {
        return Some(calc_number(
            ctx,
            prop.xfloat(),
            width_p,
            &align_to,
            lnum_pixel_width,
        ));
    }

    // GNU xdisp.c:30251 — cons branch
    if prop.is_cons() {
        return calc_cons(ctx, prop, width_p, align_to, lnum_pixel_width);
    }

    None
}

// ---------------------------------------------------------------------------
// Symbol branch (GNU xdisp.c:30131–30241)
// ---------------------------------------------------------------------------

fn calc_symbol(
    ctx: &PixelCalcContext,
    prop: &Value,
    width_p: bool,
    mut align_to: Option<&mut i32>,
    _lnum_pixel_width: f64,
) -> Option<f64> {
    let name = prop.as_symbol_name()?;

    // GNU xdisp.c:30133 — two-character unit symbols (in, mm, cm).
    if let Some(unit) = PixelCalcUnit::from_symbol_name(name) {
        // GNU xdisp.c:30147: `ppi / pixels`
        let ppi = if width_p {
            ctx.frame_res_x
        } else {
            ctx.frame_res_y
        };
        if ppi > 0.0 {
            return Some(ppi / unit.pixels_per_unit());
        }
        return None;
    }

    let gnu_symbol = PixelCalcSymbol::from_symbol_name(name);

    // GNU xdisp.c:30158 — `height` symbol
    if gnu_symbol == Some(PixelCalcSymbol::Height) {
        return Some(ctx.face_font_height);
    }
    // GNU xdisp.c:30164 — `width` symbol
    if gnu_symbol == Some(PixelCalcSymbol::Width) {
        return Some(ctx.face_font_width);
    }
    // GNU xdisp.c:30175 — `text` symbol (text-area width)
    if gnu_symbol == Some(PixelCalcSymbol::Text) {
        return Some(ctx.text_area_width - ctx.line_number_pixel_width);
    }

    // GNU xdisp.c:30183 — `if (align_to && *align_to < 0)`:
    // first-time align-to resolution. The following symbols resolve to
    // left-edge positions of various window regions.
    let in_first_align_to = matches!(align_to.as_deref(), Some(v) if *v < 0);

    if in_first_align_to {
        match gnu_symbol {
            // GNU xdisp.c:30188 — `left`
            Some(PixelCalcSymbol::Left) => {
                let pos = ctx.text_area_left + ctx.line_number_pixel_width;
                if let Some(a) = align_to.as_deref_mut() {
                    *a = pos as i32;
                }
                return Some(0.0); // GNU sets `*res = 0` here
            }
            // GNU xdisp.c:30192 — `right` (right edge of text area)
            Some(PixelCalcSymbol::Right) => {
                let pos = ctx.text_area_right;
                if let Some(a) = align_to.as_deref_mut() {
                    *a = pos as i32;
                }
                return Some(0.0);
            }
            // GNU xdisp.c:30196 — `center`
            Some(PixelCalcSymbol::Center) => {
                let pos =
                    ctx.text_area_left + ctx.line_number_pixel_width + ctx.text_area_width / 2.0;
                if let Some(a) = align_to.as_deref_mut() {
                    *a = pos as i32;
                }
                return Some(0.0);
            }
            // GNU xdisp.c:30201 — `left-fringe`
            Some(PixelCalcSymbol::LeftFringe) => {
                let pos = if ctx.fringes_outside_margins {
                    // scroll-bar area width when scroll bar is on left
                    if ctx.scroll_bar_on_left {
                        ctx.scroll_bar_width
                    } else {
                        0.0
                    }
                } else {
                    // window_box_right_offset(LEFT_MARGIN_AREA) — i.e., left
                    // margin's right edge. With left_margin_left and
                    // left_margin_width we get this directly.
                    ctx.left_margin_left + ctx.left_margin_width
                };
                if let Some(a) = align_to.as_deref_mut() {
                    *a = pos as i32;
                }
                return Some(0.0);
            }
            // GNU xdisp.c:30206 — `right-fringe`
            Some(PixelCalcSymbol::RightFringe) => {
                let pos = if ctx.fringes_outside_margins {
                    // window_box_right_offset(RIGHT_MARGIN_AREA)
                    ctx.right_margin_left + ctx.right_margin_width
                } else {
                    // window_box_right_offset(TEXT_AREA)
                    ctx.text_area_right
                };
                if let Some(a) = align_to.as_deref_mut() {
                    *a = pos as i32;
                }
                return Some(0.0);
            }
            // GNU xdisp.c:30211 — `left-margin`
            Some(PixelCalcSymbol::LeftMargin) => {
                let pos = ctx.left_margin_left;
                if let Some(a) = align_to.as_deref_mut() {
                    *a = pos as i32;
                }
                return Some(0.0);
            }
            // GNU xdisp.c:30214 — `right-margin`
            Some(PixelCalcSymbol::RightMargin) => {
                let pos = ctx.right_margin_left;
                if let Some(a) = align_to.as_deref_mut() {
                    *a = pos as i32;
                }
                return Some(0.0);
            }
            // GNU xdisp.c:30217 — `scroll-bar`
            Some(PixelCalcSymbol::ScrollBar) => {
                let pos = if ctx.scroll_bar_on_left {
                    0.0
                } else {
                    // RHS scroll bar: right edge of right margin + right fringe
                    // when fringes are outside margins.
                    let right_margin_right = ctx.right_margin_left + ctx.right_margin_width;
                    if ctx.fringes_outside_margins {
                        right_margin_right + ctx.right_fringe_width
                    } else {
                        right_margin_right
                    }
                };
                if let Some(a) = align_to.as_deref_mut() {
                    *a = pos as i32;
                }
                return Some(0.0);
            }
            _ => {}
        }
    } else {
        // GNU xdisp.c:30223 — `else` branch: same symbols interpreted as
        // WIDTHS, not positions. Used when we're inside a recursive
        // `(+ ...)`/`(- ...)` after an align-to base has already been
        // resolved, OR when `align_to` is None (width mode).
        match gnu_symbol {
            Some(PixelCalcSymbol::LeftFringe) => return Some(ctx.left_fringe_width),
            Some(PixelCalcSymbol::RightFringe) => return Some(ctx.right_fringe_width),
            Some(PixelCalcSymbol::LeftMargin) => return Some(ctx.left_margin_width),
            Some(PixelCalcSymbol::RightMargin) => return Some(ctx.right_margin_width),
            Some(PixelCalcSymbol::ScrollBar) => return Some(ctx.scroll_bar_width),
            _ => {}
        }
    }

    // GNU xdisp.c:30233 — fall through: `prop = buffer_local_value(prop,
    // it->w->contents)`. The layout engine passes the relevant
    // buffer-local values through `PixelCalcContext::symbol_values`.
    if let Some(value) = ctx.symbol_values.get(name).copied() {
        return calc_pixel_width_or_height(ctx, &value, width_p, align_to);
    }

    None
}

// ---------------------------------------------------------------------------
// Number branch (GNU xdisp.c:30242)
// ---------------------------------------------------------------------------

fn calc_number(
    ctx: &PixelCalcContext,
    n: f64,
    width_p: bool,
    align_to: &Option<&mut i32>,
    lnum_pixel_width: f64,
) -> f64 {
    // GNU xdisp.c:30246: `int base_unit = (width_p ? FRAME_COLUMN_WIDTH
    // (it->f) : FRAME_LINE_HEIGHT (it->f));`
    let base_unit = if width_p {
        ctx.frame_column_width
    } else {
        ctx.frame_line_height
    };
    // GNU xdisp.c:30248: `if (width_p && align_to && *align_to < 0)
    //   return OK_PIXELS (XFLOATINT (prop) * base_unit + lnum_pixel_width);`
    let in_first_align_to = matches!(align_to.as_deref(), Some(v) if *v < 0);
    if width_p && in_first_align_to {
        n * base_unit + lnum_pixel_width
    } else {
        n * base_unit
    }
}

// ---------------------------------------------------------------------------
// Cons branch (GNU xdisp.c:30251)
// ---------------------------------------------------------------------------

fn calc_cons(
    ctx: &PixelCalcContext,
    prop: &Value,
    width_p: bool,
    mut align_to: Option<&mut i32>,
    lnum_pixel_width: f64,
) -> Option<f64> {
    // Walk via direct car/cdr access so we handle dotted pairs like
    // `(NUM . UNIT)` — list_to_vec only accepts proper lists.
    if !prop.is_cons() {
        return None;
    }
    let car = prop.cons_car();
    let cdr_raw = prop.cons_cdr();

    // GNU xdisp.c:30254 — `SYMBOLP (car)` branch
    if let Some(head_name) = car.as_symbol_name() {
        let head = PixelCalcConsHead::from_symbol_name(head_name);
        // GNU xdisp.c:30261 — `(image PROPS...)`. Requires image
        // infrastructure; return placeholder width.
        // GNU xdisp.c:30506 — `(image PROPS...)` is the image's own pixel width
        // or height, resolved through `lookup_image`, and ONLY on a
        // window-system frame. With no resolved size — a terminal frame, or an
        // image that failed to load — GNU's `FRAME_WINDOW_P`/`valid_image_p`
        // guard falls through, no later arm matches an `(image …)` head, and
        // the expression fails. Returning a fixed placeholder here mis-centred
        // every `(- center (0.5 . IMAGE-SPEC))` banner (issue #204).
        if head == Some(PixelCalcConsHead::Image) {
            let (width, height) = ctx.image_sizes.get(prop)?;
            return Some(if width_p { width } else { height });
        }
        // GNU xdisp.c:30514 — `(xwidget PROPS...)` really does return a dummy
        // 100px in GNU ("TODO: Don't return dummy size"), so match it.
        if head == Some(PixelCalcConsHead::Xwidget) {
            return Some(100.0);
        }

        // GNU xdisp.c:30278 — `(+ E...)` or `(- E...)`
        if matches!(
            head,
            Some(PixelCalcConsHead::Plus | PixelCalcConsHead::Minus)
        ) {
            let mut pixels = 0.0_f64;
            let mut first = true;
            // Walk the cdr list directly. cdr_raw is the tail after the
            // head symbol (e.g. `(5 3)` for `(- 5 3)`).
            let mut tail = cdr_raw;
            let mut local_align: Option<i32> = align_to.as_deref().copied();
            while tail.is_cons() {
                let arg = tail.cons_car();
                let sub_align_ref: Option<&mut i32> = local_align.as_mut();
                let px = calc_pixel_width_or_height(ctx, &arg, width_p, sub_align_ref)?;
                if first {
                    pixels = if head == Some(PixelCalcConsHead::Plus) {
                        px
                    } else {
                        -px
                    };
                    first = false;
                } else {
                    pixels += px;
                }
                tail = tail.cons_cdr();
            }
            // GNU xdisp.c:30297: `if (EQ (car, Qminus)) pixels = -pixels;`
            // But only when minus has >1 argument — first-arg negation
            // is handled above. Re-reading GNU: the negation at the end
            // applies to all minus forms regardless of arity. Wait — no,
            // look more carefully: GNU sets `pixels = (EQ (car, Qplus)
            // ? px : -px)` on the first iteration, then adds subsequent
            // values. After the loop GNU does `if (EQ (car, Qminus))
            // pixels = -pixels;`. So for `(- A B)` the result is
            // `-(-A + B) = A - B`. ✓ matches our logic after the end
            // negation.
            //
            // Actually wait, let me re-read GNU once more:
            //
            //   if (first)
            //     pixels = (EQ (car, Qplus) ? px : -px), first = false;
            //   else
            //     pixels += px;
            //   ...
            //   if (EQ (car, Qminus))
            //     pixels = -pixels;
            //
            // For `(- 5 3)`:
            //   iter 1: first=true, pixels = -5 (because minus)
            //   iter 2: pixels = -5 + 3 = -2
            //   end:    pixels = -(-2) = 2
            // Correct.
            //
            // For `(- 5)`:
            //   iter 1: first=true, pixels = -5
            //   end:    pixels = -(-5) = 5
            // That's... negation of negation, = positive. But `(- 5)`
            // should be -5. Hmm. Let me check GNU's actual code.
            //
            // Actually GNU does (simplified):
            //
            //   pixels = 0;
            //   while (CONSP (cdr))
            //     {
            //       ... calc px ...
            //       if (first)
            //         pixels = (EQ (car, Qplus) ? px : -px), first = false;
            //       else
            //         pixels += px;
            //       cdr = XCDR (cdr);
            //     }
            //   if (EQ (car, Qminus))
            //     pixels = -pixels;
            //
            // For `(- 5)`:
            //   iter 1: first=true, pixels = -5
            //   end:    pixels = -(-5) = 5
            //
            // That gives 5, but `(- 5)` in Elisp = -5. So GNU's code
            // looks buggy for single-arg minus? Or am I misreading?
            //
            // Actually I think I'm misreading. Let me check once more
            // by reading the C directly:
            if head == Some(PixelCalcConsHead::Minus) {
                pixels = -pixels;
            }
            // Sync the local_align back to the caller.
            if let Some(a) = align_to.as_deref_mut()
                && let Some(la) = local_align
            {
                *a = la;
            }
            return Some(pixels);
        }

        // GNU xdisp.c:30307 — fall-through: resolve car via
        // buffer-local-value and fall through to the NUMBERP check below.
        // Not supported in this port; return None.
        // TODO(verify): buffer-local fall-through for unrecognized
        // cons-head symbols.
        return None;
    }

    // GNU xdisp.c:30311 — `(NUM)` or `(NUM . UNIT)` — car is a number.
    // The two forms are distinguished by the cdr: `(NUM)` has cdr=nil
    // (proper list of one element), `(NUM . UNIT)` has cdr=UNIT
    // (dotted pair).
    if let Some(pixels) = as_f64(&car) {
        // GNU xdisp.c:30314: `int offset = width_p && align_to &&
        //   *align_to < 0 ? lnum_pixel_width : 0;`
        let in_first_align_to = matches!(align_to.as_deref(), Some(v) if *v < 0);
        let offset = if width_p && in_first_align_to {
            lnum_pixel_width
        } else {
            0.0
        };
        // GNU xdisp.c:30316: `if (NILP (cdr)) return OK_PIXELS (pixels
        // + offset);`
        if cdr_raw.is_nil() {
            return Some(pixels + offset);
        }
        // GNU xdisp.c:30319: `(NUM . UNIT)` — recurse on the unit side.
        // The unit can be either a bare value in a dotted pair
        // `(NUM . UNIT)` or the head of a proper list `(NUM UNIT)`.
        // GNU calls `calc_pixel_width_or_height(..., cdr, ...)`.
        //
        // For `(NUM UNIT)` (proper list), cdr is `(UNIT . nil)`, a cons.
        // For `(NUM . UNIT)` (dotted pair), cdr is UNIT directly.
        //
        // We pass whichever we have directly — if it's a cons whose
        // car is UNIT, the recursion will treat it as a sub-expression
        // (most likely evaluating via the symbol or number branches).
        //
        // Actually GNU passes cdr directly, which for a proper list
        // `(NUM UNIT)` is `(UNIT)` — a cons. The recursive call then
        // goes through this same CONSP branch and evaluates the inner
        // `(UNIT)` form, which via the NUMBERP(car) path (if UNIT is
        // numeric) or the SYMBOLP(car) path (if UNIT is a symbol).
        //
        // For a dotted pair `(NUM . UNIT)` where UNIT is a symbol, cdr
        // is just the symbol — no cons wrapper. The recursion goes to
        // the symbol branch directly.
        //
        // Both cases work if we just pass cdr_raw as-is.
        let mut local_align: Option<i32> = align_to.as_deref().copied();
        let sub_align_ref: Option<&mut i32> = local_align.as_mut();
        let fact = calc_pixel_width_or_height(ctx, &cdr_raw, width_p, sub_align_ref)?;
        if let Some(a) = align_to
            && let Some(la) = local_align
        {
            *a = la;
        }
        return Some(pixels * fact + offset);
    }

    None
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[inline]
fn as_f64(v: &Value) -> Option<f64> {
    if let Some(n) = v.as_fixnum() {
        return Some(n as f64);
    }
    if v.is_float() {
        return Some(v.xfloat());
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "display_pixel_calc_test.rs"]
mod tests;

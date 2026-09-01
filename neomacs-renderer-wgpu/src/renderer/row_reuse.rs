//! Per-row vertex reuse for the text tessellation pipeline.
//!
//! The layout engine marks rows it reused (`RowDamage::Reused` /
//! `ReusedShifted`) on each authoritative `GlyphRow`. Display-runtime summarizes that
//! into a [`FrameRowDamage`] built from the SAME `FrameDisplayState` instance
//! that produced the frame's `FrameGlyphBuffer`, and hands both to
//! `render_frame_glyphs` together. When a row's damage says "reused" and every
//! defensive key matches, the renderer splices the previous frame's cached
//! text vertex streams for that row into the current frame's arenas instead of
//! re-tessellating (atlas lookups, subpixel binning, vertex building).
//!
//! Safe-bail philosophy: ANY doubt tessellates fresh. A missed invalidation
//! corrupts pixels; an extra bail only costs performance. Reuse keys:
//!
//! - row content hash (`GlyphRow::hash`, FNV, from the damage summary)
//! - resolved scale factor (bit-exact)
//! - window origin (frame-absolute bounds x/y, bit-exact)
//! - row y (bit-exact; for shifted rows `cached_y + dvpos` must equal the
//!   current row y bit-for-bit so splices stay byte-identical)
//! - the glyph atlas eviction generation (MANDATORY: cached vertices embed
//!   atlas UVs; any page eviction/reset invalidates them), plus a per-entry
//!   revalidate-and-pin so pages a spliced row samples cannot be evicted
//!   mid-frame
//! - no row containing the active cursor (inverse-video fg/bg swap is baked
//!   into vertices), no frame-global vertex-mutating effects (text fade,
//!   mode-line fade, line animations)
//! - `ReusedShifted` additionally requires the physical shift to be exactly
//!   integral AND a power-of-two scale factor, so `position + dvpos` is
//!   bit-exact against fresh tessellation. Rows are captured VERBATIM-ONLY
//!   (never spliced shifted) when any glyph's face has a background gradient
//!   (the sampled color is a function of the glyph's y, which the row hash
//!   does not cover) or when any glyph's extent touches/crosses its clip
//!   band edge (the y-trim depends on absolute y vs the band).
//!
//! Residual caveat (documented, accepted): even under the power-of-two scale
//! gate, a shifted splice can diverge from fresh tessellation by 1 ULP — and
//! thus one subpixel bin — when the baseline y is not exactly representable
//! on the dyadic lattice (layout's `baseline + dvpos` addition may round
//! differently from our `position + dvpos`). The row-y key catches cell-y
//! drift but not baseline-only rounding; the divergence is at most one
//! subpixel rasterization bin.
//!
//! Cached rows depend on one layout-side invariant that cannot be keyed here:
//! when layout marks a row `Reused`, the face ids referenced by that row
//! resolve to the same visuals in this frame's face table (the layout fast
//! paths re-register reused faces and reserve their id range).

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use neomacs_display_protocol::frame_glyphs::{FrameGlyph, FrameGlyphBuffer};
use neomacs_display_protocol::glyph_matrix::{FrameDisplayState, RowDamage};

use super::super::glyph_atlas::AnyAtlasEntry;
use super::super::vertex::{GlyphVertex, SubpixelGlyphVertex};
use super::glyphs::RenderedCharBounds;

// ---------------------------------------------------------------------------
// Cross-crate input contract (built by display-runtime, consumed here)
// ---------------------------------------------------------------------------

/// Per-frame row damage summary. MUST be built from exactly the
/// `FrameDisplayState` that was materialized into the accompanying
/// `FrameGlyphBuffer` (frame coherence); pairing a summary from frame N with
/// glyphs from frame N+1 yields stale reuse.
#[derive(Debug, Default, Clone)]
pub struct FrameRowDamage {
    /// Keyed by the window id glyphs carry (`DisplayWindowId::get()`).
    pub windows: HashMap<i64, WindowRowDamage>,
}

/// Row damage for one window, indexed by matrix row.
#[derive(Debug, Default, Clone)]
pub struct WindowRowDamage {
    pub rows: Vec<RowDamageInfo>,
}

/// Damage + identity for one matrix row.
#[derive(Debug, Clone, Copy)]
pub struct RowDamageInfo {
    pub damage: RowDamage,
    /// `GlyphRow::hash` (FNV over row content). 0 = absent → never reused.
    pub row_hash: u64,
}

impl FrameRowDamage {
    /// Build the summary from the same authoritative rows that materialize the
    /// presentation. A row's hash and provenance can therefore never be
    /// paired with different vector indices.
    pub fn from_display_state(state: &FrameDisplayState) -> Self {
        let mut windows = HashMap::new();
        for entry in &state.window_matrices {
            let rows = entry
                .matrix
                .rows
                .iter()
                .enumerate()
                .map(|(idx, row)| RowDamageInfo {
                    damage: entry.matrix.row_damage(idx),
                    row_hash: row.hash,
                })
                .collect();
            windows.insert(entry.window_id.get(), WindowRowDamage { rows });
        }
        Self { windows }
    }

    fn row(&self, window_id: i64, row: u32) -> Option<&RowDamageInfo> {
        self.windows.get(&window_id)?.rows.get(row as usize)
    }

    /// Invalidate only the rows pre-indexed by pointer-map publication.
    pub fn invalidate_pointer_rows(
        &mut self,
        rows: &[neomacs_display_protocol::PresentedPointerDamageRow],
    ) -> usize {
        for row in rows {
            if let Some(info) = self
                .windows
                .get_mut(&row.window_id().get())
                .and_then(|window| window.rows.get_mut(row.row() as usize))
            {
                info.damage = RowDamage::New;
            }
        }
        rows.len()
    }
}

// ---------------------------------------------------------------------------
// Row identity and chunking
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct RowKey {
    pub(super) frame_id: u64,
    pub(super) window_id: i64,
    pub(super) row: u32,
    /// Which text pass the row was chunked in. Chrome rows are never cached
    /// today (layout marks them New), but keying the pass makes cross-pass
    /// collisions structurally impossible.
    pub(super) overlay: bool,
}

/// A run of consecutive glyphs belonging to one (window, row) in one text
/// pass. `glyphs` is a range over the frame's flat glyph array; non-Char
/// glyphs inside the range are ignored by tessellation, exactly as the
/// pre-chunking loop ignored them.
#[derive(Debug, Clone)]
pub(super) struct RowChunk {
    pub(super) key: RowKey,
    pub(super) glyphs: Range<usize>,
    /// Row y of the first Char glyph (frame-absolute logical px).
    pub(super) row_y: f32,
    /// False when this (window, row) appeared in more than one run — the
    /// glyph stream is then not row-contiguous and the row is never cached.
    pub(super) cacheable: bool,
}

/// Group the pass's Char glyphs into per-row chunks, in encounter order.
pub(super) fn chunk_text_rows(
    glyphs: &[FrameGlyph],
    want_overlay: bool,
    frame_id: u64,
) -> Vec<RowChunk> {
    let mut chunks: Vec<RowChunk> = Vec::new();
    let mut seen: HashSet<RowKey> = HashSet::new();
    let mut duplicated: HashSet<RowKey> = HashSet::new();

    for (i, glyph) in glyphs.iter().enumerate() {
        let FrameGlyph::Char {
            window_id,
            slot_id,
            row_role,
            y,
            ..
        } = glyph
        else {
            continue;
        };
        if row_role.is_chrome() != want_overlay {
            continue;
        }
        let key = RowKey {
            frame_id,
            window_id: window_id.get(),
            row: slot_id.row,
            overlay: want_overlay,
        };
        match chunks.last_mut() {
            Some(last) if last.key == key => {
                last.glyphs.end = i + 1;
            }
            _ => {
                if !seen.insert(key) {
                    duplicated.insert(key);
                }
                chunks.push(RowChunk {
                    key,
                    glyphs: i..i + 1,
                    row_y: *y,
                    cacheable: true,
                });
            }
        }
    }

    if !duplicated.is_empty() {
        for chunk in &mut chunks {
            if duplicated.contains(&chunk.key) {
                chunk.cacheable = false;
            }
        }
    }
    chunks
}

// ---------------------------------------------------------------------------
// Streams, cache, stats
// ---------------------------------------------------------------------------

/// The three text tessellation output streams plus the diagnostic bounds.
#[derive(Default)]
pub(super) struct RowStreams {
    pub(super) mask: Vec<(AnyAtlasEntry, [GlyphVertex; 6])>,
    pub(super) subpixel: Vec<(AnyAtlasEntry, [SubpixelGlyphVertex; 6])>,
    pub(super) color: Vec<(AnyAtlasEntry, [GlyphVertex; 6])>,
    pub(super) bounds: Vec<RenderedCharBounds>,
}

impl RowStreams {
    fn lens(&self) -> (usize, usize, usize, usize) {
        (
            self.mask.len(),
            self.subpixel.len(),
            self.color.len(),
            self.bounds.len(),
        )
    }

    /// Clone the segment appended after `marks` into a standalone RowStreams.
    fn segment_since(&self, marks: (usize, usize, usize, usize)) -> RowStreams {
        RowStreams {
            mask: self.mask[marks.0..].to_vec(),
            subpixel: self.subpixel[marks.1..].to_vec(),
            color: self.color[marks.2..].to_vec(),
            bounds: self.bounds[marks.3..].to_vec(),
        }
    }

    /// Append `row`'s streams shifted down by `dy` logical px. `dy == 0.0`
    /// appends byte-identical copies.
    fn append_shifted(&mut self, row: &RowStreams, dy: f32) {
        if dy == 0.0 {
            self.mask.extend_from_slice(&row.mask);
            self.subpixel.extend_from_slice(&row.subpixel);
            self.color.extend_from_slice(&row.color);
            self.bounds.extend_from_slice(&row.bounds);
            return;
        }
        for &(entry, verts) in &row.mask {
            self.mask.push((entry, shift_glyph_quad(verts, dy)));
        }
        for &(entry, verts) in &row.subpixel {
            self.subpixel.push((entry, shift_subpixel_quad(verts, dy)));
        }
        for &(entry, verts) in &row.color {
            self.color.push((entry, shift_glyph_quad(verts, dy)));
        }
        for bounds in &row.bounds {
            let mut shifted = bounds.clone();
            shifted.geometry = shifted.geometry.translated_y(dy);
            self.bounds.push(shifted);
        }
    }

    fn entries(&self) -> impl Iterator<Item = AnyAtlasEntry> + '_ {
        self.mask
            .iter()
            .map(|&(e, _)| e)
            .chain(self.subpixel.iter().map(|&(e, _)| e))
            .chain(self.color.iter().map(|&(e, _)| e))
    }
}

fn shift_glyph_quad(mut verts: [GlyphVertex; 6], dy: f32) -> [GlyphVertex; 6] {
    for v in &mut verts {
        v.position[1] += dy;
    }
    verts
}

fn shift_subpixel_quad(mut verts: [SubpixelGlyphVertex; 6], dy: f32) -> [SubpixelGlyphVertex; 6] {
    for v in &mut verts {
        v.position[1] += dy;
    }
    verts
}

pub(super) struct CachedRow {
    row_hash: u64,
    scale_bits: u32,
    origin_bits: (u32, u32),
    row_y_bits: u32,
    atlas_generation: u64,
    font_bindings_identity: super::super::glyph_atlas::FrameFontBindingsIdentity,
    /// Row may only be reused at its captured y (dy == 0): it contains
    /// y-dependent vertex data the row hash does not cover (gradient-face
    /// background samples, clip-band y-trims).
    verbatim_only: bool,
    streams: RowStreams,
    tick: u64,
}

/// Retained per-(frame, window, row) tessellation output from earlier frames.
///
/// Entries are partitioned by frame_id; if a frame_id were ever reused across
/// a compositor teardown/recreate, a surviving entry could meet a fresh atlas
/// whose eviction_generation restarted at 0 — revalidate_and_pin's
/// page-generation check is the backstop.
#[derive(Default)]
pub(crate) struct RowReuseCache {
    rows: HashMap<RowKey, CachedRow>,
    staged: Vec<(RowKey, CachedRow)>,
    tick: u64,
}

/// Entries untouched for this many commits are pruned (closed windows/frames).
const CACHE_STALE_TICKS: u64 = 600;

impl RowReuseCache {
    pub(super) fn stage(&mut self, captures: Vec<(RowKey, CachedRow)>) {
        self.staged.extend(captures);
    }

    /// Commit this frame's staged rows; prune entries no frame has refreshed
    /// for a long time.
    pub(super) fn commit_frame(&mut self) {
        self.tick += 1;
        let tick = self.tick;
        for (key, mut row) in self.staged.drain(..) {
            row.tick = tick;
            self.rows.insert(key, row);
        }
        if self.tick.is_multiple_of(64) {
            self.rows
                .retain(|_, row| tick.saturating_sub(row.tick) < CACHE_STALE_TICKS);
        }
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.rows.len()
    }
}

/// Per-frame text-row reuse counters (reset each frame).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RowReuseStats {
    /// Rows tessellated from scratch (including bailed rows).
    pub rows_tessellated: usize,
    /// Rows spliced verbatim from the cache.
    pub rows_reused_verbatim: usize,
    /// Rows spliced with an integral vertical shift.
    pub rows_reused_shifted: usize,
    /// Rows whose damage said reusable but a defensive key failed.
    pub reuse_bails: usize,
}

// ---------------------------------------------------------------------------
// Per-pass reuse context and the assembly driver
// ---------------------------------------------------------------------------

/// Everything the classifier needs that is constant across one text pass.
pub(super) struct ReusePassCtx<'a> {
    pub(super) damage: Option<&'a FrameRowDamage>,
    pub(super) scale_bits: u32,
    /// `scale_factor` is a power of two (mantissa zero) — required for
    /// bit-exact shifted splices.
    pub(super) scale_pow2: bool,
    pub(super) scale_factor: f32,
    pub(super) atlas_generation: u64,
    pub(super) font_bindings_identity: super::super::glyph_atlas::FrameFontBindingsIdentity,
    /// The active cursor's row, if any: vertices there carry the
    /// inverse-video swap and are never cached or spliced.
    pub(super) cursor_row: Option<(i64, u32)>,
    /// Frame-global vertex-mutating effects (text fade, mode-line fade, line
    /// animations) are active: all reuse and capture is disabled.
    pub(super) global_effects_active: bool,
    /// Rows whose glyphs receive transient pointer paint. Other rows remain
    /// eligible for ordinary reuse in the same frame.
    pub(super) invalidated_rows: Option<&'a HashSet<RowKey>>,
    /// Window id → frame-absolute bounds origin bits, from `window_infos`.
    pub(super) window_origins: &'a HashMap<i64, (u32, u32)>,
    /// Capture rows for future reuse (false for the chrome/overlay pass —
    /// chrome rows are always `RowDamage::New`).
    pub(super) allow_store: bool,
}

/// Per-row facts only the tessellator can observe, reported back so the
/// capture can key them.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct RowTessellation {
    /// A glyph's vertex data depended on its absolute y beyond the row hash
    /// (gradient-face background sample, clip-band y-trim/edge-touch): the
    /// captured row must never be spliced shifted.
    pub(super) verbatim_only: bool,
}

/// The tessellation backend the driver drives. Production implements this
/// over the real atlas path; tests substitute a deterministic fake so the
/// classifier/splice/assembly pipeline is exercised CPU-only.
pub(super) trait RowTessellator {
    /// Verify a cached atlas entry still points at live texels and pin its
    /// page for this frame so mid-frame eviction cannot invalidate it.
    fn revalidate_and_pin(&mut self, entry: AnyAtlasEntry) -> bool;
    /// Tessellate the chunk's glyphs, appending to `out`.
    fn tessellate(&mut self, chunk: &RowChunk, out: &mut RowStreams) -> RowTessellation;
}

enum RowPlan {
    Tessellate { bailed: bool },
    Splice { dy: f32 },
}

fn classify(chunk: &RowChunk, ctx: &ReusePassCtx<'_>, cache: &RowReuseCache) -> RowPlan {
    let tess = |bailed| RowPlan::Tessellate { bailed };

    let Some(damage) = ctx.damage else {
        return tess(false);
    };
    let Some(info) = damage.row(chunk.key.window_id, chunk.key.row) else {
        return tess(false);
    };
    let dvpos = match info.damage {
        RowDamage::New => return tess(false),
        RowDamage::Reused => 0.0f32,
        RowDamage::ReusedShifted { dvpos } => dvpos.get(),
    };

    // From here on the layout says the row is reusable; every early return is
    // a counted bail.
    if ctx.global_effects_active
        || ctx
            .invalidated_rows
            .is_some_and(|rows| rows.contains(&chunk.key))
        || !chunk.cacheable
        || info.row_hash == 0
    {
        return tess(true);
    }
    if ctx.cursor_row == Some((chunk.key.window_id, chunk.key.row)) {
        return tess(true);
    }
    if dvpos != 0.0 {
        // Bit-exact shifted splice needs an exactly integral physical shift
        // and power-of-two scale (divisions stay exact).
        if !ctx.scale_pow2 || (dvpos * ctx.scale_factor).fract() != 0.0 {
            return tess(true);
        }
    }
    let Some(cached) = cache.rows.get(&chunk.key) else {
        return tess(true);
    };
    if dvpos != 0.0 && cached.verbatim_only {
        // The cached vertices bake y-dependent data (gradient samples,
        // clip trims); only a same-y splice reproduces them.
        return tess(true);
    }
    if cached.row_hash != info.row_hash
        || cached.scale_bits != ctx.scale_bits
        || cached.atlas_generation != ctx.atlas_generation
        || cached.font_bindings_identity != ctx.font_bindings_identity
    {
        return tess(true);
    }
    let Some(&origin) = ctx.window_origins.get(&chunk.key.window_id) else {
        return tess(true);
    };
    if cached.origin_bits != origin {
        return tess(true);
    }
    // Row y must line up bit-exactly (shift applied) so spliced positions are
    // byte-identical to fresh tessellation.
    let expected_y = f32::from_bits(cached.row_y_bits) + dvpos;
    if expected_y.to_bits() != chunk.row_y.to_bits() {
        return tess(true);
    }
    RowPlan::Splice { dy: dvpos }
}

/// Walk the pass's row chunks in order, splicing reusable rows and
/// tessellating the rest, so the assembled streams are byte-identical to a
/// full tessellation of the same frame.
///
/// Returns the streams, the rows to stage for next frame, and the counters.
pub(super) fn assemble_rows_with_reuse(
    chunks: &[RowChunk],
    ctx: &ReusePassCtx<'_>,
    cache: &RowReuseCache,
    tessellator: &mut dyn RowTessellator,
) -> (RowStreams, Vec<(RowKey, CachedRow)>, RowReuseStats) {
    let mut out = RowStreams::default();
    let mut captures: Vec<(RowKey, CachedRow)> = Vec::new();
    let mut stats = RowReuseStats::default();

    for chunk in chunks {
        let plan = classify(chunk, ctx, cache);
        let plan = match plan {
            RowPlan::Splice { dy } => {
                // All-or-nothing: every atlas entry the cached row references
                // must still be live, and its page pinned for this frame.
                let cached = &cache.rows[&chunk.key];
                if cached
                    .streams
                    .entries()
                    .all(|entry| tessellator.revalidate_and_pin(entry))
                {
                    RowPlan::Splice { dy }
                } else {
                    RowPlan::Tessellate { bailed: true }
                }
            }
            other => other,
        };

        match plan {
            RowPlan::Splice { dy } => {
                let cached = &cache.rows[&chunk.key];
                let marks = out.lens();
                out.append_shifted(&cached.streams, dy);
                if dy == 0.0 {
                    stats.rows_reused_verbatim += 1;
                } else {
                    stats.rows_reused_shifted += 1;
                }
                if ctx.allow_store {
                    // Carry the row forward, rebased to its new position, so
                    // next frame can reuse it again.
                    captures.push((
                        chunk.key,
                        CachedRow {
                            row_hash: cached.row_hash,
                            scale_bits: cached.scale_bits,
                            origin_bits: cached.origin_bits,
                            row_y_bits: chunk.row_y.to_bits(),
                            atlas_generation: ctx.atlas_generation,
                            font_bindings_identity: ctx.font_bindings_identity,
                            verbatim_only: cached.verbatim_only,
                            streams: out.segment_since(marks),
                            tick: 0,
                        },
                    ));
                }
            }
            RowPlan::Tessellate { bailed } => {
                let marks = out.lens();
                let tessellation = tessellator.tessellate(chunk, &mut out);
                stats.rows_tessellated += 1;
                if bailed {
                    stats.reuse_bails += 1;
                }
                if let Some(capture) =
                    capture_after_tessellation(chunk, ctx, tessellation, &out, marks)
                {
                    captures.push(capture);
                }
            }
        }
    }

    (out, captures, stats)
}

fn capture_after_tessellation(
    chunk: &RowChunk,
    ctx: &ReusePassCtx<'_>,
    tessellation: RowTessellation,
    out: &RowStreams,
    marks: (usize, usize, usize, usize),
) -> Option<(RowKey, CachedRow)> {
    if !ctx.allow_store
        || ctx.global_effects_active
        || ctx
            .invalidated_rows
            .is_some_and(|rows| rows.contains(&chunk.key))
        || !chunk.cacheable
    {
        return None;
    }
    if ctx.cursor_row == Some((chunk.key.window_id, chunk.key.row)) {
        return None;
    }
    let info = ctx.damage?.row(chunk.key.window_id, chunk.key.row)?;
    if info.row_hash == 0 {
        return None;
    }
    let &origin = ctx.window_origins.get(&chunk.key.window_id)?;
    Some((
        chunk.key,
        CachedRow {
            row_hash: info.row_hash,
            scale_bits: ctx.scale_bits,
            origin_bits: origin,
            row_y_bits: chunk.row_y.to_bits(),
            atlas_generation: ctx.atlas_generation,
            font_bindings_identity: ctx.font_bindings_identity,
            verbatim_only: tessellation.verbatim_only,
            streams: out.segment_since(marks),
            tick: 0,
        },
    ))
}

// ---------------------------------------------------------------------------
// Frame-level helpers used by layer_text
// ---------------------------------------------------------------------------

/// Window id → frame-absolute bounds origin bits.
pub(super) fn window_origin_bits(frame_glyphs: &FrameGlyphBuffer) -> HashMap<i64, (u32, u32)> {
    frame_glyphs
        .window_infos
        .iter()
        .map(|info| {
            (
                info.window_id.get(),
                (info.bounds.x.to_bits(), info.bounds.y.to_bits()),
            )
        })
        .collect()
}

/// True when a glyph spanning `[y, y + height)` touches or crosses either
/// edge of the clip band `[top, bottom)`. Such glyphs are y-trimmed (or would
/// become trimmed under any vertical shift), so their row is verbatim-only.
pub(super) fn glyph_extent_touches_band(y: f32, height: f32, top: f32, bottom: f32) -> bool {
    y <= top || y + height >= bottom
}

/// True when `scale` is a positive power of two (f32 mantissa zero), which
/// makes shifted-splice arithmetic bit-exact.
pub(super) fn scale_is_power_of_two(scale: f32) -> bool {
    scale > 0.0 && scale.is_finite() && (scale.to_bits() & 0x007F_FFFF) == 0
}

#[cfg(test)]
#[path = "row_reuse_test.rs"]
mod tests;

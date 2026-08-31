//! CPU-honest golden and adversarial tests for RowDamage vertex reuse.
//!
//! These drive the REAL chunker/classifier/splice/assembly pipeline
//! (`chunk_text_rows` + `assemble_rows_with_reuse`) with a deterministic fake
//! tessellator standing in for the atlas-coupled per-glyph loop (which needs
//! a live wgpu device to rasterize). The golden property asserted is the one
//! production relies on: damage-driven assembly is BYTE-IDENTICAL to a full
//! tessellation of the same frame, and any defensive-key mismatch bails to
//! fresh tessellation.

use neomacs_display_protocol::types::FaceId;
use neomacs_display_protocol::types::Px;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;

use neomacs_display_protocol::frame_glyphs::{DisplaySlotId, FrameGlyph, GlyphRowRole};
use neomacs_display_protocol::glyph_matrix::RowDamage;
use neomacs_display_protocol::types::DisplayWindowId;

use super::super::super::glyph_atlas::FrameFontBindingsIdentity;
use super::super::super::glyph_atlas::types::{
    AlphaMask, AnyAtlasEntry, AtlasContentRect, AtlasEntry, ColorRgba, GlyphMaterialKind,
    GlyphMetrics, PageId, SubpixelMask, UvRect,
};
use super::super::super::vertex::{GlyphVertex, SubpixelGlyphVertex};
use super::super::glyphs::RenderedCharBounds;
use super::*;

const FRAME: u64 = 7;

// ---------------------------------------------------------------------------
// Synthetic glyphs
// ---------------------------------------------------------------------------

fn ch(window: i64, row: u32, col: u16, c: char, x: f32, y: f32) -> FrameGlyph {
    FrameGlyph::Char {
        window_id: DisplayWindowId::new(window),
        row_role: GlyphRowRole::Text,
        clip_rect: None,
        slot_id: DisplaySlotId {
            window_id: DisplayWindowId::new(window),
            row,
            col,
        },
        bidi_level: 0,
        char: c,
        composed: None,
        x,
        y,
        baseline: y + 10.0,
        width: 8.0,
        height: 14.0,
        ascent: 10.0,
        face_id: FaceId::new(3 + (c as u32 % 5)),
        box_vertical_edges: Default::default(),
    }
}

fn chrome_ch(window: i64, row: u32, col: u16, c: char, x: f32, y: f32) -> FrameGlyph {
    match ch(window, row, col, c, x, y) {
        FrameGlyph::Char {
            window_id,
            clip_rect,
            slot_id,
            bidi_level,
            char,
            composed,
            x,
            y,
            baseline,
            width,
            height,
            ascent,
            face_id,
            box_vertical_edges,
            ..
        } => FrameGlyph::Char {
            window_id,
            row_role: GlyphRowRole::ModeLine,
            clip_rect,
            slot_id,
            bidi_level,
            char,
            composed,
            x,
            y,
            baseline,
            width,
            height,
            ascent,
            face_id,
            box_vertical_edges,
        },
        _ => unreachable!(),
    }
}

/// Two windows, two text rows each, three chars per row. `y_shift` moves
/// every row down (simulating a scrolled frame with identical content).
fn two_window_glyphs(y_shift: f32) -> Vec<FrameGlyph> {
    let mut glyphs = Vec::new();
    for (window, x0) in [(10i64, 0.0f32), (20, 200.0)] {
        for row in 0..2u32 {
            let y = 14.0 * row as f32 + y_shift;
            for col in 0..3u16 {
                let c =
                    char::from_u32('a' as u32 + (window as u32 % 7) + row + col as u32).unwrap();
                glyphs.push(ch(window, row, col, c, x0 + 8.0 * col as f32, y));
            }
        }
    }
    glyphs
}

// ---------------------------------------------------------------------------
// Fake tessellator: deterministic, position-linear, exercises all 3 streams
// ---------------------------------------------------------------------------

fn fake_entry(c: char) -> AnyAtlasEntry {
    let code = c as u32;
    let page = NonZeroU32::new(1 + code % 3).unwrap();
    let rect = AtlasContentRect::new(
        code % 64,
        code % 32,
        NonZeroU32::new(8).unwrap(),
        NonZeroU32::new(12).unwrap(),
    );
    let u = (code % 97) as f32 / 97.0;
    let uv = UvRect::new([u, u * 0.5], [u + 0.01, u * 0.5 + 0.01]);
    let metrics = GlyphMetrics {
        bearing_x: 0.5,
        bearing_y: 9.0,
        advance_width: 8.0,
    };
    match code % 3 {
        0 => AnyAtlasEntry::Alpha(AtlasEntry::new(
            PageId::<AlphaMask>::new(page),
            0,
            rect,
            uv,
            metrics,
        )),
        1 => AnyAtlasEntry::Subpixel(AtlasEntry::new(
            PageId::<SubpixelMask>::new(page),
            0,
            rect,
            uv,
            metrics,
        )),
        _ => AnyAtlasEntry::Color(AtlasEntry::new(
            PageId::<ColorRgba>::new(page),
            0,
            rect,
            uv,
            metrics,
        )),
    }
}

fn fake_glyph_quad(c: char, x: f32, y: f32, color: [f32; 4]) -> [GlyphVertex; 6] {
    let uv = fake_entry(c).uv();
    let (u0, v0) = (uv.min()[0], uv.min()[1]);
    let (u1, v1) = (uv.max()[0], uv.max()[1]);
    let (w, h) = (8.0, 12.0);
    let v = |px: f32, py: f32, u: f32, vv: f32| GlyphVertex {
        position: [px, py],
        tex_coords: [u, vv],
        color,
    };
    [
        v(x, y, u0, v0),
        v(x + w, y, u1, v0),
        v(x + w, y + h, u1, v1),
        v(x, y, u0, v0),
        v(x + w, y + h, u1, v1),
        v(x, y + h, u0, v1),
    ]
}

/// Test knobs for the fake tessellator, mirroring the conditions the live
/// tessellator detects (gradient faces, clip-band trims).
#[derive(Default, Clone)]
struct FakeConfig {
    fail_revalidate: bool,
    gradient_faces: std::collections::HashSet<FaceId>,
    clip_band: Option<(f32, f32)>,
}

struct FakeTessellator<'a> {
    glyphs: &'a [FrameGlyph],
    want_overlay: bool,
    config: FakeConfig,
    revalidated: usize,
}

impl<'a> FakeTessellator<'a> {
    fn new(glyphs: &'a [FrameGlyph]) -> Self {
        Self {
            glyphs,
            want_overlay: false,
            config: FakeConfig::default(),
            revalidated: 0,
        }
    }
}

impl RowTessellator for FakeTessellator<'_> {
    fn revalidate_and_pin(&mut self, _entry: AnyAtlasEntry) -> bool {
        self.revalidated += 1;
        !self.config.fail_revalidate
    }

    fn tessellate(&mut self, chunk: &RowChunk, out: &mut RowStreams) -> RowTessellation {
        let mut tessellation = RowTessellation::default();
        for glyph_index in chunk.glyphs.clone() {
            let FrameGlyph::Char {
                window_id,
                row_role,
                slot_id,
                char: c,
                x,
                y,
                width,
                height,
                face_id,
                ..
            } = &self.glyphs[glyph_index]
            else {
                continue;
            };
            if row_role.is_chrome() != self.want_overlay {
                continue;
            }
            if self.config.gradient_faces.contains(face_id) {
                tessellation.verbatim_only = true;
            }
            if let Some((top, bottom)) = self.config.clip_band {
                if glyph_extent_touches_band(*y, 12.0, top, bottom) {
                    tessellation.verbatim_only = true;
                }
            }
            let entry = fake_entry(*c);
            let fg = [face_id.get() as f32 / 255.0, 0.25, 0.5, 1.0];
            match entry {
                AnyAtlasEntry::Subpixel(_) => {
                    let quad = fake_glyph_quad(*c, *x, *y, fg);
                    let sub = quad.map(|v| SubpixelGlyphVertex {
                        position: v.position,
                        tex_coords: v.tex_coords,
                        fg_color: v.color,
                        bg_color: [0.0, 0.0, 0.0, 1.0],
                    });
                    out.subpixel.push((entry, sub));
                }
                AnyAtlasEntry::Color(_) => {
                    out.color.push((entry, fake_glyph_quad(*c, *x, *y, fg)));
                }
                AnyAtlasEntry::Alpha(_) => {
                    out.mask.push((entry, fake_glyph_quad(*c, *x, *y, fg)));
                }
            }
            out.bounds.push(RenderedCharBounds {
                glyph_index,
                window_id: window_id.get(),
                row_role: *row_role,
                slot_id: *slot_id,
                label: c.to_string(),
                face_id: *face_id,
                font_size: 14.0,
                cell_x: *x,
                cell_y: *y,
                cell_w: *width,
                cell_h: *height,
                glyph_x: *x,
                glyph_y: *y,
                glyph_w: 8.0,
                glyph_h: 12.0,
                left_overhang: 0.0,
                right_overhang: 0.0,
                top_overhang: 0.0,
                bottom_overhang: 0.0,
            });
        }
        tessellation
    }
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn origins() -> HashMap<i64, (u32, u32)> {
    [
        (10i64, (0.0f32.to_bits(), 0.0f32.to_bits())),
        (20, (200.0f32.to_bits(), 0.0f32.to_bits())),
    ]
    .into_iter()
    .collect()
}

/// (window, row, damage, hash) rows → summary; rows are dense from 0.
fn damage_of(rows: &[(i64, u32, RowDamage, u64)]) -> FrameRowDamage {
    let mut windows: HashMap<i64, WindowRowDamage> = HashMap::new();
    for &(window, row, damage, row_hash) in rows {
        let entry = windows.entry(window).or_default();
        let idx = row as usize;
        if entry.rows.len() <= idx {
            entry.rows.resize(
                idx + 1,
                RowDamageInfo {
                    damage: RowDamage::New,
                    row_hash: 0,
                },
            );
        }
        entry.rows[idx] = RowDamageInfo { damage, row_hash };
    }
    FrameRowDamage { windows }
}

#[test]
fn frame_damage_is_derived_from_each_authoritative_row() {
    use neomacs_display_protocol::glyph_matrix::{
        FrameDisplayState, GlyphArea, GlyphMatrix, WindowMatrixEntry,
    };
    use neomacs_display_protocol::{DisplayWindowId, FaceId, Rect};

    let mut matrix = GlyphMatrix::new(1, 1);
    neomacs_display_protocol::glyph_matrix::MatrixRow::make_mut(&mut matrix.rows[0]).glyphs
        [GlyphArea::Text.index()]
    .push(neomacs_display_protocol::glyph_matrix::Glyph::char(
        'x',
        FaceId::new(0),
        0,
    ));
    matrix.set_row_damage(0, RowDamage::Reused);
    let mut state = FrameDisplayState::new(1, 1, 8.0, 16.0);
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(10),
        matrix,
        pixel_bounds: Rect::new(0.0, 0.0, 8.0, 16.0),
        text_pixel_bounds: Rect::new(0.0, 0.0, 8.0, 16.0),
        text_clip_bounds: None,
        selected: true,
    });

    let damage = FrameRowDamage::from_display_state(&state);

    assert_eq!(damage.row(10, 0).unwrap().damage, RowDamage::Reused);
}

fn hash_for(window: i64, row: u32) -> u64 {
    0x1000 + window as u64 * 16 + row as u64
}

#[test]
fn pointer_row_index_marks_only_listed_reused_rows_new() {
    let mut damage = damage_of(&[
        (10, 0, RowDamage::Reused, hash_for(10, 0)),
        (10, 1, RowDamage::Reused, hash_for(10, 1)),
    ]);

    let inspected = damage.invalidate_pointer_rows(&[
        neomacs_display_protocol::PresentedPointerDamageRow::new(
            neomacs_display_protocol::DisplayWindowId::new(10),
            0,
        ),
    ]);

    assert_eq!(inspected, 1);
    assert!(matches!(damage.row(10, 0).unwrap().damage, RowDamage::New));
    assert!(matches!(
        damage.row(10, 1).unwrap().damage,
        RowDamage::Reused
    ));
}

#[test]
fn pointer_row_invalidation_inspects_only_affected_rows_in_a_ten_thousand_row_frame() {
    let rows = (0..10_000)
        .map(|row| (10, row, RowDamage::Reused, hash_for(10, row)))
        .collect::<Vec<_>>();
    let mut damage = damage_of(&rows);
    let affected = [
        neomacs_display_protocol::PresentedPointerDamageRow::new(
            neomacs_display_protocol::DisplayWindowId::new(10),
            7,
        ),
        neomacs_display_protocol::PresentedPointerDamageRow::new(
            neomacs_display_protocol::DisplayWindowId::new(10),
            9_007,
        ),
    ];

    let inspected = damage.invalidate_pointer_rows(&affected);

    assert_eq!(inspected, 2);
    assert!(matches!(damage.row(10, 7).unwrap().damage, RowDamage::New));
    assert!(matches!(
        damage.row(10, 9_007).unwrap().damage,
        RowDamage::New
    ));
    assert!(matches!(
        damage.row(10, 5_000).unwrap().damage,
        RowDamage::Reused
    ));
}

fn all_rows(damage_kind: impl Fn(i64, u32) -> RowDamage) -> FrameRowDamage {
    let mut rows = Vec::new();
    for window in [10i64, 20] {
        for row in 0..2u32 {
            rows.push((window, row, damage_kind(window, row), hash_for(window, row)));
        }
    }
    damage_of(&rows)
}

struct Ran {
    out: RowStreams,
    captures: Vec<(RowKey, CachedRow)>,
    stats: RowReuseStats,
}

fn run_pass_with(
    glyphs: &[FrameGlyph],
    ctx: &ReusePassCtx<'_>,
    cache: &RowReuseCache,
    config: &FakeConfig,
) -> Ran {
    let chunks = chunk_text_rows(glyphs, false, FRAME);
    let mut tess = FakeTessellator::new(glyphs);
    tess.config = config.clone();
    let (out, captures, stats) = assemble_rows_with_reuse(&chunks, ctx, cache, &mut tess);
    Ran {
        out,
        captures,
        stats,
    }
}

fn run_pass(
    glyphs: &[FrameGlyph],
    ctx: &ReusePassCtx<'_>,
    cache: &RowReuseCache,
    fail_revalidate: bool,
) -> Ran {
    run_pass_with(
        glyphs,
        ctx,
        cache,
        &FakeConfig {
            fail_revalidate,
            ..FakeConfig::default()
        },
    )
}

fn base_ctx<'a>(
    damage: Option<&'a FrameRowDamage>,
    origins: &'a HashMap<i64, (u32, u32)>,
) -> ReusePassCtx<'a> {
    ReusePassCtx {
        damage,
        scale_bits: 1.0f32.to_bits(),
        scale_pow2: true,
        scale_factor: 1.0,
        atlas_generation: 1,
        font_bindings_identity: FrameFontBindingsIdentity::default(),
        cursor_row: None,
        global_effects_active: false,
        invalidated_rows: None,
        window_origins: origins,
        allow_store: true,
    }
}

/// Warm the cache: tessellate every row of `glyphs` (damage all-New so no
/// reuse fires, hashes present so captures happen) and commit.
fn warm_cache_with(
    glyphs: &[FrameGlyph],
    origins: &HashMap<i64, (u32, u32)>,
    config: &FakeConfig,
) -> RowReuseCache {
    let mut cache = RowReuseCache::default();
    let damage = all_rows(|_, _| RowDamage::New);
    let ran = run_pass_with(glyphs, &base_ctx(Some(&damage), origins), &cache, config);
    assert_eq!(ran.stats.rows_tessellated, 4);
    assert_eq!(ran.captures.len(), 4);
    cache.stage(ran.captures);
    cache.commit_frame();
    assert_eq!(cache.len(), 4);
    cache
}

fn warm_cache(glyphs: &[FrameGlyph], origins: &HashMap<i64, (u32, u32)>) -> RowReuseCache {
    warm_cache_with(glyphs, origins, &FakeConfig::default())
}

/// Byte-level fingerprint of everything the GPU would consume, plus the
/// atlas-entry identities the draw batcher keys on.
type Fingerprint = (
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    Vec<((GlyphMaterialKind, u32), [u32; 4])>,
    Vec<(usize, u32, u32)>,
);

fn fingerprint(streams: &RowStreams) -> Fingerprint {
    let mut mask = Vec::new();
    for (_, verts) in &streams.mask {
        mask.extend_from_slice(bytemuck::cast_slice(&verts[..]));
    }
    let mut subpixel = Vec::new();
    for (_, verts) in &streams.subpixel {
        subpixel.extend_from_slice(bytemuck::cast_slice(&verts[..]));
    }
    let mut color = Vec::new();
    for (_, verts) in &streams.color {
        color.extend_from_slice(bytemuck::cast_slice(&verts[..]));
    }
    let entries = streams
        .entries()
        .map(|entry| {
            let uv = entry.uv();
            (
                entry.page_id_value(),
                [
                    uv.min()[0].to_bits(),
                    uv.min()[1].to_bits(),
                    uv.max()[0].to_bits(),
                    uv.max()[1].to_bits(),
                ],
            )
        })
        .collect();
    let bounds = streams
        .bounds
        .iter()
        .map(|b| (b.glyph_index, b.cell_y.to_bits(), b.glyph_y.to_bits()))
        .collect();
    (mask, subpixel, color, entries, bounds)
}

// ---------------------------------------------------------------------------
// Chunker
// ---------------------------------------------------------------------------

#[test]
fn chunker_groups_contiguous_rows_in_order() {
    let glyphs = two_window_glyphs(0.0);
    let chunks = chunk_text_rows(&glyphs, false, FRAME);
    assert_eq!(chunks.len(), 4);
    let keys: Vec<(i64, u32)> = chunks
        .iter()
        .map(|c| (c.key.window_id, c.key.row))
        .collect();
    assert_eq!(keys, vec![(10, 0), (10, 1), (20, 0), (20, 1)]);
    assert!(chunks.iter().all(|c| c.cacheable));
    assert_eq!(chunks[1].row_y, 14.0);
}

#[test]
fn chunker_flags_non_contiguous_rows_uncacheable() {
    let mut glyphs = two_window_glyphs(0.0);
    // Row (10, 0) reappears after other rows: both runs must be uncacheable.
    glyphs.push(ch(10, 0, 9, 'z', 72.0, 0.0));
    let chunks = chunk_text_rows(&glyphs, false, FRAME);
    assert_eq!(chunks.len(), 5);
    assert!(!chunks[0].cacheable);
    assert!(!chunks[4].cacheable);
    assert!(chunks[1].cacheable && chunks[2].cacheable && chunks[3].cacheable);
}

#[test]
fn chunker_filters_by_pass_role() {
    let mut glyphs = two_window_glyphs(0.0);
    glyphs.push(chrome_ch(10, 5, 0, 'm', 0.0, 70.0));
    let text_chunks = chunk_text_rows(&glyphs, false, FRAME);
    assert_eq!(text_chunks.len(), 4);
    let chrome_chunks = chunk_text_rows(&glyphs, true, FRAME);
    assert_eq!(chrome_chunks.len(), 1);
    assert_eq!(chrome_chunks[0].key.row, 5);
}

// ---------------------------------------------------------------------------
// Goldens
// ---------------------------------------------------------------------------

#[test]
fn golden_reused_rows_splice_byte_identical_to_full_tessellation() {
    let glyphs = two_window_glyphs(0.0);
    let origins = origins();
    let cache = warm_cache(&glyphs, &origins);

    // Damage-driven frame: every row Reused, warm cache.
    let damage = all_rows(|_, _| RowDamage::Reused);
    let reused = run_pass(&glyphs, &base_ctx(Some(&damage), &origins), &cache, false);
    assert_eq!(reused.stats.rows_reused_verbatim, 4);
    assert_eq!(reused.stats.rows_tessellated, 0);
    assert_eq!(reused.stats.reuse_bails, 0);

    // Forced-full frame: no damage, cold cache.
    let full = run_pass(
        &glyphs,
        &base_ctx(None, &origins),
        &RowReuseCache::default(),
        false,
    );
    assert_eq!(full.stats.rows_tessellated, 4);

    assert_eq!(fingerprint(&reused.out), fingerprint(&full.out));
    // Spliced rows are carried forward so the next frame can reuse them too.
    assert_eq!(reused.captures.len(), 4);
}

#[test]
fn golden_shifted_rows_splice_byte_identical_to_full_tessellation() {
    let glyphs_a = two_window_glyphs(0.0);
    let origins = origins();
    let cache = warm_cache(&glyphs_a, &origins);

    // The next frame scrolled everything down by exactly 3 logical px
    // (integral physical shift at scale 1.0).
    let dvpos = 3.0f32;
    let glyphs_b = two_window_glyphs(dvpos);
    let damage = all_rows(|_, _| RowDamage::ReusedShifted { dvpos: Px(dvpos) });

    let shifted = run_pass(&glyphs_b, &base_ctx(Some(&damage), &origins), &cache, false);
    assert_eq!(shifted.stats.rows_reused_shifted, 4);
    assert_eq!(shifted.stats.rows_tessellated, 0);

    let full = run_pass(
        &glyphs_b,
        &base_ctx(None, &origins),
        &RowReuseCache::default(),
        false,
    );
    assert_eq!(fingerprint(&shifted.out), fingerprint(&full.out));
}

#[test]
fn spliced_rows_rebase_so_next_frame_reuses_them_again() {
    let glyphs_a = two_window_glyphs(0.0);
    let origins = origins();
    let mut cache = warm_cache(&glyphs_a, &origins);

    let dvpos = 14.0f32;
    let glyphs_b = two_window_glyphs(dvpos);
    let damage_b = all_rows(|_, _| RowDamage::ReusedShifted { dvpos: Px(dvpos) });
    let ran_b = run_pass(
        &glyphs_b,
        &base_ctx(Some(&damage_b), &origins),
        &cache,
        false,
    );
    assert_eq!(ran_b.stats.rows_reused_shifted, 4);
    cache.stage(ran_b.captures);
    cache.commit_frame();

    // Frame C: unchanged relative to B → verbatim reuse of the rebased rows.
    let damage_c = all_rows(|_, _| RowDamage::Reused);
    let ran_c = run_pass(
        &glyphs_b,
        &base_ctx(Some(&damage_c), &origins),
        &cache,
        false,
    );
    assert_eq!(ran_c.stats.rows_reused_verbatim, 4);

    let full = run_pass(
        &glyphs_b,
        &base_ctx(None, &origins),
        &RowReuseCache::default(),
        false,
    );
    assert_eq!(fingerprint(&ran_c.out), fingerprint(&full.out));
}

// ---------------------------------------------------------------------------
// Adversarial bails (all must still be byte-identical via fresh tessellation)
// ---------------------------------------------------------------------------

/// Run a Reused-damage frame under `mutate` and assert every row bailed to
/// fresh tessellation with output identical to a full run.
fn assert_all_rows_bail(mutate: impl FnOnce(&mut ReusePassCtx<'_>), expected_bails: usize) {
    let glyphs = two_window_glyphs(0.0);
    let origins = origins();
    let cache = warm_cache(&glyphs, &origins);
    let damage = all_rows(|_, _| RowDamage::Reused);
    let mut ctx = base_ctx(Some(&damage), &origins);
    mutate(&mut ctx);

    let ran = run_pass(&glyphs, &ctx, &cache, false);
    assert_eq!(ran.stats.rows_reused_verbatim, 0);
    assert_eq!(ran.stats.rows_reused_shifted, 0);
    assert_eq!(ran.stats.rows_tessellated, 4);
    assert_eq!(ran.stats.reuse_bails, expected_bails);

    let full = run_pass(
        &glyphs,
        &base_ctx(None, &origins),
        &RowReuseCache::default(),
        false,
    );
    assert_eq!(fingerprint(&ran.out), fingerprint(&full.out));
}

#[test]
fn adversarial_atlas_generation_bump_forces_full_retess() {
    assert_all_rows_bail(|ctx| ctx.atlas_generation += 1, 4);
}

#[test]
fn changed_frame_font_bindings_force_full_retessellation() {
    assert_all_rows_bail(
        |ctx| ctx.font_bindings_identity = FrameFontBindingsIdentity(1),
        4,
    );
}

#[test]
fn adversarial_scale_change_bails() {
    assert_all_rows_bail(
        |ctx| {
            ctx.scale_factor = 2.0;
            ctx.scale_bits = 2.0f32.to_bits();
        },
        4,
    );
}

#[test]
fn adversarial_content_hash_mismatch_retessellates() {
    let glyphs = two_window_glyphs(0.0);
    let origins = origins();
    let cache = warm_cache(&glyphs, &origins);

    // Row (10, 1) claims Reused but its content hash changed.
    let mut rows = Vec::new();
    for window in [10i64, 20] {
        for row in 0..2u32 {
            let hash = if (window, row) == (10, 1) {
                0xDEAD_BEEF
            } else {
                hash_for(window, row)
            };
            rows.push((window, row, RowDamage::Reused, hash));
        }
    }
    let damage = damage_of(&rows);
    let ran = run_pass(&glyphs, &base_ctx(Some(&damage), &origins), &cache, false);
    assert_eq!(ran.stats.rows_reused_verbatim, 3);
    assert_eq!(ran.stats.rows_tessellated, 1);
    assert_eq!(ran.stats.reuse_bails, 1);

    let full = run_pass(
        &glyphs,
        &base_ctx(None, &origins),
        &RowReuseCache::default(),
        false,
    );
    assert_eq!(fingerprint(&ran.out), fingerprint(&full.out));
}

#[test]
fn adversarial_non_integral_shift_bails() {
    let glyphs_a = two_window_glyphs(0.0);
    let origins = origins();
    let cache = warm_cache(&glyphs_a, &origins);

    let dvpos = 1.5f32; // 1.5 physical px at scale 1.0 — not integral
    let glyphs_b = two_window_glyphs(dvpos);
    let damage = all_rows(|_, _| RowDamage::ReusedShifted { dvpos: Px(dvpos) });
    let ran = run_pass(&glyphs_b, &base_ctx(Some(&damage), &origins), &cache, false);
    assert_eq!(ran.stats.rows_reused_shifted, 0);
    assert_eq!(ran.stats.rows_tessellated, 4);
    assert_eq!(ran.stats.reuse_bails, 4);

    let full = run_pass(
        &glyphs_b,
        &base_ctx(None, &origins),
        &RowReuseCache::default(),
        false,
    );
    assert_eq!(fingerprint(&ran.out), fingerprint(&full.out));
}

#[test]
fn adversarial_non_power_of_two_scale_never_splices_shifted() {
    let glyphs_a = two_window_glyphs(0.0);
    let origins = origins();
    // Warm at scale 3.0 so the cached scale matches the reuse frame.
    let mut cache = RowReuseCache::default();
    let warm_damage = all_rows(|_, _| RowDamage::New);
    let mut warm_ctx = base_ctx(Some(&warm_damage), &origins);
    warm_ctx.scale_factor = 3.0;
    warm_ctx.scale_bits = 3.0f32.to_bits();
    warm_ctx.scale_pow2 = false;
    let warmed = run_pass(&glyphs_a, &warm_ctx, &cache, false);
    cache.stage(warmed.captures);
    cache.commit_frame();

    // dvpos * 3.0 is integral, but 3.0 is not a power of two → divisions in
    // fresh tessellation would not be bit-exact against `pos + dvpos`.
    let dvpos = 2.0f32;
    let glyphs_b = two_window_glyphs(dvpos);
    let damage = all_rows(|_, _| RowDamage::ReusedShifted { dvpos: Px(dvpos) });
    let mut ctx = base_ctx(Some(&damage), &origins);
    ctx.scale_factor = 3.0;
    ctx.scale_bits = 3.0f32.to_bits();
    ctx.scale_pow2 = false;
    let ran = run_pass(&glyphs_b, &ctx, &cache, false);
    assert_eq!(ran.stats.rows_reused_shifted, 0);
    assert_eq!(ran.stats.reuse_bails, 4);

    let mut full_ctx = base_ctx(None, &origins);
    full_ctx.scale_factor = 3.0;
    full_ctx.scale_bits = 3.0f32.to_bits();
    full_ctx.scale_pow2 = false;
    let full = run_pass(&glyphs_b, &full_ctx, &RowReuseCache::default(), false);
    assert_eq!(fingerprint(&ran.out), fingerprint(&full.out));
}

#[test]
fn reused_damage_with_empty_cache_tessellates_and_counts_bail() {
    // First frame / new window / resize: damage may say Reused while the
    // renderer has no cached rows yet — never treat summary presence as
    // reusability.
    let glyphs = two_window_glyphs(0.0);
    let origins = origins();
    let damage = all_rows(|_, _| RowDamage::Reused);
    let ran = run_pass(
        &glyphs,
        &base_ctx(Some(&damage), &origins),
        &RowReuseCache::default(),
        false,
    );
    assert_eq!(ran.stats.rows_reused_verbatim, 0);
    assert_eq!(ran.stats.rows_tessellated, 4);
    assert_eq!(ran.stats.reuse_bails, 4);

    let full = run_pass(
        &glyphs,
        &base_ctx(None, &origins),
        &RowReuseCache::default(),
        false,
    );
    assert_eq!(fingerprint(&ran.out), fingerprint(&full.out));
}

#[test]
fn cursor_row_never_spliced_and_never_captured() {
    let glyphs = two_window_glyphs(0.0);
    let origins = origins();
    let cache = warm_cache(&glyphs, &origins);
    let damage = all_rows(|_, _| RowDamage::Reused);
    let mut ctx = base_ctx(Some(&damage), &origins);
    ctx.cursor_row = Some((10, 1));

    let ran = run_pass(&glyphs, &ctx, &cache, false);
    assert_eq!(ran.stats.rows_reused_verbatim, 3);
    assert_eq!(ran.stats.rows_tessellated, 1);
    assert_eq!(ran.stats.reuse_bails, 1);
    // The cursor row is not captured either (its vertices may carry the
    // inverse-video swap).
    assert!(
        ran.captures
            .iter()
            .all(|(key, _)| (key.window_id, key.row) != (10, 1))
    );
    assert_eq!(ran.captures.len(), 3);
}

#[test]
fn revalidate_failure_bails_to_fresh_tessellation() {
    let glyphs = two_window_glyphs(0.0);
    let origins = origins();
    let cache = warm_cache(&glyphs, &origins);
    let damage = all_rows(|_, _| RowDamage::Reused);

    let ran = run_pass(&glyphs, &base_ctx(Some(&damage), &origins), &cache, true);
    assert_eq!(ran.stats.rows_reused_verbatim, 0);
    assert_eq!(ran.stats.rows_tessellated, 4);
    assert_eq!(ran.stats.reuse_bails, 4);

    let full = run_pass(
        &glyphs,
        &base_ctx(None, &origins),
        &RowReuseCache::default(),
        false,
    );
    assert_eq!(fingerprint(&ran.out), fingerprint(&full.out));
}

#[test]
fn global_effects_disable_reuse_and_capture() {
    let glyphs = two_window_glyphs(0.0);
    let origins = origins();
    let cache = warm_cache(&glyphs, &origins);
    let damage = all_rows(|_, _| RowDamage::Reused);
    let mut ctx = base_ctx(Some(&damage), &origins);
    ctx.global_effects_active = true;

    let ran = run_pass(&glyphs, &ctx, &cache, false);
    assert_eq!(ran.stats.rows_reused_verbatim, 0);
    assert_eq!(ran.stats.rows_tessellated, 4);
    assert_eq!(ran.stats.reuse_bails, 4);
    // Effect-polluted vertices must never enter the cache.
    assert!(ran.captures.is_empty());
}

#[test]
fn pointer_paint_invalidates_only_the_intersecting_row() {
    let glyphs = two_window_glyphs(0.0);
    let origins = origins();
    let cache = warm_cache(&glyphs, &origins);
    let damage = all_rows(|_, _| RowDamage::Reused);
    let invalidated = HashSet::from([RowKey {
        frame_id: FRAME,
        window_id: 10,
        row: 0,
        overlay: false,
    }]);
    let mut ctx = base_ctx(Some(&damage), &origins);
    ctx.invalidated_rows = Some(&invalidated);

    let ran = run_pass(&glyphs, &ctx, &cache, false);
    assert_eq!(ran.stats.rows_reused_verbatim, 3);
    assert_eq!(ran.stats.rows_tessellated, 1);
    assert_eq!(ran.stats.reuse_bails, 1);
    assert_eq!(ran.captures.len(), 3, "pointer-painted row is not cached");
}

#[test]
fn row_y_mismatch_bails_verbatim_reuse() {
    // Same content hash but the row landed at a different y (e.g. a chrome
    // height change layout failed to classify): verbatim reuse must bail.
    let glyphs_a = two_window_glyphs(0.0);
    let origins = origins();
    let cache = warm_cache(&glyphs_a, &origins);

    let glyphs_b = two_window_glyphs(5.0);
    let damage = all_rows(|_, _| RowDamage::Reused);
    let ran = run_pass(&glyphs_b, &base_ctx(Some(&damage), &origins), &cache, false);
    assert_eq!(ran.stats.rows_reused_verbatim, 0);
    assert_eq!(ran.stats.reuse_bails, 4);

    let full = run_pass(
        &glyphs_b,
        &base_ctx(None, &origins),
        &RowReuseCache::default(),
        false,
    );
    assert_eq!(fingerprint(&ran.out), fingerprint(&full.out));
}

#[test]
fn non_contiguous_row_bails_even_when_damage_says_reused() {
    let mut glyphs = two_window_glyphs(0.0);
    glyphs.push(ch(10, 0, 9, 'z', 72.0, 0.0));
    let origins = origins();

    // Warm from the same layout so the cache holds nothing for the dup row.
    let mut cache = RowReuseCache::default();
    let warm_damage = all_rows(|_, _| RowDamage::New);
    let warmed = run_pass(
        &glyphs,
        &base_ctx(Some(&warm_damage), &origins),
        &cache,
        false,
    );
    // Dup row (10, 0) is uncacheable: only the three clean rows captured.
    assert_eq!(warmed.captures.len(), 3);
    cache.stage(warmed.captures);
    cache.commit_frame();

    let damage = all_rows(|_, _| RowDamage::Reused);
    let ran = run_pass(&glyphs, &base_ctx(Some(&damage), &origins), &cache, false);
    // Three clean rows splice; the duplicated row's two chunks both bail.
    assert_eq!(ran.stats.rows_reused_verbatim, 3);
    assert_eq!(ran.stats.rows_tessellated, 2);
    assert_eq!(ran.stats.reuse_bails, 2);
}

#[test]
fn gradient_face_rows_never_splice_shifted_but_splice_verbatim() {
    // A face background gradient is sampled at the glyph's y and baked into
    // vertex colors; a shifted splice would keep the old sample. Rows with
    // gradient faces are captured verbatim-only.
    let glyphs_a = two_window_glyphs(0.0);
    let origins = origins();
    // Every synthetic face id (3..8) is gradient-bearing.
    let config = FakeConfig {
        gradient_faces: (3..8).map(FaceId::new).collect(),
        ..FakeConfig::default()
    };
    let cache = warm_cache_with(&glyphs_a, &origins, &config);

    // Integral shift that would otherwise splice: must bail on every row.
    let dvpos = 3.0f32;
    let glyphs_b = two_window_glyphs(dvpos);
    let damage = all_rows(|_, _| RowDamage::ReusedShifted { dvpos: Px(dvpos) });
    let shifted = run_pass_with(
        &glyphs_b,
        &base_ctx(Some(&damage), &origins),
        &cache,
        &config,
    );
    assert_eq!(shifted.stats.rows_reused_shifted, 0);
    assert_eq!(shifted.stats.rows_tessellated, 4);
    assert_eq!(shifted.stats.reuse_bails, 4);
    let full = run_pass_with(
        &glyphs_b,
        &base_ctx(None, &origins),
        &RowReuseCache::default(),
        &config,
    );
    assert_eq!(fingerprint(&shifted.out), fingerprint(&full.out));

    // Verbatim reuse (dy == 0, same sample positions) stays allowed.
    let damage = all_rows(|_, _| RowDamage::Reused);
    let verbatim = run_pass_with(
        &glyphs_a,
        &base_ctx(Some(&damage), &origins),
        &cache,
        &config,
    );
    assert_eq!(verbatim.stats.rows_reused_verbatim, 4);
    assert_eq!(verbatim.stats.reuse_bails, 0);
    let full_a = run_pass_with(
        &glyphs_a,
        &base_ctx(None, &origins),
        &RowReuseCache::default(),
        &config,
    );
    assert_eq!(fingerprint(&verbatim.out), fingerprint(&full_a.out));
}

#[test]
fn band_edge_rows_never_splice_shifted() {
    // Rows whose glyph extents touch the clip band edge carry y-dependent
    // trims; they are verbatim-only.
    let glyphs_a = two_window_glyphs(0.0);
    let origins = origins();
    // Band [0, 20): row 0 (y=0) touches the top edge, row 1 (y=14, quad
    // height 12 → extent 26) crosses the bottom edge.
    let config = FakeConfig {
        clip_band: Some((0.0, 20.0)),
        ..FakeConfig::default()
    };
    let cache = warm_cache_with(&glyphs_a, &origins, &config);

    let dvpos = 2.0f32;
    let glyphs_b = two_window_glyphs(dvpos);
    let damage = all_rows(|_, _| RowDamage::ReusedShifted { dvpos: Px(dvpos) });
    let ran = run_pass_with(
        &glyphs_b,
        &base_ctx(Some(&damage), &origins),
        &cache,
        &config,
    );
    assert_eq!(ran.stats.rows_reused_shifted, 0);
    assert_eq!(ran.stats.rows_tessellated, 4);
    assert_eq!(ran.stats.reuse_bails, 4);

    let full = run_pass_with(
        &glyphs_b,
        &base_ctx(None, &origins),
        &RowReuseCache::default(),
        &config,
    );
    assert_eq!(fingerprint(&ran.out), fingerprint(&full.out));
}

#[test]
fn glyph_extent_touches_band_flags_edges_and_crossings() {
    // Strictly inside: safe.
    assert!(!glyph_extent_touches_band(5.0, 10.0, 0.0, 20.0));
    // Touching top / crossing top.
    assert!(glyph_extent_touches_band(0.0, 10.0, 0.0, 20.0));
    assert!(glyph_extent_touches_band(-1.0, 10.0, 0.0, 20.0));
    // Touching bottom / crossing bottom.
    assert!(glyph_extent_touches_band(10.0, 10.0, 0.0, 20.0));
    assert!(glyph_extent_touches_band(15.0, 10.0, 0.0, 20.0));
    // Fully above / below the band (fully clipped) also flags.
    assert!(glyph_extent_touches_band(-20.0, 10.0, 0.0, 20.0));
    assert!(glyph_extent_touches_band(30.0, 10.0, 0.0, 20.0));
}

#[test]
fn scale_is_power_of_two_accepts_common_hidpi_factors() {
    assert!(scale_is_power_of_two(1.0));
    assert!(scale_is_power_of_two(2.0));
    assert!(scale_is_power_of_two(4.0));
    assert!(scale_is_power_of_two(0.5));
    assert!(!scale_is_power_of_two(1.25));
    assert!(!scale_is_power_of_two(1.5));
    assert!(!scale_is_power_of_two(3.0));
    assert!(!scale_is_power_of_two(0.0));
    assert!(!scale_is_power_of_two(-2.0));
}

#[test]
fn cache_prunes_rows_no_frame_refreshes() {
    let glyphs = two_window_glyphs(0.0);
    let origins = origins();
    let mut cache = warm_cache(&glyphs, &origins);
    assert_eq!(cache.len(), 4);
    // Never refreshed again: after enough commits the rows are pruned.
    for _ in 0..(CACHE_STALE_TICKS + 64) {
        cache.commit_frame();
    }
    assert_eq!(cache.len(), 0);
}

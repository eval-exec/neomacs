use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Default)]
pub struct GlyphRenderStats {
    pub total_frame_glyphs: usize,
    pub text_glyphs: usize,
    pub composed_glyphs: usize,
    pub unique_single_glyph_keys: usize,
    pub unique_composed_glyph_keys: usize,
    pub glyph_texture_uploads: usize,
    pub glyph_draw_calls: usize,
    pub glyph_bind_group_changes: usize,
    pub glyph_vertex_buffer_creations: usize,
    pub composed_glyph_draw_calls: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub page_evictions: usize,
    /// GPU vertex-buffer allocations this frame (arena growth events).
    /// Zero in steady state; nonzero only while arenas grow to high water.
    pub buffers_created: usize,
    /// Text rows tessellated from scratch this frame (includes bailed rows).
    pub rows_tessellated: usize,
    /// Text rows spliced verbatim from the row-reuse cache.
    pub rows_reused_verbatim: usize,
    /// Text rows spliced with an integral vertical shift.
    pub rows_reused_shifted: usize,
    /// Rows whose damage said reusable but a defensive reuse key failed.
    pub row_reuse_bails: usize,
}

impl GlyphRenderStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn log_if_enabled(&self) {
        if !stats_enabled() {
            return;
        }
        tracing::info!(
            "glyph-stats: total={} text={} composed={} unique_single={} unique_composed={} \
             uploads={} draws={} bind_changes={} vertex_bufs={} composed_draws={} hits={} misses={} evictions={} bufs_created={} rows_tess={} rows_reused={} rows_shifted={} row_bails={}",
            self.total_frame_glyphs,
            self.text_glyphs,
            self.composed_glyphs,
            self.unique_single_glyph_keys,
            self.unique_composed_glyph_keys,
            self.glyph_texture_uploads,
            self.glyph_draw_calls,
            self.glyph_bind_group_changes,
            self.glyph_vertex_buffer_creations,
            self.composed_glyph_draw_calls,
            self.cache_hits,
            self.cache_misses,
            self.page_evictions,
            self.buffers_created,
            self.rows_tessellated,
            self.rows_reused_verbatim,
            self.rows_reused_shifted,
            self.row_reuse_bails,
        );
    }
}

pub fn stats_enabled() -> bool {
    static ENABLED: AtomicBool = AtomicBool::new(false);
    static INITIALIZED: AtomicBool = AtomicBool::new(false);
    if !INITIALIZED.load(Ordering::Relaxed) {
        let enabled = std::env::var("NEOMACS_RENDER_STATS")
            .map(|v| v != "0" && !v.is_empty())
            .unwrap_or(false);
        ENABLED.store(enabled, Ordering::Relaxed);
        INITIALIZED.store(true, Ordering::Relaxed);
    }
    ENABLED.load(Ordering::Relaxed)
}

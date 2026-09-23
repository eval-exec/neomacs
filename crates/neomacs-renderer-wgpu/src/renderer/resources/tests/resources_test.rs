use super::*;

// Arena construction and the per-frame counter bookkeeping are
// device-free; actual `upload` (buffer creation + write_buffer) needs a
// live wgpu device and stays untested on this headless box.
#[test]
fn fresh_arenas_report_zero_created_buffers_per_frame() {
    let mut arenas = VertexArenas::new();
    assert_eq!(arenas.buffers_created_since_snapshot(), 0);
    arenas.begin_frame();
    arenas.begin_frame();
    assert_eq!(arenas.buffers_created_since_snapshot(), 0);
}

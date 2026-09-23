use super::SurfaceScissor;

fn parts(scissor: SurfaceScissor) -> (u32, u32, u32, u32) {
    (
        scissor.x,
        scissor.y,
        scissor.width.get(),
        scissor.height.get(),
    )
}

#[test]
fn surface_scissor_preserves_contained_rect() {
    let scissor = SurfaceScissor::intersect((8, 10, 20, 30), 100, 80).unwrap();
    assert_eq!(parts(scissor), (8, 10, 20, 30));
}

#[test]
fn surface_scissor_clips_partial_rect() {
    let scissor = SurfaceScissor::intersect((80, 70, 30, 20), 100, 80).unwrap();
    assert_eq!(parts(scissor), (80, 70, 20, 10));
}

#[test]
fn surface_scissor_rejects_empty_intersection() {
    assert_eq!(SurfaceScissor::intersect((8, 102, 8, 18), 824, 85), None);
    assert_eq!(SurfaceScissor::intersect((824, 10, 8, 18), 824, 85), None);
    assert_eq!(SurfaceScissor::intersect((8, 10, 0, 18), 824, 85), None);
}

#[test]
fn surface_scissor_handles_overflowing_extent() {
    let scissor = SurfaceScissor::intersect((90, 70, u32::MAX, u32::MAX), 100, 80).unwrap();
    assert_eq!(parts(scissor), (90, 70, 10, 10));
}

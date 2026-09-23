use super::*;

#[test]
fn partial_child_clip_becomes_one_physical_scissor_for_all_render_passes() {
    let clip = RootSurfaceRect::new(10.25, 20.5, 30.25, 40.0).unwrap();
    assert_eq!(child_scissor(clip, 1.5, 200, 200), Some((15, 30, 46, 61)));
}

#[test]
fn child_clip_outside_surface_has_no_renderable_scissor() {
    let clip = RootSurfaceRect::new(300.0, 300.0, 20.0, 20.0).unwrap();
    assert_eq!(child_scissor(clip, 1.0, 200, 200), None);
}

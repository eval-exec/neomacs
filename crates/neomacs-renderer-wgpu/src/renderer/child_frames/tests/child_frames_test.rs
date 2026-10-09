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

#[test]
fn opaque_child_preparation_needs_no_scratch() {
    let child = FrameGlyphBuffer::with_size(96.0, 64.0);
    let size = super::super::SnapshotSize::new(96, 64).unwrap();
    let prepared = PreparedChildFrame::new(&child, 1.0, size, None, None, None).unwrap();
    assert!(matches!(prepared.composition, ChildComposition::Direct));
    // Match the existing draw boundary's normalization of invalid alpha.
    assert!(PreparedChildFrame::new(&child, f32::NAN, size, None, None, None).is_ok());
}

#[test]
fn fractional_child_preparation_refuses_missing_scratch() {
    let mut child = FrameGlyphBuffer::with_size(96.0, 64.0);
    let size = super::super::SnapshotSize::new(96, 64).unwrap();
    assert_eq!(
        PreparedChildFrame::new(&child, 0.5, size, None, None, None).err(),
        Some(ChildPreparationError::MissingPicture)
    );
    child.background_alpha = 0.5;
    assert_eq!(
        PreparedChildFrame::new(&child, 1.0, size, None, None, None).err(),
        Some(ChildPreparationError::MissingPicture)
    );
}

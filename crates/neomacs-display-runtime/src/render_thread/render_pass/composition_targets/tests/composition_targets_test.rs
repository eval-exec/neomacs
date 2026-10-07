use super::super::tests::in_gpu_budget_child;
use super::*;
#[test]
fn mandatory_child_target_refuses_before_present_and_recovers() {
    if !in_gpu_budget_child(
        "render_thread::render_pass::composition_targets::tests::mandatory_child_target_refuses_before_present_and_recovers",
    ) {
        return;
    }
    let mut renderer = WgpuRenderer::new(None, 96, 64).expect("real GPU required");
    let size = SnapshotSize::new(96, 64).unwrap();
    let admitted = mandatory_child_target(&mut renderer, true, size)
        .unwrap()
        .unwrap();
    drop(admitted);
    let held = renderer
        .acquire_snapshot(SnapshotSize::new(512, 506).unwrap())
        .unwrap();
    assert!(matches!(
        mandatory_child_target(&mut renderer, true, size),
        Err(super::super::surface::FrameRenderFailure::WindowNotReady)
    ));
    assert!(
        mandatory_child_target(&mut renderer, false, size)
            .unwrap()
            .is_none()
    );
    drop(held);
    let recovered = mandatory_child_target(&mut renderer, true, size)
        .unwrap()
        .unwrap();
    assert_eq!(recovered.size(), size);
    drop(recovered);
}

#[test]
fn mandatory_resize_pair_releases_first_lease_when_second_is_refused() {
    if !in_gpu_budget_child(
        "render_thread::render_pass::composition_targets::tests::mandatory_resize_pair_releases_first_lease_when_second_is_refused",
    ) {
        return;
    }
    let mut renderer = WgpuRenderer::new(None, 96, 64).expect("GPU required");
    let size = SnapshotSize::new(96, 64).unwrap();
    let held = renderer
        .acquire_snapshot(SnapshotSize::new(512, 496).unwrap())
        .unwrap();
    assert!(matches!(
        mandatory_child_targets(&mut renderer, true, true, size),
        Err(super::super::surface::FrameRenderFailure::WindowNotReady)
    ));
    // Capacity admits exactly one scratch. If the failed pair stranded
    // its first lease, this retry would also be refused.
    let single = mandatory_child_target(&mut renderer, true, size)
        .unwrap()
        .unwrap();
    drop(single);
    drop(held);
    let (picture, mixed) = mandatory_child_targets(&mut renderer, true, true, size).unwrap();
    let picture = picture.unwrap();
    let mixed = mixed.unwrap();
    assert_ne!(
        picture.id(),
        mixed.id(),
        "cannot sample from the render attachment"
    );
    assert_eq!(picture.size(), size);
    assert_eq!(mixed.size(), size);
}

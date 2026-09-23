use super::*;

fn parent_rect(x: f32, y: f32, width: f32, height: f32) -> ParentFrameRect {
    ParentFrameRect::new(x, y, width, height).unwrap()
}

fn root_rect(x: f32, y: f32, width: f32, height: f32) -> RootSurfaceRect {
    RootSurfaceRect::new(x, y, width, height).unwrap()
}

fn placement(
    id: u64,
    presentation: u64,
    parent: Option<u64>,
    rect: ParentFrameRect,
    z: i32,
) -> PresentedFramePlacement {
    PresentedFramePlacement::new(
        DisplayFrameId::new(id),
        PresentationId::new(presentation),
        parent.map(DisplayFrameId::new),
        rect,
        z,
    )
}

#[test]
fn parent_frame_rect_exposes_shared_typed_origin_and_size() {
    let rect = ParentFrameRect::new(-5.0, -7.0, 40.0, 30.0).unwrap();

    let origin: &crate::GeometryPoint<crate::ParentFrameSpace, crate::LogicalPixels> =
        rect.origin();
    let size: &crate::GeometrySize<crate::LogicalPixels> = rect.size();

    assert_eq!((origin.x(), origin.y()), (-5.0, -7.0));
    assert_eq!((size.width(), size.height()), (40.0, 30.0));
}

#[test]
fn place_child_preserves_immediate_parent_coordinates_and_composes_nested_ancestry_once() {
    let scene = PresentedFrameScene::from_placements([
        placement(1, 10, None, parent_rect(0.0, 0.0, 800.0, 600.0), 0),
        placement(2, 11, Some(1), parent_rect(120.0, 48.0, 300.0, 200.0), 2),
        placement(3, 12, Some(2), parent_rect(7.0, 9.0, 100.0, 80.0), 4),
    ])
    .unwrap();

    let placed = scene
        .place(PlaceChildQuery::new(
            DisplayFrameId::new(3),
            PresentationId::new(12),
        ))
        .unwrap();
    assert_eq!(placed.parent_relative(), parent_rect(7.0, 9.0, 100.0, 80.0));
    assert_eq!(placed.root(), DisplayFrameId::new(1));
    assert_eq!(placed.root_relative(), root_rect(127.0, 57.0, 100.0, 80.0));
    assert_eq!(
        placed.clip_in_root(),
        PresentedClip::Rect(root_rect(127.0, 57.0, 100.0, 80.0))
    );
    assert_eq!(placed.z_path(), &[0, 2, 4]);
}

#[test]
fn place_child_clips_to_each_ancestor_and_rejects_stale_missing_and_cycles() {
    let scene = PresentedFrameScene::from_placements([
        placement(1, 10, None, parent_rect(0.0, 0.0, 100.0, 100.0), 0),
        placement(2, 11, Some(1), parent_rect(80.0, 80.0, 50.0, 50.0), 1),
    ])
    .unwrap();
    let placed = scene
        .place(PlaceChildQuery::new(
            DisplayFrameId::new(2),
            PresentationId::new(11),
        ))
        .unwrap();
    assert_eq!(placed.root_relative(), root_rect(80.0, 80.0, 50.0, 50.0));
    assert_eq!(
        placed.clip_in_root(),
        PresentedClip::Rect(root_rect(80.0, 80.0, 20.0, 20.0))
    );
    assert!(matches!(
        scene.place(PlaceChildQuery::new(
            DisplayFrameId::new(2),
            PresentationId::new(9)
        )),
        Err(PlaceChildError::StalePresentation { .. })
    ));
    assert_eq!(
        scene.place(PlaceChildQuery::new(
            DisplayFrameId::new(9),
            PresentationId::new(9)
        )),
        Err(PlaceChildError::MissingFrame(DisplayFrameId::new(9)))
    );
    assert!(matches!(
        PresentedFrameScene::from_placements([
            placement(1, 1, Some(2), parent_rect(0.0, 0.0, 10.0, 10.0), 0),
            placement(2, 2, Some(1), parent_rect(0.0, 0.0, 10.0, 10.0), 0),
        ]),
        Err(PlaceChildError::AncestryCycle(_))
    ));
}

#[test]
fn place_child_preserves_negative_parent_origin_and_clips_it_to_the_root() {
    let scene = PresentedFrameScene::from_placements([
        placement(1, 10, None, parent_rect(0.0, 0.0, 100.0, 100.0), 0),
        PresentedFramePlacement::new(
            DisplayFrameId::new(2),
            PresentationId::new(11),
            Some(DisplayFrameId::new(1)),
            ParentFrameRect::new(-5.0, -7.0, 40.0, 30.0).unwrap(),
            1,
        ),
    ])
    .unwrap();

    let placed = scene
        .place(PlaceChildQuery::new(
            DisplayFrameId::new(2),
            PresentationId::new(11),
        ))
        .unwrap();

    assert_eq!(
        (placed.parent_relative().x(), placed.parent_relative().y()),
        (-5.0, -7.0)
    );
    assert_eq!(
        (placed.root_relative().x(), placed.root_relative().y()),
        (-5.0, -7.0)
    );
    assert_eq!(
        placed.clip_in_root(),
        PresentedClip::Rect(root_rect(0.0, 0.0, 35.0, 23.0))
    );
}

#[test]
fn scene_rejects_nonzero_root_origin() {
    let root = PresentedFramePlacement::new(
        DisplayFrameId::new(1),
        PresentationId::new(10),
        None,
        ParentFrameRect::new(-1.0, 0.0, 100.0, 100.0).unwrap(),
        0,
    );

    assert!(matches!(
        PresentedFrameScene::from_placements([root]),
        Err(PlaceChildError::InvalidRootOrigin(frame)) if frame == DisplayFrameId::new(1)
    ));
}

#[test]
fn parent_placement_deserialization_rejects_invalid_extents() {
    let invalid = r#"{"x":-1.0,"y":-2.0,"width":-3.0,"height":4.0}"#;

    assert!(serde_json::from_str::<ParentFrameRect>(invalid).is_err());
}

#[test]
fn place_child_reports_overflow_in_derived_root_coordinates() {
    let scene = PresentedFrameScene::from_placements([
        placement(1, 10, None, parent_rect(0.0, 0.0, 100.0, 100.0), 0),
        PresentedFramePlacement::new(
            DisplayFrameId::new(2),
            PresentationId::new(11),
            Some(DisplayFrameId::new(1)),
            ParentFrameRect::new(f32::MAX, 0.0, 10.0, 10.0).unwrap(),
            1,
        ),
        PresentedFramePlacement::new(
            DisplayFrameId::new(3),
            PresentationId::new(12),
            Some(DisplayFrameId::new(2)),
            ParentFrameRect::new(f32::MAX, 0.0, 10.0, 10.0).unwrap(),
            2,
        ),
    ])
    .unwrap();

    assert_eq!(
        scene.place(PlaceChildQuery::new(
            DisplayFrameId::new(3),
            PresentationId::new(12),
        )),
        Err(PlaceChildError::InvalidDerivedPlacement(
            DisplayFrameId::new(3)
        ))
    );
}

#[test]
fn fully_clipped_descendant_stays_empty_through_remaining_ancestry() {
    let scene = PresentedFrameScene::from_placements([
        placement(1, 10, None, parent_rect(0.0, 0.0, 100.0, 100.0), 0),
        placement(2, 11, Some(1), parent_rect(150.0, 0.0, 40.0, 40.0), 1),
        placement(3, 12, Some(2), parent_rect(0.0, 0.0, 20.0, 20.0), 2),
    ])
    .unwrap();

    let placed = scene
        .place(PlaceChildQuery::new(
            DisplayFrameId::new(3),
            PresentationId::new(12),
        ))
        .unwrap();
    assert_eq!(placed.clip_in_root(), PresentedClip::Empty);
}

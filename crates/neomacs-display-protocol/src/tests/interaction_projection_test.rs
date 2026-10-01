use super::*;

fn presentation() -> PresentationId {
    PresentationId::new(7)
}

fn surface(x: f32, y: f32) -> GeometryPoint<RootSurfaceSpace, LogicalPixels> {
    GeometryPoint::<RootSurfaceSpace, LogicalPixels>::from_px(x, y).expect("a valid surface point")
}

fn content(x: f32, y: f32) -> GeometryPoint<PresentationFrameSpace, LogicalPixels> {
    GeometryPoint::<PresentationFrameSpace, LogicalPixels>::from_px(x, y)
        .expect("a valid content point")
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> GeometryRect<RootSurfaceSpace, LogicalPixels> {
    GeometryRect::<RootSurfaceSpace, LogicalPixels>::new(x, y, w, h).expect("a valid surface rect")
}

fn window(id: i64) -> LiveDisplayWindowId {
    LiveDisplayWindowId::try_from(crate::types::DisplayWindowId::new(id)).expect("a live window id")
}

#[test]
fn a_settled_frame_maps_a_surface_point_to_itself() {
    // Nothing is moving, so the pixel under the pointer is exactly the pixel
    // the destination presentation drew there.
    let projection = InteractionProjection::settled(presentation());
    let mapped = projection
        .map(surface(120.0, 48.0))
        .expect("a mapped point");
    assert_eq!(mapped.presentation(), presentation());
    assert_eq!((mapped.x(), mapped.y()), (120.0, 48.0));
}

#[test]
fn a_pane_drawn_away_from_its_destination_maps_the_point_back() {
    // The pane is drawn at x=200 on the surface but shows content that belongs
    // at x=0 in the destination: mid-morph of a horizontal split. A click at
    // surface x=260 is a click 60px into the pane's content.
    let pane = PaneProjection::new(window(3), rect(200.0, 0.0, 400.0, 600.0), content(0.0, 0.0))
        .expect("a valid pane projection");
    let projection = InteractionProjection::new(presentation(), vec![pane]);
    let mapped = projection
        .map(surface(260.0, 30.0))
        .expect("a mapped point");
    assert_eq!(
        (mapped.x(), mapped.y()),
        (60.0, 30.0),
        "the raw surface point would have selected the wrong column"
    );
}

#[test]
fn a_point_outside_every_moving_pane_maps_by_identity() {
    // The area no morphing pane covers is drawn straight from the destination,
    // so it needs no correction.
    let pane = PaneProjection::new(window(3), rect(200.0, 0.0, 400.0, 600.0), content(0.0, 0.0))
        .expect("a valid pane projection");
    let projection = InteractionProjection::new(presentation(), vec![pane]);
    let mapped = projection.map(surface(50.0, 30.0)).expect("a mapped point");
    assert_eq!((mapped.x(), mapped.y()), (50.0, 30.0));
}

#[test]
fn the_first_pane_containing_the_point_wins() {
    // Panes arrive in destination z-order, so an overlapping child frame's pane
    // must take the point before the pane beneath it.
    let above = PaneProjection::new(window(9), rect(0.0, 0.0, 800.0, 600.0), content(100.0, 0.0))
        .expect("a valid pane projection");
    let below = PaneProjection::new(window(3), rect(0.0, 0.0, 800.0, 600.0), content(0.0, 0.0))
        .expect("a valid pane projection");
    let projection = InteractionProjection::new(presentation(), vec![above, below]);
    let mapped = projection.map(surface(10.0, 10.0)).expect("a mapped point");
    assert_eq!(mapped.x(), 110.0, "the topmost pane's transform applied");
}

#[test]
fn a_pane_is_half_open_so_adjacent_panes_never_both_claim_a_point() {
    // Two panes of a vertical split share the edge at x=400. The pixel at 400
    // belongs to the right pane only; if both claimed it, which one answered
    // would depend on iteration order.
    let left = PaneProjection::new(window(1), rect(0.0, 0.0, 400.0, 600.0), content(0.0, 0.0))
        .expect("a valid pane projection");
    let right = PaneProjection::new(
        window(2),
        rect(400.0, 0.0, 400.0, 600.0),
        content(400.0, 0.0),
    )
    .expect("a valid pane projection");
    let projection = InteractionProjection::new(presentation(), vec![left, right]);

    assert_eq!(
        projection.map(surface(399.0, 0.0)).expect("mapped").x(),
        399.0
    );
    assert_eq!(
        projection.map(surface(400.0, 0.0)).expect("mapped").x(),
        400.0,
        "the shared edge belongs to the right pane, and its content is at 400"
    );
}

#[test]
fn the_mapped_point_names_the_presentation_it_belongs_to() {
    // A hit resolved against a different presentation than the one the point
    // was mapped through would be answering about pixels nobody saw.
    let projection = InteractionProjection::settled(presentation());
    assert_eq!(
        projection
            .map(surface(0.0, 0.0))
            .expect("mapped")
            .presentation(),
        presentation()
    );
}

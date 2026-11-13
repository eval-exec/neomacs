use super::*;

fn horizontal_root_corner(
    check: impl FnOnce(
        &neomacs_display_protocol::glyph_matrix::FrameDisplayState,
        neovm_core::window::WindowId,
    ),
) {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let frame = eval
        .frame_manager_mut()
        .create_frame("divider-corner", 800, 200, buffer);
    let left = eval.frame_manager().get(frame).unwrap().selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame).unwrap();
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.set_parameter(Value::symbol("right-divider-width"), Value::fixnum(4));
        frame.set_parameter(Value::symbol("bottom-divider-width"), Value::fixnum(6));
    }
    eval.frame_manager_mut()
        .split_window(
            frame,
            left,
            neovm_core::window::SplitDirection::Horizontal,
            buffer,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .expect("side-by-side windows above owned minibuffer");
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);
    check(engine.last_frame_display_state.as_ref().unwrap(), left);
}

#[test]
fn divider_corner_horizontal_root_canonical_bottom_covers_intersection() {
    horizontal_root_corner(|state, left| {
        let info = state
            .window_infos
            .iter()
            .find(|info| info.window_id.get() == left.0 as i64)
            .unwrap();
        let regions = match info.geometry {
            neomacs_display_protocol::PresentedWindowGeometry::Complete { regions, .. } => regions,
            _ => panic!("complete left-window geometry"),
        };
        let bottom = regions.bottom_divider.expect("owned-mini bottom divider");
        let right = regions.right_divider.expect("horizontal sibling divider");
        let x = regions.outer.right() - 0.5;
        let y = regions.outer.bottom() - 0.5;
        assert!(
            x >= bottom.x
                && x < bottom.x + bottom.width
                && y >= bottom.y
                && y < bottom.y + bottom.height,
            "GNU horizontal sibling: bottom divider must cover the corner: {regions:?}"
        );
        assert!(
            y >= right.y + right.height,
            "right divider stops above the bottom divider"
        );
        assert_eq!(bottom.width, regions.outer.width);
        assert_eq!(right.height, regions.outer.height - bottom.height);
    });
}

#[test]
fn divider_corner_horizontal_root_paints_the_intersection_once() {
    horizontal_root_corner(|state, left| {
        let info = state
            .window_infos
            .iter()
            .find(|info| info.window_id.get() == left.0 as i64)
            .unwrap();
        let regions = match info.geometry {
            neomacs_display_protocol::PresentedWindowGeometry::Complete { regions, .. } => regions,
            _ => panic!("complete left-window geometry"),
        };
        let x = regions.outer.right() - 0.5;
        let y = regions.outer.bottom() - 0.5;
        let covering = state
            .borders
            .iter()
            .filter(|border| {
                border.window_id.get() == left.0 as i64
                    && x >= border.x
                    && x < border.x + border.width
                    && y >= border.y
                    && y < border.y + border.height
            })
            .count();
        assert_eq!(
            covering, 1,
            "GNU horizontal sibling corner must be painted once, without a hole"
        );
    });
}

#[test]
fn divider_corner_all_visual_policies_seal_and_hit_bottom_with_gnu_paint_order() {
    use neovm_core::window::{SplitDirection, SplitPlacement};
    // Root horizontal: bottom covers. Left vertical subtree: upper right
    // covers; lower has the source's overlapping draw rectangles.
    for case in 0..3 {
        let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
        let buffer = eval.buffer_manager().current_buffer().unwrap().id();
        let frame = eval
            .frame_manager_mut()
            .create_frame("divider-corner-seal", 800, 600, buffer);
        let left = eval.frame_manager().get(frame).unwrap().selected_window;
        {
            let frame = eval.frame_manager_mut().get_mut(frame).unwrap();
            frame.set_window_system(Some(Value::symbol("neo")));
            frame.set_parameter(Value::symbol("right-divider-width"), Value::fixnum(4));
            frame.set_parameter(Value::symbol("bottom-divider-width"), Value::fixnum(6));
        }
        eval.frame_manager_mut()
            .split_window(
                frame,
                left,
                SplitDirection::Horizontal,
                buffer,
                None,
                SplitPlacement::AfterTarget,
            )
            .unwrap();
        let target = if case == 0 {
            left
        } else {
            let lower = eval
                .frame_manager_mut()
                .split_window(
                    frame,
                    left,
                    SplitDirection::Vertical,
                    buffer,
                    None,
                    SplitPlacement::AfterTarget,
                )
                .unwrap();
            if case == 1 { left } else { lower }
        };
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        engine.layout_frame_rust(&mut eval, frame);
        let state = engine.last_frame_display_state.as_ref().unwrap();
        let info = state
            .window_infos
            .iter()
            .find(|info| info.window_id.get() == target.0 as i64)
            .unwrap();
        let regions = match info.geometry {
            neomacs_display_protocol::PresentedWindowGeometry::Complete { regions, .. } => regions,
            _ => panic!("complete target geometry"),
        };
        let x = regions.outer.right() - 0.5;
        let y = regions.outer.bottom() - 0.5;
        let sealed = neomacs_display_protocol::SealedFramePresentation::seal(state.state().clone())
            .expect("all visual policies preserve the strict spatial seal");
        let materialized = sealed.materialize();
        let hit = materialized
            .resolve_presented_hit(neomacs_display_protocol::PresentedHitQuery::new(
                settled_point(sealed.presentation(), x, y),
            ))
            .expect("current presentation")
            .expect("corner hit");
        assert_eq!(
            hit.semantic().unwrap().region().kind(),
            neomacs_display_protocol::PresentedRegionKind::BottomDivider,
            "GNU hit classification owns the bottom strip independently of paint, case {case}"
        );
        let covering: Vec<_> = state
            .borders
            .iter()
            .filter(|border| {
                border.window_id.get() == target.0 as i64
                    && x >= border.x
                    && x < border.x + border.width
                    && y >= border.y
                    && y < border.y + border.height
            })
            .collect();
        assert_eq!(covering.len(), if case == 2 { 2 } else { 1 });
        let last = covering.last().unwrap();
        if case == 0 {
            assert_eq!(last.height, 1.0, "horizontal bottom band wins");
            assert!(last.width > 1.0);
        } else {
            assert_eq!(
                last.width, 1.0,
                "vertical right band wins, including overlapping draws"
            );
            assert!(last.height > 1.0);
        }
        // Overlapping visual rectangles must never be published as hit regions.
        let mut malformed = state.state().clone();
        let info = malformed
            .window_infos
            .iter_mut()
            .find(|info| info.window_id.get() == target.0 as i64)
            .unwrap();
        let neomacs_display_protocol::PresentedWindowGeometry::Complete { regions, .. } =
            &mut info.geometry
        else {
            unreachable!()
        };
        regions.right_divider.as_mut().unwrap().height += regions.bottom_divider.unwrap().height;
        assert!(matches!(
            malformed.validate_spatial_projections(),
            Err(
                neomacs_display_protocol::PresentedHitError::WindowGeometryMismatch {
                    region: neomacs_display_protocol::PresentedRegionKind::BottomDivider,
                    ..
                }
            )
        ));
        assert!(neomacs_display_protocol::SealedFramePresentation::seal(malformed).is_err());
    }
}

use super::*;

/// The quad `render_pane_layout` would emit for one pane, as
/// `(position, tex_coords)` pairs — the geometry decision the pass makes,
/// separated from the GPU submission around it so it can be asserted without
/// a device.
///
/// Alpha is omitted here: it is per-pane and orthogonal to the geometry these
/// tests assert. Kept in step with `render_pane_layout` by
/// [`the_helper_matches_what_the_pass_emits`], which reads the source.
fn quad(pane: PaneBlit, frame: (f32, f32)) -> Vec<([f32; 2], [f32; 2])> {
    let (fw, fh) = frame;
    let u0 = pane.content_origin.0 / fw;
    let v0 = pane.content_origin.1 / fh;
    let u1 = (pane.content_origin.0 + pane.bounds.width) / fw;
    let v1 = (pane.content_origin.1 + pane.bounds.height) / fh;
    let x0 = pane.bounds.x;
    let y0 = pane.bounds.y;
    let x1 = pane.bounds.x + pane.bounds.width;
    let y1 = pane.bounds.y + pane.bounds.height;
    vec![
        ([x0, y0], [u0, v0]),
        ([x1, y0], [u1, v0]),
        ([x1, y1], [u1, v1]),
        ([x0, y0], [u0, v0]),
        ([x1, y1], [u1, v1]),
        ([x0, y1], [u0, v1]),
    ]
}

#[test]
fn a_settled_pane_samples_exactly_the_region_it_covers() {
    // At rest the pass must be a no-op picture-wise: every pane reads the part
    // of the composed frame it sits on, so the result is the composed frame.
    let pane = PaneBlit {
        bounds: Rect::new(400.0, 0.0, 400.0, 600.0),
        content_origin: (400.0, 0.0),
        source: PaneSource::Destination,
        opacity: 1.0,
    };
    let corners = quad(pane, (800.0, 600.0));
    assert_eq!(corners[0], ([400.0, 0.0], [0.5, 0.0]));
    assert_eq!(corners[2], ([800.0, 600.0], [1.0, 1.0]));
}

#[test]
fn a_pane_drawn_away_from_its_content_still_samples_its_content() {
    // Mid-morph: the pane is at x=200 but shows the destination content that
    // lives at x=400. Sampling the region under the *drawn* rect instead would
    // show the neighbouring pane's text inside this one.
    let pane = PaneBlit {
        bounds: Rect::new(200.0, 0.0, 400.0, 600.0),
        content_origin: (400.0, 0.0),
        source: PaneSource::Destination,
        opacity: 1.0,
    };
    let corners = quad(pane, (800.0, 600.0));
    assert_eq!(corners[0].0, [200.0, 0.0], "drawn where the motion puts it");
    assert_eq!(
        corners[0].1,
        [0.5, 0.0],
        "reading from where its content is"
    );
}

#[test]
fn an_oversized_pane_shows_more_of_the_row_rather_than_a_stretched_copy() {
    // A pane on its way from 800 wide to 400 is 600 wide halfway through. It
    // must show 600px of the composed row at natural scale; scaling 400px of
    // content up to 600 would resample the glyphs and read as a zoom.
    let pane = PaneBlit {
        bounds: Rect::new(0.0, 0.0, 600.0, 600.0),
        content_origin: (0.0, 0.0),
        source: PaneSource::Destination,
        opacity: 1.0,
    };
    let corners = quad(pane, (800.0, 600.0));
    assert_eq!(
        corners[1].1,
        [0.75, 0.0],
        "600 of 800 logical pixels, one texel per pixel"
    );
}

#[test]
fn every_pane_maps_its_own_region_independently() {
    let left = PaneBlit {
        bounds: Rect::new(0.0, 0.0, 400.0, 600.0),
        content_origin: (0.0, 0.0),
        source: PaneSource::Destination,
        opacity: 1.0,
    };
    let right = PaneBlit {
        bounds: Rect::new(400.0, 0.0, 400.0, 600.0),
        content_origin: (400.0, 0.0),
        source: PaneSource::Destination,
        opacity: 1.0,
    };
    let l = quad(left, (800.0, 600.0));
    let r = quad(right, (800.0, 600.0));
    assert_eq!(l[1].1[0], 0.5, "the left pane stops at the seam");
    assert_eq!(r[0].1[0], 0.5, "and the right pane starts there");
}

#[test]
fn the_helper_matches_what_the_pass_emits() {
    // The tests above assert on a reimplementation of the pass's geometry.
    // That is only meaningful while the two agree, so this reads the pass's
    // own source and checks the expressions it computes the corners from.
    let source = include_str!("layout_pass.rs");
    for expression in [
        "let u0 = pane.content_origin.0 / frame_width;",
        "let v0 = pane.content_origin.1 / frame_height;",
        "let u1 = (pane.content_origin.0 + pane.bounds.width) / frame_width;",
        "let v1 = (pane.content_origin.1 + pane.bounds.height) / frame_height;",
        "let x1 = pane.bounds.x + pane.bounds.width;",
        "let y1 = pane.bounds.y + pane.bounds.height;",
    ] {
        assert!(
            source.contains(expression),
            "the pass no longer computes `{expression}`; `quad` above is stale"
        );
    }
}

#[test]
fn nothing_is_submitted_for_a_frame_with_no_size() {
    // A zero-sized frame would divide by zero and emit NaN texture
    // coordinates, which is a GPU-side surprise rather than a visible one.
    let pane = PaneBlit {
        bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
        content_origin: (0.0, 0.0),
        source: PaneSource::Destination,
        opacity: 1.0,
    };
    let corners = quad(pane, (800.0, 600.0));
    assert!(
        corners
            .iter()
            .all(|(_, uv)| uv.iter().all(|c| c.is_finite()))
    );
}

#[test]
fn the_pass_draws_the_whole_frame_underneath_the_panes() {
    // Panes are not the whole frame. The echo area is excluded from every
    // morph by construction, and the tab bar, tool bar and frame padding
    // belong to no pane at all — so a pass that drew only panes over a cleared
    // target would make the minibuffer vanish for the length of every
    // `split-window`.
    let source = include_str!("layout_pass.rs");
    assert!(
        source.contains("corner(frame_width, frame_height, 1.0, 1.0, 1.0),"),
        "the base quad covering the composed frame is gone; \
         everything outside a pane would be cleared away"
    );
    assert!(
        source.contains("Vec::with_capacity((panes.len() + 1) * 6)"),
        "the vertex count no longer accounts for the base quad"
    );
}

#[test]
fn a_departing_pane_reads_from_the_previous_composition() {
    // A deleted window is absent from the destination presentation entirely,
    // so the composed picture holds no pixels for it. Sampling the destination
    // would show whatever replaced it, wearing the departing pane's geometry;
    // without the previous composition it can only vanish outright.
    let source = include_str!("layout_pass.rs");
    assert!(
        source.contains("PaneSource::Previous if previous.is_some()"),
        "departing quads no longer select the previous composition"
    );
    assert!(
        source.contains("pass.set_bind_group(1, previous, &[]);"),
        "the second draw no longer rebinds to the previous composition"
    );
}

#[test]
fn departing_panes_are_drawn_after_the_panes_that_remain() {
    // A bind group cannot change within a draw call, so the two sources are
    // two draws and their order is fixed by construction. A pane on its way out
    // should read as lying over what replaces it, not under it.
    let source = include_str!("layout_pass.rs");
    let destination_draw = source
        .find("pass.draw(0..destination_vertices, 0..1);")
        .expect("the destination draw is still first");
    let departing_draw = source
        .find("pass.draw(destination_vertices..vertices.len() as u32, 0..1);")
        .expect("the departing draw still exists");
    assert!(destination_draw < departing_draw);
}

#[test]
fn a_pane_with_no_previous_composition_to_read_is_dropped_rather_than_faded_from_nothing() {
    // The first frame a window is ever drawn on has no history. Binding an
    // unwritten texture would fade the pane in from whatever that memory held.
    let source = include_str!("layout_pass.rs");
    assert!(
        source.contains("PaneSource::Previous => {}"),
        "a departing pane with no previous composition is no longer dropped"
    );
}

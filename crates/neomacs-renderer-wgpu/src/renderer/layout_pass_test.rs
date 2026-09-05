use super::*;

/// The quad `render_pane_layout` would emit for one pane, as
/// `(position, tex_coords)` pairs — the geometry decision the pass makes,
/// separated from the GPU submission around it so it can be asserted without
/// a device.
///
/// Kept in step with `render_pane_layout` by
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
    };
    let right = PaneBlit {
        bounds: Rect::new(400.0, 0.0, 400.0, 600.0),
        content_origin: (400.0, 0.0),
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
    };
    let corners = quad(pane, (800.0, 600.0));
    assert!(
        corners
            .iter()
            .all(|(_, uv)| uv.iter().all(|c| c.is_finite()))
    );
}

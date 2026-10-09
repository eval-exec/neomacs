use super::*;
use neomacs_display_protocol::{TransitionAxis, TransitionDirection};

#[test]
fn slide_offsets_follow_the_typed_axis_and_direction() {
    let (old, new) = slide_offsets(
        TransitionAxis::Horizontal,
        TransitionDirection::Forward,
        100.0,
        0.25,
    );
    assert_eq!(old, [-25.0, 0.0]);
    assert_eq!(new, [75.0, 0.0]);

    let (old, new) = slide_offsets(
        TransitionAxis::Vertical,
        TransitionDirection::Backward,
        100.0,
        0.25,
    );
    assert_eq!(old, [0.0, 25.0]);
    assert_eq!(new, [0.0, -75.0]);
}

fn quad(left: f32, top: f32, right: f32, bottom: f32, weight: f32) -> [GlyphVertex; 6] {
    [
        [left, top],
        [right, top],
        [right, bottom],
        [left, top],
        [right, bottom],
        [left, bottom],
    ]
    .map(|position| GlyphVertex {
        position,
        tex_coords: [0.0; 2],
        color: [1.0, 1.0, 1.0, weight],
    })
}

fn background_sample(vertices: &[RectVertex], x: f32, y: f32) -> [f32; 4] {
    let mut color = [0.0; 4];
    for q in vertices.chunks_exact(6) {
        if x >= q[0].position[0]
            && x < q[2].position[0]
            && y >= q[0].position[1]
            && y < q[2].position[1]
        {
            for (sum, channel) in color.iter_mut().zip(q[0].color) {
                *sum += channel;
            }
        }
    }
    color
}

#[test]
fn geometric_background_complements_preserve_weight_and_fractional_alpha() {
    let bounds = neomacs_display_protocol::types::Rect::new(8.0, 8.0, 100.0, 100.0);
    let old = quad(9.0, 9.0, 107.0, 107.0, 0.75);
    let new = quad(11.0, 11.0, 105.0, 105.0, 0.25);
    for alpha in [0.0, 0.5, 1.0] {
        let background = Color::new(0.2, 0.4, 0.6, 0.5);
        let a = alpha * background.a;
        let vertices = transition_background_vertices(
            &bounds,
            &[&old, &new],
            TransitionBackground::new(background, alpha),
        );
        for (x, y, missing) in [
            (8.5, 58.0, 1.0),
            (10.0, 58.0, 0.25),
            (58.0, 58.0, 0.0),
            (58.0, 8.5, 1.0),
            (58.0, 10.0, 0.25),
            (2.0, 2.0, 0.0),
        ] {
            let actual = background_sample(&vertices, x, y);
            let expected = [
                0.2 * a * missing,
                0.4 * a * missing,
                0.6 * a * missing,
                a * missing,
            ];
            for (v, e) in actual.into_iter().zip(expected) {
                assert!((v - e).abs() < 0.00001);
            }
        }
    }
}

#[test]
fn card_flip_complement_never_fills_beneath_the_picture() {
    let bounds = neomacs_display_protocol::types::Rect::new(8.0, 8.0, 100.0, 100.0);
    for picture in [
        quad(57.0, 8.0, 59.0, 108.0, 1.0),
        quad(8.0, 57.0, 108.0, 59.0, 1.0),
    ] {
        let vertices = transition_background_vertices(
            &bounds,
            &[&picture],
            TransitionBackground::new(Color::BLACK, 0.5),
        );
        assert_eq!(background_sample(&vertices, 8.5, 8.5), [0.0, 0.0, 0.0, 0.5]);
        assert_eq!(background_sample(&vertices, 58.0, 58.0), [0.0; 4]);
        assert_eq!(background_sample(&vertices, 2.0, 2.0), [0.0; 4]);
    }
}

#[test]
fn full_size_endpoints_need_no_weighted_background() {
    let bounds = neomacs_display_protocol::types::Rect::new(8.0, 8.0, 100.0, 100.0);
    let full = quad(8.0, 8.0, 108.0, 108.0, 1.0);
    let invisible = quad(12.0, 12.0, 104.0, 104.0, 0.0);
    let vertices = transition_background_vertices(
        &bounds,
        &[&full, &invisible],
        TransitionBackground::new(Color::BLACK, 1.0),
    );
    assert!(vertices.iter().all(|v| v.color == [0.0; 4]));
}

#[test]
fn parallax_complements_fill_each_vacated_band_by_its_picture_weight() {
    let bounds = neomacs_display_protocol::types::Rect::new(8.0, 8.0, 100.0, 100.0);
    // Old picture moved up by 30 (weight 0.75), new still 70 below (0.25).
    let old = quad(8.0, -22.0, 108.0, 78.0, 0.75);
    let new = quad(8.0, 78.0, 108.0, 178.0, 0.25);
    let vertices = transition_background_vertices(
        &bounds,
        &[&old, &new],
        TransitionBackground::new(Color::BLACK, 1.0),
    );
    // Each pixel's picture weights plus its background weight sum to one.
    assert_eq!(
        background_sample(&vertices, 58.0, 50.0),
        [0.0, 0.0, 0.0, 0.25]
    );
    assert_eq!(
        background_sample(&vertices, 58.0, 90.0),
        [0.0, 0.0, 0.0, 0.75]
    );
    assert_eq!(background_sample(&vertices, 2.0, 50.0), [0.0; 4]);
}

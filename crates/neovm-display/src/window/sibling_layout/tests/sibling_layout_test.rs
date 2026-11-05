use super::*;

fn flexible(bounds: Rect) -> ChildExtent {
    ChildExtent {
        bounds,
        fixed_width_cols: 0,
        fixed_height_lines: 0,
    }
}

fn fixed_width(bounds: Rect, cols: usize) -> ChildExtent {
    ChildExtent {
        bounds,
        fixed_width_cols: cols,
        fixed_height_lines: 0,
    }
}

fn fixed_height(bounds: Rect, lines: usize) -> ChildExtent {
    ChildExtent {
        bounds,
        fixed_width_cols: 0,
        fixed_height_lines: lines,
    }
}

#[test]
fn no_children_lay_out_nothing() {
    assert!(sibling_bounds(Rect::new(0.0, 0.0, 80.0, 24.0), &[]).is_empty());
}

#[test]
fn a_lone_child_inherits_the_parent_rectangle_unrounded() {
    let parent = Rect::new(0.5, 1.5, 80.25, 24.75);
    let children = [flexible(Rect::new(0.0, 0.0, 1.0, 1.0))];
    assert_eq!(sibling_bounds(parent, &children), vec![parent]);
}

#[test]
fn siblings_differing_in_x_are_a_horizontal_split() {
    let children = [
        flexible(Rect::new(0.0, 0.0, 40.0, 24.0)),
        flexible(Rect::new(40.0, 0.0, 40.0, 24.0)),
    ];
    assert_eq!(
        detect_direction(&children),
        Some(SplitDirection::Horizontal)
    );
}

#[test]
fn siblings_sharing_an_x_are_a_vertical_split() {
    let children = [
        flexible(Rect::new(0.0, 0.0, 80.0, 12.0)),
        flexible(Rect::new(0.0, 12.0, 80.0, 12.0)),
    ];
    assert_eq!(detect_direction(&children), Some(SplitDirection::Vertical));
}

#[test]
fn a_single_child_has_no_direction_to_detect() {
    assert_eq!(
        detect_direction(&[flexible(Rect::new(0.0, 0.0, 80.0, 24.0))]),
        None
    );
}

#[test]
fn children_tile_the_parent_without_gaps_or_overlap() {
    let parent = Rect::new(10.0, 20.0, 81.0, 25.0);
    let children = [
        flexible(Rect::new(10.0, 20.0, 27.0, 25.0)),
        flexible(Rect::new(37.0, 20.0, 27.0, 25.0)),
        flexible(Rect::new(64.0, 20.0, 27.0, 25.0)),
    ];
    let laid_out = sibling_bounds(parent, &children);

    let mut edge = parent.x;
    for rect in &laid_out {
        assert_eq!(rect.x, edge, "children must abut: {laid_out:?}");
        edge = rect.right();
    }
    assert_eq!(edge, parent.right(), "children must fill the parent");
}

#[test]
fn an_indivisible_width_gives_the_extra_pixel_to_the_leading_child() {
    // 81 pixels over 2 children: 41 then 40, never 40.5 twice.
    let parent = Rect::new(0.0, 0.0, 81.0, 24.0);
    let children = [
        flexible(Rect::new(0.0, 0.0, 0.0, 24.0)),
        flexible(Rect::new(1.0, 0.0, 0.0, 24.0)),
    ];
    let widths: Vec<f32> = sibling_bounds(parent, &children)
        .iter()
        .map(|rect| rect.width)
        .collect();
    assert_eq!(widths, vec![41.0, 40.0]);
}

#[test]
fn a_fixed_width_child_keeps_its_width_while_its_sibling_absorbs_the_rest() {
    let parent = Rect::new(0.0, 0.0, 100.0, 24.0);
    let children = [
        fixed_width(Rect::new(0.0, 0.0, 30.0, 24.0), 30),
        flexible(Rect::new(30.0, 0.0, 50.0, 24.0)),
    ];
    let widths: Vec<f32> = sibling_bounds(parent, &children)
        .iter()
        .map(|rect| rect.width)
        .collect();
    assert_eq!(widths, vec![30.0, 70.0]);
}

#[test]
fn a_fixed_height_child_keeps_its_height_while_its_sibling_absorbs_the_rest() {
    let parent = Rect::new(0.0, 0.0, 80.0, 40.0);
    let children = [
        flexible(Rect::new(0.0, 0.0, 80.0, 20.0)),
        fixed_height(Rect::new(0.0, 20.0, 80.0, 4.0), 4),
    ];
    let heights: Vec<f32> = sibling_bounds(parent, &children)
        .iter()
        .map(|rect| rect.height)
        .collect();
    assert_eq!(heights, vec![36.0, 4.0]);
}

#[test]
fn flexible_children_keep_their_proportions() {
    // 30:60 of the 90 flexible pixels, held across a grow to 180.
    let parent = Rect::new(0.0, 0.0, 180.0, 24.0);
    let children = [
        flexible(Rect::new(0.0, 0.0, 30.0, 24.0)),
        flexible(Rect::new(30.0, 0.0, 60.0, 24.0)),
    ];
    let widths: Vec<f32> = sibling_bounds(parent, &children)
        .iter()
        .map(|rect| rect.width)
        .collect();
    assert_eq!(widths, vec![60.0, 120.0]);
}

#[test]
fn a_fixed_child_that_still_fits_leaves_only_the_slack_to_its_sibling() {
    // 60 fixed pixels inside an 80-pixel parent: the fixed child is honored
    // and its sibling is squeezed into what is left, however little.
    let parent = Rect::new(0.0, 0.0, 80.0, 24.0);
    let children = [
        fixed_width(Rect::new(0.0, 0.0, 60.0, 24.0), 60),
        flexible(Rect::new(60.0, 0.0, 60.0, 24.0)),
    ];
    let widths: Vec<f32> = sibling_bounds(parent, &children)
        .iter()
        .map(|rect| rect.width)
        .collect();
    assert_eq!(widths, vec![60.0, 20.0]);
}

#[test]
fn fixed_children_that_overfill_the_parent_fall_back_to_an_even_split() {
    // Two 60-pixel fixed children cannot both fit in 80 pixels, so neither
    // keeps its size: the parent is shared evenly instead.
    let parent = Rect::new(0.0, 0.0, 80.0, 24.0);
    let children = [
        fixed_width(Rect::new(0.0, 0.0, 60.0, 24.0), 60),
        fixed_width(Rect::new(60.0, 0.0, 60.0, 24.0), 60),
    ];
    let widths: Vec<f32> = sibling_bounds(parent, &children)
        .iter()
        .map(|rect| rect.width)
        .collect();
    assert_eq!(widths, vec![40.0, 40.0]);
}

#[test]
fn children_with_no_size_yet_are_split_evenly() {
    let parent = Rect::new(0.0, 0.0, 80.0, 24.0);
    let children = [
        flexible(Rect::new(0.0, 0.0, 0.0, 24.0)),
        fixed_width(Rect::new(20.0, 0.0, 20.0, 24.0), 20),
        flexible(Rect::new(40.0, 0.0, 0.0, 24.0)),
    ];
    let widths: Vec<f32> = sibling_bounds(parent, &children)
        .iter()
        .map(|rect| rect.width)
        .collect();
    assert_eq!(widths, vec![30.0, 20.0, 30.0]);
}

#[test]
fn every_child_gets_exactly_one_rectangle() {
    let parent = Rect::new(0.0, 0.0, 80.0, 24.0);
    for count in 0..8usize {
        let children: Vec<ChildExtent> = (0..count)
            .map(|idx| flexible(Rect::new(0.0, idx as f32 * 3.0, 80.0, 3.0)))
            .collect();
        assert_eq!(sibling_bounds(parent, &children).len(), count);
    }
}

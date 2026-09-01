use super::*;

#[test]
fn first_allocation_starts_at_origin() {
    let mut alloc = ShelfAllocator::new(256, 1);
    let result = alloc.allocate(PixelSize::new(10, 10).unwrap()).unwrap();
    assert_eq!(result.allocation_rect.x(), 0);
    assert_eq!(result.allocation_rect.y(), 0);
    assert_eq!(result.content_rect.x(), 1);
    assert_eq!(result.content_rect.y(), 1);
    assert_eq!(result.content_rect.width(), 10);
    assert_eq!(result.content_rect.height(), 10);
}

#[test]
fn adjacent_allocations_dont_overlap() {
    let mut alloc = ShelfAllocator::new(256, 1);
    let a = alloc.allocate(PixelSize::new(10, 10).unwrap()).unwrap();
    let b = alloc.allocate(PixelSize::new(10, 10).unwrap()).unwrap();

    assert!(b.allocation_rect.x() >= a.allocation_rect.x() + a.allocation_rect.width());
}

#[test]
fn first_two_glyphs_on_a_wrapped_shelf_do_not_overlap() {
    // Regression for the intermittent "wrong glyph" rendering bug: after a
    // shelf fills and allocation wraps to a new shelf, `cursor_x` must advance
    // past the glyph just placed at x=0.
    let page_size = 50u32;
    let padding = 1u32;
    let glyph_w = 10u32;
    let alloc_w = glyph_w + 2 * padding;
    let mut alloc = ShelfAllocator::new(page_size, padding);

    let fits = page_size / alloc_w;
    for _ in 0..fits {
        alloc
            .allocate(PixelSize::new(glyph_w, 10).unwrap())
            .unwrap();
    }

    let wrapped = alloc
        .allocate(PixelSize::new(glyph_w, 10).unwrap())
        .unwrap();
    let next = alloc
        .allocate(PixelSize::new(glyph_w, 10).unwrap())
        .unwrap();

    assert_eq!(wrapped.allocation_rect.y(), next.allocation_rect.y());
    assert!(
        next.allocation_rect.x() >= wrapped.allocation_rect.x() + wrapped.allocation_rect.width(),
        "next glyph (x={}) must start after the wrapped glyph (x={}, w={})",
        next.allocation_rect.x(),
        wrapped.allocation_rect.x(),
        wrapped.allocation_rect.width(),
    );
}

#[test]
fn shelf_wraps_when_width_exceeded() {
    let page_size = 50u32;
    let padding = 1u32;
    let glyph_w = 10u32;
    let alloc_w = glyph_w + 2 * padding;

    let mut alloc = ShelfAllocator::new(page_size, padding);
    let fits_per_shelf = page_size / alloc_w;

    for i in 0..fits_per_shelf {
        let result = alloc.allocate(PixelSize::new(glyph_w, 10).unwrap());
        assert!(result.is_some(), "allocation {i} should fit on first shelf");
        assert_eq!(result.unwrap().allocation_rect.y(), 0);
    }

    let result = alloc.allocate(PixelSize::new(glyph_w, 10).unwrap());
    assert!(result.is_some(), "should wrap to second shelf");
    assert!(
        result.unwrap().allocation_rect.y() > 0,
        "y must advance to new shelf"
    );
}

#[test]
fn rejects_oversized_glyph() {
    let mut alloc = ShelfAllocator::new(64, 1);
    let result = alloc.allocate(PixelSize::new(64, 10).unwrap());
    assert!(
        result.is_none(),
        "glyph width 64 + 2 padding = 66 > 64 page"
    );
}

#[test]
fn fills_multiple_shelves() {
    let mut alloc = ShelfAllocator::new(32, 0);
    let glyph = PixelSize::new(16, 4).unwrap();
    let mut shelves_used = std::collections::HashSet::new();

    for _ in 0..20 {
        if let Some(result) = alloc.allocate(glyph) {
            shelves_used.insert(result.allocation_rect.y());
        }
    }

    assert!(shelves_used.len() > 1, "should have used multiple shelves");
}

#[test]
fn content_rect_is_inside_allocation_rect() {
    let mut alloc = ShelfAllocator::new(256, 2);
    let result = alloc.allocate(PixelSize::new(10, 10).unwrap()).unwrap();
    let a = result.allocation_rect;
    let c = result.content_rect;

    assert!(c.x() >= a.x());
    assert!(c.y() >= a.y());
    assert!(c.x() + c.width() <= a.x() + a.width());
    assert!(c.y() + c.height() <= a.y() + a.height());
}

#[test]
fn padding_applied_exactly_once() {
    let padding = 3u32;
    let mut alloc = ShelfAllocator::new(256, padding);
    let result = alloc.allocate(PixelSize::new(10, 10).unwrap()).unwrap();

    assert_eq!(
        result.content_rect.x(),
        result.allocation_rect.x() + padding
    );
    assert_eq!(
        result.content_rect.y(),
        result.allocation_rect.y() + padding
    );
    assert_eq!(result.allocation_rect.width(), 10 + 2 * padding);
    assert_eq!(result.allocation_rect.height(), 10 + 2 * padding);
}

#[test]
fn returns_none_when_page_full() {
    let mut alloc = ShelfAllocator::new(16, 0);
    loop {
        if alloc.allocate(PixelSize::new(8, 8).unwrap()).is_none() {
            break;
        }
    }
}

#[test]
fn rejects_taller_glyph_when_current_shelf_would_overrun_page_bottom() {
    let mut alloc = ShelfAllocator::new(32, 0);

    alloc.allocate(PixelSize::new(32, 18).unwrap()).unwrap();

    let short = alloc.allocate(PixelSize::new(8, 4).unwrap()).unwrap();
    assert_eq!(short.allocation_rect.y(), 18);

    let too_tall_for_remaining_page = alloc.allocate(PixelSize::new(8, 15).unwrap());
    assert!(
        too_tall_for_remaining_page.is_none(),
        "same-shelf allocation must check vertical page bounds"
    );
}

#[test]
fn mixed_size_glyphs_fill_correctly() {
    let mut alloc = ShelfAllocator::new(64, 1);
    let tall = alloc.allocate(PixelSize::new(10, 30).unwrap());
    assert!(tall.is_some());
    assert_eq!(tall.unwrap().allocation_rect.y(), 0);

    let short = alloc.allocate(PixelSize::new(10, 5).unwrap());
    assert!(short.is_some());
    assert_eq!(short.unwrap().allocation_rect.y(), 0);

    let tall2 = alloc.allocate(PixelSize::new(10, 30).unwrap());
    assert!(tall2.is_some());
    assert_eq!(tall2.unwrap().allocation_rect.y(), 0);

    let big = alloc.allocate(PixelSize::new(60, 30).unwrap());
    assert!(big.is_some());
    assert!(
        big.unwrap().allocation_rect.y() > 0,
        "wide glyph should force a new shelf"
    );
}

#[test]
fn zero_padding_works() {
    let mut alloc = ShelfAllocator::new(256, 0);
    let result = alloc.allocate(PixelSize::new(10, 10).unwrap()).unwrap();
    assert_eq!(result.content_rect.x(), 0);
    assert_eq!(result.content_rect.y(), 0);
    assert_eq!(result.content_rect.width(), 10);
    assert_eq!(result.content_rect.height(), 10);
}

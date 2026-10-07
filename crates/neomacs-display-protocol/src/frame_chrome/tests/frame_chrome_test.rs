use crate::frame_chrome::{
    BandRect, ChromeAction, ChromeBandRequest, ChromeHitRegion, ChromeLayoutError, FrameChrome,
    FrameChromeContent, FrameChromeKind, FrameRect, FrameSize, InteractionId, MenuBarContent,
};
use crate::types::Rect;
use proptest::prelude::*;

#[test]
fn frame_chrome_layout_stacks_visible_bands_once() {
    let chrome = FrameChrome::layout(
        FrameSize::new(624.0, 648.0).expect("valid frame"),
        vec![
            ChromeBandRequest::empty(FrameChromeKind::MenuBar, 18.0),
            ChromeBandRequest::empty(FrameChromeKind::ToolBar, 34.0),
            ChromeBandRequest::empty(FrameChromeKind::TabBar, 18.0),
        ],
    )
    .expect("valid chrome");

    assert_eq!(
        chrome
            .band(FrameChromeKind::MenuBar)
            .expect("menu band")
            .bounds()
            .y(),
        0.0
    );
    assert_eq!(
        chrome
            .band(FrameChromeKind::ToolBar)
            .expect("tool band")
            .bounds()
            .y(),
        18.0
    );
    assert_eq!(
        chrome
            .band(FrameChromeKind::TabBar)
            .expect("tab band")
            .bounds()
            .y(),
        52.0
    );
}

#[test]
fn frame_rect_places_band_local_rect_exactly_once() {
    let band = FrameRect::new(0.0, 52.0, 624.0, 18.0).expect("valid band");
    let local = BandRect::new(8.0, 0.0, 40.0, 18.0).expect("valid local rect");

    assert_eq!(
        band.place(local).expect("content fits band").raw(),
        Rect::new(8.0, 52.0, 40.0, 18.0)
    );
}

#[test]
fn frame_rect_places_band_content_through_fixed_point_space_transform() {
    let band = FrameRect::new(0.0, 52.100_001, 100.0, 18.0).expect("valid band");
    let local = BandRect::new(8.150_001, 0.0, 40.0, 18.0).expect("valid local rect");

    assert_eq!(
        band.place(local).expect("content fits band").raw(),
        Rect::new(8.15625, 52.09375, 40.0, 18.0)
    );
}

#[test]
fn frame_chrome_layout_omits_zero_height_bands() {
    let chrome = FrameChrome::layout(
        FrameSize::new(624.0, 648.0).expect("valid frame"),
        vec![
            ChromeBandRequest::empty(FrameChromeKind::MenuBar, 0.0),
            ChromeBandRequest::empty(FrameChromeKind::TabBar, 18.0),
        ],
    )
    .expect("valid chrome");

    assert!(chrome.band(FrameChromeKind::MenuBar).is_none());
    assert_eq!(
        chrome
            .band(FrameChromeKind::TabBar)
            .expect("tab band")
            .bounds()
            .y(),
        0.0
    );
}

#[test]
fn frame_chrome_layout_rejects_compact_and_separate_bars() {
    let error = FrameChrome::layout(
        FrameSize::new(624.0, 648.0).expect("valid frame"),
        vec![
            ChromeBandRequest::empty(FrameChromeKind::CompactBar, 18.0),
            ChromeBandRequest::empty(FrameChromeKind::MenuBar, 18.0),
        ],
    )
    .expect_err("compact and separate menu bars conflict");

    assert_eq!(error, ChromeLayoutError::ConflictingPresentation);
}

#[test]
fn frame_chrome_layout_rejects_duplicate_band_kinds() {
    let error = FrameChrome::layout(
        FrameSize::new(624.0, 648.0).expect("valid frame"),
        vec![
            ChromeBandRequest::empty(FrameChromeKind::TabBar, 18.0),
            ChromeBandRequest::empty(FrameChromeKind::TabBar, 18.0),
        ],
    )
    .expect_err("duplicate tab bands are invalid");

    assert_eq!(
        error,
        ChromeLayoutError::DuplicateBand {
            kind: FrameChromeKind::TabBar,
        }
    );
}

#[test]
fn frame_chrome_layout_rejects_bands_outside_frame() {
    let error = FrameChrome::layout(
        FrameSize::new(624.0, 60.0).expect("valid frame"),
        vec![
            ChromeBandRequest::empty(FrameChromeKind::MenuBar, 18.0),
            ChromeBandRequest::empty(FrameChromeKind::ToolBar, 34.0),
            ChromeBandRequest::empty(FrameChromeKind::TabBar, 18.0),
        ],
    )
    .expect_err("chrome exceeds frame height");

    assert_eq!(
        error,
        ChromeLayoutError::ContentExceedsFrame {
            kind: FrameChromeKind::TabBar,
        }
    );
}

#[test]
fn frame_chrome_geometry_rejects_non_finite_and_negative_dimensions() {
    assert_eq!(
        FrameSize::new(f32::NAN, 10.0),
        Err(ChromeLayoutError::InvalidFrameSize)
    );
    assert_eq!(
        FrameRect::new(0.0, 0.0, -1.0, 10.0),
        Err(ChromeLayoutError::InvalidRect)
    );
    assert_eq!(
        BandRect::new(0.0, 0.0, 1.0, f32::INFINITY),
        Err(ChromeLayoutError::InvalidRect)
    );
}

#[test]
fn frame_rect_rejects_local_content_outside_band() {
    let band = FrameRect::new(0.0, 52.0, 100.0, 18.0).expect("valid band");
    let local = BandRect::new(90.0, 0.0, 20.0, 18.0).expect("valid local rect");

    assert_eq!(
        band.place(local),
        Err(ChromeLayoutError::ContentExceedsBand)
    );
}

#[test]
fn frame_chrome_layout_rejects_hit_regions_outside_their_band() {
    let request = ChromeBandRequest::empty(FrameChromeKind::TabBar, 18.0).with_hit_regions(vec![
        ChromeHitRegion::new(
            BandRect::new(610.0, 0.0, 20.0, 18.0).expect("valid local rect"),
            ChromeAction::Presented {
                interaction: InteractionId::new(0),
            },
        ),
    ]);

    let error = FrameChrome::layout(
        FrameSize::new(624.0, 648.0).expect("valid frame"),
        vec![request],
    )
    .expect_err("hit region exceeds its band");

    assert_eq!(error, ChromeLayoutError::ContentExceedsBand);
}

#[test]
fn tab_bar_hit_regions_publish_only_snapshot_scoped_interaction_references() {
    let interaction = InteractionId::new(7);
    let action = ChromeAction::Presented { interaction };

    let chrome = FrameChrome::layout(
        FrameSize::new(100.0, 18.0).expect("valid frame"),
        vec![
            ChromeBandRequest::empty(FrameChromeKind::TabBar, 18.0).with_hit_regions(vec![
                ChromeHitRegion::new(
                    BandRect::new(0.0, 0.0, 20.0, 18.0).expect("valid hit bounds"),
                    action.clone(),
                ),
            ]),
        ],
    )
    .expect("valid chrome");

    let published = chrome
        .band(FrameChromeKind::TabBar)
        .expect("tab band")
        .hit_regions()[0]
        .action();
    assert_eq!(published, &action);
}

#[test]
fn frame_chrome_layout_rejects_content_for_another_band_kind() {
    let error = FrameChrome::layout(
        FrameSize::new(624.0, 648.0).expect("valid frame"),
        vec![ChromeBandRequest::new(
            FrameChromeKind::TabBar,
            18.0,
            FrameChromeContent::MenuBar(MenuBarContent::empty()),
        )],
    )
    .expect_err("menu content cannot inhabit a tab band");

    assert_eq!(
        error,
        ChromeLayoutError::ContentKindMismatch {
            kind: FrameChromeKind::TabBar,
        }
    );
}

proptest! {
    #[test]
    fn frame_chrome_layout_keeps_generated_bands_ordered_and_inside_frame(
        menu_height in 0.0_f32..80.0,
        tool_height in 0.0_f32..80.0,
        tab_height in 0.0_f32..80.0,
    ) {
        let total = menu_height + tool_height + tab_height;
        let chrome = FrameChrome::layout(
            FrameSize::new(624.0, total + 1.0).expect("valid generated frame"),
            vec![
                ChromeBandRequest::empty(FrameChromeKind::TabBar, tab_height),
                ChromeBandRequest::empty(FrameChromeKind::MenuBar, menu_height),
                ChromeBandRequest::empty(FrameChromeKind::ToolBar, tool_height),
            ],
        )
        .expect("valid generated chrome");

        let mut previous_bottom = 0.0;
        for band in chrome.bands() {
            prop_assert!(band.bounds().y() >= previous_bottom);
            prop_assert!(band.bounds().bottom() <= total + 1.0);
            previous_bottom = band.bounds().bottom();
        }
    }
}

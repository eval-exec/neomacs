use super::*;

fn options(hinting: Hinting) -> RenderOptions {
    RenderOptions {
        hinting,
        antialias: true,
        autohint: false,
        horizontal_lcd: true,
        lcd_filter: Filter::Default,
    }
}

#[test]
fn light_and_full_hinting_select_distinct_freetype_targets() {
    let target_mask = 0xF0000;
    assert_eq!(
        options(Hinting::Slight)
            .load_flags(false, GlyphRenderMode::Alpha)
            .bits()
            & target_mask,
        LoadFlag::TARGET_LIGHT.bits()
    );
    assert_eq!(
        options(Hinting::Full)
            .load_flags(false, GlyphRenderMode::Alpha)
            .bits()
            & target_mask,
        LoadFlag::TARGET_NORMAL.bits()
    );
    for (hinting, target) in [
        (Hinting::Slight, LoadFlag::TARGET_LIGHT),
        (Hinting::Medium, LoadFlag::TARGET_NORMAL),
        (Hinting::Full, LoadFlag::TARGET_LCD),
    ] {
        assert_eq!(
            options(hinting)
                .load_flags(false, GlyphRenderMode::Subpixel)
                .bits()
                & target_mask,
            target.bits()
        );
    }
    let forced = RenderOptions {
        autohint: true,
        ..options(Hinting::Slight)
    };
    assert!(
        forced
            .load_flags(false, GlyphRenderMode::Alpha)
            .contains(LoadFlag::FORCE_AUTOHINT)
    );
    for policy in [options(Hinting::None), forced] {
        let flags = policy.load_flags(true, GlyphRenderMode::Alpha);
        assert!(flags.contains(LoadFlag::NO_HINTING));
        assert!(!flags.contains(LoadFlag::FORCE_AUTOHINT));
    }
    assert!(
        options(Hinting::None)
            .load_flags(false, GlyphRenderMode::Alpha)
            .contains(LoadFlag::NO_HINTING)
    );
}

#[test]
fn antialias_and_lcd_policy_select_the_bitmap_format() {
    let grayscale = RenderOptions {
        horizontal_lcd: false,
        ..options(Hinting::Slight)
    };
    assert!(matches!(
        grayscale.render_mode(GlyphRenderMode::Subpixel),
        RenderMode::Normal
    ));
    assert!(matches!(
        options(Hinting::Slight).render_mode(GlyphRenderMode::Subpixel),
        RenderMode::Lcd
    ));
    let mono = RenderOptions {
        antialias: false,
        ..options(Hinting::Slight)
    };
    assert!(matches!(
        mono.render_mode(GlyphRenderMode::Alpha),
        RenderMode::Mono
    ));
    assert_eq!(
        mono.load_flags(false, GlyphRenderMode::Alpha).bits() & 0xF0000,
        LoadFlag::TARGET_MONO.bits()
    );
}

#[test]
fn native_bitmap_rows_follow_signed_pitch_without_copying_padding() {
    let bytes = [1, 2, 99, 3, 4, 99];
    // SAFETY: both two-byte rows and their positive/negative strides stay
    // within bytes; the second pointer names the first logical reversed row.
    unsafe {
        assert_eq!(copy_rows(bytes.as_ptr(), 3, 2, 2), Some(vec![1, 2, 3, 4]));
        assert_eq!(
            copy_rows(bytes.as_ptr().add(3), -3, 2, 2),
            Some(vec![3, 4, 1, 2])
        );
        assert_eq!(copy_rows(bytes.as_ptr(), 1, 2, 2), None);
    }
}

#[test]
fn native_mono_and_lcd_masks_keep_their_coverage_channels() {
    assert_eq!(
        unpack_mono(&[0b10100000, 0b01000000], 3, 2),
        Some(vec![255, 0, 255, 0, 255, 0])
    );
    assert_eq!(
        normalize_gray(vec![0, 1, 2, 3], 4),
        Some(vec![0, 85, 170, 255])
    );
    assert_eq!(
        pack_lcd(&[10, 80, 30, 255, 0, 7]),
        Some(vec![10, 80, 30, 80, 255, 0, 7, 255])
    );
    assert_eq!(pack_lcd(&[10, 80]), None);
}

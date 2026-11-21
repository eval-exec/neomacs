use super::*;

fn pattern(hinting: Hinting) -> FontRenderOptions {
    FontRenderOptions {
        hinting,
        antialias: true,
        autohint: false,
        horizontal_lcd: true,
        metrics_mode: FontRenderMode::Alpha,
        lcd_filter: Filter::Light,
    }
}

fn xlib(resources: XftResources<'_>) -> SurfaceFontOptions {
    SurfaceFontOptions::from_xlib_resources(resources, SurfaceSubpixelOrder::Unknown)
}

#[test]
fn xlib_defaults_override_pattern_raster_but_leave_metrics_unchanged() {
    let font = pattern(Hinting::Slight);
    let metrics = font.metrics_load_flags();
    let raster = font.for_surface(xlib(XftResources::default()));
    assert_eq!(
        raster.load_flags(false, FontRenderMode::Subpixel).bits() & 0xf0000,
        LoadFlag::TARGET_NORMAL.bits()
    );
    assert!(matches!(
        raster.render_mode(FontRenderMode::Subpixel),
        RenderMode::Normal
    ));
    assert_eq!(font.metrics_load_flags().bits(), metrics.bits());
    assert_eq!(metrics.bits() & 0xf0000, LoadFlag::TARGET_LIGHT.bits());
    let untouched = font.for_surface(SurfaceFontOptions::default());
    assert_eq!(
        untouched.load_flags(false, FontRenderMode::Alpha).bits() & 0xf0000,
        LoadFlag::TARGET_LIGHT.bits()
    );
    assert!(matches!(
        untouched.render_mode(FontRenderMode::Subpixel),
        RenderMode::Lcd
    ));
}

#[test]
fn xft_resources_follow_cairo_boolean_constant_and_numeric_parsing() {
    for value in ["hintslight", "1", "0x1suffix", "01"] {
        let surface = xlib(XftResources {
            hintstyle: Some(value),
            ..Default::default()
        });
        assert_eq!(
            pattern(Hinting::Full)
                .for_surface(surface)
                .load_flags(false, FontRenderMode::Alpha)
                .bits()
                & 0xf0000,
            LoadFlag::TARGET_LIGHT.bits(),
            "{value}"
        );
    }
    for value in ["false", "no", "off", "0"] {
        let surface = xlib(XftResources {
            hinting: Some(value),
            ..Default::default()
        });
        assert!(
            pattern(Hinting::Full)
                .for_surface(surface)
                .load_flags(false, FontRenderMode::Alpha)
                .contains(LoadFlag::NO_HINTING)
        );
    }
    // An unrecognized integer has Cairo DEFAULT semantics, while a string
    // that cannot parse at all selects the Xlib default, FULL.
    for (value, target) in [
        ("99", LoadFlag::TARGET_LIGHT),
        ("invalid", LoadFlag::TARGET_NORMAL),
    ] {
        let surface = xlib(XftResources {
            hintstyle: Some(value),
            ..Default::default()
        });
        assert_eq!(
            pattern(Hinting::Slight)
                .for_surface(surface)
                .load_flags(false, FontRenderMode::Alpha)
                .bits()
                & 0xf0000,
            target.bits()
        );
    }
    let off = xlib(XftResources {
        antialias: Some("off"),
        ..Default::default()
    });
    let raster = pattern(Hinting::Slight).for_surface(off);
    assert!(matches!(
        raster.render_mode(FontRenderMode::Subpixel),
        RenderMode::Mono
    ));
    assert!(
        raster
            .load_flags(false, FontRenderMode::Alpha)
            .contains(LoadFlag::MONOCHROME)
    );
    // Cairo's boolean parser recognizes a lowercase leading 'o' only.
    let invalid = xlib(XftResources {
        antialias: Some("Off"),
        ..Default::default()
    });
    assert!(matches!(
        pattern(Hinting::Slight)
            .for_surface(invalid)
            .render_mode(FontRenderMode::Alpha),
        RenderMode::Normal
    ));
}

#[test]
fn pattern_none_survives_surface_overrides_and_filter_default_is_distinct() {
    let surface = xlib(XftResources {
        rgba: Some("rgb"),
        lcdfilter: Some("lcddefault"),
        ..Default::default()
    });
    assert!(
        pattern(Hinting::None)
            .for_surface(surface)
            .load_flags(false, FontRenderMode::Subpixel)
            .contains(LoadFlag::NO_HINTING)
    );
    let mono = FontRenderOptions {
        antialias: false,
        ..pattern(Hinting::Full)
    };
    assert!(matches!(
        mono.for_surface(surface)
            .render_mode(FontRenderMode::Subpixel),
        RenderMode::Mono
    ));
    let disabled_filter = FontRenderOptions {
        lcd_filter: Filter::None,
        ..pattern(Hinting::Full)
    };
    assert!(matches!(
        disabled_filter.for_surface(surface).lcd_filter(),
        LcdFilter::LcdFilterNone
    ));
    assert!(matches!(
        pattern(Hinting::Full).for_surface(surface).lcd_filter(),
        LcdFilter::LcdFilterDefault
    ));
    assert!(matches!(
        pattern(Hinting::Full)
            .for_surface(xlib(XftResources::default()))
            .lcd_filter(),
        LcdFilter::LcdFilterLight
    ));
}

#[test]
fn explicit_rgba_overrides_xrender_and_gray_disables_subpixel_masks() {
    let server = SurfaceSubpixelOrder::from_xrender(2);
    assert_eq!(server, SurfaceSubpixelOrder::Bgr);
    let inherited = SurfaceFontOptions::from_xlib_resources(XftResources::default(), server);
    assert_eq!(
        inherited.raster_subpixel_order(),
        Some(SurfaceSubpixelOrder::Bgr)
    );
    let explicit = SurfaceFontOptions::from_xlib_resources(
        XftResources {
            rgba: Some("rgb"),
            ..Default::default()
        },
        server,
    );
    assert_eq!(
        explicit.raster_subpixel_order(),
        Some(SurfaceSubpixelOrder::Rgb)
    );
    let gray = SurfaceFontOptions::from_xlib_resources(
        XftResources {
            rgba: Some("none"),
            ..Default::default()
        },
        server,
    );
    assert_eq!(
        gray.raster_subpixel_order(),
        Some(SurfaceSubpixelOrder::None)
    );
    assert!(matches!(
        pattern(Hinting::Full)
            .for_surface(gray)
            .render_mode(FontRenderMode::Subpixel),
        RenderMode::Normal
    ));
    assert_eq!(SurfaceFontOptions::default().raster_subpixel_order(), None);
    assert_eq!(
        SurfaceSubpixelOrder::from_xrender(123),
        SurfaceSubpixelOrder::Unknown
    );
}

#[test]
fn monochrome_patterns_leave_hintstyle_default_until_surface_merge() {
    let font = FontRenderOptions {
        antialias: false,
        ..pattern(Hinting::None)
    };
    for surface in [
        SurfaceFontOptions::default(),
        xlib(XftResources {
            hintstyle: Some("99"),
            ..Default::default()
        }),
    ] {
        let raster = font.for_surface(surface);
        let flags = raster.load_flags(false, FontRenderMode::Alpha);
        assert!(matches!(
            raster.render_mode(FontRenderMode::Alpha),
            RenderMode::Mono
        ));
        assert_eq!(flags.bits() & 0xf0000, LoadFlag::TARGET_MONO.bits());
        assert!(!flags.contains(LoadFlag::NO_HINTING));
    }
    let disabled = font.for_surface(xlib(XftResources {
        hinting: Some("false"),
        ..Default::default()
    }));
    assert!(
        disabled
            .load_flags(false, FontRenderMode::Alpha)
            .contains(LoadFlag::NO_HINTING)
    );
}

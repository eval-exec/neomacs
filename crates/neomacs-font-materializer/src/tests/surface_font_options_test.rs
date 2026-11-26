use super::*;

fn pattern(hinting: Hinting) -> FontRenderOptions {
    FontRenderOptions {
        hinting,
        antialias: true,
        autohint: false,
        horizontal_lcd: true,
        metrics_mode: MetricPolicy::Pattern(FontMetricMode::Alpha),
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

#[test]
fn pgtk_missing_invalid_and_unknown_values_retain_cairo_defaults() {
    for value in [
        GSettingsValue::Missing,
        GSettingsValue::Invalid,
        GSettingsValue::String(""),
        GSettingsValue::String("unknown"),
        GSettingsValue::String("NONE"),
    ] {
        assert_eq!(
            SurfaceFontOptions::from_pgtk_settings(PgtkFontSettings {
                antialias: value,
                hinting: value,
                rgba: value,
            }),
            SurfaceFontOptions::default(),
            "{value:?}"
        );
    }
}

#[test]
fn pgtk_hinting_strings_select_exact_targets_without_xft_parsing() {
    for (value, expected) in [
        ("none", Hinting::None),
        ("slight", Hinting::Slight),
        ("medium", Hinting::Medium),
        ("full", Hinting::Full),
    ] {
        let options = SurfaceFontOptions::from_pgtk_settings(PgtkFontSettings {
            hinting: GSettingsValue::String(value),
            ..Default::default()
        });
        assert_eq!(options.hinting, Some(expected), "{value}");
        assert_eq!(options.raster_subpixel_order(), None);
    }
    for value in ["hintslight", "1", " slight", "Full"] {
        assert_eq!(
            SurfaceFontOptions::from_pgtk_settings(PgtkFontSettings {
                hinting: GSettingsValue::String(value),
                ..Default::default()
            }),
            SurfaceFontOptions::default(),
            "{value}"
        );
    }
}

#[test]
fn pgtk_antialias_and_order_settings_are_independent() {
    for (value, order) in [
        ("rgb", SurfaceSubpixelOrder::Rgb),
        ("bgr", SurfaceSubpixelOrder::Bgr),
        ("vrgb", SurfaceSubpixelOrder::VRgb),
        ("vbgr", SurfaceSubpixelOrder::VBgr),
    ] {
        let gray = SurfaceFontOptions::from_pgtk_settings(PgtkFontSettings {
            antialias: GSettingsValue::String("grayscale"),
            rgba: GSettingsValue::String(value),
            ..Default::default()
        });
        assert_eq!(
            gray.raster_subpixel_order(),
            Some(SurfaceSubpixelOrder::None)
        );
        let lcd = gray.with_pgtk_settings(PgtkFontSettings {
            antialias: GSettingsValue::String("rgba"),
            ..Default::default()
        });
        assert_eq!(lcd.raster_subpixel_order(), Some(order));
        let mono = lcd.with_pgtk_settings(PgtkFontSettings {
            antialias: GSettingsValue::String("none"),
            ..Default::default()
        });
        assert!(matches!(
            pattern(Hinting::Full)
                .for_surface(mono)
                .render_mode(FontRenderMode::Subpixel),
            RenderMode::Mono
        ));
        assert_eq!(
            mono.with_pgtk_settings(PgtkFontSettings {
                antialias: GSettingsValue::String("rgba"),
                ..Default::default()
            }),
            lcd
        );
    }
}

#[test]
fn pgtk_unrecognized_updates_retain_the_last_valid_options() {
    let previous = SurfaceFontOptions::from_pgtk_settings(PgtkFontSettings {
        antialias: GSettingsValue::String("rgba"),
        hinting: GSettingsValue::String("slight"),
        rgba: GSettingsValue::String("bgr"),
    });
    for value in [
        GSettingsValue::Missing,
        GSettingsValue::Invalid,
        GSettingsValue::String("unknown"),
        GSettingsValue::String("None"),
    ] {
        assert_eq!(
            previous.with_pgtk_settings(PgtkFontSettings {
                antialias: value,
                hinting: value,
                rgba: value,
            }),
            previous
        );
    }
    let changed = previous.with_pgtk_settings(PgtkFontSettings {
        hinting: GSettingsValue::String("full"),
        ..Default::default()
    });
    assert_eq!(changed.hinting, Some(Hinting::Full));
    assert_eq!(
        changed.raster_subpixel_order(),
        Some(SurfaceSubpixelOrder::Bgr)
    );
    assert_eq!(
        previous.with_pgtk_settings(PgtkFontSettings {
            rgba: GSettingsValue::String("none"),
            ..Default::default()
        }),
        previous,
        "PGTK's order key does not accept Xft's none constant"
    );
}

#[test]
fn pgtk_surface_options_preserve_pattern_none_precedence() {
    let options = SurfaceFontOptions::from_pgtk_settings(PgtkFontSettings {
        antialias: GSettingsValue::String("rgba"),
        hinting: GSettingsValue::String("full"),
        rgba: GSettingsValue::String("rgb"),
    });
    assert!(
        pattern(Hinting::None)
            .for_surface(options)
            .load_flags(false, FontRenderMode::Subpixel)
            .contains(LoadFlag::NO_HINTING)
    );
    assert!(
        pattern(Hinting::None)
            .for_pgtk(options)
            .metrics_load_flags()
            .contains(LoadFlag::NO_HINTING)
    );
    let mono = FontRenderOptions {
        antialias: false,
        ..pattern(Hinting::Full)
    };
    assert!(matches!(
        mono.for_surface(options)
            .render_mode(FontRenderMode::Subpixel),
        RenderMode::Mono
    ));
    assert!(
        mono.for_pgtk(options)
            .metrics_load_flags()
            .contains(LoadFlag::MONOCHROME)
    );
    let no_filter = FontRenderOptions {
        lcd_filter: Filter::None,
        ..pattern(Hinting::Full)
    };
    assert!(matches!(
        no_filter.for_surface(options).lcd_filter(),
        LcdFilter::LcdFilterNone
    ));
}

#[test]
fn pgtk_rgba_without_an_order_uses_cairo_rgb_default() {
    let options = SurfaceFontOptions::from_pgtk_settings(PgtkFontSettings {
        antialias: GSettingsValue::String("rgba"),
        ..Default::default()
    });
    assert_eq!(
        options.raster_subpixel_order(),
        Some(SurfaceSubpixelOrder::Rgb)
    );
    assert!(matches!(
        pattern(Hinting::Full)
            .for_surface(options)
            .render_mode(FontRenderMode::Subpixel),
        RenderMode::Lcd
    ));
    let vertical = FontRenderOptions {
        horizontal_lcd: false,
        metrics_mode: MetricPolicy::Pattern(FontMetricMode::VerticalSubpixel),
        ..pattern(Hinting::Full)
    };
    assert_eq!(
        vertical.for_pgtk(options).metrics_load_flags().bits() & 0xf0000,
        LoadFlag::TARGET_LCD.bits()
    );
}

#[test]
fn pgtk_opened_font_metrics_use_the_merged_settings_targets() {
    let font = pattern(Hinting::Slight);
    let original = font.metrics_load_flags();
    for (antialias, hinting, order, target) in [
        ("grayscale", "slight", "rgb", LoadFlag::TARGET_LIGHT),
        ("grayscale", "full", "rgb", LoadFlag::TARGET_NORMAL),
        ("rgba", "full", "rgb", LoadFlag::TARGET_LCD),
        ("rgba", "full", "bgr", LoadFlag::TARGET_LCD),
        ("rgba", "full", "vrgb", LoadFlag::TARGET_LCD_V),
        ("rgba", "full", "vbgr", LoadFlag::TARGET_LCD_V),
        ("rgba", "medium", "rgb", LoadFlag::TARGET_NORMAL),
        ("none", "full", "rgb", LoadFlag::TARGET_MONO),
    ] {
        let settings = SurfaceFontOptions::from_pgtk_settings(PgtkFontSettings {
            antialias: GSettingsValue::String(antialias),
            hinting: GSettingsValue::String(hinting),
            rgba: GSettingsValue::String(order),
        });
        let flags = font.for_pgtk(settings).metrics_load_flags();
        assert_eq!(
            flags.bits() & 0xf0000,
            target.bits(),
            "{antialias}/{hinting}/{order}"
        );
        assert_eq!(flags.contains(LoadFlag::MONOCHROME), antialias == "none");
        assert!(!flags.contains(LoadFlag::NO_BITMAP));
    }
    assert_eq!(
        font.metrics_load_flags().bits(),
        original.bits(),
        "X11 pattern metrics stay unchanged"
    );
    let disabled = SurfaceFontOptions::from_pgtk_settings(PgtkFontSettings {
        antialias: GSettingsValue::String("none"),
        hinting: GSettingsValue::String("none"),
        ..Default::default()
    });
    let flags = font.for_pgtk(disabled).metrics_load_flags();
    assert!(flags.contains(LoadFlag::NO_HINTING | LoadFlag::MONOCHROME));
    assert_eq!(flags.bits() & 0xf0000, LoadFlag::TARGET_NORMAL.bits());
}

#[test]
fn pgtk_default_antialias_uses_pattern_order_until_rgba_is_explicit() {
    let vertical = FontRenderOptions {
        horizontal_lcd: false,
        metrics_mode: MetricPolicy::Pattern(FontMetricMode::VerticalSubpixel),
        ..pattern(Hinting::Full)
    };
    let order_only = SurfaceFontOptions::from_pgtk_settings(PgtkFontSettings {
        rgba: GSettingsValue::String("bgr"),
        ..Default::default()
    });
    assert_eq!(order_only.raster_subpixel_order(), None);
    assert_eq!(
        vertical.for_pgtk(order_only).metrics_load_flags().bits() & 0xf0000,
        LoadFlag::TARGET_LCD_V.bits()
    );
    let explicit = order_only.with_pgtk_settings(PgtkFontSettings {
        antialias: GSettingsValue::String("rgba"),
        ..Default::default()
    });
    assert_eq!(
        explicit.raster_subpixel_order(),
        Some(SurfaceSubpixelOrder::Bgr)
    );
    assert_eq!(
        vertical.for_pgtk(explicit).metrics_load_flags().bits() & 0xf0000,
        LoadFlag::TARGET_LCD.bits()
    );
}

#[test]
fn pgtk_protocol_round_trip_preserves_defaults_and_independent_fields() {
    use neomacs_display_protocol::{
        DesktopFontAntialias as Aa, DesktopFontHinting as Hint, DesktopFontRenderOptions,
        DesktopFontSubpixelOrder as Order,
    };
    for antialias in [Aa::Default, Aa::None, Aa::Grayscale, Aa::Subpixel] {
        for hinting in [
            Hint::Default,
            Hint::None,
            Hint::Slight,
            Hint::Medium,
            Hint::Full,
        ] {
            for subpixel_order in [
                Order::Default,
                Order::Rgb,
                Order::Bgr,
                Order::VRgb,
                Order::VBgr,
            ] {
                let snapshot = DesktopFontRenderOptions {
                    antialias,
                    hinting,
                    subpixel_order,
                };
                let surface = SurfaceFontOptions::from_desktop_options(snapshot);
                assert_eq!(surface.desktop_options(), snapshot);
                assert_eq!(surface.lcd_filter, None, "PGTK has no LCD-filter setting");
            }
        }
    }
    let defaults = DesktopFontRenderOptions::default();
    assert_eq!(
        SurfaceFontOptions::from_desktop_options(defaults),
        SurfaceFontOptions::default()
    );
}

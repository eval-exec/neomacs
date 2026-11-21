use super::*;

struct Config(NonNull<fc::FcConfig>);

impl Drop for Config {
    fn drop(&mut self) {
        // SAFETY: this fixture exclusively owns the configuration.
        unsafe { fc::FcConfigDestroy(self.0.as_ptr()) };
    }
}

#[test]
fn full_and_slight_hinting_preserve_noto_device_advances() {
    let path = neomacs_test_fonts::noto_sans_variable();
    for (policy, expected_small) in [("hintfull", 5), ("hintslight", 7)] {
        // The fixture owns its font and rules; it neither reads nor changes the
        // process configuration. Both policies must remain distinct in one run.
        // SAFETY: allocate a new, independent configuration owned by Config.
        let config = Config(NonNull::new(unsafe { fc::FcConfigCreate() }).unwrap());
        let rules = CString::new(format!(
            "<fontconfig><match target=\"pattern\"><edit name=\"hintstyle\" mode=\"assign\"><const>{policy}</const></edit><edit name=\"rgba\" mode=\"assign\"><const>none</const></edit></match></fontconfig>"
        ))
        .unwrap();
        unsafe extern "C" {
            fn FcConfigParseAndLoadFromMemory(
                config: *mut fc::FcConfig,
                data: *const fc::FcChar8,
                complain: fc::FcBool,
            ) -> fc::FcBool;
        }
        let filename = CString::new(path.as_os_str().as_bytes()).unwrap();
        // SAFETY: the configuration is exclusive and both strings outlive
        // these calls; Fontconfig retains its own copies of rules/font data.
        unsafe {
            assert_ne!(
                FcConfigParseAndLoadFromMemory(config.0.as_ptr(), rules.as_ptr().cast(), 1),
                0
            );
            assert_ne!(
                fc::FcConfigAppFontAddFile(config.0.as_ptr(), filename.as_ptr().cast()),
                0
            );
            assert_ne!(fc::FcConfigBuildFonts(config.0.as_ptr()), 0);
        }
        // SAFETY: config stays live and exclusively borrowed through this loop.
        let context = unsafe { FontconfigContext::from_raw(config.0) };
        let library = freetype::Library::init().unwrap();
        let face = library.new_face(path, 0).unwrap();
        for (size, expected, average) in [(14, 13, 7), (7, expected_small, 3)] {
            let options = context.options_for_file(path, 0, size as f32).unwrap();
            face.set_pixel_sizes(size, size).unwrap();
            face.load_char('m' as usize, options.metrics_load_flags())
                .unwrap();
            assert_eq!(
                (face.glyph().advance().x + 32) >> 6,
                expected,
                "{policy}/{size}"
            );
            let mut advances = Vec::new();
            for character in 32..127 {
                face.load_char(character, options.metrics_load_flags())
                    .unwrap();
                let advance = (face.glyph().advance().x + 32) >> 6;
                if advance > 0 {
                    advances.push(advance);
                }
            }
            assert_eq!(
                advances
                    .iter()
                    .map(|value| i128::from(*value))
                    .sum::<i128>()
                    / advances.len() as i128,
                average
            );
        }
    }
}

fn options(hinting: Hinting) -> FontRenderOptions {
    FontRenderOptions {
        hinting,
        antialias: true,
        autohint: false,
        horizontal_lcd: true,
        metrics_mode: FontRenderMode::Alpha,
        lcd_filter: Filter::Default,
    }
}

#[test]
fn light_and_full_hinting_select_distinct_freetype_targets() {
    let target_mask = 0xF0000;
    assert_eq!(
        options(Hinting::Slight)
            .load_flags(false, FontRenderMode::Alpha)
            .bits()
            & target_mask,
        LoadFlag::TARGET_LIGHT.bits()
    );
    assert_eq!(
        options(Hinting::Full)
            .load_flags(false, FontRenderMode::Alpha)
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
                .load_flags(false, FontRenderMode::Subpixel)
                .bits()
                & target_mask,
            target.bits()
        );
    }
    let lcd_metrics = FontRenderOptions {
        metrics_mode: FontRenderMode::Subpixel,
        ..options(Hinting::Full)
    };
    assert_eq!(
        lcd_metrics.metrics_load_flags().bits() & target_mask,
        LoadFlag::TARGET_LCD.bits()
    );
    let forced = FontRenderOptions {
        autohint: true,
        ..options(Hinting::Slight)
    };
    assert!(
        forced
            .load_flags(false, FontRenderMode::Alpha)
            .contains(LoadFlag::FORCE_AUTOHINT)
    );
    for policy in [options(Hinting::None), forced] {
        let flags = policy.load_flags(true, FontRenderMode::Alpha);
        assert!(flags.contains(LoadFlag::NO_HINTING));
        assert!(!flags.contains(LoadFlag::FORCE_AUTOHINT));
    }
    assert!(
        options(Hinting::None)
            .load_flags(false, FontRenderMode::Alpha)
            .contains(LoadFlag::NO_HINTING)
    );
}

#[test]
fn antialias_and_lcd_policy_select_the_bitmap_format() {
    let grayscale = FontRenderOptions {
        horizontal_lcd: false,
        ..options(Hinting::Slight)
    };
    assert!(matches!(
        grayscale.render_mode(FontRenderMode::Subpixel),
        RenderMode::Normal
    ));
    assert!(matches!(
        options(Hinting::Slight).render_mode(FontRenderMode::Subpixel),
        RenderMode::Lcd
    ));
    let mono = FontRenderOptions {
        antialias: false,
        ..options(Hinting::Slight)
    };
    assert!(matches!(
        mono.render_mode(FontRenderMode::Alpha),
        RenderMode::Mono
    ));
    assert_eq!(
        mono.load_flags(false, FontRenderMode::Alpha).bits() & 0xF0000,
        LoadFlag::TARGET_MONO.bits()
    );
}

use super::*;

struct Config(NonNull<fc::FcConfig>);

impl Config {
    fn new(path: &Path, rules: &str) -> Self {
        // SAFETY: allocate a new, independent configuration owned by Config.
        let config = Self(NonNull::new(unsafe { fc::FcConfigCreate() }).unwrap());
        let rules = CString::new(format!("<fontconfig>{rules}</fontconfig>")).unwrap();
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
        config
    }

    fn context(&self) -> FontconfigContext<'_> {
        // SAFETY: the borrow keeps this exclusively owned config live, and
        // the helper exposes no mutation while queries borrow the context.
        unsafe { FontconfigContext::from_raw(self.0) }
    }
}

impl Drop for Config {
    fn drop(&mut self) {
        // SAFETY: this fixture exclusively owns the configuration.
        unsafe { fc::FcConfigDestroy(self.0.as_ptr()) };
    }
}

#[test]
fn full_and_slight_hinting_preserve_noto_device_advances() {
    let path = neomacs_test_fonts::noto_sans_variable();
    for (policy, target) in [
        ("hintfull", LoadFlag::TARGET_NORMAL),
        ("hintslight", LoadFlag::TARGET_LIGHT),
    ] {
        // The fixture owns its font and rules; it neither reads nor changes the
        // process configuration. Targets remain distinct even if this native
        // hinter produces identical advances for them (FreeType permits that).
        let rules = format!(
            "<match target=\"pattern\"><edit name=\"hintstyle\" mode=\"assign\"><const>{policy}</const></edit><edit name=\"rgba\" mode=\"assign\"><const>none</const></edit></match>"
        );
        let config = Config::new(path, &rules);
        let context = config.context();
        let library = freetype::Library::init().unwrap();
        let face = library.new_face(path, 0).unwrap();
        let reference = library.new_face(path, 0).unwrap();
        for size in [14, 7] {
            let options = context.options_for_file(path, 0, size as f32).unwrap();
            assert_eq!(options.metrics_load_flags().bits(), target.bits());
            face.set_pixel_sizes(size, size).unwrap();
            reference.set_pixel_sizes(size, size).unwrap();
            let mut advances = Vec::new();
            let mut expected_advances = Vec::new();
            for character in 32..127 {
                assert!(reference.get_char_index(character).is_some());
                face.load_char(character, options.metrics_load_flags())
                    .unwrap();
                let advance = (face.glyph().advance().x + 32) >> 6;
                // This separate face bypasses production options and uses the
                // requested FT target directly, on the same library build.
                reference.load_char(character, target).unwrap();
                let expected = (reference.glyph().advance().x + 32) >> 6;
                assert_eq!(advance, expected, "{policy}/{size}/U+{character:04X}");
                if advance > 0 {
                    advances.push(advance);
                }
                if expected > 0 {
                    expected_advances.push(expected);
                }
            }
            assert!(!advances.is_empty());
            assert_eq!(
                advances
                    .iter()
                    .map(|value| i128::from(*value))
                    .sum::<i128>()
                    / advances.len() as i128,
                expected_advances
                    .iter()
                    .map(|value| i128::from(*value))
                    .sum::<i128>()
                    / expected_advances.len() as i128,
                "{policy}/{size} ASCII average"
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

#[test]
fn explicit_variable_weight_does_not_invent_synthetic_bold() {
    let path = neomacs_test_fonts::noto_sans_variable();
    // The standard synthetic-bold rule exposes a query incorrectly pinned to
    // the raw file's Regular/default instance while requesting a real weight.
    let config = Config::new(
        path,
        r#"
<match target="font">
<test name="weight" compare="less_eq"><const>medium</const></test>
<test target="pattern" name="weight" compare="more"><const>medium</const></test>
<edit name="embolden" mode="assign"><bool>true</bool></edit>
</match>"#,
    );
    let context = config.context();
    for weight in [800.0, 900.0] {
        assert!(
            context
                .options_for_file_with_weight(path, 0, 1.0, Some(weight))
                .is_some(),
            "real weight {weight} must not require synthetic bold"
        );
    }
    assert!(
        context
            .options_for_file_with_weight(path, 0, 1.0, None)
            .is_some(),
        "the unmodified default instance still resolves"
    );
}

#[test]
fn explicit_synthetic_bold_rendering_is_still_refused() {
    let path = neomacs_test_fonts::noto_sans_variable();
    for target in ["pattern", "font"] {
        let rules = format!(
            "<match target=\"{target}\"><edit name=\"embolden\" mode=\"assign\"><bool>true</bool></edit></match>"
        );
        let config = Config::new(path, &rules);
        assert!(
            config
                .context()
                .options_for_file_with_weight(path, 0, 1.0, Some(900.0))
                .is_none(),
            "explicit {target} synthesis must retain the native adapter guard"
        );
    }
}

use super::*;

/// Ground truth captured from the GNU Emacs 31.0.50 cairo build on this
/// machine (Xvfb, `(font-info (find-font (font-spec :family ...)))`); see
/// the render-boundary design notes. Immutable release assets make every
/// expectation execute on hosts without a Nix-store font installation.
fn assert_gnu_probe(file: &str, size: u32, expect: FontPxMetrics) {
    // These historical measurements used slight hinting. Make that recorded
    // policy explicit rather than depending on this host's Fontconfig defaults.
    let config = probe_config(file, "hintslight");
    let got = probe_font_px_metrics_with_policy(file, 0, size, None, |size| {
        config.render_options(file, 0, size as f32, None)
    })
    .expect("probe succeeds");
    assert_eq!(got, expect, "probe of {file} at px={size}");
}

#[test]
fn gnu_parity_noto_sans_px1() {
    // GNU font-info: "... 1 3 0 0 0 1 2 1 0 1" => size 1 height 3
    // max-width 1 ascent 2 descent 1 space-width 0 average-width 1.
    assert_gnu_probe(
        neomacs_test_fonts::noto_sans_variable()
            .to_str()
            .expect("UTF-8 pinned Noto path"),
        1,
        FontPxMetrics {
            pixel_size: 1,
            height: 3,
            ascent: 2,
            descent: 1,
            max_width: 1,
            space_width: 0,
            average_width: 1,
        },
    );
}

#[test]
fn gnu_parity_hack_px1() {
    // GNU font-info: "... 1 2 0 0 0 1 1 1 1 1".
    assert_gnu_probe(
        neomacs_test_fonts::hack_italic_3_003()
            .to_str()
            .expect("UTF-8 pinned Hack path"),
        1,
        FontPxMetrics {
            pixel_size: 1,
            height: 2,
            ascent: 1,
            descent: 1,
            max_width: 1,
            space_width: 1,
            average_width: 1,
        },
    );
}

#[test]
fn gnu_parity_dejavu_sans_mono_px1() {
    // GNU font-info: "... 1 2 0 0 0 1 1 1 1 1".
    assert_gnu_probe(
        neomacs_test_fonts::dejavu_2_37()
            .mono_bold_oblique()
            .to_str()
            .expect("UTF-8 pinned DejaVu path"),
        1,
        FontPxMetrics {
            pixel_size: 1,
            height: 2,
            ascent: 1,
            descent: 1,
            max_width: 1,
            space_width: 1,
            average_width: 1,
        },
    );
}

#[test]
fn noto_sans_named_instances_cover_thin_to_black() {
    let file = neomacs_test_fonts::noto_sans_variable()
        .to_str()
        .expect("UTF-8 pinned Noto path");
    let weights = named_instance_wght_values(file, 0);
    // GNU/fontconfig snap requests to these; the fvar table must expose the
    // standard weight ladder (no synthesized 350/950).
    for w in [100u16, 200, 300, 400, 500, 600, 700, 800, 900] {
        assert!(
            weights.contains(&w),
            "expected instance weight {w} in {weights:?}"
        );
    }
    assert!(!weights.contains(&350), "350 is not a named instance");
    assert!(!weights.contains(&950), "950 is not a named instance");
}

fn probe_config(
    file: &str,
    hintstyle: &str,
) -> crate::font::fontconfig::test_config::TestFontconfig {
    let library = Library::init().unwrap();
    let face = library.new_face(file, 0).unwrap();
    let alias = crate::font_backend::FontFamilyName::new(face.family_name().unwrap()).unwrap();
    let rules = format!(
        "<match target=\"pattern\"><edit name=\"hintstyle\" mode=\"assign\"><const>{hintstyle}</const></edit><edit name=\"rgba\" mode=\"assign\"><const>none</const></edit></match>"
    );
    crate::font::fontconfig::test_config::TestFontconfig::with_pattern_rules(
        &[std::path::PathBuf::from(file)],
        &alias,
        &alias,
        &alias,
        &rules,
    )
    .unwrap()
}

/// Independent same-build FreeType reference: no production policy/probe helpers.
/// Native hinters may ignore a requested target, and interpreter builds differ;
/// target selection is invariant, while pixel values belong to this FT build.
fn native_ascii_reference(file: &str, size: u32, target: LoadFlag) -> (FontPxMetrics, [i32; 128]) {
    let library = Library::init().unwrap();
    let face = library.new_face(file, 0).unwrap();
    face.set_pixel_sizes(size, size).unwrap();
    let mut widths = [0; 128];
    for byte in 32u8..127 {
        assert!(face.get_char_index(usize::from(byte)).is_some());
        face.load_char(usize::from(byte), target).unwrap();
        widths[usize::from(byte)] = ((face.glyph().advance().x + 32) >> 6) as i32;
    }
    let positive: Vec<_> = widths[32..127]
        .iter()
        .copied()
        .filter(|width| *width > 0)
        .collect();
    assert!(!positive.is_empty());
    let metrics = face.size_metrics().unwrap();
    let ascent = ((metrics.ascender + 32) >> 6) as i32;
    let descent = ((-metrics.descender + 32) >> 6) as i32;
    (
        FontPxMetrics {
            pixel_size: size,
            height: ascent + descent,
            ascent,
            descent,
            max_width: *positive.iter().max().unwrap(),
            space_width: widths[32],
            average_width: (positive.iter().map(|width| i64::from(*width)).sum::<i64>()
                / positive.len() as i64) as i32,
        },
        widths,
    )
}

#[test]
fn full_and_slight_noto_ascii_probes_keep_selected_policy() {
    let file = neomacs_test_fonts::noto_sans_variable().to_str().unwrap();
    for (policy, target) in [
        ("hintfull", LoadFlag::TARGET_NORMAL),
        ("hintslight", LoadFlag::TARGET_LIGHT),
    ] {
        let config = probe_config(file, policy);
        for size in [14, 7] {
            let options = config.render_options(file, 0, size as f32, None).unwrap();
            assert_eq!(options.metrics_load_flags().bits(), target.bits());
            let (reference, widths) = native_ascii_reference(file, size, target);
            let request = ExactDeviceFontRequest {
                file,
                selector: FreeTypeFaceSelector(0),
                size: DeviceFontPixelSize::new(size).unwrap(),
                weight: DeviceFontWeight::Default,
                options: Some(options),
            };
            let advances = probe_device_ascii_advances_for_request(&request).unwrap();
            assert_eq!(advances.device_pixel_size(), size);
            assert_eq!(advances.widths, widths, "{policy}/{size} ASCII");
            let metrics = probe_font_px_metrics_with_policy(file, 0, size, None, |size| {
                config.render_options(file, 0, size as f32, None)
            })
            .unwrap();
            assert_eq!(metrics, reference, "{policy}/{size} aggregate and vertical");
        }
    }
}

#[test]
fn fixture_default_and_explicit_hinting_policies_stay_independent() {
    use crate::font::fontconfig::test_config::TestFontconfig;
    use crate::font_backend::FontFamilyName;
    let file = neomacs_test_fonts::noto_sans_mono_variable();
    let family = FontFamilyName::new("Noto Sans Mono").unwrap();
    let files = [file.to_path_buf()];
    let default = TestFontconfig::new(&files, &family, &family, &family).unwrap();
    let full = TestFontconfig::with_pattern_rules(
        &files,
        &family,
        &family,
        &family,
        "<match target=\"pattern\"><edit name=\"hintstyle\" mode=\"assign\"><const>hintfull</const></edit></match>",
    )
    .unwrap();
    // Owners must retain different policies even on a FreeType build where
    // the two targets happen to produce identical rounded glyph advances.
    let mut observations = Vec::new();
    for (config, target) in [
        (&default, LoadFlag::TARGET_LIGHT),
        (&full, LoadFlag::TARGET_NORMAL),
        (&default, LoadFlag::TARGET_LIGHT),
    ] {
        let file = file.to_str().unwrap();
        let options = config.render_options(file, 0, 14.0, None).unwrap();
        assert_eq!(options.metrics_load_flags().bits(), target.bits());
        let (_, expected) = native_ascii_reference(file, 14, target);
        let request = ExactDeviceFontRequest {
            file,
            selector: FreeTypeFaceSelector(0),
            size: DeviceFontPixelSize::new(14).unwrap(),
            weight: DeviceFontWeight::Default,
            options: Some(options),
        };
        let advances = probe_device_ascii_advances_for_request(&request).unwrap();
        assert_eq!(advances.widths, expected);
        observations.push(advances.widths);
    }
    assert_eq!(
        observations[0], observations[2],
        "default before/after full"
    );
}

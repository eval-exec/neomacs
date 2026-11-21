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

#[test]
fn full_and_slight_noto_ascii_probes_keep_selected_policy() {
    let file = neomacs_test_fonts::noto_sans_variable().to_str().unwrap();
    for (policy, small_width) in [("hintfull", 5.0), ("hintslight", 7.0)] {
        let config = probe_config(file, policy);
        for (size, width, average) in [(14, 13.0, 7), (7, small_width, 3)] {
            let request = ExactDeviceFontRequest {
                file,
                selector: FreeTypeFaceSelector(0),
                size: DeviceFontPixelSize::new(size).unwrap(),
                weight: DeviceFontWeight::Default,
                options: Some(config.render_options(file, 0, size as f32, None).unwrap()),
            };
            let advances = probe_device_ascii_advances_for_request(&request).unwrap();
            assert_eq!(
                advances.logical_advance('m', neomacs_display_protocol::geometry::DeviceScale::ONE),
                Some(width),
                "{policy}/{size}"
            );
            let metrics = probe_font_px_metrics_with_policy(file, 0, size, None, |size| {
                config.render_options(file, 0, size as f32, None)
            })
            .unwrap();
            assert_eq!(metrics.average_width, average, "{policy}/{size}");
        }
    }
}

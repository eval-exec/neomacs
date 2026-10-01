use super::*;

/// Ground truth captured from the GNU Emacs 31.0.50 cairo build on this
/// machine (Xvfb, `(font-info (find-font (font-spec :family ...)))`); see
/// the render-boundary design notes. Each test skips when the nix-store
/// font file is absent so the suite stays green on other machines.
fn assert_gnu_probe(file: &str, size: u32, expect: FontPxMetrics) {
    if !std::path::Path::new(file).exists() {
        eprintln!("skipping: {file} not present");
        return;
    }
    let got = probe_font_px_metrics(file, 0, size, None).expect("probe succeeds");
    assert_eq!(got, expect, "probe of {file} at px={size}");
}

#[test]
fn gnu_parity_noto_sans_px1() {
    // GNU font-info: "... 1 3 0 0 0 1 2 1 0 1" => size 1 height 3
    // max-width 1 ascent 2 descent 1 space-width 0 average-width 1.
    assert_gnu_probe(
        "/nix/store/7lrhms8rphrd8ywphjbvjyll57pkim64-noto-fonts-2025.11.01/share/fonts/noto/NotoSans[wdth,wght].ttf",
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
        "/nix/store/b7ybgcl00ak8q66bc0w15vfnyly4g13k-hack-font-3.003/share/fonts/truetype/Hack-Italic.ttf",
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
        "/nix/store/b5gf37jp4y3965bp6x9wanzqchkkvbvs-dejavu-fonts-2.37/share/fonts/truetype/DejaVuSansMono-BoldOblique.ttf",
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
    let file = "/nix/store/7lrhms8rphrd8ywphjbvjyll57pkim64-noto-fonts-2025.11.01/share/fonts/noto/NotoSans[wdth,wght].ttf";
    if !std::path::Path::new(file).exists() {
        eprintln!("skipping: {file} not present");
        return;
    }
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

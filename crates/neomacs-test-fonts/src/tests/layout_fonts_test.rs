use crate::*;

struct ExpectedFamily(&'static str);
struct ExpectedStyle(&'static str);

fn assert_identity(path: &Path, family: ExpectedFamily, style: ExpectedStyle) {
    let data = std::fs::read(path).expect("verified fixture bytes");
    let face = ttf_parser::Face::parse(&data, 0).expect("verified outline face");
    let names: Vec<_> = face
        .names()
        .into_iter()
        .filter(|name| name.is_unicode())
        .filter_map(|name| name.to_string().map(|text| (name.name_id, text)))
        .collect();
    assert!(
        names.iter().any(|(id, text)| matches!(
            *id,
            ttf_parser::name_id::FAMILY | ttf_parser::name_id::TYPOGRAPHIC_FAMILY
        ) && text == family.0),
        "{} must contain the exact family {:?}: {names:?}",
        path.display(),
        family.0
    );
    assert!(
        names.iter().any(|(id, text)| matches!(
            *id,
            ttf_parser::name_id::SUBFAMILY | ttf_parser::name_id::TYPOGRAPHIC_SUBFAMILY
        ) && text == style.0),
        "{} must contain the exact style {:?}: {names:?}",
        path.display(),
        style.0
    );
}

#[test]
fn verified_latin_asset_apis_preserve_real_families_and_styles() {
    let d = dejavu_2_37();
    let u = ubuntu_0_83();
    for (path, family, style) in [
        (d.mono_regular(), "DejaVu Sans Mono", "Book"),
        (d.mono_bold(), "DejaVu Sans Mono", "Bold"),
        (d.mono_oblique(), "DejaVu Sans Mono", "Oblique"),
        (d.mono_bold_oblique(), "DejaVu Sans Mono", "Bold Oblique"),
        (d.sans_regular(), "DejaVu Sans", "Book"),
        (d.sans_bold(), "DejaVu Sans", "Bold"),
        (d.sans_oblique(), "DejaVu Sans", "Oblique"),
        (d.sans_bold_oblique(), "DejaVu Sans", "Bold Oblique"),
        (d.serif_regular(), "DejaVu Serif", "Book"),
        (d.serif_bold(), "DejaVu Serif", "Bold"),
        (d.serif_italic(), "DejaVu Serif", "Italic"),
        (d.serif_bold_italic(), "DejaVu Serif", "Bold Italic"),
        (u.mono_regular(), "Ubuntu Mono", "Regular"),
        (u.mono_bold(), "Ubuntu Mono", "Bold"),
        (u.mono_italic(), "Ubuntu Mono", "Italic"),
        (u.mono_bold_italic(), "Ubuntu Mono", "Bold Italic"),
        (u.regular(), "Ubuntu", "Regular"),
        (u.bold(), "Ubuntu", "Bold"),
        (u.italic(), "Ubuntu", "Italic"),
        (u.bold_italic(), "Ubuntu", "Bold Italic"),
        (noto_sans_variable(), "Noto Sans", "Regular"),
        (noto_sans_italic_variable(), "Noto Sans", "Italic"),
        (hack_regular_3_003(), "Hack", "Regular"),
        (hack_bold_3_003(), "Hack", "Bold"),
        (hack_italic_3_003(), "Hack", "Italic"),
        (jetbrains_mono_regular_2_304(), "JetBrains Mono", "Regular"),
        (jetbrains_mono_bold_2_304(), "JetBrains Mono", "Bold"),
        (jetbrains_mono_italic_2_304(), "JetBrains Mono", "Italic"),
        (
            jetbrains_mono_nerd_regular_3_4_0(),
            "JetBrainsMono Nerd Font",
            "Regular",
        ),
        (
            jetbrains_mono_nerd_bold_3_4_0(),
            "JetBrainsMono Nerd Font",
            "Bold",
        ),
        (
            jetbrains_mono_nerd_italic_3_4_0(),
            "JetBrainsMono Nerd Font",
            "Italic",
        ),
        (
            symbols_nerd_mono_3_4_0(),
            "Symbols Nerd Font Mono",
            "Regular",
        ),
    ] {
        assert_identity(path, ExpectedFamily(family), ExpectedStyle(style));
    }
}

#[test]
fn verified_script_and_native_conformance_assets_keep_their_exact_families() {
    let iosevka = iosevka_33_2_9();
    for (path, family, style) in [
        (
            noto_sans_devanagari_variable(),
            "Noto Sans Devanagari",
            "Regular",
        ),
        (noto_sans_thai_variable(), "Noto Sans Thai", "Regular"),
        (noto_sans_cjk_variable(), "Noto Sans CJK JP", "Regular"),
        (iosevka.regular(), "Iosevka", "Regular"),
        (iosevka.bold(), "Iosevka", "Bold"),
    ] {
        assert_identity(path, ExpectedFamily(family), ExpectedStyle(style));
    }
}

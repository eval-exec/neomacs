use super::*;

#[test]
fn width_distance_precedes_weight_distance() {
    let wrong_weight = candidate_selection_score(
        0,
        13 * 64,
        FontWeight::NORMAL,
        FontSlant::Normal,
        Some(FontWidth::Normal),
        FontWeight::BLACK,
        FontSlant::Normal,
        Some(FontWidth::Normal),
        PlatformFontSize::Scalable,
    )
    .expect("scalable candidate");
    let wrong_width = candidate_selection_score(
        0,
        13 * 64,
        FontWeight::NORMAL,
        FontSlant::Normal,
        Some(FontWidth::Normal),
        FontWeight::NORMAL,
        FontSlant::Normal,
        Some(FontWidth::Expanded),
        PlatformFontSize::Scalable,
    )
    .expect("scalable candidate");
    assert!(wrong_weight < wrong_width);
}

#[test]
fn weight_distance_precedes_slant_distance() {
    let wrong_slant = candidate_selection_score(
        0,
        13 * 64,
        FontWeight::NORMAL,
        FontSlant::Normal,
        Some(FontWidth::Normal),
        FontWeight::NORMAL,
        FontSlant::Italic,
        Some(FontWidth::Normal),
        PlatformFontSize::Scalable,
    )
    .expect("scalable candidate");
    let wrong_weight = candidate_selection_score(
        0,
        13 * 64,
        FontWeight::NORMAL,
        FontSlant::Normal,
        Some(FontWidth::Normal),
        FontWeight::MEDIUM,
        FontSlant::Normal,
        Some(FontWidth::Normal),
        PlatformFontSize::Scalable,
    )
    .expect("scalable candidate");
    assert!(wrong_slant < wrong_weight);
}

#[test]
fn fixed_size_distance_uses_integer_pixels_and_the_gnu_seven_bit_cap() {
    assert_eq!(
        size_distance(
            100 * 64,
            PlatformFontSize::Fixed {
                device_ppem_26_6: 164 * 64,
            },
        ),
        Some(PropertyDistance(127))
    );
    assert_eq!(
        size_distance(
            100 * 64,
            PlatformFontSize::Fixed {
                device_ppem_26_6: 200 * 64,
            },
        ),
        Some(PropertyDistance(127))
    );
    assert_eq!(
        size_distance(
            100 * 64,
            PlatformFontSize::Fixed {
                device_ppem_26_6: 201 * 64,
            },
        ),
        None
    );
}

#[test]
fn unknown_size_is_never_scored_as_scalable() {
    assert_eq!(size_distance(13 * 64, PlatformFontSize::Unknown), None);
}

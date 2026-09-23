use super::*;

#[test]
fn box_line_width_preserves_gnu_inside_and_outside_semantics() {
    let scale = DeviceScale::new(2.0).unwrap();
    let inside = BoxLineWidth::from_gnu(-2);
    assert!(inside.is_visible());
    assert!(!inside.expands_row_height());
    let inside_geometry = inside.logical_geometry(scale);
    assert_eq!(inside_geometry.paint_thickness().get(), 1.0);
    assert_eq!(inside_geometry.row_expansion_per_edge().get(), 0.0);

    let outside = BoxLineWidth::from_gnu(2);
    assert!(outside.is_visible());
    assert!(outside.expands_row_height());
    let outside_geometry = outside.logical_geometry(scale);
    assert_eq!(outside_geometry.paint_thickness().get(), 1.0);
    assert_eq!(outside_geometry.row_expansion_per_edge().get(), 1.0);
}

#[test]
fn default_box_width_stays_one_device_pixel_at_fractional_scale() {
    let geometry = BoxLineWidth::from_gnu(1).logical_geometry(DeviceScale::new(1.5).unwrap());

    assert!((geometry.paint_thickness().get() - 0.666_666_7).abs() < 1.0e-6);
    assert!((geometry.row_expansion_per_edge().get() - 0.666_666_7).abs() < 1.0e-6);
}

#[test]
fn basic_face_ids_preserve_gnu_slots_and_names() {
    let faces = [
        (BasicFaceId::Default, 0, "default"),
        (BasicFaceId::ModeLineActive, 1, "mode-line-active"),
        (BasicFaceId::ModeLineInactive, 2, "mode-line-inactive"),
        (BasicFaceId::ToolBar, 3, "tool-bar"),
        (BasicFaceId::Fringe, 4, "fringe"),
        (BasicFaceId::HeaderLineActive, 5, "header-line-active"),
        (BasicFaceId::HeaderLineInactive, 6, "header-line-inactive"),
        (BasicFaceId::ScrollBar, 7, "scroll-bar"),
        (BasicFaceId::Border, 8, "border"),
        (BasicFaceId::Cursor, 9, "cursor"),
        (BasicFaceId::Mouse, 10, "mouse"),
        (BasicFaceId::Menu, 11, "menu"),
        (BasicFaceId::VerticalBorder, 12, "vertical-border"),
        (BasicFaceId::WindowDivider, 13, "window-divider"),
        (
            BasicFaceId::WindowDividerFirstPixel,
            14,
            "window-divider-first-pixel",
        ),
        (
            BasicFaceId::WindowDividerLastPixel,
            15,
            "window-divider-last-pixel",
        ),
        (BasicFaceId::InternalBorder, 16, "internal-border"),
        (BasicFaceId::ChildFrameBorder, 17, "child-frame-border"),
        (BasicFaceId::TabBar, 18, "tab-bar"),
        (BasicFaceId::TabLine, 19, "tab-line"),
    ];

    for (face, id, name) in faces {
        assert_eq!(u32::from(face), id);
        assert_eq!(face.gnu_code(), id);
        assert_eq!(BasicFaceId::from_gnu_code(id), Some(face));
        assert_eq!(face.name(), name);
        assert_eq!(BasicFaceId::from_name(name), Some(face));
    }
    assert_eq!(BasicFaceId::SENTINEL, 20);
    assert_eq!(BasicFaceId::from_gnu_code(BasicFaceId::SENTINEL), None);
}

#[test]
fn underline_style_codes_match_gnu_face_underline_type() {
    let styles = [
        (UnderlineStyle::None, 0),
        (UnderlineStyle::Line, 1),
        (UnderlineStyle::Double, 2),
        (UnderlineStyle::Wave, 3),
        (UnderlineStyle::Dotted, 4),
        (UnderlineStyle::Dashed, 5),
    ];

    for (style, code) in styles {
        assert_eq!(style.gnu_code(), code);
        assert_eq!(UnderlineStyle::from_gnu_code(code), Some(style));
    }

    assert_eq!(UnderlineStyle::from_gnu_code(6), None);
}

#[test]
fn box_type_codes_match_gnu_face_box_type() {
    let boxes = [
        (BoxType::None, 0),
        (BoxType::Line, 1),
        (BoxType::Raised3D, 2),
        (BoxType::Sunken3D, 3),
    ];

    for (box_type, code) in boxes {
        assert_eq!(box_type.gnu_code(), code);
        assert_eq!(BoxType::from_gnu_code(code), Some(box_type));
    }

    assert_eq!(BoxType::from_gnu_code(4), None);
}

#[test]
fn box_border_style_codes_round_trip_fill_face_data_values() {
    let styles = [
        (BoxBorderStyle::Solid, 0),
        (BoxBorderStyle::Rainbow, 1),
        (BoxBorderStyle::AnimatedRainbow, 2),
        (BoxBorderStyle::Gradient, 3),
        (BoxBorderStyle::Glow, 4),
        (BoxBorderStyle::Neon, 5),
        (BoxBorderStyle::Dashed, 6),
        (BoxBorderStyle::Comet, 7),
        (BoxBorderStyle::Iridescent, 8),
        (BoxBorderStyle::Fire, 9),
        (BoxBorderStyle::Heartbeat, 10),
    ];

    for (style, code) in styles {
        assert_eq!(style.gnu_code(), code);
        assert_eq!(BoxBorderStyle::from_gnu_code(code), Some(style));
    }

    assert_eq!(BoxBorderStyle::from_gnu_code(11), None);
    assert!(!BoxBorderStyle::Solid.is_fancy());
    assert!(BoxBorderStyle::Rainbow.is_fancy());
}

#[test]
fn basic_face_id_rejects_fringe_area_symbols() {
    assert_eq!(BasicFaceId::from_name("fringe"), Some(BasicFaceId::Fringe));
    assert_eq!(BasicFaceId::from_name("left-fringe"), None);
    assert_eq!(BasicFaceId::from_name("right-fringe"), None);
    assert_eq!(BasicFaceId::from_name("unknown-face"), None);
}

#[test]
fn test_face_creation() {
    let face = Face::new(FaceId::new(1));
    assert_eq!(face.id, FaceId::new(1));
    assert!(!face.is_bold());
}

#[test]
fn test_pango_font_desc() {
    let mut face = Face::new(FaceId::new(0));
    face.font_family = "DejaVu Sans Mono".to_string();
    face.font_size = 14.0;
    face.attributes = FaceAttributes::BOLD | FaceAttributes::ITALIC;

    let desc = face.to_pango_font_description();
    assert!(desc.contains("DejaVu Sans Mono"));
    assert!(desc.contains("Bold"));
    assert!(desc.contains("Italic"));
    assert!(desc.contains("14"));
}

#[test]
fn test_default_face_values() {
    let face = Face::default();
    assert_eq!(face.id, FaceId::new(0));
    assert_eq!(face.foreground, Color::WHITE);
    assert_eq!(face.background, Color::BLACK);
    assert_eq!(face.font_family, "monospace");
    assert_eq!(face.font_size, 10.0);
    assert_eq!(face.font_weight, 400);
    assert_eq!(face.attributes, FaceAttributes::empty());
    assert_eq!(face.underline_style, UnderlineStyle::None);
    assert_eq!(face.box_type, BoxType::None);
    assert_eq!(face.box_line_width.gnu_value(), 0);
    assert_eq!(face.box_corner_radius, 0);
    assert!(face.underline_color.is_none());
    assert!(face.overline_color.is_none());
    assert!(face.strike_through_color.is_none());
    assert!(face.box_color.is_none());
    assert!(face.font_file_path.is_none());
    assert_eq!(face.font_ascent, 0);
    assert_eq!(face.font_descent, 0);
    assert_eq!(face.underline_position, 1);
    assert_eq!(face.underline_placement, UnderlinePosition::default());
    assert_eq!(face.underline_thickness, 1);
}

#[test]
fn test_face_foreground_background_colors() {
    let mut face = Face::new(FaceId::new(1));
    let red = Color::rgb(1.0, 0.0, 0.0);
    let blue = Color::rgb(0.0, 0.0, 1.0);
    face.foreground = red;
    face.background = blue;
    assert_eq!(face.foreground, Color::RED);
    assert_eq!(face.background, Color::BLUE);
}

#[test]
fn test_bold_via_attribute_flag() {
    let mut face = Face::new(FaceId::new(2));
    assert!(!face.is_bold());
    face.attributes |= FaceAttributes::BOLD;
    assert!(face.is_bold());
    // font_weight stays at 400 but is_bold returns true via attribute
    assert_eq!(face.font_weight, 400);
}

#[test]
fn test_bold_via_font_weight() {
    let mut face = Face::new(FaceId::new(3));
    assert!(!face.is_bold());
    // Bold via high font_weight without the BOLD attribute flag
    face.font_weight = 700;
    assert!(face.is_bold());
    assert!(!face.attributes.contains(FaceAttributes::BOLD));

    // Extra-bold weight
    face.font_weight = 900;
    assert!(face.is_bold());

    // Semi-bold (600) should NOT be bold
    face.font_weight = 600;
    assert!(!face.is_bold());
}

#[test]
fn test_italic_attribute() {
    let mut face = Face::new(FaceId::new(4));
    assert!(!face.is_italic());
    face.attributes |= FaceAttributes::ITALIC;
    assert!(face.is_italic());
}

#[test]
fn test_underline_style_none() {
    let face = Face::new(FaceId::new(5));
    assert!(!face.has_underline());
    assert_eq!(face.underline_style, UnderlineStyle::None);
}

#[test]
fn test_underline_style_line() {
    let mut face = Face::new(FaceId::new(6));
    face.underline_style = UnderlineStyle::Line;
    assert!(face.has_underline());
}

#[test]
fn test_underline_style_wave() {
    let mut face = Face::new(FaceId::new(7));
    face.underline_style = UnderlineStyle::Wave;
    assert!(face.has_underline());
}

#[test]
fn test_underline_style_double() {
    let mut face = Face::new(FaceId::new(8));
    face.underline_style = UnderlineStyle::Double;
    assert!(face.has_underline());
}

#[test]
fn test_underline_style_dotted() {
    let mut face = Face::new(FaceId::new(9));
    face.underline_style = UnderlineStyle::Dotted;
    assert!(face.has_underline());
}

#[test]
fn test_underline_style_dashed() {
    let mut face = Face::new(FaceId::new(10));
    face.underline_style = UnderlineStyle::Dashed;
    assert!(face.has_underline());
}

#[test]
fn test_all_underline_styles_detected() {
    // Verify every non-None variant is detected by has_underline
    let styles = [
        UnderlineStyle::Line,
        UnderlineStyle::Wave,
        UnderlineStyle::Double,
        UnderlineStyle::Dotted,
        UnderlineStyle::Dashed,
    ];
    for style in &styles {
        let mut face = Face::new(FaceId::new(0));
        face.underline_style = *style;
        assert!(
            face.has_underline(),
            "has_underline() should be true for {:?}",
            style
        );
    }
    // None should NOT be detected
    let mut face = Face::new(FaceId::new(0));
    face.underline_style = UnderlineStyle::None;
    assert!(!face.has_underline());
}

#[test]
fn test_underline_color_fallback_to_foreground() {
    let mut face = Face::new(FaceId::new(11));
    face.foreground = Color::RED;
    face.underline_color = None;
    // When no explicit underline color, get_underline_color returns foreground
    assert_eq!(face.get_underline_color(), Color::RED);
}

#[test]
fn test_underline_color_explicit() {
    let mut face = Face::new(FaceId::new(12));
    face.foreground = Color::RED;
    face.underline_color = Some(Color::BLUE);
    // When explicit underline color is set, it takes precedence
    assert_eq!(face.get_underline_color(), Color::BLUE);
}

#[test]
fn test_strike_through_attribute() {
    let mut face = Face::new(FaceId::new(13));
    assert!(!face.attributes.contains(FaceAttributes::STRIKE_THROUGH));
    face.attributes |= FaceAttributes::STRIKE_THROUGH;
    assert!(face.attributes.contains(FaceAttributes::STRIKE_THROUGH));
}

#[test]
fn test_overline_attribute() {
    let mut face = Face::new(FaceId::new(14));
    assert!(!face.attributes.contains(FaceAttributes::OVERLINE));
    face.attributes |= FaceAttributes::OVERLINE;
    assert!(face.attributes.contains(FaceAttributes::OVERLINE));
}

#[test]
fn test_inverse_attribute() {
    let mut face = Face::new(FaceId::new(15));
    assert!(!face.attributes.contains(FaceAttributes::INVERSE));
    face.attributes |= FaceAttributes::INVERSE;
    assert!(face.attributes.contains(FaceAttributes::INVERSE));
}

#[test]
fn test_strike_through_and_overline_colors() {
    let mut face = Face::new(FaceId::new(16));
    assert!(face.strike_through_color.is_none());
    assert!(face.overline_color.is_none());
    face.strike_through_color = Some(Color::GREEN);
    face.overline_color = Some(Color::BLUE);
    assert_eq!(face.strike_through_color.unwrap(), Color::GREEN);
    assert_eq!(face.overline_color.unwrap(), Color::BLUE);
}

#[test]
fn test_box_attribute_and_types() {
    let mut face = Face::new(FaceId::new(17));
    assert_eq!(face.box_type, BoxType::None);
    assert!(!face.attributes.contains(FaceAttributes::BOX));

    // Line box
    face.box_type = BoxType::Line;
    face.attributes |= FaceAttributes::BOX;
    face.box_line_width = 2.into();
    face.box_corner_radius = 4;
    face.box_color = Some(Color::RED);
    assert!(face.attributes.contains(FaceAttributes::BOX));
    assert_eq!(face.box_type, BoxType::Line);
    assert_eq!(face.box_line_width.gnu_value(), 2);
    assert_eq!(face.box_corner_radius, 4);
    assert_eq!(face.box_color.unwrap(), Color::RED);

    // Raised3D box
    face.box_type = BoxType::Raised3D;
    assert_eq!(face.box_type, BoxType::Raised3D);

    // Sunken3D box
    face.box_type = BoxType::Sunken3D;
    assert_eq!(face.box_type, BoxType::Sunken3D);
}

#[test]
fn ffi_face_data_preserves_gnu_box_type_codes() {
    let boxes = [
        (1, BoxType::Line),
        (2, BoxType::Raised3D),
        (3, BoxType::Sunken3D),
    ];

    for (code, box_type) in boxes {
        let ffi = FaceDataFFI {
            box_type: code,
            box_line_width: 1,
            ..Default::default()
        };
        let face = unsafe { ffi.to_face() };
        assert_eq!(face.box_type, box_type);
        assert!(face.attributes.contains(FaceAttributes::BOX));
    }
}

#[test]
fn test_combined_attributes() {
    let mut face = Face::new(FaceId::new(18));
    face.attributes = FaceAttributes::BOLD
        | FaceAttributes::ITALIC
        | FaceAttributes::UNDERLINE
        | FaceAttributes::STRIKE_THROUGH
        | FaceAttributes::OVERLINE;
    assert!(face.attributes.contains(FaceAttributes::BOLD));
    assert!(face.attributes.contains(FaceAttributes::ITALIC));
    assert!(face.attributes.contains(FaceAttributes::UNDERLINE));
    assert!(face.attributes.contains(FaceAttributes::STRIKE_THROUGH));
    assert!(face.attributes.contains(FaceAttributes::OVERLINE));
    assert!(!face.attributes.contains(FaceAttributes::INVERSE));
    assert!(!face.attributes.contains(FaceAttributes::BOX));
    assert!(face.is_bold());
    assert!(face.is_italic());
}

#[test]
fn test_pango_font_desc_plain() {
    // No bold, no italic — should just be family + size
    let mut face = Face::new(FaceId::new(0));
    face.font_family = "Fira Code".to_string();
    face.font_size = 16.0;
    let desc = face.to_pango_font_description();
    assert_eq!(desc, "Fira Code 16");
}

#[test]
fn test_pango_font_desc_bold_only() {
    let mut face = Face::new(FaceId::new(0));
    face.font_family = "monospace".to_string();
    face.font_size = 10.0;
    face.attributes = FaceAttributes::BOLD;
    let desc = face.to_pango_font_description();
    assert_eq!(desc, "monospace Bold 10");
}

#[test]
fn test_pango_font_desc_italic_only() {
    let mut face = Face::new(FaceId::new(0));
    face.font_family = "monospace".to_string();
    face.font_size = 10.0;
    face.attributes = FaceAttributes::ITALIC;
    let desc = face.to_pango_font_description();
    assert_eq!(desc, "monospace Italic 10");
}

#[test]
fn test_pango_font_desc_bold_via_weight() {
    // Bold should appear in description when font_weight >= 700 even without BOLD attribute
    let mut face = Face::new(FaceId::new(0));
    face.font_family = "serif".to_string();
    face.font_size = 12.0;
    face.font_weight = 700;
    let desc = face.to_pango_font_description();
    assert!(desc.contains("Bold"));
    assert_eq!(desc, "serif Bold 12");
}

#[test]
fn test_pango_font_desc_truncates_size() {
    // font_size 13.7 should be truncated to 13 (cast as i32)
    let mut face = Face::new(FaceId::new(0));
    face.font_family = "monospace".to_string();
    face.font_size = 13.7;
    let desc = face.to_pango_font_description();
    assert_eq!(desc, "monospace 13");
}

#[test]
fn test_font_weight_and_slant_values() {
    let mut face = Face::new(FaceId::new(19));
    // Test various CSS font weight values
    face.font_weight = 100; // Thin
    assert!(!face.is_bold());
    face.font_weight = 300; // Light
    assert!(!face.is_bold());
    face.font_weight = 400; // Normal
    assert!(!face.is_bold());
    face.font_weight = 500; // Medium
    assert!(!face.is_bold());
    face.font_weight = 600; // Semi-bold
    assert!(!face.is_bold());
    face.font_weight = 700; // Bold
    assert!(face.is_bold());
    face.font_weight = 800; // Extra-bold
    assert!(face.is_bold());
    face.font_weight = 900; // Black
    assert!(face.is_bold());
}

#[test]
fn test_font_metrics() {
    let mut face = Face::new(FaceId::new(20));
    face.font_ascent = 14;
    face.font_descent = 4;
    face.underline_position = 2;
    face.underline_placement = UnderlinePosition::FontMetric {
        offset_from_baseline: 2,
    };
    face.underline_thickness = 1;
    assert_eq!(face.font_ascent, 14);
    assert_eq!(face.font_descent, 4);
    assert_eq!(face.underline_position, 2);
    assert_eq!(
        face.underline_placement,
        UnderlinePosition::FontMetric {
            offset_from_baseline: 2
        }
    );
    assert_eq!(face.underline_thickness, 1);
}

#[test]
fn descent_line_underline_is_placed_at_the_row_bottom() {
    let geometry =
        UnderlinePosition::DescentLine { pixels_above: 0 }.resolve(20.0, 17.0, 33.0, 1.0);

    assert_eq!(geometry.top_y, 36.0);
    assert_eq!(geometry.thickness, 1.0);
}

#[test]
fn descent_line_underline_honors_explicit_pixels_above() {
    let geometry =
        UnderlinePosition::DescentLine { pixels_above: 2 }.resolve(20.0, 17.0, 33.0, 1.0);

    assert_eq!(geometry.top_y, 34.0);
    assert_eq!(geometry.thickness, 1.0);
}

// --- FaceCache tests ---

#[test]
fn test_face_cache_new_empty() {
    let cache = FaceCache::new();
    assert!(cache.get(FaceId::new(0)).is_none());
    assert!(cache.get(FaceId::new(1)).is_none());
    assert!(cache.default_face().is_none());
}

#[test]
fn test_face_cache_insert_and_get() {
    let mut cache = FaceCache::new();
    let mut face = Face::new(FaceId::new(5));
    face.foreground = Color::GREEN;
    cache.insert(face);

    let retrieved = cache.get(FaceId::new(5)).unwrap();
    assert_eq!(retrieved.id, FaceId::new(5));
    assert_eq!(retrieved.foreground, Color::GREEN);
}

#[test]
fn test_face_cache_insert_updates_existing() {
    let mut cache = FaceCache::new();
    let mut face = Face::new(FaceId::new(5));
    face.foreground = Color::GREEN;
    cache.insert(face);

    // Insert again with same ID but different color
    let mut face2 = Face::new(FaceId::new(5));
    face2.foreground = Color::RED;
    cache.insert(face2);

    let retrieved = cache.get(FaceId::new(5)).unwrap();
    assert_eq!(retrieved.foreground, Color::RED);
}

#[test]
fn test_face_cache_get_or_create() {
    let mut cache = FaceCache::new();
    // Should not exist yet
    assert!(cache.get(FaceId::new(42)).is_none());
    // get_or_create should create it
    let face = cache.get_or_create(FaceId::new(42));
    assert_eq!(face.id, FaceId::new(42));
    // Now it should exist
    assert!(cache.get(FaceId::new(42)).is_some());
}

#[test]
fn test_face_cache_get_or_create_returns_existing() {
    let mut cache = FaceCache::new();
    let mut face = Face::new(FaceId::new(7));
    face.font_size = 24.0;
    cache.insert(face);

    // get_or_create should return the existing face, not overwrite
    let retrieved = cache.get_or_create(FaceId::new(7));
    assert_eq!(retrieved.font_size, 24.0);
}

#[test]
fn test_face_cache_default_face() {
    let mut cache = FaceCache::new();
    assert!(cache.default_face().is_none());

    let default = Face::new(FaceId::new(0));
    cache.insert(default);
    assert!(cache.default_face().is_some());
    assert_eq!(cache.default_face().unwrap().id, FaceId::new(0));
}

#[test]
fn test_face_cache_multiple_faces() {
    let mut cache = FaceCache::new();
    for i in 0..10 {
        let mut face = Face::new(FaceId::new(i));
        face.font_size = 10.0 + i as f32;
        cache.insert(face);
    }
    for i in 0..10 {
        let face = cache.get(FaceId::new(i)).unwrap();
        assert_eq!(face.id, FaceId::new(i));
        assert_eq!(face.font_size, 10.0 + i as f32);
    }
    assert!(cache.get(FaceId::new(10)).is_none());
}

// --- Enum default tests ---

#[test]
fn test_underline_style_default() {
    let style: UnderlineStyle = Default::default();
    assert_eq!(style, UnderlineStyle::None);
}

#[test]
fn test_box_type_default() {
    let bt: BoxType = Default::default();
    assert_eq!(bt, BoxType::None);
}

#[test]
fn test_face_attributes_bitflags_all() {
    let all = FaceAttributes::BOLD
        | FaceAttributes::ITALIC
        | FaceAttributes::UNDERLINE
        | FaceAttributes::OVERLINE
        | FaceAttributes::STRIKE_THROUGH
        | FaceAttributes::INVERSE
        | FaceAttributes::BOX;
    assert!(all.contains(FaceAttributes::BOLD));
    assert!(all.contains(FaceAttributes::ITALIC));
    assert!(all.contains(FaceAttributes::UNDERLINE));
    assert!(all.contains(FaceAttributes::OVERLINE));
    assert!(all.contains(FaceAttributes::STRIKE_THROUGH));
    assert!(all.contains(FaceAttributes::INVERSE));
    assert!(all.contains(FaceAttributes::BOX));
    // All 7 flags set: bits 0-6
    assert_eq!(all.bits(), 0b1111111);
}

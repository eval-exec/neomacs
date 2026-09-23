use super::*;

#[test]
fn display_origin_models_all_display_text_sources() {
    let _ = DisplayOrigin::BufferText {
        charpos: CharPos0::new(0),
    };
    let _ = DisplayOrigin::OverlayString {
        overlay_id: Value::fixnum(1),
        anchor_charpos: CharPos0::new(0),
        kind: OverlayStringKind::Before,
    };
    let _ = DisplayOrigin::DisplayPropertyString {
        anchor_charpos: CharPos0::new(0),
        source: DisplayPropertySource::TextProperty,
    };
    let _ = DisplayOrigin::LinePrefix {
        anchor_charpos: CharPos0::new(0),
    };
    let _ = DisplayOrigin::WrapPrefix {
        anchor_charpos: CharPos0::new(0),
    };
    let _ = DisplayOrigin::ModeLine { selected: true };
    let _ = DisplayOrigin::HeaderLine { selected: true };
    let _ = DisplayOrigin::TabLine;
    let _ = DisplayOrigin::TabBar;
}

#[test]
fn display_origin_derives_chrome_row_roles() {
    assert_eq!(
        DisplayOrigin::ModeLine { selected: true }.glyph_row_role(),
        Some(GlyphRowRole::ModeLine)
    );
    assert_eq!(
        DisplayOrigin::HeaderLine { selected: true }.glyph_row_role(),
        Some(GlyphRowRole::HeaderLine)
    );
    assert_eq!(
        DisplayOrigin::TabLine.glyph_row_role(),
        Some(GlyphRowRole::TabLine)
    );
    assert_eq!(
        DisplayOrigin::TabBar.glyph_row_role(),
        Some(GlyphRowRole::TabBar)
    );
    assert_eq!(
        DisplayOrigin::BufferText {
            charpos: CharPos0::new(0),
        }
        .glyph_row_role(),
        None
    );
}

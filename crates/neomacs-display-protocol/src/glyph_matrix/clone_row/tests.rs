use super::*;
use std::sync::Arc;

fn terminal_plan() -> Arc<TerminalComposition> {
    Arc::new(TerminalComposition {
        cells: vec![TerminalCompositionCell {
            base: 'e',
            extenders: "\u{301}".into(),
            width_cols: 1,
            source_char_len: 2,
        }]
        .into_boxed_slice(),
        width_cols: 1,
    })
}

fn nonowning_types() -> Vec<GlyphType> {
    vec![
        GlyphType::Char { ch: '界' },
        GlyphType::Stretch { width_cols: 3 },
        GlyphType::Image {
            image_id: 17,
            width_cols: 2,
            source_rect: ImageSourceRect::new(0.1, 0.2, 0.5, 0.4).unwrap(),
            margins: GlyphImageMarginsId::from_index(0).unwrap(),
            opaque_background: ImageOpaqueBackground::new(Some(0x12_34_56)),
        },
        GlyphType::Video {
            video_id: VideoId::new(23),
            width_cols: 4,
            opacity: 0.75,
        },
        GlyphType::Xwidget {
            xwidget_id: XwidgetId::new(31),
            webview_id: WebViewId::new(37),
            width_cols: 5,
            content: XwidgetContentExtent::new(15.0, 16.0).unwrap(),
        },
        GlyphType::Surface {
            surface_id: 41,
            width_cols: 6,
        },
        GlyphType::Glyphless {
            ch: '\u{2060}',
            presentation: GlyphlessPresentation::HexCode,
        },
    ]
}

fn glyph(kind: GlyphType, index: usize) -> Glyph {
    Glyph {
        glyph_type: kind,
        face_id: FaceId::new(53),
        provenance: GlyphProvenance::buffer(index + 10),
        bidi_level: 3,
        wide: true,
        pixel_width: 17.5,
        pixel_height: 18.5,
        pixel_ascent: 13.25,
        vertical_offset_px: -2.5,
        padding: true,
        box_vertical_edges: BoxVerticalEdges::Right,
        pointer_appearance: Some(GlyphPointerAppearanceId::from_index(0).unwrap()),
    }
}

#[test]
fn every_nonowning_variant_matches_independent_legacy_glyph_clone() {
    let mut source: Vec<_> = nonowning_types()
        .into_iter()
        .enumerate()
        .map(|(index, kind)| glyph(kind, index))
        .collect();
    let provenances = [
        GlyphProvenance::buffer(10),
        GlyphProvenance::string(GlyphStringSourceId::from_index(0).unwrap(), 2),
        GlyphProvenance::LineEnd,
        GlyphProvenance::Mark,
        GlyphProvenance::LeftTruncation,
        GlyphProvenance::EmptyLineNewline { charpos: 20 },
    ];
    for (glyph, provenance) in source.iter_mut().zip(provenances) {
        glyph.provenance = provenance;
    }
    assert_eq!(source.len(), 7);
    assert!(source.iter().all(glyph_has_only_copy_payloads));
    // Glyph/GlyphType still derive Clone. This oracle never invokes the new
    // GlyphRow clone or the area helper.
    let legacy = source.to_vec();
    let mut actual = clone_area(&source);
    assert_eq!(actual, legacy);
    assert_ne!(actual.as_ptr(), source.as_ptr());
    actual[0].face_id = FaceId::new(99);
    actual.reverse();
    assert_eq!(source, legacy, "new storage must not mutate its source");
    assert!(clone_area(&[]).is_empty());
}

#[test]
fn mixed_owned_payload_at_every_position_uses_ordinary_clone_semantics() {
    for owned_index in 0..3 {
        for automatic in [false, true] {
            let terminal = terminal_plan();
            let mut source: Vec<_> = (0..3)
                .map(|index| glyph(GlyphType::Char { ch: 'x' }, index))
                .collect();
            source[owned_index].glyph_type = if automatic {
                GlyphType::AutomaticComposite {
                    text: "e\u{301}".into(),
                    terminal: terminal.clone(),
                }
            } else {
                GlyphType::Composite {
                    text: "owned".into(),
                }
            };
            assert!(!glyph_has_only_copy_payloads(&source[owned_index]));
            let legacy = source.to_vec();
            let actual = clone_area(&source);
            assert_eq!(actual, legacy);
            assert_ne!(actual.as_ptr(), source.as_ptr());
            match (
                &source[owned_index].glyph_type,
                &actual[owned_index].glyph_type,
            ) {
                (GlyphType::Composite { text: original }, GlyphType::Composite { text: copy }) => {
                    assert_ne!(original.as_ptr(), copy.as_ptr());
                }
                (
                    GlyphType::AutomaticComposite {
                        text: original,
                        terminal: original_plan,
                    },
                    GlyphType::AutomaticComposite {
                        text: copy,
                        terminal: copy_plan,
                    },
                ) => {
                    assert_ne!(original.as_ptr(), copy.as_ptr());
                    assert!(Arc::ptr_eq(original_plan, copy_plan));
                    assert_eq!(Arc::strong_count(&terminal), 4);
                }
                _ => panic!("clone changed active ownership variant"),
            }
        }
    }
}

#[test]
fn composition_boxes_and_arcs_survive_both_drop_orders() {
    for drop_source_first in [false, true] {
        let terminal = terminal_plan();
        let source = vec![
            glyph(
                GlyphType::Composite {
                    text: "owned".into(),
                },
                0,
            ),
            glyph(
                GlyphType::AutomaticComposite {
                    text: "e\u{301}".into(),
                    terminal: terminal.clone(),
                },
                1,
            ),
        ];
        assert_eq!(Arc::strong_count(&terminal), 2);
        let copy = clone_area(&source);
        assert_eq!(Arc::strong_count(&terminal), 3);
        let survivor = if drop_source_first {
            drop(source);
            copy
        } else {
            drop(copy);
            source
        };
        assert_eq!(Arc::strong_count(&terminal), 2);
        assert!(
            matches!(&survivor[0].glyph_type, GlyphType::Composite { text } if text.as_ref() == "owned")
        );
        assert!(
            matches!(&survivor[1].glyph_type, GlyphType::AutomaticComposite { text, terminal } if text.as_ref() == "e\u{301}" && terminal.cells[0].extenders.as_ref() == "\u{301}")
        );
        drop(survivor);
        assert_eq!(Arc::strong_count(&terminal), 1);
        assert_eq!(terminal.width_cols, 1);
    }
}

fn complete_row(mixed: bool) -> GlyphRow {
    let mut row = GlyphRow::new(GlyphRowRole::HeaderLine);
    row.pointer_appearances.push(GlyphPointerAppearance {
        source: GlyphPointerSourceIdentity {
            kind: GlyphPointerSourceKind::Buffer,
            source_id: 71,
            range_start: 10,
            range_end: 20,
            property_owner: 73,
            occurrence: GlyphPointerOccurrenceIdentity::BufferDisplayReplacement {
                buffer_id: 71,
                anchor: 10,
            },
        },
        face_id: FaceId::new(53),
    });
    row.image_margins
        .push(ImageMargins::asymmetric(1.0, 2.0, 3.0, 4.0));
    row.string_sources.push(GlyphStringSource::replacement(
        GlyphStringId::new(79),
        GlyphStringBufferRange::new(10, 20),
    ));
    for (index, kind) in nonowning_types().into_iter().enumerate() {
        row.glyphs[index % GlyphArea::COUNT].push(glyph(kind, index));
    }
    row.glyphs[0][0].padding = false;
    row.glyphs[0][0].provenance =
        GlyphProvenance::string(GlyphStringSourceId::from_index(0).unwrap(), 2);
    if mixed {
        row.glyphs[2].push(glyph(
            GlyphType::Composite {
                text: "owned".into(),
            },
            8,
        ));
        row.glyphs[2].push(glyph(
            GlyphType::AutomaticComposite {
                text: "e\u{301}".into(),
                terminal: terminal_plan(),
            },
            9,
        ));
    }
    row.rebuild_pointer_runs(8.0, 80.0);
    assert!(!row.pointer_runs.is_empty());
    row.hash = 0x1234_5678_9abc_def0;
    row.enabled = false;
    row.cursor_col = Some(7);
    row.cursor_type = Some(CursorStyle::Bar(2.5));
    row.truncated_left = true;
    row.continued = true;
    row.reversed_p = true;
    row.displays_text = true;
    row.ends_at_zv = true;
    row.mode_line = true;
    row.pixel_x = -3.5;
    row.start_col = 2;
    row.pixel_y = 5.5;
    row.height_px = 19.0;
    row.line_spacing_px = 1.25;
    row.ascent_px = 14.5;
    row.start_charpos = 10;
    row.end_charpos = 20;
    row.left_fringe_bitmap = Some(FringeBitmapInfo {
        bitmap_index: 3,
        face_id: FaceId::new(83),
    });
    row.right_fringe_bitmap = Some(FringeBitmapInfo {
        bitmap_index: 5,
        face_id: FaceId::new(89),
    });
    row.overlay_arrow_bitmap = Some(FringeBitmapInfo {
        bitmap_index: 7,
        face_id: FaceId::new(97),
    });
    row
}

#[test]
fn complete_row_metadata_wire_and_storage_match_legacy_content() {
    for mixed in [false, true] {
        let source = complete_row(mixed);
        let wire = serde_json::to_value(&source).unwrap();
        // Independent complete-field oracle: Deserialize does not use Clone;
        // then replace each area with the existing derived Glyph Vec clone.
        let mut expected: GlyphRow = serde_json::from_value(wire.clone()).unwrap();
        expected.glyphs = std::array::from_fn(|area| source.glyphs[area].to_vec());
        let mut actual = source.clone();
        assert_eq!(actual, expected);
        assert_eq!(serde_json::to_value(&actual).unwrap(), wire);
        assert_ne!(actual.appearance_id(), source.appearance_id());
        for area in GlyphArea::ALL {
            assert_ne!(
                actual.glyphs[area.index()].as_ptr(),
                source.glyphs[area.index()].as_ptr()
            );
        }
        assert_ne!(
            actual.pointer_appearances.as_ptr(),
            source.pointer_appearances.as_ptr()
        );
        assert_ne!(actual.image_margins.as_ptr(), source.image_margins.as_ptr());
        assert_ne!(
            actual.string_sources.as_ptr(),
            source.string_sources.as_ptr()
        );
        assert_ne!(actual.pointer_runs.as_ptr(), source.pointer_runs.as_ptr());
        actual.glyphs[0].clear();
        actual.shift_string_source_buffer_positions(0, 5);
        actual.shift_pointer_appearance_buffer_positions(0, 5);
        actual.image_margins.clear();
        actual.pointer_runs.clear();
        assert_eq!(
            serde_json::to_value(&source).unwrap(),
            wire,
            "source is immutable"
        );
    }
}

#[test]
fn copy_preserves_float_bits_and_shared_row_isolation() {
    let mut value = glyph(
        GlyphType::Video {
            video_id: VideoId::new(3),
            width_cols: 2,
            opacity: f32::from_bits(0x7fc0_1234),
        },
        0,
    );
    value.pixel_width = -0.0;
    value.pixel_height = f32::from_bits(0x7fc0_5678);
    let actual = clone_area(&[value]);
    assert_eq!(actual[0].pixel_width.to_bits(), (-0.0f32).to_bits());
    assert_eq!(actual[0].pixel_height.to_bits(), 0x7fc0_5678);
    let GlyphType::Video { opacity, .. } = &actual[0].glyph_type else {
        panic!("video");
    };
    assert_eq!(opacity.to_bits(), 0x7fc0_1234);

    for mixed in [false, true] {
        let mut current = MatrixRow::new(complete_row(mixed));
        let retained = current.clone();
        let appearance = retained.appearance_id();
        MatrixRow::make_mut(&mut current).glyphs[0].clear();
        assert!(!retained.glyphs[0].is_empty());
        assert_ne!(current.appearance_id(), appearance);
        assert_eq!(retained.appearance_id(), appearance);
    }
}

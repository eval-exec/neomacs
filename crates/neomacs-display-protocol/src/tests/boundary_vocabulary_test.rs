//! The boundary vocabulary crosses NDP/1 through its serde derives and the
//! bincode 2.0.1 `standard()` codec (DESIGN §6.9). Each value must come back
//! equal, every byte must be consumed, and a value whose type carries an
//! invariant must be refused on decode rather than rebuilt unchecked.

mod boundary_vocabulary_bincode_roundtrip {
    use crate::input_progress::{
        FrontierSnapshot, InputCancelled, InputReceipt, InputSerial, InputStreamId, SerialSet,
    };
    use crate::menu::{
        MenuAvailability, MenuBarRequestId, MenuCheckState, MenuIndicator, MenuItemKind, MenuToken,
        PopupMenuItem,
    };
    use crate::*;
    use std::num::{NonZeroU16, NonZeroU64};

    fn bytes<T: serde::Serialize>(value: &T) -> Vec<u8> {
        bincode::serde::encode_to_vec(value, bincode::config::standard()).expect("encode")
    }

    fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
        bincode::serde::decode_from_slice(bytes, bincode::config::standard())
            .map_err(|error| error.to_string())
            .and_then(|(value, read)| {
                (read == bytes.len())
                    .then_some(value)
                    .ok_or_else(|| format!("{read} of {} bytes read", bytes.len()))
            })
    }

    fn roundtrip<T>(values: impl IntoIterator<Item = T>)
    where
        T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        for value in values {
            assert_eq!(decode::<T>(&bytes(&value)).as_ref(), Ok(&value));
        }
    }

    fn nz(raw: u64) -> NonZeroU64 {
        NonZeroU64::new(raw).unwrap()
    }

    fn receipt(stream: u64, serial: u64) -> InputReceipt {
        InputReceipt::new(InputStreamId::new(nz(stream)), InputSerial::new(nz(serial)))
    }

    #[test]
    fn frontend_key() {
        roundtrip([
            FrontendKey::Character('\u{ff0d}'),
            FrontendKey::Character('a'),
            FrontendKey::Keysym(0xff0d),
        ]);
    }

    #[test]
    fn modifier_bits() {
        roundtrip([
            ModifierBits::NONE,
            ModifierBits::from_bits(
                ModifierBits::SHIFT.bits() | ModifierBits::HYPER.bits() | ModifierBits::ALT.bits(),
            ),
        ]);
    }

    #[test]
    fn display_time() {
        roundtrip([
            DisplayTime::from_millis(0),
            DisplayTime::from_millis(u64::MAX),
        ]);
    }

    #[test]
    fn input_frontier() {
        roundtrip([receipt(1, 1), receipt(u64::MAX, u64::MAX)]);
        roundtrip([InputCancelled(receipt(2, 7))]);
        roundtrip([
            FrontierSnapshot::Exact {
                stream: InputStreamId::FIRST,
                consumed_through: 9,
                in_progress: [3, 8]
                    .map(|raw| InputSerial::new(nz(raw)))
                    .into_iter()
                    .collect::<SerialSet>(),
            },
            FrontierSnapshot::Indeterminate {
                stream: InputStreamId::new(nz(4)),
                reflected_through: 2,
            },
        ]);
        let zero_stream = bytes(&0u64);
        assert!(decode::<InputStreamId>(&zero_stream).is_err());
        let unordered = bytes(&vec![2u64, 1]);
        assert!(decode::<SerialSet>(&unordered).is_err());
    }

    /// The serde shape of `FrontierSnapshot::Exact`, written without the
    /// invariants the real type checks on decode.
    #[derive(serde::Serialize)]
    enum ExactShape {
        Exact {
            stream: u64,
            consumed_through: u64,
            in_progress: Vec<u64>,
        },
    }

    /// Every in-progress serial was consumed, so it is at most
    /// `consumed_through`; otherwise `completed_prefix` and `Disposition`
    /// disagree about the same serial, and a frontend ledger would settle
    /// input the evaluator never consumed.
    #[test]
    fn frontier_decode_refuses_in_progress_beyond_consumed() {
        let exact = |consumed_through, in_progress: &[u64]| ExactShape::Exact {
            stream: 1,
            consumed_through,
            in_progress: in_progress.to_vec(),
        };
        assert!(decode::<FrontierSnapshot>(&bytes(&exact(3, &[2, 3]))).is_ok());
        assert!(decode::<FrontierSnapshot>(&bytes(&exact(3, &[10]))).is_err());
        assert!(decode::<FrontierSnapshot>(&bytes(&exact(0, &[1]))).is_err());
        assert!(
            serde_json::from_str::<FrontierSnapshot>(
                r#"{"Exact":{"stream":1,"consumed_through":3,"in_progress":[10]}}"#
            )
            .is_err()
        );
    }

    #[test]
    fn pointer_target() {
        let index = PresentedHitIndex::from_parts(
            PresentationId::new(5),
            vec![PresentedHitRegion::new(
                Some(DisplayWindowId::new(1)),
                PresentedRegionKind::TextBody,
                FrameRect::new(0.0, 0.0, 80.0, 16.0).unwrap(),
                0,
            )],
            Vec::new(),
        )
        .unwrap();
        let hit = index
            .resolve(PresentedHitQuery::new(
                InteractionProjection::settled(PresentationId::new(5))
                    .map(
                        GeometryPoint::<RootSurfaceSpace, LogicalPixels>::from_px(4.0, 8.0)
                            .unwrap(),
                    )
                    .unwrap(),
            ))
            .unwrap();
        assert!(hit.is_some());
        roundtrip([
            PointerTarget::Presented {
                presentation: 5,
                hit,
            },
            PointerTarget::Presented {
                presentation: 6,
                hit: None,
            },
            PointerTarget::Unpresented,
        ]);
    }

    #[test]
    fn live_display_frame_id() {
        roundtrip([
            LiveDisplayFrameId::new(nz(1)),
            LiveDisplayFrameId::new(nz(u64::MAX)),
        ]);
        assert!(decode::<LiveDisplayFrameId>(&bytes(&0u64)).is_err());
    }

    #[test]
    fn window_and_display_state() {
        roundtrip([GuiFrameGeometryHints {
            base_width: 1,
            base_height: 2,
            min_width: 3,
            min_height: 4,
            width_inc: 5,
            height_inc: 6,
        }]);
        roundtrip([
            WindowVisibility::Visible,
            WindowVisibility::Iconified,
            WindowVisibility::Invisible,
        ]);
        roundtrip([WindowFullscreenMode::None, WindowFullscreenMode::Maximized]);
        roundtrip([MonitorInfo {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080,
            scale: 1.25,
            width_mm: 600,
            height_mm: 340,
            name: Some("DP-1".into()),
        }]);
        roundtrip([GuiResizeRequestId::FIRST, GuiResizeRequestId::FIRST.next()]);
        roundtrip([
            FrameVisibility::Visible,
            FrameVisibility::Iconified,
            FrameVisibility::Invisible,
        ]);
    }

    #[test]
    fn desktop_fonts_and_resources() {
        roundtrip([SystemFontRole::Monospace, SystemFontRole::Application]);
        roundtrip([SystemFonts::new(
            SystemFontName::new("Monospace 11".into()),
            None,
        )]);
        assert!(decode::<SystemFontName>(&bytes(&"  ".to_owned())).is_err());
        roundtrip([GuiResourceQuery {
            name: "emacs.font".into(),
            class: "Emacs.Font".into(),
            inhibit_native: true,
        }]);
    }

    #[test]
    fn frame_font_size() {
        roundtrip([
            FrameFontSize::Default,
            FrameFontSize::pixels(13).unwrap(),
            FrameFontSize::points(10.5).unwrap(),
            FrameFontSize::relative(1.2).unwrap(),
        ]);
        assert!(decode::<PositiveFontScalar>(&bytes(&0.0f64)).is_err());
        assert!(decode::<PositiveFontScalar>(&bytes(&f64::NAN)).is_err());
    }

    /// The serde shape of `FrameFontSize`, written without its range checks.
    #[derive(serde::Serialize)]
    enum FontSizeShape {
        Default,
        Pixels(u32),
        Points(f64),
        Relative(f64),
    }

    /// A decoded point size meets the same range `FrameFontSize::points`
    /// requires (tenths of a point within `1..=i32::MAX`).
    #[test]
    fn frame_font_size_decode_applies_the_point_range() {
        assert_eq!(
            decode::<FrameFontSize>(&bytes(&FontSizeShape::Default)),
            Ok(FrameFontSize::Default)
        );
        assert_eq!(
            decode::<FrameFontSize>(&bytes(&FontSizeShape::Pixels(13))),
            Ok(FrameFontSize::pixels(13).unwrap())
        );
        assert_eq!(
            decode::<FrameFontSize>(&bytes(&FontSizeShape::Points(10.5))),
            Ok(FrameFontSize::points(10.5).unwrap())
        );
        for out_of_range in [0.05, 1e300] {
            assert!(
                decode::<FrameFontSize>(&bytes(&FontSizeShape::Points(out_of_range))).is_err(),
                "{out_of_range} points"
            );
        }
        assert_eq!(
            decode::<FrameFontSize>(&bytes(&FontSizeShape::Relative(1e300))),
            Ok(FrameFontSize::relative(1e300).unwrap()),
            "relative sizes have no point range"
        );
    }

    #[test]
    fn surfaces() {
        roundtrip([SurfaceShaderLanguage::Wgsl, SurfaceShaderLanguage::Glsl]);
        roundtrip([SurfaceUniformInit {
            name: "speed".into(),
            value: [1.0, 0.5, -0.0, f32::MIN_POSITIVE],
            components: 4,
        }]);
        roundtrip([
            SurfaceChannelSource::Surface(3),
            SurfaceChannelSource::Image(ImageId::new(4)),
            SurfaceChannelSource::Video(VideoId::new(5)),
        ]);
    }

    #[test]
    fn images() {
        let data = crate::image::EncodedBytes::new(vec![0x89, b'P', b'N', b'G']);
        roundtrip([
            ImageDataSource::Isolated(data.clone()),
            ImageDataSource::WithBaseUri {
                data,
                base_uri: ImageBaseUri::new(ImageSourceSpelling::new(
                    b"/images/\xff".to_vec(),
                    ImageSpellingMode::Unibyte,
                )),
            },
        ]);
        roundtrip([ResolvedImageMetadata::layout_is_image_pixels(
            32,
            16,
            0x00ff_8000,
            true,
            ImageMaskKind::default(),
        )]);
    }

    #[test]
    fn neo_term() {
        roundtrip([
            TerminalId::new(1).unwrap(),
            TerminalId::new(u32::MAX).unwrap(),
        ]);
        assert!(decode::<TerminalId>(&bytes(&0u32)).is_err());
        roundtrip([
            TerminalDisplayTarget::Window { buffer: 9 },
            TerminalDisplayTarget::Inline,
            TerminalDisplayTarget::Floating,
        ]);
        roundtrip([TerminalGridSize {
            cols: NonZeroU16::new(80).unwrap(),
            rows: NonZeroU16::new(24).unwrap(),
        }]);
        roundtrip([TerminalFloatPlacement::new(1.5, -2.0, 0.75).unwrap()]);
        let opaque_beyond_one = TerminalFloatPlacement::new(0.0, 0.0, 1.0).unwrap();
        let mut encoded = bytes(&opaque_beyond_one);
        let last = encoded.len() - 4;
        encoded[last..].copy_from_slice(&2.0f32.to_le_bytes());
        assert!(decode::<TerminalFloatPlacement>(&encoded).is_err());
    }

    #[test]
    fn menus() {
        roundtrip([MenuBarRequestId(7)]);
        roundtrip([MenuToken {
            session: 1,
            revision: 2,
        }]);
        roundtrip([PopupMenuItem {
            kind: MenuItemKind::Command {
                availability: MenuAvailability::Enabled,
                indicator: MenuIndicator::Radio(MenuCheckState::On),
            },
            help: Some("help".into()),
            label: "Open".into(),
            shortcut: "C-x C-f".into(),
            depth: 1,
        }]);
    }

    #[test]
    fn layout_scale() {
        roundtrip([
            geometry::DeviceScale::ONE,
            geometry::DeviceScale::new(1.5).unwrap(),
        ]);
        assert!(decode::<geometry::DeviceScale>(&bytes(&0.0f32)).is_err());
    }
}

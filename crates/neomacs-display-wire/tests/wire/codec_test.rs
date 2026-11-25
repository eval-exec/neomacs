//! Codec, golden fixtures and the schema id (DESIGN §6.6, §6.9).

use neomacs_display_wire::codec::{WireCodec, WireError};
use neomacs_display_wire::msg::{Downlink, Uplink};
use neomacs_display_wire::payload::Payload;
use neomacs_display_wire::present::PresentOp;
use neomacs_display_wire::rows::RowRuns;
use neomacs_display_wire::schema::{WIRE_SCHEMA_ID, schema_digest, wire_shapes};
use neomacs_display_wire::shape::{ShapeError, shape_of};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;

use crate::corpus::{
    DownTag, UpTag, down_sample, down_tag, family, family_text, golden_corpus, up_sample, up_tag,
};
use crate::largest_allocation;

/// Families with a `golden_bytes_*` test below.
const GOLDEN_FAMILIES: [&str; 16] = [
    "down.Window",
    "down.Present",
    "down.Asset",
    "down.Ui",
    "down.Config",
    "down.Request",
    "down.Hint",
    "down.Media",
    "down.",
    "up.Input",
    "up.Presentation",
    "up.Fact",
    "up.Asset",
    "up.Reply",
    "up.Media",
    "up.",
];

macro_rules! golden {
    ($test:ident, $family:literal, $expect:expr) => {
        #[test]
        fn $test() {
            $expect.assert_eq(&family_text(&golden_corpus(), $family));
        }
    };
}

golden!(
    golden_bytes_down_window,
    "down.Window",
    expect_test::expect![[r#"
    down.WindowCreate 0203000100fb2003fb58020107fb58020000092a736372617463682a10085028081000
    down.WindowDestroy 020301
    down.WindowSetTitle 02030206c3a96d616373
    down.WindowSetGeometryHints 02030300000101070f
    down.WindowResize 020304fb0004fb00030105
    down.WindowSetPosition 02030507fb5802
    down.WindowSetVisibility 02030601
    down.WindowSetFullscreen 02030705
    down.WindowSetDecorations 02030801
    down.WindowIconify 020309
    down.WindowFocus 02030a01
    down.WindowRequestAttention 02030b00
    down.WindowSetPointerShape 02030cfb3001
    down.WindowWarpPointer 02030d07fb5802
    down.WindowShowChild 02030e0100000041000080410000f0420000024204
    down.WindowRemoveChild 02030f
    down.WindowOpacityAccept 02031000
    down.WindowOpacitySetLowerLimit 02031001cdcc4c3e
    down.WindowOpacitySetRedirects 0203100201
    down.WindowOpacityRetire 02031003
    down.WindowImeSpot 02031100000041000080410000f04200000242
"#]]
);
golden!(
    golden_bytes_down_present,
    "down.Present",
    expect_test::expect![[r#"
    down.PresentBegin 03000301010000
    down.PresentDefineRow 0301000c41778def59d33e1040a9449cc85fb77c001100000000000000804100000000000040410000000000000000000000000000000000000c0c48656c6c6f2c20576f726c64010c0000040102060001020400010c00010c00010c000100000041010c00010c00000000010c00000000010c00000000010c00010c0000000000000000000000000000000000000000
    down.PresentTables 03020300006666663f6666663f6666663f0000803f0000000000000000000000000000803f000000000000000000096d6f6e6f737061636500002041fb90010000000000000000803f00000000020200000000000201000101cdcc4c3ecdcc4c3ecdcc4c3e0000803f0000000000000000000000000000803f000000000000000000096d6f6e6f737061636500002041fb90010000000000000000803f00000000020200000000000201000202cdcccc3e9a99193f0000803f0000803f0000000000000000000000000000803f000000000000000000096d6f6e6f737061636500002041fb90010000000000000000803f00000000020200000000000201000000000000000000000000
    down.PresentWindows 030301020102010000000000000000000040430000804200000000000000000000404300004042000104180000000001040100000100180100010001010000160000010200000a00000103000100000000
    down.PresentFacts 030401000000000000f03f0000803f0001000000000000000000000040430000804200180400004043000080420000004100008041000080410000000000000000000000000000803f010000000000000000000000000000000000000000803f000000000000000000000000000000000000803f0000803f000080bf000080bf00010001000101000000000000000000004043000080420000000000000000000000000000803f01000100010102000002000000000000000000000000004100008041000000803f0000803f0000803f0000803f0000000000000000000000000000803f000040410101020000000200000000000000000000000000410000804100004041000000803f0000803f0000803f0000803f0000000000000000000000000000803f01000100000000000100000100000100010000
    down.PresentCommit 030501
    down.PresentStoreReset 0306
"#]]
);
golden!(
    golden_bytes_down_asset,
    "down.Asset",
    expect_test::expect![[r#"
    down.AssetOffer 040000abababababababababababababababababababababababababababababababab000001fc40e20100
    down.AssetForget 04010109
    down.AssetImageRealize 04020401094020
    down.AssetImageDrop 040304
    down.AssetPut 040400ababababababababababababababababababababababababababababababababfc0000010004676c796600
"#]]
);
golden!(
    golden_bytes_down_ui,
    "down.Ui",
    expect_test::expect![[r#"
    down.UiShowPopupMenu 0500010203010700000041000080410000f04200000242010000803f00000040000102010e4375742074686520726567696f6e0343757403432d7700010445646974010000000000000000000000000000803f0000803f0000803f
    down.UiHidePopupMenu 05010102
    down.UiPresentTooltip 050203010b0300000041000080410000f04200000242d4601001001100000000000000804100000000000040410000000000000000000000000000000000000c0c48656c6c6f2c20576f726c64010c0000040102060001020400010c00010c00010c000100000041010c00010c00000000010c00000000010c00000000010c00010c00000000000000000000000000000000000000000100000000803f0000803f0000803f0000803f0000000000000000000000000000803f000000000000000000096d6f6e6f737061636500002041fb90010000000000000000803f0000000002020000000000020100
    down.UiDismissTooltip 050303
    down.UiVisualBell 050403
"#]]
);
golden!(
    golden_bytes_down_config,
    "down.Config",
    expect_test::expect![[r#"
    down.ConfigVisualConfig 06000e0d7b22637572736f72223a7b7d7d
    down.ConfigModifierPolicy 0601010401040104000103010301030001010101010100000000
    down.ConfigFontRenderOptions 0602000000
    down.ConfigScrollIndicators 060301
    down.ConfigTitlebarHeight 06040000e041
    down.ConfigShowFps 060500
    down.ConfigCornerRadius 06060000d040
    down.ConfigExtraSpacing 0607000000400000003f
    down.ConfigChildFrameStyle 0608000080400103000000400000803e
    down.ConfigConfirmQuit 060901
    down.ConfigSelectedRoot 060a0103
    down.ConfigCursorBlink 060b01fbf401
    down.ConfigQuitKeys 060c070101
"#]]
);
golden!(
    golden_bytes_down_request,
    "down.Request",
    expect_test::expect![[r#"
    down.RequestSelectionGet 0b010100020754415247455453
    down.RequestSelectionExists 0b02010101
    down.RequestOpenUri 0b0301022368747470733a2f2f7777772e676e752e6f72672f736f6674776172652f656d6163732f
    down.RequestTerminalText 0b04010306
"#]]
);
golden!(
    golden_bytes_down_hint,
    "down.Hint",
    expect_test::expect![[r#"
    down.HintScrollPreview 0e000302090207000040c2020d0e
    down.HintCoverageRefill 0e01030202
"#]]
);
golden!(
    golden_bytes_down_media,
    "down.Media",
    expect_test::expect![[r#"
    down.MediaSurfaceCreate 0f0005000011666e206d61696e496d6167652829207b7d01067553706565640000803f00000000000000000000000001010004fb00018001011e00
    down.MediaSurfaceSetUniform 0f01050675537065656400000040000000000000000000000000
    down.MediaSurfaceFree 0f0205
    down.MediaFrameShaderSet 0f0301011640667261676d656e7420666e206d61696e2829207b7d0000
    down.MediaFrameShaderSetUniform 0f0401057554696e740000003f0000003f0000003f0000803f
"#]]
);
golden!(
    golden_bytes_down_other,
    "down.",
    expect_test::expect![[r#"
    down.OpenChannel 0001020a6e656f3a6c6170746f70
    down.CloseChannel 010100
    down.SelectionOwn 0701000204000479616e6b
    down.SelectionDisown 0701010005
    down.ConvertReply 0802000b555446385f535452494e47fc70110100
    down.SelectionData 09020006726567696f6e01
    down.InputProgress 0a012a
    down.Cancel 0c01
    down.InputCancelled 0d01010c
"#]]
);
golden!(
    golden_bytes_up_input,
    "up.Input",
    expect_test::expect![[r#"
    up.InputKey 0215fc40e201000003007801010158
    up.InputImeCommit 0215fc40e20100010306e697a5e69cac
    up.InputPointer 0215fc40e201000203010000484100002042020100000000000060c000
    up.InputPresentedPointer 0215fc40e201000303090400010000484100002042
    up.InputToolBarClick 0215fc40e2010004030902
    up.InputMenuBarClick 0215fc40e20100050309010301046564697400000041000080410000f04200000242
    up.InputMenuSelection 0215fc40e201000601020003
    up.InputTooltipDismissed 0215fc40e20100070300
    up.InputFileDrop 0215fc40e20100080301011f66696c653a2f2f6c6170746f702f686f6d652f6d652f6e6f7465732e6f7267
    up.InputCloseRequested 0215fc40e201000903
    up.InputFocus 0215fc40e201000a03000108
    up.InputMoved 0215fc40e201000b031428
    up.InputIconified 0215fc40e201000c03
    up.InputDeiconified 0215fc40e201000d03
    up.InputResizeCompleted 0215fc40e201000e05
    up.InputApplicationQuit 0215fc40e201000f01
    up.InputOverflow 0c1e28
"#]]
);
golden!(
    golden_bytes_up_presentation,
    "up.Presentation",
    expect_test::expect![[r#"
    up.PresentationReceived 05000309
    up.PresentationActivated 050103090100abababababababababababababababababababababababababababababababab
    up.PresentationDiscarded 05020309
    up.PresentationRetired 050309
    up.PresentationRejected 050403090511
    up.PresentationStoreAck 05050c
"#]]
);
golden!(
    golden_bytes_up_fact,
    "up.Fact",
    expect_test::expect![[r#"
    up.FactMonitors 080002010000fb0014fb400b000000000000f83ffbb004fba802010444502d31
    up.FactDesktop 08010100000000da4201000000
    up.FactResources 0802010110456d6163732e6261636b67726f756e6405626c61636b
    up.FactCapabilities 080303010a73656c656374696f6e73010109636c6970626f617264056578616374
    up.FactDisplayReset 080400
"#]]
);
golden!(
    golden_bytes_up_asset,
    "up.Asset",
    expect_test::expect![[r#"
    up.AssetHave 09000100abababababababababababababababababababababababababababababababab
    up.AssetWant 09010100abababababababababababababababababababababababababababababababab00
    up.AssetStored 090200abababababababababababababababababababababababababababababababab
    up.AssetEvicted 090300abababababababababababababababababababababababababababababababab
    up.AssetResidencyLost 090404
"#]]
);
golden!(
    golden_bytes_up_reply,
    "up.Reply",
    expect_test::expect![[r#"
    up.ReplySelectionData 0a0100000b555446385f535452494e470106706173746564
    up.ReplySelectionExists 0a01000100
    up.ReplyUriOpened 0a010002
    up.ReplyTerminalText 0a010003022420
    up.ReplyTimedOut 0a0101
    up.ReplyRefused 0a0102000a6a617661736372697074
    up.ReplyFailed 0a01030c6e6f20636c6970626f617264
"#]]
);
golden!(
    golden_bytes_up_media,
    "up.Media",
    expect_test::expect![[r#"
    up.MediaSurfaceCreateFailed 0b00050c646576696365206c696d6974
    up.MediaFrameShaderAck 0b010100
    up.MediaFrameShaderFailed 0b0201186e6167613a20756e6b6e6f776e206964656e746966696572
"#]]
);
golden!(
    golden_bytes_up_other,
    "up.",
    expect_test::expect![[r#"
    up.ChannelOpened 0001010000fb0014fb400b000000000000f83ffbb004fba802010444502d3100000000c0420001066672616d65730100010a456d6163732e666f6e740c4d6f6e6f73706163652d3132
    up.ChannelRefused 010100
    up.PointerMotion 030301000048410000204200
    up.WindowGeometry 0403fb0005fbd0020000000000000040
    up.SelectionState 06010200040163
    up.Convert 0702010218746578742f706c61696e3b636861727365743d7574662d3804
"#]]
);

#[test]
fn every_variant_has_a_golden_fixture() {
    for tag in DownTag::iter() {
        assert_eq!(down_tag(&down_sample(tag)), tag, "the sample for {tag:?}");
    }
    for tag in UpTag::iter() {
        assert_eq!(up_tag(&up_sample(tag)), tag, "the sample for {tag:?}");
    }
    let corpus = golden_corpus();
    assert_eq!(
        corpus.len(),
        DownTag::iter().count() + UpTag::iter().count()
    );
    for fixture in &corpus {
        assert!(
            GOLDEN_FAMILIES.contains(&family(&fixture.name)),
            "{} has no golden_bytes test",
            fixture.name
        );
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn wire_schema_id_is_the_digest_of_the_corpus_and_the_shapes() {
    let shapes = wire_shapes().unwrap_or_else(|error| panic!("the wire shapes: {error}"));
    let digest = schema_digest(&golden_corpus(), &shapes);
    assert_eq!(
        hex(&digest),
        hex(&WIRE_SCHEMA_ID),
        "the golden corpus or a wire type changed: update schema::WIRE_SCHEMA_ID to this digest"
    );
}

/// The read shape of every wire-reachable type, pinned so that a change to
/// any nested type shows in review as a diff of this file (and changes the
/// schema id).
#[test]
fn golden_wire_shapes() {
    let shapes = wire_shapes().unwrap_or_else(|error| panic!("the wire shapes: {error}"));
    expect_test::expect_file!["wire_shapes.txt"].assert_eq(&shapes);
}

/// The shape of a nested variant that no fixture takes is part of the id:
/// every `CursorStyle` in the corpus is `FilledBox`, so swapping `Bar` and
/// `Hbar` changes no fixture byte (M2 review r1, architecture 1).
#[test]
fn wire_shapes_reach_nested_variants_no_sample_takes() {
    let shapes = wire_shapes().unwrap_or_else(|error| panic!("the wire shapes: {error}"));
    let cursor_style = "enum CursorStyle\n";
    let at = shapes
        .find(cursor_style)
        .expect("CursorStyle is wire-reachable");
    let indent = shapes[..at]
        .rsplit('\n')
        .next()
        .map_or(0, |line| line.len() - line.trim_start().len() + 2);
    let variants: Vec<&str> = shapes[at + cursor_style.len()..]
        .lines()
        .take(4)
        .map(|line| &line[indent.min(line.len())..])
        .collect();
    assert_eq!(
        variants,
        ["FilledBox: unit", "Bar: f32", "Hbar: f32", "Hollow: unit"]
    );
    // A payload's contents are traced, not just its bytes.
    assert!(
        shapes.contains("struct RowRuns\n"),
        "RowRuns inside DefineRow"
    );
    assert!(
        shapes.contains("struct TooltipLayout\n"),
        "TooltipLayout inside a payload"
    );
}

#[test]
fn wire_schema_id_changes_when_a_nested_field_changes() {
    // Test-only types shaped like nested vocabulary; each variant of `Inner`
    // differs from the base in one nested change that leaves every corpus
    // byte as it is.
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename = "Style")]
    enum Style {
        Box,
        Bar(f32),
        Hbar(f32),
    }
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename = "Style")]
    enum StyleSwapped {
        Box,
        Hbar(f32),
        Bar(f32),
    }
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename = "Inner")]
    struct Inner<S> {
        width: u32,
        style: S,
    }
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename = "Inner")]
    struct InnerWithField {
        width: u32,
        style: Style,
        height: u16,
    }
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename = "Inner")]
    struct InnerReordered {
        style: Style,
        width: u32,
    }
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename = "Inner")]
    struct InnerWidened {
        width: u64,
        style: Style,
    }

    // The hazard: a peer with the swapped enum reads every bar as an hbar,
    // with no decode error.
    let codec = WireCodec::default();
    let sent = Inner {
        width: 3,
        style: Style::Bar(2.0),
    };
    let bytes = codec.encode(&sent).unwrap();
    assert_eq!(codec.decode::<Inner<Style>>(&bytes).unwrap(), sent);
    assert_eq!(
        codec.decode::<Inner<StyleSwapped>>(&bytes).unwrap(),
        Inner {
            width: 3,
            style: StyleSwapped::Hbar(2.0),
        }
    );
    for other in [
        InnerWithField {
            width: 1,
            style: Style::Box,
            height: 2,
        },
        InnerWithField {
            width: 1,
            style: Style::Hbar(1.0),
            height: 2,
        },
    ] {
        assert_eq!(
            codec
                .decode::<InnerWithField>(&codec.encode(&other).unwrap())
                .unwrap(),
            other
        );
    }
    let reordered = InnerReordered {
        style: Style::Box,
        width: 1,
    };
    let widened = InnerWidened {
        width: 1,
        style: Style::Box,
    };
    assert_eq!(
        codec
            .decode::<InnerReordered>(&codec.encode(&reordered).unwrap())
            .unwrap(),
        reordered
    );
    assert_eq!(
        codec
            .decode::<InnerWidened>(&codec.encode(&widened).unwrap())
            .unwrap(),
        widened
    );

    // The id: the same corpus, each nested change gives another id.
    let corpus = golden_corpus();
    let id = |shapes: Result<String, ShapeError>| schema_digest(&corpus, &shapes.unwrap());
    let base = id(shape_of::<Vec<Inner<Style>>>());
    assert_eq!(base, id(shape_of::<Vec<Inner<Style>>>()), "a pure function");
    for (change, changed) in [
        (
            "swapping two nested variants",
            id(shape_of::<Vec<Inner<StyleSwapped>>>()),
        ),
        (
            "adding a nested field",
            id(shape_of::<Vec<InnerWithField>>()),
        ),
        (
            "reordering nested fields",
            id(shape_of::<Vec<InnerReordered>>()),
        ),
        (
            "widening a nested integer",
            id(shape_of::<Vec<InnerWidened>>()),
        ),
    ] {
        assert_ne!(base, changed, "{change} changes the id");
    }
}

#[test]
fn shapes_of_repeating_and_refusing_types() {
    // A position that repeats an enclosing type is cut and named.
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct MenuItem {
        label: String,
        submenu: Vec<MenuItem>,
    }
    let item = MenuItem {
        label: "File".into(),
        submenu: vec![MenuItem {
            label: "Open".into(),
            submenu: Vec::new(),
        }],
    };
    let codec = WireCodec::default();
    assert_eq!(
        codec
            .decode::<MenuItem>(&codec.encode(&item).unwrap())
            .unwrap(),
        item
    );
    expect_test::expect![[r#"
        struct MenuItem
          label: str
          submenu: seq of MenuItem (repeats)
    "#]]
    .assert_eq(&shape_of::<MenuItem>().unwrap());

    // A type that refuses the placeholder values is a typed error naming
    // where, never a silently shorter shape.
    #[derive(Debug, Deserialize)]
    #[serde(try_from = "u8")]
    struct Even(u8);
    impl TryFrom<u8> for Even {
        type Error = &'static str;

        fn try_from(value: u8) -> Result<Self, Self::Error> {
            if value.is_multiple_of(2) {
                Ok(Self(value))
            } else {
                Err("an odd number")
            }
        }
    }
    assert_eq!(Even::try_from(4).map(|even| even.0), Ok(4));
    assert_eq!(
        shape_of::<Vec<Even>>(),
        Err(ShapeError::Refused {
            path: "[]".into(),
            message: "an odd number".into(),
        })
    );
}

/// The display service names the session's lanes itself, because neither
/// crate may depend on the other (M2 amendment 3). A reordered or added lane
/// would route presentations or facts to the wrong lane in the binder, so the
/// two are kept one to one here: both mappings are exhaustive, and each lane
/// is the same record byte on both sides.
#[test]
fn wire_lanes_are_the_session_lanes() {
    use neomacs_display_wire::msg::Lane;
    use neomacs_session::lane::Lane as SessionLane;

    fn to_session(lane: Lane) -> SessionLane {
        match lane {
            Lane::Control => SessionLane::Control,
            Lane::Input => SessionLane::Input,
            Lane::Present => SessionLane::Present,
            Lane::Facts => SessionLane::Facts,
            Lane::Hint => SessionLane::Hint,
            Lane::Bulk => SessionLane::Bulk,
        }
    }
    fn to_wire(lane: SessionLane) -> Lane {
        match lane {
            SessionLane::Control => Lane::Control,
            SessionLane::Input => Lane::Input,
            SessionLane::Present => Lane::Present,
            SessionLane::Facts => Lane::Facts,
            SessionLane::Hint => Lane::Hint,
            SessionLane::Bulk => Lane::Bulk,
        }
    }
    for session in SessionLane::ALL {
        let wire = to_wire(session);
        assert_eq!(to_session(wire), session);
        assert_eq!(wire as u8, session as u8, "{wire:?}");
        assert_eq!(SessionLane::from_byte(wire as u8), Some(session));
    }
}

/// Decode, encode, decode, encode: the two encodings agree.
fn normalizes<T: serde::de::DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<(), String> {
    let codec = WireCodec::default();
    let Ok(first) = codec.decode::<T>(bytes) else {
        return Ok(());
    };
    let once = codec.encode(&first).map_err(|error| error.to_string())?;
    let again: T = codec
        .decode(&once)
        .map_err(|error| format!("own encoding does not decode: {error}"))?;
    let twice = codec.encode(&again).map_err(|error| error.to_string())?;
    if once == twice {
        Ok(())
    } else {
        Err(format!("{} != {}", hex(&once), hex(&twice)))
    }
}

#[test]
fn every_golden_sample_roundtrips_exactly() {
    let codec = WireCodec::default();
    for tag in DownTag::iter() {
        let bytes = codec.encode(&down_sample(tag)).unwrap();
        let decoded: Downlink = codec.decode(&bytes).unwrap();
        assert_eq!(codec.encode(&decoded).unwrap(), bytes, "{tag:?}");
        assert_eq!(down_tag(&decoded), tag);
    }
    for tag in UpTag::iter() {
        let bytes = codec.encode(&up_sample(tag)).unwrap();
        let decoded: Uplink = codec.decode(&bytes).unwrap();
        assert_eq!(codec.encode(&decoded).unwrap(), bytes, "{tag:?}");
        assert_eq!(up_tag(&decoded), tag);
    }
}

fn fixture_and_tail() -> impl Strategy<Value = (usize, usize, Vec<u8>)> {
    let count = DownTag::iter().count() + UpTag::iter().count();
    (
        0..count,
        any::<prop::sample::Index>(),
        prop::collection::vec(any::<u8>(), 0..48),
    )
        .prop_map(|(fixture, cut, tail)| (fixture, cut.index(usize::MAX), tail))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Every variant, with random field values: a fixture's prefix chooses
    /// the variant, random bytes fill its fields; whatever decodes survives
    /// encode → decode → encode unchanged.
    #[test]
    fn every_message_variant_roundtrips((fixture, cut, tail) in fixture_and_tail()) {
        let corpus = golden_corpus();
        let fixture = &corpus[fixture];
        let cut = cut % (fixture.bytes.len() + 1);
        let mut bytes = fixture.bytes[..cut].to_vec();
        bytes.extend(tail);
        if fixture.name.starts_with("down.") {
            prop_assert_eq!(normalizes::<Downlink>(&bytes), Ok(()));
        } else {
            prop_assert_eq!(normalizes::<Uplink>(&bytes), Ok(()));
        }
    }

    /// Random, truncated and corrupted input is a typed error or a value,
    /// never a panic, and decoding allocates in proportion to the input.
    #[test]
    fn decode_is_total(
        (fixture, cut, tail) in fixture_and_tail(),
        flips in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 0..4),
    ) {
        let corpus = golden_corpus();
        let fixture = &corpus[fixture];
        let mut bytes = fixture.bytes.clone();
        for (index, value) in flips {
            if !bytes.is_empty() {
                let at = index.index(bytes.len());
                bytes[at] ^= value;
            }
        }
        bytes.truncate(cut % (bytes.len() + 1));
        bytes.extend(tail);
        let codec = WireCodec::default();
        let bound = (1 << 20) + 16 * bytes.len() + 65_536;
        let (_, largest) = largest_allocation(|| {
            let _ = codec.decode::<Downlink>(&bytes);
            let _ = codec.decode::<Uplink>(&bytes);
            let _ = codec.decode::<PresentOp>(&bytes);
            let _ = codec.decode::<RowRuns>(&bytes);
        });
        prop_assert!(largest <= bound, "{} bytes allocated at once for {} input bytes", largest, bytes.len());
    }
}

#[test]
fn declared_lengths_are_claimed_before_allocation() {
    let codec = WireCodec::default();
    // A byte string declaring 2^56 bytes, a string declaring 2^32, and a
    // payload declaring 64 MiB, each with almost nothing behind it.
    let huge_bytes = [0xfd, 0, 0, 0, 0, 0, 0, 0, 1];
    let huge_string = [0xfd, 0, 0, 0, 0, 1, 0, 0, 0];
    let huge_payload = [0xfc, 0, 0, 0, 4, b'x'];
    for input in [&huge_bytes[..], &huge_string[..], &huge_payload[..]] {
        let (results, largest) = largest_allocation(|| {
            (
                codec.decode::<Vec<u8>>(input).is_err(),
                codec.decode::<String>(input).is_err(),
                codec.decode::<Payload<[u8]>>(input).is_err(),
            )
        });
        assert_eq!(results, (true, true, true), "{input:?}");
        // No claimed length is allocated; a sequence may still preallocate
        // serde's cautious 1 MiB before its first element fails.
        assert!(
            largest <= (1 << 20) + 64 * 1024,
            "{largest} bytes for {input:?}"
        );
    }
    assert!(matches!(
        WireCodec::new(16).decode::<Vec<u8>>(&[0; 17]),
        Err(WireError::TooLarge { len: 17, limit: 16 })
    ));
    assert!(matches!(
        codec.decode::<u8>(&[1, 2]),
        Err(WireError::TrailingBytes { trailing: 1 })
    ));
}

#[test]
fn deflate_roundtrips_and_inflation_is_bounded() {
    let codec = WireCodec::new(1 << 20);
    let body: Vec<u8> = b"the quick brown fox ".repeat(500);
    let compressed = codec.deflate(&body).unwrap();
    assert!(compressed.len() < body.len() / 10);
    assert_eq!(codec.inflate(&compressed).unwrap(), body);

    // A declared length above the limit is refused before inflating.
    let small = WireCodec::new(1000);
    assert!(matches!(
        small.inflate(&compressed),
        Err(WireError::InflatedTooLarge { .. })
    ));
    // A lie about the length is caught.
    let mut lying = codec.deflate(&body).unwrap();
    lying[0] = 5;
    assert!(matches!(
        codec.inflate(&lying),
        Err(WireError::InflatedLengthMismatch { .. } | WireError::Inflate(_))
    ));
    assert!(codec.inflate(&[0x05, 0xff, 0xff]).is_err());
}

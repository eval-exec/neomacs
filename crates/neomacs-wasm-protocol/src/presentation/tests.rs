use super::*;
use neomacs_display_protocol::{FrameDisplayState, font::*};
use std::sync::Arc;

fn frame() -> BrowserPresentation {
    let mut frame = FrameDisplayState::new(80, 24, 10.0, 20.0);
    frame.fonts.insert(
        ResolvedFontId(1),
        ResolvedFont {
            id: ResolvedFontId(1),
            identity: ResolvedFontIdentity::from_file("/fonts/test.ttf", 0, None),
            replay: FontReplay::Swash {
                asset: FontOutlineAsset::Memory(
                    FontMemoryAsset::new("test", Arc::new(vec![42; 512 * 1024]), 0).unwrap(),
                ),
            },
            family: "Test".into(),
            full_name: None,
            postscript_name: None,
            weight: 400,
            slant: FontSlantKind::Normal,
            width: 5,
            pixel_size: 16.0,
            ascent_px: 12.0,
            descent_px: 4.0,
            space_advance_px: 8.0,
            glyph_advance: Default::default(),
        },
    );
    BrowserPresentation {
        frame,
        images: Vec::new(),
        retired_images: Vec::new(),
    }
}

#[test]
fn repeated_presentations_reuse_font_bytes_without_changing_font_bindings() {
    let mut sender = PresentationEncoder::default();
    let mut receiver = PresentationDecoder::default();
    let first = receiver.decode(&sender.encode(frame()).unwrap()).unwrap();
    let second_bytes = sender.encode(frame()).unwrap();
    assert!(
        second_bytes.len() < 16 * 1024,
        "font bytes resent: {} bytes",
        second_bytes.len()
    );
    let second = receiver.decode(&second_bytes).unwrap();
    assert_eq!(first.frame.fonts, second.frame.fonts);
    let memory = |font: &ResolvedFont| match &font.replay {
        FontReplay::Swash {
            asset: FontOutlineAsset::Memory(asset),
        } => asset.shared_bytes(),
        _ => panic!("memory font changed replay kind"),
    };
    assert!(Arc::ptr_eq(
        &memory(&first.frame.fonts[&ResolvedFontId(1)]),
        &memory(&second.frame.fonts[&ResolvedFontId(1)])
    ));
}

fn memory(font: &ResolvedFont) -> Arc<Vec<u8>> {
    match &font.replay {
        FontReplay::Swash {
            asset: FontOutlineAsset::Memory(asset),
        } => asset.shared_bytes(),
        _ => panic!("expected memory font"),
    }
}

#[test]
fn sizes_and_collection_faces_share_one_binary_upload() {
    let mut presentation = frame();
    let mut larger = presentation.frame.fonts[&ResolvedFontId(1)].clone();
    larger.id = ResolvedFontId(2);
    larger.pixel_size = 32.0;
    larger.replay = FontReplay::Swash {
        asset: FontOutlineAsset::Memory(FontMemoryAsset::new("test", memory(&larger), 2).unwrap()),
    };
    presentation.frame.fonts.insert(larger.id, larger.clone());
    let encoded = PresentationEncoder::default().encode(presentation).unwrap();
    assert!(
        encoded.len() < 512 * 1024 + 16 * 1024,
        "font duplicated or encoded as integers"
    );
    let decoded = PresentationDecoder::default().decode(&encoded).unwrap();
    assert_eq!(decoded.frame.fonts[&ResolvedFontId(2)], larger);
    assert!(Arc::ptr_eq(
        &memory(&decoded.frame.fonts[&ResolvedFontId(1)]),
        &memory(&decoded.frame.fonts[&ResolvedFontId(2)])
    ));
}

#[test]
fn missing_resources_fail_without_poisoning_decoder() {
    let mut sender = PresentationEncoder::default();
    let initial = sender.encode(frame()).unwrap();
    let reference_only = sender.encode(frame()).unwrap();
    let mut receiver = PresentationDecoder::default();
    assert!(receiver.decode(&reference_only).is_err());
    receiver.decode(&initial).unwrap();
    receiver.decode(&reference_only).unwrap();
}

#[test]
fn retiring_transport_resources_keeps_old_presentations_alive() {
    let mut sender = PresentationEncoder::default();
    let mut receiver = PresentationDecoder::default();
    let old = receiver.decode(&sender.encode(frame()).unwrap()).unwrap();
    let stale = sender.encode(frame()).unwrap();
    let mut empty = frame();
    empty.frame.fonts.clear();
    receiver.decode(&sender.encode(empty).unwrap()).unwrap();
    assert!(receiver.decode(&stale).is_err());
    assert_eq!(
        memory(&old.frame.fonts[&ResolvedFontId(1)]).len(),
        512 * 1024
    );
    let fresh = sender.encode(frame()).unwrap();
    assert!(fresh.len() > 512 * 1024);
    let reloaded = receiver.decode(&fresh).unwrap();
    assert_eq!(old.frame.fonts, reloaded.frame.fonts);
    assert!(!Arc::ptr_eq(
        &memory(&old.frame.fonts[&ResolvedFontId(1)]),
        &memory(&reloaded.frame.fonts[&ResolvedFontId(1)])
    ));
}

#[test]
fn catalog_changes_and_replaced_bytes_do_not_alias_old_resources() {
    let mut sender = PresentationEncoder::default();
    let mut receiver = PresentationDecoder::default();
    let old = receiver.decode(&sender.encode(frame()).unwrap()).unwrap();
    let stale = sender.encode(frame()).unwrap();
    let mut changed = frame();
    changed.frame.font_catalog_generation = FontCatalogGeneration::from_raw(2);
    let new = receiver.decode(&sender.encode(changed).unwrap()).unwrap();
    assert!(!Arc::ptr_eq(
        &memory(&old.frame.fonts[&ResolvedFontId(1)]),
        &memory(&new.frame.fonts[&ResolvedFontId(1)])
    ));
    assert!(receiver.decode(&stale).is_err());
    let mut replaced = frame();
    replaced.frame.font_catalog_generation = FontCatalogGeneration::from_raw(2);
    replaced
        .frame
        .fonts
        .get_mut(&ResolvedFontId(1))
        .unwrap()
        .replay = FontReplay::Swash {
        asset: FontOutlineAsset::Memory(
            FontMemoryAsset::new("test", Arc::new(vec![99; 512 * 1024]), 0).unwrap(),
        ),
    };
    let replaced = receiver.decode(&sender.encode(replaced).unwrap()).unwrap();
    assert_eq!(memory(&replaced.frame.fonts[&ResolvedFontId(1)])[0], 99);
    assert_eq!(memory(&old.frame.fonts[&ResolvedFontId(1)])[0], 42);
}

#[test]
fn discarded_render_frames_still_install_resources_for_later_frames() {
    let mut sender = PresentationEncoder::default();
    let mut receiver = PresentationDecoder::default();
    drop(receiver.decode(&sender.encode(frame()).unwrap()).unwrap());
    let bytes = sender.encode(frame()).unwrap();
    assert!(bytes.len() < 16 * 1024);
    assert_eq!(
        receiver.decode(&bytes).unwrap().frame.fonts,
        frame().frame.fonts
    );
}

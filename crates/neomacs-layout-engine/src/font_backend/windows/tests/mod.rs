use super::*;
use neomacs_display_protocol::font::FontOutlineAsset;

#[test]
fn replay_source_keeps_locator_and_size_capability_consistent() {
    assert_eq!(
        DirectWriteReplaySource::classify(None, FontFaceType::TrueType)
            .expect("URL-less TrueType stream is replayable")
            .size(),
        crate::font_backend::PlatformFontSize::Scalable
    );
    assert!(DirectWriteReplaySource::classify(None, FontFaceType::Bitmap).is_none());

    let file =
        DirectWriteReplaySource::classify(Some("C:/fixture/font.fon".into()), FontFaceType::Bitmap)
            .expect("file-backed bitmap can be classified by the shared materializer");
    assert_eq!(file.size(), crate::font_backend::PlatformFontSize::Unknown);
}

#[test]
fn selected_native_face_replays_from_the_directwrite_stream_without_a_path() {
    let collection = FontCollection::system();
    let (identity, mut candidate) = collection
        .families_iter()
        .find_map(|family| {
            (0..family.get_font_count()).find_map(|index| {
                let identity = native_identity_from_font(&family.font(index).ok()?)?;
                let candidate = font_candidate_from_font(family.font(index).ok()?)?.matched;
                Some((identity, candidate))
            })
        })
        .expect("DirectWrite exposes at least one materializable system font");

    // Force the native-locator path even when this machine's system font has
    // a local URL. The production path reaches the same seam for custom or
    // process fonts whose DirectWrite loader exposes only a stream.
    candidate.identity = identity;
    candidate.locator = PlatformFontCandidateLocator::Native;

    let matched = DirectWriteBackend::default()
        .finalize_match(candidate)
        .expect("selected DirectWrite face can be copied from its loader stream");
    let FontOutlineAsset::Memory(asset) = matched.asset else {
        panic!("native DirectWrite finalization must produce immutable bytes");
    };

    ttf_parser::Face::parse(asset.bytes(), asset.face_index())
        .expect("DirectWrite stream bytes contain the selected collection face");
}

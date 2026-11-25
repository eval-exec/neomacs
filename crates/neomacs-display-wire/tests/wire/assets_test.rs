//! Assets by content (DESIGN §6.11).

use neomacs_display_wire::assets::{
    AssetError, AssetId, AssetKind, AssetOffers, AssetReceiver, AssetUp, ContentId, FontContainer,
    OfferState, WantPriority,
};
use neomacs_display_wire::msg::ChunkEnd;

const FONT: AssetKind = AssetKind::FontFile {
    container: FontContainer::Sfnt,
    faces: 1,
};

#[test]
fn asset_content_is_verified_before_use() {
    let bytes = b"\x00\x01\x00\x00 a font file".to_vec();
    let id = AssetId::Content(ContentId::of(&bytes));
    let mut receiver = AssetReceiver::new(1 << 20);
    receiver.on_offer(id, FONT, bytes.len() as u64).unwrap();
    assert_eq!(
        receiver.on_put(id, 0, &bytes[..5], ChunkEnd::More),
        Ok(None)
    );
    let verified = receiver
        .on_put(id, 5, &bytes[5..], ChunkEnd::Last)
        .unwrap()
        .expect("complete");
    assert_eq!(verified.bytes, bytes);

    // Poisoned bytes never reach a cache.
    let mut receiver = AssetReceiver::new(1 << 20);
    receiver.on_offer(id, FONT, bytes.len() as u64).unwrap();
    let mut poisoned = bytes.clone();
    poisoned[0] ^= 1;
    assert_eq!(
        receiver.on_put(id, 0, &poisoned, ChunkEnd::Last),
        Err(AssetError::DigestMismatch)
    );

    // Out of order, overrun, short and oversized transfers are typed errors.
    let mut receiver = AssetReceiver::new(16);
    assert!(matches!(
        receiver.on_offer(id, FONT, 17),
        Err(AssetError::TooLarge { .. })
    ));
    receiver.on_offer(id, FONT, 4).unwrap();
    assert!(matches!(
        receiver.on_put(id, 1, b"x", ChunkEnd::More),
        Err(AssetError::OutOfOrder { .. })
    ));
    assert!(matches!(
        receiver.on_put(id, 0, b"xxxxx", ChunkEnd::More),
        Err(AssetError::Overrun { .. })
    ));
    assert!(matches!(
        receiver.on_put(id, 0, b"xx", ChunkEnd::Last),
        Err(AssetError::Short { .. })
    ));
}

#[test]
fn only_offered_ids_may_be_wanted() {
    let id = AssetId::Content(ContentId([7; 32]));
    let mut offers = AssetOffers::default();
    assert_eq!(
        offers.on_uplink(&AssetUp::Want {
            ids: vec![id],
            priority: WantPriority::Visible,
        }),
        Err(AssetError::NotOffered(id))
    );
    assert!(offers.offer(id, FONT, 10).is_some());
    assert!(offers.offer(id, FONT, 10).is_none(), "offered once");
    offers
        .on_uplink(&AssetUp::Want {
            ids: vec![id],
            priority: WantPriority::Visible,
        })
        .unwrap();
    assert_eq!(
        offers.state(id),
        Some(OfferState::Wanted(WantPriority::Visible))
    );
    offers.on_uplink(&AssetUp::Stored { id }).unwrap();
    assert_eq!(offers.state(id), Some(OfferState::Stored));
    offers.on_uplink(&AssetUp::Evicted { id }).unwrap();
    assert_eq!(offers.state(id), None, "re-offered on next reference");
}

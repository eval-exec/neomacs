//! The wire schema id (DESIGN §6.6 item 5).
//!
//! `WireSchemaId` is the SHA-256, truncated to 128 bits, of two canonical
//! texts: the golden fixture corpus (one fixture per message variant, in a
//! fixed order), followed by the serde shape of every wire-reachable type
//! (`shape::shape_of` on `Downlink` and `Uplink`, nested vocabulary and
//! payload contents included). The corpus pins the bytes of sampled values,
//! including hand-written encodings. The shapes pin every struct, field, enum
//! and variant in declaration order, including variants that no sample
//! takes. Bincode is positional, so a change that could make two builds read
//! each other's bytes differently changes this id, and the `schema` build
//! policy refuses a peer whose id differs. The test
//! `wire_schema_id_is_the_digest_of_the_corpus_and_the_shapes` recomputes it.

use sha2::{Digest, Sha256};

/// The display service's schema id for NDP/1 revision 1.0.
pub const WIRE_SCHEMA_ID: [u8; 16] = [
    0xb0, 0xd6, 0x6f, 0xcf, 0xae, 0x44, 0xf6, 0x40, 0x19, 0x4b, 0x53, 0xe8, 0xab, 0x13, 0x4e, 0xb7,
];

/// The display service revision this crate speaks.
pub const REVISION: (u16, u16) = (1, 0);

/// One golden fixture: a variant's name and its canonical bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoldenFixture {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// The canonical text of a corpus: `name hex` per line, in corpus order.
#[must_use]
pub fn corpus_text(fixtures: &[GoldenFixture]) -> String {
    let mut text = String::new();
    for fixture in fixtures {
        text.push_str(&fixture.name);
        text.push(' ');
        for byte in &fixture.bytes {
            text.push_str(&format!("{byte:02x}"));
        }
        text.push('\n');
    }
    text
}

/// The schema id of a corpus and the shape text of the types it encodes.
#[must_use]
pub fn schema_digest(fixtures: &[GoldenFixture], shapes: &str) -> [u8; 16] {
    let mut hasher = Sha256::new();
    hasher.update(corpus_text(fixtures).as_bytes());
    hasher.update(b"--- shapes\n");
    hasher.update(shapes.as_bytes());
    let digest = hasher.finalize();
    let mut id = [0u8; 16];
    id.copy_from_slice(&digest[..16]);
    id
}

/// The shape text of every wire-reachable type: the downlink root, then the
/// uplink root.
#[cfg(feature = "testing")]
pub fn wire_shapes() -> Result<String, crate::shape::ShapeError> {
    use crate::msg::{Downlink, Uplink};
    use crate::shape::shape_of;
    Ok(format!(
        "downlink {}uplink {}",
        shape_of::<Downlink>()?,
        shape_of::<Uplink>()?
    ))
}

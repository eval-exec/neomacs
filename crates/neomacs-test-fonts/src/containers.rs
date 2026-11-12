//! Container fixtures derived from integrity-checked standalone SFNT assets.

/// Wrap a standalone SFNT in WOFF1 without compressing its tables.
///
/// Every original table and checksum is preserved, allowing tests to exercise
/// native container decoding without depending on a separate compressor.
///
/// # Panics
///
/// Panics if `sfnt` is not a complete standalone SFNT test fixture.
pub fn uncompressed_woff(sfnt: &[u8]) -> Vec<u8> {
    let count = u16::from_be_bytes(sfnt[4..6].try_into().unwrap());
    let mut out = vec![0; 44 + usize::from(count) * 20];
    out[..4].copy_from_slice(b"wOFF");
    out[4..8].copy_from_slice(&sfnt[..4]);
    out[12..14].copy_from_slice(&count.to_be_bytes());
    out[20..22].copy_from_slice(&1u16.to_be_bytes());
    let mut sfnt_size = 12 + u32::from(count) * 16;
    for index in 0..usize::from(count) {
        let record = &sfnt[12 + index * 16..28 + index * 16];
        let start = u32::from_be_bytes(record[8..12].try_into().unwrap()) as usize;
        let length = u32::from_be_bytes(record[12..16].try_into().unwrap()) as usize;
        let offset = u32::try_from(out.len()).unwrap();
        let entry = &mut out[44 + index * 20..64 + index * 20];
        entry[..4].copy_from_slice(&record[..4]);
        entry[4..8].copy_from_slice(&offset.to_be_bytes());
        entry[8..12].copy_from_slice(&record[12..16]);
        entry[12..16].copy_from_slice(&record[12..16]);
        entry[16..20].copy_from_slice(&record[4..8]);
        out.extend_from_slice(&sfnt[start..start + length]);
        let padded = (length + 3) & !3;
        out.resize(out.len() + padded - length, 0);
        sfnt_size += u32::try_from(padded).unwrap();
    }
    let size = u32::try_from(out.len()).unwrap();
    out[8..12].copy_from_slice(&size.to_be_bytes());
    out[16..20].copy_from_slice(&sfnt_size.to_be_bytes());
    out
}

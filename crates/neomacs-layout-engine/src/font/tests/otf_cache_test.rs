use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new(bytes: &[u8]) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "neomacs-otf-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }
    fn path(&self) -> &str {
        self.0.to_str().unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

// Minimal real SFNTs: mandatory face tables and one script/default langsys
// referencing a feature in each layout table. ttf-parser parses the bytes.
fn layout_table(script: &[u8; 4], feature: &[u8; 4]) -> Vec<u8> {
    let mut out = vec![0, 1, 0, 0, 0, 10, 0, 30, 0, 42, 0, 1];
    out.extend(script);
    out.extend([0, 8, 0, 4, 0, 0, 0, 0, 255, 255, 0, 1, 0, 0, 0, 1]);
    out.extend(feature);
    out.extend([0, 8, 0, 0, 0, 0, 0, 0]);
    assert_eq!(out.len(), 44);
    out
}
fn sfnt(base: usize, feature: &[u8; 4]) -> Vec<u8> {
    let mut head = vec![0; 54];
    head[18..20].copy_from_slice(&1000u16.to_be_bytes());
    let mut hhea = vec![0; 36];
    hhea[4..6].copy_from_slice(&800i16.to_be_bytes());
    hhea[6..8].copy_from_slice(&(-200i16).to_be_bytes());
    hhea[34..36].copy_from_slice(&1u16.to_be_bytes());
    let maxp = vec![0, 0, 0x50, 0, 0, 1];
    let tables = [
        (b"GSUB", layout_table(b"latn", feature)),
        (b"GPOS", layout_table(b"DFLT", b"kern")),
        (b"head", head),
        (b"hhea", hhea),
        (b"maxp", maxp),
    ];
    let mut out = vec![0, 1, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0];
    let mut offset = 12 + 16 * tables.len();
    for (tag, bytes) in &tables {
        out.extend(*tag);
        out.extend([0; 4]);
        out.extend(((base + offset) as u32).to_be_bytes());
        out.extend((bytes.len() as u32).to_be_bytes());
        offset += bytes.len();
    }
    for (_, bytes) in tables {
        out.extend(bytes);
    }
    out
}
fn counted(
    cache: &mut OtfCapabilityCache,
    file: &str,
    face: u32,
    reads: &mut usize,
) -> Option<OtfCapability> {
    cache.get_with(file, face, |path| {
        *reads += 1;
        std::fs::read(path)
    })
}

#[test]
fn unchanged_font_reads_once_and_preserves_both_layout_tables() {
    let font = Fixture::new(&sfnt(0, b"liga"));
    let expected = otf_capability(font.path(), 0).unwrap();
    assert_eq!(expected.gsub[0].tag, "latn");
    assert_eq!(expected.gsub[0].lang_syses[0].features, ["liga"]);
    assert_eq!(expected.gpos[0].lang_syses[0].features, ["kern"]);
    let mut cache = OtfCapabilityCache::default();
    let mut reads = 0;
    for _ in 0..30 {
        assert_eq!(
            counted(&mut cache, font.path(), 0, &mut reads),
            Some(expected.clone())
        );
    }
    assert_eq!(
        reads, 1,
        "same selected font must not be reread per navigation command"
    );
    cache.clear();
    assert_eq!(
        counted(&mut cache, font.path(), 0, &mut reads),
        Some(expected)
    );
    assert_eq!(
        reads, 2,
        "native font-cache invalidation must retire OTF observations"
    );
}

#[test]
fn collection_faces_keep_distinct_capabilities() {
    let first = sfnt(20, b"liga");
    let second_offset = 20 + first.len();
    let second = sfnt(second_offset, b"calt");
    let mut bytes = b"ttcf\0\x01\0\0\0\0\0\x02".to_vec();
    bytes.extend(20u32.to_be_bytes());
    bytes.extend((second_offset as u32).to_be_bytes());
    bytes.extend(first);
    bytes.extend(second);
    let font = Fixture::new(&bytes);
    let mut cache = OtfCapabilityCache::default();
    let mut reads = 0;
    for _ in 0..3 {
        for (face, feature) in [(0, "liga"), (1, "calt")] {
            let caps = counted(&mut cache, font.path(), face, &mut reads).unwrap();
            assert_eq!(caps, otf_capability_from_bytes(&bytes, face).unwrap());
            assert_eq!(caps.gsub[0].lang_syses[0].features, [feature]);
        }
    }
    assert_eq!(reads, 2, "a collection face must have its own observation");
}

#[test]
fn replacement_in_place_changes_and_failures_do_not_pin_stale_metadata() {
    let font = Fixture::new(&sfnt(0, b"liga"));
    let mut cache = OtfCapabilityCache::default();
    let mut reads = 0;
    let first = counted(&mut cache, font.path(), 0, &mut reads).unwrap();
    let original_time = std::fs::metadata(&font.0).unwrap().modified().unwrap();
    let replacement = Fixture::new(&sfnt(0, b"calt"));
    std::fs::File::open(&replacement.0)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(original_time))
        .unwrap();
    std::fs::rename(&replacement.0, &font.0).unwrap();
    let second = counted(&mut cache, font.path(), 0, &mut reads).unwrap();
    assert_ne!(first, second);
    assert_eq!(second.gsub[0].lang_syses[0].features, ["calt"]);
    // Same inode, same length, and restored mtime: ctime still invalidates.
    std::fs::write(&font.0, sfnt(0, b"liga")).unwrap();
    std::fs::File::open(&font.0)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(original_time))
        .unwrap();
    assert_eq!(
        counted(&mut cache, font.path(), 0, &mut reads),
        Some(first.clone())
    );
    std::fs::write(&font.0, b"invalid font").unwrap();
    assert_eq!(counted(&mut cache, font.path(), 0, &mut reads), None);
    let before = reads;
    assert_eq!(counted(&mut cache, font.path(), 0, &mut reads), None);
    assert_eq!(reads, before, "unchanged readable parse failure is cached");
    std::fs::write(&font.0, sfnt(0, b"liga")).unwrap();
    assert_eq!(
        counted(&mut cache, font.path(), 0, &mut reads),
        Some(first.clone())
    );
    std::fs::remove_file(&font.0).unwrap();
    assert_eq!(counted(&mut cache, font.path(), 0, &mut reads), None);
    std::fs::write(&font.0, sfnt(0, b"liga")).unwrap();
    assert_eq!(counted(&mut cache, font.path(), 0, &mut reads), Some(first));
}

#[test]
fn failed_read_is_retryable_and_cache_is_bounded() {
    let bytes = sfnt(0, b"liga");
    let font = Fixture::new(&bytes);
    let mut cache = OtfCapabilityCache::default();
    assert_eq!(
        cache.get_with(font.path(), 0, |_| Err(
            std::io::ErrorKind::PermissionDenied.into()
        )),
        None
    );
    assert!(cache.get(font.path(), 0).is_some());
    cache.clear();
    let old = cache
        .get_with(font.path(), 0, |path| {
            let data = std::fs::read(path)?;
            std::fs::write(path, sfnt(0, b"calt"))?;
            Ok(data)
        })
        .unwrap();
    let current = cache.get(font.path(), 0).unwrap();
    assert_ne!(
        old, current,
        "an asset changed during the read must not be cached"
    );
    let fonts: Vec<_> = (0..65).map(|_| Fixture::new(&bytes)).collect();
    let mut reads = 0;
    for font in &fonts {
        assert!(counted(&mut cache, font.path(), 0, &mut reads).is_some());
    }
    assert!(counted(&mut cache, fonts[0].path(), 0, &mut reads).is_some());
    assert_eq!(
        reads, 66,
        "overflow must retire earlier entries instead of growing indefinitely"
    );
}

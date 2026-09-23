use std::io::{Read, Seek, SeekFrom, Write};

use super::*;

#[test]
fn write_and_load_sections_from_mmap() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.pdump");

    write_image(
        &path,
        &[
            ImageSection {
                kind: DumpSectionKind::Metadata,
                flags: 0,
                bytes: b"metadata",
            },
            ImageSection {
                kind: DumpSectionKind::HeapImage,
                flags: 7,
                bytes: b"heap bytes",
            },
        ],
    )
    .unwrap();

    let image = load_image(&path).unwrap();
    assert_eq!(
        image.section(DumpSectionKind::Metadata),
        Some(&b"metadata"[..])
    );
    assert_eq!(
        image.section(DumpSectionKind::HeapImage),
        Some(&b"heap bytes"[..])
    );

    let mapped = image.mapped_range();
    let section_ptr = image.section(DumpSectionKind::HeapImage).unwrap().as_ptr() as usize;
    assert!(
        mapped.contains(&section_ptr),
        "section bytes must be borrowed from the mmap, not copied"
    );
}

#[test]
fn load_image_does_not_hash_payload_corruption_on_startup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.pdump");
    write_image(
        &path,
        &[ImageSection {
            kind: DumpSectionKind::HeapImage,
            flags: 0,
            bytes: b"heap bytes",
        }],
    )
    .unwrap();

    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    file.seek(SeekFrom::End(-1)).unwrap();
    let mut byte = [0u8; 1];
    file.read_exact(&mut byte).unwrap();
    byte[0] ^= 0x55;
    file.seek(SeekFrom::End(-1)).unwrap();
    file.write_all(&byte).unwrap();
    file.sync_all().unwrap();

    let image = load_image(&path).unwrap();
    assert_ne!(
        image.section(DumpSectionKind::HeapImage),
        Some(&b"heap bytes"[..])
    );
}

#[test]
fn rejects_bad_section_bounds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.pdump");
    write_image(
        &path,
        &[ImageSection {
            kind: DumpSectionKind::HeapImage,
            flags: 0,
            bytes: b"heap bytes",
        }],
    )
    .unwrap();

    let mut bytes = std::fs::read(&path).unwrap();
    let section_start = HEADER_SIZE;
    let offset_start = section_start + 8;
    let bogus_offset = (bytes.len() as u64 + 128).to_le_bytes();
    bytes[offset_start..offset_start + 8].copy_from_slice(&bogus_offset);

    let checksum = checksum_body(&bytes);
    let checksum_start = 16 + 4 + 4 + 4 + 4 + 32;
    bytes[checksum_start..checksum_start + 32].copy_from_slice(&checksum);
    std::fs::write(&path, bytes).unwrap();

    assert!(matches!(
        load_image(&path),
        Err(DumpError::ImageFormatError(_))
    ));
}

#[test]
fn relocations_patch_mapped_pointers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.pdump");
    let mut heap_bytes = vec![0u8; 2 * std::mem::size_of::<usize>()];
    heap_bytes[..std::mem::size_of::<usize>()]
        .copy_from_slice(&std::mem::size_of::<usize>().to_ne_bytes());
    let relocations = relocation_section_bytes(&[ImageRelocation {
        location_offset: 0,
        addend: 0,
    }]);

    write_image(
        &path,
        &[
            ImageSection {
                kind: DumpSectionKind::HeapImage,
                flags: 0,
                bytes: &heap_bytes,
            },
            ImageSection {
                kind: DumpSectionKind::Relocations,
                flags: 0,
                bytes: &relocations,
            },
        ],
    )
    .unwrap();

    let mut image = load_image(&path).unwrap();
    // v13: the on-disk word is BAKED for the planned base — it must not
    // equal the raw heap-relative input any more.
    let before = image.section(DumpSectionKind::HeapImage).unwrap();
    let baked = usize::from_ne_bytes(before[..std::mem::size_of::<usize>()].try_into().unwrap());
    assert!(
        baked as u64 >= PLANNED_MAP_BASE,
        "word should be baked for the planned base, got {baked:#x}"
    );

    // Correct on BOTH paths: a planned-base hit skips the walk and the
    // baked word already equals the live pointer; a fallback delta-applies
    // to the same live pointer.
    image.apply_relocations().unwrap();

    let heap = image.section(DumpSectionKind::HeapImage).unwrap();
    let patched = usize::from_ne_bytes(heap[..std::mem::size_of::<usize>()].try_into().unwrap());
    let expected = unsafe { heap.as_ptr().add(std::mem::size_of::<usize>()) as usize };
    assert_eq!(patched, expected);
    assert!(image.mapped_range().contains(&patched));
}

#[test]
fn relocations_can_patch_tagged_pointer_addends() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.pdump");
    let mut heap_bytes = vec![0u8; 2 * std::mem::size_of::<usize>()];
    heap_bytes[..std::mem::size_of::<usize>()]
        .copy_from_slice(&std::mem::size_of::<usize>().to_ne_bytes());
    let relocations = relocation_section_bytes(&[ImageRelocation {
        location_offset: 0,
        addend: 0b011,
    }]);

    write_image(
        &path,
        &[
            ImageSection {
                kind: DumpSectionKind::HeapImage,
                flags: 0,
                bytes: &heap_bytes,
            },
            ImageSection {
                kind: DumpSectionKind::Relocations,
                flags: 0,
                bytes: &relocations,
            },
        ],
    )
    .unwrap();

    let mut image = load_image(&path).unwrap();
    image.apply_relocations().unwrap();

    let heap = image.section(DumpSectionKind::HeapImage).unwrap();
    let patched = usize::from_ne_bytes(heap[..std::mem::size_of::<usize>()].try_into().unwrap());
    let expected = unsafe { heap.as_ptr().add(std::mem::size_of::<usize>()) as usize } + 0b011;
    assert_eq!(patched, expected);
}

#[test]
fn heap_to_heap_relocations_patch_mapped_pointers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.pdump");
    let mut heap_bytes = vec![0u8; 2 * std::mem::size_of::<usize>()];
    heap_bytes[..std::mem::size_of::<usize>()]
        .copy_from_slice(&std::mem::size_of::<usize>().to_ne_bytes());
    heap_bytes[std::mem::size_of::<usize>()..].copy_from_slice(&0xfeedusize.to_ne_bytes());
    let relocations = relocation_section_bytes(&[ImageRelocation {
        location_offset: 0,
        addend: 0b011,
    }]);

    write_image(
        &path,
        &[
            ImageSection {
                kind: DumpSectionKind::HeapImage,
                flags: 0,
                bytes: &heap_bytes,
            },
            ImageSection {
                kind: DumpSectionKind::Relocations,
                flags: 0,
                bytes: &relocations,
            },
        ],
    )
    .unwrap();

    let mut image = load_image(&path).unwrap();
    image.apply_relocations().unwrap();

    let heap = image.section(DumpSectionKind::HeapImage).unwrap();
    let patched = usize::from_ne_bytes(heap[..std::mem::size_of::<usize>()].try_into().unwrap());
    let expected = unsafe { heap.as_ptr().add(std::mem::size_of::<usize>()) as usize } + 0b011;
    assert_eq!(patched, expected);
}

#[test]
fn rejects_malformed_relocation_section() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.pdump");

    // v13: the bake sweep validates the section shape at WRITE time —
    // a malformed section never reaches disk. (The loader keeps its own
    // multiple-of check for images from other writers; the fallback-path
    // tests exercise it.)
    assert!(matches!(
        write_image(
            &path,
            &[
                ImageSection {
                    kind: DumpSectionKind::HeapImage,
                    flags: 0,
                    bytes: &[0u8; std::mem::size_of::<usize>()],
                },
                ImageSection {
                    kind: DumpSectionKind::Relocations,
                    flags: 0,
                    bytes: &[0u8; RELOCATION_SIZE - 1],
                },
            ],
        ),
        Err(DumpError::ImageFormatError(_))
    ));
}

#[test]
fn malformed_relocation_write_rejected_files_do_not_exist() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.pdump");
    let _ = write_image(
        &path,
        &[
            ImageSection {
                kind: DumpSectionKind::HeapImage,
                flags: 0,
                bytes: &[0u8; std::mem::size_of::<usize>()],
            },
            ImageSection {
                kind: DumpSectionKind::Relocations,
                flags: 0,
                bytes: &[0u8; RELOCATION_SIZE - 1],
            },
        ],
    );
    // A bake-time rejection must not leave a partial image behind: the
    // writer goes through a tempfile + rename, so the target is absent.
    assert!(!path.exists());
}

#[test]
fn rejects_relocation_outside_location_section() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.pdump");
    let relocations = relocation_section_bytes(&[ImageRelocation {
        location_offset: 1,
        addend: 0,
    }]);

    // v13: the out-of-bounds location is caught by the bake sweep at
    // WRITE time — strictly earlier than the old load-time rejection.
    assert!(matches!(
        write_image(
            &path,
            &[
                ImageSection {
                    kind: DumpSectionKind::HeapImage,
                    flags: 0,
                    bytes: &[0u8; std::mem::size_of::<usize>()],
                },
                ImageSection {
                    kind: DumpSectionKind::Relocations,
                    flags: 0,
                    bytes: &relocations,
                },
            ],
        ),
        Err(DumpError::ImageFormatError(_))
    ));
}

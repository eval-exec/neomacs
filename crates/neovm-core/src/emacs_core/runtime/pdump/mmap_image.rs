//! Memory-mapped pdump image primitives.
//!
//! GNU Emacs's pdumper does not read a serialized payload and rebuild the Lisp
//! heap.  It maps a dump image, validates the build fingerprint, then applies
//! relocations to sections that already contain heap-shaped objects.  This
//! module is the Neomacs image-format boundary for that design: a fixed header,
//! section table, fingerprint, checksum, and an mmap-backed owner that exposes
//! section bytes directly from the mapped file.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use bytemuck::{Pod, Zeroable};
use memmap2::{MmapMut, MmapOptions};
use sha2::{Digest, Sha256};

use super::{DumpError, fingerprint_bytes, hex_string};

const MMAP_MAGIC: [u8; 16] = *b"NEOMMAPDUMP\0\0\0\0\0";
// 7: hash tables encode as insertion-ordered (key, value, snapshot) triples
// in the object stream. ANY object-codec change must bump this — a stale
// same-version file passes the header check and then misparses into memory
// unsafety (the advice-stack test cache found this the hard way).
// v12: Symbol-class fixup words are BAKED as tagged Value::symbol bits over
// the dump-local id (previously raw ids rewritten at load). The bump lives at
// the TOP-LEVEL gate so a mismatched image is refused before
// load_symbol_table_section irreversibly interns ~18K names.
// v13: relocation target words are BAKED as absolute pointers for a planned
// map base recorded in the header; the loader attempts MAP_FIXED_NOREPLACE
// at that base and on a hit skips the 305K-entry relocation walk (and never
// reads the section body). The fallback delta-applies. planned_base == 0 is
// the defensive "unbaked" sentinel: words hold heap-relative targets and the
// legacy absolute apply runs.
// v14: BytecodeExtras relayout for lazy stubs — gnu_rel/const_rel are
// object-relative, const_count added, presence via BC_FLAG_HAS_GNU.
// v15: lazy-stub ByteCodeFunction bytes are BAKED into extras-bearing
// bytecode struct spans at dump time (loader writes nothing there), guarded
// by the stub layout witness header field. A v14 image's struct spans are
// zeros — NOT a valid stub — so the version gate must refuse them before
// the loader trusts baked bytes.
const MMAP_FORMAT_VERSION: u32 = 15;

/// The address every production image plans to map at. Above the worst-case
/// mmap_rnd_bits=32 PIE window top (0x6555_5555_4000) and the ASAN shadow
/// ceiling (0x6000_0000_0000), ~7 TiB below the worst-case descending
/// mmap_base region — collisions are EEXIST-safe (MAP_FIXED_NOREPLACE) and
/// land on the delta fallback. 39-bit-VA hosts can never map it and fall
/// back permanently. One constant for all images: production loads exactly
/// one image per process, so per-image bases would only complicate the
/// bootstrap-chain fallback story. Note: baking fixes the Lisp heap address
/// across runs (GNU pdumper keeps ASLR) — a deliberate, documented trade.
pub(crate) const PLANNED_MAP_BASE: u64 = 0x6900_0000_0000;
const SECTION_ALIGN: u64 = 8;
const RELOCATION_TAG_BITS: u64 = 4;
const RELOCATION_TAG_MASK: u64 = (1 << RELOCATION_TAG_BITS) - 1;

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DumpSectionKind {
    Metadata = 1,
    HeapImage = 2,
    Roots = 3,
    Relocations = 4,
    ObjectStarts = 5,
    EmacsRelocations = 6,
    RuntimeState = 7,
    SymbolTable = 8,
    Obarray = 10,
    Autoloads = 11,
    CharsetRegistry = 12,
    CodingSystems = 13,
    FaceTable = 14,
    Buffers = 15,
    RuntimeManagers = 16,
    ObjectExtra = 17,
    ValueRelocations = 18,
}

impl DumpSectionKind {
    fn from_raw(raw: u32) -> Result<Self, DumpError> {
        match raw {
            1 => Ok(Self::Metadata),
            2 => Ok(Self::HeapImage),
            3 => Ok(Self::Roots),
            4 => Ok(Self::Relocations),
            5 => Ok(Self::ObjectStarts),
            6 => Ok(Self::EmacsRelocations),
            7 => Ok(Self::RuntimeState),
            8 => Ok(Self::SymbolTable),
            10 => Ok(Self::Obarray),
            11 => Ok(Self::Autoloads),
            12 => Ok(Self::CharsetRegistry),
            13 => Ok(Self::CodingSystems),
            14 => Ok(Self::FaceTable),
            15 => Ok(Self::Buffers),
            16 => Ok(Self::RuntimeManagers),
            17 => Ok(Self::ObjectExtra),
            18 => Ok(Self::ValueRelocations),
            other => Err(DumpError::ImageFormatError(format!(
                "unknown section kind {other}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ImageSection<'a> {
    pub kind: DumpSectionKind,
    pub flags: u32,
    pub bytes: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ImageRelocation {
    pub location_offset: u64,
    pub addend: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct DumpImageHeader {
    magic: [u8; 16],
    version: u32,
    header_size: u32,
    section_count: u32,
    reserved0: u32,
    fingerprint: [u8; 32],
    checksum: [u8; 32],
    file_len: u64,
    section_table_offset: u64,
    section_table_len: u64,
    payload_offset: u64,
    flags: u64,
    /// v13: absolute address the relocation words were baked for (0 = unbaked).
    /// The LOADER trusts this field, never its own compiled-in constant —
    /// unstamped dev binaries share a placeholder fingerprint, so a constant
    /// changed between builds would otherwise map at the wrong base.
    planned_base: u64,
    /// v15: FNV-1a of the canonical baked-stub template bytes
    /// (`mapped_heap::stub_layout_witness`). Baked `ByteCodeFunction` bytes
    /// make repr(Rust) layout part of the image contract; the fingerprint
    /// cannot police it (unstamped builds share a placeholder), so a
    /// mismatch here REJECTS the image cleanly instead of letting the
    /// publish-site drop interpret foreign-layout bytes.
    stub_layout_witness: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct DumpImageSection {
    kind: u32,
    flags: u32,
    offset: u64,
    len: u64,
    reserved: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct DumpImageRelocation {
    packed: u64,
}

const HEADER_SIZE: usize = std::mem::size_of::<DumpImageHeader>();
const SECTION_SIZE: usize = std::mem::size_of::<DumpImageSection>();
const RELOCATION_SIZE: usize = std::mem::size_of::<DumpImageRelocation>();

/// The image mapping's owner. Production hits map raw at the planned base
/// (memmap2 cannot request an address); everything else keeps MmapMut.
/// MAP_PRIVATE is load-bearing in both arms: the placeholder pass writes
/// live process-local pointers into mapped objects, and MAP_SHARED would
/// persist them into the file.
enum ImageMapping {
    Anywhere(MmapMut),
    /// Raw-mmap'd (Linux only — the planned-base attempt is a Linux
    /// mechanism; every other platform maps anywhere and delta-applies):
    /// `len` is the LOGICAL file length, not page-rounded, so the header
    /// `file_len` check holds unchanged.
    #[cfg(target_os = "linux")]
    Fixed {
        ptr: *mut u8,
        len: usize,
    },
}

impl ImageMapping {
    fn len(&self) -> usize {
        match self {
            Self::Anywhere(mmap) => mmap.len(),
            #[cfg(target_os = "linux")]
            Self::Fixed { len, .. } => *len,
        }
    }

    fn as_ptr(&self) -> *const u8 {
        match self {
            Self::Anywhere(mmap) => mmap.as_ptr(),
            #[cfg(target_os = "linux")]
            Self::Fixed { ptr, .. } => *ptr,
        }
    }

    fn as_mut_ptr(&mut self) -> *mut u8 {
        match self {
            Self::Anywhere(mmap) => mmap.as_mut_ptr(),
            #[cfg(target_os = "linux")]
            Self::Fixed { ptr, .. } => *ptr,
        }
    }

    fn bytes(&self) -> &[u8] {
        // SAFETY: both arms own a live mapping of exactly `len` logical bytes
        // for as long as `self` lives.
        unsafe { std::slice::from_raw_parts(self.as_ptr(), self.len()) }
    }
}

impl Drop for ImageMapping {
    fn drop(&mut self) {
        // Dropping MUST really unmap: a fingerprint-rejected first candidate
        // would otherwise poison the planned base for every later candidate
        // in the same process.
        #[cfg(target_os = "linux")]
        if let Self::Fixed { ptr, len } = self {
            // SAFETY: ptr/len are the live mapping this arm owns; munmap
            // rounds the length up to page granularity itself.
            unsafe {
                libc::munmap((*ptr).cast(), *len);
            }
        }
    }
}

pub(crate) struct LoadedMmapImage {
    mmap: ImageMapping,
    sections: Vec<DumpImageSection>,
    /// True when the mapping landed exactly at the header's planned base —
    /// the relocation words are already final and the walk is skipped.
    fixed_at_planned_base: bool,
    planned_base: u64,
}

impl LoadedMmapImage {
    pub(crate) fn section(&self, kind: DumpSectionKind) -> Option<&[u8]> {
        let section = self
            .sections
            .iter()
            .find(|section| section.kind == kind as u32)?;
        Some(
            &self.mmap.bytes()
                [section.offset as usize..section.offset as usize + section.len as usize],
        )
    }

    pub(crate) fn section_mut_ptr(&self, kind: DumpSectionKind) -> Option<(*mut u8, usize)> {
        let section = self
            .sections
            .iter()
            .find(|section| section.kind == kind as u32)?;
        let start = section.offset as usize;
        let len = section.len as usize;
        Some((unsafe { self.mmap.as_ptr().cast_mut().add(start) }, len))
    }

    pub(crate) fn apply_relocations(&mut self) -> Result<(), DumpError> {
        if self.fixed_at_planned_base {
            // The mapping landed at the base the words were baked for: every
            // relocation target is already final. No per-entry section reads
            // (the 2.33MB body stays untouched page cache), zero word writes.
            tracing::debug!("pdump mapped at planned base; relocation walk skipped");
            return Ok(());
        }
        let Ok(reloc_section) = self.section_bounds(DumpSectionKind::Relocations) else {
            return Ok(());
        };
        if !reloc_section.len().is_multiple_of(RELOCATION_SIZE) {
            return Err(DumpError::ImageFormatError(format!(
                "relocation section length {} is not a multiple of {RELOCATION_SIZE}",
                reloc_section.len()
            )));
        }
        let heap = self.section_bounds(DumpSectionKind::HeapImage)?;
        let heap_len = heap.end - heap.start;
        let word = std::mem::size_of::<usize>();
        if heap_len < word {
            return Err(DumpError::ImageFormatError(
                "heap image too small to hold a relocated word".into(),
            ));
        }
        // Trust model (v13): on the PLANNED-BASE HIT PATH above, 305,768
        // baked words are trusted with zero per-word validation — like GNU's
        // dump_do_dump_reloc — and the body checksum is skipped by design, so
        // a bit-flipped word in a user-writable cache file surfaces as a wild
        // pointer, not a clean ImageFormatError. The bounds checks BELOW are
        // therefore the validation story only for the fallback paths; the
        // audit story for the hit path is NEOVM_PDUMP_NO_FIXED_MAP=1 (full
        // delta-apply validation, exercised continuously by in-process test
        // double-loads and a CI lane).
        //
        // v15 extends the trusted surface to the BAKED STUB BYTES in every
        // extras-bearing bytecode struct span — on ALL paths, hit and
        // fallback alike (no relocation ever targets them, so the delta walk
        // never validates them either). Their guards are the stub layout
        // witness in the header (cross-binary layout) and the byte-wise
        // template comparison in the stub-finalize pass (per-object
        // corruption, release mode included).
        let max_location = (heap_len - word) as u64;
        let planned_base = self.planned_base;
        let base = self.mmap.as_mut_ptr();
        // Safety: section ranges were validated against the mapping length
        // when the section table was read.
        let heap_base = unsafe { base.add(heap.start) };
        let heap_addr = heap_base as usize;
        // Baked images (v13, planned_base != 0): words hold
        // planned + heap_file_offset + target + tag; rebase to the actual
        // mapping. Unbaked images (sentinel 0, hand-built tests): words hold
        // heap-relative targets; the legacy absolute apply runs. Bounds are
        // explicit non-wrapping compares at both edges — a corrupt small word
        // must not usize-underflow past the check, and the baked residue
        // legitimately exceeds heap_len by up to the tag mask.
        let baked_floor = (planned_base as usize)
            .checked_add(heap.start)
            .ok_or_else(|| {
                DumpError::ImageFormatError("planned base + heap offset overflows".into())
            })?;
        for relocation_offset in (reloc_section.start..reloc_section.end).step_by(RELOCATION_SIZE) {
            // Read through the same raw provenance the write below uses.
            let relocation = unsafe {
                base.add(relocation_offset)
                    .cast::<DumpImageRelocation>()
                    .read_unaligned()
            };
            let location_offset = relocation.packed >> RELOCATION_TAG_BITS;
            let addend = (relocation.packed & RELOCATION_TAG_MASK) as usize;
            if location_offset > max_location {
                return Err(DumpError::ImageFormatError(format!(
                    "relocation location {location_offset} exceeds heap image length {heap_len}"
                )));
            }
            let location = unsafe { heap_base.add(location_offset as usize).cast::<usize>() };
            let current = unsafe { location.read_unaligned() };
            let new_word = if planned_base != 0 {
                if current < baked_floor {
                    return Err(DumpError::ImageFormatError(format!(
                        "baked relocation word {current:#x} is below the planned heap base {baked_floor:#x}"
                    )));
                }
                let residue = current - baked_floor;
                if residue > heap_len + RELOCATION_TAG_MASK as usize {
                    return Err(DumpError::ImageFormatError(format!(
                        "baked relocation residue {residue} exceeds heap image length {heap_len}"
                    )));
                }
                // residue = target + tag; the delta between page-aligned
                // bases cannot disturb tag bits 0-3.
                heap_addr + residue
            } else {
                if current > heap_len {
                    return Err(DumpError::ImageFormatError(format!(
                        "relocation target {current} exceeds heap image length {heap_len}"
                    )));
                }
                heap_addr + current + addend
            };
            unsafe { location.write_unaligned(new_word) };
        }
        Ok(())
    }

    fn section_bounds(&self, kind: DumpSectionKind) -> Result<std::ops::Range<usize>, DumpError> {
        let section = self
            .sections
            .iter()
            .find(|section| section.kind == kind as u32)
            .ok_or_else(|| DumpError::ImageFormatError(format!("missing {kind:?} section")))?;
        let start = section.offset as usize;
        let end = start + section.len as usize;
        Ok(start..end)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn contains_ptr(&self, ptr: *const u8) -> bool {
        let ptr = ptr as usize;
        self.mapped_range().contains(&ptr)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn mapped_range(&self) -> std::ops::Range<usize> {
        let start = self.mmap.as_ptr() as usize;
        start..start + self.mmap.len()
    }
}

/// Rewrite each relocation target word from its heap-relative target to the
/// absolute pointer valid at [`PLANNED_MAP_BASE`]: planned + heap_file_offset
/// + target + tag-addend. The relocation section itself is unchanged — it
/// remains the fallback's worklist (which then delta-applies) and the audit
/// surface.
fn bake_relocations_at_planned_base(
    bytes: &mut [u8],
    section_headers: &[DumpImageSection],
) -> Result<(), DumpError> {
    let find = |kind: DumpSectionKind| {
        section_headers
            .iter()
            .find(|section| section.kind == kind as u32)
            .map(|section| (section.offset as usize, section.len as usize))
    };
    let Some((reloc_start, reloc_len)) = find(DumpSectionKind::Relocations) else {
        return Ok(());
    };
    let Some((heap_start, heap_len)) = find(DumpSectionKind::HeapImage) else {
        return Ok(());
    };
    if !reloc_len.is_multiple_of(RELOCATION_SIZE) {
        return Err(DumpError::ImageFormatError(
            "relocation section length is not a multiple of the entry size at bake time".into(),
        ));
    }
    let word = std::mem::size_of::<usize>();
    for offset in (reloc_start..reloc_start + reloc_len).step_by(RELOCATION_SIZE) {
        let packed = u64::from_ne_bytes(bytes[offset..offset + 8].try_into().expect("entry"));
        let location_offset = (packed >> RELOCATION_TAG_BITS) as usize;
        let addend = (packed & RELOCATION_TAG_MASK) as usize;
        if location_offset > heap_len - word {
            return Err(DumpError::ImageFormatError(format!(
                "relocation location {location_offset} exceeds heap image length {heap_len} at bake time"
            )));
        }
        let loc = heap_start + location_offset;
        let target = usize::from_ne_bytes(bytes[loc..loc + word].try_into().expect("word"));
        if target > heap_len {
            return Err(DumpError::ImageFormatError(format!(
                "relocation target {target} exceeds heap image length {heap_len} at bake time"
            )));
        }
        let baked = PLANNED_MAP_BASE as usize + heap_start + target + addend;
        bytes[loc..loc + word].copy_from_slice(&baked.to_ne_bytes());
    }
    Ok(())
}

pub(crate) fn write_image(path: &Path, sections: &[ImageSection<'_>]) -> Result<(), DumpError> {
    if sections.is_empty() {
        return Err(DumpError::ImageFormatError(
            "mmap pdump image must contain at least one section".to_string(),
        ));
    }

    let section_table_offset = HEADER_SIZE as u64;
    let section_table_len = (sections.len() * SECTION_SIZE) as u64;
    let payload_offset = align_up(section_table_offset + section_table_len, SECTION_ALIGN);

    let mut section_headers = Vec::with_capacity(sections.len());
    let mut cursor = payload_offset;
    for section in sections {
        cursor = align_up(cursor, SECTION_ALIGN);
        section_headers.push(DumpImageSection {
            kind: section.kind as u32,
            flags: section.flags,
            offset: cursor,
            len: section.bytes.len() as u64,
            reserved: 0,
        });
        cursor = cursor
            .checked_add(section.bytes.len() as u64)
            .ok_or_else(|| DumpError::ImageFormatError("pdump image length overflow".into()))?;
    }

    let file_len = cursor as usize;
    let mut bytes = vec![0u8; file_len];

    for (idx, section_header) in section_headers.iter().enumerate() {
        let start = section_table_offset as usize + idx * SECTION_SIZE;
        bytes[start..start + SECTION_SIZE].copy_from_slice(bytemuck::bytes_of(section_header));
    }
    for (section, section_header) in sections.iter().zip(section_headers.iter()) {
        let start = section_header.offset as usize;
        let end = start + section_header.len as usize;
        bytes[start..end].copy_from_slice(section.bytes);
    }

    // Post-layout bake: rewrite every relocation target word as the absolute
    // pointer it will hold when the file maps at PLANNED_MAP_BASE. This must
    // run here — section file offsets are only assigned above — and before
    // checksum_body so the checksum covers the baked bytes.
    bake_relocations_at_planned_base(&mut bytes, &section_headers)?;

    let checksum = checksum_body(&bytes);
    let header = DumpImageHeader {
        magic: MMAP_MAGIC,
        version: MMAP_FORMAT_VERSION,
        header_size: HEADER_SIZE as u32,
        section_count: sections.len() as u32,
        reserved0: 0,
        fingerprint: fingerprint_bytes(),
        checksum,
        file_len: file_len as u64,
        section_table_offset,
        section_table_len,
        payload_offset,
        flags: 0,
        planned_base: PLANNED_MAP_BASE,
        stub_layout_witness: super::mapped_heap::stub_layout_witness(),
    };
    bytes[..HEADER_SIZE].copy_from_slice(bytemuck::bytes_of(&header));

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&bytes)?;
    file.flush()?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|err| DumpError::Io(err.error))?;
    Ok(())
}

pub(crate) fn relocation_section_bytes(relocations: &[ImageRelocation]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(relocations.len() * RELOCATION_SIZE);
    for relocation in relocations {
        assert!(u64::from(relocation.addend) <= RELOCATION_TAG_MASK);
        assert!(relocation.location_offset <= (u64::MAX >> RELOCATION_TAG_BITS));
        let raw = DumpImageRelocation {
            packed: (relocation.location_offset << RELOCATION_TAG_BITS)
                | u64::from(relocation.addend),
        };
        bytes.extend_from_slice(bytemuck::bytes_of(&raw));
    }
    bytes
}

/// Test support: flood one section's on-disk payload with 0xFF, leaving the
/// header and section table intact. Corruption tests can no longer
/// round-trip loaded sections through `write_image` — its bake sweep rejects
/// The on-disk header size, so tests can address trailing header fields
/// (the stub layout witness is the last u64) without duplicating layout.
#[cfg(test)]
pub(crate) fn header_size_for_test() -> usize {
    HEADER_SIZE
}

/// invalid relocation shapes at write time (by design), and re-writing an
/// already-baked heap would double-bake — so they corrupt the file directly.
#[cfg(test)]
pub(crate) fn corrupt_section_on_disk_for_test(
    path: &Path,
    kind: DumpSectionKind,
) -> Result<(), DumpError> {
    let mut bytes = std::fs::read(path)?;
    if bytes.len() < HEADER_SIZE {
        return Err(DumpError::BadMagic);
    }
    let header = *bytemuck::from_bytes::<DumpImageHeader>(&bytes[..HEADER_SIZE]);
    let table_start = header.section_table_offset as usize;
    for idx in 0..header.section_count as usize {
        let start = table_start + idx * SECTION_SIZE;
        let section =
            *bytemuck::from_bytes::<DumpImageSection>(&bytes[start..start + SECTION_SIZE]);
        if section.kind == kind as u32 {
            let payload_start = section.offset as usize;
            let payload_end = payload_start + section.len as usize;
            bytes[payload_start..payload_end].fill(0xFF);
            std::fs::write(path, &bytes)?;
            return Ok(());
        }
    }
    Err(DumpError::ImageFormatError(format!(
        "section {kind:?} not present to corrupt"
    )))
}

pub(crate) fn load_image(path: &Path) -> Result<LoadedMmapImage, DumpError> {
    let file = File::open(path)?;
    let (mapping, fixed_at_planned_base) = map_image_file(&file)?;
    validate_image(mapping, fixed_at_planned_base)
}

/// Map the image, attempting the planned base first on Linux. The hit
/// condition is `returned address == planned base`, never mere success:
/// kernels < 4.17, qemu-user, and some seccomp profiles silently degrade
/// MAP_FIXED_NOREPLACE to a hint, and skipping relocations on a
/// wrong-address mapping is UB at the first heap dereference. A mapping
/// returned at the wrong address is still a valid anywhere-map and is KEPT
/// for the delta fallback (no second mmap).
fn map_image_file(file: &File) -> Result<(ImageMapping, bool), DumpError> {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("NEOVM_PDUMP_NO_FIXED_MAP").is_none()
            && let Some(planned_base) = peek_planned_base(file)?
        {
            use std::os::fd::AsRawFd;
            let len = file.metadata()?.len() as usize;
            // SAFETY: fd is open; the address is a plain hint made exclusive
            // by MAP_FIXED_NOREPLACE (never MAP_FIXED — no clobbering).
            let ret = unsafe {
                libc::mmap(
                    planned_base as *mut libc::c_void,
                    len,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_PRIVATE | libc::MAP_FIXED_NOREPLACE,
                    file.as_raw_fd(),
                    0,
                )
            };
            if ret != libc::MAP_FAILED {
                let hit = ret as u64 == planned_base;
                if !hit {
                    // Hint degradation: keep the mapping, take the fallback.
                    tracing::info!(
                        planned_base,
                        actual = ret as u64,
                        "pdump fixed map degraded to a hint; delta-applying relocations"
                    );
                }
                return Ok((
                    ImageMapping::Fixed {
                        ptr: ret.cast(),
                        len,
                    },
                    hit,
                ));
            }
            let errno = std::io::Error::last_os_error();
            tracing::info!(
                planned_base,
                %errno,
                "pdump fixed map unavailable (occupied base or unsupported);                  mapping anywhere and delta-applying relocations"
            );
        }
    }
    let mmap = unsafe { MmapOptions::new().map_copy(file)? };
    Ok((ImageMapping::Anywhere(mmap), false))
}

/// Read version + planned base from the file header without mapping.
/// Returns None when the image is unbaked (planned_base 0), pre-v13, or too
/// short — full validation happens later on whatever mapping results.
#[cfg(target_os = "linux")]
fn peek_planned_base(file: &File) -> Result<Option<u64>, DumpError> {
    use std::os::unix::fs::FileExt;
    let mut head = [0u8; HEADER_SIZE];
    if file.read_at(&mut head, 0)? != HEADER_SIZE {
        return Ok(None);
    }
    let header = *bytemuck::from_bytes::<DumpImageHeader>(&head);
    if header.magic != MMAP_MAGIC || header.version != MMAP_FORMAT_VERSION {
        return Ok(None);
    }
    Ok((header.planned_base != 0).then_some(header.planned_base))
}

fn validate_image(
    mmap: ImageMapping,
    fixed_at_planned_base: bool,
) -> Result<LoadedMmapImage, DumpError> {
    if mmap.len() < HEADER_SIZE {
        return Err(DumpError::BadMagic);
    }

    let header = *bytemuck::from_bytes::<DumpImageHeader>(&mmap.bytes()[..HEADER_SIZE]);
    if header.magic != MMAP_MAGIC {
        return Err(DumpError::BadMagic);
    }
    if header.version != MMAP_FORMAT_VERSION {
        return Err(DumpError::UnsupportedVersion(header.version));
    }
    if header.header_size != HEADER_SIZE as u32 {
        return Err(DumpError::ImageFormatError(format!(
            "header size {} does not match runtime header size {HEADER_SIZE}",
            header.header_size
        )));
    }
    if header.file_len as usize != mmap.len() {
        return Err(DumpError::ImageFormatError(format!(
            "header file length {} does not match mapped length {}",
            header.file_len,
            mmap.len()
        )));
    }

    let expected_fingerprint = fingerprint_bytes();
    if header.fingerprint != expected_fingerprint {
        return Err(DumpError::FingerprintMismatch {
            expected: hex_string(&expected_fingerprint),
            found: hex_string(&header.fingerprint),
        });
    }

    // Baked stub bytes make ByteCodeFunction's repr(Rust) layout part of the
    // image contract, and the fingerprint alone cannot police it (unstamped
    // dev builds share a placeholder). A mismatch here is a clean reject —
    // the bootstrap cache regenerates; beside-binary images fail loudly
    // until fresh-build reruns (the pre-existing stale-image policy).
    let expected_witness = super::mapped_heap::stub_layout_witness();
    if header.stub_layout_witness != expected_witness {
        return Err(DumpError::ImageFormatError(format!(
            "stub layout witness {:#018x} does not match this binary's {:#018x} \
             (image dumped by a binary with a different ByteCodeFunction layout)",
            header.stub_layout_witness, expected_witness
        )));
    }

    // GNU pdumper validates the fixed header and build fingerprint on the
    // startup path; it does not hash the full mapped image before relocation.
    // Keep the checksum in the writer format for offline/debug validation, but
    // do not make normal pdump startup walk the entire file.

    let section_table_start = header.section_table_offset as usize;
    let section_table_len = header.section_table_len as usize;
    let section_table_end = checked_end(section_table_start, section_table_len, mmap.len())?;
    let expected_table_len = header.section_count as usize * SECTION_SIZE;
    if section_table_len != expected_table_len {
        return Err(DumpError::ImageFormatError(format!(
            "section table length {section_table_len} does not match section count {}",
            header.section_count
        )));
    }
    if section_table_start < HEADER_SIZE {
        return Err(DumpError::ImageFormatError(
            "section table overlaps header".to_string(),
        ));
    }
    if header.payload_offset < section_table_end as u64 {
        return Err(DumpError::ImageFormatError(
            "payload starts before section table ends".to_string(),
        ));
    }

    let mut sections = Vec::with_capacity(header.section_count as usize);
    for idx in 0..header.section_count as usize {
        let start = section_table_start + idx * SECTION_SIZE;
        let raw =
            *bytemuck::from_bytes::<DumpImageSection>(&mmap.bytes()[start..start + SECTION_SIZE]);
        DumpSectionKind::from_raw(raw.kind)?;
        if raw.reserved != 0 {
            return Err(DumpError::ImageFormatError(format!(
                "section {idx} reserved field is nonzero"
            )));
        }
        if !raw.offset.is_multiple_of(SECTION_ALIGN) {
            return Err(DumpError::ImageFormatError(format!(
                "section {idx} offset {} is not {SECTION_ALIGN}-byte aligned",
                raw.offset
            )));
        }
        if raw.offset < header.payload_offset {
            return Err(DumpError::ImageFormatError(format!(
                "section {idx} starts before payload offset"
            )));
        }
        checked_end(raw.offset as usize, raw.len as usize, mmap.len())?;
        sections.push(raw);
    }

    let mut ranges: Vec<_> = sections
        .iter()
        .map(|section| {
            (
                section.offset,
                section.offset.saturating_add(section.len),
                section.kind,
            )
        })
        .collect();
    ranges.sort_unstable_by_key(|range| range.0);
    for pair in ranges.windows(2) {
        if pair[0].1 > pair[1].0 {
            return Err(DumpError::ImageFormatError(format!(
                "sections {} and {} overlap",
                pair[0].2, pair[1].2
            )));
        }
    }

    if std::env::var_os("NEOVM_PDUMP_STATS").is_some() {
        // One line per section so measurement sessions stop reverse-
        // engineering the byte budget from hexdumps (the fault-diet campaign
        // did exactly that). stderr keeps --batch stdout clean.
        eprintln!(
            "NEOVM_PDUMP_STATS: file {} bytes, planned_base {:#x}, mapped_at_planned_base {}",
            mmap.len(),
            header.planned_base,
            fixed_at_planned_base,
        );
        for section in &sections {
            eprintln!(
                "NEOVM_PDUMP_STATS: section kind={} offset {} len {}",
                section.kind, section.offset, section.len,
            );
        }
    }
    Ok(LoadedMmapImage {
        mmap,
        sections,
        fixed_at_planned_base,
        planned_base: header.planned_base,
    })
}

fn checked_end(start: usize, len: usize, file_len: usize) -> Result<usize, DumpError> {
    let end = start
        .checked_add(len)
        .ok_or_else(|| DumpError::ImageFormatError("pdump image offset overflow".into()))?;
    if end > file_len {
        return Err(DumpError::ImageFormatError(format!(
            "section range {start}..{end} exceeds image length {file_len}"
        )));
    }
    Ok(end)
}

fn align_up(value: u64, align: u64) -> u64 {
    debug_assert!(align.is_power_of_two());
    (value + align - 1) & !(align - 1)
}

fn checksum_body(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(&bytes[HEADER_SIZE..]);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
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
        let baked =
            usize::from_ne_bytes(before[..std::mem::size_of::<usize>()].try_into().unwrap());
        assert!(
            baked as u64 >= PLANNED_MAP_BASE,
            "word should be baked for the planned base, got {baked:#x}"
        );

        // Correct on BOTH paths: a planned-base hit skips the walk and the
        // baked word already equals the live pointer; a fallback delta-applies
        // to the same live pointer.
        image.apply_relocations().unwrap();

        let heap = image.section(DumpSectionKind::HeapImage).unwrap();
        let patched =
            usize::from_ne_bytes(heap[..std::mem::size_of::<usize>()].try_into().unwrap());
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
        let patched =
            usize::from_ne_bytes(heap[..std::mem::size_of::<usize>()].try_into().unwrap());
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
        let patched =
            usize::from_ne_bytes(heap[..std::mem::size_of::<usize>()].try_into().unwrap());
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
}

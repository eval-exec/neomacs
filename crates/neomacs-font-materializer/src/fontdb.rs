//! Fontdb/Swash source materialization shared by layout and rendering.
//!
//! Ensures the Rust renderer uses the exact same font file that Emacs/Fontconfig
//! resolved, by pre-loading it into cosmic-text's fontdb and returning the
//! fontdb-registered family name for use in `Family::Name(...)`.

use allsorts::binary::read::ReadScope;
use allsorts::font_data::FontData;
use allsorts::tables::{FontTableProvider, SfntVersion};
use cosmic_text::FontSystem;
use flate2::read::GzDecoder;
use neomacs_display_protocol::font::{FontFileAsset, FontMemoryAsset, FontOutlineAsset};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const HEAD_TAG: u32 = u32::from_be_bytes(*b"head");
const TTCF_TAG: u32 = u32::from_be_bytes(*b"ttcf");
const CFF_TAG: u32 = u32::from_be_bytes(*b"CFF ");
const CFF2_TAG: u32 = u32::from_be_bytes(*b"CFF2");
const OTTO_TAG: u32 = u32::from_be_bytes(*b"OTTO");
const TRUE_TYPE_TAG: u32 = 0x0001_0000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FontContainer {
    Sfnt,
    WebFont,
    LegacyBitmap(LegacyBitmapFormat),
}

/// Raster source carried by one SFNT face, used to select the adapter that
/// can replay it exactly.  Container shape alone is insufficient: both
/// fixed monochrome OTB fonts and scalable color emoji fonts are outline-free
/// SFNTs, but Swash owns the latter while FreeType owns the former.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SfntRasterSource {
    Outline,
    ColorBitmap,
    MonochromeBitmap,
    Unknown,
}

struct OpenedFontDbSource {
    ids: Vec<fontdb::ID>,
    selected_id: fontdb::ID,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyBitmapFormat {
    Bdf,
    Pcf,
    CompressedPcf,
    OpenTypeMonochromeBitmap,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum FontDbSourceError {
    #[error("fontdb does not materialize {format:?} containers")]
    Unsupported { format: LegacyBitmapFormat },
    #[error("failed reading font source {path}: {reason}")]
    Read { path: String, reason: String },
    #[error("failed decoding webfont source {path}: {reason}")]
    Decode { path: String, reason: String },
    #[error("decoded font source {path} contains no usable tables")]
    MissingTables { path: String },
    #[error("fontdb rejected font source {path}")]
    Rejected { path: String },
    #[error("font source {path} has no face at index {face_index}")]
    MissingFace { path: String, face_index: u32 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FontDbLoadOutcome {
    Loaded { family: Option<String> },
    Unsupported { format: LegacyBitmapFormat },
    Failed(FontDbSourceError),
}

/// One exact source face registered in a caller's `FontSystem`.
///
/// The synthetic family is the selector cosmic-text consumes; the database
/// id is the generation-local handle used for exact glyph replay. Keeping
/// both in one value prevents layout and rendering from reconstructing the
/// pinning result independently.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PinnedFontFace {
    family: &'static str,
    fontdb_id: fontdb::ID,
}

impl PinnedFontFace {
    pub fn family(self) -> &'static str {
        self.family
    }

    pub fn fontdb_id(self) -> fontdb::ID {
        self.fontdb_id
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum ExactFaceKey {
    File { file_path: String, face_index: u32 },
    Memory(FontMemoryAsset),
}

impl ExactFaceKey {
    fn from_asset(asset: &FontOutlineAsset) -> Self {
        match asset {
            FontOutlineAsset::File(asset) => Self::File {
                file_path: asset.path().to_owned(),
                face_index: asset.face_index(),
            },
            FontOutlineAsset::Memory(asset) => Self::Memory(asset.clone()),
        }
    }
}

impl FontContainer {
    fn detect(path: &Path, bytes: &[u8], face_index: u32) -> Self {
        if bytes.starts_with(b"wOFF") || bytes.starts_with(b"wOF2") {
            return Self::WebFont;
        }
        if bytes.starts_with(b"STARTFONT") {
            return Self::LegacyBitmap(LegacyBitmapFormat::Bdf);
        }
        if bytes.starts_with(&[0x01, b'f', b'c', b'p']) {
            return Self::LegacyBitmap(LegacyBitmapFormat::Pcf);
        }
        if bytes.starts_with(&[0x1f, 0x8b]) {
            let mut magic = [0; 4];
            if GzDecoder::new(bytes).read_exact(&mut magic).is_ok()
                && magic == [0x01, b'f', b'c', b'p']
            {
                return Self::LegacyBitmap(LegacyBitmapFormat::CompressedPcf);
            }
        }

        let sfnt = bytes.starts_with(&[0x00, 0x01, 0x00, 0x00])
            || bytes.starts_with(b"OTTO")
            || bytes.starts_with(b"ttcf")
            || bytes.starts_with(b"true")
            || bytes.starts_with(b"typ1");
        if sfnt {
            return match sfnt_raster_source(bytes, face_index) {
                SfntRasterSource::MonochromeBitmap => {
                    Self::LegacyBitmap(LegacyBitmapFormat::OpenTypeMonochromeBitmap)
                }
                SfntRasterSource::Outline
                | SfntRasterSource::ColorBitmap
                | SfntRasterSource::Unknown => Self::Sfnt,
            };
        }

        match path.extension().and_then(|extension| extension.to_str()) {
            Some(extension)
                if extension.eq_ignore_ascii_case("woff")
                    || extension.eq_ignore_ascii_case("woff2") =>
            {
                Self::WebFont
            }
            // An unrecognized source is intentionally NOT called an
            // unsupported bitmap solely because of its suffix. Let fontdb
            // reject it so malformed and mislabeled inputs remain visible.
            _ => Self::Sfnt,
        }
    }
}

/// Inspect the selected SFNT face rather than trusting its suffix or
/// Fontconfig's `FC_SCALABLE` hint.  GNU's Cairo path scales CBDT/CBLC and
/// `sbix` color strikes at the requested size; Swash provides that same
/// capability.  Outline-free monochrome strikes remain on the exact FreeType
/// replay path because fontdb/Swash cannot materialize them.
fn sfnt_raster_source(bytes: &[u8], face_index: u32) -> SfntRasterSource {
    sfnt_raster_source_inner(bytes, face_index).unwrap_or(SfntRasterSource::Unknown)
}

fn sfnt_raster_source_inner(bytes: &[u8], face_index: u32) -> Option<SfntRasterSource> {
    let directory = if bytes.starts_with(b"ttcf") {
        let count = read_be_u32(bytes, 8)?;
        if face_index >= count {
            return Some(SfntRasterSource::Unknown);
        }
        read_be_u32(bytes, 12 + face_index as usize * 4)? as usize
    } else if face_index == 0 {
        0
    } else {
        return Some(SfntRasterSource::Unknown);
    };
    let count = read_be_u16(bytes, directory + 4)? as usize;
    let mut has_monochrome_bitmap_data = false;
    let mut has_monochrome_bitmap_location = false;
    let mut has_color_bitmap_data = false;
    let mut has_color_bitmap_location = false;
    let mut has_sbix = false;
    let mut has_outline = false;
    for index in 0..count {
        let record = directory + 12 + index * 16;
        let tag: [u8; 4] = bytes.get(record..record + 4)?.try_into().ok()?;
        let length = read_be_u32(bytes, record + 12)?;
        match &tag {
            b"EBDT" | b"bdat" if length != 0 => {
                has_monochrome_bitmap_data = true;
            }
            b"EBLC" | b"bloc" if length != 0 => {
                has_monochrome_bitmap_location = true;
            }
            b"CBDT" if length != 0 => {
                has_color_bitmap_data = true;
            }
            b"CBLC" if length != 0 => {
                has_color_bitmap_location = true;
            }
            b"sbix" if length != 0 => has_sbix = true,
            b"glyf" | b"CFF " | b"CFF2" if length != 0 => has_outline = true,
            _ => {}
        }
    }
    Some(if has_outline {
        SfntRasterSource::Outline
    } else if (has_color_bitmap_data && has_color_bitmap_location) || has_sbix {
        SfntRasterSource::ColorBitmap
    } else if has_monochrome_bitmap_data && has_monochrome_bitmap_location {
        SfntRasterSource::MonochromeBitmap
    } else {
        SfntRasterSource::Unknown
    })
}

fn read_be_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

/// Cache of font file path -> fontdb family name.
/// Avoids re-loading and re-scanning on every frame.
#[derive(Debug, Default)]
pub struct FontFileCache {
    /// Every state is explicit: an absent family is not a failed load, and an
    /// unsupported bitmap container is not a corrupt outline font.
    path_to_outcome: HashMap<String, FontDbLoadOutcome>,
    /// Exact `(path, source face index)` pins for this cache's `FontSystem`.
    /// Errors are cached too so a bad platform answer cannot trigger file I/O
    /// per glyph; [`Self::retry_failed_exact_faces`] makes transient failures
    /// retryable at an explicit font-generation boundary.
    exact_faces: HashMap<ExactFaceKey, Result<PinnedFontFace, FontDbSourceError>>,
    next_synthetic_family: u64,
}

impl FontFileCache {
    pub fn new() -> Self {
        Self {
            path_to_outcome: HashMap::new(),
            exact_faces: HashMap::new(),
            next_synthetic_family: 0,
        }
    }

    /// Build a standalone SFNT from native platform table bytes.
    ///
    /// CoreText and future native adapters use this one serializer instead of
    /// handing platform handles to layout/rendering or duplicating OpenType
    /// checksum rules. The output contains one face at collection index zero.
    pub fn standalone_sfnt_from_tables(mut tables: Vec<(u32, Vec<u8>)>) -> Option<Vec<u8>> {
        tables.retain(|(_, data)| !data.is_empty());
        tables.sort_unstable_by_key(|(tag, _)| *tag);
        tables.dedup_by_key(|(tag, _)| *tag);
        if tables.is_empty() {
            return None;
        }
        for (tag, data) in &mut tables {
            if *tag == HEAD_TAG && data.len() >= 12 {
                data[8..12].fill(0);
            }
        }
        let sfnt_version = if tables
            .iter()
            .any(|(tag, _)| *tag == CFF_TAG || *tag == CFF2_TAG)
        {
            OTTO_TAG
        } else {
            TRUE_TYPE_TAG
        };
        Some(Self::serialize_sfnt(sfnt_version, tables))
    }

    /// Materialize and cache one exact file face under an opaque synthetic
    /// family. Both layout and rendering call this operation, so decoding,
    /// collection-index normalization, naming, and failure policy cannot
    /// drift across the display boundary.
    pub fn pin_exact_face(
        &mut self,
        font_system: &mut FontSystem,
        file_path: &str,
        face_index: u32,
    ) -> Result<PinnedFontFace, FontDbSourceError> {
        let asset =
            FontFileAsset::new(file_path, face_index).ok_or_else(|| FontDbSourceError::Read {
                path: file_path.to_owned(),
                reason: "empty font path".to_owned(),
            })?;
        self.pin_exact_asset(font_system, &FontOutlineAsset::File(asset))
    }

    /// Materialize one exact outline asset under an opaque synthetic family.
    /// File and native-memory sources share this operation, so layout and the
    /// renderer cannot drift in face-index or naming policy.
    pub fn pin_exact_asset(
        &mut self,
        font_system: &mut FontSystem,
        asset: &FontOutlineAsset,
    ) -> Result<PinnedFontFace, FontDbSourceError> {
        let key = ExactFaceKey::from_asset(asset);
        if let Some(cached) = self.exact_faces.get(&key) {
            return cached.clone();
        }

        let synthetic_family = format!("neomacs-pin-{}", self.next_synthetic_family);
        self.next_synthetic_family = self
            .next_synthetic_family
            .checked_add(1)
            .expect("synthetic font family id overflow");
        let result = Self::pin_asset_as_family(font_system.db_mut(), asset, &synthetic_family).map(
            |fontdb_id| PinnedFontFace {
                // cosmic-text's `Family::Name` borrows selectors for the shaping
                // call, so successful cache entries own one process-lifetime
                // selector. Failed/retried opens do not leak a family string.
                family: Box::leak(synthetic_family.into_boxed_str()),
                fontdb_id,
            },
        );
        self.exact_faces.insert(key, result.clone());
        result
    }

    /// Return a previously materialized exact face without touching the file
    /// system or mutating fontdb.
    pub fn pinned_exact_face(&self, file_path: &str, face_index: u32) -> Option<PinnedFontFace> {
        let asset = FontFileAsset::new(file_path, face_index)?;
        self.pinned_exact_asset(&FontOutlineAsset::File(asset))
    }

    pub fn pinned_exact_asset(&self, asset: &FontOutlineAsset) -> Option<PinnedFontFace> {
        self.exact_faces
            .get(&ExactFaceKey::from_asset(asset))?
            .as_ref()
            .ok()
            .copied()
    }

    /// Drop only failed exact-face observations. Successful pins remain valid
    /// for the lifetime of their `FontSystem`; missing/replaced files can be
    /// retried after the caller advances its font generation.
    pub fn retry_failed_exact_faces(&mut self) {
        self.exact_faces.retain(|_, result| result.is_ok());
    }

    /// Pre-load a font file into the FontSystem's fontdb and return the
    /// family name that fontdb assigned to it. Returns None if the file
    /// couldn't be loaded or has no family metadata.
    ///
    /// Results are cached so subsequent calls with the same path are free.
    pub fn resolve_family<'a>(
        &'a mut self,
        font_system: &mut FontSystem,
        file_path: &str,
    ) -> Option<&'a str> {
        if !self.path_to_outcome.contains_key(file_path) {
            let outcome = Self::load_and_resolve(font_system, file_path);
            self.path_to_outcome.insert(file_path.to_string(), outcome);
        }
        self.path_to_outcome
            .get(file_path)
            .and_then(|outcome| match outcome {
                FontDbLoadOutcome::Loaded { family } => family.as_deref(),
                FontDbLoadOutcome::Unsupported { .. } | FontDbLoadOutcome::Failed(_) => None,
            })
    }

    pub fn prime_file(&mut self, font_system: &mut FontSystem, file_path: &str) -> bool {
        if !self.path_to_outcome.contains_key(file_path) {
            let outcome = Self::load_and_resolve(font_system, file_path);
            self.path_to_outcome.insert(file_path.to_string(), outcome);
        }
        self.path_to_outcome
            .get(file_path)
            .is_some_and(|outcome| matches!(outcome, FontDbLoadOutcome::Loaded { .. }))
    }

    fn load_and_resolve(font_system: &mut FontSystem, file_path: &str) -> FontDbLoadOutcome {
        let db = font_system.db_mut();
        let ids = match Self::open_file(db, file_path, 0) {
            Ok(ids) => ids,
            Err(FontDbSourceError::Unsupported { format }) => {
                tracing::debug!(?format, %file_path, "fontdb adapter does not own bitmap source");
                return FontDbLoadOutcome::Unsupported { format };
            }
            Err(error) => {
                tracing::warn!(?error, %file_path, "failed materializing exact fontdb source");
                return FontDbLoadOutcome::Failed(error);
            }
        };

        // Extract family name from the first loaded face
        let family = ids.first().and_then(|&id| {
            db.face(id)
                .and_then(|face_info| face_info.families.first().map(|(name, _)| name.clone()))
        });

        if family.is_some() {
            tracing::debug!(
                "FontFileCache: loaded {} -> family {:?}",
                file_path,
                family.as_deref().unwrap_or("?")
            );
        }

        FontDbLoadOutcome::Loaded { family }
    }

    /// Open one platform-selected font file through the renderer's supported
    /// container boundary. Every caller that materializes an exact platform
    /// identity must use this function rather than constructing a raw
    /// `fontdb::Source::File`.
    pub fn open_file(
        db: &mut fontdb::Database,
        file_path: &str,
        face_index: u32,
    ) -> Result<Vec<fontdb::ID>, FontDbSourceError> {
        Self::open_exact_face(db, file_path, face_index).map(|opened| opened.ids)
    }

    fn open_exact_face(
        db: &mut fontdb::Database,
        file_path: &str,
        face_index: u32,
    ) -> Result<OpenedFontDbSource, FontDbSourceError> {
        let bytes = std::fs::read(file_path).map_err(|error| FontDbSourceError::Read {
            path: file_path.to_owned(),
            reason: error.to_string(),
        })?;
        match FontContainer::detect(Path::new(file_path), &bytes, face_index) {
            FontContainer::LegacyBitmap(format) => Err(FontDbSourceError::Unsupported { format }),
            // Fontconfig may resolve to WOFF/WOFF2. fontdb/ttf-parser doesn't
            // parse those containers directly, so decode to SFNT first.
            FontContainer::WebFont => {
                let ids = Self::load_web_font_source(db, file_path, face_index, &bytes)?;
                let selected_id =
                    ids.first()
                        .copied()
                        .ok_or_else(|| FontDbSourceError::MissingFace {
                            path: file_path.to_owned(),
                            face_index,
                        })?;
                // Decoding extracts exactly the requested collection member
                // into a standalone SFNT, whose local face index is zero. The
                // durable source selector remains `face_index`; `selected_id`
                // is the explicit mapping between those two domains.
                Ok(OpenedFontDbSource { ids, selected_id })
            }
            FontContainer::Sfnt => {
                let ids: Vec<_> = db
                    .load_font_source(fontdb::Source::File(file_path.into()))
                    .into_iter()
                    .collect();
                if ids.is_empty() {
                    Err(FontDbSourceError::Rejected {
                        path: file_path.to_owned(),
                    })
                } else {
                    let selected_id = ids
                        .iter()
                        .copied()
                        .find(|&id| db.face(id).map(|face| face.index) == Some(face_index));
                    let Some(selected_id) = selected_id else {
                        // `load_font_source` mutates fontdb. A rejected source
                        // selector must not leave unrelated collection faces
                        // registered behind an error result.
                        for id in ids {
                            db.remove_face(id);
                        }
                        return Err(FontDbSourceError::MissingFace {
                            path: file_path.to_owned(),
                            face_index,
                        });
                    };
                    Ok(OpenedFontDbSource { ids, selected_id })
                }
            }
        }
    }

    /// Register one exact source face under a caller-owned synthetic family.
    /// The returned database id remains authoritative even when the source
    /// was decoded from WOFF into an in-memory `fontdb::Source::SharedFile`.
    fn pin_face_as_family(
        db: &mut fontdb::Database,
        file_path: &str,
        face_index: u32,
        synthetic_family: &str,
    ) -> Result<fontdb::ID, FontDbSourceError> {
        let opened = Self::open_exact_face(db, file_path, face_index)?;
        let info = db.face(opened.selected_id).cloned();
        for id in &opened.ids {
            db.remove_face(*id);
        }
        let mut info = info.ok_or_else(|| FontDbSourceError::MissingFace {
            path: file_path.to_owned(),
            face_index,
        })?;
        info.families = vec![(
            synthetic_family.to_owned(),
            fontdb::Language::English_UnitedStates,
        )];
        Ok(db.push_face_info(info))
    }

    fn pin_asset_as_family(
        db: &mut fontdb::Database,
        asset: &FontOutlineAsset,
        synthetic_family: &str,
    ) -> Result<fontdb::ID, FontDbSourceError> {
        match asset {
            FontOutlineAsset::File(asset) => {
                Self::pin_face_as_family(db, asset.path(), asset.face_index(), synthetic_family)
            }
            FontOutlineAsset::Memory(asset) => {
                let ids: Vec<_> = db
                    .load_font_source(fontdb::Source::Binary(asset.shared_bytes()))
                    .into_iter()
                    .collect();
                if ids.is_empty() {
                    return Err(FontDbSourceError::Rejected {
                        path: asset.key().to_owned(),
                    });
                }
                let selected_id = ids
                    .iter()
                    .copied()
                    .find(|&id| db.face(id).map(|face| face.index) == Some(asset.face_index()));
                let info = selected_id.and_then(|id| db.face(id).cloned());
                for id in ids {
                    db.remove_face(id);
                }
                let mut info = info.ok_or_else(|| FontDbSourceError::MissingFace {
                    path: asset.key().to_owned(),
                    face_index: asset.face_index(),
                })?;
                info.families = vec![(
                    synthetic_family.to_owned(),
                    fontdb::Language::English_UnitedStates,
                )];
                Ok(db.push_face_info(info))
            }
        }
    }

    fn load_web_font_source(
        db: &mut fontdb::Database,
        file_path: &str,
        face_index: u32,
        bytes: &[u8],
    ) -> Result<Vec<fontdb::ID>, FontDbSourceError> {
        let sfnt = Self::decode_web_font_to_sfnt(file_path, face_index, bytes)?;
        ttf_parser::Face::parse(&sfnt, 0).map_err(|error| FontDbSourceError::Decode {
            path: file_path.to_owned(),
            reason: format!("decoded face {face_index} is not a valid standalone SFNT: {error}"),
        })?;
        // Keep the platform-selected webfont path as the face's durable
        // identity while fontdb consumes the decoded standalone SFNT bytes.
        // `Binary` would erase that identity and make exact-primary
        // publication report no source file.
        let ids = db.load_font_source(fontdb::Source::SharedFile(
            PathBuf::from(file_path),
            Arc::new(sfnt),
        ));
        if ids.is_empty() {
            return Err(FontDbSourceError::Rejected {
                path: file_path.to_owned(),
            });
        }
        Ok(ids.into_iter().collect())
    }

    fn decode_web_font_to_sfnt(
        file_path: &str,
        face_index: u32,
        bytes: &[u8],
    ) -> Result<Vec<u8>, FontDbSourceError> {
        let ctxt = ReadScope::new(bytes);
        let font_data = ctxt
            .read::<FontData<'_>>()
            .map_err(|error| FontDbSourceError::Decode {
                path: file_path.to_owned(),
                reason: format!("{error:?}"),
            })?;

        let provider = font_data
            .table_provider(face_index as usize)
            .map_err(|error| FontDbSourceError::Decode {
                path: file_path.to_owned(),
                reason: format!("face {face_index}: {error:?}"),
            })?;

        let mut tags = provider.table_tags().unwrap_or_default();
        if tags.is_empty() {
            return Err(FontDbSourceError::MissingTables {
                path: file_path.to_owned(),
            });
        }
        tags.sort_unstable();
        tags.dedup();

        let mut tables = Vec::with_capacity(tags.len());
        for tag in tags {
            let mut data = match provider.table_data(tag) {
                Ok(Some(data)) => data.into_owned(),
                Ok(None) => continue,
                Err(error) => {
                    return Err(FontDbSourceError::Decode {
                        path: file_path.to_owned(),
                        reason: format!("table {tag:#010x}: {error:?}"),
                    });
                }
            };

            // OpenType requires this field zeroed while checksums are computed.
            if tag == HEAD_TAG && data.len() >= 12 {
                data[8..12].fill(0);
            }
            tables.push((tag, data));
        }

        if tables.is_empty() {
            return Err(FontDbSourceError::MissingTables {
                path: file_path.to_owned(),
            });
        }

        let sfnt_version = if provider.sfnt_version() == TTCF_TAG {
            // allsorts currently reports the WOFF2 container's `ttcf` flavor
            // for every selected collection member. We emit one standalone
            // face, so its header must name that face's outline flavor.
            if tables
                .iter()
                .any(|(tag, _)| *tag == CFF_TAG || *tag == CFF2_TAG)
            {
                OTTO_TAG
            } else {
                TRUE_TYPE_TAG
            }
        } else {
            provider.sfnt_version()
        };
        Ok(Self::serialize_sfnt(sfnt_version, tables))
    }

    fn serialize_sfnt(sfnt_version: u32, tables: Vec<(u32, Vec<u8>)>) -> Vec<u8> {
        #[derive(Clone, Copy)]
        struct Record {
            tag: u32,
            checksum: u32,
            offset: u32,
            length: u32,
        }

        let num_tables = tables.len() as u16;
        let table_dir_len = 12usize + tables.len() * 16;
        let table_data_start = Self::align4(table_dir_len);
        let mut out = vec![0u8; table_data_start];
        let mut records = Vec::with_capacity(tables.len());

        for (tag, table) in tables {
            let length = table.len() as u32;
            let checksum = Self::checksum(&table);
            let offset = out.len() as u32;
            out.extend_from_slice(&table);

            let pad = (4 - (table.len() % 4)) % 4;
            if pad > 0 {
                out.resize(out.len() + pad, 0);
            }

            records.push(Record {
                tag,
                checksum,
                offset,
                length,
            });
        }

        let (search_range, entry_selector, range_shift) = Self::sfnt_search_params(num_tables);
        Self::write_u32_be(&mut out, 0, sfnt_version);
        Self::write_u16_be(&mut out, 4, num_tables);
        Self::write_u16_be(&mut out, 6, search_range);
        Self::write_u16_be(&mut out, 8, entry_selector);
        Self::write_u16_be(&mut out, 10, range_shift);

        for (i, rec) in records.iter().enumerate() {
            let base = 12 + i * 16;
            Self::write_u32_be(&mut out, base, rec.tag);
            Self::write_u32_be(&mut out, base + 4, rec.checksum);
            Self::write_u32_be(&mut out, base + 8, rec.offset);
            Self::write_u32_be(&mut out, base + 12, rec.length);
        }

        if let Some(head_rec) = records.iter().find(|rec| rec.tag == HEAD_TAG) {
            let head_off = head_rec.offset as usize;
            if head_off + 12 <= out.len() {
                let whole_sum = Self::checksum(&out);
                let check_sum_adjustment = 0xB1B0_AFBAu32.wrapping_sub(whole_sum);
                Self::write_u32_be(&mut out, head_off + 8, check_sum_adjustment);
            }
        }

        out
    }

    fn sfnt_search_params(num_tables: u16) -> (u16, u16, u16) {
        if num_tables == 0 {
            return (0, 0, 0);
        }

        let mut max_pow2 = 1u16;
        let mut entry_selector = 0u16;
        while max_pow2.saturating_mul(2) <= num_tables {
            max_pow2 *= 2;
            entry_selector += 1;
        }
        let search_range = max_pow2 * 16;
        let range_shift = num_tables * 16 - search_range;
        (search_range, entry_selector, range_shift)
    }

    fn checksum(bytes: &[u8]) -> u32 {
        let mut sum = 0u32;
        for chunk in bytes.chunks(4) {
            let mut word = [0u8; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            sum = sum.wrapping_add(u32::from_be_bytes(word));
        }
        sum
    }

    fn write_u16_be(out: &mut [u8], offset: usize, value: u16) {
        out[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }

    fn write_u32_be(out: &mut [u8], offset: usize, value: u32) {
        out[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    fn align4(value: usize) -> usize {
        (value + 3) & !3
    }
}

#[cfg(test)]
#[path = "fontdb_test.rs"]
mod tests;

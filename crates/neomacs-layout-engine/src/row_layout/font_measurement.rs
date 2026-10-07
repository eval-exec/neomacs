//! Worker-local native font handles. Jobs carry identity/metric receipts, never
//! fontdb IDs, native handles, or evaluator values. Only numeric measurements
//! leave this service; renderer font IDs remain owned by the evaluator.

use super::program::RowProgramError;
use crate::display_row::face_state::DisplayRowFace;
use crate::font::metrics::FontMetricsService;
use neomacs_display_protocol::DeviceScale;
use neomacs_display_protocol::font::{
    FontCatalogGeneration, ResolvedFont, ResolvedFontAdvance, ResolvedFontIdentity,
};

const MAX_ROW_FONT_BYTES: usize = 4096;
const MAX_CACHED_FACES: usize = 64;
const MAX_CACHED_CHARACTER_QUERIES: usize = 512;

#[derive(Clone, Debug, PartialEq)]
struct PrimaryFontReceipt {
    identity: ResolvedFontIdentity,
    pixel_size: f32,
    ascent: f32,
    descent: f32,
    space_advance: f32,
    glyph_advance: ResolvedFontAdvance,
}

impl PrimaryFontReceipt {
    fn of(font: ResolvedFont) -> Self {
        Self {
            identity: font.identity,
            pixel_size: font.pixel_size,
            ascent: font.ascent_px,
            descent: font.descent_px,
            space_advance: font.space_advance_px,
            glyph_advance: font.glyph_advance,
        }
    }

    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.identity.stable_key.len()
            + self.identity.file_path.as_ref().map_or(0, String::len)
            + self
                .identity
                .postscript_name
                .as_ref()
                .map_or(0, String::len)
            + self.identity.variation_coords.len()
                * std::mem::size_of::<neomacs_display_protocol::font::FontVariationCoord>()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MeasurementRevision {
    catalog: FontCatalogGeneration,
    fontset: u64,
    scale_bits: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FontMeasurementSnapshot {
    revision: MeasurementRevision,
    receipts: Vec<PrimaryFontReceipt>,
    policy: crate::font::metrics::DetachedFontPolicy,
    bytes: usize,
}

impl FontMeasurementSnapshot {
    pub(crate) fn capture(
        faces: &[DisplayRowFace],
        fonts: &mut FontMetricsService,
        items: &[crate::display_item::DisplayItem],
    ) -> Result<Self, RowProgramError> {
        if faces.len() > MAX_CACHED_FACES {
            return Err(RowProgramError::Budget);
        }
        let mut receipts = Vec::with_capacity(faces.len());
        let mut bytes = std::mem::size_of::<Self>();
        for face in faces {
            let font = fonts
                .resolved_font_for_face(
                    &face.font_family,
                    face.font_weight,
                    face.italic,
                    face.font_size.max(1.0),
                )
                .ok_or(RowProgramError::Unsupported)?;
            let receipt = PrimaryFontReceipt::of(font);
            bytes = bytes.saturating_add(receipt.bytes());
            if bytes > MAX_ROW_FONT_BYTES {
                return Err(RowProgramError::Budget);
            }
            receipts.push(receipt);
        }
        let mut requests = Vec::new();
        for item in items {
            let base = faces.first().ok_or(RowProgramError::Unsupported)?.face_id;
            let id = crate::display_face_ref::render_face_ref_id(item.face, base);
            let measurement = match &item.kind {
                crate::display_item::DisplayItemKind::SourceMappedText(text) => {
                    text.measurement_face
                }
                _ => None,
            };
            for id in std::iter::once(id).chain(measurement.filter(|other| *other != id)) {
                let face = faces
                    .iter()
                    .find(|face| face.face_id == id)
                    .ok_or(RowProgramError::Unsupported)?;
                let text = match &item.kind {
                    crate::display_item::DisplayItemKind::TextRun(run) => Some(run.text.as_ref()),
                    crate::display_item::DisplayItemKind::SourceMappedText(run) => {
                        Some(run.text.as_ref())
                    }
                    _ => None,
                };
                let source = match &item.kind {
                    crate::display_item::DisplayItemKind::Stretch(
                        crate::display_item::DisplayStretch {
                            width:
                                crate::display_item::DisplayStretchWidth::RelativeToSource {
                                    source,
                                    ..
                                },
                            ..
                        },
                    ) => source.as_rust_char(),
                    _ => None,
                };
                for ch in text
                    .unwrap_or("")
                    .chars()
                    .chain(source)
                    .filter(|ch| !ch.is_ascii())
                {
                    let request = (
                        face.font_family.as_str(),
                        ch,
                        face.font_weight,
                        face.italic,
                        face.font_size.max(1.0),
                    );
                    if !requests.contains(&request) {
                        if requests.len() == 128 {
                            return Err(RowProgramError::Budget);
                        }
                        requests.push(request);
                    }
                }
            }
        }
        let policy = fonts
            .capture_worker_font_policy(&requests, MAX_ROW_FONT_BYTES.saturating_sub(bytes))
            .ok_or(RowProgramError::Unsupported)?;
        bytes += policy.bytes();
        Ok(Self {
            revision: MeasurementRevision {
                catalog: fonts.font_catalog_generation(),
                fontset: neovm_core::emacs_core::fontset::fontset_generation(),
                scale_bits: fonts.device_scale().get().to_bits(),
            },
            receipts,
            policy,
            bytes,
        })
    }

    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }
}

/// The thread creates and destroys its own fontdb/native handles. A revision
/// change or a bounded working-set overflow rebuilds this cache on the worker.
#[derive(Default)]
pub(crate) struct WorkerFontMeasurements {
    service: Option<FontMetricsService>,
    revision: Option<MeasurementRevision>,
    faces: Vec<(String, u16, bool, u32)>,
    #[cfg(test)]
    cache_rebuilds: usize,
}

impl WorkerFontMeasurements {
    pub(crate) fn prepare(
        &mut self,
        snapshot: &FontMeasurementSnapshot,
        faces: &[DisplayRowFace],
        cancelled: &impl Fn() -> bool,
    ) -> Result<&mut FontMetricsService, RowProgramError> {
        if cancelled() || faces.len() != snapshot.receipts.len() {
            return Err(RowProgramError::Cancelled);
        }
        let mut new_faces: rustc_hash::FxHashSet<_> = faces
            .iter()
            .map(|face| {
                (
                    face.font_family.as_str(),
                    face.font_weight,
                    face.italic,
                    face.font_size.to_bits(),
                )
            })
            .collect();
        for (family, weight, italic, size) in &self.faces {
            new_faces.remove(&(family.as_str(), *weight, *italic, *size));
        }
        if self.revision != Some(snapshot.revision)
            || self.faces.len() + new_faces.len() > MAX_CACHED_FACES
            || self.service.as_ref().is_none_or(|service| {
                !service
                    .worker_font_policy_fits_cache(&snapshot.policy, MAX_CACHED_CHARACTER_QUERIES)
            })
        {
            self.service = Some(FontMetricsService::new());
            #[cfg(test)]
            {
                self.cache_rebuilds += 1;
            }
            self.revision = Some(snapshot.revision);
            self.faces.clear();
        }
        let service = self
            .service
            .as_mut()
            .ok_or(RowProgramError::MissingMeasurement)?;
        service.set_device_scale(
            DeviceScale::new(f32::from_bits(snapshot.revision.scale_bits))
                .map_err(|_| RowProgramError::Unsupported)?,
        );
        for (face, expected) in faces.iter().zip(&snapshot.receipts) {
            if cancelled() {
                return Err(RowProgramError::Cancelled);
            }
            let actual = service
                .resolved_font_for_face(
                    &face.font_family,
                    face.font_weight,
                    face.italic,
                    face.font_size.max(1.0),
                )
                .map(PrimaryFontReceipt::of)
                .ok_or(RowProgramError::MissingMeasurement)?;
            // A newly opened font must agree with the frame's selected font.
            // A catalog race is an admission failure, never guessed geometry.
            if &actual != expected {
                return Err(RowProgramError::Cancelled);
            }
            let key = (
                face.font_family.clone(),
                face.font_weight,
                face.italic,
                face.font_size.to_bits(),
            );
            if !self.faces.contains(&key) {
                self.faces.push(key);
            }
        }
        service.install_worker_font_policy(&snapshot.policy);
        Ok(service)
    }
}

#[cfg(test)]
#[path = "tests/font_measurement_test.rs"]
mod tests;

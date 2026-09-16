//! Ordered worker stream with explicitly owned font resources.
//!
//! Only the private wire frame may have an unresolved font table. Decoding
//! restores native bindings before rendering. Cache residency follows the
//! latest received frame; older presentations retain their own font Arcs.

use crate::BrowserPresentation;
use neomacs_display_protocol::{DecodedImage, FrameDisplayState, ImageId, font::*};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU64;
use std::sync::Arc;

mod bytes;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
struct FontResourceId(NonZeroU64);

#[derive(Serialize, Deserialize)]
struct FontReference {
    resource: FontResourceId,
    face_index: u32,
}

#[derive(Serialize, Deserialize)]
struct FontUpload {
    id: FontResourceId,
    key: String,
    #[serde(with = "bytes")]
    bytes: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct WirePresentation {
    frame: FrameDisplayState,
    fonts: HashMap<ResolvedFontId, ResolvedFont<FontReference>>,
    font_resources: Vec<FontUpload>,
    images: Vec<DecodedImage>,
    retired_images: Vec<ImageId>,
}

#[derive(Clone, Eq, Hash, PartialEq)]
struct ResourceKey {
    generation: FontCatalogGeneration,
    key: String,
}

#[derive(Clone)]
struct SentResource {
    id: FontResourceId,
    bytes: Arc<Vec<u8>>,
}

/// One encoder per ordered worker stream. Evicted resources are uploaded again
/// with fresh identities if they reappear; IDs are never recycled.
#[derive(Default)]
pub struct PresentationEncoder {
    resources: HashMap<ResourceKey, SentResource>,
    last_id: u64,
}

impl PresentationEncoder {
    /// Encode the next packet. Delivery failure is fatal to this stream: later
    /// packets may reference resources introduced by this one.
    pub fn encode(&mut self, presentation: BrowserPresentation) -> Result<Vec<u8>, String> {
        let BrowserPresentation {
            mut frame,
            images,
            retired_images,
        } = presentation;
        let generation = frame.font_catalog_generation;
        let mut live = HashMap::<ResourceKey, SentResource>::new();
        let mut uploads = Vec::new();
        let mut last_id = self.last_id;
        let mut fonts = HashMap::new();
        for (id, font) in std::mem::take(&mut frame.fonts) {
            if id != font.id {
                return Err("font binding ID mismatch".into());
            }
            let font = font.try_map_memory(|asset: FontMemoryAsset| {
                let key = ResourceKey {
                    generation,
                    key: asset.key().to_owned(),
                };
                let shared = asset.shared_bytes();
                let same_bytes = |resource: &SentResource| {
                    Arc::ptr_eq(&resource.bytes, &shared) || resource.bytes == shared
                };
                if let Some(resource) = live.get(&key) {
                    if !same_bytes(resource) {
                        return Err("conflicting font bytes within one presentation".to_owned());
                    }
                }
                let resource = if let Some(resource) = live
                    .get(&key)
                    .or_else(|| self.resources.get(&key).filter(|r| same_bytes(r)))
                {
                    resource.clone()
                } else {
                    last_id = last_id
                        .checked_add(1)
                        .ok_or("font resource identity exhausted")?;
                    let id = FontResourceId(NonZeroU64::new(last_id).unwrap());
                    uploads.push(FontUpload {
                        id,
                        key: key.key.clone(),
                        bytes: shared.as_ref().clone(),
                    });
                    SentResource { id, bytes: shared }
                };
                let reference = FontReference {
                    resource: resource.id,
                    face_index: asset.face_index(),
                };
                live.insert(key, resource);
                Ok(reference)
            })?;
            fonts.insert(id, font);
        }
        let wire = WirePresentation {
            frame,
            fonts,
            font_resources: uploads,
            images,
            retired_images,
        };
        let mut bytes = Vec::new();
        ciborium::ser::into_writer(&wire, &mut bytes).map_err(|e| e.to_string())?;
        self.resources = live;
        self.last_id = last_id;
        Ok(bytes)
    }
}

#[derive(Clone)]
struct ReceivedResource {
    generation: FontCatalogGeneration,
    key: String,
    bytes: Arc<Vec<u8>>,
}

/// Restores complete native bindings. Failed decoding leaves residency intact.
#[derive(Default)]
pub struct PresentationDecoder {
    resources: HashMap<FontResourceId, ReceivedResource>,
    last_id: u64,
}

impl PresentationDecoder {
    /// Decode every packet in order, even when its pixels will be coalesced.
    /// Once decoding succeeds, rejection by later frame/image validation must
    /// terminate the stream, not skip the packet and attempt recovery.
    pub fn decode(&mut self, bytes: &[u8]) -> Result<BrowserPresentation, String> {
        let wire: WirePresentation = ciborium::de::from_reader(bytes).map_err(|e| e.to_string())?;
        let WirePresentation {
            mut frame,
            fonts,
            font_resources,
            images,
            retired_images,
        } = wire;
        if !frame.fonts.is_empty() {
            return Err("inline font table in resource presentation".into());
        }
        let generation = frame.font_catalog_generation;
        let mut resources = self.resources.clone();
        let mut last_id = self.last_id;
        for upload in font_resources {
            if upload.id.0.get() <= last_id || upload.key.is_empty() || upload.bytes.is_empty() {
                return Err("invalid or stale font resource upload".into());
            }
            last_id = upload.id.0.get();
            resources.insert(
                upload.id,
                ReceivedResource {
                    generation,
                    key: upload.key,
                    bytes: Arc::new(upload.bytes),
                },
            );
        }
        let mut live = HashSet::new();
        for (id, font) in fonts {
            if id != font.id {
                return Err("font binding ID mismatch".into());
            }
            let font = font.try_map_memory(|reference: FontReference| {
                let resource = resources
                    .get(&reference.resource)
                    .ok_or("missing font resource")?;
                if resource.generation != generation {
                    return Err("stale font catalog resource".to_owned());
                }
                live.insert(reference.resource);
                FontMemoryAsset::new(
                    resource.key.clone(),
                    resource.bytes.clone(),
                    reference.face_index,
                )
                .ok_or_else(|| "invalid font memory asset".to_owned())
            })?;
            frame.fonts.insert(id, font);
        }
        resources.retain(|id, _| live.contains(id));
        self.resources = resources;
        self.last_id = last_id;
        Ok(BrowserPresentation {
            frame,
            images,
            retired_images,
        })
    }
}

#[cfg(test)]
mod tests;

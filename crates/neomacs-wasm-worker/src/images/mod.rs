//! Worker-local image catalog. Redisplay only queues; the wait boundary decodes.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use neomacs_display_protocol::{DecodedImage, ImageSequenceId};
use neomacs_image::portable::{EncodedImage, PortableImageDecoder};
use neovm_core::emacs_core::display_host::ImageHost;
use neovm_core::emacs_core::fileio::RuntimeResourceStore;
use neovm_core::emacs_core::image_catalog::*;

// Below the renderer's 64 MiB cache limit. Admission rather than silent GPU
// eviction keeps Ready identities resident until explicit catalog invalidation.
const MAX_RESIDENT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct BrowserImages {
    resources: Option<Rc<dyn RuntimeResourceStore>>,
    entries: RefCell<HashMap<ImageResolveRequest, ImageLookup>>,
    next_id: Cell<u32>,
    decoder: PortableImageDecoder,
    uploads: RefCell<Vec<DecodedImage>>,
    retirements: RefCell<Vec<ImageId>>,
    resident_bytes: RefCell<HashMap<ImageId, usize>>,
}

impl BrowserImages {
    pub(crate) fn new(resources: Option<Rc<dyn RuntimeResourceStore>>) -> Self {
        Self {
            resources,
            ..Self::default()
        }
    }

    fn resolve(&self, request: &ImageResolveRequest) -> ImageLookup {
        let pending = match self.entries.borrow().get(request) {
            Some(ImageLookup::Pending(pending)) => pending.clone(),
            Some(state) => return state.clone(),
            None => unreachable!("decode requests are admitted by lookup"),
        };
        let result = (|| {
            let bytes = match &request.source {
                ImageResolveSource::Data(ImageDataSource::Isolated(bytes)) => bytes.clone(),
                ImageResolveSource::Data(ImageDataSource::WithBaseUri { .. }) => {
                    return Err("browser images with external SVG resources are not supported");
                }
                ImageResolveSource::File(path) => {
                    let path = std::str::from_utf8(path.as_bytes())
                        .map_err(|_| "browser image file names must be UTF-8")?;
                    self.resources.as_ref()
                        .and_then(|resources| resources.file_contents(std::path::Path::new(path)))
                        .ok_or("browser image file is not a packaged runtime resource; supply user images as :data")?
                        .to_vec()
                }
            };
            let image = self.decoder.decode(EncodedImage {
                load: pending.load(),
                bytes,
                size: request.size,
                rotation: request.rotation,
                realization: request.realization,
                colors: request.colors,
                mask: request.mask,
                frame: request.frame,
                sequence: ImageSequenceId::new(u64::from(pending.load().image().get())).unwrap(),
            })?;
            let mut resident = self.resident_bytes.borrow_mut();
            if resident
                .values()
                .sum::<usize>()
                .saturating_add(image.data.len())
                > MAX_RESIDENT_BYTES
            {
                return Err(
                    "browser image cache budget exceeded; clear-image-cache before loading more images",
                );
            }
            resident.insert(image.load.image(), image.data.len());
            let ready = ReadyImage {
                load: image.load,
                metadata: ResolvedImageMetadata {
                    layout: image.metadata.layout,
                    reported: image.metadata.reported,
                    background: image.metadata.background,
                    background_transparent: image.metadata.background_transparent,
                    mask: image.metadata.mask,
                    embedded: image.metadata.embedded.clone(),
                },
            };
            self.uploads.borrow_mut().push(image);
            Ok(ready)
        })();
        let state = match result {
            Ok(ready) => ImageLookup::Ready(ready),
            Err(error) => ImageLookup::Failed(pending.failed(error.to_owned())),
        };
        self.entries
            .borrow_mut()
            .insert(request.clone(), state.clone());
        state
    }

    /// Runs only at the host wait boundary, never from redisplay's lookup.
    pub(crate) fn complete_pending(&self) -> Vec<ImageStateEvent> {
        let requests: Vec<_> = self
            .entries
            .borrow()
            .iter()
            .filter_map(|(request, state)| {
                matches!(state, ImageLookup::Pending(_)).then(|| request.clone())
            })
            .collect();
        requests
            .iter()
            .map(|request| {
                let load = match self.resolve(request) {
                    ImageLookup::Ready(image) => image.load,
                    ImageLookup::Failed(image) => image.load(),
                    ImageLookup::Pending(_) => unreachable!("decode completed"),
                };
                ImageStateEvent::DecodeCompleted(load)
            })
            .collect()
    }

    pub(crate) fn take_updates(&self) -> (Vec<DecodedImage>, Vec<ImageId>) {
        (
            std::mem::take(&mut *self.uploads.borrow_mut()),
            std::mem::take(&mut *self.retirements.borrow_mut()),
        )
    }
}

impl ImageCatalog for BrowserImages {
    fn lookup(&self, request: ImageResolveRequest) -> ImageLookup {
        let mut entries = self.entries.borrow_mut();
        entries
            .entry(request)
            .or_insert_with(|| {
                let id = self
                    .next_id
                    .get()
                    .checked_add(1)
                    .expect("browser image identity exhausted");
                self.next_id.set(id);
                ImageLookup::Pending(PendingImage::new(
                    ImageLoadToken::new(ImageId::new(id), ImageLoadAttempt::new(1).unwrap()),
                    ImageLayoutExtent::new(1, 1),
                ))
            })
            .clone()
    }

    fn invalidate(&self, target: ImageInvalidation) -> ImageInvalidationResult {
        let mut removed = Vec::new();
        self.entries.borrow_mut().retain(|request, state| {
            let remove = match &target {
                ImageInvalidation::All => true,
                ImageInvalidation::Spec { spec } => &request.spec == spec,
                ImageInvalidation::Dependency(source) => &request.source == source,
            };
            if remove {
                removed.push(state.placement().image_id());
            }
            !remove
        });
        if removed.is_empty() {
            return ImageInvalidationResult::Unchanged;
        }
        self.uploads
            .borrow_mut()
            .retain(|image| !removed.contains(&image.load.image()));
        self.resident_bytes
            .borrow_mut()
            .retain(|image, _| !removed.contains(image));
        self.retirements.borrow_mut().extend(removed);
        ImageInvalidationResult::Changed
    }

    fn cached_size_bytes(&self) -> i64 {
        self.resident_bytes.borrow().values().sum::<usize>() as i64
    }
}

pub(crate) struct BrowserImageHost(pub Rc<BrowserImages>);

impl ImageHost for BrowserImageHost {
    fn image_catalog(&self) -> &dyn ImageCatalog {
        self.0.as_ref()
    }

    fn image_catalog_shared(&self) -> Rc<dyn ImageCatalog> {
        self.0.clone()
    }

    fn resolve_image_sync(
        &self,
        request: ImageResolveRequest,
    ) -> Result<Option<ReadyImage>, String> {
        self.0.lookup(request.clone());
        match self.0.resolve(&request) {
            ImageLookup::Ready(image) => Ok(Some(image)),
            ImageLookup::Failed(image) => Err(image.error),
            ImageLookup::Pending(_) => unreachable!("explicit image query completed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neovm_core::emacs_core::{Context, Value};

    #[test]
    fn lisp_image_size_and_flush_use_decoded_svg_geometry() {
        let images = Rc::new(BrowserImages::default());
        let mut context = Context::new();
        let buffer = context.buffers.create_buffer("*image test*");
        let frame = context
            .frame_manager_mut()
            .create_frame("browser", 960, 640, buffer);
        context
            .frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .set_window_system(Some(Value::symbol("neo")));
        context.install_image_host(Box::new(BrowserImageHost(images.clone())));
        assert!(
            context.display_host.is_none(),
            "images must not claim window or font ownership"
        );
        let result = context.eval_str(r##"(progn
          (setq test-image '(image :type svg :width 160 :scale 1
            :data "<svg xmlns='http://www.w3.org/2000/svg' width='320' height='160'><rect width='320' height='160' fill='#00ff00'/></svg>"))
          (equal (image-size test-image t) '(160 . 80)))"##).unwrap();
        assert!(result.is_truthy());
        let (uploads, retired) = images.take_updates();
        assert!(retired.is_empty());
        assert_eq!(uploads.len(), 1);
        assert!(uploads[0].validate());
        assert_eq!(uploads[0].metadata.layout, ImageLayoutExtent::new(160, 80));
        context.eval_str("(image-size test-image t)").unwrap();
        assert!(
            images.take_updates().0.is_empty(),
            "cached query must not resend pixels"
        );
        context.eval_str("(image-flush test-image)").unwrap();
        assert_eq!(images.take_updates().1, vec![uploads[0].load.image()]);
        context.eval_str("(image-size test-image t)").unwrap();
        assert_ne!(
            images.take_updates().0[0].load.image(),
            uploads[0].load.image()
        );
    }
}

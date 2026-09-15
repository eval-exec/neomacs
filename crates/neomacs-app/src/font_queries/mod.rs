//! Worker-owned font queries over the same catalog and selection machinery as layout.
//! Query caches are separate from a borrowed redisplay engine, so Lisp font
//! queries during layout callbacks do not reenter its mutable transaction.
use neomacs_layout_engine::font::metrics::{FontMetricsService, SelectedFontInfo};
use neomacs_layout_engine::font::sizing::FontSizing;
use neovm_core::emacs_core::display_host::FontResolveRequest;
use neovm_core::emacs_core::eval::{
    FontEntityMetricsRequest, FontOtfCapability, FontSpecResolveRequest, FrameFontRequest,
    ResolvedFontEntityMetrics, ResolvedFontMatch, ResolvedFontSpecMatch, ResolvedFrameFont,
    ResolvedOpenedFont,
};
use neovm_core::face::FontWeight;
use neovm_core::heap_types::LispString;
use neovm_core::window::FrameId;
use std::cell::{Cell, RefCell, RefMut};
use std::rc::Rc;
mod conversion;
mod host;
use conversion::{core_font_px_metrics, font_otf_capability_for_asset};
pub use conversion::{core_opened_font_from_selection, font_otf_capability_for_file};

#[derive(Clone)]
pub struct FontQueryService {
    metrics: Rc<RefCell<Option<FontMetricsService>>>,
    font_sizing: Rc<Cell<FontSizing>>,
}

impl FontQueryService {
    pub fn new(font_sizing: FontSizing) -> Self {
        Self {
            metrics: Rc::new(RefCell::new(None)),
            font_sizing: Rc::new(Cell::new(font_sizing)),
        }
    }
    pub fn set_font_sizing(&self, font_sizing: FontSizing) {
        self.font_sizing.set(font_sizing);
    }
    fn synchronized_font_metrics(&self) -> RefMut<'_, FontMetricsService> {
        let mut metrics = self.metrics.borrow_mut();
        let service = metrics.get_or_insert_with(FontMetricsService::new);
        let _ = service.synchronize_font_catalog();
        RefMut::map(metrics, |metrics| {
            metrics.as_mut().expect("initialized font metrics")
        })
    }
    pub fn list_font_families(
        &self,
    ) -> Vec<neovm_core::emacs_core::display_host::AvailableFontFamilyName> {
        self.synchronized_font_metrics()
            .list_font_families()
            .into_iter()
            .filter_map(|family| {
                neovm_core::emacs_core::display_host::AvailableFontFamilyName::from_utf8(
                    family.as_str(),
                )
            })
            .collect()
    }
    pub fn resolve_font_for_char(
        &mut self,
        request: FontResolveRequest,
    ) -> Result<Option<ResolvedFontMatch>, String> {
        // cosmic-text/fontdb consume Unicode scalar values. Keep the full
        // Emacs character in the protocol and reject unsupported raw-byte or
        // non-Unicode codes only at this explicit backend boundary.
        let Some(character) = request.character.as_rust_char() else {
            return Ok(None);
        };
        let requested_family_storage = request.faces.ascii_face.family_runtime_string_owned();
        let requested_family = requested_family_storage.as_deref().unwrap_or("Monospace");
        let fontset_base_family_storage = request
            .faces
            .fontset_base_face
            .family_runtime_string_owned();
        let fontset_base_family = fontset_base_family_storage
            .as_deref()
            .unwrap_or("Monospace");
        let requested_weight = request
            .faces
            .ascii_face
            .weight
            .unwrap_or(FontWeight::NORMAL)
            .css_weight();
        let requested_italic = request
            .faces
            .ascii_face
            .slant
            .map(|slant| slant.is_italic())
            .unwrap_or(false);
        let font_size = self
            .font_sizing
            .get()
            .font_size_px_for_face(&request.faces.ascii_face);
        let selected = self
            .synchronized_font_metrics()
            .select_font_for_realized_face_char(
                character,
                neomacs_layout_engine::font::metrics::RealizedFaceFontSelection::new(
                    neomacs_layout_engine::font::metrics::PrimaryFontFamily::new(requested_family),
                    neomacs_layout_engine::font::metrics::FontsetBaseFamily::new(
                        fontset_base_family,
                    ),
                    requested_weight,
                    requested_italic,
                    font_size,
                ),
            );
        tracing::debug!(
            target: "neomacs::font_at",
            character = request.character.code(),
            requested_family,
            requested_weight,
            requested_italic,
            font_size,
            request_faces = ?request.faces,
            selected = ?selected,
            "display host resolved font-at request"
        );
        Ok(selected.map(|font| {
            let glyph_code = font.glyph_code;
            ResolvedFontMatch {
                glyph_code,
                font: core_opened_font_from_selection(font, |file, face_index| {
                    self.font_otf_capability(file, face_index).ok().flatten()
                }),
            }
        }))
    }

    pub fn resolve_frame_font(
        &mut self,
        frame_id: FrameId,
        request: FrameFontRequest,
    ) -> Result<Option<ResolvedFrameFont>, String> {
        // Every frame in this host shares one frontend/display connection.
        // Its logical point policy is therefore shared just like GNU's
        // display-level FRAME_RES; backing/device scale remains frame-local
        // and is applied later by the renderer.
        let font_sizing = self.font_sizing.get();
        let face = request.face();
        let requested_family_storage = face.family_runtime_string_owned();
        let requested_family = requested_family_storage.as_deref().unwrap_or("Monospace");
        let requested_weight = face.weight.unwrap_or(FontWeight::NORMAL).css_weight();
        let requested_italic = face.slant.map(|slant| slant.is_italic()).unwrap_or(false);
        let Some(font_size) = font_sizing.font_size_px_for_request(request.size()) else {
            return Ok(None);
        };
        let selected = self.synchronized_font_metrics().select_font_for_char(
            'M',
            requested_family,
            requested_weight,
            requested_italic,
            font_size.get(),
        );
        let Some(font) = selected else {
            return Ok(None);
        };
        let height_tenths =
            font_sizing.face_height_tenths_for_layout_pixels(font.metrics.pixel_size.max(1));
        tracing::debug!(
            frame_id = frame_id.0,
            requested_size = ?request.size(),
            realized_pixel_size = font.metrics.pixel_size,
            height_tenths,
            "resolved frame-local font geometry"
        );
        Ok(Some(ResolvedFrameFont {
            height_tenths,
            font: core_opened_font_from_selection(font, font_otf_capability_for_file),
        }))
    }

    pub fn resolve_font_for_spec(
        &mut self,
        request: FontSpecResolveRequest,
    ) -> Result<Option<ResolvedFontSpecMatch>, String> {
        let family = request
            .family
            .as_ref()
            .and_then(LispString::as_utf8_str)
            .and_then(neomacs_layout_engine::font_backend::FontFamilyName::new);
        let mut query = neomacs_layout_engine::font::resolver::FontEntityQuery::new(family)
            .with_selection(request.selection);
        if let Some(registry) = request.registry.as_ref().and_then(LispString::as_utf8_str) {
            query = query.with_registry(registry);
        }
        if let Some(language) = request.lang.as_ref().and_then(LispString::as_utf8_str) {
            query = query.with_language(language);
        }
        if let Some(weight) = request.weight {
            query = query.with_weight(weight.css_weight());
        }
        if let Some(slant) = request.slant {
            query = query.with_slant(slant);
        }
        if let Some(width) = request.width {
            query = query.with_width(width);
        }
        let entity = self.synchronized_font_metrics().resolve_font_entity(&query);
        Ok(entity.map(|entity| ResolvedFontSpecMatch {
            family: LispString::from_utf8(entity.matched.family()),
            foundry: entity
                .matched
                .metadata
                .foundry
                .as_ref()
                .map(|foundry| LispString::from_utf8(foundry)),
            registry: entity
                .registry
                .as_ref()
                .map(|registry| LispString::from_utf8(registry)),
            file: entity
                .matched
                .identity
                .file_path
                .as_ref()
                .map(|file| LispString::from_utf8(file)),
            weight: entity.matched.weight().map(FontWeight::from_css_weight),
            slant: Some(entity.matched.slant()),
            width: entity.matched.metadata.width,
            spacing: entity.matched.metadata.spacing,
            postscript_name: entity
                .matched
                .identity
                .postscript_name
                .as_ref()
                .map(|name| LispString::from_utf8(name)),
        }))
    }

    pub fn probe_font_px_metrics(
        &mut self,
        file: &str,
        face_index: u32,
        pixel_size: u32,
        wght: Option<f32>,
    ) -> Result<Option<neovm_core::emacs_core::eval::FontPxProbeResult>, String> {
        Ok(neomacs_layout_engine::font::probe::probe_font_px_metrics(
            file, face_index, pixel_size, wght,
        )
        .map(core_font_px_metrics))
    }

    pub fn probe_font_entity_metrics(
        &mut self,
        request: FontEntityMetricsRequest,
    ) -> Result<Option<ResolvedFontEntityMetrics>, String> {
        let pixel_size = self
            .font_sizing
            .get()
            .font_opening_size_px(request.size)
            .get();
        let family = request
            .family
            .as_ref()
            .and_then(LispString::as_utf8_str)
            .and_then(neomacs_layout_engine::font_backend::FontFamilyName::new);
        let mut query = neomacs_layout_engine::font::resolver::FontEntityQuery::new(family);
        if let Some(registry) = request.registry.as_ref().and_then(LispString::as_utf8_str) {
            query = query.with_registry(registry);
        }
        if let Some(postscript_name) = request
            .postscript_name
            .as_ref()
            .and_then(LispString::as_utf8_str)
        {
            query = query.with_postscript_name(postscript_name);
        }
        if let Some(weight) = request.weight {
            query = query.with_weight(weight.css_weight());
        }
        if let Some(slant) = request.slant {
            query = query.with_slant(slant);
        }
        if let Some(width) = request.width {
            query = query.with_width(width);
        }

        if let Some(opened) = self
            .synchronized_font_metrics()
            .open_font_entity(&query, pixel_size)
        {
            let file = opened
                .entity
                .matched
                .file_path()
                .map(|file| LispString::from_utf8(file));
            let capability = font_otf_capability_for_asset(&opened.entity.matched.asset);
            return Ok(Some(ResolvedFontEntityMetrics {
                metrics: core_font_px_metrics(opened.metrics),
                file,
                capability,
            }));
        }

        // Compatibility fallback for callers that only have a standalone
        // font file. Native entities must take the path above so a collection
        // face or named variation is not silently reopened as face zero.
        let Some(file) = request.file.as_ref().and_then(LispString::as_utf8_str) else {
            return Ok(None);
        };
        let Some(metrics) = neomacs_layout_engine::font::probe::probe_font_px_metrics(
            file,
            0,
            pixel_size,
            request.weight.map(|weight| f32::from(weight.css_weight())),
        ) else {
            return Ok(None);
        };
        let capability = font_otf_capability_for_file(file, 0);
        Ok(Some(ResolvedFontEntityMetrics {
            metrics: core_font_px_metrics(metrics),
            file: request.file,
            capability,
        }))
    }

    pub fn font_otf_capability(
        &mut self,
        file: &str,
        face_index: u32,
    ) -> Result<Option<neovm_core::emacs_core::eval::FontOtfCapability>, String> {
        Ok(font_otf_capability_for_file(file, face_index))
    }
}

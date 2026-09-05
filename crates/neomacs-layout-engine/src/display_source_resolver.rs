use crate::display_face_layout::{DisplayHeightFaceBasis, height_adjusted_face};
use crate::display_face_policy::BaseFacePolicy;
use crate::display_face_ref::render_face_ref_id;
use crate::display_item::{
    DisplayImageItem, DisplayItem, DisplayItemKind, DisplayMediaReplacement, DisplaySurfaceItem,
    DisplayVideoItem, DisplayXwidgetItem, RenderFaceRef,
};
use crate::display_origin::DisplayOrigin;
use crate::display_property::{
    DisplayMarginContent, DisplayMediaReplacementProperty, DisplayPropertyClassification,
    DisplayReplacementProperty,
};
use crate::display_row::face_state::DisplayRowActiveFaceState;
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_row::width::DisplayRowCharWidthPolicy;
use crate::display_source::{DisplayItemFaceResolver, DisplayItemSource, DisplaySourceContext};
use crate::display_source::{
    DisplayMarginEmission, DisplayMarginEmissionContent, DisplayPropertyReplacementSourceInputs,
    DisplayPropertyReplacementSourceItem, DisplayReplacementMediaSourceItem,
    DisplayReplacementMediaSourceResolution, DisplayReplacementSourceMappedTextItem,
};
use crate::display_spec::{DisplayImageDimensionEnvironment, DisplayImageSliceSpec};
use crate::display_spec::{
    DisplaySpecHead, DisplayVideoReference, parse_display_image_layout,
    parse_display_surface_source_layout, parse_display_video_layout, parse_display_webkit_layout,
};
use crate::font::metrics::FontMetricsService;
use crate::frame_face_arena::FrameFaceAttempt;
use crate::neovm_bridge::{
    BufferFaceRemapping, FaceResolver, LayoutBufferView, OrderedFaceSources, ResolvedFace,
};
use crate::types::WindowParams;
use crate::unicode::decode_utf8;
use neomacs_display_protocol::face::BasicFaceId;
use neomacs_display_protocol::types::FaceId;
use neovm_core::buffer::CharPos0;
use neovm_core::emacs_core::Value;
use neovm_core::emacs_core::emacs_char::EmacsChar;
use neovm_core::emacs_core::eval::{DisplayHost, SurfaceChannelKind};
use neovm_core::emacs_core::image_catalog::ImageScaleEnvironment;
use neovm_core::emacs_core::value::list_to_vec;
use neovm_core::emacs_core::video::parse_video_display_reference;
// Internal per-glyph face-resolution caches keyed by FaceId / cache keys
// (non-adversarial); FxHash, not std SipHash -- resolve_face_ref + remember_face
// were the largest residual SipHash callers in a Doom scroll profile after the
// atlas/font caches were swapped.
use rustc_hash::FxHashMap as HashMap;

#[derive(Clone, Copy)]
pub(crate) struct DisplaySourceFaceBasis<'a> {
    face_resolver: &'a FaceResolver,
    base_face_id: FaceId,
    base_face: &'a ResolvedFace,
    canonical_face: &'a ResolvedFace,
    fallback_metrics: DisplayRowFallbackMetrics,
}

impl<'a> DisplaySourceFaceBasis<'a> {
    pub(crate) fn new(
        face_resolver: &'a FaceResolver,
        base_face_id: FaceId,
        base_face: &'a ResolvedFace,
        fallback_metrics: DisplayRowFallbackMetrics,
    ) -> Self {
        Self {
            face_resolver,
            base_face_id,
            base_face,
            canonical_face: face_resolver.default_face(),
            fallback_metrics,
        }
    }

    pub(crate) fn face_resolver(self) -> &'a FaceResolver {
        self.face_resolver
    }

    pub(crate) fn base_face_id(self) -> FaceId {
        self.base_face_id
    }

    pub(crate) fn base_face(self) -> &'a ResolvedFace {
        self.base_face
    }

    pub(crate) fn canonical_face(self) -> &'a ResolvedFace {
        self.canonical_face
    }

    pub(crate) fn fallback_metrics(self) -> DisplayRowFallbackMetrics {
        self.fallback_metrics
    }

    fn height_basis(self) -> DisplayHeightFaceBasis<'a> {
        DisplayHeightFaceBasis {
            canonical_face: self.canonical_face(),
            base_face: self.base_face(),
            fallback_metrics: self.fallback_metrics(),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct DisplaySourceResolveParams<'a> {
    face_basis: DisplaySourceFaceBasis<'a>,
    display_host: Option<&'a dyn DisplayHost>,
    image_scale_environment: ImageScaleEnvironment,
}

impl<'a> DisplaySourceResolveParams<'a> {
    pub(crate) fn new(
        face_basis: DisplaySourceFaceBasis<'a>,
        display_host: Option<&'a dyn DisplayHost>,
        image_scale_environment: ImageScaleEnvironment,
    ) -> Self {
        Self {
            face_basis,
            display_host,
            image_scale_environment,
        }
    }

    pub(crate) fn face_basis(self) -> DisplaySourceFaceBasis<'a> {
        self.face_basis
    }

    fn display_host(self) -> Option<&'a dyn DisplayHost> {
        self.display_host
    }

    fn image_scale_environment(self) -> ImageScaleEnvironment {
        self.image_scale_environment
    }
}

#[derive(Default)]
pub(crate) struct DisplaySourceResolveState {
    face_cache: HashMap<DisplayFaceCacheKey, FaceId>,
    height_face_cache: HashMap<DisplayHeightFaceKey, FaceId>,
    resolved_faces: HashMap<FaceId, ResolvedFace>,
}

impl DisplaySourceResolveState {
    pub(crate) fn remember_face(&mut self, face_id: FaceId, face: &ResolvedFace) {
        self.resolved_faces.insert(face_id, face.clone());
    }

    pub(crate) fn resolved_face(&self, face_id: FaceId) -> Option<&ResolvedFace> {
        self.resolved_faces.get(&face_id)
    }

    fn cached_face(&self, base_face_id: FaceId, face_value: &Value) -> Option<RenderFaceRef> {
        self.face_cache
            .get(&DisplayFaceCacheKey {
                base_face_id,
                face_value: *face_value,
            })
            .copied()
            .map(RenderFaceRef::FaceId)
    }

    fn cache_face(
        &mut self,
        base_face_id: FaceId,
        face_value: Value,
        face_id: FaceId,
        resolved: &ResolvedFace,
    ) {
        self.face_cache.insert(
            DisplayFaceCacheKey {
                base_face_id,
                face_value,
            },
            face_id,
        );
        self.remember_face(face_id, resolved);
    }

    fn resolved_face_for(&self, face: RenderFaceRef, base_face: &ResolvedFace) -> ResolvedFace {
        let RenderFaceRef::FaceId(face_id) = face else {
            return base_face.clone();
        };
        self.resolved_faces
            .get(&face_id)
            .cloned()
            .unwrap_or_else(|| base_face.clone())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct DisplayHeightFaceKey {
    base_face_id: FaceId,
    factor_bits: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingDisplaySourceFace {
    face_id: FaceId,
    resolved: ResolvedFace,
}

impl PendingDisplaySourceFace {
    pub(crate) fn new(face_id: FaceId, resolved: ResolvedFace) -> Self {
        Self { face_id, resolved }
    }

    pub(crate) fn face_id(&self) -> FaceId {
        self.face_id
    }

    pub(crate) fn resolved(&self) -> &ResolvedFace {
        &self.resolved
    }

    pub(crate) fn into_parts(self) -> (FaceId, ResolvedFace) {
        (self.face_id, self.resolved)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct DisplayFaceCacheKey {
    base_face_id: FaceId,
    face_value: Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DisplayDefaultFaceInstallPolicy {
    InstallDefaultFace,
    ReuseInstalledDefaultFace,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ActiveDisplayStringBaseFace<'a> {
    face_id: FaceId,
    resolved: &'a ResolvedFace,
}

impl<'a> ActiveDisplayStringBaseFace<'a> {
    pub(crate) fn new(face_id: FaceId, resolved: &'a ResolvedFace) -> Self {
        Self { face_id, resolved }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DisplayStringBaseFace {
    face: ResolvedFace,
    face_id: FaceId,
    pending_face: Option<PendingDisplaySourceFace>,
}

impl DisplayStringBaseFace {
    pub(crate) fn face(&self) -> &ResolvedFace {
        &self.face
    }

    pub(crate) fn face_id(&self) -> FaceId {
        self.face_id
    }

    pub(crate) fn pending_face(&self) -> Option<&PendingDisplaySourceFace> {
        self.pending_face.as_ref()
    }
}

pub(crate) fn resolve_display_string_base_face<B: LayoutBufferView>(
    buffer: &B,
    face_resolver: &FaceResolver,
    origin: DisplayOrigin,
    policy: BaseFacePolicy,
    active_base_face: Option<ActiveDisplayStringBaseFace<'_>>,
    default_install_policy: DisplayDefaultFaceInstallPolicy,
    face_ids: &mut FrameFaceAttempt,
) -> DisplayStringBaseFace {
    let mut next_check = buffer.layout_point_max_char_pos().get();
    let face = face_resolver.base_face_for_origin(Some(buffer), &origin, policy, &mut next_check);

    let (face_id, pending_face) = if let Some(active_base_face) = active_base_face
        && same_resolved_face(&face, active_base_face.resolved)
    {
        (active_base_face.face_id, None)
    } else if same_resolved_face(&face, face_resolver.default_face()) {
        let face_id = FaceId::from(BasicFaceId::Default);
        let pending_face = match default_install_policy {
            DisplayDefaultFaceInstallPolicy::InstallDefaultFace => {
                Some(PendingDisplaySourceFace::new(face_id, face.clone()))
            }
            DisplayDefaultFaceInstallPolicy::ReuseInstalledDefaultFace => None,
        };
        (face_id, pending_face)
    } else {
        let face_id = crate::display_row::face_state::stable_face_id_for_resolved(face_ids, &face);
        let pending_face = Some(PendingDisplaySourceFace::new(face_id, face.clone()));
        (face_id, pending_face)
    };

    DisplayStringBaseFace {
        face,
        face_id,
        pending_face,
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedDisplaySourceItem {
    item: Option<DisplayItem>,
    pending_faces: Vec<PendingDisplaySourceFace>,
    pending_non_text_area: Vec<crate::display_source::DisplayNonTextAreaEmission>,
}

impl ResolvedDisplaySourceItem {
    pub(crate) fn new(
        item: Option<DisplayItem>,
        pending_faces: Vec<PendingDisplaySourceFace>,
    ) -> Self {
        Self {
            item,
            pending_faces,
            pending_non_text_area: Vec::new(),
        }
    }

    fn with_non_text_area(
        item: Option<DisplayItem>,
        pending_faces: Vec<PendingDisplaySourceFace>,
        pending_non_text_area: Vec<crate::display_source::DisplayNonTextAreaEmission>,
    ) -> Self {
        Self {
            item,
            pending_faces,
            pending_non_text_area,
        }
    }

    pub(crate) fn empty() -> Self {
        Self::new(None, Vec::new())
    }

    pub(crate) fn item(&self) -> Option<&DisplayItem> {
        self.item.as_ref()
    }

    pub(crate) fn take_pending_non_text_area(
        &mut self,
    ) -> Vec<crate::display_source::DisplayNonTextAreaEmission> {
        std::mem::take(&mut self.pending_non_text_area)
    }

    pub(crate) fn into_parts(self) -> (Option<DisplayItem>, Vec<PendingDisplaySourceFace>) {
        (self.item, self.pending_faces)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ResolvedDisplayReplacement {
    Media(DisplayMediaReplacement),
    Placeholder(&'static str),
}

fn resolved_media_replacement(geometry: DisplayMediaReplacement) -> ResolvedDisplayReplacement {
    ResolvedDisplayReplacement::Media(geometry)
}

pub(crate) fn resolve_display_replacement(
    display_prop: Value,
    replacement: &DisplayMediaReplacementProperty,
    display_host: Option<&dyn DisplayHost>,
    resolved_face: &ResolvedFace,
    fallback_metrics: DisplayRowFallbackMetrics,
    image_scale_environment: ImageScaleEnvironment,
    image_slice: Option<DisplayImageSliceSpec>,
) -> Option<ResolvedDisplayReplacement> {
    if let Some(media) = replacement.direct_replacement() {
        return Some(resolved_media_replacement(media));
    }

    if let Some(media) = resolve_display_property_media(
        &display_prop,
        display_host,
        resolved_face,
        fallback_metrics,
        image_scale_environment,
        image_slice,
    )
    .filter(|media| replacement.accepts_media_replacement(media))
    {
        return Some(resolved_media_replacement(media));
    }

    replacement
        .media_fallback_placeholder()
        .map(ResolvedDisplayReplacement::Placeholder)
}

impl DisplayReplacementMediaSourceItem {
    pub(crate) fn resolve_display_property(
        display_prop: Value,
        replacement: &DisplayMediaReplacementProperty,
        display_host: Option<&dyn DisplayHost>,
        active_face_state: &DisplayRowActiveFaceState,
        fallback_metrics: DisplayRowFallbackMetrics,
        image_scale_environment: ImageScaleEnvironment,
        image_slice: Option<DisplayImageSliceSpec>,
    ) -> Option<DisplayReplacementMediaSourceResolution> {
        match resolve_display_replacement(
            display_prop,
            replacement,
            display_host,
            active_face_state.resolved_face(),
            fallback_metrics,
            image_scale_environment,
            image_slice,
        )? {
            ResolvedDisplayReplacement::Media(media) => {
                Some(DisplayReplacementMediaSourceResolution::Media(Self::new(
                    media,
                    active_face_state.metrics().row_height(),
                    active_face_state.metrics().ascent(),
                    replacement.uses_xwidget_cursor_extents(),
                )))
            }
            ResolvedDisplayReplacement::Placeholder(placeholder) => {
                Some(DisplayReplacementMediaSourceResolution::Placeholder(
                    DisplayReplacementSourceMappedTextItem::new(placeholder),
                ))
            }
        }
    }
}

pub(crate) struct DisplayPropertyReplacementSourceResolveRequest<'a, 'source> {
    display_property: &'a DisplayPropertyClassification,
    anchor_charpos: CharPos0,
    source_text: &'source [u8],
    active_face_state: &'a DisplayRowActiveFaceState,
    font_metrics: &'a mut Option<FontMetricsService>,
    current_x: f32,
    content_x: f32,
    params: &'a WindowParams,
    display_host: Option<&'a dyn DisplayHost>,
}

impl<'a, 'source> DisplayPropertyReplacementSourceResolveRequest<'a, 'source> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_typed_replacement(
        display_property: &'a DisplayPropertyClassification,
        anchor_charpos: CharPos0,
        source_text: &'source [u8],
        active_face_state: &'a DisplayRowActiveFaceState,
        font_metrics: &'a mut Option<FontMetricsService>,
        current_x: f32,
        content_x: f32,
        params: &'a WindowParams,
        display_host: Option<&'a dyn DisplayHost>,
    ) -> Self {
        Self {
            display_property,
            anchor_charpos,
            source_text,
            active_face_state,
            font_metrics,
            current_x,
            content_x,
            params,
            display_host,
        }
    }

    fn face_metrics(&self) -> crate::display_row::metrics::DisplayRowMeasuredFaceMetrics {
        self.active_face_state.metrics()
    }

    pub(crate) fn resolve(self) -> Option<DisplayPropertyReplacementSourceItem> {
        let display_property = self.display_property;
        // The payload of the replacement is the SPEC that produced it, not the
        // whole `display` value -- see `DisplayPropertyClassification`.
        let replacement_value = display_property.replacement_spec();
        let anchor_charpos = self.anchor_charpos;
        let source_text = self.source_text;
        let face_metrics = self.face_metrics();
        let fallback_metrics = DisplayRowFallbackMetrics::from_measured_face(face_metrics);
        if let DisplayReplacementProperty::Margin(margin) = display_property.replacement()? {
            let content = match margin.content() {
                DisplayMarginContent::String(value) => DisplayMarginEmissionContent::String(*value),
                DisplayMarginContent::Stretch { layout, .. } => {
                    let (source_char, _) = decode_utf8(source_text);
                    DisplayMarginEmissionContent::Item(DisplayItemKind::Stretch(
                        layout.bind_source(EmacsChar::from_char(source_char)),
                    ))
                }
                DisplayMarginContent::Media {
                    spec,
                    replacement,
                    image_slice,
                } => {
                    let resolution = DisplayReplacementMediaSourceItem::resolve_display_property(
                        *spec,
                        replacement,
                        self.display_host,
                        self.active_face_state,
                        fallback_metrics,
                        self.params.image_scale_environment,
                        *image_slice,
                    )?;
                    let kind = match resolution {
                        DisplayReplacementMediaSourceResolution::Media(item) => {
                            DisplayItemKind::MediaReplacement(item.media())
                        }
                        DisplayReplacementMediaSourceResolution::Placeholder(item) => {
                            DisplayItemKind::SourceMappedText(
                                crate::display_item::DisplaySourceMappedText::new(item.into_text()),
                            )
                        }
                    };
                    DisplayMarginEmissionContent::Item(kind)
                }
            };
            return Some(DisplayPropertyReplacementSourceItem::Margin(
                DisplayMarginEmission::new(margin.side(), content),
            ));
        }
        let source_inputs = match display_property.replacement()? {
            DisplayReplacementProperty::String => {
                let replacement = replacement_value.as_utf8_str()?;
                let cursor_slot_width_px = self
                    .active_face_state
                    .display_replacement_string_cursor_slot_width(self.font_metrics, replacement);
                DisplayPropertyReplacementSourceInputs::empty()
                    .with_string_cursor_slot_width_px(cursor_slot_width_px)
            }
            DisplayReplacementProperty::Stretch(_) => {
                let (display_ch, _) = decode_utf8(source_text);
                let display_char_width = self
                    .active_face_state
                    .display_replacement_stretch_source_char_width(self.font_metrics, display_ch);
                DisplayPropertyReplacementSourceInputs::empty()
                    .with_stretch_display_char_width_px(display_char_width)
            }
            DisplayReplacementProperty::Media(media_replacement) => {
                let media = DisplayReplacementMediaSourceItem::resolve_display_property(
                    replacement_value,
                    media_replacement,
                    self.display_host,
                    self.active_face_state,
                    fallback_metrics,
                    self.params.image_scale_environment,
                    display_property.image_slice(),
                )?;
                DisplayPropertyReplacementSourceInputs::empty().with_media(media)
            }
            // `(left-fringe …)`: no inline output. The covered text is still
            // consumed (the descriptor's skip range), and the empty source item
            // emits no glyph.
            DisplayReplacementProperty::Fringe(_) => {
                DisplayPropertyReplacementSourceInputs::empty()
            }
            // `((margin …) …)`: marginal-area content, no inline output — the
            // covered placeholder is suppressed (#188).
            DisplayReplacementProperty::Margin(_) => unreachable!("returned above"),
        };
        DisplayPropertyReplacementSourceItem::from_display_property_parts(
            display_property,
            anchor_charpos,
            self.current_x,
            self.content_x,
            self.params,
            fallback_metrics,
            source_inputs,
        )
    }
}

/// Selects the namespace used when a display source resolves named faces.
/// There is intentionally no default: each source state must declare whether
/// its strings are frame-owned chrome or belong to a displayed buffer.
#[derive(Clone, Copy, Debug)]
pub(crate) enum DisplaySourceFaceScope {
    FrameLocal,
    BufferLocal(BufferFaceRemapping),
}

impl DisplaySourceFaceScope {
    pub(crate) fn for_buffer(buffer: &impl LayoutBufferView) -> Self {
        Self::BufferLocal(BufferFaceRemapping::capture(buffer))
    }

    fn resolve_face_value_over(
        self,
        resolver: &FaceResolver,
        base: &ResolvedFace,
        face_value: &Value,
    ) -> Option<ResolvedFace> {
        match self {
            Self::FrameLocal => resolver.resolve_face_value_over(base, face_value),
            Self::BufferLocal(remapping) => {
                resolver.resolve_remapped_face_value_over(remapping, base, face_value)
            }
        }
    }

    fn resolve_named_face(self, resolver: &FaceResolver, name: &str) -> ResolvedFace {
        match self {
            Self::FrameLocal => resolver.resolve_named_face(name),
            Self::BufferLocal(remapping) => resolver.resolve_remapped_named_face(remapping, name),
        }
    }

    fn resolve_lisp_face_over(
        self,
        resolver: &FaceResolver,
        base: &ResolvedFace,
        lisp_face_id: neovm_core::face::LispFaceId,
        face_ref: &Value,
    ) -> Option<ResolvedFace> {
        match self {
            Self::FrameLocal => resolver.resolve_lisp_face_over(base, lisp_face_id),
            Self::BufferLocal(remapping) => {
                resolver.resolve_remapped_face_value_over(remapping, base, face_ref)
            }
        }
    }

    fn resolve_face_sources_over(
        self,
        resolver: &FaceResolver,
        base: &ResolvedFace,
        sources: &OrderedFaceSources,
    ) -> Option<ResolvedFace> {
        match self {
            Self::FrameLocal => resolver.resolve_face_sources_over(base, sources),
            Self::BufferLocal(remapping) => {
                resolver.resolve_remapped_face_sources_over(remapping, base, sources)
            }
        }
    }
}

pub(crate) struct DisplaySourcePropertyResolver<'a> {
    face_scope: DisplaySourceFaceScope,
    params: DisplaySourceResolveParams<'a>,
    state: &'a mut DisplaySourceResolveState,
    face_ids: &'a mut FrameFaceAttempt,
    pending_faces: &'a mut Vec<PendingDisplaySourceFace>,
}

impl<'a> DisplaySourcePropertyResolver<'a> {
    #[cfg(test)]
    pub(crate) fn frame_local(
        params: DisplaySourceResolveParams<'a>,
        state: &'a mut DisplaySourceResolveState,
        face_ids: &'a mut FrameFaceAttempt,
        pending_faces: &'a mut Vec<PendingDisplaySourceFace>,
    ) -> Self {
        Self::with_scope(
            DisplaySourceFaceScope::FrameLocal,
            params,
            state,
            face_ids,
            pending_faces,
        )
    }

    pub(crate) fn buffer_local(
        buffer: &impl LayoutBufferView,
        params: DisplaySourceResolveParams<'a>,
        state: &'a mut DisplaySourceResolveState,
        face_ids: &'a mut FrameFaceAttempt,
        pending_faces: &'a mut Vec<PendingDisplaySourceFace>,
    ) -> Self {
        Self::with_scope(
            DisplaySourceFaceScope::for_buffer(buffer),
            params,
            state,
            face_ids,
            pending_faces,
        )
    }

    fn with_scope(
        face_scope: DisplaySourceFaceScope,
        params: DisplaySourceResolveParams<'a>,
        state: &'a mut DisplaySourceResolveState,
        face_ids: &'a mut FrameFaceAttempt,
        pending_faces: &'a mut Vec<PendingDisplaySourceFace>,
    ) -> Self {
        let face_basis = params.face_basis();
        state.remember_face(face_basis.base_face_id(), face_basis.base_face());
        Self {
            face_scope,
            params,
            state,
            face_ids,
            pending_faces,
        }
    }

    fn resolve_item_layout(&mut self, mut item: DisplayItem) -> DisplayItem {
        if let Some(overlay) = item.kind.semantic_face_overlay() {
            item.face = self.resolve_face_ref(item.face, Value::symbol(overlay.face_name()));
        }
        if let Some(factor) = item
            .layout
            .height
            .filter(|factor| factor.is_finite() && *factor > 0.0)
        {
            item.face = self.resolve_height_face_ref(item.face, factor);
        }
        if let DisplayItemKind::RowBreak(row_break) = &mut item.kind {
            row_break.line_spacing =
                self.resolve_line_spacing_policy(item.face, row_break.line_spacing);
        }
        item
    }

    fn resolve_line_spacing_policy(
        &self,
        current_face: RenderFaceRef,
        policy: crate::display_item::DisplayLineSpacingPolicy,
    ) -> crate::display_item::DisplayLineSpacingPolicy {
        use crate::display_item::{DisplayLineSpacingPolicy, DisplayLineSpacingReference};

        let DisplayLineSpacingPolicy::Scale { factor, reference } = policy else {
            return policy;
        };
        let face_basis = self.params.face_basis();
        let current = || {
            self.state
                .resolved_face_for(current_face, face_basis.base_face())
        };
        let height = match reference {
            // GNU's bare float is relative to FRAME_FONT, not the newline's
            // effective face.  Keep the canonical frame face distinct from
            // the source iterator's base face (mode/header lines may differ).
            DisplayLineSpacingReference::DefaultFace => {
                face_basis.canonical_face().font_line_height
            }
            // `(nil . FACTOR)` uses the effective face at the newline.
            DisplayLineSpacingReference::CurrentFace => current().font_line_height,
            // GNU treats `(t . FACTOR)` as the current font and otherwise
            // performs an exact named-face lookup for the target window.
            DisplayLineSpacingReference::NamedFace(face) if face.is_t() => {
                current().font_line_height
            }
            DisplayLineSpacingReference::NamedFace(face) => face
                .as_symbol_name()
                .map(|name| {
                    self.face_scope
                        .resolve_named_face(face_basis.face_resolver(), name)
                        .font_line_height
                })
                .unwrap_or(0.0),
        };
        let pixels = factor * height;
        DisplayLineSpacingPolicy::Pixels(if pixels.is_finite() {
            pixels.max(0.0)
        } else {
            0.0
        })
    }

    fn resolve_height_face_ref(&mut self, face: RenderFaceRef, factor: f32) -> RenderFaceRef {
        let face_basis = self.params.face_basis();
        let fallback = face_basis.fallback_metrics();
        if fallback.char_width() <= 1.0 && fallback.row_height() <= 1.0 {
            return face;
        }

        let base_face_id = render_face_ref_id(face, face_basis.base_face_id());
        let key = DisplayHeightFaceKey {
            base_face_id,
            factor_bits: factor.to_bits(),
        };
        if let Some(face_id) = self.state.height_face_cache.get(&key).copied() {
            return RenderFaceRef::FaceId(face_id);
        }

        let source = self.state.resolved_face_for(face, face_basis.base_face());
        let Some(resolved) = height_adjusted_face(&source, face_basis.height_basis(), factor)
        else {
            return face;
        };
        if same_resolved_face(&resolved, &source) {
            return face;
        }

        let face_id =
            crate::display_row::face_state::stable_face_id_for_resolved(self.face_ids, &resolved);
        self.state.height_face_cache.insert(key, face_id);
        self.state.remember_face(face_id, &resolved);
        self.pending_faces
            .push(PendingDisplaySourceFace::new(face_id, resolved));
        RenderFaceRef::FaceId(face_id)
    }
}

fn resolve_source_face_ref(
    state: &mut DisplaySourceResolveState,
    face_ids: &mut FrameFaceAttempt,
    pending_faces: &mut Vec<PendingDisplaySourceFace>,
    face_basis: DisplaySourceFaceBasis<'_>,
    base: RenderFaceRef,
    face_value: Value,
    resolve: impl FnOnce(&ResolvedFace, &Value) -> Option<ResolvedFace>,
) -> RenderFaceRef {
    let base_face_id = render_face_ref_id(base, face_basis.base_face_id());
    if let Some(cached) = state.cached_face(base_face_id, &face_value) {
        return cached;
    }

    let base_resolved = state.resolved_face_for(base, face_basis.base_face());
    let Some(resolved) = resolve(&base_resolved, &face_value) else {
        return base;
    };

    if same_resolved_face(&resolved, &base_resolved) {
        state.cache_face(base_face_id, face_value, base_face_id, &base_resolved);
        return RenderFaceRef::FaceId(base_face_id);
    }

    let face_id = crate::display_row::face_state::stable_face_id_for_resolved(face_ids, &resolved);
    state.cache_face(base_face_id, face_value, face_id, &resolved);
    pending_faces.push(PendingDisplaySourceFace::new(face_id, resolved));
    RenderFaceRef::FaceId(face_id)
}

fn resolve_source_lisp_face_ref(
    state: &mut DisplaySourceResolveState,
    face_ids: &mut FrameFaceAttempt,
    pending_faces: &mut Vec<PendingDisplaySourceFace>,
    face_basis: DisplaySourceFaceBasis<'_>,
    face_scope: DisplaySourceFaceScope,
    base: RenderFaceRef,
    lisp_face_id: neovm_core::face::LispFaceId,
) -> RenderFaceRef {
    let Some(face_ref) = face_basis.face_resolver().lisp_face_ref(lisp_face_id) else {
        return base;
    };
    resolve_source_face_ref(
        state,
        face_ids,
        pending_faces,
        face_basis,
        base,
        face_ref,
        |base_resolved, face_ref| {
            face_scope.resolve_lisp_face_over(
                face_basis.face_resolver(),
                base_resolved,
                lisp_face_id,
                face_ref,
            )
        },
    )
}

fn resolve_source_face_sources(
    state: &mut DisplaySourceResolveState,
    face_ids: &mut FrameFaceAttempt,
    pending_faces: &mut Vec<PendingDisplaySourceFace>,
    face_basis: DisplaySourceFaceBasis<'_>,
    base: RenderFaceRef,
    sources: &OrderedFaceSources,
    resolve: impl FnOnce(&ResolvedFace, &OrderedFaceSources) -> Option<ResolvedFace>,
) -> RenderFaceRef {
    if sources.is_empty() {
        return base;
    }

    let base_face_id = render_face_ref_id(base, face_basis.base_face_id());
    let base_resolved = state.resolved_face_for(base, face_basis.base_face());
    let Some(resolved) = resolve(&base_resolved, sources) else {
        return base;
    };

    if same_resolved_face(&resolved, &base_resolved) {
        state.remember_face(base_face_id, &base_resolved);
        return RenderFaceRef::FaceId(base_face_id);
    }

    let face_id = crate::display_row::face_state::stable_face_id_for_resolved(face_ids, &resolved);
    state.remember_face(face_id, &resolved);
    pending_faces.push(PendingDisplaySourceFace::new(face_id, resolved));
    RenderFaceRef::FaceId(face_id)
}

impl DisplayItemFaceResolver for DisplaySourcePropertyResolver<'_> {
    fn face_has_box(&self, face: RenderFaceRef) -> bool {
        let base = self.params.face_basis().base_face();
        self.state.resolved_face_for(face, base).box_type > 0
    }

    fn resolve_face_ref(&mut self, base: RenderFaceRef, face_value: Value) -> RenderFaceRef {
        let face_basis = self.params.face_basis();
        let face_scope = self.face_scope;
        resolve_source_face_ref(
            self.state,
            self.face_ids,
            self.pending_faces,
            face_basis,
            base,
            face_value,
            |base_resolved, face_value| {
                face_scope.resolve_face_value_over(
                    face_basis.face_resolver(),
                    base_resolved,
                    face_value,
                )
            },
        )
    }

    fn resolve_lisp_face_ref(
        &mut self,
        base: RenderFaceRef,
        lisp_face_id: neovm_core::face::LispFaceId,
    ) -> RenderFaceRef {
        let face_basis = self.params.face_basis();
        resolve_source_lisp_face_ref(
            self.state,
            self.face_ids,
            self.pending_faces,
            face_basis,
            self.face_scope,
            base,
            lisp_face_id,
        )
    }

    fn resolve_face_sources(
        &mut self,
        base: RenderFaceRef,
        sources: &OrderedFaceSources,
    ) -> RenderFaceRef {
        let face_basis = self.params.face_basis();
        let face_scope = self.face_scope;
        resolve_source_face_sources(
            self.state,
            self.face_ids,
            self.pending_faces,
            face_basis,
            base,
            sources,
            |base_resolved, sources| {
                face_scope.resolve_face_sources_over(
                    face_basis.face_resolver(),
                    base_resolved,
                    sources,
                )
            },
        )
    }

    fn resolve_display_media_replacement(
        &mut self,
        display_prop: Value,
        image_slice: Option<DisplayImageSliceSpec>,
        face: RenderFaceRef,
    ) -> Option<DisplayMediaReplacement> {
        let face_basis = self.params.face_basis();
        let fallback = face_basis.fallback_metrics();
        let resolved_face = self.state.resolved_face_for(face, face_basis.base_face());
        resolve_display_property_media(
            &display_prop,
            self.params.display_host(),
            &resolved_face,
            fallback,
            self.params.image_scale_environment(),
            image_slice,
        )
    }
}

pub(crate) fn resolve_next_display_source_item(
    source: &mut impl DisplayItemSource,
    face_scope: DisplaySourceFaceScope,
    params: DisplaySourceResolveParams<'_>,
    state: &mut DisplaySourceResolveState,
    face_ids: &mut FrameFaceAttempt,
) -> ResolvedDisplaySourceItem {
    let mut pending_faces = Vec::new();
    let mut pending_non_text_area = Vec::new();
    let item = {
        let mut resolver = DisplaySourcePropertyResolver::with_scope(
            face_scope,
            params,
            state,
            face_ids,
            &mut pending_faces,
        );
        let mut context = DisplaySourceContext::with_face_resolver_and_non_text_area_sink(
            &mut resolver,
            &mut pending_non_text_area,
        );
        source
            .next_item(&mut context)
            .map(|item| resolver.resolve_item_layout(item))
    };
    ResolvedDisplaySourceItem::with_non_text_area(item, pending_faces, pending_non_text_area)
}

#[derive(Clone, Copy)]
pub(crate) struct DisplayMediaResolveParams<'a> {
    pub(crate) display_host: &'a dyn DisplayHost,
    pub(crate) default_fg: u32,
    pub(crate) default_bg: u32,
    pub(crate) fallback_metrics: DisplayRowFallbackMetrics,
    pub(crate) image_scale_environment: ImageScaleEnvironment,
    pub(crate) image_dimension_environment: DisplayImageDimensionEnvironment,
    pub(crate) image_slice: Option<DisplayImageSliceSpec>,
}

pub(crate) fn resolve_display_media_property(
    display_prop: &Value,
    params: DisplayMediaResolveParams<'_>,
) -> Option<DisplayMediaReplacement> {
    resolve_image_display_property(display_prop, params)
        .or_else(|| resolve_video_display_property(display_prop, params))
        .or_else(|| resolve_webkit_display_property(display_prop, params))
        .or_else(|| resolve_surface_display_property(display_prop, params))
}

fn resolve_image_display_property(
    display_prop: &Value,
    params: DisplayMediaResolveParams<'_>,
) -> Option<DisplayMediaReplacement> {
    let spec = parse_display_image_layout(display_prop, params.default_fg, params.default_bg)?;
    let ascent = spec.ascent;
    let margin = spec.margin;
    let request = spec.into_resolve_request(
        params.image_scale_environment,
        params.image_dimension_environment,
    );
    let lookup = params.display_host.image_catalog()?.lookup(request);
    let placement = lookup.placement();
    let opaque_background = lookup
        .ready_metadata()
        .and_then(|metadata| (!metadata.background_transparent).then_some(metadata.background));
    let full_width = placement.width().max(1) as f32;
    let full_height = placement.height().max(1) as f32;
    let (source_rect, width, height) = match params.image_slice {
        Some(slice) => {
            let Some(resolved) = slice.resolve(full_width, full_height) else {
                return Some(DisplayMediaReplacement::empty_image_slice());
            };
            (resolved.source_rect, resolved.width, resolved.height)
        }
        None => (
            neomacs_display_protocol::ImageSourceRect::FULL,
            full_width,
            full_height,
        ),
    };
    Some(DisplayMediaReplacement::image(DisplayImageItem {
        image_id: display_media_id(placement.image_id().get()),
        source_rect,
        width,
        height,
        ascent: ascent.resolve(
            height,
            params.fallback_metrics.row_height(),
            params.fallback_metrics.ascent(),
        ),
        horizontal_margin: margin.horizontal,
        vertical_margin: margin.vertical,
        opaque_background,
    }))
}

fn resolve_video_display_property(
    display_prop: &Value,
    params: DisplayMediaResolveParams<'_>,
) -> Option<DisplayMediaReplacement> {
    let spec = parse_display_video_layout(
        display_prop,
        DisplayRowCharWidthPolicy::new(params.fallback_metrics.char_width()).fallback() * 40.0,
        params.fallback_metrics.row_height() * 12.0,
    )?;
    let video_id = match &spec.reference {
        DisplayVideoReference::Session(id) => *id,
        DisplayVideoReference::Resolve(request) => {
            params
                .display_host
                .request_video(request.clone())
                .ok()
                .flatten()?
                .video_id
        }
    };
    Some(DisplayMediaReplacement::video(DisplayVideoItem {
        video_id,
        width: spec.width.max(1.0),
        height: spec.height.max(1.0),
        opacity: spec.opacity,
    }))
}

fn resolve_webkit_display_property(
    display_prop: &Value,
    params: DisplayMediaResolveParams<'_>,
) -> Option<DisplayMediaReplacement> {
    let spec = parse_display_webkit_layout(
        display_prop,
        DisplayRowCharWidthPolicy::new(params.fallback_metrics.char_width()).fallback() * 40.0,
        params.fallback_metrics.row_height() * 12.0,
    )?;
    let resolved = params
        .display_host
        .request_webkit(spec.request.clone())
        .ok()
        .flatten()?;
    Some(DisplayMediaReplacement::xwidget(DisplayXwidgetItem {
        xwidget_id: neomacs_display_protocol::XwidgetId::new(display_media_id(
            resolved.webview_id.get(),
        ) as u32),
        webview_id: resolved.webview_id,
        width: spec.width.max(1.0),
        height: spec.height.max(1.0),
    }))
}

/// Declarative `(surface :shader …)` spec: resolve through the display host,
/// which memoizes request content into a surface id (the video pattern). The
/// `:id` form never reaches here (classify resolves it directly).
///
/// `:channel0` accepts a surface id, an `(image …)` spec (resolved through
/// the nonblocking image catalog), or a `(video …)` spec (resolved through
/// the video host, `:autoplay` defaulting to t — a never-playing channel
/// samples black forever). The resolved `(kind, id)` becomes part of the
/// memo key.
fn resolve_surface_display_property(
    display_prop: &Value,
    params: DisplayMediaResolveParams<'_>,
) -> Option<DisplayMediaReplacement> {
    let mut spec = parse_display_surface_source_layout(
        display_prop,
        DisplayRowCharWidthPolicy::new(params.fallback_metrics.char_width()).fallback() * 40.0,
        params.fallback_metrics.row_height() * 4.0,
    )?;
    if let Some(channel_value) = spec.channel0_value {
        // A named-but-unresolvable channel keeps the surface unresolved
        // (blank) rather than silently rendering with a black channel.
        spec.request.channel0 = Some(resolve_surface_channel(&channel_value, params)?);
    }
    let resolved = params
        .display_host
        .request_surface(spec.request.clone())
        .ok()
        .flatten()?;
    Some(DisplayMediaReplacement::surface(DisplaySurfaceItem {
        surface_id: display_media_id(resolved.surface_id),
        width: spec.width.max(1.0),
        height: spec.height.max(1.0),
    }))
}

fn resolve_surface_channel(
    value: &Value,
    params: DisplayMediaResolveParams<'_>,
) -> Option<(SurfaceChannelKind, u32)> {
    if let Some(id) = value
        .as_surface_handle()
        .or_else(|| value.as_int().filter(|id| *id >= 0).map(|id| id as u32))
    {
        return Some((SurfaceChannelKind::Surface, id));
    }
    if DisplaySpecHead::Image.is_head_of(value) {
        let spec = parse_display_image_layout(value, params.default_fg, params.default_bg)?;
        let request = spec.into_resolve_request(
            params.image_scale_environment,
            params.image_dimension_environment,
        );
        let lookup = params.display_host.image_catalog()?.lookup(request);
        return Some((
            SurfaceChannelKind::Image,
            lookup.placement().image_id().get(),
        ));
    }
    if DisplaySpecHead::Video.is_head_of(value) {
        let items = list_to_vec(value)?;
        let video_id = match parse_video_display_reference(&items, true)? {
            DisplayVideoReference::Session(id) => id,
            DisplayVideoReference::Resolve(request) => {
                params
                    .display_host
                    .request_video(request)
                    .ok()
                    .flatten()?
                    .video_id
            }
        };
        return Some((SurfaceChannelKind::Video, video_id.get()));
    }
    None
}

fn display_media_id(id: u32) -> i32 {
    id.min(i32::MAX as u32) as i32
}

pub(crate) fn resolve_display_property_media(
    display_prop: &Value,
    display_host: Option<&dyn DisplayHost>,
    resolved_face: &ResolvedFace,
    fallback_metrics: DisplayRowFallbackMetrics,
    image_scale_environment: ImageScaleEnvironment,
    image_slice: Option<DisplayImageSliceSpec>,
) -> Option<DisplayMediaReplacement> {
    let face_metrics = display_media_face_metrics(resolved_face, fallback_metrics);
    let font_size = if resolved_face.font_size.is_finite() && resolved_face.font_size > 0.0 {
        resolved_face.font_size
    } else {
        face_metrics.row_height()
    };
    let media = resolve_display_media_property(
        display_prop,
        DisplayMediaResolveParams {
            display_host: display_host?,
            default_fg: resolved_face.fg,
            default_bg: resolved_face.bg,
            fallback_metrics: face_metrics,
            image_scale_environment,
            image_dimension_environment: DisplayImageDimensionEnvironment::new(
                font_size,
                face_metrics.row_height(),
                face_metrics.char_width(),
            ),
            image_slice,
        },
    )?;
    let box_expansion = if resolved_face.box_type != 0 {
        resolved_face
            .box_line_width
            .logical_geometry(image_scale_environment.device_scale())
            .row_expansion_per_edge()
            .get()
    } else {
        0.0
    };
    Some(media.with_positive_box_line_width(box_expansion))
}

fn display_media_face_metrics(
    resolved_face: &ResolvedFace,
    fallback: DisplayRowFallbackMetrics,
) -> DisplayRowFallbackMetrics {
    let char_width = if resolved_face.measured_char_width_px().is_finite()
        && resolved_face.measured_char_width_px() > 0.0
    {
        resolved_face.measured_char_width_px()
    } else {
        fallback.char_width()
    };
    let row_height =
        if resolved_face.font_line_height.is_finite() && resolved_face.font_line_height > 0.0 {
            resolved_face.font_line_height
        } else {
            fallback.row_height()
        };
    let ascent = if resolved_face.font_ascent.is_finite() && resolved_face.font_ascent > 0.0 {
        resolved_face.font_ascent
    } else {
        fallback.ascent()
    }
    .min(row_height);
    DisplayRowFallbackMetrics::from_default_face_extents(char_width, row_height, ascent)
}

pub(crate) fn same_resolved_face(lhs: &ResolvedFace, rhs: &ResolvedFace) -> bool {
    lhs.fg == rhs.fg
        && lhs.bg == rhs.bg
        && lhs.font_family == rhs.font_family
        && lhs.font_weight == rhs.font_weight
        && lhs.italic == rhs.italic
        && (lhs.font_size - rhs.font_size).abs() <= f32::EPSILON
        && lhs.underline_style == rhs.underline_style
        && lhs.underline_color == rhs.underline_color
        && lhs.strike_through == rhs.strike_through
        && lhs.strike_through_color == rhs.strike_through_color
        && lhs.overline == rhs.overline
        && lhs.overline_color == rhs.overline_color
        && lhs.box_type == rhs.box_type
        && lhs.box_color == rhs.box_color
        && lhs.box_line_width == rhs.box_line_width
        && lhs.extend == rhs.extend
        && lhs.terminal_inverse_video == rhs.terminal_inverse_video
        // A face that differs from the base ONLY in its realized `:stipple`
        // bitmap (e.g. `indent-bars` faces, which inherit the default colors and
        // add just a stipple) must NOT be collapsed onto the base id — doing so
        // dropped the stipple, so indentation whitespace resolved to the plain
        // default face and no bars were drawn (issue #174).
        && lhs.stipple == rhs.stipple
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::neovm_bridge::LayoutBufferSnapshot;
    use neovm_core::buffer::CharPos0;
    use neovm_core::emacs_core::Context;
    use neovm_core::face::{Color as NeoColor, Face as NeoFace, FaceTable};

    fn test_buffer_snapshot() -> LayoutBufferSnapshot {
        let mut context = Context::new();
        let buf_id = context
            .buffer_manager()
            .current_buffer()
            .expect("current buffer")
            .id();
        {
            let buffer = context
                .buffer_manager_mut()
                .get_mut(buf_id)
                .expect("current buffer");
            buffer.insert("abc");
            buffer.widen();
        }
        let buffer = context
            .buffer_manager()
            .get(buf_id)
            .expect("current buffer");
        LayoutBufferSnapshot::from_buffer(buffer)
    }

    fn test_face_resolver(table: &FaceTable) -> FaceResolver {
        FaceResolver::new(table, 0x00ffffff, 0x000000, 14.0, None)
    }

    fn face_id(face: RenderFaceRef) -> FaceId {
        match face {
            RenderFaceRef::FaceId(face_id) => face_id,
            RenderFaceRef::Inherit => panic!("expected concrete face id"),
        }
    }

    fn dashboard_like_face_table() -> FaceTable {
        let mut table = FaceTable::new();

        let mut blue_title = NeoFace::new("dashboard-title-blue");
        blue_title.foreground = Some(NeoColor::rgb(0x51, 0xaf, 0xef));
        table.define("dashboard-title-blue", blue_title);

        let mut purple_title = NeoFace::new("dashboard-title-purple");
        purple_title.foreground = Some(NeoColor::rgb(0xa9, 0xa1, 0xe1));
        table.define("dashboard-title-purple", purple_title);

        let mut hl_line = NeoFace::new("dashboard-hl-line");
        hl_line.background = Some(NeoColor::rgb(0x21, 0x24, 0x2b));
        hl_line.extend = Some(true);
        table.define("dashboard-hl-line", hl_line);

        table
    }

    #[test]
    fn source_face_resolver_merges_overlay_face_over_current_base_face() {
        let table = dashboard_like_face_table();
        let face_resolver = test_face_resolver(&table);
        let base_face = face_resolver.default_face();
        let mut resolve_state = DisplaySourceResolveState::default();
        let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
        let mut pending_faces = Vec::new();
        let params = DisplaySourceResolveParams::new(
            DisplaySourceFaceBasis::new(
                &face_resolver,
                FaceId::new(0),
                base_face,
                DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
            ),
            None,
            ImageScaleEnvironment::default(),
        );

        let highlighted_id = {
            let mut resolver = DisplaySourcePropertyResolver::frame_local(
                params,
                &mut resolve_state,
                &mut face_ids,
                &mut pending_faces,
            );
            let title = DisplayItemFaceResolver::resolve_face_ref(
                &mut resolver,
                RenderFaceRef::Inherit,
                Value::symbol("dashboard-title-blue"),
            );
            let highlighted = DisplayItemFaceResolver::resolve_face_ref(
                &mut resolver,
                title,
                Value::symbol("dashboard-hl-line"),
            );
            face_id(highlighted)
        };

        let highlighted = resolve_state
            .resolved_face(highlighted_id)
            .expect("highlighted face");
        assert_eq!(highlighted.fg, 0x0051afef);
        assert_eq!(highlighted.bg, 0x0021242b);
        assert!(highlighted.extend);
    }

    #[test]
    fn source_face_cache_is_keyed_by_base_face_id() {
        let table = dashboard_like_face_table();
        let face_resolver = test_face_resolver(&table);
        let base_face = face_resolver.default_face();
        let mut resolve_state = DisplaySourceResolveState::default();
        let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
        let mut pending_faces = Vec::new();
        let params = DisplaySourceResolveParams::new(
            DisplaySourceFaceBasis::new(
                &face_resolver,
                FaceId::new(0),
                base_face,
                DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
            ),
            None,
            ImageScaleEnvironment::default(),
        );

        let (blue_hl_id, purple_hl_id) = {
            let mut resolver = DisplaySourcePropertyResolver::frame_local(
                params,
                &mut resolve_state,
                &mut face_ids,
                &mut pending_faces,
            );
            let blue = DisplayItemFaceResolver::resolve_face_ref(
                &mut resolver,
                RenderFaceRef::Inherit,
                Value::symbol("dashboard-title-blue"),
            );
            let blue_hl = DisplayItemFaceResolver::resolve_face_ref(
                &mut resolver,
                blue,
                Value::symbol("dashboard-hl-line"),
            );
            let purple = DisplayItemFaceResolver::resolve_face_ref(
                &mut resolver,
                RenderFaceRef::Inherit,
                Value::symbol("dashboard-title-purple"),
            );
            let purple_hl = DisplayItemFaceResolver::resolve_face_ref(
                &mut resolver,
                purple,
                Value::symbol("dashboard-hl-line"),
            );
            (face_id(blue_hl), face_id(purple_hl))
        };

        assert_ne!(blue_hl_id, purple_hl_id);
        assert_eq!(
            resolve_state
                .resolved_face(blue_hl_id)
                .expect("blue highlight")
                .fg,
            0x0051afef
        );
        assert_eq!(
            resolve_state
                .resolved_face(purple_hl_id)
                .expect("purple highlight")
                .fg,
            0x00a9a1e1
        );
    }

    #[test]
    fn buffer_source_face_resolver_uses_buffer_face_remapping() {
        let mut context = Context::new();
        let buf_id = context
            .buffer_manager()
            .current_buffer()
            .expect("current buffer")
            .id();
        let remapping = Value::list(vec![Value::list(vec![
            Value::symbol("dashboard-hl-line"),
            Value::list(vec![
                Value::keyword("background"),
                Value::string("#282c34"),
                Value::keyword("extend"),
                Value::T,
            ]),
            Value::symbol("dashboard-hl-line"),
        ])]);
        {
            let buffer = context
                .buffer_manager_mut()
                .get_mut(buf_id)
                .expect("current buffer");
            buffer.insert("abc");
            buffer.widen();
            buffer.set_buffer_local("face-remapping-alist", remapping);
        }
        let table = dashboard_like_face_table();
        let face_resolver = test_face_resolver(&table);
        let buffer = context
            .buffer_manager()
            .get(buf_id)
            .expect("current buffer");
        let base_face = face_resolver.default_face();
        let mut resolve_state = DisplaySourceResolveState::default();
        let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
        let mut pending_faces = Vec::new();
        let params = DisplaySourceResolveParams::new(
            DisplaySourceFaceBasis::new(
                &face_resolver,
                FaceId::new(0),
                base_face,
                DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
            ),
            None,
            ImageScaleEnvironment::default(),
        );

        let highlighted_id = {
            let mut resolver = DisplaySourcePropertyResolver::buffer_local(
                buffer,
                params,
                &mut resolve_state,
                &mut face_ids,
                &mut pending_faces,
            );
            let title = DisplayItemFaceResolver::resolve_face_ref(
                &mut resolver,
                RenderFaceRef::Inherit,
                Value::symbol("dashboard-title-blue"),
            );
            let highlighted = DisplayItemFaceResolver::resolve_face_ref(
                &mut resolver,
                title,
                Value::symbol("dashboard-hl-line"),
            );
            face_id(highlighted)
        };

        let highlighted = resolve_state
            .resolved_face(highlighted_id)
            .expect("highlighted face");
        assert_eq!(highlighted.fg, 0x0051afef);
        assert_eq!(highlighted.bg, 0x00282c34);
        assert!(highlighted.extend);
    }

    #[test]
    fn named_face_background_equal_to_global_default_still_overrides_buffer_default() {
        let mut context = Context::new();
        let buf_id = context
            .buffer_manager()
            .current_buffer()
            .expect("current buffer")
            .id();
        let remapping = Value::list(vec![Value::list(vec![
            Value::symbol("default"),
            Value::list(vec![Value::keyword("background"), Value::string("#21242b")]),
            Value::symbol("default"),
        ])]);
        {
            let buffer = context
                .buffer_manager_mut()
                .get_mut(buf_id)
                .expect("current buffer");
            buffer.insert("abc");
            buffer.widen();
            buffer.set_buffer_local("face-remapping-alist", remapping);
        }

        let mut table = FaceTable::new();
        let mut default = NeoFace::new("default");
        default.background = Some(NeoColor::rgb(0x28, 0x2c, 0x34));
        table.define("default", default);
        let mut selected_line = NeoFace::new("selected-line");
        selected_line.background = Some(NeoColor::rgb(0x28, 0x2c, 0x34));
        selected_line.extend = Some(true);
        table.define("selected-line", selected_line);

        let face_resolver = test_face_resolver(&table);
        let buffer = context
            .buffer_manager()
            .get(buf_id)
            .expect("current buffer");
        let buffer_default = face_resolver.resolve_buffer_default_face(buffer);
        assert_eq!(buffer_default.bg, 0x0021242b);

        let highlighted = face_resolver
            .resolve_buffer_face_value_over(
                buffer,
                &buffer_default,
                &Value::symbol("selected-line"),
            )
            .expect("selected face should resolve");

        assert_eq!(highlighted.bg, 0x00282c34);
        assert!(highlighted.extend);
    }

    #[test]
    fn display_string_base_face_reuses_active_face_before_prefix_policy() {
        let buffer = test_buffer_snapshot();
        let table = FaceTable::new();
        let resolver = test_face_resolver(&table);
        let mut face_ids = FrameFaceAttempt::for_test_with_next_id(BasicFaceId::SENTINEL);

        let base_face = resolve_display_string_base_face(
            &buffer,
            &resolver,
            DisplayOrigin::LinePrefix {
                anchor_charpos: CharPos0::new(0),
            },
            BaseFacePolicy::BufferRemappedBasicFace(BasicFaceId::Default),
            Some(ActiveDisplayStringBaseFace::new(
                FaceId::new(500),
                resolver.default_face(),
            )),
            DisplayDefaultFaceInstallPolicy::InstallDefaultFace,
            &mut face_ids,
        );

        assert_eq!(base_face.face_id(), FaceId::new(500));
        assert!(base_face.pending_face().is_none());
        assert!(same_resolved_face(
            base_face.face(),
            resolver.default_face()
        ));
    }

    #[test]
    fn display_string_unremapped_default_controls_pending_face() {
        let buffer = test_buffer_snapshot();
        let table = FaceTable::new();
        let resolver = test_face_resolver(&table);
        let mut install_face_ids = FrameFaceAttempt::for_test_with_next_id(BasicFaceId::SENTINEL);
        let mut reuse_face_ids = FrameFaceAttempt::for_test_with_next_id(BasicFaceId::SENTINEL);

        let installed = resolve_display_string_base_face(
            &buffer,
            &resolver,
            DisplayOrigin::LinePrefix {
                anchor_charpos: CharPos0::new(0),
            },
            BaseFacePolicy::BufferRemappedBasicFace(BasicFaceId::Default),
            None,
            DisplayDefaultFaceInstallPolicy::InstallDefaultFace,
            &mut install_face_ids,
        );
        let reused = resolve_display_string_base_face(
            &buffer,
            &resolver,
            DisplayOrigin::LinePrefix {
                anchor_charpos: CharPos0::new(0),
            },
            BaseFacePolicy::BufferRemappedBasicFace(BasicFaceId::Default),
            None,
            DisplayDefaultFaceInstallPolicy::ReuseInstalledDefaultFace,
            &mut reuse_face_ids,
        );

        assert_eq!(installed.face_id(), FaceId::from(BasicFaceId::Default));
        assert!(installed.pending_face().is_some());
        assert_eq!(reused.face_id(), FaceId::from(BasicFaceId::Default));
        assert!(reused.pending_face().is_none());
    }

    #[test]
    fn display_string_base_face_allocates_pending_face_for_dynamic_source_face() {
        let buffer = test_buffer_snapshot();
        let table = FaceTable::new();
        let resolver = test_face_resolver(&table);
        let mut face_ids = FrameFaceAttempt::for_test_with_next_id(500);

        let base_face = resolve_display_string_base_face(
            &buffer,
            &resolver,
            DisplayOrigin::ModeLine { selected: true },
            BaseFacePolicy::BufferRemappedBasicFace(BasicFaceId::ModeLineActive),
            None,
            DisplayDefaultFaceInstallPolicy::ReuseInstalledDefaultFace,
            &mut face_ids,
        );

        assert_eq!(base_face.face_id(), FaceId::new(500));
        let pending_face = base_face.pending_face().expect("pending face");
        assert_eq!(pending_face.face_id(), FaceId::new(500));
        assert!(same_resolved_face(
            pending_face.resolved(),
            base_face.face()
        ));
        assert_eq!(face_ids.next_face_id_for_test(), 501);
    }

    #[test]
    fn resolve_display_replacement_returns_direct_xwidget_media() {
        let table = FaceTable::new();
        let resolver = test_face_resolver(&table);
        let xwidget = DisplayXwidgetItem {
            xwidget_id: neomacs_display_protocol::XwidgetId::new(42),
            webview_id: neomacs_display_protocol::WebViewId::new(420),
            width: 120.0,
            height: 36.0,
        };
        let media = DisplayMediaReplacement::xwidget(xwidget);

        let resolved = resolve_display_replacement(
            Value::NIL,
            &DisplayMediaReplacementProperty::Xwidget(media),
            None,
            resolver.default_face(),
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
            ImageScaleEnvironment::default(),
            None,
        );

        assert_eq!(resolved, Some(ResolvedDisplayReplacement::Media(media)));
    }

    #[test]
    fn resolve_display_replacement_uses_media_placeholder_without_host() {
        let table = FaceTable::new();
        let resolver = test_face_resolver(&table);

        let resolved = resolve_display_replacement(
            Value::NIL,
            &DisplayMediaReplacementProperty::Image,
            None,
            resolver.default_face(),
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
            ImageScaleEnvironment::default(),
            None,
        );

        assert_eq!(
            resolved,
            Some(ResolvedDisplayReplacement::Placeholder("[img]"))
        );
    }

    #[test]
    fn display_media_face_metrics_prefer_active_face_extents() {
        let table = FaceTable::new();
        let resolver = test_face_resolver(&table);
        let mut active_face = resolver.default_face().clone();
        active_face.set_measured_char_width_px(11.0);
        active_face.font_line_height = 24.0;
        active_face.font_ascent = 20.0;
        let fallback = DisplayRowFallbackMetrics::from_default_face_extents(8.0, 18.0, 14.0);

        let metrics = display_media_face_metrics(&active_face, fallback);

        assert_eq!(metrics.row_height(), 24.0);
        assert_eq!(metrics.ascent(), 20.0);
        assert_eq!(metrics.char_width(), 11.0);
    }
}

//! Buffer source face and source-item layout resolution.
//!
//! This module resolves buffer source faces at scan checkpoints and prepares
//! display source items whose layout changes require derived measured faces.

use crate::display_face_layout::{DisplayHeightFaceBasis, height_adjusted_face};
use crate::display_face_policy::BaseFacePolicy;
use crate::display_face_ref::render_face_ref_id;
use crate::display_item::{DisplayItem, DisplayItemKind, RenderFaceRef};
use crate::display_origin::DisplayOrigin;
use crate::display_row::face_state::{
    DisplayRowActiveFaceState, DisplayRowMeasurementPolicy, stable_face_id_for_resolved,
};
use crate::display_row::geometry::{DisplayRowExtendState, DisplayRowGeometryState};
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_row::source_render::TextRowSourceRenderState;
use crate::display_row::walk_state::{BoxFaceRowState, FaceScanCheckpoint};
use crate::display_source::{is_escape_glyph_octal, nonascii_hyphen_p, nonascii_space_p};
use crate::display_source_resolver::{
    ActiveDisplayStringBaseFace, DisplayDefaultFaceInstallPolicy, DisplaySourceFaceBasis,
    DisplaySourceResolveParams, DisplayStringBaseFace, PendingDisplaySourceFace,
    resolve_display_string_base_face,
};
use crate::frame_face_arena::FrameFaceAttempt;
use crate::neovm_bridge::{FaceResolver, LayoutBufferView, ResolvedFace};
use neomacs_display_protocol::types::FaceId;
use neovm_core::emacs_core::eval::DisplayHost;
use neovm_core::emacs_core::image_catalog::ImageScaleEnvironment;

pub(crate) struct BufferSourceFaceResolutionContext<'a, B: LayoutBufferView> {
    buffer: &'a B,
    face_resolver: &'a FaceResolver,
    measurement_policy: DisplayRowMeasurementPolicy,
    default_resolved: &'a ResolvedFace,
    default_face_id: FaceId,
    default_face_metrics: DisplayRowFallbackMetrics,
    window_metrics: DisplayRowFallbackMetrics,
    image_scale_environment: ImageScaleEnvironment,
}

#[derive(Clone, Copy)]
pub(crate) struct BufferSourceItemLayoutResolutionContext<'a> {
    measurement_policy: DisplayRowMeasurementPolicy,
    default_resolved: &'a ResolvedFace,
    default_face_metrics: DisplayRowFallbackMetrics,
    window_metrics: DisplayRowFallbackMetrics,
}

impl<'a> BufferSourceItemLayoutResolutionContext<'a> {
    pub(crate) fn new(
        measurement_policy: DisplayRowMeasurementPolicy,
        default_resolved: &'a ResolvedFace,
        default_face_metrics: DisplayRowFallbackMetrics,
        window_metrics: DisplayRowFallbackMetrics,
    ) -> Self {
        Self {
            measurement_policy,
            default_resolved,
            default_face_metrics,
            window_metrics,
        }
    }
}

impl<'a, B: LayoutBufferView> Clone for BufferSourceFaceResolutionContext<'a, B> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, B: LayoutBufferView> Copy for BufferSourceFaceResolutionContext<'a, B> {}

impl<'a, B: LayoutBufferView> BufferSourceFaceResolutionContext<'a, B> {
    pub(crate) fn new(
        buffer: &'a B,
        face_resolver: &'a FaceResolver,
        measurement_policy: DisplayRowMeasurementPolicy,
        default_resolved: &'a ResolvedFace,
        default_face_id: FaceId,
        default_face_metrics: DisplayRowFallbackMetrics,
        window_metrics: DisplayRowFallbackMetrics,
        image_scale_environment: ImageScaleEnvironment,
    ) -> Self {
        Self {
            buffer,
            face_resolver,
            measurement_policy,
            default_resolved,
            default_face_id,
            default_face_metrics,
            window_metrics,
            image_scale_environment,
        }
    }

    pub(crate) fn buffer(&self) -> &'a B {
        self.buffer
    }

    pub(crate) fn resolve_routed_position_face(
        &self,
        face_ids: &mut FrameFaceAttempt,
        pos: neovm_core::buffer::CharPos0,
    ) -> (FaceId, ResolvedFace) {
        crate::buffer_source::row_route::resolve_routed_position_face(
            self.buffer,
            self.face_resolver,
            face_ids,
            pos,
        )
    }

    pub(crate) fn routed_segment_item_face_diverges(
        &self,
        face_ids: &mut FrameFaceAttempt,
        pos: neovm_core::buffer::CharPos0,
        expected_face_id: FaceId,
    ) -> bool {
        crate::buffer_source::row_route::routed_segment_item_face_diverges(
            self.buffer,
            self.face_resolver,
            face_ids,
            self.default_resolved,
            self.default_face_id,
            pos,
            expected_face_id,
        )
    }

    pub(crate) fn resolve_display_string_base_face(
        &self,
        origin: DisplayOrigin,
        policy: BaseFacePolicy,
        active_base_face: Option<ActiveDisplayStringBaseFace<'_>>,
        default_install_policy: DisplayDefaultFaceInstallPolicy,
        face_ids: &mut FrameFaceAttempt,
    ) -> DisplayStringBaseFace {
        resolve_display_string_base_face(
            self.buffer,
            self.face_resolver,
            origin,
            policy,
            active_base_face,
            default_install_policy,
            face_ids,
        )
    }

    /// Measured-face PROBE for the routed row acquisition: the active-face
    /// state `resolve_at_checkpoint` would install for `face_id`/`resolved`,
    /// WITHOUT installing it or touching row geometry — the route decision
    /// must measure candidate segments before mutating any loop state.
    pub(crate) fn probe_measured_active_face(
        &self,
        source_render: &mut TextRowSourceRenderState<'_>,
        face_id: FaceId,
        resolved: ResolvedFace,
    ) -> DisplayRowActiveFaceState {
        source_render.resolve_measured_face_without_install(
            self.measurement_policy,
            face_id,
            resolved,
            self.window_metrics.char_width(),
            self.window_metrics,
        )
    }

    pub(crate) fn resolve_at_checkpoint(
        &self,
        state: &mut BufferSourceFaceResolutionState<'_, '_>,
        charpos: i64,
    ) -> bool {
        if !state.face_scan.should_resolve_at(charpos as usize) {
            return false;
        }

        let origin = DisplayOrigin::BufferText {
            charpos: neovm_core::buffer::CharPos0::new(charpos as usize),
        };
        let resolved = self.face_resolver.default_base_face_for_origin(
            Some(self.buffer),
            &origin,
            state.face_scan.next_check_mut(),
        );
        let face_id = stable_face_id_for_resolved(state.face_ids, &resolved);
        let resolved_box_type = resolved.box_type;
        *state.active_face_state = state.source_render.resolve_and_install_measured_face(
            self.measurement_policy,
            face_id,
            resolved,
            self.window_metrics.char_width(),
            self.window_metrics,
        );
        let face_metrics = state.active_face_state.metrics();
        state
            .row_geometry
            .include_row_extents(face_metrics.row_height(), face_metrics.ascent());

        // The `:extend` line-fill face must track the face resolved AT this
        // position, exactly like GNU `extend_face_to_end_of_line`'s
        // `face_at_pos(it, LFACE_EXTEND_INDEX)` — activate when the resolved
        // face extends, clear otherwise. Clearing here (rather than only when a
        // later non-extend source item renders) is what fixes a blank line that
        // immediately follows an `:extend` line (e.g. hl-line / region): the
        // blank line resolves to the non-extending default face at its own bol,
        // so it must NOT inherit the previous line's fill (issue #185). A face
        // that covers the newline (hl-line's `[bol, next-bol)` overlay) stays
        // extending across the checkpoint and still fills its own row.
        if let Some(fill) = state.active_face_state.row_extend_fill() {
            state
                .row_extend
                .activate(state.row_geometry.current_row_marker(), fill);
        } else {
            state.row_extend.clear();
        }

        if state.box_face.is_active() && resolved_box_type == 0 {
            state.box_face.clear();
        }
        if resolved_box_type > 0 {
            state
                .box_face
                .activate(state.row_geometry.current_row_marker(), state.x);
        }
        true
    }

    pub(crate) fn source_item_layout_resolution_context(
        self,
    ) -> BufferSourceItemLayoutResolutionContext<'a> {
        BufferSourceItemLayoutResolutionContext::new(
            self.measurement_policy,
            self.default_resolved,
            self.default_face_metrics,
            self.window_metrics,
        )
    }

    pub(crate) fn source_resolve_params(
        self,
        display_host: Option<&'a dyn DisplayHost>,
    ) -> DisplaySourceResolveParams<'a> {
        DisplaySourceResolveParams::new(
            DisplaySourceFaceBasis::new(
                self.face_resolver,
                self.default_face_id,
                self.default_resolved,
                self.default_face_metrics,
            ),
            display_host,
            self.image_scale_environment,
        )
    }

    pub(crate) fn install_pending_source_faces(
        self,
        source_render: &mut TextRowSourceRenderState<'_>,
        row_geometry: &mut DisplayRowGeometryState,
        pending_faces: Vec<PendingDisplaySourceFace>,
    ) {
        for pending in pending_faces {
            let (face_id, resolved) = pending.into_parts();
            let active_face = source_render.resolve_and_install_measured_face(
                self.measurement_policy,
                face_id,
                resolved,
                self.window_metrics.char_width(),
                self.window_metrics,
            );
            let metrics = active_face.metrics();
            row_geometry.include_row_extents(metrics.row_height(), metrics.ascent());
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resolve_at_checkpoint_with_source_state(
        &self,
        source_render: &mut TextRowSourceRenderState<'_>,
        face_scan: &mut FaceScanCheckpoint,
        face_ids: &mut FrameFaceAttempt,
        active_face_state: &mut DisplayRowActiveFaceState,
        row_geometry: &mut DisplayRowGeometryState,
        row_extend: &mut DisplayRowExtendState,
        box_face: &mut BoxFaceRowState,
        x: f32,
        charpos: i64,
    ) -> bool {
        self.resolve_at_checkpoint(
            &mut BufferSourceFaceResolutionState::new(
                source_render,
                face_scan,
                face_ids,
                active_face_state,
                row_geometry,
                row_extend,
                box_face,
                x,
            ),
            charpos,
        )
    }
}

/// The original buffer source char of a display item plus the active
/// `nobreak-char-display` policy, threaded into face resolution so the nbsp /
/// nobreak-hyphen highlight branch can be keyed on the unsubstituted char.
///
/// GNU `get_next_display_element` reads `Vnobreak_char_display` with the
/// window's buffer current and classifies the raw `it->c`; we mirror that with
/// the original `source_step_char` (not the post-substitution `SourceMappedText`
/// item), so dpvec replacement-string `SourceMappedText` items -- which never
/// carry an nbsp/hyphen source char -- are untouched.
#[derive(Clone, Copy)]
pub(crate) struct DisplaySourceNobreakHint {
    source_char: char,
    nobreak_char_display: i32,
}

impl DisplaySourceNobreakHint {
    pub(crate) fn new(source_char: char, nobreak_char_display: i32) -> Self {
        Self {
            source_char,
            nobreak_char_display,
        }
    }

    /// The merge face name for a precluster substitute, or `None` when this char
    /// is not highlighted. In highlight mode (`nobreak-char-display` == t == 1) a
    /// non-ASCII space/hyphen paints in `nobreak-space`/`nobreak-hyphen`.
    /// Independent of that policy, a non-printable char (its `\`+octal escape,
    /// see [`is_escape_glyph_octal`]) paints in `escape-glyph` -- GNU always
    /// merges the escape glyph for these (xdisp.c:8631-8633).
    fn highlight_face_name(self) -> Option<&'static str> {
        if self.nobreak_char_display == 1 {
            if nonascii_space_p(self.source_char) {
                return Some("nobreak-space");
            }
            if nonascii_hyphen_p(self.source_char) {
                return Some("nobreak-hyphen");
            }
        }
        if is_escape_glyph_octal(self.source_char) {
            return Some("escape-glyph");
        }
        None
    }
}

impl BufferSourceItemLayoutResolutionContext<'_> {
    pub(crate) fn resolve_source_item_layout_for_active_face(
        &self,
        source_render: &mut TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        row_geometry: &mut DisplayRowGeometryState,
        active_face_state: &DisplayRowActiveFaceState,
        item: &mut DisplayItem,
        nobreak_hint: DisplaySourceNobreakHint,
    ) -> DisplayRowActiveFaceState {
        item.face =
            RenderFaceRef::FaceId(render_face_ref_id(item.face, active_face_state.face_id()));

        let active_face_state = active_face_state.clone();

        // GNU `merge_escape_glyph_face` (xdisp.c:8372-8389): a control char shown
        // as `^A` paints in the `escape-glyph` face merged over the surrounding
        // base face. Realize that merged face here, install it, and use it as the
        // ACTIVE face for the append so `push_control_char` writes both `^` and
        // the caret letter with the one merged face id (GNU's `dpvec_face_id`),
        // and the row's face table picks up the realized face under that id.
        if matches!(item.kind, DisplayItemKind::ControlChar { .. })
            && let Some(merged) = self.merge_named_active_face(
                source_render,
                face_ids,
                row_geometry,
                &active_face_state,
                item,
                "escape-glyph",
            )
        {
            return merged;
        }

        // GNU `get_next_display_element` (xdisp.c:8594-8617): in highlight mode
        // (`nobreak-char-display` == t) a non-ASCII space / nobreak hyphen is
        // shown via its ASCII substitute painted in the `nobreak-space` /
        // `nobreak-hyphen` face MERGED over the surrounding base face. Unlike a
        // control char, nbsp/hyphen reach here as a plain `TextRun` item (they
        // classify as `Text`), so the branch is keyed on the ORIGINAL source char
        // (threaded via `nobreak_hint`), NOT `item.kind` -- this keeps display
        // table (dpvec) `SourceMappedText` substitutions and normal text
        // untouched. The substitute glyph itself is produced later on the
        // precluster Special path; merging the active face here makes that
        // append pick up the merged face id (mirrors the escape-glyph hook).
        if let Some(face_name) = nobreak_hint.highlight_face_name()
            && let Some(merged) = self.merge_named_active_face(
                source_render,
                face_ids,
                row_geometry,
                &active_face_state,
                item,
                face_name,
            )
        {
            return merged;
        }

        let Some(factor) = item
            .layout
            .height
            .filter(|factor| factor.is_finite() && *factor > 0.0)
        else {
            return active_face_state.clone();
        };

        item.layout.height = None;
        let Some(resolved) = height_adjusted_face(
            active_face_state.resolved_face(),
            DisplayHeightFaceBasis {
                canonical_face: self.default_resolved,
                base_face: self.default_resolved,
                fallback_metrics: self.default_face_metrics,
            },
            factor,
        ) else {
            return active_face_state.clone();
        };

        let face_id = stable_face_id_for_resolved(face_ids, &resolved);
        item.face = RenderFaceRef::FaceId(face_id);
        let resolved_active_face = source_render.resolve_and_install_measured_face(
            self.measurement_policy,
            face_id,
            resolved,
            self.window_metrics.char_width(),
            self.window_metrics,
        );
        let metrics = resolved_active_face.metrics();
        row_geometry.include_row_extents(metrics.row_height(), metrics.ascent());
        resolved_active_face
    }

    /// Realize the legacy escape/nobreak face merge. Its foreground-only
    /// no-op rule is intentional and keeps those paths unchanged.
    fn merge_named_active_face(
        &self,
        source_render: &mut TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        row_geometry: &mut DisplayRowGeometryState,
        active_face_state: &DisplayRowActiveFaceState,
        item: &mut DisplayItem,
        face_name: &str,
    ) -> Option<DisplayRowActiveFaceState> {
        let base = active_face_state.resolved_face();
        let merged = source_render.merge_named_face_over(base, face_name);
        if merged.fg == base.fg && merged.use_default_foreground == base.use_default_foreground {
            return None;
        }
        Some(self.install_merged_active_face(source_render, face_ids, row_geometry, item, merged))
    }

    fn install_merged_active_face(
        &self,
        source_render: &mut TextRowSourceRenderState<'_>,
        face_ids: &mut FrameFaceAttempt,
        row_geometry: &mut DisplayRowGeometryState,
        item: &mut DisplayItem,
        merged: ResolvedFace,
    ) -> DisplayRowActiveFaceState {
        let face_id = stable_face_id_for_resolved(face_ids, &merged);
        item.face = RenderFaceRef::FaceId(face_id);
        let resolved_active_face = source_render.resolve_and_install_measured_face(
            self.measurement_policy,
            face_id,
            merged,
            self.window_metrics.char_width(),
            self.window_metrics,
        );
        let metrics = resolved_active_face.metrics();
        row_geometry.include_row_extents(metrics.row_height(), metrics.ascent());
        resolved_active_face
    }
}

pub(crate) struct BufferSourceFaceResolutionState<'a, 'source> {
    source_render: &'a mut TextRowSourceRenderState<'source>,
    face_scan: &'a mut FaceScanCheckpoint,
    face_ids: &'a mut FrameFaceAttempt,
    active_face_state: &'a mut DisplayRowActiveFaceState,
    row_geometry: &'a mut DisplayRowGeometryState,
    row_extend: &'a mut DisplayRowExtendState,
    box_face: &'a mut BoxFaceRowState,
    x: f32,
}

impl<'a, 'source> BufferSourceFaceResolutionState<'a, 'source> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        source_render: &'a mut TextRowSourceRenderState<'source>,
        face_scan: &'a mut FaceScanCheckpoint,
        face_ids: &'a mut FrameFaceAttempt,
        active_face_state: &'a mut DisplayRowActiveFaceState,
        row_geometry: &'a mut DisplayRowGeometryState,
        row_extend: &'a mut DisplayRowExtendState,
        box_face: &'a mut BoxFaceRowState,
        x: f32,
    ) -> Self {
        Self {
            source_render,
            face_scan,
            face_ids,
            active_face_state,
            row_geometry,
            row_extend,
            box_face,
            x,
        }
    }
}

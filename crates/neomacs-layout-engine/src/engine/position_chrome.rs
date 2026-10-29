//! Query-local chrome measurement, before any Position body reuse.
//!
//! The numeric witness belongs to one engine/Context query. It contains no Lisp
//! values or shared mutable cache; every consumer validates its original owner
//! and complete post-callback input token before using its partition.

use super::*;
use crate::display_frame_output::WindowFrameGeometry;
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_status_line::{
    ChromeRowRenderServices, WindowChromeRowsPlan, WindowChromeRowsRenderOutcome,
    WindowChromeRowsRenderState,
};
use crate::output::builder::DisplayOutputBuilder;
use crate::output::window_request::OutputWindowRowCapacity;
use crate::window_output::{
    TextWindowOutputBegin, TextWindowOutputTarget, WindowOutputEmitter, begin_text_window_output,
};
use neovm_core::buffer::LispCharPos1;

#[derive(Debug)]
pub(super) enum MeasurementFailure {
    SourceChanged,
    DeferredExit,
}

pub(super) struct MeasuredPositionChrome {
    frame: neovm_core::window::FrameId,
    window: neovm_core::window::WindowId,
    buffer: neovm_core::buffer::BufferId,
    after: neovm_core::window::WindowLayoutAttemptFreshness,
    metrics: WindowChromeMetrics,
}

impl MeasuredPositionChrome {
    pub(super) fn measure(
        evaluator: &mut neovm_core::emacs_core::Context,
        frame: neovm_core::window::FrameId,
        params: &WindowParams,
        layout_box: WindowLayoutBox,
        resolver: &super::super::neovm_bridge::FaceResolver,
        font_metrics: &mut Option<crate::font::metrics::FontMetricsService>,
        geometry: WindowFrameGeometry,
    ) -> Result<Self, MeasurementFailure> {
        let window = neovm_core::window::WindowId(params.window_id as u64);
        let buffer = neovm_core::buffer::BufferId(params.buffer_id);
        let before = evaluator
            .window_layout_attempt_freshness(frame, window, buffer)
            .ok_or(MeasurementFailure::SourceChanged)?;
        resolver.set_current_window_parameters(
            evaluator.frame_manager().window_parameters_pairs(window),
        );
        resolver.set_current_window_id(Some(window.0));
        let (plan, name) = {
            let source = evaluator
                .buffer_manager()
                .get(buffer)
                .ok_or(MeasurementFailure::SourceChanged)?;
            (
                WindowChromeRowsPlan::new(params, source, resolver),
                source.name_runtime_string_owned(),
            )
        };
        let mut builder = DisplayOutputBuilder::new();
        let mut face_ids = FrameFaceArena::default().begin_attempt();
        builder.set_face_attempt(face_ids.clone());
        let mut output = TextWindowOutputTarget::from_builder(&mut builder);
        begin_text_window_output(
            output.reborrow(),
            TextWindowOutputBegin {
                window_id: window.0,
                rows: 3,
                cols: (params.bounds.width / params.char_width.max(1.0))
                    .ceil()
                    .max(1.0) as usize,
                bounds: params.bounds,
                text_bounds: params.text_bounds,
                text_clip_bounds: layout_box.body(),
                selected: params.selected,
                row_capacity: OutputWindowRowCapacity::Growing,
            },
        );
        let mut emitter = WindowOutputEmitter::new_speculative(
            frame,
            window,
            0,
            params.text_bounds.x,
            params.bounds.y,
        );
        let request = plan.render_request(
            params,
            layout_box,
            2,
            geometry.reserve_terminal_right_border_col,
            DisplayRowFallbackMetrics::from_window_defaults(params),
            &name,
        );
        let services = ChromeRowRenderServices::new(font_metrics, resolver, &mut face_ids);
        let outcome = request.measure_position(&mut WindowChromeRowsRenderState::new(
            output,
            &mut emitter,
            evaluator,
            services,
            None,
        ));
        if evaluator.has_mode_line_display_flow() {
            return Err(MeasurementFailure::DeferredExit);
        }
        let (metrics, admission) = match outcome {
            WindowChromeRowsRenderOutcome::Rendered {
                metrics,
                lisp_admission,
            } => (metrics, lisp_admission),
            WindowChromeRowsRenderOutcome::SourceInvalidated => {
                return Err(MeasurementFailure::SourceChanged);
            }
        };
        let after = crate::attempt_freshness::after_span(
            evaluator,
            &before,
            admission,
            crate::attempt_freshness::mode(),
        )
        .ok_or(MeasurementFailure::SourceChanged)?;
        // Unlike late redisplay chrome, GNU measures BEFORE the body here.
        // Lisp may legitimately change body inputs; the coordinator recollects
        // them after this call. This original AFTER token gates cache admission
        // only; physical body retries never evaluate the chrome again.
        Ok(Self {
            frame,
            window,
            buffer,
            after: after.into_owned(),
            metrics,
        })
    }

    pub(super) fn validate_owner(
        &self,
        frame: neovm_core::window::FrameId,
        params: &WindowParams,
    ) -> Result<(), MeasurementFailure> {
        if self.frame == frame
            && self.window.0 == params.window_id as u64
            && self.buffer.0 == params.buffer_id
        {
            Ok(())
        } else {
            Err(MeasurementFailure::SourceChanged)
        }
    }

    pub(super) fn partition<'ctx>(
        &self,
        evaluator: &'ctx neovm_core::emacs_core::Context,
        params: &WindowParams,
        layout: WindowLayoutBox,
    ) -> Option<PositionQueryPartition<'ctx>> {
        (self.validate_owner(self.frame, params).is_ok()
            && evaluator
                .window_layout_attempt_freshness(self.frame, self.window, self.buffer)
                .as_ref()
                == Some(&self.after)
            && layout.chrome() == self.metrics)
            .then_some(PositionQueryPartition {
                evaluator,
                frame: self.frame,
                window: self.window,
                partition: PositionBodyPartition::measured(layout),
            })
    }

    pub(super) fn display_window(&self) -> DisplayWindowId {
        DisplayWindowId::new(self.window.0 as i64)
    }

    pub(super) fn metrics(&self) -> WindowChromeMetrics {
        self.metrics
    }
}

/// The measured physical key is separate from the existing complete source,
/// point, collection-read and placement proofs in QueryCache.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PositionBodyPartition {
    chrome: WindowChromeMetrics,
    regions: neovm_core::window::PresentedWindowRegions,
}

impl PositionBodyPartition {
    fn measured(layout: WindowLayoutBox) -> Self {
        Self {
            chrome: layout.chrome(),
            regions: layout.regions(),
        }
    }

    pub(super) fn observed(snapshot: &neovm_core::window::WindowDisplaySnapshot) -> Self {
        Self {
            chrome: WindowChromeMetrics::from_snapshot(snapshot),
            regions: snapshot.regions,
        }
    }
}

/// Native-only admission after the measurement's ORIGINAL owner was checked.
/// The immutable Context borrow prevents a Lisp callback between validation
/// and the cache's exact source/read-journal checks.
#[must_use]
pub(super) struct PositionQueryPartition<'ctx> {
    evaluator: &'ctx neovm_core::emacs_core::Context,
    frame: neovm_core::window::FrameId,
    window: neovm_core::window::WindowId,
    partition: PositionBodyPartition,
}

impl<'ctx> PositionQueryPartition<'ctx> {
    pub(super) fn into_parts(
        self,
    ) -> (
        &'ctx neovm_core::emacs_core::Context,
        neovm_core::window::FrameId,
        neovm_core::window::WindowId,
        PositionBodyPartition,
    ) {
        (self.evaluator, self.frame, self.window, self.partition)
    }
}

static_assertions::assert_not_impl_any!(PositionQueryPartition<'static>: Send, Sync);

/// The same typed request owns normalization and cache/body consumption.
#[derive(Clone, Copy)]
pub(super) struct Request {
    frame: neovm_core::window::FrameId,
    window: neovm_core::window::WindowId,
    target: LispCharPos1,
}

impl Request {
    pub(super) fn new(
        frame: neovm_core::window::FrameId,
        window: neovm_core::window::WindowId,
        target: LispCharPos1,
    ) -> Self {
        Self {
            frame,
            window,
            target,
        }
    }
}

/// One logical Position query. The normalized owner is moved into the read
/// stage; physical body retries borrow it and cannot evaluate chrome again.
pub(super) struct QueryRun<'run> {
    engine: &'run mut LayoutEngine,
    evaluator: &'run mut neovm_core::emacs_core::Context,
    request: Request,
    prepared: QueryPreparation,
}

enum QueryPreparation {
    Pending,
    RecursiveFormatter,
    Measured {
        chrome: MeasuredPositionChrome,
        inputs: LiveWindowLayoutInputs,
        layout: WindowLayoutBox,
    },
    Refused(MeasurementFailure),
}

impl<'run> QueryRun<'run> {
    pub(super) fn new(
        engine: &'run mut LayoutEngine,
        evaluator: &'run mut neovm_core::emacs_core::Context,
        request: Request,
    ) -> Self {
        Self {
            engine,
            evaluator,
            request,
            prepared: QueryPreparation::Pending,
        }
    }

    pub(super) fn normalize(&mut self) {
        let Request {
            frame,
            window,
            target,
        } = self.request;
        self.evaluator.sync_runtime_faces_for_frame(frame);
        self.prepared = if neovm_core::emacs_core::xdisp::mode_line_display_active(self.evaluator) {
            QueryPreparation::RecursiveFormatter
        } else {
            match self.measure(frame, window, target) {
                Ok((chrome, inputs, layout)) => QueryPreparation::Measured {
                    chrome,
                    inputs,
                    layout,
                },
                Err(reason) => QueryPreparation::Refused(reason),
            }
        };
    }

    fn measure(
        &mut self,
        frame: neovm_core::window::FrameId,
        window: neovm_core::window::WindowId,
        target: LispCharPos1,
    ) -> Result<
        (
            MeasuredPositionChrome,
            LiveWindowLayoutInputs,
            WindowLayoutBox,
        ),
        MeasurementFailure,
    > {
        let font_inputs = self
            .engine
            .prepare_frame_font_inputs(self.evaluator, frame)
            .ok_or(MeasurementFailure::SourceChanged)?;
        let fonts = self
            .engine
            .realize_frame_fonts(self.evaluator, frame, &font_inputs);
        let paths = self
            .evaluator
            .frame_manager()
            .leaf_window_paths(frame)
            .ok_or(MeasurementFailure::SourceChanged)?;
        let path = paths
            .iter()
            .find_map(|(id, path)| (*id == window).then_some(path))
            .ok_or(MeasurementFailure::SourceChanged)?;
        let purpose = WindowLayoutWalkPurpose::SynchronousQuery(
            neovm_core::window::WindowLayoutQueryScope::Position { target },
        );
        let mut accepted = self.engine.retained_window_chrome_metrics.clone();
        let inputs = collect_live_window_layout_inputs(
            self.evaluator,
            frame,
            window,
            path,
            fonts.default_metrics.map(|metrics| metrics.ascent),
            self.engine.font_sizing,
            &accepted,
            purpose,
        )
        .ok_or(MeasurementFailure::SourceChanged)?;
        let geometry =
            WindowFrameGeometryRequest::new(&inputs.window, &inputs.frame, inputs.main_area_bottom)
                .resolve();
        let layout = WindowLayoutBox::resolve(
            &inputs.window,
            WindowChromeMetrics::from_params(&inputs.window),
            WindowDividerLayout::resolve(&inputs.window, &inputs.frame, geometry),
        );
        let measured = MeasuredPositionChrome::measure(
            self.evaluator,
            frame,
            &inputs.window,
            layout,
            &fonts.resolver,
            &mut self.engine.font_metrics,
            geometry,
        );
        // An invalid-face diagnostic from an earlier row survives a later
        // deferred exit or source refusal, just as in the canonical renderer.
        let invalid = fonts.resolver.take_invalid_face_references();
        log_invalid_face_references(self.evaluator, &invalid);
        let chrome = measured?;
        accepted.insert(chrome.display_window(), chrome.metrics());
        // Any Lisp edits precede the body observation. Recollect from the
        // original path, refusing replacement of the window/buffer owner.
        let inputs = collect_live_window_layout_inputs(
            self.evaluator,
            frame,
            window,
            path,
            fonts.default_metrics.map(|metrics| metrics.ascent),
            self.engine.font_sizing,
            &accepted,
            purpose,
        )
        .ok_or(MeasurementFailure::SourceChanged)?;
        chrome.validate_owner(frame, &inputs.window)?;
        let geometry =
            WindowFrameGeometryRequest::new(&inputs.window, &inputs.frame, inputs.main_area_bottom)
                .resolve();
        let layout = WindowLayoutBox::resolve(
            &inputs.window,
            chrome.metrics(),
            WindowDividerLayout::resolve(&inputs.window, &inputs.frame, geometry),
        );
        Ok((chrome, inputs, layout))
    }

    pub(super) fn read(&mut self) -> Option<neovm_core::window::WindowLayoutQuery> {
        let Request {
            frame,
            window,
            target,
        } = self.request;
        let scope = neovm_core::window::WindowLayoutQueryScope::Position { target };
        let chrome = match &self.prepared {
            QueryPreparation::Pending => return None,
            QueryPreparation::Refused(MeasurementFailure::SourceChanged)
            | QueryPreparation::Refused(MeasurementFailure::DeferredExit) => return None,
            QueryPreparation::RecursiveFormatter => None,
            QueryPreparation::Measured {
                chrome,
                inputs,
                layout,
            } => {
                if let Some(partition) = chrome.partition(self.evaluator, &inputs.window, *layout)
                    && let Some(query) = self.engine.query_cache.get_position(partition, target)
                {
                    self.engine.query_body_cache_state = QueryBodyCacheState::Reused;
                    return Some(query);
                }
                Some(chrome)
            }
        };
        self.engine.layout_frame_rust_with_face_services(
            self.evaluator,
            frame,
            LayoutPurpose::SynchronousQuery {
                window_id: window,
                scope,
            },
            None,
            #[cfg(test)]
            None,
            &rows_face_services::NoopFaceServiceFactory,
            chrome,
        )
    }
}

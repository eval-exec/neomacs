//! GNU's basic/demand face lifecycle. Definition materialization never calls
//! Lisp; only creating a realized attribute combination crosses that boundary.

use super::*;
use crate::emacs_core::error::Flow;
use crate::face::{Color, Face, FaceDecoration, SpecifiedColor, UnderlineColorOperation};
use crate::gc_trace::GcTrace;
use strum::IntoEnumIterator;

pub(crate) use crate::face::realization::{BasicFace, FaceChangeScope};

/// Consumed authority for the two recorded same-epoch definition installers.
///
/// The private field restricts construction to this module. Do not expose a
/// factory, return/store this permit, or derive Copy/Clone/Default: the two
/// production constructions must remain inline at the audited installs below.
/// All other installs use prepare, which retires the existing epoch. This ZST
/// adds no persistent state, threading mechanism or ordinary mutation work.
/// Each permit is created and consumed under one exclusive Context borrow;
/// it is never stored or published to another mutator.
///
/// prepare_runtime_faces_result refreshes after recorded dirtiness without a
/// Lisp callback before install. realize_default_after_global_merge can call
/// Lisp after recording; its full builder always gives the table a fresh
/// palette, so the retained palette identity also rejects an intermediate
/// same-count/same-epoch receipt. Count and epoch alone are insufficient there.
#[derive(Debug)]
pub(crate) struct RecordedDefinitionsRefresh {
    _private: (),
}
static_assertions::assert_eq_size!(RecordedDefinitionsRefresh, ());

/// Constructor exclusively for cfg(test) raw lifecycle/fault fixtures.
/// It grants no production caller access to an unrecorded replacement.
#[cfg(test)]
pub(crate) fn replace_definitions_for_test_fault(
    cache: &mut crate::face::realization::FrameFaceRealizationCache,
    table: crate::face::FaceTable,
) {
    cache.replace_recorded_definitions(table, RecordedDefinitionsRefresh { _private: () });
}

/// Faces such as `cursor` are marked `face-no-inherit`: changing them does
/// not dirty realized dependents (GNU xfaces.c:3104,3194,3774-3780).
pub(crate) fn named_face_change_scope<'a>(
    context: &super::super::eval::Context,
    face: &str,
    scope: FaceChangeScope<'a>,
) -> FaceChangeScope<'a> {
    if context
        .obarray()
        .get_property(face, "face-no-inherit")
        .is_some_and(|value| value.is_truthy())
    {
        match scope {
            FaceChangeScope::AllFrames => FaceChangeScope::Frames(&[]),
            FaceChangeScope::Frames(frames) | FaceChangeScope::DefinitionsOnly(frames) => {
                FaceChangeScope::DefinitionsOnly(frames)
            }
        }
    } else {
        scope
    }
}

pub(crate) fn record_named_face_change(
    context: &mut super::super::eval::Context,
    face: &str,
    scope: FaceChangeScope<'_>,
) {
    let scope = if crate::face::realization::enabled() {
        named_face_change_scope(context, face, scope)
    } else {
        scope
    };
    record_face_change_for(context, scope, Some(face));
}

/// GNU sets one global `face_change` flag, or `f->face_change` on the frames
/// whose definition changed (xfaces.c:3104-3111, 3190-3201, 3770-3776). It does
/// not visit the other frames. The counter bump already retires every cache
/// that is not carried forward, so a walk exists only to preserve realizations
/// GNU would have left alone.
fn other_frames_keep_realizations(
    frames: &crate::window::FrameManager,
    scope: FaceChangeScope<'_>,
) -> bool {
    match scope {
        FaceChangeScope::AllFrames => false,
        FaceChangeScope::Frames(affected) => frames.has_frame_outside(affected),
        FaceChangeScope::DefinitionsOnly(_) => true,
    }
}

/// Preserve GNU's frame-local realization invalidation while keeping the
/// existing global counter for layout freshness. One exclusive Context
/// mutator updates its own frame caches; no new shared state is introduced.
pub(crate) fn record_face_change(
    context: &mut super::super::eval::Context,
    scope: FaceChangeScope<'_>,
) {
    record_face_change_for(context, scope, None);
}

/// Record a face change, optionally naming the face whose definition moved.
/// The name lets each affected frame's cache re-derive only that face on the
/// next definition refresh, instead of re-deriving every definition. GNU
/// stores definitions as frame face vectors and rewrites just the changed
/// slot (xfaces.c:3767-3773); an unnamed writer plays it safe and retires
/// the whole derived snapshot for its frames.
pub(crate) fn record_face_change_for(
    context: &mut super::super::eval::Context,
    scope: FaceChangeScope<'_>,
    face: Option<&str>,
) {
    let previous = context.face_change_count;
    context.face_change_count += 1;
    let current = context.face_change_count;
    if !crate::face::realization::enabled() {
        return;
    }
    let mark_dirty = |frame: &mut crate::window::Frame| match face {
        Some(name) => frame.face_realization.mark_definition_dirty(name),
        None => frame.face_realization.mark_all_definitions_dirty(),
    };
    if !other_frames_keep_realizations(&context.frames, scope) {
        // No walk runs here, so the affected frames' caches stay stale; mark
        // their definitions dirty so a later refresh cannot reuse them.
        match scope {
            FaceChangeScope::AllFrames => {
                for frame_id in context.frames.frame_list() {
                    if let Some(frame) = context.frames.get_mut(frame_id) {
                        mark_dirty(frame);
                    }
                }
            }
            FaceChangeScope::Frames(affected) | FaceChangeScope::DefinitionsOnly(affected) => {
                for frame_id in affected {
                    if let Some(frame) = context.frames.get_mut(*frame_id) {
                        mark_dirty(frame);
                    }
                }
            }
        }
        return;
    }
    for frame_id in context.frames.frame_list() {
        let Some(frame) = context.frames.get_mut(frame_id) else {
            continue;
        };
        match scope {
            FaceChangeScope::AllFrames => {}
            FaceChangeScope::Frames(affected) if affected.contains(&frame_id) => {
                mark_dirty(frame);
            }
            FaceChangeScope::Frames(_) => {
                frame
                    .face_realization
                    .preserve_across_change(previous, current);
            }
            FaceChangeScope::DefinitionsOnly(affected) if affected.contains(&frame_id) => {
                frame
                    .face_realization
                    .definitions_changed(previous, current, face);
            }
            FaceChangeScope::DefinitionsOnly(_) => {
                frame
                    .face_realization
                    .preserve_across_change(previous, current);
            }
        }
    }
}

/// Temporary roots use the existing current-mutator root stack, rather than
/// storing Lisp state in a new TLS cache. Drop restores it on every exit.
#[must_use]
#[derive(Debug)]
struct RealizationRoots {
    saved_len: usize,
    _mutator: std::marker::PhantomData<std::rc::Rc<()>>,
}
static_assertions::assert_not_impl_any!(RealizationRoots: Send, Sync);
impl RealizationRoots {
    fn new(attributes: &Face) -> Self {
        let mut roots = Vec::new();
        attributes.trace_roots(&mut roots);
        Self::from_values(roots)
    }

    fn from_values(values: impl IntoIterator<Item = Value>) -> Self {
        let scope = Self {
            saved_len: crate::emacs_core::eval::save_scratch_gc_roots(),
            _mutator: Default::default(),
        };
        for value in values {
            crate::emacs_core::eval::push_scratch_gc_root(value);
        }
        scope
    }
}
impl Drop for RealizationRoots {
    fn drop(&mut self) {
        crate::emacs_core::eval::restore_scratch_gc_roots(self.saved_len);
    }
}

/// Rooting for one full source-table snapshot across a realization's Lisp
/// callbacks. The warm path shares the frame cache's retained batch lease —
/// an Arc bump for values the lease already pins for every collector —
/// instead of re-walking and re-admitting every definition on each cache
/// miss. GNU's per-realization code touches one lface vector at a time while
/// the frame's face cache and the symbol plists hold the definitions
/// (`realize_tty_face`, src/xfaces.c:6731-6803); only lease admission
/// failure (no installed mutator heap) falls back to scratch roots.
enum SourceTableRoots {
    Lease {
        _lease: std::sync::Arc<[crate::tagged::transport::SharedRoot]>,
    },
    Scratch {
        _scope: RealizationRoots,
    },
}

impl SourceTableRoots {
    fn new(
        context: &mut super::super::eval::Context,
        frame: FrameId,
        table: &crate::face::FaceTable,
    ) -> Self {
        match context.display_face_table_root_lease(frame, table) {
            Ok(lease) => Self::Lease { _lease: lease },
            Err(_) => {
                let mut roots = Vec::new();
                table.trace_roots(&mut roots);
                Self::Scratch {
                    _scope: RealizationRoots::from_values(roots),
                }
            }
        }
    }
}

fn descriptor_color(desc: Value, color_cells: i64) -> Option<Color> {
    let items = list_to_vec(&desc)?;
    let index = items.get(1)?.as_fixnum()?;
    let terminal = TerminalColor::from_tty_color_desc(index, color_cells)?;
    let component = |at: usize| {
        items
            .get(at)?
            .as_fixnum()
            .map(|value| (value.clamp(0, 65535) / 257) as u8)
    };
    Some(Color::rgb(component(2)?, component(3)?, component(4)?).with_terminal(terminal))
}

/// map_tty_color (GNU xfaces.c:6639-6654) first uses an exact association in
/// tty-color-alist; only a missing association calls Lisp tty-color-desc.
/// No native palette approximation is permitted at this boundary.
fn map_color(
    eval: &mut super::super::eval::Context,
    frame: FrameId,
    spec: &SpecifiedColor,
) -> Result<Option<Color>, Flow> {
    let Some(name) = spec.spec_string() else {
        return Ok(match spec {
            SpecifiedColor::Rgb(r, g, b) => Some(Color::rgb(*r, *g, *b)),
            _ => None,
        });
    };
    let cells = crate::emacs_core::terminal::pure::builtin_tty_display_color_cells(
        eval,
        vec![Value::make_frame(frame.0)],
    )?
    .as_fixnum()
    .unwrap_or(0);
    if !name.is_empty()
        && eval
            .variable_value_by_name("tty-defined-color-alist")
            .is_some_and(|value| value.is_cons())
        && eval.obarray.fboundp("tty-color-alist")
    {
        let alist = eval.funcall_general(
            Value::symbol("tty-color-alist"),
            vec![Value::make_frame(frame.0)],
        )?;
        if let Some(entries) = list_to_vec(&alist)
            && let Some(desc) = entries
                .into_iter()
                .find(|entry| entry.is_cons() && entry.cons_car().as_utf8_str() == Some(name))
        {
            // Exact associations need only NAME/INDEX in GNU map_tty_color;
            // tty-color-define permits omitted RGB. Keep that terminal identity
            // even when there are no render-preview RGB components.
            let terminal = list_to_vec(&desc)
                .and_then(|items| items.get(1).and_then(|value| value.as_fixnum()))
                .and_then(|index| TerminalColor::from_tty_color_desc(index, cells));
            return Ok(terminal.map(|terminal| {
                descriptor_color(desc, cells).unwrap_or_else(|| {
                    Color::from_name(name)
                        .or_else(|| Color::from_hex(name))
                        .unwrap_or(Color::rgb(0, 0, 0))
                        .with_terminal(terminal)
                })
            }));
        }
    }
    if name.is_empty() || !eval.obarray.fboundp("tty-color-desc") {
        return Ok(None);
    }
    let descriptor = eval.funcall_general(
        Value::symbol("tty-color-desc"),
        vec![Value::string(name), Value::make_frame(frame.0)],
    )?;
    Ok(descriptor_color(descriptor, cells))
}

/// One dynamic binding in the frame's exclusive mutator. Normal Lisp exits
/// execute unbind watchers; Rust unwinding restores native storage without
/// invoking more Lisp. The borrow prevents migration or another mutator from
/// observing this temporary binding.
#[must_use = "finish the binding to propagate Lisp unwind conditions"]
struct RealizationBinding<'a> {
    context: &'a mut super::super::eval::Context,
    depth: Option<usize>,
}

impl std::fmt::Debug for RealizationBinding<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RealizationBinding")
            .field("depth", &self.depth)
            .finish_non_exhaustive()
    }
}
static_assertions::assert_not_impl_any!(RealizationBinding<'static>: Send, Sync, Copy);

impl<'a> RealizationBinding<'a> {
    fn enter(
        context: &'a mut super::super::eval::Context,
        variable: &str,
        value: Value,
    ) -> Result<Self, Flow> {
        let depth = context.specpdl.len();
        let mut binding = Self {
            context,
            depth: Some(depth),
        };
        binding.context().try_specbind_or_unwind_to(
            depth,
            crate::emacs_core::intern::intern(variable),
            value,
        )?;
        Ok(binding)
    }

    fn context(&mut self) -> &mut super::super::eval::Context {
        self.context
    }

    fn finish<T>(mut self, result: Result<T, Flow>) -> Result<T, Flow> {
        let cleanup_input = match &result {
            Ok(_) => Ok(Value::NIL),
            Err(flow) => Err(flow.clone()),
        };
        let cleanup = match self.depth {
            Some(depth) => self.context.unbind_to_with_result(depth, cleanup_input),
            None => cleanup_input,
        };
        self.depth = None;
        cleanup?;
        result
    }
}

impl Drop for RealizationBinding<'_> {
    fn drop(&mut self) {
        if let Some(depth) = self.depth.take() {
            self.context.discard_specpdl_to(depth);
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum RealizationLookup {
    Demand,
    Basic(BasicFace),
}

impl super::super::eval::Context {
    /// Frame face classification shared with pure layout adapters. GNU's
    /// initial terminal is not a termcap face target (xfaces.c:6201-6209).
    pub fn frame_is_tty_face_target(&self, frame: FrameId) -> bool {
        self.frames
            .get(frame)
            .is_some_and(crate::face::realization::frame_is_tty_target)
    }

    /// Initialize the frame cache and basic faces at the creation boundary.
    pub fn initialize_display_faces_for_frame(&mut self, frame: FrameId) {
        initialize_frame_faces(self, frame);
    }

    /// Whether a layout resolver still holds this frame's current source
    /// definitions. A callback that changes the same frame makes the old
    /// resolver ineligible for further callbacks until layout retries.
    pub fn display_face_source_table_is_current(
        &self,
        frame: FrameId,
        source: &crate::face::FaceTable,
    ) -> bool {
        self.frames
            .get(frame)
            .and_then(|owner| owner.face_realization.table(self.face_change_count))
            .is_some_and(|table| table.shares_definitions_with(source))
    }

    /// Independently root one raw definition snapshot for layout. A current
    /// frame snapshot shares its cached lease; a retained older snapshot gets
    /// an uncached lease while its caller still owns the existing source roots.
    /// Admission belongs to this heap's current mutator and runs no Lisp.
    pub fn display_face_table_root_lease(
        &mut self,
        frame: FrameId,
        source: &crate::face::FaceTable,
    ) -> Result<
        std::sync::Arc<[crate::tagged::transport::SharedRoot]>,
        crate::tagged::transport::SharedRootError,
    > {
        let current = self.display_face_source_table_is_current(frame, source);
        if current
            && let Some(lease) = self
                .frames
                .get(frame)
                .and_then(|owner| owner.face_realization.root_lease())
        {
            return Ok(lease);
        }
        let mut values = Vec::new();
        source.trace_roots(&mut values);
        // SAFETY: binding runs on the installed Context's heap mutator. A
        // current snapshot is rooted by its frame cache and Context; a retained
        // older snapshot is admitted while the caller still owns its previous
        // layout roots. No callback or collection occurs between tracing these
        // live same-heap definitions and batch admission, whose private vector
        // allocation invokes neither Lisp nor GC. Every collector of this heap
        // traces its shared-root table while the resulting lease is retained.
        let leases =
            unsafe { crate::tagged::transport::SharedRoot::batch_from_current_heap(&values) }?;
        let lease: std::sync::Arc<[crate::tagged::transport::SharedRoot]> = leases.into();
        if current && let Some(owner) = self.frames.get_mut(frame) {
            owner
                .face_realization
                .set_root_lease(std::sync::Arc::clone(&lease));
        }
        Ok(lease)
    }

    /// Read a complete cached realization without evaluating Lisp.
    pub fn cached_display_face_for_frame(&self, frame: FrameId, attributes: &Face) -> Option<Face> {
        let cache = &self.frames.get(frame)?.face_realization;
        cache
            .generation_matches(self.face_change_count)
            .then(|| cache.lookup(attributes))
            .flatten()
            .cloned()
    }

    /// Whether the current generic lookup resolves to GNU's default slot.
    /// This is read independently of paint memos: equal-attribute insertions
    /// can change the winning basic slot without a face-change counter bump.
    pub fn cached_display_face_uses_default_slot_for_frame(
        &self,
        frame: FrameId,
        attributes: &Face,
    ) -> Option<bool> {
        let cache = &self.frames.get(frame)?.face_realization;
        cache
            .generation_matches(self.face_change_count)
            .then(|| cache.lookup_uses_default_slot(attributes))
            .flatten()
    }

    /// Import one fixed GNU basic role without evaluating Lisp. Equal source
    /// attributes do not make separate role results interchangeable.
    pub fn cached_basic_display_face_for_frame(
        &self,
        frame: FrameId,
        name: &str,
    ) -> Option<(Face, Face)> {
        let role = name.parse::<BasicFace>().ok()?;
        let cache = &self.frames.get(frame)?.face_realization;
        cache
            .generation_matches(self.face_change_count)
            .then(|| cache.basic(role))
            .flatten()
            .cloned()
    }

    /// Realize fully merged attributes on FRAME, caching the complete source
    /// attribute combination exactly like GNU lookup_face. The owning frame's
    /// exclusive mutator performs Lisp callbacks; no cross-mutator state exists.
    /// Initial placeholder frames use dummy faces (GNU xfaces.c:6205-6209).
    pub fn realize_display_face_for_frame(&mut self, frame: FrameId, attributes: &Face) -> Face {
        if self.redisplay_hook_flow_pending() {
            return attributes.clone();
        }
        match self.realize_display_face_with_lookup(frame, attributes, RealizationLookup::Demand) {
            Ok(realized) => realized,
            Err(flow) => {
                self.defer_redisplay_hook_flow(flow);
                attributes.clone()
            }
        }
    }

    fn realize_display_face_with_lookup(
        &mut self,
        frame: FrameId,
        attributes: &Face,
        lookup: RealizationLookup,
    ) -> Result<Face, Flow> {
        if !crate::face::realization::enabled() || !self.frame_is_tty_face_target(frame) {
            return Ok(attributes.clone());
        }
        if matches!(lookup, RealizationLookup::Demand)
            && let Some(realized) = self.frames.get(frame).and_then(|frame| {
                frame
                    .face_realization
                    .generation_matches(self.face_change_count)
                    .then(|| frame.face_realization.lookup(attributes))
                    .flatten()
            })
        {
            return Ok(realized.clone());
        }
        // Keep the full default lface before invoking user Lisp. Snapshot
        // publication shares the role result's epoch/generation guard below.
        let default_lface = if matches!(lookup, RealizationLookup::Basic(BasicFace::Default)) {
            lookup_frame_lisp_face_vector(self, frame, "default").and_then(|vector| {
                vector
                    .as_vector_data()
                    .map(|slots| Value::vector(slots.clone()))
            })
        } else {
            None
        };
        let _default_source_root = RealizationRoots::from_values(default_lface);
        let realization_epoch = self
            .frames
            .get(frame)
            .and_then(|owner| owner.face_realization.realization_epoch());
        let source_table = self
            .frames
            .get(frame)
            .and_then(|owner| owner.face_realization.table(self.face_change_count))
            .cloned();
        let _source_roots = source_table
            .as_ref()
            .map(|table| SourceTableRoots::new(self, frame, table));
        let _roots = RealizationRoots::new(attributes);
        let mut realized = attributes.clone();
        // GNU realize_tty_face:6731-6803: underline BEFORE foreground/background.
        if let FaceDecoration::Enabled(underline) = &mut realized.underline {
            if let Some(operations) = &attributes.color_specifications.underline_operations {
                for operation in operations.iter() {
                    underline.color = match operation {
                        UnderlineColorOperation::Lookup(specification) => {
                            map_color(self, frame, specification)?
                        }
                        UnderlineColorOperation::Foreground => None,
                    };
                }
            } else if let Some(spec) = &attributes.color_specifications.underline {
                underline.color = map_color(self, frame, spec)?;
            }
        }
        if let Some(spec) = &attributes.color_specifications.foreground {
            realized.foreground = map_color(self, frame, spec)?;
        }
        if let Some(spec) = &attributes.color_specifications.background {
            realized.background = map_color(self, frame, spec)?;
        }
        let current = self.face_change_count;
        if let Some(epoch) = &realization_epoch
            && let Some(owner) = self.frames.get_mut(frame)
            && owner.face_realization.generation_matches(current)
            && owner.face_realization.retains_epoch(epoch)
        {
            match lookup {
                RealizationLookup::Demand => owner
                    .face_realization
                    .insert(attributes.clone(), realized.clone()),
                RealizationLookup::Basic(role) => {
                    owner
                        .face_realization
                        .insert_basic(role, attributes.clone(), realized.clone());
                    if let Some(source) = default_lface {
                        owner.face_realization.retain_default_lface(source);
                    }
                }
            }
        }
        Ok(realized)
    }
}

pub(crate) fn prepare_runtime_faces(
    eval: &mut super::super::eval::Context,
    frame: FrameId,
) -> bool {
    if eval.redisplay_hook_flow_pending() {
        return false;
    }
    match prepare_runtime_faces_result(eval, frame) {
        Ok(changed) => changed,
        Err(flow) => {
            eval.defer_redisplay_hook_flow(flow);
            false
        }
    }
}

/// Ordinary Lisp frame creation and parameter updates propagate realization
/// errors directly; only the display transaction wrapper defers their flow.
pub(crate) fn prepare_runtime_faces_result(
    eval: &mut super::super::eval::Context,
    frame: FrameId,
) -> Result<bool, Flow> {
    let generation = eval.face_change_count;
    if let Some(table) = eval
        .frames
        .get(frame)
        .and_then(|frame| frame.face_realization.table(generation))
        .cloned()
    {
        *eval.face_table = table;
        return Ok(false);
    }
    // A no-purge mutation keeps the valid realization generation while
    // retiring just its raw source table. Refresh it without forcing basics.
    let definitions_only = eval
        .frames
        .get(frame)
        .is_some_and(|owner| owner.face_realization.generation_matches(generation));
    realize_default_lisp_face_for_frame(eval, frame);
    // Purged realizations and a definitions-only refresh share the reuse
    // rule: untouched definitions re-derive to the same content, so the
    // previous entry passes through shared storage unchanged. Only names in
    // the frame's dirty set -- and `default`, refreshed from live frame
    // parameters just above -- pay a Lisp derivation.
    let table = match eval.frames.get(frame).and_then(|owner| {
        (!owner.face_realization.definitions_all_dirty())
            .then(|| owner.face_realization.reusable_definitions())
            .flatten()
    }) {
        Some((previous, dirty)) => {
            runtime_face_table_from_frame_lisp_faces_incremental(eval, frame, previous, dirty)
        }
        None => runtime_face_table_from_frame_lisp_faces(eval, frame, false),
    };
    if let Some(owner) = eval.frames.get_mut(frame) {
        if definitions_only {
            owner.face_realization.replace_recorded_definitions(
                table.clone(),
                RecordedDefinitionsRefresh { _private: () },
            );
        } else {
            owner.face_realization.prepare(generation, table.clone());
        }
    }
    *eval.face_table = table;
    if definitions_only || !eval.frame_is_tty_face_target(frame) {
        return Ok(true);
    }
    if let Err(flow) = realize_basic_faces(eval, frame) {
        if let Some(owner) = eval.frames.get_mut(frame) {
            owner.face_realization.invalidate();
        }
        return Err(flow);
    }
    Ok(true)
}

pub(crate) fn realize_basic_faces(
    eval: &mut super::super::eval::Context,
    frame: FrameId,
) -> Result<(), Flow> {
    if !eval.frame_is_tty_face_target(frame) {
        return Ok(());
    }
    let table = eval.face_table.clone();
    // Advice can invalidate or replace both runtime owners. Keep every source
    // in this local snapshot alive until all later basic faces have been read.
    let mut table_roots = Vec::new();
    table.trace_roots(&mut table_roots);
    let _table_roots = RealizationRoots::from_values(table_roots);
    let mut default = table.resolve("default");
    default.inherit = None;
    default.doc = None;
    // GNU realize_default_face inhibits redisplay while the default cache slot
    // is incomplete (xfaces.c:6105-6111). Advice may request redisplay here.
    let mut scope = RealizationBinding::enter(eval, "inhibit-redisplay", Value::T)?;
    let result = scope.context().realize_display_face_with_lookup(
        frame,
        &default,
        RealizationLookup::Basic(BasicFace::Default),
    );
    scope.finish(result)?;

    // GNU's remaining basics are canonical even when the current displayed
    // buffer remaps faces, and callbacks observe that dynamic binding too.
    let mut scope = RealizationBinding::enter(eval, "face-remapping-alist", Value::NIL)?;
    let result: Result<(), Flow> = (|| {
        for basic in BasicFace::iter().filter(|basic| !matches!(basic, BasicFace::Default)) {
            let name: &'static str = basic.into();
            let mut attributes = default.merge(&table.resolve(name));
            attributes.inherit = None;
            attributes.doc = None;
            // Basic IDs always re-realize (realize_face rather than lookup).
            scope.context().realize_display_face_with_lookup(
                frame,
                &attributes,
                RealizationLookup::Basic(basic),
            )?;
        }
        Ok(())
    })();
    scope.finish(result)
}

/// Merge the raw attributes needed by GNU's TTY default-face refresh. Keeping
/// Lisp values preserves original color/plist/fontset specifications; named
/// inheritance is merged before direct attributes, with earlier list members
/// taking precedence (GNU xfaces.c:2298-2335).
fn merge_default_lface_source(
    eval: &mut super::super::eval::Context,
    frame: FrameId,
    from: Value,
    into: Value,
    active: &[usize],
) {
    if active.contains(&from.bits()) || active.len() > 80 {
        return;
    }
    let mut active = active.to_vec();
    active.push(from.bits());
    let _roots = RealizationRoots::from_values([from, into]);
    if let Some(inherit) = lisp_face_vector_attr(from, LFaceAttr::Inherit)
        && !inherit.is_nil()
        && !is_reset_like_face_attr_value(&inherit)
    {
        merge_default_inheritance(eval, frame, inherit, into, &active);
    }
    for attr in LFACE_ATTRS {
        let Some(value) = lisp_face_vector_attr(from, attr) else {
            continue;
        };
        if is_reset_like_face_attr_value(&value) {
            continue;
        }
        let value = if attr == LFaceAttr::Height && !value.is_fixnum() {
            let previous = lisp_face_vector_attr(into, attr).unwrap_or(Value::fixnum(1));
            merge_face_height_value(Some(eval), value, previous, previous)
        } else {
            value
        };
        set_lisp_face_vector_attr(into, attr, value);
    }
}

fn merge_default_inheritance(
    eval: &mut super::super::eval::Context,
    frame: FrameId,
    reference: Value,
    into: Value,
    active: &[usize],
) {
    if reference.is_nil() || active.contains(&reference.bits()) || active.len() > 80 {
        return;
    }
    let mut active = active.to_vec();
    active.push(reference.bits());
    let _roots = RealizationRoots::from_values([reference, into]);
    if !reference.is_cons() {
        if let Ok(resolved) =
            resolve_face_designator(eval, reference, FaceAliasCyclePolicy::UseDefault)
            && let Some(name) = resolved.name()
            && let Some(vector) = lookup_frame_lisp_face_vector(eval, frame, name.name())
        {
            merge_default_lface_source(eval, frame, vector, into, &active);
        }
        return;
    }
    let Some(items) = list_to_vec(&reference) else {
        return;
    };
    // A height callback may destructively detach later inherited references
    // before collecting. Root the copied work list independently of its spine.
    let _items_roots = RealizationRoots::from_values(items.iter().copied());
    if reference
        .cons_car()
        .as_bare_symbol_name()
        .is_some_and(|name| name.starts_with(':'))
    {
        let vector = make_lisp_face_vector();
        let _vector_root = RealizationRoots::from_values([vector]);
        for pair in items.chunks_exact(2) {
            if let Some(attr) = pair[0]
                .as_bare_symbol_name()
                .and_then(LFaceAttr::from_keyword)
            {
                set_lisp_face_vector_attr(vector, attr, pair[1]);
            }
        }
        merge_default_lface_source(eval, frame, vector, into, &active);
    } else {
        for reference in items.into_iter().rev() {
            merge_default_inheritance(eval, frame, reference, into, &active);
        }
    }
}

/// GNU internal-merge-in-global-face refreshes only an existing cached default
/// immediately. It does not create a face cache, nor purge unrelated roles or
/// ordinary lookup entries (xfaces.c:4353-4412). The graphical font-opening
/// branch is intentionally outside this TTY lifecycle helper.
pub(crate) fn realize_default_after_global_merge(
    eval: &mut super::super::eval::Context,
    frame: FrameId,
    global: Value,
    local: Value,
) -> Result<(), Flow> {
    let cached_source = eval
        .frames
        .get(frame)
        .and_then(|owner| owner.face_realization.default_lface());
    let _roots = RealizationRoots::from_values(
        [Some(global), Some(local), cached_source]
            .into_iter()
            .flatten(),
    );
    record_face_change_for(
        eval,
        FaceChangeScope::DefinitionsOnly(&[frame]),
        Some("default"),
    );
    let Some(cached_source) = cached_source else {
        return Ok(());
    };
    let mut scope = RealizationBinding::enter(eval, "inhibit-redisplay", Value::T)?;
    let result = (|| {
        let complete = Value::vector(
            cached_source
                .as_vector_data()
                .expect("cached default lface")
                .clone(),
        );
        let _complete_root = RealizationRoots::from_values([complete]);
        merge_default_lface_source(scope.context(), frame, local, complete, &[]);
        copy_lisp_face_vector_slots(complete, local);
        sync_face_overrides_from_lisp_face_vector("default", local, false);

        // Rebuild raw definitions without realize_default_lisp_face_for_frame:
        // GNU uses the merged absolute lface here, not TTY default normalization.
        let table = runtime_face_table_from_frame_lisp_faces(scope.context(), frame, false);
        if let Some(owner) = scope.context().frames.get_mut(frame) {
            owner.face_realization.replace_recorded_definitions(
                table.clone(),
                RecordedDefinitionsRefresh { _private: () },
            );
        }
        *scope.context().face_table = table.clone();
        let mut attributes = table.resolve("default");
        attributes.inherit = None;
        attributes.doc = None;
        if let Some(owner) = scope.context().frames.get_mut(frame) {
            owner.face_realization.retire_default();
        }
        scope.context().realize_display_face_with_lookup(
            frame,
            &attributes,
            RealizationLookup::Basic(BasicFace::Default),
        )?;

        // Read the rooted global vector after callbacks, as GNU does. Only its
        // string fg/bg attributes enter the existing primitive parameter seam.
        for (attr, parameter) in [
            (LFaceAttr::Foreground, FrameParam::ForegroundColor),
            (LFaceAttr::Background, FrameParam::BackgroundColor),
        ] {
            if let Some(value) =
                lisp_face_vector_attr(global, attr).filter(|value| value.is_string())
            {
                publish_face_attribute_to_frame_parameter(
                    scope.context(),
                    frame,
                    parameter,
                    value,
                )?;
            }
        }
        Ok(())
    })();
    let result = scope.finish(result);
    if result.is_err()
        && let Some(owner) = eval.frames.get_mut(frame)
    {
        // The old default was retired before Lisp. Retry realization on the
        // next display after a failed callback instead of using an empty slot.
        owner.face_realization.invalidate();
    }
    result
}

/// Eager frame-creation boundary (GNU init_frame_faces). Batch creation leaves
/// the cache uninitialized until the first iterator, frame.c:1822-1823.
pub(crate) fn initialize_frame_faces(eval: &mut super::super::eval::Context, frame: FrameId) {
    if crate::face::realization::enabled()
        && !eval.noninteractive()
        && eval.frame_is_tty_face_target(frame)
    {
        prepare_runtime_faces(eval, frame);
    }
}

pub(crate) fn basic_face_names() -> impl Iterator<Item = &'static str> {
    BasicFace::iter().map(Into::into)
}

/// GNU creates basic faces before copying the selected frame's lface table
/// into a newly created terminal frame (frame.c:1822-1823,1886-1897).
/// This guard temporarily exposes that empty definition table and restores
/// both owners on every Rust exit. The exclusively borrowed Context prevents
/// another mutator from observing the temporary table.
#[must_use]
struct InitialTerminalFaces<'a> {
    context: &'a mut super::super::eval::Context,
    frame: FrameId,
    definitions: Value,
    foreground: Option<Value>,
    background: Option<Value>,
    runtime: crate::face::FaceTable,
    _roots: RealizationRoots,
}

impl std::fmt::Debug for InitialTerminalFaces<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InitialTerminalFaces")
            .field("frame", &self.frame)
            .field("runtime_faces", &self.runtime.len())
            .finish_non_exhaustive()
    }
}
static_assertions::assert_not_impl_any!(InitialTerminalFaces<'static>: Send, Sync, Copy);

impl<'a> InitialTerminalFaces<'a> {
    fn new(context: &'a mut super::super::eval::Context, frame: FrameId) -> Option<Self> {
        let definitions = context.frames.get(frame)?.face_hash_table;
        let runtime = (*context.face_table).clone();
        let owner = context.frames.get(frame)?;
        let foreground = owner.known_parameter(FrameParam::ForegroundColor);
        let background = owner.known_parameter(FrameParam::BackgroundColor);
        let mut roots = vec![definitions];
        roots.extend([foreground, background].into_iter().flatten());
        runtime.trace_roots(&mut roots);
        let roots = RealizationRoots::from_values(roots);
        let empty = Value::hash_table(HashTableTest::Eq);
        let owner = context.frames.get_mut(frame)?;
        owner.face_hash_table = empty;
        // GNU's first basic pass precedes Fmodify_frame_parameters. The plain
        // Rust constructor has stored these parameters already; hide them until
        // that pass finishes, then replay their actual parameter handlers.
        // GNU's parameter alist has no colors yet, but frame-parameter
        // synthesizes these TTY defaults (frame.c:4209-4233). Represent that
        // same Lisp-visible state while hiding constructor-stored user colors.
        owner.set_known_parameter(FrameParam::ForegroundColor, Value::string("unspecified-fg"));
        owner.set_known_parameter(FrameParam::BackgroundColor, Value::string("unspecified-bg"));
        Some(Self {
            context,
            frame,
            definitions,
            foreground,
            background,
            runtime,
            _roots: roots,
        })
    }
}

impl Drop for InitialTerminalFaces<'_> {
    fn drop(&mut self) {
        if let Some(frame) = self.context.frames.get_mut(self.frame) {
            frame.face_hash_table = self.definitions;
            for (parameter, value) in [
                (FrameParam::ForegroundColor, self.foreground),
                (FrameParam::BackgroundColor, self.background),
            ] {
                match value {
                    Some(value) => {
                        frame.set_known_parameter(parameter, value);
                    }
                    None => {
                        frame.remove_known_parameter(parameter);
                    }
                }
            }
            frame.face_realization.invalidate();
        }
        *self.context.face_table = self.runtime.clone();
    }
}

/// The plain constructor already stored the complete parameter list. GNU
/// applies colors individually only after init_frame_faces and the selected
/// frame's lfaces have been copied (frame.c:1822,1880-1897,4294-4303).
/// Keep each future color at its terminal default until its own handler runs;
/// otherwise the first handler realizes the other, unapplied user color too.
/// This runs after the initial guard has restored its owners, preserving that
/// guard's constructor-state restoration on a callback error.
fn defer_creation_color_parameters(
    context: &mut super::super::eval::Context,
    frame: FrameId,
    colors: &[Value],
) {
    let Some(owner) = context.frames.get_mut(frame) else {
        return;
    };
    for pair in colors {
        let (parameter, initial) = match pair.cons_car().as_bare_symbol_name() {
            Some("foreground-color") => (FrameParam::ForegroundColor, "unspecified-fg"),
            Some("background-color") => (FrameParam::BackgroundColor, "unspecified-bg"),
            _ => continue,
        };
        owner.set_known_parameter(parameter, Value::string(initial));
    }
}

pub(crate) fn initialize_new_terminal_frame_faces(
    context: &mut super::super::eval::Context,
    frame: FrameId,
    parameters: Value,
) -> Result<(), Flow> {
    if !crate::face::realization::enabled() || !context.frame_is_tty_face_target(frame) {
        return Ok(());
    }
    let _roots = RealizationRoots::from_values([parameters]);
    if !context.noninteractive() {
        let Some(initial) = InitialTerminalFaces::new(context, frame) else {
            return Ok(());
        };
        for name in basic_face_names() {
            let _ = ensure_frame_lisp_face_vector(
                initial.context,
                frame,
                name,
                FrameFaceInitial::Empty,
            );
        }
        prepare_runtime_faces_result(initial.context, frame)?;
    }
    // Frame creation applies its color parameters only after the initial basic
    // faces and selected-frame face definitions exist (GNU frame.c:1886-1905).
    // Other parameter handlers already ran in the plain constructor.
    let colors: Vec<Value> = list_to_vec(&parameters)
        .unwrap_or_default()
        .into_iter()
        .filter(|pair| {
            pair.is_cons()
                && matches!(
                    pair.cons_car().as_bare_symbol_name(),
                    Some("foreground-color" | "background-color")
                )
        })
        .collect();
    if !colors.is_empty() {
        defer_creation_color_parameters(context, frame, &colors);
        let color_parameters = Value::list(colors);
        let _replay_roots = RealizationRoots::from_values([color_parameters]);
        super::super::frame::builtin_modify_frame_parameters(
            context,
            vec![Value::make_frame(frame.0), color_parameters],
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/realization_boundaries_test.rs"]
mod boundary_tests;

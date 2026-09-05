//! Renderer-safe pointer interactions and transient paint overrides.
//!
//! A map is validated against the presentation that owns its geometry and
//! primitive tables before it can be published. The validation context is not
//! retained: render-time consumers only receive coherent immutable records.

use crate::{
    DisplaySlotId, DisplayWindowId, FaceId, FrameGlyph, FrameGlyphBuffer, FrameRect, FrameSize,
    InteractionId, PresentationId,
};

/// Semantic area resolved from the immutable geometry of one presentation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PresentedRegionKind {
    TextBody,
    LeftMargin,
    RightMargin,
    LeftFringe,
    RightFringe,
    LeftScrollBar,
    RightScrollBar,
    HorizontalScrollBar,
    TabLine,
    HeaderLine,
    ModeLine,
    RightDivider,
    BottomDivider,
    MenuBar,
    ToolBar,
    CompactBar,
    TabBar,
}

impl PresentedRegionKind {
    /// Return the resize dimension represented by this semantic region.
    ///
    /// The exhaustive match makes adding a new presented region a compile-time
    /// prompt to decide whether it controls window resizing.
    #[must_use]
    pub const fn resize_axis(self) -> Option<PresentedResizeAxis> {
        match self {
            Self::RightDivider => Some(PresentedResizeAxis::Horizontal),
            Self::BottomDivider => Some(PresentedResizeAxis::Vertical),
            Self::TextBody
            | Self::LeftMargin
            | Self::RightMargin
            | Self::LeftFringe
            | Self::RightFringe
            | Self::LeftScrollBar
            | Self::RightScrollBar
            | Self::HorizontalScrollBar
            | Self::TabLine
            | Self::HeaderLine
            | Self::ModeLine
            | Self::MenuBar
            | Self::ToolBar
            | Self::CompactBar
            | Self::TabBar => None,
        }
    }
}

/// Dimension changed by dragging a presented window resize handle.
///
/// This is deliberately distinct from [`PresentedRegionKind`]: resize handles
/// are interaction overlays and may overlap the non-overlapping regions used
/// to describe painted window geometry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PresentedResizeAxis {
    Horizontal,
    Vertical,
}

impl PresentedResizeAxis {
    #[must_use]
    pub const fn region_kind(self) -> PresentedRegionKind {
        match self {
            Self::Horizontal => PresentedRegionKind::RightDivider,
            Self::Vertical => PresentedRegionKind::BottomDivider,
        }
    }
}

/// Side of the window allocation that owns a resize handle.
///
/// A leading horizontal handle is used when vertical scroll bars are on the
/// left; GNU then asks Lisp to resize the window on the handle's left.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PresentedResizeEdge {
    Leading,
    Trailing,
}

/// An interaction-only resize target over one presented window.
///
/// Unlike structural hit regions, a handle is allowed to overlap fringes,
/// margins, or text. Resolution gives handles precedence over those regions.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PresentedResizeHandle {
    window: DisplayWindowId,
    axis: PresentedResizeAxis,
    edge: PresentedResizeEdge,
    bounds: FrameRect,
}

impl PresentedResizeHandle {
    #[must_use]
    pub const fn new(
        window: DisplayWindowId,
        axis: PresentedResizeAxis,
        edge: PresentedResizeEdge,
        bounds: FrameRect,
    ) -> Self {
        Self {
            window,
            axis,
            edge,
            bounds,
        }
    }

    #[must_use]
    pub const fn window(self) -> DisplayWindowId {
        self.window
    }

    #[must_use]
    pub const fn axis(self) -> PresentedResizeAxis {
        self.axis
    }

    #[must_use]
    pub const fn edge(self) -> PresentedResizeEdge {
        self.edge
    }

    #[must_use]
    pub const fn bounds(self) -> FrameRect {
        self.bounds
    }

    const fn as_hit_region(self) -> PresentedHitRegion {
        PresentedHitRegion::new(
            Some(self.window),
            self.axis.region_kind(),
            self.bounds,
            i32::MAX,
        )
    }
}

/// Stable semantic identity shared by presentation geometry and pointer data.
///
/// The identity contains meaning, not vector position, so serialization and
/// private spatial-index rebuilds cannot silently retarget an interaction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PresentedRegionId {
    window: Option<DisplayWindowId>,
    kind: PresentedRegionKind,
}

impl PresentedRegionId {
    #[must_use]
    pub const fn new(window: Option<DisplayWindowId>, kind: PresentedRegionKind) -> Self {
        Self { window, kind }
    }

    #[must_use]
    pub const fn window(self) -> Option<DisplayWindowId> {
        self.window
    }

    #[must_use]
    pub const fn kind(self) -> PresentedRegionKind {
        self.kind
    }
}

/// One z-ordered semantic region in frame-local logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PresentedHitRegion {
    window: Option<DisplayWindowId>,
    kind: PresentedRegionKind,
    bounds: FrameRect,
    z_order: i32,
}

impl PresentedHitRegion {
    #[must_use]
    pub const fn new(
        window: Option<DisplayWindowId>,
        kind: PresentedRegionKind,
        bounds: FrameRect,
        z_order: i32,
    ) -> Self {
        Self {
            window,
            kind,
            bounds,
            z_order,
        }
    }

    #[must_use]
    pub const fn window(self) -> Option<DisplayWindowId> {
        self.window
    }

    #[must_use]
    pub const fn id(self) -> PresentedRegionId {
        PresentedRegionId::new(self.window, self.kind)
    }

    #[must_use]
    pub const fn kind(self) -> PresentedRegionKind {
        self.kind
    }

    #[must_use]
    pub const fn bounds(self) -> FrameRect {
        self.bounds
    }

    #[must_use]
    pub const fn z_order(self) -> i32 {
        self.z_order
    }
}

/// Exact displayed buffer position occupying one frame-local cell rectangle.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PresentedTextPosition {
    window: DisplayWindowId,
    bounds: FrameRect,
    buffer_position: i64,
    row: i64,
    column: i64,
}

/// Exact position in a displayed Lisp string occupying one chrome rectangle.
///
/// GNU glyphs retain `(object, charpos)` directly.  The display protocol cannot
/// transport a VM object, so it carries the row's stable string identity and
/// character index; the evaluator pairs that identity with the rooted Lisp
/// value retained in its window snapshot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PresentedWindowChromeArea {
    TabLine,
    HeaderLine,
    ModeLine,
}

impl PresentedWindowChromeArea {
    #[must_use]
    pub const fn region_kind(self) -> PresentedRegionKind {
        match self {
            Self::TabLine => PresentedRegionKind::TabLine,
            Self::HeaderLine => PresentedRegionKind::HeaderLine,
            Self::ModeLine => PresentedRegionKind::ModeLine,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PresentedStringPosition {
    window: DisplayWindowId,
    area: PresentedWindowChromeArea,
    bounds: FrameRect,
    string: crate::glyph_matrix::GlyphStringId,
    char_index: u64,
}

impl PresentedStringPosition {
    #[must_use]
    pub const fn new(
        window: DisplayWindowId,
        area: PresentedWindowChromeArea,
        bounds: FrameRect,
        string: crate::glyph_matrix::GlyphStringId,
        char_index: u64,
    ) -> Self {
        Self {
            window,
            area,
            bounds,
            string,
            char_index,
        }
    }

    #[must_use]
    pub const fn window(self) -> DisplayWindowId {
        self.window
    }

    #[must_use]
    pub const fn area(self) -> PresentedWindowChromeArea {
        self.area
    }

    #[must_use]
    pub const fn region(self) -> PresentedRegionKind {
        self.area.region_kind()
    }

    #[must_use]
    pub const fn bounds(self) -> FrameRect {
        self.bounds
    }

    #[must_use]
    pub const fn string(self) -> crate::glyph_matrix::GlyphStringId {
        self.string
    }

    #[must_use]
    pub const fn char_index(self) -> u64 {
        self.char_index
    }
}

impl PresentedTextPosition {
    #[must_use]
    pub const fn new(
        window: DisplayWindowId,
        bounds: FrameRect,
        buffer_position: i64,
        row: i64,
        column: i64,
    ) -> Self {
        Self {
            window,
            bounds,
            buffer_position,
            row,
            column,
        }
    }

    #[must_use]
    pub const fn window(self) -> DisplayWindowId {
        self.window
    }

    #[must_use]
    pub const fn bounds(self) -> FrameRect {
        self.bounds
    }

    #[must_use]
    pub const fn buffer_position(self) -> i64 {
        self.buffer_position
    }

    #[must_use]
    pub const fn row(self) -> i64 {
        self.row
    }

    #[must_use]
    pub const fn column(self) -> i64 {
        self.column
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresentedHitQuery {
    point: crate::interaction_projection::PresentationFramePoint,
}

impl PresentedHitQuery {
    /// A query about the point `point` names.
    ///
    /// There is deliberately no constructor taking raw coordinates. A
    /// `PresentationFramePoint` can only come from mapping a surface point
    /// through the transform the frame was drawn with, so a hit test cannot
    /// silently ask about the wrong pixel while panes are in motion — the case
    /// that would otherwise select the wrong window mid-`split-window`.
    #[must_use]
    pub const fn new(point: crate::interaction_projection::PresentationFramePoint) -> Self {
        Self { point }
    }

    #[must_use]
    pub const fn point(self) -> crate::interaction_projection::PresentationFramePoint {
        self.point
    }

    #[must_use]
    pub const fn presentation(self) -> PresentationId {
        self.point.presentation()
    }

    #[must_use]
    pub fn x(self) -> f32 {
        self.point.x()
    }

    #[must_use]
    pub fn y(self) -> f32 {
        self.point.y()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresentedUnifiedHit {
    semantic: Option<PresentedHit>,
    interaction: Option<InteractionId>,
    appearance: Option<PointerAppearanceId>,
}

impl PresentedUnifiedHit {
    pub(crate) const fn new(
        semantic: Option<PresentedHit>,
        interaction: Option<InteractionId>,
        appearance: Option<PointerAppearanceId>,
    ) -> Self {
        Self {
            semantic,
            interaction,
            appearance,
        }
    }
    #[must_use]
    pub const fn semantic(self) -> Option<PresentedHit> {
        self.semantic
    }

    #[must_use]
    pub const fn interaction(self) -> Option<InteractionId> {
        self.interaction
    }

    #[must_use]
    pub const fn appearance(self) -> Option<PointerAppearanceId> {
        self.appearance
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresentedHit {
    region: PresentedHitRegion,
    text_position: Option<PresentedTextPosition>,
    string_position: Option<PresentedStringPosition>,
}

impl PresentedHit {
    #[must_use]
    pub const fn region(self) -> PresentedHitRegion {
        self.region
    }

    #[must_use]
    pub const fn text_position(self) -> Option<PresentedTextPosition> {
        self.text_position
    }

    #[must_use]
    pub const fn string_position(self) -> Option<PresentedStringPosition> {
        self.string_position
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentedHitError {
    StalePresentation {
        expected: PresentationId,
        requested: PresentationId,
    },
    InvalidRegionGeometry,
    InvalidResizeHandleGeometry,
    InvalidTextPositionGeometry,
    InvalidStringPositionGeometry,
    StringPositionOutsideSemanticRegion,
    MissingBodyRow {
        window: DisplayWindowId,
        output_row: i64,
    },
    WindowGeometryMismatch {
        window: DisplayWindowId,
        region: PresentedRegionKind,
    },
    PointerOutsideSemanticRegion,
    MissingPointerSemanticOwner,
    UnknownPointerSemanticOwner(PresentedRegionId),
}

impl std::fmt::Display for PresentedHitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StalePresentation {
                expected,
                requested,
            } => write!(
                formatter,
                "stale presentation: expected {}, requested {}",
                expected.get(),
                requested.get()
            ),
            Self::InvalidRegionGeometry => formatter.write_str("invalid hit-region geometry"),
            Self::InvalidResizeHandleGeometry => {
                formatter.write_str("invalid resize-handle geometry")
            }
            Self::InvalidTextPositionGeometry => {
                formatter.write_str("invalid text-position geometry")
            }
            Self::InvalidStringPositionGeometry => {
                formatter.write_str("invalid string-position geometry")
            }
            Self::StringPositionOutsideSemanticRegion => {
                formatter.write_str("string position is outside its window-chrome semantic region")
            }
            Self::MissingBodyRow { window, output_row } => write!(
                formatter,
                "window {window} has no canonical body row for output row {output_row}"
            ),
            Self::WindowGeometryMismatch { window, region } => write!(
                formatter,
                "window {window} has divergent geometry for semantic region {region:?}"
            ),
            Self::PointerOutsideSemanticRegion => formatter.write_str(
                "pointer interaction/appearance region is outside semantic presentation geometry",
            ),
            Self::MissingPointerSemanticOwner => {
                formatter.write_str("pointer region has no semantic owner")
            }
            Self::UnknownPointerSemanticOwner(owner) => {
                write!(
                    formatter,
                    "pointer region names unknown semantic owner {owner:?}"
                )
            }
        }
    }
}

impl std::error::Error for PresentedHitError {}

/// Immutable, presentation-qualified semantic hit index.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct PresentedHitIndex {
    presentation: PresentationId,
    regions: Vec<PresentedHitRegion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    resize_handles: Vec<PresentedResizeHandle>,
    text_positions: Vec<PresentedTextPosition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    string_positions: Vec<PresentedStringPosition>,
    #[serde(skip)]
    region_buckets: Vec<PresentedHitBucket>,
    #[serde(skip)]
    resize_handle_buckets: Vec<PresentedHitBucket>,
    #[serde(skip)]
    text_buckets: Vec<PresentedHitBucket>,
    #[serde(skip)]
    string_buckets: Vec<PresentedHitBucket>,
    #[serde(skip)]
    pointer_regions: Vec<PresentedPointerRegion>,
    #[serde(skip)]
    pointer_buckets: Vec<PresentedHitBucket>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
struct PresentedHitBucket {
    top: f32,
    bottom: f32,
    prefix_max_bottom: f32,
    candidates: Vec<usize>,
    prefix_max_right: Vec<f32>,
}

#[derive(serde::Deserialize)]
struct RawPresentedHitIndex {
    presentation: PresentationId,
    regions: Vec<PresentedHitRegion>,
    #[serde(default)]
    resize_handles: Vec<PresentedResizeHandle>,
    text_positions: Vec<PresentedTextPosition>,
    #[serde(default)]
    string_positions: Vec<PresentedStringPosition>,
}

impl<'de> serde::Deserialize<'de> for PresentedHitIndex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawPresentedHitIndex::deserialize(deserializer)?;
        Self::from_parts_with_strings(
            raw.presentation,
            raw.regions,
            raw.text_positions,
            raw.string_positions,
        )
        .and_then(|index| index.with_resize_handles(raw.resize_handles))
        .map_err(serde::de::Error::custom)
    }
}

impl Default for PresentedHitIndex {
    fn default() -> Self {
        Self::empty(PresentationId::default())
    }
}

impl PresentedHitIndex {
    #[must_use]
    pub const fn empty(presentation: PresentationId) -> Self {
        Self {
            presentation,
            regions: Vec::new(),
            resize_handles: Vec::new(),
            text_positions: Vec::new(),
            string_positions: Vec::new(),
            region_buckets: Vec::new(),
            resize_handle_buckets: Vec::new(),
            text_buckets: Vec::new(),
            string_buckets: Vec::new(),
            pointer_regions: Vec::new(),
            pointer_buckets: Vec::new(),
        }
    }

    pub fn from_parts(
        presentation: PresentationId,
        regions: Vec<PresentedHitRegion>,
        text_positions: Vec<PresentedTextPosition>,
    ) -> Result<Self, PresentedHitError> {
        Self::from_parts_with_strings(presentation, regions, text_positions, Vec::new())
    }

    pub fn from_parts_with_strings(
        presentation: PresentationId,
        regions: Vec<PresentedHitRegion>,
        text_positions: Vec<PresentedTextPosition>,
        string_positions: Vec<PresentedStringPosition>,
    ) -> Result<Self, PresentedHitError> {
        if regions
            .iter()
            .any(|region| !rect_has_valid_geometry(region.bounds))
        {
            return Err(PresentedHitError::InvalidRegionGeometry);
        }
        if text_positions
            .iter()
            .any(|position| !rect_has_valid_geometry(position.bounds))
        {
            return Err(PresentedHitError::InvalidTextPositionGeometry);
        }
        if string_positions
            .iter()
            .any(|position| !rect_has_valid_geometry(position.bounds))
        {
            return Err(PresentedHitError::InvalidStringPositionGeometry);
        }
        for position in &string_positions {
            let owner = regions.iter().find(|region| {
                region.window() == Some(position.window()) && region.kind() == position.region()
            });
            if !owner.is_some_and(|region| rect_contains_rect(region.bounds(), position.bounds())) {
                return Err(PresentedHitError::StringPositionOutsideSemanticRegion);
            }
        }
        let region_buckets = build_presented_hit_buckets(
            regions
                .iter()
                .enumerate()
                .map(|(index, region)| (index, region.bounds)),
        );
        let text_buckets = build_presented_hit_buckets(
            text_positions
                .iter()
                .enumerate()
                .map(|(index, position)| (index, position.bounds)),
        );
        let string_buckets = build_presented_hit_buckets(
            string_positions
                .iter()
                .enumerate()
                .map(|(index, position)| (index, position.bounds)),
        );
        Ok(Self {
            presentation,
            regions,
            resize_handles: Vec::new(),
            text_positions,
            string_positions,
            region_buckets,
            resize_handle_buckets: Vec::new(),
            text_buckets,
            string_buckets,
            pointer_regions: Vec::new(),
            pointer_buckets: Vec::new(),
        })
    }

    /// Attach typed interaction overlays without weakening the structural
    /// window-region partition.
    pub fn with_resize_handles(
        mut self,
        resize_handles: Vec<PresentedResizeHandle>,
    ) -> Result<Self, PresentedHitError> {
        if resize_handles
            .iter()
            .any(|handle| !rect_has_valid_geometry(handle.bounds))
        {
            return Err(PresentedHitError::InvalidResizeHandleGeometry);
        }
        self.resize_handle_buckets = build_presented_hit_buckets(
            resize_handles
                .iter()
                .enumerate()
                .map(|(index, handle)| (index, handle.bounds)),
        );
        self.resize_handles = resize_handles;
        Ok(self)
    }

    /// Validate pointer ownership once and attach it to this immutable query
    /// object. Runtime input never merges independently resolved maps.
    pub(crate) fn bind_pointer_regions(
        &mut self,
        pointer_regions: &[PresentedPointerRegion],
    ) -> Result<(), PresentedHitError> {
        for pointer in pointer_regions {
            let owner = pointer
                .owner()
                .ok_or(PresentedHitError::MissingPointerSemanticOwner)?;
            let semantic = self
                .regions
                .iter()
                .find(|region| region.id() == owner)
                .ok_or(PresentedHitError::UnknownPointerSemanticOwner(owner))?;
            if !rect_contains_rect(semantic.bounds(), pointer.bounds()) {
                return Err(PresentedHitError::PointerOutsideSemanticRegion);
            }
        }
        self.pointer_regions = pointer_regions.to_vec();
        self.pointer_buckets = build_presented_hit_buckets(
            self.pointer_regions
                .iter()
                .enumerate()
                .map(|(index, region)| (index, region.bounds())),
        );
        Ok(())
    }

    pub(crate) fn resolve_unified(
        &self,
        query: PresentedHitQuery,
    ) -> Result<Option<PresentedUnifiedHit>, PresentedHitError> {
        if query.presentation() != self.presentation {
            return Err(PresentedHitError::StalePresentation {
                expected: self.presentation,
                requested: query.presentation(),
            });
        }
        let resolved_semantic = self.resolve(query)?;
        let resize_handle_wins =
            resolved_semantic.is_some_and(|hit| hit.region().kind().resize_axis().is_some());
        let pointer = (!resize_handle_wins)
            .then(|| {
                find_presented_pointer_candidate(
                    &self.pointer_regions,
                    &self.pointer_buckets,
                    query.x(),
                    query.y(),
                )
            })
            .flatten();
        let semantic = if let Some(pointer) = pointer {
            let owner = pointer
                .owner()
                .expect("published pointer regions have validated semantic owners");
            let region = *self
                .regions
                .iter()
                .find(|region| region.id() == owner)
                .expect("published pointer owner remains in its immutable semantic index");
            Some(PresentedHit {
                region,
                text_position: self.resolve_text_position(region, query.x(), query.y()),
                string_position: self.resolve_string_position(region, query.x(), query.y()),
            })
        } else {
            resolved_semantic
        };
        if semantic.is_none() && pointer.is_none() {
            return Ok(None);
        }
        Ok(Some(PresentedUnifiedHit::new(
            semantic,
            pointer.and_then(PresentedPointerRegion::interaction),
            pointer.and_then(PresentedPointerRegion::appearance),
        )))
    }

    fn resolve_text_position(
        &self,
        region: PresentedHitRegion,
        x: f32,
        y: f32,
    ) -> Option<PresentedTextPosition> {
        if region.kind != PresentedRegionKind::TextBody {
            return None;
        }
        let mut selected = None;
        for_each_presented_hit_candidate(
            &self.text_buckets,
            x,
            y,
            |index| self.text_positions[index].bounds,
            |index| {
                if Some(self.text_positions[index].window) == region.window {
                    selected = Some(selected.map_or(index, |current: usize| current.min(index)));
                }
            },
        );
        selected.map(|index| self.text_positions[index])
    }

    fn resolve_string_position(
        &self,
        region: PresentedHitRegion,
        x: f32,
        y: f32,
    ) -> Option<PresentedStringPosition> {
        let window = region.window()?;
        let mut selected = None;
        for_each_presented_hit_candidate(
            &self.string_buckets,
            x,
            y,
            |index| self.string_positions[index].bounds,
            |index| {
                let candidate = self.string_positions[index];
                if candidate.window == window && candidate.region() == region.kind {
                    selected = Some(selected.map_or(index, |current: usize| current.min(index)));
                }
            },
        );
        selected.map(|index| self.string_positions[index])
    }

    #[must_use]
    pub const fn presentation(&self) -> PresentationId {
        self.presentation
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
            && self.resize_handles.is_empty()
            && self.text_positions.is_empty()
            && self.string_positions.is_empty()
    }

    pub fn resolve(
        &self,
        query: PresentedHitQuery,
    ) -> Result<Option<PresentedHit>, PresentedHitError> {
        if query.presentation() != self.presentation {
            return Err(PresentedHitError::StalePresentation {
                expected: self.presentation,
                requested: query.presentation(),
            });
        }
        // No non-finite guard: the query's point is a `LogicalPixels` pair, and
        // `LogicalPixels::new` rejects anything but a finite value, so an
        // infinite or NaN coordinate cannot reach here to be checked for.
        let resize_handle = best_presented_hit_candidate(
            &self.resize_handle_buckets,
            query.x(),
            query.y(),
            |index| self.resize_handles[index].bounds,
            std::cmp::Reverse,
        );
        if let Some(index) = resize_handle {
            return Ok(Some(PresentedHit {
                region: self.resize_handles[index].as_hit_region(),
                text_position: None,
                string_position: None,
            }));
        }
        let best = best_presented_hit_candidate(
            &self.region_buckets,
            query.x(),
            query.y(),
            |index| self.regions[index].bounds,
            |index| (self.regions[index].z_order, std::cmp::Reverse(index)),
        );
        let Some(region_index) = best else {
            return Ok(None);
        };
        let region = &self.regions[region_index];
        let text_position = self.resolve_text_position(*region, query.x(), query.y());
        Ok(Some(PresentedHit {
            region: *region,
            text_position,
            string_position: self.resolve_string_position(*region, query.x(), query.y()),
        }))
    }

    #[must_use]
    pub fn regions(&self) -> &[PresentedHitRegion] {
        &self.regions
    }

    #[must_use]
    pub fn resize_handles(&self) -> &[PresentedResizeHandle] {
        &self.resize_handles
    }

    #[must_use]
    pub fn text_positions(&self) -> &[PresentedTextPosition] {
        &self.text_positions
    }

    #[must_use]
    pub fn string_positions(&self) -> &[PresentedStringPosition] {
        &self.string_positions
    }

    #[cfg(test)]
    pub(crate) fn candidate_count(&self, x: f32, y: f32) -> usize {
        let mut count = 0;
        for_each_presented_hit_candidate(
            &self.region_buckets,
            x,
            y,
            |index| self.regions[index].bounds,
            |_| count += 1,
        );
        for_each_presented_hit_candidate(
            &self.resize_handle_buckets,
            x,
            y,
            |index| self.resize_handles[index].bounds,
            |_| count += 1,
        );
        for_each_presented_hit_candidate(
            &self.text_buckets,
            x,
            y,
            |index| self.text_positions[index].bounds,
            |_| count += 1,
        );
        for_each_presented_hit_candidate(
            &self.string_buckets,
            x,
            y,
            |index| self.string_positions[index].bounds,
            |_| count += 1,
        );
        count
    }
}

fn build_presented_hit_buckets(
    entries: impl Iterator<Item = (usize, FrameRect)>,
) -> Vec<PresentedHitBucket> {
    let mut entries = entries
        .map(|(index, bounds)| (bounds.y(), bounds.y() + bounds.height(), index, bounds))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.0
            .total_cmp(&right.0)
            .then(left.1.total_cmp(&right.1))
            .then(left.2.cmp(&right.2))
    });
    let mut buckets: Vec<PresentedHitBucket> = Vec::new();
    for (top, bottom, index, _) in &entries {
        if buckets.last().is_none_or(|bucket| {
            bucket.top.total_cmp(top).is_ne() || bucket.bottom.total_cmp(bottom).is_ne()
        }) {
            buckets.push(PresentedHitBucket {
                top: *top,
                bottom: *bottom,
                prefix_max_bottom: *bottom,
                candidates: Vec::new(),
                prefix_max_right: Vec::new(),
            });
        }
        buckets.last_mut().unwrap().candidates.push(*index);
    }
    let bounds_by_index = entries
        .iter()
        .map(|(_, _, index, bounds)| (*index, *bounds))
        .collect::<std::collections::HashMap<_, _>>();
    let mut prefix_max_bottom = 0.0_f32;
    for bucket in &mut buckets {
        prefix_max_bottom = prefix_max_bottom.max(bucket.bottom);
        bucket.prefix_max_bottom = prefix_max_bottom;
        bucket.candidates.sort_by(|left, right| {
            bounds_by_index[left]
                .x()
                .total_cmp(&bounds_by_index[right].x())
                .then(left.cmp(right))
        });
        let mut prefix_max_right = 0.0_f32;
        bucket.prefix_max_right = bucket
            .candidates
            .iter()
            .map(|index| {
                let bounds = bounds_by_index[index];
                prefix_max_right = prefix_max_right.max(bounds.x() + bounds.width());
                prefix_max_right
            })
            .collect();
    }
    buckets
}

fn for_each_presented_hit_candidate(
    buckets: &[PresentedHitBucket],
    x: f32,
    y: f32,
    bounds: impl Fn(usize) -> FrameRect,
    mut visit: impl FnMut(usize),
) {
    let mut bucket_end = buckets.partition_point(|bucket| bucket.top <= y);
    while bucket_end > 0 {
        let bucket = &buckets[bucket_end - 1];
        if bucket.prefix_max_bottom <= y {
            break;
        }
        if y < bucket.bottom {
            let mut candidate_end = bucket
                .candidates
                .partition_point(|&index| bounds(index).x() <= x);
            while candidate_end > 0 {
                let position = candidate_end - 1;
                if bucket.prefix_max_right[position] <= x {
                    break;
                }
                let index = bucket.candidates[position];
                if contains(bounds(index), x, y) {
                    visit(index);
                }
                candidate_end -= 1;
            }
        }
        bucket_end -= 1;
    }
}

fn best_presented_hit_candidate<Key: Ord>(
    buckets: &[PresentedHitBucket],
    x: f32,
    y: f32,
    bounds: impl Fn(usize) -> FrameRect,
    key: impl Fn(usize) -> Key,
) -> Option<usize> {
    let mut best: Option<(usize, Key)> = None;
    for_each_presented_hit_candidate(buckets, x, y, bounds, |index| {
        let candidate_key = key(index);
        if best
            .as_ref()
            .is_none_or(|(_, best_key)| &candidate_key > best_key)
        {
            best = Some((index, candidate_key));
        }
    });
    best.map(|(index, _)| index)
}

fn contains(bounds: FrameRect, x: f32, y: f32) -> bool {
    x >= bounds.x()
        && x < bounds.x() + bounds.width()
        && y >= bounds.y()
        && y < bounds.y() + bounds.height()
}

fn rect_contains_rect(outer: FrameRect, inner: FrameRect) -> bool {
    inner.x() >= outer.x()
        && inner.y() >= outer.y()
        && inner.x() + inner.width() <= outer.x() + outer.width()
        && inner.y() + inner.height() <= outer.y() + outer.height()
}

fn find_presented_pointer_candidate<'a>(
    regions: &'a [PresentedPointerRegion],
    buckets: &[PresentedHitBucket],
    x: f32,
    y: f32,
) -> Option<&'a PresentedPointerRegion> {
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    let mut best = None;
    for_each_presented_hit_candidate(
        buckets,
        x,
        y,
        |index| regions[index].bounds(),
        |index| best = Some(best.map_or(index, |current: usize| current.min(index))),
    );
    best.map(|index| &regions[index])
}

/// Presentation-local index of one transient pointer appearance.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct PointerAppearanceId(u32);

impl PointerAppearanceId {
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<usize> for PointerAppearanceId {
    type Error = PointerAppearanceIdOverflow;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        u32::try_from(value)
            .map(Self)
            .map_err(|_| PointerAppearanceIdOverflow)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PointerAppearanceIdOverflow;

impl std::fmt::Display for PointerAppearanceIdOverflow {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("pointer appearance index exceeds u32")
    }
}

impl std::error::Error for PointerAppearanceIdOverflow {}

/// Transient phase selected by pointer input for an immutable presentation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PointerAppearancePhase {
    Hover,
    Pressed,
}

/// Renderer-safe transient appearance selection.
///
/// Presentation identity stays at the runtime boundary: callers may create
/// this value only after proving that the active appearance belongs to the
/// exact [`FrameGlyphBuffer`] being rendered.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PointerAppearanceSelection {
    appearance: PointerAppearanceId,
    phase: PointerAppearancePhase,
}

impl PointerAppearanceSelection {
    #[must_use]
    pub const fn new(appearance: PointerAppearanceId, phase: PointerAppearancePhase) -> Self {
        Self { appearance, phase }
    }

    #[must_use]
    pub const fn appearance(self) -> PointerAppearanceId {
        self.appearance
    }

    #[must_use]
    pub const fn phase(self) -> PointerAppearancePhase {
        self.phase
    }
}

/// Existing presentation primitive table addressed by a paint span.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PresentedPrimitiveKind {
    Glyph,
    Image,
}

/// Contiguous primitives redrawn with a transient pointer override.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PresentedPaintSpan {
    kind: PresentedPrimitiveKind,
    first: u32,
    len: u32,
    clip: FrameRect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hover: Option<PointerDrawMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pressed: Option<PointerDrawMode>,
}

impl PresentedPaintSpan {
    #[must_use]
    pub const fn new(kind: PresentedPrimitiveKind, first: u32, len: u32, clip: FrameRect) -> Self {
        Self {
            kind,
            first,
            len,
            clip,
            hover: None,
            pressed: None,
        }
    }

    #[must_use]
    pub const fn with_modes(mut self, hover: PointerDrawMode, pressed: PointerDrawMode) -> Self {
        self.hover = Some(hover);
        self.pressed = Some(pressed);
        self
    }

    #[must_use]
    pub const fn kind(&self) -> PresentedPrimitiveKind {
        self.kind
    }

    #[must_use]
    pub const fn first(&self) -> u32 {
        self.first
    }

    #[must_use]
    pub const fn len(&self) -> u32 {
        self.len
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub const fn clip(&self) -> FrameRect {
        self.clip
    }

    #[must_use]
    pub const fn hover(&self) -> Option<PointerDrawMode> {
        self.hover
    }

    #[must_use]
    pub const fn pressed(&self) -> Option<PointerDrawMode> {
        self.pressed
    }
}

/// Source-addressed primitive paint resolved during canonical materialization.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PresentedSourcePaintSpan {
    kind: PresentedPrimitiveKind,
    row_role: crate::GlyphRowRole,
    slot: DisplaySlotId,
    #[serde(default = "source_span_default_len")]
    len: u32,
    clip: FrameRect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hover: Option<PointerDrawMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pressed: Option<PointerDrawMode>,
}

const fn source_span_default_len() -> u32 {
    1
}

impl PresentedSourcePaintSpan {
    #[must_use]
    pub const fn new(
        kind: PresentedPrimitiveKind,
        row_role: crate::GlyphRowRole,
        slot: DisplaySlotId,
        clip: FrameRect,
    ) -> Self {
        Self {
            kind,
            row_role,
            slot,
            len: 1,
            clip,
            hover: None,
            pressed: None,
        }
    }

    #[must_use]
    pub const fn new_run(
        kind: PresentedPrimitiveKind,
        row_role: crate::GlyphRowRole,
        slot: DisplaySlotId,
        len: u32,
        clip: FrameRect,
    ) -> Self {
        Self {
            kind,
            row_role,
            slot,
            len,
            clip,
            hover: None,
            pressed: None,
        }
    }

    #[must_use]
    pub const fn with_modes(mut self, hover: PointerDrawMode, pressed: PointerDrawMode) -> Self {
        self.hover = Some(hover);
        self.pressed = Some(pressed);
        self
    }

    #[must_use]
    pub const fn kind(&self) -> PresentedPrimitiveKind {
        self.kind
    }

    #[must_use]
    pub const fn slot(&self) -> DisplaySlotId {
        self.slot
    }

    #[must_use]
    pub const fn row_role(&self) -> crate::GlyphRowRole {
        self.row_role
    }

    #[must_use]
    pub const fn len(&self) -> u32 {
        self.len
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub const fn hover(&self) -> Option<PointerDrawMode> {
        self.hover
    }

    #[must_use]
    pub const fn pressed(&self) -> Option<PointerDrawMode> {
        self.pressed
    }

    #[must_use]
    pub const fn clip(&self) -> FrameRect {
        self.clip
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PointerReliefMargins {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl PointerReliefMargins {
    #[must_use]
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    #[must_use]
    pub const fn left(self) -> f32 {
        self.left
    }
    #[must_use]
    pub const fn top(self) -> f32 {
        self.top
    }
    #[must_use]
    pub const fn right(self) -> f32 {
        self.right
    }
    #[must_use]
    pub const fn bottom(self) -> f32 {
        self.bottom
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PointerReliefEdges {
    top: bool,
    left: bool,
    bottom: bool,
    right: bool,
}

impl PointerReliefEdges {
    #[must_use]
    pub const fn new(top: bool, left: bool, bottom: bool, right: bool) -> Self {
        Self {
            top,
            left,
            bottom,
            right,
        }
    }

    #[must_use]
    pub const fn top(self) -> bool {
        self.top
    }
    #[must_use]
    pub const fn left(self) -> bool {
        self.left
    }
    #[must_use]
    pub const fn bottom(self) -> bool {
        self.bottom
    }
    #[must_use]
    pub const fn right(self) -> bool {
        self.right
    }
}

/// Fully resolved GNU-style corner erasure applied after image-relief edges.
/// The producer supplies the background color and geometry; the renderer only
/// executes this paint operation.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PointerReliefCornerErase {
    color: crate::Color,
    radius: f32,
    margin: f32,
}

impl PointerReliefCornerErase {
    #[must_use]
    pub const fn new(color: crate::Color, radius: f32, margin: f32) -> Self {
        Self {
            color,
            radius,
            margin,
        }
    }

    #[must_use]
    pub const fn color(self) -> crate::Color {
        self.color
    }

    #[must_use]
    pub const fn radius(self) -> f32 {
        self.radius
    }

    #[must_use]
    pub const fn margin(self) -> f32 {
        self.margin
    }
}

/// Fully resolved image-relief geometry and colors. Semantic raised/sunken
/// policy is resolved before this renderer-safe value enters the protocol.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PointerImageRelief {
    top_left_color: crate::Color,
    bottom_right_color: crate::Color,
    thickness: f32,
    margins: PointerReliefMargins,
    edges: PointerReliefEdges,
    corner_erase: PointerReliefCornerErase,
}

impl PointerImageRelief {
    #[must_use]
    pub const fn new(
        top_left_color: crate::Color,
        bottom_right_color: crate::Color,
        thickness: f32,
        margins: PointerReliefMargins,
        edges: PointerReliefEdges,
        corner_erase: PointerReliefCornerErase,
    ) -> Self {
        Self {
            top_left_color,
            bottom_right_color,
            thickness,
            margins,
            edges,
            corner_erase,
        }
    }

    #[must_use]
    pub const fn top_left_color(self) -> crate::Color {
        self.top_left_color
    }
    #[must_use]
    pub const fn bottom_right_color(self) -> crate::Color {
        self.bottom_right_color
    }
    #[must_use]
    pub const fn thickness(self) -> f32 {
        self.thickness
    }
    #[must_use]
    pub const fn margins(self) -> PointerReliefMargins {
        self.margins
    }
    #[must_use]
    pub const fn edges(self) -> PointerReliefEdges {
        self.edges
    }

    #[must_use]
    pub const fn corner_erase(self) -> PointerReliefCornerErase {
        self.corner_erase
    }
}

/// Renderer operation selected for a hovered or pressed appearance.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PointerDrawMode {
    Face(FaceId),
    ImageRelief(PointerImageRelief),
}

/// Paint behavior shared by one or more independent interaction regions.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct PresentedPointerAppearance {
    paint_spans: Vec<PresentedPaintSpan>,
    hover: PointerDrawMode,
    pressed: PointerDrawMode,
    #[serde(skip)]
    damage_bounds: Option<FrameRect>,
    #[serde(skip)]
    damage_rows: Vec<PresentedPointerDamageRow>,
}

impl PartialEq for PresentedPointerAppearance {
    fn eq(&self, other: &Self) -> bool {
        self.paint_spans == other.paint_spans
            && self.hover == other.hover
            && self.pressed == other.pressed
    }
}

/// One matrix row whose cached vertices can be affected by an appearance.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PresentedPointerDamageRow {
    window_id: crate::DisplayWindowId,
    row: u32,
}

impl PresentedPointerDamageRow {
    #[must_use]
    pub const fn new(window_id: crate::DisplayWindowId, row: u32) -> Self {
        Self { window_id, row }
    }

    #[must_use]
    pub const fn window_id(self) -> crate::DisplayWindowId {
        self.window_id
    }

    #[must_use]
    pub const fn row(self) -> u32 {
        self.row
    }
}

/// One appearance before its source slots become canonical primitive indices.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PresentedPointerSourceAppearance {
    paint_spans: Vec<PresentedSourcePaintSpan>,
    hover: PointerDrawMode,
    pressed: PointerDrawMode,
}

impl PresentedPointerSourceAppearance {
    #[must_use]
    pub fn new(
        paint_spans: Vec<PresentedSourcePaintSpan>,
        hover: PointerDrawMode,
        pressed: PointerDrawMode,
    ) -> Self {
        Self {
            paint_spans,
            hover,
            pressed,
        }
    }

    #[must_use]
    pub fn paint_spans(&self) -> &[PresentedSourcePaintSpan] {
        &self.paint_spans
    }

    #[must_use]
    pub const fn hover(&self) -> PointerDrawMode {
        self.hover
    }

    #[must_use]
    pub const fn pressed(&self) -> PointerDrawMode {
        self.pressed
    }
}

impl PresentedPointerAppearance {
    #[must_use]
    pub fn new(
        paint_spans: Vec<PresentedPaintSpan>,
        hover: PointerDrawMode,
        pressed: PointerDrawMode,
    ) -> Self {
        let mut appearance = Self {
            paint_spans,
            hover,
            pressed,
            damage_bounds: None,
            damage_rows: Vec::new(),
        };
        appearance.rebuild_damage_bounds();
        appearance
    }

    #[must_use]
    pub fn paint_spans(&self) -> &[PresentedPaintSpan] {
        &self.paint_spans
    }

    #[must_use]
    pub const fn hover(&self) -> PointerDrawMode {
        self.hover
    }

    #[must_use]
    pub const fn pressed(&self) -> PointerDrawMode {
        self.pressed
    }

    #[must_use]
    pub fn damage_bounds(&self) -> FrameRect {
        self.damage_bounds
            .expect("validated pointer appearance has paint bounds")
    }

    #[must_use]
    pub fn damage_rows(&self) -> &[PresentedPointerDamageRow] {
        &self.damage_rows
    }

    fn rebuild_damage_bounds(&mut self) {
        self.damage_bounds = self
            .paint_spans
            .iter()
            .map(PresentedPaintSpan::clip)
            .reduce(|left, right| {
                let x = left.x().min(right.x());
                let y = left.y().min(right.y());
                let right_edge = (left.x() + left.width()).max(right.x() + right.width());
                let bottom = (left.y() + left.height()).max(right.y() + right.height());
                FrameRect::new(x, y, right_edge - x, bottom - y)
                    .expect("union of validated pointer clips is valid")
            });
    }
}

/// Hit geometry, evaluator-owned click meaning, and renderer-owned appearance.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PresentedPointerRegion {
    owner: Option<PresentedRegionId>,
    bounds: FrameRect,
    interaction: Option<InteractionId>,
    appearance: Option<PointerAppearanceId>,
}

/// Protocol-safe pointer metadata awaiting the one canonical materialization pass.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PresentedPointerSourceMap {
    regions: Vec<PresentedPointerRegion>,
    appearances: Vec<PresentedPointerSourceAppearance>,
}

impl PresentedPointerSourceMap {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            regions: Vec::new(),
            appearances: Vec::new(),
        }
    }

    #[must_use]
    pub fn new(
        regions: Vec<PresentedPointerRegion>,
        appearances: Vec<PresentedPointerSourceAppearance>,
    ) -> Self {
        Self {
            regions,
            appearances,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty() && self.appearances.is_empty()
    }

    #[must_use]
    pub fn regions(&self) -> &[PresentedPointerRegion] {
        &self.regions
    }

    #[must_use]
    pub fn appearances(&self) -> &[PresentedPointerSourceAppearance] {
        &self.appearances
    }

    /// Append another source-addressed adapter, remapping only its local
    /// appearance IDs.  This lets independent layout producers (buffer text,
    /// frame chrome) share the one canonical primitive materialization pass.
    pub fn append(
        &mut self,
        other: PresentedPointerSourceMap,
    ) -> Result<(), PresentedPointerMapError> {
        let offset = self.appearances.len();
        let new_len = offset
            .checked_add(other.appearances.len())
            .ok_or(PresentedPointerMapError::PaintSpanOutOfRange)?;
        if new_len > u32::MAX as usize + 1 {
            return Err(PresentedPointerMapError::PaintSpanOutOfRange);
        }
        let mut remapped_regions = Vec::with_capacity(other.regions.len());
        for region in &other.regions {
            let appearance = region
                .appearance
                .map(|id| {
                    let local = usize::try_from(id.get())
                        .map_err(|_| PresentedPointerMapError::PaintSpanOutOfRange)?;
                    if local >= other.appearances.len() {
                        return Err(PresentedPointerMapError::UnknownAppearance(id));
                    }
                    PointerAppearanceId::try_from(offset + local)
                        .map_err(|_| PresentedPointerMapError::PaintSpanOutOfRange)
                })
                .transpose()?;
            let mut remapped =
                PresentedPointerRegion::new(region.bounds, region.interaction, appearance);
            remapped.owner = region.owner;
            remapped_regions.push(remapped);
        }
        self.appearances.extend(other.appearances);
        self.regions.extend(remapped_regions);
        Ok(())
    }

    pub(crate) fn resolve_against(
        &self,
        frame: &FrameGlyphBuffer,
    ) -> Result<
        (Vec<PresentedPointerRegion>, Vec<PresentedPointerAppearance>),
        PresentedPointerMapError,
    > {
        let referenced = self
            .appearances
            .iter()
            .flat_map(|appearance| appearance.paint_spans.iter())
            .map(|span| (span.kind, span.row_role, span.slot))
            .collect::<std::collections::HashSet<_>>();
        let mut primitive_index = std::collections::HashMap::with_capacity(referenced.len());
        for (index, primitive) in frame.glyphs.iter().enumerate() {
            let Some(slot) = primitive.slot_id() else {
                continue;
            };
            let Some(row_role) = primitive.row_role() else {
                continue;
            };
            let kind = match primitive {
                FrameGlyph::Char { .. } | FrameGlyph::Stretch { .. } => {
                    PresentedPrimitiveKind::Glyph
                }
                FrameGlyph::Image { .. } => PresentedPrimitiveKind::Image,
                _ => continue,
            };
            if !referenced.contains(&(kind, row_role, slot)) {
                continue;
            }
            if let Some(previous_index) = primitive_index.insert((kind, row_role, slot), index) {
                tracing::error!(
                    ?kind,
                    ?row_role,
                    ?slot,
                    previous_index,
                    index,
                    previous = ?frame.glyphs[previous_index],
                    duplicate = ?primitive,
                    "presented pointer source identity resolves to multiple materialized primitives"
                );
                return Err(PresentedPointerMapError::DuplicateSourceIdentity {
                    kind,
                    row_role,
                    slot,
                });
            }
        }
        let mut resolved_appearances = Vec::new();
        let mut appearance_remap = Vec::with_capacity(self.appearances.len());
        for appearance in &self.appearances {
            let mut seen = std::collections::HashSet::new();
            let mut paint_spans: Vec<PresentedPaintSpan> = Vec::new();
            for source_span in &appearance.paint_spans {
                let Some(&index) = primitive_index.get(&(
                    source_span.kind,
                    source_span.row_role,
                    source_span.slot,
                )) else {
                    continue;
                };
                if !seen.insert(index) {
                    continue;
                }
                let first = u32::try_from(index)
                    .map_err(|_| PresentedPointerMapError::PaintSpanOutOfRange)?;
                let len = source_span.len;
                let end = index
                    .checked_add(len as usize)
                    .ok_or(PresentedPointerMapError::PaintSpanOutOfRange)?;
                let primitives = frame
                    .glyphs
                    .get(index..end)
                    .ok_or(PresentedPointerMapError::PaintSpanOutOfRange)?;
                let compatible = primitives.iter().all(|primitive| {
                    let same_source_row = primitive.slot_id().is_some_and(|slot| {
                        slot.window_id == source_span.slot.window_id
                            && slot.row == source_span.slot.row
                    }) && primitive.row_role() == Some(source_span.row_role);
                    same_source_row
                        && match source_span.kind {
                            PresentedPrimitiveKind::Glyph => matches!(
                                primitive,
                                FrameGlyph::Char { .. } | FrameGlyph::Stretch { .. }
                            ),
                            PresentedPrimitiveKind::Image => {
                                matches!(primitive, FrameGlyph::Image { .. })
                            }
                        }
                });
                if !compatible {
                    return Err(PresentedPointerMapError::PrimitiveKindMismatch);
                }
                let mut next =
                    PresentedPaintSpan::new(source_span.kind, first, len, source_span.clip);
                if let (Some(hover), Some(pressed)) = (source_span.hover, source_span.pressed) {
                    next = next.with_modes(hover, pressed);
                }
                if let Some(previous) = paint_spans.last_mut()
                    && previous.kind == next.kind
                    && previous.clip == next.clip
                    && previous.hover == next.hover
                    && previous.pressed == next.pressed
                    && previous.first + previous.len == next.first
                {
                    previous.len = previous
                        .len
                        .checked_add(next.len)
                        .ok_or(PresentedPointerMapError::PaintSpanOutOfRange)?;
                } else {
                    paint_spans.push(next);
                }
            }
            if paint_spans.is_empty() {
                appearance_remap.push(None);
            } else {
                let id = PointerAppearanceId::try_from(resolved_appearances.len())
                    .map_err(|_| PresentedPointerMapError::PaintSpanOutOfRange)?;
                appearance_remap.push(Some(id));
                resolved_appearances.push(PresentedPointerAppearance::new(
                    paint_spans,
                    appearance.hover,
                    appearance.pressed,
                ));
            }
        }

        let mut regions = Vec::with_capacity(self.regions.len());
        for region in &self.regions {
            let appearance = if let Some(id) = region.appearance {
                let index = usize::try_from(id.get())
                    .map_err(|_| PresentedPointerMapError::UnknownAppearance(id))?;
                *appearance_remap
                    .get(index)
                    .ok_or(PresentedPointerMapError::UnknownAppearance(id))?
            } else {
                None
            };
            if region.interaction.is_none() && appearance.is_none() {
                continue;
            }
            let mut resolved =
                PresentedPointerRegion::new(region.bounds, region.interaction, appearance);
            resolved.owner = region.owner;
            regions.push(resolved);
        }
        Ok((regions, resolved_appearances))
    }
}

impl PresentedPointerRegion {
    #[must_use]
    pub const fn new(
        bounds: FrameRect,
        interaction: Option<InteractionId>,
        appearance: Option<PointerAppearanceId>,
    ) -> Self {
        Self {
            owner: None,
            bounds,
            interaction,
            appearance,
        }
    }

    #[must_use]
    pub const fn new_owned(
        owner: PresentedRegionId,
        bounds: FrameRect,
        interaction: Option<InteractionId>,
        appearance: Option<PointerAppearanceId>,
    ) -> Self {
        Self {
            owner: Some(owner),
            bounds,
            interaction,
            appearance,
        }
    }

    #[must_use]
    pub const fn owner(&self) -> Option<PresentedRegionId> {
        self.owner
    }

    #[must_use]
    pub const fn bounds(&self) -> FrameRect {
        self.bounds
    }

    #[must_use]
    pub const fn interaction(&self) -> Option<InteractionId> {
        self.interaction
    }

    #[must_use]
    pub const fn appearance(&self) -> Option<PointerAppearanceId> {
        self.appearance
    }
}

/// Cross-field limits supplied by the completed presentation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PointerMapValidationContext<'a> {
    frame_buffer: &'a FrameGlyphBuffer,
}

impl<'a> PointerMapValidationContext<'a> {
    pub(crate) fn from_frame_buffer(
        frame_buffer: &'a FrameGlyphBuffer,
    ) -> Result<Self, PresentedPointerMapError> {
        FrameSize::new(frame_buffer.width, frame_buffer.height)
            .map_err(|_| PresentedPointerMapError::InvalidFrameGeometry)?;
        Ok(Self { frame_buffer })
    }

    fn frame(self) -> FrameSize {
        FrameSize::new(self.frame_buffer.width, self.frame_buffer.height)
            .expect("validation context checked frame dimensions")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentedPointerMapError {
    Semantic(PresentedHitError),
    UnknownAppearance(PointerAppearanceId),
    MissingRegionBehavior,
    EmptyAppearance,
    EmptyPaintSpan,
    OverlappingPaintSpans,
    PaintSpanOutOfRange,
    InvalidRegionGeometry,
    InvalidClipGeometry,
    InvalidFrameGeometry,
    RegionOutsideFrame,
    ClipOutsideFrame,
    PrimitiveKindMismatch,
    DuplicateSourceIdentity {
        kind: PresentedPrimitiveKind,
        row_role: crate::GlyphRowRole,
        slot: DisplaySlotId,
    },
    UnknownFace(FaceId),
    InvalidImageRelief,
    IncompleteSpanModes,
}

impl From<PresentedHitError> for PresentedPointerMapError {
    fn from(error: PresentedHitError) -> Self {
        Self::Semantic(error)
    }
}

impl std::fmt::Display for PresentedPointerMapError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid presented pointer map: {self:?}")
    }
}

impl std::error::Error for PresentedPointerMapError {}

/// Intrinsically valid pointer metadata for one immutable presentation.
///
/// Deserialization validates internal geometry, indices, and references only.
/// Renderer-safe contextual validity is established atomically when the map is
/// installed through [`FrameGlyphBuffer::install_presented_pointer_map`].
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct PresentedPointerMap {
    regions: Vec<PresentedPointerRegion>,
    appearances: Vec<PresentedPointerAppearance>,
    #[serde(skip)]
    row_buckets: Vec<PointerRowBucket>,
}

#[derive(Clone, Debug, PartialEq)]
struct PointerRowBucket {
    top: f32,
    bottom: f32,
    prefix_max_bottom: f32,
    candidates: Vec<usize>,
    prefix_max_right: Vec<f32>,
}

impl PresentedPointerMap {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            regions: Vec::new(),
            appearances: Vec::new(),
            row_buckets: Vec::new(),
        }
    }

    pub(crate) fn from_parts(
        regions: Vec<PresentedPointerRegion>,
        appearances: Vec<PresentedPointerAppearance>,
    ) -> Result<Self, PresentedPointerMapError> {
        let mut map = Self {
            regions,
            appearances,
            row_buckets: Vec::new(),
        };
        map.validate_intrinsic()?;
        map.rebuild_damage_bounds();
        map.rebuild_hit_index();
        Ok(map)
    }

    /// Revalidates snapshot-dependent references after transport.
    pub(crate) fn validate_against(
        &self,
        context: PointerMapValidationContext<'_>,
    ) -> Result<(), PresentedPointerMapError> {
        let frame = context.frame();
        for region in &self.regions {
            if !rect_is_within_frame(region.bounds, frame) {
                return Err(PresentedPointerMapError::RegionOutsideFrame);
            }
        }

        for appearance in &self.appearances {
            validate_mode(appearance.hover, context.frame_buffer)?;
            validate_mode(appearance.pressed, context.frame_buffer)?;
            for span in &appearance.paint_spans {
                if span.hover.is_some() != span.pressed.is_some() {
                    return Err(PresentedPointerMapError::IncompleteSpanModes);
                }
                if let Some(hover) = span.hover {
                    validate_mode(hover, context.frame_buffer)?;
                }
                if let Some(pressed) = span.pressed {
                    validate_mode(pressed, context.frame_buffer)?;
                }
                if !rect_is_within_frame(span.clip, frame) {
                    return Err(PresentedPointerMapError::ClipOutsideFrame);
                }
                let end = span
                    .first
                    .checked_add(span.len)
                    .ok_or(PresentedPointerMapError::PaintSpanOutOfRange)?;
                let (Ok(first), Ok(end)) = (usize::try_from(span.first), usize::try_from(end))
                else {
                    return Err(PresentedPointerMapError::PaintSpanOutOfRange);
                };
                let Some(primitives) = context.frame_buffer.glyphs.get(first..end) else {
                    return Err(PresentedPointerMapError::PaintSpanOutOfRange);
                };
                let matches_kind = match span.kind {
                    PresentedPrimitiveKind::Glyph => primitives.iter().all(|primitive| {
                        matches!(
                            primitive,
                            FrameGlyph::Char { .. } | FrameGlyph::Stretch { .. }
                        )
                    }),
                    PresentedPrimitiveKind::Image => primitives
                        .iter()
                        .all(|primitive| matches!(primitive, FrameGlyph::Image { .. })),
                };
                if !matches_kind {
                    return Err(PresentedPointerMapError::PrimitiveKindMismatch);
                }
            }
        }

        Ok(())
    }

    pub(crate) fn rebuild_damage_index(&mut self, frame: &FrameGlyphBuffer) {
        for appearance in &mut self.appearances {
            appearance.rebuild_damage_bounds();
            appearance.damage_rows.clear();
            for span in &appearance.paint_spans {
                let first = span.first as usize;
                let end = first + span.len as usize;
                for primitive in &frame.glyphs[first..end] {
                    let slot = match primitive {
                        FrameGlyph::Char { slot_id, .. } | FrameGlyph::Stretch { slot_id, .. } => {
                            Some(*slot_id)
                        }
                        FrameGlyph::Image {
                            slot_id: Some(slot_id),
                            ..
                        } => Some(*slot_id),
                        _ => None,
                    };
                    if let Some(slot) = slot {
                        let row = PresentedPointerDamageRow::new(slot.window_id, slot.row);
                        appearance.damage_rows.push(row);
                    }
                }
            }
            appearance
                .damage_rows
                .sort_unstable_by_key(|row| (row.window_id().get(), row.row()));
            appearance.damage_rows.dedup();
        }
    }

    fn rebuild_damage_bounds(&mut self) {
        for appearance in &mut self.appearances {
            appearance.rebuild_damage_bounds();
        }
    }

    fn validate_intrinsic(&self) -> Result<(), PresentedPointerMapError> {
        for region in &self.regions {
            if !rect_has_valid_geometry(region.bounds) {
                return Err(PresentedPointerMapError::InvalidRegionGeometry);
            }
            if region.interaction.is_none() && region.appearance.is_none() {
                return Err(PresentedPointerMapError::MissingRegionBehavior);
            }
            if let Some(appearance) = region.appearance
                && usize::try_from(appearance.get())
                    .map_or(true, |index| index >= self.appearances.len())
            {
                return Err(PresentedPointerMapError::UnknownAppearance(appearance));
            }
        }

        for appearance in &self.appearances {
            if appearance.paint_spans.is_empty() {
                return Err(PresentedPointerMapError::EmptyAppearance);
            }
            for span in &appearance.paint_spans {
                if span.hover.is_some() != span.pressed.is_some() {
                    return Err(PresentedPointerMapError::IncompleteSpanModes);
                }
                if span.len == 0 {
                    return Err(PresentedPointerMapError::EmptyPaintSpan);
                }
                if !rect_has_valid_geometry(span.clip) {
                    return Err(PresentedPointerMapError::InvalidClipGeometry);
                }
                if span.first.checked_add(span.len).is_none() {
                    return Err(PresentedPointerMapError::PaintSpanOutOfRange);
                }
            }
            let mut intervals = appearance
                .paint_spans
                .iter()
                .map(|span| (span.first, span.first + span.len))
                .collect::<Vec<_>>();
            intervals.sort_unstable_by_key(|&(first, end)| (first, end));
            for pair in intervals.windows(2) {
                if pair[1].0 < pair[0].1 {
                    return Err(PresentedPointerMapError::OverlappingPaintSpans);
                }
            }
        }

        Ok(())
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty() && self.appearances.is_empty()
    }

    #[must_use]
    pub fn regions(&self) -> &[PresentedPointerRegion] {
        &self.regions
    }

    #[must_use]
    pub fn appearances(&self) -> &[PresentedPointerAppearance] {
        &self.appearances
    }

    #[must_use]
    pub fn appearance(&self, id: PointerAppearanceId) -> Option<&PresentedPointerAppearance> {
        usize::try_from(id.get())
            .ok()
            .and_then(|index| self.appearances.get(index))
    }

    /// Returns the first published region containing `(x, y)`.
    ///
    /// Rectangle edges are half-open, matching frame chrome hit testing. Input
    /// order defines stable priority if producers publish overlapping regions.
    #[must_use]
    pub fn hit_test(&self, x: f32, y: f32) -> Option<&PresentedPointerRegion> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let mut bucket_end = self.row_buckets.partition_point(|bucket| bucket.top <= y);
        let mut best = None;
        while bucket_end > 0 {
            let bucket_index = bucket_end - 1;
            let bucket = &self.row_buckets[bucket_index];
            if bucket.prefix_max_bottom <= y {
                break;
            }
            if y < bucket.bottom {
                let mut candidate_end = bucket
                    .candidates
                    .partition_point(|&index| self.regions[index].bounds.x() <= x);
                while candidate_end > 0 {
                    let candidate_position = candidate_end - 1;
                    if bucket.prefix_max_right[candidate_position] <= x {
                        break;
                    }
                    let region_index = bucket.candidates[candidate_position];
                    let bounds = self.regions[region_index].bounds;
                    if x < bounds.x() + bounds.width() {
                        best = Some(
                            best.map_or(region_index, |current: usize| current.min(region_index)),
                        );
                    }
                    candidate_end -= 1;
                }
            }
            bucket_end -= 1;
        }
        best.map(|index| &self.regions[index])
    }

    fn rebuild_hit_index(&mut self) {
        let mut entries: Vec<_> = self
            .regions
            .iter()
            .enumerate()
            .map(|(index, region)| {
                let bounds = region.bounds;
                (bounds.y(), bounds.y() + bounds.height(), index)
            })
            .collect();
        entries.sort_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then(left.1.total_cmp(&right.1))
                .then(left.2.cmp(&right.2))
        });

        self.row_buckets.clear();
        for (top, bottom, region_index) in entries {
            let starts_new_bucket = self.row_buckets.last().is_none_or(|bucket| {
                bucket.top.total_cmp(&top).is_ne() || bucket.bottom.total_cmp(&bottom).is_ne()
            });
            if starts_new_bucket {
                self.row_buckets.push(PointerRowBucket {
                    top,
                    bottom,
                    prefix_max_bottom: bottom,
                    candidates: Vec::new(),
                    prefix_max_right: Vec::new(),
                });
            }
            self.row_buckets
                .last_mut()
                .expect("bucket was just created")
                .candidates
                .push(region_index);
        }

        let mut prefix_max_bottom = 0.0_f32;
        for bucket in &mut self.row_buckets {
            prefix_max_bottom = prefix_max_bottom.max(bucket.bottom);
            bucket.prefix_max_bottom = prefix_max_bottom;
            bucket.candidates.sort_by(|&left, &right| {
                self.regions[left]
                    .bounds
                    .x()
                    .total_cmp(&self.regions[right].bounds.x())
                    .then(left.cmp(&right))
            });
            let mut prefix_max_right = 0.0_f32;
            bucket.prefix_max_right = bucket
                .candidates
                .iter()
                .map(|&index| {
                    let bounds = self.regions[index].bounds;
                    prefix_max_right = prefix_max_right.max(bounds.x() + bounds.width());
                    prefix_max_right
                })
                .collect();
        }
    }

    #[cfg(test)]
    pub(crate) fn hit_test_candidate_count(&self, y: f32) -> usize {
        self.row_buckets
            .iter()
            .filter(|bucket| bucket.top <= y && y < bucket.bottom)
            .map(|bucket| bucket.candidates.len())
            .sum()
    }

    #[cfg(test)]
    pub(crate) fn hit_index_entry_count(&self) -> usize {
        self.row_buckets
            .iter()
            .map(|bucket| bucket.candidates.len())
            .sum()
    }
}

#[derive(serde::Deserialize)]
struct RawPresentedPointerMap {
    regions: Vec<PresentedPointerRegion>,
    appearances: Vec<PresentedPointerAppearance>,
}

impl<'de> serde::Deserialize<'de> for PresentedPointerMap {
    fn deserialize<Deserializer>(deserializer: Deserializer) -> Result<Self, Deserializer::Error>
    where
        Deserializer: serde::Deserializer<'de>,
    {
        let raw = <RawPresentedPointerMap as serde::Deserialize>::deserialize(deserializer)?;
        let mut map = Self {
            regions: raw.regions,
            appearances: raw.appearances,
            row_buckets: Vec::new(),
        };
        map.validate_intrinsic().map_err(serde::de::Error::custom)?;
        map.rebuild_damage_bounds();
        map.rebuild_hit_index();
        Ok(map)
    }
}

fn validate_mode(
    mode: PointerDrawMode,
    frame_buffer: &FrameGlyphBuffer,
) -> Result<(), PresentedPointerMapError> {
    match mode {
        PointerDrawMode::Face(face_id) if !frame_buffer.faces.contains_key(&face_id) => {
            return Err(PresentedPointerMapError::UnknownFace(face_id));
        }
        PointerDrawMode::ImageRelief(relief) if !image_relief_is_valid(relief) => {
            return Err(PresentedPointerMapError::InvalidImageRelief);
        }
        PointerDrawMode::Face(_) | PointerDrawMode::ImageRelief(_) => {}
    }
    Ok(())
}

fn image_relief_is_valid(relief: PointerImageRelief) -> bool {
    let corner_erase = relief.corner_erase();
    let colors = [
        relief.top_left_color(),
        relief.bottom_right_color(),
        corner_erase.color(),
    ];
    let margins = relief.margins();
    colors.into_iter().all(|color| {
        [color.r, color.g, color.b, color.a]
            .into_iter()
            .all(f32::is_finite)
    }) && relief.thickness().is_finite()
        && relief.thickness() >= 0.0
        && corner_erase.radius().is_finite()
        && corner_erase.radius() > 0.0
        && corner_erase.margin().is_finite()
        && corner_erase.margin() >= 0.0
        && [
            margins.left(),
            margins.top(),
            margins.right(),
            margins.bottom(),
        ]
        .into_iter()
        .all(|margin| margin.is_finite() && margin >= 0.0)
}

fn rect_is_within_frame(rect: FrameRect, frame: FrameSize) -> bool {
    rect_has_valid_geometry(rect)
        && rect.x() + rect.width() <= frame.width()
        && rect.y() + rect.height() <= frame.height()
}

fn rect_has_valid_geometry(rect: FrameRect) -> bool {
    rect.x().is_finite()
        && rect.y().is_finite()
        && rect.width().is_finite()
        && rect.height().is_finite()
        && rect.x() >= 0.0
        && rect.y() >= 0.0
        && rect.width() >= 0.0
        && rect.height() >= 0.0
        && (rect.x() + rect.width()).is_finite()
        && (rect.y() + rect.height()).is_finite()
}

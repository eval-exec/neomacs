//! Coordinate-safe frame-level chrome layout.
//!
//! A frame chrome band owns one absolute frame rectangle. Everything inside
//! the band is band-local and can be translated to frame coordinates only via
//! [`FrameRect::place`].

use crate::frame_glyphs::GlyphRowRole;
use crate::geometry::{BandSpace, FrameSpace, LayoutRect, SpaceTranslation};
use crate::glyph_matrix::GlyphRow;
use crate::types::{Color, Rect};
use crate::ui_types::{MenuBarItem, ToolBarItem};

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FrameSize {
    width: f32,
    height: f32,
}

impl FrameSize {
    pub fn new(width: f32, height: f32) -> Result<Self, ChromeLayoutError> {
        if !valid_extent(width) || !valid_extent(height) {
            return Err(ChromeLayoutError::InvalidFrameSize);
        }
        Ok(Self { width, height })
    }

    pub fn width(self) -> f32 {
        self.width
    }

    pub fn height(self) -> f32 {
        self.height
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct FrameRect(Rect);

impl FrameRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Result<Self, ChromeLayoutError> {
        if !valid_origin(x) || !valid_origin(y) || !valid_extent(width) || !valid_extent(height) {
            return Err(ChromeLayoutError::InvalidRect);
        }
        Ok(Self(Rect::new(x, y, width, height)))
    }

    pub fn x(self) -> f32 {
        self.0.x
    }

    pub fn y(self) -> f32 {
        self.0.y
    }

    pub fn width(self) -> f32 {
        self.0.width
    }

    pub fn height(self) -> f32 {
        self.0.height
    }

    pub fn bottom(self) -> f32 {
        self.y() + self.height()
    }

    pub fn raw(self) -> Rect {
        self.0
    }

    pub fn place(self, local: BandRect) -> Result<Self, ChromeLayoutError> {
        let local = local.raw();
        if local.x + local.width > self.width() || local.y + local.height > self.height() {
            return Err(ChromeLayoutError::ContentExceedsBand);
        }
        let local = LayoutRect::<BandSpace>::from_px(local.x, local.y, local.width, local.height);
        let placed =
            SpaceTranslation::<BandSpace, FrameSpace>::from_px(self.x(), self.y()).map_rect(local);
        Self::new(
            placed.x().to_px(),
            placed.y().to_px(),
            placed.width().to_px(),
            placed.height().to_px(),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct BandRect(Rect);

impl BandRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Result<Self, ChromeLayoutError> {
        if !valid_origin(x) || !valid_origin(y) || !valid_extent(width) || !valid_extent(height) {
            return Err(ChromeLayoutError::InvalidRect);
        }
        Ok(Self(Rect::new(x, y, width, height)))
    }

    pub fn raw(self) -> Rect {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FramePoint {
    x: f32,
    y: f32,
}

impl FramePoint {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

fn valid_origin(value: f32) -> bool {
    value.is_finite() && value >= 0.0
}

fn valid_extent(value: f32) -> bool {
    value.is_finite() && value >= 0.0
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FrameChromeKind {
    MenuBar,
    ToolBar,
    CompactBar,
    TabBar,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ChromeBandId(u32);

impl ChromeBandId {
    fn from_position(position: usize) -> Self {
        Self(position as u32)
    }
}

/// Identifies the immutable evaluator presentation that produced a frame.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct PresentationId(u64);

impl PresentationId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Opaque evaluator-owned meaning for one interactive region in a presentation.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct InteractionId(u32);

impl InteractionId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ChromeAction {
    OpenMenu { index: u32, key: String },
    InvokeToolBarItem { index: u32 },
    Presented { interaction: InteractionId },
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChromeHitRegion {
    local_bounds: BandRect,
    action: ChromeAction,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MaterializedChromeHitRegion {
    bounds: FrameRect,
    action: ChromeAction,
}

impl MaterializedChromeHitRegion {
    pub fn bounds(&self) -> FrameRect {
        self.bounds
    }

    pub fn action(&self) -> &ChromeAction {
        &self.action
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PositionedChromeItem<T> {
    local_bounds: BandRect,
    item: T,
    action: Option<ChromeAction>,
}

impl<T> PositionedChromeItem<T> {
    pub fn new(local_bounds: BandRect, item: T, action: ChromeAction) -> Self {
        Self {
            local_bounds,
            item,
            action: Some(action),
        }
    }

    pub fn decorative(local_bounds: BandRect, item: T) -> Self {
        Self {
            local_bounds,
            item,
            action: None,
        }
    }

    pub fn local_bounds(&self) -> BandRect {
        self.local_bounds
    }

    pub fn item(&self) -> &T {
        &self.item
    }

    pub fn action(&self) -> Option<&ChromeAction> {
        self.action.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MenuBarContent {
    items: Vec<PositionedChromeItem<MenuBarItem>>,
    foreground: Color,
    background: Color,
    terminal_style: Option<TerminalMenuBarStyle>,
}

/// The `menu` face as a TERMINAL writes it.
///
/// The colours are the realized terminal colours, not pixels: GNU's
/// `turn_on_face` (src/term.c:2093-2117) writes the index the realized face
/// carries and nothing else, and `None` is GNU's `FACE_TTY_DEFAULT_FG_COLOR` --
/// a slot `face_tty_specified_color` (src/dispextern.h:1933-1936) rejects, so
/// no colour is emitted at all.  A separate "use the default" flag beside a
/// pixel would say the same thing twice.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TerminalMenuBarStyle {
    pub fg: Option<crate::terminal_color::TerminalColor>,
    pub bg: Option<crate::terminal_color::TerminalColor>,
    pub bold: bool,
    pub inverse: bool,
}

impl MenuBarContent {
    pub fn new(
        items: Vec<PositionedChromeItem<MenuBarItem>>,
        foreground: Color,
        background: Color,
    ) -> Self {
        Self {
            items,
            foreground,
            background,
            terminal_style: None,
        }
    }

    pub fn with_terminal_style(mut self, style: TerminalMenuBarStyle) -> Self {
        self.terminal_style = Some(style);
        self
    }

    pub fn empty() -> Self {
        Self::new(Vec::new(), Color::WHITE, Color::BLACK)
    }

    pub fn items(&self) -> &[PositionedChromeItem<MenuBarItem>] {
        &self.items
    }

    pub fn foreground(&self) -> Color {
        self.foreground
    }

    pub fn background(&self) -> Color {
        self.background
    }

    pub fn terminal_style(&self) -> Option<TerminalMenuBarStyle> {
        self.terminal_style
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToolBarContent {
    items: Vec<PositionedChromeItem<ToolBarItem>>,
    foreground: Color,
    background: Color,
    icon_size: u32,
    padding: u32,
}

impl ToolBarContent {
    pub fn new(
        items: Vec<PositionedChromeItem<ToolBarItem>>,
        foreground: Color,
        background: Color,
        icon_size: u32,
        padding: u32,
    ) -> Self {
        Self {
            items,
            foreground,
            background,
            icon_size,
            padding,
        }
    }

    pub fn empty() -> Self {
        Self::new(Vec::new(), Color::WHITE, Color::BLACK, 1, 0)
    }

    pub fn items(&self) -> &[PositionedChromeItem<ToolBarItem>] {
        &self.items
    }

    pub fn foreground(&self) -> Color {
        self.foreground
    }

    pub fn background(&self) -> Color {
        self.background
    }

    pub fn icon_size(&self) -> u32 {
        self.icon_size
    }

    pub fn padding(&self) -> u32 {
        self.padding
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompactBarContent {
    menu_items: Vec<PositionedChromeItem<MenuBarItem>>,
    tool_items: Vec<PositionedChromeItem<ToolBarItem>>,
    menu_foreground: Color,
    menu_background: Color,
    tool_foreground: Color,
    tool_background: Color,
    icon_size: u32,
    padding: u32,
}

impl CompactBarContent {
    pub fn new(
        menu_items: Vec<PositionedChromeItem<MenuBarItem>>,
        tool_items: Vec<PositionedChromeItem<ToolBarItem>>,
        menu_foreground: Color,
        menu_background: Color,
        tool_foreground: Color,
        tool_background: Color,
        icon_size: u32,
        padding: u32,
    ) -> Self {
        Self {
            menu_items,
            tool_items,
            menu_foreground,
            menu_background,
            tool_foreground,
            tool_background,
            icon_size,
            padding,
        }
    }

    pub fn empty() -> Self {
        Self::new(
            Vec::new(),
            Vec::new(),
            Color::WHITE,
            Color::BLACK,
            Color::WHITE,
            Color::BLACK,
            1,
            0,
        )
    }

    pub fn menu_items(&self) -> &[PositionedChromeItem<MenuBarItem>] {
        &self.menu_items
    }

    pub fn tool_items(&self) -> &[PositionedChromeItem<ToolBarItem>] {
        &self.tool_items
    }

    pub fn menu_foreground(&self) -> Color {
        self.menu_foreground
    }

    pub fn menu_background(&self) -> Color {
        self.menu_background
    }

    pub fn tool_foreground(&self) -> Color {
        self.tool_foreground
    }

    pub fn tool_background(&self) -> Color {
        self.tool_background
    }

    pub fn icon_size(&self) -> u32 {
        self.icon_size
    }

    pub fn padding(&self) -> u32 {
        self.padding
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChromeDisplayRow {
    row: GlyphRow,
}

impl ChromeDisplayRow {
    pub fn new(mut row: GlyphRow) -> Self {
        row.pixel_y = 0.0;
        Self { row }
    }

    pub fn empty_tab_bar() -> Self {
        Self::new(GlyphRow::new(GlyphRowRole::TabBar))
    }

    pub fn row(&self) -> &GlyphRow {
        &self.row
    }
}

impl ChromeHitRegion {
    pub fn new(local_bounds: BandRect, action: ChromeAction) -> Self {
        Self {
            local_bounds,
            action,
        }
    }

    pub fn local_bounds(&self) -> BandRect {
        self.local_bounds
    }

    pub fn action(&self) -> &ChromeAction {
        &self.action
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FrameChromeContent {
    DisplayRow(ChromeDisplayRow),
    MenuBar(MenuBarContent),
    ToolBar(ToolBarContent),
    CompactBar(CompactBarContent),
}

impl FrameChromeContent {
    pub fn kind(&self) -> FrameChromeKind {
        match self {
            Self::DisplayRow(_) => FrameChromeKind::TabBar,
            Self::MenuBar(_) => FrameChromeKind::MenuBar,
            Self::ToolBar(_) => FrameChromeKind::ToolBar,
            Self::CompactBar(_) => FrameChromeKind::CompactBar,
        }
    }

    fn validate_in(&self, bounds: FrameRect) -> Result<(), ChromeLayoutError> {
        match self {
            Self::DisplayRow(_) => {}
            Self::MenuBar(content) => {
                for item in content.items() {
                    bounds.place(item.local_bounds())?;
                }
            }
            Self::ToolBar(content) => {
                for item in content.items() {
                    bounds.place(item.local_bounds())?;
                }
            }
            Self::CompactBar(content) => {
                for item in content.menu_items() {
                    bounds.place(item.local_bounds())?;
                }
                for item in content.tool_items() {
                    bounds.place(item.local_bounds())?;
                }
            }
        }
        Ok(())
    }

    fn semantic_hit_regions(&self) -> Vec<ChromeHitRegion> {
        let positioned = |bounds: BandRect, action: Option<&ChromeAction>| {
            action
                .cloned()
                .map(|action| ChromeHitRegion::new(bounds, action))
        };
        match self {
            Self::DisplayRow(_) => Vec::new(),
            Self::MenuBar(content) => content
                .items()
                .iter()
                .filter_map(|item| positioned(item.local_bounds(), item.action()))
                .collect(),
            Self::ToolBar(content) => content
                .items()
                .iter()
                .filter_map(|item| positioned(item.local_bounds(), item.action()))
                .collect(),
            Self::CompactBar(content) => content
                .menu_items()
                .iter()
                .filter_map(|item| positioned(item.local_bounds(), item.action()))
                .chain(
                    content
                        .tool_items()
                        .iter()
                        .filter_map(|item| positioned(item.local_bounds(), item.action())),
                )
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChromeBandRequest {
    kind: FrameChromeKind,
    height: f32,
    content: FrameChromeContent,
    hit_regions: Vec<ChromeHitRegion>,
}

impl ChromeBandRequest {
    pub fn empty(kind: FrameChromeKind, height: f32) -> Self {
        let content = match kind {
            FrameChromeKind::MenuBar => FrameChromeContent::MenuBar(MenuBarContent::empty()),
            FrameChromeKind::ToolBar => FrameChromeContent::ToolBar(ToolBarContent::empty()),
            FrameChromeKind::CompactBar => {
                FrameChromeContent::CompactBar(CompactBarContent::empty())
            }
            FrameChromeKind::TabBar => {
                FrameChromeContent::DisplayRow(ChromeDisplayRow::empty_tab_bar())
            }
        };
        Self::new(kind, height, content)
    }

    pub fn new(kind: FrameChromeKind, height: f32, content: FrameChromeContent) -> Self {
        let hit_regions = content.semantic_hit_regions();
        Self {
            kind,
            height,
            content,
            hit_regions,
        }
    }

    pub fn with_hit_regions(mut self, hit_regions: Vec<ChromeHitRegion>) -> Self {
        self.hit_regions = hit_regions;
        self
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FrameChromeBand {
    id: ChromeBandId,
    kind: FrameChromeKind,
    bounds: FrameRect,
    content: FrameChromeContent,
    hit_regions: Vec<ChromeHitRegion>,
}

impl FrameChromeBand {
    pub fn id(&self) -> ChromeBandId {
        self.id
    }

    pub fn kind(&self) -> FrameChromeKind {
        self.kind
    }

    pub fn bounds(&self) -> FrameRect {
        self.bounds
    }

    /// Canonical grid-row identity used by materialized chrome primitives and source maps.
    pub fn canonical_row(&self, char_height: f32) -> u32 {
        (self.bounds.y() / char_height.max(1.0)).round().max(0.0) as u32
    }

    pub fn content(&self) -> &FrameChromeContent {
        &self.content
    }

    pub fn hit_regions(&self) -> &[ChromeHitRegion] {
        &self.hit_regions
    }

    pub fn materialized_hit_regions(
        &self,
    ) -> Result<Vec<MaterializedChromeHitRegion>, ChromeLayoutError> {
        self.hit_regions
            .iter()
            .map(|region| {
                Ok(MaterializedChromeHitRegion {
                    bounds: self.bounds.place(region.local_bounds())?,
                    action: region.action().clone(),
                })
            })
            .collect()
    }
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FrameChrome {
    bands: Vec<FrameChromeBand>,
}

impl FrameChrome {
    pub fn layout(
        frame: FrameSize,
        requests: Vec<ChromeBandRequest>,
    ) -> Result<Self, ChromeLayoutError> {
        validate_requests(&requests)?;

        let order = if requests
            .iter()
            .any(|request| request.kind == FrameChromeKind::CompactBar && request.height > 0.0)
        {
            [
                Some(FrameChromeKind::CompactBar),
                Some(FrameChromeKind::TabBar),
                None,
            ]
        } else {
            [
                Some(FrameChromeKind::MenuBar),
                Some(FrameChromeKind::ToolBar),
                Some(FrameChromeKind::TabBar),
            ]
        };

        let mut bands = Vec::new();
        let mut y = 0.0;
        for kind in order.into_iter().flatten() {
            let Some(request) = requests.iter().find(|request| request.kind == kind) else {
                continue;
            };
            if request.height == 0.0 {
                continue;
            }
            if y + request.height > frame.height() {
                return Err(ChromeLayoutError::ContentExceedsFrame { kind });
            }
            let bounds = FrameRect::new(0.0, y, frame.width(), request.height)?;
            request.content.validate_in(bounds)?;
            for region in &request.hit_regions {
                bounds.place(region.local_bounds())?;
            }
            bands.push(FrameChromeBand {
                id: ChromeBandId::from_position(bands.len()),
                kind,
                bounds,
                content: request.content.clone(),
                hit_regions: request.hit_regions.clone(),
            });
            y += request.height;
        }
        Ok(Self { bands })
    }

    pub fn bands(&self) -> &[FrameChromeBand] {
        &self.bands
    }

    pub fn band(&self, kind: FrameChromeKind) -> Option<&FrameChromeBand> {
        self.bands.iter().find(|band| band.kind == kind)
    }

    pub fn hit_test(&self, point: FramePoint) -> Option<(&ChromeAction, FrameRect)> {
        self.bands.iter().find_map(|band| {
            band.hit_regions().iter().find_map(|region| {
                let bounds = band.bounds().place(region.local_bounds()).ok()?;
                let inside = point.x >= bounds.x()
                    && point.x < bounds.x() + bounds.width()
                    && point.y >= bounds.y()
                    && point.y < bounds.y() + bounds.height();
                inside.then_some((region.action(), bounds))
            })
        })
    }
}

fn validate_requests(requests: &[ChromeBandRequest]) -> Result<(), ChromeLayoutError> {
    let mut seen = Vec::new();
    for request in requests {
        if !request.height.is_finite() || request.height < 0.0 {
            return Err(ChromeLayoutError::InvalidMeasuredHeight { kind: request.kind });
        }
        if seen.contains(&request.kind) {
            return Err(ChromeLayoutError::DuplicateBand { kind: request.kind });
        }
        if request.content.kind() != request.kind {
            return Err(ChromeLayoutError::ContentKindMismatch { kind: request.kind });
        }
        seen.push(request.kind);
    }

    let compact = requests
        .iter()
        .any(|request| request.kind == FrameChromeKind::CompactBar && request.height > 0.0);
    let separate = requests.iter().any(|request| {
        matches!(
            request.kind,
            FrameChromeKind::MenuBar | FrameChromeKind::ToolBar
        ) && request.height > 0.0
    });
    if compact && separate {
        return Err(ChromeLayoutError::ConflictingPresentation);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChromeLayoutError {
    InvalidFrameSize,
    InvalidRect,
    InvalidMeasuredHeight { kind: FrameChromeKind },
    DuplicateBand { kind: FrameChromeKind },
    ConflictingPresentation,
    ContentExceedsFrame { kind: FrameChromeKind },
    ContentExceedsBand,
    ContentKindMismatch { kind: FrameChromeKind },
}

//! FrameSync: a sealed presentation as row-store updates (DESIGN §6.10).
//!
//! The sender keeps a mirror of what the receiver holds: a store of
//! sender-managed row slots, the last-sent tables and facts as digests, and
//! the last-sent window layouts. Each presentation is one transaction:
//! `Begin`, the rows the receiver lacks (`DefineRow`), the table deltas, the
//! changed window layouts, the changed facts and `Commit`. The receiver
//! applies every operation in order, including those of a presentation it
//! will discard, rebuilds the `FrameDisplayState` at `Commit`, seals it and
//! validates it before anything can draw it, so a peer's frame can never
//! panic the frontend.
//!
//! The hit index crosses without its text source (D4): its non-text regions
//! and resize handles only.

mod decoder;
mod encoder;
mod mirror;

pub use decoder::{DecodedPresentation, FrameSyncDecoder};
pub use encoder::FrameSyncEncoder;

use neomacs_display_protocol::font::{
    FontCatalogGeneration, ResolvedCharGlyph, ResolvedFont, ResolvedFontId, ResolvedGlyph,
};
use neomacs_display_protocol::frame_chrome::{
    ChromeHitRegion, CompactBarContent, FrameChromeKind, MenuBarContent, ToolBarContent,
};
use neomacs_display_protocol::input_progress::FrontierSnapshot;
use neomacs_display_protocol::presentation_origin::PresentationOrigin;
use neomacs_display_protocol::scroll_coverage::{CoveragePublicationId, ScrollCoverageRenewal};
use neomacs_display_protocol::{
    BackgroundItem, BorderItem, Color, ContentTransitionHint, CursorItem, CursorStyle,
    DesktopFontRenderOptions, DeviceScale, DisplayFrameId, DisplayWindowId, EffectsConfig, Face,
    FaceFillItem, FaceId, FringeBitmapData, LiveDisplayFrameId, ParentFrameRect, PhysCursor,
    PresentationId, PresentedHitError, PresentedHitIndex, PresentedHitRegion,
    PresentedPointerSourceMap, PresentedResizeHandle, Rect, ScrollBarItem, ScrollInputPolicy,
    WindowInfo,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::assets::AssetId;
use crate::codec::{WireError, encode_canonical};
use crate::payload::Payload;
use crate::rows::{RowError, RowKey, RowPlacement, RowRuns};

/// One presentation's position in the session's update stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct UpdateSeq(pub u64);

/// A sender-managed slot of the receiver's row store (`< store_rows`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RowSlot(pub u32);

/// The receiver's row-store bounds, from the session limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StoreLimits {
    pub rows: u32,
    pub bytes: u64,
}

impl Default for StoreLimits {
    fn default() -> Self {
        Self {
            rows: 16_384,
            bytes: 64 << 20,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PresentBase {
    /// The receiver starts this frame from nothing.
    Keyframe,
    /// Against what the receiver holds after everything already sent.
    Delta,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Begin {
    pub frame: LiveDisplayFrameId,
    pub presentation: PresentationId,
    pub update: UpdateSeq,
    pub base: PresentBase,
    pub requires: Vec<AssetId>,
}

/// One FrameSync operation; each is one downlink message on the present lane.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PresentOp {
    Begin(Box<Begin>),
    DefineRow {
        slot: RowSlot,
        key: RowKey,
        row: Payload<RowRuns>,
    },
    Tables(Box<TableDelta>),
    Windows(Box<WindowsUpdate>),
    Facts(Box<FrameFacts>),
    Commit {
        presentation: PresentationId,
    },
    /// Forget every slot and every frame; keyframes follow.
    StoreReset,
}

/// Upserts and removals of one keyed table, sorted by key.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyedDelta<K, V> {
    pub upsert: Vec<(K, V)>,
    pub remove: Vec<K>,
}

impl<K, V> Default for KeyedDelta<K, V> {
    fn default() -> Self {
        Self {
            upsert: Vec::new(),
            remove: Vec::new(),
        }
    }
}

impl<K, V> KeyedDelta<K, V> {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.upsert.is_empty() && self.remove.is_empty()
    }
}

/// `char → glyph` fallback fonts of one face, sorted by character.
pub type CharFontEntries = Vec<(char, ResolvedCharGlyph)>;
/// `cluster → glyphs` of one face, sorted by cluster text.
pub type ShapedClusterEntries = Vec<(Box<str>, Vec<ResolvedGlyph>)>;

/// Changed definitions of a frame's keyed tables.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TableDelta {
    pub faces: KeyedDelta<FaceId, Face>,
    pub fonts: KeyedDelta<ResolvedFontId, ResolvedFont>,
    pub char_fonts: KeyedDelta<FaceId, CharFontEntries>,
    pub shaped_clusters: KeyedDelta<FaceId, ShapedClusterEntries>,
    pub fringe_bitmaps: KeyedDelta<u16, FringeBitmapData>,
    pub cursor_effects: KeyedDelta<DisplayWindowId, EffectsConfig>,
}

impl TableDelta {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
            && self.fonts.is_empty()
            && self.char_fonts.is_empty()
            && self.shaped_clusters.is_empty()
            && self.fringe_bitmaps.is_empty()
            && self.cursor_effects.is_empty()
    }
}

/// A value that is sent only when it changed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Changed<T> {
    Same,
    Set(T),
}

/// An `f32` compared and hashed by its bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct F32Bits(pub u32);

impl F32Bits {
    #[must_use]
    pub fn of(value: f32) -> Self {
        Self(value.to_bits())
    }

    #[must_use]
    pub fn get(self) -> f32 {
        f32::from_bits(self.0)
    }
}

/// A row's top relative to the row before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum YRef {
    /// Exactly `previous.pixel_y + previous.height_px` (`0.0` for the first).
    Follows,
    At(F32Bits),
}

/// A row's start position relative to the row before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum StartRef {
    /// The previous row's `next_buffer_row_start`.
    Follows,
    At(u64),
}

/// One row of a window layout: a slot and its placement, relative to the row
/// before it, so a run of rows that only moved is identical to what was sent.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlotRef {
    pub slot: RowSlot,
    pub y: YRef,
    pub start: StartRef,
    /// `end_charpos - start_charpos`.
    pub span: i64,
    pub cursor_col: Option<u16>,
    pub cursor_type: Option<CursorStyle>,
}

impl SlotRef {
    /// The fields an edit script indexes rows by; equal refs have equal keys.
    pub(crate) fn index_key(&self) -> (RowSlot, YRef, StartRef, i64, Option<u16>) {
        (self.slot, self.y, self.start, self.span, self.cursor_col)
    }
}

/// One step of a window layout edit script against the last sent layout.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum RowEdit {
    /// The next `len` rows are the previous layout's rows `from..from + len`.
    Copy {
        from: u32,
        len: u32,
    },
    Place(SlotRef),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RowsUpdate {
    Same,
    Script(Vec<RowEdit>),
}

/// A window matrix's geometry (everything of `WindowMatrixEntry` and
/// `GlyphMatrix` except the rows).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub pixel_bounds: Rect,
    pub text_pixel_bounds: Rect,
    pub text_clip_bounds: Option<Rect>,
    pub selected: bool,
    pub nrows: u64,
    pub ncols: u64,
    pub matrix_x: u64,
    pub matrix_y: u64,
    pub header_line: bool,
    pub tab_line: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowLayout {
    pub window: DisplayWindowId,
    pub geometry: Changed<WindowGeometry>,
    pub rows: RowsUpdate,
}

/// The frame's windows in order, and the layouts that changed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowsUpdate {
    pub order: Vec<DisplayWindowId>,
    pub changed: Vec<WindowLayout>,
}

/// A row placed absolutely (chrome and coverage rows).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RowRef {
    pub slot: RowSlot,
    pub placement: RowPlacement,
}

/// The frame's placement without its presentation, which `Begin` carries.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FramePlacement {
    pub frame: DisplayFrameId,
    pub parent: Option<DisplayFrameId>,
    pub outer_in_parent: ParentFrameRect,
    pub z_order: i32,
}

/// Frame-wide scalars.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrameScalars {
    pub scroll_input_policy: ScrollInputPolicy,
    pub layout_scale: DeviceScale,
    pub origin: PresentationOrigin,
    pub placement: FramePlacement,
    pub frame_cols: u64,
    pub frame_rows: u64,
    pub frame_pixel_width: f32,
    pub frame_pixel_height: f32,
    pub char_width: f32,
    pub char_height: f32,
    pub font_pixel_size: f32,
    pub background: Color,
    pub font_catalog_generation: FontCatalogGeneration,
    pub desktop_font_render_options: Option<DesktopFontRenderOptions>,
    pub undecorated: bool,
    pub border_width: f32,
    pub border_color: Color,
    pub outer_border_width: f32,
    pub outer_border_color: Color,
    pub background_alpha: f32,
    pub frame_alpha: [f32; 2],
    pub no_accept_focus: bool,
}

/// The part of a hit index that crosses the wire: no text or string
/// positions and no lazy, GC-root-bearing text source (D4).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NonTextHitIndex {
    pub regions: Vec<PresentedHitRegion>,
    pub resize_handles: Vec<PresentedResizeHandle>,
}

impl NonTextHitIndex {
    #[must_use]
    pub fn of(index: &PresentedHitIndex) -> Self {
        Self {
            regions: index.regions().to_vec(),
            resize_handles: index.resize_handles().to_vec(),
        }
    }

    /// The hit index a receiver holds for `presentation`.
    pub fn rebuild(
        &self,
        presentation: PresentationId,
    ) -> Result<PresentedHitIndex, PresentedHitError> {
        PresentedHitIndex::from_parts_with_strings(
            presentation,
            self.regions.clone(),
            Vec::new(),
            Vec::new(),
        )?
        .with_resize_handles(self.resize_handles.clone())
    }
}

/// One band of the frame chrome; its geometry is recomputed by
/// `FrameChrome::layout` from the band kinds and heights.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChromeBand {
    pub kind: FrameChromeKind,
    pub height: f32,
    pub content: ChromeContent,
    pub hit_regions: Vec<ChromeHitRegion>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ChromeContent {
    DisplayRow(RowRef),
    MenuBar(MenuBarContent),
    ToolBar(ToolBarContent),
    CompactBar(CompactBarContent),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chrome {
    pub width: f32,
    pub bands: Vec<ChromeBand>,
}

/// One immutable scroll coverage, its rows by slot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Coverage {
    pub epoch: u64,
    pub publication: CoveragePublicationId,
    pub predict_pixels: bool,
    pub compositor_enabled: bool,
    pub anchor_row: u64,
    pub viewport: Rect,
    pub origin: f32,
    pub window: DisplayWindowId,
    pub geometry: WindowGeometry,
    pub rows: Vec<RowRef>,
    pub faces: Vec<(FaceId, Face)>,
    pub fonts: Vec<(ResolvedFontId, ResolvedFont)>,
    pub char_fonts: Vec<(FaceId, CharFontEntries)>,
    pub shaped_clusters: Vec<(FaceId, ShapedClusterEntries)>,
    pub hit_presentation: PresentationId,
    pub hit_index: NonTextHitIndex,
    pub pointer_source: PresentedPointerSourceMap,
}

/// Frame facts, each group sent only when its digest changed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FrameFacts {
    pub scalars: Changed<FrameScalars>,
    pub window_infos: Changed<Vec<WindowInfo>>,
    pub transition_hints: Changed<Vec<ContentTransitionHint>>,
    pub backgrounds: Changed<Vec<BackgroundItem>>,
    pub face_fills: Changed<Vec<FaceFillItem>>,
    pub borders: Changed<Vec<BorderItem>>,
    pub cursors: Changed<Vec<CursorItem>>,
    pub phys_cursor: Changed<Option<PhysCursor>>,
    pub scroll_bars: Changed<Vec<ScrollBarItem>>,
    pub chrome: Changed<Chrome>,
    pub pointer_source: Changed<PresentedPointerSourceMap>,
    pub regions: Changed<NonTextHitIndex>,
    pub coverage: Changed<Vec<Coverage>>,
    pub coverage_renewals: Changed<Vec<ScrollCoverageRenewal>>,
    /// The input frontier of this exact presentation (always sent).
    pub frontier: Vec<FrontierSnapshot>,
}

/// Why a receiver refused a presentation. It answers `Rejected{reason}` and
/// the session closes with `Goodbye{ProtocolViolation}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum RejectReason {
    #[error("an operation arrived outside a Begin…Commit transaction")]
    OutsideTransaction,
    #[error("Begin inside an open transaction")]
    NestedBegin,
    #[error("Commit names another presentation")]
    CommitMismatch,
    #[error("a delta for a frame the receiver does not hold")]
    UnknownFrame,
    #[error("row slot {0} is not below the store bound")]
    SlotOutOfRange(u32),
    #[error("row slot {0} holds no row")]
    UnknownSlot(u32),
    #[error("a defined row does not match its key")]
    KeyMismatch,
    #[error("the row store exceeds its byte bound")]
    StoreOverflow,
    #[error("a layout edit copies rows the previous layout does not have")]
    CopyOutOfRange,
    #[error("a window layout names an unknown window")]
    UnknownWindow,
    #[error("a frame lists a window twice")]
    DuplicateWindow,
    #[error("the frame lacks its {0:?}")]
    IncompleteKeyframe(FactGroup),
    #[error("face {0} is referenced but not defined")]
    MissingFace(u32),
    #[error("malformed row: {0}")]
    Row(RowError),
    #[error("malformed payload: {0}")]
    Payload(String),
    #[error("chrome cannot be laid out: {0}")]
    Chrome(String),
    #[error("hit index: {0}")]
    HitIndex(String),
    #[error("cannot seal: {0}")]
    Seal(String),
    #[error("cannot materialize: {0}")]
    Materialize(String),
    #[error("the input frontier names stream {0} more than once")]
    FrontierRepeatsStream(u64),
}

/// A group of frame facts, named when a receiver lacks it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FactGroup {
    Facts,
    Scalars,
    WindowInfos,
    TransitionHints,
    Backgrounds,
    FaceFills,
    Borders,
    Cursors,
    PhysCursor,
    ScrollBars,
    Chrome,
    PointerSource,
    Regions,
    Coverage,
    CoverageRenewals,
}

/// A 128-bit digest of a value's canonical encoding.
pub(crate) type Digest128 = [u8; 16];

pub(crate) fn digest<T: Serialize + ?Sized>(value: &T) -> Result<Digest128, WireError> {
    let bytes = encode_canonical(value)?;
    let full = Sha256::digest(&bytes);
    let mut out = [0u8; 16];
    out.copy_from_slice(&full[..16]);
    Ok(out)
}

/// Entries of a hash map sorted by key: the canonical order on the wire.
pub(crate) fn sorted<K: Ord + Copy, V: Clone>(
    map: impl IntoIterator<Item = (K, V)>,
) -> Vec<(K, V)> {
    let mut entries: Vec<(K, V)> = map.into_iter().collect();
    entries.sort_by_key(|(key, _)| *key);
    entries
}

pub(crate) fn sorted_char_fonts(
    map: &neomacs_display_protocol::font::CharFontTable,
) -> Vec<(FaceId, CharFontEntries)> {
    sorted(map.iter().map(|(face, entries)| {
        let mut inner: CharFontEntries = entries.iter().map(|(ch, glyph)| (*ch, *glyph)).collect();
        inner.sort_by_key(|(ch, _)| *ch);
        (*face, inner)
    }))
}

pub(crate) fn sorted_shaped_clusters(
    map: &neomacs_display_protocol::font::ShapedClusterTable,
) -> Vec<(FaceId, ShapedClusterEntries)> {
    sorted(map.iter().map(|(face, entries)| {
        let mut inner: ShapedClusterEntries = entries
            .iter()
            .map(|(text, glyphs)| (text.clone(), glyphs.clone()))
            .collect();
        inner.sort_by(|(a, _), (b, _)| a.cmp(b));
        (*face, inner)
    }))
}

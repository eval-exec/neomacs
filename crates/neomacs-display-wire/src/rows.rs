//! `RowRuns`: one glyph row as it crosses the wire (DESIGN §6.9-§6.10).
//!
//! A row travels as columns: the UTF-8 text of its character glyphs, then
//! run-length runs of faces, provenance steps, bidi levels, cell flags and
//! metrics, with a per-row palette of distinct advance bit patterns. Floats
//! keep their exact bits. Buffer positions are relative to the row's start
//! (base plus steps plus exceptions), so a row that only moved because text
//! was inserted above it has the same `RowRuns` and the same content key.
//!
//! The encoder destructures [`Glyph`] and [`GlyphRowView`] without `..` and
//! the decoder builds [`GlyphRowParts`] with every field named, so a new
//! glyph or row field is a compile error here, not a field silently dropped.
//! The row hash is never sent: the receiver recomputes it.

use neomacs_display_protocol::face::BoxVerticalEdges;
use neomacs_display_protocol::{
    CursorStyle, FaceId, FringeBitmapInfo, Glyph, GlyphArea, GlyphPointerAppearance,
    GlyphPointerAppearanceId, GlyphPointerOccurrenceIdentity, GlyphPointerRun,
    GlyphPointerSourceIdentity, GlyphPointerSourceKind, GlyphProvenance, GlyphRow, GlyphRowParts,
    GlyphRowPartsError, GlyphRowRole, GlyphRowView, GlyphStringBufferRange, GlyphStringId,
    GlyphStringSource, GlyphStringSourceId, GlyphType, ImageMargins,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::codec::{WireError, encode_canonical};

/// Glyphs per row area (DESIGN §6.18 hard cap).
pub const MAX_GLYPHS_PER_AREA: u32 = 65_535;

/// A run of `len` equal values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Run<T> {
    pub len: u32,
    pub value: T,
}

/// Collapse values into runs; `same` decides equality (bitwise for floats).
fn runs<T: Clone>(
    values: impl IntoIterator<Item = T>,
    same: impl Fn(&T, &T) -> bool,
) -> Vec<Run<T>> {
    let mut out: Vec<Run<T>> = Vec::new();
    for value in values {
        match out.last_mut() {
            Some(run) if same(&run.value, &value) => run.len += 1,
            _ => out.push(Run { len: 1, value }),
        }
    }
    out
}

fn expand<T: Clone>(runs: &[Run<T>], len: u32, column: RowColumn) -> Result<Vec<T>, RowError> {
    let total = runs
        .iter()
        .try_fold(0u32, |sum, run| sum.checked_add(run.len))
        .ok_or(RowError::RunLengths { column })?;
    if total != len || runs.iter().any(|run| run.len == 0) {
        return Err(RowError::RunLengths { column });
    }
    let mut out = Vec::with_capacity(len as usize);
    for run in runs {
        out.extend(std::iter::repeat_n(run.value.clone(), run.len as usize));
    }
    Ok(out)
}

/// Row-level flags, one bit each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RowFlags(u8);

impl RowFlags {
    pub const ENABLED: Self = Self(1);
    pub const TRUNCATED_LEFT: Self = Self(1 << 1);
    pub const CONTINUED: Self = Self(1 << 2);
    pub const REVERSED: Self = Self(1 << 3);
    pub const DISPLAYS_TEXT: Self = Self(1 << 4);
    pub const ENDS_AT_ZV: Self = Self(1 << 5);
    pub const MODE_LINE: Self = Self(1 << 6);
    const ALL: u8 = (1 << 7) - 1;

    fn with(self, flag: Self, set: bool) -> Self {
        if set { Self(self.0 | flag.0) } else { self }
    }

    #[must_use]
    pub const fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 == flag.0
    }
}

/// Wide and padding cell flags of one glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CellFlags(u8);

impl CellFlags {
    const WIDE: u8 = 1;
    const PADDING: u8 = 2;

    fn of(wide: bool, padding: bool) -> Self {
        Self((u8::from(wide) * Self::WIDE) | (u8::from(padding) * Self::PADDING))
    }

    fn wide(self) -> bool {
        self.0 & Self::WIDE != 0
    }

    fn padding(self) -> bool {
        self.0 & Self::PADDING != 0
    }
}

/// Whether a glyph is a character (its text is in the row's string) or
/// another kind (its whole type is in `others`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GlyphKind {
    Char,
    Other,
}

/// A buffer position relative to the row's start, or absolute when the
/// difference does not fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelPos {
    Delta(i64),
    Absolute(u64),
}

impl RelPos {
    fn of(position: u64, base: usize) -> Self {
        i64::try_from(i128::from(position) - base as i128)
            .map_or(Self::Absolute(position), Self::Delta)
    }

    fn resolve(self, base: usize) -> Result<u64, RowError> {
        match self {
            Self::Delta(delta) => {
                u64::try_from(base as i128 + i128::from(delta)).map_err(|_| RowError::Position)
            }
            Self::Absolute(position) => Ok(position),
        }
    }

    fn resolve_usize(self, base: usize) -> Result<usize, RowError> {
        usize::try_from(self.resolve(base)?).map_err(|_| RowError::Position)
    }
}

/// One glyph's provenance relative to the glyph before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProvenanceStep {
    /// The buffer position after the previous buffer glyph's (the row start
    /// for the first one).
    NextBuffer,
    /// The previous glyph's provenance again (a wide character's padding).
    Repeat,
    /// The next index of the previous glyph's string.
    NextInString,
    Buffer {
        offset: RelPos,
    },
    Str {
        source: GlyphStringSourceId,
        index: u64,
    },
    LineEnd,
    Mark,
    LeftTruncation,
    EmptyLineNewline {
        offset: RelPos,
    },
}

/// Tracks the provenance stream; encoder and decoder run the same steps.
struct ProvenanceCursor {
    base: usize,
    next_buffer: i128,
    previous: Option<GlyphProvenance>,
}

impl ProvenanceCursor {
    fn new(base: usize) -> Self {
        Self {
            base,
            next_buffer: base as i128,
            previous: None,
        }
    }

    fn advance(&mut self, provenance: GlyphProvenance) {
        if let GlyphProvenance::Buffer { charpos } = provenance {
            self.next_buffer = charpos as i128 + 1;
        }
        self.previous = Some(provenance);
    }

    fn encode(&mut self, provenance: GlyphProvenance) -> ProvenanceStep {
        let step = if self.previous == Some(provenance) {
            ProvenanceStep::Repeat
        } else {
            match provenance {
                GlyphProvenance::Buffer { charpos } if charpos as i128 == self.next_buffer => {
                    ProvenanceStep::NextBuffer
                }
                GlyphProvenance::Buffer { charpos } => ProvenanceStep::Buffer {
                    offset: RelPos::of(charpos as u64, self.base),
                },
                GlyphProvenance::Str { source, index } => match self.previous {
                    Some(GlyphProvenance::Str {
                        source: previous_source,
                        index: previous_index,
                    }) if previous_source == source
                        && previous_index.checked_add(1) == Some(index) =>
                    {
                        ProvenanceStep::NextInString
                    }
                    _ => ProvenanceStep::Str {
                        source,
                        index: index as u64,
                    },
                },
                GlyphProvenance::LineEnd => ProvenanceStep::LineEnd,
                GlyphProvenance::Mark => ProvenanceStep::Mark,
                GlyphProvenance::LeftTruncation => ProvenanceStep::LeftTruncation,
                GlyphProvenance::EmptyLineNewline { charpos } => ProvenanceStep::EmptyLineNewline {
                    offset: RelPos::of(charpos as u64, self.base),
                },
            }
        };
        self.advance(provenance);
        step
    }

    fn decode(&mut self, step: ProvenanceStep) -> Result<GlyphProvenance, RowError> {
        let provenance = match step {
            ProvenanceStep::NextBuffer => GlyphProvenance::Buffer {
                charpos: usize::try_from(self.next_buffer).map_err(|_| RowError::Position)?,
            },
            ProvenanceStep::Repeat => self.previous.ok_or(RowError::Position)?,
            ProvenanceStep::NextInString => match self.previous {
                Some(GlyphProvenance::Str { source, index }) => GlyphProvenance::Str {
                    source,
                    index: index.checked_add(1).ok_or(RowError::Position)?,
                },
                _ => return Err(RowError::Position),
            },
            ProvenanceStep::Buffer { offset } => GlyphProvenance::Buffer {
                charpos: offset.resolve_usize(self.base)?,
            },
            ProvenanceStep::Str { source, index } => GlyphProvenance::Str {
                source,
                index: usize::try_from(index).map_err(|_| RowError::Position)?,
            },
            ProvenanceStep::LineEnd => GlyphProvenance::LineEnd,
            ProvenanceStep::Mark => GlyphProvenance::Mark,
            ProvenanceStep::LeftTruncation => GlyphProvenance::LeftTruncation,
            ProvenanceStep::EmptyLineNewline { offset } => GlyphProvenance::EmptyLineNewline {
                charpos: offset.resolve_usize(self.base)?,
            },
        };
        self.advance(provenance);
        Ok(provenance)
    }
}

/// One glyph area as columns.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AreaRuns {
    pub len: u32,
    pub text: String,
    pub kinds: Vec<Run<GlyphKind>>,
    pub others: Vec<GlyphType>,
    pub faces: Vec<Run<FaceId>>,
    pub provenance: Vec<Run<ProvenanceStep>>,
    pub bidi: Vec<Run<u8>>,
    pub cells: Vec<Run<CellFlags>>,
    pub advance_palette: Vec<f32>,
    pub advances: Vec<Run<u16>>,
    pub heights: Vec<Run<f32>>,
    pub ascents: Vec<Run<f32>>,
    pub voffsets: Vec<Run<f32>>,
    pub box_edges: Vec<Run<BoxVerticalEdges>>,
    pub pointer: Vec<Run<Option<GlyphPointerAppearanceId>>>,
}

fn same_bits(a: &f32, b: &f32) -> bool {
    a.to_bits() == b.to_bits()
}

/// A pointer appearance with buffer positions relative to the row start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RowPointerAppearance {
    pub kind: GlyphPointerSourceKind,
    pub source_id: u64,
    pub range_start: RelPos,
    pub range_end: RelPos,
    pub property_owner: u64,
    pub occurrence: RowPointerOccurrence,
    pub face_id: FaceId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RowPointerOccurrence {
    Source,
    OverlayString { overlay_id: u64, after: bool },
    BufferDisplayReplacement { buffer_id: u64, anchor: RelPos },
}

impl RowPointerAppearance {
    fn of(appearance: &GlyphPointerAppearance, base: usize) -> Self {
        let GlyphPointerAppearance { source, face_id } = appearance;
        let GlyphPointerSourceIdentity {
            kind,
            source_id,
            range_start,
            range_end,
            property_owner,
            occurrence,
        } = *source;
        // Buffer ranges move with insertions; string ranges and synthetic ids
        // do not (`GlyphRow::shift_pointer_appearance_buffer_positions`).
        let position = |value: u64| match kind {
            GlyphPointerSourceKind::Buffer => RelPos::of(value, base),
            GlyphPointerSourceKind::LispString | GlyphPointerSourceKind::Synthetic => {
                RelPos::Absolute(value)
            }
        };
        Self {
            kind,
            source_id,
            range_start: position(range_start),
            range_end: position(range_end),
            property_owner,
            occurrence: match occurrence {
                GlyphPointerOccurrenceIdentity::Source => RowPointerOccurrence::Source,
                GlyphPointerOccurrenceIdentity::OverlayString { overlay_id, after } => {
                    RowPointerOccurrence::OverlayString { overlay_id, after }
                }
                GlyphPointerOccurrenceIdentity::BufferDisplayReplacement { buffer_id, anchor } => {
                    RowPointerOccurrence::BufferDisplayReplacement {
                        buffer_id,
                        anchor: RelPos::of(anchor, base),
                    }
                }
            },
            face_id: *face_id,
        }
    }

    fn resolve(self, base: usize) -> Result<GlyphPointerAppearance, RowError> {
        let Self {
            kind,
            source_id,
            range_start,
            range_end,
            property_owner,
            occurrence,
            face_id,
        } = self;
        Ok(GlyphPointerAppearance {
            source: GlyphPointerSourceIdentity {
                kind,
                source_id,
                range_start: range_start.resolve(base)?,
                range_end: range_end.resolve(base)?,
                property_owner,
                occurrence: match occurrence {
                    RowPointerOccurrence::Source => GlyphPointerOccurrenceIdentity::Source,
                    RowPointerOccurrence::OverlayString { overlay_id, after } => {
                        GlyphPointerOccurrenceIdentity::OverlayString { overlay_id, after }
                    }
                    RowPointerOccurrence::BufferDisplayReplacement { buffer_id, anchor } => {
                        GlyphPointerOccurrenceIdentity::BufferDisplayReplacement {
                            buffer_id,
                            anchor: anchor.resolve(base)?,
                        }
                    }
                },
            },
            face_id,
        })
    }
}

/// A string occurrence with its covered buffer range relative to the row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RowStringSource {
    pub string: GlyphStringId,
    pub covered: Option<(RelPos, RelPos)>,
}

/// One row's content, independent of where it is placed and of the buffer
/// position it starts at.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RowRuns {
    pub role: GlyphRowRole,
    pub flags: RowFlags,
    pub start_col: u16,
    pub pixel_x: f32,
    pub height_px: f32,
    pub line_spacing_px: f32,
    pub ascent_px: f32,
    pub left_fringe_bitmap: Option<FringeBitmapInfo>,
    pub right_fringe_bitmap: Option<FringeBitmapInfo>,
    pub overlay_arrow_bitmap: Option<FringeBitmapInfo>,
    pub areas: [AreaRuns; GlyphArea::COUNT],
    pub pointer_appearances: Vec<RowPointerAppearance>,
    pub image_margins: Vec<ImageMargins>,
    pub string_sources: Vec<RowStringSource>,
    pub pointer_runs: Vec<GlyphPointerRun>,
}

/// Where a row is placed and what it starts at: the part of a row that is
/// not its content (DESIGN §6.8 `SlotRef`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RowPlacement {
    pub pixel_y: f32,
    pub start_charpos: usize,
    pub end_charpos: usize,
    pub cursor_col: Option<u16>,
    pub cursor_type: Option<CursorStyle>,
}

impl RowPlacement {
    /// Bitwise equality (floats included).
    #[must_use]
    pub fn same_as(&self, other: &Self) -> bool {
        self.pixel_y.to_bits() == other.pixel_y.to_bits()
            && self.start_charpos == other.start_charpos
            && self.end_charpos == other.end_charpos
            && self.cursor_col == other.cursor_col
            && self.cursor_type == other.cursor_type
    }
}

/// The 128-bit content key of a row: SHA-256 of its canonical `RowRuns`
/// bytes, truncated (DESIGN §6.8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RowKey(pub [u8; 16]);

impl RowKey {
    #[must_use]
    pub fn of_bytes(bytes: &[u8]) -> Self {
        let digest = Sha256::digest(bytes);
        let mut key = [0u8; 16];
        key.copy_from_slice(&digest[..16]);
        Self(key)
    }
}

/// A column of a row, for errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RowColumn {
    Kinds,
    Faces,
    Provenance,
    Bidi,
    Cells,
    Advances,
    Heights,
    Ascents,
    VerticalOffsets,
    BoxEdges,
    Pointer,
}

/// Why a peer's row was refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum RowError {
    #[error("an area holds {len} glyphs, above {MAX_GLYPHS_PER_AREA}")]
    TooManyGlyphs { len: u32 },
    #[error("the runs of column {column:?} do not cover the area exactly")]
    RunLengths { column: RowColumn },
    #[error("the row text does not match its character glyphs")]
    Text,
    #[error("the row's non-character glyphs do not match its kinds")]
    Others,
    #[error("an advance names no palette entry")]
    Palette,
    #[error("a provenance step names no position")]
    Position,
    #[error("unknown row flags {0:#04x}")]
    Flags(u8),
    #[error("dangling row-local table: {0}")]
    Tables(String),
}

impl From<GlyphRowPartsError> for RowError {
    fn from(error: GlyphRowPartsError) -> Self {
        Self::Tables(error.to_string())
    }
}

fn encode_area(glyphs: &[Glyph], cursor: &mut ProvenanceCursor) -> Result<AreaRuns, WireError> {
    let len = u32::try_from(glyphs.len())
        .ok()
        .filter(|len| *len <= MAX_GLYPHS_PER_AREA)
        .ok_or_else(|| WireError::Encode(format!("{} glyphs in one row area", glyphs.len())))?;
    let mut text = String::new();
    let mut kinds = Vec::with_capacity(glyphs.len());
    let mut others = Vec::new();
    let mut faces = Vec::with_capacity(glyphs.len());
    let mut provenance = Vec::with_capacity(glyphs.len());
    let mut bidi = Vec::with_capacity(glyphs.len());
    let mut cells = Vec::with_capacity(glyphs.len());
    let mut advance_palette: Vec<f32> = Vec::new();
    let mut advances = Vec::with_capacity(glyphs.len());
    let mut heights = Vec::with_capacity(glyphs.len());
    let mut ascents = Vec::with_capacity(glyphs.len());
    let mut voffsets = Vec::with_capacity(glyphs.len());
    let mut box_edges = Vec::with_capacity(glyphs.len());
    let mut pointer = Vec::with_capacity(glyphs.len());
    for glyph in glyphs {
        let Glyph {
            glyph_type,
            face_id,
            provenance: glyph_provenance,
            bidi_level,
            wide,
            pixel_width,
            pixel_height,
            pixel_ascent,
            vertical_offset_px,
            padding,
            box_vertical_edges,
            pointer_appearance,
        } = glyph;
        match glyph_type {
            GlyphType::Char { ch } => {
                text.push(*ch);
                kinds.push(GlyphKind::Char);
            }
            other => {
                others.push(other.clone());
                kinds.push(GlyphKind::Other);
            }
        }
        faces.push(*face_id);
        provenance.push(cursor.encode(*glyph_provenance));
        bidi.push(*bidi_level);
        cells.push(CellFlags::of(*wide, *padding));
        let slot = match advance_palette
            .iter()
            .position(|known| known.to_bits() == pixel_width.to_bits())
        {
            Some(slot) => slot,
            None => {
                advance_palette.push(*pixel_width);
                advance_palette.len() - 1
            }
        };
        advances.push(u16::try_from(slot).expect("at most 65535 glyphs per area"));
        heights.push(*pixel_height);
        ascents.push(*pixel_ascent);
        voffsets.push(*vertical_offset_px);
        box_edges.push(*box_vertical_edges);
        pointer.push(*pointer_appearance);
    }
    Ok(AreaRuns {
        len,
        text,
        kinds: runs(kinds, PartialEq::eq),
        others,
        faces: runs(faces, PartialEq::eq),
        provenance: runs(provenance, PartialEq::eq),
        bidi: runs(bidi, PartialEq::eq),
        cells: runs(cells, PartialEq::eq),
        advance_palette,
        advances: runs(advances, PartialEq::eq),
        heights: runs(heights, same_bits),
        ascents: runs(ascents, same_bits),
        voffsets: runs(voffsets, same_bits),
        box_edges: runs(box_edges, PartialEq::eq),
        pointer: runs(pointer, PartialEq::eq),
    })
}

fn decode_area(area: &AreaRuns, cursor: &mut ProvenanceCursor) -> Result<Vec<Glyph>, RowError> {
    let AreaRuns {
        len,
        text,
        kinds,
        others,
        faces,
        provenance,
        bidi,
        cells,
        advance_palette,
        advances,
        heights,
        ascents,
        voffsets,
        box_edges,
        pointer,
    } = area;
    let len = *len;
    if len > MAX_GLYPHS_PER_AREA {
        return Err(RowError::TooManyGlyphs { len });
    }
    let kinds = expand(kinds, len, RowColumn::Kinds)?;
    let faces = expand(faces, len, RowColumn::Faces)?;
    let provenance = expand(provenance, len, RowColumn::Provenance)?;
    let bidi = expand(bidi, len, RowColumn::Bidi)?;
    let cells = expand(cells, len, RowColumn::Cells)?;
    let advances = expand(advances, len, RowColumn::Advances)?;
    let heights = expand(heights, len, RowColumn::Heights)?;
    let ascents = expand(ascents, len, RowColumn::Ascents)?;
    let voffsets = expand(voffsets, len, RowColumn::VerticalOffsets)?;
    let box_edges = expand(box_edges, len, RowColumn::BoxEdges)?;
    let pointer = expand(pointer, len, RowColumn::Pointer)?;
    let mut chars = text.chars();
    let mut others = others.iter();
    let mut glyphs = Vec::with_capacity(len as usize);
    for index in 0..len as usize {
        let glyph_type = match kinds[index] {
            GlyphKind::Char => GlyphType::Char {
                ch: chars.next().ok_or(RowError::Text)?,
            },
            GlyphKind::Other => {
                let other = others.next().ok_or(RowError::Others)?;
                if matches!(other, GlyphType::Char { .. }) {
                    return Err(RowError::Others);
                }
                other.clone()
            }
        };
        let pixel_width = *advance_palette
            .get(usize::from(advances[index]))
            .ok_or(RowError::Palette)?;
        glyphs.push(Glyph {
            glyph_type,
            face_id: faces[index],
            provenance: cursor.decode(provenance[index])?,
            bidi_level: bidi[index],
            wide: cells[index].wide(),
            pixel_width,
            pixel_height: heights[index],
            pixel_ascent: ascents[index],
            vertical_offset_px: voffsets[index],
            padding: cells[index].padding(),
            box_vertical_edges: box_edges[index],
            pointer_appearance: pointer[index],
        });
    }
    if chars.next().is_some() {
        return Err(RowError::Text);
    }
    if others.next().is_some() {
        return Err(RowError::Others);
    }
    Ok(glyphs)
}

impl RowRuns {
    /// The content of `row`, with its placement split off.
    pub fn of_row(row: &GlyphRow) -> Result<(Self, RowPlacement), WireError> {
        let GlyphRowView {
            glyphs,
            pointer_appearances,
            image_margins,
            string_sources,
            pointer_runs,
            enabled,
            role,
            cursor_col,
            cursor_type,
            truncated_left,
            continued,
            reversed_p,
            displays_text,
            ends_at_zv,
            mode_line,
            pixel_x,
            start_col,
            pixel_y,
            height_px,
            line_spacing_px,
            ascent_px,
            start_charpos,
            end_charpos,
            left_fringe_bitmap,
            right_fringe_bitmap,
            overlay_arrow_bitmap,
        } = row.view();
        let mut cursor = ProvenanceCursor::new(start_charpos);
        let [left, text, right] = glyphs;
        let areas = [
            encode_area(left, &mut cursor)?,
            encode_area(text, &mut cursor)?,
            encode_area(right, &mut cursor)?,
        ];
        let flags = RowFlags(0)
            .with(RowFlags::ENABLED, enabled)
            .with(RowFlags::TRUNCATED_LEFT, truncated_left)
            .with(RowFlags::CONTINUED, continued)
            .with(RowFlags::REVERSED, reversed_p)
            .with(RowFlags::DISPLAYS_TEXT, displays_text)
            .with(RowFlags::ENDS_AT_ZV, ends_at_zv)
            .with(RowFlags::MODE_LINE, mode_line);
        let runs = Self {
            role,
            flags,
            start_col,
            pixel_x,
            height_px,
            line_spacing_px,
            ascent_px,
            left_fringe_bitmap,
            right_fringe_bitmap,
            overlay_arrow_bitmap,
            areas,
            pointer_appearances: pointer_appearances
                .iter()
                .map(|appearance| RowPointerAppearance::of(appearance, start_charpos))
                .collect(),
            image_margins: image_margins.to_vec(),
            string_sources: string_sources
                .iter()
                .map(|source| RowStringSource {
                    string: source.string(),
                    covered: source.covered_buffer_range().map(|range| {
                        (
                            RelPos::of(range.start() as u64, start_charpos),
                            RelPos::of(range.end() as u64, start_charpos),
                        )
                    }),
                })
                .collect(),
            pointer_runs: pointer_runs.to_vec(),
        };
        let placement = RowPlacement {
            pixel_y,
            start_charpos,
            end_charpos,
            cursor_col,
            cursor_type,
        };
        Ok((runs, placement))
    }

    /// The canonical bytes and the content key.
    pub fn keyed_bytes(&self) -> Result<(RowKey, Vec<u8>), WireError> {
        let bytes = encode_canonical(self)?;
        Ok((RowKey::of_bytes(&bytes), bytes))
    }

    /// Rebuild the row at `placement`, validating every column.
    pub fn to_row(&self, placement: &RowPlacement) -> Result<GlyphRow, RowError> {
        let Self {
            role,
            flags,
            start_col,
            pixel_x,
            height_px,
            line_spacing_px,
            ascent_px,
            left_fringe_bitmap,
            right_fringe_bitmap,
            overlay_arrow_bitmap,
            areas,
            pointer_appearances,
            image_margins,
            string_sources,
            pointer_runs,
        } = self;
        if flags.0 & !RowFlags::ALL != 0 {
            return Err(RowError::Flags(flags.0));
        }
        let RowPlacement {
            pixel_y,
            start_charpos,
            end_charpos,
            cursor_col,
            cursor_type,
        } = *placement;
        let mut cursor = ProvenanceCursor::new(start_charpos);
        let [left, text, right] = areas;
        let glyphs = [
            decode_area(left, &mut cursor)?,
            decode_area(text, &mut cursor)?,
            decode_area(right, &mut cursor)?,
        ];
        let pointer_appearances = pointer_appearances
            .iter()
            .map(|appearance| appearance.resolve(start_charpos))
            .collect::<Result<Vec<_>, _>>()?;
        let string_sources = string_sources
            .iter()
            .map(|source| {
                Ok(match source.covered {
                    None => GlyphStringSource::new(source.string),
                    Some((start, end)) => GlyphStringSource::replacement(
                        source.string,
                        GlyphStringBufferRange::new(
                            start.resolve_usize(start_charpos)?,
                            end.resolve_usize(start_charpos)?,
                        ),
                    ),
                })
            })
            .collect::<Result<Vec<_>, RowError>>()?;
        let parts = GlyphRowParts {
            glyphs,
            pointer_appearances,
            image_margins: image_margins.clone(),
            string_sources,
            pointer_runs: pointer_runs.clone(),
            enabled: flags.contains(RowFlags::ENABLED),
            role: *role,
            cursor_col,
            cursor_type,
            truncated_left: flags.contains(RowFlags::TRUNCATED_LEFT),
            continued: flags.contains(RowFlags::CONTINUED),
            reversed_p: flags.contains(RowFlags::REVERSED),
            displays_text: flags.contains(RowFlags::DISPLAYS_TEXT),
            ends_at_zv: flags.contains(RowFlags::ENDS_AT_ZV),
            mode_line: flags.contains(RowFlags::MODE_LINE),
            pixel_x: *pixel_x,
            start_col: *start_col,
            pixel_y,
            height_px: *height_px,
            line_spacing_px: *line_spacing_px,
            ascent_px: *ascent_px,
            start_charpos,
            end_charpos,
            left_fringe_bitmap: *left_fringe_bitmap,
            right_fringe_bitmap: *right_fringe_bitmap,
            overlay_arrow_bitmap: *overlay_arrow_bitmap,
        };
        Ok(GlyphRow::from_parts(parts)?)
    }

    /// Faces the row's glyphs, pointer appearances and fringes reference.
    pub fn referenced_faces(&self) -> impl Iterator<Item = FaceId> + '_ {
        self.areas
            .iter()
            .flat_map(|area| area.faces.iter().map(|run| run.value))
            .chain(
                self.pointer_appearances
                    .iter()
                    .map(|appearance| appearance.face_id),
            )
            .chain(
                [
                    self.left_fringe_bitmap,
                    self.right_fringe_bitmap,
                    self.overlay_arrow_bitmap,
                ]
                .into_iter()
                .flatten()
                .map(|bitmap| bitmap.face_id),
            )
    }
}

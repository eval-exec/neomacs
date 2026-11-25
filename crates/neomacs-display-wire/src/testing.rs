//! Test support (feature `testing`): a small frame producer, a wire round
//! trip through the real codec, and the D4 reference projection.
//!
//! [`TestFrame`] lays out monospace text the way the layout engine shapes a
//! frame (window matrices of copy-on-write rows that are reused while their
//! content and placement hold), so FrameSync sequences of edits, scrolls,
//! resizes and splits can be checked without the evaluator. The layout
//! engine's parity corpus uses the same round trip on evaluator-driven
//! frames.

use std::collections::HashMap;
use std::sync::Arc;

use neomacs_display_protocol::face::Face;
use neomacs_display_protocol::frame_glyphs::{DisplaySlotId, GlyphRowRole};
use neomacs_display_protocol::scroll_coverage::{CoveragePublicationId, ScrollCoverage};
use neomacs_display_protocol::{
    BackgroundItem, BorderItem, Color, CursorItem, CursorItemRole, CursorStyle, DisplayFrameId,
    DisplayWindowId, FaceId, FrameDisplayState, FrameDisplayStateParts, FrameGlyphBuffer,
    FrameRect, Glyph, GlyphArea, GlyphMatrix, GlyphRow, LiveDisplayFrameId, MatrixRow,
    ParentFrameRect, PhysCursor, PresentationId, PresentedFramePlacement, PresentedHitIndex,
    PresentedHitRegion, PresentedRegionKind, Rect, SealedFramePresentation, WindowMatrixEntry,
};

use crate::codec::WireCodec;
use crate::msg::Downlink;
use crate::present::{
    DecodedPresentation, FrameSyncDecoder, FrameSyncEncoder, NonTextHitIndex, PresentOp,
    RejectReason,
};

pub const CHAR_WIDTH: f32 = 8.0;
pub const LINE_HEIGHT: f32 = 16.0;
pub const DEFAULT_FACE: FaceId = FaceId::new(0);
pub const MODE_LINE_FACE: FaceId = FaceId::new(1);
pub const KEYWORD_FACE: FaceId = FaceId::new(2);

/// One window of a [`TestFrame`]: rows `top..top + height` of the frame, the
/// last of which is its mode line.
#[derive(Clone, Debug)]
pub struct TestWindow {
    pub id: DisplayWindowId,
    pub top: usize,
    pub height: usize,
    /// The first buffer line shown.
    pub start_line: usize,
}

/// A frame of monospace text in one or more windows over one buffer.
#[derive(Debug)]
pub struct TestFrame {
    pub cols: usize,
    pub rows: usize,
    pub lines: Vec<String>,
    pub windows: Vec<TestWindow>,
    pub point: (usize, usize),
    presentation: u64,
    rows_cache: HashMap<RowCacheKey, MatrixRow>,
    coverage: Option<CoverageExport>,
}

/// The scroll coverage a [`TestFrame`] exports for its first window.
#[derive(Clone, Copy, Debug)]
struct CoverageExport {
    /// Buffer lines carried below the window's text rows.
    prefetch: usize,
    publication: CoveragePublicationId,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct RowCacheKey {
    text: String,
    role: u8,
    start: usize,
    end: usize,
    top_bits: u32,
    cursor: Option<u16>,
    cols: usize,
}

impl TestFrame {
    #[must_use]
    pub fn new(cols: usize, rows: usize, text: &str) -> Self {
        Self {
            cols,
            rows,
            lines: text.lines().map(str::to_owned).collect(),
            windows: vec![TestWindow {
                id: DisplayWindowId::new(1),
                top: 0,
                height: rows,
                start_line: 0,
            }],
            point: (0, 0),
            presentation: 0,
            rows_cache: HashMap::new(),
            coverage: None,
        }
    }

    /// Export scroll coverage for the first window (DESIGN §6.10, "Scroll
    /// coverage"): its text rows and `prefetch` more buffer lines below
    /// them, in a presentation whose buffer has enough lines to fill the
    /// window.
    #[must_use]
    pub fn with_scroll_coverage(mut self, prefetch: usize) -> Self {
        self.coverage = Some(CoverageExport {
            prefetch,
            publication: CoveragePublicationId::allocate()
                .expect("a test process allocates few coverage ids"),
        });
        self
    }

    /// Buffer offset (0-based charpos) of the start of `line`.
    fn line_start(&self, line: usize) -> usize {
        self.lines[..line.min(self.lines.len())]
            .iter()
            .map(|text| text.chars().count() + 1)
            .sum()
    }

    pub fn insert_char(&mut self, line: usize, col: usize, ch: char) {
        let text = &mut self.lines[line];
        let byte = text
            .char_indices()
            .nth(col)
            .map_or(text.len(), |(index, _)| index);
        text.insert(byte, ch);
        self.point = (line, col + 1);
    }

    pub fn insert_line(&mut self, line: usize, text: &str) {
        self.lines.insert(line, text.to_owned());
    }

    pub fn scroll(&mut self, window: usize, lines: isize) {
        let window = &mut self.windows[window];
        window.start_line = window.start_line.saturating_add_signed(lines);
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let grow = rows as isize - self.rows as isize;
        self.cols = cols;
        self.rows = rows;
        if let Some(last) = self.windows.last_mut() {
            last.height = last.height.saturating_add_signed(grow).max(2);
        }
    }

    /// Split the first window into two of half its height.
    pub fn split(&mut self) {
        let first = self.windows[0].clone();
        let upper = first.height / 2;
        self.windows[0].height = upper;
        let next_id = self
            .windows
            .iter()
            .map(|window| window.id.get())
            .max()
            .unwrap_or(0)
            + 1;
        self.windows.insert(
            1,
            TestWindow {
                id: DisplayWindowId::new(next_id),
                top: first.top + upper,
                height: first.height - upper,
                start_line: first.start_line,
            },
        );
    }

    fn row(&mut self, key: RowCacheKey, face: FaceId, role: GlyphRowRole) -> MatrixRow {
        if let Some(row) = self.rows_cache.get(&key) {
            return row.clone();
        }
        let mut row = GlyphRow::new(role);
        row.enabled = true;
        row.pixel_y = f32::from_bits(key.top_bits);
        row.height_px = LINE_HEIGHT;
        row.ascent_px = 12.0;
        row.start_charpos = key.start;
        row.end_charpos = key.end;
        row.displays_text = role == GlyphRowRole::Text;
        row.mode_line = role == GlyphRowRole::ModeLine;
        row.cursor_col = key.cursor;
        row.cursor_type = key.cursor.map(|_| CursorStyle::FilledBox);
        for (index, ch) in key.text.chars().take(key.cols).enumerate() {
            let face = if role == GlyphRowRole::Text && ch.is_ascii_uppercase() {
                KEYWORD_FACE
            } else {
                face
            };
            let mut glyph = Glyph::char(ch, face, key.start + index);
            glyph.pixel_width = CHAR_WIDTH;
            row.glyphs[GlyphArea::Text.index()].push(glyph);
        }
        row.hash = row.compute_hash();
        let row = MatrixRow::new(row);
        self.rows_cache.insert(key, row.clone());
        row
    }

    fn window_entry(&mut self, index: usize) -> (WindowMatrixEntry, Option<PhysCursor>) {
        let window = self.windows[index].clone();
        let text_rows = window.height.saturating_sub(1);
        let mut rows = Vec::with_capacity(window.height);
        let mut cursor = None;
        for screen_row in 0..text_rows {
            let line = window.start_line + screen_row;
            let top = screen_row as f32 * LINE_HEIGHT;
            let (text, start, end) = match self.lines.get(line) {
                Some(text) => {
                    let start = self.line_start(line);
                    (text.clone(), start, start + text.chars().count())
                }
                None => (String::new(), 0, 0),
            };
            let cursor_col = (index == 0 && self.point.0 == line)
                .then(|| u16::try_from(self.point.1).unwrap_or(u16::MAX));
            if let Some(col) = cursor_col {
                cursor = Some(PhysCursor {
                    window_id: window.id,
                    charpos: start + usize::from(col),
                    row: screen_row,
                    col,
                    slot_id: DisplaySlotId {
                        window_id: window.id,
                        row: screen_row as u32,
                        col,
                    },
                    x: f32::from(col) * CHAR_WIDTH,
                    y: window.top as f32 * LINE_HEIGHT + top,
                    width: CHAR_WIDTH,
                    height: LINE_HEIGHT,
                    ascent: 12.0,
                    style: CursorStyle::FilledBox,
                    color: Color::rgb(1.0, 1.0, 1.0),
                    cursor_fg: Color::rgb(0.0, 0.0, 0.0),
                });
            }
            let key = RowCacheKey {
                text,
                role: 0,
                start,
                end,
                top_bits: top.to_bits(),
                cursor: cursor_col,
                cols: self.cols,
            };
            rows.push(self.row(key, DEFAULT_FACE, GlyphRowRole::Text));
        }
        let mode_line = format!(
            "-UUU:----  *test*  L{}  window {}",
            window.start_line + 1,
            window.id.get()
        );
        let key = RowCacheKey {
            text: mode_line,
            role: 1,
            start: 0,
            end: 0,
            top_bits: (text_rows as f32 * LINE_HEIGHT).to_bits(),
            cursor: None,
            cols: self.cols,
        };
        rows.push(self.row(key, MODE_LINE_FACE, GlyphRowRole::ModeLine));
        let width = self.cols as f32 * CHAR_WIDTH;
        let bounds = Rect::new(
            0.0,
            window.top as f32 * LINE_HEIGHT,
            width,
            window.height as f32 * LINE_HEIGHT,
        );
        let text_bounds = Rect::new(
            0.0,
            window.top as f32 * LINE_HEIGHT,
            width,
            text_rows as f32 * LINE_HEIGHT,
        );
        let mut matrix = GlyphMatrix::new(0, self.cols);
        matrix.nrows = rows.len();
        matrix.rows = rows;
        (
            WindowMatrixEntry {
                window_id: window.id,
                matrix,
                pixel_bounds: bounds,
                text_pixel_bounds: text_bounds,
                text_clip_bounds: None,
                selected: index == 0,
            },
            cursor,
        )
    }

    /// The first window's text rows and the prefetched lines below them, as
    /// one coverage export; `None` when the buffer cannot fill the window.
    fn scroll_coverage(
        &mut self,
        presentation: PresentationId,
        export: CoverageExport,
    ) -> Option<Arc<ScrollCoverage>> {
        let window = self.windows[0].clone();
        let text_rows = window.height.saturating_sub(1);
        let first = window.start_line;
        let end = (first + text_rows + export.prefetch).min(self.lines.len());
        if text_rows == 0 || first + text_rows > end {
            return None;
        }
        let mut rows = Vec::with_capacity(end - first);
        for (index, line) in (first..end).enumerate() {
            let text = self.lines[line].clone();
            let start = self.line_start(line);
            let key = RowCacheKey {
                end: start + text.chars().count(),
                text,
                role: 0,
                start,
                top_bits: (index as f32 * LINE_HEIGHT).to_bits(),
                cursor: None,
                cols: self.cols,
            };
            rows.push(self.row(key, DEFAULT_FACE, GlyphRowRole::Text));
        }
        let width = self.cols as f32 * CHAR_WIDTH;
        let top = window.top as f32 * LINE_HEIGHT;
        let viewport = Rect::new(0.0, top, width, text_rows as f32 * LINE_HEIGHT);
        let bounds = Rect::new(0.0, top, width, rows.len() as f32 * LINE_HEIGHT);
        let region = FrameRect::new(bounds.x, bounds.y, bounds.width, bounds.height)
            .expect("a non-empty coverage has positive bounds");
        let mut matrix = GlyphMatrix::new(0, self.cols);
        matrix.nrows = rows.len();
        matrix.rows = rows;
        Some(Arc::new(ScrollCoverage {
            epoch: presentation.get(),
            publication: export.publication,
            predict_pixels: false,
            compositor_enabled: false,
            anchor_row: 0,
            viewport,
            origin: 0.0,
            content: WindowMatrixEntry {
                window_id: window.id,
                matrix,
                pixel_bounds: Rect::new(0.0, top, width, window.height as f32 * LINE_HEIGHT),
                text_pixel_bounds: viewport,
                text_clip_bounds: Some(bounds),
                selected: true,
            },
            faces: test_faces().collect(),
            fonts: Default::default(),
            char_fonts: Default::default(),
            shaped_clusters: Default::default(),
            hit_index: PresentedHitIndex::from_parts(
                presentation,
                vec![PresentedHitRegion::new(
                    Some(window.id),
                    PresentedRegionKind::TextBody,
                    region,
                    0,
                )],
                Vec::new(),
            )
            .expect("one text-body region is a valid index"),
            pointer_source: Default::default(),
        }))
    }

    /// The next presentation of the frame's current content.
    #[must_use]
    pub fn present(&mut self) -> SealedFramePresentation {
        self.presentation += 1;
        let presentation = PresentationId::new(self.presentation);
        let mut state = FrameDisplayState::new(self.cols, self.rows, CHAR_WIDTH, LINE_HEIGHT);
        state.presentation_id = presentation;
        state.frame_placement = PresentedFramePlacement::new(
            DisplayFrameId::new(1),
            presentation,
            None,
            ParentFrameRect::new(
                0.0,
                0.0,
                self.cols as f32 * CHAR_WIDTH,
                self.rows as f32 * LINE_HEIGHT,
            )
            .expect("a positive frame size"),
            0,
        );
        state.presented_hit_index =
            PresentedHitIndex::from_parts(presentation, Vec::new(), Vec::new())
                .expect("an empty hit index is valid");
        state.faces.extend(test_faces());
        state.backgrounds.push(BackgroundItem {
            bounds: Rect::new(
                0.0,
                0.0,
                self.cols as f32 * CHAR_WIDTH,
                self.rows as f32 * LINE_HEIGHT,
            ),
            color: Color::rgb(0.0, 0.0, 0.0),
        });
        for index in 0..self.windows.len() {
            let (entry, cursor) = self.window_entry(index);
            if index > 0 {
                state.borders.push(BorderItem {
                    window_id: entry.window_id,
                    x: 0.0,
                    y: entry.pixel_bounds.y,
                    width: entry.pixel_bounds.width,
                    height: 1.0,
                    color: Color::rgb(0.5, 0.5, 0.5),
                });
            }
            if let Some(cursor) = cursor {
                state.cursors.push(CursorItem {
                    window_id: cursor.window_id,
                    role: CursorItemRole::WindowCaret {
                        charpos: cursor.charpos,
                    },
                    slot_id: cursor.slot_id,
                    x: cursor.x,
                    y: cursor.y,
                    width: cursor.width,
                    height: cursor.height,
                    style: cursor.style,
                    color: cursor.color,
                    cursor_fg: cursor.cursor_fg,
                    ascent: cursor.ascent,
                });
                state.phys_cursor = Some(cursor);
            }
            state.window_matrices.push(entry);
        }
        if let Some(export) = self.coverage
            && let Some(coverage) = self.scroll_coverage(presentation, export)
        {
            state.scroll_coverage.push(coverage);
        }
        SealedFramePresentation::seal(state).expect("a test frame seals")
    }
}

/// The faces every [`TestFrame`] defines.
fn test_faces() -> impl Iterator<Item = (FaceId, Face)> {
    [
        (DEFAULT_FACE, Color::rgb(0.9, 0.9, 0.9)),
        (MODE_LINE_FACE, Color::rgb(0.2, 0.2, 0.2)),
        (KEYWORD_FACE, Color::rgb(0.4, 0.6, 1.0)),
    ]
    .into_iter()
    .map(|(id, color)| {
        (
            id,
            Face {
                id,
                foreground: color,
                ..Face::default()
            },
        )
    })
}

/// The frame a receiver can rebuild under D4: every hit index without its
/// text and string positions (and without a lazy text source).
///
/// This projects through [`NonTextHitIndex`], the encoder's own projection,
/// so it is not an independent check of the hit index: a non-text part that
/// `NonTextHitIndex` failed to carry would be missing on both sides. It
/// carries regions and resize handles, the only non-text parts the protocol
/// crate's `PresentedHitIndex` exposes today.
#[must_use]
pub fn without_text_positions(sealed: &SealedFramePresentation) -> SealedFramePresentation {
    let mut parts = FrameDisplayStateParts::from_view(sealed.state().view());
    let presentation = parts.presentation_id;
    parts.presented_hit_index = NonTextHitIndex::of(&parts.presented_hit_index)
        .rebuild(presentation)
        .expect("a valid index stays valid without text positions");
    parts.scroll_coverage = parts
        .scroll_coverage
        .iter()
        .map(|coverage| {
            let mut coverage: ScrollCoverage = (**coverage).clone();
            coverage.hit_index = NonTextHitIndex::of(&coverage.hit_index)
                .rebuild(coverage.hit_index.presentation())
                .expect("a valid coverage index stays valid without text positions");
            Arc::new(coverage)
        })
        .collect();
    SealedFramePresentation::seal(FrameDisplayState::from_parts(parts))
        .expect("the projection of a sealed frame seals")
}

/// Bytes one presentation took on the wire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WireSize {
    pub messages: usize,
    pub bytes: usize,
}

/// Encode `ops` as downlink message bodies with the real codec, decode them
/// back and apply them to `decoder`.
pub fn transmit(
    codec: &WireCodec,
    ops: Vec<PresentOp>,
    decoder: &mut FrameSyncDecoder,
) -> (WireSize, Result<Option<DecodedPresentation>, RejectReason>) {
    let mut size = WireSize::default();
    let mut decoded = Ok(None);
    for op in ops {
        let bytes = codec
            .encode(&Downlink::Present(op))
            .expect("a present op encodes");
        size.messages += 1;
        size.bytes += bytes.len();
        let Downlink::Present(op) = codec.decode::<Downlink>(&bytes).expect("own bytes decode")
        else {
            panic!("a present op decodes as a present op");
        };
        match decoder.apply(op) {
            Ok(None) => {}
            Ok(Some(presentation)) => decoded = Ok(Some(presentation)),
            Err(reason) => return (size, Err(reason)),
        }
    }
    (size, decoded)
}

/// Send `presentation` of `frame` through encoder, codec and decoder.
pub fn round_trip(
    encoder: &mut FrameSyncEncoder,
    decoder: &mut FrameSyncDecoder,
    frame: LiveDisplayFrameId,
    presentation: &SealedFramePresentation,
) -> (WireSize, Result<SealedFramePresentation, RejectReason>) {
    let ops = encoder
        .encode(frame, presentation)
        .expect("a sealed frame encodes");
    let (size, decoded) = transmit(&WireCodec::default(), ops, decoder);
    let decoded = decoded.and_then(|decoded| {
        decoded
            .map(|decoded| decoded.presentation)
            .ok_or(RejectReason::OutsideTransaction)
    });
    (size, decoded)
}

/// Assert two materialized frames draw the same thing: equal canonical text
/// (bit-exact floats, sorted maps) and equal values.
pub fn assert_same_materialized(
    expected: &FrameGlyphBuffer,
    actual: &FrameGlyphBuffer,
    context: &str,
) {
    let expected_text = crate::canonical::materialized(expected);
    let actual_text = crate::canonical::materialized(actual);
    if expected_text != actual_text {
        let first = expected_text
            .lines()
            .zip(actual_text.lines())
            .position(|(a, b)| a != b);
        panic!(
            "{context}: materialized frames differ at line {first:?}\nexpected: {:?}\nactual:   {:?}",
            first.and_then(|line| expected_text.lines().nth(line)),
            first.and_then(|line| actual_text.lines().nth(line)),
        );
    }
    assert!(
        expected == actual,
        "{context}: materialized frames are not equal"
    );
}

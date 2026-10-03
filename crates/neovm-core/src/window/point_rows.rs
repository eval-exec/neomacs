//! C6: immutable point cells shared independently of a row's placement.
//!
//! `NEOMACS_PRESENT_POINT_ROWS=off|on|verify` is read once per process and
//! defaults to on. `on` publishes row storage in place of the flat point
//! vector; `verify` checks each frozen row's encoding roundtrip. This
//! storage owns plain numeric geometry, never Lisp values or mutable caches.
//! Producers own mutable placement descriptors and publish immutable snapshots;
//! concurrent readers share initialized cell and order arrays through `Arc`.
//!
//! | Knob | Default | Values | Gate |
//! | --- | --- | --- | --- |
//! | `NEOMACS_PRESENT_POINT_ROWS` | `on` | `off`, `on`, `verify` | Compact immutable row cells and direct row hit queries; verify checks decoded cells. |
//! | `NEOMACS_POINT_ROW_ITER` | `on` | `off`, `on` | Concatenate row point streams when placed source bounds prove the existing heap order. |
//! Descriptors move rows without rewriting cells. Compact encoding is checked
//! field by field; an unencodable row retains every original i64 in wide form.
//! Iterator mode is read once per process. Its numeric range certificate is
//! recomputed for each borrow because producers can replace the public row
//! vector between publications; concurrent readers own independent iterators.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};
use std::sync::{Arc, OnceLock};

use crate::buffer::LispCharPos1;

use super::{DisplayPointRole, DisplayPointSnapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayPointRowsMode {
    Off,
    On,
    Verify,
}

impl DisplayPointRowsMode {
    #[inline]
    pub const fn enabled(self) -> bool {
        !matches!(self, Self::Off)
    }
}

pub fn display_point_rows_mode() -> DisplayPointRowsMode {
    static MODE: OnceLock<DisplayPointRowsMode> = OnceLock::new();
    *MODE.get_or_init(|| {
        match std::env::var("NEOMACS_PRESENT_POINT_ROWS")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref()
        {
            None | Some("on" | "1" | "true" | "yes") => DisplayPointRowsMode::On,
            Some("verify") => DisplayPointRowsMode::Verify,
            _ => DisplayPointRowsMode::Off,
        }
    })
}

/// Process-wide numeric policy published by OnceLock; readers share no cursor or Lisp state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PointRowIterMode {
    Off,
    On,
}

impl PointRowIterMode {
    #[inline]
    fn from_setting(setting: Option<&str>) -> Self {
        match setting
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            None | Some("on" | "1" | "true" | "yes") => Self::On,
            _ => Self::Off,
        }
    }
}

#[inline]
fn point_row_iter_mode() -> PointRowIterMode {
    static MODE: OnceLock<PointRowIterMode> = OnceLock::new();
    *MODE.get_or_init(|| {
        PointRowIterMode::from_setting(std::env::var("NEOMACS_POINT_ROW_ITER").ok().as_deref())
    })
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PointCellRole {
    Glyph,
    OverlaidMarker,
    InsertionBoundary,
    SyntheticBoundary,
}

/// The usual glyph/insertion slot: exactly sixteen bytes, with checked bounds.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointCell {
    position_delta: u32,
    x: i32,
    col: u16,
    width: u16,
    height: u16,
    y_offset: i8,
    role: PointCellRole,
}

const _: [(); 16] = [(); std::mem::size_of::<PointCell>()];

impl PointCell {
    fn encode(point: &DisplayPointSnapshot, base: i64, y: i64) -> Option<Self> {
        Some(Self {
            position_delta: point
                .buffer_pos
                .as_i64()
                .checked_sub(base)?
                .try_into()
                .ok()?,
            x: point.x.try_into().ok()?,
            col: point.col.try_into().ok()?,
            width: point.width.try_into().ok()?,
            height: point.height.try_into().ok()?,
            y_offset: point.y.checked_sub(y)?.try_into().ok()?,
            role: match point.role {
                DisplayPointRole::Glyph => PointCellRole::Glyph,
                DisplayPointRole::OverlaidMarker => PointCellRole::OverlaidMarker,
                DisplayPointRole::InsertionBoundary => PointCellRole::InsertionBoundary,
                DisplayPointRole::SyntheticBoundary => PointCellRole::SyntheticBoundary,
            },
        })
    }
}

#[derive(Clone)]
enum Cells {
    Compact(Arc<[PointCell]>),
    Wide(Arc<[DisplayPointSnapshot]>),
}

#[derive(Clone)]
enum PointOrder {
    Compact(Arc<[u32]>),
    Wide(Arc<[usize]>),
}

impl PointOrder {
    fn new(indices: Vec<usize>) -> Self {
        if let Some(compact) = indices
            .iter()
            .map(|&index| u32::try_from(index).ok())
            .collect::<Option<Vec<_>>>()
        {
            Self::Compact(compact.into())
        } else {
            Self::Wide(indices.into())
        }
    }

    #[inline]
    fn len(&self) -> usize {
        match self {
            Self::Compact(indices) => indices.len(),
            Self::Wide(indices) => indices.len(),
        }
    }

    #[inline]
    fn get(&self, index: usize) -> usize {
        match self {
            Self::Compact(indices) => indices[index] as usize,
            Self::Wide(indices) => indices[index],
        }
    }
}

struct PointRowData {
    cells: Cells,
    source_order: PointOrder,
    x_order: PointOrder,
    original_y: i64,
    original_base: i64,
    source_min: i64,
    source_max: i64,
    y_min: i64,
    y_max: i64,
    max_height: i64,
    min_x: Option<i64>,
    max_x: Option<i64>,
}

/// Placement changes clone this descriptor and share its immutable cell arrays.
#[derive(Clone)]
pub struct DisplayPointRow {
    row: i64,
    y: i64,
    buffer_base: i64,
    data: Arc<PointRowData>,
}

#[inline]
fn shifted(value: i64, original_base: i64, new_base: i64) -> Option<i64> {
    i64::try_from(i128::from(value) - i128::from(original_base) + i128::from(new_base)).ok()
}

impl DisplayPointRow {
    /// Freeze one row. Points may be in visual or source order, including bidi.
    /// Empty rows contain no point cells and use an inert zero placement.
    pub fn from_points(points: Vec<DisplayPointSnapshot>) -> Self {
        let row = points.first().map_or(0, |point| point.row);
        let y = points.first().map_or(0, |point| point.y);
        let base = points
            .iter()
            .map(|point| point.buffer_pos.as_i64())
            .min()
            .unwrap_or(0);
        Self::from_placement(row, y, base, points)
    }

    pub fn from_placement(
        row: i64,
        y: i64,
        buffer_base: i64,
        points: Vec<DisplayPointSnapshot>,
    ) -> Self {
        assert!(
            points.iter().all(|point| point.row == row),
            "point row must match descriptor"
        );
        let mut source_order: Vec<_> = (0..points.len()).collect();
        // Stable tie ordering preserves the producer's walk order.
        source_order.sort_by_key(|&index| {
            let point = &points[index];
            (point.buffer_pos, point.col, point.x)
        });
        let mut x_order: Vec<_> = (0..points.len()).collect();
        x_order.sort_by_key(|&index| {
            let point = &points[index];
            (point.x, point.col, point.buffer_pos)
        });
        let source_min = points
            .iter()
            .map(|point| point.buffer_pos.as_i64())
            .min()
            .unwrap_or(buffer_base);
        let source_max = points
            .iter()
            .map(|point| point.buffer_pos.as_i64())
            .max()
            .unwrap_or(buffer_base);
        let y_min = points.iter().map(|point| point.y).min().unwrap_or(y);
        let y_max = points.iter().map(|point| point.y).max().unwrap_or(y);
        let max_height = points
            .iter()
            .map(|point| point.height.max(1))
            .max()
            .unwrap_or(0);
        let min_x = points.iter().map(|point| point.x).min();
        let max_x = points
            .iter()
            .map(|point| point.x.saturating_add(point.width.max(1)))
            .max();
        let verify_points =
            (display_point_rows_mode() == DisplayPointRowsMode::Verify).then(|| {
                let mut original = points.clone();
                original.sort_by_key(|point| (point.buffer_pos, point.row, point.col, point.x));
                original
            });
        let compact = points
            .iter()
            .map(|point| PointCell::encode(point, buffer_base, y))
            .collect::<Option<Vec<_>>>();
        let cells = match compact {
            Some(cells) => Cells::Compact(cells.into()),
            None => Cells::Wide(points.into()),
        };
        let frozen = Self {
            row,
            y,
            buffer_base,
            data: Arc::new(PointRowData {
                cells,
                source_order: PointOrder::new(source_order),
                x_order: PointOrder::new(x_order),
                original_y: y,
                original_base: buffer_base,
                source_min,
                source_max,
                y_min,
                y_max,
                max_height,
                min_x,
                max_x,
            }),
        };
        if let Some(original) = verify_points {
            assert_eq!(
                frozen.points().collect::<Vec<_>>(),
                original,
                "point rows verify: encoding changed authoritative geometry"
            );
        }
        frozen
    }

    #[inline]
    pub const fn row(&self) -> i64 {
        self.row
    }
    #[inline]
    pub const fn y(&self) -> i64 {
        self.y
    }
    #[inline]
    pub const fn buffer_base(&self) -> i64 {
        self.buffer_base
    }
    #[inline]
    pub fn point_count(&self) -> usize {
        self.data.source_order.len()
    }
    #[inline]
    pub fn has_uniform_y(&self) -> bool {
        self.data.y_min == self.data.y_max
    }
    #[inline]
    pub fn max_height(&self) -> i64 {
        self.data.max_height
    }
    #[inline]
    pub fn min_x(&self) -> Option<i64> {
        self.data.min_x
    }
    /// Right edge including the insertion slot's minimum one-pixel extent.
    #[inline]
    pub fn max_x(&self) -> Option<i64> {
        self.data.max_x
    }
    #[inline]
    pub fn min_buffer_position(&self) -> Option<LispCharPos1> {
        (self.point_count() != 0).then(|| {
            LispCharPos1::new(
                shifted(
                    self.data.source_min,
                    self.data.original_base,
                    self.buffer_base,
                )
                .expect("validated point minimum"),
            )
        })
    }
    #[inline]
    pub fn max_buffer_position(&self) -> Option<LispCharPos1> {
        (self.point_count() != 0).then(|| {
            LispCharPos1::new(
                shifted(
                    self.data.source_max,
                    self.data.original_base,
                    self.buffer_base,
                )
                .expect("validated point maximum"),
            )
        })
    }
    #[inline]
    pub fn is_compact(&self) -> bool {
        matches!(self.data.cells, Cells::Compact(_))
    }

    /// Reject arithmetic overflow without narrowing, clamping or rewriting cells.
    pub fn try_replaced_placement(&self, row: i64, y: i64, delta_pos: i64) -> Option<Self> {
        let buffer_base = self.buffer_base.checked_add(delta_pos)?;
        for value in [self.data.source_min, self.data.source_max] {
            shifted(value, self.data.original_base, buffer_base)?;
        }
        for value in [self.data.y_min, self.data.y_max] {
            shifted(value, self.data.original_y, y)?;
        }
        Some(Self {
            row,
            y,
            buffer_base,
            data: Arc::clone(&self.data),
        })
    }

    /// The producer must establish representable i64 geometry before placement.
    pub fn replaced_placement(&self, row: i64, y: i64, delta_pos: i64) -> Self {
        self.try_replaced_placement(row, y, delta_pos)
            .expect("point placement fits i64")
    }

    #[inline]
    pub fn points(&self) -> DisplayPointRowIter<'_> {
        self.iter_order(&self.data.source_order)
    }

    /// The producer's original walk order, independent of source/x indexes.
    /// Fresh emitters retain this order when one source anchor draws repeatedly.
    #[inline]
    pub fn points_emission_order(
        &self,
    ) -> impl DoubleEndedIterator<Item = DisplayPointSnapshot> + ExactSizeIterator + '_ {
        (0..self.point_count()).map(|index| self.point(index))
    }

    #[inline]
    pub fn points_x_order(&self) -> DisplayPointRowIter<'_> {
        self.iter_order(&self.data.x_order)
    }

    #[inline]
    fn iter_order<'a>(&'a self, order: &'a PointOrder) -> DisplayPointRowIter<'a> {
        DisplayPointRowIter {
            row: self,
            order,
            front: 0,
            back: order.len(),
        }
    }

    #[inline]
    fn point(&self, index: usize) -> DisplayPointSnapshot {
        match &self.data.cells {
            Cells::Compact(cells) => {
                let cell = cells[index];
                DisplayPointSnapshot {
                    buffer_pos: LispCharPos1::new(
                        self.buffer_base + i64::from(cell.position_delta),
                    ),
                    role: match cell.role {
                        PointCellRole::Glyph => DisplayPointRole::Glyph,
                        PointCellRole::OverlaidMarker => DisplayPointRole::OverlaidMarker,
                        PointCellRole::InsertionBoundary => DisplayPointRole::InsertionBoundary,
                        PointCellRole::SyntheticBoundary => DisplayPointRole::SyntheticBoundary,
                    },
                    x: i64::from(cell.x),
                    y: self.y + i64::from(cell.y_offset),
                    width: i64::from(cell.width),
                    height: i64::from(cell.height),
                    row: self.row,
                    col: i64::from(cell.col),
                }
            }
            Cells::Wide(points) => {
                let mut point = points[index].clone();
                point.buffer_pos = LispCharPos1::new(
                    shifted(
                        point.buffer_pos.as_i64(),
                        self.data.original_base,
                        self.buffer_base,
                    )
                    .expect("validated point position"),
                );
                point.y =
                    shifted(point.y, self.data.original_y, self.y).expect("validated point y");
                point.row = self.row;
                point
            }
        }
    }

    /// First exact glyph in walk order, falling back to a marker slot.
    pub fn point_for_buffer_pos(&self, pos: LispCharPos1) -> Option<DisplayPointSnapshot> {
        if pos < self.min_buffer_position()? || pos > self.max_buffer_position()? {
            return None;
        }
        let order = &self.data.source_order;
        let mut lo = 0;
        let mut hi = order.len();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.point(order.get(mid)).buffer_pos < pos {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let mut marker = None;
        for index in lo..order.len() {
            let point = self.point(order.get(index));
            if point.buffer_pos != pos {
                break;
            }
            if point.role.is_position() {
                return Some(point);
            }
            if marker.is_none() {
                marker = Some(point);
            }
        }
        marker
    }

    #[inline]
    pub fn shares_cells_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.data, &other.data)
    }
}

impl std::fmt::Debug for DisplayPointRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DisplayPointRow")
            .field("row", &self.row)
            .field("y", &self.y)
            .field("buffer_base", &self.buffer_base)
            .field("point_count", &self.point_count())
            .field("compact", &self.is_compact())
            .finish()
    }
}

impl PartialEq for DisplayPointRow {
    fn eq(&self, other: &Self) -> bool {
        self.points().eq(other.points())
    }
}
impl Eq for DisplayPointRow {}

pub struct DisplayPointRowIter<'a> {
    row: &'a DisplayPointRow,
    order: &'a PointOrder,
    front: usize,
    back: usize,
}

impl Iterator for DisplayPointRowIter<'_> {
    type Item = DisplayPointSnapshot;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let point = self.row.point(self.order.get(self.front));
        self.front += 1;
        Some(point)
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.back - self.front;
        (len, Some(len))
    }
}
impl DoubleEndedIterator for DisplayPointRowIter<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        Some(self.row.point(self.order.get(self.back)))
    }
}
impl ExactSizeIterator for DisplayPointRowIter<'_> {}

/// Per-window row index. Clone copies only descriptors, never the point arrays.
#[derive(Clone, Default)]
pub struct DisplayPointRows {
    /// Sorted by visual row, with at most one descriptor per row.
    pub rows: Vec<DisplayPointRow>,
}

impl DisplayPointRows {
    pub fn from_points(points: Vec<DisplayPointSnapshot>) -> Self {
        let mut rows: BTreeMap<i64, Vec<DisplayPointSnapshot>> = BTreeMap::new();
        for point in points {
            rows.entry(point.row).or_default().push(point);
        }
        Self {
            rows: rows
                .into_values()
                .map(DisplayPointRow::from_points)
                .collect(),
        }
    }

    #[inline]
    pub fn point_count(&self) -> usize {
        self.rows.iter().map(DisplayPointRow::point_count).sum()
    }
    #[inline]
    pub fn row(&self, row: i64) -> Option<&DisplayPointRow> {
        self.rows
            .binary_search_by_key(&row, DisplayPointRow::row)
            .ok()
            .map(|index| &self.rows[index])
    }

    #[inline]
    pub fn iter_points(&self) -> DisplayPointRowsIter<'_> {
        self.iter_points_with_mode(point_row_iter_mode())
    }

    #[inline]
    fn iter_points_with_mode(&self, mode: PointRowIterMode) -> DisplayPointRowsIter<'_> {
        if mode == PointRowIterMode::On
            && let Some(remaining) = self.disjoint_point_count()
        {
            return DisplayPointRowsIter {
                rows: self,
                order: PointRowsIterOrder::Concat {
                    row_index: 0,
                    source_index: 0,
                },
                remaining,
            };
        }
        let mut heap = BinaryHeap::new();
        for (row_index, row) in self.rows.iter().enumerate() {
            if let Some(point) = row.points().next() {
                heap.push(Reverse((point.buffer_pos.as_i64(), row_index, 0)));
            }
        }
        DisplayPointRowsIter {
            rows: self,
            order: PointRowsIterOrder::Merge { heap },
            remaining: self.point_count(),
        }
    }

    /// Certify the current vector order without decoding or sorting points.
    /// Equal source endpoints retain the heap's lower-vector-index precedence.
    /// Empty rows have no interval. Do not cache this proof on a public vector
    /// that a producer can reorder or replace before the next immutable borrow.
    #[inline]
    fn disjoint_point_count(&self) -> Option<usize> {
        let mut previous_max = None;
        let mut count = 0;
        for row in &self.rows {
            let row_count = row.point_count();
            count += row_count;
            if row_count == 0 {
                continue;
            }
            let min = row.min_buffer_position()?.as_i64();
            let max = row.max_buffer_position()?.as_i64();
            if previous_max.is_some_and(|previous| previous > min) {
                return None;
            }
            previous_max = Some(max);
        }
        Some(count)
    }

    pub fn point_for_buffer_pos(&self, pos: LispCharPos1) -> Option<DisplayPointSnapshot> {
        let mut marker = None;
        for row in &self.rows {
            if let Some(point) = row.point_for_buffer_pos(pos) {
                if point.role.is_position() {
                    return Some(point);
                }
                if marker.is_none() {
                    marker = Some(point);
                }
            }
        }
        marker
    }
}

impl std::fmt::Debug for DisplayPointRows {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DisplayPointRows")
            .field("row_count", &self.rows.len())
            .field("point_count", &self.point_count())
            .finish()
    }
}
impl PartialEq for DisplayPointRows {
    fn eq(&self, other: &Self) -> bool {
        self.iter_points().eq(other.iter_points())
    }
}
impl Eq for DisplayPointRows {}

/// Owned numeric traversal state over immutable shared cell arrays. A live
/// borrow prevents producer mutation; concurrent readers share no cursor state.
enum PointRowsIterOrder {
    Merge {
        heap: BinaryHeap<Reverse<(i64, usize, usize)>>,
    },
    Concat {
        row_index: usize,
        source_index: usize,
    },
}

/// One immutable row-vector borrow with independent numeric traversal state.
/// This iterator owns neither Lisp values nor mutator-local caches.
pub struct DisplayPointRowsIter<'a> {
    rows: &'a DisplayPointRows,
    order: PointRowsIterOrder,
    remaining: usize,
}

impl Iterator for DisplayPointRowsIter<'_> {
    type Item = DisplayPointSnapshot;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let point = match &mut self.order {
            PointRowsIterOrder::Merge { heap } => {
                let Reverse((_, row_index, source_index)) = heap.pop()?;
                let row = &self.rows.rows[row_index];
                let point = row.point(row.data.source_order.get(source_index));
                let next = source_index + 1;
                if next < row.point_count() {
                    let next_point = row.point(row.data.source_order.get(next));
                    heap.push(Reverse((next_point.buffer_pos.as_i64(), row_index, next)));
                }
                point
            }
            PointRowsIterOrder::Concat {
                row_index,
                source_index,
            } => loop {
                let row = self.rows.rows.get(*row_index)?;
                if *source_index == row.point_count() {
                    *row_index += 1;
                    *source_index = 0;
                    continue;
                }
                let point = row.point(row.data.source_order.get(*source_index));
                *source_index += 1;
                break point;
            },
        };
        self.remaining -= 1;
        Some(point)
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
impl ExactSizeIterator for DisplayPointRowsIter<'_> {}
impl std::iter::FusedIterator for DisplayPointRowsIter<'_> {}

pub(super) enum WindowDisplayPointIter<'a> {
    Flat(std::iter::Cloned<std::slice::Iter<'a, DisplayPointSnapshot>>),
    Rows(DisplayPointRowsIter<'a>),
}

impl<'a> WindowDisplayPointIter<'a> {
    #[inline]
    pub(super) fn new(
        points: &'a [DisplayPointSnapshot],
        point_rows: Option<&'a DisplayPointRows>,
    ) -> Self {
        match point_rows {
            Some(rows) => Self::Rows(rows.iter_points()),
            None => Self::Flat(points.iter().cloned()),
        }
    }
}

impl Iterator for WindowDisplayPointIter<'_> {
    type Item = DisplayPointSnapshot;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Flat(points) => points.next(),
            Self::Rows(points) => points.next(),
        }
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Flat(points) => points.size_hint(),
            Self::Rows(points) => points.size_hint(),
        }
    }
}
impl ExactSizeIterator for WindowDisplayPointIter<'_> {}
impl std::iter::FusedIterator for WindowDisplayPointIter<'_> {}

#[cfg(test)]
#[path = "tests/point_rows_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/point_rows_iterator_test.rs"]
mod iterator_tests;

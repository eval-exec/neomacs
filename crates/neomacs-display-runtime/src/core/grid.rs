//! Character grid for storing complete window content.
//!
//! This follows the Neovide model: store complete cell content per window,
//! allowing the renderer to clear and redraw each frame without dealing
//! with incremental updates.

use std::sync::Arc;

use super::types::Color;

/// A single cell in the character grid
#[derive(Debug, Clone)]
pub struct GridCell {
    /// Character to display (as UTF-8 string for multi-byte support)
    pub text: String,
    /// Style for this cell
    pub style: Option<Arc<CellStyle>>,
    /// Cell width (1 for normal, 2 for wide chars)
    pub width: u8,
}

impl Default for GridCell {
    fn default() -> Self {
        Self {
            text: " ".to_string(),
            style: None,
            width: 1,
        }
    }
}

/// Style for a grid cell
#[derive(Debug, Clone, PartialEq)]
pub struct CellStyle {
    /// Foreground color
    pub fg: Color,
    /// Background color (None = transparent/default)
    pub bg: Option<Color>,
    /// Bold
    pub bold: bool,
    /// Italic
    pub italic: bool,
    /// Underline style (0=none, 1=single, 2=double, 3=wave)
    pub underline: u8,
    /// Underline color (None = use fg)
    pub underline_color: Option<Color>,
    /// Strikethrough
    pub strikethrough: bool,
}

impl Default for CellStyle {
    fn default() -> Self {
        Self {
            fg: Color::BLACK,
            bg: None,
            bold: false,
            italic: false,
            underline: 0,
            underline_color: None,
            strikethrough: false,
        }
    }
}

/// A single row in the grid
#[derive(Debug, Clone)]
pub struct GridLine {
    /// Cells in this row
    pub cells: Vec<GridCell>,
    /// Whether this line has been modified since last render
    pub dirty: bool,
}

impl GridLine {
    pub fn new(width: usize) -> Self {
        Self {
            cells: vec![GridCell::default(); width],
            dirty: true,
        }
    }

    pub fn resize(&mut self, width: usize) {
        self.cells.resize(width, GridCell::default());
        self.dirty = true;
    }

    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            *cell = GridCell::default();
        }
        self.dirty = true;
    }
}

/// Character grid storing complete window content
#[derive(Debug)]
pub struct CharacterGrid {
    /// Grid dimensions
    pub width: usize,
    pub height: usize,

    /// Lines in the grid (ring buffer for efficient scrolling)
    lines: Vec<GridLine>,

    /// Ring buffer offset for O(1) scrolling
    scroll_offset: isize,
}

impl CharacterGrid {
    pub fn new(width: usize, height: usize) -> Self {
        let lines = (0..height).map(|_| GridLine::new(width)).collect();
        Self {
            width,
            height,
            lines,
            scroll_offset: 0,
        }
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        // Reset scroll offset on resize
        self.scroll_offset = 0;

        // Resize existing lines
        for line in &mut self.lines {
            line.resize(width);
        }

        // Add or remove lines
        self.lines.resize_with(height, || GridLine::new(width));

        self.width = width;
        self.height = height;
    }

    pub fn clear(&mut self) {
        self.scroll_offset = 0;
        for line in &mut self.lines {
            line.clear();
        }
    }

    /// Get a cell at (x, y) - handles ring buffer offset
    pub fn get_cell(&self, x: usize, y: usize) -> Option<&GridCell> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let actual_y = self.actual_row(y);
        self.lines.get(actual_y)?.cells.get(x)
    }

    /// Get mutable cell at (x, y)
    pub fn get_cell_mut(&mut self, x: usize, y: usize) -> Option<&mut GridCell> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let actual_y = self.actual_row(y);
        self.lines.get_mut(actual_y)?.cells.get_mut(x)
    }

    /// Get a row
    pub fn get_row(&self, y: usize) -> Option<&GridLine> {
        if y >= self.height {
            return None;
        }
        let actual_y = self.actual_row(y);
        self.lines.get(actual_y)
    }

    /// Get mutable row
    pub fn get_row_mut(&mut self, y: usize) -> Option<&mut GridLine> {
        if y >= self.height {
            return None;
        }
        let actual_y = self.actual_row(y);
        self.lines.get_mut(actual_y)
    }

    /// Mark a row as dirty
    pub fn mark_row_dirty(&mut self, y: usize) {
        if let Some(row) = self.get_row_mut(y) {
            row.dirty = true;
        }
    }

    /// Mark a row as clean (rendered)
    pub fn mark_row_clean(&mut self, y: usize) {
        if let Some(row) = self.get_row_mut(y) {
            row.dirty = false;
        }
    }

    /// Check if a row is dirty
    pub fn is_row_dirty(&self, y: usize) -> bool {
        self.get_row(y).map(|r| r.dirty).unwrap_or(false)
    }

    /// Scroll the grid by `rows` (positive = down, negative = up)
    /// Uses ring buffer rotation for O(1) performance
    pub fn scroll(&mut self, rows: isize) {
        self.scroll_offset = (self.scroll_offset + rows).rem_euclid(self.height as isize);
        // Mark all rows as dirty after scroll
        for line in &mut self.lines {
            line.dirty = true;
        }
    }

    /// Scroll a region of the grid
    pub fn scroll_region(
        &mut self,
        top: usize,
        bottom: usize,
        left: usize,
        right: usize,
        rows: isize,
        cols: isize,
    ) {
        // Full-width vertical scroll can use ring buffer
        if left == 0 && right == self.width && cols == 0 && top == 0 && bottom == self.height {
            self.scroll(rows);
            return;
        }

        // Partial scroll requires copying cells
        if rows > 0 {
            // Scroll down: copy from top to bottom
            for y in ((top as isize + rows) as usize..bottom).rev() {
                let src_y = (y as isize - rows) as usize;
                for x in left..right {
                    if let Some(src_cell) = self.get_cell(x, src_y).cloned() {
                        if let Some(dst_cell) = self.get_cell_mut(x, y) {
                            *dst_cell = src_cell;
                        }
                    }
                }
            }
        } else if rows < 0 {
            // Scroll up: copy from bottom to top
            for y in top..(bottom as isize + rows) as usize {
                let src_y = (y as isize - rows) as usize;
                for x in left..right {
                    if let Some(src_cell) = self.get_cell(x, src_y).cloned() {
                        if let Some(dst_cell) = self.get_cell_mut(x, y) {
                            *dst_cell = src_cell;
                        }
                    }
                }
            }
        }

        // Mark affected rows as dirty
        for y in top..bottom {
            self.mark_row_dirty(y);
        }
    }

    /// Set a cell's content
    pub fn set_cell(&mut self, x: usize, y: usize, text: String, style: Option<Arc<CellStyle>>, width: u8) {
        if let Some(cell) = self.get_cell_mut(x, y) {
            cell.text = text;
            cell.style = style;
            cell.width = width;
        }
        self.mark_row_dirty(y);
    }

    /// Update a range of cells in a row from a vector of (text, style) pairs
    pub fn update_row(&mut self, y: usize, col_start: usize, cells: &[(String, Option<Arc<CellStyle>>)]) {
        let mut x = col_start;
        for (text, style) in cells {
            if x >= self.width {
                break;
            }
            let width = if text.chars().next().map_or(false, |c| {
                // CJK Unified Ideographs and common wide character ranges
                let cp = c as u32;
                (0x1100..=0x115F).contains(&cp)   // Hangul Jamo
                || (0x2E80..=0x303E).contains(&cp) // CJK Radicals, Kangxi, CJK Symbols
                || (0x3041..=0x33BF).contains(&cp) // Hiragana, Katakana, CJK Compat
                || (0x3400..=0x4DBF).contains(&cp) // CJK Unified Ext A
                || (0x4E00..=0x9FFF).contains(&cp) // CJK Unified Ideographs
                || (0xA000..=0xA4CF).contains(&cp) // Yi
                || (0xAC00..=0xD7AF).contains(&cp) // Hangul Syllables
                || (0xF900..=0xFAFF).contains(&cp) // CJK Compat Ideographs
                || (0xFE30..=0xFE6F).contains(&cp) // CJK Compat Forms
                || (0xFF01..=0xFF60).contains(&cp) // Fullwidth Forms
                || (0xFFE0..=0xFFE6).contains(&cp) // Fullwidth Signs
                || (0x20000..=0x2FA1F).contains(&cp) // CJK Unified Ext B-F + Compat Supp
                || (0x30000..=0x3134F).contains(&cp) // CJK Unified Ext G-H
            }) { 2u8 } else { 1u8 };
            if let Some(cell) = self.get_cell_mut(x, y) {
                cell.text = text.clone();
                cell.style = style.clone();
                cell.width = width;
            }
            x += width as usize;
        }
        self.mark_row_dirty(y);
    }

    /// Convert logical row to actual row index (handling ring buffer offset)
    fn actual_row(&self, y: usize) -> usize {
        ((y as isize + self.scroll_offset).rem_euclid(self.height as isize)) as usize
    }

    /// Iterator over all rows (in display order)
    pub fn rows(&self) -> impl Iterator<Item = (usize, &GridLine)> {
        (0..self.height).map(move |y| {
            let actual_y = self.actual_row(y);
            (y, &self.lines[actual_y])
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let mut grid = CharacterGrid::new(10, 5);

        // Set a cell
        grid.set_cell(0, 0, "A".to_string(), None, 1);
        assert_eq!(grid.get_cell(0, 0).unwrap().text, "A");

        // Row should be dirty
        assert!(grid.is_row_dirty(0));

        // Mark clean
        grid.mark_row_clean(0);
        assert!(!grid.is_row_dirty(0));
    }

    #[test]
    fn test_scroll() {
        let mut grid = CharacterGrid::new(10, 5);
        grid.set_cell(0, 0, "A".to_string(), None, 1);

        // Scroll down by 1
        grid.scroll(1);

        // Row 0 content should now be at row 4 (wrapped)
        // But actually the content stays in place, view shifts
        // So row 0 now shows what was at row -1 (which wraps to row 4)
    }

    #[test]
    fn test_resize() {
        let mut grid = CharacterGrid::new(10, 5);
        grid.set_cell(0, 0, "A".to_string(), None, 1);

        grid.resize(20, 10);
        assert_eq!(grid.width, 20);
        assert_eq!(grid.height, 10);

        // Original cell should still be there
        assert_eq!(grid.get_cell(0, 0).unwrap().text, "A");
    }

    #[test]
    fn test_update_row_ascii_width() {
        let mut grid = CharacterGrid::new(20, 5);
        let cells = vec![
            ("H".to_string(), None),
            ("e".to_string(), None),
            ("l".to_string(), None),
            ("l".to_string(), None),
            ("o".to_string(), None),
        ];
        grid.update_row(0, 0, &cells);

        // Each ASCII character should have width=1
        for x in 0..5 {
            assert_eq!(grid.get_cell(x, 0).unwrap().width, 1,
                "ASCII char at column {} should have width 1", x);
        }
        // Verify text content
        assert_eq!(grid.get_cell(0, 0).unwrap().text, "H");
        assert_eq!(grid.get_cell(4, 0).unwrap().text, "o");
    }

    #[test]
    fn test_update_row_cjk_unified_ideographs() {
        // CJK Unified Ideographs: U+4E00..U+9FFF
        // '中' = U+4E2D, '文' = U+6587
        let mut grid = CharacterGrid::new(20, 5);
        let cells = vec![
            ("\u{4E2D}".to_string(), None),  // 中
            ("\u{6587}".to_string(), None),  // 文
        ];
        grid.update_row(0, 0, &cells);

        // First CJK char at column 0, width=2
        assert_eq!(grid.get_cell(0, 0).unwrap().text, "\u{4E2D}");
        assert_eq!(grid.get_cell(0, 0).unwrap().width, 2);

        // Second CJK char at column 2 (0 + width 2), width=2
        assert_eq!(grid.get_cell(2, 0).unwrap().text, "\u{6587}");
        assert_eq!(grid.get_cell(2, 0).unwrap().width, 2);
    }

    #[test]
    fn test_update_row_fullwidth_forms() {
        // Fullwidth Forms: U+FF01..U+FF60
        // 'Ａ' = U+FF21, 'Ｂ' = U+FF22
        let mut grid = CharacterGrid::new(20, 5);
        let cells = vec![
            ("\u{FF21}".to_string(), None),  // Ａ
            ("\u{FF22}".to_string(), None),  // Ｂ
        ];
        grid.update_row(0, 0, &cells);

        assert_eq!(grid.get_cell(0, 0).unwrap().text, "\u{FF21}");
        assert_eq!(grid.get_cell(0, 0).unwrap().width, 2);

        assert_eq!(grid.get_cell(2, 0).unwrap().text, "\u{FF22}");
        assert_eq!(grid.get_cell(2, 0).unwrap().width, 2);
    }

    #[test]
    fn test_update_row_hangul_syllables() {
        // Hangul Syllables: U+AC00..U+D7AF
        // '한' = U+D55C, '글' = U+AE00
        let mut grid = CharacterGrid::new(20, 5);
        let cells = vec![
            ("\u{D55C}".to_string(), None),  // 한
            ("\u{AE00}".to_string(), None),  // 글
        ];
        grid.update_row(0, 0, &cells);

        assert_eq!(grid.get_cell(0, 0).unwrap().text, "\u{D55C}");
        assert_eq!(grid.get_cell(0, 0).unwrap().width, 2);

        assert_eq!(grid.get_cell(2, 0).unwrap().text, "\u{AE00}");
        assert_eq!(grid.get_cell(2, 0).unwrap().width, 2);
    }

    #[test]
    fn test_update_row_column_advance() {
        // Verify that column positions advance by the character's width
        let mut grid = CharacterGrid::new(20, 5);
        let cells = vec![
            ("\u{4E2D}".to_string(), None),  // 中 (width=2, occupies col 0-1)
            ("A".to_string(), None),          // A  (width=1, occupies col 2)
            ("\u{D55C}".to_string(), None),  // 한 (width=2, occupies col 3-4)
            ("B".to_string(), None),          // B  (width=1, occupies col 5)
        ];
        grid.update_row(0, 0, &cells);

        // '中' at column 0
        assert_eq!(grid.get_cell(0, 0).unwrap().text, "\u{4E2D}");
        assert_eq!(grid.get_cell(0, 0).unwrap().width, 2);

        // 'A' at column 2
        assert_eq!(grid.get_cell(2, 0).unwrap().text, "A");
        assert_eq!(grid.get_cell(2, 0).unwrap().width, 1);

        // '한' at column 3
        assert_eq!(grid.get_cell(3, 0).unwrap().text, "\u{D55C}");
        assert_eq!(grid.get_cell(3, 0).unwrap().width, 2);

        // 'B' at column 5
        assert_eq!(grid.get_cell(5, 0).unwrap().text, "B");
        assert_eq!(grid.get_cell(5, 0).unwrap().width, 1);
    }

    #[test]
    fn test_update_row_mixed_ascii_and_wide() {
        // A complete mixed-content row:
        // "Hi中文ok" should lay out as:
        //   col 0: 'H' (w=1)
        //   col 1: 'i' (w=1)
        //   col 2: '中' (w=2)
        //   col 4: '文' (w=2)
        //   col 6: 'o' (w=1)
        //   col 7: 'k' (w=1)
        let mut grid = CharacterGrid::new(20, 5);
        let cells = vec![
            ("H".to_string(), None),
            ("i".to_string(), None),
            ("\u{4E2D}".to_string(), None),  // 中
            ("\u{6587}".to_string(), None),  // 文
            ("o".to_string(), None),
            ("k".to_string(), None),
        ];
        grid.update_row(0, 0, &cells);

        assert_eq!(grid.get_cell(0, 0).unwrap().text, "H");
        assert_eq!(grid.get_cell(0, 0).unwrap().width, 1);

        assert_eq!(grid.get_cell(1, 0).unwrap().text, "i");
        assert_eq!(grid.get_cell(1, 0).unwrap().width, 1);

        assert_eq!(grid.get_cell(2, 0).unwrap().text, "\u{4E2D}");
        assert_eq!(grid.get_cell(2, 0).unwrap().width, 2);

        assert_eq!(grid.get_cell(4, 0).unwrap().text, "\u{6587}");
        assert_eq!(grid.get_cell(4, 0).unwrap().width, 2);

        assert_eq!(grid.get_cell(6, 0).unwrap().text, "o");
        assert_eq!(grid.get_cell(6, 0).unwrap().width, 1);

        assert_eq!(grid.get_cell(7, 0).unwrap().text, "k");
        assert_eq!(grid.get_cell(7, 0).unwrap().width, 1);

        // The row should be marked dirty
        assert!(grid.is_row_dirty(0));
    }

    #[test]
    fn test_update_row_col_start_offset() {
        // Verify that col_start offsets wide character placement
        let mut grid = CharacterGrid::new(20, 5);
        let cells = vec![
            ("\u{4E2D}".to_string(), None),  // 中 (width=2)
        ];
        grid.update_row(0, 5, &cells);

        // '中' should be placed at column 5
        assert_eq!(grid.get_cell(5, 0).unwrap().text, "\u{4E2D}");
        assert_eq!(grid.get_cell(5, 0).unwrap().width, 2);

        // Columns before 5 should be untouched (default space)
        assert_eq!(grid.get_cell(0, 0).unwrap().text, " ");
        assert_eq!(grid.get_cell(4, 0).unwrap().text, " ");
    }
}

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
            if let Some(cell) = self.get_cell_mut(x, y) {
                cell.text = text.clone();
                cell.style = style.clone();
                cell.width = 1; // TODO: handle wide chars
            }
            x += 1;
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
}

//! Basic types used throughout the display engine.

use std::ops::{Add, Sub, Mul};

/// RGBA color with f32 components (0.0 - 1.0)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::new(r, g, b, 1.0)
    }

    pub fn from_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// Convert from Emacs pixel value (0xAARRGGBB or 0x00RRGGBB)
    pub fn from_pixel(pixel: u32) -> Self {
        let a = ((pixel >> 24) & 0xFF) as u8;
        let r = ((pixel >> 16) & 0xFF) as u8;
        let g = ((pixel >> 8) & 0xFF) as u8;
        let b = (pixel & 0xFF) as u8;
        // If alpha is 0, assume fully opaque
        let a = if a == 0 { 255 } else { a };
        Self::from_u8(r, g, b, a)
    }

    #[cfg(feature = "gtk4-backend")]
    pub fn to_gdk(&self) -> gdk4::RGBA {
        gdk4::RGBA::new(self.r, self.g, self.b, self.a)
    }

    // Common colors
    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);
    pub const RED: Self = Self::rgb(1.0, 0.0, 0.0);
    pub const GREEN: Self = Self::rgb(0.0, 1.0, 0.0);
    pub const BLUE: Self = Self::rgb(0.0, 0.0, 1.0);
    pub const TRANSPARENT: Self = Self::new(0.0, 0.0, 0.0, 0.0);
}

impl Default for Color {
    fn default() -> Self {
        Self::BLACK
    }
}

/// 2D point with f32 coordinates
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub const ZERO: Self = Self::new(0.0, 0.0);

    #[cfg(feature = "gtk4-backend")]
    pub fn to_graphene(&self) -> gtk4::graphene::Point {
        gtk4::graphene::Point::new(self.x, self.y)
    }
}

impl Add for Point {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
}

impl Sub for Point {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }
}

/// 2D size with f32 dimensions
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    pub const ZERO: Self = Self::new(0.0, 0.0);

    #[cfg(feature = "gtk4-backend")]
    pub fn to_graphene(&self) -> gtk4::graphene::Size {
        gtk4::graphene::Size::new(self.width, self.height)
    }
}

/// Rectangle with position and size
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    pub fn from_point_size(point: Point, size: Size) -> Self {
        Self::new(point.x, point.y, size.width, size.height)
    }

    pub fn origin(&self) -> Point {
        Point::new(self.x, self.y)
    }

    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.x
            && point.x < self.right()
            && point.y >= self.y
            && point.y < self.bottom()
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }

    #[cfg(feature = "gtk4-backend")]
    pub fn to_graphene(&self) -> gtk4::graphene::Rect {
        gtk4::graphene::Rect::new(self.x, self.y, self.width, self.height)
    }

    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0, 0.0);
}

/// 2D transform matrix
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// 2D affine transform: [a, b, c, d, tx, ty]
    /// | a  b  0 |
    /// | c  d  0 |
    /// | tx ty 1 |
    pub matrix: [f32; 6],
}

impl Transform {
    pub const IDENTITY: Self = Self {
        matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    };

    pub fn translate(tx: f32, ty: f32) -> Self {
        Self {
            matrix: [1.0, 0.0, 0.0, 1.0, tx, ty],
        }
    }

    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            matrix: [sx, 0.0, 0.0, sy, 0.0, 0.0],
        }
    }

    #[cfg(feature = "gtk4-backend")]
    pub fn to_gsk(&self) -> gsk4::Transform {
        gsk4::Transform::new()
            .translate(&gtk4::graphene::Point::new(self.matrix[4], self.matrix[5]))
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_from_pixel() {
        let color = Color::from_pixel(0x00FF8040);
        assert!((color.r - 1.0).abs() < 0.01);
        assert!((color.g - 0.5).abs() < 0.01);
        assert!((color.b - 0.25).abs() < 0.01);
        assert!((color.a - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_rect_contains() {
        let rect = Rect::new(10.0, 10.0, 100.0, 50.0);
        assert!(rect.contains(Point::new(50.0, 30.0)));
        assert!(!rect.contains(Point::new(5.0, 30.0)));
    }
}

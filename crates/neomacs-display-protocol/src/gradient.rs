//! GPU-accelerated gradient backgrounds for faces.
//!
//! Supports linear, radial, conic gradients and procedural noise patterns.
//! Gradients are rendered on the GPU during fragment shader execution,
//! allowing efficient per-pixel gradient evaluation without CPU overhead.

use crate::types::Color;
use std::fmt;

/// A color stop in a gradient (position and color pair).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ColorStop {
    /// Position along gradient (0.0 = start, 1.0 = end)
    pub position: f32,
    /// Color at this stop
    pub color: Color,
}

impl ColorStop {
    pub fn new(position: f32, color: Color) -> Self {
        Self { position, color }
    }
}

/// Gradient background type.
///
/// These are rendered on the GPU, making them zero-cost for CPU execution.
/// All gradient types are normalized to 0.0-1.0 coordinates within the region
/// being rendered (e.g., a glyph cell or window region).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Gradient {
    /// Linear gradient sweeping in a given direction.
    ///
    /// # Example
    /// ```ignore
    /// Gradient::Linear {
    ///     angle: 90.0,  // vertical (top to bottom)
    ///     stops: vec![
    ///         ColorStop::new(0.0, Color::rgb(1.0, 0.0, 0.0)),    // red at top
    ///         ColorStop::new(1.0, Color::rgb(0.0, 0.0, 1.0)),    // blue at bottom
    ///     ],
    /// }
    /// ```
    Linear {
        /// Angle in degrees (0° = left→right, 90° = top→bottom)
        angle: f32,
        /// Color stops along the gradient
        stops: Vec<ColorStop>,
    },

    /// Radial gradient emanating from a center point.
    ///
    /// # Example
    /// ```ignore
    /// Gradient::Radial {
    ///     center_x: 0.5,
    ///     center_y: 0.5,
    ///     radius: 0.7,
    ///     stops: vec![
    ///         ColorStop::new(0.0, Color::rgb(1.0, 1.0, 1.0)),    // white at center
    ///         ColorStop::new(1.0, Color::rgb(0.0, 0.0, 0.0)),    // black at edge
    ///     ],
    /// }
    /// ```
    Radial {
        /// Center X in normalized space (0.0-1.0)
        center_x: f32,
        /// Center Y in normalized space (0.0-1.0)
        center_y: f32,
        /// Radius as fraction of region size (0.0-1.0)
        radius: f32,
        /// Color stops from center to edge
        stops: Vec<ColorStop>,
    },

    /// Conic gradient sweeping around a center point (like a spinner/pie).
    ///
    /// # Example
    /// ```ignore
    /// Gradient::Conic {
    ///     center_x: 0.5,
    ///     center_y: 0.5,
    ///     angle_offset: 0.0,
    ///     stops: vec![
    ///         ColorStop::new(0.0, Color::rgb(1.0, 0.0, 0.0)),
    ///         ColorStop::new(0.33, Color::rgb(0.0, 1.0, 0.0)),
    ///         ColorStop::new(0.66, Color::rgb(0.0, 0.0, 1.0)),
    ///         ColorStop::new(1.0, Color::rgb(1.0, 0.0, 0.0)),
    ///     ],
    /// }
    /// ```
    Conic {
        /// Center X in normalized space (0.0-1.0)
        center_x: f32,
        /// Center Y in normalized space (0.0-1.0)
        center_y: f32,
        /// Starting angle in degrees (rotation of the gradient)
        angle_offset: f32,
        /// Color stops around the circle (0.0-1.0 = 0°-360°)
        stops: Vec<ColorStop>,
    },

    /// Procedural Perlin noise pattern (2D simplex noise).
    ///
    /// Useful for organic-looking backgrounds without explicit gradient definition.
    /// The noise ranges from `color1` to `color2`.
    ///
    /// # Example
    /// ```ignore
    /// Gradient::Noise {
    ///     scale: 5.0,  // 5 tiles of noise pattern
    ///     octaves: 3,  // 3 levels of detail
    ///     color1: Color::rgb(0.1, 0.1, 0.2),  // dark blue
    ///     color2: Color::rgb(0.3, 0.5, 0.8),  // lighter blue
    /// }
    /// ```
    Noise {
        /// Frequency of noise pattern (higher = smaller features)
        scale: f32,
        /// Number of octaves (layers) of noise for detail
        octaves: u32,
        /// Primary color for noise
        color1: Color,
        /// Secondary color for noise
        color2: Color,
    },
}

impl Gradient {
    /// Get the minimum and maximum X/Y coordinates needed to render this gradient.
    /// Used for optimization (clipping, bounds checking).
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        match self {
            Gradient::Linear { .. } => None, // Gradients typically fill entire region
            Gradient::Radial {
                center_x,
                center_y,
                radius,
                ..
            } => {
                let min_x = (center_x - radius).max(0.0);
                let max_x = (center_x + radius).min(1.0);
                let min_y = (center_y - radius).max(0.0);
                let max_y = (center_y + radius).min(1.0);
                Some((min_x, min_y, max_x, max_y))
            }
            Gradient::Conic { .. } => None, // Typically fills entire region
            Gradient::Noise { .. } => None,
        }
    }

    /// Validate gradient parameters (e.g., stops in order, valid ranges).
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Gradient::Linear { angle: _, stops } => {
                if stops.is_empty() {
                    return Err("Linear gradient must have at least one color stop".to_string());
                }
                if stops.len() < 2 {
                    return Err("Linear gradient should have at least 2 color stops".to_string());
                }
                let mut prev_pos = -1.0f32;
                for stop in stops {
                    if stop.position < 0.0 || stop.position > 1.0 {
                        return Err(format!(
                            "Color stop position {} out of range [0.0, 1.0]",
                            stop.position
                        ));
                    }
                    if stop.position < prev_pos {
                        return Err("Color stops must be in increasing position order".to_string());
                    }
                    prev_pos = stop.position;
                }
                Ok(())
            }
            Gradient::Radial {
                center_x,
                center_y,
                radius,
                stops,
            } => {
                if *center_x < 0.0 || *center_x > 1.0 {
                    return Err(format!(
                        "Radial gradient center_x {} out of range [0.0, 1.0]",
                        center_x
                    ));
                }
                if *center_y < 0.0 || *center_y > 1.0 {
                    return Err(format!(
                        "Radial gradient center_y {} out of range [0.0, 1.0]",
                        center_y
                    ));
                }
                if *radius <= 0.0 || *radius > 1.0 {
                    return Err(format!(
                        "Radial gradient radius {} out of range (0.0, 1.0]",
                        radius
                    ));
                }
                if stops.is_empty() {
                    return Err("Radial gradient must have at least one color stop".to_string());
                }
                Ok(())
            }
            Gradient::Conic {
                center_x, center_y, ..
            } => {
                if *center_x < 0.0 || *center_x > 1.0 {
                    return Err(format!(
                        "Conic gradient center_x {} out of range [0.0, 1.0]",
                        center_x
                    ));
                }
                if *center_y < 0.0 || *center_y > 1.0 {
                    return Err(format!(
                        "Conic gradient center_y {} out of range [0.0, 1.0]",
                        center_y
                    ));
                }
                Ok(())
            }
            Gradient::Noise { scale, octaves, .. } => {
                if *scale <= 0.0 {
                    return Err("Noise scale must be positive".to_string());
                }
                if *octaves == 0 {
                    return Err("Noise octaves must be at least 1".to_string());
                }
                Ok(())
            }
        }
    }
}

impl fmt::Display for Gradient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Gradient::Linear { angle, stops } => {
                write!(f, "Linear(angle={}°, stops={})", angle, stops.len())
            }
            Gradient::Radial {
                center_x,
                center_y,
                radius,
                stops,
            } => {
                write!(
                    f,
                    "Radial(center=({:.1}, {:.1}), radius={:.1}, stops={})",
                    center_x,
                    center_y,
                    radius,
                    stops.len()
                )
            }
            Gradient::Conic {
                center_x,
                center_y,
                stops,
                ..
            } => {
                write!(
                    f,
                    "Conic(center=({:.1}, {:.1}), stops={})",
                    center_x,
                    center_y,
                    stops.len()
                )
            }
            Gradient::Noise { scale, octaves, .. } => {
                write!(f, "Noise(scale={:.1}, octaves={})", scale, octaves)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_gradient_validation() {
        let grad = Gradient::Linear {
            angle: 45.0,
            stops: vec![
                ColorStop::new(0.0, Color::rgb(1.0, 0.0, 0.0)),
                ColorStop::new(1.0, Color::rgb(0.0, 0.0, 1.0)),
            ],
        };
        assert!(grad.validate().is_ok());

        // Invalid: out of order
        let bad = Gradient::Linear {
            angle: 45.0,
            stops: vec![
                ColorStop::new(1.0, Color::rgb(0.0, 0.0, 1.0)),
                ColorStop::new(0.0, Color::rgb(1.0, 0.0, 0.0)),
            ],
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn radial_gradient_validation() {
        let grad = Gradient::Radial {
            center_x: 0.5,
            center_y: 0.5,
            radius: 0.7,
            stops: vec![ColorStop::new(0.0, Color::rgb(1.0, 1.0, 1.0))],
        };
        assert!(grad.validate().is_ok());

        // Invalid: center out of bounds
        let bad = Gradient::Radial {
            center_x: 1.5,
            center_y: 0.5,
            radius: 0.7,
            stops: vec![ColorStop::new(0.0, Color::rgb(1.0, 1.0, 1.0))],
        };
        assert!(bad.validate().is_err());
    }
}

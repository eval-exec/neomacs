//! Coherent cursor paint and motion presentation.
//!
//! This module is the single seam between GNU-compatible resolved cursor
//! paint and Neomacs-only visual effects.  Callers receive a presentation
//! whose type prevents an in-flight filled box from also claiming that the
//! destination glyph is already covered for inverse video.

use neomacs_display_protocol::CursorColorCycleConfig;
use neomacs_display_protocol::frame_glyphs::DisplaySlotId;
use neomacs_display_protocol::types::{AnimatedCursor, Color, DisplayWindowId, Rect};

use super::WgpuRenderer;

const SETTLED_GEOMETRY_EPSILON: f32 = 0.01;

/// GNU-compatible paint resolved by layout from the cursor face, glyph face,
/// and frame cursor colors.  Visual effects cannot construct this type from a
/// post-effect color accidentally.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ResolvedCursorPaint {
    box_background: Color,
    glyph_foreground: Color,
}

impl ResolvedCursorPaint {
    pub(super) const fn new(box_background: Color, glyph_foreground: Color) -> Self {
        Self {
            box_background,
            glyph_foreground,
        }
    }
}

/// Explicit policy for translating GNU's resolved paint into a Neomacs
/// presentation.  Error-pulse override and ambient cycling are mutually
/// exclusive states instead of order-dependent optional colors.
#[derive(Clone, Copy, Debug)]
pub(super) enum CursorColorPolicy<'a> {
    Inherit,
    Cycle {
        config: &'a CursorColorCycleConfig,
        origin: crate::clock::Instant,
    },
    Override(Color),
}

/// Cursor paint after one explicit visual color policy is applied. Keeping
/// the box and glyph colors together prevents renderer passes from
/// independently re-resolving half of the inverse-video pair.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PresentedCursorPaint {
    pub(super) body_background: Color,
    pub(super) glyph_foreground: Color,
}

impl PresentedCursorPaint {
    pub(super) fn resolve(
        resolved: ResolvedCursorPaint,
        policy: CursorColorPolicy<'_>,
        sample_time: crate::clock::Instant,
    ) -> Self {
        let body_background = match policy {
            CursorColorPolicy::Inherit => resolved.box_background,
            CursorColorPolicy::Cycle { config, origin } => {
                cursor_color_cycle_color_at(resolved.box_background, config, sample_time, origin)
            }
            CursorColorPolicy::Override(background) => background,
        };
        Self {
            body_background,
            glyph_foreground: resolved.glyph_foreground,
        }
    }
}

pub(super) fn cursor_color_cycle_color_at(
    resolved_background: Color,
    cycle: &CursorColorCycleConfig,
    sample_time: crate::clock::Instant,
    cycle_start: crate::clock::Instant,
) -> Color {
    let elapsed = sample_time
        .saturating_duration_since(cycle_start)
        .as_secs_f64();
    // Keep the unbounded uptime calculation in f64. Converting a year of
    // elapsed seconds to f32 loses more precision than a 24 Hz frame interval.
    let phase = ((elapsed * f64::from(cycle.speed)) % 1.0) as f32;
    let cycle_color = WgpuRenderer::hsl_to_color(phase, cycle.saturation, cycle.lightness);
    // Phase zero is GNU's resolved cursor background.  The smooth periodic
    // envelope returns to that anchor on every completed revolution.
    let envelope = (std::f32::consts::PI * phase).sin().powi(2);
    lerp_color(resolved_background, cycle_color, envelope)
}

fn lerp_color(from: Color, to: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    Color::new(
        from.r + (to.r - from.r) * amount,
        from.g + (to.g - from.g) * amount,
        from.b + (to.b - from.b) * amount,
        from.a + (to.a - from.a) * amount,
    )
}

/// The one shape occupied by a cursor that is currently in flight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum CursorShape {
    Rect(Rect),
    Quad([(f32, f32); 4]),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct InverseVideoCell {
    pub(super) slot_id: DisplaySlotId,
    pub(super) paint: PresentedCursorPaint,
}

/// A filled box is either settled on one glyph cell, where GNU inverse video
/// is valid, or it is in flight as one render-local shape.  There is no
/// variant that can combine an in-flight shape with destination-cell inverse
/// video, making the vertical-motion artifact unrepresentable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum FilledBoxPresentation {
    Settled {
        rect: Rect,
        inverse_video: InverseVideoCell,
        paint: PresentedCursorPaint,
    },
    InFlight {
        shape: CursorShape,
        paint: PresentedCursorPaint,
    },
}

impl FilledBoxPresentation {
    pub(super) fn resolve(
        window_id: DisplayWindowId,
        destination_slot: DisplaySlotId,
        destination: Rect,
        animated_cursor: Option<&AnimatedCursor>,
        paint: PresentedCursorPaint,
    ) -> Self {
        let animated = animated_cursor.filter(|animated| animated.window_id == window_id);
        let in_flight = animated.is_some_and(|animated| {
            animated.corners.is_some()
                || !rect_approximately_equal(
                    Rect::new(animated.x, animated.y, animated.width, animated.height),
                    destination,
                )
        });

        if in_flight {
            let animated = animated.expect("in-flight presentation has animated geometry");
            let shape = animated.corners.map_or_else(
                || {
                    CursorShape::Rect(Rect::new(
                        animated.x,
                        animated.y,
                        animated.width,
                        animated.height,
                    ))
                },
                CursorShape::Quad,
            );
            Self::InFlight { shape, paint }
        } else {
            Self::Settled {
                rect: destination,
                inverse_video: InverseVideoCell {
                    slot_id: destination_slot,
                    paint,
                },
                paint,
            }
        }
    }

    pub(super) const fn inverse_video(self) -> Option<InverseVideoCell> {
        match self {
            Self::Settled { inverse_video, .. } => Some(inverse_video),
            Self::InFlight { .. } => None,
        }
    }
}

fn rect_approximately_equal(left: Rect, right: Rect) -> bool {
    (left.x - right.x).abs() <= SETTLED_GEOMETRY_EPSILON
        && (left.y - right.y).abs() <= SETTLED_GEOMETRY_EPSILON
        && (left.width - right.width).abs() <= SETTLED_GEOMETRY_EPSILON
        && (left.height - right.height).abs() <= SETTLED_GEOMETRY_EPSILON
}

#[cfg(test)]
#[path = "cursor_presentation_test.rs"]
mod tests;

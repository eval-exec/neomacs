//! Declarative popup placement shared by evaluator, runtime, and renderer.

use crate::{Point, Rect, Size};

/// Side of an anchor on which a popup should be attached.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum PopupPreferredSide {
    /// Treat the anchor origin as an explicit popup origin.
    #[default]
    AtAnchor,
    Below,
    Above,
    Right,
    Left,
}

impl PopupPreferredSide {
    const fn opposite(self) -> Option<Self> {
        match self {
            Self::Below => Some(Self::Above),
            Self::Above => Some(Self::Below),
            Self::Right => Some(Self::Left),
            Self::Left => Some(Self::Right),
            Self::AtAnchor => None,
        }
    }
}

/// Policy applied after placing a popup against its anchor.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PopupConstraintPolicy {
    /// Preserve the requested origin exactly.
    #[default]
    None,
    /// Slide the popup into the viewport without changing its side.
    Shift { padding: f32 },
    /// Prefer the opposite side when it overflows less, then slide into view.
    FlipAndShift { padding: f32 },
}

/// A semantic popup request. It deliberately does not contain the popup's
/// final origin: only the owner of popup layout knows its measured extent and
/// current viewport.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PopupPlacement {
    anchor: Rect,
    preferred_side: PopupPreferredSide,
    offset: Point,
    constraint: PopupConstraintPolicy,
}

impl PopupPlacement {
    #[must_use]
    pub const fn new(
        anchor: Rect,
        preferred_side: PopupPreferredSide,
        offset: Point,
        constraint: PopupConstraintPolicy,
    ) -> Self {
        Self {
            anchor,
            preferred_side,
            offset,
            constraint,
        }
    }

    /// Preserve GNU's explicit `(x, y)` popup-menu form.
    #[must_use]
    pub const fn at(origin: Point) -> Self {
        Self::new(
            Rect::new(origin.x, origin.y, 0.0, 0.0),
            PopupPreferredSide::AtAnchor,
            Point::ZERO,
            PopupConstraintPolicy::None,
        )
    }

    #[must_use]
    pub const fn anchor(self) -> Rect {
        self.anchor
    }

    #[must_use]
    pub const fn preferred_side(self) -> PopupPreferredSide {
        self.preferred_side
    }

    #[must_use]
    pub const fn offset(self) -> Point {
        self.offset
    }

    #[must_use]
    pub const fn constraint(self) -> PopupConstraintPolicy {
        self.constraint
    }

    /// Origin on the preferred side before viewport constraints are applied.
    /// This is useful only for coarse pre-layout estimates; final drawing must
    /// use [`Self::resolve`] with the measured popup extent.
    #[must_use]
    pub fn preferred_origin(self, popup: Size) -> Point {
        self.origin_for(self.preferred_side, popup)
    }

    /// Resolve the final origin after popup measurement, against the same
    /// frame-local viewport used for drawing.
    #[must_use]
    pub fn resolve(self, popup: Size, viewport: Rect) -> ResolvedPopupPlacement {
        let popup = Size::new(popup.width.max(0.0), popup.height.max(0.0));
        let mut side = self.preferred_side;
        let mut origin = self.preferred_origin(popup);
        let padding = match self.constraint {
            PopupConstraintPolicy::None => {
                return ResolvedPopupPlacement { origin, side };
            }
            PopupConstraintPolicy::Shift { padding }
            | PopupConstraintPolicy::FlipAndShift { padding } => padding.max(0.0),
        };

        if matches!(self.constraint, PopupConstraintPolicy::FlipAndShift { .. })
            && let Some(opposite) = side.opposite()
        {
            let opposite_origin = self.origin_for(opposite, popup);
            if overflow(opposite_origin, popup, viewport, padding)
                < overflow(origin, popup, viewport, padding)
            {
                side = opposite;
                origin = opposite_origin;
            }
        }

        origin = shift_into_viewport(origin, popup, viewport, padding);
        ResolvedPopupPlacement { origin, side }
    }

    fn origin_for(self, side: PopupPreferredSide, popup: Size) -> Point {
        let origin = match side {
            PopupPreferredSide::AtAnchor => Point::new(self.anchor.x, self.anchor.y),
            PopupPreferredSide::Below => Point::new(self.anchor.x, self.anchor.bottom()),
            PopupPreferredSide::Above => Point::new(self.anchor.x, self.anchor.y - popup.height),
            PopupPreferredSide::Right => Point::new(self.anchor.right(), self.anchor.y),
            PopupPreferredSide::Left => Point::new(self.anchor.x - popup.width, self.anchor.y),
        };
        origin + self.offset
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ResolvedPopupPlacement {
    origin: Point,
    side: PopupPreferredSide,
}

impl ResolvedPopupPlacement {
    #[must_use]
    pub const fn origin(self) -> Point {
        self.origin
    }

    #[must_use]
    pub const fn side(self) -> PopupPreferredSide {
        self.side
    }
}

fn overflow(origin: Point, popup: Size, viewport: Rect, padding: f32) -> f32 {
    let left = viewport.x + padding;
    let top = viewport.y + padding;
    let right = viewport.right() - padding;
    let bottom = viewport.bottom() - padding;
    (left - origin.x).max(0.0)
        + (top - origin.y).max(0.0)
        + (origin.x + popup.width - right).max(0.0)
        + (origin.y + popup.height - bottom).max(0.0)
}

fn shift_into_viewport(origin: Point, popup: Size, viewport: Rect, padding: f32) -> Point {
    let min_x = viewport.x + padding;
    let min_y = viewport.y + padding;
    let max_x = (viewport.right() - padding - popup.width).max(min_x);
    let max_y = (viewport.bottom() - padding - popup.height).max(min_y);
    Point::new(origin.x.clamp(min_x, max_x), origin.y.clamp(min_y, max_y))
}

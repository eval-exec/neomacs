/// Frame-local logical coordinates shared by one pointer action and its
/// presentation-qualified target. Keeping the coordinates here prevents the
/// semantic hit and raw action from disagreeing about where the input occurred.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PointerPosition {
    pub x: f32,
    pub y: f32,
    pub target_frame_id: u64,
}

/// How a pointer position relates to the immutable presentation on screen.
///
/// `Unpresented` is explicit for exposed/native surface area. A producer cannot
/// accidentally omit presentation state from a pointer action.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PointerTarget {
    Presented {
        presentation: u64,
        hit: Option<crate::PresentedHit>,
    },
    Unpresented,
}

/// The unit carried by a scroll delta. This replaces the invalid state where a
/// boolean precision flag can disagree with the meaning of the numbers.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ScrollDelta {
    Lines { x: f32, y: f32 },
    Pixels { x: f32, y: f32 },
}

/// Pointer action interpreted at one [`PointerPosition`].
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PointerAction {
    Button {
        button: u32,
        pressed: bool,
        modifiers: u32,
    },
    Move {
        modifiers: u32,
    },
    Scroll {
        delta: ScrollDelta,
        modifiers: u32,
    },
}

/// Atomic display-to-evaluator pointer input.
///
/// The transport has one variant for native move, button, and scroll actions,
/// so target qualification cannot be sent, reordered, or forgotten separately.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PositionedPointerInput {
    pub position: PointerPosition,
    pub target: PointerTarget,
    pub action: PointerAction,
}

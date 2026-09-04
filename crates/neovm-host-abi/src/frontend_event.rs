//! Typed events crossing from a presentation host into the editor session.

/// Stable editor-frame identity attached to host observations.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FrontendFrameId(u64);

impl FrontendFrameId {
    /// The initial frame before a product adapter has adopted its runtime ID.
    pub const PRIMARY: Self = Self(0);

    /// Construct an identity from the evaluator's raw frame ID.
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Return the evaluator's raw frame ID.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Opaque revision of one immutable frame presentation.
///
/// The evaluator issues this identity and the frontend only echoes it when
/// reporting whether that exact revision became visible or was retired.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FrontendPresentationId(u64);

impl FrontendPresentationId {
    /// Construct an identity from the evaluator's presentation revision.
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Return the evaluator's raw presentation revision.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Logical key symbol understood by Neomacs's keyboard adapter.
///
/// Zero is a real value: terminal `Ctrl-2` produces the NUL character. Host
/// adapters must model translation failure with `Option`, not a numeric
/// sentinel.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FrontendKeySymbol(u32);

impl FrontendKeySymbol {
    /// Construct a key symbol from its frontend-independent numeric value.
    #[must_use]
    pub const fn new(symbol: u32) -> Self {
        Self(symbol)
    }

    /// Return the frontend-independent numeric value.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Modifier state sampled atomically with one key event.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FrontendModifiers {
    shift: bool,
    control: bool,
    meta: bool,
    super_: bool,
}

impl FrontendModifiers {
    /// Bit used by the established wire representation for Shift.
    pub const SHIFT_MASK: u32 = 1 << 0;
    /// Bit used by the established wire representation for Control.
    pub const CONTROL_MASK: u32 = 1 << 1;
    /// Bit used by the established wire representation for Meta.
    pub const META_MASK: u32 = 1 << 2;
    /// Bit used by the established wire representation for Super.
    pub const SUPER_MASK: u32 = 1 << 3;

    /// Construct one atomic modifier-state sample.
    #[must_use]
    pub const fn new(shift: bool, control: bool, meta: bool, super_: bool) -> Self {
        Self {
            shift,
            control,
            meta,
            super_,
        }
    }

    /// Decode the established renderer-to-evaluator modifier representation.
    #[must_use]
    pub const fn from_bits(bits: u32) -> Self {
        Self::new(
            bits & Self::SHIFT_MASK != 0,
            bits & Self::CONTROL_MASK != 0,
            bits & Self::META_MASK != 0,
            bits & Self::SUPER_MASK != 0,
        )
    }

    /// Encode modifiers for the existing evaluator keyboard conversion.
    #[must_use]
    pub const fn bits(self) -> u32 {
        (if self.shift { Self::SHIFT_MASK } else { 0 })
            | (if self.control { Self::CONTROL_MASK } else { 0 })
            | (if self.meta { Self::META_MASK } else { 0 })
            | (if self.super_ { Self::SUPER_MASK } else { 0 })
    }

    /// Whether Shift was active.
    #[must_use]
    pub const fn shift(self) -> bool {
        self.shift
    }

    /// Whether Control was active.
    #[must_use]
    pub const fn control(self) -> bool {
        self.control
    }

    /// Whether Meta was active.
    #[must_use]
    pub const fn meta(self) -> bool {
        self.meta
    }

    /// Whether Super was active.
    #[must_use]
    pub const fn super_(self) -> bool {
        self.super_
    }
}

/// Whether the host reports key activation or release.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrontendKeyState {
    /// The key became active.
    Pressed,
    /// The key became inactive.
    Released,
}

/// Invalid logical-to-device scale reported by a presentation host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidFrontendScaleFactor;

impl std::fmt::Display for InvalidFrontendScaleFactor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("frontend scale factor must be finite and positive")
    }
}

impl std::error::Error for InvalidFrontendScaleFactor {}

/// Lossless, validated logical-to-device scale at the frontend boundary.
///
/// Native window systems report this value as `f64`. Renderer adapters may
/// narrow it to their own representation after the editor has consumed the
/// host observation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrontendScaleFactor(f64);

impl FrontendScaleFactor {
    /// Unit scale for hosts whose logical and device coordinates coincide.
    pub const ONE: Self = Self(1.0);

    /// Validate and construct a finite, positive scale factor.
    pub fn new(scale: f64) -> Result<Self, InvalidFrontendScaleFactor> {
        if scale.is_finite() && scale > 0.0 {
            Ok(Self(scale))
        } else {
            Err(InvalidFrontendScaleFactor)
        }
    }

    /// Return the validated scale factor.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl FrontendKeyState {
    /// Whether this observation activates the key.
    #[must_use]
    pub const fn is_pressed(self) -> bool {
        matches!(self, Self::Pressed)
    }
}

/// One logical key observation, including all data required for dispatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrontendKeyEvent {
    symbol: FrontendKeySymbol,
    modifiers: FrontendModifiers,
    state: FrontendKeyState,
    target: FrontendFrameId,
}

impl FrontendKeyEvent {
    /// Construct a complete logical key observation.
    #[must_use]
    pub const fn new(
        symbol: FrontendKeySymbol,
        modifiers: FrontendModifiers,
        state: FrontendKeyState,
        target: FrontendFrameId,
    ) -> Self {
        Self {
            symbol,
            modifiers,
            state,
            target,
        }
    }

    /// Logical key symbol reported by the host.
    #[must_use]
    pub const fn symbol(self) -> FrontendKeySymbol {
        self.symbol
    }

    /// Modifier state sampled with the key.
    #[must_use]
    pub const fn modifiers(self) -> FrontendModifiers {
        self.modifiers
    }

    /// Activation state reported by the host.
    #[must_use]
    pub const fn state(self) -> FrontendKeyState {
        self.state
    }

    /// Editor frame targeted by the key.
    #[must_use]
    pub const fn target(self) -> FrontendFrameId {
        self.target
    }
}

/// Two-dimensional editor extent measured in logical pixels.
///
/// Frontends must convert device/backing dimensions before constructing this
/// value. Keeping the unit in the type makes that conversion explicit and
/// prevents host APIs from accepting an unlabelled width/height pair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrontendLogicalExtent {
    width: u32,
    height: u32,
}

impl FrontendLogicalExtent {
    /// Construct a logical editor extent.
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Logical width in editor pixels.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    /// Logical height in editor pixels.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }
}

/// Two-dimensional terminal viewport measured in character cells.
///
/// This is deliberately distinct from [`FrontendLogicalExtent`]: terminal
/// columns and rows are not pixels, even though the evaluator's legacy frame
/// representation stores both through the same resize path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrontendTerminalExtent {
    columns: u32,
    rows: u32,
}

impl FrontendTerminalExtent {
    /// Construct a terminal character-cell extent.
    #[must_use]
    pub const fn new(columns: u32, rows: u32) -> Self {
        Self { columns, rows }
    }

    /// Terminal width in character-cell columns.
    #[must_use]
    pub const fn columns(self) -> u32 {
        self.columns
    }

    /// Terminal height in character-cell rows.
    #[must_use]
    pub const fn rows(self) -> u32 {
        self.rows
    }
}

/// Character-cell terminal viewport and the editor frame it targets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrontendTerminalViewport {
    extent: FrontendTerminalExtent,
    target: FrontendFrameId,
}

impl FrontendTerminalViewport {
    /// Construct a terminal viewport observation.
    #[must_use]
    pub const fn new(extent: FrontendTerminalExtent, target: FrontendFrameId) -> Self {
        Self { extent, target }
    }

    /// Terminal grid sampled by this observation.
    #[must_use]
    pub const fn extent(self) -> FrontendTerminalExtent {
        self.extent
    }

    /// Editor frame targeted by the terminal resize.
    #[must_use]
    pub const fn target(self) -> FrontendFrameId {
        self.target
    }
}

/// Logical editor extent paired with its validated logical-to-device scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrontendViewport {
    logical_extent: FrontendLogicalExtent,
    scale: FrontendScaleFactor,
    target: FrontendFrameId,
}

impl FrontendViewport {
    /// Validate and construct a logical viewport observation.
    pub fn new(
        logical_extent: FrontendLogicalExtent,
        scale: f64,
        target: FrontendFrameId,
    ) -> Result<Self, InvalidFrontendScaleFactor> {
        Ok(Self {
            logical_extent,
            scale: FrontendScaleFactor::new(scale)?,
            target,
        })
    }

    /// Logical editor extent sampled with the scale factor.
    #[must_use]
    pub const fn logical_extent(self) -> FrontendLogicalExtent {
        self.logical_extent
    }

    /// Logical-to-device scale sampled with the extent.
    #[must_use]
    pub const fn scale(self) -> FrontendScaleFactor {
        self.scale
    }

    /// Editor frame targeted by the viewport change.
    #[must_use]
    pub const fn target(self) -> FrontendFrameId {
        self.target
    }
}

/// Host observation consumed by the editor session.
#[derive(Clone, Debug, PartialEq)]
pub enum FrontendEvent {
    /// Logical keyboard input.
    Key(FrontendKeyEvent),
    /// Text committed by an IME or other host text service.
    TextCommitted {
        /// Committed Unicode text.
        text: String,
        /// Editor frame receiving the text.
        target: FrontendFrameId,
    },
    /// Logical extent or logical-to-device scale change.
    ViewportChanged(FrontendViewport),
    /// Character-cell terminal viewport change.
    TerminalViewportChanged(FrontendTerminalViewport),
    /// Keyboard-focus change.
    FocusChanged {
        /// Whether the target gained focus.
        focused: bool,
        /// Editor frame whose focus changed.
        target: FrontendFrameId,
    },
    /// User request to close an editor frame.
    CloseRequested {
        /// Editor frame requested for closure.
        target: FrontendFrameId,
    },
    /// The renderer installed this immutable revision for drawing and input.
    PresentationActivated {
        /// Presentation revision installed by the renderer.
        presentation: FrontendPresentationId,
        /// Editor frame whose visible revision changed.
        target: FrontendFrameId,
    },
    /// The renderer rejected or superseded this revision before activation.
    PresentationDiscarded {
        /// Presentation revision rejected by the renderer.
        presentation: FrontendPresentationId,
        /// Editor frame that owned the rejected revision.
        target: FrontendFrameId,
    },
    /// A formerly visible presentation can no longer produce input hits.
    PresentationRetired {
        /// Presentation revision no longer retained by the renderer.
        presentation: FrontendPresentationId,
    },
}

impl FrontendEvent {
    /// Construct committed Unicode text input.
    #[must_use]
    pub fn text_committed(text: impl Into<String>, target: FrontendFrameId) -> Self {
        Self::TextCommitted {
            text: text.into(),
            target,
        }
    }
}

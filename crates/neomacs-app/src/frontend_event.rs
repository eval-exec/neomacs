//! Typed events crossing from a presentation host into the editor session.

/// Stable editor-frame identity attached to host observations.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FrontendFrameId(u64);

impl FrontendFrameId {
    /// The initial frame before a product adapter has adopted its runtime ID.
    pub const PRIMARY: Self = Self(0);

    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

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
    #[must_use]
    pub const fn new(symbol: u32) -> Self {
        Self(symbol)
    }

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
    pub const SHIFT_MASK: u32 = 1 << 0;
    pub const CONTROL_MASK: u32 = 1 << 1;
    pub const META_MASK: u32 = 1 << 2;
    pub const SUPER_MASK: u32 = 1 << 3;

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

    #[must_use]
    pub const fn shift(self) -> bool {
        self.shift
    }

    #[must_use]
    pub const fn control(self) -> bool {
        self.control
    }

    #[must_use]
    pub const fn meta(self) -> bool {
        self.meta
    }

    #[must_use]
    pub const fn super_(self) -> bool {
        self.super_
    }
}

/// Whether the host reports key activation or release.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrontendKeyState {
    Pressed,
    Released,
}

/// Invalid logical-to-device scale reported by a presentation host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidFrontendScaleFactor;

/// Lossless, validated logical-to-device scale at the frontend boundary.
///
/// Native window systems report this value as `f64`. Renderer adapters may
/// narrow it to their own representation after the editor has consumed the
/// host observation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrontendScaleFactor(f64);

impl FrontendScaleFactor {
    pub fn new(scale: f64) -> Result<Self, InvalidFrontendScaleFactor> {
        if scale.is_finite() && scale > 0.0 {
            Ok(Self(scale))
        } else {
            Err(InvalidFrontendScaleFactor)
        }
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl FrontendKeyState {
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

    #[must_use]
    pub const fn symbol(self) -> FrontendKeySymbol {
        self.symbol
    }

    #[must_use]
    pub const fn modifiers(self) -> FrontendModifiers {
        self.modifiers
    }

    #[must_use]
    pub const fn state(self) -> FrontendKeyState {
        self.state
    }

    #[must_use]
    pub const fn target(self) -> FrontendFrameId {
        self.target
    }
}

/// Physical drawable extent paired with its validated logical/device scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrontendViewport {
    width: u32,
    height: u32,
    scale: FrontendScaleFactor,
    target: FrontendFrameId,
}

impl FrontendViewport {
    pub fn new(
        width: u32,
        height: u32,
        scale: f64,
        target: FrontendFrameId,
    ) -> Result<Self, InvalidFrontendScaleFactor> {
        Ok(Self {
            width,
            height,
            scale: FrontendScaleFactor::new(scale)?,
            target,
        })
    }

    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }

    #[must_use]
    pub const fn scale(self) -> FrontendScaleFactor {
        self.scale
    }

    #[must_use]
    pub const fn target(self) -> FrontendFrameId {
        self.target
    }
}

/// Host observation consumed by the editor session.
#[derive(Clone, Debug, PartialEq)]
pub enum FrontendEvent {
    Key(FrontendKeyEvent),
    /// Text committed by an IME or other host text service.
    TextCommitted {
        text: String,
        target: FrontendFrameId,
    },
    ViewportChanged(FrontendViewport),
    FocusChanged {
        focused: bool,
        target: FrontendFrameId,
    },
    CloseRequested {
        target: FrontendFrameId,
    },
}

impl FrontendEvent {
    #[must_use]
    pub fn text_committed(text: impl Into<String>, target: FrontendFrameId) -> Self {
        Self::TextCommitted {
            text: text.into(),
            target,
        }
    }
}

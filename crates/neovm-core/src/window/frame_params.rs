//! GNU-shaped frame parameter keys.
//!
//! GNU Emacs keeps the meaningful GUI frame parameters in `frame_parms[]`
//! in `src/frame.c`.  Lisp can still attach arbitrary frame parameters, but
//! known parameters get routed through dedicated semantic handlers.  Keep the
//! same split here: typed known parameters for Rust internals, plus dynamic
//! symbol keys for Lisp-visible unknown parameters.

use crate::emacs_core::intern::{SymId, WellKnown, resolve_sym};
use crate::emacs_core::value::Value;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use strum::{EnumCount, EnumString, IntoStaticStr, VariantArray};

pub const GNU_FRAME_PARAM_COUNT: usize = 54;

#[repr(u8)]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    EnumString,
    EnumCount,
    IntoStaticStr,
    IntoPrimitive,
    TryFromPrimitive,
)]
#[strum(serialize_all = "kebab-case")]
pub enum FrameParam {
    AutoRaise = 0,
    AutoLower,
    BackgroundColor,
    BorderColor,
    BorderWidth,
    CursorColor,
    CursorType,
    Font,
    ForegroundColor,
    IconName,
    IconType,
    ChildFrameBorderWidth,
    InternalBorderWidth,
    RightDividerWidth,
    BottomDividerWidth,
    MenuBarLines,
    MouseColor,
    Name,
    ScrollBarWidth,
    ScrollBarHeight,
    Title,
    Unsplittable,
    VerticalScrollBars,
    HorizontalScrollBars,
    Visibility,
    TabBarLines,
    ToolBarLines,
    ScrollBarForeground,
    ScrollBarBackground,
    ScreenGamma,
    LineSpacing,
    LeftFringe,
    RightFringe,
    WaitForWm,
    Fullscreen,
    FontBackend,
    Alpha,
    Sticky,
    ToolBarPosition,
    InhibitDoubleBuffering,
    Undecorated,
    ParentFrame,
    SkipTaskbar,
    NoFocusOnMap,
    NoAcceptFocus,
    ZGroup,
    OverrideRedirect,
    NoSpecialGlyphs,
    AlphaBackground,
    BordersRespectAlphaBackground,
    UseFrameSynchronization,
    Shaded,
    NsAppearance,
    NsTransparentTitlebar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum FrameFullscreen {
    /// Lisp nil explicitly requests restoration; it is not a missing request.
    #[strum(serialize = "nil")]
    Windowed,
    Fullboth,
    Fullscreen,
    Fullwidth,
    Fullheight,
    Maximized,
}

impl FrameFullscreen {
    pub fn from_symbol_value(value: &Value) -> Option<Self> {
        if value.is_nil() {
            return Some(Self::Windowed);
        }
        value.as_bare_symbol_name()?.parse().ok()
    }

    pub fn name(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum FrameToolBarPosition {
    Left,
    Right,
    Top,
    Bottom,
}

impl FrameToolBarPosition {
    pub fn from_symbol_value(value: &Value) -> Option<Self> {
        value.as_bare_symbol_name()?.parse().ok()
    }

    pub fn name(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum FrameZGroup {
    Above,
    AboveSuspended,
    Below,
}

impl FrameZGroup {
    pub fn from_symbol_value(value: &Value) -> Option<Self> {
        value.as_bare_symbol_name()?.parse().ok()
    }

    pub fn name(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum CursorTypeSymbol {
    Box,
    Hollow,
    Bar,
    Hbar,
}

impl CursorTypeSymbol {
    pub fn from_symbol_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    pub fn from_symbol_value(value: &Value) -> Option<Self> {
        Self::from_symbol_name(value.as_bare_symbol_name()?)
    }

    pub fn name(self) -> &'static str {
        self.into()
    }

    pub fn accepts_width_tail(self) -> bool {
        matches!(self, Self::Box | Self::Bar | Self::Hbar)
    }
}

pub const GNU_FRAME_PARAMS: [FrameParam; GNU_FRAME_PARAM_COUNT] = [
    FrameParam::AutoRaise,
    FrameParam::AutoLower,
    FrameParam::BackgroundColor,
    FrameParam::BorderColor,
    FrameParam::BorderWidth,
    FrameParam::CursorColor,
    FrameParam::CursorType,
    FrameParam::Font,
    FrameParam::ForegroundColor,
    FrameParam::IconName,
    FrameParam::IconType,
    FrameParam::ChildFrameBorderWidth,
    FrameParam::InternalBorderWidth,
    FrameParam::RightDividerWidth,
    FrameParam::BottomDividerWidth,
    FrameParam::MenuBarLines,
    FrameParam::MouseColor,
    FrameParam::Name,
    FrameParam::ScrollBarWidth,
    FrameParam::ScrollBarHeight,
    FrameParam::Title,
    FrameParam::Unsplittable,
    FrameParam::VerticalScrollBars,
    FrameParam::HorizontalScrollBars,
    FrameParam::Visibility,
    FrameParam::TabBarLines,
    FrameParam::ToolBarLines,
    FrameParam::ScrollBarForeground,
    FrameParam::ScrollBarBackground,
    FrameParam::ScreenGamma,
    FrameParam::LineSpacing,
    FrameParam::LeftFringe,
    FrameParam::RightFringe,
    FrameParam::WaitForWm,
    FrameParam::Fullscreen,
    FrameParam::FontBackend,
    FrameParam::Alpha,
    FrameParam::Sticky,
    FrameParam::ToolBarPosition,
    FrameParam::InhibitDoubleBuffering,
    FrameParam::Undecorated,
    FrameParam::ParentFrame,
    FrameParam::SkipTaskbar,
    FrameParam::NoFocusOnMap,
    FrameParam::NoAcceptFocus,
    FrameParam::ZGroup,
    FrameParam::OverrideRedirect,
    FrameParam::NoSpecialGlyphs,
    FrameParam::AlphaBackground,
    FrameParam::BordersRespectAlphaBackground,
    FrameParam::UseFrameSynchronization,
    FrameParam::Shaded,
    FrameParam::NsAppearance,
    FrameParam::NsTransparentTitlebar,
];

// The GNU handler domain stays separate from native queries of parameters
// which have no entry in frame_parms[]. Both resolve to fixed DEFSYM table
// identities; no registry lookup, lock or epoch read is involved.

/// Fixed Core query keys outside GNU's indexed frame-parameter handler table.
#[repr(u8)]
#[derive(Clone, Copy, Debug, EnumCount, IntoPrimitive, IntoStaticStr, VariantArray)]
#[strum(serialize_all = "kebab-case")]
pub(crate) enum FrameQueryParam {
    Width,
    Height,
    WindowSystem,
    FontParameter,
    Fontsize,
    Tooltip,
    Display,
    MinWidth,
    MinHeight,
    #[strum(serialize = "neovm--frame-text-lines")]
    TextLines,
    #[strum(serialize = "neovm--frame-total-cols")]
    TotalCols,
    #[strum(serialize = "neovm--frame-total-lines")]
    TotalLines,
}

const _: [(); FrameQueryParam::COUNT] = [(); FrameQueryParam::VARIANTS.len()];

impl FrameQueryParam {
    /// The fixed DEFSYM identity this query key is spelled as.
    pub(crate) const fn well_known(self) -> WellKnown {
        use crate::emacs_core::intern::WellKnown;

        match self {
            FrameQueryParam::Width => WellKnown::Width,
            FrameQueryParam::Height => WellKnown::Height,
            FrameQueryParam::WindowSystem => WellKnown::WindowSystem,
            FrameQueryParam::FontParameter => WellKnown::FontParameter,
            FrameQueryParam::Fontsize => WellKnown::Fontsize,
            FrameQueryParam::Tooltip => WellKnown::Tooltip,
            FrameQueryParam::Display => WellKnown::Display,
            FrameQueryParam::MinWidth => WellKnown::MinWidth,
            FrameQueryParam::MinHeight => WellKnown::MinHeight,
            FrameQueryParam::TextLines => WellKnown::NeovmFrameTextLines,
            FrameQueryParam::TotalCols => WellKnown::NeovmFrameTotalCols,
            FrameQueryParam::TotalLines => WellKnown::NeovmFrameTotalLines,
        }
    }
}

impl From<FrameQueryParam> for FrameParamKey {
    fn from(value: FrameQueryParam) -> Self {
        Self::Unknown(value.well_known().id())
    }
}

impl FrameQueryParam {
    /// Interned symbol this query key names in the frame parameter map.
    pub(crate) fn symbol_id(self) -> SymId {
        self.well_known().id()
    }

    /// Lisp-visible symbol value of this query key.
    pub(crate) fn symbol(self) -> Value {
        Value::from_sym_id(self.symbol_id())
    }
}

impl FrameParam {
    /// The fixed DEFSYM identity GNU's `frame_parms[]` entry is spelled as.
    const fn well_known(self) -> WellKnown {
        use crate::emacs_core::intern::WellKnown;

        match self {
            FrameParam::AutoRaise => WellKnown::AutoRaise,
            FrameParam::AutoLower => WellKnown::AutoLower,
            FrameParam::BackgroundColor => WellKnown::BackgroundColor,
            FrameParam::BorderColor => WellKnown::BorderColor,
            FrameParam::BorderWidth => WellKnown::BorderWidth,
            FrameParam::CursorColor => WellKnown::CursorColor,
            FrameParam::CursorType => WellKnown::CursorType,
            FrameParam::Font => WellKnown::Font,
            FrameParam::ForegroundColor => WellKnown::ForegroundColor,
            FrameParam::IconName => WellKnown::IconName,
            FrameParam::IconType => WellKnown::IconType,
            FrameParam::ChildFrameBorderWidth => WellKnown::ChildFrameBorderWidth,
            FrameParam::InternalBorderWidth => WellKnown::InternalBorderWidth,
            FrameParam::RightDividerWidth => WellKnown::RightDividerWidth,
            FrameParam::BottomDividerWidth => WellKnown::BottomDividerWidth,
            FrameParam::MenuBarLines => WellKnown::MenuBarLines,
            FrameParam::MouseColor => WellKnown::MouseColor,
            FrameParam::Name => WellKnown::Name,
            FrameParam::ScrollBarWidth => WellKnown::ScrollBarWidth,
            FrameParam::ScrollBarHeight => WellKnown::ScrollBarHeight,
            FrameParam::Title => WellKnown::Title,
            FrameParam::Unsplittable => WellKnown::Unsplittable,
            FrameParam::VerticalScrollBars => WellKnown::VerticalScrollBars,
            FrameParam::HorizontalScrollBars => WellKnown::HorizontalScrollBars,
            FrameParam::Visibility => WellKnown::Visibility,
            FrameParam::TabBarLines => WellKnown::TabBarLines,
            FrameParam::ToolBarLines => WellKnown::ToolBarLines,
            FrameParam::ScrollBarForeground => WellKnown::ScrollBarForeground,
            FrameParam::ScrollBarBackground => WellKnown::ScrollBarBackground,
            FrameParam::ScreenGamma => WellKnown::ScreenGamma,
            FrameParam::LineSpacing => WellKnown::LineSpacing,
            FrameParam::LeftFringe => WellKnown::LeftFringe,
            FrameParam::RightFringe => WellKnown::RightFringe,
            FrameParam::WaitForWm => WellKnown::WaitForWm,
            FrameParam::Fullscreen => WellKnown::Fullscreen,
            FrameParam::FontBackend => WellKnown::FontBackend,
            FrameParam::Alpha => WellKnown::Alpha,
            FrameParam::Sticky => WellKnown::Sticky,
            FrameParam::ToolBarPosition => WellKnown::ToolBarPosition,
            FrameParam::InhibitDoubleBuffering => WellKnown::InhibitDoubleBuffering,
            FrameParam::Undecorated => WellKnown::Undecorated,
            FrameParam::ParentFrame => WellKnown::ParentFrame,
            FrameParam::SkipTaskbar => WellKnown::SkipTaskbar,
            FrameParam::NoFocusOnMap => WellKnown::NoFocusOnMap,
            FrameParam::NoAcceptFocus => WellKnown::NoAcceptFocus,
            FrameParam::ZGroup => WellKnown::ZGroup,
            FrameParam::OverrideRedirect => WellKnown::OverrideRedirect,
            FrameParam::NoSpecialGlyphs => WellKnown::NoSpecialGlyphs,
            FrameParam::AlphaBackground => WellKnown::AlphaBackground,
            FrameParam::BordersRespectAlphaBackground => WellKnown::BordersRespectAlphaBackground,
            FrameParam::UseFrameSynchronization => WellKnown::UseFrameSynchronization,
            FrameParam::Shaded => WellKnown::Shaded,
            FrameParam::NsAppearance => WellKnown::NsAppearance,
            FrameParam::NsTransparentTitlebar => WellKnown::NsTransparentTitlebar,
        }
    }

    pub fn gnu_index(self) -> usize {
        usize::from(u8::from(self))
    }

    pub fn from_gnu_index(index: usize) -> Option<Self> {
        let index = u8::try_from(index).ok()?;
        Self::try_from(index).ok()
    }

    pub fn name(self) -> &'static str {
        self.into()
    }

    pub fn symbol(self) -> Value {
        Value::from_sym_id(self.well_known().id())
    }

    pub fn from_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    pub fn from_symbol_id(symbol: SymId) -> Option<Self> {
        Self::from_name(resolve_sym(symbol))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FrameParamKey {
    Known(FrameParam),
    Unknown(SymId),
}

impl FrameParamKey {
    pub fn from_symbol_id(symbol: SymId) -> Self {
        FrameParam::from_symbol_id(symbol)
            .map(Self::Known)
            .unwrap_or(Self::Unknown(symbol))
    }

    pub fn from_symbol_value(value: Value) -> Option<Self> {
        value.as_bare_symbol_id().map(Self::from_symbol_id)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Known(param) => param.name(),
            Self::Unknown(symbol) => resolve_sym(symbol),
        }
    }

    pub fn symbol(self) -> Value {
        match self {
            Self::Known(param) => param.symbol(),
            Self::Unknown(symbol) => Value::from_sym_id(symbol),
        }
    }
}

impl From<FrameParam> for FrameParamKey {
    fn from(value: FrameParam) -> Self {
        Self::Known(value)
    }
}

#[cfg(test)]
#[path = "frame_params/tests/frame_params_test.rs"]
mod tests;

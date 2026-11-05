//! GNU-shaped frame parameter keys.
//!
//! GNU Emacs keeps the meaningful GUI frame parameters in `frame_parms[]`
//! in `src/frame.c`.  Lisp can still attach arbitrary frame parameters, but
//! known parameters get routed through dedicated semantic handlers.  Keep the
//! same split here: typed known parameters for Rust internals, plus dynamic
//! symbol keys for Lisp-visible unknown parameters.

use neovm_value::intern::{SymId, resolve_sym};
use neovm_value::value::Value;
use strum::{EnumString, IntoStaticStr};

pub const GNU_FRAME_PARAM_COUNT: usize = 54;

pub use neovm_value::display_vocab::FrameParam;

pub use neovm_value::display_vocab::FrameFullscreen;

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

pub(crate) use neovm_value::display_vocab::FrameQueryParam;

pub use neovm_eval::emacs_core::eval::data::window_state::FrameParamKey;

#[cfg(test)]
#[path = "frame_params/tests/frame_params_test.rs"]
mod tests;

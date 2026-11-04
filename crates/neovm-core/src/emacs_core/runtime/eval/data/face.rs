//! Face vocabulary owned by the evaluator layer (stage-4 W1 E2).

/// GNU's forced basic face roles, in realization order. A role retains its
/// own result even when advised color resolution gives equal source attrs
/// different colors on successive calls (xfaces.c:5952-5983,6182-6219).
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    enum_map::Enum,
    strum::EnumIter,
    strum::EnumString,
    strum::IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum BasicFace {
    Default,
    ModeLineActive,
    ModeLineInactive,
    ToolBar,
    Fringe,
    HeaderLineActive,
    HeaderLineInactive,
    ScrollBar,
    Border,
    Cursor,
    Mouse,
    Menu,
    VerticalBorder,
    WindowDivider,
    WindowDividerFirstPixel,
    WindowDividerLastPixel,
    InternalBorder,
    ChildFrameBorder,
    TabBar,
    TabLineActive,
    TabLineInactive,
    Margin,
}

impl BasicFace {
    pub fn name(self) -> &'static str {
        self.into()
    }

    pub fn from_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }
}

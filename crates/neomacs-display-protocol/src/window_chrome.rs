//! Native window appearance; independent of editor menu/tool/tab bar layout.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeTitlebarStyle {
    System,
    FrameBackground,
    Overlay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowChromePolicy {
    Native(NativeTitlebarStyle),
    ClientDecorated,
    Undecorated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeChromeSupport {
    SystemOnly,
    AppKit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnsupportedChrome(pub NativeTitlebarStyle);

impl WindowChromePolicy {
    pub const fn resolve(self, support: NativeChromeSupport) -> Result<Self, UnsupportedChrome> {
        match self {
            Self::ClientDecorated | Self::Undecorated => Ok(self),
            Self::Native(style) => match style {
                NativeTitlebarStyle::System => Ok(self),
                NativeTitlebarStyle::FrameBackground | NativeTitlebarStyle::Overlay => {
                    match support {
                        NativeChromeSupport::AppKit => Ok(self),
                        NativeChromeSupport::SystemOnly => Err(UnsupportedChrome(style)),
                    }
                }
            },
        }
    }
}

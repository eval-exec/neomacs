use neomacs_display_protocol::{Color, ContentInsets, NativeTitlebarStyle, WindowChromePolicy};
use winit::window::{Window, WindowAttributes};

/// Native state is owned per window, never per renderer or process.
#[derive(Default)]
pub(crate) struct WindowChromeController {
    applied: Option<NativeAppearance>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct NativeAppearance {
    pub policy: WindowChromePolicy,
    pub background: Color,
}

impl WindowChromeController {
    pub fn prepare(attrs: WindowAttributes, decorated: bool) -> WindowAttributes {
        super::platform::prepare(attrs, Self::policy(decorated))
    }

    fn policy(decorated: bool) -> WindowChromePolicy {
        if decorated {
            WindowChromePolicy::Native(std::cfg_select! {
                target_os = "macos" => { NativeTitlebarStyle::Overlay }
                _ => { NativeTitlebarStyle::System }
            })
        } else {
            WindowChromePolicy::ClientDecorated
        }
    }

    /// An external native-style operation may invalidate the cached appearance.
    pub fn invalidate(&mut self) {
        self.applied = None;
    }

    pub fn synchronize(&mut self, window: &dyn Window, decorated: bool, background: Color) {
        let desired = NativeAppearance {
            policy: Self::policy(decorated),
            background,
        };
        if let Err(error) = self.apply(desired, |appearance| {
            super::platform::apply(window, appearance.policy, appearance.background)
        }) {
            tracing::error!(?error, "could not apply native frame chrome");
        }
    }

    /// The effect is the OS adapter seam. Only successful native application
    /// acknowledges a request; failure remains retryable on the next event.
    pub(super) fn apply<E>(
        &mut self,
        desired: NativeAppearance,
        native: impl FnOnce(NativeAppearance) -> Result<(), E>,
    ) -> Result<(), E> {
        if self.applied != Some(desired) {
            native(desired)?;
            self.applied = Some(desired);
        }
        Ok(())
    }

    pub fn insets(window: &dyn Window, decorated: bool) -> ContentInsets {
        match Self::policy(decorated) {
            WindowChromePolicy::Native(NativeTitlebarStyle::Overlay) => {
                let insets = window.safe_area();
                ContentInsets::new(insets.left, insets.top, insets.right, insets.bottom)
            }
            _ => ContentInsets::default(),
        }
    }
}

//! Input/capture adapter for an isolated Weston X11 window.
use super::*;
use std::{path::PathBuf, process::Command};
pub mod popup_trace;

pub struct LinuxSession {
    env: Vec<(String, String)>,
    window: String,
    display: DesktopRect,
    held: Vec<InputAction>,
    trace: PathBuf,
    scale: (f64, f64),
}
impl LinuxSession {
    pub fn new(
        env: &[(String, String)],
        window: &str,
        display: DesktopRect,
        trace: PathBuf,
    ) -> Result<Self> {
        CaptureMapping::new(display, 1, 1)?;
        let mut result = Self {
            env: env.to_vec(),
            window: window.into(),
            display,
            held: vec![],
            trace,
            scale: (1.0, 1.0),
        };
        result.command(&["windowfocus", window])?;
        let output = Command::new("xdotool")
            .args(["getwindowgeometry", "--shell", window])
            .envs(env.iter().map(|(k, v)| (k, v)))
            .output()?;
        if !output.status.success() {
            return Err(DriverError::Protocol(
                "cannot observe Weston window geometry".into(),
            ));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let dimension = |name: &str| -> Result<f64> {
            text.lines()
                .find_map(|line| line.strip_prefix(name))
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| DriverError::Protocol(format!("missing {name} in X11 geometry")))
        };
        result.scale = (
            dimension("WIDTH=")? / display.width,
            dimension("HEIGHT=")? / display.height,
        );
        Ok(result)
    }
    fn command(&self, args: &[&str]) -> Result<()> {
        let output = Command::new("xdotool")
            .args(args)
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(DriverError::Protocol(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ))
        }
    }
}
impl DesktopDriver for LinuxSession {
    fn fit_window(&mut self) -> Result<DesktopRect> {
        Ok(self.display)
    }
    fn input(&mut self, action: InputAction) -> Result<()> {
        match action {
            InputAction::Move { point } => {
                if !self.display.contains(point) {
                    return Err(DriverError::InvalidGeometry);
                }
                self.command(&[
                    "mousemove",
                    "--window",
                    &self.window,
                    &((point.x - self.display.x) * self.scale.0)
                        .round()
                        .to_string(),
                    &((point.y - self.display.y) * self.scale.1)
                        .round()
                        .to_string(),
                ])?;
            }
            InputAction::Button { button, down } => self.command(&[
                if down { "mousedown" } else { "mouseup" },
                match button {
                    Button::Left => "1",
                    Button::Right => "3",
                },
            ])?,
            InputAction::Key { key, down } => self.command(&[
                if down { "keydown" } else { "keyup" },
                match key {
                    Key::Escape => "Escape",
                    Key::Enter => "Return",
                    Key::Down => "Down",
                    Key::Up => "Up",
                    Key::Left => "Left",
                    Key::Right => "Right",
                    Key::Home => "Home",
                    Key::End => "End",
                    Key::X => "x",
                },
            ])?,
            InputAction::Scroll { lines } => {
                if lines.unsigned_abs() > 100 {
                    return Err(DriverError::Protocol("scroll count exceeds 100".into()));
                }
                for _ in 0..lines.unsigned_abs() {
                    self.command(&["click", if lines > 0 { "4" } else { "5" }])?;
                }
            }
        }
        match action {
            InputAction::Button { down: true, .. } | InputAction::Key { down: true, .. } => {
                self.held.push(action)
            }
            InputAction::Button {
                button,
                down: false,
            } => self
                .held
                .retain(|a| !matches!(a, InputAction::Button { button:b, .. } if *b == button)),
            InputAction::Key { key, down: false } => self
                .held
                .retain(|a| !matches!(a, InputAction::Key { key:k, .. } if *k == key)),
            _ => {}
        }
        Ok(())
    }
    fn observe(&mut self) -> Result<DesktopObservation> {
        Ok(DesktopObservation {
            display: self.display,
            usable: self.display,
            windows: popup_trace::observed(
                &std::fs::read_to_string(&self.trace)?,
                DesktopPoint {
                    x: self.display.x,
                    y: self.display.y,
                },
            ),
        })
    }
    fn capture(&mut self, path: &Path) -> Result<Capture> {
        let result = Command::new("import")
            .args(["-window", &self.window])
            .arg(path)
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .output()?;
        if !result.status.success() {
            return Err(DriverError::Protocol(
                String::from_utf8_lossy(&result.stderr).into_owned(),
            ));
        }
        let pixels = image::open(path)
            .map_err(|e| DriverError::Protocol(e.to_string()))?
            .to_rgb8();
        let mapping = CaptureMapping::new(self.display, pixels.width(), pixels.height())?;
        Ok(Capture { pixels, mapping })
    }
}
impl Drop for LinuxSession {
    fn drop(&mut self) {
        for action in std::mem::take(&mut self.held) {
            let release = match action {
                InputAction::Key { key, .. } => InputAction::Key { key, down: false },
                InputAction::Button { button, .. } => InputAction::Button {
                    button,
                    down: false,
                },
                _ => continue,
            };
            let _ = self.input(release);
        }
    }
}

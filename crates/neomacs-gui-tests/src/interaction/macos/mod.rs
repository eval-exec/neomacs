//! Client for the persistent helper in the logged-in macOS desktop session.
//! Available on Unix so SSH controllers can also validate the wire protocol.
use super::*;
use serde_json::Value;
#[cfg(target_os = "macos")]
mod keyboard;
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    time::Duration,
};

#[derive(Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request<'a> {
    Preflight,
    Attach { pid: u32 },
    Fit,
    Input { action: InputAction },
    Observe,
    Capture { path: &'a Path },
}

pub struct MacSession {
    connection: BufReader<UnixStream>,
    #[cfg(target_os = "macos")]
    keyboard: Option<keyboard::KeyboardLayout>,
}
impl MacSession {
    pub fn connect(socket: &Path, pid: u32) -> Result<Self> {
        let stream = UnixStream::connect(socket).map_err(|e| {
            DriverError::Blocked(vec![format!(
                "macOS desktop helper unavailable at {}: {e}",
                socket.display()
            )])
        })?;
        stream.set_read_timeout(Some(Duration::from_secs(15)))?;
        stream.set_write_timeout(Some(Duration::from_secs(15)))?;
        let mut session = Self {
            connection: BufReader::new(stream),
            #[cfg(target_os = "macos")]
            keyboard: None,
        };
        let state = session.request(Request::Preflight)?;
        let missing: Vec<String> = ["accessibility", "post_events", "screen_capture", "desktop"]
            .into_iter()
            .filter(|name| state[*name] != true)
            .map(str::to_owned)
            .collect();
        if !missing.is_empty() {
            return Err(DriverError::Blocked(missing));
        }
        session.request(Request::Attach { pid })?;
        #[cfg(target_os = "macos")]
        {
            session.keyboard = Some(keyboard::KeyboardLayout::select_abc()?);
        }
        Ok(session)
    }
    fn request(&mut self, request: Request<'_>) -> Result<Value> {
        let mut bytes =
            serde_json::to_vec(&request).map_err(|e| DriverError::Protocol(e.to_string()))?;
        bytes.push(b'\n');
        self.connection.get_mut().write_all(&bytes)?;
        let mut line = String::new();
        (&mut self.connection)
            .take(1_048_576)
            .read_line(&mut line)?;
        if !line.ends_with('\n') {
            return Err(DriverError::Protocol(
                "truncated or oversized helper response".into(),
            ));
        }
        let response: Value =
            serde_json::from_str(&line).map_err(|e| DriverError::Protocol(e.to_string()))?;
        if let Some(error) = response.get("error") {
            return Err(DriverError::Protocol(error.to_string()));
        }
        response
            .get("result")
            .cloned()
            .ok_or_else(|| DriverError::Protocol("missing result".into()))
    }
}
impl DesktopDriver for MacSession {
    fn fit_window(&mut self) -> Result<DesktopRect> {
        serde_json::from_value(self.request(Request::Fit)?)
            .map_err(|e| DriverError::Protocol(e.to_string()))
    }
    fn input(&mut self, action: InputAction) -> Result<()> {
        self.request(Request::Input { action })?;
        Ok(())
    }
    fn observe(&mut self) -> Result<DesktopObservation> {
        serde_json::from_value(self.request(Request::Observe)?)
            .map_err(|e| DriverError::Protocol(e.to_string()))
    }
    fn capture(&mut self, path: &Path) -> Result<Capture> {
        let response = self.request(Request::Capture { path })?;
        let bounds: DesktopRect = serde_json::from_value(response["bounds"].clone())
            .map_err(|e| DriverError::Protocol(e.to_string()))?;
        let pixels = image::open(path)
            .map_err(|e| DriverError::Protocol(e.to_string()))?
            .to_rgb8();
        let mapping = CaptureMapping::new(bounds, pixels.width(), pixels.height())?;
        Ok(Capture { pixels, mapping })
    }
}
impl Drop for MacSession {
    fn drop(&mut self) {
        // Closing the connection releases every held key/button in the helper,
        // including during a panic or failed assertion. Do not block on an RPC.
        let _ = self.connection.get_mut().shutdown(std::net::Shutdown::Both);
    }
}

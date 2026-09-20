//! Scoped keyboard-layout ownership: physical key codes require a known layout.
//! Uses the public Text Input Sources API; no extra accessibility grant needed.
use super::{DriverError, Result};
use core_foundation::{
    array::{CFArray, CFArrayRef},
    base::{CFType, CFTypeRef, TCFType},
    dictionary::{CFDictionary, CFDictionaryRef},
    string::{CFString, CFStringRef},
};

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    static kTISPropertyInputSourceID: CFStringRef;
    fn TISCreateInputSourceList(properties: CFDictionaryRef, all_installed: u8) -> CFArrayRef;
    fn TISCopyCurrentKeyboardInputSource() -> CFTypeRef;
    fn TISSelectInputSource(source: CFTypeRef) -> i32;
}

pub(super) struct KeyboardLayout {
    previous: CFType,
}
impl KeyboardLayout {
    pub(super) fn select_abc() -> Result<Self> {
        // SAFETY: Copy/Create APIs return owned CF objects. Wrappers release
        // them exactly once; dictionary/list values stay alive during selection.
        unsafe {
            let previous = TISCopyCurrentKeyboardInputSource();
            if previous.is_null() {
                return Err(blocked("cannot read current keyboard input source"));
            }
            let guard = Self {
                previous: CFType::wrap_under_create_rule(previous),
            };
            let key = CFString::wrap_under_get_rule(kTISPropertyInputSourceID);
            let id = CFString::new("com.apple.keylayout.ABC");
            let filter = CFDictionary::from_CFType_pairs(&[(key, id)]);
            let sources = TISCreateInputSourceList(filter.as_concrete_TypeRef(), 0);
            if sources.is_null() {
                return Err(blocked("cannot enumerate enabled keyboard input sources"));
            }
            let sources = CFArray::<CFType>::wrap_under_create_rule(sources);
            let source = sources
                .get(0)
                .ok_or_else(|| blocked("enable the ABC keyboard layout in macOS input sources"))?;
            let status = TISSelectInputSource(source.as_CFTypeRef());
            if status != 0 {
                return Err(blocked(&format!(
                    "selecting ABC keyboard layout failed: {status}"
                )));
            }
            Ok(guard)
        }
    }
}
impl Drop for KeyboardLayout {
    fn drop(&mut self) {
        // SAFETY: the retained input source remains alive until after this call.
        let status = unsafe { TISSelectInputSource(self.previous.as_CFTypeRef()) };
        if status != 0 {
            eprintln!("failed to restore macOS keyboard input source: {status}");
        }
    }
}
fn blocked(message: &str) -> DriverError {
    DriverError::Blocked(vec![message.to_owned()])
}

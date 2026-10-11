use neovm_core::window::{PresentedWindowChromeString, PresentedWindowChromeStrings};

fn main() {
    let sources = PresentedWindowChromeStrings::default();
    // Published chrome only exposes an immutable slice, even for one holder.
    let _: &mut [PresentedWindowChromeString] = sources.as_slice();
}

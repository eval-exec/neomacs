use glib::translate::IntoGlibPtr;

use super::GlibErrorSlot;

#[test]
fn empty_error_slot_uses_the_callers_fallback() {
    let slot = GlibErrorSlot::new();

    assert_eq!(
        slot.into_message("native call failed"),
        "native call failed"
    );
}

#[test]
fn error_slot_owns_and_formats_the_error_returned_by_glib() {
    let mut slot = GlibErrorSlot::new();
    let error = glib::Error::new(glib::FileError::Failed, "native failure");

    unsafe {
        *slot.out_ptr() = error.into_glib_ptr();
    }

    assert_eq!(slot.into_message("fallback"), "native failure");
}

use super::*;

#[test]
fn test_platform_display_types() {
    // Just verify types compile
    let _: *mut plat::WPEDisplay = std::ptr::null_mut();
    let _: *mut plat::WPEView = std::ptr::null_mut();
    let _: *mut plat::WPEBuffer = std::ptr::null_mut();
}

#[test]
fn display_adapter_constructs_the_frame_acknowledging_view_type() {
    unsafe {
        let delegate = plat::wpe_display_headless_new();
        assert!(!delegate.is_null());
        let display = super::native::new_display(delegate);
        plat::g_object_unref(delegate.cast());
        assert!(!display.is_null());

        let view = plat::wpe_view_new(display);
        assert!(!view.is_null());
        assert!(super::native::is_custom_view(view));
        assert_eq!(plat::wpe_view_get_display(view), display);

        let toplevel = plat::wpe_view_get_toplevel(view);
        assert!(!toplevel.is_null());
        assert_eq!(plat::wpe_toplevel_get_display(toplevel), display);

        plat::g_object_unref(view.cast());
        plat::g_object_unref(display.cast());
    }
}

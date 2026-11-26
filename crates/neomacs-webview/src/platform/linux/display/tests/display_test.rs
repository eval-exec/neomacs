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

#[test]
fn linux_wpe_raw_pointer_boundaries_are_sound() {
    let view_src = include_str!("../../view.rs");
    let creation_block = view_src
        .split("pub(crate) struct WpeViewCreation")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .expect("WpeViewCreation struct definition");
    assert!(
        !creation_block.contains("NonNull<wk::WebKitNetworkSession>"),
        "WpeViewCreation must borrow &NetworkSession rather than accepting a raw pointer (B5)"
    );
    assert!(
        !creation_block.contains("NonNull<wk::WebKitWebView>"),
        "WpeViewCreation must borrow Option<&WpeWebView> rather than accepting a raw pointer (B5)"
    );

    let display_src = include_str!("../../display.rs");
    assert!(
        display_src.contains("pub unsafe fn buffer_dmabuf_info("),
        "buffer_dmabuf_info dereferences a raw *mut WPEBuffer and must be `pub unsafe fn` (B5)"
    );
    // SAFETY: null is explicitly accepted and returns `None`.
    assert!(unsafe { buffer_dmabuf_info(std::ptr::null_mut()) }.is_none());

    // SAFETY: `non_dmabuf` is a live GObject (`GTypeInstance`) for the duration
    // of the call; `buffer_dmabuf_info` checks `g_type_check_instance_is_a`
    // against `WPEBufferDMABuf` and returns `None` without calling DMA-BUF
    // getters.
    unsafe {
        let non_dmabuf = plat::wpe_display_headless_new();
        assert!(!non_dmabuf.is_null());
        assert!(buffer_dmabuf_info(non_dmabuf.cast()).is_none());
        plat::g_object_unref(non_dmabuf.cast());
    }
}

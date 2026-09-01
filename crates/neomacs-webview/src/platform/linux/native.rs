//! Rust-owned WPEPlatform GObject subclasses.
//!
//! WPE exposes frame acceptance through the `WPEViewClass::render_buffer`
//! virtual method. These wrappers use gtk-rs' GObject subclass machinery so
//! the platform adapter, its state, and every lifetime rule remain Rust code.

use std::cell::RefCell;

use glib::prelude::*;
use glib::subclass::prelude::*;
use glib::translate::*;

use super::sys::platform as plat;

/// Contain panics at every Rust vfunc exported through WPE's C ABI.
fn guard_native_vfunc<T>(name: &str, neutral: T, body: impl FnOnce() -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(value) => value,
        Err(_) => {
            tracing::error!(
                name,
                "Rust WPE vfunc panicked; returning its neutral result"
            );
            neutral
        }
    }
}

pub(super) type RenderBufferCallback = unsafe extern "C" fn(
    *mut plat::WPEView,
    *mut plat::WPEBuffer,
    *const plat::WPERectangle,
    plat::guint,
    *mut *mut plat::GError,
    plat::gpointer,
) -> plat::gboolean;

glib::wrapper! {
    struct WpeDisplay(Object<plat::WPEDisplay, plat::WPEDisplayClass>);

    match fn {
        type_ => || unsafe { plat::wpe_display_get_type() as glib::ffi::GType },
    }
}

trait WpeDisplayImpl: ObjectImpl + ObjectSubclass<Type: IsA<WpeDisplay>> {}

unsafe impl<T: WpeDisplayImpl> IsSubclassable<T> for WpeDisplay {}

glib::wrapper! {
    struct WpeView(Object<plat::WPEView, plat::WPEViewClass>);

    match fn {
        type_ => || unsafe { plat::wpe_view_get_type() as glib::ffi::GType },
    }
}

trait WpeViewImpl: ObjectImpl + ObjectSubclass<Type: IsA<WpeView>> {}

unsafe impl<T: WpeViewImpl> IsSubclassable<T> for WpeView {}

glib::wrapper! {
    struct WpeToplevel(Object<plat::WPEToplevel, plat::WPEToplevelClass>);

    match fn {
        type_ => || unsafe { plat::wpe_toplevel_get_type() as glib::ffi::GType },
    }
}

trait WpeToplevelImpl: ObjectImpl + ObjectSubclass<Type: IsA<WpeToplevel>> {}

unsafe impl<T: WpeToplevelImpl> IsSubclassable<T> for WpeToplevel {}

struct RenderBufferSink {
    callback: RenderBufferCallback,
    user_data: plat::gpointer,
}

mod view_imp {
    use super::*;

    #[derive(Default)]
    pub(super) struct NeomacsWpeView {
        pub(super) sink: RefCell<Option<RenderBufferSink>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for NeomacsWpeView {
        const NAME: &'static str = "NeomacsWpeView";
        type Type = super::NeomacsWpeView;
        type ParentType = WpeView;

        fn class_init(class: &mut Self::Class) {
            let class = unsafe { &mut *(class as *mut _ as *mut plat::WPEViewClass) };
            class.render_buffer = Some(super::render_buffer);
        }
    }

    impl ObjectImpl for NeomacsWpeView {}
    impl WpeViewImpl for NeomacsWpeView {}
}

glib::wrapper! {
    struct NeomacsWpeView(ObjectSubclass<view_imp::NeomacsWpeView>) @extends WpeView;
}

unsafe extern "C" fn render_buffer(
    view: *mut plat::WPEView,
    buffer: *mut plat::WPEBuffer,
    damage_rects: *const plat::WPERectangle,
    damage_rect_count: plat::guint,
    error: *mut *mut plat::GError,
) -> plat::gboolean {
    guard_native_vfunc("render-buffer", 0, || unsafe {
        if view.is_null() {
            return 0;
        }
        let instance = &*(view as *mut <view_imp::NeomacsWpeView as ObjectSubclass>::Instance);
        let Ok(sink) = instance.imp().sink.try_borrow() else {
            return 0;
        };
        let Some(sink) = sink.as_ref() else {
            return 0;
        };
        (sink.callback)(
            view,
            buffer,
            damage_rects,
            damage_rect_count,
            error,
            sink.user_data,
        )
    })
}

mod toplevel_imp {
    use super::*;

    #[derive(Default)]
    pub(super) struct NeomacsWpeToplevel;

    #[glib::object_subclass]
    impl ObjectSubclass for NeomacsWpeToplevel {
        const NAME: &'static str = "NeomacsWpeToplevel";
        type Type = super::NeomacsWpeToplevel;
        type ParentType = WpeToplevel;

        fn class_init(class: &mut Self::Class) {
            let class = unsafe { &mut *(class as *mut _ as *mut plat::WPEToplevelClass) };
            class.resize = Some(super::resize_toplevel);
            class.set_fullscreen = Some(super::set_toplevel_fullscreen);
        }
    }

    impl ObjectImpl for NeomacsWpeToplevel {}
    impl WpeToplevelImpl for NeomacsWpeToplevel {}
}

glib::wrapper! {
    struct NeomacsWpeToplevel(ObjectSubclass<toplevel_imp::NeomacsWpeToplevel>) @extends WpeToplevel;
}

unsafe extern "C" fn resize_view(
    _toplevel: *mut plat::WPEToplevel,
    view: *mut plat::WPEView,
    dimensions: plat::gpointer,
) -> plat::gboolean {
    guard_native_vfunc("resize-view", 0, || unsafe {
        if view.is_null() || dimensions.is_null() {
            return 0;
        }
        let dimensions = &*(dimensions.cast::<(i32, i32)>());
        plat::wpe_view_resized(view, dimensions.0, dimensions.1);
        0
    })
}

unsafe extern "C" fn resize_toplevel(
    toplevel: *mut plat::WPEToplevel,
    width: i32,
    height: i32,
) -> plat::gboolean {
    guard_native_vfunc("resize-toplevel", 0, || unsafe {
        if toplevel.is_null() {
            return 0;
        }
        plat::wpe_toplevel_resized(toplevel, width, height);
        let dimensions = (width, height);
        plat::wpe_toplevel_foreach_view(
            toplevel,
            Some(resize_view),
            (&raw const dimensions).cast_mut().cast(),
        );
        1
    })
}

unsafe extern "C" fn set_toplevel_fullscreen(
    toplevel: *mut plat::WPEToplevel,
    fullscreen: plat::gboolean,
) -> plat::gboolean {
    guard_native_vfunc("set-toplevel-fullscreen", 0, || unsafe {
        if toplevel.is_null() {
            return 0;
        }
        let mut state = plat::wpe_toplevel_get_state(toplevel);
        if fullscreen != 0 {
            state |= plat::WPEToplevelState_WPE_TOPLEVEL_STATE_FULLSCREEN;
        } else {
            state &= !plat::WPEToplevelState_WPE_TOPLEVEL_STATE_FULLSCREEN;
        }
        plat::wpe_toplevel_state_changed(toplevel, state);
        1
    })
}

mod display_imp {
    use super::*;

    #[derive(Default)]
    pub(super) struct NeomacsWpeDisplay {
        pub(super) delegate: RefCell<Option<WpeDisplay>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for NeomacsWpeDisplay {
        const NAME: &'static str = "NeomacsWpeDisplay";
        type Type = super::NeomacsWpeDisplay;
        type ParentType = WpeDisplay;

        fn class_init(class: &mut Self::Class) {
            let class = unsafe { &mut *(class as *mut _ as *mut plat::WPEDisplayClass) };
            class.connect = Some(super::connect_display);
            class.create_view = Some(super::create_view);
            class.get_egl_display = Some(super::egl_display);
            class.get_keymap = Some(super::keymap);
            class.get_clipboard = Some(super::clipboard);
            class.get_preferred_dma_buf_formats = Some(super::preferred_dma_buf_formats);
            class.get_n_screens = Some(super::screen_count);
            class.get_screen = Some(super::screen);
            class.get_drm_device = Some(super::drm_device);
            class.use_explicit_sync = Some(super::uses_explicit_sync);
            class.create_gamepad_manager = Some(super::create_gamepad_manager);
        }
    }

    impl ObjectImpl for NeomacsWpeDisplay {}
    impl WpeDisplayImpl for NeomacsWpeDisplay {}
}

glib::wrapper! {
    struct NeomacsWpeDisplay(ObjectSubclass<display_imp::NeomacsWpeDisplay>) @extends WpeDisplay;
}

unsafe fn delegate(display: *mut plat::WPEDisplay) -> Option<*mut plat::WPEDisplay> {
    let instance = &*(display as *mut <display_imp::NeomacsWpeDisplay as ObjectSubclass>::Instance);
    instance
        .imp()
        .delegate
        .try_borrow()
        .ok()?
        .as_ref()
        .map(|delegate| delegate.to_glib_none().0)
}

unsafe extern "C" fn connect_display(
    display: *mut plat::WPEDisplay,
    error: *mut *mut plat::GError,
) -> plat::gboolean {
    guard_native_vfunc("connect-display", 0, || unsafe {
        delegate(display).map_or(0, |delegate| plat::wpe_display_connect(delegate, error))
    })
}

unsafe extern "C" fn create_view(display: *mut plat::WPEDisplay) -> *mut plat::WPEView {
    guard_native_vfunc("create-view", std::ptr::null_mut(), || unsafe {
        if display.is_null() {
            return std::ptr::null_mut();
        }
        let view = plat::g_object_new(
            NeomacsWpeView::static_type().into_glib() as plat::GType,
            c"display".as_ptr(),
            display.cast::<libc::c_void>(),
            std::ptr::null::<libc::c_char>(),
        )
        .cast::<plat::WPEView>();
        if view.is_null() {
            return std::ptr::null_mut();
        }
        let toplevel = plat::g_object_new(
            NeomacsWpeToplevel::static_type().into_glib() as plat::GType,
            c"display".as_ptr(),
            display.cast::<libc::c_void>(),
            c"max-views".as_ptr(),
            1u32,
            std::ptr::null::<libc::c_char>(),
        )
        .cast::<plat::WPEToplevel>();
        if toplevel.is_null() {
            plat::g_object_unref(view.cast());
            return std::ptr::null_mut();
        }
        plat::wpe_toplevel_state_changed(
            toplevel,
            plat::WPEToplevelState_WPE_TOPLEVEL_STATE_ACTIVE,
        );
        plat::wpe_view_set_toplevel(view, toplevel);
        plat::g_object_unref(toplevel.cast());
        view
    })
}

unsafe extern "C" fn egl_display(
    display: *mut plat::WPEDisplay,
    error: *mut *mut plat::GError,
) -> plat::gpointer {
    guard_native_vfunc("get-egl-display", std::ptr::null_mut(), || unsafe {
        delegate(display).map_or(std::ptr::null_mut(), |delegate| {
            plat::wpe_display_get_egl_display(delegate, error)
        })
    })
}

unsafe extern "C" fn keymap(display: *mut plat::WPEDisplay) -> *mut plat::WPEKeymap {
    guard_native_vfunc("get-keymap", std::ptr::null_mut(), || unsafe {
        delegate(display).map_or(std::ptr::null_mut(), |delegate| {
            plat::wpe_display_get_keymap(delegate)
        })
    })
}

unsafe extern "C" fn clipboard(display: *mut plat::WPEDisplay) -> *mut plat::WPEClipboard {
    guard_native_vfunc("get-clipboard", std::ptr::null_mut(), || unsafe {
        delegate(display).map_or(std::ptr::null_mut(), |delegate| {
            plat::wpe_display_get_clipboard(delegate)
        })
    })
}

unsafe extern "C" fn preferred_dma_buf_formats(
    display: *mut plat::WPEDisplay,
) -> *mut plat::WPEBufferDMABufFormats {
    guard_native_vfunc(
        "get-preferred-dma-buf-formats",
        std::ptr::null_mut(),
        || unsafe {
            delegate(display).map_or(std::ptr::null_mut(), |delegate| {
                plat::wpe_display_get_preferred_dma_buf_formats(delegate)
            })
        },
    )
}

unsafe extern "C" fn screen_count(display: *mut plat::WPEDisplay) -> plat::guint {
    guard_native_vfunc("get-screen-count", 0, || unsafe {
        delegate(display).map_or(0, |delegate| plat::wpe_display_get_n_screens(delegate))
    })
}

unsafe extern "C" fn screen(
    display: *mut plat::WPEDisplay,
    index: plat::guint,
) -> *mut plat::WPEScreen {
    guard_native_vfunc("get-screen", std::ptr::null_mut(), || unsafe {
        delegate(display).map_or(std::ptr::null_mut(), |delegate| {
            plat::wpe_display_get_screen(delegate, index)
        })
    })
}

unsafe extern "C" fn drm_device(display: *mut plat::WPEDisplay) -> *mut plat::WPEDRMDevice {
    guard_native_vfunc("get-drm-device", std::ptr::null_mut(), || unsafe {
        delegate(display).map_or(std::ptr::null_mut(), |delegate| {
            plat::wpe_display_get_drm_device(delegate)
        })
    })
}

unsafe extern "C" fn uses_explicit_sync(display: *mut plat::WPEDisplay) -> plat::gboolean {
    guard_native_vfunc("uses-explicit-sync", 0, || unsafe {
        delegate(display).map_or(0, |delegate| plat::wpe_display_use_explicit_sync(delegate))
    })
}

unsafe extern "C" fn create_gamepad_manager(
    display: *mut plat::WPEDisplay,
) -> *mut plat::WPEGamepadManager {
    guard_native_vfunc("create-gamepad-manager", std::ptr::null_mut(), || unsafe {
        delegate(display).map_or(std::ptr::null_mut(), |delegate| {
            plat::wpe_display_create_gamepad_manager(delegate)
        })
    })
}

pub(super) unsafe fn new_display(delegate: *mut plat::WPEDisplay) -> *mut plat::WPEDisplay {
    if delegate.is_null() {
        return std::ptr::null_mut();
    }
    let display: NeomacsWpeDisplay = glib::Object::new();
    display
        .imp()
        .delegate
        .replace(Some(from_glib_none(delegate)));
    let display: *mut <display_imp::NeomacsWpeDisplay as ObjectSubclass>::Instance =
        display.into_glib_ptr();
    display.cast::<plat::WPEDisplay>()
}

pub(super) unsafe fn is_custom_view(view: *mut plat::WPEView) -> bool {
    !view.is_null()
        && plat::g_type_check_instance_is_a(
            view.cast::<plat::GTypeInstance>(),
            NeomacsWpeView::static_type().into_glib() as plat::GType,
        ) != 0
}

pub(super) unsafe fn set_render_buffer_callback(
    view: *mut plat::WPEView,
    callback: RenderBufferCallback,
    user_data: plat::gpointer,
) -> bool {
    if !is_custom_view(view) {
        return false;
    }
    let instance = &*(view as *mut <view_imp::NeomacsWpeView as ObjectSubclass>::Instance);
    let Ok(mut sink) = instance.imp().sink.try_borrow_mut() else {
        return false;
    };
    *sink = Some(RenderBufferSink {
        callback,
        user_data,
    });
    true
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_vfunc_panics_are_contained_at_the_abi_boundary() {
        assert_eq!(
            super::guard_native_vfunc("test", 41_u32, || panic!("contained")),
            41
        );
    }
}

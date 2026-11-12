//! Exact GNU ftfont_pattern_entity bitmap property projection.
use freetype::freetype_sys as ft;
use std::ffi::{c_char, c_int};

// FreeType's ftbdf.h ABI is not exposed by the pinned freetype-sys bindings.
// Its enum has C-int representation; the union contains a pointer or a 32-bit
// signed/unsigned scalar. In particular, these are not platform C longs.
#[repr(C)]
union PropertyValue {
    atom: *const c_char,
    integer: i32,
    cardinal: u32,
}

#[repr(C)]
struct Property {
    kind: c_int,
    value: PropertyValue,
}

unsafe extern "C" {
    fn FT_Get_BDF_Property(
        face: ft::FT_Face,
        name: *const c_char,
        property: *mut Property,
    ) -> ft::FT_Error;
}

pub(super) fn average_width(file: &str, face_index: u32) -> Option<i32> {
    let library = freetype::Library::init().ok()?;
    let mut face = library
        .new_face(file, isize::try_from(face_index).ok()?)
        .ok()?;
    let mut property = Property {
        kind: 0,
        value: PropertyValue {
            atom: std::ptr::null(),
        },
    };
    // SAFETY: `face` owns a live FT_Face until after this call and its property
    // read; the property name is static/NUL-terminated, and the initialized
    // output has the exact ftbdf.h layout. The function retains no Rust borrow.
    let status =
        unsafe { FT_Get_BDF_Property(face.raw_mut(), c"AVERAGE_WIDTH".as_ptr(), &mut property) };
    if status != 0 || property.kind != 2 {
        return None;
    }
    // SAFETY: successful lookup with BDF_PROPERTY_TYPE_INTEGER initializes
    // exactly this union member. Zero and negative integers remain values.
    Some(unsafe { property.value.integer })
}

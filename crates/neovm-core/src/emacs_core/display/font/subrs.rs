//! Native Lisp declarations owned by GNU `src/font.c`'s mirror.

use super::*;
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

crate::emacs_core::subr::define_subrs! {
    SubrSpec::new(
        "fontp",
        NativeFn::NoContextVec(fontp),
        SubrArity::new(1, Some(2)),
    ),
    SubrSpec::new(
        "font-spec",
        NativeFn::NoContextVec(font_spec),
        SubrArity::new(0, None),
    ),
    SubrSpec::new(
        "font-get",
        NativeFn::NoContextVec(font_get),
        SubrArity::new(2, Some(2)),
    ),
    SubrSpec::new(
        "font-face-attributes",
        NativeFn::NoContextVec(font_face_attributes),
        SubrArity::new(1, Some(2)),
    ),
    SubrSpec::new(
        "font-put",
        NativeFn::NoContextVec(font_put),
        SubrArity::new(3, Some(3)),
    ),
    SubrSpec::new(
        "list-fonts",
        NativeFn::ContextVec(list_fonts),
        SubrArity::new(1, Some(4)),
    ),
    SubrSpec::new(
        "font-family-list",
        NativeFn::ContextVec(font_family_list),
        SubrArity::new(0, Some(1)),
    ),
    SubrSpec::new(
        "find-font",
        NativeFn::ContextVec(find_font),
        SubrArity::new(1, Some(2)),
    ),
    SubrSpec::new(
        "font-xlfd-name",
        NativeFn::NoContextVec(font_xlfd_name),
        SubrArity::new(1, Some(3)),
    ),
    SubrSpec::new(
        "clear-font-cache",
        NativeFn::NoContextVec(clear_font_cache),
        SubrArity::new(0, Some(0)),
    ),
    SubrSpec::new(
        "font-shape-gstring",
        NativeFn::ContextVec(font_shape_gstring),
        SubrArity::new(2, Some(2)),
    ),
    SubrSpec::new(
        "font-variation-glyphs",
        NativeFn::NoContextVec(font_variation_glyphs),
        SubrArity::new(2, Some(2)),
    ),
    SubrSpec::new(
        "internal-char-font",
        NativeFn::ContextVec(internal_char_font),
        SubrArity::new(1, Some(2)),
    ),
    SubrSpec::new(
        "close-font",
        NativeFn::NoContextVec(close_font),
        SubrArity::new(1, Some(2)),
    ),
    SubrSpec::new(
        "query-font",
        NativeFn::ContextVec(query_font),
        SubrArity::new(1, Some(1)),
    ),
    SubrSpec::new(
        "font-get-glyphs",
        NativeFn::NoContextVec(font_get_glyphs),
        SubrArity::new(3, Some(4)),
    ),
    SubrSpec::new(
        "font-has-char-p",
        NativeFn::NoContextVec(font_has_char_p),
        SubrArity::new(2, Some(3)),
    ),
    SubrSpec::new(
        "font-match-p",
        NativeFn::NoContextVec(font_match_p),
        SubrArity::new(2, Some(2)),
    ),
    SubrSpec::new(
        "font-at",
        NativeFn::ContextVec(font_at),
        SubrArity::new(1, Some(3)),
    )
    .requires_eval_state(),
    SubrSpec::new(
        "font-info",
        NativeFn::ContextVec(font_info),
        SubrArity::new(1, Some(2)),
    ),
}

//! Divergence tests: computed SVG animation (the `:animation` extension).
//!
//! The feature is a deliberate, gated divergence: GNU renders SVG through
//! librsvg, which has no document clock, so an animated SVG is a static
//! frame there. The default (no `:animation` property) must therefore stay
//! GNU-compatible — these batch tests pin specification construction and
//! frame selection. Raster metadata requires a window-system frame in GNU
//! and is covered by renderer and GUI tests instead.
//! The opt-in arm (`:animation t` materializing frames) is exercised by the
//! renderer engine tests in `neomacs-renderer-wgpu/src/svg_animation`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// An SMIL spinner-class document: `values`/`dur`/`repeatCount` on a
/// parent-targeted rule — the exact subset the engine materializes.
///
/// The document must be embedded as an Elisp *string*: Rust's `{:?}`
/// escaping of this ASCII document (`\"`, `\\`) is valid Elisp string
/// syntax. Interpolating it bare evaluates an unbound `<svg` symbol in
/// both emacsen, and a parity helper that accepts matching errors would
/// pass without ever loading an image.
const ANIMATED_SVG: &str = concat!(
    "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"100\" ",
    "viewBox=\"0 0 100 100\"><circle id=\"dot\" cx=\"50\" cy=\"50\" r=\"10\" ",
    "fill=\"tomato\"><animate attributeName=\"r\" values=\"10;40;10\" ",
    "dur=\"2s\" repeatCount=\"indefinite\"/></circle></svg>",
);

#[test]
fn divergence_animated_svg_static_by_default_matches_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (image svg nil 0 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        &format!(
            r#"(progn (require 'image)
              (let ((image (create-image {ANIMATED_SVG:?} 'svg t)))
                (list (car image) (plist-get (cdr image) :type)
                      (plist-get (cdr image) :animation)
                      (image-current-frame image) (image-animate-timer image))))"#
        ),
        expect,
    );
}

#[test]
fn divergence_animated_svg_explicit_nil_preserves_nonzero_index_matches_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A static SVG accepts a nonzero index in its spec. It must remain a
    // valid image; the raster backend ignores that index when policy is off.
    let expect = expect_test::expect![[r#""OK (image svg nil 19 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        &format!(
            r#"(progn (require 'image)
              (let ((image (create-image {ANIMATED_SVG:?} 'svg t :animation nil :index 19)))
                (list (car image) (plist-get (cdr image) :type)
                      (plist-get (cdr image) :animation)
                      (image-current-frame image) (image-animate-timer image))))"#
        ),
        expect,
    );
}

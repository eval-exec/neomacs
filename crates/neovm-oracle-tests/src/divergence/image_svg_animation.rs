//! Divergence tests: computed SVG animation (the `:animation` extension).
//!
//! The feature is a deliberate, gated divergence: GNU renders SVG through
//! librsvg, which has no document clock, so an animated SVG is a static
//! frame there. The default (no `:animation` property) must therefore stay
//! byte-for-byte GNU-compatible — that is what the parity test here pins.
//! The opt-in arm (`:animation t` materializing frames) is exercised by the
//! renderer engine tests in `neomacs-renderer-wgpu/src/svg_animation`.

use crate::common::assert_oracle_parity;
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

    // Both renderers must report a single frame: GNU because librsvg has
    // no animation, neomacs because the policy is off. The frame count is
    // what `image-multi-frame-p` reads, so pinning it pins the
    // default-path divergence gate.
    assert_oracle_parity(&format!(
        r#"(image-multi-frame-p (list 'image :type 'svg :data {ANIMATED_SVG:?}))"#
    ));
}

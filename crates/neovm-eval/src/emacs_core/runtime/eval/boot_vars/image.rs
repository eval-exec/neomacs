//! C-level DEFVAR seeding moved below the display layer (stage-4 K2 display).
//!
//! The evaluator bootstrap used to call the display modules' `syms_of_*`
//! equivalents; these bodies only touch the Obarray and lower-crate
//! vocabulary, so they live with the bootstrap that runs them.

use neovm_value::display_vocab::{decode_env_path_default, supported_image_types_value};
use neovm_value::system_vocab::platform_facts::X_BITMAP_FILE_PATH;
use neovm_value::value::Value;

pub(crate) fn register_bootstrap_vars(obarray: &mut neovm_value::symbol::Obarray) {
    // image.c:13265, `Vx_bitmap_file_path = decode_env_path (0, PATH_BITMAPS, 0)`.
    // PATH_BITMAPS is the configure `bitmapdir` substitution -- a compiled-in
    // path, not a probe of the running system -- so the value for this
    // platform's reference build lives with the other platform facts.
    obarray.define_special_variable(
        "x-bitmap-file-path",
        decode_env_path_default(X_BITMAP_FILE_PATH),
    );
    // image.c:13034 DEFVAR_LISP, make_float (MAX_IMAGE_SIZE) = 10.0.
    obarray.define_special_variable("max-image-size", Value::make_float(10.0));
    // image.c:13269 DEFVAR_LISP, make_fixnum (300).
    obarray.define_special_variable("image-cache-eviction-delay", Value::fixnum(300));
    // image.c:13279 DEFVAR_LISP, Qauto.
    obarray.define_special_variable("image-scaling-factor", Value::symbol("auto"));
    // image.c:13028 DEFVAR_LISP; GNU's define_image_type fills the list at
    // C init, our equivalent enumerates the built-in decoders.
    obarray.define_special_variable("image-types", supported_image_types_value());
}

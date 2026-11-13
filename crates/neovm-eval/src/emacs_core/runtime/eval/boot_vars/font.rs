//! C-level DEFVAR seeding moved below the display layer (stage-4 K2 display).
//!
//! The evaluator bootstrap used to call the display modules' `syms_of_*`
//! equivalents; these bodies only touch the Obarray and lower-crate
//! vocabulary, so they live with the bootstrap that runs them.

use neovm_value::display_vocab::{
    FONT_SLANT_STYLE_TABLE, FONT_WEIGHT_STYLE_TABLE, FONT_WIDTH_STYLE_TABLE, font_style_table,
};
use neovm_value::value::Value;

pub(crate) fn init_font_vars(obarray: &mut neovm_value::symbol::Obarray) {
    for (name, value) in [
        (
            "font-weight-table",
            font_style_table(FONT_WEIGHT_STYLE_TABLE),
        ),
        ("font-slant-table", font_style_table(FONT_SLANT_STYLE_TABLE)),
        ("font-width-table", font_style_table(FONT_WIDTH_STYLE_TABLE)),
    ] {
        obarray.seed_value(name, value);
        obarray.make_special(name);
        obarray.set_constant(name);
    }

    obarray.seed_value("font-log", Value::T);
    obarray.make_special("font-log");
}

//! C-level DEFVAR seeding moved below the display layer (stage-4 K2 display).
//!
//! The evaluator bootstrap used to call the display modules' `syms_of_*`
//! equivalents; these bodies only touch the Obarray and lower-crate
//! vocabulary, so they live with the bootstrap that runs them.

use crate::emacs_core::reader::ObarrayReadSymbolsExt;
use neovm_value::display_vocab::{
    FONT_SLANT_STYLE_TABLE, FONT_WEIGHT_STYLE_TABLE, FONT_WIDTH_STYLE_TABLE, font_style_table,
};
use neovm_value::value::Value;

pub(crate) fn init_font_vars(obarray: &mut neovm_value::symbol::Obarray) {
    // GNU builds `font-weight-table' / `font-slant-table' / `font-width-table'
    // with `BUILD_STYLE_TABLE' (`src/font.c:5707-5721'), and every name in
    // those C tables is interned with `intern_c_string' - so `normal', `bold',
    // `reverse-italic', `reverse-oblique' and the rest are startup obarray
    // members even though most of them are not DEFSYMs. This port built the
    // same tables from the same vocabulary but only touched the process
    // interner, so those spellings were missing from `(obarray)'.
    for (_, names) in FONT_WEIGHT_STYLE_TABLE
        .iter()
        .chain(FONT_SLANT_STYLE_TABLE)
        .chain(FONT_WIDTH_STYLE_TABLE)
    {
        for name in *names {
            obarray.intern(name);
        }
    }
    for (name, value) in [
        (
            "font-weight-table",
            font_style_table(FONT_WEIGHT_STYLE_TABLE),
        ),
        ("font-slant-table", font_style_table(FONT_SLANT_STYLE_TABLE)),
        ("font-width-table", font_style_table(FONT_WIDTH_STYLE_TABLE)),
    ] {
        // GNU publishes these in `Vfont_weight_table` and friends after
        // interning the symbols (font.c); the port builds the tables in
        // Rust, so publish the symbols as members at the seed boundary
        // (PEER 5786 census: font weight/slant/width aliases).
        obarray.intern_member_symbols_in(value);
        obarray.seed_value(name, value);
        obarray.make_special(name);
        obarray.set_constant(name);
    }

    obarray.seed_value("font-log", Value::T);
    obarray.make_special("font-log");
}

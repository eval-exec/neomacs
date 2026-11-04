//! C-level DEFVAR seeding moved below the display layer (stage-4 K2 display).
//!
//! The evaluator bootstrap used to call the display modules' `syms_of_*`
//! equivalents; these bodies only touch the Obarray and lower-crate
//! vocabulary, so they live with the bootstrap that runs them.

use neovm_value::symbol::Obarray;
use neovm_value::value::Value;

pub(crate) fn init_xwidget_variables(obarray: &mut Obarray) {
    obarray.seed_value("xwidget-list", Value::NIL);
    obarray.make_special("xwidget-list");
    obarray.seed_value("xwidget-view-list", Value::NIL);
    obarray.make_special("xwidget-view-list");
    obarray.seed_value("xwidget-webkit-disable-javascript", Value::NIL);
    obarray.make_special("xwidget-webkit-disable-javascript");
}

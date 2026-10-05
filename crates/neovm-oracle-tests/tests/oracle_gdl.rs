//! Lane GDL oracle cases, isolated to keep compilation memory bounded.
#[path = "../src/common.rs"]
mod common;
#[cfg(test)]
#[path = "../src/tests/gdl_ash_validation.rs"]
mod gdl_ash_validation;
#[cfg(test)]
#[path = "../src/format/tests/gdl_float.rs"]
mod gdl_float;
#[cfg(test)]
#[path = "../src/format/tests/gdl_integer.rs"]
mod gdl_integer;
#[cfg(test)]
#[path = "../src/tests/gdl_integer_width.rs"]
mod gdl_integer_width;
#[cfg(test)]
#[path = "../src/tests/gdl_ldexp_ieee.rs"]
mod gdl_ldexp_ieee;
#[cfg(test)]
#[path = "../src/tests/gdl_logb_exact.rs"]
mod gdl_logb_exact;
#[cfg(test)]
#[path = "../src/tests/gdl_mod_nan.rs"]
mod gdl_mod_nan;
#[cfg(test)]
#[path = "../src/tests/gdl_random_bignum.rs"]
mod gdl_random_bignum;
#[cfg(test)]
#[path = "../src/tests/gdl_sequences.rs"]
mod gdl_sequences;

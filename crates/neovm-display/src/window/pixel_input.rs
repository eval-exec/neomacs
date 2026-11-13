//! Owned dependencies of pixel expressions, not an evaluator or a Lisp clone.
//!
//! Follows the arithmetic/number forms consumed by display_pixel_calc and GNU
//! xdisp.c's calc_pixel_width_or_height. Image operands share the catalog's
//! owned identity; resource-content revisions have a separate contract.

use crate::emacs_core::image_catalog::ImageSpecIdentity;
use neovm_value::value::Value;
use rustc_hash::FxHashMap;

pub(crate) use neovm_eval::emacs_core::eval::data::layout_input::pixel_input::{
    PixelInput, Role, SpaceInput, Token,
};

pub(crate) trait SpaceInputExt: Sized {
    fn capture(value: Value) -> Option<Self>;
}

impl SpaceInputExt for SpaceInput {
    fn capture(value: Value) -> Option<Self> {
        use neovm_eval::emacs_core::eval::data::display_spec::DisplaySpaceKey;
        if !value.is_cons() || !value.cons_car().is_bare_symbol_named("space") {
            return None;
        }
        let items = neovm_value::value::list_to_vec(&value)?;
        let mut operands = std::array::from_fn(|_| None);
        for pair in items[1..].chunks_exact(2) {
            if let Some(key) = DisplaySpaceKey::from_lisp_value(pair[0]) {
                // Geometry evaluation uses the first occurrence.
                operands[key as usize].get_or_insert_with(|| PixelInput::capture(pair[1]));
            }
        }
        Some(Self {
            identity: value.bits(),
            operands,
        })
    }
}

pub(crate) trait PixelInputExt: Sized {
    fn capture(value: Value) -> Self;
}

impl PixelInputExt for PixelInput {
    fn capture(value: Value) -> Self {
        if !value.is_cons() {
            return Self::Atom(value.bits());
        }
        let mut tokens = Vec::new();
        let mut pending = vec![(value, Role::Expression)];
        let mut seen = FxHashMap::default();
        while let Some((value, role)) = pending.pop() {
            if !value.is_cons() {
                tokens.push(match role {
                    Role::Expression => Token::Atom(value.bits()),
                    // Arithmetic ignores a non-cons tail, including dotted tails.
                    Role::Arguments => Token::End,
                });
                continue;
            }
            let next = tokens.len();
            if let Some(previous) = seen.insert((value.bits(), role), next) {
                tokens.push(Token::Reference(previous));
                continue;
            }
            let car = value.cons_car();
            let cdr = value.cons_cdr();
            match role {
                Role::Arguments => {
                    pending.push((cdr, Role::Arguments));
                    pending.push((car, Role::Expression));
                }
                Role::Expression
                    if car.is_bare_symbol_named("+") || car.is_bare_symbol_named("-") =>
                {
                    tokens.push(if car.is_bare_symbol_named("+") {
                        Token::Sum
                    } else {
                        Token::Difference
                    });
                    pending.push((cdr, Role::Arguments));
                }
                Role::Expression if car.as_int().is_some() || car.as_float().is_some() => {
                    if cdr.is_nil() {
                        tokens.push(Token::AbsolutePixels(car.bits()));
                    } else {
                        tokens.push(Token::Scale(car.bits()));
                        pending.push((cdr, Role::Expression));
                    }
                }
                Role::Expression if car.is_bare_symbol_named("image") => {
                    tokens.push(
                        ImageSpecIdentity::from_lisp_spec(&value)
                            .map(Token::Image)
                            .unwrap_or(Token::Opaque(value.bits())),
                    );
                }
                Role::Expression => tokens.push(Token::Opaque(value.bits())),
            }
        }
        Self::Expression(tokens.into())
    }
}

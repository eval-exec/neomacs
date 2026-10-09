use neovm_core::emacs_core::intern::SymId;
use neovm_core::emacs_core::symbol::Obarray;
use neovm_core::emacs_core::value::Value;

fn borrowed_global(owner: &Obarray) -> Option<&Value> {
    owner.symbol_value_copied("p74-no-slot-borrow")
}

fn borrowed_identity(owner: &Obarray, symbol: SymId) -> Option<&Value> {
    owner.symbol_value_id_copied(symbol)
}

fn borrowed_default(owner: &Obarray, symbol: SymId) -> Option<&Value> {
    owner.default_value_id_copied(symbol)
}

fn main() {}

/// Stand-in for `LispSymbol`. The included test helper casts a chunk address
/// to this type and reads `name`. The grace model does not call that helper.
pub struct LispSymbol;

impl LispSymbol {
    pub fn name(&self) -> crate::intern::NameId {
        crate::intern::NameId
    }
}
